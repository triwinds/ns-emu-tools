use super::*;
static VALIDATION_ERRORS: AtomicU32 = AtomicU32::new(0);
unsafe extern "system" fn validation_message(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    _: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut std::ffi::c_void,
) -> vk::Bool32 {
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        VALIDATION_ERRORS.fetch_add(1, Ordering::Relaxed);
    }
    eprintln!(
        "Vulkan: {}",
        CStr::from_ptr((*data).p_message).to_string_lossy()
    );
    vk::FALSE
}

unsafe fn ordinary_image(
    d: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    width: u32,
    height: u32,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
) -> (Image, Resource) {
    let r = texture_with_usage(d, props, vk::Extent2D { width, height }, format, usage).unwrap();
    (
        Image {
            device: d.clone(),
            image: vk::Image::from_raw(r.image),
            memory: vk::DeviceMemory::from_raw(r.memory),
            view: vk::ImageView::from_raw(r.view),
            extent: vk::Extent2D { width, height },
        },
        r,
    )
}
#[test]
#[ignore = "Runs the NVIDIA hardware optical-flow engine on a local Vulkan GPU"]
fn hardware_translation_reset_and_static() {
    run_hardware_sequence(false, None, Default::default());
}

#[test]
#[ignore = "Runs both temporal-hint modes on a local NVIDIA Vulkan GPU"]
fn hardware_fast_pan_with_and_without_temporal_hints() {
    for hints in [false, true] {
        run_hardware_sequence(true, Some(hints), Default::default());
    }
}

#[test]
#[ignore = "Validates all three hardware optical-flow quality levels and configurable FG protection on a local GPU"]
fn hardware_optical_flow_quality_levels() {
    use crate::advanced_settings::OpticalFlowQuality;
    for quality in [
        OpticalFlowQuality::Quality,
        OpticalFlowQuality::Balanced,
        OpticalFlowQuality::Performance,
    ] {
        run_hardware_sequence(false, None, quality);
    }
}

#[test]
#[ignore = "Validates cropped FP16 SR pixel motion with Vulkan GPU readback and synchronization validation"]
fn hardware_sr_motion_crop_and_scale() {
    unsafe {
        VALIDATION_ERRORS.store(0, Ordering::Relaxed);
        let entry = ash::Entry::load().unwrap();
        let checks = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
        let mut validation =
            vk::ValidationFeaturesEXT::default().enabled_validation_features(&checks);
        let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_3);
        let instance = entry
            .create_instance(
                &vk::InstanceCreateInfo::default()
                    .application_info(&app)
                    .enabled_extension_names(&[ash::ext::debug_utils::NAME.as_ptr()])
                    .enabled_layer_names(&[c"VK_LAYER_KHRONOS_validation".as_ptr()])
                    .push_next(&mut validation),
                None,
            )
            .unwrap();
        let debug = ash::ext::debug_utils::Instance::new(&entry, &instance);
        let messenger = debug
            .create_debug_utils_messenger(
                &vk::DebugUtilsMessengerCreateInfoEXT::default()
                    .message_severity(
                        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                            | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                    )
                    .message_type(
                        vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                            | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                    )
                    .pfn_user_callback(Some(validation_message)),
                None,
            )
            .unwrap();
        let physical = instance
            .enumerate_physical_devices()
            .unwrap()
            .into_iter()
            .find(|p| instance.get_physical_device_properties(*p).vendor_id == 0x10de)
            .unwrap();
        let family = instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .position(|q| {
                q.queue_flags
                    .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
            })
            .unwrap() as u32;
        let device = instance
            .create_device(
                physical,
                &vk::DeviceCreateInfo::default().queue_create_infos(&[
                    vk::DeviceQueueCreateInfo::default()
                        .queue_family_index(family)
                        .queue_priorities(&[1.0]),
                ]),
                None,
            )
            .unwrap();
        let d = &device;
        let props = instance.get_physical_device_memory_properties(physical);
        let pool = d
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
            .unwrap();
        let cmd = d
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .unwrap()[0];
        let fence = d
            .create_fence(&vk::FenceCreateInfo::default(), None)
            .unwrap();
        let queue = d.get_device_queue(family, 0);
        let (width, height) = (39u32, 23u32);
        let crop = [3.0, 2.0, 31.0, 19.0];
        let input_halfs: Vec<u16> = (0..width * height)
            .flat_map(|i| {
                let (x, y) = (i % width, i / width);
                if !(3..34).contains(&x) || !(2..21).contains(&y) {
                    [0x5c00, 0xdc00]
                } else if x < 18 {
                    [0x4400, 0xc800]
                } else {
                    [0x4a00, 0xcc00]
                }
            })
            .collect();
        let input_bytes: Vec<u8> = input_halfs.iter().flat_map(|v| v.to_le_bytes()).collect();
        let sample = |x: f32, y: f32, channel: usize| {
            let (bx, by) = (x.floor() as i32, y.floor() as i32);
            let (fx, fy) = (x - x.floor(), y - y.floor());
            let mut result = 0.;
            for dy in 0..2 {
                for dx in 0..2 {
                    let px = (bx + dx).clamp(0, width as i32 - 1) as u32;
                    let py = (by + dy).clamp(0, height as i32 - 1) as u32;
                    let weight =
                        (if dx == 0 { 1. - fx } else { fx }) * (if dy == 0 { 1. - fy } else { fy });
                    result += crate::frame_capture::half(
                        input_halfs[((py * width + px) * 2) as usize + channel],
                    ) * weight;
                }
            }
            result
        };
        for (out_width, out_height) in [(17u32, 11u32), (23, 13), (8, 6), (55, 31)] {
            let output_bytes = vec![0u8; (out_width * out_height * 4) as usize];
            let usage = vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST;
            let (input, input_resource) = ordinary_image(
                d,
                &props,
                width,
                height,
                vk::Format::R16G16_SFLOAT,
                usage | vk::ImageUsageFlags::SAMPLED,
            );
            let (output, output_resource) = ordinary_image(
                d,
                &props,
                out_width,
                out_height,
                vk::Format::R16G16_SFLOAT,
                usage | vk::ImageUsageFlags::STORAGE,
            );
            let total = input_bytes.len() + output_bytes.len();
            let upload = buffer(
                d,
                &props,
                total as u64,
                vk::BufferUsageFlags::TRANSFER_SRC,
                true,
            )
            .unwrap();
            let read = buffer(
                d,
                &props,
                total as u64,
                vk::BufferUsageFlags::TRANSFER_DST,
                true,
            )
            .unwrap();
            let pointer = d
                .map_memory(upload.memory, 0, total as u64, vk::MemoryMapFlags::empty())
                .unwrap()
                .cast::<u8>();
            std::ptr::copy_nonoverlapping(input_bytes.as_ptr(), pointer, input_bytes.len());
            std::ptr::copy_nonoverlapping(
                output_bytes.as_ptr(),
                pointer.add(input_bytes.len()),
                output_bytes.len(),
            );
            d.unmap_memory(upload.memory);
            let adapter = crate::target_sr_motion::Adapter::new(d, output_resource).unwrap();
            d.reset_command_pool(pool, vk::CommandPoolResetFlags::empty())
                .unwrap();
            d.reset_fences(&[fence]).unwrap();
            d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            let layers = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1);
            for (image, w, h, offset) in [
                (input.image, width, height, 0),
                (
                    output.image,
                    out_width,
                    out_height,
                    input_bytes.len() as u64,
                ),
            ] {
                transition(
                    d,
                    cmd,
                    image,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
                d.cmd_copy_buffer_to_image(
                    cmd,
                    upload.buffer,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::BufferImageCopy::default()
                        .buffer_offset(offset)
                        .image_subresource(layers)
                        .image_extent(vk::Extent3D {
                            width: w,
                            height: h,
                            depth: 1,
                        })],
                );
                transition(
                    d,
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
            }
            adapter.record(cmd, input_resource, crop).unwrap();
            for (image, w, h, offset) in [
                (input.image, width, height, 0),
                (
                    output.image,
                    out_width,
                    out_height,
                    input_bytes.len() as u64,
                ),
            ] {
                transition(
                    d,
                    cmd,
                    image,
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                );
                d.cmd_copy_image_to_buffer(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    read.buffer,
                    &[vk::BufferImageCopy::default()
                        .buffer_offset(offset)
                        .image_subresource(layers)
                        .image_extent(vk::Extent3D {
                            width: w,
                            height: h,
                            depth: 1,
                        })],
                );
            }
            d.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)],
                &[],
                &[],
            );
            d.end_command_buffer(cmd).unwrap();
            d.queue_submit(
                queue,
                &[vk::SubmitInfo::default().command_buffers(&[cmd])],
                fence,
            )
            .unwrap();
            d.wait_for_fences(&[fence], true, 5_000_000_000).unwrap();
            let pointer = d
                .map_memory(read.memory, 0, total as u64, vk::MemoryMapFlags::empty())
                .unwrap()
                .cast::<u8>();
            let captured = std::slice::from_raw_parts(pointer, total).to_vec();
            d.unmap_memory(read.memory);
            assert_eq!(
                &captured[..input_bytes.len()],
                &input_bytes,
                "SR input was modified"
            );
            let result: Vec<f32> = captured[input_bytes.len()..]
                .chunks_exact(2)
                .map(|v| crate::frame_capture::half(u16::from_le_bytes([v[0], v[1]])))
                .collect();
            for y in 0..out_height {
                for x in 0..out_width {
                    let pos = [
                        (crop[0] + (x as f32 + 0.5) * crop[2] / out_width as f32)
                            .clamp(crop[0] + 0.5, crop[0] + crop[2] - 0.5)
                            - 0.5,
                        (crop[1] + (y as f32 + 0.5) * crop[3] / out_height as f32)
                            .clamp(crop[1] + 0.5, crop[1] + crop[3] - 0.5)
                            - 0.5,
                    ];
                    for (axis, scale) in [out_width as f32 / crop[2], out_height as f32 / crop[3]]
                        .into_iter()
                        .enumerate()
                    {
                        let expected = sample(pos[0], pos[1], axis) * scale;
                        let actual = result[((y * out_width + x) * 2) as usize + axis];
                        assert!(actual.is_finite() && (actual - expected).abs() < 0.02 + expected.abs() * 0.001,
                            "pixel motion crop/scale mismatch ({x},{y}) axis {axis}: {actual} vs {expected}");
                    }
                }
            }
            println!("SR FP16 pixels {width}x{height}, crop {crop:?} -> {out_width}x{out_height}: scale, direction, letterbox isolation and input preservation passed");
            drop(adapter);
            drop(input);
            drop(output);
            drop(upload);
            drop(read);
        }
        d.destroy_fence(fence, None);
        d.destroy_command_pool(pool, None);
        d.destroy_device(None);
        debug.destroy_debug_utils_messenger(messenger, None);
        instance.destroy_instance(None);
        assert_eq!(
            VALIDATION_ERRORS.load(Ordering::Relaxed),
            0,
            "Vulkan validation errors"
        );
    }
}
fn sample_grid(grid: vk::Extent2D, p: [f32; 2], load: impl Fn(usize) -> f32) -> f32 {
    let position = [p[0] / 4. - 0.5, p[1] / 4. - 0.5];
    let base = [position[0].floor() as i32, position[1].floor() as i32];
    let fraction = [position[0] - base[0] as f32, position[1] - base[1] as f32];
    let at = |dx: i32, dy: i32| {
        let x = (base[0] + dx).clamp(0, grid.width as i32 - 1) as usize;
        let y = (base[1] + dy).clamp(0, grid.height as i32 - 1) as usize;
        load(y * grid.width as usize + x)
    };
    let upper = at(0, 0) * (1. - fraction[0]) + at(1, 0) * fraction[0];
    let lower = at(0, 1) * (1. - fraction[0]) + at(1, 1) * fraction[0];
    upper * (1. - fraction[1]) + lower * fraction[1]
}
fn sample_flow(raw: &[u32], grid: vk::Extent2D, p: [f32; 2]) -> [f32; 2] {
    [0, 1].map(|axis| {
        sample_grid(grid, p, |i| {
            let word = raw[i];
            ((word >> (axis * 16)) as u16 as i16) as f32 / 32.
        })
    })
}
fn sample_fg_flow(raw: &[u32], grid: vk::Extent2D, p: [f32; 2]) -> [f32; 2] {
    let at = |x: i32, y: i32| {
        let x = x.clamp(0, grid.width as i32 - 1) as usize;
        let y = y.clamp(0, grid.height as i32 - 1) as usize;
        let word = raw[y * grid.width as usize + x];
        [
            word as u16 as i16 as f32 / 32.,
            (word >> 16) as u16 as i16 as f32 / 32.,
        ]
    };
    let anchor = at((p[0] / 4.).floor() as i32, (p[1] / 4.).floor() as i32);
    let pos = [p[0] / 4. - 0.5, p[1] / 4. - 0.5];
    let base = pos.map(|v| v.floor() as i32);
    let f = [pos[0] - base[0] as f32, pos[1] - base[1] as f32];
    let mut sum = [0.; 2];
    let mut support = 0.;
    for y in 0..2 {
        for x in 0..2 {
            let v = at(base[0] + x, base[1] + y);
            if (v[0] - anchor[0]).powi(2) + (v[1] - anchor[1]).powi(2) <= 4. {
                let weight = (if x == 0 { 1. - f[0] } else { f[0] })
                    * (if y == 0 { 1. - f[1] } else { f[1] });
                for axis in 0..2 {
                    sum[axis] += v[axis] * weight;
                }
                support += weight;
            }
        }
    }
    sum.map(|v| v / support)
}

// Exercise the actual shipped shader with opposing velocities, independently
// of optical-flow estimation. No pixel at the boundary may acquire a fictitious
// in-between velocity. Also cover reset and exact-static protection.
unsafe fn verify_dense_boundary(
    flow: &Flow,
    queue: vk::Queue,
    cmd: vk::CommandBuffer,
    read: &Buffer,
) {
    let d = &flow.device;
    let words = (0..flow.grid.height)
        .flat_map(|_| {
            (0..flow.grid.width).map(|x| {
                let velocity: i16 = if x < flow.grid.width / 2 { 512 } else { -512 };
                u32::from(velocity as u16)
            })
        })
        .collect::<Vec<_>>();
    let bytes = words
        .iter()
        .flat_map(|w| w.to_ne_bytes())
        .collect::<Vec<_>>();
    let region = vk::BufferImageCopy::default()
        .image_subresource(
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1),
        )
        .image_extent(vk::Extent3D {
            width: flow.extent.width,
            height: flow.extent.height,
            depth: 1,
        });
    for (reset, flags, threshold) in [
        (0u32, 8u32, 0u32),
        (0, 8, 2),
        (0, 8, 32),
        (0, 0, 2),
        (1, 8, 2),
        (0, 12, 2),
    ] {
        d.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
            .unwrap();
        d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
            .unwrap();
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)],
            &[],
            &[],
        );
        for (i, chunk) in bytes.chunks(65536).enumerate() {
            d.cmd_update_buffer(
                cmd,
                flow.data.as_ref().unwrap().buffer,
                (i * 65536) as u64,
                chunk,
            );
        }
        d.cmd_fill_buffer(
            cmd,
            flow.difference.as_ref().unwrap().flags.buffer,
            0,
            vk::WHOLE_SIZE,
            1,
        );
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)],
            &[],
            &[],
        );
        d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, flow.pipeline);
        d.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            flow.pipeline_layout,
            0,
            &[flow.set],
            &[],
        );
        let constants = [flow.grid.width, flow.grid.height, reset, flags, threshold]
            .iter()
            .flat_map(|v| v.to_ne_bytes())
            .collect::<Vec<_>>();
        d.cmd_push_constants(
            cmd,
            flow.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &constants,
        );
        d.cmd_dispatch(
            cmd,
            flow.extent.width.div_ceil(8),
            flow.extent.height.div_ceil(8),
            1,
        );
        let image = flow.pixel_motion.as_ref().unwrap().0.image;
        transition(
            d,
            cmd,
            image,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        d.cmd_copy_image_to_buffer(
            cmd,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            read.buffer,
            &[region],
        );
        transition(
            d,
            cmd,
            image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)],
            &[],
            &[],
        );
        d.end_command_buffer(cmd).unwrap();
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default().command_buffers(&[cmd])],
            vk::Fence::null(),
        )
        .unwrap();
        d.queue_wait_idle(queue).unwrap();
        let count = (flow.extent.width * flow.extent.height * 2) as usize;
        let ptr = d
            .map_memory(
                read.memory,
                0,
                (count * 2) as u64,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();
        let values = std::slice::from_raw_parts(ptr.cast::<u16>(), count);
        for y in 0..flow.extent.height {
            for x in 0..flow.extent.width {
                let at = ((y * flow.extent.width + x) * 2) as usize;
                let boundary = (flow.grid.width / 2 * 4) as i32;
                let expected = if reset != 0 || flags & 4 != 0 {
                    0.
                } else if (flags & 8 == 0 || threshold == 32)
                    && (boundary - 2..boundary + 2).contains(&(x as i32))
                {
                    [12., 4., -4., -12.][(x as i32 - boundary + 2) as usize]
                } else if x / 4 < flow.grid.width / 2 {
                    16.
                } else {
                    -16.
                };
                assert_eq!(
                    crate::frame_capture::half(values[at]),
                    expected,
                    "opposing FG layers mixed at ({x},{y}) reset={reset} flags={flags}"
                );
                assert_eq!(crate::frame_capture::half(values[at + 1]), 0.);
            }
        }
        d.unmap_memory(read.memory);
    }
    println!("GPU FG boundary/reset/static regression passed");
}
fn run_hardware_sequence(
    fast_pan: bool,
    temporal_hints: Option<bool>,
    quality: crate::advanced_settings::OpticalFlowQuality,
) {
    unsafe {
        let entry = ash::Entry::load().unwrap();
        let validation = std::env::var_os("NS_NVOF_VALIDATION").is_some();
        let extensions = if validation {
            vec![ash::ext::debug_utils::NAME.as_ptr()]
        } else {
            vec![]
        };
        let layers = if validation {
            vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
        } else {
            vec![]
        };
        let checks = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
        let mut validation_features =
            vk::ValidationFeaturesEXT::default().enabled_validation_features(&checks);
        let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_3);
        let mut info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&extensions)
            .enabled_layer_names(&layers);
        if validation {
            info = info.push_next(&mut validation_features);
        }
        let instance = entry.create_instance(&info, None).unwrap();
        let debug = ash::ext::debug_utils::Instance::new(&entry, &instance);
        let messenger = if validation {
            Some(
                debug
                    .create_debug_utils_messenger(
                        &vk::DebugUtilsMessengerCreateInfoEXT::default()
                            .message_severity(
                                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
                            )
                            .message_type(
                                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
                            )
                            .pfn_user_callback(Some(validation_message)),
                        None,
                    )
                    .unwrap(),
            )
        } else {
            None
        };
        let physical = instance
            .enumerate_physical_devices()
            .unwrap()
            .into_iter()
            .find(|p| instance.get_physical_device_properties(*p).vendor_id == 0x10de)
            .unwrap();
        let graphics = instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .position(|q| {
                q.queue_flags
                    .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
            })
            .unwrap() as u32;
        let optical = capability(&instance, physical, graphics).unwrap();
        let mut of = vk::PhysicalDeviceOpticalFlowFeaturesNV::default().optical_flow(true);
        let mut sync = vk::PhysicalDeviceVulkan13Features::default().synchronization2(true);
        let device = instance
            .create_device(
                physical,
                &vk::DeviceCreateInfo::default()
                    .enabled_features(
                        &vk::PhysicalDeviceFeatures::default()
                            .shader_storage_image_extended_formats(true),
                    )
                    .enabled_extension_names(&[ash::nv::optical_flow::NAME.as_ptr()])
                    .queue_create_infos(&[
                        vk::DeviceQueueCreateInfo::default()
                            .queue_family_index(graphics)
                            .queue_priorities(&[1.0]),
                        vk::DeviceQueueCreateInfo::default()
                            .queue_family_index(optical)
                            .queue_priorities(&[1.0]),
                    ])
                    .push_next(&mut of)
                    .push_next(&mut sync),
                None,
            )
            .unwrap();
        let d = &device;
        let q = d.get_device_queue(graphics, 0);
        let props = instance.get_physical_device_memory_properties(physical);
        let (width, height) = if fast_pan {
            (1024u32, 576u32)
        } else if std::env::var_os("NS_NVOF_TEST_GAME_EXTENT").is_some() {
            (2560u32, 1335u32)
        } else if std::env::var_os("NS_NVOF_TEST_1080P").is_some() {
            (1920u32, 1080u32)
        } else {
            (321u32, 193u32)
        };
        let pixels = (width * height) as usize;
        let (source, _) = ordinary_image(
            d,
            &props,
            width,
            height,
            vk::Format::B8G8R8A8_UNORM,
            vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST
                | vk::ImageUsageFlags::SAMPLED,
        );
        let (output, r) = ordinary_image(
            d,
            &props,
            width,
            height,
            vk::Format::R32G32_SFLOAT,
            vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
        );
        let upload = buffer(
            d,
            &props,
            (pixels * 4) as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            true,
        )
        .unwrap();
        let grid_count = u64::from(width.div_ceil(4)) * u64::from(height.div_ceil(4));
        let raw_offset = (pixels as u64 * 16).div_ceil(4) * 4;
        let read_bytes = raw_offset + grid_count * 4;
        let read = buffer(
            d,
            &props,
            read_bytes,
            vk::BufferUsageFlags::TRANSFER_DST,
            true,
        )
        .unwrap();
        let pool = d
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(graphics)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
            .unwrap();
        let cmd = d
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .unwrap()[0];
        let read_cmd = d
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .unwrap()[0];
        let app_ready = d
            .create_semaphore(&vk::SemaphoreCreateInfo::default(), None)
            .unwrap();
        let mut flow = Flow::create_tuned(
            &instance,
            physical,
            d,
            graphics,
            optical,
            vk::SwapchainKHR::null(),
            r,
            crate::advanced_settings::OpticalFlowOptions {
                quality,
                ..Default::default()
            },
        )
        .unwrap();
        if let Some(hints) = temporal_hints {
            flow.temporal_hints = hints;
        }
        let layers = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        let region = vk::BufferImageCopy::default()
            .image_subresource(layers)
            .image_extent(vk::Extent3D {
                width,
                height,
                depth: 1,
            });
        let mut cases = if fast_pan {
            vec![
                (0, 0, true),
                (8, 0, false),
                (24, 0, false),
                (48, 0, false),
                (80, 0, false),
                (128, 0, false),
                (192, 0, false),
                (256, 0, false),
                (320, 0, false),
                (384, 0, false),
                (448, 0, false),
                (512, 0, false),
                (512, 0, false),
                (512, 0, false),
                (448, 0, false),
                (384, 0, false),
                (320, 0, false),
                (256, 0, false),
                (256, 0, true),
                (256, 0, false),
            ]
        } else {
            vec![
                (0, 0, true),
                (0, 0, false),
                (8, 0, false),
                (8, 0, false),
                (8, 6, false),
                (-4, 6, true),
                (-4, 6, false),
                (-12, 6, false),
                (1300, 900, false),
                (1300, 900, false),
                (1300, 900, false),
            ]
        };
        let last = *cases.last().unwrap();
        cases.extend([(last.0, last.1, false); 2]);
        let mutation_frame = cases.len() - 2;
        println!("temporal_hints={} fast_pan={fast_pan}", flow.temporal_hints);
        let mut previous = (0, 0);
        let mut previous_mutation = false;
        let mut previous_pixels: Option<Vec<u8>> = None;
        for (frame, (sx, sy, reset)) in cases.into_iter().enumerate() {
            let ptr = d
                .map_memory(
                    upload.memory,
                    0,
                    (pixels * 4) as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap()
                .cast::<u8>();
            let bytes = std::slice::from_raw_parts_mut(ptr, pixels * 4);
            for y in 0..height as i32 {
                for x in 0..width as i32 {
                    // Textured plane translated independently along both axes.
                    let a = (x - sx) as u32;
                    let b = (y - sy) as u32;
                    let value = ((a.wrapping_mul(7919) ^ b.wrapping_mul(104729))
                        .wrapping_mul(2654435761)
                        >> 24) as u8;
                    let at = ((y as u32 * width + x as u32) * 4) as usize;
                    bytes[at..at + 4].copy_from_slice(&[value, value, value, 255]);
                    // Static UI must keep exact-zero FG motion, while NR/SR
                    // retain the raw field.
                    if x < 32 && y < 32 {
                        bytes[at..at + 4].copy_from_slice(&[8, 16, 32, 255]);
                    }
                }
            }
            let mutate_alpha = frame == mutation_frame;
            if mutate_alpha {
                bytes[pixels * 4 - 1] = 254;
            }
            let current_pixels = bytes.to_vec();
            d.unmap_memory(upload.memory);
            d.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .unwrap();
            d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            if frame == 0 {
                transition(
                    d,
                    cmd,
                    output.image,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::GENERAL,
                );
            }
            transition(
                d,
                cmd,
                source.image,
                if frame == 0 {
                    vk::ImageLayout::UNDEFINED
                } else {
                    vk::ImageLayout::GENERAL
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            d.cmd_copy_buffer_to_image(
                cmd,
                upload.buffer,
                source.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );
            transition(
                d,
                cmd,
                source.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            d.end_command_buffer(cmd).unwrap();
            d.queue_submit(
                q,
                &[vk::SubmitInfo::default()
                    .command_buffers(&[cmd])
                    .signal_semaphores(&[app_ready])],
                vk::Fence::null(),
            )
            .unwrap();
            // Synthetic CPU image generation is not a source-frame pause.
            flow.last_frame = Some(Instant::now());
            let started = Instant::now();
            let guidance = flow
                .run_source(
                    q,
                    source.image,
                    vk::ImageLayout::GENERAL,
                    &[app_ready],
                    r,
                    reset,
                    frame as u32,
                )
                .unwrap();
            let ready = guidance.ready;
            let history_reset = guidance.reset;
            assert_eq!(
                guidance.duplicate,
                !reset && previous == (sx, sy) && mutate_alpha == previous_mutation,
                "exact comparison must include alpha and partial edge tiles"
            );
            println!(
                "NVOF {width}x{height} frame={frame} cpu_submit_us={}",
                started.elapsed().as_micros()
            );
            // run_source now returns before the producer completes. Readback
            // must use a different command buffer and wait on the returned semaphore.
            let cmd = read_cmd;
            d.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .unwrap();
            d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            transition(
                d,
                cmd,
                output.image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            d.cmd_copy_image_to_buffer(
                cmd,
                output.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                read.buffer,
                &[region],
            );
            transition(
                d,
                cmd,
                output.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            for (image, offset) in [
                (
                    flow.pixel_motion.as_ref().unwrap().0.image,
                    pixels as u64 * 8,
                ),
                (
                    flow.sr_pixel_motion.as_ref().unwrap().0.image,
                    pixels as u64 * 12,
                ),
            ] {
                transition(
                    d,
                    cmd,
                    image,
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                );
                d.cmd_copy_image_to_buffer(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    read.buffer,
                    &[region.buffer_offset(offset)],
                );
                transition(
                    d,
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
            }
            if !history_reset && !guidance.duplicate {
                d.cmd_copy_buffer(
                    cmd,
                    flow.data.as_ref().unwrap().buffer,
                    read.buffer,
                    &[vk::BufferCopy {
                        src_offset: 0,
                        dst_offset: raw_offset,
                        size: grid_count * 4,
                    }],
                );
            }
            d.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)],
                &[],
                &[],
            );
            d.end_command_buffer(cmd).unwrap();
            d.queue_submit(
                q,
                &[vk::SubmitInfo::default()
                    .command_buffers(&[cmd])
                    .wait_semaphores(&[ready])
                    .wait_dst_stage_mask(&[vk::PipelineStageFlags::ALL_COMMANDS])],
                vk::Fence::null(),
            )
            .unwrap();
            d.queue_wait_idle(q).unwrap();
            if let Some(timing) = &flow.timing {
                let mut ticks = [0u64; 4];
                d.get_query_pool_results(timing.pool, 0, &mut ticks, vk::QueryResultFlags::TYPE_64)
                    .unwrap();
                let mask = u64::MAX >> (64 - timing.bits);
                let us = |a: usize, b: usize| {
                    ((ticks[b].wrapping_sub(ticks[a])) & mask) as f64 * f64::from(timing.period)
                        / 1000.
                };
                let total = us(0, 3);
                assert!(total > 0. && total.is_finite());
                println!("GPU copy_us={:.1} flow_and_queue_gap_us={:.1} map_copy_and_dense_us={:.1} total_us={:.1}", us(0,1),us(1,2),us(2,3),total);
            }
            let ptr = d
                .map_memory(read.memory, 0, read_bytes, vk::MemoryMapFlags::empty())
                .unwrap();
            let result = std::slice::from_raw_parts(ptr.cast::<f32>(), pixels * 2).to_vec();
            let halfs = std::slice::from_raw_parts(
                ptr.cast::<u8>().add(pixels * 8).cast::<u16>(),
                pixels * 2,
            );
            let pixel_motion = halfs[..pixels * 2]
                .iter()
                .map(|v| crate::frame_capture::half(*v))
                .collect::<Vec<_>>();
            let sr_pixels = std::slice::from_raw_parts(
                ptr.cast::<u8>().add(pixels * 12).cast::<u16>(),
                pixels * 2,
            )
            .iter()
            .copied()
            .map(crate::frame_capture::half)
            .collect::<Vec<_>>();
            let raw = if history_reset || guidance.duplicate {
                vec![]
            } else {
                std::slice::from_raw_parts(
                    ptr.cast::<u8>().add(raw_offset as usize).cast::<u32>(),
                    grid_count as usize,
                )
                .to_vec()
            };
            d.unmap_memory(read.memory);
            assert!(result.iter().all(|v| v.is_finite()));
            assert!(pixel_motion.iter().all(|v| v.is_finite()));
            assert!(sr_pixels.iter().all(|v| v.is_finite()));
            if history_reset || guidance.duplicate {
                assert!(pixel_motion
                    .iter()
                    .chain(sr_pixels.iter())
                    .all(|v| *v == 0.));
            } else {
                for (x, y) in [
                    (0, 0),
                    (4, 4),
                    (15, 15),
                    (31, 31),
                    (width / 2, height / 2),
                    (width - 1, height - 1),
                ] {
                    let p = [x as f32 + 0.5, y as f32 + 0.5];
                    let forward = sample_flow(&raw, flow.grid, p);
                    let at = (y * width + x) as usize;
                    let unchanged = previous_pixels.as_ref().is_some_and(|previous| {
                        let tile_x = x / 4 * 4;
                        let tile_y = y / 4 * 4;
                        (tile_y..(tile_y + 4).min(height)).all(|py| {
                            (tile_x..(tile_x + 4).min(width)).all(|px| {
                                let i = ((py * width + px) * 4) as usize;
                                current_pixels[i..i + 4] == previous[i..i + 4]
                            })
                        })
                    });
                    for axis in 0..2 {
                        let scale = if axis == 0 { width } else { height } as f32;
                        assert!(
                            (result[at * 2 + axis] * scale - forward[axis]).abs() < 0.002,
                            "raw forward motion was altered at ({x},{y})"
                        );
                        assert!(
                            (sr_pixels[at * 2 + axis] - forward[axis]).abs()
                                < 0.002 + forward[axis].abs() * 0.001,
                            "raw SR FP16 motion was altered at ({x},{y})"
                        );
                        let expected_fg = if flow.static_guard && unchanged {
                            0.
                        } else {
                            sample_fg_flow(&raw, flow.grid, p)[axis]
                        };
                        assert!(
                            (pixel_motion[at * 2 + axis] - expected_fg).abs()
                                < 0.002 + expected_fg.abs() * 0.001,
                            "FG motion differs from the exact-static/raw contract at ({x},{y})"
                        );
                    }
                }
                if flow.static_guard {
                    for y in 0..32 {
                        for x in 0..32 {
                            let at = ((y * width + x) * 2) as usize;
                            assert_eq!(
                                &pixel_motion[at..at + 2],
                                &[0., 0.],
                                "static UI acquired FG motion"
                            );
                        }
                    }
                }
            }
            if frame == 8 && !fast_pan {
                let zero = result
                    .chunks_exact(2)
                    .filter(|v| v[0] == 0. && v[1] == 0.)
                    .count();
                println!("scene cut: zero vectors={zero}/{pixels}");
                assert!(
                    !history_reset,
                    "an unrelated color frame alone must not reset FG history"
                );
                assert!(
                    zero < pixels / 2,
                    "unrelated frames must retain the raw estimated flow"
                );
            } else if reset {
                assert!(history_reset);
                assert!(result.iter().all(|v| *v == 0.));
            } else {
                let expected = ((previous.0 - sx) as f32, (previous.1 - sy) as f32);
                let (mut correct, mut total) = (0, 0);
                let margin = if fast_pan { 96 } else { 32 };
                for y in margin..height - margin {
                    for x in margin..width - margin {
                        let i = ((y * width + x) * 2) as usize;
                        if (result[i] * width as f32 - expected.0).abs() < 1.0
                            && (result[i + 1] * height as f32 - expected.1).abs() < 1.0
                        {
                            correct += 1;
                        }
                        total += 1;
                    }
                }
                println!("frame={frame} expected={expected:?} matching={correct}/{total} reset={history_reset}");
                assert!(
                    !history_reset,
                    "unexpected history reset on coherent translation"
                );
                assert!(
                    correct * 100 > total * 85,
                    "NVOF direction or magnitude failed"
                );
            }
            previous = (sx, sy);
            previous_mutation = mutate_alpha;
            previous_pixels = Some(current_pixels);
        }
        d.device_wait_idle().unwrap();
        verify_dense_boundary(&flow, q, cmd, &read);
        drop(flow);
        drop(source);
        drop(output);
        drop(upload);
        drop(read);
        d.destroy_command_pool(pool, None);
        d.destroy_semaphore(app_ready, None);
        d.destroy_device(None);
        if let Some(messenger) = messenger {
            debug.destroy_debug_utils_messenger(messenger, None);
        }
        assert_eq!(
            VALIDATION_ERRORS.load(Ordering::Relaxed),
            0,
            "Vulkan validation errors"
        );
        instance.destroy_instance(None);
    }
}
