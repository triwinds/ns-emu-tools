//! Private SDR input preparation for pass two. Nr owns submission and retirement.
use crate::{
    advanced_settings::InterpassOptions,
    fg_api::{self, Resource, Result},
};
use ash::vk::{self, Handle};

pub(super) struct Interpass {
    device: ash::Device,
    pub output: Option<Resource>,
    pub bytes: u64,
    pub initialized: bool,
    pub capture: Option<crate::frame_capture::Capture>,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
}
impl Interpass {
    pub unsafe fn new(
        d: &ash::Device,
        memory: &vk::PhysicalDeviceMemoryProperties,
        original: Resource,
        first: Resource,
        second: Resource,
    ) -> Result<Self> {
        if original.image == first.image
            || original.width != first.width
            || original.height != first.height
            || [original.format, first.format]
                != [vk::Format::R16G16B16A16_SFLOAT.as_raw() as u32; 2]
        {
            return Err("interpass requires distinct equally sized SDR RGBA16F inputs".into());
        }
        let mut r = Self {
            device: d.clone(),
            output: None,
            bytes: 0,
            initialized: false,
            capture: None,
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
        };
        r.output = Some(fg_api::texture_with_usage(
            d,
            memory,
            vk::Extent2D {
                width: original.width,
                height: original.height,
            },
            vk::Format::R16G16B16A16_SFLOAT,
            vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
        )?);
        r.bytes = d
            .get_image_memory_requirements(vk::Image::from_raw(r.output.unwrap().image))
            .size;
        let bindings: Vec<_> = (0..3)
            .map(|i| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        r.layout = d.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        r.pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_IMAGE,
                    descriptor_count: 3,
                }]),
            None,
        )?;
        r.set = d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(r.pool)
                .set_layouts(&[r.layout]),
        )?[0];
        for (binding, image) in [(0, original), (1, first), (2, r.output.unwrap())] {
            d.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(r.set)
                    .dst_binding(binding)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&[vk::DescriptorImageInfo::default()
                        .image_view(vk::ImageView::from_raw(image.view))
                        .image_layout(vk::ImageLayout::GENERAL)])],
                &[],
            );
        }
        r.pipeline_layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[r.layout])
                .push_constant_ranges(&[vk::PushConstantRange {
                    stage_flags: vk::ShaderStageFlags::COMPUTE,
                    offset: 0,
                    size: 8,
                }]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(
            "../shaders/nr_interpass.spv"
        )))?;
        let shader =
            d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        let pipeline = d.create_compute_pipelines(
            vk::PipelineCache::null(),
            &[vk::ComputePipelineCreateInfo::default()
                .layout(r.pipeline_layout)
                .stage(
                    vk::PipelineShaderStageCreateInfo::default()
                        .stage(vk::ShaderStageFlags::COMPUTE)
                        .module(shader)
                        .name(c"main"),
                )],
            None,
        );
        d.destroy_shader_module(shader, None);
        r.pipeline = pipeline.map_err(|(partial, error)| {
            for p in partial {
                d.destroy_pipeline(p, None);
            }
            error
        })?[0];
        r.capture = crate::frame_capture::Capture::new(d, memory, &[r.output.unwrap(), second])?;
        Ok(r)
    }
    pub unsafe fn record(&self, cmd: vk::CommandBuffer, options: InterpassOptions) -> Resource {
        let d = &self.device;
        let output = self.output.unwrap();
        fg_api::transition(
            d,
            cmd,
            vk::Image::from_raw(output.image),
            if self.initialized {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::UNDEFINED
            },
            vk::ImageLayout::GENERAL,
        );
        // Also orders the consolidated first/second command buffers on one queue.
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
        let mut push = [0u8; 8];
        push[..4].copy_from_slice(&(f32::from(options.feedback) / 100.0).to_ne_bytes());
        push[4..].copy_from_slice(&u32::from(options.clamp).to_ne_bytes());
        d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        d.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.set],
            &[],
        );
        d.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &push,
        );
        d.cmd_dispatch(cmd, output.width.div_ceil(8), output.height.div_ceil(8), 1);
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)],
            &[],
            &[],
        );
        output
    }
}
impl Drop for Interpass {
    fn drop(&mut self) {
        unsafe {
            drop(self.capture.take());
            let d = &self.device;
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_pipeline_layout(self.pipeline_layout, None);
            d.destroy_descriptor_pool(self.pool, None);
            d.destroy_descriptor_set_layout(self.layout, None);
            if let Some(r) = self.output.take() {
                d.destroy_image_view(vk::ImageView::from_raw(r.view), None);
                d.destroy_image(vk::Image::from_raw(r.image), None);
                d.free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
        }
    }
}
