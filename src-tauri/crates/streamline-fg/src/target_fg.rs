//! Game FG presentation and live control, with optional bounded diagnostics.
use super::*;
use crate::fg_api::*;
use std::{sync::Arc, time::Instant};
struct Chain {
    input_scale: Option<crate::target_input_scale::InputScale>,
    input_error: Option<String>,
    input_original_extent: Option<vk::Extent2D>,
    input_revision: u64,
    input_sdr: bool,
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
    fg_revision: u64,
    temporal_boundary: crate::target_fg_gate::TemporalBoundary,
    region: Option<[u32; 4]>,
    applied_frame_limit_us: u32,
    reflex_ab: bool,
    frame_budget: Option<u32>,
    stopped: Option<&'static str>,
    resources: Option<[Resource; 2]>,
    flow: Option<crate::target_nvof::Flow>,
    #[cfg(feature = "native-nr")]
    nr: Option<crate::target_nr::Nr>,
    #[cfg(feature = "native-nr")]
    nr_failed: bool,
    #[cfg(feature = "native-nr")]
    nr_look_srgb: bool,
    #[cfg(feature = "native-nr")]
    nr_history: crate::nr_history::History,
    #[cfg(feature = "native-nr")]
    flow_identity: Option<crate::nr_history::Source>,
    #[cfg(feature = "native-nr")]
    flow_frame: Option<u32>,
}
static CHAINS: OnceLock<Mutex<HashMap<u64, Arc<Mutex<Chain>>>>> = OnceLock::new();
static FRAME: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
fn chains() -> std::sync::MutexGuard<'static, HashMap<u64, Arc<Mutex<Chain>>>> {
    CHAINS.get_or_init(Default::default).lock().unwrap()
}
unsafe fn finish_pending(chain: &mut Chain) -> Result<()> {
    crate::route_objects::drain_device(chain.device.handle())?;
    if let Some(sr) = chain.sr.as_mut() {
        sr.finish()?;
    }
    if let Some(input) = chain.input_scale.as_mut() {
        input.finish()?;
    }
    #[cfg(feature = "native-nr")]
    if let Some(nr) = chain.nr.as_mut() {
        nr.finish_handoff()?;
    }
    Ok(())
}
pub(super) unsafe fn drain_device(device: vk::Device) {
    let records = chains().values().cloned().collect::<Vec<_>>();
    for record in records {
        let mut chain = record.lock().unwrap();
        if chain.device.handle() == device && finish_pending(&mut chain).is_err() {
            std::process::abort();
        }
    }
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
    let fg = std::env::var("NS_STREAMLINE_TARGET_FG").as_deref() == Ok("1")
        || std::env::var("NS_STREAMLINE_GRAPHICS_RUNTIME").as_deref() == Ok("1");
    #[cfg(feature = "native-nr")]
    {
        fg || crate::nr_runtime::requested()
    }
    #[cfg(not(feature = "native-nr"))]
    {
        fg
    }
}
pub(super) unsafe fn created(
    device: vk::Device,
    handle: vk::SwapchainKHR,
    extent: vk::Extent2D,
    format: vk::Format,
    color_space: vk::ColorSpaceKHR,
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
            input_scale: None,
            input_error: None,
            input_original_extent: None,
            input_revision: 0,
            input_sdr: color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR,
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
            fg_revision: 0,
            temporal_boundary: crate::target_fg_gate::TemporalBoundary::default(),
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
            #[cfg(feature = "native-nr")]
            nr: None,
            #[cfg(feature = "native-nr")]
            nr_failed: !sr_allowed,
            #[cfg(feature = "native-nr")]
            nr_look_srgb: color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR,
            #[cfg(feature = "native-nr")]
            nr_history: crate::nr_history::History::default(),
            #[cfg(feature = "native-nr")]
            flow_identity: None,
            #[cfg(feature = "native-nr")]
            flow_frame: None,
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
        match crate::target_nvof::Flow::new(
            &instance,
            parent.gipa,
            d.physical,
            device,
            swapchain,
            resources[1],
        ) {
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
        let retire_started = Instant::now();
        finish_pending(&mut chain)?;
        let retire_us = retire_started.elapsed().as_micros();
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
        let (
            (requested, control_revision, fg_options),
            (sr_mode, sr_revision, sr_scale, sr_preset, sr_options),
            nr_applied,
            (input_sizing, input_revision),
        ) = crate::live::frame_controls();
        #[cfg(feature = "native-nr")]
        let (nr_controls, nr_revision) = nr_applied;
        #[cfg(not(feature = "native-nr"))]
        let _ = nr_applied;
        let original_source = crate::source_auto::select(queue, &*info);
        let original_extent = crate::target_input_scale::extent(chain.extent, original_source.ok());
        let input_changed = chain.input_revision != input_revision
            || chain.input_original_extent != Some(original_extent)
            || chain
                .input_scale
                .as_ref()
                .is_some_and(|input| !input.matches(original_source.ok()));
        if input_changed {
            // Retire all consumers before destroying their shared source, then
            // rebuild the processing chain from one frame's control snapshot.
            device.device_wait_idle()?;
            drop(chain.sr.take());
            chain.sr_failed = !chain.sr_allowed;
            #[cfg(feature = "native-nr")]
            {
                drop(chain.nr.take());
                chain.nr_failed = !chain.sr_allowed;
                chain.nr_history.recreated();
                chain.flow_identity = None;
                chain.flow_frame = None;
            }
            drop(chain.input_scale.take());
            chain.input_revision = input_revision;
            chain.input_original_extent = Some(original_extent);
            chain.input_error = None;
        }
        if chain.input_scale.is_none() && chain.input_error.is_none() {
            let parent = instance(vk::Instance::from_raw(chain.instance))
                .ok_or("input scaling instance missing")?;
            let inst = ash::Instance::load_with(
                |name| {
                    (parent.gipa)(parent.handle, name.as_ptr())
                        .map_or(std::ptr::null(), |f| f as *const _)
                },
                parent.handle,
            );
            match crate::target_input_scale::InputScale::new(
                &inst,
                d.physical,
                &device,
                handle,
                chain.extent,
                original_source.ok(),
                chain.input_sdr && chain.sr_allowed,
                input_sizing,
            ) {
                Ok(input) => chain.input_scale = input,
                Err(error) => chain.input_error = Some(error.to_string()),
            }
        }
        let native_source = match chain.input_scale.as_ref() {
            Some(input) => Ok(input.source(original_source.ok())?),
            None => original_source,
        };
        if chain.input_scale.is_none() {
            let (percent, cap) = (input_sizing.scale_percent, input_sizing.max_edge);
            crate::live::input_scale(
                json!({"active":false,"scalePercent":percent,"maxEdge":cap,"mode":if cap == 0 {"percentage"} else {"max_edge"},
                "originalExtent":[original_extent.width,original_extent.height],"inputExtent":[original_extent.width,original_extent.height],"outputExtent":[chain.extent.width,chain.extent.height],
                "reason":if chain.input_error.is_some(){"preparation_failed"}else{"unchanged"},"error":chain.input_error,"startupOnly":false,"appliedRevision":input_revision}),
            );
        }
        let region = crate::target_fg_gate::presentation_region(
            native_source.ok().map(|source| source.viewport),
            [chain.extent.width, chain.extent.height],
            std::env::var("NS_STREAMLINE_TARGET_FAMILY").as_deref() == Ok("yuzu"),
        );
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
        .or(region_off)
        .or(if !fg_options.supported(s.maximum) && s.maximum > 0 {
            Some("fg_multiplier_unsupported")
        } else {
            None
        });
        let on = off_reason.is_none();
        let boundary_reset = chain.temporal_boundary.next(foreground == root, on);
        #[cfg(feature = "native-nr")]
        let nr_extent = crate::target_nr::extent(chain.extent, native_source.ok());
        #[cfg(feature = "native-nr")]
        let nr_requested = crate::nr_runtime::requested()
            && crate::nr_runtime::ready()
            && nr_controls.enabled
            && crate::target_fg_gate::processing_extent([nr_extent.width, nr_extent.height])
            && crate::target_fg_gate::processing_extent([chain.extent.width, chain.extent.height])
            && !chain.nr_failed
            && !crate::target_window::stopping();
        #[cfg(not(feature = "native-nr"))]
        let nr_requested = false;
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
        let sr_dimensions_allowed =
            crate::target_sr::can_process(chain.extent, native_source.ok(), sr_scale);
        let sr_requested = crate::target_sr::available()
            && sr_mode != 0
            && !chain.sr_failed
            && sr_dimensions_allowed;
        if sr_mode == 0 {
            crate::live::sr(json!({"active":false,"reason":"已关闭"}));
        } else if !chain.sr_allowed {
            crate::live::sr(json!({"active":false,"reason":"当前显示表面不支持 SR 读写"}));
        } else if !sr_dimensions_allowed {
            crate::live::sr(
                json!({"active":false,"reason":"当前窗口或处理画面的尺寸不支持 SR，等待游戏画面"}),
            );
        }
        if chain.resources.is_none() && (on || sr_requested || nr_requested) {
            chain.resources = Some(guides(d, &device, &mut chain, queue, handle)?);
        }
        let mut flow_ready = None;
        let mut reset = !chain.was_on
            || input_changed
            || region_changed
            || boundary_reset
            || chain.fg_revision != control_revision;
        let mut sr_reset = chain.sr.is_none() || region_changed || boundary_reset || input_changed;
        #[cfg(feature = "native-nr")]
        let nr_identity = crate::target_nr::identity(handle, chain.extent, native_source.ok());
        #[cfg(feature = "native-nr")]
        let flow_identity = crate::target_nr::identity(handle, chain.extent, original_source.ok());
        #[cfg(feature = "native-nr")]
        let flow_reset = if crate::nr_runtime::requested() {
            // Flow analyzes consecutive unprocessed present images. Rotation of
            // equivalent native color images does not break that present pair.
            chain.flow_identity.is_none_or(|old| {
                old.mapping != flow_identity.mapping || old.extent != flow_identity.extent
            }) || chain
                .flow_frame
                .is_none_or(|old| old.wrapping_add(1) != frame)
        } else if sr_requested {
            sr_reset
        } else {
            reset
        };
        #[cfg(not(feature = "native-nr"))]
        let flow_reset = if sr_requested { sr_reset } else { reset };
        // A reset flow produces zero motion for this frame. NR pauses rather
        // than consuming stale guidance, then resets on its first valid pair;
        // SR and FG receive the same boundary reset directly.
        let flow_reset = flow_reset || boundary_reset || input_changed;
        if boundary_reset {
            trace::event!(
                "target_temporal_boundary",
                json!({"frame":frame,"foreground":foreground == root,"fg_on":on,
                    "flow_reset":true,"sr_reset":true,"fg_reset":true})
            );
        }
        let mut motion_valid = false;
        let mut duplicate = false;
        if on || sr_requested || nr_requested {
            if let Some(resources) = chain.resources {
                if let Some(flow) = chain.flow.as_mut() {
                    let guidance = flow.run(queue, &*info, resources[1], flow_reset, frame)?;
                    flow_ready = Some(guidance.ready);
                    reset |= guidance.reset;
                    sr_reset |= guidance.reset;
                    motion_valid = !guidance.reset;
                    duplicate = guidance.duplicate;
                    #[cfg(feature = "native-nr")]
                    {
                        chain.flow_identity = Some(flow_identity);
                        chain.flow_frame = Some(frame);
                    }
                }
            }
        }
        let motion_ready = flow_ready.is_some();
        let input_scaled = chain.input_scale.is_some();
        let processing_info = if let Some(input) = chain.input_scale.as_mut() {
            let waits = flow_ready.into_iter().collect::<Vec<_>>();
            let source_info = if waits.is_empty() {
                *info
            } else {
                (*info).wait_semaphores(&waits)
            };
            let changed = input.prepare(queue, &source_info, original_source.ok())?;
            reset |= changed;
            sr_reset |= changed;
            flow_ready = None;
            (*info).wait_semaphores(&[])
        } else {
            *info
        };
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
                sr_options,
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
        let nr_motion = if motion_ready {
            chain.resources.map(|r| r[1])
        } else {
            None
        };
        // Availability survives NR consuming the flow semaphore. NR's completed
        // fence or handoff wait also orders this raw field before SR.
        let sr_motion = if nr_motion.is_some() {
            chain.flow.as_ref().map(|flow| flow.sr_pixel_motion())
        } else {
            None
        };
        #[cfg(feature = "native-nr")]
        let mut processed_color = None;
        #[cfg(not(feature = "native-nr"))]
        let processed_color = None;
        #[cfg(feature = "native-nr")]
        let mut nr_completed = false;
        #[cfg(not(feature = "native-nr"))]
        let nr_completed = false;
        #[cfg(feature = "native-nr")]
        let mut nr_evaluated = false;
        #[cfg(not(feature = "native-nr"))]
        let nr_evaluated = false;
        #[cfg(feature = "native-nr")]
        {
            if !nr_controls.enabled && chain.nr.is_some() {
                device.device_wait_idle()?;
                drop(chain.nr.take());
                chain.nr_history.recreated();
            } else if let Some(nr) = chain.nr.as_mut() {
                nr.retire_second(nr_controls.options)?;
            }
            let mut nr_source_frame = None;
            let mut nr_applied_revision = (!nr_controls.enabled).then_some(nr_revision);
            let mut decision = chain.nr_history.next(
                nr_controls,
                nr_identity,
                frame as u64,
                motion_valid && nr_requested,
            )?;
            if decision.evaluate {
                if chain
                    .nr
                    .as_ref()
                    .is_some_and(|nr| !nr.matches(native_source.ok()))
                {
                    device.device_wait_idle()?;
                    drop(chain.nr.take());
                    chain.nr_history.recreated();
                    decision.reset_nr = true;
                    decision.reset_sr = true;
                    decision.reset_fg = true;
                }
                if chain.nr.is_none() {
                    let parent = instance(vk::Instance::from_raw(chain.instance))
                        .ok_or("NR instance missing")?;
                    let inst = ash::Instance::load_with(
                        |name| {
                            (parent.gipa)(parent.handle, name.as_ptr())
                                .map_or(std::ptr::null(), |f| f as *const _)
                        },
                        parent.handle,
                    );
                    match crate::target_nr::Nr::new(
                        &inst,
                        d.physical,
                        &device,
                        handle,
                        chain.extent,
                        native_source.ok(),
                        chain.nr_look_srgb,
                    ) {
                        Ok(nr) => chain.nr = Some(nr),
                        Err(error) => {
                            chain.nr_failed = true;
                            decision = chain.nr_history.safe_fallback();
                            trace::event!(
                                "target_nr_fallback",
                                json!({"error":error.to_string(),"stage":"prepare_before_submit"})
                            );
                        }
                    }
                }
                let to_present = chain.sr.is_none();
                let defer_tail = std::env::var("NS_STREAMLINE_DEFER_PRESENT").as_deref() != Ok("0")
                    && !requested
                    && !chain.was_on
                    && s.value == 0;
                if let (Some(nr), Some(motion)) = (chain.nr.as_mut(), nr_motion) {
                    let output = nr.run(
                        queue,
                        &processing_info
                            .wait_semaphores(&flow_ready.into_iter().collect::<Vec<_>>()),
                        motion,
                        native_source.ok(),
                        nr_controls.intensity,
                        nr_controls.options,
                        decision.reset_nr,
                        matches!(
                            decision.reason,
                            crate::nr_history::Reason::Created
                                | crate::nr_history::Reason::Resumed
                                | crate::nr_history::Reason::SourceChanged
                                | crate::nr_history::Reason::Discontinuity
                        ),
                        nr_revision,
                        to_present,
                        defer_tail,
                        frame,
                    )?;
                    processed_color = Some(output.resource);
                    nr_source_frame = Some(output.source_frame);
                    nr_evaluated = output.source_frame.evaluate;
                    nr_applied_revision = Some(output.applied_revision);
                    decision.reset_sr |= output.source_frame.reset_consumers;
                    decision.reset_fg |= output.source_frame.reset_consumers;
                    flow_ready = None;
                    nr_completed = true;
                }
            }
            sr_reset |= decision.reset_sr;
            reset |= decision.reset_fg;
            let reason = if !nr_controls.enabled {
                "disabled"
            } else if let Some(reason) = crate::nr_runtime::failure() {
                reason
            } else if chain.nr_failed {
                "resource_preparation_failed"
            } else if crate::target_window::stopping() {
                "window_transition"
            } else if !motion_valid {
                "motion_unavailable"
            } else {
                "active"
            };
            crate::live::nr(
                json!({"requested":nr_controls.enabled,"active":nr_completed,"evaluated":nr_source_frame.is_some_and(|f|f.evaluate),"firstEvaluated":nr_source_frame.is_some_and(|f|f.first_evaluate),"sourceObserved":nr_source_frame.is_some_and(|f|f.observed),"modelRecomputed":nr_source_frame.is_some_and(|f|f.model_recompute),"outputReused":nr_source_frame.is_some_and(|f|!f.evaluate),"lookRecomputed":nr_source_frame.is_some_and(|f|f.look_recompute),"finalOutputReused":nr_source_frame.is_some_and(|f|!f.evaluate && !f.look_recompute),"sourceFrameId":nr_source_frame.map(|f|f.id),"sourceFrameBasis":"exact_nr_gamma_input","controlsPending":nr_source_frame.is_some_and(|f|f.controls_pending),"reason":reason,"intensity":nr_controls.intensity,"appliedIntensity":chain.nr.as_ref().and_then(|nr|nr.applied_intensity()),"options":nr_controls.options,"appliedOptions":chain.nr.as_ref().and_then(|nr|nr.applied_options()),"pipeline":chain.nr.as_ref().map(|nr| { let mut status = nr.pipeline_status(nr_controls.options, nr_evaluated); if !nr_completed { status["actualPasses"] = json!(0); status["secondActive"] = json!(false); status["secondEvaluated"] = json!(false); status["reason"] = json!("nr_inactive"); } status }),"look":chain.nr.as_ref().map(|nr| { let mut status = nr.look_status(nr.applied_options().map_or(nr_controls.options.look, |o|o.look)); if let Some(temporal) = status.get_mut("temporal").filter(|v|v.is_object()) { temporal["evaluated"] = json!(nr_evaluated && temporal["active"] == true); if !nr_completed {temporal["active"] = json!(false); temporal["reason"] = json!("nr_inactive");} } if !nr_completed {status["active"] = json!(false); status["reason"] = json!("nr_inactive"); if let Some(spatial) = status.get_mut("spatial").filter(|v| v.is_object()) {spatial["active"] = json!(false); spatial["reason"] = json!("nr_inactive");}} status }),"revision":nr_revision,"appliedRevision":nr_applied_revision,"sourceIdentity":nr_identity.identity,"sourceGeneration":native_source.ok().map(|s|s.generation),"sourceGroupSize":native_source.ok().map(|s|s.history_members),"sourcePathCount":native_source.ok().map(|s|s.history_paths),"historyUpdate":native_source.ok().map(|s|s.history_update),"reset":nr_source_frame.is_some_and(|f|f.reset),"resetReason":format!("{:?}",decision.reason),"pendingSrReset":decision.reset_sr,"pendingFgReset":decision.reset_fg,"motionValid":motion_valid,"depth":"synthetic_constant","source":if native_source.is_ok(){"native_source"}else{"present_source"},"input":nr_identity.extent}),
            );
            trace::event!(
                "target_nr_frame",
                json!({"frame":frame,"requested":nr_controls.enabled,"active":nr_completed,"evaluated":nr_source_frame.is_some_and(|f|f.evaluate),"first_evaluated":nr_source_frame.is_some_and(|f|f.first_evaluate),"source_observed":nr_source_frame.is_some_and(|f|f.observed),"model_recomputed":nr_source_frame.is_some_and(|f|f.model_recompute),"output_reused":nr_source_frame.is_some_and(|f|!f.evaluate),"look_recomputed":nr_source_frame.is_some_and(|f|f.look_recompute),"final_output_reused":nr_source_frame.is_some_and(|f|!f.evaluate && !f.look_recompute),"source_frame_id":nr_source_frame.map(|f|f.id),"source_frame_basis":"exact_nr_gamma_input","pipeline":chain.nr.as_ref().map(|nr|nr.pipeline_status(nr_controls.options,nr_evaluated)),"controls_pending":nr_source_frame.is_some_and(|f|f.controls_pending),"applied_revision":nr_applied_revision,"reason":reason,"revision":nr_revision,"intensity":nr_controls.intensity,"applied_intensity":chain.nr.as_ref().and_then(|nr|nr.applied_intensity()),"reset":nr_source_frame.is_some_and(|f|f.reset),"reset_reason":format!("{:?}",decision.reason),"motion_valid":motion_valid,"pending_sr_reset":decision.reset_sr,"pending_fg_reset":decision.reset_fg,"source_image":native_source.map_or(handle.as_raw(), |s|s.image.as_raw()),"source_generation":native_source.ok().map(|s|s.generation),"source_identity":nr_identity.identity,"source_group_size":native_source.ok().map(|s|s.history_members),"source_path_count":native_source.ok().map(|s|s.history_paths),"history_update":native_source.ok().map(|s|s.history_update),"source_route":native_source.ok().map(|s|s.history_route),"source_usage":native_source.ok().map(|s|s.usage),"mapping":nr_identity.mapping,"input":nr_identity.extent,"nvof_wait_consumed":nr_completed})
            );
        }
        #[cfg(not(feature = "native-nr"))]
        let _ = motion_valid;
        let mut sr_completed = false;
        let mut sr_ready = None;
        #[cfg(feature = "native-nr")]
        let nr_waits = chain
            .nr
            .as_ref()
            .and_then(|nr| nr.ready_semaphore())
            .into_iter()
            .collect::<Vec<_>>();
        #[cfg(not(feature = "native-nr"))]
        let nr_waits: Vec<vk::Semaphore> = Vec::new();
        #[cfg(feature = "native-nr")]
        let defer_tail = chain.nr.as_ref().is_some_and(|nr| nr.tail_deferred());
        #[cfg(not(feature = "native-nr"))]
        let defer_tail = false;
        if let Some(sr) = chain.sr.as_mut() {
            let waits = flow_ready.into_iter().collect::<Vec<_>>();
            let input = if nr_completed {
                processing_info.wait_semaphores(&nr_waits)
            } else if waits.is_empty() {
                processing_info
            } else {
                processing_info.wait_semaphores(&waits)
            };
            sr_ready = sr.run(
                queue,
                &input,
                token,
                sr_motion,
                sr_reset,
                native_source,
                processed_color,
                defer_tail,
                frame,
            )?;
            #[cfg(feature = "native-nr")]
            if sr_ready.is_none() {
                if let Some(nr) = chain.nr.as_mut() {
                    nr.finish_handoff()?;
                }
            }
            flow_ready = None;
            sr_completed = true;
            #[cfg(feature = "native-nr")]
            chain.nr_history.sr_consumed();
        }
        if let Some(input) = chain.input_scale.as_mut() {
            if !nr_completed && !sr_completed {
                input.present(queue, &processing_info, original_source.ok())?;
            }
            crate::live::input_scale(input.status(nr_completed, sr_completed, input_revision));
        }
        if let Some(resources) = chain.resources {
            let mut fg_resources = resources;
            if let Some(flow) = &chain.flow {
                fg_resources[1] = flow.pixel_motion();
            }
            checked(
                probe_fg_inputs(
                    &api,
                    token,
                    u32::from(reset),
                    fg_resources.as_ptr(),
                    region.as_ref().map_or(std::ptr::null(), |r| r.as_ptr()),
                ),
                "FG constants/tags",
            )?;
        }
        // An external layer knows only the presentation boundary. Never invent game simulation markers.
        checked(probe_fg_marker(&api, token, 4), "present start")?;
        crate::target_window::active(on || chain.was_on);
        let requested_frame_limit_us = if chain.reflex_ab {
            crate::target_fg_gate::frame_limit_us(on, chain.on_frames)
        } else {
            fg_options.frame_limit_us()
        };
        // Unsupported multipliers never reach the SDK, even while FG is off.
        let effective_fg = if !fg_options.supported(s.maximum) {
            crate::advanced_settings::FgOptions {
                multiplier: 2,
                ..fg_options
            }
        } else {
            fg_options
        };
        let options_started = Instant::now();
        let fg_motion_format = if chain.flow.is_some() {
            vk::Format::R16G16_SFLOAT
        } else {
            motion_format()
        };
        if on && (duplicate || reset) {
            // Keep the application Present and resource-lifetime waits intact.
            // Streamline suspends interpolation for this pair while retaining
            // allocations, instead of generating from a repeat/reset frame.
            if effective_fg == Default::default() {
                crate::fg_pause::suspend(
                    &api,
                    chain.extent,
                    chain.count,
                    fg_motion_format,
                    requested_frame_limit_us,
                )?;
            } else {
                crate::fg_pause::configure_tuned(
                    &api,
                    false,
                    true,
                    chain.extent,
                    chain.count,
                    fg_motion_format,
                    requested_frame_limit_us,
                    effective_fg,
                )?;
            }
        } else {
            crate::fg_pause::configure_tuned(
                &api,
                on,
                false,
                chain.extent,
                chain.count,
                fg_motion_format,
                requested_frame_limit_us,
                effective_fg,
            )?;
        }
        chain.applied_frame_limit_us = requested_frame_limit_us;
        let options_us = options_started.elapsed().as_micros();
        let present_started = Instant::now();
        let flow_waits = flow_ready.into_iter().collect::<Vec<_>>();
        let present_waits = sr_ready.into_iter().collect::<Vec<_>>();
        let forwarded = if !present_waits.is_empty() {
            (*info).wait_semaphores(&present_waits)
        } else if sr_completed || nr_completed || input_scaled {
            (*info).wait_semaphores(&[])
        } else if flow_waits.is_empty() {
            *info
        } else {
            (*info).wait_semaphores(&flow_waits)
        };
        let result =
            crate::route_objects::with_deferred(sr_ready.is_some(), || next(queue, &forwarded));
        let present_us = present_started.elapsed().as_micros();
        if !matches!(
            result,
            vk::Result::SUCCESS | vk::Result::SUBOPTIMAL_KHR | vk::Result::ERROR_OUT_OF_DATE_KHR
        ) {
            return Err(format!("proxy present failed {result:?}").into());
        }
        checked(probe_fg_marker(&api, token, 5), "present end")?;
        let s = crate::fg_api::state(&api)?;
        let input_started = Instant::now();
        wait_inputs(&device, &s)?;
        #[cfg(feature = "native-nr")]
        if on {
            chain.nr_history.fg_consumed();
        }
        let input_wait_us = input_started.elapsed().as_micros();
        if s.status != 0 {
            return Err(format!("SDK FG status {}", s.status).into());
        }
        if result == vk::Result::ERROR_OUT_OF_DATE_KHR {
            // WSI still enqueues semaphore waits for an out-of-date present.
            // Retire processing and SDK inputs before returning the original
            // result so Qt can recreate its swapchain after a window resize.
            finish_pending(&mut chain)?;
            chain.was_on = false;
            chain.temporal_boundary = crate::target_fg_gate::TemporalBoundary::default();
            crate::target_window::active(false);
            crate::live::fg(json!({"state":"off","generationObserved":false,
                "sdkPresented":0,"requested":false,"suspended":false,
                "feedback":"streamline_present_count","perOutputDisableFlagAvailable":false}));
            crate::live::frame(false, Some("window_operation"), control_revision);
            trace::event!(
                "target_swapchain_out_of_date",
                json!({"swapchain":handle.as_raw(),"frame":frame,
                    "input_wait_completed":true,"pending_processing_completed":true,
                    "result":result.as_raw()}),
            );
            return Ok(result);
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
        chain.fg_revision = control_revision;
        let generation = s.generation(on, on && (duplicate || reset));
        crate::live::fg(
            json!({"state":generation,"generationObserved":generation=="generated_observed",
            "sdkPresented":s.presented,"requested":on,"suspended":on && (duplicate || reset),
            "maximumGenerated":s.maximum,"options":effective_fg,"unsupportedMultiplier":off_reason==Some("fg_multiplier_unsupported"),
            "feedback":"streamline_present_count","perOutputDisableFlagAvailable":false}),
        );
        crate::live::sr_applied(sr_mode, sr_revision, sr_preset, sr_options);
        crate::live::frame(on, off_reason, control_revision);
        if crate::frame_capture::selected(frame) {
            trace::event!(
                "target_capture_frame",
                json!({
                    "frame":frame,"on":on,"off_reason":off_reason,"sdk":s.json(),
                    "foreground":foreground == root,"reset":reset,"boundary_reset":boundary_reset,
                    "flow_reset":flow_reset,"motion_valid":motion_valid,"duplicate":duplicate,
                    "nr_active":nr_completed,"nr_evaluated":nr_evaluated,"nr_output_reused":nr_completed && !nr_evaluated,"sr_evaluated":sr_completed,"sr_reset":sr_reset,
                    "native_source_generation":native_source.ok().map(|s|s.generation),"region":region,
                })
            );
        }
        trace::event!(
            "target_fg_frame",
            json!({"frame":frame,"swapchain":handle.as_raw(),"composition_version":1,"fg_control_requested":requested,"fg_revision":control_revision,"sr_mode":sr_mode,"sr_revision":sr_revision,"nr_active":nr_completed,"nr_evaluated":nr_evaluated,"nr_output_reused":nr_completed && !nr_evaluated,"sr_evaluated":sr_completed,"color_source":if sr_completed {"sr_output"} else if nr_completed {"nr_output"} else if input_scaled {"scaled_input"} else {"original_present"},"history_reset":reset,"duplicate_color":duplicate,"interpolation_suspended":on && (duplicate || reset),"tail_deferred":sr_ready.is_some(),"present_waits":present_waits.iter().map(|s|s.as_raw()).collect::<Vec<_>>(),"requested_on":on,"off_reason":off_reason,"fg_region":region,"source_error":native_source.err(),"input_wait_completed":true,"timing_us":{"retire_previous":retire_us,"begin_and_reflex_sleep":begin_us,"options":options_us,"proxy_present":present_us,"input_wait":input_wait_us},"reflex_frame_limit_us":requested_frame_limit_us,"sleep_frame_limit_us":sleep_frame_limit_us,"on_frames":chain.on_frames,"state":s.json(),"window_stop_reason":crate::target_window::reason(),"present_markers_only":true}),
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
        let mut chain = record.lock().unwrap();
        if finish_pending(&mut chain).is_err() {
            std::process::abort();
        }
        let api = crate::target_runtime::fg_api().unwrap();
        let motion = if chain.flow.is_some() {
            vk::Format::R16G16_SFLOAT
        } else {
            motion_format()
        };
        if crate::fg_pause::configure(&api, false, false, chain.extent, chain.count, motion, 0)
            .is_err()
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
        #[cfg(feature = "native-nr")]
        drop(chain.nr.take());
        drop(chain.input_scale.take());
        crate::live::input_scale(json!({"active":false,"reason":"waiting"}));
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
