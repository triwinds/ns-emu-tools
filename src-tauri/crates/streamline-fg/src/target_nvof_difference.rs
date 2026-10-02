//! Exact color comparison before NVOF. Read back one word, never full frames.
use super::*;

pub(super) struct Difference {
    device: ash::Device,
    pub(super) flags: Buffer,
    mapped: usize,
    fence: vk::Fence,
    sampler: vk::Sampler,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sets: Vec<vk::DescriptorSet>,
    pipeline_layout: vk::PipelineLayout,
    shader: vk::ShaderModule,
    pipeline: vk::Pipeline,
}
impl Difference {
    pub(super) unsafe fn new(
        d: &ash::Device,
        memory: &vk::PhysicalDeviceMemoryProperties,
        colors: &[Image],
    ) -> Result<Self> {
        let extent = colors[0].extent;
        let bytes =
            4 + u64::from(extent.width.div_ceil(4)) * u64::from(extent.height.div_ceil(4)) * 4;
        let mut result = Self {
            device: d.clone(),
            flags: buffer(
                d,
                memory,
                bytes,
                vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
                true,
            )?,
            mapped: 0,
            fence: vk::Fence::null(),
            sampler: vk::Sampler::null(),
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            sets: vec![],
            pipeline_layout: vk::PipelineLayout::null(),
            shader: vk::ShaderModule::null(),
            pipeline: vk::Pipeline::null(),
        };
        result.mapped =
            d.map_memory(result.flags.memory, 0, bytes, vk::MemoryMapFlags::empty())? as usize;
        result.fence = d.create_fence(&vk::FenceCreateInfo::default(), None)?;
        result.sampler = d.create_sampler(
            &vk::SamplerCreateInfo::default()
                .min_filter(vk::Filter::NEAREST)
                .mag_filter(vk::Filter::NEAREST)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
            None,
        )?;
        let bindings = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_BUFFER,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, ty)| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(i as u32)
                .descriptor_type(ty)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        })
        .collect::<Vec<_>>();
        result.layout = d.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        let sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 4,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 2,
            },
        ];
        result.pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(2)
                .pool_sizes(&sizes),
            None,
        )?;
        result.sets = d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(result.pool)
                .set_layouts(&[result.layout; 2]),
        )?;
        for current in 0..2 {
            let images = [current, 1 - current].map(|i| {
                [vk::DescriptorImageInfo::default()
                    .sampler(result.sampler)
                    .image_view(colors[i].view)
                    .image_layout(vk::ImageLayout::GENERAL)]
            });
            let data = [vk::DescriptorBufferInfo::default()
                .buffer(result.flags.buffer)
                .range(bytes)];
            d.update_descriptor_sets(
                &[
                    vk::WriteDescriptorSet::default()
                        .dst_set(result.sets[current])
                        .dst_binding(0)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&images[0]),
                    vk::WriteDescriptorSet::default()
                        .dst_set(result.sets[current])
                        .dst_binding(1)
                        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                        .image_info(&images[1]),
                    vk::WriteDescriptorSet::default()
                        .dst_set(result.sets[current])
                        .dst_binding(2)
                        .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                        .buffer_info(&data),
                ],
                &[],
            );
        }
        result.pipeline_layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default().set_layouts(&[result.layout]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(
            "../shaders/frame_difference.spv"
        )))?;
        result.shader =
            d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        result.pipeline = d
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .layout(result.pipeline_layout)
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(result.shader)
                            .name(c"main"),
                    )],
                None,
            )
            .map_err(|(partial, error)| {
                for pipeline in partial {
                    d.destroy_pipeline(pipeline, None);
                }
                error
            })?[0];
        Ok(result)
    }
    pub(super) unsafe fn record(&self, cmd: vk::CommandBuffer, current: usize, grid: vk::Extent2D) {
        let d = &self.device;
        d.cmd_fill_buffer(cmd, self.flags.buffer, 0, 4, 0);
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
        d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        d.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.sets[current]],
            &[],
        );
        d.cmd_dispatch(cmd, grid.width.div_ceil(8), grid.height.div_ceil(8), 1);
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::HOST | vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ | vk::AccessFlags::SHADER_READ)],
            &[],
            &[],
        );
    }
    pub(super) unsafe fn submit(
        &self,
        queue: vk::Queue,
        cmd: vk::CommandBuffer,
        waits: &[vk::Semaphore],
        signal: vk::Semaphore,
    ) -> Result<bool> {
        let d = &self.device;
        d.reset_fences(&[self.fence])?;
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[cmd])
                .wait_semaphores(waits)
                .wait_dst_stage_mask(&stages)
                .signal_semaphores(&[signal])],
            self.fence,
        )?;
        d.wait_for_fences(&[self.fence], true, 5_000_000_000)?;
        Ok(std::ptr::read_volatile(self.mapped as *const u32) == 0)
    }
}
impl Drop for Difference {
    fn drop(&mut self) {
        unsafe {
            let d = &self.device;
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_shader_module(self.shader, None);
            d.destroy_pipeline_layout(self.pipeline_layout, None);
            d.destroy_descriptor_pool(self.pool, None);
            d.destroy_descriptor_set_layout(self.layout, None);
            d.destroy_sampler(self.sampler, None);
            d.destroy_fence(self.fence, None);
            if self.mapped != 0 {
                d.unmap_memory(self.flags.memory);
            }
        }
    }
}
