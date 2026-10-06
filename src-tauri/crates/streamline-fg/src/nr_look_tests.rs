//! Opt-in offscreen hardware test of the shipped SPIR-V, without NGX or games.
use super::*;
use crate::fg_api;
use std::{
    ffi::CStr,
    sync::atomic::{AtomicU32, Ordering},
};
static VALIDATION: AtomicU32 = AtomicU32::new(0);
static LOADER_NOTICES: AtomicU32 = AtomicU32::new(0);
unsafe extern "system" fn validation(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _: *mut std::ffi::c_void,
) -> vk::Bool32 {
    let message = CStr::from_ptr((*data).p_message).to_string_lossy();
    // Loader installation notices are reported separately from core/sync
    // validation. Never exempt DLL-load failures or any validation message.
    if severity == vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
        && kind == vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
        && (*data).message_id_number == 0
        && (message.starts_with("Removing layer VK_LAYER_reshade (") && message.contains("because it is a duplicate of VK_LAYER_reshade (")
            || ["VK_LAYER_NV_optimus", "VK_LAYER_NV_present", "VK_LAYER_AMD_switchable_graphics", "VK_LAYER_reshade"].iter().any(|name|
                message == format!("Layer \"{name}\" forced disabled because name matches filter of env var 'VK_LOADER_LAYERS_DISABLE'."))) {
        LOADER_NOTICES.fetch_add(1, Ordering::Relaxed);
        eprintln!("Look loader notice: {message}");
        return vk::FALSE;
    }
    if severity.intersects(
        vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
            | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING,
    ) {
        VALIDATION.fetch_add(1, Ordering::Relaxed);
        eprintln!("Look validation: {}", message);
    }
    vk::FALSE
}
struct Gpu {
    _entry: ash::Entry,
    instance: ash::Instance,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    device: ash::Device,
    queue: vk::Queue,
    images: Vec<Resource>,
    look: Option<Look>,
    difference: Option<crate::nr_input_difference::Difference>,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    initialized: bool,
    temporal_context: Option<crate::nr_look_history::Context>,
    motion: Option<Resource>,
    motion_initialized: bool,
    motion_value: [f32; 2],
    guide_pixels: Option<Vec<[u16; 4]>>,
    recomposing: bool,
}
const SIZE: vk::Extent2D = vk::Extent2D {
    width: 19,
    height: 3,
};
const PIXELS: usize = 57;
const BYTES: u64 = PIXELS as u64 * 8;
impl Gpu {
    unsafe fn new() -> Self {
        let entry = ash::Entry::load().unwrap();
        let strict = std::env::var("NS_NR_LOOK_GPU_VALIDATION").as_deref() == Ok("1");
        let layers = [c"VK_LAYER_KHRONOS_validation".as_ptr()];
        let extensions = [
            ash::ext::debug_utils::NAME.as_ptr(),
            c"VK_EXT_validation_features".as_ptr(),
        ];
        let mut callback = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(validation));
        let synchronization = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
        let mut features =
            vk::ValidationFeaturesEXT::default().enabled_validation_features(&synchronization);
        let app = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);
        let mut info = vk::InstanceCreateInfo::default().application_info(&app);
        if strict {
            info = info
                .enabled_layer_names(&layers)
                .enabled_extension_names(&extensions)
                .push_next(&mut features)
                .push_next(&mut callback);
        }
        let instance = entry.create_instance(&info, None).unwrap();
        let debug = strict.then(|| {
            let loader = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let messenger = loader
                .create_debug_utils_messenger(&callback, None)
                .unwrap();
            (loader, messenger)
        });
        let physical = instance.enumerate_physical_devices().unwrap()[0];
        let properties = instance.get_physical_device_properties(physical);
        eprintln!(
            "Look GPU: {}",
            CStr::from_ptr(properties.device_name.as_ptr()).to_string_lossy()
        );
        let family = instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .position(|q| q.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .unwrap() as u32;
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&[1.0])];
        let required =
            vk::PhysicalDeviceFeatures::default().shader_storage_image_extended_formats(true);
        let device = instance
            .create_device(
                physical,
                &vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queues)
                    .enabled_features(&required),
                None,
            )
            .unwrap();
        let queue = device.get_device_queue(family, 0);
        let props = instance.get_physical_device_memory_properties(physical);
        let usage = vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        let images = (0..3)
            .map(|_| {
                fg_api::texture_with_usage(
                    &device,
                    &props,
                    SIZE,
                    vk::Format::R16G16B16A16_SFLOAT,
                    usage,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let look = Some(Look::new(&device, props, images[0], images[1]).unwrap());
        let buffer = device
            .create_buffer(
                &vk::BufferCreateInfo::default()
                    .size(BYTES * 4)
                    .usage(vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST),
                None,
            )
            .unwrap();
        let req = device.get_buffer_memory_requirements(buffer);
        let index = fg_api::memory_type(
            &props,
            req.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .unwrap();
        let memory = device
            .allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(req.size)
                    .memory_type_index(index),
                None,
            )
            .unwrap();
        device.bind_buffer_memory(buffer, memory, 0).unwrap();
        let pool = device
            .create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
            .unwrap();
        let command = device
            .allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
            .unwrap()[0];
        let fence = device
            .create_fence(&vk::FenceCreateInfo::default(), None)
            .unwrap();
        Self {
            _entry: entry,
            instance,
            debug,
            device,
            queue,
            images,
            look,
            difference: None,
            buffer,
            memory,
            pool,
            command,
            fence,
            initialized: false,
            temporal_context: None,
            motion: None,
            motion_initialized: false,
            motion_value: [0.0; 2],
            guide_pixels: None,
            recomposing: false,
        }
    }
    unsafe fn run(
        &mut self,
        input: &[[u16; 4]],
        nr: &[[u16; 4]],
        options: LookOptions,
    ) -> Vec<[u16; 4]> {
        self.look
            .as_mut()
            .unwrap()
            .prepare_spatial(options)
            .unwrap();
        if let Some(motion) = self.motion {
            self.look
                .as_mut()
                .unwrap()
                .prepare_temporal(options, motion)
                .unwrap();
        } else if options.temporal.mode == crate::advanced_settings::TemporalMode::Static {
            self.look
                .as_mut()
                .unwrap()
                .prepare_temporal(options, self.images[0])
                .unwrap();
        }
        assert_eq!(input.len(), PIXELS);
        assert_eq!(nr.len(), PIXELS);
        let d = &self.device;
        let mapped = d
            .map_memory(self.memory, 0, BYTES * 4, vk::MemoryMapFlags::empty())
            .unwrap() as *mut [u16; 4];
        std::ptr::copy_nonoverlapping(input.as_ptr(), mapped, PIXELS);
        std::ptr::copy_nonoverlapping(nr.as_ptr(), mapped.add(PIXELS), PIXELS);
        let guide = self.guide_pixels.as_deref().unwrap_or(input);
        assert_eq!(guide.len(), PIXELS);
        std::ptr::copy_nonoverlapping(guide.as_ptr(), mapped.add(PIXELS * 2), PIXELS);
        d.unmap_memory(self.memory);
        d.reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())
            .unwrap();
        d.reset_fences(&[self.fence]).unwrap();
        d.begin_command_buffer(self.command, &vk::CommandBufferBeginInfo::default())
            .unwrap();
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(vk::Extent3D {
                width: SIZE.width,
                height: SIZE.height,
                depth: 1,
            });
        for (i, r) in self.images.iter().enumerate() {
            if i == 1 && self.recomposing {
                continue;
            }
            let image = vk::Image::from_raw(r.image);
            fg_api::transition(
                d,
                self.command,
                image,
                if self.initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            d.cmd_copy_buffer_to_image(
                self.command,
                self.buffer,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region.buffer_offset(i as u64 * BYTES)],
            );
            fg_api::transition(
                d,
                self.command,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        if let Some(motion) = self.motion {
            let image = vk::Image::from_raw(motion.image);
            fg_api::transition(
                d,
                self.command,
                image,
                if self.motion_initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            d.cmd_clear_color_image(
                self.command,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue {
                    float32: [self.motion_value[0], self.motion_value[1], 0.0, 0.0],
                },
                &[fg_api::range()],
            );
            fg_api::transition(
                d,
                self.command,
                image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            self.motion_initialized = true;
        }
        if let Some(difference) = &self.difference {
            difference.record(self.command, SIZE);
        }
        if self.recomposing {
            self.look
                .as_mut()
                .unwrap()
                .recompose(self.command, SIZE, options);
        } else if let Some(context) = self.temporal_context {
            self.look.as_mut().unwrap().record_with_context(
                self.command,
                SIZE,
                options,
                Some(context),
            );
        } else {
            self.look
                .as_mut()
                .unwrap()
                .record(self.command, SIZE, options);
        }
        let output = vk::Image::from_raw(self.images[1].image);
        fg_api::transition(
            d,
            self.command,
            output,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        d.cmd_copy_image_to_buffer(
            self.command,
            output,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            self.buffer,
            &[region.buffer_offset(BYTES * 3)],
        );
        fg_api::transition(
            d,
            self.command,
            output,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        d.cmd_pipeline_barrier(
            self.command,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)],
            &[],
            &[],
        );
        d.end_command_buffer(self.command).unwrap();
        d.queue_submit(
            self.queue,
            &[vk::SubmitInfo::default().command_buffers(&[self.command])],
            self.fence,
        )
        .unwrap();
        d.wait_for_fences(&[self.fence], true, 10_000_000_000)
            .unwrap();
        self.initialized = true;
        if !self.recomposing {
            self.look.as_mut().unwrap().submitted();
        }
        let mapped = d
            .map_memory(self.memory, 0, BYTES * 4, vk::MemoryMapFlags::empty())
            .unwrap() as *const [u16; 4];
        let result = std::slice::from_raw_parts(mapped.add(PIXELS * 3), PIXELS).to_vec();
        d.unmap_memory(self.memory);
        result
    }
}
impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            let d = &self.device;
            d.device_wait_idle().unwrap();
            drop(self.look.take());
            drop(self.difference.take());
            d.destroy_fence(self.fence, None);
            d.destroy_command_pool(self.pool, None);
            d.destroy_buffer(self.buffer, None);
            d.free_memory(self.memory, None);
            for r in self.images.iter().chain(self.motion.iter()) {
                d.destroy_image_view(vk::ImageView::from_raw(r.view), None);
                d.destroy_image(vk::Image::from_raw(r.image), None);
                d.free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
            d.destroy_device(None);
            if let Some((loader, messenger)) = &self.debug {
                loader.destroy_debug_utils_messenger(*messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
fn half(v: u16) -> f32 {
    let exponent = (v >> 10) & 31;
    let fraction = f32::from(v & 1023);
    match exponent {
        0 => fraction * 2f32.powi(-24),
        31 => f32::NAN,
        _ => (1.0 + fraction / 1024.0) * 2f32.powi(i32::from(exponent) - 15),
    }
}
fn linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}
#[test]
#[ignore = "requires a Vulkan GPU; set NS_NR_LOOK_GPU_VALIDATION=1 for strict validation"]
fn gpu_reconstruction_preserves_full_resolution_base_and_alpha() {
    unsafe {
        VALIDATION.store(0, Ordering::Relaxed);
        LOADER_NOTICES.store(0, Ordering::Relaxed);
        let gpu = Gpu::new();
        let d = &gpu.device;
        let physical = gpu.instance.enumerate_physical_devices().unwrap()[0];
        let memory = gpu.instance.get_physical_device_memory_properties(physical);
        let full = vk::Extent2D {
            width: 37,
            height: 5,
        };
        let reconstruct =
            crate::nr_reconstruct::Reconstruction::new(d, &memory, full, gpu.images[0]).unwrap();
        let base = reconstruct.base.unwrap();
        let count = (full.width * full.height) as usize;
        assert!(count * 8 <= (BYTES * 4) as usize);
        let original: Vec<[u16; 4]> = (0..count)
            .map(|i| match i % 5 {
                0 => [0x3400, 0x3a00, 0x3800, 0x3400],
                1 => [0x3a00, 0x3400, 0x3800, 0x3800],
                2 => [0, 0, 0, 0x3c00],
                3 => [1, 2, 1, 0x3400],
                _ => [0x3c00; 4],
            })
            .collect();
        for (iteration, nr) in [0.5, 0.75, 0.25, 1.0, 0.0].into_iter().enumerate() {
            let mapped = d
                .map_memory(gpu.memory, 0, BYTES * 4, vk::MemoryMapFlags::empty())
                .unwrap();
            std::ptr::copy_nonoverlapping(original.as_ptr(), mapped.cast::<[u16; 4]>(), count);
            d.unmap_memory(gpu.memory);
            d.reset_command_pool(gpu.pool, vk::CommandPoolResetFlags::empty())
                .unwrap();
            d.reset_fences(&[gpu.fence]).unwrap();
            d.begin_command_buffer(gpu.command, &vk::CommandBufferBeginInfo::default())
                .unwrap();
            for (r, clear) in [(gpu.images[0], 0.5), (gpu.images[1], nr)] {
                fg_api::transition(
                    d,
                    gpu.command,
                    vk::Image::from_raw(r.image),
                    if iteration == 0 {
                        vk::ImageLayout::UNDEFINED
                    } else {
                        vk::ImageLayout::GENERAL
                    },
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
                d.cmd_clear_color_image(
                    gpu.command,
                    vk::Image::from_raw(r.image),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &vk::ClearColorValue {
                        float32: [clear, clear, clear, 1.0],
                    },
                    &[fg_api::range()],
                );
                fg_api::transition(
                    d,
                    gpu.command,
                    vk::Image::from_raw(r.image),
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
            }
            let region = vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width: full.width,
                    height: full.height,
                    depth: 1,
                });
            fg_api::transition(
                d,
                gpu.command,
                vk::Image::from_raw(base.image),
                if iteration == 0 {
                    vk::ImageLayout::UNDEFINED
                } else {
                    vk::ImageLayout::GENERAL
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            d.cmd_copy_buffer_to_image(
                gpu.command,
                gpu.buffer,
                vk::Image::from_raw(base.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[region],
            );
            fg_api::transition(
                d,
                gpu.command,
                vk::Image::from_raw(base.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            let output = reconstruct.record(gpu.command, gpu.images[1], iteration != 0);
            fg_api::transition(
                d,
                gpu.command,
                vk::Image::from_raw(output.image),
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            d.cmd_pipeline_barrier(
                gpu.command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)],
                &[],
                &[],
            );
            d.cmd_copy_image_to_buffer(
                gpu.command,
                vk::Image::from_raw(output.image),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                gpu.buffer,
                &[region],
            );
            fg_api::transition(
                d,
                gpu.command,
                vk::Image::from_raw(output.image),
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            d.cmd_pipeline_barrier(
                gpu.command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::HOST,
                vk::DependencyFlags::empty(),
                &[vk::MemoryBarrier::default()
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::HOST_READ)],
                &[],
                &[],
            );
            d.end_command_buffer(gpu.command).unwrap();
            d.queue_submit(
                gpu.queue,
                &[vk::SubmitInfo::default().command_buffers(&[gpu.command])],
                gpu.fence,
            )
            .unwrap();
            d.wait_for_fences(&[gpu.fence], true, 10_000_000_000)
                .unwrap();
            let mapped = d
                .map_memory(gpu.memory, 0, BYTES * 4, vk::MemoryMapFlags::empty())
                .unwrap();
            let pixels = std::slice::from_raw_parts(mapped.cast::<[u16; 4]>(), count).to_vec();
            d.unmap_memory(gpu.memory);
            if nr == 0.5 {
                assert_eq!(
                    pixels, original,
                    "zero delta preserves fine detail and subnormal bits"
                );
            }
            for (p, o) in pixels.iter().zip(&original) {
                assert_eq!(p[3], o[3], "original alpha remains exact");
                for c in 0..3 {
                    let expected = (linear(half(o[c])) + linear(nr) - linear(0.5)).clamp(0.0, 1.0);
                    assert!(half(p[c]).is_finite());
                    assert!(
                        (linear(half(p[c])) - expected).abs() < 0.002,
                        "linear correction mismatch"
                    );
                }
            }
        }
        drop(reconstruct);
        drop(gpu);
        eprintln!(
            "Reconstruction validation warnings/errors: {}; loader installation notices: {}",
            VALIDATION.load(Ordering::Relaxed),
            LOADER_NOTICES.load(Ordering::Relaxed)
        );
        assert_eq!(VALIDATION.load(Ordering::Relaxed), 0);
    }
}
#[test]
#[ignore = "requires a Vulkan GPU; set NS_NR_LOOK_GPU_VALIDATION=1 for strict validation"]
fn gpu_look_fixed_inputs() {
    unsafe {
        VALIDATION.store(0, Ordering::Relaxed);
        LOADER_NOTICES.store(0, Ordering::Relaxed);
        let mut gpu = Gpu::new();
        let p = (0..PIXELS)
            .map(|i| match i % 6 {
                0 | 1 => [0x3800, 0x3800, 0x3800, 0x3c00], // grey .5
                2 => [0, 0, 0, 0x3c00],
                3 => [1, 2, 1, 0x3c00],                // subnormal dark
                4 => [0x3800, 0x3400, 0x3000, 0x3c00], // colored
                _ => [0x3c00; 4],
            })
            .collect::<Vec<_>>();
        let n = (0..PIXELS)
            .map(|i| match i % 6 {
                0 => [0x3a00, 0x3a00, 0x3a00, 0x3800],     // grey .75
                1 | 2 => [0x3400, 0x3400, 0x3400, 0x3800], // grey .25
                3 => [2, 1, 2, 0x3800],
                4 => [0x3400, 0x3800, 0x3400, 0x3800],
                _ => [0x3c00, 0x3c00, 0x3c00, 0x3800],
            })
            .collect::<Vec<_>>();
        assert_eq!(
            gpu.run(&p, &n, LookOptions::default()),
            n,
            "neutral must be bit-exact"
        );
        assert_eq!(
            gpu.run(
                &p,
                &n,
                LookOptions {
                    enabled: false,
                    amount: 200,
                    ..Default::default()
                }
            ),
            n
        );
        let source = gpu.run(
            &p,
            &n,
            LookOptions {
                amount: 0,
                ..Default::default()
            },
        );
        for i in 0..PIXELS {
            assert_eq!(&source[i][..3], &p[i][..3]);
            assert_eq!(source[i][3], n[i][3]);
        }
        // A successful second pass uses R1 as its Look baseline, not P0.
        let r2 = vec![[0x3600, 0x3200, 0x3800, 0x3400]; PIXELS];
        assert_eq!(gpu.run(&n, &r2, LookOptions::default()), r2);
        let second_source = gpu.run(
            &n,
            &r2,
            LookOptions {
                amount: 0,
                ..Default::default()
            },
        );
        for i in 0..PIXELS {
            assert_eq!(&second_source[i][..3], &n[i][..3]);
            assert_eq!(second_source[i][3], r2[i][3]);
        }
        assert_ne!(&second_source[0][..3], &p[0][..3]);
        let suppress = gpu.run(
            &p,
            &n,
            LookOptions {
                brighten: 0,
                darken: 0,
                ..Default::default()
            },
        );
        for i in (0..PIXELS).filter(|i| i % 6 <= 1) {
            assert!((half(suppress[i][0]) - 0.5).abs() < 0.001);
        }
        let cap = gpu.run(
            &p,
            &n,
            LookOptions {
                brighten_cap: 25,
                darken_cap: 25,
                amount: 200,
                midtones: 200,
                ..Default::default()
            },
        );
        for i in (0..PIXELS).filter(|i| i % 6 <= 1) {
            let stops = (linear(half(cap[i][0])) / linear(0.5)).log2();
            assert!(
                stops.abs() <= 0.26 && stops.abs() > 0.05,
                "soft cap: {stops}"
            );
        }
        for options in [
            LookOptions {
                amount: 200,
                color: 200,
                hue: 200,
                shadows: 200,
                ..Default::default()
            },
            LookOptions {
                color: 0,
                ..Default::default()
            },
            LookOptions {
                hue: 0,
                ..Default::default()
            },
            LookOptions {
                midtones: 0,
                highlights: 0,
                ..Default::default()
            },
        ] {
            let result = gpu.run(&p, &n, options);
            for (i, pixel) in result.iter().enumerate() {
                assert!(pixel
                    .iter()
                    .all(|&v| half(v).is_finite() && (0.0..=1.0).contains(&half(v))));
                assert_eq!(pixel[3], n[i][3]);
                if i % 6 == 2 || i % 6 == 3 {
                    assert_eq!(*pixel, n[i], "dark confidence must retain NR");
                }
            }
            assert_ne!(
                result[4], n[4],
                "chroma/zone control must affect the colored fixture"
            );
        }
        let invalid = vec![[0x7e00, 0x7c00, 0x7e00, 0x3800]; PIXELS];
        let unchanged = p
            .iter()
            .map(|v| [v[0], v[1], v[2], 0x3800])
            .collect::<Vec<_>>();
        assert_eq!(
            gpu.run(
                &p,
                &unchanged,
                LookOptions {
                    amount: 200,
                    color: 0,
                    hue: 0,
                    brighten_cap: 25,
                    ..Default::default()
                }
            ),
            unchanged,
            "zero model change must not introduce a Look change"
        );
        let recovered = gpu.run(
            &p,
            &invalid,
            LookOptions {
                amount: 50,
                ..Default::default()
            },
        );
        for i in 0..PIXELS {
            assert_eq!(&recovered[i][..3], &p[i][..3]);
            assert_eq!(recovered[i][3], invalid[i][3]);
        }
        assert!(
            gpu.look.as_ref().unwrap().spatial.is_none(),
            "basic Look must not allocate spatial textures"
        );
        let flat = vec![[0x3800, 0x3800, 0x3800, 0x3c00]; PIXELS];
        let bright = vec![[0x3a00, 0x3a00, 0x3a00, 0x3800]; PIXELS];
        let spatial = |lighting, detail, halo, radius| LookOptions {
            spatial: crate::advanced_settings::SpatialLook {
                enabled: true,
                lighting,
                detail,
                halo,
                radius,
            },
            ..Default::default()
        };
        assert_eq!(gpu.run(&flat, &bright, spatial(100, 100, 0, 32)), bright);
        assert!(
            gpu.look.as_ref().unwrap().spatial.is_none(),
            "neutral spatial parameters must bypass allocation"
        );
        // A failed spatial stage may retain basic Look, and must not retry on
        // every frame. Simulate the sticky preparation failure before creation.
        gpu.look.as_mut().unwrap().spatial_failure = Some("simulated allocation failure".into());
        assert_eq!(gpu.run(&flat, &bright, spatial(0, 100, 0, 4)), bright);
        assert!(!gpu.look.as_ref().unwrap().active(spatial(0, 100, 0, 4)));
        assert_eq!(
            gpu.look
                .as_ref()
                .unwrap()
                .spatial_status(spatial(0, 100, 0, 4))["reason"],
            "preparation_failed"
        );
        let memory = gpu.look.as_ref().unwrap().memory;
        drop(gpu.look.take());
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        let broad_off = gpu.run(&flat, &bright, spatial(0, 100, 0, 4));
        for v in broad_off {
            assert!(
                (half(v[0]) - 0.5).abs() < 0.002,
                "uniform broad light should be removed"
            );
        }
        let band_image = gpu
            .look
            .as_ref()
            .unwrap()
            .spatial
            .as_ref()
            .unwrap()
            .image
            .unwrap()
            .image;
        let detail_off = gpu.run(&flat, &bright, spatial(100, 0, 0, 4));
        for v in detail_off {
            assert!(
                (half(v[0]) - 0.75).abs() < 0.002,
                "detail suppression must retain uniform lighting"
            );
        }
        let peak = (0..PIXELS)
            .map(|i| {
                if i % 19 == 9 {
                    [0x3a00, 0x3a00, 0x3a00, 0x3800]
                } else {
                    [0x3800; 4]
                }
            })
            .collect::<Vec<_>>();
        let softened = gpu.run(&flat, &peak, spatial(100, 0, 0, 4));
        for i in 0..PIXELS {
            if i % 19 == 9 {
                assert!(
                    (0.5..0.68).contains(&half(softened[i][0])),
                    "fine peak must be reduced"
                );
            } else {
                assert_eq!(
                    softened[i], peak[i],
                    "unchanged pixels must not gain a glow"
                );
            }
        }
        let edge = (0..PIXELS)
            .map(|i| {
                if i % 19 < 9 {
                    [0x3400, 0x3400, 0x3400, 0x3c00]
                } else {
                    [0x3a00, 0x3a00, 0x3a00, 0x3c00]
                }
            })
            .collect::<Vec<_>>();
        let halo = edge
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i % 19 == 9 {
                    [0x3800; 4]
                } else {
                    [p[0], p[1], p[2], 0x3800]
                }
            })
            .collect::<Vec<_>>();
        let suppressed = gpu.run(&edge, &halo, spatial(100, 100, 100, 4));
        for i in 0..PIXELS {
            if i % 19 == 9 {
                assert!(
                    half(suppressed[i][0]) > 0.55 && half(suppressed[i][0]) <= 0.75,
                    "bright-side dark halo should weaken without bright overshoot"
                );
            } else {
                assert_eq!(
                    suppressed[i], halo[i],
                    "halo processing must not create a new outline"
                );
            }
        }
        let text = edge
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i % 19 == 8 {
                    [0x3000, 0x3000, 0x3000, 0x3800]
                } else {
                    [p[0], p[1], p[2], 0x3800]
                }
            })
            .collect::<Vec<_>>();
        let text_result = gpu.run(&edge, &text, spatial(100, 100, 100, 4));
        for i in (0..PIXELS).filter(|i| i % 19 == 8) {
            assert!(
                (half(text_result[i][0]) - 0.125).abs() < 0.002,
                "dark-side text must retain its darkening"
            );
        }
        let shadow = edge
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if i % 19 >= 9 {
                    [0x3800; 4]
                } else {
                    [p[0], p[1], p[2], 0x3800]
                }
            })
            .collect::<Vec<_>>();
        let shadow_result = gpu.run(&edge, &shadow, spatial(100, 100, 100, 4));
        for i in (0..PIXELS).filter(|i| i % 19 >= 9) {
            assert!(
                (half(shadow_result[i][0]) - 0.5).abs() < 0.002,
                "a broad real shadow is not a halo"
            );
        }
        let invalid_spatial = gpu.run(&p, &invalid, spatial(0, 200, 100, 32));
        for i in 0..PIXELS {
            assert_eq!(&invalid_spatial[i][..3], &p[i][..3]);
        }
        for radius in [1, 32, 4] {
            let result = gpu.run(&edge, &halo, spatial(200, 200, 100, radius));
            assert!(result.iter().flatten().all(|&v| half(v).is_finite()));
            assert_eq!(
                gpu.look
                    .as_ref()
                    .unwrap()
                    .spatial
                    .as_ref()
                    .unwrap()
                    .image
                    .unwrap()
                    .image,
                band_image,
                "radius updates must reuse the band texture"
            );
            assert_eq!(
                gpu.run(&flat, &bright, LookOptions::default()),
                bright,
                "returning to neutral is bit-exact after spatial dispatches"
            );
        }
        let status = gpu
            .look
            .as_ref()
            .unwrap()
            .spatial_status(spatial(100, 100, 100, 4));
        assert_eq!(status["textureCount"], 1);
        assert!(status["allocationBytes"].as_u64().unwrap() >= BYTES);
        // Destroy and recreate at a fenced boundary; the first dispatch must
        // initialize a fresh band texture rather than reuse its previous layout.
        drop(gpu.look.take());
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        assert_eq!(gpu.run(&edge, &halo, spatial(100, 100, 100, 4)), suppressed);
        // Compare the actual FP16 input contract, rather than an NVOF proxy.
        // The right/bottom edge and one-bit/alpha changes must all invalidate
        // the cache; reset the flag buffer on every dispatch.
        gpu.difference = Some(
            crate::nr_input_difference::Difference::new(
                &gpu.device,
                &memory,
                gpu.images[0],
                gpu.images[1],
            )
            .unwrap(),
        );
        let compare_input = vec![[0x3800, 0x3400, 0x3000, 0x3c00]; PIXELS];
        gpu.run(&compare_input, &compare_input, LookOptions::default());
        assert!(gpu.difference.as_ref().unwrap().identical());
        for (pixel, channel) in [(0, 0), (18, 1), (PIXELS - 1, 2), (PIXELS - 1, 3)] {
            let mut changed = compare_input.clone();
            changed[pixel][channel] += 1;
            gpu.run(&compare_input, &changed, LookOptions::default());
            assert!(
                !gpu.difference.as_ref().unwrap().identical(),
                "one FP16 bit must invalidate {pixel}:{channel}"
            );
            gpu.run(&changed, &changed, LookOptions::default());
            assert!(
                gpu.difference.as_ref().unwrap().identical(),
                "comparison flags must clear on every dispatch"
            );
        }
        let mut signed_zero = compare_input.clone();
        signed_zero[0][0] = 0;
        let mut negative_zero = signed_zero.clone();
        negative_zero[0][0] = 0x8000;
        gpu.run(&signed_zero, &negative_zero, LookOptions::default());
        assert!(!gpu.difference.as_ref().unwrap().identical());
        for nonfinite in [0x7c00, 0x7e00] {
            let mut invalid = compare_input.clone();
            invalid[PIXELS - 1][0] = nonfinite;
            gpu.run(&invalid, &invalid, LookOptions::default());
            assert!(
                !gpu.difference.as_ref().unwrap().identical(),
                "nonfinite input cannot be cached"
            );
        }
        drop(gpu.difference.take());
        gpu.difference = Some(
            crate::nr_input_difference::Difference::new(
                &gpu.device,
                &memory,
                gpu.images[0],
                gpu.images[1],
            )
            .unwrap(),
        );
        gpu.run(&compare_input, &compare_input, LookOptions::default());
        assert!(gpu.difference.as_ref().unwrap().identical());
        // P5: real SPIR-V history, with deterministic source-observation times.
        drop(gpu.difference.take());
        drop(gpu.look.take());
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        let mut temporal = LookOptions::default();
        temporal.temporal.enabled = true;
        let source = vec![[0x3800, 0x3800, 0x3800, 0x3c00]; PIXELS];
        let high = vec![[0x3a00, 0x3a00, 0x3a00, 0x3800]; PIXELS];
        let lower = vec![[0x3900, 0x3900, 0x3900, 0x3400]; PIXELS];
        assert_eq!(
            gpu.run(&source, &high, temporal),
            high,
            "no valid motion/context means bypass"
        );
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["textureCount"],
            0
        );
        gpu.motion = Some(
            fg_api::texture_with_usage(
                &gpu.device,
                &memory,
                SIZE,
                vk::Format::R32G32_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .unwrap(),
        );
        let context = |id, ms, reset| crate::nr_look_history::Context {
            source_frame_id: id,
            now_ms: ms,
            reset,
            uv_scale: [1.0; 2],
        };
        gpu.temporal_context = Some(context(1, 100, false));
        assert_eq!(
            gpu.run(&source, &high, temporal),
            high,
            "first temporal frame is bit-exact NR"
        );
        let bytes = gpu.look.as_ref().unwrap().temporal_status(temporal)["allocationBytes"]
            .as_u64()
            .unwrap();
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["textureCount"],
            3
        );
        let mut changed_source = source.clone();
        changed_source[PIXELS - 1][0] += 1;
        gpu.temporal_context = Some(context(2, 116, false));
        let smooth = gpu.run(&changed_source, &lower, temporal);
        assert!(
            half(smooth[20][0]) > half(lower[20][0]) && half(smooth[20][0]) < half(high[20][0]),
            "history reduces downward delta flicker without exceeding neighborhood limits"
        );
        assert_eq!(smooth[20][3], lower[20][3]);
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["maximumHistoryWeight"],
            0.75
        );
        gpu.temporal_context = Some(context(3, 500, false));
        assert_eq!(
            gpu.run(&source, &lower, temporal),
            lower,
            "long interval rejects history"
        );
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["resetReason"],
            "long_interval"
        );
        gpu.temporal_context = Some(context(4, 516, true));
        assert_eq!(
            gpu.run(&source, &high, temporal),
            high,
            "upstream reset seeds Look alone"
        );
        let cut = vec![[0x3400, 0x3400, 0x3400, 0x3c00]; PIXELS];
        gpu.temporal_context = Some(context(5, 532, false));
        assert_eq!(
            gpu.run(&cut, &lower, temporal),
            lower,
            "photometric cut rejects history"
        );
        gpu.temporal_context = Some(context(6, 548, true));
        gpu.run(&source, &high, temporal);
        gpu.motion_value = [f32::NAN, 0.0];
        gpu.temporal_context = Some(context(7, 564, false));
        assert_eq!(
            gpu.run(&changed_source, &lower, temporal),
            lower,
            "nonfinite motion rejects history"
        );
        gpu.motion_value = [0.5, 0.0];
        gpu.temporal_context = Some(context(8, 580, false));
        assert_eq!(
            gpu.run(&source, &lower, temporal),
            lower,
            "large motion rejects history"
        );
        gpu.motion_value = [0.0; 2];
        gpu.temporal_context = Some(context(9, 596, true));
        let mut stripes = lower.clone();
        for y in 0..SIZE.height as usize {
            stripes[y * SIZE.width as usize + 7] = high[0];
        }
        gpu.run(&source, &stripes, temporal);
        gpu.motion_value = [-0.5 / SIZE.width as f32, 0.0];
        let mut cropped = context(10, 612, false);
        cropped.uv_scale = [2.0, 1.0];
        gpu.temporal_context = Some(cropped);
        let translated = gpu.run(&changed_source, &lower, temporal);
        assert!(
            half(translated[SIZE.width as usize + 8][0]) > half(lower[0][0]),
            "current-to-previous sign and crop UV scale"
        );
        assert_eq!(
            translated[SIZE.width as usize + 6][0],
            lower[0][0],
            "opposite direction must not receive stripe history"
        );
        assert_eq!(
            translated[0], lower[0],
            "out-of-bounds reprojection preserves NR"
        );
        gpu.motion_value = [0.0; 2];
        gpu.temporal_context = Some(context(11, 628, true));
        gpu.run(&source, &stripes, temporal);
        gpu.motion_value = [-0.25 / SIZE.width as f32, 0.0];
        gpu.temporal_context = Some(context(12, 644, false));
        let fractional = gpu.run(&changed_source, &lower, temporal);
        let sample = SIZE.width as usize + 8;
        assert!(
            half(fractional[sample][0]) > half(lower[0][0])
                && half(fractional[sample][0]) < half(translated[sample][0]),
            "bilinear subpixel reprojection retains a fractional stripe response"
        );
        gpu.motion_value = [0.0; 2];
        gpu.temporal_context = Some(context(13, 660, true));
        let mut invalid_history = high.clone();
        invalid_history[20][0] = 0x7e00;
        gpu.run(&source, &invalid_history, temporal);
        gpu.temporal_context = Some(context(14, 676, false));
        assert_eq!(
            gpu.run(&changed_source, &lower, temporal)[20],
            lower[20],
            "invalid raw model history must be rejected"
        );
        gpu.temporal_context = Some(context(15, 692, true));
        temporal.spatial.enabled = true;
        temporal.spatial.detail = 50;
        let combined = gpu.run(&source, &stripes, temporal);
        assert!(
            combined.iter().flatten().all(|v| v & 0x7c00 != 0x7c00),
            "temporal plus spatial remains finite"
        );
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["allocationBytes"],
            bytes
        );
        gpu.temporal_context = Some(context(16, 708, false));
        let mut invalid = high.clone();
        invalid[20][0] = 0x7e00;
        let rejected = gpu.run(&source, &invalid, temporal);
        assert_eq!(rejected[20][0], source[20][0]);
        assert!(rejected.iter().flatten().all(|v| v & 0x7c00 != 0x7c00));
        assert_eq!(
            gpu.run(&source, &lower, LookOptions::default()),
            lower,
            "disabling temporal restores exact neutral output"
        );
        drop(gpu.look.take());
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        gpu.temporal_context = Some(context(17, 724, false));
        temporal.spatial = Default::default();
        assert_eq!(
            gpu.run(&source, &lower, temporal),
            lower,
            "recreated histories begin uninitialized"
        );
        drop(gpu.look.take());
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        assert!(gpu
            .look
            .as_mut()
            .unwrap()
            .prepare_temporal(temporal, gpu.images[0])
            .is_err());
        assert_eq!(
            gpu.run(&source, &lower, temporal),
            lower,
            "failed history preparation preserves basic neutral NR"
        );
        let failed = gpu.look.as_ref().unwrap().temporal_status(temporal);
        assert_eq!(failed["active"], false);
        assert_eq!(failed["textureCount"], 0);
        assert!(
            failed["error"].is_string(),
            "preparation failure is sticky despite subsequent valid motion"
        );
        // Production NVOF-to-NR UV guides use RG16F, unlike the earlier fixture.
        // Retire old descriptors before replacing the guide, then exercise the
        // shipped FP16 shader with real image views and history transitions.
        drop(gpu.look.take());
        let old_motion = gpu.motion.take().unwrap();
        gpu.device
            .destroy_image_view(vk::ImageView::from_raw(old_motion.view), None);
        gpu.device
            .destroy_image(vk::Image::from_raw(old_motion.image), None);
        gpu.device
            .free_memory(vk::DeviceMemory::from_raw(old_motion.memory), None);
        gpu.motion = Some(
            fg_api::texture_with_usage(
                &gpu.device,
                &memory,
                SIZE,
                vk::Format::R16G16_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .unwrap(),
        );
        gpu.motion_initialized = false;
        gpu.motion_value = [0.0; 2];
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        gpu.temporal_context = Some(context(1, 100, true));
        assert_eq!(gpu.run(&source, &high, temporal), high);
        gpu.temporal_context = Some(context(2, 116, false));
        let fp16_smoothed = gpu.run(&source, &lower, temporal);
        assert!(fp16_smoothed[20][0] > lower[20][0]);
        assert!(fp16_smoothed[20][0] < high[20][0]);
        assert_eq!(fp16_smoothed[20][3], lower[20][3]);
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["active"],
            true
        );
        gpu.motion_value = [0.5, 0.0];
        gpu.temporal_context = Some(context(3, 132, false));
        assert_eq!(
            gpu.run(&source, &lower, temporal),
            lower,
            "FP16 large motion rejects history"
        );
        gpu.motion_value = [0.0; 2];
        gpu.temporal_context = Some(context(4, 500, false));
        assert_eq!(
            gpu.run(&source, &lower, temporal),
            lower,
            "FP16 long gap resets history"
        );
        // M1: P0, R1 and R2 are separate resources. Scope changes the delta
        // base, while temporal validation continues reading unmodified P0.
        drop(gpu.look.take());
        gpu.guide_pixels = Some(source.clone());
        gpu.look = Some(
            Look::with_guide(
                &gpu.device,
                memory,
                gpu.images[0],
                gpu.images[1],
                gpu.images[2],
            )
            .unwrap(),
        );
        let first_output = lower.clone();
        let mut zero = LookOptions {
            amount: 0,
            ..Default::default()
        };
        assert_eq!(
            gpu.run(&first_output, &high, zero),
            first_output
                .iter()
                .zip(&high)
                .map(|(p, n)| [p[0], p[1], p[2], n[3]])
                .collect::<Vec<_>>()
        );
        zero.scope = crate::advanced_settings::LookScope::ChainTotal;
        gpu.look.as_mut().unwrap().set_input(gpu.images[2]);
        assert_eq!(
            gpu.run(&first_output, &high, zero),
            source
                .iter()
                .zip(&high)
                .map(|(p, n)| [p[0], p[1], p[2], n[3]])
                .collect::<Vec<_>>()
        );
        zero.amount = 100;
        assert_eq!(
            gpu.run(&first_output, &high, zero),
            high,
            "chain neutral is bit-exact R2"
        );
        zero.enabled = false;
        assert_eq!(
            gpu.run(&first_output, &high, zero),
            high,
            "chain bypass is bit-exact R2"
        );
        gpu.look.as_mut().unwrap().set_input(gpu.images[0]);
        temporal.spatial = Default::default();
        gpu.temporal_context = Some(context(1, 100, true));
        gpu.run(&source, &high, temporal);
        gpu.temporal_context = Some(context(2, 116, false));
        let varied_r1 = vec![[0x3400, 0x3400, 0x3400, 0x3c00]; PIXELS];
        let r2 = vec![[0x3600, 0x3600, 0x3600, 0x3800]; PIXELS];
        let original_guided = gpu.run(&varied_r1, &r2, temporal);
        assert!(
            original_guided[20][0] > r2[20][0],
            "R1 variation must not reject stable P0 history"
        );
        gpu.guide_pixels = Some(varied_r1.clone());
        gpu.temporal_context = Some(context(3, 132, false));
        assert_eq!(
            gpu.run(&varied_r1, &r2, temporal),
            r2,
            "a real P0 change rejects old correction"
        );
        // M2: static history works without a motion image and checks nearby
        // edge changes, not just the center pixel. Switching modes retires it.
        drop(gpu.look.take());
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        let saved_motion = gpu.motion.take();
        gpu.guide_pixels = None;
        temporal.temporal.mode = crate::advanced_settings::TemporalMode::Static;
        gpu.temporal_context = Some(context(1, 100, true));
        assert_eq!(gpu.run(&source, &high, temporal), high);
        gpu.temporal_context = Some(context(2, 116, false));
        assert!(gpu.run(&source, &lower, temporal)[20][0] > lower[20][0]);
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["actualMode"],
            "static"
        );
        let mut moving_edge = source.clone();
        moving_edge[21] = [0x3400; 4];
        gpu.temporal_context = Some(context(3, 132, false));
        assert_eq!(
            gpu.run(&moving_edge, &lower, temporal)[20],
            lower[20],
            "static local edge change rejects center history"
        );
        gpu.motion = saved_motion;
        temporal.temporal.mode = crate::advanced_settings::TemporalMode::OpticalFlow;
        temporal.temporal.sampling = crate::advanced_settings::HistorySampling::PerTap;
        gpu.temporal_context = Some(context(4, 148, false));
        assert_eq!(
            gpu.run(&source, &high, temporal),
            high,
            "mode switch seeds a fresh history"
        );
        gpu.motion_value = [-0.25 / SIZE.width as f32, 0.0];
        gpu.temporal_context = Some(context(5, 164, false));
        let per_tap = gpu.run(&source, &lower, temporal);
        assert!(per_tap[20][0] > lower[20][0]);
        assert_eq!(
            per_tap[0], lower[0],
            "per-tap out-of-domain history is rejected"
        );
        gpu.motion_value = [0.0; 2];
        gpu.temporal_context = Some(context(6, 180, true));
        let mut mixed_history = high.clone();
        mixed_history[19][0] = 0x7e00;
        gpu.run(&source, &mixed_history, temporal);
        gpu.motion_value = [-0.25 / SIZE.width as f32, 0.0];
        gpu.temporal_context = Some(context(7, 196, false));
        assert!(
            gpu.run(&source, &lower, temporal)[20][0] > lower[20][0],
            "a rejected quarter tap must not poison valid support"
        );
        gpu.motion_value = [0.0; 2];
        gpu.temporal_context = Some(context(8, 212, true));
        mixed_history = high.clone();
        mixed_history[20][0] = 0x7e00;
        gpu.run(&source, &mixed_history, temporal);
        gpu.motion_value = [-0.1 / SIZE.width as f32, 0.0];
        gpu.temporal_context = Some(context(9, 228, false));
        assert_eq!(
            gpu.run(&source, &lower, temporal)[20],
            lower[20],
            "tiny valid support must not be amplified"
        );
        // M3: repeatedly tune the existing GPU output without uploading R2,
        // evaluating NR or adding a temporal observation.
        gpu.motion_value = [0.0; 2];
        gpu.temporal_context = Some(context(10, 244, true));
        gpu.run(&source, &high, temporal);
        gpu.temporal_context = Some(context(11, 260, false));
        let stabilized = gpu.run(&source, &lower, temporal);
        let history_before = gpu.look.as_ref().unwrap().temporal_status(temporal);
        gpu.recomposing = true;
        let mut tuned = temporal;
        tuned.amount = 50;
        let controlled = gpu.run(&source, &lower, tuned);
        assert_ne!(controlled, stabilized);
        for _ in 0..12 {
            assert_eq!(
                gpu.run(&source, &lower, tuned),
                controlled,
                "recomposition must not compound a previous gain"
            );
        }
        assert_eq!(
            gpu.run(&source, &lower, temporal),
            stabilized,
            "returning to original controls reuses the same delta"
        );
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal),
            history_before,
            "recomposition cannot advance the observation clock/history"
        );
        assert_eq!(
            gpu.run(&source, &lower, LookOptions::default()),
            lower,
            "neutral restores pristine model output"
        );
        gpu.recomposing = false;
        // Same-source model recomputation snapshots fresh NR, clears the delta
        // history and never publishes another Look observation.
        gpu.temporal_context = None;
        assert_eq!(gpu.run(&source, &high, temporal), high);
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["historyReady"],
            false
        );
        gpu.temporal_context = Some(context(12, 276, false));
        assert_eq!(gpu.run(&source, &lower, temporal), lower);
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["resetReason"],
            "first_frame"
        );
        // Persistence is opt-in and has a distinct conditional amplitude and
        // support history. A vanished model correction fades instead of adding
        // zero observations to its conditional magnitude.
        temporal.temporal.mode = crate::advanced_settings::TemporalMode::OpticalFlowPlus;
        gpu.temporal_context = Some(context(20, 500, true));
        assert_eq!(gpu.run(&source, &high, temporal), high);
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["textureCount"],
            4
        );
        gpu.temporal_context = Some(context(21, 516, false));
        let missing = gpu.run(&source, &source, temporal);
        assert!(missing[20][0] > source[20][0] && missing[20][0] < high[20][0]);
        assert_eq!(missing[20][3], source[20][3]);
        let mut previous = missing[20][0];
        for id in 22..=110 {
            gpu.temporal_context = Some(context(id, 516 + (id - 21) * 16, false));
            let absent = gpu.run(&source, &source, temporal);
            assert!(
                absent[20][0] <= previous,
                "long absence must decay without revival"
            );
            previous = absent[20][0];
        }
        assert_eq!(previous, source[20][0]);
        gpu.temporal_context = Some(context(111, 1956, false));
        let appearing = gpu.run(&source, &high, temporal);
        assert!(appearing[20][0] > source[20][0] && appearing[20][0] < high[20][0]);
        gpu.temporal_context = Some(context(112, 1972, false));
        assert_eq!(
            gpu.run(&source, &cut, temporal),
            cut,
            "opposite corrections reject old support"
        );
        gpu.temporal_context = Some(context(113, 1988, true));
        gpu.run(&source, &high, temporal);
        gpu.temporal_context = Some(context(114, 2004, false));
        assert_eq!(
            gpu.run(&cut, &cut, temporal),
            cut,
            "real color changes discard persistent correction"
        );
        gpu.temporal_context = Some(context(115, 2020, true));
        gpu.run(&source, &high, temporal);
        gpu.motion_value = [f32::NAN, 0.0];
        gpu.temporal_context = Some(context(116, 2036, false));
        assert_eq!(
            gpu.run(&source, &source, temporal),
            source,
            "invalid motion discards support immediately"
        );
        gpu.motion_value = [0.0; 2];
        gpu.temporal_context = Some(context(117, 2052, true));
        gpu.run(&source, &high, temporal);
        gpu.temporal_context = Some(context(118, 2068, false));
        let persisted = gpu.run(&source, &source, temporal);
        let persistent_before = gpu.look.as_ref().unwrap().temporal_status(temporal);
        gpu.recomposing = true;
        assert_eq!(gpu.run(&source, &source, temporal), persisted);
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal),
            persistent_before
        );
        assert_eq!(gpu.run(&source, &source, LookOptions::default()), source);
        gpu.recomposing = false;
        temporal.temporal.mode = crate::advanced_settings::TemporalMode::OpticalFlow;
        gpu.temporal_context = Some(context(119, 2084, false));
        assert_eq!(
            gpu.run(&source, &high, temporal),
            high,
            "leaving persistence resets ordinary history"
        );
        assert_eq!(
            gpu.look.as_ref().unwrap().temporal_status(temporal)["textureCount"],
            3
        );
        // M4: color experiments stay opt-in, with exact neutral/zero/bypass.
        gpu.temporal_context = None;
        let mut experiments = LookOptions::default();
        experiments.algorithm = crate::advanced_settings::ColorAlgorithm::Oklab;
        assert_eq!(gpu.run(&p, &n, experiments), n);
        experiments.amount = 0;
        let zero = gpu.run(&p, &n, experiments);
        for i in 0..PIXELS {
            assert_eq!(&zero[i][..3], &p[i][..3]);
            assert_eq!(zero[i][3], n[i][3]);
        }
        experiments.amount = 200;
        experiments.color = 200;
        experiments.hue = 200;
        let lab = gpu.run(&p, &n, experiments);
        assert!(lab.iter().flatten().all(|v| v & 0x7c00 != 0x7c00));
        for i in 0..PIXELS {
            assert_eq!(lab[i][3], n[i][3]);
        }
        experiments.enabled = false;
        assert_eq!(gpu.run(&p, &n, experiments), n);
        experiments = LookOptions::default();
        experiments.protection.hue = 100;
        let hue_protected = gpu.run(&p, &n, experiments);
        let log_chroma = |rgb: [u16; 4]| {
            let v = rgb.map(|v| linear(half(v)).max(1e-6).log2());
            [v[0] - v[1], v[2] - v[1]]
        };
        let basis = log_chroma(p[4]);
        let original = log_chroma(n[4]);
        let protected = log_chroma(hue_protected[4]);
        assert!(
            (basis[0] * protected[1] - basis[1] * protected[0]).abs()
                < (basis[0] * original[1] - basis[1] * original[0]).abs()
        );
        assert!(
            basis[0] * protected[0] + basis[1] * protected[1] >= -0.01,
            "hue protection also rejects a radial color reversal"
        );
        experiments.protection = Default::default();
        experiments.protection.overcorrection = 100;
        let protected = gpu.run(&source, &high, experiments);
        assert!(protected[20][0] > source[20][0] && protected[20][0] < high[20][0]);
        experiments.protection = Default::default();
        experiments.protection.shadows = 100;
        let dark = vec![[0x3000, 0x3000, 0x3000, 0x3400]; PIXELS];
        let protected_dark = gpu.run(&dark, &lower, experiments);
        assert_eq!(&protected_dark[20][..3], &dark[20][..3]);
        assert_eq!(protected_dark[20][3], lower[20][3]);
        experiments.protection = Default::default();
        experiments.protection.highlights = 100;
        let white = vec![[0x3c00; 4]; PIXELS];
        assert_eq!(
            &gpu.run(&white, &lower, experiments)[20][..3],
            &white[20][..3]
        );
        experiments = LookOptions::default();
        for view in [
            crate::advanced_settings::LookDiagnostic::Original,
            crate::advanced_settings::LookDiagnostic::FirstPass,
            crate::advanced_settings::LookDiagnostic::ModelOutput,
        ] {
            experiments.diagnostic = view;
            let shown = gpu.run(&source, &high, experiments);
            let expected = if view == crate::advanced_settings::LookDiagnostic::Original {
                &source
            } else {
                &high
            };
            for i in 0..PIXELS {
                assert_eq!(&shown[i][..3], &expected[i][..3]);
                assert_eq!(shown[i][3], high[i][3]);
            }
        }
        for view in [
            crate::advanced_settings::LookDiagnostic::RawDelta,
            crate::advanced_settings::LookDiagnostic::ControlledDelta,
            crate::advanced_settings::LookDiagnostic::LowFrequency,
            crate::advanced_settings::LookDiagnostic::HighFrequency,
            crate::advanced_settings::LookDiagnostic::Protection,
            crate::advanced_settings::LookDiagnostic::HistoryValidity,
            crate::advanced_settings::LookDiagnostic::HistoryWeight,
        ] {
            experiments.diagnostic = view;
            assert!(gpu
                .run(&source, &high, experiments)
                .iter()
                .flatten()
                .all(|v| v & 0x7c00 != 0x7c00));
        }
        experiments.diagnostic = crate::advanced_settings::LookDiagnostic::ModelOutput;
        drop(gpu.look.take());
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        gpu.look.as_mut().unwrap().spatial_failure = Some("injected unavailable band stage".into());
        experiments.diagnostic = crate::advanced_settings::LookDiagnostic::LowFrequency;
        assert_eq!(
            gpu.run(&source, &high, experiments),
            high,
            "unavailable diagnostic band falls back without touching NR"
        );
        assert!(!gpu.look.as_ref().unwrap().active(experiments));
        gpu.look.as_mut().unwrap().spatial_failure = None;
        experiments.diagnostic = crate::advanced_settings::LookDiagnostic::ModelOutput;
        gpu.run(&source, &high, experiments);
        gpu.recomposing = true;
        assert_eq!(
            gpu.run(&source, &high, LookOptions::default()),
            high,
            "leaving diagnostics restores raw NR without another observation"
        );
        gpu.recomposing = false;
        drop(gpu.look.take());
        gpu.look = Some(
            Look::with_guide(
                &gpu.device,
                memory,
                gpu.images[0],
                gpu.images[1],
                gpu.images[2],
            )
            .unwrap(),
        );
        gpu.look.as_mut().unwrap().set_first_output(gpu.images[0]);
        gpu.look.as_mut().unwrap().set_input(gpu.images[2]);
        gpu.guide_pixels = Some(source.clone());
        experiments.scope = crate::advanced_settings::LookScope::ChainTotal;
        for (view, expected) in [
            (crate::advanced_settings::LookDiagnostic::Original, &source),
            (
                crate::advanced_settings::LookDiagnostic::FirstPass,
                &first_output,
            ),
            (crate::advanced_settings::LookDiagnostic::ModelOutput, &r2),
        ] {
            experiments.diagnostic = view;
            let shown = gpu.run(&first_output, &r2, experiments);
            for i in 0..PIXELS {
                assert_eq!(&shown[i][..3], &expected[i][..3]);
                assert_eq!(shown[i][3], r2[i][3]);
            }
        }
        drop(gpu.look.take());
        let fp16 = gpu.motion.take().unwrap();
        gpu.device
            .destroy_image_view(vk::ImageView::from_raw(fp16.view), None);
        gpu.device
            .destroy_image(vk::Image::from_raw(fp16.image), None);
        gpu.device
            .free_memory(vk::DeviceMemory::from_raw(fp16.memory), None);
        gpu.motion = Some(
            fg_api::texture_with_usage(
                &gpu.device,
                &memory,
                SIZE,
                vk::Format::R32G32_SFLOAT,
                vk::ImageUsageFlags::STORAGE | vk::ImageUsageFlags::TRANSFER_DST,
            )
            .unwrap(),
        );
        gpu.motion_initialized = false;
        gpu.guide_pixels = None;
        gpu.look = Some(Look::new(&gpu.device, memory, gpu.images[0], gpu.images[1]).unwrap());
        temporal.temporal.mode = crate::advanced_settings::TemporalMode::OpticalFlowPlus;
        gpu.temporal_context = Some(context(1, 100, true));
        assert_eq!(gpu.run(&source, &high, temporal), high);
        gpu.temporal_context = Some(context(2, 116, false));
        assert!(
            gpu.run(&source, &source, temporal)[20][0] > source[20][0],
            "RG32F persistence variant also retains valid missing support"
        );
        drop(gpu);
        eprintln!(
            "Look validation warnings/errors: {}; loader installation notices: {}",
            VALIDATION.load(Ordering::Relaxed),
            LOADER_NOTICES.load(Ordering::Relaxed)
        );
        assert_eq!(
            VALIDATION.load(Ordering::Relaxed),
            0,
            "GPU validation must be clean"
        );
    }
}
