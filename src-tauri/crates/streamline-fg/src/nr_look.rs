//! SDR Look dispatch and optional delta history; completion is owned by Nr.
use crate::{
    advanced_settings::LookOptions,
    fg_api::{self, Resource, Result},
};
use ash::vk::{self, Handle};
#[path = "nr_look_temporal.rs"]
mod temporal;

pub(super) struct Look {
    device: ash::Device,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    band_set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    input: Resource,
    output: Resource,
    memory: vk::PhysicalDeviceMemoryProperties,
    spatial: Option<Spatial>,
    spatial_failure: Option<String>,
    temporal: Option<temporal::Temporal>,
    temporal_failure: Option<String>,
    temporal_recorded: bool,
}
impl Look {
    pub(super) unsafe fn new(
        d: &ash::Device,
        memory: vk::PhysicalDeviceMemoryProperties,
        input: Resource,
        output: Resource,
    ) -> Result<Self> {
        if input.image == output.image
            || input.width != output.width
            || input.height != output.height
            || [input.format, output.format] != [vk::Format::R16G16B16A16_SFLOAT.as_raw() as u32; 2]
        {
            return Err("Look requires distinct, equally sized gamma RGBA16F resources".into());
        }
        let mut look = Self {
            device: d.clone(),
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            band_set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            input,
            output,
            memory,
            spatial: None,
            spatial_failure: None,
            temporal: None,
            temporal_failure: None,
            temporal_recorded: false,
        };
        let bindings = [0, 1, 2, 3].map(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::COMPUTE)
        });
        look.layout = d.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        look.pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(2)
                .pool_sizes(&[vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_IMAGE,
                    descriptor_count: 8,
                }]),
            None,
        )?;
        let sets = d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(look.pool)
                .set_layouts(&[look.layout; 2]),
        )?;
        look.set = sets[0];
        look.band_set = sets[1];
        // The unused band slot has a valid initialized input descriptor until
        // spatial processing is prepared, preserving the allocation-free path.
        let images = [input, output, input, input].map(|r| {
            [vk::DescriptorImageInfo::default()
                .image_view(vk::ImageView::from_raw(r.view))
                .image_layout(vk::ImageLayout::GENERAL)]
        });
        for (binding, image) in images.iter().enumerate() {
            d.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(look.set)
                    .dst_binding(binding as u32)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(image)],
                &[],
            );
        }
        look.pipeline_layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[look.layout])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(64)]),
            None,
        )?;
        look.pipeline = pipeline(
            d,
            look.pipeline_layout,
            include_bytes!("../shaders/nr_look.spv"),
        )?;
        Ok(look)
    }
    pub(super) unsafe fn prepare_temporal(
        &mut self,
        options: LookOptions,
        motion: Resource,
    ) -> Result<()> {
        if !options.temporal_active() || self.temporal.is_some() || self.temporal_failure.is_some()
        {
            return Ok(());
        }
        match temporal::Temporal::new(&self.device, &self.memory, self.input, self.output, motion) {
            Ok(t) => {
                self.temporal = Some(t);
                Ok(())
            }
            Err(error) => {
                self.temporal_failure = Some(error.to_string());
                Err(error)
            }
        }
    }
    pub(super) fn submitted(&mut self) {
        if let Some(t) = &mut self.temporal {
            t.submitted();
        }
    }
    pub(super) fn resource_status(&self) -> serde_json::Value {
        let temporal = self.temporal_status(LookOptions::default());
        serde_json::json!({"textureCount":usize::from(self.spatial.is_some())+temporal["textureCount"].as_u64().unwrap_or(0) as usize,
            "allocationBytes":self.spatial.as_ref().map_or(0,|s|s.bytes)+temporal["allocationBytes"].as_u64().unwrap_or(0)})
    }
    pub(super) fn temporal_status(&self, options: LookOptions) -> serde_json::Value {
        let mut status = self.temporal.as_ref().map_or_else(
            || serde_json::json!({"textureCount":0,"allocationBytes":0,"historyReady":false}),
            |t| t.status(),
        );
        status["requested"] = serde_json::json!(options.temporal_active());
        status["active"] = serde_json::json!(options.temporal_active() && self.temporal_recorded);
        status["reason"] = serde_json::json!(if !options.temporal_active() {
            "bypassed"
        } else if self.temporal_failure.is_some() {
            "preparation_failed"
        } else if !self.temporal_recorded {
            "motion_or_context_unavailable"
        } else {
            "active"
        });
        status["error"] = serde_json::json!(self.temporal_failure);
        status
    }
    /// Called only after the owner's completion fence, before recording.
    pub(super) unsafe fn prepare_spatial(&mut self, options: LookOptions) -> Result<()> {
        if !options.spatial_active() || self.spatial.is_some() || self.spatial_failure.is_some() {
            return Ok(());
        }
        match Spatial::new(&self.device, &self.memory, self.input, self.pipeline_layout) {
            Ok(spatial) => {
                let images =
                    [self.input, self.output, spatial.image.unwrap(), self.input].map(|r| {
                        [vk::DescriptorImageInfo::default()
                            .image_view(vk::ImageView::from_raw(r.view))
                            .image_layout(vk::ImageLayout::GENERAL)]
                    });
                for (binding, image) in images.iter().enumerate() {
                    self.device.update_descriptor_sets(
                        &[vk::WriteDescriptorSet::default()
                            .dst_set(self.band_set)
                            .dst_binding(binding as u32)
                            .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                            .image_info(image)],
                        &[],
                    );
                }
                self.device.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(self.set)
                        .dst_binding(2)
                        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                        .image_info(&images[2])],
                    &[],
                );
                self.spatial = Some(spatial);
                Ok(())
            }
            Err(error) => {
                self.spatial_failure = Some(error.to_string());
                Err(error)
            }
        }
    }
    pub(super) fn spatial_status(&self, options: LookOptions) -> serde_json::Value {
        serde_json::json!({"requested":options.spatial_active(),"active":options.spatial_active() && self.spatial.is_some(),
            "reason":if !options.spatial_active(){"bypassed"}else if self.spatial_failure.is_some(){"preparation_failed"}else if self.spatial.is_none(){"waiting"}else{"active"},
            "error":self.spatial_failure,"radiusPixels":options.spatial.radius,"textureCount":usize::from(self.spatial.is_some()),
            "allocationBytes":self.spatial.as_ref().map_or(0, |s|s.bytes),"history":"none"})
    }
    pub(super) fn active(&self, mut options: LookOptions) -> bool {
        if self.spatial.is_none() {
            options.spatial.enabled = false;
        }
        if !self.temporal_recorded {
            options.temporal.enabled = false;
        }
        !options.bypass()
    }
    #[cfg(test)]
    pub(super) unsafe fn record(
        &mut self,
        cmd: vk::CommandBuffer,
        size: vk::Extent2D,
        options: LookOptions,
    ) {
        self.record_with_context(cmd, size, options, None);
    }
    pub(super) unsafe fn record_with_context(
        &mut self,
        cmd: vk::CommandBuffer,
        size: vk::Extent2D,
        mut options: LookOptions,
        context: Option<crate::nr_look_history::Context>,
    ) {
        self.temporal_recorded =
            options.temporal_active() && context.is_some() && self.temporal.is_some();
        if !self.temporal_recorded {
            options.temporal.enabled = false;
            if let Some(t) = &mut self.temporal {
                t.invalidate();
            }
        }
        if self.spatial.is_none() {
            options.spatial.enabled = false;
        }
        if options.bypass() {
            return;
        }
        let d = &self.device;
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
        if self.temporal_recorded {
            let delta =
                self.temporal
                    .as_mut()
                    .unwrap()
                    .record(cmd, self.input, options, context.unwrap());
            let image = [vk::DescriptorImageInfo::default()
                .image_view(vk::ImageView::from_raw(delta.view))
                .image_layout(vk::ImageLayout::GENERAL)];
            for set in [self.set, self.band_set] {
                d.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(set)
                        .dst_binding(3)
                        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                        .image_info(&image)],
                    &[],
                );
            }
        }
        d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        let mut constants = options.constants();
        constants[11] = if self.temporal_recorded { 1.0 } else { 0.0 };
        let bytes = std::slice::from_raw_parts(constants.as_ptr().cast::<u8>(), 64);
        d.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes,
        );
        if options.spatial_active() {
            let spatial = self.spatial.as_mut().unwrap();
            let image = vk::Image::from_raw(spatial.image.unwrap().image);
            fg_api::transition(
                d,
                cmd,
                image,
                if spatial.initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::GENERAL,
            );
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, spatial.pipeline);
            d.cmd_bind_descriptor_sets(
                cmd,
                vk::PipelineBindPoint::COMPUTE,
                self.pipeline_layout,
                0,
                &[self.band_set],
                &[],
            );
            d.cmd_dispatch(cmd, size.width.div_ceil(8), size.height.div_ceil(8), 1);
            // Finish all neighbour reads of NR and make band writes visible
            // before the compose dispatch mutates that same NR output.
            d.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::PipelineStageFlags::COMPUTE_SHADER,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)],
                &[],
                &[],
            );
            spatial.initialized = true;
            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        }
        d.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.set],
            &[],
        );
        d.cmd_dispatch(cmd, size.width.div_ceil(8), size.height.div_ceil(8), 1);
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)],
            &[],
            &[],
        );
    }
}
impl Drop for Look {
    fn drop(&mut self) {
        unsafe {
            drop(self.spatial.take());
            drop(self.temporal.take());
            self.device.destroy_pipeline(self.pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_descriptor_pool(self.pool, None);
            self.device.destroy_descriptor_set_layout(self.layout, None);
        }
    }
}

struct Spatial {
    device: ash::Device,
    image: Option<Resource>,
    pipeline: vk::Pipeline,
    initialized: bool,
    bytes: u64,
}
impl Spatial {
    unsafe fn new(
        d: &ash::Device,
        memory: &vk::PhysicalDeviceMemoryProperties,
        input: Resource,
        layout: vk::PipelineLayout,
    ) -> Result<Self> {
        let mut spatial = Self {
            device: d.clone(),
            image: None,
            pipeline: vk::Pipeline::null(),
            initialized: false,
            bytes: 0,
        };
        spatial.image = Some(fg_api::texture_with_usage(
            d,
            memory,
            vk::Extent2D {
                width: input.width,
                height: input.height,
            },
            vk::Format::R16G16B16A16_SFLOAT,
            vk::ImageUsageFlags::STORAGE,
        )?);
        spatial.bytes = d
            .get_image_memory_requirements(vk::Image::from_raw(spatial.image.unwrap().image))
            .size;
        spatial.pipeline = pipeline(d, layout, include_bytes!("../shaders/nr_look_band.spv"))?;
        Ok(spatial)
    }
}
impl Drop for Spatial {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            if let Some(r) = self.image.take() {
                self.device
                    .destroy_image_view(vk::ImageView::from_raw(r.view), None);
                self.device
                    .destroy_image(vk::Image::from_raw(r.image), None);
                self.device
                    .free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
        }
    }
}
unsafe fn pipeline(
    d: &ash::Device,
    layout: vk::PipelineLayout,
    bytes: &[u8],
) -> Result<vk::Pipeline> {
    let code = ash::util::read_spv(&mut std::io::Cursor::new(bytes))?;
    let shader =
        d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
    let result = d.create_compute_pipelines(
        vk::PipelineCache::null(),
        &[vk::ComputePipelineCreateInfo::default()
            .layout(layout)
            .stage(
                vk::PipelineShaderStageCreateInfo::default()
                    .stage(vk::ShaderStageFlags::COMPUTE)
                    .module(shader)
                    .name(c"main"),
            )],
        None,
    );
    d.destroy_shader_module(shader, None);
    Ok(result.map_err(|(partial, error)| {
        for p in partial {
            d.destroy_pipeline(p, None);
        }
        error
    })?[0])
}

#[cfg(test)]
#[path = "nr_look_tests.rs"]
mod tests;
