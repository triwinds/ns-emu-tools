//! Bounded, host-owned FG experiment. No emulator integration or display acceptance claim.
use crate::fg_api::*;
use crate::host::write_json;
use ash::vk::{self, Handle};
use libloading::Library;
use serde_json::json;
use std::path::Path;
use windows_sys::Win32::{System::LibraryLoader::GetModuleHandleW, UI::WindowsAndMessaging::*};
pub(super) unsafe fn exercise(
    session: &Path,
    sdk: &Library,
    entry: &ash::Entry,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
) -> Result<()> {
    // Any uncertain GPU state is fatal to the isolated child; never unwind cleanup.
    if let Err(error) = run(session, sdk, entry, instance, physical, device, family) {
        let _ = write_json(
            &session.join("fg-failure.json"),
            &json!({"error":error.to_string(),"fg_verified":false}),
        );
        eprintln!("FG experiment failed: {error}");
        std::process::abort();
    }
    Ok(())
}
unsafe fn run(
    session: &Path,
    sdk: &Library,
    entry: &ash::Entry,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
) -> Result<()> {
    type Address = unsafe extern "C" fn();
    let api = Api {
        feature: *sdk.get::<Address>(b"slGetFeatureFunction\0")? as *mut _,
        token: *sdk.get::<Address>(b"slGetNewFrameToken\0")? as *mut _,
        constants: *sdk.get::<Address>(b"slSetConstants\0")? as *mut _,
        tags: *sdk.get::<Address>(b"slSetTagForFrame\0")? as *mut _,
    };
    checked(probe_fg_options(&api, 0, 0, 0, 0, 16667), "initial FG off")?;
    let initial = state(&api)?;
    write_json(
        &session.join("fg-initial.json"),
        &json!({"state":initial.json()}),
    )?;
    let module = GetModuleHandleW(std::ptr::null());
    let (sender, receiver) = std::sync::mpsc::channel();
    let window_thread = std::thread::spawn(move || unsafe {
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let title: Vec<u16> = "Streamline 2x FG diagnostic\0".encode_utf16().collect();
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            40,
            40,
            1280,
            800,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        );
        ShowWindow(hwnd, SW_SHOW);
        SetForegroundWindow(hwnd);
        sender.send(hwnd as usize).unwrap();
        let mut message: MSG = std::mem::zeroed();
        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
            if message.message == WM_APP {
                DestroyWindow(hwnd);
                break;
            }
            // This bounded fixed-window diagnostic defers user close/window commands
            // until the render thread applies Off and flushes the SDK worker queues.
            if message.message == WM_CLOSE || message.message == WM_SYSCOMMAND {
                continue;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    });
    let hwnd = receiver.recv()? as windows_sys::Win32::Foundation::HWND;
    if hwnd.is_null() {
        return Err("failed creating diagnostic window".into());
    }
    write_json(
        &session.join("fg-window.json"),
        &json!({"hwnd":hwnd as usize,"foreground":GetForegroundWindow() as usize,"visible":IsWindowVisible(hwnd)}),
    )?;
    // SDK GIPA does not return its Win32 surface hook in the pinned version.
    // Use the actual exports to register the HWND/surface association with FG.
    let create_surface =
        *sdk.get::<vk::PFN_vkCreateWin32SurfaceKHR>(b"vkCreateWin32SurfaceKHR\0")?;
    let destroy_surface = *sdk.get::<vk::PFN_vkDestroySurfaceKHR>(b"vkDestroySurfaceKHR\0")?;
    let mut surface = vk::SurfaceKHR::null();
    create_surface(
        instance.handle(),
        &vk::Win32SurfaceCreateInfoKHR::default()
            .hinstance(module as isize)
            .hwnd(hwnd as isize),
        std::ptr::null(),
        &mut surface,
    )
    .result()?;
    let surface_api = ash::khr::surface::Instance::new(entry, instance);
    let caps = surface_api.get_physical_device_surface_capabilities(physical, surface)?;
    let extent = caps.current_extent;
    let pre = state(&api)?;
    write_json(
        &session.join("fg-enable-check.json"),
        &json!({"state":pre.json(),"width":extent.width,"height":extent.height}),
    )?;
    validate_enable(&pre, extent)?;
    if !surface_api.get_physical_device_surface_support(physical, family, surface)? {
        return Err("selected queue cannot present FG surface".into());
    }
    if !caps
        .supported_composite_alpha
        .contains(vk::CompositeAlphaFlagsKHR::OPAQUE)
    {
        return Err("missing opaque alpha".into());
    }
    if !surface_api
        .get_physical_device_surface_present_modes(physical, surface)?
        .contains(&vk::PresentModeKHR::IMMEDIATE)
    {
        return Err("FG requires immediate mode".into());
    }
    if !caps
        .supported_usage_flags
        .contains(vk::ImageUsageFlags::TRANSFER_DST)
    {
        return Err("missing transfer usage".into());
    }
    let format = vk::Format::B8G8R8A8_UNORM;
    if !surface_api
        .get_physical_device_surface_formats(physical, surface)?
        .iter()
        .any(|f| f.format == format && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR)
    {
        return Err("missing SDR format".into());
    }
    let count = (caps.min_image_count + 1).min(if caps.max_image_count == 0 {
        u32::MAX
    } else {
        caps.max_image_count
    });
    let swap = ash::khr::swapchain::Device::new(instance, device);
    let chain = swap.create_swapchain(
        &vk::SwapchainCreateInfoKHR::default()
            .surface(surface)
            .min_image_count(count)
            .image_format(format)
            .image_color_space(vk::ColorSpaceKHR::SRGB_NONLINEAR)
            .image_extent(extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::TRANSFER_DST)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(caps.current_transform)
            .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
            .present_mode(vk::PresentModeKHR::IMMEDIATE)
            .clipped(true),
        None,
    )?;
    let images = swap.get_swapchain_images(chain)?;
    let generation_state = state(&api)?;
    write_json(
        &session.join("fg-generation-check.json"),
        &json!({"state":generation_state.json(),"width":extent.width,"height":extent.height}),
    )?;
    validate_enable(&generation_state, extent)?;
    let props = instance.get_physical_device_memory_properties(physical);
    for format in [vk::Format::R32_SFLOAT, vk::Format::R32G32_SFLOAT] {
        if !instance
            .get_physical_device_format_properties(physical, format)
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::TRANSFER_DST)
        {
            return Err("unsupported FG guide texture format".into());
        }
    }
    let resources = [
        texture(device, &props, extent, vk::Format::R32_SFLOAT)?,
        texture(device, &props, extent, vk::Format::R32G32_SFLOAT)?,
    ];
    let size = u64::from(extent.width) * u64::from(extent.height) * 4;
    let buffer = device.create_buffer(
        &vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE),
        None,
    )?;
    let req = device.get_buffer_memory_requirements(buffer);
    let memory = device.allocate_memory(
        &vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(memory_type(
                &props,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?),
        None,
    )?;
    device.bind_buffer_memory(buffer, memory, 0)?;
    let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;
    let pool = device.create_command_pool(
        &vk::CommandPoolCreateInfo::default()
            .queue_family_index(family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        None,
    )?;
    let cmd = device.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1),
    )?[0];
    let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
    let queue = device.get_device_queue(family, 0);
    // Initialize immutable guides exactly once, before tagging/enabling FG.
    device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
    for resource in resources {
        let image = vk::Image::from_raw(resource.image);
        transition(
            device,
            cmd,
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        device.cmd_clear_color_image(
            cmd,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue { float32: [0.0; 4] },
            &[range()],
        );
        transition(
            device,
            cmd,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
    }
    device.end_command_buffer(cmd)?;
    device.queue_submit(
        queue,
        &[vk::SubmitInfo::default().command_buffers(&[cmd])],
        fence,
    )?;
    device.wait_for_fences(&[fence], true, 5_000_000_000)?;

    let mut semaphores = Vec::new();
    let mut rows = Vec::new();
    let mut token = 0;
    for frame in 0..600u32 {
        // Last iteration applies Off on the presenting thread before any window destruction.
        checked(
            probe_fg_begin(&api, frame + 100, &mut token),
            "new frame / Reflex sleep",
        )?;
        checked(probe_fg_marker(&api, token, 0), "simulation start")?;
        let pixels = std::slice::from_raw_parts_mut(mapped, size as usize);
        let x = (frame * 7) % (extent.width - 160);
        for y in 0..extent.height {
            for col in 0..extent.width {
                let offset = ((y * extent.width + col) * 4) as usize;
                let inside = (x..x + 160).contains(&col) && (200..440).contains(&y);
                pixels[offset..offset + 4].copy_from_slice(if inside {
                    &[40, 200, 250, 255]
                } else {
                    &[35, 24, 18, 255]
                });
            }
        }
        checked(probe_fg_marker(&api, token, 1), "simulation end")?;
        let acquired = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
        let ready = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
        semaphores.extend([acquired, ready]);
        let (index, suboptimal) =
            swap.acquire_next_image(chain, 5_000_000_000, acquired, vk::Fence::null())?;
        if suboptimal {
            return Err("unexpected surface change while FG enabled".into());
        }
        checked(
            probe_fg_inputs(&api, token, u32::from(frame == 0), resources.as_ptr()),
            "constants / tags",
        )?;
        device.reset_fences(&[fence])?;
        device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
        transition(
            device,
            cmd,
            images[index as usize],
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        device.cmd_copy_buffer_to_image(
            cmd,
            buffer,
            images[index as usize],
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                })],
        );
        transition(
            device,
            cmd,
            images[index as usize],
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        device.end_command_buffer(cmd)?;
        checked(probe_fg_marker(&api, token, 2), "render submit start")?;
        device.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .wait_semaphores(&[acquired])
                .wait_dst_stage_mask(&[vk::PipelineStageFlags::TRANSFER])
                .command_buffers(&[cmd])
                .signal_semaphores(&[ready])],
            fence,
        )?;
        checked(probe_fg_marker(&api, token, 3), "render submit end")?;
        checked(probe_fg_marker(&api, token, 4), "present start")?;
        checked(
            probe_fg_options(
                &api,
                u32::from(frame < 599),
                extent.width,
                extent.height,
                images.len() as u32,
                16667,
            ),
            "per-frame FG options",
        )?;
        if swap.queue_present(
            queue,
            &vk::PresentInfoKHR::default()
                .wait_semaphores(&[ready])
                .swapchains(&[chain])
                .image_indices(&[index]),
        )? {
            return Err("suboptimal FG present".into());
        }
        checked(probe_fg_marker(&api, token, 5), "present end")?;
        let s = state(&api)?;
        write_json(
            &session.join(format!("fg-frame-{frame:03}.json")),
            &json!({"frame":frame,"state":s.json(),"requested_on":frame<599,"foreground":GetForegroundWindow()==hwnd,"activation_state":if frame==599 {"Off"} else if s.value>0 && s.presented>1 {"GeneratingReported"} else {"WarmingUpOrNotGenerating"}}),
        )?;
        if s.status != 0 {
            return Err(format!("FG runtime status {}", s.status).into());
        }
        wait_inputs(device, &s)?;
        device.wait_for_fences(&[fence], true, 5_000_000_000)?;
        rows.push(s);
    }
    let nulls = resources.map(|mut r| {
        r.image = 0;
        r
    });
    // New token to clear tags without setting constants twice for one frame.
    checked(probe_fg_begin(&api, 700, &mut token), "cleanup token")?;
    checked(
        probe_fg_inputs(&api, token, 1, nulls.as_ptr()),
        "clear tags",
    )?;
    swap.destroy_swapchain(chain, None);
    device.device_wait_idle()?;
    for semaphore in semaphores {
        device.destroy_semaphore(semaphore, None);
    }
    device.destroy_fence(fence, None);
    device.destroy_command_pool(pool, None);
    device.unmap_memory(memory);
    device.destroy_buffer(buffer, None);
    device.free_memory(memory, None);
    for r in resources {
        device.destroy_image_view(vk::ImageView::from_raw(r.view), None);
        device.destroy_image(vk::Image::from_raw(r.image), None);
        device.free_memory(vk::DeviceMemory::from_raw(r.memory), None);
    }
    destroy_surface(instance.handle(), surface, std::ptr::null());
    PostMessageW(hwnd, WM_APP, 0, 0);
    window_thread.join().map_err(|_| "window thread failed")?;
    let presented: u64 = rows.iter().map(|s| u64::from(s.presented)).sum();
    write_json(
        &session.join("fg-result.json"),
        &json!({"application_frames":600,"sdk_reported_presented":presented,"sdk_reports_extra_frames":presented>600,"width":extent.width,"height":extent.height,"requested_multiplier":2,"display_effect_verified":false,"game_integration_verified":false,"input_completion_wait_count":rows.iter().filter(|s|s.value>0).count(),"maximum_input_completion_value":rows.iter().map(|s|s.value).max().unwrap_or(0),"input_completion_waited":rows.iter().any(|s|s.value>0),"shutdown_fg_off":true}),
    )?;
    if presented <= 600 || !rows.iter().any(|s| s.value > 0) {
        return Err("SDK did not report extra frames".into());
    }
    Ok(())
}

pub(super) fn validate(
    events: &[serde_json::Value],
    report: &serde_json::Value,
) -> Result<serde_json::Value> {
    let app = report["application_frames"]
        .as_u64()
        .ok_or("missing FG frame count")?;
    if app != 600
        || report["sdk_reports_extra_frames"] != true
        || report["input_completion_waited"] != true
        || report["shutdown_fg_off"] != true
    {
        return Err("incomplete FG activation/retirement evidence".into());
    }
    let native: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "route_present_retired" && e["phase"] != 18)
        .collect();
    let worker: Vec<_> = native.iter().filter(|e| e["phase"] == 0).collect();
    if native.len() as u64 <= app
        || worker.len() < 2
        || native
            .iter()
            .any(|e| e["details"]["fence_wait_succeeded"] != true || e["details"]["result"] != 0)
    {
        return Err("missing successful asynchronous native FG presents".into());
    }
    if events
        .iter()
        .any(|e| (e["phase"] == 0 || e["phase"] == 19) && e["event"] == "vkQueuePresentKHR")
    {
        return Err("FG SDK present reentered the application layer".into());
    }
    Ok(
        json!({"native_presents":native.len(),"worker_native_presents":worker.len(),"host_frames":app,"sdk_reported_presented":report["sdk_reported_presented"],"all_native_present_fences_completed":true,"sdk_owned_present_worker_observed":true,"display_effect_verified":false}),
    )
}

fn validate_enable(state: &State, extent: vk::Extent2D) -> Result<()> {
    if state.status != 0
        || state.minimum == 0
        || state.maximum < 1
        || extent.width < state.minimum
        || extent.height < state.minimum
    {
        return Err("FG enable conditions not satisfied".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fg_requires_dimensions_status_and_generation_support() {
        let mut s = State {
            minimum: 100,
            maximum: 1,
            ..State::default()
        };
        let extent = vk::Extent2D {
            width: 100,
            height: 100,
        };
        assert!(validate_enable(&s, extent).is_ok());
        for extent in [
            vk::Extent2D {
                width: 0,
                height: 100,
            },
            vk::Extent2D {
                width: 100,
                height: 99,
            },
        ] {
            assert!(validate_enable(&s, extent).is_err());
        }
        s.status = 2;
        assert!(validate_enable(&s, extent).is_err());
        s.status = 0;
        s.maximum = 0;
        assert!(validate_enable(&s, extent).is_err());
        s.maximum = 1;
        s.minimum = 0;
        assert!(validate_enable(&s, extent).is_err());
        assert_eq!(std::mem::size_of::<State>(), 32);
        assert_eq!(std::mem::size_of::<Resource>(), 40);
    }
    #[test]
    fn activation_requires_native_worker_and_completed_inputs() {
        let rows: Vec<serde_json::Value> =
            include_str!("../evidence/sdk-fg-2026-09-25/passed-002/cycle-trace.jsonl")
                .lines()
                .map(|r| serde_json::from_str(r).unwrap())
                .collect();
        let mut report: serde_json::Value = serde_json::from_str(include_str!(
            "../evidence/sdk-fg-2026-09-25/passed-002/fg-result.json"
        ))
        .unwrap();
        assert!(validate(&rows, &report).is_ok());
        let missing: Vec<_> = rows.iter().filter(|r| r["phase"] != 0).cloned().collect();
        assert!(validate(&missing, &report).is_err());
        report["input_completion_waited"] = json!(false);
        assert!(validate(&rows, &report).is_err());
        report["input_completion_waited"] = json!(true);
        let mut broken = rows.clone();
        broken.push(json!({"phase":0,"event":"vkQueuePresentKHR"}));
        assert!(validate(&broken, &report).is_err());
        let failure = broken
            .iter_mut()
            .find(|r| r["event"] == "route_present_retired" && r["phase"] == 0)
            .unwrap();
        failure["details"]["fence_wait_succeeded"] = json!(false);
        assert!(validate(&broken[..broken.len() - 1], &report).is_err());
    }
}
