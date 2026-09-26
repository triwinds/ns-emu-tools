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
        let (width, height) = if std::env::var_os("NS_NVOF_TEST_GAME_EXTENT").is_some() {
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
        let read = buffer(
            d,
            &props,
            (pixels * 8) as u64,
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
        let mut flow = Flow::create(
            &instance,
            physical,
            d,
            graphics,
            optical,
            vk::SwapchainKHR::null(),
            r,
        )
        .unwrap();
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
        let cases = [
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
        ];
        let mut previous = (0, 0);
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
                }
            }
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
            let (ready, history_reset) = flow
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
                .map_memory(
                    read.memory,
                    0,
                    (pixels * 8) as u64,
                    vk::MemoryMapFlags::empty(),
                )
                .unwrap();
            let result = std::slice::from_raw_parts(ptr.cast::<f32>(), pixels * 2).to_vec();
            d.unmap_memory(read.memory);
            assert!(result.iter().all(|v| v.is_finite()));
            if frame == 8 {
                let zero = result
                    .chunks_exact(2)
                    .filter(|v| v[0] == 0. && v[1] == 0.)
                    .count();
                println!("scene cut: zero vectors={zero}/{pixels}");
                assert!(
                    !history_reset,
                    "low confidence alone must not reset FG history"
                );
                assert!(zero < pixels / 2, "raw flow must survive low confidence");
            } else if reset {
                assert!(history_reset);
                assert!(result.iter().all(|v| *v == 0.));
            } else {
                let expected = ((previous.0 - sx) as f32, (previous.1 - sy) as f32);
                let (mut correct, mut total) = (0, 0);
                for y in 32..height - 32 {
                    for x in 32..width - 32 {
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
                    "NVOF direction, magnitude or confidence failed"
                );
            }
            previous = (sx, sy);
        }
        d.device_wait_idle().unwrap();
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
