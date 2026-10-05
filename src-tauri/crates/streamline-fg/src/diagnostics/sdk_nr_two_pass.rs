//! Isolated real-NGX validation of independent features and fenced two-pass
//! recording. Synthetic colors/guides; no emulator or Streamline coexistence.
use super::*;

unsafe fn create(
    api: &nr_api::Api,
    device: &ash::Device,
    command: vk::CommandBuffer,
    params: *mut c_void,
    width: u32,
    height: u32,
) -> Result<*mut c_void> {
    nr_api::set_dimensions(params, width, height);
    let mut handle = std::ptr::null_mut();
    let mut call = nr_api::empty_call(2);
    call.device = device.handle().as_raw() as VkHandle;
    call.command = command.as_raw() as VkHandle;
    call.parameters = params;
    call.output = &mut handle;
    let _recording = crate::nr_layout::Recording::enter(command, width, height);
    sdk_nr::ngx_result("two_pass_create", api.call(call))?;
    if handle.is_null() {
        return Err("two-pass create returned null".into());
    }
    Ok(handle)
}
unsafe fn evaluate(
    api: &nr_api::Api,
    command: vk::CommandBuffer,
    params: *mut c_void,
    feature: *mut c_void,
    resources: &mut [ResourceVk; 4],
    reset: bool,
    intensity: f32,
    options: crate::advanced_settings::NrOptions,
) -> Result<()> {
    if options == Default::default() {
        nr_api::set_frame(params, resources, intensity, reset);
    } else {
        nr_api::set_frame_tuned(params, resources, intensity, reset, [1.0, 1.0], options);
    }
    let mut call = nr_api::empty_call(3);
    call.command = command.as_raw() as VkHandle;
    call.parameters = params;
    call.feature = feature;
    let _recording =
        crate::nr_layout::Recording::enter(command, resources[0].width, resources[0].height);
    sdk_nr::ngx_result("two_pass_evaluate", api.call(call))
}
unsafe fn release(api: &nr_api::Api, feature: *mut c_void) -> Result<()> {
    let mut call = nr_api::empty_call(4);
    call.feature = feature;
    sdk_nr::ngx_result("two_pass_release", api.call(call))
}
unsafe fn read(
    device: &ash::Device,
    queue: vk::Queue,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    resource: ResourceVk,
    readback: (vk::Buffer, vk::DeviceMemory),
) -> Result<Vec<u16>> {
    begin(device, command)?;
    let image = vk::Image::from_raw(resource.image);
    transition(
        device,
        command,
        image,
        vk::ImageLayout::GENERAL,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
    );
    device.cmd_copy_image_to_buffer(
        command,
        image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        readback.0,
        &[region(resource.width, resource.height)],
    );
    transition(
        device,
        command,
        image,
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
    submit(device, queue, command, fence)?;
    let bytes = u64::from(resource.width) * u64::from(resource.height) * 8;
    let mapped = device.map_memory(readback.1, 0, bytes, vk::MemoryMapFlags::empty())?;
    let pixels = std::slice::from_raw_parts(mapped.cast::<u16>(), bytes as usize / 2).to_vec();
    device.unmap_memory(readback.1);
    if pixels.iter().any(|v| !decode_half(*v).is_finite()) {
        return Err("nonfinite two-pass output".into());
    }
    Ok(pixels)
}
pub(super) unsafe fn exercise(
    options: &Options,
    api: &nr_api::Api,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
    first_params: *mut c_void,
) -> Result<Value> {
    let (width, height) = (options.width, options.height);
    let memory = instance.get_physical_device_memory_properties(physical);
    let allocated = [
        image(
            device,
            &memory,
            width,
            height,
            vk::Format::R16G16B16A16_SFLOAT,
            false,
        )?,
        image(
            device,
            &memory,
            width,
            height,
            vk::Format::R16G16B16A16_SFLOAT,
            true,
        )?,
        image(
            device,
            &memory,
            width,
            height,
            vk::Format::R32_SFLOAT,
            false,
        )?,
        image(
            device,
            &memory,
            width,
            height,
            vk::Format::R16G16_SFLOAT,
            false,
        )?,
        image(
            device,
            &memory,
            width,
            height,
            vk::Format::R16G16B16A16_SFLOAT,
            true,
        )?,
    ];
    let r = allocated.map(|(r, _)| r);
    let mut first = [r[0], r[1], r[2], r[3]];
    let mut second = [r[1], r[4], r[2], r[3]];
    second[0].read_write = 0;
    let bytes = u64::from(width) * u64::from(height) * 8;
    let upload = buffer(device, &memory, bytes, vk::BufferUsageFlags::TRANSFER_SRC)?;
    let readback = buffer(device, &memory, bytes, vk::BufferUsageFlags::TRANSFER_DST)?;
    let pool = device.create_command_pool(
        &vk::CommandPoolCreateInfo::default()
            .queue_family_index(family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        None,
    )?;
    let commands = device.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(2),
    )?;
    let queue = device.get_device_queue(family, 0);
    let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
    let mut second_params = std::ptr::null_mut();
    sdk_nr::ngx_result(
        "two_pass_allocate",
        nr_api::NVSDK_NGX_VULKAN_AllocateParameters(&mut second_params),
    )?;
    if second_params.is_null() || second_params == first_params {
        return Err("NR parameter maps are not independent".into());
    }
    let mut populate = nr_api::empty_call(1);
    populate.parameters = second_params;
    sdk_nr::ngx_result("two_pass_populate", api.call(populate))?;
    let mut first_handle: *mut c_void = std::ptr::null_mut();
    let mut second_handle: *mut c_void = std::ptr::null_mut();
    let mut second_fresh = true;
    let mut first_initialized = false;
    let mut secondary_initialized = false;
    let mut second_evaluations = 0;
    let mut discarded = 0;
    let mut output_changed = false;
    let mut recreated = 0;
    let mut output_hashes = Vec::new();
    for frame in 0..options.frames {
        let input = pattern(width, height, frame * 2);
        let mapped = device.map_memory(upload.1, 0, bytes, vk::MemoryMapFlags::empty())?;
        std::ptr::copy_nonoverlapping(input.as_ptr(), mapped.cast::<u16>(), input.len());
        device.unmap_memory(upload.1);
        begin(device, commands[0])?;
        for (i, resource) in first.iter().enumerate() {
            let image = vk::Image::from_raw(resource.image);
            transition(
                device,
                commands[0],
                image,
                if first_initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            if i == 0 {
                device.cmd_copy_buffer_to_image(
                    commands[0],
                    upload.0,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region(width, height)],
                );
            } else {
                device.cmd_clear_color_image(
                    commands[0],
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue {
                        float32: match i {
                            1 => [-16.0, 0.0, -16.0, 1.0],
                            2 => [0.5; 4],
                            _ => [
                                if frame == 0 { 0.0 } else { -2.0 / width as f32 },
                                0.0,
                                0.0,
                                0.0,
                            ],
                        },
                    },
                    &[range()],
                );
            }
            transition(
                device,
                commands[0],
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        if first_handle.is_null() {
            first_handle = create(api, device, commands[0], first_params, width, height)?;
        }
        evaluate(
            api,
            commands[0],
            first_params,
            first_handle,
            &mut first,
            frame == 0,
            1.0,
            Default::default(),
        )?;
        submit(device, queue, commands[0], fence)?;
        first_initialized = true;
        let first_output = read(device, queue, commands[0], fence, r[1], readback)?;
        let old_second = if frame == options.frames / 2 {
            Some(read(device, queue, commands[0], fence, r[4], readback)?)
        } else {
            None
        };
        begin(device, commands[1])?;
        transition(
            device,
            commands[1],
            vk::Image::from_raw(r[4].image),
            if secondary_initialized {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::UNDEFINED
            },
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        device.cmd_clear_color_image(
            commands[1],
            vk::Image::from_raw(r[4].image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue {
                float32: [-16.0, 0.0, -16.0, 1.0],
            },
            &[range()],
        );
        transition(
            device,
            commands[1],
            vk::Image::from_raw(r[4].image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        transition(
            device,
            commands[1],
            vk::Image::from_raw(r[1].image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::GENERAL,
        );
        if second_handle.is_null() {
            second_handle = create(api, device, commands[1], second_params, width, height)?;
            if second_handle == first_handle {
                return Err("NR feature handles are not independent".into());
            }
            sdk_nr::event(
                "two_pass_instances",
                json!({"first_handle":first_handle as usize,"second_handle":second_handle as usize,
                "first_parameters":first_params as usize,"second_parameters":second_params as usize}),
            )?;
        }
        let tuning_frame = options.frames * 3 / 4;
        let independent = frame >= tuning_frame;
        let second_options = crate::advanced_settings::NrOptions {
            style: if independent {
                crate::advanced_settings::NrStyle::B
            } else {
                crate::advanced_settings::NrStyle::A
            },
            ..Default::default()
        };
        let second_reset = second_fresh || frame == tuning_frame;
        evaluate(
            api,
            commands[1],
            second_params,
            second_handle,
            &mut second,
            second_reset,
            if independent { 0.5 } else { 1.0 },
            second_options,
        )?;
        if let Some(old_second) = old_second {
            // Fault injection happens after valid NGX recording and before
            // submission. No invalid SDK parameters or invalid GPU work.
            device.reset_command_buffer(
                commands[1],
                vk::CommandBufferResetFlags::RELEASE_RESOURCES,
            )?;
            release(api, second_handle)?;
            second_handle = std::ptr::null_mut();
            second_fresh = true;
            discarded += 1;
            recreated += 1;
            if read(device, queue, commands[0], fence, r[1], readback)? != first_output
                || read(device, queue, commands[0], fence, r[4], readback)? != old_second
            {
                return Err("discarded NGX recording modified GPU colors".into());
            }
            sdk_nr::event(
                "two_pass_discard",
                json!({"frame":frame,"first_pass_fenced":true,"second_submitted":false,"both_colors_unchanged":true}),
            )?;
            continue;
        }
        submit(device, queue, commands[1], fence)?;
        secondary_initialized = true;
        second_fresh = false;
        second_evaluations += 1;
        let output = read(device, queue, commands[0], fence, r[4], readback)?;
        if output
            .chunks_exact(4)
            .any(|p| p == [half(-16.0), half(0.0), half(-16.0), half(1.0)])
        {
            return Err("second NR retained sentinel".into());
        }
        output_changed |= output != first_output;
        let hash = format!(
            "{:x}",
            Sha256::digest(std::slice::from_raw_parts(
                output.as_ptr().cast::<u8>(),
                bytes as usize
            ))
        );
        if output_hashes
            .last()
            .is_some_and(|previous| previous == &hash)
        {
            return Err("moving second NR output stopped updating".into());
        }
        output_hashes.push(hash);
        if frame < 3 || frame == tuning_frame || frame + 1 == options.frames {
            sdk_nr::event(
                "two_pass_output",
                json!({"frame":frame,"first_reset":frame==0,"second_reset":second_reset,"second_independent_tuning":independent,"changed_from_first":output!=first_output,"finite":true}),
            )?;
        }
    }
    release(api, second_handle)?;
    release(api, first_handle)?;
    sdk_nr::ngx_result(
        "two_pass_destroy_parameters",
        nr_api::NVSDK_NGX_VULKAN_DestroyParameters(second_params),
    )?;
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
    let summary = json!({"two_pass":true,"independent_maps":true,"independent_handles":true,"first_evaluations":options.frames,
        "second_submitted_evaluations":second_evaluations,"discarded_recordings":discarded,"second_recreations":recreated,"output_changed_from_first":output_changed,
        "independent_tuning_changed_at":options.frames*3/4,"first_reset_frames":[0],"second_reset_frames":[0,options.frames/2+1,options.frames*3/4],
        "moving_output_verified":true,"readback_complete":true,"synthetic_inputs":true,"game_integration_verified":false,"output_hashes":output_hashes});
    sdk_nr::write_json(&options.session.join("nr-output-summary.json"), &summary)?;
    if !output_changed {
        return Err("second NR did not change first output".into());
    }
    Ok(summary)
}
