//! Compare the private gamma input, including native crop/flip/codec conversion.
//! Only a four-byte result crosses to the CPU, after the owner's input fence.
use crate::fg_api::{self, Resource, Result};
use ash::vk::{self, Handle};

pub(super) struct Difference {
    device: ash::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    mapped: usize,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
}
impl Difference {
    pub(super) unsafe fn new(
        d: &ash::Device,
        memory: &vk::PhysicalDeviceMemoryProperties,
        input: Resource,
        previous: Resource,
    ) -> Result<Self> {
        if input.image == previous.image
            || input.width != previous.width
            || input.height != previous.height
            || input.width == 0
            || input.height == 0
            || [input.format, previous.format]
                != [vk::Format::R16G16B16A16_SFLOAT.as_raw() as u32; 2]
        {
            return Err("NR comparison requires distinct, equal RGBA16F inputs".into());
        }
        let mut result = Self {
            device: d.clone(),
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
            mapped: 0,
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
        };
        result.buffer = d.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(4)
                .usage(vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::TRANSFER_DST),
            None,
        )?;
        let requirements = d.get_buffer_memory_requirements(result.buffer);
        let memory_type = fg_api::memory_type(
            memory,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;
        result.memory = d.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type),
            None,
        )?;
        d.bind_buffer_memory(result.buffer, result.memory, 0)?;
        result.mapped = d.map_memory(result.memory, 0, 4, vk::MemoryMapFlags::empty())? as usize;
        let bindings = [
            vk::DescriptorType::STORAGE_IMAGE,
            vk::DescriptorType::STORAGE_IMAGE,
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
        result.pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_IMAGE,
                        descriptor_count: 2,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: 1,
                    },
                ]),
            None,
        )?;
        result.set = d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(result.pool)
                .set_layouts(&[result.layout]),
        )?[0];
        for (i, r) in [input, previous].into_iter().enumerate() {
            d.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(result.set)
                    .dst_binding(i as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::default()
                        .image_view(vk::ImageView::from_raw(r.view))
                        .image_layout(vk::ImageLayout::GENERAL)])],
                &[],
            );
        }
        d.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(result.set)
                .dst_binding(2)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .buffer_info(&[vk::DescriptorBufferInfo::default()
                    .buffer(result.buffer)
                    .range(4)])],
            &[],
        );
        result.pipeline_layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default().set_layouts(&[result.layout]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(
            "../shaders/nr_input_difference.spv"
        )))?;
        let shader =
            d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        let pipeline = d.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[vk::ComputePipelineCreateInfo::default()
                .layout(result.pipeline_layout)
                .stage(
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::COMPUTE)
                        .module(shader)
                        .name(c"main"),
                )],
            None,
        );
        d.destroy_shader_module(shader, None);
        result.pipeline = pipeline.map_err(|(partial, error)| {
            for p in partial {
                d.destroy_pipeline(p, None);
            }
            error
        })?[0];
        Ok(result)
    }
    pub(super) unsafe fn record(&self, cmd: vk::CommandBuffer, extent: vk::Extent2D) {
        let d = &self.device;
        d.cmd_fill_buffer(cmd, self.buffer, 0, 4, 0);
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
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
            &[self.set],
            &[],
        );
        d.cmd_dispatch(cmd, extent.width.div_ceil(8), extent.height.div_ceil(8), 1);
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)],
            &[],
            &[],
        );
    }
    /// Caller must have waited for the fence covering record().
    pub(super) unsafe fn identical(&self) -> bool {
        std::ptr::read_volatile(self.mapped as *const u32) == 0
    }
}
impl Drop for Difference {
    fn drop(&mut self) {
        unsafe {
            let d = &self.device;
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline_layout(self.pipeline_layout, None);
            d.destroy_descriptor_pool(self.pool, None);
            d.destroy_descriptor_set_layout(self.layout, None);
            if self.mapped != 0 {
                d.unmap_memory(self.memory);
            }
            d.destroy_buffer(self.buffer, None);
            d.free_memory(self.memory, None);
        }
    }
}
