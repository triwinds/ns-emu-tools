//! Adapt raw optical-flow pixels to the cropped SR input pixel coordinates.
use crate::fg_api::{Resource, Result};
use ash::vk::{self, Handle};

pub(super) struct Adapter {
    device: ash::Device,
    sampler: vk::Sampler,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    shader: vk::ShaderModule,
    pipeline: vk::Pipeline,
    extent: vk::Extent2D,
    output: Resource,
}
impl Adapter {
    /// Owner retains both GENERAL images and retires its fence before reuse/drop.
    pub(super) unsafe fn new(d: &ash::Device, output: Resource) -> Result<Self> {
        if output.image == 0
            || output.view == 0
            || output.width == 0
            || output.height == 0
            || output.format != vk::Format::R16G16_SFLOAT.as_raw() as u32
            || output.usage & vk::ImageUsageFlags::STORAGE.as_raw() == 0
        {
            return Err("SR pixel motion output contract mismatch".into());
        }
        let mut adapter = Self {
            device: d.clone(),
            output,
            sampler: vk::Sampler::null(),
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            shader: vk::ShaderModule::null(),
            pipeline: vk::Pipeline::null(),
            extent: vk::Extent2D {
                width: output.width,
                height: output.height,
            },
        };
        adapter.sampler = d.create_sampler(
            &vk::SamplerCreateInfo::default()
                .min_filter(vk::Filter::LINEAR)
                .mag_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE),
            None,
        )?;
        let bindings = [
            vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
            vk::DescriptorType::STORAGE_IMAGE,
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
        adapter.layout = d.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        adapter.pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                        descriptor_count: 1,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_IMAGE,
                        descriptor_count: 1,
                    },
                ]),
            None,
        )?;
        adapter.set = d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(adapter.pool)
                .set_layouts(&[adapter.layout]),
        )?[0];
        let result = [vk::DescriptorImageInfo::default()
            .image_view(vk::ImageView::from_raw(output.view))
            .image_layout(vk::ImageLayout::GENERAL)];
        d.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(adapter.set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&result)],
            &[],
        );
        adapter.pipeline_layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[adapter.layout])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(16)]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(
            "../shaders/sr_motion.spv"
        )))?;
        adapter.shader =
            d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        adapter.pipeline = d
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(adapter.shader)
                            .name(c"main"),
                    )
                    .layout(adapter.pipeline_layout)],
                None,
            )
            .map_err(|(partial, error)| {
                for pipeline in partial {
                    d.destroy_pipeline(pipeline, None);
                }
                error
            })?[0];
        Ok(adapter)
    }
    /// Owner retires its fence before updating the descriptor. Both images are
    /// GENERAL; caller supplies producer -> compute and compute -> SDK barriers.
    pub(super) unsafe fn record(
        &self,
        cmd: vk::CommandBuffer,
        input: Resource,
        region: [f32; 4],
    ) -> Result<()> {
        if input.image == 0
            || input.view == 0
            || input.image == self.output.image
            || input.format != vk::Format::R16G16_SFLOAT.as_raw() as u32
            || input.usage & vk::ImageUsageFlags::SAMPLED.as_raw() == 0
            || !region.iter().all(|v| v.is_finite())
            || region[0] < 0.0
            || region[1] < 0.0
            || region[2] < 1.0
            || region[3] < 1.0
            || region[0] + region[2] > input.width as f32
            || region[1] + region[3] > input.height as f32
        {
            return Err("SR pixel motion input contract mismatch".into());
        }
        let source = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(vk::ImageView::from_raw(input.view))
            .image_layout(vk::ImageLayout::GENERAL)];
        self.device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(self.set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&source)],
            &[],
        );
        self.device.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            std::slice::from_raw_parts(region.as_ptr().cast(), 16),
        );
        self.device
            .cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        self.device.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.set],
            &[],
        );
        self.device.cmd_dispatch(
            cmd,
            self.extent.width.div_ceil(8),
            self.extent.height.div_ceil(8),
            1,
        );
        Ok(())
    }
}
impl Drop for Adapter {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_shader_module(self.shader, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_descriptor_pool(self.pool, None);
            self.device.destroy_descriptor_set_layout(self.layout, None);
            self.device.destroy_sampler(self.sampler, None);
        }
    }
}
