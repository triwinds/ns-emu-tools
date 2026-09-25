//! Experimental SDK-off creation integration for the frozen target.
use super::*;
use libloading::Library;
use std::{
    ffi::{c_void, CString},
    path::PathBuf,
    sync::Arc,
};
type Result<T> = std::result::Result<T, String>;
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
    fn probe_sl_init(function: *mut c_void, directory: *const u16) -> i32;
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
struct Sdk {
    library: Library,
    _directory: Vec<u16>,
    instance_ext: Vec<CString>,
    device_ext: Vec<CString>,
    graphics: u32,
    compute: u32,
}
static SDK: OnceLock<Arc<Sdk>> = OnceLock::new();
static INITIALIZING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ACTIVE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CLOSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub(super) fn enabled() -> bool {
    std::env::var("NS_STREAMLINE_TARGET_SDK").as_deref() == Ok("1")
}
unsafe fn address(sdk: &Sdk, name: &[u8]) -> Result<*mut c_void> {
    type F = unsafe extern "C" fn();
    Ok(*sdk.library.get::<F>(name).map_err(|e| e.to_string())? as *mut c_void)
}
fn require(code: i32, call: &str) -> Result<()> {
    if code != 0 {
        Err(format!("{call} failed: {code}"))
    } else {
        Ok(())
    }
}
unsafe fn initialize() -> Result<Option<Arc<Sdk>>> {
    if let Some(sdk) = SDK.get() {
        return Ok(Some(sdk.clone()));
    }
    if INITIALIZING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(None);
    }
    let result = (|| -> Result<Arc<Sdk>> {
        let path = PathBuf::from(
            std::env::var_os("NS_STREAMLINE_TARGET_RUNTIME").ok_or("missing target runtime")?,
        );
        use std::os::windows::ffi::OsStrExt;
        let directory: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let library = libloading::os::windows::Library::load_with_flags(
            path.join("sl.interposer.dll"),
            0x100 | 0x1000,
        )
        .map_err(|e| e.to_string())?;
        let mut sdk = Sdk {
            library: library.into(),
            _directory: directory,
            instance_ext: Vec::new(),
            device_ext: Vec::new(),
            graphics: 0,
            compute: 0,
        };
        require(
            probe_sl_init(address(&sdk, b"slInit\0")?, sdk._directory.as_ptr()),
            "slInit",
        )?;
        for feature in [1000, 3, 4] {
            let mut req: Requirements = std::mem::zeroed();
            require(
                probe_sl_requirements(
                    address(&sdk, b"slGetFeatureRequirements\0")?,
                    feature,
                    &mut req,
                ),
                "requirements",
            )?;
            if req.flags & 4 == 0 {
                return Err("feature has no Vulkan support".into());
            }
            sdk.graphics += req.graphics;
            sdk.compute += req.compute;
            for group in 0..4 {
                if req.counts[group] > 64 {
                    return Err("invalid requirement count".into());
                }
                for bytes in req.names[group].iter().take(req.counts[group] as usize) {
                    let end = bytes
                        .iter()
                        .position(|&b| b == 0)
                        .ok_or("unterminated requirement")?;
                    let name = CString::new(&bytes[..end]).map_err(|e| e.to_string())?;
                    match group {
                        0 => {
                            if !sdk.instance_ext.contains(&name) {
                                sdk.instance_ext.push(name)
                            }
                        }
                        1 => {
                            if !sdk.device_ext.contains(&name) {
                                sdk.device_ext.push(name)
                            }
                        }
                        2 => {
                            if ![
                                c"timelineSemaphore",
                                c"descriptorIndexing",
                                c"bufferDeviceAddress",
                            ]
                            .contains(&name.as_c_str())
                            {
                                return Err("unimplemented SDK feature12".into());
                            }
                        }
                        3 => {
                            if name.as_c_str() != c"synchronization2" {
                                return Err("unimplemented SDK feature13".into());
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        sdk.instance_ext.extend([
            ash::khr::get_surface_capabilities2::NAME.to_owned(),
            ash::ext::surface_maintenance1::NAME.to_owned(),
        ]);
        sdk.device_ext
            .push(ash::ext::swapchain_maintenance1::NAME.to_owned());
        trace::event(
            "target_sdk_requirements",
            json!({"graphics":sdk.graphics,"compute":sdk.compute,"instance_extensions":sdk.instance_ext.iter().map(|s|s.to_string_lossy()).collect::<Vec<_>>(),"device_extensions":sdk.device_ext.iter().map(|s|s.to_string_lossy()).collect::<Vec<_>>(),"fg_enabled":false}),
        );
        Ok(Arc::new(sdk))
    })();
    INITIALIZING.store(false, std::sync::atomic::Ordering::SeqCst);
    let sdk = result?;
    let _ = SDK.set(sdk.clone());
    Ok(Some(sdk))
}
unsafe fn extensions(
    original: *const *const c_char,
    count: u32,
    extra: &[CString],
) -> Vec<CString> {
    let mut result: Vec<_> = (0..count as usize)
        .map(|i| CStr::from_ptr(*original.add(i)).to_owned())
        .collect();
    for name in extra {
        if !result.contains(name) {
            result.push(name.clone())
        }
    }
    result
}
pub(super) unsafe fn create_instance(
    next: vk::PFN_vkCreateInstance,
    info: *const vk::InstanceCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::Instance,
) -> vk::Result {
    let result = (|| -> Result<vk::Result> {
        let Some(sdk) = initialize()? else {
            return Ok(next(info, alloc, out));
        };
        let names = extensions(
            (*info).pp_enabled_extension_names,
            (*info).enabled_extension_count,
            &sdk.instance_ext,
        );
        let pointers: Vec<_> = names.iter().map(|s| s.as_ptr()).collect();
        let mut app = if (*info).p_application_info.is_null() {
            vk::ApplicationInfo::default()
        } else {
            *(*info).p_application_info
        };
        app.api_version = app.api_version.max(vk::API_VERSION_1_3);
        let copy = (*info)
            .application_info(&app)
            .enabled_extension_names(&pointers);
        Ok(next(&copy, alloc, out))
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            trace::event(
                "target_sdk_error",
                json!({"stage":"instance","error":error}),
            );
            vk::Result::ERROR_INITIALIZATION_FAILED
        }
    }
}
pub(super) unsafe fn create_device(
    parent: Instance,
    next: vk::PFN_vkCreateDevice,
    physical: vk::PhysicalDevice,
    info: *const vk::DeviceCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::Device,
) -> vk::Result {
    if INITIALIZING.load(std::sync::atomic::Ordering::SeqCst) {
        return next(physical, info, alloc, out);
    }
    let result = (|| -> Result<vk::Result> {
        let sdk = SDK
            .get()
            .ok_or("SDK was not initialized before instance creation")?;
        if ACTIVE.load(std::sync::atomic::Ordering::SeqCst) != 0
            || CLOSED.load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err("experimental target mode supports one device lifecycle".into());
        }
        let instance = ash::Instance::load_with(
            |name| {
                (parent.gipa)(parent.handle, name.as_ptr())
                    .map_or(std::ptr::null(), |f| f as *const _)
            },
            parent.handle,
        );
        if (*info).queue_create_info_count != 1 {
            return Err("target requires multiple queue families".into());
        }
        let original = &*(*info).p_queue_create_infos;
        if original.flags != vk::DeviceQueueCreateFlags::empty() || !original.p_next.is_null() {
            return Err("unsupported target queue flags/chain".into());
        }
        if crate::target_fg::enabled() && original.queue_family_index != 0 {
            return Err("bounded FG diagnostic requires queue family zero".into());
        }
        let families = instance.get_physical_device_queue_family_properties(physical);
        let family = families
            .get(original.queue_family_index as usize)
            .ok_or("unknown queue family")?;
        if !family
            .queue_flags
            .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
        {
            return Err("combined graphics/compute required".into());
        }
        let plan = crate::target_device_plan::queues(
            original.queue_count,
            family.queue_count,
            sdk.graphics,
            sdk.compute,
        )?;
        let props = instance.get_physical_device_properties(physical);
        if props.vendor_id != 0x10de || props.api_version < vk::API_VERSION_1_3 {
            return Err("unsupported target GPU/API".into());
        }
        let names = extensions(
            (*info).pp_enabled_extension_names,
            (*info).enabled_extension_count,
            &sdk.device_ext,
        );
        let available = instance
            .enumerate_device_extension_properties(physical)
            .map_err(|e| e.to_string())?;
        for name in &names {
            if !available
                .iter()
                .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name.as_c_str())
            {
                return Err(format!("missing extension {name:?}"));
            }
        }
        let mut f12 = vk::PhysicalDeviceVulkan12Features::default();
        let mut f13 = vk::PhysicalDeviceVulkan13Features::default();
        let mut maintenance = vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default();
        instance.get_physical_device_features2(
            physical,
            &mut vk::PhysicalDeviceFeatures2::default()
                .push_next(&mut f12)
                .push_next(&mut f13)
                .push_next(&mut maintenance),
        );
        if f12.timeline_semaphore == 0
            || f12.buffer_device_address == 0
            || f12.descriptor_indexing == 0
            || f13.synchronization2 == 0
            || maintenance.swapchain_maintenance1 == 0
        {
            return Err("required SDK feature unavailable".into());
        }
        let mut priorities =
            std::slice::from_raw_parts(original.p_queue_priorities, original.queue_count as usize)
                .to_vec();
        priorities.resize(plan.total as usize, 1.0);
        let queues = [(*original).queue_priorities(&priorities)];
        let chain = crate::target_device_plan::FeatureChain::for_target((*info).p_next)?;
        let pointers: Vec<_> = names.iter().map(|s| s.as_ptr()).collect();
        let mut copy = (*info)
            .enabled_extension_names(&pointers)
            .queue_create_infos(&queues);
        copy.p_next = chain.head();
        trace::event(
            "target_device_plan",
            json!({"family":original.queue_family_index,"application_count":plan.application_count,"sdk_graphics_start":plan.graphics_start,"sdk_compute_start":plan.compute_start,"total":plan.total,"capacity":family.queue_count,"original_api":"1.2","requested_api":"1.3","fg_enabled":false}),
        );
        Ok(next(physical, &copy, alloc, out))
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            trace::event("target_sdk_error", json!({"stage":"device","error":error}));
            vk::Result::ERROR_INITIALIZATION_FAILED
        }
    }
}
struct Pending {
    parent: Instance,
    physical: vk::PhysicalDevice,
    device: vk::Device,
    family: u32,
    application_count: u32,
}
static PENDING: OnceLock<Mutex<Option<Pending>>> = OnceLock::new();
pub(super) unsafe fn device_created(
    parent: Instance,
    physical: vk::PhysicalDevice,
    device: vk::Device,
    info: *const vk::DeviceCreateInfo,
) -> Result<()> {
    if INITIALIZING.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    let queue = &*(*info).p_queue_create_infos;
    *PENDING.get_or_init(Default::default).lock().unwrap() = Some(Pending {
        parent,
        physical,
        device,
        family: queue.queue_family_index,
        application_count: queue.queue_count,
    });
    Ok(())
}
pub(super) unsafe fn ensure_device(device: vk::Device) {
    let pending = {
        let mut slot = PENDING.get_or_init(Default::default).lock().unwrap();
        if slot.as_ref().is_none_or(|p| p.device != device) {
            return;
        }
        slot.take().unwrap()
    };
    // The Loader must finish vkCreateDevice before NvLowLatencyVk may inspect it.
    // Initialization is deferred to the first application device command.
    let result = (|| -> Result<()> {
        let sdk = SDK.get().ok_or("SDK missing")?;
        require(
            probe_sl_register_route(
                address(sdk, b"slRegisterVulkanLayerRouteV1\0")?,
                pending.parent.handle.as_raw(),
                pending.physical.as_raw(),
                device.as_raw(),
                probeRouteGipa,
                probeRouteGdpa,
            ),
            "register route",
        )?;
        require(
            probe_sl_set_vulkan(
                address(sdk, b"slSetVulkanInfo\0")?,
                pending.parent.handle.as_raw(),
                pending.physical.as_raw(),
                device.as_raw(),
                pending.family,
                pending.application_count,
                pending.application_count + sdk.graphics,
            ),
            "set Vulkan info",
        )?;
        ACTIVE.store(device.as_raw(), std::sync::atomic::Ordering::SeqCst);
        trace::event(
            "target_sdk_ready",
            json!({"device":device.as_raw(),"fg_enabled":false,"after_loader_create_device":true}),
        );
        Ok(())
    })();
    if let Err(error) = result {
        trace::event(
            "target_sdk_error",
            json!({"stage":"deferred_set_info","error":error}),
        );
        std::process::abort()
    }
}
pub(super) unsafe fn destroy_device(device: vk::Device) {
    if ACTIVE.load(std::sync::atomic::Ordering::SeqCst) != device.as_raw() {
        return;
    }
    let sdk = SDK.get().unwrap();
    let function = address(sdk, b"slShutdown\0").unwrap_or_else(|_| std::process::abort());
    let result = probe_sl_shutdown(function);
    trace::event("target_sdk_shutdown", json!({"result":result}));
    if result != 0 {
        std::process::abort()
    }
    ACTIVE.store(0, std::sync::atomic::Ordering::SeqCst);
    CLOSED.store(true, std::sync::atomic::Ordering::SeqCst);
}

#[derive(Clone, Copy)]
struct Surface {
    instance: u64,
    hwnd: isize,
    hinstance: isize,
    shadow: u64,
}
static SURFACES: OnceLock<Mutex<HashMap<u64, Surface>>> = OnceLock::new();
fn surfaces() -> std::sync::MutexGuard<'static, HashMap<u64, Surface>> {
    SURFACES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
pub(super) unsafe fn surface_created(
    instance: vk::Instance,
    info: *const vk::Win32SurfaceCreateInfoKHR,
    surface: vk::SurfaceKHR,
) {
    surfaces().insert(
        surface.as_raw(),
        Surface {
            instance: instance.as_raw(),
            hwnd: (*info).hwnd,
            hinstance: (*info).hinstance,
            shadow: 0,
        },
    );
}
pub(super) unsafe fn surface_destroyed(surface: vk::SurfaceKHR) {
    let record = surfaces().remove(&surface.as_raw());
    if let Some(record) = record {
        if record.shadow != 0 {
            let sdk = SDK.get().unwrap();
            let destroy = *sdk
                .library
                .get::<vk::PFN_vkDestroySurfaceKHR>(b"vkDestroySurfaceKHR\0")
                .unwrap();
            destroy(
                vk::Instance::from_raw(record.instance),
                vk::SurfaceKHR::from_raw(record.shadow),
                std::ptr::null(),
            );
        }
    }
}
pub(super) unsafe fn device_proc(device: vk::Device, name: &CStr) -> vk::PFN_vkVoidFunction {
    if ACTIVE.load(std::sync::atomic::Ordering::SeqCst) != device.as_raw() {
        return None;
    }
    if !matches!(
        name.to_bytes(),
        b"vkGetSwapchainImagesKHR"
            | b"vkAcquireNextImageKHR"
            | b"vkQueuePresentKHR"
            | b"vkDestroySwapchainKHR"
            | b"vkDeviceWaitIdle"
    ) {
        return None;
    }
    let sdk = SDK.get()?;
    let gdpa = *sdk
        .library
        .get::<vk::PFN_vkGetDeviceProcAddr>(b"vkGetDeviceProcAddr\0")
        .ok()?;
    gdpa(device, name.as_ptr())
}
pub(super) unsafe fn create_swapchain(
    device: vk::Device,
    info: *const vk::SwapchainCreateInfoKHR,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::SwapchainKHR,
) -> Option<vk::Result> {
    if ACTIVE.load(std::sync::atomic::Ordering::SeqCst) != device.as_raw() {
        return None;
    }
    let result = (|| -> Result<vk::Result> {
        if !alloc.is_null() {
            return Err("custom swapchain allocators not supported in target experiment".into());
        }
        let sdk = SDK.get().ok_or("SDK missing")?;
        let mut surface = surfaces()
            .get(&(*info).surface.as_raw())
            .copied()
            .ok_or("unknown application surface")?;
        if surface.shadow == 0 {
            let create = *sdk
                .library
                .get::<vk::PFN_vkCreateWin32SurfaceKHR>(b"vkCreateWin32SurfaceKHR\0")
                .map_err(|e| e.to_string())?;
            let mut shadow = vk::SurfaceKHR::null();
            create(
                vk::Instance::from_raw(surface.instance),
                &vk::Win32SurfaceCreateInfoKHR::default()
                    .hwnd(surface.hwnd)
                    .hinstance(surface.hinstance),
                std::ptr::null(),
                &mut shadow,
            )
            .result()
            .map_err(|e| e.to_string())?;
            surface.shadow = shadow.as_raw();
            surfaces().insert((*info).surface.as_raw(), surface);
        }
        let create = *sdk
            .library
            .get::<vk::PFN_vkCreateSwapchainKHR>(b"vkCreateSwapchainKHR\0")
            .map_err(|e| e.to_string())?;
        let mut copy = (*info).surface(vk::SurfaceKHR::from_raw(surface.shadow));
        if crate::target_fg::enabled() {
            let dispatch =
                instance(vk::Instance::from_raw(surface.instance)).ok_or("instance missing")?;
            let query: vk::PFN_vkGetPhysicalDeviceSurfacePresentModesKHR = std::mem::transmute(
                (dispatch.gipa)(
                    dispatch.handle,
                    c"vkGetPhysicalDeviceSurfacePresentModesKHR".as_ptr(),
                )
                .ok_or("surface mode query missing")?,
            );
            let physical = crate::device(device).ok_or("device missing")?.physical;
            let mut count = 0;
            query(physical, (*info).surface, &mut count, std::ptr::null_mut())
                .result()
                .map_err(|e| e.to_string())?;
            let mut modes = vec![vk::PresentModeKHR::FIFO; count as usize];
            query(physical, (*info).surface, &mut count, modes.as_mut_ptr())
                .result()
                .map_err(|e| e.to_string())?;
            modes.truncate(count as usize);
            if !modes.contains(&vk::PresentModeKHR::IMMEDIATE) {
                return Err("Immediate presentation required for FG".into());
            }
            copy.present_mode = vk::PresentModeKHR::IMMEDIATE;
            if crate::target_motion::enabled() {
                let caps_query: vk::PFN_vkGetPhysicalDeviceSurfaceCapabilitiesKHR =
                    std::mem::transmute(
                        (dispatch.gipa)(
                            dispatch.handle,
                            c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR".as_ptr(),
                        )
                        .ok_or("missing surface capabilities")?,
                    );
                let mut caps = vk::SurfaceCapabilitiesKHR::default();
                caps_query(physical, (*info).surface, &mut caps)
                    .result()
                    .map_err(|e| e.to_string())?;
                if !caps
                    .supported_usage_flags
                    .contains(vk::ImageUsageFlags::TRANSFER_SRC)
                {
                    return Err("surface cannot supply motion input".into());
                }
                copy.image_usage |= vk::ImageUsageFlags::TRANSFER_SRC;
            }
        }
        let result = create(device, &copy, alloc, out);
        if result == vk::Result::SUCCESS && crate::target_fg::enabled() {
            crate::target_fg::created(
                device,
                *out,
                copy.image_extent,
                copy.image_format,
                copy.min_image_count,
                copy.queue_family_index_count,
                surface.instance,
                surface.hwnd,
            )?;
        }

        trace::event(
            "target_proxy_swapchain",
            json!({"result":result.as_raw(),"application_surface":(*info).surface.as_raw(),"sdk_surface":surface.shadow,"width":copy.image_extent.width,"height":copy.image_extent.height,"format":copy.image_format.as_raw(),"usage":copy.image_usage.as_raw(),"present_mode":copy.present_mode.as_raw(),"fg_enabled":false}),
        );
        Ok(result)
    })();
    Some(match result {
        Ok(value) => value,
        Err(error) => {
            trace::event(
                "target_sdk_error",
                json!({"stage":"swapchain","error":error}),
            );
            vk::Result::ERROR_INITIALIZATION_FAILED
        }
    })
}

pub(super) unsafe fn fg_api() -> Result<crate::fg_api::Api> {
    let sdk = SDK.get().ok_or("SDK missing")?;
    Ok(crate::fg_api::Api {
        feature: address(sdk, b"slGetFeatureFunction\0")?,
        token: address(sdk, b"slGetNewFrameToken\0")?,
        constants: address(sdk, b"slSetConstants\0")?,
        tags: address(sdk, b"slSetTagForFrame\0")?,
    })
}
pub(super) unsafe fn sdk_device(handle: vk::Device) -> Result<ash::Device> {
    let sdk = SDK.get().ok_or("SDK missing")?;
    let gdpa = *sdk
        .library
        .get::<vk::PFN_vkGetDeviceProcAddr>(b"vkGetDeviceProcAddr\0")
        .map_err(|e| e.to_string())?;
    Ok(ash::Device::load_with(
        |name| gdpa(handle, name.as_ptr()).map_or(std::ptr::null(), |f| f as *const _),
        handle,
    ))
}
