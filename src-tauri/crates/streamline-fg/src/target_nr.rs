//! Present-boundary native NR, private gamma resources and bounded completion.
use crate::{
    fg_api::{self, Resource, Result},
    nr_abi::ResourceVk,
    nr_runtime,
    source_auto::Source,
    trace,
};
use ash::vk::{self, Handle};
use serde_json::json;
use std::{
    hash::{Hash, Hasher},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};
static CAPTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Timing {
    pool: vk::QueryPool,
    bits: u32,
    period: f32,
}
impl Timing {
    unsafe fn read_passes(&self, device: &ash::Device) -> Option<[f64; 2]> {
        let mut ticks = [0u64; 4];
        device
            .get_query_pool_results(self.pool, 6, &mut ticks, vk::QueryResultFlags::TYPE_64)
            .ok()?;
        Some(std::array::from_fn(|i| {
            (ticks[i * 2 + 1].wrapping_sub(ticks[i * 2]) & (u64::MAX >> (64 - self.bits))) as f64
                * f64::from(self.period)
                / 1000.0
        }))
    }
    unsafe fn mark(&self, device: &ash::Device, command: vk::CommandBuffer, index: u32) {
        device.cmd_write_timestamp(
            command,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            self.pool,
            index,
        );
    }
    unsafe fn read(&self, device: &ash::Device) -> Option<[f64; 4]> {
        let mut ticks = [0u64; 6];
        // Read only after the existing completion fence; add no query WAIT.
        device
            .get_query_pool_results(self.pool, 0, &mut ticks, vk::QueryResultFlags::TYPE_64)
            .ok()?;
        Some(std::array::from_fn(|i| {
            let (a, b) = [(0, 1), (2, 3), (3, 4), (4, 5)][i];
            (ticks[b].wrapping_sub(ticks[a]) & (u64::MAX >> (64 - self.bits))) as f64
                * f64::from(self.period)
                / 1000.0
        }))
    }
}

pub(super) fn extent(window: vk::Extent2D, native: Option<Source>) -> vk::Extent2D {
    native.map_or(window, |s| vk::Extent2D {
        width: s.offsets[1].x.abs_diff(s.offsets[0].x),
        height: s.offsets[1].y.abs_diff(s.offsets[0].y),
    })
}
pub(super) fn identity(
    swapchain: vk::SwapchainKHR,
    window: vk::Extent2D,
    native: Option<Source>,
) -> crate::nr_history::Source {
    let size = extent(window, native);
    let mut mapping = std::collections::hash_map::DefaultHasher::new();
    [window.width, window.height].hash(&mut mapping);
    if let Some(n) = native {
        n.raw_copy.hash(&mut mapping);
        [n.extent.width, n.extent.height].hash(&mut mapping);
        for o in n.offsets {
            [o.x, o.y, o.z].hash(&mut mapping);
        }
        for v in n.viewport {
            v.to_bits().hash(&mut mapping);
        }
    }
    crate::nr_history::Source {
        identity: native.map_or(swapchain.as_raw(), |n| n.history_identity),
        extent: [size.width, size.height],
        mapping: mapping.finish(),
    }
}
fn offsets(size: vk::Extent2D) -> [vk::Offset3D; 2] {
    [
        vk::Offset3D::default(),
        vk::Offset3D {
            x: size.width as i32,
            y: size.height as i32,
            z: 1,
        },
    ]
}
fn viewport(window: vk::Extent2D, native: Option<Source>) -> Result<[f32; 4]> {
    let v = native.map_or([0.0, 0.0, window.width as f32, window.height as f32], |n| {
        n.viewport
    });
    if !v.iter().all(|v| v.is_finite())
        || v[0] < 0.0
        || v[1] < 0.0
        || v[2] < 1.0
        || v[3] < 1.0
        || v[0] + v[2] > window.width as f32 + 0.5
        || v[1] + v[3] > window.height as f32 + 0.5
    {
        return Err("NR viewport is outside the presented image".into());
    }
    Ok(v)
}
fn viewport_offsets(v: [f32; 4]) -> [vk::Offset3D; 2] {
    [
        vk::Offset3D {
            x: v[0].round() as i32,
            y: v[1].round() as i32,
            z: 0,
        },
        vk::Offset3D {
            x: (v[0] + v[2]).round() as i32,
            y: (v[1] + v[3]).round() as i32,
            z: 1,
        },
    ]
}
fn descriptor(r: Resource, write: bool) -> ResourceVk {
    ResourceVk {
        view: r.view,
        image: r.image,
        aspect: vk::ImageAspectFlags::COLOR.as_raw(),
        base_mip: 0,
        level_count: 1,
        base_layer: 0,
        layer_count: 1,
        format: r.format,
        width: r.width,
        height: r.height,
        resource_type: 0,
        read_write: u8::from(write),
        padding: [0; 3],
    }
}
pub(super) struct Nr {
    device: ash::Device,
    swapchain: vk::SwapchainKHR,
    window: vk::Extent2D,
    size: vk::Extent2D,
    raw_extent: Option<vk::Extent2D>,
    resources: Vec<Resource>,
    feature: Option<nr_runtime::Feature>,
    images: Vec<vk::Image>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    evaluate_command: vk::CommandBuffer,
    second_command: vk::CommandBuffer,
    input_fence: vk::Fence,
    fence: vk::Fence,
    ready: vk::Semaphore,
    pending: Option<(Resource, serde_json::Value)>,
    initialized: bool,
    in_flight: bool,
    readback: Option<(vk::Buffer, vk::DeviceMemory)>,
    captures: u32,
    frame_capture: Option<crate::frame_capture::Capture>,
    capture_controls: Option<(u32, bool, crate::advanced_settings::NrOptions)>,
    timing: Option<Timing>,
    look: Option<crate::nr_look::Look>,
    look_failure: Option<String>,
    look_srgb: bool,
    look_memory: vk::PhysicalDeviceMemoryProperties,
    previous_input: Option<Resource>,
    difference: Option<crate::nr_input_difference::Difference>,
    source_frames: crate::nr_source_frames::SourceFrames,
    second: Option<Second>,
    second_failure: Option<String>,
    second_retry: u16,
    last_passes: u8,
    last_output: Option<Resource>,
    last_pass_details: Vec<serde_json::Value>,
    look_origin: Instant,
}
struct Second {
    device: ash::Device,
    allocation_bytes: u64,
    output: Option<Resource>,
    feature: Option<nr_runtime::Feature>,
    history: crate::nr_pass_history::History,
    initialized: bool,
    look: Option<crate::nr_look::Look>,
    look_failure: Option<String>,
    capture: Option<crate::frame_capture::Capture>,
}
impl Second {
    unsafe fn new(
        device: &ash::Device,
        memory: &vk::PhysicalDeviceMemoryProperties,
        input: Resource,
    ) -> Result<Self> {
        let mut second = Self {
            device: device.clone(),
            allocation_bytes: 0,
            output: None,
            feature: None,
            history: Default::default(),
            initialized: false,
            look: None,
            look_failure: None,
            capture: None,
        };
        second.output = Some(fg_api::texture_with_usage(
            device,
            memory,
            vk::Extent2D {
                width: input.width,
                height: input.height,
            },
            vk::Format::R16G16B16A16_SFLOAT,
            vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::STORAGE
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST,
        )?);
        second.allocation_bytes = device
            .get_image_memory_requirements(vk::Image::from_raw(second.output.unwrap().image))
            .size;
        second.feature = Some(nr_runtime::Feature::allocate(2)?);
        second.capture =
            crate::frame_capture::Capture::new(device, memory, &[input, second.output.unwrap()])?;
        Ok(second)
    }
}
impl Drop for Second {
    fn drop(&mut self) {
        unsafe {
            // Owner retires submitted work or discards an unsubmitted recording.
            drop(self.feature.take());
            drop(self.look.take());
            drop(self.capture.take());
            if let Some(r) = self.output.take() {
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
fn look_status(
    look: Option<&crate::nr_look::Look>,
    failure: Option<&str>,
    options: crate::advanced_settings::LookOptions,
    passes: u8,
) -> serde_json::Value {
    let active = look.is_some_and(|look| look.active(options));
    let temporal = look.map(|look| look.temporal_status(options));
    let temporal_active = temporal
        .as_ref()
        .is_some_and(|status| status["active"] == true);
    json!({"requested":!options.bypass(),"active":active,
        "reason":if options.bypass() {"bypassed"} else if failure.is_some() {"preparation_failed"} else if !active {"look_stage_fallback"} else {"active"},
        "error":failure,"spatial":look.map(|look|look.spatial_status(options)),"temporal":temporal,"codec":"sdr_srgb_rgba16f","history":if temporal_active{"model_delta_only"}else{"none"},"passes":passes})
}
pub(super) struct Output {
    pub(super) resource: Resource,
    pub(super) source_frame: crate::nr_source_frames::Frame,
    pub(super) applied_revision: u64,
}
impl Nr {
    pub(super) fn look_status(
        &self,
        options: crate::advanced_settings::LookOptions,
    ) -> serde_json::Value {
        if self.last_passes == 2 {
            if let Some(second) = &self.second {
                return look_status(
                    second.look.as_ref(),
                    second.look_failure.as_deref(),
                    options,
                    2,
                );
            }
        }
        look_status(self.look.as_ref(), self.look_failure.as_deref(), options, 1)
    }
    pub(super) fn pipeline_status(
        &self,
        options: crate::advanced_settings::NrOptions,
        evaluated: bool,
    ) -> serde_json::Value {
        let mut passes = self.last_pass_details.clone();
        if !evaluated {
            for pass in &mut passes {
                pass["evaluated"] = json!(false);
                pass["reset"] = json!(false);
                pass["outputReused"] = json!(true);
            }
        }
        json!({"requestedPasses":if options.second_pass.enabled {2} else {1},"actualPasses":self.last_passes,
            "secondActive":self.last_passes == 2,"secondEvaluated":evaluated && self.last_passes == 2,
            "reason":if !options.second_pass.enabled {"disabled"} else if self.second_failure.is_some() {"first_pass_fallback"} else if self.last_passes == 2 {"active"} else {"waiting"},
            "error":self.second_failure,"retry":self.second_retry,"retryPolicy":"explicit_retry_toggle_or_recreation",
            "secondTextureCount":usize::from(self.second.is_some()),
            "secondAllocationBytes":self.second.as_ref().map_or(0, |s| s.allocation_bytes),
            "activeFeatureCount":usize::from(self.feature.as_ref().is_some_and(|f| f.identity().1 != 0))
                + usize::from(self.second.as_ref().and_then(|s| s.feature.as_ref()).is_some_and(|f| f.identity().1 != 0)),
            "privateTextureCount":self.resources.len()+usize::from(self.previous_input.is_some())+usize::from(self.second.is_some()),
            "lookCacheResources":{"first":self.look.as_ref().map(|look|look.resource_status()),"second":self.second.as_ref().and_then(|s|s.look.as_ref()).map(|look|look.resource_status())},"sdkMemoryBytes":serde_json::Value::Null,"passes":passes})
    }
    pub(super) fn matches(&self, native: Option<Source>) -> bool {
        self.size == extent(self.window, native)
            && self.raw_extent == native.filter(|n| n.raw_copy).map(|n| n.extent)
    }
    pub(super) fn retire_second(
        &mut self,
        options: crate::advanced_settings::NrOptions,
    ) -> Result<()> {
        if !options.second_pass.enabled || self.second_retry != options.second_pass.retry {
            if self.in_flight || self.pending.is_some() {
                return Err("NR secondary retirement requires completed work".into());
            }
            drop(self.second.take());
            self.second_failure = None;
        }
        self.second_retry = options.second_pass.retry;
        Ok(())
    }
    pub(super) unsafe fn new(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        swapchain: vk::SwapchainKHR,
        window: vk::Extent2D,
        native: Option<Source>,
        look_srgb: bool,
    ) -> Result<Self> {
        let size = extent(window, native);
        viewport(window, native)?;
        if [size.width, size.height]
            .iter()
            .any(|v| !(1..=8192).contains(v))
        {
            return Err("NR extent exceeds limit".into());
        }
        let mut nr = Self {
            device: device.clone(),
            swapchain,
            window,
            size,
            raw_extent: native.filter(|n| n.raw_copy).map(|n| n.extent),
            resources: Vec::new(),
            feature: None,
            images: Vec::new(),
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            evaluate_command: vk::CommandBuffer::null(),
            second_command: vk::CommandBuffer::null(),
            input_fence: vk::Fence::null(),
            fence: vk::Fence::null(),
            ready: vk::Semaphore::null(),
            pending: None,
            initialized: false,
            in_flight: false,
            readback: None,
            captures: 0,
            frame_capture: None,
            capture_controls: None,
            timing: None,
            look: None,
            look_failure: None,
            look_srgb,
            look_memory: instance.get_physical_device_memory_properties(physical),
            previous_input: None,
            difference: None,
            source_frames: Default::default(),
            second: None,
            second_failure: None,
            second_retry: 0,
            last_passes: 1,
            last_output: None,
            last_pass_details: Vec::new(),
            look_origin: Instant::now(),
        };
        if std::env::var("NS_STREAMLINE_NR_TIMING").as_deref() == Ok("1") {
            let bits = instance.get_physical_device_queue_family_properties(physical)[0]
                .timestamp_valid_bits;
            if bits != 0 {
                nr.timing = Some(Timing {
                    pool: device.create_query_pool(
                        &vk::QueryPoolCreateInfo::default()
                            .query_type(vk::QueryType::TIMESTAMP)
                            .query_count(10),
                        None,
                    )?,
                    bits,
                    period: instance
                        .get_physical_device_properties(physical)
                        .limits
                        .timestamp_period,
                });
            }
        }
        let usage = vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        let props = instance.get_physical_device_memory_properties(physical);
        for format in [
            vk::Format::R16G16B16A16_SFLOAT,
            vk::Format::R32_SFLOAT,
            vk::Format::R16G16_SFLOAT,
            vk::Format::R32G32_SFLOAT,
            vk::Format::B8G8R8A8_UNORM,
            vk::Format::R8G8B8A8_UNORM,
        ] {
            let available = instance
                .get_physical_device_format_properties(physical, format)
                .optimal_tiling_features;
            let required = vk::FormatFeatureFlags::SAMPLED_IMAGE
                | if matches!(
                    format,
                    vk::Format::R16G16B16A16_SFLOAT
                        | vk::Format::R32_SFLOAT
                        | vk::Format::R16G16_SFLOAT
                        | vk::Format::R8G8B8A8_UNORM
                ) {
                    vk::FormatFeatureFlags::STORAGE_IMAGE
                } else {
                    vk::FormatFeatureFlags::empty()
                }
                | vk::FormatFeatureFlags::TRANSFER_DST
                | vk::FormatFeatureFlags::TRANSFER_SRC
                | if format == vk::Format::R32_SFLOAT {
                    vk::FormatFeatureFlags::empty()
                } else {
                    vk::FormatFeatureFlags::BLIT_SRC
                        | vk::FormatFeatureFlags::BLIT_DST
                        | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                };
            if !available.contains(required) {
                return Err(format!("unsupported NR format {format:?}").into());
            }
        }
        for format in [
            vk::Format::R16G16B16A16_SFLOAT,
            vk::Format::R16G16B16A16_SFLOAT,
            vk::Format::R32_SFLOAT,
            vk::Format::R16G16_SFLOAT,
        ] {
            nr.resources.push(fg_api::texture_with_usage(
                device, &props, size, format, usage,
            )?);
        }
        if let Some(raw) = nr.raw_extent {
            nr.resources.push(fg_api::texture_with_usage(
                device,
                &props,
                raw,
                vk::Format::R8G8B8A8_UNORM,
                usage,
            )?);
        }
        nr.previous_input = Some(fg_api::texture_with_usage(
            device,
            &props,
            size,
            vk::Format::R16G16B16A16_SFLOAT,
            usage,
        )?);
        nr.difference = Some(crate::nr_input_difference::Difference::new(
            device,
            &props,
            nr.resources[0],
            nr.previous_input.unwrap(),
        )?);
        let get: vk::PFN_vkGetSwapchainImagesKHR = std::mem::transmute(
            crate::target_runtime::device_proc(device.handle(), c"vkGetSwapchainImagesKHR")
                .ok_or("NR swapchain query missing")?,
        );
        let mut count = 0;
        get(device.handle(), swapchain, &mut count, std::ptr::null_mut()).result()?;
        nr.images.resize(count as usize, vk::Image::null());
        get(
            device.handle(),
            swapchain,
            &mut count,
            nr.images.as_mut_ptr(),
        )
        .result()?;
        nr.images.truncate(count as usize);
        nr.pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(0)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        let commands = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(nr.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(3),
        )?;
        nr.command = commands[0];
        nr.evaluate_command = commands[1];
        nr.second_command = commands[2];
        nr.input_fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        nr.fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        nr.ready = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
        nr.feature = Some(nr_runtime::Feature::allocate(1)?);
        nr.frame_capture = crate::frame_capture::Capture::new(device, &props, &nr.resources[..2])?;
        if std::env::var("NS_STREAMLINE_NR_READBACK").as_deref() == Ok("1") {
            let bytes = u64::from(size.width) * u64::from(size.height) * 16;
            let buffer = device.create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(bytes)
                    .usage(vk::BufferUsageFlags::TRANSFER_DST),
                None,
            )?;
            let requirements = device.get_buffer_memory_requirements(buffer);
            let memory_type = match readback_memory_type(&props, requirements.memory_type_bits) {
                Ok(index) => index,
                Err(error) => {
                    device.destroy_buffer(buffer, None);
                    return Err(error);
                }
            };
            trace::event!(
                "target_nr_readback_memory",
                json!({"memory_type":memory_type,"flags":props.memory_types[memory_type as usize].property_flags.as_raw(),"host_cached":props.memory_types[memory_type as usize].property_flags.contains(vk::MemoryPropertyFlags::HOST_CACHED),"host_coherent":true})
            );
            let allocation = device.allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type),
                None,
            );
            let memory = match allocation {
                Ok(m) => m,
                Err(e) => {
                    device.destroy_buffer(buffer, None);
                    return Err(e.into());
                }
            };
            nr.readback = Some((buffer, memory));
            device.bind_buffer_memory(buffer, memory, 0)?;
        }
        trace::event!(
            "target_nr_ready",
            json!({"input":[size.width,size.height],"output":[size.width,size.height],"color":"gamma_rgba16f","depth":"synthetic_constant","motion":"present_nvof_uv","raw_copy_extent":nr.raw_extent.map(|e|[e.width,e.height])})
        );
        Ok(nr)
    }
    pub(super) unsafe fn run(
        &mut self,
        queue: vk::Queue,
        info: &vk::PresentInfoKHR,
        motion: Resource,
        native: Option<Source>,
        intensity: f32,
        options: crate::advanced_settings::NrOptions,
        reset: bool,
        source_boundary: bool,
        revision: u64,
        write_present: bool,
        defer_tail: bool,
        frame: u32,
    ) -> Result<Output> {
        let frame_capture = crate::frame_capture::selected(frame) && self.frame_capture.is_some();
        let defer_tail = defer_tail && !frame_capture;
        if self.in_flight || self.pending.is_some() {
            return Err("NR previous submission has not completed".into());
        }
        if info.p_image_indices.is_null()
            || (info.wait_semaphore_count != 0 && info.p_wait_semaphores.is_null())
        {
            return Err("invalid NR Present inputs".into());
        }
        self.retire_second(options)?;
        if options.second_pass.enabled && self.second.is_none() && self.second_failure.is_none() {
            match Second::new(&self.device, &self.look_memory, self.resources[1]) {
                Ok(second) => self.second = Some(second),
                Err(error) => {
                    self.second_failure = Some(error.to_string());
                    trace::event!(
                        "target_nr_second_fallback",
                        json!({"stage":"prepare_before_submit","error":self.second_failure,"output":"first_pass","retry":"explicit_retry_toggle_or_recreation"})
                    );
                }
            }
        }
        if let Some(second) = &mut self.second {
            if !options.look.bypass() && second.look.is_none() && second.look_failure.is_none() {
                let prepared = if self.look_srgb {
                    crate::nr_look::Look::new(
                        &self.device,
                        self.look_memory,
                        self.resources[1],
                        second.output.unwrap(),
                    )
                } else {
                    Err("Look supports only the SDR sRGB nonlinear color contract".into())
                };
                match prepared {
                    Ok(look) => second.look = Some(look),
                    Err(error) => {
                        second.look_failure = Some(error.to_string());
                        trace::event!(
                            "target_nr_look_fallback",
                            json!({"pass":2,"error":second.look_failure,"output":"unmodified_second_nr","retry":"recreation"})
                        );
                    }
                }
            }
            if let Some(look) = &mut second.look {
                if let Err(error) = look.prepare_temporal(options.look, self.resources[3]) {
                    trace::event!(
                        "target_nr_temporal_look_fallback",
                        json!({"pass":2,"error":error.to_string(),"output":"spatial_or_basic_look","retry":"recreation"})
                    );
                }
                if let Err(error) = look.prepare_spatial(options.look) {
                    trace::event!(
                        "target_nr_spatial_look_fallback",
                        json!({"pass":2,"error":error.to_string(),"output":"basic_look_or_unmodified_second_nr","retry":"recreation"})
                    );
                }
            }
        }
        // Allocate only at a quiescent boundary; retain failure until resize or
        // relaunch, avoiding repeated attempts on every frame.
        if !options.look.bypass() && self.look.is_none() && self.look_failure.is_none() {
            let prepared = if self.look_srgb {
                crate::nr_look::Look::new(
                    &self.device,
                    self.look_memory,
                    self.resources[0],
                    self.resources[1],
                )
            } else {
                Err("Look supports only the SDR sRGB nonlinear color contract".into())
            };
            match prepared {
                Ok(look) => self.look = Some(look),
                Err(error) => {
                    self.look_failure = Some(error.to_string());
                    trace::event!(
                        "target_nr_look_fallback",
                        json!({"error":self.look_failure,"output":"unmodified_nr","retry":"resize_or_relaunch"})
                    );
                }
            }
        }
        if let Some(look) = self.look.as_mut() {
            if let Err(error) = look.prepare_temporal(options.look, self.resources[3]) {
                trace::event!(
                    "target_nr_temporal_look_fallback",
                    json!({"pass":1,"error":error.to_string(),"output":"spatial_or_basic_look","retry":"recreation"})
                );
            }
            if let Err(error) = look.prepare_spatial(options.look) {
                trace::event!(
                    "target_nr_spatial_look_fallback",
                    json!({"error":error.to_string(),"output":"basic_look_or_unmodified_nr","retry":"resize_or_relaunch"})
                );
            }
        }
        let source = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("invalid NR present index")?;
        let v = viewport(self.window, native)?;
        let contract = (intensity.to_bits(), write_present, options);
        if self.capture_controls != Some(contract) {
            self.capture_controls = Some(contract);
            self.captures = 0;
        }
        let capture = self.readback.filter(|_| self.captures < 3);
        let tail_deferred = defer_tail && !write_present && native.is_some() && capture.is_none();
        // The comparison needs only one word after this bounded input fence.
        // It also retires borrowed game images before a deferred NR/SR tail.
        let original_waits = if info.wait_semaphore_count == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
        };
        let d = &self.device;
        let mut cmd = self.command;
        let started = Instant::now();
        d.reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())?;
        d.reset_fences(&[self.fence])?;
        d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
        if let Some(t) = &self.timing {
            d.cmd_reset_query_pool(cmd, t.pool, 0, 10);
            t.mark(d, cmd, 0);
        }
        let previous = vk::Image::from_raw(
            self.previous_input
                .ok_or("NR previous input missing")?
                .image,
        );
        fg_api::transition(
            d,
            cmd,
            previous,
            if self.initialized {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::UNDEFINED
            },
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        if self.initialized {
            let input = vk::Image::from_raw(self.resources[0].image);
            fg_api::transition(
                d,
                cmd,
                input,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            let layers = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1);
            d.cmd_copy_image(
                cmd,
                input,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                previous,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageCopy::default()
                    .src_subresource(layers)
                    .dst_subresource(layers)
                    .extent(vk::Extent3D {
                        width: self.size.width,
                        height: self.size.height,
                        depth: 1,
                    })],
            );
            fg_api::transition(
                d,
                cmd,
                input,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        } else {
            d.cmd_clear_color_image(
                cmd,
                previous,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue { float32: [0.0; 4] },
                &[fg_api::range()],
            );
        }
        fg_api::transition(
            d,
            cmd,
            previous,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        for r in &self.resources {
            fg_api::transition(
                d,
                cmd,
                vk::Image::from_raw(r.image),
                if self.initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
        }
        let color = native.map_or(source, |n| n.image);
        let color_layout = if native.is_some() {
            vk::ImageLayout::GENERAL
        } else {
            vk::ImageLayout::PRESENT_SRC_KHR
        };
        fg_api::transition(
            d,
            cmd,
            color,
            color_layout,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let mut blit_source = color;
        if let Some(raw) = self.raw_extent {
            let image = vk::Image::from_raw(self.resources[4].image);
            let sub = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1);
            // Copy all encoded bytes first, then crop/flip the private UNORM
            // image. The source may be larger than the selected crop.
            d.cmd_copy_image(
                cmd,
                color,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageCopy::default()
                    .src_subresource(sub)
                    .dst_subresource(sub)
                    .extent(vk::Extent3D {
                        width: raw.width,
                        height: raw.height,
                        depth: 1,
                    })],
            );
            fg_api::transition(
                d,
                cmd,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            blit_source = image;
        }
        blit(
            d,
            cmd,
            blit_source,
            native.map_or(offsets(self.window), |n| n.offsets),
            vk::Image::from_raw(self.resources[0].image),
            offsets(self.size),
        );
        if self.raw_extent.is_some() {
            fg_api::transition(
                d,
                cmd,
                blit_source,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
        }
        fg_api::transition(
            d,
            cmd,
            color,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            color_layout,
        );
        d.cmd_clear_color_image(
            cmd,
            vk::Image::from_raw(self.resources[2].image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue { float32: [0.5; 4] },
            &[fg_api::range()],
        );
        let motion_image = vk::Image::from_raw(motion.image);
        fg_api::transition(
            d,
            cmd,
            motion_image,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let scale = [
            motion.width as f32 / self.window.width as f32,
            motion.height as f32 / self.window.height as f32,
        ];
        let motion_region = viewport_offsets([
            v[0] * scale[0],
            v[1] * scale[1],
            v[2] * scale[0],
            v[3] * scale[1],
        ]);
        blit(
            d,
            cmd,
            motion_image,
            motion_region,
            vk::Image::from_raw(self.resources[3].image),
            offsets(self.size),
        );
        fg_api::transition(
            d,
            cmd,
            motion_image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        for r in &self.resources {
            fg_api::transition(
                d,
                cmd,
                vk::Image::from_raw(r.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        self.difference
            .as_ref()
            .ok_or("NR input comparison missing")?
            .record(cmd, self.size);
        let mut resources = std::array::from_fn(|i| descriptor(self.resources[i], i == 1));
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 1);
        }
        let input_started = Instant::now();
        // Finish every access to borrowed game images before returning to
        // the emulator. Only private resources remain in flight afterwards.
        d.end_command_buffer(cmd)?;
        d.reset_fences(&[self.input_fence])?;
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; original_waits.len()];
        self.in_flight = true;
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[cmd])
                .wait_semaphores(original_waits)
                .wait_dst_stage_mask(&stages)],
            self.input_fence,
        )?;
        d.wait_for_fences(&[self.input_fence], true, 10_000_000_000)?;
        self.in_flight = false;
        cmd = self.evaluate_command;
        d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
        let input_wait_us = input_started.elapsed().as_micros();
        let requested = crate::nr_source_frames::Controls {
            intensity,
            options,
            revision,
        };
        let mut source_frame = self.source_frames.plan(
            self.difference.as_ref().unwrap().identical(),
            source_boundary,
            reset,
            requested,
        )?;
        let look_now_ms = self.look_origin.elapsed().as_millis() as u64;
        let mut look_reset = source_frame.reset;
        let capture = capture.filter(|_| source_frame.evaluate);
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 2);
        }
        let evaluate_started = Instant::now();
        let mut actual_passes = self.last_passes;
        let mut output = self.last_output.unwrap_or(self.resources[1]);
        let mut look_input = if actual_passes == 2 {
            self.resources[1]
        } else {
            self.resources[0]
        };
        let (second_intensity, second_options) = options.second_pass.resolve(intensity, options);
        let mut intermediate_wait_us = 0;
        let mut second_call_us = 0;
        let mut pass_details = Vec::new();
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 6);
        }
        if source_frame.evaluate {
            let output = vk::Image::from_raw(self.resources[1].image);
            fg_api::transition(
                d,
                cmd,
                output,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            d.cmd_clear_color_image(
                cmd,
                output,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue {
                    float32: [-16.0, 0.0, -16.0, 1.0],
                },
                &[fg_api::range()],
            );
            fg_api::transition(
                d,
                cmd,
                output,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            self.feature.as_mut().ok_or("NR feature missing")?.record(
                cmd,
                &mut resources,
                intensity,
                options,
                source_frame.reset,
                [
                    self.window.width as f32 / v[2],
                    self.window.height as f32 / v[3],
                ],
            )?;
            let (parameters, feature) = self.feature.as_ref().unwrap().identity();
            pass_details.push(json!({"instance":self.feature.as_ref().unwrap().instance(),"pass":1,"evaluated":true,"outputReused":false,"reset":source_frame.reset,
                "sourceFrameId":source_frame.id,"intensity":intensity,"options":options.model_only(),
                "parameters":parameters,"feature":feature,"inputImage":self.resources[0].image,"outputImage":self.resources[1].image,
                "depthImage":self.resources[2].image,"motionImage":self.resources[3].image}));
        }
        let first_call_us = evaluate_started.elapsed().as_micros();
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 7);
            // Defaults cover repeats and a discarded/disabled second pass.
            t.mark(d, cmd, 8);
            t.mark(d, cmd, 9);
        }
        if source_frame.evaluate {
            actual_passes = 1;
            output = self.resources[1];
            look_input = self.resources[0];
            if let Some(second) = &mut self.second {
                // Complete pass one before asking the SDK to record pass two.
                // Any failed second recording can then be discarded in full.
                d.end_command_buffer(cmd)?;
                d.reset_fences(&[self.input_fence])?;
                self.in_flight = true;
                let wait_started = Instant::now();
                d.queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&[cmd])],
                    self.input_fence,
                )?;
                d.wait_for_fences(&[self.input_fence], true, 10_000_000_000)?;
                self.in_flight = false;
                intermediate_wait_us = wait_started.elapsed().as_micros();
                cmd = self.second_command;
                d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
                let second_output = second.output.unwrap();
                fg_api::transition(
                    d,
                    cmd,
                    vk::Image::from_raw(second_output.image),
                    if second.initialized {
                        vk::ImageLayout::GENERAL
                    } else {
                        vk::ImageLayout::UNDEFINED
                    },
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
                d.cmd_clear_color_image(
                    cmd,
                    vk::Image::from_raw(second_output.image),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue {
                        float32: [-16.0, 0.0, -16.0, 1.0],
                    },
                    &[fg_api::range()],
                );
                fg_api::transition(
                    d,
                    cmd,
                    vk::Image::from_raw(second_output.image),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
                d.cmd_pipeline_barrier(
                    cmd,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[vk::MemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                        .dst_access_mask(
                            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
                        )],
                    &[],
                    &[],
                );
                let second_reset = second.history.reset(
                    source_frame.id,
                    second_intensity,
                    second_options,
                    source_frame.reset,
                );
                let mut second_resources = [
                    descriptor(self.resources[1], false),
                    descriptor(second_output, true),
                    descriptor(self.resources[2], false),
                    descriptor(self.resources[3], false),
                ];
                let call_started = Instant::now();
                if let Some(t) = &self.timing {
                    d.cmd_reset_query_pool(cmd, t.pool, 8, 2);
                    t.mark(d, cmd, 8);
                }
                let result = second
                    .feature
                    .as_mut()
                    .ok_or("NR second feature missing")?
                    .record(
                        cmd,
                        &mut second_resources,
                        second_intensity,
                        second_options,
                        second_reset,
                        [
                            self.window.width as f32 / v[2],
                            self.window.height as f32 / v[3],
                        ],
                    );
                second_call_us = call_started.elapsed().as_micros();
                match result {
                    Ok(()) => {
                        look_reset |= second_reset;
                        if let Some(t) = &self.timing {
                            t.mark(d, cmd, 9);
                        }
                        actual_passes = 2;
                        output = second_output;
                        look_input = self.resources[1];
                        let (parameters, feature) = second.feature.as_ref().unwrap().identity();
                        pass_details.push(json!({"instance":second.feature.as_ref().unwrap().instance(),"pass":2,"evaluated":true,"outputReused":false,"reset":second_reset,
                            "sourceFrameId":source_frame.id,"intensity":second_intensity,"options":second_options,
                            "parameters":parameters,"feature":feature,"inputImage":self.resources[1].image,"outputImage":second_output.image,
                            "depthImage":self.resources[2].image,"motionImage":self.resources[3].image}));
                    }
                    Err(error) => {
                        // No second-pass command has been submitted. Reset it
                        // before releasing SDK/images referenced by that recording.
                        d.reset_command_buffer(
                            cmd,
                            vk::CommandBufferResetFlags::RELEASE_RESOURCES,
                        )?;
                        drop(self.second.take());
                        self.second_failure = Some(error.to_string());
                        trace::event!(
                            "target_nr_second_fallback",
                            json!({"stage":"discard_unsubmitted_recording","source_frame_id":source_frame.id,
                            "error":self.second_failure,"first_pass_fenced":true,"second_submitted":false,"output":"first_pass","retry":"explicit_retry_toggle_or_recreation"})
                        );
                        d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
                    }
                }
            }
            source_frame.reset_consumers |= self.last_passes != actual_passes;
        }
        let evaluate_cpu_us = first_call_us + second_call_us;
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 3);
        }
        if source_frame.evaluate && !options.look.bypass() {
            if actual_passes == 2 {
                if let Some(look) = self.second.as_mut().and_then(|s| s.look.as_mut()) {
                    look.record_with_context(
                        cmd,
                        self.size,
                        options.look,
                        Some(crate::nr_look_history::Context {
                            source_frame_id: source_frame.id,
                            now_ms: look_now_ms,
                            reset: look_reset,
                            uv_scale: [
                                self.window.width as f32 / v[2],
                                self.window.height as f32 / v[3],
                            ],
                        }),
                    );
                }
            } else if let Some(look) = &mut self.look {
                look.record_with_context(
                    cmd,
                    self.size,
                    options.look,
                    Some(crate::nr_look_history::Context {
                        source_frame_id: source_frame.id,
                        now_ms: look_now_ms,
                        reset: look_reset,
                        uv_scale: [
                            self.window.width as f32 / v[2],
                            self.window.height as f32 / v[3],
                        ],
                    }),
                );
            }
        }
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 4);
        }
        if write_present {
            let image = vk::Image::from_raw(output.image);
            fg_api::transition(
                d,
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            fg_api::transition(
                d,
                cmd,
                source,
                vk::ImageLayout::PRESENT_SRC_KHR,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            if native.is_some() {
                d.cmd_clear_color_image(
                    cmd,
                    source,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue {
                        float32: [0.0, 0.0, 0.0, 1.0],
                    },
                    &[fg_api::range()],
                );
                fg_api::transition(
                    d,
                    cmd,
                    source,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
            }
            blit(
                d,
                cmd,
                image,
                offsets(self.size),
                source,
                viewport_offsets(v),
            );
            fg_api::transition(
                d,
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            fg_api::transition(
                d,
                cmd,
                source,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::PRESENT_SRC_KHR,
            );
        }
        // Sample each applied strength/output contract, without overwriting the
        // previous contract's evidence after a resize or control transition.
        let bytes = u64::from(self.size.width) * u64::from(self.size.height) * 8;
        if let Some((buffer, _)) = capture {
            for (index, resource) in [look_input, output].into_iter().enumerate() {
                let image = vk::Image::from_raw(resource.image);
                fg_api::transition(
                    d,
                    cmd,
                    image,
                    vk::ImageLayout::GENERAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                );
                d.cmd_copy_image_to_buffer(
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    buffer,
                    &[vk::BufferImageCopy::default()
                        .buffer_offset(index as u64 * bytes)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width: self.size.width,
                            height: self.size.height,
                            depth: 1,
                        })],
                );
                fg_api::transition(
                    d,
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
            }
            d.cmd_pipeline_barrier(
                cmd,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)],
                &[],
                &[],
            );
        }
        if let Some(t) = &self.timing {
            t.mark(d, cmd, 5);
        }
        if frame_capture {
            if actual_passes == 2 {
                self.second
                    .as_ref()
                    .unwrap()
                    .capture
                    .as_ref()
                    .ok_or("second NR capture missing")?
                    .record(cmd);
            } else {
                self.frame_capture.as_ref().unwrap().record(cmd);
            }
        }
        d.end_command_buffer(cmd)?;
        let record_cpu_us = started.elapsed().as_micros();
        // Keep captures and NR-only synchronous. SR consumes this semaphore and
        // waits its own fence before the chain may reuse any NR resource.
        let deferred = tail_deferred
            || (!write_present
                && !frame_capture
                && capture.is_none()
                && std::env::var("NS_STREAMLINE_NR_GPU_HANDOFF").as_deref() != Ok("0"));
        let signals = if deferred { vec![self.ready] } else { vec![] };
        self.in_flight = true;
        let submit_started = Instant::now();
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[cmd])
                .signal_semaphores(&signals)],
            self.fence,
        )?;
        let submit_cpu_us = submit_started.elapsed().as_micros();
        let wait_started = Instant::now();
        if !deferred {
            d.wait_for_fences(&[self.fence], true, 10_000_000_000)?;
        }
        let wait_cpu_us = wait_started.elapsed().as_micros();
        self.in_flight = deferred;
        if frame_capture {
            if actual_passes == 2 {
                self.second
                    .as_ref()
                    .unwrap()
                    .capture
                    .as_ref()
                    .unwrap()
                    .save("nr", frame)?;
            } else {
                self.frame_capture.as_ref().unwrap().save("nr", frame)?;
            }
        }
        self.initialized = true;
        if source_frame.evaluate {
            if actual_passes == 2 {
                let second = self.second.as_mut().unwrap();
                second.initialized = true;
                second
                    .history
                    .submitted(source_frame.id, second_intensity, second_options);
            }
            self.last_passes = actual_passes;
            self.last_output = Some(output);
            self.last_pass_details = pass_details;
            let source_identity = identity(self.swapchain, self.window, native);
            for details in &self.last_pass_details {
                let mut details = details.clone();
                details["presentFrame"] = json!(frame);
                details["sourceIdentity"] = json!(source_identity.identity);
                details["mapping"] = json!(source_identity.mapping);
                trace::event!("target_nr_pass_frame", details);
            }
        }
        self.source_frames.submitted(source_frame, requested);
        if source_frame.evaluate {
            if actual_passes == 2 {
                if let Some(look) = self.second.as_mut().and_then(|s| s.look.as_mut()) {
                    look.submitted();
                }
            } else if let Some(look) = &mut self.look {
                look.submitted();
            }
        }
        let applied = self
            .source_frames
            .applied()
            .ok_or("NR applied controls missing")?;
        let gpu = (!deferred)
            .then(|| self.timing.as_ref().and_then(|t| t.read(d)))
            .flatten();
        let readback_started = Instant::now();
        if let Some((_, memory)) = capture {
            let mapped = d.map_memory(memory, 0, bytes * 2, vk::MemoryMapFlags::empty())?;
            // Device-local/write-combined mappings are costly for repeated tiny
            // CPU reads. Copy once into cached host RAM before statistics/PPM.
            let data = std::slice::from_raw_parts(mapped as *const u16, bytes as usize).to_vec();
            d.unmap_memory(memory);
            let (input, output) = data.split_at(data.len() / 2);
            let finite = data.iter().all(|&v| half(v).is_finite());
            let sentinel = output
                .chunks_exact(4)
                .filter(|v| *v == [0xcc00, 0, 0xcc00, 0x3c00])
                .count();
            let difference = input
                .iter()
                .zip(output)
                .map(|(&a, &b)| (half(a) - half(b)).abs() as f64)
                .sum::<f64>()
                / input.len() as f64;
            use sha2::{Digest, Sha256};
            let output_hash = format!(
                "{:x}",
                Sha256::digest(std::slice::from_raw_parts(
                    output.as_ptr() as *const u8,
                    bytes as usize
                ))
            );
            let capture_id = CAPTURE_ID.fetch_add(1, Ordering::Relaxed);
            let mut files = Vec::new();
            if let Some(dir) = std::env::var_os("NS_STREAMLINE_LIVE_DIR") {
                for (name, pixels) in [("input", input), ("output", output)] {
                    let mut ppm =
                        format!("P6\n{} {}\n255\n", self.size.width, self.size.height).into_bytes();
                    for pixel in pixels.chunks_exact(4) {
                        ppm.extend(
                            pixel[..3]
                                .iter()
                                .map(|&v| (half(v).clamp(0.0, 1.0) * 255.0).round() as u8),
                        );
                    }
                    let file = format!(
                        "nr-{name}-{}-{}-{capture_id}.ppm",
                        self.size.width, self.size.height
                    );
                    files.push(file.clone());
                    if let Err(error) =
                        std::fs::write(std::path::PathBuf::from(&dir).join(file), ppm)
                    {
                        return Err(error.into());
                    }
                }
            }
            trace::event!(
                "target_nr_readback",
                json!({"capture":capture_id,"files":files,"extent":[self.size.width,self.size.height],"finite":finite,"sentinel_pixels":sentinel,"mean_absolute_input_difference":difference,"output_sha256":output_hash,"intensity":if actual_passes == 2 {second_intensity} else {intensity},"actual_passes":actual_passes,"fence_completed":true})
            );
            self.captures += 1;
            if !finite || sentinel != 0 {
                return Err("NR readback is nonfinite or retained sentinel".into());
            }
        }
        let pass_gpu = (!deferred && source_frame.evaluate)
            .then(|| self.timing.as_ref().and_then(|t| t.read_passes(d)))
            .flatten();
        let profile = json!({"look":self.look_status(applied.options.look),"pipeline":self.pipeline_status(applied.options,source_frame.evaluate),"cpu_intermediate_fence_wait_us":intermediate_wait_us,"cpu_pass_evaluate_call_us":[first_call_us,second_call_us],"gpu_pass_us":pass_gpu,"source_frame_id":source_frame.id,"actual_passes":actual_passes,"evaluated":source_frame.evaluate,"output_reused":!source_frame.evaluate,"controls_pending":source_frame.controls_pending,"gpu_us":gpu,"gpu_stages":["prepare","evaluate_including_intermediate_gap","look","output_and_optional_readback"],"cpu_record_us":record_cpu_us,"cpu_input_submit_wait_us":input_wait_us,"cpu_evaluate_call_us":evaluate_cpu_us,"cpu_submit_us":submit_cpu_us,"cpu_fence_wait_us":if deferred {0} else {wait_cpu_us},"cpu_readback_us":readback_started.elapsed().as_micros(),"readback":capture.is_some(),"input":[self.size.width,self.size.height],"gpu_handoff":deferred,"tail_deferred":tail_deferred});
        if deferred {
            self.pending = Some((output, profile));
        } else if self.timing.is_some() {
            trace::event!("target_nr_profile", profile);
        }
        trace::event!(
            "target_nr_submission",
            json!({"present_frame":frame,"source_frame_id":source_frame.id,"source_frame_basis":"exact_nr_gamma_input","actual_passes":actual_passes,"evaluated":source_frame.evaluate,"output_reused":!source_frame.evaluate,"controls_pending":source_frame.controls_pending,"applied_revision":applied.revision,"input":[self.size.width,self.size.height],"source":if native.is_some(){"native_source"}else{"present_source"},"output_image":output.image,"history_reset":source_frame.reset,"intensity":applied.intensity,"synthetic_depth":true,"motion":"nvof_present_uv","motion_uv_basis_scale":[self.window.width as f32/v[2],self.window.height as f32/v[3]],"fence_completed":!deferred,"ready_semaphore":if deferred {self.ready.as_raw()} else {0},"tail_deferred":tail_deferred,"borrowed_inputs_completed":true,"consumed_wait_count":original_waits.len(),"output_to":if write_present{"swapchain"}else{"SR"},"cpu_record_submit_wait_us":started.elapsed().as_micros()})
        );
        Ok(Output {
            resource: output,
            source_frame,
            applied_revision: applied.revision,
        })
    }
    pub(super) fn applied_options(&self) -> Option<crate::advanced_settings::NrOptions> {
        self.source_frames.applied().map(|c| c.options)
    }
    pub(super) fn applied_intensity(&self) -> Option<f32> {
        self.source_frames.applied().map(|c| c.intensity)
    }
    pub(super) fn ready_semaphore(&self) -> Option<vk::Semaphore> {
        self.pending.as_ref().map(|_| self.ready)
    }
    pub(super) fn tail_deferred(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|(_, p)| p["tail_deferred"] == true)
    }
    /// Called only after SR's fence completed on the same queue. Its semaphore
    /// wait is retired, and NR's earlier fence must be signaled. Never block here.
    pub(super) unsafe fn finish_handoff(&mut self) -> Result<()> {
        if self.pending.is_none() {
            return Ok(());
        }
        if !self.device.get_fence_status(self.fence)? {
            return Err("NR fence not signaled after SR completion".into());
        }
        let (output, mut profile) = self.pending.take().unwrap();
        self.in_flight = false;
        if let Some(t) = &self.timing {
            profile["gpu_us"] = json!(t.read(&self.device));
            if profile["evaluated"] == true {
                profile["gpu_pass_us"] = json!(t.read_passes(&self.device));
            }
            trace::event!("target_nr_profile", profile);
        }
        trace::event!(
            "target_nr_completion",
            json!({"output_image":output.image,"ready_semaphore":self.ready.as_raw(),"fence_completed":true})
        );
        Ok(())
    }
}
unsafe fn blit(
    d: &ash::Device,
    cmd: vk::CommandBuffer,
    src: vk::Image,
    a: [vk::Offset3D; 2],
    dst: vk::Image,
    b: [vk::Offset3D; 2],
) {
    let sub = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1);
    d.cmd_blit_image(
        cmd,
        src,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        dst,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &[vk::ImageBlit::default()
            .src_subresource(sub)
            .src_offsets(a)
            .dst_subresource(sub)
            .dst_offsets(b)],
        vk::Filter::LINEAR,
    );
}
impl Drop for Nr {
    fn drop(&mut self) {
        unsafe {
            if self.in_flight {
                std::process::abort();
            }
            // The chain owner waits for quiescence before resize/retirement; normal
            // run completion already fenced every submission.
            drop(self.feature.take());
            drop(self.look.take());
            drop(self.second.take());
            drop(self.difference.take());
            if let Some(t) = self.timing.take() {
                self.device.destroy_query_pool(t.pool, None);
            }
            if let Some((buffer, memory)) = self.readback.take() {
                self.device.destroy_buffer(buffer, None);
                self.device.free_memory(memory, None);
            }
            for r in self.resources.drain(..).chain(self.previous_input.take()) {
                self.device
                    .destroy_image_view(vk::ImageView::from_raw(r.view), None);
                self.device
                    .destroy_image(vk::Image::from_raw(r.image), None);
                self.device
                    .free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
            if self.fence != vk::Fence::null() {
                self.device.destroy_fence(self.fence, None);
            }
            if self.input_fence != vk::Fence::null() {
                self.device.destroy_fence(self.input_fence, None);
            }
            if self.ready != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.ready, None);
            }
            if self.pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.pool, None);
            }
        }
    }
}
fn readback_memory_type(props: &vk::PhysicalDeviceMemoryProperties, bits: u32) -> Result<u32> {
    let required = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
    fg_api::memory_type(props, bits, required | vk::MemoryPropertyFlags::HOST_CACHED)
        .or_else(|_| fg_api::memory_type(props, bits, required))
}
fn half(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 31);
    let fraction = u32::from(bits & 1023);
    let magnitude = match exponent {
        0 => ((fraction as f32) * f32::from_bits(0x3380_0000)).to_bits(),
        31 => 0x7f80_0000 | (fraction << 13),
        _ => ((exponent + 112) << 23) | (fraction << 13),
    };
    f32::from_bits(sign | magnitude)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readback_prefers_cached_coherent_memory_and_respects_buffer_type_bits() {
        let mut props = vk::PhysicalDeviceMemoryProperties {
            memory_type_count: 3,
            ..Default::default()
        };
        props.memory_types[0].property_flags = vk::MemoryPropertyFlags::DEVICE_LOCAL;
        props.memory_types[1].property_flags =
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        props.memory_types[2].property_flags =
            props.memory_types[1].property_flags | vk::MemoryPropertyFlags::HOST_CACHED;
        assert_eq!(readback_memory_type(&props, 0b111).unwrap(), 2);
        assert_eq!(readback_memory_type(&props, 0b011).unwrap(), 1);
        assert!(readback_memory_type(&props, 0b001).is_err());
    }
    #[test]
    fn readback_half_conversion_preserves_every_finite_value_and_special_class() {
        for bits in 0..=u16::MAX {
            let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
            let exponent = i32::from((bits >> 10) & 31);
            let fraction = f32::from(bits & 1023);
            let expected = match exponent {
                0 => sign * fraction * 2f32.powi(-24),
                31 if fraction == 0.0 => sign * f32::INFINITY,
                31 => f32::NAN,
                _ => sign * (1.0 + fraction / 1024.0) * 2f32.powi(exponent - 15),
            };
            let actual = half(bits);
            if expected.is_nan() {
                assert!(actual.is_nan());
            } else {
                assert_eq!(actual.to_bits(), expected.to_bits(), "half {bits:#x}");
            }
        }
    }
    #[test]
    fn mapping_identity_includes_crop_flip_encoding_and_viewport() {
        let window = vk::Extent2D {
            width: 1920,
            height: 1080,
        };
        let chain = vk::SwapchainKHR::from_raw(1);
        let mut source = Source {
            image: vk::Image::from_raw(2),
            generation: 1,
            history_identity: 1,
            history_members: 1,
            history_paths: 1,
            history_update: "initial",
            history_route: [0; 4],
            usage: 1,
            extent: window,
            raw_copy: false,
            offsets: offsets(window),
            viewport: [0.0, 0.0, 1920.0, 1080.0],
        };
        let first = identity(chain, window, Some(source));
        source.offsets.swap(0, 1);
        assert_ne!(first.mapping, identity(chain, window, Some(source)).mapping);
        assert_eq!(first.extent, identity(chain, window, Some(source)).extent);
        source.raw_copy = true;
        let flipped = identity(chain, window, Some(source));
        source.viewport = [120.0, 0.0, 1680.0, 1080.0];
        assert_ne!(
            flipped.mapping,
            identity(chain, window, Some(source)).mapping
        );
        assert!(viewport(window, Some(source)).is_ok());
        source.viewport[2] = f32::NAN;
        assert!(viewport(window, Some(source)).is_err());
    }
}
