//! Opt-in GPU block-matching prototype. Original app waits are consumed once;
//! a replacement semaphore carries the copy/compute dependency into SDK present.
use crate::{fg_api::*, target_runtime, trace};
use ash::vk::{self, Handle};
use serde_json::json;
use std::time::Instant;
pub(super) fn enabled() -> bool {
    std::env::var("NS_STREAMLINE_TARGET_MOTION").as_deref() == Ok("1")
}
pub(super) struct Flow {
    device: ash::Device,
    colors: Vec<Resource>,
    current: usize,
    initialized: bool,
    images: Vec<vk::Image>,
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    fence: vk::Fence,
    ready: vk::Semaphore,
    sampler: vk::Sampler,
    layout: vk::DescriptorSetLayout,
    descriptors: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    shader: vk::ShaderModule,
    stats: vk::Buffer,
    stats_memory: vk::DeviceMemory,
}
impl Flow {
    pub(super) unsafe fn new(
        device: &ash::Device,
        props: &vk::PhysicalDeviceMemoryProperties,
        swapchain: vk::SwapchainKHR,
        output: Resource,
    ) -> Result<Self> {
        let mut f = Self {
            device: device.clone(),
            colors: Vec::new(),
            current: 0,
            initialized: false,
            images: Vec::new(),
            pool: vk::CommandPool::null(),
            cmd: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            ready: vk::Semaphore::null(),
            sampler: vk::Sampler::null(),
            layout: vk::DescriptorSetLayout::null(),
            descriptors: vk::DescriptorPool::null(),
            sets: Vec::new(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            shader: vk::ShaderModule::null(),
            stats: vk::Buffer::null(),
            stats_memory: vk::DeviceMemory::null(),
        };
        if swapchain != vk::SwapchainKHR::null() {
            let get: vk::PFN_vkGetSwapchainImagesKHR = std::mem::transmute(
                target_runtime::device_proc(device.handle(), c"vkGetSwapchainImagesKHR")
                    .ok_or("missing SDK image query")?,
            );
            let mut n = 0;
            get(device.handle(), swapchain, &mut n, std::ptr::null_mut()).result()?;
            f.images.resize(n as usize, vk::Image::null());
            get(device.handle(), swapchain, &mut n, f.images.as_mut_ptr()).result()?;
            f.images.truncate(n as usize);
        }
        // Bounded resolution controls compute cost independently of display resolution.
        let height = output.height.min(180);
        let width = ((output.width as u64 * height as u64) / output.height as u64).max(8) as u32;
        for _ in 0..2 {
            f.colors.push(texture(
                device,
                props,
                vk::Extent2D { width, height },
                vk::Format::R8G8B8A8_UNORM,
            )?);
        }
        f.pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(0)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        f.cmd = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(f.pool)
                .command_buffer_count(1)
                .level(vk::CommandBufferLevel::PRIMARY),
        )?[0];
        f.fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        f.ready = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
        f.sampler = device.create_sampler(
            &vk::SamplerCreateInfo::default()
                .mag_filter(vk::Filter::NEAREST)
                .min_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
            None,
        )?;
        let bindings = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::STORAGE_BUFFER,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(i as u32)
                .descriptor_type(t)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        })
        .collect::<Vec<_>>();
        f.layout = device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 4,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_IMAGE,
                descriptor_count: 2,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 2,
            },
        ];
        f.descriptors = device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(2)
                .pool_sizes(&sizes),
            None,
        )?;
        f.sets = device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(f.descriptors)
                .set_layouts(&[f.layout, f.layout]),
        )?;
        f.stats = device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(16)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        )?;
        let req = device.get_buffer_memory_requirements(f.stats);
        f.stats_memory = device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(memory_type(
                    props,
                    req.memory_type_bits,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )?),
            None,
        )?;
        device.bind_buffer_memory(f.stats, f.stats_memory, 0)?;
        for i in 0..2 {
            let curr = [vk::DescriptorImageInfo::default()
                .sampler(f.sampler)
                .image_view(vk::ImageView::from_raw(f.colors[i].view))
                .image_layout(vk::ImageLayout::GENERAL)];
            let prev = [vk::DescriptorImageInfo::default()
                .sampler(f.sampler)
                .image_view(vk::ImageView::from_raw(f.colors[1 - i].view))
                .image_layout(vk::ImageLayout::GENERAL)];
            let out = [vk::DescriptorImageInfo::default()
                .image_view(vk::ImageView::from_raw(output.view))
                .image_layout(vk::ImageLayout::GENERAL)];
            let buf = [vk::DescriptorBufferInfo::default()
                .buffer(f.stats)
                .range(16)];
            device.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(f.sets[i])
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&curr),
                    vk::WriteDescriptorSet::default()
                        .dst_set(f.sets[i])
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&prev),
                    vk::WriteDescriptorSet::default()
                        .dst_set(f.sets[i])
                        .dst_binding(2)
                        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                        .image_info(&out),
                    vk::WriteDescriptorSet::default()
                        .dst_set(f.sets[i])
                        .dst_binding(3)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&buf),
                ],
                &[],
            );
        }
        f.pipeline_layout = device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[f.layout])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(4)]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(
            "../shaders/motion.spv"
        )))?;
        f.shader = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        let pipelines = device
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(f.shader)
                            .name(c"main"),
                    )
                    .layout(f.pipeline_layout)],
                None,
            )
            .map_err(|(partial, e)| {
                for p in partial {
                    device.destroy_pipeline(p, None);
                }
                e
            })?;
        f.pipeline = pipelines[0];
        Ok(f)
    }
    pub(super) unsafe fn run(
        &mut self,
        queue: vk::Queue,
        info: &vk::PresentInfoKHR,
        output: Resource,
        reset: bool,
        frame: u32,
    ) -> Result<(vk::Semaphore, bool)> {
        let started = Instant::now();
        let d = &self.device;
        if info.p_image_indices.is_null() {
            return Err("missing present index".into());
        }
        let source = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("invalid present image index")?;
        d.reset_fences(&[self.fence])?;
        d.reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty())?;
        d.begin_command_buffer(self.cmd, &vk::CommandBufferBeginInfo::default())?;
        if !self.initialized {
            for color in &self.colors {
                transition(
                    d,
                    self.cmd,
                    vk::Image::from_raw(color.image),
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::GENERAL,
                );
            }
        }
        transition(
            d,
            self.cmd,
            source,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let curr = self.colors[self.current];
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(curr.image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let layers = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        d.cmd_blit_image(
            self.cmd,
            source,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::Image::from_raw(curr.image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageBlit::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .src_offsets([
                    vk::Offset3D::default(),
                    vk::Offset3D {
                        x: output.width as i32,
                        y: output.height as i32,
                        z: 1,
                    },
                ])
                .dst_offsets([
                    vk::Offset3D::default(),
                    vk::Offset3D {
                        x: curr.width as i32,
                        y: curr.height as i32,
                        z: 1,
                    },
                ])],
            vk::Filter::LINEAR,
        );
        transition(
            d,
            self.cmd,
            source,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        transition(
            d,
            self.cmd,
            vk::Image::from_raw(curr.image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        d.cmd_fill_buffer(self.cmd, self.stats, 0, 16, 0);
        barrier(d, self.cmd);
        d.cmd_bind_pipeline(self.cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        d.cmd_bind_descriptor_sets(
            self.cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.sets[self.current]],
            &[],
        );
        let reset = u32::from(reset || !self.initialized);
        d.cmd_push_constants(
            self.cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &reset.to_ne_bytes(),
        );
        d.cmd_dispatch(
            self.cmd,
            output.width.div_ceil(64),
            output.height.div_ceil(64),
            1,
        );
        barrier(d, self.cmd);
        d.end_command_buffer(self.cmd)?;
        let waits = if info.wait_semaphore_count == 0 {
            &[][..]
        } else {
            if info.p_wait_semaphores.is_null() {
                return Err("missing present waits".into());
            }
            std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
        };
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[self.cmd])
                .wait_semaphores(waits)
                .wait_dst_stage_mask(&stages)
                .signal_semaphores(&[self.ready])],
            self.fence,
        )?;
        d.wait_for_fences(&[self.fence], true, 5_000_000_000)?;
        let ptr = d.map_memory(self.stats_memory, 0, 16, vk::MemoryMapFlags::empty())?;
        let stats = std::slice::from_raw_parts(ptr.cast::<u32>(), 4).to_vec();
        d.unmap_memory(self.stats_memory);
        trace::event(
            "target_motion",
            json!({"frame":frame,"reset":reset!=0,"blocks":stats[0],"moving_blocks":stats[1],"rejected_blocks":stats[2],"magnitude_sum":stats[3],"elapsed_us":started.elapsed().as_micros(),"width":curr.width,"height":curr.height,"output_format":output.format,"app_waits_consumed":waits.len(),"replacement_wait":self.ready.as_raw()}),
        );
        let scene_cut = stats[0] > 0 && u64::from(stats[2]) * 100 > u64::from(stats[0]) * 70;
        self.current = 1 - self.current;
        self.initialized = true;
        Ok((self.ready, scene_cut))
    }
}
unsafe fn barrier(d: &ash::Device, cmd: vk::CommandBuffer) {
    d.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::ALL_COMMANDS | vk::PipelineStageFlags::HOST,
        vk::DependencyFlags::empty(),
        &[vk::MemoryBarrier::default()
            .src_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
            .dst_access_mask(
                vk::AccessFlags::MEMORY_READ
                    | vk::AccessFlags::MEMORY_WRITE
                    | vk::AccessFlags::HOST_READ,
            )],
        &[],
        &[],
    );
}
impl Drop for Flow {
    fn drop(&mut self) {
        unsafe {
            let d = &self.device;
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_shader_module(self.shader, None);
            d.destroy_pipeline_layout(self.pipeline_layout, None);
            d.destroy_descriptor_pool(self.descriptors, None);
            d.destroy_descriptor_set_layout(self.layout, None);
            d.destroy_sampler(self.sampler, None);
            d.destroy_buffer(self.stats, None);
            d.free_memory(self.stats_memory, None);
            d.destroy_semaphore(self.ready, None);
            d.destroy_fence(self.fence, None);
            d.destroy_command_pool(self.pool, None);
            for r in &self.colors {
                d.destroy_image_view(vk::ImageView::from_raw(r.view), None);
                d.destroy_image(vk::Image::from_raw(r.image), None);
                d.free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    unsafe fn buffer(
        d: &ash::Device,
        p: &vk::PhysicalDeviceMemoryProperties,
        size: u64,
    ) -> (vk::Buffer, vk::DeviceMemory) {
        let b = d
            .create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(size)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST),
                None,
            )
            .unwrap();
        let req = d.get_buffer_memory_requirements(b);
        let m = d
            .allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(
                        memory_type(
                            p,
                            req.memory_type_bits,
                            vk::MemoryPropertyFlags::HOST_VISIBLE
                                | vk::MemoryPropertyFlags::HOST_COHERENT,
                        )
                        .unwrap(),
                    ),
                None,
            )
            .unwrap();
        d.bind_buffer_memory(b, m, 0).unwrap();
        (b, m)
    }
    unsafe fn case(
        d: &ash::Device,
        p: &vk::PhysicalDeviceMemoryProperties,
        q: vk::Queue,
        shift: i32,
        reset: bool,
    ) -> Vec<f32> {
        let out = texture_with_usage(
            d,
            p,
            vk::Extent2D {
                width: 128,
                height: 128,
            },
            vk::Format::R32G32_SFLOAT,
            vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_SRC,
        )
        .unwrap();
        let flow = Flow::new(d, p, vk::SwapchainKHR::null(), out).unwrap();
        let (upload, upload_mem) = buffer(d, p, 128 * 128 * 8);
        let (read, read_mem) = buffer(d, p, 128 * 128 * 8);
        let ptr = d
            .map_memory(upload_mem, 0, 128 * 128 * 8, vk::MemoryMapFlags::empty())
            .unwrap()
            .cast::<u8>();
        let bytes = std::slice::from_raw_parts_mut(ptr, 128 * 128 * 8);
        for frame in 0..2 {
            for y in 0..128i32 {
                for x in 0..128i32 {
                    let at = x - if frame == 0 { shift } else { 0 };
                    let value = (((at as u32).wrapping_mul(7919) ^ (y as u32).wrapping_mul(104729))
                        .wrapping_mul(2654435761)
                        >> 24) as u8;
                    let i = (frame * 128 * 128 + (y * 128 + x) as usize) * 4;
                    bytes[i..i + 4].copy_from_slice(&[value, value, value, 255]);
                }
            }
        }
        d.unmap_memory(upload_mem);
        d.begin_command_buffer(flow.cmd, &vk::CommandBufferBeginInfo::default())
            .unwrap();
        for (i, color) in flow.colors.iter().enumerate() {
            transition(
                d,
                flow.cmd,
                vk::Image::from_raw(color.image),
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            d.cmd_copy_buffer_to_image(
                flow.cmd,
                upload,
                vk::Image::from_raw(color.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::BufferImageCopy::default()
                    .buffer_offset((i * 128 * 128 * 4) as u64)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: 128,
                        height: 128,
                        depth: 1,
                    })],
            );
            transition(
                d,
                flow.cmd,
                vk::Image::from_raw(color.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        transition(
            d,
            flow.cmd,
            vk::Image::from_raw(out.image),
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::GENERAL,
        );
        d.cmd_fill_buffer(flow.cmd, flow.stats, 0, 16, 0);
        barrier(d, flow.cmd);
        d.cmd_bind_pipeline(flow.cmd, vk::PipelineBindPoint::COMPUTE, flow.pipeline);
        d.cmd_bind_descriptor_sets(
            flow.cmd,
            vk::PipelineBindPoint::COMPUTE,
            flow.pipeline_layout,
            0,
            &[flow.sets[0]],
            &[],
        );
        d.cmd_push_constants(
            flow.cmd,
            flow.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &u32::from(reset).to_ne_bytes(),
        );
        d.cmd_dispatch(flow.cmd, 2, 2, 1);
        transition(
            d,
            flow.cmd,
            vk::Image::from_raw(out.image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        d.cmd_copy_image_to_buffer(
            flow.cmd,
            vk::Image::from_raw(out.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            read,
            &[vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width: 128,
                    height: 128,
                    depth: 1,
                })],
        );
        barrier(d, flow.cmd);
        d.end_command_buffer(flow.cmd).unwrap();
        d.queue_submit(
            q,
            &[vk::SubmitInfo::default().command_buffers(&[flow.cmd])],
            flow.fence,
        )
        .unwrap();
        d.wait_for_fences(&[flow.fence], true, 5_000_000_000)
            .unwrap();
        let ptr = d
            .map_memory(read_mem, 0, 128 * 128 * 8, vk::MemoryMapFlags::empty())
            .unwrap();
        let data = std::slice::from_raw_parts(ptr.cast::<f32>(), 128 * 128 * 2).to_vec();
        d.unmap_memory(read_mem);
        drop(flow);
        d.destroy_buffer(upload, None);
        d.free_memory(upload_mem, None);
        d.destroy_buffer(read, None);
        d.free_memory(read_mem, None);
        d.destroy_image_view(vk::ImageView::from_raw(out.view), None);
        d.destroy_image(vk::Image::from_raw(out.image), None);
        d.free_memory(vk::DeviceMemory::from_raw(out.memory), None);
        data
    }
    #[test]
    #[ignore = "Runs the actual SPIR-V on a local Vulkan GPU"]
    fn gpu_translation_static_and_reset() {
        unsafe {
            let entry = ash::Entry::load().unwrap();
            let instance = entry
                .create_instance(
                    &vk::InstanceCreateInfo::default().application_info(
                        &vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_2),
                    ),
                    None,
                )
                .unwrap();
            let physical = instance
                .enumerate_physical_devices()
                .unwrap()
                .into_iter()
                .find(|p| {
                    instance.get_physical_device_properties(*p).device_type
                        == vk::PhysicalDeviceType::DISCRETE_GPU
                })
                .unwrap();
            assert!(
                instance.get_physical_device_queue_family_properties(physical)[0]
                    .queue_flags
                    .contains(vk::QueueFlags::COMPUTE)
            );
            let d = instance
                .create_device(
                    physical,
                    &vk::DeviceCreateInfo::default().queue_create_infos(&[
                        vk::DeviceQueueCreateInfo::default()
                            .queue_family_index(0)
                            .queue_priorities(&[1.0]),
                    ]),
                    None,
                )
                .unwrap();
            let p = instance.get_physical_device_memory_properties(physical);
            let q = d.get_device_queue(0, 0);
            let still = case(&d, &p, q, 0, false);
            assert!(still.iter().all(|v| *v == 0.0));
            let reset = case(&d, &p, q, 4, true);
            assert!(reset.iter().all(|v| *v == 0.0));
            let moved = case(&d, &p, q, 4, false);
            let mut correct = 0;
            let mut total = 0;
            for y in 16..112 {
                for x in 16..112 {
                    let i = (y * 128 + x) * 2;
                    total += 1;
                    if (moved[i] + 4.0 / 128.0).abs() < 0.0001 && moved[i + 1].abs() < 0.0001 {
                        correct += 1;
                    }
                }
            }
            println!("correct translated pixels: {correct}/{total}");
            d.device_wait_idle().unwrap();
            d.destroy_device(None);
            instance.destroy_instance(None);
            assert!(
                correct * 100 > total * 70,
                "motion direction/scale or matching failed"
            );
        }
    }
}
