//! Creation adapters used only by the explicit experimental SDK resolver.
use super::*;
use std::ffi::c_void;

unsafe fn exact(handle: vk::Device) -> Option<Device> {
    state()
        .devices
        .values()
        .find(|d| d.handle == handle)
        .copied()
}

unsafe fn initialize<T: Handle + Copy>(d: Device, object: T) -> vk::Result {
    let result = if object.as_raw() == 0 {
        vk::Result::ERROR_INITIALIZATION_FAILED
    } else if let Some(set) = d.set_loader_data {
        set(d.handle, object.as_raw() as *mut c_void)
    } else {
        vk::Result::ERROR_INITIALIZATION_FAILED
    };
    let matches = result == vk::Result::SUCCESS && key(object) == key(d.handle);
    trace::event!(
        "route_loader_data",
        json!({
            "device": d.handle.as_raw(), "object": object.as_raw(), "type": format!("{:?}", T::TYPE),
            "result": result.as_raw(), "dispatch_matches": matches
        }),
    );
    if result == vk::Result::SUCCESS && !matches {
        vk::Result::ERROR_INITIALIZATION_FAILED
    } else {
        result
    }
}

unsafe extern "system" fn get_queue(
    handle: vk::Device,
    family: u32,
    index: u32,
    output: *mut vk::Queue,
) {
    let Some(d) = exact(handle) else {
        std::process::abort()
    };
    if output.is_null() || d.set_loader_data.is_none() {
        std::process::abort()
    }
    let next: vk::PFN_vkGetDeviceQueue =
        std::mem::transmute((d.gdpa)(handle, c"vkGetDeviceQueue".as_ptr()).unwrap());
    next(handle, family, index, output);
    if initialize(d, *output) != vk::Result::SUCCESS {
        std::process::abort()
    }
}

unsafe extern "system" fn get_queue2(
    handle: vk::Device,
    info: *const vk::DeviceQueueInfo2,
    output: *mut vk::Queue,
) {
    let Some(d) = exact(handle) else {
        std::process::abort()
    };
    if output.is_null() || info.is_null() || d.set_loader_data.is_none() {
        std::process::abort()
    }
    let next: vk::PFN_vkGetDeviceQueue2 =
        std::mem::transmute((d.gdpa)(handle, c"vkGetDeviceQueue2".as_ptr()).unwrap());
    next(handle, info, output);
    if initialize(d, *output) != vk::Result::SUCCESS {
        std::process::abort()
    }
}

unsafe extern "system" fn allocate(
    handle: vk::Device,
    info: *const vk::CommandBufferAllocateInfo,
    output: *mut vk::CommandBuffer,
) -> vk::Result {
    let Some(d) = exact(handle) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    if info.is_null() || output.is_null() || d.set_loader_data.is_none() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let next: vk::PFN_vkAllocateCommandBuffers =
        std::mem::transmute((d.gdpa)(handle, c"vkAllocateCommandBuffers".as_ptr()).unwrap());
    let result = next(handle, info, output);
    if result != vk::Result::SUCCESS {
        return result;
    }
    for i in 0..(*info).command_buffer_count as usize {
        let result = initialize(d, *output.add(i));
        if result != vk::Result::SUCCESS {
            let free: vk::PFN_vkFreeCommandBuffers =
                std::mem::transmute((d.gdpa)(handle, c"vkFreeCommandBuffers".as_ptr()).unwrap());
            free(
                handle,
                (*info).command_pool,
                (*info).command_buffer_count,
                output,
            );
            for j in 0..(*info).command_buffer_count as usize {
                *output.add(j) = vk::CommandBuffer::null();
            }
            return result;
        }
    }
    result
}

// Diagnostic retirement adapter: fence each native present, including SDK worker
// presents. This serializes presentation and is not a production pacing strategy.
// FG-off callers additionally require the same-thread completion counter.
thread_local! { static PRESENT_COMPLETIONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) }; }
thread_local! { static DEFER_PRESENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
struct PendingPresent {
    device: ash::Device,
    fence: vk::Fence,
    details: serde_json::Value,
    submitted: std::time::Instant,
}
static PENDING: OnceLock<Mutex<HashMap<u64, PendingPresent>>> = OnceLock::new();
static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[cfg(all(windows, feature = "sdk-bridge"))]
pub(super) fn with_deferred<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            DEFER_PRESENT.with(|v| v.set(self.0));
        }
    }
    let _reset = Reset(DEFER_PRESENT.with(|v| v.replace(enabled)));
    f()
}

// One pending native present per device. Called before next use, swapchain
// destruction and device shutdown, including an out-of-date presentation.
pub(super) unsafe fn drain_device(handle: vk::Device) -> Result<(), vk::Result> {
    let mut pending = PENDING.get_or_init(Default::default).lock().unwrap();
    if let Some(p) = pending.get(&handle.as_raw()) {
        let started = std::time::Instant::now();
        let waited = p.device.wait_for_fences(&[p.fence], true, 5_000_000_000);
        let mut details = p.details.clone();
        details["fence_wait_succeeded"] = json!(waited.is_ok());
        details["cpu_wait_us"] = json!(started.elapsed().as_micros());
        details["age_us"] = json!(p.submitted.elapsed().as_micros());
        trace::event!("route_present_retired", details);
        waited?;
        p.device.destroy_fence(p.fence, None);
        #[cfg(all(windows, feature = "sdk-bridge"))]
        if p.details["result"] == 0 || p.details["result"] == vk::Result::SUBOPTIMAL_KHR.as_raw() {
            crate::live::native();
        }
        PRESENT_COMPLETIONS.with(|count| count.set(count.get() + 1));
        pending.remove(&handle.as_raw());
    }
    Ok(())
}

#[no_mangle]
pub extern "system" fn probeRoutePresentCompletions() -> u64 {
    PRESENT_COMPLETIONS.with(|count| count.get())
}

unsafe extern "system" fn present(queue: vk::Queue, info: *const vk::PresentInfoKHR) -> vk::Result {
    let d = { state().devices.get(&key(queue)).copied() };
    let Some(d) = d else { std::process::abort() };
    if info.is_null() || (*info).swapchain_count != 1 {
        std::process::abort();
    }
    let device = ash::Device::load_with(
        |name| (d.gdpa)(d.handle, name.as_ptr()).map_or(std::ptr::null(), |f| f as *const _),
        d.handle,
    );
    if drain_device(d.handle).is_err() {
        std::process::abort();
    }
    let next: vk::PFN_vkQueuePresentKHR =
        std::mem::transmute((d.gdpa)(d.handle, c"vkQueuePresentKHR".as_ptr()).unwrap());
    let mut base = (*info).p_next as *const vk::BaseInStructure;
    while !base.is_null() {
        if (*base).s_type == vk::StructureType::SWAPCHAIN_PRESENT_FENCE_INFO_EXT {
            std::process::abort();
        }
        base = (*base).p_next;
    }
    let fence = device
        .create_fence(&vk::FenceCreateInfo::default(), None)
        .unwrap_or_else(|_| std::process::abort());
    let fences = [fence];
    let mut retirement = vk::SwapchainPresentFenceInfoEXT::default().fences(&fences);
    let copy = (*info).push_next(&mut retirement);
    let submitted = std::time::Instant::now();
    let result = crate::queue_sync::native(d.handle, || next(queue, &copy));
    let native_call_us = submitted.elapsed().as_micros();
    if !matches!(
        result,
        vk::Result::SUCCESS | vk::Result::SUBOPTIMAL_KHR | vk::Result::ERROR_OUT_OF_DATE_KHR
    ) {
        std::process::abort();
    }
    let deferred = DEFER_PRESENT.with(|v| v.get()) && result == vk::Result::SUCCESS;
    let details = json!({"serial":SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed),"deferred":deferred,"result":result.as_raw(),"queue":queue.as_raw(),"swapchain":(*(*info).p_swapchains).as_raw(),"image_index":*(*info).p_image_indices,"native_call_us":native_call_us});
    trace::event!("route_present_submitted", details.clone());
    let mut pending = PENDING.get_or_init(Default::default).lock().unwrap();
    if pending.contains_key(&d.handle.as_raw()) {
        std::process::abort();
    }
    pending.insert(
        d.handle.as_raw(),
        PendingPresent {
            device,
            fence,
            details,
            submitted,
        },
    );
    drop(pending);
    if !deferred && drain_device(d.handle).is_err() {
        std::process::abort();
    }
    result
}

unsafe extern "system" fn destroy_swapchain(
    device: vk::Device,
    swapchain: vk::SwapchainKHR,
    alloc: *const vk::AllocationCallbacks,
) {
    let Some(d) = exact(device) else {
        std::process::abort()
    };
    if drain_device(device).is_err() {
        std::process::abort();
    }
    let next: vk::PFN_vkDestroySwapchainKHR =
        std::mem::transmute((d.gdpa)(device, c"vkDestroySwapchainKHR".as_ptr()).unwrap());
    next(device, swapchain, alloc);
}

pub(super) unsafe fn intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    match name.to_bytes() {
        b"vkQueuePresentKHR" => Some(std::mem::transmute(present as *const ())),
        b"vkDestroySwapchainKHR" => Some(std::mem::transmute(destroy_swapchain as *const ())),
        b"vkGetDeviceQueue" => Some(std::mem::transmute(get_queue as *const ())),
        b"vkGetDeviceQueue2" => Some(std::mem::transmute(get_queue2 as *const ())),
        b"vkAllocateCommandBuffers" => Some(std::mem::transmute(allocate as *const ())),
        _ => crate::sdk_layout_trace::intercept(name)
            .or_else(|| crate::queue_sync::intercept(name))
            .or_else(|| crate::present_layout::intercept(name)),
    }
}
