//! Game FG presentation and live control, with optional bounded diagnostics.
use super::*;
use crate::fg_api::*;
use std::{sync::Arc, time::Instant};
struct Chain {
    sr: Option<crate::target_sr::Sr>,
    sr_failed: bool,
    sr_allowed: bool,
    sr_mode: u32,
    sr_revision: u64,
    extent: vk::Extent2D,
    device: ash::Device,
    count: u32,
    instance: u64,
    hwnd: isize,
    created: Instant,
    frames: u32,
    on_frames: u32,
    was_on: bool,
    region: Option<[u32; 4]>,
    applied_frame_limit_us: u32,
    reflex_ab: bool,
    frame_budget: Option<u32>,
    stopped: Option<&'static str>,
    resources: Option<[Resource; 2]>,
    flow: Option<crate::target_nvof::Flow>,
}
static CHAINS: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Chain>>>>> = OnceLock::new();
static FRAME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
fn chains() -> std::sync::MutexGuard<'static, HashMap<u64, Arc<Mutex<Chain>>>> {
    CHAINS.get_or_init(Default::default).lock().unwrap()
}
fn motion_format() -> vk::Format {
    if crate::target_nvof::requested() {
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
    sr_allowed: bool,
) -> std::result::Result<(), String> {
    if format != vk::Format::B8G8R8A8_UNORM || queue_family_count > 1 {
        return Err("unsupported FG swapchain format/sharing".into());
    }
    crate::target_window::install(hwnd)?;
    if crate::target_sr::available() && !sr_allowed {
        crate::live::sr(json!({"active":false,"reason":"当前显示表面不支持 SR 读写"}));
    }
    chains().insert(
        handle.as_raw(),
        Arc::new(Mutex::new(Chain {
            extent,
            sr: None,
            sr_failed: !sr_allowed,
            sr_allowed,
            sr_mode: 0,
            sr_revision: 0,
            device: crate::target_runtime::sdk_device(device)?,
            count,
            instance,
            hwnd,
            created: Instant::now(),
            frames: 0,
            on_frames: 0,
            was_on: false,
            region: None,
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
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST
                | if crate::target_nvof::enabled() {
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
    if crate::target_nvof::enabled() {
        // Initialization has not consumed any application semaphore. Unsupported
        // formats/session creation can safely fall back to already-cleared guides.
        match crate::target_nvof::Flow::new(&instance, d.physical, device, swapchain, resources[1])
        {
            Ok(flow) => chain.flow = Some(flow),
            Err(error) => {
                if error.downcast_ref::<vk::Result>() == Some(&vk::Result::ERROR_DEVICE_LOST) {
                    return Err(error);
                }
                trace::event!(
                    "target_nvof_fallback",
                    json!({"reason":error.to_string(),"motion":"zero","stage":"initialization"}),
                );
            }
        }
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
        let native_source = crate::source_auto::select(queue, &*info);
        let region = native_source.ok().and_then(|source| {
            crate::target_fg_gate::content_region(
                source.viewport,
                [chain.extent.width, chain.extent.height],
            )
        });
        let region_changed = region != chain.region;
        chain.region = region;
        let region_off = match region {
            None => Some("fg_region_unavailable"),
            Some([_, _, w, h]) if w < s.minimum || h < s.minimum => Some("fg_region_too_small"),
            _ => None,
        };
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
        })
        .or(region_off);
        let on = off_reason.is_none();
        let (sr_mode, sr_revision, sr_scale, sr_preset) = crate::live::sr_requested();
        if sr_mode != chain.sr_mode || sr_revision != chain.sr_revision {
            // Previous SR submission and FG input consumption have completed.
            device.device_wait_idle()?;
            drop(chain.sr.take());
            chain.sr_mode = sr_mode;
            chain.sr_revision = sr_revision;
            chain.sr_failed = !chain.sr_allowed;
        }
        if chain
            .sr
            .as_ref()
            .is_some_and(|sr| !sr.matches_source(native_source.ok()))
        {
            device.device_wait_idle()?;
            drop(chain.sr.take());
        }
        let sr_requested = crate::target_sr::available() && sr_mode != 0 && !chain.sr_failed;
        if sr_mode == 0 {
            crate::live::sr(json!({"active":false,"reason":"已关闭"}));
        } else if !chain.sr_allowed {
            crate::live::sr(json!({"active":false,"reason":"当前显示表面不支持 SR 读写"}));
        }
        if chain.resources.is_none() && (on || sr_requested) {
            chain.resources = Some(guides(d, &device, &mut chain, queue, handle)?);
        }
        let mut flow_ready = None;
        let mut reset = !chain.was_on || region_changed;
        let mut sr_reset = chain.sr.is_none() || region_changed;
        if on || sr_requested {
            if let Some(resources) = chain.resources {
                if let Some(flow) = chain.flow.as_mut() {
                    let (ready, guidance_reset) = flow.run(
                        queue,
                        &*info,
                        resources[1],
                        if sr_requested { sr_reset } else { reset },
                        frame,
                    )?;
                    flow_ready = Some(ready);
                    reset |= guidance_reset;
                    sr_reset |= guidance_reset;
                }
            }
        }
        if sr_requested && chain.sr.is_none() {
            let parent =
                instance(vk::Instance::from_raw(chain.instance)).ok_or("missing SR instance")?;
            let inst = ash::Instance::load_with(
                |name| {
                    (parent.gipa)(parent.handle, name.as_ptr())
                        .map_or(std::ptr::null(), |f| f as *const _)
                },
                parent.handle,
            );
            match crate::target_sr::Sr::new(
                &inst,
                d.physical,
                &device,
                handle,
                chain.extent,
                sr_mode,
                sr_scale,
                sr_preset,
                native_source.ok(),
            ) {
                Ok(sr) => chain.sr = Some(sr),
                Err(error) => {
                    chain.sr_failed = true;
                    crate::live::sr(json!({"active":false,"reason":error.to_string()}));
                    trace::event!("target_sr_fallback", json!({"reason":error.to_string()}));
                }
            }
        }
        let sr_motion = if flow_ready.is_some() {
            chain.resources.map(|r| r[1])
        } else {
            None
        };
        let mut sr_completed = false;
        if let Some(sr) = chain.sr.as_mut() {
            let waits = flow_ready.into_iter().collect::<Vec<_>>();
            let input = if waits.is_empty() {
                *info
            } else {
                (*info).wait_semaphores(&waits)
            };
            sr.run(queue, &input, token, sr_motion, sr_reset, native_source)?;
            flow_ready = None;
            sr_completed = true;
        }
        if let Some(resources) = chain.resources {
            checked(
                probe_fg_inputs(
                    &api,
                    token,
                    u32::from(reset),
                    resources.as_ptr(),
                    region.as_ref().map_or(std::ptr::null(), |r| r.as_ptr()),
                ),
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
        let forwarded = if sr_completed {
            (*info).wait_semaphores(&[])
        } else if flow_waits.is_empty() {
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
            trace::event!(
                if chain.stopped.is_some() {
                    "target_fg_stopped"
                } else {
                    "target_fg_paused"
                },
                json!({"swapchain":handle.as_raw(),"reason":off_reason,"on_frames":chain.on_frames,"device_idle_completed":true}),
            );
            crate::target_window::active(false);
        }
        if region_changed {
            trace::event!(
                "target_fg_region",
                json!({"region":region,"source_error":native_source.err(),"reset":reset})
            );
        }
        chain.was_on = on;
        crate::live::sr_applied(sr_mode, sr_revision, sr_preset);
        crate::live::frame(on, off_reason, control_revision);
        trace::event!(
            "target_fg_frame",
            json!({"frame":frame,"swapchain":handle.as_raw(),"requested_on":on,"off_reason":off_reason,"fg_region":region,"source_error":native_source.err(),"input_wait_completed":true,"timing_us":{"begin_and_reflex_sleep":begin_us,"options":options_us,"proxy_present":present_us,"input_wait":input_wait_us},"reflex_frame_limit_us":requested_frame_limit_us,"sleep_frame_limit_us":sleep_frame_limit_us,"on_frames":chain.on_frames,"state":s.json(),"window_stop_reason":crate::target_window::reason(),"present_markers_only":true}),
        );
        Ok(result)
    })();
    match result {
        Ok(value) => value,
        Err(error) => {
            trace::event!("target_fg_failure", json!({"error":error.to_string()}));
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
        drop(chain.sr.take());
        crate::live::sr(json!({"active":false,"reason":"waiting"}));
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
            if probe_fg_inputs(&api, token, 1, nulls.as_ptr(), std::ptr::null()) != 0 {
                std::process::abort()
            }
            for r in resources {
                device.destroy_image_view(vk::ImageView::from_raw(r.view), None);
                device.destroy_image(vk::Image::from_raw(r.image), None);
                device.free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
        }
        crate::target_window::retired(chain.frame_budget.is_none());
        trace::event!(
            "target_fg_window_guard_rearmed",
            json!({"swapchain":swapchain.as_raw(),"allowed":chain.frame_budget.is_none(),"device_idle_completed":true}),
        );
        trace::event!(
            "target_fg_retired",
            json!({"swapchain":swapchain.as_raw(),"on_frames":chain.on_frames}),
        );
    }
    crate::target_window::active(false);
    if chains().is_empty() {
        crate::target_window::uninstall();
    }
}
