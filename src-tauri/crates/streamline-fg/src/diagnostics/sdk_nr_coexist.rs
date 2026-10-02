//! Real Streamline SR consumer on the NR diagnostic's device. No emulator UI.
use crate::{nr_api::Result, sdk_nr};
use ash::vk::{self, Handle};
use libloading::Library;
use serde_json::{json, Value};
use std::{
    ffi::{c_void, CString},
    fs,
    path::{Path, PathBuf},
};

#[repr(C)]
struct Requirements {
    flags: u32,
    cpu: u32,
    viewports: u32,
    graphics: u32,
    compute: u32,
    optical: u32,
    counts: [u32; 5],
    tags: [u32; 64],
    names: [[[u8; 128]; 64]; 4],
    versions: [[u32; 3]; 4],
}
#[link(name = "streamline_query_bridge", kind = "static")]
unsafe extern "C" {
    fn probe_sl_init_sr(function: *mut c_void, directory: *const u16) -> i32;
    fn probe_sl_requirements(function: *mut c_void, feature: u32, output: *mut Requirements)
        -> i32;
    fn probe_sl_set_vulkan(
        function: *mut c_void,
        instance: u64,
        physical: u64,
        device: u64,
        family: u32,
        graphics: u32,
        compute: u32,
    ) -> i32;
    fn probe_sl_register_route(
        function: *mut c_void,
        instance: u64,
        physical: u64,
        device: u64,
        gipa: vk::PFN_vkGetInstanceProcAddr,
        gdpa: vk::PFN_vkGetDeviceProcAddr,
    ) -> i32;
    fn probe_sl_shutdown(function: *mut c_void) -> i32;
}
pub(super) fn stage(source: &Path, destination: &Path) -> Result<Value> {
    fs::create_dir(destination)?;
    let mut files = Vec::new();
    for manifest in [
        include_str!("../../sdk-route/runtime-v3.json"),
        include_str!("../../sdk-route/sr-runtime.json"),
    ] {
        let manifest: Value = serde_json::from_str(manifest)?;
        for file in manifest["files"]
            .as_array()
            .ok_or("missing runtime manifest files")?
        {
            let name = file["name"].as_str().ok_or("missing runtime name")?;
            let expected = file["sha256"].as_str().ok_or("missing runtime hash")?;
            let path = source.join(name);
            if sdk_nr::hash(&path)? != expected {
                return Err(
                    format!("coexistence runtime hash mismatch: {}", path.display()).into(),
                );
            }
            let target = destination.join(name);
            fs::copy(path, &target)?;
            if sdk_nr::hash(&target)? != expected {
                return Err("staged runtime hash mismatch".into());
            }
            files.push(json!({"name":name,"sha256":expected}));
        }
    }
    for name in crate::sdk_sr::PLUGINS {
        crate::sdk_sr::verify_plugin(&destination.join(name), name)?;
    }
    Ok(json!(files))
}
unsafe fn address(library: &Library, name: &[u8]) -> Result<*mut c_void> {
    Ok(*library.get::<unsafe extern "C" fn()>(name)? as *mut c_void)
}
fn checked(code: i32, stage: &str) -> Result<()> {
    sdk_nr::event(stage, json!({"result":code}))?;
    if code != 0 {
        return Err(format!("{stage}: Streamline result {code}").into());
    }
    Ok(())
}
pub(super) struct Consumer {
    library: Library,
    _directory: Vec<u16>,
    requirements: Requirements,
    pub(super) instance_extensions: Vec<CString>,
    pub(super) device_extensions: Vec<CString>,
    repair: std::cell::Cell<bool>,
}
impl Consumer {
    pub(super) unsafe fn initialize(directory: &Path) -> Result<Self> {
        let library: Library = libloading::os::windows::Library::load_with_flags(
            directory.join("sl.interposer.dll"),
            0x100 | 0x800,
        )?
        .into();
        // Failed initialization may leave SDK callbacks alive. The isolated
        // child aborts on error, so retain this module rather than unloading it.
        let library = std::mem::ManuallyDrop::new(library);
        use std::os::windows::ffi::OsStrExt;
        let path: Vec<_> = directory.as_os_str().encode_wide().chain(Some(0)).collect();
        checked(
            probe_sl_init_sr(address(&library, b"slInit\0")?, path.as_ptr()),
            "coexist_sl_init",
        )?;
        let mut requirements: Requirements = std::mem::zeroed();
        checked(
            probe_sl_requirements(
                address(&library, b"slGetFeatureRequirements\0")?,
                0,
                &mut requirements,
            ),
            "coexist_sr_requirements",
        )?;
        if requirements.flags & 4 == 0
            || requirements.graphics != 0
            || requirements.compute != 0
            || requirements.optical != 0
        {
            return Err("unimplemented SR coexistence queue requirements".into());
        }
        let names = |group: usize| -> Result<Vec<CString>> {
            let count = requirements.counts[group] as usize;
            if count > 64 {
                return Err("coexistence requirements count exceeds bridge capacity".into());
            }
            requirements.names[group][..count]
                .iter()
                .map(|bytes| {
                    let end = bytes
                        .iter()
                        .position(|b| *b == 0)
                        .ok_or("unterminated requirement")?;
                    Ok(CString::new(&bytes[..end])?)
                })
                .collect()
        };
        let instance_extensions = names(0)?;
        // Vulkan 1.3 supplies KHR buffer-device-address. EXT and KHR must not
        // both be enabled; the pinned SR contract accepts the core path.
        let device_extensions = names(1)?
            .into_iter()
            .filter(|s| {
                ![
                    c"VK_EXT_buffer_device_address",
                    c"VK_KHR_buffer_device_address",
                ]
                .contains(&s.as_c_str())
            })
            .collect();
        sdk_nr::event(
            "coexist_requirements",
            json!({"instance":instance_extensions.iter().map(|s|s.to_string_lossy()).collect::<Vec<_>>(),
            "features12":names(2)?.iter().map(|s|s.to_string_lossy()).collect::<Vec<_>>(),"features13":names(3)?.iter().map(|s|s.to_string_lossy()).collect::<Vec<_>>()}),
        )?;
        Ok(Self {
            library: std::mem::ManuallyDrop::into_inner(library),
            _directory: path,
            requirements,
            instance_extensions,
            device_extensions,
            repair: std::cell::Cell::new(false),
        })
    }
    pub(super) fn enable_features(
        &self,
        available: &vk::PhysicalDeviceVulkan12Features<'_>,
        enabled: &mut vk::PhysicalDeviceVulkan12Features<'_>,
    ) -> Result<()> {
        if self.requirements.counts[3] != 0 {
            return Err("unexpected SR Vulkan 1.3 feature requirement".into());
        }
        for bytes in &self.requirements.names[2][..self.requirements.counts[2] as usize] {
            let end = bytes
                .iter()
                .position(|b| *b == 0)
                .ok_or("unterminated requirement")?;
            let (supported, target) = match &bytes[..end] {
                b"timelineSemaphore" => (
                    available.timeline_semaphore,
                    &mut enabled.timeline_semaphore,
                ),
                b"descriptorIndexing" => (
                    available.descriptor_indexing,
                    &mut enabled.descriptor_indexing,
                ),
                b"bufferDeviceAddress" => (
                    available.buffer_device_address,
                    &mut enabled.buffer_device_address,
                ),
                _ => return Err("unimplemented SR Vulkan 1.2 feature requirement".into()),
            };
            if supported != vk::TRUE {
                return Err("unsupported SR feature".into());
            }
            *target = vk::TRUE;
        }
        Ok(())
    }
    pub(super) unsafe fn attach(
        &self,
        entry: &ash::Entry,
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        family: u32,
        repair: bool,
    ) -> Result<()> {
        self.repair.set(repair);
        checked(
            probe_sl_register_route(
                address(&self.library, b"slRegisterVulkanLayerRouteV1\0")?,
                instance.handle().as_raw(),
                physical.as_raw(),
                device.handle().as_raw(),
                if repair {
                    crate::nr_layout::gipa
                } else {
                    entry.static_fn().get_instance_proc_addr
                },
                if repair {
                    crate::nr_layout::gdpa
                } else {
                    instance.fp_v1_0().get_device_proc_addr
                },
            ),
            "coexist_register_route",
        )?;
        checked(
            probe_sl_set_vulkan(
                address(&self.library, b"slSetVulkanInfo\0")?,
                instance.handle().as_raw(),
                physical.as_raw(),
                device.handle().as_raw(),
                family,
                0,
                0,
            ),
            "coexist_set_vulkan",
        )
    }
    pub(super) unsafe fn exercise(
        &self,
        session: &Path,
        stage: &str,
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        family: u32,
    ) -> Result<()> {
        let path: PathBuf = session.join(stage);
        fs::create_dir(&path)?;
        sdk_nr::event("coexist_sr_evaluate_enter", json!({"phase":stage}))?;
        if self.repair.get() {
            crate::sdk_sr::exercise_scoped(
                &path,
                &self.library,
                instance,
                physical,
                device,
                family,
                Some(|command, width, height| {
                    Box::new(crate::nr_layout::Recording::sr(command, width, height))
                }),
            )?;
        } else {
            crate::sdk_sr::exercise(&path, &self.library, instance, physical, device, family)?;
        }
        sdk_nr::event(
            "coexist_sr_evaluate_complete",
            json!({"phase":stage,"result":serde_json::from_str::<Value>(&fs::read_to_string(path.join("sr-result.json"))?)?}),
        )
    }
    pub(super) unsafe fn shutdown(&self) -> Result<()> {
        checked(
            probe_sl_shutdown(address(&self.library, b"slShutdown\0")?),
            "coexist_sl_shutdown",
        )
    }
}
