//! FG-off swapchain lifecycle through either application or SDK dispatch tables.
use crate::host::{present_frames_recoverable, FrameFault, Result};
use ash::vk::{self, Handle};
use serde_json::{json, Value};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetClientRect, IsIconic, SetWindowPos, ShowWindow,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SW_MINIMIZE, SW_SHOWNOACTIVATE, WS_OVERLAPPEDWINDOW,
};

pub(super) unsafe fn exercise(
    entry: &ash::Entry,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
    downstream_completion: Option<unsafe extern "system" fn() -> u64>,
) -> Result<Value> {
    // Failures can leave GPU/presentation work pending. The isolated process must
    // exit rather than unwind into SDK shutdown and destroy in-flight objects.
    match run(
        entry,
        instance,
        physical,
        device,
        family,
        downstream_completion,
    ) {
        Ok(value) => Ok(value),
        Err(error) => {
            eprintln!("swapchain lifecycle failed: {error}");
            std::process::abort();
        }
    }
}

unsafe fn run(
    entry: &ash::Entry,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
    downstream_completion: Option<unsafe extern "system" fn() -> u64>,
) -> Result<Value> {
    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    let module = GetModuleHandleW(std::ptr::null());
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        class.as_ptr(),
        WS_OVERLAPPEDWINDOW,
        0,
        0,
        320,
        240,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        module,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    let surface = ash::khr::win32_surface::Instance::new(entry, instance).create_win32_surface(
        &vk::Win32SurfaceCreateInfoKHR::default()
            .hinstance(module as isize)
            .hwnd(hwnd as isize),
        None,
    )?;
    let surface_api = ash::khr::surface::Instance::new(entry, instance);
    if !surface_api.get_physical_device_surface_support(physical, family, surface)? {
        return Err("selected queue cannot present".into());
    }
    let queue = device.get_device_queue(family, 0);
    let caps = surface_api.get_physical_device_surface_capabilities(physical, surface)?;
    if !caps
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_DST)
    {
        return Err("surface lacks transfer destination usage".into());
    }
    let format = surface_api
        .get_physical_device_surface_formats(physical, surface)?
        .into_iter()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_UNORM
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .ok_or("no SDR format")?;
    let count = if caps.max_image_count > 0 {
        (caps.min_image_count + 1).min(caps.max_image_count)
    } else {
        caps.min_image_count + 1
    };
    let alpha = [
        vk::CompositeAlphaFlagsKHR::OPAQUE,
        vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::INHERIT,
    ]
    .into_iter()
    .find(|a| caps.supported_composite_alpha.contains(*a))
    .ok_or("no alpha mode")?;
    let swapchain_api = ash::khr::swapchain::Device::new(instance, device);
    let info = vk::SwapchainCreateInfoKHR::default()
        .surface(surface)
        .min_image_count(count)
        .image_format(format.format)
        .image_color_space(format.color_space)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::TRANSFER_DST)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(caps.current_transform)
        .composite_alpha(alpha)
        .present_mode(vk::PresentModeKHR::FIFO)
        .clipped(true);

    let mut old = vk::SwapchainKHR::null();
    let mut rows = Vec::new();
    let started = std::time::Instant::now();
    let mut transitions = Vec::new();
    let mut previous_extent = None;
    for generation in 0..3 {
        if generation == 1 {
            // present_frames has waited for every submission and present queue.
            // This is a host-owned pre-notification, with FG already off.
            transitions.push(json!({"event":"before_resize", "elapsed_us":started.elapsed().as_micros(), "fg_enabled":false}));
            if SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                640,
                480,
                SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOZORDER,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            transitions.push(
                json!({"event":"window_resized", "elapsed_us":started.elapsed().as_micros()}),
            );
        } else if generation == 2 {
            // Stop presentation completely, then restart on the existing device/surface.
            swapchain_api.destroy_swapchain(old, None);
            old = vk::SwapchainKHR::null();
            transitions.push(
                json!({"event":"presentation_stopped", "elapsed_us":started.elapsed().as_micros()}),
            );
            transitions.push(json!({"event":"before_minimize","elapsed_us":started.elapsed().as_micros(),"fg_enabled":false}));
            ShowWindow(hwnd, SW_MINIMIZE);
            if IsIconic(hwnd) == 0 {
                return Err("window did not minimize".into());
            }
            let minimized_caps =
                surface_api.get_physical_device_surface_capabilities(physical, surface)?;
            transitions.push(json!({"event":"presentation_suspended","elapsed_us":started.elapsed().as_micros(),"iconic":true,"width":minimized_caps.current_extent.width,"height":minimized_caps.current_extent.height,"swapchain_created":false,"present_called":false}));
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            if IsIconic(hwnd) != 0 {
                return Err("window did not restore".into());
            }
            transitions.push(
                json!({"event":"window_restored","elapsed_us":started.elapsed().as_micros()}),
            );
        }
        let caps = surface_api.get_physical_device_surface_capabilities(physical, surface)?;
        let mut rect = windows_sys::Win32::Foundation::RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetClientRect(hwnd, &mut rect) == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: ((rect.right - rect.left) as u32)
                    .clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: ((rect.bottom - rect.top) as u32)
                    .clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        if extent.width == 0 || extent.height == 0 {
            return Err("zero extent requires suspended presentation".into());
        }
        if generation == 1 && previous_extent == Some(extent) {
            return Err("resize did not change surface extent".into());
        }
        previous_extent = Some(extent);
        for attempt in 0..3 {
            // Refresh capabilities for every retry; a previous extent may already be stale.
            let fresh = surface_api.get_physical_device_surface_capabilities(physical, surface)?;
            let retry_extent = if fresh.current_extent.width == u32::MAX {
                extent
            } else {
                fresh.current_extent
            };
            if retry_extent.width == 0 || retry_extent.height == 0 {
                return Err("surface suspended during recovery; no swapchain created".into());
            }
            let chain = swapchain_api.create_swapchain(
                &info
                    .old_swapchain(old)
                    .image_extent(retry_extent)
                    .pre_transform(fresh.current_transform),
                None,
            )?;
            if old != vk::SwapchainKHR::null() {
                swapchain_api.destroy_swapchain(old, None);
            }
            let fault = match (generation, attempt) {
                (1, 0) => FrameFault::AcquireOutOfDate,
                (2, 0) => FrameFault::PresentOutOfDate,
                _ => FrameFault::None,
            };
            let frames = present_frames_recoverable(
                device,
                &swapchain_api,
                chain,
                queue,
                family,
                fault,
                downstream_completion,
            )?;
            rows.push(json!({"generation":generation, "attempt":attempt, "swapchain":chain.as_raw(),
                "old_swapchain":old.as_raw(), "present_count":frames.presented,
                "acquire_count":frames.acquired, "acquire2_count":frames.acquire2,
                "rebuild":frames.rebuild, "reason":frames.reason,
                "fault_injected":fault != FrameFault::None, "queue_idle":true, "present_fences_waited":frames.presented,
                "width":retry_extent.width, "height":retry_extent.height, "elapsed_us":started.elapsed().as_micros()}));
            old = chain;
            if !frames.rebuild {
                break;
            }
            transitions.push(
                json!({"event":"rebuild_requested", "generation":generation, "attempt":attempt,
                "reason":frames.reason, "fault_injected":fault != FrameFault::None,
                "elapsed_us":started.elapsed().as_micros(), "fg_enabled":false}),
            );
            if attempt == 2 {
                return Err("swapchain recovery retry limit exceeded".into());
            }
        }
    }
    swapchain_api.destroy_swapchain(old, None);
    surface_api.destroy_surface(surface, None);
    if DestroyWindow(hwnd) == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(json!({"generations":rows, "transitions":transitions,
        "resize_tested":true, "minimize_restore_tested":true, "presentation_restart_tested":true, "sdk_reinitialization_tested":false,
        "thread":format!("{:?}", std::thread::current().id()),
        "surface_destroyed":true, "window_destroyed":true, "swapchains_destroyed":rows.len(), "out_of_date_fault_injection_tested":true}))
}

pub(super) fn validate_report(events: &[Value], report: &Value) -> Result<()> {
    for path in ["application", "sdk"] {
        let data = &report[path];
        let attempts = data["generations"]
            .as_array()
            .ok_or("missing swapchain attempts")?;
        if !(5..=9).contains(&attempts.len())
            || data["swapchains_destroyed"].as_u64() != Some(attempts.len() as u64)
        {
            return Err("incomplete swapchain retirement evidence".into());
        }
        if attempts
            .iter()
            .any(|r| r["present_fences_waited"] != r["present_count"])
        {
            return Err("incomplete presentation retirement".into());
        }
        for generation in 0..3 {
            let last = attempts
                .iter()
                .rev()
                .find(|r| r["generation"] == generation)
                .ok_or("missing swapchain generation")?;
            if last["rebuild"] != false || last["present_count"] != 3 {
                return Err("swapchain did not resume presentation".into());
            }
        }
        for reason in ["acquire_out_of_date", "injected_present_out_of_date"] {
            if !attempts.iter().any(|r| {
                r["fault_injected"] == true && r["rebuild"] == true && r["reason"] == reason
            }) {
                return Err("missing recoverable fault evidence".into());
            }
        }
    }
    let sdk_present_count: u64 = report["sdk"]["generations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["present_count"].as_u64().unwrap_or(0))
        .sum();
    let retired: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "route_present_retired" && e["phase"] == 18)
        .collect();
    if retired.len() as u64 != sdk_present_count
        || retired
            .iter()
            .any(|e| e["phase"] != 18 || e["details"]["fence_wait_succeeded"] != true)
    {
        return Err("missing downstream presentation fence evidence".into());
    }
    let rows = report["application"]["generations"]
        .as_array()
        .ok_or("missing application swapchain evidence")?;
    let sum = |key: &str| -> Result<usize> {
        rows.iter()
            .map(|r| {
                r[key]
                    .as_u64()
                    .map(|n| n as usize)
                    .ok_or_else(|| "missing frame counter".into())
            })
            .sum()
    };
    validate_counts(
        events,
        rows.len(),
        sum("acquire_count")?,
        sum("acquire2_count")?,
        sum("present_count")?,
    )
}

#[cfg(test)]
fn validate(events: &[Value]) -> Result<()> {
    validate_counts(events, 3, 6, 3, 9)
}

fn validate_counts(
    events: &[Value],
    chains: usize,
    acquired: usize,
    acquire2: usize,
    presents: usize,
) -> Result<()> {
    for (name, expected) in [
        ("vkCreateWin32SurfaceKHR", 1),
        ("vkDestroySurfaceKHR", 1),
        ("vkCreateSwapchainKHR", chains),
        ("vkDestroySwapchainKHR", chains),
        ("vkGetSwapchainImagesKHR", chains * 2),
        ("vkAcquireNextImageKHR", acquired),
        ("vkAcquireNextImage2KHR", acquire2),
        ("vkQueuePresentKHR", presents),
        ("vkGetDeviceQueue", 1),
        ("vkAllocateCommandBuffers", presents),
        ("vkBeginCommandBuffer", presents),
        ("vkEndCommandBuffer", presents),
        ("vkQueueSubmit", presents),
        ("vkQueueWaitIdle", presents),
    ] {
        for (phase, count) in [(17, expected), (18, 0)] {
            if events
                .iter()
                .filter(|v| v["event"] == name && v["phase"] == phase)
                .count()
                != count
            {
                return Err(
                    format!("swapchain route evidence mismatch: {name}, phase {phase}").into(),
                );
            }
        }
    }
    let initialized: Vec<_> = events
        .iter()
        .filter(|v| v["event"] == "route_loader_data" && v["phase"] == 18)
        .collect();
    for kind in ["QUEUE", "COMMAND_BUFFER"] {
        let minimum = if kind == "QUEUE" { 1 } else { 9 };
        if initialized
            .iter()
            .filter(|v| v["details"]["type"] == kind)
            .count()
            < minimum
        {
            return Err("missing swapchain Loader initialization evidence".into());
        }
    }
    if initialized
        .iter()
        .any(|v| v["details"]["result"] != 0 || v["details"]["dispatch_matches"] != true)
    {
        return Err("failed swapchain Loader initialization".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_evidence_requires_resume_and_native_fence_completion() {
        let rows: Vec<Value> = include_str!(
            "../../evidence/sdk-recovery-2026-09-25/passed-001/cycle-00/cycle-trace.jsonl"
        )
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
        let report: Value = serde_json::from_str(include_str!(
            "../../evidence/sdk-recovery-2026-09-25/passed-001/cycle-00/sdk-swapchain-calls.json"
        ))
        .unwrap();
        assert!(validate_report(&rows, &report).is_ok());
        let missing: Vec<_> = rows
            .iter()
            .filter(|r| r["event"] != "route_present_retired")
            .cloned()
            .collect();
        assert!(validate_report(&missing, &report).is_err());
        let mut broken = report.clone();
        broken["sdk"]["generations"][4]["rebuild"] = json!(true);
        assert!(validate_report(&rows, &broken).is_err());
        let mut broken = report.clone();
        broken["application"]["generations"][0]["present_fences_waited"] = json!(0);
        assert!(validate_report(&rows, &broken).is_err());
    }

    #[test]
    fn recorded_swapchain_trace_rejects_reentry_missing_control_and_bad_initialization() {
        let rows: Vec<Value> =
            include_str!("../../evidence/sdk-route-resize-2026-09-25/passed-001/layer.jsonl")
                .lines()
                .map(|s| serde_json::from_str(s).unwrap())
                .collect();
        assert!(validate(&rows).is_ok());
        let mut broken = rows.clone();
        broken.push(json!({"event":"vkQueuePresentKHR","phase":18}));
        assert!(validate(&broken).is_err());
        let broken: Vec<_> = rows
            .iter()
            .filter(|v| !(v["event"] == "vkDestroySwapchainKHR" && v["phase"] == 17))
            .cloned()
            .collect();
        assert!(validate(&broken).is_err());
        let mut broken = rows.clone();
        let row = broken
            .iter_mut()
            .find(|v| v["event"] == "route_loader_data" && v["phase"] == 18)
            .unwrap();
        row["details"]["dispatch_matches"] = json!(false);
        assert!(validate(&broken).is_err());
        let broken: Vec<_> = rows
            .into_iter()
            .filter(|v| !(v["event"] == "route_loader_data" && v["phase"] == 18))
            .collect();
        assert!(validate(&broken).is_err());
    }
}
