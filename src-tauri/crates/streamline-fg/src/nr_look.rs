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
    guide: Resource,
    output: Resource,
    memory: vk::PhysicalDeviceMemoryProperties,
    spatial: Option<Spatial>,
    spatial_failure: Option<String>,
    temporal: Option<temporal::Temporal>,
    temporal_failure: Option<String>,
    temporal_recorded: bool,
    temporal_mode: Option<crate::advanced_settings::TemporalMode>,
    raw_output: Option<Resource>,
    raw_initialized: bool,
    raw_bytes: u64,
}
impl Look {
    #[cfg(test)]
    pub(super) unsafe fn new(
        d: &ash::Device,
        memory: vk::PhysicalDeviceMemoryProperties,
        input: Resource,
        output: Resource,
    ) -> Result<Self> {
        Self::with_guide(d, memory, input, output, input)
    }
    pub(super) unsafe fn with_guide(
        d: &ash::Device,
        memory: vk::PhysicalDeviceMemoryProperties,
        input: Resource,
        output: Resource,
        guide: Resource,
    ) -> Result<Self> {
        if input.image == output.image
            || guide.image == output.image
            || guide.width != input.width
            || guide.height != input.height
            || guide.format != input.format
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
            guide,
            output,
            memory,
            spatial: None,
            spatial_failure: None,
            temporal: None,
            temporal_failure: None,
            temporal_recorded: false,
            temporal_mode: None,
            raw_output: None,
            raw_initialized: false,
            raw_bytes: 0,
        };
        let bindings = [0, 1, 2, 3, 4, 5, 6].map(|binding| {
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
                    descriptor_count: 14,
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
                    .size(96)]),
            None,
        )?;
        look.pipeline = pipeline(
            d,
            look.pipeline_layout,
            include_bytes!("../shaders/nr_look.spv"),
        )?;
        look.raw_output = Some(fg_api::texture_with_usage(
            d,
            &memory,
            vk::Extent2D {
                width: output.width,
                height: output.height,
            },
            vk::Format::R16G16B16A16_SFLOAT,
            vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
        )?);
        look.raw_bytes = d
            .get_image_memory_requirements(vk::Image::from_raw(look.raw_output.unwrap().image))
            .size;
        for (binding, r) in [
            (4, guide),
            (5, look.raw_output.unwrap()),
            (6, look.raw_output.unwrap()),
        ] {
            let image = [vk::DescriptorImageInfo::default()
                .image_view(vk::ImageView::from_raw(r.view))
                .image_layout(vk::ImageLayout::GENERAL)];
            d.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(look.set)
                    .dst_binding(binding)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&image)],
                &[],
            );
        }
        Ok(look)
    }
    pub(super) unsafe fn set_first_output(&mut self, first: Resource) {
        let image = [vk::DescriptorImageInfo::default()
            .image_view(vk::ImageView::from_raw(first.view))
            .image_layout(vk::ImageLayout::GENERAL)];
        self.device.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(self.set)
                .dst_binding(5)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&image)],
            &[],
        );
    }
    unsafe fn copy_raw(&mut self, cmd: vk::CommandBuffer, restore: bool) {
        let raw = self.raw_output.unwrap();
        let (source, destination) = if restore {
            (raw, self.output)
        } else {
            (self.output, raw)
        };
        fg_api::transition(
            &self.device,
            cmd,
            vk::Image::from_raw(source.image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        fg_api::transition(
            &self.device,
            cmd,
            vk::Image::from_raw(destination.image),
            if !restore && !self.raw_initialized {
                vk::ImageLayout::UNDEFINED
            } else {
                vk::ImageLayout::GENERAL
            },
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let sub = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        self.device.cmd_copy_image(
            cmd,
            vk::Image::from_raw(source.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::Image::from_raw(destination.image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageCopy::default()
                .src_subresource(sub)
                .dst_subresource(sub)
                .extent(vk::Extent3D {
                    width: source.width,
                    height: source.height,
                    depth: 1,
                })],
        );
        fg_api::transition(
            &self.device,
            cmd,
            vk::Image::from_raw(source.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        fg_api::transition(
            &self.device,
            cmd,
            vk::Image::from_raw(destination.image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        self.raw_initialized = true;
    }
    /// Update only at the owner's fenced boundary. Scope never recreates NGX.
    pub(super) unsafe fn set_input(&mut self, input: Resource) {
        if self.input.image == input.image {
            return;
        }
        self.input = input;
        self.temporal_recorded = false;
        let image = [vk::DescriptorImageInfo::default()
            .image_view(vk::ImageView::from_raw(input.view))
            .image_layout(vk::ImageLayout::GENERAL)];
        for set in [self.set, self.band_set] {
            self.device.update_descriptor_sets(
                &[vk::WriteDescriptorSet::default()
                    .dst_set(set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&image)],
                &[],
            );
        }
        if let Some(t) = &mut self.temporal {
            t.set_input(input);
        }
    }
    pub(super) unsafe fn prepare_temporal(
        &mut self,
        options: LookOptions,
        motion: Resource,
    ) -> Result<()> {
        if self.temporal_mode != Some(options.temporal.mode) {
            self.temporal = None;
            self.temporal_recorded = false;
            self.temporal_failure = None;
            self.temporal_mode = Some(options.temporal.mode);
        }
        if !options.temporal_active() || self.temporal.is_some() || self.temporal_failure.is_some()
        {
            return Ok(());
        }
        match temporal::Temporal::new(
            &self.device,
            &self.memory,
            self.input,
            self.output,
            self.guide,
            motion,
            options.temporal.mode,
        ) {
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
        serde_json::json!({"textureCount":1+usize::from(self.spatial.is_some())+temporal["textureCount"].as_u64().unwrap_or(0) as usize,
            "allocationBytes":self.raw_bytes+self.spatial.as_ref().map_or(0,|s|s.bytes)+temporal["allocationBytes"].as_u64().unwrap_or(0),"rawModelCache":{"textureCount":1,"allocationBytes":self.raw_bytes,"ready":self.raw_initialized}})
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
        status["requestedMode"] = serde_json::json!(options.temporal.mode);
        status["actualMode"] = if options.temporal_active() && self.temporal_recorded {
            serde_json::json!(options.temporal.mode)
        } else {
            serde_json::Value::Null
        };
        status["guide"] = serde_json::json!("original_nr_input");
        status["sampling"] = serde_json::json!(options.temporal.sampling);
        status
    }
    /// Called only after the owner's completion fence, before recording.
    pub(super) unsafe fn prepare_spatial(&mut self, options: LookOptions) -> Result<()> {
        if !options.band_required() || self.spatial.is_some() || self.spatial_failure.is_some() {
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
            "error":self.spatial_failure,"diagnosticBand":options.diagnostic.needs_band() && self.spatial.is_some(),"radiusPixels":options.spatial.radius,"textureCount":usize::from(self.spatial.is_some()),
            "allocationBytes":self.spatial.as_ref().map_or(0, |s|s.bytes),"history":"none"})
    }
    pub(super) fn active(&self, mut options: LookOptions) -> bool {
        if self.spatial.is_none() {
            options.spatial.enabled = false;
            if options.diagnostic.needs_band() {
                options.diagnostic = Default::default();
            }
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
        options: LookOptions,
        context: Option<crate::nr_look_history::Context>,
    ) {
        self.copy_raw(cmd, false);
        self.record_impl(cmd, size, options, context, false);
    }
    pub(super) unsafe fn recompose(
        &mut self,
        cmd: vk::CommandBuffer,
        size: vk::Extent2D,
        options: LookOptions,
    ) {
        if !self.raw_initialized {
            self.copy_raw(cmd, false);
        }
        self.copy_raw(cmd, true);
        self.record_impl(cmd, size, options, None, true);
    }
    unsafe fn record_impl(
        &mut self,
        cmd: vk::CommandBuffer,
        size: vk::Extent2D,
        mut options: LookOptions,
        context: Option<crate::nr_look_history::Context>,
        recompose: bool,
    ) {
        let cached_delta = if recompose {
            self.temporal.as_ref().and_then(|t| t.latest(options))
        } else {
            None
        };
        if recompose && cached_delta.is_none() {
            if let Some(t) = &mut self.temporal {
                t.invalidate();
            }
        }
        self.temporal_recorded = options.temporal_active()
            && ((context.is_some() && self.temporal.is_some()) || cached_delta.is_some());
        if !self.temporal_recorded {
            options.temporal.enabled = false;
            if let Some(t) = self.temporal.as_mut().filter(|_| !recompose) {
                t.invalidate();
            }
        }
        if self.spatial.is_none() {
            options.spatial.enabled = false;
        }
        if self.spatial.is_none() && options.diagnostic.needs_band() {
            options.diagnostic = Default::default();
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
            let delta = if let Some(delta) = cached_delta {
                delta
            } else {
                self.temporal.as_mut().unwrap().record(
                    cmd,
                    self.input,
                    self.guide,
                    options,
                    context.unwrap(),
                )
            };
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
        constants[11] = if !self.temporal_recorded {
            0.0
        } else if options.temporal.mode.persistent() {
            2.0
        } else {
            1.0
        };
        let bytes = std::slice::from_raw_parts(constants.as_ptr().cast::<u8>(), 96);
        d.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            bytes,
        );
        if options.band_required() {
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
            if let Some(r) = self.raw_output.take() {
                self.device
                    .destroy_image_view(vk::ImageView::from_raw(r.view), None);
                self.device
                    .destroy_image(vk::Image::from_raw(r.image), None);
                self.device
                    .free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
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
