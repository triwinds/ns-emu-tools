//! NVIDIA hardware optical flow through VK_NV_optical_flow.
//! Independent of the retired colour block-matching experiment. Native flow is
//! current -> previous in signed 10.5 pixel units; Streamline consumes UV units.
use crate::{fg_api::*, target_runtime, trace};
use ash::vk::{self, Handle};
use serde_json::json;
use std::{
    ffi::CStr,
    sync::atomic::{AtomicU32, Ordering},
    time::Instant,
};
static FAMILY: AtomicU32 = AtomicU32::new(u32::MAX);
pub(super) fn requested() -> bool {
    std::env::var("NS_STREAMLINE_TARGET_MOTION").as_deref() == Ok("1")
}
pub(super) fn enabled() -> bool {
    requested() && FAMILY.load(Ordering::Relaxed) != u32::MAX
}
pub(super) fn set_family(family: Option<u32>) {
    FAMILY.store(family.unwrap_or(u32::MAX), Ordering::Relaxed);
}
pub(super) unsafe fn capability(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    graphics: u32,
) -> Result<u32> {
    let extensions = instance.enumerate_device_extension_properties(physical)?;
    if !extensions
        .iter()
        .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == ash::nv::optical_flow::NAME)
    {
        return Err("VK_NV_optical_flow unavailable".into());
    }
    let mut feature = vk::PhysicalDeviceOpticalFlowFeaturesNV::default();
    instance.get_physical_device_features2(
        physical,
        &mut vk::PhysicalDeviceFeatures2::default().push_next(&mut feature),
    );
    let mut props = vk::PhysicalDeviceOpticalFlowPropertiesNV::default();
    instance.get_physical_device_properties2(
        physical,
        &mut vk::PhysicalDeviceProperties2::default().push_next(&mut props),
    );
    if feature.optical_flow == 0
        || !props
            .supported_output_grid_sizes
            .contains(vk::OpticalFlowGridSizeFlagsNV::TYPE_4X4)
    {
        return Err("NVOF requires a 4x4 grid".into());
    }
    instance
        .get_physical_device_queue_family_properties(physical)
        .iter()
        .enumerate()
        .find(|(i, q)| {
            *i as u32 != graphics
                && q.queue_count > 0
                && q.queue_flags.contains(vk::QueueFlags::OPTICAL_FLOW_NV)
        })
        .map(|(i, _)| i as u32)
        .ok_or_else(|| "dedicated NVOF queue unavailable".into())
}
struct Image {
    device: ash::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    extent: vk::Extent2D,
}
impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_image_view(self.view, None);
            self.device.destroy_image(self.image, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
struct Buffer {
    device: ash::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
}
impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
unsafe fn buffer(
    d: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
    usage: vk::BufferUsageFlags,
    host: bool,
) -> Result<Buffer> {
    let mut b = Buffer {
        device: d.clone(),
        buffer: vk::Buffer::null(),
        memory: vk::DeviceMemory::null(),
    };
    b.buffer = d.create_buffer(
        &vk::BufferCreateInfo::default().size(size).usage(usage),
        None,
    )?;
    let req = d.get_buffer_memory_requirements(b.buffer);
    let flags = if host {
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT
    } else {
        vk::MemoryPropertyFlags::DEVICE_LOCAL
    };
    b.memory = d.allocate_memory(
        &vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(memory_type(props, req.memory_type_bits, flags)?),
        None,
    )?;
    d.bind_buffer_memory(b.buffer, b.memory, 0)?;
    Ok(b)
}
unsafe fn image(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    d: &ash::Device,
    extent: vk::Extent2D,
    format: vk::Format,
    optical: vk::OpticalFlowUsageFlagsNV,
    usage: vk::ImageUsageFlags,
    families: &[u32],
) -> Result<Image> {
    let usage = usage | vk::ImageUsageFlags::SAMPLED;
    let mut of = vk::OpticalFlowImageFormatInfoNV::default().usage(optical);
    let query = vk::PhysicalDeviceImageFormatInfo2::default()
        .format(format)
        .ty(vk::ImageType::TYPE_2D)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .push_next(&mut of);
    let mut properties = vk::ImageFormatProperties2::default();
    instance.get_physical_device_image_format_properties2(physical, &query, &mut properties)?;
    let mut i = Image {
        device: d.clone(),
        image: vk::Image::null(),
        memory: vk::DeviceMemory::null(),
        view: vk::ImageView::null(),
        extent,
    };
    let info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(format)
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::CONCURRENT)
        .queue_family_indices(families)
        .push_next(&mut of);
    i.image = d.create_image(&info, None)?;
    let req = d.get_image_memory_requirements(i.image);
    let props = instance.get_physical_device_memory_properties(physical);
    i.memory = d.allocate_memory(
        &vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(memory_type(
                &props,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?),
        None,
    )?;
    d.bind_image_memory(i.image, i.memory, 0)?;
    i.view = d.create_image_view(
        &vk::ImageViewCreateInfo::default()
            .image(i.image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(range()),
        None,
    )?;
    Ok(i)
}
struct GpuTiming {
    pool: vk::QueryPool,
    bits: u32,
    period: f32,
}
impl GpuTiming {
    unsafe fn report(&self, d: &ash::Device, frame: u32) {
        let mut ticks = [0u64; 4];
        // No WAIT: query availability must never introduce another blocking read.
        if d.get_query_pool_results(self.pool, 0, &mut ticks, vk::QueryResultFlags::TYPE_64)
            .is_ok()
        {
            let mask = u64::MAX >> (64 - self.bits);
            let us = |a: usize, b: usize| {
                ((ticks[b].wrapping_sub(ticks[a])) & mask) as f64 * f64::from(self.period) / 1000.
            };
            trace::event!(
                "target_nvof_gpu",
                json!({"frame":frame,"copy_us":us(0,1),
                "flow_and_queue_gap_us":us(1,2),"map_copy_and_dense_us":us(2,3),"total_us":us(0,3)})
            );
        }
    }
}
pub(super) struct Flow {
    device: ash::Device,
    api: ash::nv::optical_flow::Device,
    session: vk::OpticalFlowSessionNV,
    colors: Vec<Image>,
    maps: Vec<Image>,
    data: Option<Buffer>,
    timing: Option<GpuTiming>,
    pools: Vec<vk::CommandPool>,
    commands: Vec<vk::CommandBuffer>,
    semaphores: Vec<vk::Semaphore>,
    fence: vk::Fence,
    layout: vk::DescriptorSetLayout,
    descriptors: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    shader: vk::ShaderModule,
    optical_queue: vk::Queue,
    images: Vec<vk::Image>,
    extent: vk::Extent2D,
    grid: vk::Extent2D,
    current: usize,
    initialized: bool,
    previous_frame: u32,
    last_frame: Option<Instant>,
}
impl Flow {
    pub(super) unsafe fn new(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        swapchain: vk::SwapchainKHR,
        output: Resource,
    ) -> Result<Self> {
        Self::create(
            instance,
            physical,
            device,
            0,
            FAMILY.load(Ordering::Relaxed),
            swapchain,
            output,
        )
    }
    unsafe fn create(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        graphics: u32,
        optical: u32,
        swapchain: vk::SwapchainKHR,
        output: Resource,
    ) -> Result<Self> {
        let api = ash::nv::optical_flow::Device::new(instance, device);
        let extent = vk::Extent2D {
            width: output.width,
            height: output.height,
        };
        let grid = vk::Extent2D {
            width: extent.width.div_ceil(4),
            height: extent.height.div_ceil(4),
        };
        let mut f = Self {
            device: device.clone(),
            api,
            session: vk::OpticalFlowSessionNV::null(),
            colors: vec![],
            maps: vec![],
            data: None,
            timing: None,
            pools: vec![],
            commands: vec![],
            semaphores: vec![],
            fence: vk::Fence::null(),
            layout: vk::DescriptorSetLayout::null(),
            descriptors: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            shader: vk::ShaderModule::null(),
            optical_queue: device.get_device_queue(optical, 0),
            images: vec![],
            extent,
            grid,
            current: 0,
            initialized: false,
            previous_frame: 0,
            last_frame: None,
        };
        let mut props = vk::PhysicalDeviceOpticalFlowPropertiesNV::default();
        instance.get_physical_device_properties2(
            physical,
            &mut vk::PhysicalDeviceProperties2::default().push_next(&mut props),
        );
        if extent.width < props.min_width
            || extent.height < props.min_height
            || extent.width > props.max_width
            || extent.height > props.max_height
        {
            return Err("NVOF input extent outside device limits".into());
        }
        (f.api.fp().create_optical_flow_session_nv)(
            device.handle(),
            &vk::OpticalFlowSessionCreateInfoNV::default()
                .width(extent.width)
                .height(extent.height)
                .image_format(vk::Format::B8G8R8A8_UNORM)
                .flow_vector_format(vk::Format::R16G16_S10_5_NV)
                .cost_format(vk::Format::UNDEFINED)
                .output_grid_size(vk::OpticalFlowGridSizeFlagsNV::TYPE_4X4)
                .performance_level(vk::OpticalFlowPerformanceLevelNV::MEDIUM)
                .flags(vk::OpticalFlowSessionCreateFlagsNV::empty()),
            std::ptr::null(),
            &mut f.session,
        )
        .result()?;
        if std::env::var("NS_STREAMLINE_NVOF_TIMING").as_deref() == Ok("1") {
            let bits = instance.get_physical_device_queue_family_properties(physical)
                [graphics as usize]
                .timestamp_valid_bits;
            if bits != 0 {
                let period = instance
                    .get_physical_device_properties(physical)
                    .limits
                    .timestamp_period;
                f.timing = Some(GpuTiming {
                    pool: device.create_query_pool(
                        &vk::QueryPoolCreateInfo::default()
                            .query_type(vk::QueryType::TIMESTAMP)
                            .query_count(4),
                        None,
                    )?,
                    bits,
                    period,
                });
            }
        }
        let families = [graphics, optical];
        for _ in 0..2 {
            f.colors.push(image(
                instance,
                physical,
                device,
                extent,
                vk::Format::B8G8R8A8_UNORM,
                vk::OpticalFlowUsageFlagsNV::INPUT,
                vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC,
                &families,
            )?);
        }
        f.maps.push(image(
            instance,
            physical,
            device,
            grid,
            vk::Format::R16G16_S10_5_NV,
            vk::OpticalFlowUsageFlagsNV::OUTPUT,
            vk::ImageUsageFlags::TRANSFER_SRC,
            &families,
        )?);
        let memory = instance.get_physical_device_memory_properties(physical);
        let count = u64::from(grid.width) * u64::from(grid.height);
        let bytes = count * 4;
        f.data = Some(buffer(
            device,
            &memory,
            bytes,
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_BUFFER,
            false,
        )?);
        for family in [graphics, optical, graphics] {
            let pool = device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )?;
            f.pools.push(pool);
            f.commands.push(
                device.allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )?[0],
            );
            f.semaphores
                .push(device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?);
        }
        f.fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        let bindings = [
            vk::DescriptorType::STORAGE_BUFFER,
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
        f.layout = device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )?;
        f.descriptors = device.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_BUFFER,
                        descriptor_count: 1,
                    },
                    vk::DescriptorPoolSize {
                        ty: vk::DescriptorType::STORAGE_IMAGE,
                        descriptor_count: 1,
                    },
                ]),
            None,
        )?;
        f.set = device.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(f.descriptors)
                .set_layouts(&[f.layout]),
        )?[0];
        let data = [vk::DescriptorBufferInfo::default()
            .buffer(f.data.as_ref().unwrap().buffer)
            .range(bytes)];
        let out = [vk::DescriptorImageInfo::default()
            .image_view(vk::ImageView::from_raw(output.view))
            .image_layout(vk::ImageLayout::GENERAL)];
        device.update_descriptor_sets(
            &[
                vk::WriteDescriptorSet::default()
                    .dst_set(f.set)
                    .dst_binding(0)
                    .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                    .buffer_info(&data),
                vk::WriteDescriptorSet::default()
                    .dst_set(f.set)
                    .dst_binding(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .image_info(&out),
            ],
            &[],
        );
        f.pipeline_layout = device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[f.layout])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(16)]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(
            "../shaders/nvof.spv"
        )))?;
        f.shader = device
            .create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        f.pipeline = device
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
            })?[0];
        if swapchain != vk::SwapchainKHR::null() {
            let get: vk::PFN_vkGetSwapchainImagesKHR = std::mem::transmute(
                target_runtime::device_proc(device.handle(), c"vkGetSwapchainImagesKHR")
                    .ok_or("missing swapchain images")?,
            );
            let mut count = 0;
            get(device.handle(), swapchain, &mut count, std::ptr::null_mut()).result()?;
            f.images.resize(count as usize, vk::Image::null());
            get(
                device.handle(),
                swapchain,
                &mut count,
                f.images.as_mut_ptr(),
            )
            .result()?;
            f.images.truncate(count as usize);
        }
        trace::event!(
            "target_nvof_ready",
            json!({"backend":"VK_NV_optical_flow","width":extent.width,"height":extent.height,"grid":4,"quality":"balanced","bidirectional":false,"cost":false}),
        );
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
        if info.p_image_indices.is_null()
            || (info.wait_semaphore_count > 0 && info.p_wait_semaphores.is_null())
        {
            return Err("invalid NVOF present inputs".into());
        }
        let source = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("invalid NVOF image index")?;
        let waits = if info.wait_semaphore_count == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
        };
        self.run_source(
            queue,
            source,
            vk::ImageLayout::PRESENT_SRC_KHR,
            waits,
            output,
            reset,
            frame,
        )
    }
    unsafe fn run_source(
        &mut self,
        queue: vk::Queue,
        source: vk::Image,
        source_layout: vk::ImageLayout,
        waits: &[vk::Semaphore],
        output: Resource,
        reset: bool,
        frame: u32,
    ) -> Result<(vk::Semaphore, bool)> {
        let started = Instant::now();
        let reset = reset
            || !self.initialized
            || self
                .last_frame
                .is_some_and(|t| t.elapsed().as_millis() >= 500);
        let d = &self.device;
        // This completion protects command-buffer and input resource reuse.
        // Optional GPU queries are read only after that existing dependency completes.
        if self.initialized {
            d.wait_for_fences(&[self.fence], true, 5_000_000_000)?;
            if let Some(timing) = &self.timing {
                timing.report(d, self.previous_frame);
            }
        }
        for &cmd in &self.commands {
            d.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
        }
        d.reset_fences(&[self.fence])?;
        let copy = self.commands[0];
        d.begin_command_buffer(copy, &vk::CommandBufferBeginInfo::default())?;
        if let Some(timing) = &self.timing {
            d.cmd_reset_query_pool(copy, timing.pool, 0, 4);
            d.cmd_write_timestamp(copy, vk::PipelineStageFlags::TOP_OF_PIPE, timing.pool, 0);
        }
        if !self.initialized {
            for i in self.colors.iter().chain(&self.maps) {
                transition(
                    d,
                    copy,
                    i.image,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::GENERAL,
                );
            }
        }
        transition(
            d,
            copy,
            source,
            source_layout,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let current = &self.colors[self.current];
        transition(
            d,
            copy,
            current.image,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let layers = vk::ImageSubresourceLayers::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .layer_count(1);
        let extent = vk::Extent3D {
            width: self.extent.width,
            height: self.extent.height,
            depth: 1,
        };
        d.cmd_copy_image(
            copy,
            source,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            current.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::ImageCopy::default()
                .src_subresource(layers)
                .dst_subresource(layers)
                .extent(extent)],
        );
        transition(
            d,
            copy,
            current.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        transition(
            d,
            copy,
            source,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            source_layout,
        );
        // On reset do not read uninitialised/obsolete history. Copy current into
        // reference as well, and invalidate both SDK and NVOF temporal history.
        if reset {
            let previous = &self.colors[1 - self.current];
            transition(
                d,
                copy,
                current.image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            transition(
                d,
                copy,
                previous.image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            d.cmd_copy_image(
                copy,
                current.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                previous.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageCopy::default()
                    .src_subresource(layers)
                    .dst_subresource(layers)
                    .extent(extent)],
            );
            transition(
                d,
                copy,
                current.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            transition(
                d,
                copy,
                previous.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        if let Some(timing) = &self.timing {
            d.cmd_write_timestamp(copy, vk::PipelineStageFlags::BOTTOM_OF_PIPE, timing.pool, 1);
        }
        d.end_command_buffer(copy)?;
        for (binding, view) in [
            (vk::OpticalFlowSessionBindingPointNV::INPUT, current.view),
            (
                vk::OpticalFlowSessionBindingPointNV::REFERENCE,
                self.colors[1 - self.current].view,
            ),
            (
                vk::OpticalFlowSessionBindingPointNV::FLOW_VECTOR,
                self.maps[0].view,
            ),
        ] {
            (self.api.fp().bind_optical_flow_session_image_nv)(
                d.handle(),
                self.session,
                binding,
                view,
                vk::ImageLayout::GENERAL,
            )
            .result()?;
        }
        let optical = self.commands[1];
        d.begin_command_buffer(optical, &vk::CommandBufferBeginInfo::default())?;
        (self.api.fp().cmd_optical_flow_execute_nv)(
            optical,
            self.session,
            &vk::OpticalFlowExecuteInfoNV::default().flags(if reset {
                vk::OpticalFlowExecuteFlagsNV::DISABLE_TEMPORAL_HINTS
            } else {
                vk::OpticalFlowExecuteFlagsNV::empty()
            }),
        );
        d.end_command_buffer(optical)?;
        let dense = self.commands[2];
        d.begin_command_buffer(dense, &vk::CommandBufferBeginInfo::default())?;
        if let Some(timing) = &self.timing {
            d.cmd_write_timestamp(dense, vk::PipelineStageFlags::TOP_OF_PIPE, timing.pool, 2);
        }
        for map in &self.maps {
            transition(
                d,
                dense,
                map.image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            d.cmd_copy_image_to_buffer(
                dense,
                map.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.data.as_ref().unwrap().buffer,
                &[vk::BufferImageCopy::default()
                    .buffer_offset(0)
                    .image_subresource(layers)
                    .image_extent(vk::Extent3D {
                        width: map.extent.width,
                        height: map.extent.height,
                        depth: 1,
                    })],
            );
            transition(
                d,
                dense,
                map.image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        d.cmd_pipeline_barrier(
            dense,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)],
            &[],
            &[],
        );
        d.cmd_bind_pipeline(dense, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        d.cmd_bind_descriptor_sets(
            dense,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.set],
            &[],
        );
        let mut constants = [0u8; 16];
        for (i, value) in [self.grid.width, self.grid.height, u32::from(reset), 0]
            .iter()
            .enumerate()
        {
            constants[i * 4..i * 4 + 4].copy_from_slice(&value.to_ne_bytes());
        }
        d.cmd_push_constants(
            dense,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &constants,
        );
        d.cmd_dispatch(
            dense,
            output.width.div_ceil(8),
            output.height.div_ceil(8),
            1,
        );
        d.cmd_pipeline_barrier(
            dense,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                .dst_access_mask(vk::AccessFlags::MEMORY_READ)],
            &[],
            &[],
        );
        if let Some(timing) = &self.timing {
            d.cmd_write_timestamp(
                dense,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                timing.pool,
                3,
            );
        }
        d.end_command_buffer(dense)?;
        // Prepare everything before consuming application waits. After submission
        // failures must propagate: the same binary waits cannot be reused.
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[copy])
                .wait_semaphores(waits)
                .wait_dst_stage_mask(&stages)
                .signal_semaphores(&[self.semaphores[0]])],
            vk::Fence::null(),
        )?;
        d.queue_submit(
            self.optical_queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[optical])
                .wait_semaphores(&[self.semaphores[0]])
                .wait_dst_stage_mask(&[vk::PipelineStageFlags::ALL_COMMANDS])
                .signal_semaphores(&[self.semaphores[1]])],
            vk::Fence::null(),
        )?;
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[dense])
                .wait_semaphores(&[self.semaphores[1]])
                .wait_dst_stage_mask(&[vk::PipelineStageFlags::ALL_COMMANDS])
                .signal_semaphores(&[self.semaphores[2]])],
            self.fence,
        )?;
        trace::event!(
            "target_nvof_frame",
            json!({"frame":frame,"reset":reset,"submit_us":started.elapsed().as_micros(),"grid":4,"confidence_masked":false,"diagnostic_statistics":false}),
        );
        self.current = 1 - self.current;
        self.initialized = true;
        self.previous_frame = frame;
        self.last_frame = Some(Instant::now());
        Ok((self.semaphores[2], reset))
    }
}
impl Drop for Flow {
    fn drop(&mut self) {
        unsafe {
            let d = &self.device;
            // Owner must have waited for SDK input completion before retiring this flow.
            d.destroy_pipeline(self.pipeline, None);
            d.destroy_shader_module(self.shader, None);
            d.destroy_pipeline_layout(self.pipeline_layout, None);
            d.destroy_descriptor_pool(self.descriptors, None);
            d.destroy_descriptor_set_layout(self.layout, None);
            if self.session != vk::OpticalFlowSessionNV::null() {
                (self.api.fp().destroy_optical_flow_session_nv)(
                    d.handle(),
                    self.session,
                    std::ptr::null(),
                );
            }
            for &s in &self.semaphores {
                d.destroy_semaphore(s, None);
            }
            if let Some(timing) = &self.timing {
                d.destroy_query_pool(timing.pool, None);
            }
            d.destroy_fence(self.fence, None);
            for &p in &self.pools {
                d.destroy_command_pool(p, None);
            }
        }
    }
}

#[cfg(test)]
#[path = "target_nvof_tests.rs"]
mod tests;
