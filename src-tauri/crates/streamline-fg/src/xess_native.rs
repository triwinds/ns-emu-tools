//! Process-scoped native WSI path, independent of Streamline and optical flow.
use super::*;
use crate::fg_api::*;
use std::{
    ffi::c_void,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub(super) fn enabled() -> bool {
    if target_xess_aa::enabled() {
        return true;
    }
    #[cfg(feature = "xess-fg")]
    {
        return xess_fg::enabled();
    }
    #[cfg(not(feature = "xess-fg"))]
    false
}
use crate::advanced_settings::InputSizing;
static SIZING: OnceLock<Mutex<(InputSizing, u64)>> = OnceLock::new();
fn sizing() -> (InputSizing, u64) {
    *SIZING
        .get_or_init(|| {
            Mutex::new((
                std::env::var("NS_EMU_XESS_INPUT_SIZING")
                    .ok()
                    .and_then(|s| serde_json::from_str::<InputSizing>(&s).ok())
                    .filter(|s| s.valid())
                    .unwrap_or_default(),
                0,
            ))
        })
        .lock()
        .unwrap()
}
static CHAINS: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Chain>>>>> = OnceLock::new();
static FAMILIES: OnceLock<Mutex<HashMap<u64, u32>>> = OnceLock::new();
static FRAME: AtomicU32 = AtomicU32::new(1);
static ON: AtomicBool = AtomicBool::new(true);
static STOP: AtomicBool = AtomicBool::new(false);
static WORKER: OnceLock<Mutex<Option<std::thread::JoinHandle<()>>>> = OnceLock::new();
fn chains() -> std::sync::MutexGuard<'static, HashMap<u64, Arc<Mutex<Chain>>>> {
    CHAINS.get_or_init(Default::default).lock().unwrap()
}
fn failure(stage: &str, error: impl std::fmt::Display) {
    trace::event!(
        "xess_native_failure",
        json!({"stage":stage,"error":error.to_string()})
    );
}
fn controls() {
    let mut worker = WORKER.get_or_init(Default::default).lock().unwrap();
    if worker.is_none() {
        ON.store(
            std::env::var("NS_EMU_XESS_AA_INITIAL_OFF").as_deref() != Ok("1"),
            Ordering::Relaxed,
        );
        let Some(dir) = std::env::var_os("NS_STREAMLINE_LIVE_DIR").map(std::path::PathBuf::from)
        else {
            return;
        };
        STOP.store(false, Ordering::Release);
        *worker = Some(std::thread::spawn(move || {
            while !STOP.load(Ordering::Acquire) {
                if let Ok(bytes) = std::fs::read(dir.join("xess-control.json")) {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        if let Some(on) = value["enabled"].as_bool() {
                            ON.store(on, Ordering::Relaxed);
                        }
                        crate::frame_capture::request(&value);
                    }
                }
                if let Ok(bytes) = std::fs::read(dir.join("xess-input-control.json")) {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        if let (Ok(input), Some(revision)) = (
                            serde_json::from_value::<InputSizing>(value["inputSizing"].clone()),
                            value["revision"].as_u64(),
                        ) {
                            if input.valid() {
                                let _ = sizing();
                                let mut current = SIZING.get().unwrap().lock().unwrap();
                                if revision > current.1 {
                                    *current = (input, revision);
                                }
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }));
    }
}
pub(super) unsafe fn create_instance(
    next: vk::PFN_vkCreateInstance,
    info: *const vk::InstanceCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::Instance,
) -> vk::Result {
    let _internal = target_xess_aa::internal();
    let result = (|| -> Result<vk::Result> {
        let api = target_xess_aa::sdk()?;
        let mut count = 0;
        let mut names = std::ptr::null();
        let mut version = 0;
        xess_api::check(
            (api.instance_extensions)(&mut count, &mut names, &mut version),
            "XeSS instance extensions",
        )?;
        let mut extensions: Vec<_> = scale_probe::items(
            (*info).pp_enabled_extension_names,
            (*info).enabled_extension_count,
        )
        .iter()
        .map(|p| CStr::from_ptr(*p).to_owned())
        .collect();
        if count > 0 {
            for p in std::slice::from_raw_parts(names, count as usize) {
                let name = CStr::from_ptr(*p).to_owned();
                if !extensions.contains(&name) {
                    extensions.push(name);
                }
            }
        }
        let pointers: Vec<_> = extensions.iter().map(|s| s.as_ptr()).collect();
        let mut app = if (*info).p_application_info.is_null() {
            vk::ApplicationInfo::default()
        } else {
            *(*info).p_application_info
        };
        app.api_version = app.api_version.max(version);
        #[cfg(feature = "xess-fg")]
        if xess_fg::requested() {
            app.api_version = app.api_version.max(vk::API_VERSION_1_2);
        }
        Ok(next(
            &(*info)
                .application_info(&app)
                .enabled_extension_names(&pointers),
            alloc,
            out,
        ))
    })();
    match result {
        Ok(r) => r,
        Err(e) => {
            failure("instance", e);
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
    let _internal = target_xess_aa::internal();
    let result = (|| -> Result<vk::Result> {
        let instance = ash::Instance::load_with(
            |name| {
                (parent.gipa)(parent.handle, name.as_ptr())
                    .map_or(std::ptr::null(), |f| f as *const _)
            },
            parent.handle,
        );
        let mut names: Vec<_> = scale_probe::items(
            (*info).pp_enabled_extension_names,
            (*info).enabled_extension_count,
        )
        .iter()
        .map(|p| CStr::from_ptr(*p).to_owned())
        .collect();
        for name in target_xess_aa::device_extensions(&instance, physical)? {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let pointers: Vec<_> = names.iter().map(|n| n.as_ptr()).collect();
        let mut storage = super::vulkan_features::copy_features((*info).p_next)?;
        let mut head = storage
            .first_mut()
            .map_or(std::ptr::null_mut(), |n| n.as_mut_ptr().cast());
        let mut core = vk::PhysicalDeviceFeatures2::default().features(
            if (*info).p_enabled_features.is_null() {
                vk::PhysicalDeviceFeatures::default()
            } else {
                *(*info).p_enabled_features
            },
        );
        let mut link = &mut head as *mut *mut c_void;
        while !(*link).is_null() {
            let node = (*link).cast::<vk::BaseOutStructure>();
            if (*node).s_type == vk::StructureType::PHYSICAL_DEVICE_FEATURES_2 {
                if !(*info).p_enabled_features.is_null() {
                    return Err("duplicate core feature sources".into());
                }
                core.features = (*node.cast::<vk::PhysicalDeviceFeatures2>()).features;
                *link = (*node).p_next.cast();
                break;
            }
            link = (&mut (*node).p_next as *mut *mut vk::BaseOutStructure).cast();
        }
        if instance
            .get_physical_device_features(physical)
            .shader_storage_image_extended_formats
            == 0
        {
            return Err("FP16 storage image support required".into());
        }
        core.features.shader_storage_image_extended_formats = vk::TRUE;
        core.p_next = head;
        head = (&mut core as *mut vk::PhysicalDeviceFeatures2).cast();
        target_xess_aa::features(&instance, physical, &mut head)?;
        let mut copy = (*info).enabled_extension_names(&pointers);
        copy.p_enabled_features = std::ptr::null();
        copy.p_next = head;
        let props = instance.get_physical_device_properties(physical);
        trace::event!(
            "xess_native_device_plan",
            json!({"vendor_id":props.vendor_id,"device_id":props.device_id,"device_extensions":names.iter().map(|n|n.to_string_lossy()).collect::<Vec<_>>(),"streamline":false,"optical_flow":false})
        );
        #[cfg(feature = "xess-fg")]
        if xess_fg::requested() {
            return Ok(xess_fg::create_device(
                parent, next, physical, &copy, alloc, out,
            ));
        }
        Ok(next(physical, &copy, alloc, out))
    })();
    match result {
        Ok(r) => r,
        Err(e) => {
            failure("device", e);
            vk::Result::ERROR_INITIALIZATION_FAILED
        }
    }
}
pub(super) unsafe fn device_created(handle: vk::Device, info: *const vk::DeviceCreateInfo) {
    let Some(d) = device(handle) else {
        return;
    };
    let Some(parent) = instance(d.physical) else {
        return;
    };
    let instance = ash::Instance::load_with(
        |n| (parent.gipa)(parent.handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const _),
        parent.handle,
    );
    let props = instance.get_physical_device_queue_family_properties(d.physical);
    let family = scale_probe::items(
        (*info).p_queue_create_infos,
        (*info).queue_create_info_count,
    )
    .iter()
    .map(|q| q.queue_family_index)
    .find(|&f| {
        props.get(f as usize).is_some_and(|p| {
            p.queue_flags
                .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
        })
    })
    .unwrap_or(u32::MAX);
    FAMILIES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(handle.as_raw(), family);
}
pub(super) unsafe fn swapchain_usage(
    d: Device,
    info: &vk::SwapchainCreateInfoKHR,
) -> vk::ImageUsageFlags {
    let Some(parent) = instance(d.physical) else {
        return info.image_usage;
    };
    let Some(f) = (parent.gipa)(
        parent.handle,
        c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR".as_ptr(),
    ) else {
        return info.image_usage;
    };
    let f: vk::PFN_vkGetPhysicalDeviceSurfaceCapabilitiesKHR = std::mem::transmute(f);
    let mut caps = vk::SurfaceCapabilitiesKHR::default();
    let required = vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST;
    if f(d.physical, info.surface, &mut caps) == vk::Result::SUCCESS
        && caps.supported_usage_flags.contains(required)
    {
        info.image_usage | required
    } else {
        info.image_usage
    }
}
struct Chain {
    d: Device,
    size: vk::Extent2D,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
    renderer: Option<Renderer>,
    failed: bool,
    attempted: Option<(InputSizing, u64)>,
    scale_error: Option<String>,
}
pub(super) fn swapchain_created(
    d: Device,
    swap: vk::SwapchainKHR,
    info: &vk::SwapchainCreateInfoKHR,
) {
    chains().insert(
        swap.as_raw(),
        Arc::new(Mutex::new(Chain {
            d,
            size: info.image_extent,
            format: info.image_format,
            usage: info.image_usage,
            renderer: None,
            failed: false,
            attempted: None,
            scale_error: None,
        })),
    );
}
pub(super) unsafe fn destroy_swapchain(device: vk::Device, swap: vk::SwapchainKHR) {
    if let Some(chain) = chains().remove(&swap.as_raw()) {
        let mut chain = chain.lock().unwrap();
        if let Some(renderer) = chain.renderer.take() {
            if queue_sync::native(device, || renderer.device.device_wait_idle()).is_err() {
                std::process::abort();
            }
            drop(renderer);
        }
    }
}
pub(super) unsafe fn destroy_device(device: vk::Device) {
    let swaps: Vec<_> = chains()
        .iter()
        .filter(|(_, c)| c.lock().unwrap().d.handle == device)
        .map(|(k, _)| *k)
        .collect();
    for swap in swaps {
        destroy_swapchain(device, vk::SwapchainKHR::from_raw(swap));
    }
    let empty = {
        let mut families = FAMILIES.get_or_init(Default::default).lock().unwrap();
        families.remove(&device.as_raw());
        families.is_empty()
    };
    if empty {
        let mut worker = WORKER.get_or_init(Default::default).lock().unwrap();
        STOP.store(true, Ordering::Release);
        if let Some(thread) = worker.take() {
            let _ = thread.join();
        }
    }
}
pub(super) unsafe fn present(
    d: Device,
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
    next: vk::PFN_vkQueuePresentKHR,
) -> vk::Result {
    if info.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let prepared = prepare_present(queue, info);
    let present_start = Instant::now();
    let result = queue_sync::native(d.handle, || next(queue, &prepared.info));
    prepared.record(result, present_start.elapsed().as_micros());
    result
}
pub(super) struct Prepared<'a> {
    pub info: vk::PresentInfoKHR<'a>,
    frame: u32,
    on: bool,
    active: bool,
    processing_us: u128,
}
impl Prepared<'_> {
    pub(super) fn record(&self, result: vk::Result, present_us: u128) {
        trace::event!(
            "xess_native_frame",
            json!({"frame":self.frame,"requested":self.on,"active":self.active,"reset_history":self.active,"zero_motion":true,"optical_flow":false,"processing_cpu_us":self.processing_us,"native_present_us":present_us,"result":result.as_raw()})
        );
    }
}
pub(super) unsafe fn prepare_present<'a>(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR<'a>,
) -> Prepared<'a> {
    controls();
    let frame = FRAME.fetch_add(1, Ordering::Relaxed);
    let started = Instant::now();
    let on = ON.load(Ordering::Relaxed) && target_xess_aa::enabled();
    let desired = sizing();
    let mut forwarded = *info;
    let mut active = false;
    if (*info).swapchain_count == 1
        && !(*info).p_swapchains.is_null()
        && !(*info).p_image_indices.is_null()
    {
        let swap = *(*info).p_swapchains;
        let record = { chains().get(&swap.as_raw()).cloned() };
        if let Some(record) = record {
            let mut chain = record.lock().unwrap();
            if chain.size.width >= 64 && chain.size.height >= 64 {
                if chain.attempted != Some(desired) {
                    chain.attempted = Some(desired);
                    chain.scale_error = None;
                    match Renderer::new(&chain, swap, desired.0) {
                        Ok(r) => {
                            if let Some(old) = chain.renderer.take() {
                                if queue_sync::native(old.owner, || old.device.device_wait_idle())
                                    .is_err()
                                {
                                    std::process::abort();
                                }
                                drop(old);
                            }
                            chain.renderer = Some(r);
                            chain.failed = false;
                        }
                        Err(e) => {
                            chain.scale_error = Some(e.to_string());
                        }
                    }
                }
                let record_failed = chain.failed;
                let mut applied = InputSizing::default();
                let mut low = chain.size;
                if let Some(renderer) = chain.renderer.as_mut().filter(|_| !record_failed) {
                    applied = renderer.sizing;
                    low = renderer.input_size;
                    if on || renderer.input_size != renderer.source_size {
                        match renderer.run(queue, &*info, frame, on) {
                            Ok(()) => {
                                forwarded.wait_semaphore_count = 0;
                                forwarded.p_wait_semaphores = std::ptr::null();
                                active = on && renderer.aa.is_some();
                            }
                            Err(e) => {
                                failure("frame", e);
                                if renderer.submitted {
                                    std::process::abort();
                                }
                                chain.failed = true;
                            }
                        }
                    }
                }
                let applied_revision = if applied == desired.0 || chain.scale_error.is_some() {
                    desired.1
                } else {
                    0
                };
                trace::event!(
                    "xess_input_scale",
                    json!({"active":low != chain.size,"scalePercent":applied.scale_percent,"maxEdge":applied.max_edge,
                    "originalExtent":[chain.size.width,chain.size.height],"inputExtent":[low.width,low.height],"outputExtent":[chain.size.width,chain.size.height],
                    "appliedRevision":applied_revision,"error":chain.scale_error,"fgExtent":"presentation_output","startupOnly":false})
                );
            }
        }
    }
    Prepared {
        info: forwarded,
        frame,
        on,
        active,
        processing_us: started.elapsed().as_micros(),
    }
}

struct Renderer {
    device: ash::Device,
    sizing: InputSizing,
    source_size: vk::Extent2D,
    input_size: vk::Extent2D,
    images: Vec<vk::Image>,
    resources: Vec<Resource>,
    aa: Option<target_xess_aa::Aa>,
    capture: Option<frame_capture::Capture>,
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    initialized: bool,
    submitted: bool,
    owner: vk::Device,
}
impl Renderer {
    unsafe fn new(chain: &Chain, swap: vk::SwapchainKHR, sizing: InputSizing) -> Result<Self> {
        let low = crate::input_dimensions::resized(chain.size, sizing)?;
        if chain.size.width < 64 || chain.size.height < 64 {
            return Err("loading surface too small".into());
        }
        let raw_format = match chain.format {
            vk::Format::B8G8R8A8_UNORM | vk::Format::B8G8R8A8_SRGB => vk::Format::B8G8R8A8_UNORM,
            vk::Format::R8G8B8A8_UNORM | vk::Format::R8G8B8A8_SRGB => vk::Format::R8G8B8A8_UNORM,
            _ => return Err("XeSS diagnostic supports SDR 8-bit surfaces only".into()),
        };
        if !chain
            .usage
            .contains(vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST)
        {
            return Err("surface cannot copy color for AA".into());
        }
        let d = chain.d;
        let parent = instance(d.physical).ok_or("missing parent instance")?;
        let instance = ash::Instance::load_with(
            |n| {
                (parent.gipa)(parent.handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const _)
            },
            parent.handle,
        );
        let device = ash::Device::load_with(
            |n| (d.gdpa)(d.handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const _),
            d.handle,
        );
        let family = *FAMILIES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .get(&d.handle.as_raw())
            .ok_or("missing graphics queue family")?;
        if family == u32::MAX {
            return Err("combined graphics/compute queue required".into());
        }
        let blit = vk::FormatFeatureFlags::BLIT_SRC
            | vk::FormatFeatureFlags::BLIT_DST
            | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
        for format in [chain.format, raw_format, vk::Format::R8G8B8A8_UNORM] {
            if !instance
                .get_physical_device_format_properties(d.physical, format)
                .optimal_tiling_features
                .contains(blit)
            {
                return Err(format!("unsupported XeSS input scaling format {format:?}").into());
            }
        }
        let props = instance.get_physical_device_memory_properties(d.physical);
        let mut r = Self {
            device,
            sizing,
            source_size: chain.size,
            input_size: low,
            images: Vec::new(),
            resources: Vec::new(),
            aa: None,
            capture: None,
            pool: vk::CommandPool::null(),
            cmd: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            initialized: false,
            submitted: false,
            owner: d.handle,
        };
        let usage = vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        for (format, usage) in [
            (raw_format, usage),
            (
                vk::Format::R8G8B8A8_UNORM,
                usage | vk::ImageUsageFlags::STORAGE,
            ),
            (raw_format, usage),
        ] {
            r.resources
                .push(texture_with_usage(&r.device, &props, low, format, usage)?);
        }
        let get: vk::PFN_vkGetSwapchainImagesKHR = std::mem::transmute(
            (d.gdpa)(d.handle, c"vkGetSwapchainImagesKHR".as_ptr())
                .ok_or("missing swapchain image enumeration")?,
        );
        let mut count = 0;
        get(d.handle, swap, &mut count, std::ptr::null_mut()).result()?;
        r.images.resize(count as usize, vk::Image::null());
        get(d.handle, swap, &mut count, r.images.as_mut_ptr()).result()?;
        r.images.truncate(count as usize);
        r.pool = r.device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        r.cmd = r.device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(r.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        d.set_loader_data
            .ok_or("loader callback required for private XeSS command buffer")?(
            d.handle,
            r.cmd.as_raw() as *mut c_void,
        )
        .result()?;
        r.fence = r
            .device
            .create_fence(&vk::FenceCreateInfo::default(), None)?;
        if target_xess_aa::enabled() {
            r.aa = Some(target_xess_aa::Aa::new(
                &instance,
                d.physical,
                &r.device,
                family,
                r.resources[0],
                r.resources[1],
            )?);
        }
        r.capture = frame_capture::Capture::new(&r.device, &props, &r.resources[..2])?;
        trace::event!(
            "xess_native_ready",
            json!({"size":[low.width,low.height],"output":[chain.size.width,chain.size.height],"swapchain_format":chain.format.as_raw(),"family":family,"nvidia_runtime":false,"streamline":false,"optical_flow":false,"jitter":[0,0],"motion":[0,0],"reset_every_frame":true})
        );
        Ok(r)
    }
    unsafe fn run(
        &mut self,
        queue: vk::Queue,
        info: &vk::PresentInfoKHR,
        frame: u32,
        aa_on: bool,
    ) -> Result<()> {
        self.submitted = false;
        let src = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("invalid presented image index")?;
        let d = &self.device;
        d.reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty())?;
        d.reset_fences(&[self.fence])?;
        d.begin_command_buffer(
            self.cmd,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        for r in &self.resources {
            transition(
                d,
                self.cmd,
                vk::Image::from_raw(r.image),
                if self.initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::GENERAL,
            );
        }
        let input = self.resources[0];
        let output = self.resources[1];
        let packed = self.resources[2];
        transition(
            d,
            self.cmd,
            src,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(input.image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let layers = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        let offsets = |size: vk::Extent2D| {
            [
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: size.width as i32,
                    y: size.height as i32,
                    z: 1,
                },
            ]
        };
        d.cmd_blit_image(
            self.cmd,
            src,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::Image::from_raw(input.image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageBlit::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .src_offsets(offsets(self.source_size))
                .dst_offsets(offsets(self.input_size))],
            vk::Filter::LINEAR,
        );
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(input.image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        if aa_on {
            self.aa.as_mut().ok_or("AA not loaded")?.record(self.cmd)?;
        }
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(if aa_on { output.image } else { input.image }),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(packed.image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        d.cmd_blit_image(
            self.cmd,
            vk::Image::from_raw(if aa_on { output.image } else { input.image }),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::Image::from_raw(packed.image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageBlit::default()
                .src_subresource(layers)
                .src_offsets(offsets(self.input_size))
                .dst_subresource(layers)
                .dst_offsets(offsets(self.input_size))],
            vk::Filter::NEAREST,
        );
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(packed.image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        transition(
            d,
            self.cmd,
            src,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        d.cmd_blit_image(
            self.cmd,
            vk::Image::from_raw(packed.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            src,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageBlit::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .src_offsets(offsets(self.input_size))
                .dst_offsets(offsets(self.source_size))],
            vk::Filter::LINEAR,
        );
        transition(
            d,
            self.cmd,
            src,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(if aa_on { output.image } else { input.image }),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(packed.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        let capture = aa_on && frame_capture::selected(frame) && self.capture.is_some();
        if capture {
            self.capture.as_ref().unwrap().record(self.cmd);
        }
        d.end_command_buffer(self.cmd)?;
        let waits = scale_probe::items(info.p_wait_semaphores, info.wait_semaphore_count);
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        self.submitted = true;
        queue_sync::native(self.owner, || {
            d.queue_submit(
                queue,
                &[vk::SubmitInfo::default()
                    .command_buffers(&[self.cmd])
                    .wait_semaphores(waits)
                    .wait_dst_stage_mask(&stages)],
                self.fence,
            )
        })?;
        d.wait_for_fences(&[self.fence], true, 30_000_000_000)?;
        self.initialized = true;
        if aa_on {
            self.aa.as_ref().unwrap().profile(frame);
        }
        if capture {
            self.capture.as_ref().unwrap().save("xess-native", frame)?;
        }
        Ok(())
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            drop(self.aa.take());
            drop(self.capture.take());
            for r in self.resources.drain(..) {
                self.device
                    .destroy_image_view(vk::ImageView::from_raw(r.view), None);
                self.device
                    .destroy_image(vk::Image::from_raw(r.image), None);
                self.device
                    .free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
        }
    }
}
