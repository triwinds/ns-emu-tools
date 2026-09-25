//! Fixed-window, bounded game FG activation experiment; no production/UI integration.
use super::*;
use crate::fg_api::*;
use std::{sync::Arc, time::Instant};
struct Chain {
    extent: vk::Extent2D,
    device: ash::Device,
    count: u32,
    instance: u64,
    hwnd: isize,
    created: Instant,
    frames: u32,
    on_frames: u32,
    was_on: bool,
    applied_frame_limit_us: u32,
    reflex_ab: bool,
    frame_budget: Option<u32>,
    stopped: Option<&'static str>,
    resources: Option<[Resource; 2]>,
    flow: Option<crate::target_motion::Flow>,
}
static CHAINS: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Chain>>>>> = OnceLock::new();
static FRAME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
fn chains() -> std::sync::MutexGuard<'static, HashMap<u64, Arc<Mutex<Chain>>>> {
    CHAINS.get_or_init(Default::default).lock().unwrap()
}
fn motion_format() -> vk::Format {
    if crate::target_motion::enabled() {
        return vk::Format::R32G32_SFLOAT;
    }
    if std::env::var("NS_STREAMLINE_TARGET_REFERENCE_PARAMS").as_deref() == Ok("1") {
        vk::Format::R16G16_SFLOAT
    } else {
        vk::Format::R32G32_SFLOAT
    }
}
pub(super) fn enabled() -> bool {
    std::env::var("NS_STREAMLINE_TARGET_FG").as_deref() == Ok("1")
}
pub(super) unsafe fn created(
    device: vk::Device,
    handle: vk::SwapchainKHR,
    extent: vk::Extent2D,
    format: vk::Format,
    count: u32,
    queue_family_count: u32,
    instance: u64,
    hwnd: isize,
) -> std::result::Result<(), String> {
    if format != vk::Format::B8G8R8A8_UNORM || queue_family_count > 1 {
        return Err("unsupported FG swapchain format/sharing".into());
    }
    crate::target_window::install(hwnd)?;
    chains().insert(
        handle.as_raw(),
        Arc::new(Mutex::new(Chain {
            extent,
            device: crate::target_runtime::sdk_device(device)?,
            count,
            instance,
            hwnd,
            created: Instant::now(),
            frames: 0,
            on_frames: 0,
            was_on: false,
            applied_frame_limit_us: 0,
            reflex_ab: std::env::var("NS_STREAMLINE_TARGET_REFLEX_AB").as_deref() == Ok("1"),
            frame_budget: std::env::var("NS_STREAMLINE_TARGET_FRAME_BUDGET")
                .ok()
                .and_then(|s| s.parse().ok())
                .filter(|n| *n > 0),
            stopped: None,
            resources: None,
            flow: None,
        })),
    );
    Ok(())
}
unsafe fn guides(
    d: Device,
    device: &ash::Device,
    chain: &mut Chain,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
) -> Result<[Resource; 2]> {
    let parent = instance(vk::Instance::from_raw(chain.instance)).ok_or("missing instance")?;
    let instance = ash::Instance::load_with(
        |name| {
            (parent.gipa)(parent.handle, name.as_ptr()).map_or(std::ptr::null(), |f| f as *const _)
        },
        parent.handle,
    );
    let props = instance.get_physical_device_memory_properties(d.physical);
    for format in [vk::Format::R32_SFLOAT, motion_format()] {
        if !instance
            .get_physical_device_format_properties(d.physical, format)
            .optimal_tiling_features
            .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::TRANSFER_DST)
        {
            return Err("unsupported guide format".into());
        }
    }
    let resources = [
        texture(device, &props, chain.extent, vk::Format::R32_SFLOAT)?,
        texture_with_usage(
            device,
            &props,
            chain.extent,
            motion_format(),
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_DST
                | if crate::target_motion::enabled() {
                    vk::ImageUsageFlags::STORAGE
                } else {
                    vk::ImageUsageFlags::empty()
                },
        )?,
    ];
    // Frozen target family zero, checked at device creation; queue remains application-owned.
    let pool = device.create_command_pool(
        &vk::CommandPoolCreateInfo::default().queue_family_index(0),
        None,
    )?;
    let cmd = device.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .command_buffer_count(1)
            .level(vk::CommandBufferLevel::PRIMARY),
    )?[0];
    let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
    device.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
    for r in resources {
        let image = vk::Image::from_raw(r.image);
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
    device.destroy_fence(fence, None);
    device.destroy_command_pool(pool, None);
    if crate::target_motion::enabled() {
        let formats = [
            (
                vk::Format::R32G32_SFLOAT,
                vk::FormatFeatureFlags::STORAGE_IMAGE,
            ),
            (
                vk::Format::B8G8R8A8_UNORM,
                vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
            ),
            (
                vk::Format::R8G8B8A8_UNORM,
                vk::FormatFeatureFlags::BLIT_DST | vk::FormatFeatureFlags::SAMPLED_IMAGE,
            ),
        ];
        for (format, required) in formats {
            if !instance
                .get_physical_device_format_properties(d.physical, format)
                .optimal_tiling_features
                .contains(required)
            {
                return Err("unsupported optical flow format".into());
            }
        }
        if !instance.get_physical_device_queue_family_properties(d.physical)[0]
            .queue_flags
            .contains(vk::QueueFlags::COMPUTE)
        {
            return Err("flow requires compute on graphics queue".into());
        }
        chain.flow = Some(crate::target_motion::Flow::new(
            device,
            &props,
            swapchain,
            resources[1],
        )?);
    }
    Ok(resources)
}
pub(super) unsafe fn present(
    d: Device,
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
    next: vk::PFN_vkQueuePresentKHR,
) -> vk::Result {
    let result = (|| -> Result<vk::Result> {
        if info.is_null() || (*info).swapchain_count != 1 {
            return Err("FG only supports one swapchain per present".into());
        }
        let handle = *(*info).p_swapchains;
        let record = chains()
            .get(&handle.as_raw())
            .cloned()
            .ok_or("unknown FG swapchain")?;
        let mut chain = record.lock().unwrap();
        chain.frames = chain.frames.saturating_add(1);
        let api = crate::target_runtime::fg_api()?;
        let device = chain.device.clone();
        let frame = FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let mut token = 0;
        let sleep_frame_limit_us = chain.applied_frame_limit_us;
        let begin_started = Instant::now();
        checked(
            probe_fg_begin(&api, frame, &mut token),
            "frame token and Reflex pacing",
        )?;
        let begin_us = begin_started.elapsed().as_micros();
        let s = crate::fg_api::state(&api)?;
        let foreground = windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow();
        let root = windows_sys::Win32::UI::WindowsAndMessaging::GetAncestor(
            chain.hwnd as _,
            windows_sys::Win32::UI::WindowsAndMessaging::GA_ROOT,
        );
        let (requested, control_revision) = crate::live::requested();
        let off_reason = crate::target_fg_gate::Inputs {
            status: s.status,
            minimum: s.minimum,
            maximum: s.maximum,
            width: chain.extent.width,
            height: chain.extent.height,
            warmed_up: chain.created.elapsed().as_secs() >= 10,
            foreground: foreground == root,
            window_stop: crate::target_window::stopping(),
            on_frames: chain.on_frames,
            frame_budget: chain.frame_budget,
            stopped: chain.stopped,
        }
        .off_reason()
        .or(if requested {
            None
        } else {
            Some("user_disabled")
        });
        let on = off_reason.is_none();
        if chain.resources.is_none() && on {
            chain.resources = Some(guides(d, &device, &mut chain, queue, handle)?);
        }
        let mut flow_ready = None;
        let mut reset = !chain.was_on;
        if on {
            if let Some(resources) = chain.resources {
                if let Some(flow) = chain.flow.as_mut() {
                    let (ready, scene_cut) = flow.run(queue, &*info, resources[1], reset, frame)?;
                    flow_ready = Some(ready);
                    reset |= scene_cut;
                }
            }
        }
        if let Some(resources) = chain.resources {
            checked(
                probe_fg_inputs(&api, token, u32::from(reset), resources.as_ptr()),
                "FG constants/tags",
            )?;
        }
        // An external layer knows only the presentation boundary. Never invent game simulation markers.
        checked(probe_fg_marker(&api, token, 4), "present start")?;
        crate::target_window::active(on || chain.was_on);
        let requested_frame_limit_us =
            crate::target_fg_gate::frame_limit_us(chain.reflex_ab && on, chain.on_frames);
        let options_started = Instant::now();
        checked(
            probe_fg_options(
                &api,
                u32::from(on),
                chain.extent.width,
                chain.extent.height,
                chain.count,
                requested_frame_limit_us,
            ),
            "FG options",
        )?;
        chain.applied_frame_limit_us = requested_frame_limit_us;
        let options_us = options_started.elapsed().as_micros();
        let present_started = Instant::now();
        let flow_waits = flow_ready.into_iter().collect::<Vec<_>>();
        let forwarded = if flow_waits.is_empty() {
            *info
        } else {
            (*info).wait_semaphores(&flow_waits)
        };
        let result = next(queue, &forwarded);
        let present_us = present_started.elapsed().as_micros();
        if result != vk::Result::SUCCESS && result != vk::Result::SUBOPTIMAL_KHR {
            return Err(format!("proxy present failed {result:?}").into());
        }
        checked(probe_fg_marker(&api, token, 5), "present end")?;
        let s = crate::fg_api::state(&api)?;
        let input_started = Instant::now();
        wait_inputs(&device, &s)?;
        let input_wait_us = input_started.elapsed().as_micros();
        if s.status != 0 {
            return Err(format!("SDK FG status {}", s.status).into());
        }
        if on {
            chain.on_frames = chain.on_frames.saturating_add(1);
        }
        if !on && chain.was_on {
            device.device_wait_idle()?;
            chain.stopped =
                crate::target_fg_gate::terminal_stop(off_reason, chain.frame_budget.is_some());
            trace::event(
                if chain.stopped.is_some() {
                    "target_fg_stopped"
                } else {
                    "target_fg_paused"
                },
                json!({"swapchain":handle.as_raw(),"reason":off_reason,"on_frames":chain.on_frames,"device_idle_completed":true}),
            );
            crate::target_window::active(false);
        }
        chain.was_on = on;
        crate::live::frame(on, off_reason, control_revision);
        trace::event(
            "target_fg_frame",
            json!({"frame":frame,"swapchain":handle.as_raw(),"requested_on":on,"off_reason":off_reason,"input_wait_completed":true,"timing_us":{"begin_and_reflex_sleep":begin_us,"options":options_us,"proxy_present":present_us,"input_wait":input_wait_us},"reflex_frame_limit_us":requested_frame_limit_us,"sleep_frame_limit_us":sleep_frame_limit_us,"on_frames":chain.on_frames,"state":s.json(),"window_stop_reason":crate::target_window::reason(),"present_markers_only":true}),
        );
        Ok(result)
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            trace::event("target_fg_failure", json!({"error":error.to_string()}));
            std::process::abort()
        }
    }
}
pub(super) unsafe fn before_destroy(handle: vk::SwapchainKHR) {
    let record = chains().get(&handle.as_raw()).cloned();
    if let Some(record) = record {
        let chain = record.lock().unwrap();
        let api = crate::target_runtime::fg_api().unwrap();
        if probe_fg_options(
            &api,
            0,
            chain.extent.width,
            chain.extent.height,
            chain.count,
            0,
        ) != 0
        {
            std::process::abort()
        }
    }
}
pub(super) unsafe fn after_destroy(handle: vk::Device, swapchain: vk::SwapchainKHR) {
    let record = chains().remove(&swapchain.as_raw());
    if let Some(record) = record {
        let mut chain = record.lock().unwrap();
        let device = crate::target_runtime::sdk_device(handle).unwrap();
        if device.device_wait_idle().is_err() {
            std::process::abort()
        }
        drop(chain.flow.take());
        if let Some(resources) = chain.resources {
            let api = crate::target_runtime::fg_api().unwrap();
            let mut token = 0;
            if probe_fg_begin(
                &api,
                FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                &mut token,
            ) != 0
            {
                std::process::abort()
            }
            let nulls = resources.map(|mut r| {
                r.image = 0;
                r
            });
            if probe_fg_inputs(&api, token, 1, nulls.as_ptr()) != 0 {
                std::process::abort()
            }
            for r in resources {
                device.destroy_image_view(vk::ImageView::from_raw(r.view), None);
                device.destroy_image(vk::Image::from_raw(r.image), None);
                device.free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
        }
        crate::target_window::retired(chain.frame_budget.is_none());
        trace::event(
            "target_fg_window_guard_rearmed",
            json!({"swapchain":swapchain.as_raw(),"allowed":chain.frame_budget.is_none(),"device_idle_completed":true}),
        );
        trace::event(
            "target_fg_retired",
            json!({"swapchain":swapchain.as_raw(),"on_frames":chain.on_frames}),
        );
    }
    crate::target_window::active(false);
    if chains().is_empty() {
        crate::target_window::uninstall();
    }
}
