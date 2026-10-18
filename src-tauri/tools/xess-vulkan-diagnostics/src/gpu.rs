use crate::{api, Result};
use ash::{vk, Entry};
use serde_json::json;
use std::ffi::{c_char, c_void, CStr};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};

pub static VALIDATION_ERRORS: AtomicUsize = AtomicUsize::new(0);
pub static VALIDATION_WARNINGS: AtomicUsize = AtomicUsize::new(0);
pub static VALIDATION_MESSAGES: Mutex<Vec<String>> = Mutex::new(Vec::new());
pub static LOADER_NOTICES: Mutex<Vec<String>> = Mutex::new(Vec::new());
unsafe extern "system" fn callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _: *mut c_void,
) -> vk::Bool32 {
    let msg = CStr::from_ptr((*data).p_message)
        .to_string_lossy()
        .into_owned();
    // Exact loader isolation notices, not validation findings. Preserve them in
    // the report; never suppress any other warning or error.
    if severity == vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
        && kind == vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
        && (*data).message_id_number == 0
        && ["VK_LAYER_NV_optimus", "VK_LAYER_NV_present", "VK_LAYER_AMD_switchable_graphics", "VK_LAYER_reshade"]
            .iter().any(|name| msg == format!("Layer \"{name}\" forced disabled because name matches filter of env var 'VK_LOADER_LAYERS_DISABLE'."))
    {
        if let Ok(mut notices) = LOADER_NOTICES.lock() { notices.push(msg); }
        return vk::FALSE;
    }
    if severity.contains(vk::DebugUtilsMessageSeverityFlagsEXT::ERROR) {
        VALIDATION_ERRORS.fetch_add(1, Ordering::Relaxed);
    } else {
        VALIDATION_WARNINGS.fetch_add(1, Ordering::Relaxed);
    }
    eprintln!("Vulkan {severity:?}: {msg}");
    if let Ok(mut messages) = VALIDATION_MESSAGES.lock() {
        if messages.len() < 100 {
            messages.push(msg);
        }
    }
    vk::FALSE
}
pub struct Gpu {
    _entry: Entry,
    pub instance: ash::Instance,
    pub physical: vk::PhysicalDevice,
    pub device: ash::Device,
    pub family: u32,
    pub queue: vk::Queue,
    pub memory: vk::PhysicalDeviceMemoryProperties,
    pub props: vk::PhysicalDeviceProperties,
    pub timestamp_bits: u32,
    pub pool: vk::CommandPool,
    pub cmd: vk::CommandBuffer,
    pub fence: vk::Fence,
    pub queries: vk::QueryPool,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    pub requirements: serde_json::Value,
}
impl Gpu {
    pub unsafe fn new(api: &api::Api, validation: bool, adapter: usize) -> Result<Self> {
        let entry = Entry::load()?;
        let mut count = 0;
        let mut names = std::ptr::null();
        let mut version = 0;
        api::check(
            (api.instance_extensions)(&mut count, &mut names, &mut version),
            "instance requirements",
        )?;
        let mut extensions = if count == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(names, count as usize).to_vec()
        };
        let layers = if validation {
            vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
        } else {
            Vec::new()
        };
        if validation {
            extensions.push(ash::ext::debug_utils::NAME.as_ptr());
        }
        let app = vk::ApplicationInfo::default()
            .application_name(c"XeSS isolated Vulkan diagnostics")
            .api_version(version.max(vk::API_VERSION_1_1));
        let enables = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
        let mut validation_features =
            vk::ValidationFeaturesEXT::default().enabled_validation_features(&enables);
        let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(callback));
        let mut info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_extension_names(&extensions)
            .enabled_layer_names(&layers);
        if validation {
            info = info
                .push_next(&mut validation_features)
                .push_next(&mut debug_info);
        }
        let instance = entry.create_instance(&info, None)?;
        let debug = if validation {
            let d = ash::ext::debug_utils::Instance::new(&entry, &instance);
            let m = d.create_debug_utils_messenger(&debug_info, None)?;
            Some((d, m))
        } else {
            None
        };
        let physical = *instance
            .enumerate_physical_devices()?
            .get(adapter)
            .ok_or("GPU index out of range")?;
        let props = instance.get_physical_device_properties(physical);
        let families = instance.get_physical_device_queue_family_properties(physical);
        let family = families
            .iter()
            .position(|p| {
                p.queue_flags
                    .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
                    && p.timestamp_valid_bits > 0
            })
            .ok_or("graphics+compute queue with timestamps required")? as u32;
        let mut device_names = std::ptr::null();
        api::check(
            (api.device_extensions)(instance.handle(), physical, &mut count, &mut device_names),
            "device extensions",
        )?;
        let device_extensions = if count == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(device_names, count as usize).to_vec()
        };
        let supported = instance.get_physical_device_features(physical);
        if supported.shader_storage_image_extended_formats == 0 {
            return Err("FP16 storage images unsupported".into());
        }
        let mut root = vk::PhysicalDeviceFeatures2::default().features(
            vk::PhysicalDeviceFeatures::default().shader_storage_image_extended_formats(true),
        );
        let mut features = (&mut root as *mut vk::PhysicalDeviceFeatures2).cast::<c_void>();
        api::check(
            (api.features)(instance.handle(), physical, &mut features),
            "device features",
        )?;
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let mut device_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&device_extensions);
        device_info.p_next = features;
        let device = instance.create_device(physical, &device_info, None)?;
        let queue = device.get_device_queue(family, 0);
        let memory = instance.get_physical_device_memory_properties(physical);
        let pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        let cmd = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        let queries = device.create_query_pool(
            &vk::QueryPoolCreateInfo::default()
                .query_type(vk::QueryType::TIMESTAMP)
                .query_count(2),
            None,
        )?;
        let requirements = json!({"instance_api":version,"instance_extensions":string_names(&extensions),"device_extensions":string_names(&device_extensions)});
        Ok(Self {
            _entry: entry,
            instance,
            physical,
            device,
            family,
            queue,
            memory,
            props,
            timestamp_bits: families[family as usize].timestamp_valid_bits,
            pool,
            cmd,
            fence,
            queries,
            debug,
            requirements,
        })
    }
    pub unsafe fn begin(&self) -> Result<()> {
        self.device
            .reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty())?;
        self.device.begin_command_buffer(
            self.cmd,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        Ok(())
    }
    pub unsafe fn submit(&self) -> Result<()> {
        self.device.end_command_buffer(self.cmd)?;
        self.device.reset_fences(&[self.fence])?;
        if let Err(e) = self.device.queue_submit(
            self.queue,
            &[vk::SubmitInfo::default().command_buffers(&[self.cmd])],
            self.fence,
        ) {
            eprintln!("GPU submission state uncertain: {e}; terminating without unsafe resource destruction");
            std::process::exit(3);
        }
        if let Err(e) = self
            .device
            .wait_for_fences(&[self.fence], true, 30_000_000_000)
        {
            eprintln!(
                "GPU fence did not retire: {e}; terminating without unsafe resource destruction"
            );
            std::process::exit(3);
        }
        Ok(())
    }
    pub unsafe fn milliseconds(&self) -> Result<f64> {
        let mut ticks = [0u64; 2];
        self.device.get_query_pool_results(
            self.queries,
            0,
            &mut ticks,
            vk::QueryResultFlags::TYPE_64 | vk::QueryResultFlags::WAIT,
        )?;
        let mask = if self.timestamp_bits == 64 {
            u64::MAX
        } else {
            (1u64 << self.timestamp_bits) - 1
        };
        Ok((ticks[1].wrapping_sub(ticks[0]) & mask) as f64
            * self.props.limits.timestamp_period as f64
            / 1e6)
    }
}
unsafe fn string_names(names: &[*const c_char]) -> Vec<String> {
    names
        .iter()
        .map(|p| CStr::from_ptr(*p).to_string_lossy().into_owned())
        .collect()
}
impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_query_pool(self.queries, None);
            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.pool, None);
            self.device.destroy_device(None);
            if let Some((d, m)) = &self.debug {
                d.destroy_debug_utils_messenger(*m, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}

pub fn range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}
fn memory_type(gpu: &Gpu, bits: u32, flags: vk::MemoryPropertyFlags) -> Result<u32> {
    (0..gpu.memory.memory_type_count)
        .find(|&i| {
            bits & (1 << i) != 0
                && gpu.memory.memory_types[i as usize]
                    .property_flags
                    .contains(flags)
        })
        .ok_or_else(|| "no compatible Vulkan memory type".into())
}
pub struct Image {
    device: ash::Device,
    pub image: vk::Image,
    pub view: vk::ImageView,
    memory: vk::DeviceMemory,
    pub size: api::Size,
    pub format: vk::Format,
    pub allocated: u64,
}
impl Image {
    pub unsafe fn new(gpu: &Gpu, size: api::Size, format: vk::Format) -> Result<Self> {
        let mut image = Self {
            device: gpu.device.clone(),
            image: vk::Image::null(),
            view: vk::ImageView::null(),
            memory: vk::DeviceMemory::null(),
            size,
            format,
            allocated: 0,
        };
        image.image = gpu.device.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D {
                    width: size.x,
                    height: size.y,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::SAMPLED
                        | vk::ImageUsageFlags::STORAGE
                        | vk::ImageUsageFlags::TRANSFER_SRC,
                ),
            None,
        )?;
        let req = gpu.device.get_image_memory_requirements(image.image);
        image.allocated = req.size;
        image.memory = gpu.device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(memory_type(
                    gpu,
                    req.memory_type_bits,
                    vk::MemoryPropertyFlags::DEVICE_LOCAL,
                )?),
            None,
        )?;
        gpu.device.bind_image_memory(image.image, image.memory, 0)?;
        image.view = gpu.device.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(image.image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(range()),
            None,
        )?;
        Ok(image)
    }
    pub fn info(&self) -> api::ImageInfo {
        api::ImageInfo {
            view: self.view,
            image: self.image,
            range: range(),
            format: self.format,
            width: self.size.x,
            height: self.size.y,
        }
    }
    pub unsafe fn barrier(&self, gpu: &Gpu, old: vk::ImageLayout, new: vk::ImageLayout) {
        gpu.device.cmd_pipeline_barrier(
            gpu.cmd,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .image(self.image)
                .subresource_range(range())
                .old_layout(old)
                .new_layout(new)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .src_access_mask(if old == vk::ImageLayout::UNDEFINED {
                    vk::AccessFlags::empty()
                } else {
                    vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE
                })
                .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)],
        );
    }
    pub unsafe fn read(&self, gpu: &Gpu) -> Result<Vec<f32>> {
        let bytes = self.size.x as u64 * self.size.y as u64 * 8;
        let mut buffer = Buffer {
            device: gpu.device.clone(),
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
        };
        buffer.buffer = gpu.device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(bytes)
                .usage(vk::BufferUsageFlags::TRANSFER_DST),
            None,
        )?;
        let req = gpu.device.get_buffer_memory_requirements(buffer.buffer);
        buffer.memory = gpu.device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(memory_type(
                    gpu,
                    req.memory_type_bits,
                    vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
                )?),
            None,
        )?;
        gpu.device
            .bind_buffer_memory(buffer.buffer, buffer.memory, 0)?;
        gpu.begin()?;
        self.barrier(
            gpu,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        gpu.device.cmd_copy_image_to_buffer(
            gpu.cmd,
            self.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            buffer.buffer,
            &[vk::BufferImageCopy::default()
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width: self.size.x,
                    height: self.size.y,
                    depth: 1,
                })],
        );
        self.barrier(
            gpu,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        gpu.device.cmd_pipeline_barrier(
            gpu.cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)],
            &[],
            &[],
        );
        gpu.submit()?;
        let mapped = gpu
            .device
            .map_memory(buffer.memory, 0, bytes, vk::MemoryMapFlags::empty())?;
        let pixels = std::slice::from_raw_parts(mapped.cast::<u16>(), bytes as usize / 2)
            .iter()
            .map(|&v| half::f16::from_bits(v).to_f32())
            .collect();
        gpu.device.unmap_memory(buffer.memory);
        Ok(pixels)
    }
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
