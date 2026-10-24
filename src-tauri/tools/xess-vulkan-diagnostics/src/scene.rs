use crate::{
    api::Size,
    gpu::{Gpu, Image},
    Result,
};
use ash::vk;

#[repr(C)]
pub struct Parameters {
    pub input: Size,
    pub output: Size,
    pub frame: u32,
    pub capture: u32,
    pub jitter: [f32; 2],
    pub high_motion: u32,
    pub flat_depth: u32,
    pub zero_motion: u32,
}
pub struct Scene {
    device: ash::Device,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    shader: vk::ShaderModule,
    pipeline: vk::Pipeline,
}
impl Scene {
    pub unsafe fn new(gpu: &Gpu, images: [&Image; 5]) -> Result<Self> {
        let mut s = Self {
            device: gpu.device.clone(),
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            shader: vk::ShaderModule::null(),
            pipeline: vk::Pipeline::null(),
        };
        let bindings: Vec<_> = (0..5)
            .map(|i| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(i)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect();
        s.layout = s.device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        s.pool = s.device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_IMAGE,
                    descriptor_count: 5,
                }]),
            None,
        )?;
        s.set = s.device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(s.pool)
                .set_layouts(&[s.layout]),
        )?[0];
        let infos: Vec<_> = images
            .iter()
            .map(|i| {
                [vk::DescriptorImageInfo::default()
                    .image_view(i.view)
                    .image_layout(vk::ImageLayout::GENERAL)]
            })
            .collect();
        let writes: Vec<_> = infos
            .iter()
            .enumerate()
            .map(|(i, info)| {
                vk::WriteDescriptorSet::default()
                    .dst_set(s.set)
                    .dst_binding(i as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(info)
            })
            .collect();
        s.device.update_descriptor_sets(&writes, &[]);
        s.pipeline_layout = s.device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[s.layout])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(std::mem::size_of::<Parameters>() as u32)]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/scene.spv"
        ))))?;
        s.shader = s
            .device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        s.pipeline = s
            .device
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .layout(s.pipeline_layout)
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(s.shader)
                            .name(c"main"),
                    )],
                None,
            )
            .map_err(|(pipelines, e)| {
                for p in pipelines {
                    s.device.destroy_pipeline(p, None);
                }
                e
            })?[0];
        Ok(s)
    }
    pub unsafe fn record(&self, gpu: &Gpu, p: &Parameters) {
        self.device
            .cmd_bind_pipeline(gpu.cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        self.device.cmd_bind_descriptor_sets(
            gpu.cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.set],
            &[],
        );
        let bytes = std::slice::from_raw_parts(
            (p as *const Parameters).cast::<u8>(),
            std::mem::size_of::<Parameters>(),
        );
        self.device.cmd_push_constants(
            gpu.cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes,
        );
        self.device
            .cmd_dispatch(gpu.cmd, p.output.x.div_ceil(8), p.output.y.div_ceil(8), 1);
    }
}
impl Drop for Scene {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device.destroy_shader_module(self.shader, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_descriptor_pool(self.pool, None);
            self.device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}
