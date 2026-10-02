//! Owned synthetic inputs; bounded fence waits and readback after every frame.
//! No emulator resources, NVOF history or Streamline session are touched.
use crate::{
    nr_abi::*,
    nr_api::{self, Result},
    sdk_nr::{self, Options},
};
use ash::vk::{self, Handle};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{ffi::c_void, fs, path::Path};

fn range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}
fn region(width: u32, height: u32) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .image_subresource(
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1),
        )
        .image_extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
}
fn memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Result<u32> {
    (0..props.memory_type_count)
        .find(|&i| {
            bits & (1 << i) != 0
                && props.memory_types[i as usize]
                    .property_flags
                    .contains(flags)
        })
        .ok_or_else(|| "no compatible NR diagnostic memory type".into())
}
unsafe fn image(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    width: u32,
    height: u32,
    format: vk::Format,
    writable: bool,
) -> Result<(ResourceVk, vk::DeviceMemory)> {
    let image = device.create_image(
        &vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_SRC
                    | vk::ImageUsageFlags::TRANSFER_DST,
            ),
        None,
    )?;
    let req = device.get_image_memory_requirements(image);
    let memory = device.allocate_memory(
        &vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(memory_type(
                props,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?),
        None,
    )?;
    device.bind_image_memory(image, memory, 0)?;
    let view = device.create_image_view(
        &vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(range()),
        None,
    )?;
    Ok((
        ResourceVk {
            view: view.as_raw(),
            image: image.as_raw(),
            aspect: vk::ImageAspectFlags::COLOR.as_raw(),
            base_mip: 0,
            level_count: 1,
            base_layer: 0,
            layer_count: 1,
            format: format.as_raw() as u32,
            width,
            height,
            resource_type: 0,
            read_write: u8::from(writable),
            padding: [0; 3],
        },
        memory,
    ))
}
unsafe fn buffer(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    bytes: u64,
    usage: vk::BufferUsageFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory)> {
    let buffer = device.create_buffer(
        &vk::BufferCreateInfo::default().size(bytes).usage(usage),
        None,
    )?;
    let req = device.get_buffer_memory_requirements(buffer);
    let memory = device.allocate_memory(
        &vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(memory_type(
                props,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?),
        None,
    )?;
    device.bind_buffer_memory(buffer, memory, 0)?;
    Ok((buffer, memory))
}
unsafe fn transition(
    device: &ash::Device,
    command: vk::CommandBuffer,
    image: vk::Image,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
) {
    device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[vk::ImageMemoryBarrier::default()
            .image(image)
            .subresource_range(range())
            .old_layout(old)
            .new_layout(new)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(if old == vk::ImageLayout::UNDEFINED {
                vk::AccessFlags::empty()
            } else {
                vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE
            })
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)],
    );
}
unsafe fn begin(device: &ash::Device, command: vk::CommandBuffer) -> Result<()> {
    device.reset_command_buffer(command, vk::CommandBufferResetFlags::empty())?;
    device.begin_command_buffer(
        command,
        &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
    )?;
    Ok(())
}
unsafe fn submit(
    device: &ash::Device,
    queue: vk::Queue,
    command: vk::CommandBuffer,
    fence: vk::Fence,
) -> Result<()> {
    device.end_command_buffer(command)?;
    device.reset_fences(&[fence])?;
    device.queue_submit(
        queue,
        &[vk::SubmitInfo::default().command_buffers(&[command])],
        fence,
    )?;
    device.wait_for_fences(&[fence], true, 10_000_000_000)?;
    Ok(())
}

// All generated values are finite normal half-floats or zero. This narrower
// encoder deliberately rejects inputs outside that contract.
fn half(value: f32) -> u16 {
    let bits = value.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    if bits & 0x7fff_ffff == 0 {
        return sign;
    }
    let exponent = ((bits >> 23) & 0xff) as i32 - 127 + 15;
    assert!(
        (1..31).contains(&exponent),
        "synthetic half outside normal range"
    );
    let mantissa = bits & 0x7f_ffff;
    let rounded = (mantissa + 0xfff + ((mantissa >> 13) & 1)) >> 13;
    sign | (((exponent as u32) << 10) + rounded) as u16
}
fn decode_half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = ((bits >> 10) & 31) as i32;
    let fraction = (bits & 1023) as f32;
    match exponent {
        0 => sign * fraction * 2f32.powi(-24),
        31 => {
            if fraction == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15),
    }
}
fn pattern(width: u32, height: u32, offset: u32) -> Vec<u16> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let px = (x + width - offset % width) % width;
            let cell = ((px / 32 + y / 32) % 2) as f32;
            let red = 0.125 + 0.5 * px as f32 / width as f32 + 0.125 * cell;
            let green = 0.125 + 0.5 * y as f32 / height as f32;
            pixels.extend([half(red), half(green), half(0.25 + 0.375 * cell), half(1.0)]);
        }
    }
    pixels
}
fn motion_state(frame: u32, count: u32) -> (u32, bool, bool, &'static str) {
    let a = count / 3;
    let b = count * 2 / 3;
    let valid = frame != 0 && (frame < a || frame >= b);
    let offset = if frame < a {
        frame * 2
    } else if frame < b {
        (a - 1) * 2
    } else {
        (a - 1 + frame - b + 1) * 2
    };
    let reset = frame <= 1 || !valid || frame == b;
    (
        offset,
        valid,
        reset,
        if frame == 0 {
            "feature_created"
        } else if frame == 1 && valid {
            "motion_warmup_complete"
        } else if !valid {
            "zero_motion_reset_each_frame_experiment"
        } else if frame == b {
            "motion_restored"
        } else {
            "continuous_motion"
        },
    )
}
fn ppm(path: &Path, width: u32, height: u32, data: &[u16]) -> Result<()> {
    let mut out = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in data.chunks_exact(4) {
        out.extend(
            pixel[..3]
                .iter()
                .map(|&v| (decode_half(v).clamp(0.0, 1.0) * 255.0).round() as u8),
        );
    }
    fs::write(path, out)?;
    Ok(())
}

pub(super) unsafe fn exercise(
    options: &Options,
    api: &nr_api::Api,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
    params: *mut c_void,
    mut after_frame: Option<&mut dyn FnMut(u32, u32) -> Result<()>>,
) -> Result<Value> {
    let width = options.width;
    let height = options.height;
    let props = instance.get_physical_device_memory_properties(physical);
    for format in [
        vk::Format::R16G16B16A16_SFLOAT,
        vk::Format::R32_SFLOAT,
        vk::Format::R16G16_SFLOAT,
    ] {
        let features = instance
            .get_physical_device_format_properties(physical, format)
            .optimal_tiling_features;
        if !features.contains(
            vk::FormatFeatureFlags::SAMPLED_IMAGE
                | vk::FormatFeatureFlags::STORAGE_IMAGE
                | vk::FormatFeatureFlags::TRANSFER_SRC
                | vk::FormatFeatureFlags::TRANSFER_DST,
        ) {
            return Err(format!("NR test format unsupported: {format:?}").into());
        }
    }
    let pool = device.create_command_pool(
        &vk::CommandPoolCreateInfo::default()
            .queue_family_index(family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        None,
    )?;
    let command = device.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1),
    )?[0];
    let queue = device.get_device_queue(family, 0);
    let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
    let allocated = [
        image(
            device,
            &props,
            width,
            height,
            vk::Format::R16G16B16A16_SFLOAT,
            false,
        )?,
        image(
            device,
            &props,
            width,
            height,
            vk::Format::R16G16B16A16_SFLOAT,
            true,
        )?,
        image(device, &props, width, height, vk::Format::R32_SFLOAT, false)?,
        image(
            device,
            &props,
            width,
            height,
            vk::Format::R16G16_SFLOAT,
            false,
        )?,
    ];
    let mut resources = allocated.map(|(resource, _)| resource);
    sdk_nr::event(
        "external_resources",
        json!({"images":resources.iter().map(|r| r.image).collect::<Vec<_>>(),
        "views":resources.iter().map(|r| r.view).collect::<Vec<_>>(),"dimensions":[width,height]}),
    )?;
    let bytes = u64::from(width) * u64::from(height) * 8;
    let upload = buffer(device, &props, bytes, vk::BufferUsageFlags::TRANSFER_SRC)?;
    let readback = buffer(device, &props, bytes, vk::BufferUsageFlags::TRANSFER_DST)?;
    let mut initialized = false;
    let mut zero_controls = Vec::new();
    let mut summary = Vec::new();
    let mut response_detected = false;
    let mut history = crate::nr_history::History::default();
    let mut serial = 0;
    let mut evaluations = 0u32;
    let mut paused = 0u32;
    let frames_per_intensity = if options.trace_layouts {
        3
    } else {
        options.frames
    };
    for (cycle, enabled, intensity, frames) in [
        (0, false, 0.0, 3),
        (1, true, 0.0, frames_per_intensity),
        (2, true, 1.0, frames_per_intensity),
        (3, true, 1.0, 3),
    ] {
        let mut feature = std::ptr::null_mut();
        if enabled {
            history.recreated();
            nr_api::set_dimensions(params, width, height);
            begin(device, command)?;
            let mut call = nr_api::empty_call(2);
            call.device = device.handle().as_raw() as VkHandle;
            call.command = command.as_raw() as VkHandle;
            call.parameters = params;
            call.output = &mut feature;
            sdk_nr::event(
                "create_feature_enter",
                json!({"cycle":cycle,"width":width,"height":height}),
            )?;
            {
                let _recording = crate::nr_layout::Recording::enter(command, width, height);
                sdk_nr::ngx_result("create_feature", api.call(call))?;
            }
            if feature.is_null() {
                return Err("successful feature creation returned null".into());
            }
            // Keep creation/weight uploads and the first evaluation in the
            // same command buffer, matching the inspected Vulkan reference.
        }
        let mut moving_pairs = 0;
        let mut changed_pairs = 0;
        let mut previous_hash = String::new();
        let mut previous_motion = false;
        let mut interleaved = false;
        for frame in 0..frames {
            let (offset, valid_motion, experimental_reset, experimental_reason) =
                motion_state(frame, frames);
            let decision = history.next(
                crate::nr_history::Controls { enabled, intensity },
                crate::nr_history::Source {
                    identity: resources[0].image,
                    extent: [width, height],
                    mapping: 0,
                },
                serial,
                valid_motion,
            )?;
            serial += 1;
            let evaluate = enabled && (!options.pause_without_motion || decision.evaluate);
            let reset = if options.pause_without_motion {
                decision.reset_nr
            } else {
                experimental_reset
            };
            let reason = if options.pause_without_motion {
                format!("{:?}", decision.reason)
            } else {
                experimental_reason.to_owned()
            };
            let input = pattern(width, height, offset);
            let mapped = device.map_memory(upload.1, 0, bytes, vk::MemoryMapFlags::empty())?;
            std::ptr::copy_nonoverlapping(input.as_ptr(), mapped.cast::<u16>(), input.len());
            device.unmap_memory(upload.1);
            if !enabled || frame != 0 {
                begin(device, command)?;
            }
            for (index, resource) in resources.iter().enumerate() {
                let image = vk::Image::from_raw(resource.image);
                transition(
                    device,
                    command,
                    image,
                    if initialized {
                        vk::ImageLayout::GENERAL
                    } else {
                        vk::ImageLayout::UNDEFINED
                    },
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
                if index == 0 {
                    device.cmd_copy_buffer_to_image(
                        command,
                        upload.0,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[region(width, height)],
                    );
                } else {
                    let color = match index {
                        1 => [-16.0, 0.0, -16.0, 1.0],
                        2 => [0.5; 4],
                        _ => [
                            if valid_motion && frame != 0 {
                                -2.0 / width as f32
                            } else {
                                0.0
                            },
                            0.0,
                            0.0,
                            0.0,
                        ],
                    };
                    device.cmd_clear_color_image(
                        command,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue { float32: color },
                        &[range()],
                    );
                }
                transition(
                    device,
                    command,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
            }
            initialized = true;
            if evaluate {
                nr_api::set_frame(params, &mut resources, intensity, reset);
                let mut call = nr_api::empty_call(3);
                call.command = command.as_raw() as VkHandle;
                call.parameters = params;
                call.feature = feature;
                sdk_nr::event(
                    "evaluate_enter",
                    json!({"cycle":cycle,"frame":frame,"reset":reset,"reset_reason":reason,"motion_valid":valid_motion}),
                )?;
                {
                    let _recording = crate::nr_layout::Recording::enter(command, width, height);
                    sdk_nr::ngx_result("evaluate", api.call(call))?;
                }
                evaluations += 1;
            } else {
                paused += u32::from(enabled);
                let source = vk::Image::from_raw(resources[0].image);
                let output = vk::Image::from_raw(resources[1].image);
                transition(
                    device,
                    command,
                    source,
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                );
                transition(
                    device,
                    command,
                    output,
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
                let sub = vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1);
                device.cmd_copy_image(
                    command,
                    source,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    output,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::ImageCopy::default()
                        .src_subresource(sub)
                        .dst_subresource(sub)
                        .extent(vk::Extent3D {
                            width,
                            height,
                            depth: 1,
                        })],
                );
                transition(
                    device,
                    command,
                    source,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
                transition(
                    device,
                    command,
                    output,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
            }
            let output = vk::Image::from_raw(resources[1].image);
            transition(
                device,
                command,
                output,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            device.cmd_copy_image_to_buffer(
                command,
                output,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback.0,
                &[region(width, height)],
            );
            transition(
                device,
                command,
                output,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)],
                &[],
                &[],
            );
            sdk_nr::event(
                "submit_enter",
                json!({"cycle":cycle,"frame":frame,"fence_timeout_ns":10_000_000_000u64}),
            )?;
            submit(device, queue, command, fence)?;
            let mapped = device.map_memory(readback.1, 0, bytes, vk::MemoryMapFlags::empty())?;
            let output = std::slice::from_raw_parts(mapped.cast::<u16>(), input.len()).to_vec();
            device.unmap_memory(readback.1);
            let sentinel = [half(-16.0), half(0.0), half(-16.0), half(1.0)];
            let written = output.chunks_exact(4).filter(|p| *p != sentinel).count();
            let finite = output.iter().all(|&v| decode_half(v).is_finite());
            let digest = format!(
                "{:x}",
                Sha256::digest(std::slice::from_raw_parts(
                    output.as_ptr().cast::<u8>(),
                    bytes as usize
                ))
            );
            if !evaluate && output != input {
                return Err("NR-off upload/readback control mismatch".into());
            }
            if !finite || written * 100 < (width * height) as usize * 99 {
                return Err("NR output nonfinite or retained sentinel".into());
            }
            if frame > 0 && valid_motion && previous_motion {
                moving_pairs += 1;
                changed_pairs += usize::from(digest != previous_hash);
            }
            let input_difference = output
                .iter()
                .zip(&input)
                .map(|(&a, &b)| (decode_half(a) - decode_half(b)).abs() as f64)
                .sum::<f64>()
                / input.len() as f64;
            let zero_difference = if cycle == 2 && frame < 3 {
                Some(
                    output
                        .iter()
                        .zip(&zero_controls[frame as usize])
                        .map(|(&a, &b)| (decode_half(a) - decode_half(b)).abs() as f64)
                        .sum::<f64>()
                        / input.len() as f64,
                )
            } else {
                None
            };
            response_detected |= zero_difference.is_some_and(|delta| delta > 1e-5);
            if cycle == 1 && frame < 3 {
                zero_controls.push(output.clone());
            }
            sdk_nr::event(
                "frame_complete",
                json!({"cycle":cycle,"frame":frame,"nr_enabled":evaluate,"nr_requested":enabled,"intensity":intensity,
                "reset":reset,"reset_reason":reason,"motion_valid":valid_motion,"depth":"synthetic_constant",
                "source_image":resources[0].image,"paused_without_motion":enabled&&!evaluate,"pending_sr_reset":decision.reset_sr,"pending_fg_reset":decision.reset_fg,
                "motion":"synthetic_uv_current_to_previous","fence_completed":true,"written_pixels":written,
                "finite":finite,"sha256":digest,"mean_absolute_input_difference":input_difference,"mean_absolute_zero_intensity_difference":zero_difference}),
            )?;
            if frame < 3 || frame == frames - 1 {
                ppm(
                    &options
                        .session
                        .join(format!("nr-output-{cycle}-{frame}.ppm")),
                    width,
                    height,
                    &output,
                )?;
                ppm(
                    &options
                        .session
                        .join(format!("nr-input-{cycle}-{frame}.ppm")),
                    width,
                    height,
                    &input,
                )?;
            }
            previous_hash = digest;
            previous_motion = valid_motion;
            if cycle == 2 && evaluate && !interleaved {
                if let Some(callback) = after_frame.as_deref_mut() {
                    callback(cycle, frame)?;
                }
                interleaved = true;
            }
        }
        if enabled {
            let mut call = nr_api::empty_call(4);
            call.feature = feature;
            sdk_nr::event("release_feature_enter", json!({"cycle":cycle}))?;
            sdk_nr::ngx_result("release_feature", api.call(call))?;
        }
        summary.push(
            json!({"cycle":cycle,"nr_enabled":enabled,"intensity":intensity,"frames":frames,
            "moving_pairs":moving_pairs,"changed_moving_pairs":changed_pairs,"released":enabled}),
        );
        if enabled && moving_pairs != changed_pairs {
            return Err("NR moving output stopped updating".into());
        }
    }
    for (resource, memory) in allocated {
        device.destroy_image_view(vk::ImageView::from_raw(resource.view), None);
        device.destroy_image(vk::Image::from_raw(resource.image), None);
        device.free_memory(memory, None);
    }
    for (buffer, memory) in [upload, readback] {
        device.destroy_buffer(buffer, None);
        device.free_memory(memory, None);
    }
    device.destroy_fence(fence, None);
    device.destroy_command_pool(pool, None);
    let summary = json!({"cycles":summary,"intensity_response_detected":response_detected,
        "evaluations":evaluations,"paused_frames":paused,"synthetic_inputs":true,"readback_complete":true,"zero_motion_policy_verified":options.pause_without_motion});
    sdk_nr::write_json(&options.session.join("nr-output-summary.json"), &summary)?;
    if !response_detected {
        return Err(
            "nonzero intensity did not differ from the matching zero-intensity control".into(),
        );
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_half_roundtrip_and_special_readback() {
        for value in [0.0, 1.0, -16.0, 0.5, -2.0 / 640.0, 0.125] {
            assert!((decode_half(half(value)) - value).abs() < 0.00001);
        }
        assert!(decode_half(0x7c00).is_infinite());
        assert!(decode_half(0x7e00).is_nan());
        assert_eq!(decode_half(1), 2f32.powi(-24));
    }
    #[test]
    fn zero_motion_is_a_reset_experiment_and_recovery_resets() {
        assert_eq!(motion_state(0, 300), (0, false, true, "feature_created"));
        assert_eq!(
            motion_state(1, 300),
            (2, true, true, "motion_warmup_complete")
        );
        assert_eq!(
            motion_state(99, 300),
            (198, true, false, "continuous_motion")
        );
        assert_eq!(
            motion_state(100, 300),
            (198, false, true, "zero_motion_reset_each_frame_experiment")
        );
        assert_eq!(
            motion_state(199, 300),
            (198, false, true, "zero_motion_reset_each_frame_experiment")
        );
        assert_eq!(motion_state(200, 300), (200, true, true, "motion_restored"));
        assert_eq!(
            motion_state(201, 300),
            (202, true, false, "continuous_motion")
        );
    }
}
