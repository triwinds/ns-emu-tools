//! Experimental native-window XeSS FG bridge. Zero motion/fixed depth only.
use super::*;
use crate::fg_api::{range, transition, Result};
use sha2::{Digest, Sha256};
use std::{
    ffi::c_void,
    fs::File,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

extern "C" {
    fn xess_fg_game_create(
        sdk: *const u16,
        agility: *const u16,
        window: *mut c_void,
        luid: *const u8,
        width: u32,
        height: u32,
        callback: unsafe extern "C" fn(*const c_char),
        out: *mut *mut c_void,
        color: *mut isize,
        fence: *mut isize,
    ) -> i32;
    fn xess_fg_game_present(
        runtime: *mut c_void,
        ready: u64,
        released: u64,
        enabled: u32,
        frames: *mut u32,
        result: *mut i32,
    ) -> i32;
    fn xess_fg_game_destroy(runtime: *mut c_void);
    fn xess_fg_game_wait(runtime: *mut c_void) -> i32;
    #[cfg(feature = "hybrid-xess-fg")]
    fn xess_fg_game_suspend(runtime: *mut c_void) -> i32;
    fn xess_fg_overlay_window(owner: *mut c_void, window: *mut isize) -> *mut c_void;
    fn xess_fg_overlay_destroy(overlay: *mut c_void);
    fn xess_fg_overlay_visible(overlay: *mut c_void, owner: *mut c_void, visible: u32);
    fn xess_fg_shadow_window(owner: *mut c_void) -> *mut c_void;
    fn xess_fg_shadow_sync(shadow: *mut c_void, owner: *mut c_void);
    fn xess_fg_shadow_destroy(shadow: *mut c_void);
}
pub(super) fn enabled() -> bool {
    if crate::dual_fg_enabled() {
        return false;
    }
    #[cfg(feature = "xess-native-aa")]
    if target_xess_aa::internal_active() {
        return false;
    }
    requested()
}
pub(super) fn requested() -> bool {
    std::env::var("NS_EMU_XESS_FG").as_deref() == Ok("1")
}
fn failure(stage: &str, error: impl std::fmt::Display) {
    trace::event!(
        "xess_fg_failure",
        json!({"stage":stage,"error":error.to_string()})
    );
}
unsafe extern "C" fn sdk_log(message: *const c_char) {
    let text = CStr::from_ptr(message).to_string_lossy();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
        trace::event!("xess_fg_runtime", value);
    }
}
static PINNED: OnceLock<Vec<File>> = OnceLock::new();
fn paths() -> Result<(Vec<u16>, Vec<u16>)> {
    let sdk = PathBuf::from(std::env::var_os("NS_EMU_XESS_FG_SDK").ok_or("missing FG SDK path")?);
    let agility =
        PathBuf::from(std::env::var_os("NS_EMU_XESS_FG_AGILITY").ok_or("missing Agility path")?);
    if !sdk.is_absolute() || !agility.is_absolute() {
        return Err("absolute runtime paths required".into());
    }
    if PINNED.get().is_none() {
        use std::{io::Read, os::windows::fs::OpenOptionsExt};
        let mut files = Vec::new();
        for (root, manifest) in [
            (
                &sdk,
                include_str!("../../../tools/xess-fg-diagnostics/sdk.json"),
            ),
            (
                &agility,
                include_str!("../../../tools/xess-fg-diagnostics/agility.json"),
            ),
        ] {
            let value: serde_json::Value = serde_json::from_str(manifest)?;
            for file in value["files"]
                .as_array()
                .ok_or("invalid runtime manifest")?
            {
                let name = file["path"].as_str().ok_or("invalid runtime file")?;
                if !name.ends_with(".dll") {
                    continue;
                }
                let mut handle = std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(1)
                    .open(root.join(name))?;
                let mut bytes = Vec::new();
                handle.read_to_end(&mut bytes)?;
                if format!("{:x}", Sha256::digest(bytes)) != file["sha256"] {
                    return Err(format!("FG runtime hash mismatch: {name}").into());
                }
                files.push(handle);
            }
        }
        let _ = PINNED.set(files);
    }
    let wide = |p: &std::path::Path| -> Result<Vec<u16>> {
        Ok(p.to_str()
            .ok_or("Unicode path required")?
            .encode_utf16()
            .chain(Some(0))
            .collect())
    };
    let mut agility_text = agility.to_str().ok_or("Unicode path required")?.to_string();
    if !agility_text.ends_with(['\\', '/']) {
        agility_text.push('\\');
    }
    Ok((
        wide(&sdk)?,
        agility_text.encode_utf16().chain(Some(0)).collect(),
    ))
}
#[derive(Clone, Copy)]
struct Surface {
    owner: isize,
    shadow: isize,
}
static SURFACES: OnceLock<Mutex<HashMap<u64, Surface>>> = OnceLock::new();
fn surfaces() -> std::sync::MutexGuard<'static, HashMap<u64, Surface>> {
    SURFACES.get_or_init(Default::default).lock().unwrap()
}
#[cfg(feature = "hybrid-xess-fg")]
pub(super) fn track_dual_surface(surface: vk::SurfaceKHR, owner: isize) {
    surfaces().insert(surface.as_raw(), Surface { owner, shadow: 0 });
}
pub(super) unsafe fn create_surface(
    instance: vk::Instance,
    next: vk::PFN_vkCreateWin32SurfaceKHR,
    info: *const vk::Win32SurfaceCreateInfoKHR,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::SurfaceKHR,
) -> vk::Result {
    if let Err(e) = paths() {
        failure("runtime verification", e);
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let shadow = xess_fg_shadow_window((*info).hwnd as *mut c_void);
    if shadow.is_null() {
        failure("shadow window", "creation failed");
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let mut copy = *info;
    copy.hwnd = shadow as isize;
    let result = next(instance, &copy, alloc, out);
    if result == vk::Result::SUCCESS {
        surfaces().insert(
            (*out).as_raw(),
            Surface {
                owner: (*info).hwnd,
                shadow: shadow as isize,
            },
        );
        trace::event!(
            "xess_fg_surface",
            json!({"surface":(*out).as_raw(),"original_window":(*info).hwnd,"shadow_window":shadow as usize})
        );
    } else {
        xess_fg_shadow_destroy(shadow);
    }
    result
}
fn sync_surface(surface: vk::SurfaceKHR) {
    if let Some(s) = surfaces().get(&surface.as_raw()).copied() {
        unsafe {
            xess_fg_shadow_sync(s.shadow as *mut c_void, s.owner as *mut c_void);
        }
    }
}
const _: vk::PFN_vkGetPhysicalDeviceSurfaceCapabilitiesKHR = get_surface_capabilities;
pub(super) unsafe extern "system" fn get_surface_capabilities(
    physical: vk::PhysicalDevice,
    surface: vk::SurfaceKHR,
    out: *mut vk::SurfaceCapabilitiesKHR,
) -> vk::Result {
    sync_surface(surface);
    let Some(parent) = instance(physical) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let f: vk::PFN_vkGetPhysicalDeviceSurfaceCapabilitiesKHR = std::mem::transmute(
        (parent.gipa)(
            parent.handle,
            c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR".as_ptr(),
        )
        .unwrap(),
    );
    f(physical, surface, out)
}
pub(super) unsafe fn destroy_surface(surface: vk::SurfaceKHR) {
    if let Some(s) = surfaces().remove(&surface.as_raw()) {
        if s.shadow != 0 {
            xess_fg_shadow_destroy(s.shadow as *mut c_void);
        }
    }
}
pub(super) unsafe fn create_instance(
    next: vk::PFN_vkCreateInstance,
    info: *const vk::InstanceCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::Instance,
) -> vk::Result {
    let mut app = if (*info).p_application_info.is_null() {
        vk::ApplicationInfo::default()
    } else {
        *(*info).p_application_info
    };
    app.api_version = app.api_version.max(vk::API_VERSION_1_2);
    next(&(*info).application_info(&app), alloc, out)
}
pub(super) unsafe fn create_device(
    parent: Instance,
    next: vk::PFN_vkCreateDevice,
    physical: vk::PhysicalDevice,
    info: *const vk::DeviceCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::Device,
) -> vk::Result {
    let result = (|| -> Result<vk::Result> {
        let instance = ash::Instance::load_with(
            |n| {
                (parent.gipa)(parent.handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const _)
            },
            parent.handle,
        );
        let mut supported = vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
        instance.get_physical_device_features2(
            physical,
            &mut vk::PhysicalDeviceFeatures2::default().push_next(&mut supported),
        );
        if supported.timeline_semaphore == 0 {
            return Err("timeline semaphores required for XeSS FG bridge".into());
        }
        let available = instance.enumerate_device_extension_properties(physical)?;
        let mut names: Vec<_> = scale_probe::items(
            (*info).pp_enabled_extension_names,
            (*info).enabled_extension_count,
        )
        .iter()
        .map(|p| CStr::from_ptr(*p).to_owned())
        .collect();
        for name in [
            ash::khr::external_memory_win32::NAME,
            ash::khr::external_semaphore_win32::NAME,
        ] {
            if !available
                .iter()
                .any(|p| CStr::from_ptr(p.extension_name.as_ptr()) == name)
            {
                return Err(format!("missing {}", name.to_string_lossy()).into());
            }
            if !names.iter().any(|n| n.as_c_str() == name) {
                names.push(name.to_owned());
            }
        }
        let pointers: Vec<_> = names.iter().map(|n| n.as_ptr()).collect();
        let mut storage = crate::vulkan_features::copy_features((*info).p_next)?;
        let mut head = storage
            .first_mut()
            .map_or(std::ptr::null_mut(), |p| p.as_mut_ptr().cast::<c_void>());
        let mut node = head.cast::<vk::BaseOutStructure>();
        let mut found = false;
        while !node.is_null() {
            match (*node).s_type {
                vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_2_FEATURES => {
                    (*node.cast::<vk::PhysicalDeviceVulkan12Features>()).timeline_semaphore =
                        vk::TRUE;
                    found = true;
                }
                vk::StructureType::PHYSICAL_DEVICE_TIMELINE_SEMAPHORE_FEATURES => {
                    (*node.cast::<vk::PhysicalDeviceTimelineSemaphoreFeatures>())
                        .timeline_semaphore = vk::TRUE;
                    found = true;
                }
                _ => {}
            }
            node = (*node).p_next;
        }
        let mut timeline =
            vk::PhysicalDeviceTimelineSemaphoreFeatures::default().timeline_semaphore(true);
        if !found {
            timeline.p_next = head;
            head = (&mut timeline as *mut vk::PhysicalDeviceTimelineSemaphoreFeatures).cast();
        }
        let mut copy = (*info).enabled_extension_names(&pointers);
        copy.p_next = head;
        trace::event!(
            "xess_fg_device_plan",
            json!({"extensions":names.iter().map(|n|n.to_string_lossy()).collect::<Vec<_>>(),"nvof":false,"streamline":false})
        );
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
static FAMILIES: OnceLock<Mutex<HashMap<u64, u32>>> = OnceLock::new();
pub(super) unsafe fn device_created(handle: vk::Device, info: *const vk::DeviceCreateInfo) {
    let d = device(handle).unwrap();
    let p = instance(d.physical).unwrap();
    let i = ash::Instance::load_with(
        |n| (p.gipa)(p.handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const _),
        p.handle,
    );
    let props = i.get_physical_device_queue_family_properties(d.physical);
    let family = scale_probe::items(
        (*info).p_queue_create_infos,
        (*info).queue_create_info_count,
    )
    .iter()
    .map(|q| q.queue_family_index)
    .find(|&f| {
        props[f as usize]
            .queue_flags
            .contains(vk::QueueFlags::GRAPHICS)
    })
    .unwrap_or(u32::MAX);
    FAMILIES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .insert(handle.as_raw(), family);
}
struct Chain {
    d: Device,
    size: vk::Extent2D,
    format: vk::Format,
    surface: vk::SurfaceKHR,
    usage: vk::ImageUsageFlags,
    renderer: Option<Renderer>,
}
static CHAINS: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Chain>>>>> = OnceLock::new();
fn chains() -> std::sync::MutexGuard<'static, HashMap<u64, Arc<Mutex<Chain>>>> {
    CHAINS.get_or_init(Default::default).lock().unwrap()
}
pub(super) unsafe fn before_swapchain(d: Device, info: &mut vk::SwapchainCreateInfoKHR) {
    sync_surface(info.surface);
    if info.old_swapchain != vk::SwapchainKHR::null() {
        destroy_swapchain(d.handle, info.old_swapchain);
    }
    let mut caps = vk::SurfaceCapabilitiesKHR::default();
    if get_surface_capabilities(d.physical, info.surface, &mut caps) == vk::Result::SUCCESS
        && caps
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_SRC)
    {
        info.image_usage |= vk::ImageUsageFlags::TRANSFER_SRC;
    }
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
            surface: info.surface,
            usage: info.image_usage,
            renderer: None,
        })),
    );
}
pub(super) unsafe fn destroy_swapchain(device: vk::Device, swap: vk::SwapchainKHR) {
    if let Some(chain) = chains().remove(&swap.as_raw()) {
        if let Some(renderer) = chain.lock().unwrap().renderer.take() {
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
        let mut worker = CONTROLS.get_or_init(Default::default).lock().unwrap();
        STOP.store(true, Ordering::Release);
        if let Some(thread) = worker.take() {
            // The Vulkan loader may unload this DLL immediately after device/instance
            // destruction. A detached sleeping worker would return into unmapped code.
            let _ = thread.join();
        }
    }
}
static ON: AtomicBool = AtomicBool::new(true);
#[cfg(feature = "hybrid-xess-fg")]
static HYBRID_STATUS: Mutex<Option<serde_json::Value>> = Mutex::new(None);
#[cfg(feature = "hybrid-xess-fg")]
pub(super) fn hybrid_status() -> Option<serde_json::Value> {
    HYBRID_STATUS.lock().unwrap().clone()
}
#[cfg(feature = "hybrid-xess-fg")]
pub(super) unsafe fn drain_dual(swap: vk::SwapchainKHR) -> Result<()> {
    let record = chains().get(&swap.as_raw()).cloned();
    if let Some(record) = record {
        if let Some(renderer) = record.lock().unwrap().renderer.as_ref() {
            if xess_fg_game_wait(renderer.runtime) != 0 {
                return Err("XeSS frame retirement failed".into());
            }
        }
    }
    Ok(())
}
#[cfg(feature = "hybrid-xess-fg")]
pub(super) unsafe fn suspend_dual(swap: vk::SwapchainKHR) -> Result<()> {
    let record = chains().get(&swap.as_raw()).cloned();
    if let Some(record) = record {
        if let Some(renderer) = record.lock().unwrap().renderer.as_mut() {
            renderer.suspend()?;
        }
    }
    Ok(())
}
#[cfg(feature = "hybrid-xess-fg")]
pub(super) unsafe extern "system" fn dual_present(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
) -> vk::Result {
    let Some(d) = device(queue) else {
        return vk::Result::ERROR_DEVICE_LOST;
    };
    let Some(next) = crate::target_runtime::device_proc(d.handle, c"vkQueuePresentKHR") else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let next: vk::PFN_vkQueuePresentKHR = std::mem::transmute(next);
    if crate::xess_output() {
        return present(d, queue, info, next);
    }
    if !info.is_null() {
        for swap in scale_probe::items((*info).p_swapchains, (*info).swapchain_count) {
            let record = chains().get(&swap.as_raw()).cloned();
            if let Some(record) = record {
                if let Some(renderer) = record.lock().unwrap().renderer.as_mut() {
                    if let Err(e) = renderer.suspend() {
                        failure("suspend", e);
                        std::process::abort();
                    }
                }
            }
        }
    }
    next(queue, info)
}
#[cfg(feature = "hybrid-xess-fg")]
pub(super) unsafe extern "system" fn hybrid_present(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
) -> vk::Result {
    let Some(d) = device(queue) else {
        return vk::Result::ERROR_DEVICE_LOST;
    };
    let next = std::mem::transmute((d.gdpa)(d.handle, c"vkQueuePresentKHR".as_ptr()).unwrap());
    if let Err(e) = crate::target_runtime::hybrid_common_present(queue, info, false) {
        failure("hybrid common pre-present", e);
        std::process::abort();
    }
    let result = present(d, queue, info, next);
    if let Err(e) = crate::target_runtime::hybrid_common_present(queue, info, true) {
        failure("hybrid common post-present", e);
        std::process::abort();
    }
    result
}
static STOP: AtomicBool = AtomicBool::new(false);
static CONTROLS: OnceLock<Mutex<Option<std::thread::JoinHandle<()>>>> = OnceLock::new();
fn controls() {
    let mut worker = CONTROLS.get_or_init(Default::default).lock().unwrap();
    if worker.is_none() {
        ON.store(
            std::env::var("NS_EMU_XESS_FG_INITIAL_OFF").as_deref() != Ok("1"),
            Ordering::Relaxed,
        );
        if let Some(root) = std::env::var_os("NS_STREAMLINE_LIVE_DIR") {
            STOP.store(false, Ordering::Release);
            *worker = Some(std::thread::spawn(move || {
                while !STOP.load(Ordering::Acquire) {
                    if let Ok(bytes) =
                        std::fs::read(PathBuf::from(&root).join("xess-fg-control.json"))
                    {
                        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                            if let Some(on) = value["enabled"].as_bool() {
                                ON.store(on, Ordering::Relaxed);
                            }
                        }
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }));
        }
    }
}
pub(super) unsafe fn present(
    d: Device,
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
    next: vk::PFN_vkQueuePresentKHR,
) -> vk::Result {
    if !crate::hybrid_enabled() && !crate::dual_fg_enabled() {
        controls();
    }
    if info.is_null() || (*info).swapchain_count != 1 {
        return next(queue, info);
    }
    let swap = *(*info).p_swapchains;
    let Some(record) = chains().get(&swap.as_raw()).cloned() else {
        return next(queue, info);
    };
    #[cfg(feature = "xess-native-aa")]
    let aa = xess_native::enabled().then(|| xess_native::prepare_present(queue, info));
    #[cfg(feature = "xess-native-aa")]
    let info = aa.as_ref().map_or(&*info, |p| &p.info);
    let mut chain = record.lock().unwrap();
    if chain.size.width < 64 || chain.size.height < 64 {
        return next(queue, info);
    }
    if chain.renderer.is_none() {
        match Renderer::new(&chain, swap) {
            Ok(r) => chain.renderer = Some(r),
            Err(e) => {
                failure("prepare", e);
                std::process::abort();
            }
        }
    }
    let renderer = chain.renderer.as_mut().unwrap();
    let started = Instant::now();
    let on = ON.load(Ordering::Relaxed);
    #[cfg(feature = "hybrid-xess-fg")]
    let on = if crate::hybrid_enabled() || crate::dual_fg_enabled() {
        crate::live::requested().0
    } else {
        on
    };
    let stats = match renderer.run(queue, &*info, on) {
        Ok(s) => s,
        Err(e) => {
            failure("frame", e);
            std::process::abort();
        }
    };
    let xess_cpu_us = started.elapsed().as_micros();
    let mut forwarded = *info;
    forwarded.wait_semaphore_count = 0;
    forwarded.p_wait_semaphores = std::ptr::null();
    let bookkeeping_started = Instant::now();
    let result = if crate::dual_fg_enabled() {
        next(queue, &forwarded)
    } else {
        queue_sync::native(d.handle, || next(queue, &forwarded))
    };
    let bookkeeping_cpu_us = bookkeeping_started.elapsed().as_micros();
    #[cfg(feature = "hybrid-xess-fg")]
    if crate::hybrid_enabled() || crate::dual_fg_enabled() {
        let native_base = u32::from(crate::dual_fg_enabled());
        for _ in 0..stats.0.saturating_sub(native_base) {
            crate::live::native();
        }
        *HYBRID_STATUS.lock().unwrap() = Some(
            json!({"backend":"xess","state":if on && stats.0 > 1 {"generated_observed"} else if on {"original_only_observed"} else {"off"},
            "generationObserved":on && stats.0 > 1,"requested":on,"suspended":false,"sdkPresented":stats.0,"maximumGenerated":1,
            "guidance":"zero_motion_fixed_depth","fixedDepth":0.5,"motion":"zero","reflexEnabled":false,"xellEnabled":true,"fgResult":stats.1}),
        );
    }
    #[cfg(feature = "xess-native-aa")]
    if let Some(aa) = aa {
        aa.record(result, started.elapsed().as_micros());
    }
    trace::event!(
        "xess_fg_frame",
        json!({"frame":renderer.frame,"enabled":on,"frames_queued":stats.0,"fg_result":stats.1,"zero_motion":true,"fixed_depth":0.5,"processing_cpu_us":started.elapsed().as_micros(),"xess_cpu_us":xess_cpu_us,"bookkeeping_cpu_us":bookkeeping_cpu_us,"native_result":result.as_raw()})
    );
    result
}
struct Renderer {
    device: ash::Device,
    owner: vk::Device,
    runtime: *mut c_void,
    image: vk::Image,
    memory: vk::DeviceMemory,
    semaphore: vk::Semaphore,
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    images: Vec<vk::Image>,
    source_size: vk::Extent2D,
    size: vk::Extent2D,
    family: u32,
    frame: u64,
    submitted: bool,
    initialized: bool,
    overlay: isize,
    window: isize,
    active: bool,
}
// The owning chain mutex serializes every FFI call and private GPU submission.
unsafe impl Send for Renderer {}
impl Renderer {
    unsafe fn new(chain: &Chain, swap: vk::SwapchainKHR) -> Result<Self> {
        if !chain.usage.contains(vk::ImageUsageFlags::TRANSFER_SRC) {
            return Err("game surface transfer source unsupported".into());
        }
        if ![vk::Format::R8G8B8A8_UNORM, vk::Format::B8G8R8A8_UNORM].contains(&chain.format) {
            return Err("XeSS FG test currently requires SDR UNORM RGBA/BGRA".into());
        }
        let parent = instance(chain.d.physical).ok_or("missing parent instance")?;
        let instance = ash::Instance::load_with(
            |n| {
                (parent.gipa)(parent.handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const _)
            },
            parent.handle,
        );
        let d = chain.d;
        let device = ash::Device::load_with(
            |n| (d.gdpa)(d.handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const _),
            d.handle,
        );
        let family = *FAMILIES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .get(&d.handle.as_raw())
            .ok_or("missing present queue family")?;
        if family == u32::MAX {
            return Err("graphics queue required".into());
        }
        let surface = surfaces()
            .get(&chain.surface.as_raw())
            .copied()
            .ok_or("missing original game window")?;
        let size = chain.size;
        let mut ids = vk::PhysicalDeviceIDProperties::default();
        instance.get_physical_device_properties2(
            d.physical,
            &mut vk::PhysicalDeviceProperties2::default().push_next(&mut ids),
        );
        if ids.device_luid_valid == 0 {
            return Err("Vulkan adapter has no valid D3D12 LUID".into());
        }
        let (sdk, agility) = paths()?;
        let mut r = Self {
            device,
            owner: d.handle,
            runtime: std::ptr::null_mut(),
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            semaphore: vk::Semaphore::null(),
            pool: vk::CommandPool::null(),
            cmd: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            images: Vec::new(),
            source_size: chain.size,
            size,
            family,
            frame: 0,
            submitted: false,
            initialized: false,
            overlay: 0,
            window: surface.owner,
            active: true,
        };
        let mut overlay_hwnd = 0;
        if crate::dual_fg_enabled() {
            r.overlay =
                xess_fg_overlay_window(surface.owner as *mut c_void, &mut overlay_hwnd) as isize;
            if r.overlay == 0 {
                return Err("XeSS overlay creation failed".into());
            }
        }
        let (mut color, mut fence) = (0isize, 0isize);
        if xess_fg_game_create(
            sdk.as_ptr(),
            agility.as_ptr(),
            if r.overlay != 0 {
                overlay_hwnd
            } else {
                surface.owner
            } as *mut c_void,
            ids.device_luid.as_ptr(),
            size.width,
            size.height,
            sdk_log,
            &mut r.runtime,
            &mut color,
            &mut fence,
        ) != 0
        {
            return Err("D3D12 game FG initialization failed".into());
        }
        let d = &r.device;
        let mut external = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::D3D12_RESOURCE);
        r.image = d.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width: size.width,
                    height: size.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::TRANSFER_DST
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                )
                .push_next(&mut external),
            None,
        )?;
        let requirements = d.get_image_memory_requirements(r.image);
        let mut props = vk::MemoryWin32HandlePropertiesKHR::default();
        ash::khr::external_memory_win32::Device::new(&instance, d)
            .get_memory_win32_handle_properties(
                vk::ExternalMemoryHandleTypeFlags::D3D12_RESOURCE,
                color,
                &mut props,
            )?;
        let bits = props.memory_type_bits & requirements.memory_type_bits;
        let memory_type = (0..32)
            .find(|n| bits & (1 << n) != 0)
            .ok_or("no shared memory type")?;
        let mut import = vk::ImportMemoryWin32HandleInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::D3D12_RESOURCE)
            .handle(color);
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(r.image);
        r.memory = d.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type)
                .push_next(&mut import)
                .push_next(&mut dedicated),
            None,
        )?;
        d.bind_image_memory(r.image, r.memory, 0)?;
        let mut timeline =
            vk::SemaphoreTypeCreateInfo::default().semaphore_type(vk::SemaphoreType::TIMELINE);
        r.semaphore = d.create_semaphore(
            &vk::SemaphoreCreateInfo::default().push_next(&mut timeline),
            None,
        )?;
        ash::khr::external_semaphore_win32::Device::new(&instance, d)
            .import_semaphore_win32_handle(
                &vk::ImportSemaphoreWin32HandleInfoKHR::default()
                    .semaphore(r.semaphore)
                    .handle_type(vk::ExternalSemaphoreHandleTypeFlags::D3D12_FENCE)
                    .handle(fence),
            )?;
        let get: vk::PFN_vkGetSwapchainImagesKHR = std::mem::transmute(
            if crate::dual_fg_enabled() {
                #[cfg(feature = "hybrid-xess-fg")]
                {
                    crate::target_runtime::device_proc(chain.d.handle, c"vkGetSwapchainImagesKHR")
                }
                #[cfg(not(feature = "hybrid-xess-fg"))]
                {
                    None
                }
            } else {
                (chain.d.gdpa)(chain.d.handle, c"vkGetSwapchainImagesKHR".as_ptr())
            }
            .ok_or("missing swapchain images")?,
        );
        let mut count = 0;
        get(chain.d.handle, swap, &mut count, std::ptr::null_mut()).result()?;
        r.images.resize(count as usize, vk::Image::null());
        get(chain.d.handle, swap, &mut count, r.images.as_mut_ptr()).result()?;
        r.images.truncate(count as usize);
        r.pool = d.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        r.cmd = d.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(r.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        chain
            .d
            .set_loader_data
            .ok_or("private command loader callback required")?(
            chain.d.handle,
            r.cmd.as_raw() as *mut c_void,
        )
        .result()?;
        r.fence = d.create_fence(&vk::FenceCreateInfo::default(), None)?;
        trace::event!(
            "xess_fg_bridge_ready",
            json!({"input":[chain.size.width,chain.size.height],"fg_output":[size.width,size.height],"input_sizing_stage":"before_aa_full_size_fg","jitter":[0,0],"motion":[0,0],"depth":0.5,"original_window_presentation":true,"nvof":false,"streamline":false})
        );
        Ok(r)
    }
    #[cfg(feature = "hybrid-xess-fg")]
    unsafe fn suspend(&mut self) -> Result<()> {
        if !self.active {
            return Ok(());
        }
        if xess_fg_game_suspend(self.runtime) != 0 {
            return Err("XeSS suspension failed".into());
        }
        if self.overlay != 0 {
            xess_fg_overlay_visible(self.overlay as *mut c_void, self.window as *mut c_void, 0);
        }
        self.active = false;
        Ok(())
    }
    unsafe fn run(
        &mut self,
        queue: vk::Queue,
        info: &vk::PresentInfoKHR,
        on: bool,
    ) -> Result<(u32, i32)> {
        let d = &self.device;
        if self.submitted {
            d.wait_for_fences(&[self.fence], true, 30_000_000_000)?;
            self.submitted = false;
        }
        d.reset_fences(&[self.fence])?;
        d.reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty())?;
        d.begin_command_buffer(
            self.cmd,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        let src = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("invalid game image")?;
        transition(
            d,
            self.cmd,
            src,
            if crate::dual_fg_enabled() {
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL
            } else {
                vk::ImageLayout::PRESENT_SRC_KHR
            },
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let acquire = vk::ImageMemoryBarrier::default()
            .image(self.image)
            .subresource_range(range())
            .old_layout(if self.initialized {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::UNDEFINED
            })
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
            .dst_queue_family_index(self.family)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
        d.cmd_pipeline_barrier(
            self.cmd,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[acquire],
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
            self.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageBlit::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .src_offsets(offsets(self.source_size))
                .dst_offsets(offsets(self.size))],
            vk::Filter::LINEAR,
        );
        let release = vk::ImageMemoryBarrier::default()
            .image(self.image)
            .subresource_range(range())
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(self.family)
            .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE);
        d.cmd_pipeline_barrier(
            self.cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[release],
        );
        transition(
            d,
            self.cmd,
            src,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            if crate::dual_fg_enabled() {
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL
            } else {
                vk::ImageLayout::PRESENT_SRC_KHR
            },
        );
        d.end_command_buffer(self.cmd)?;
        let mut waits =
            scale_probe::items(info.p_wait_semaphores, info.wait_semaphore_count).to_vec();
        let mut values = vec![0; waits.len()];
        if self.frame > 0 {
            waits.push(self.semaphore);
            values.push(self.frame * 2);
        }
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        let signals = [self.semaphore];
        let signal_values = [self.frame * 2 + 1];
        let cmds = [self.cmd];
        let mut timeline = vk::TimelineSemaphoreSubmitInfo::default()
            .wait_semaphore_values(&values)
            .signal_semaphore_values(&signal_values);
        self.submitted = true;
        queue_sync::native(self.owner, || {
            d.queue_submit(
                queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&waits)
                    .wait_dst_stage_mask(&stages)
                    .signal_semaphores(&signals)
                    .command_buffers(&cmds)
                    .push_next(&mut timeline)],
                self.fence,
            )
        })?;
        // D3D12 waits for this Vulkan signal on the GPU. Keep the CPU free to
        // record its copy/tag work; the next command-buffer reuse retires it.
        let (mut count, mut result) = (0, 0);
        if xess_fg_game_present(
            self.runtime,
            self.frame * 2 + 1,
            self.frame * 2 + 2,
            u32::from(on),
            &mut count,
            &mut result,
        ) != 0
        {
            return Err("game XeSS FG present failed".into());
        }
        self.initialized = true;
        self.frame += 1;
        if self.overlay != 0 {
            xess_fg_overlay_visible(self.overlay as *mut c_void, self.window as *mut c_void, 1);
        }
        self.active = true;
        Ok((count, result))
    }
}
impl Drop for Renderer {
    fn drop(&mut self) {
        unsafe {
            if self.submitted
                && self
                    .device
                    .wait_for_fences(&[self.fence], true, 30_000_000_000)
                    .is_err()
            {
                std::process::abort();
            }
            // The final D3D12 read can outlive the Vulkan copy now that frames
            // overlap. Drain it before freeing the imported image/fence memory.
            if !self.runtime.is_null() && xess_fg_game_wait(self.runtime) != 0 {
                std::process::abort();
            }
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
            self.device.destroy_semaphore(self.semaphore, None);
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
            if !self.runtime.is_null() {
                xess_fg_game_destroy(self.runtime);
            }
            if self.overlay != 0 {
                xess_fg_overlay_destroy(self.overlay as *mut c_void);
            }
        }
    }
}
