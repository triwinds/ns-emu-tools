//! Diagnostic Vulkan layer: transparent by default, explicit target SDK/FG experiments opt in.
#![allow(clippy::missing_safety_doc)]
mod abi;
mod capture;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod live;
mod route_objects;
#[cfg(any(test, all(windows, feature = "sdk-bridge")))]
mod target_device_plan;
#[cfg(any(test, all(windows, feature = "sdk-bridge")))]
mod target_fg_gate;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod target_runtime;
mod trace;
use abi::*;
use ash::vk::{self, Handle};
use serde_json::json;
use std::collections::HashMap;
use std::ffi::{c_char, CStr};
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy)]
struct Instance {
    handle: vk::Instance,
    gipa: vk::PFN_vkGetInstanceProcAddr,
    physical_gpa: Option<vk::PFN_vkGetInstanceProcAddr>,
}
#[derive(Clone, Copy)]
struct Device {
    set_loader_data: Option<SetDeviceLoaderData>,
    physical: vk::PhysicalDevice,
    handle: vk::Device,
    gdpa: vk::PFN_vkGetDeviceProcAddr,
}
#[derive(Default)]
struct State {
    instances: HashMap<usize, Instance>,
    devices: HashMap<usize, Device>,
}
static STATE: OnceLock<Mutex<State>> = OnceLock::new();
fn state() -> std::sync::MutexGuard<'static, State> {
    STATE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
// Vulkan dispatchable objects share their dispatch-table key with child objects.
unsafe fn key<T: Handle>(handle: T) -> usize {
    let raw = handle.as_raw();
    if raw == 0 {
        0
    } else {
        *(raw as *const usize)
    }
}
unsafe fn instance<T: Handle>(handle: T) -> Option<Instance> {
    state().instances.get(&key(handle)).copied()
}
unsafe fn device<T: Handle>(handle: T) -> Option<Device> {
    state().devices.get(&key(handle)).copied()
}
fn address(f: vk::PFN_vkVoidFunction) -> usize {
    f.map_or(0, |f| f as usize)
}

#[no_mangle]
pub unsafe extern "system" fn vkNegotiateLoaderLayerInterfaceVersion(
    p: *mut Negotiation,
) -> vk::Result {
    if p.is_null() || (*p).s_type != 1 || (*p).version < 2 {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    (*p).version = 2;
    (*p).gipa = Some(vkGetInstanceProcAddr);
    (*p).gdpa = Some(vkGetDeviceProcAddr);
    (*p).physical_gpa = Some(physical_gpa);
    trace::event("negotiate", json!({"version": 2}));
    vk::Result::SUCCESS
}

#[no_mangle]
pub unsafe extern "system" fn vkCreateInstance(
    info: *const vk::InstanceCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    output: *mut vk::Instance,
) -> vk::Result {
    if !trace::authorized() || info.is_null() || output.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let chain = find_link(
        (*info).p_next,
        vk::StructureType::LOADER_INSTANCE_CREATE_INFO,
    )
    .cast::<InstanceChain>();
    if chain.is_null() || (*chain).data[0] == 0 {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let link = (*chain).data[0] as *mut InstanceLink;
    let gipa = (*link).gipa;
    let physical_gpa = (*link).physical_gpa;
    let Some(next) = gipa(vk::Instance::null(), c"vkCreateInstance".as_ptr()) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let next: vk::PFN_vkCreateInstance = std::mem::transmute(next);
    trace::event(
        "instance_request",
        json!({"extensions":extension_names((*info).pp_enabled_extension_names,(*info).enabled_extension_count),"api_version":if (*info).p_application_info.is_null(){0}else{(*(*info).p_application_info).api_version}}),
    );
    // Only advance the Loader-owned link, never application creation parameters.
    (*chain).data[0] = (*link).next as usize;
    #[cfg(all(windows, feature = "sdk-bridge"))]
    let result = if target_runtime::enabled() {
        target_runtime::create_instance(next, info, alloc, output)
    } else {
        next(info, alloc, output)
    };
    #[cfg(not(all(windows, feature = "sdk-bridge")))]
    let result = next(info, alloc, output);
    (*chain).data[0] = link as usize;
    if result == vk::Result::SUCCESS {
        state().instances.insert(
            key(*output),
            Instance {
                handle: *output,
                gipa,
                physical_gpa,
            },
        );
    }
    trace::event(
        "vkCreateInstance",
        json!({"result": result.as_raw(), "next": next as usize}),
    );
    result
}

#[no_mangle]
pub unsafe extern "system" fn vkDestroyInstance(
    handle: vk::Instance,
    alloc: *const vk::AllocationCallbacks,
) {
    let Some(dispatch) = instance(handle) else {
        return;
    };
    let k = key(handle);
    let next: vk::PFN_vkDestroyInstance =
        std::mem::transmute((dispatch.gipa)(handle, c"vkDestroyInstance".as_ptr()).unwrap());
    next(handle, alloc);
    state().instances.remove(&k);
    trace::event(
        "vkDestroyInstance",
        json!({"remaining_instances": state().instances.len()}),
    );
}

#[no_mangle]
pub unsafe extern "system" fn vkCreateDevice(
    physical: vk::PhysicalDevice,
    info: *const vk::DeviceCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    output: *mut vk::Device,
) -> vk::Result {
    let Some(parent) = instance(physical) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    if info.is_null() || output.is_null() {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let chain = find_link((*info).p_next, vk::StructureType::LOADER_DEVICE_CREATE_INFO)
        .cast::<DeviceChain>();
    if chain.is_null() || (*chain).data == 0 {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    }
    let callback = find_function(
        (*info).p_next,
        vk::StructureType::LOADER_DEVICE_CREATE_INFO,
        1,
    )
    .cast::<DeviceChain>();
    let set_loader_data = if callback.is_null() || (*callback).data == 0 {
        None
    } else {
        Some(std::mem::transmute::<usize, SetDeviceLoaderData>(
            (*callback).data,
        ))
    };
    let link = (*chain).data as *mut DeviceLink;
    let gdpa = (*link).gdpa;
    let Some(next) = ((*link).gipa)(parent.handle, c"vkCreateDevice".as_ptr()) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let next: vk::PFN_vkCreateDevice = std::mem::transmute(next);
    let queues: Vec<_> = (0..(*info).queue_create_info_count as usize)
        .map(|index| {
            let q = &*(*info).p_queue_create_infos.add(index);
            json!({"family":q.queue_family_index,"count":q.queue_count,"flags":q.flags.as_raw()})
        })
        .collect();
    trace::event(
        "device_request",
        json!({"queues":queues,"extensions":extension_names((*info).pp_enabled_extension_names,(*info).enabled_extension_count)}),
    );
    (*chain).data = (*link).next as usize;
    #[cfg(all(windows, feature = "sdk-bridge"))]
    let result = if target_runtime::enabled() {
        target_runtime::create_device(parent, next, physical, info, alloc, output)
    } else {
        next(physical, info, alloc, output)
    };
    #[cfg(not(all(windows, feature = "sdk-bridge")))]
    let result = next(physical, info, alloc, output);
    (*chain).data = link as usize;
    if result == vk::Result::SUCCESS {
        state().devices.insert(
            key(*output),
            Device {
                set_loader_data,
                physical,
                handle: *output,
                gdpa,
            },
        );
    }
    #[cfg(all(windows, feature = "sdk-bridge"))]
    if result == vk::Result::SUCCESS && target_runtime::enabled() {
        if let Err(error) = target_runtime::device_created(parent, physical, *output, info) {
            trace::event(
                "target_sdk_error",
                json!({"stage":"set_info","error":error}),
            );
            std::process::abort();
        }
    }
    trace::event(
        "vkCreateDevice",
        json!({"result": result.as_raw(), "next": next as usize}),
    );
    result
}

#[no_mangle]
pub unsafe extern "system" fn vkDestroyDevice(
    handle: vk::Device,
    alloc: *const vk::AllocationCallbacks,
) {
    let Some(dispatch) = device(handle) else {
        return;
    };
    let k = key(handle);
    #[cfg(all(windows, feature = "sdk-bridge"))]
    if target_runtime::enabled() {
        target_runtime::destroy_device(handle);
    }
    let next: vk::PFN_vkDestroyDevice =
        std::mem::transmute((dispatch.gdpa)(handle, c"vkDestroyDevice".as_ptr()).unwrap());
    next(handle, alloc);
    state().devices.remove(&k);
    trace::event(
        "vkDestroyDevice",
        json!({"remaining_devices": state().devices.len()}),
    );
}

// Copy dispatch under lock; no map or trace lock is held over the call down-chain.
macro_rules! device_hook {
    ($name:ident, $pfn:ident, ($first:ident: $ty:ty $(, $arg:ident: $argty:ty)*), $ret:ty, $failure:expr) => {
        #[no_mangle]
        pub unsafe extern "system" fn $name($first: $ty, $($arg: $argty),*) -> $ret {
            let Some(d) = device($first) else { return $failure; };
            #[cfg(all(windows,feature="sdk-bridge"))]
            if target_runtime::enabled(){target_runtime::ensure_device(d.handle);}
            let name = concat!(stringify!($name), "\0");
            let Some(next) = (d.gdpa)(d.handle, name.as_ptr().cast()) else { return $failure; };
            #[cfg(all(windows,feature="sdk-bridge"))]
            let next=if target_runtime::enabled(){target_runtime::device_proc(d.handle,CStr::from_bytes_with_nul_unchecked(name.as_bytes())).unwrap_or(next)}else{next};
            let next: vk::$pfn = std::mem::transmute(next);
            trace::event(stringify!($name), json!({"next": next as usize, "hook": $name as *const () as usize, "object": $first.as_raw()}));
            next($first, $($arg),*)
        }
    };
}
device_hook!(vkGetDeviceQueue, PFN_vkGetDeviceQueue, (handle: vk::Device, family: u32, index: u32, output: *mut vk::Queue), (), ());
device_hook!(vkGetDeviceQueue2, PFN_vkGetDeviceQueue2, (handle: vk::Device, info: *const vk::DeviceQueueInfo2, output: *mut vk::Queue), (), ());
device_hook!(vkDeviceWaitIdle, PFN_vkDeviceWaitIdle, (handle: vk::Device), vk::Result, vk::Result::ERROR_DEVICE_LOST);

device_hook!(vkGetSwapchainImagesKHR, PFN_vkGetSwapchainImagesKHR, (handle: vk::Device, swapchain: vk::SwapchainKHR, count: *mut u32, images: *mut vk::Image), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);
device_hook!(vkAcquireNextImageKHR, PFN_vkAcquireNextImageKHR, (handle: vk::Device, swapchain: vk::SwapchainKHR, timeout: u64, semaphore: vk::Semaphore, fence: vk::Fence, index: *mut u32), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);
device_hook!(vkAcquireNextImage2KHR, PFN_vkAcquireNextImage2KHR, (handle: vk::Device, info: *const vk::AcquireNextImageInfoKHR, index: *mut u32), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);

device_hook!(vkAllocateCommandBuffers, PFN_vkAllocateCommandBuffers, (handle: vk::Device, info: *const vk::CommandBufferAllocateInfo, output: *mut vk::CommandBuffer), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);
device_hook!(vkBeginCommandBuffer, PFN_vkBeginCommandBuffer, (handle: vk::CommandBuffer, info: *const vk::CommandBufferBeginInfo), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);
device_hook!(vkEndCommandBuffer, PFN_vkEndCommandBuffer, (handle: vk::CommandBuffer), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);
device_hook!(vkQueueSubmit, PFN_vkQueueSubmit, (handle: vk::Queue, count: u32, submits: *const vk::SubmitInfo, fence: vk::Fence), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);
device_hook!(vkQueueWaitIdle, PFN_vkQueueWaitIdle, (handle: vk::Queue), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);

unsafe fn device_intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    macro_rules! pick { ($($f:ident),*) => { match name.to_bytes() { $(s if s == stringify!($f).as_bytes() => return Some(std::mem::transmute($f as *const ())),)* _ => {} } }; }
    pick!(
        vkGetDeviceProcAddr,
        vkDestroyDevice,
        vkGetDeviceQueue,
        vkGetDeviceQueue2,
        vkAllocateCommandBuffers,
        vkBeginCommandBuffer,
        vkEndCommandBuffer,
        vkQueueSubmit,
        vkQueueWaitIdle,
        vkDeviceWaitIdle,
        vkCreateSwapchainKHR,
        vkDestroySwapchainKHR,
        vkGetSwapchainImagesKHR,
        vkAcquireNextImageKHR,
        vkAcquireNextImage2KHR,
        vkQueuePresentKHR
    );
    None
}

#[no_mangle]
pub unsafe extern "system" fn vkGetDeviceProcAddr(
    handle: vk::Device,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if name.is_null() || handle == vk::Device::null() {
        return None;
    }
    let d = device(handle)?;
    let next = (d.gdpa)(handle, name);
    // Never advertise an unsupported command merely because we have a wrapper.
    next?;
    if capture::select_next(CStr::from_ptr(name)) {
        trace::event(
            "idle_capture_query",
            json!({"next": address(next), "device": handle.as_raw()}),
        );
        return next;
    }
    let intercepted = device_intercept(CStr::from_ptr(name));
    let result = intercepted.or(next);
    if intercepted.is_some() {
        trace::event(
            "gdpa",
            json!({"name": CStr::from_ptr(name).to_string_lossy(), "next": address(next), "returned": address(result)}),
        );
    }
    result
}

#[no_mangle]
pub unsafe extern "system" fn vkGetInstanceProcAddr(
    handle: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if name.is_null() {
        return None;
    }
    let name_str = CStr::from_ptr(name);
    match name_str.to_bytes() {
        b"vkGetInstanceProcAddr" => {
            return Some(std::mem::transmute(vkGetInstanceProcAddr as *const ()))
        }
        b"vkCreateInstance" => return Some(std::mem::transmute(vkCreateInstance as *const ())),
        _ => {}
    }
    let d = instance(handle)?;
    let next = (d.gipa)(handle, name);
    next?;
    let result = match name_str.to_bytes() {
        b"vkCreateDevice" => Some(std::mem::transmute(vkCreateDevice as *const ())),
        b"vkDestroyInstance" => Some(std::mem::transmute(vkDestroyInstance as *const ())),
        b"vkCreateWin32SurfaceKHR" => {
            Some(std::mem::transmute(vkCreateWin32SurfaceKHR as *const ()))
        }
        b"vkDestroySurfaceKHR" => Some(std::mem::transmute(vkDestroySurfaceKHR as *const ())),
        _ => device_intercept(name_str).or(next),
    };
    result
}
unsafe extern "system" fn physical_gpa(
    handle: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if name.is_null() {
        return None;
    }
    let d = instance(handle)?;
    d.physical_gpa.and_then(|f| f(handle, name))
}

/// Diagnostic export: exposes the saved next-layer address, never invokes it.
#[no_mangle]
pub unsafe extern "system" fn probeNextAddress(
    handle: u64,
    is_device: u32,
    name: *const c_char,
) -> usize {
    if !trace::authorized() || name.is_null() {
        return 0;
    }
    if is_device != 0 {
        device(vk::Device::from_raw(handle)).map_or(0, |d| address((d.gdpa)(d.handle, name)))
    } else {
        instance(vk::Instance::from_raw(handle)).map_or(0, |d| address((d.gipa)(d.handle, name)))
    }
}

#[no_mangle]
pub extern "system" fn probeLiveObjects() -> u64 {
    let s = state();
    ((s.instances.len() as u64) << 32) | s.devices.len() as u64
}

/// Experimental callbacks use exact live object identity, without dereferencing
/// untrusted handles. Copy the dispatch record before calling any downstream code.
#[no_mangle]
pub unsafe extern "system" fn probeRouteGipa(
    handle: vk::Instance,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if !trace::authorized() || name.is_null() || handle == vk::Instance::null() {
        return None;
    }
    let dispatch = state()
        .instances
        .values()
        .find(|d| d.handle == handle)
        .copied()?;
    let next = (dispatch.gipa)(handle, name);
    trace::event(
        "route_gipa",
        json!({"handle":handle.as_raw(),"name":CStr::from_ptr(name).to_string_lossy(),"next":address(next)}),
    );
    next
}
#[no_mangle]
pub unsafe extern "system" fn probeRouteGdpa(
    handle: vk::Device,
    name: *const c_char,
) -> vk::PFN_vkVoidFunction {
    if !trace::authorized() || name.is_null() || handle == vk::Device::null() {
        return None;
    }
    let dispatch = state()
        .devices
        .values()
        .find(|d| d.handle == handle)
        .copied()?;
    let next = (dispatch.gdpa)(handle, name);
    trace::event(
        "route_gdpa",
        json!({"handle":handle.as_raw(),"name":CStr::from_ptr(name).to_string_lossy(),"next":address(next)}),
    );
    next?;
    route_objects::intercept(CStr::from_ptr(name)).or(next)
}

/// Physical-device handle at this layer's boundary, associated during creation.
#[no_mangle]
pub unsafe extern "system" fn probeRoutePhysical(handle: vk::Device) -> vk::PhysicalDevice {
    if !trace::authorized() {
        return vk::PhysicalDevice::null();
    }
    state()
        .devices
        .values()
        .find(|d| d.handle == handle)
        .map_or(vk::PhysicalDevice::null(), |d| d.physical)
}

unsafe fn extension_names(names: *const *const c_char, count: u32) -> Vec<String> {
    (0..count as usize)
        .map(|i| CStr::from_ptr(*names.add(i)).to_string_lossy().into_owned())
        .collect()
}

#[no_mangle]
pub unsafe extern "system" fn vkCreateWin32SurfaceKHR(
    handle: vk::Instance,
    info: *const vk::Win32SurfaceCreateInfoKHR,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::SurfaceKHR,
) -> vk::Result {
    let Some(d) = instance(handle) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let next: vk::PFN_vkCreateWin32SurfaceKHR =
        std::mem::transmute((d.gipa)(handle, c"vkCreateWin32SurfaceKHR".as_ptr()).unwrap());
    let result = next(handle, info, alloc, out);
    #[cfg(all(windows, feature = "sdk-bridge"))]
    if target_runtime::enabled() && result == vk::Result::SUCCESS {
        target_runtime::surface_created(handle, info, *out);
    }
    trace::event(
        "vkCreateWin32SurfaceKHR",
        json!({"result":result.as_raw(),"object":handle.as_raw()}),
    );
    result
}
#[no_mangle]
pub unsafe extern "system" fn vkDestroySurfaceKHR(
    handle: vk::Instance,
    surface: vk::SurfaceKHR,
    alloc: *const vk::AllocationCallbacks,
) {
    let Some(d) = instance(handle) else { return };
    #[cfg(all(windows, feature = "sdk-bridge"))]
    if target_runtime::enabled() {
        target_runtime::surface_destroyed(surface);
    }
    let next: vk::PFN_vkDestroySurfaceKHR =
        std::mem::transmute((d.gipa)(handle, c"vkDestroySurfaceKHR".as_ptr()).unwrap());
    next(handle, surface, alloc);
    trace::event("vkDestroySurfaceKHR", json!({"object":handle.as_raw()}));
}
#[no_mangle]
pub unsafe extern "system" fn vkCreateSwapchainKHR(
    handle: vk::Device,
    info: *const vk::SwapchainCreateInfoKHR,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::SwapchainKHR,
) -> vk::Result {
    let Some(d) = device(handle) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    trace::event("vkCreateSwapchainKHR", json!({"object":handle.as_raw()}));
    #[cfg(all(windows, feature = "sdk-bridge"))]
    if target_runtime::enabled() {
        target_runtime::ensure_device(handle);
        if let Some(result) = target_runtime::create_swapchain(handle, info, alloc, out) {
            return result;
        }
    }
    let next: vk::PFN_vkCreateSwapchainKHR =
        std::mem::transmute((d.gdpa)(handle, c"vkCreateSwapchainKHR".as_ptr()).unwrap());
    next(handle, info, alloc, out)
}

#[cfg(all(windows, feature = "sdk-bridge"))]
mod fg_api;

#[cfg(all(windows, feature = "sdk-bridge"))]
mod target_fg;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod target_motion;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod target_window;

#[no_mangle]
pub unsafe extern "system" fn vkQueuePresentKHR(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
) -> vk::Result {
    let Some(d) = device(queue) else {
        return vk::Result::ERROR_INITIALIZATION_FAILED;
    };
    let native = (d.gdpa)(d.handle, c"vkQueuePresentKHR".as_ptr()).unwrap();
    #[cfg(all(windows, feature = "sdk-bridge"))]
    let address = if target_runtime::enabled() {
        target_runtime::device_proc(d.handle, c"vkQueuePresentKHR").unwrap_or(native)
    } else {
        native
    };
    #[cfg(not(all(windows, feature = "sdk-bridge")))]
    let address = native;
    let next: vk::PFN_vkQueuePresentKHR = std::mem::transmute(address);
    trace::event(
        "vkQueuePresentKHR",
        json!({"object":queue.as_raw(),"next":address as usize,"hook":vkQueuePresentKHR as *const () as usize}),
    );
    #[cfg(all(windows, feature = "sdk-bridge"))]
    if target_fg::enabled() {
        return target_fg::present(d, queue, info, next);
    }
    next(queue, info)
}
#[no_mangle]
pub unsafe extern "system" fn vkDestroySwapchainKHR(
    handle: vk::Device,
    swapchain: vk::SwapchainKHR,
    alloc: *const vk::AllocationCallbacks,
) {
    let Some(d) = device(handle) else { return };
    let native = (d.gdpa)(handle, c"vkDestroySwapchainKHR".as_ptr()).unwrap();
    #[cfg(all(windows, feature = "sdk-bridge"))]
    let address = if target_runtime::enabled() {
        target_runtime::device_proc(handle, c"vkDestroySwapchainKHR").unwrap_or(native)
    } else {
        native
    };
    #[cfg(not(all(windows, feature = "sdk-bridge")))]
    let address = native;
    #[cfg(all(windows, feature = "sdk-bridge"))]
    if target_fg::enabled() {
        target_fg::before_destroy(swapchain);
    }
    let next: vk::PFN_vkDestroySwapchainKHR = std::mem::transmute(address);
    next(handle, swapchain, alloc);
    #[cfg(all(windows, feature = "sdk-bridge"))]
    if target_fg::enabled() {
        target_fg::after_destroy(handle, swapchain);
    }
    trace::event(
        "vkDestroySwapchainKHR",
        json!({"object":handle.as_raw(),"next":address as usize,"hook":vkDestroySwapchainKHR as *const () as usize}),
    );
}
