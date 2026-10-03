//! Present-source SR. Dedicated, fence-completed commands leave application state intact.
use crate::{fg_api::*, target_runtime, trace};
use ash::vk::{self, Handle};
use serde_json::json;
use std::ffi::c_void;
use std::time::Instant;

struct Timing {
    pool: vk::QueryPool,
    bits: u32,
    period: f32,
}
impl Timing {
    unsafe fn mark(&self, device: &ash::Device, cmd: vk::CommandBuffer, index: u32) {
        device.cmd_write_timestamp(
            cmd,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            self.pool,
            index,
        );
    }
    unsafe fn read(&self, device: &ash::Device) -> Option<[f64; 5]> {
        let mut ticks = [0u64; 6];
        // The existing SR completion fence has already completed. Never add a query WAIT.
        device
            .get_query_pool_results(self.pool, 0, &mut ticks, vk::QueryResultFlags::TYPE_64)
            .ok()?;
        Some(std::array::from_fn(|i| {
            timestamp_us(ticks[i], ticks[i + 1], self.bits, self.period)
        }))
    }
}
fn timestamp_us(start: u64, end: u64, bits: u32, period: f32) -> f64 {
    (end.wrapping_sub(start) & (u64::MAX >> (64 - bits))) as f64 * f64::from(period) / 1000.0
}

pub(super) fn available() -> bool {
    std::env::var("NS_STREAMLINE_TARGET_SR_READY").as_deref() == Ok("1")
}
pub(super) fn initial_mode() -> u32 {
    match std::env::var("NS_STREAMLINE_TARGET_SR_MODE").as_deref() {
        Ok("quality") => 3,
        Ok("balanced") => 2,
        Ok("performance") => 1,
        Ok("dlaa") => 6,
        _ => 0,
    }
}
pub(super) fn initial_preset() -> crate::sr_preset::StreamlineSrPreset {
    std::env::var("NS_STREAMLINE_TARGET_SR_PRESET")
        .ok()
        .as_deref()
        .and_then(crate::sr_preset::StreamlineSrPreset::parse)
        .unwrap_or_default()
}
pub(super) fn initial_scale() -> u16 {
    std::env::var("NS_STREAMLINE_TARGET_SR_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|v| (50..=200).contains(v))
        .unwrap_or(match initial_mode() {
            1 => 200,
            2 => 172,
            6 => 100,
            _ => 150,
        })
}
fn source_extent(window: vk::Extent2D, native: Option<crate::source_auto::Source>) -> vk::Extent2D {
    native.map_or(window, |s| vk::Extent2D {
        width: s.offsets[1].x.abs_diff(s.offsets[0].x),
        height: s.offsets[1].y.abs_diff(s.offsets[0].y),
    })
}
fn sizes(original: vk::Extent2D, scale: u16) -> Result<(vk::Extent2D, vk::Extent2D)> {
    if !(50..=200).contains(&scale) {
        return Err("invalid SR scale".into());
    }
    let dimension = |v: u32| -> Result<u32> {
        let n = (u64::from(v) * u64::from(scale) + 50) / 100;
        if !(1..=8192).contains(&v) || !(1..=8192).contains(&n) {
            return Err("SR dimensions exceed 8192 pixels".into());
        }
        Ok(n as u32)
    };
    let output = vk::Extent2D {
        width: dimension(original.width)?,
        height: dimension(original.height)?,
    };
    // DLSS does not downscale: below 1x first reduce the color input.
    Ok((if scale < 100 { output } else { original }, output))
}
pub(super) fn can_process(
    window: vk::Extent2D,
    native: Option<crate::source_auto::Source>,
    scale: u16,
) -> bool {
    crate::target_fg_gate::processing_extent([window.width, window.height])
        && sizes(source_extent(window, native), scale).is_ok_and(|(input, output)| {
            [input, output]
                .into_iter()
                .all(|size| crate::target_fg_gate::processing_extent([size.width, size.height]))
        })
}
fn offsets(size: vk::Extent2D) -> [vk::Offset3D; 2] {
    [
        vk::Offset3D::default(),
        vk::Offset3D {
            x: size.width as i32,
            y: size.height as i32,
            z: 1,
        },
    ]
}
fn viewport_offsets(v: [f32; 4]) -> [vk::Offset3D; 2] {
    [
        vk::Offset3D {
            x: v[0].round() as i32,
            y: v[1].round() as i32,
            z: 0,
        },
        vk::Offset3D {
            x: (v[0] + v[2]).round() as i32,
            y: (v[1] + v[3]).round() as i32,
            z: 1,
        },
    ]
}
unsafe extern "C" {
    fn target_sr_options(
        api: &Api,
        width: u32,
        height: u32,
        mode: u32,
        preset: u32,
        input: *mut u32,
    ) -> i32;
    fn target_sr_evaluate(
        api: &Api,
        evaluate: *mut c_void,
        command: u64,
        token: u64,
        reset: u32,
        resources: *const Resource,
    ) -> i32;
    fn target_sr_free(function: *mut c_void) -> i32;
}
pub(super) struct Sr {
    device: ash::Device,
    resources: Vec<Resource>,
    raw_copy: bool,
    motion_adapter: Option<super::target_sr_motion::Adapter>,
    raw_capture: Option<crate::frame_capture::Capture>,
    images: Vec<vk::Image>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    ready: vk::Semaphore,
    in_flight: bool,
    pending: Option<(serde_json::Value, serde_json::Value)>,
    initialized: bool,
    extent: vk::Extent2D,
    scale: u16,
    original: vk::Extent2D,
    previous_source: Option<crate::source_auto::Source>,
    timing: Option<Timing>,
    capture: Option<crate::frame_capture::Capture>,
}
impl Sr {
    pub(super) fn matches_source(&self, native: Option<crate::source_auto::Source>) -> bool {
        self.original == source_extent(self.extent, native)
            && self.raw_copy == native.is_some_and(|n| n.raw_copy)
            && (!self.raw_copy
                || native.is_some_and(|n| {
                    [n.extent.width, n.extent.height]
                        == [self.resources[4].width, self.resources[4].height]
                }))
    }

    pub(super) unsafe fn new(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        swapchain: vk::SwapchainKHR,
        extent: vk::Extent2D,
        mode: u32,
        scale: u16,
        preset: crate::sr_preset::StreamlineSrPreset,
        native: Option<crate::source_auto::Source>,
    ) -> Result<Self> {
        let mut this = Self {
            device: device.clone(),
            resources: Vec::new(),
            raw_copy: native.is_some_and(|n| n.raw_copy),
            motion_adapter: None,
            raw_capture: None,
            images: Vec::new(),
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            ready: vk::Semaphore::null(),
            in_flight: false,
            pending: None,
            initialized: false,
            extent,
            scale,
            original: source_extent(extent, native),
            previous_source: None,
            timing: None,
            capture: None,
        };
        if std::env::var("NS_STREAMLINE_SR_TIMING").as_deref() == Ok("1") {
            let bits = instance.get_physical_device_queue_family_properties(physical)[0]
                .timestamp_valid_bits;
            if bits != 0 {
                this.timing = Some(Timing {
                    pool: device.create_query_pool(
                        &vk::QueryPoolCreateInfo::default()
                            .query_type(vk::QueryType::TIMESTAMP)
                            .query_count(6),
                        None,
                    )?,
                    bits,
                    period: instance
                        .get_physical_device_properties(physical)
                        .limits
                        .timestamp_period,
                });
            }
        }
        let (input, work) = sizes(this.original, scale)?;
        let mut size = [input.width, input.height];
        checked(
            target_sr_options(
                &target_runtime::fg_api()?,
                work.width,
                work.height,
                mode,
                preset.sdk_value(),
                size.as_mut_ptr(),
            ),
            "SR ratio unsupported by SDK",
        )?;
        for (format, required) in [
            (
                vk::Format::B8G8R8A8_UNORM,
                vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
            ),
            (
                vk::Format::R8G8B8A8_UNORM,
                vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                    | vk::FormatFeatureFlags::STORAGE_IMAGE
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE,
            ),
            (
                vk::Format::R32_SFLOAT,
                vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::TRANSFER_DST,
            ),
            (
                vk::Format::R16G16_SFLOAT,
                vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE
                    | vk::FormatFeatureFlags::STORAGE_IMAGE,
            ),
        ] {
            if !instance
                .get_physical_device_format_properties(physical, format)
                .optimal_tiling_features
                .contains(required)
            {
                return Err(format!("SR unsupported format {format:?}").into());
            }
        }
        let props = instance.get_physical_device_memory_properties(physical);
        let usage = vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        for (size, format, usage) in [
            (input, vk::Format::R8G8B8A8_UNORM, usage),
            (
                work,
                vk::Format::R8G8B8A8_UNORM,
                usage | vk::ImageUsageFlags::STORAGE,
            ),
            (input, vk::Format::R32_SFLOAT, usage),
            (
                input,
                vk::Format::R16G16_SFLOAT,
                usage | vk::ImageUsageFlags::STORAGE,
            ),
        ] {
            this.resources
                .push(texture_with_usage(device, &props, size, format, usage)?);
        }
        if native.is_some_and(|n| n.raw_copy) {
            this.resources.push(texture_with_usage(
                device,
                &props,
                native.unwrap().extent,
                vk::Format::R8G8B8A8_UNORM,
                usage,
            )?);
        }
        this.capture = crate::frame_capture::Capture::new(
            device,
            &props,
            &[this.resources[0], this.resources[1], this.resources[3]],
        )?;
        this.raw_capture =
            crate::frame_capture::Capture::new(device, &props, &[this.resources[1]])?;
        this.motion_adapter = Some(super::target_sr_motion::Adapter::new(
            device,
            this.resources[3],
        )?);
        let get: vk::PFN_vkGetSwapchainImagesKHR = std::mem::transmute(
            target_runtime::device_proc(device.handle(), c"vkGetSwapchainImagesKHR")
                .ok_or("missing SR swapchain images")?,
        );
        let mut count = 0;
        get(device.handle(), swapchain, &mut count, std::ptr::null_mut()).result()?;
        this.images.resize(count as usize, vk::Image::null());
        get(
            device.handle(),
            swapchain,
            &mut count,
            this.images.as_mut_ptr(),
        )
        .result()?;
        this.images.truncate(count as usize);
        // Target device planner requires application graphics family 0.
        this.pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(0)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        this.command = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(this.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        this.fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        this.ready = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
        trace::event!(
            "target_sr_ready",
            json!({"input":size,"output":[extent.width,extent.height],"mode":mode,"source":"auto_native_or_present","native_render_resolution_changed":false,"preset":preset.as_str(),"depth":"synthetic_zero","motion_format":"fp16_input_pixels"})
        );
        Ok(this)
    }
    pub(super) unsafe fn run(
        &mut self,
        queue: vk::Queue,
        info: &vk::PresentInfoKHR,
        token: u64,
        motion: Option<Resource>,
        reset: bool,
        native: std::result::Result<crate::source_auto::Source, &'static str>,
        processed: Option<Resource>,
        defer_tail: bool,
        frame: u32,
    ) -> Result<Option<vk::Semaphore>> {
        let capture = crate::frame_capture::selected(frame) && self.capture.is_some();
        let defer_tail = defer_tail && !capture;
        if self.in_flight || self.pending.is_some() {
            return Err("SR previous submission has not retired".into());
        }
        if defer_tail && processed.is_none() {
            return Err("deferred SR requires private NR input".into());
        }
        let started = self.timing.as_ref().map(|_| Instant::now());
        if info.p_image_indices.is_null()
            || (info.wait_semaphore_count > 0 && info.p_wait_semaphores.is_null())
        {
            return Err("invalid SR present".into());
        }
        let source = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("SR image index")?;
        let native_reason = native.as_ref().err().copied();
        let native = native.ok();
        let reset = reset
            || native.map(|s| (s.offsets, s.viewport))
                != self.previous_source.map(|s| (s.offsets, s.viewport));
        let d = &self.device;
        let cmd = self.command;
        d.reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())?;
        d.reset_fences(&[self.fence])?;
        d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
        if let Some(t) = &self.timing {
            d.cmd_reset_query_pool(cmd, t.pool, 0, 6);
            t.mark(d, cmd, 0);
        }
        for r in &self.resources {
            transition(
                d,
                cmd,
                vk::Image::from_raw(r.image),
                if self.initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
        }
        transition(
            d,
            cmd,
            source,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let input = self.resources[0];
        let output = self.resources[1];
        let mv = self.resources[3];
        let input_extent = vk::Extent2D {
            width: input.width,
            height: input.height,
        };
        if let Some(color) = processed {
            if [color.width, color.height] != [self.original.width, self.original.height]
                || color.format != vk::Format::R16G16B16A16_SFLOAT.as_raw() as u32
            {
                return Err("SR processed NR color contract mismatch".into());
            }
            let image = vk::Image::from_raw(color.image);
            transition(
                d,
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            if let Some(t) = &self.timing {
                t.mark(d, cmd, 1);
            }
            blit_region(
                d,
                cmd,
                image,
                offsets(self.original),
                vk::Image::from_raw(input.image),
                offsets(input_extent),
            );
            transition(
                d,
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        } else if let Some(native) = native {
            transition(
                d,
                cmd,
                native.image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            // A mutable SRGB image sampled through a UNORM view must retain its
            // encoded bytes. Blitting the SRGB image directly would decode gamma.
            let blit_source = if native.raw_copy {
                let raw = vk::Image::from_raw(self.resources[4].image);
                let layers = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                d.cmd_copy_image(
                    cmd,
                    native.image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    raw,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageCopy::default()
                        .src_subresource(layers)
                        .dst_subresource(layers)
                        .extent(vk::Extent3D {
                            width: self.resources[4].width,
                            height: self.resources[4].height,
                            depth: 1,
                        })],
                );
                transition(
                    d,
                    cmd,
                    raw,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                );
                raw
            } else {
                native.image
            };
            if let Some(t) = &self.timing {
                t.mark(d, cmd, 1);
            }
            blit_region(
                d,
                cmd,
                blit_source,
                native.offsets,
                vk::Image::from_raw(input.image),
                offsets(input_extent),
            );
            if native.raw_copy {
                transition(
                    d,
                    cmd,
                    blit_source,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
            }
            transition(
                d,
                cmd,
                native.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        } else {
            if let Some(t) = &self.timing {
                t.mark(d, cmd, 1);
            }
            blit(
                d,
                cmd,
                source,
                self.extent,
                vk::Image::from_raw(input.image),
                input_extent,
            );
        }
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 2);
        }
        d.cmd_clear_color_image(
            cmd,
            vk::Image::from_raw(self.resources[2].image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue { float32: [0.0; 4] },
            &[range()],
        );
        if motion.is_none() {
            d.cmd_clear_color_image(
                cmd,
                vk::Image::from_raw(mv.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue { float32: [0.0; 4] },
                &[range()],
            );
        }
        for r in &self.resources {
            transition(
                d,
                cmd,
                vk::Image::from_raw(r.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        if let Some(motion) = motion {
            let region =
                native.map_or([0.0, 0.0, motion.width as f32, motion.height as f32], |n| {
                    let x = motion.width as f32 / self.extent.width as f32;
                    let y = motion.height as f32 / self.extent.height as f32;
                    [
                        n.viewport[0] * x,
                        n.viewport[1] * y,
                        n.viewport[2] * x,
                        n.viewport[3] * y,
                    ]
                });
            self.motion_adapter
                .as_ref()
                .unwrap()
                .record(cmd, motion, region)?;
            transition(
                d,
                cmd,
                vk::Image::from_raw(mv.image),
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::GENERAL,
            );
        }
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 3);
        }
        let evaluate_started = started.map(|_| Instant::now());
        #[cfg(feature = "native-nr")]
        let _nr_recording = crate::nr_runtime::requested()
            .then(|| crate::nr_layout::Recording::sr(cmd, input.width, input.height));
        checked(
            target_sr_evaluate(
                &target_runtime::fg_api()?,
                target_runtime::sr_function(b"slEvaluateFeature\0")?,
                cmd.as_raw(),
                token,
                u32::from(reset || !self.initialized || motion.is_none()),
                self.resources.as_ptr(),
            ),
            "SR evaluate",
        )?;
        #[cfg(feature = "native-nr")]
        drop(_nr_recording);
        let evaluate_us = evaluate_started.map(|s| s.elapsed().as_micros());
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 4);
        }
        if capture {
            self.raw_capture.as_ref().unwrap().record(cmd);
        }
        transition(
            d,
            cmd,
            vk::Image::from_raw(output.image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        transition(
            d,
            cmd,
            source,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let output_extent = vk::Extent2D {
            width: output.width,
            height: output.height,
        };
        if let Some(native) = native {
            d.cmd_clear_color_image(
                cmd,
                source,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
                &[range()],
            );
            transition(
                d,
                cmd,
                source,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            blit_region(
                d,
                cmd,
                vk::Image::from_raw(output.image),
                offsets(output_extent),
                source,
                viewport_offsets(native.viewport),
            );
        } else {
            blit(
                d,
                cmd,
                vk::Image::from_raw(output.image),
                output_extent,
                source,
                self.extent,
            );
        }
        transition(
            d,
            cmd,
            vk::Image::from_raw(output.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        transition(
            d,
            cmd,
            source,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 5);
        }
        if capture {
            self.capture.as_ref().unwrap().record(cmd);
        }
        d.end_command_buffer(cmd)?;
        let waits = if info.wait_semaphore_count == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
        };
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        let record_us = started.map(|s| s.elapsed().as_micros());
        let submit_started = started.map(|_| Instant::now());
        let signals = if defer_tail { vec![self.ready] } else { vec![] };
        self.in_flight = true;
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[cmd])
                .wait_semaphores(waits)
                .wait_dst_stage_mask(&stages)
                .signal_semaphores(&signals)],
            self.fence,
        )?;
        let submit_us = submit_started.map(|s| s.elapsed().as_micros());
        let wait_started = started.map(|_| Instant::now());
        if !defer_tail {
            d.wait_for_fences(&[self.fence], true, 5_000_000_000)?;
            self.in_flight = false;
        }
        if capture {
            self.capture.as_ref().unwrap().save("sr", frame)?;
            self.raw_capture.as_ref().unwrap().save("sr-raw", frame)?;
        }
        let wait_us = wait_started.map(|s| s.elapsed().as_micros());
        let mut profile = json!({"token":token,"frame":frame,"scale":self.scale,
                "input":[input.width,input.height],"output":[output.width,output.height],
                "raw_copy":native.is_some_and(|n|n.raw_copy),
                "cpu_us":{"record":record_us,"evaluate_call_in_record":evaluate_us,"submit":submit_us,"fence_wait":if defer_tail {Some(0)} else {wait_us}},
                "tail_deferred":defer_tail,"cpu_wait_includes_upstream":true});
        if defer_tail {
            self.pending = Some((
                profile,
                json!({"frame":frame,"fence":self.fence.as_raw(),"ready_semaphore":self.ready.as_raw(),"color_image":processed.map(|r|r.image),"fence_completed":true}),
            ));
        } else if let Some(t) = &self.timing {
            Self::profile_gpu(&mut profile, t.read(d));
            trace::event!("target_sr_profile", profile);
        }
        crate::live::sr(
            json!({"source":if processed.is_some(){"nr_output"}else if native.is_some(){"native_source"}else{"present_source"},"colorImage":processed.map(|r|r.image),"fallbackReason":native_reason,"scale":self.scale,"active":true,"original_input":[self.original.width,self.original.height],"input":[input.width,input.height],"output":[self.extent.width,self.extent.height],"processing_output":[output.width,output.height],"motion":motion.is_some()}),
        );
        self.initialized = true;
        self.previous_source = native;
        trace::event!(
            "target_sr_frame",
            json!({"frame":frame,"fence":self.fence.as_raw(),"tail_deferred":defer_tail,"ready_semaphore":if defer_tail {self.ready.as_raw()} else {0},"source":if processed.is_some(){"nr_output"}else if native.is_some(){"native_source"}else{"present_source"},"color_image":processed.map(|r|r.image),"consumed_waits":waits.iter().map(|s|s.as_raw()).collect::<Vec<_>>(),"fence_completed":!defer_tail,"fallback_reason":native_reason,"scale":self.scale,"evaluated":true,"motion":if motion.is_some(){"nvof"}else{"zero"},"history_reset":reset || motion.is_none(),"original_input":[self.original.width,self.original.height],"input":[input.width,input.height],"output":[self.extent.width,self.extent.height],"processing_output":[output.width,output.height]})
        );
        Ok(defer_tail.then_some(self.ready))
    }
    fn profile_gpu(profile: &mut serde_json::Value, gpu: Option<[f64; 5]>) {
        profile["gpu_available"] = json!(gpu.is_some());
        profile["gpu_us"] = json!(gpu.map(|v|json!({"transitions_and_raw_copy":v[0],"color_blit":v[1],
            "depth_motion_prepare":v[2],"dlss_evaluate":v[3],"output_blit":v[4],"total":v.iter().sum::<f64>()})));
    }
    /// Chain owner first retires the native present, which covers semaphore
    /// consumption. Only then may this single slot and its ready signal be reused.
    pub(super) unsafe fn finish(&mut self) -> Result<()> {
        if self.pending.is_none() {
            return Ok(());
        }
        let started = Instant::now();
        self.device
            .wait_for_fences(&[self.fence], true, 5_000_000_000)?;
        self.in_flight = false;
        let (mut profile, completion) = self.pending.take().unwrap();
        profile["cpu_retire_wait_us"] = json!(started.elapsed().as_micros());
        if let Some(t) = &self.timing {
            Self::profile_gpu(&mut profile, t.read(&self.device));
            trace::event!("target_sr_profile", profile);
        }
        trace::event!("target_sr_completion", completion);
        Ok(())
    }
}
unsafe fn blit(
    d: &ash::Device,
    cmd: vk::CommandBuffer,
    src: vk::Image,
    a: vk::Extent2D,
    dst: vk::Image,
    b: vk::Extent2D,
) {
    let layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1);
    d.cmd_blit_image(
        cmd,
        src,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        dst,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &[vk::ImageBlit::default()
            .src_subresource(layers)
            .dst_subresource(layers)
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: a.width as i32,
                    y: a.height as i32,
                    z: 1,
                },
            ])
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: b.width as i32,
                    y: b.height as i32,
                    z: 1,
                },
            ])],
        vk::Filter::LINEAR,
    );
}
unsafe fn blit_region(
    d: &ash::Device,
    cmd: vk::CommandBuffer,
    src: vk::Image,
    a: [vk::Offset3D; 2],
    dst: vk::Image,
    b: [vk::Offset3D; 2],
) {
    let layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1);
    d.cmd_blit_image(
        cmd,
        src,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        dst,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &[vk::ImageBlit::default()
            .src_subresource(layers)
            .dst_subresource(layers)
            .src_offsets(a)
            .dst_offsets(b)],
        vk::Filter::LINEAR,
    );
}
impl Drop for Sr {
    fn drop(&mut self) {
        unsafe {
            if self.in_flight || self.pending.is_some() {
                std::process::abort();
            }
            drop(self.motion_adapter.take());
            // Caller only drops after device idle; uncertain GPU failures abort at the presentation boundary.
            if let Ok(f) = target_runtime::sr_function(b"slFreeResources\0") {
                if target_sr_free(f) != 0 {
                    std::process::abort();
                }
            }
            for r in self.resources.drain(..) {
                self.device
                    .destroy_image_view(vk::ImageView::from_raw(r.view), None);
                self.device
                    .destroy_image(vk::Image::from_raw(r.image), None);
                self.device
                    .free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
            if self.fence != vk::Fence::null() {
                self.device.destroy_fence(self.fence, None);
            }
            if self.ready != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.ready, None);
            }
            if let Some(t) = &self.timing {
                self.device.destroy_query_pool(t.pool, None);
            }
            if self.pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.pool, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn skips_tiny_loading_surfaces_and_scaled_inputs_before_sdk_evaluation() {
        let size = |width, height| vk::Extent2D { width, height };
        assert!(!can_process(size(100, 30), None, 100));
        assert!(!can_process(size(32, 32), None, 50));
        assert!(can_process(size(64, 64), None, 50));
        assert!(can_process(size(1920, 1080), None, 100));
        assert!(!can_process(size(8192, 8192), None, 200));
    }
    #[test]
    fn timestamp_wrap_and_period() {
        assert_eq!(timestamp_us(250, 10, 8, 1000.0), 16.0);
        assert_eq!(timestamp_us(u64::MAX - 3, 2, 64, 500.0), 3.0);
    }
    #[test]
    fn native_resolution_and_flip_are_independent_of_window_size() {
        let window = vk::Extent2D {
            width: 2560,
            height: 1335,
        };
        let native = crate::source_auto::Source {
            raw_copy: false,
            image: vk::Image::null(),
            generation: 1,
            history_identity: 1,
            history_members: 1,
            history_paths: 1,
            history_update: "initial",
            history_route: [0; 4],
            usage: 1,
            extent: vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            offsets: [
                vk::Offset3D {
                    x: 0,
                    y: 1080,
                    z: 0,
                },
                vk::Offset3D {
                    x: 1920,
                    y: 0,
                    z: 1,
                },
            ],
            viewport: [93.0, 0.0, 2374.0, 1335.0],
        };
        let original = source_extent(window, Some(native));
        let (input, output) = sizes(original, 200).unwrap();
        assert_eq!([input.width, input.height], [1920, 1080]);
        assert_eq!([output.width, output.height], [3840, 2160]);
        let (_, fallback) = sizes(source_extent(window, None), 200).unwrap();
        assert_eq!([fallback.width, fallback.height], [5120, 2670]);
        let rect = viewport_offsets(native.viewport);
        assert_eq!([rect[0].x, rect[1].x], [93, 2467]);
    }
    #[test]
    fn custom_ratio_dimensions_and_downsampling() {
        let extent = vk::Extent2D {
            width: 1920,
            height: 1080,
        };
        for (scale, input, output) in [
            (50, [960, 540], [960, 540]),
            (100, [1920, 1080], [1920, 1080]),
            (150, [1920, 1080], [2880, 1620]),
            (200, [1920, 1080], [3840, 2160]),
        ] {
            let (a, b) = sizes(extent, scale).unwrap();
            assert_eq!([a.width, a.height], input);
            assert_eq!([b.width, b.height], output);
        }
        assert!(sizes(extent, 49).is_err());
        assert!(sizes(extent, 201).is_err());
        assert!(sizes(
            vk::Extent2D {
                width: 8192,
                height: 8192
            },
            200
        )
        .is_err());
    }
}
