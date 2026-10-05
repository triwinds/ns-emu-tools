//! GPU-only log-delta history, owned and retired by the final Look pair.
use super::*;
use crate::nr_look_history::{Clock, Context, Frame};
pub(super) struct Temporal {
    device: ash::Device,
    images: Vec<Resource>,
    layout: vk::DescriptorSetLayout,
    pool: vk::DescriptorPool,
    sets: [vk::DescriptorSet; 2],
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    index: usize,
    initialized: bool,
    clock: Clock,
    pending: Option<(Frame, usize)>,
    last: Option<Frame>,
    bytes: u64,
}
impl Temporal {
    pub(super) unsafe fn new(
        d: &ash::Device,
        memory: &vk::PhysicalDeviceMemoryProperties,
        input: Resource,
        output: Resource,
        motion: Resource,
    ) -> Result<Self> {
        let shader: &[u8] = match vk::Format::from_raw(motion.format as i32) {
            vk::Format::R16G16_SFLOAT => include_bytes!("../shaders/nr_look_temporal_fp16.spv"),
            vk::Format::R32G32_SFLOAT => include_bytes!("../shaders/nr_look_temporal.spv"),
            _ => return Err("temporal Look requires RG16F or RG32F UV motion".into()),
        };
        if motion.width != input.width || motion.height != input.height {
            return Err("temporal Look requires same-sized current-to-previous UV motion".into());
        }
        let mut t = Self {
            device: d.clone(),
            images: Vec::new(),
            layout: vk::DescriptorSetLayout::null(),
            pool: vk::DescriptorPool::null(),
            sets: [vk::DescriptorSet::null(); 2],
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            index: 0,
            initialized: false,
            clock: Clock::default(),
            pending: None,
            last: None,
            bytes: 0,
        };
        for _ in 0..3 {
            let image = fg_api::texture_with_usage(
                d,
                memory,
                vk::Extent2D {
                    width: input.width,
                    height: input.height,
                },
                vk::Format::R16G16B16A16_SFLOAT,
                vk::ImageUsageFlags::STORAGE
                    | vk::ImageUsageFlags::TRANSFER_DST
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )?;
            t.bytes += d
                .get_image_memory_requirements(vk::Image::from_raw(image.image))
                .size;
            t.images.push(image);
        }
        let bindings = (0..6)
            .map(|binding| {
                vk::DescriptorSetLayoutBinding::default()
                    .binding(binding)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
            })
            .collect::<Vec<_>>();
        t.layout = d.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        t.pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(2)
                .pool_sizes(&[vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_IMAGE,
                    descriptor_count: 12,
                }]),
            None,
        )?;
        let sets = d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(t.pool)
                .set_layouts(&[t.layout; 2]),
        )?;
        t.sets = [sets[0], sets[1]];
        for index in 0..2 {
            for (binding, r) in [
                input,
                output,
                t.images[index],
                t.images[1 - index],
                t.images[2],
                motion,
            ]
            .into_iter()
            .enumerate()
            {
                let image = [vk::DescriptorImageInfo::default()
                    .image_view(vk::ImageView::from_raw(r.view))
                    .image_layout(vk::ImageLayout::GENERAL)];
                d.update_descriptor_sets(
                    &[vk::WriteDescriptorSet::default()
                        .dst_set(t.sets[index])
                        .dst_binding(binding as u32)
                        .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                        .image_info(&image)],
                    &[],
                );
            }
        }
        t.pipeline_layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[t.layout])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(32)]),
            None,
        )?;
        t.pipeline = pipeline(d, t.pipeline_layout, shader)?;
        Ok(t)
    }
    pub(super) fn invalidate(&mut self) {
        self.clock.invalidate();
        self.pending = None;
        self.last = None;
    }
    pub(super) fn submitted(&mut self) {
        if let Some((frame, index)) = self.pending.take() {
            self.clock.submitted(frame);
            self.index = index;
            self.last = Some(frame);
        }
    }
    pub(super) fn status(&self) -> serde_json::Value {
        serde_json::json!({"textureCount":self.images.len(),"allocationBytes":self.bytes,"historyReady":self.last.is_some(),
            "sourceFrameId":self.last.map(|f|f.context.source_frame_id),"intervalMs":self.last.map(|f|f.interval_ms),
            "maximumHistoryWeight":self.last.map(|f|f.weight),"resetReason":self.last.map(|f|f.reason),
            "history":"raw_log_model_delta","reprojection":"current_to_previous_uv_bilinear","pixelHistoryAcceptanceMeasured":false})
    }
    pub(super) unsafe fn record(
        &mut self,
        cmd: vk::CommandBuffer,
        input: Resource,
        options: LookOptions,
        context: Context,
    ) -> Resource {
        let d = &self.device;
        if !self.initialized {
            for r in &self.images {
                fg_api::transition(
                    d,
                    cmd,
                    vk::Image::from_raw(r.image),
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
                d.cmd_clear_color_image(
                    cmd,
                    vk::Image::from_raw(r.image),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, -1.0],
                    },
                    &[fg_api::range()],
                );
                fg_api::transition(
                    d,
                    cmd,
                    vk::Image::from_raw(r.image),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
            }
            self.initialized = true;
        }
        let frame = self.clock.plan(options, context);
        let values = [
            frame.weight,
            f32::from(options.temporal.rejection) / 100.0,
            0.0,
            0.0,
            context.uv_scale[0],
            context.uv_scale[1],
            0.0,
            0.0,
        ];
        d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        d.cmd_push_constants(
            cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            std::slice::from_raw_parts(values.as_ptr().cast::<u8>(), 32),
        );
        d.cmd_bind_descriptor_sets(
            cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.sets[self.index]],
            &[],
        );
        d.cmd_dispatch(cmd, input.width.div_ceil(8), input.height.div_ceil(8), 1);
        d.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)],
            &[],
            &[],
        );
        // Save raw Look input before any final composition; never accumulate RGB output.
        fg_api::transition(
            d,
            cmd,
            vk::Image::from_raw(input.image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        fg_api::transition(
            d,
            cmd,
            vk::Image::from_raw(self.images[2].image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let sub = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        d.cmd_copy_image(
            cmd,
            vk::Image::from_raw(input.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::Image::from_raw(self.images[2].image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageCopy::default()
                .src_subresource(sub)
                .dst_subresource(sub)
                .extent(vk::Extent3D {
                    width: input.width,
                    height: input.height,
                    depth: 1,
                })],
        );
        fg_api::transition(
            d,
            cmd,
            vk::Image::from_raw(input.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        fg_api::transition(
            d,
            cmd,
            vk::Image::from_raw(self.images[2].image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        self.pending = Some((frame, 1 - self.index));
        self.images[1 - self.index]
    }
}
impl Drop for Temporal {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_pipeline(self.pipeline, None);
            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_descriptor_pool(self.pool, None);
            self.device.destroy_descriptor_set_layout(self.layout, None);
            for r in self.images.drain(..) {
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
