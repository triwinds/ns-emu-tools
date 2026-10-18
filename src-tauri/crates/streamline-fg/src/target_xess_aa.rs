//! Independent XeSS Native AA: zero jitter, zero motion, reset on every frame.
//! No Streamline, NVIDIA optical flow, vendor restriction or temporal-history claim.
use crate::{fg_api::*, trace, xess_api as x};
use ash::vk::{self, Handle};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, c_void, CStr, CString},
    fs::File,
    io::Read,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        OnceLock,
    },
};

const HASH: &str = "251659dd84a3e84de67c886a4186e01f3eca49b00641906fe38bb6b807e5d5b7";
static SDK: OnceLock<(x::Api, File)> = OnceLock::new();
static INTERNAL: AtomicU32 = AtomicU32::new(0);
pub(super) struct Internal;
pub(super) fn internal() -> Internal {
    INTERNAL.fetch_add(1, Ordering::SeqCst);
    Internal
}
impl Drop for Internal {
    fn drop(&mut self) {
        INTERNAL.fetch_sub(1, Ordering::SeqCst);
    }
}
pub(super) fn backend() -> &'static str {
    "xess_zero_motion_reset"
}
pub(super) fn enabled() -> bool {
    !internal_active() && std::env::var("NS_EMU_XESS_NATIVE_AA").as_deref() == Ok("1")
}
pub(super) fn internal_active() -> bool {
    INTERNAL.load(Ordering::SeqCst) != 0
}
pub(super) unsafe fn sdk() -> Result<&'static x::Api> {
    if let Some(sdk) = SDK.get() {
        return Ok(&sdk.0);
    }
    let path = PathBuf::from(std::env::var_os("NS_EMU_XESS_DLL").ok_or("missing NS_EMU_XESS_DLL")?);
    if !path.is_absolute() {
        return Err("XeSS DLL must have an absolute path".into());
    }
    use std::os::windows::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if format!("{:x}", Sha256::digest(&bytes)) != HASH {
        return Err("XeSS A/B pinned DLL hash mismatch".into());
    }
    let api = x::Api::load(&path)?;
    let _ = SDK.set((api, file));
    Ok(&SDK.get().ok_or("XeSS API initialization failed")?.0)
}
pub(super) unsafe fn device_extensions(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
) -> Result<Vec<CString>> {
    let mut count = 0;
    let mut names = std::ptr::null();
    x::check(
        (sdk()?.device_extensions)(
            instance.handle(),
            public_physical(instance, physical)?,
            &mut count,
            &mut names,
        ),
        "XeSS device requirements",
    )?;
    Ok(if count == 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(names, count as usize)
            .iter()
            .map(|p| CStr::from_ptr(*p).to_owned())
            .collect()
    })
}
pub(super) unsafe fn features(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    head: &mut *mut c_void,
) -> Result<()> {
    x::check(
        (sdk()?.features)(
            instance.handle(),
            public_physical(instance, physical)?,
            head,
        ),
        "XeSS device features",
    )
}
unsafe fn public_physical(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
) -> Result<vk::PhysicalDevice> {
    // A layer receives a downstream physical handle. XeSS uses the public
    // loader, so resolve the same adapter through its public enumeration.
    let library = libloading::Library::new("vulkan-1.dll")?;
    let enumerate =
        *library.get::<vk::PFN_vkEnumeratePhysicalDevices>(b"vkEnumeratePhysicalDevices\0")?;
    let properties = *library
        .get::<vk::PFN_vkGetPhysicalDeviceProperties>(b"vkGetPhysicalDeviceProperties\0")?;
    let expected = instance.get_physical_device_properties(physical);
    let mut count = 0;
    enumerate(instance.handle(), &mut count, std::ptr::null_mut()).result()?;
    let mut handles = vec![vk::PhysicalDevice::null(); count as usize];
    enumerate(instance.handle(), &mut count, handles.as_mut_ptr()).result()?;
    let mut found = Vec::new();
    for handle in handles.into_iter().take(count as usize) {
        let mut props = vk::PhysicalDeviceProperties::default();
        properties(handle, &mut props);
        if props.vendor_id == expected.vendor_id && props.device_id == expected.device_id {
            found.push(handle);
        }
    }
    if found.len() != 1 {
        return Err("XeSS A/B public adapter mapping is ambiguous".into());
    }
    Ok(found[0])
}
unsafe extern "C" fn log(message: *const c_char, level: i32) {
    trace::event!(
        "target_xess_log",
        json!({"level":level,"message":CStr::from_ptr(message).to_string_lossy()})
    );
}
pub(super) struct Aa {
    session: Option<x::Session<'static>>,
    device: ash::Device,
    resources: Vec<Resource>,
    sampler: vk::Sampler,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    shader: vk::ShaderModule,
    pipeline: vk::Pipeline,
    queries: vk::QueryPool,
    bits: u32,
    period: f32,
    initialized: bool,
}
// Access is serialized by the owning swapchain's mutex and completion fence.
unsafe impl Send for Aa {}
impl Aa {
    pub(super) unsafe fn new(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        family: u32,
        input: Resource,
        output: Resource,
    ) -> Result<Self> {
        let _internal = internal();
        if input.width != output.width
            || input.height != output.height
            || ![
                vk::Format::R8G8B8A8_UNORM.as_raw() as u32,
                vk::Format::B8G8R8A8_UNORM.as_raw() as u32,
            ]
            .contains(&input.format)
            || output.format != vk::Format::R8G8B8A8_UNORM.as_raw() as u32
        {
            return Err("XeSS A/B accepts only 1:1 RGBA8 Native AA".into());
        }
        let api = sdk()?;
        let mut handle = std::ptr::null_mut();
        x::check(
            (api.create)(
                instance.handle(),
                public_physical(instance, physical)?,
                device.handle(),
                &mut handle,
            ),
            "XeSS create",
        )?;
        let session = x::Session { handle, api };
        x::check((api.logging)(handle, 1, log), "XeSS logging")?;
        let size = x::Size {
            x: input.width,
            y: input.height,
        };
        let mut optimal = x::Size::default();
        let mut min = x::Size::default();
        let mut max = x::Size::default();
        x::check(
            (api.optimal)(handle, &size, 106, &mut optimal, &mut min, &mut max),
            "XeSS AA resolution",
        )?;
        if [optimal.x, optimal.y] != [size.x, size.y] {
            return Err("XeSS AA returned a nonnative input".into());
        }
        let mut version = x::Version::default();
        x::check((api.version)(&mut version), "XeSS version")?;
        let mut props = x::Properties::default();
        x::check(
            (api.properties)(handle, &size, &mut props),
            "XeSS properties",
        )?;
        x::check(
            (api.init)(
                handle,
                &x::Init {
                    output: size,
                    quality: 106,
                    flags: (1 << 6) | 1,
                    ..Default::default()
                },
            ),
            "XeSS AA init",
        )?;
        let mut aa = Self {
            session: Some(session),
            device: device.clone(),
            resources: Vec::new(),
            sampler: vk::Sampler::null(),
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            shader: vk::ShaderModule::null(),
            pipeline: vk::Pipeline::null(),
            queries: vk::QueryPool::null(),
            bits: instance.get_physical_device_queue_family_properties(physical)[family as usize]
                .timestamp_valid_bits,
            period: instance
                .get_physical_device_properties(physical)
                .limits
                .timestamp_period,
            initialized: false,
        };
        let memory = instance.get_physical_device_memory_properties(physical);
        for _ in 0..2 {
            aa.resources.push(texture_with_usage(
                device,
                &memory,
                vk::Extent2D {
                    width: size.x,
                    height: size.y,
                },
                vk::Format::R16G16B16A16_SFLOAT,
                vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )?);
        }
        aa.resources.push(texture_with_usage(
            device,
            &memory,
            vk::Extent2D {
                width: size.x,
                height: size.y,
            },
            vk::Format::R16G16_SFLOAT,
            vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
        )?);
        aa.sampler = device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .min_filter(vk::Filter::NEAREST)
                .mag_filter(vk::Filter::NEAREST),
            None,
        )?;
        let bindings: Vec<_> = (0..4)
            .map(|i| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i)
                    .descriptor_type(if i == 0 {
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER
                    } else {
                        vk::DescriptorType::STORAGE_IMAGE
                    })
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        aa.layout = device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        aa.pool = device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        descriptor_count: 1,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_IMAGE,
                        descriptor_count: 3,
                    },
                ]),
            None,
        )?;
        aa.set = device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(aa.pool)
                .set_layouts(&[aa.layout]),
        )?[0];
        let infos: Vec<_> = [input, aa.resources[0], aa.resources[1], output]
            .iter()
            .enumerate()
            .map(|(i, r)| {
                [vk::DescriptorImageInfo::default()
                    .sampler(if i == 0 {
                        aa.sampler
                    } else {
                        vk::Sampler::null()
                    })
                    .image_view(vk::ImageView::from_raw(r.view))
                    .image_layout(vk::ImageLayout::GENERAL)]
            })
            .collect();
        let writes: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(i, v)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(aa.set)
                    .dst_binding(i as u32)
                    .descriptor_type(if i == 0 {
                        vk::DescriptorType::COMBINED_IMAGE_SAMPLER
                    } else {
                        vk::DescriptorType::STORAGE_IMAGE
                    })
                    .image_info(v)
            })
            .collect();
        device.update_descriptor_sets(&writes, &[]);
        aa.pipeline_layout = device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[aa.layout])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(4)]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(
            "../shaders/xess_aa_color.spv"
        )))?;
        aa.shader = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        aa.pipeline = device
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .layout(aa.pipeline_layout)
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(aa.shader)
                            .name(c"main"),
                    )],
                None,
            )
            .map_err(|(p, e)| {
                for p in p {
                    device.destroy_pipeline(p, None);
                }
                e
            })?[0];
        if aa.bits > 0 {
            aa.queries = device.create_query_pool(
                &vk::QueryPoolCreateInfo::default()
                    .query_type(vk::QueryType::TIMESTAMP)
                    .query_count(4),
                None,
            )?;
        }
        trace::event!(
            "target_xess_aa_ready",
            json!({"backend":backend(),"input":[size.x,size.y],"output":[size.x,size.y],"preset":"native_aa","jitter":[0,0],"motion":"zero; no optical flow","depth":"not required with high-resolution motion","reset_every_frame":true,"temporal_reconstruction":false,"streamline_required":false,"color":"sRGB decode -> linear FP16 XeSS -> sRGB encode","sr_version":format!("{}.{}.{}",version.major,version.minor,version.patch),"dll_sha256":HASH,"temporary_texture_bytes":props.texture_bytes})
        );
        Ok(aa)
    }
    unsafe fn mark(&self, cmd: vk::CommandBuffer, index: u32) {
        if self.queries != vk::QueryPool::null() {
            self.device.cmd_write_timestamp(
                cmd,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                self.queries,
                index,
            );
        }
    }
    unsafe fn dispatch(&self, cmd: vk::CommandBuffer, encode: u32) {
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.set],
            &[],
        );
        self.device.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &encode.to_ne_bytes(),
        );
        self.device.cmd_dispatch(
            cmd,
            self.resources[0].width.div_ceil(8),
            self.resources[0].height.div_ceil(8),
            1,
        );
    }
    pub(super) unsafe fn record(&mut self, cmd: vk::CommandBuffer) -> Result<()> {
        let _internal = internal();
        let session = self.session.as_ref().ok_or("XeSS session closed")?;
        if self.queries != vk::QueryPool::null() {
            self.device.cmd_reset_query_pool(cmd, self.queries, 0, 4);
        }
        self.mark(cmd, 0);
        for r in &self.resources {
            transition(
                &self.device,
                cmd,
                vk::Image::from_raw(r.image),
                if self.initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::GENERAL,
            );
        }
        let motion = self.resources[2];
        if !self.initialized {
            transition(
                &self.device,
                cmd,
                vk::Image::from_raw(motion.image),
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            self.device.cmd_clear_color_image(
                cmd,
                vk::Image::from_raw(motion.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue { float32: [0.0; 4] },
                &[range()],
            );
            transition(
                &self.device,
                cmd,
                vk::Image::from_raw(motion.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        self.dispatch(cmd, 0);
        self.mark(cmd, 1);
        for r in [self.resources[0], motion] {
            transition(
                &self.device,
                cmd,
                vk::Image::from_raw(r.image),
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
        let info = |r: Resource| x::ImageInfo {
            view: vk::ImageView::from_raw(r.view),
            image: vk::Image::from_raw(r.image),
            range: range(),
            format: vk::Format::from_raw(r.format as i32),
            width: r.width,
            height: r.height,
        };
        x::check(
            (session.api.execute)(
                session.handle,
                cmd,
                &x::Execute {
                    color: info(self.resources[0]),
                    motion: info(motion),
                    output: info(self.resources[1]),
                    exposure_scale: 1.0,
                    reset: 1,
                    width: motion.width,
                    height: motion.height,
                    ..Default::default()
                },
            ),
            "XeSS AA execute",
        )?;
        for r in [self.resources[0], motion] {
            transition(
                &self.device,
                cmd,
                vk::Image::from_raw(r.image),
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        transition(
            &self.device,
            cmd,
            vk::Image::from_raw(self.resources[1].image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::GENERAL,
        );
        self.mark(cmd, 2);
        // The output transition also separates XeSS from the encode dispatch.
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)],
            &[],
            &[],
        );
        self.dispatch(cmd, 1);
        self.mark(cmd, 3);
        self.initialized = true;
        Ok(())
    }
    pub(super) unsafe fn profile(&self, frame: u32) {
        if self.queries == vk::QueryPool::null() {
            return;
        }
        let mut ticks = [0u64; 4];
        if self
            .device
            .get_query_pool_results(self.queries, 0, &mut ticks, vk::QueryResultFlags::TYPE_64)
            .is_ok()
        {
            let mask = u64::MAX >> (64 - self.bits);
            let times: [f64; 3] = std::array::from_fn(|i| {
                (ticks[i + 1].wrapping_sub(ticks[i]) & mask) as f64 * self.period as f64 / 1000.0
            });
            trace::event!(
                "target_xess_aa_profile",
                json!({"frame":frame,"backend":backend(),"gpu_us":{"decode":times[0],"xess_evaluate":times[1],"encode":times[2],"total":times.iter().sum::<f64>()},"jitter":[0,0]})
            );
        }
    }
}
impl Drop for Aa {
    fn drop(&mut self) {
        unsafe {
            if let Some(session) = self.session.take() {
                if session.close().is_err() {
                    std::process::abort();
                }
            }
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_shader_module(self.shader, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_descriptor_pool(self.pool, None);
            self.device.destroy_descriptor_set_layout(self.layout, None);
            self.device.destroy_sampler(self.sampler, None);
            if self.queries != vk::QueryPool::null() {
                self.device.destroy_query_pool(self.queries, None);
            }
            for r in self.resources.drain(..) {
                self.device
                    .destroy_image_view(vk::ImageView::from_raw(r.view), None);
                self.device
                    .destroy_image(vk::Image::from_raw(r.image), None);
                self.device
                    .free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
        }
    }
}
