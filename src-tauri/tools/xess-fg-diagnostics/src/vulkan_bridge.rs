//! Same-adapter D3D12 resource/fence import, exercised by a Vulkan compute producer.
use crate::Result;
use ash::{vk, Entry};
use serde_json::json;
use std::{
    cell::RefCell,
    ffi::{c_void, CStr},
    sync::atomic::{AtomicUsize, Ordering},
};
thread_local! { static BRIDGE: RefCell<Option<Bridge>> = const { RefCell::new(None) }; }
static BAD: AtomicUsize = AtomicUsize::new(0);
fn log(value: serde_json::Value) {
    crate::record(value);
}
unsafe extern "system" fn validation(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT<'_>,
    _: *mut c_void,
) -> vk::Bool32 {
    let message = CStr::from_ptr((*data).p_message).to_string_lossy();
    let isolation=kind==vk::DebugUtilsMessageTypeFlagsEXT::GENERAL&&(*data).message_id_number==0&&[
        "VK_LAYER_NV_optimus","VK_LAYER_NV_present","VK_LAYER_AMD_switchable_graphics","VK_LAYER_reshade"
    ].iter().any(|name|message==format!("Layer \"{name}\" forced disabled because name matches filter of env var 'VK_LOADER_LAYERS_DISABLE'."));
    if !isolation {
        BAD.fetch_add(1, Ordering::Relaxed);
    }
    log(
        json!({"event":if isolation{"vulkan_loader_notice"}else{"vulkan_validation"},"severity":severity.as_raw(),"message":message}),
    );
    vk::FALSE
}
struct Bridge {
    _entry: Entry,
    instance: ash::Instance,
    debug: Option<(ash::ext::debug_utils::Instance, vk::DebugUtilsMessengerEXT)>,
    device: Option<ash::Device>,
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
    semaphore: vk::Semaphore,
    fence: vk::Fence,
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    queue: vk::Queue,
    family: u32,
    width: u32,
    height: u32,
    set_layout: vk::DescriptorSetLayout,
    descriptor_pool: vk::DescriptorPool,
    set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    shader: vk::ShaderModule,
    submitted: bool,
    initialized: bool,
}
impl Bridge {
    unsafe fn new(
        luid: &[u8],
        resource: isize,
        fence: isize,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let entry = Entry::load()?;
        let checking = std::env::var("NS_XESS_FG_VULKAN_VALIDATION").as_deref() == Ok("1");
        let layers = if checking {
            vec![c"VK_LAYER_KHRONOS_validation".as_ptr()]
        } else {
            vec![]
        };
        let extensions = if checking {
            vec![ash::ext::debug_utils::NAME.as_ptr()]
        } else {
            vec![]
        };
        let app = vk::ApplicationInfo::default()
            .application_name(c"Independent XeSS FG Vulkan producer")
            .api_version(vk::API_VERSION_1_2);
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
            .pfn_user_callback(Some(validation));
        let validation_enables = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
        let mut validation_info =
            vk::ValidationFeaturesEXT::default().enabled_validation_features(&validation_enables);
        let mut info = vk::InstanceCreateInfo::default()
            .application_info(&app)
            .enabled_layer_names(&layers)
            .enabled_extension_names(&extensions);
        if checking {
            info = info
                .push_next(&mut debug_info)
                .push_next(&mut validation_info);
        }
        let instance = entry.create_instance(&info, None)?;
        let mut b = Self {
            _entry: entry,
            instance,
            debug: None,
            device: None,
            image: vk::Image::null(),
            memory: vk::DeviceMemory::null(),
            view: vk::ImageView::null(),
            semaphore: vk::Semaphore::null(),
            fence: vk::Fence::null(),
            pool: vk::CommandPool::null(),
            cmd: vk::CommandBuffer::null(),
            queue: vk::Queue::null(),
            family: 0,
            width,
            height,
            set_layout: vk::DescriptorSetLayout::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            set: vk::DescriptorSet::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            shader: vk::ShaderModule::null(),
            submitted: false,
            initialized: false,
        };
        if checking {
            let loader = ash::ext::debug_utils::Instance::new(&b._entry, &b.instance);
            let messenger = loader.create_debug_utils_messenger(&debug_info, None)?;
            b.debug = Some((loader, messenger));
        }
        let mut matches = Vec::new();
        for p in b.instance.enumerate_physical_devices()? {
            let mut ids = vk::PhysicalDeviceIDProperties::default();
            let mut props = vk::PhysicalDeviceProperties2::default().push_next(&mut ids);
            b.instance.get_physical_device_properties2(p, &mut props);
            if ids.device_luid_valid != 0 && ids.device_luid == luid {
                matches.push(p);
            }
        }
        if matches.len() != 1 {
            return Err("Vulkan/D3D12 LUID match must be unique".into());
        }
        let physical = matches[0];
        let family = b
            .instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::COMPUTE))
            .ok_or("no compute queue")? as u32;
        b.family = family;
        let usage = vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        let mut external = vk::PhysicalDeviceExternalImageFormatInfo::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::D3D12_RESOURCE);
        let image_info = vk::PhysicalDeviceImageFormatInfo2::default()
            .format(vk::Format::R8G8B8A8_UNORM)
            .ty(vk::ImageType::TYPE_2D)
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(usage)
            .push_next(&mut external);
        let mut external_props = vk::ExternalImageFormatProperties::default();
        let mut image_props = vk::ImageFormatProperties2::default().push_next(&mut external_props);
        b.instance.get_physical_device_image_format_properties2(
            physical,
            &image_info,
            &mut image_props,
        )?;
        if !external_props
            .external_memory_properties
            .external_memory_features
            .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
        {
            return Err("D3D12 texture import unsupported".into());
        }
        let mut timeline =
            vk::SemaphoreTypeCreateInfo::default().semaphore_type(vk::SemaphoreType::TIMELINE);
        let semaphore_info = vk::PhysicalDeviceExternalSemaphoreInfo::default()
            .handle_type(vk::ExternalSemaphoreHandleTypeFlags::D3D12_FENCE)
            .push_next(&mut timeline);
        let mut semaphore_props = vk::ExternalSemaphoreProperties::default();
        b.instance
            .get_physical_device_external_semaphore_properties(
                physical,
                &semaphore_info,
                &mut semaphore_props,
            );
        if !semaphore_props
            .external_semaphore_features
            .contains(vk::ExternalSemaphoreFeatureFlags::IMPORTABLE)
        {
            return Err("D3D12 fence import unsupported".into());
        }
        let mut timeline_feature =
            vk::PhysicalDeviceTimelineSemaphoreFeatures::default().timeline_semaphore(true);
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let extensions = [
            ash::khr::external_memory_win32::NAME.as_ptr(),
            ash::khr::external_semaphore_win32::NAME.as_ptr(),
        ];
        b.device = Some(
            b.instance.create_device(
                physical,
                &vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queues)
                    .enabled_extension_names(&extensions)
                    .push_next(&mut timeline_feature),
                None,
            )?,
        );
        let d = b.device.as_ref().unwrap();
        b.queue = d.get_device_queue(family, 0);
        let mut image_external = vk::ExternalMemoryImageCreateInfo::default()
            .handle_types(vk::ExternalMemoryHandleTypeFlags::D3D12_RESOURCE);
        b.image = d.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .push_next(&mut image_external),
            None,
        )?;
        let requirements = d.get_image_memory_requirements(b.image);
        let memory_loader = ash::khr::external_memory_win32::Device::new(&b.instance, d);
        let mut handle_props = vk::MemoryWin32HandlePropertiesKHR::default();
        memory_loader.get_memory_win32_handle_properties(
            vk::ExternalMemoryHandleTypeFlags::D3D12_RESOURCE,
            resource,
            &mut handle_props,
        )?;
        let bits = requirements.memory_type_bits & handle_props.memory_type_bits;
        let memory_index = (0..32)
            .find(|i| bits & (1 << i) != 0)
            .ok_or("no compatible imported memory type")?;
        let mut import = vk::ImportMemoryWin32HandleInfoKHR::default()
            .handle_type(vk::ExternalMemoryHandleTypeFlags::D3D12_RESOURCE)
            .handle(resource);
        let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(b.image);
        b.memory = d.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_index)
                .push_next(&mut import)
                .push_next(&mut dedicated),
            None,
        )?;
        d.bind_image_memory(b.image, b.memory, 0)?;
        b.view = d.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(b.image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_UNORM)
                .subresource_range(range()),
            None,
        )?;
        b.semaphore = d.create_semaphore(
            &vk::SemaphoreCreateInfo::default().push_next(&mut timeline),
            None,
        )?;
        ash::khr::external_semaphore_win32::Device::new(&b.instance, d)
            .import_semaphore_win32_handle(
                &vk::ImportSemaphoreWin32HandleInfoKHR::default()
                    .semaphore(b.semaphore)
                    .handle_type(vk::ExternalSemaphoreHandleTypeFlags::D3D12_FENCE)
                    .handle(fence),
            )?;
        b.pool = d.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        b.cmd = d.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(b.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        b.fence = d.create_fence(&vk::FenceCreateInfo::default(), None)?;
        b.set_layout = d.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&[
                vk::DescriptorSetLayoutBinding::default()
                    .binding(0)
                    .descriptor_count(1)
                    .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                    .stage_flags(vk::ShaderStageFlags::COMPUTE),
            ]),
            None,
        )?;
        b.descriptor_pool = d.create_descriptor_pool(
            &vk::DescriptorPoolCreateInfo::default()
                .max_sets(1)
                .pool_sizes(&[vk::DescriptorPoolSize {
                    ty: vk::DescriptorType::STORAGE_IMAGE,
                    descriptor_count: 1,
                }]),
            None,
        )?;
        b.set = d.allocate_descriptor_sets(
            &vk::DescriptorSetAllocateInfo::default()
                .descriptor_pool(b.descriptor_pool)
                .set_layouts(&[b.set_layout]),
        )?[0];
        d.update_descriptor_sets(
            &[vk::WriteDescriptorSet::default()
                .dst_set(b.set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::STORAGE_IMAGE)
                .image_info(&[vk::DescriptorImageInfo::default()
                    .image_view(b.view)
                    .image_layout(vk::ImageLayout::GENERAL)])],
            &[],
        );
        b.pipeline_layout = d.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(&[b.set_layout])
                .push_constant_ranges(&[vk::PushConstantRange::default()
                    .stage_flags(vk::ShaderStageFlags::COMPUTE)
                    .size(4)]),
            None,
        )?;
        let code = ash::util::read_spv(&mut std::io::Cursor::new(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/bridge_scene.spv"
        ))))?;
        b.shader =
            d.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&code), None)?;
        b.pipeline = d
            .create_compute_pipelines(
                vk::PipelineCache::null(),
                &[vk::ComputePipelineCreateInfo::default()
                    .layout(b.pipeline_layout)
                    .stage(
                        vk::PipelineShaderStageCreateInfo::default()
                            .stage(vk::ShaderStageFlags::COMPUTE)
                            .module(b.shader)
                            .name(c"main"),
                    )],
                None,
            )
            .map_err(|(partial, e)| {
                for p in partial {
                    d.destroy_pipeline(p, None);
                }
                e
            })?[0];
        let props = b.instance.get_physical_device_properties(physical);
        log(
            json!({"event":"vulkan_bridge_ready","device":CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy(),"luid":luid,"memory_import":"D3D12_RESOURCE","semaphore_import":"D3D12_FENCE timeline","zero_cpu_color_copy":true,"validation":checking,"size":[width,height]}),
        );
        Ok(b)
    }
    unsafe fn render(&mut self, frame: u32) -> Result<()> {
        if frame == 0 {
            log(json!({"event":"bridge_stage","stage":"Vulkan record begin"}));
        }
        let d = self.device.as_ref().unwrap();
        if self.submitted {
            if d.wait_for_fences(&[self.fence], true, 30_000_000_000)
                .is_err()
            {
                std::process::abort();
            }
            self.submitted = false;
        }
        d.reset_fences(&[self.fence])?;
        d.reset_command_buffer(self.cmd, vk::CommandBufferResetFlags::empty())?;
        d.begin_command_buffer(
            self.cmd,
            &vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
        )?;
        let acquire = vk::ImageMemoryBarrier::default()
            .image(self.image)
            .subresource_range(range())
            .old_layout(if self.initialized {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::UNDEFINED
            })
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
            .dst_queue_family_index(self.family)
            .dst_access_mask(vk::AccessFlags::SHADER_WRITE);
        d.cmd_pipeline_barrier(
            self.cmd,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[acquire],
        );
        d.cmd_bind_pipeline(self.cmd, vk::PipelineBindPoint::COMPUTE, self.pipeline);
        d.cmd_bind_descriptor_sets(
            self.cmd,
            vk::PipelineBindPoint::COMPUTE,
            self.pipeline_layout,
            0,
            &[self.set],
            &[],
        );
        d.cmd_push_constants(
            self.cmd,
            self.pipeline_layout,
            vk::ShaderStageFlags::COMPUTE,
            0,
            &frame.to_ne_bytes(),
        );
        d.cmd_dispatch(self.cmd, self.width.div_ceil(8), self.height.div_ceil(8), 1);
        let release = vk::ImageMemoryBarrier::default()
            .image(self.image)
            .subresource_range(range())
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(self.family)
            .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
            .src_access_mask(vk::AccessFlags::SHADER_WRITE);
        d.cmd_pipeline_barrier(
            self.cmd,
            vk::PipelineStageFlags::COMPUTE_SHADER,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[release],
        );
        d.end_command_buffer(self.cmd)?;
        if frame == 0 {
            log(json!({"event":"bridge_stage","stage":"Vulkan command recorded"}));
        }
        // The fresh D3D12 resource has no outstanding producer on frame zero.
        // Avoid an unnecessary external fence wait at its initial value.
        let wait_values = if frame == 0 {
            vec![]
        } else {
            vec![frame as u64 * 2]
        };
        let signal_values = [frame as u64 * 2 + 1];
        let mut timeline = vk::TimelineSemaphoreSubmitInfo::default()
            .wait_semaphore_values(&wait_values)
            .signal_semaphore_values(&signal_values);
        let semaphores = [self.semaphore];
        let waits = if frame == 0 {
            vec![]
        } else {
            vec![self.semaphore]
        };
        let commands = [self.cmd];
        let stages = [vk::PipelineStageFlags::ALL_COMMANDS];
        self.submitted = true;
        if let Err(error) = d.queue_submit(
            self.queue,
            &[vk::SubmitInfo::default()
                .wait_semaphores(&waits)
                .wait_dst_stage_mask(&stages[..waits.len()])
                .signal_semaphores(&semaphores)
                .command_buffers(&commands)
                .push_next(&mut timeline)],
            self.fence,
        ) {
            log(json!({"event":"vulkan_bridge_submit_failure","error":format!("{error:?}")}));
            std::process::abort();
        }
        self.initialized = true;
        if frame == 0 {
            log(json!({"event":"bridge_stage","stage":"Vulkan submit returned"}));
        }
        Ok(())
    }
}
fn range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}
impl Drop for Bridge {
    fn drop(&mut self) {
        unsafe {
            if let Some(d) = &self.device {
                if self.submitted
                    && d.wait_for_fences(&[self.fence], true, 30_000_000_000)
                        .is_err()
                {
                    std::process::abort();
                }
                d.destroy_pipeline(self.pipeline, None);
                d.destroy_shader_module(self.shader, None);
                d.destroy_pipeline_layout(self.pipeline_layout, None);
                d.destroy_descriptor_pool(self.descriptor_pool, None);
                d.destroy_descriptor_set_layout(self.set_layout, None);
                d.destroy_fence(self.fence, None);
                d.destroy_command_pool(self.pool, None);
                d.destroy_semaphore(self.semaphore, None);
                d.destroy_image_view(self.view, None);
                d.destroy_image(self.image, None);
                d.free_memory(self.memory, None);
                d.destroy_device(None);
            }
            if let Some((loader, messenger)) = &self.debug {
                loader.destroy_debug_utils_messenger(*messenger, None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
#[no_mangle]
unsafe extern "C" fn xess_vk_init(
    luid: *const u8,
    resource: isize,
    fence: isize,
    width: u32,
    height: u32,
) -> i32 {
    match Bridge::new(
        std::slice::from_raw_parts(luid, 8),
        resource,
        fence,
        width,
        height,
    ) {
        Ok(bridge) => {
            BRIDGE.with_borrow_mut(|b| *b = Some(bridge));
            0
        }
        Err(e) => {
            log(json!({"event":"vulkan_bridge_failure","error":e.to_string()}));
            1
        }
    }
}
#[no_mangle]
unsafe extern "C" fn xess_vk_render(frame: u32) -> i32 {
    BRIDGE.with_borrow_mut(|b| {
        match b
            .as_mut()
            .ok_or("bridge not initialized".into())
            .and_then(|b| b.render(frame))
        {
            Ok(()) => 0,
            Err(e) => {
                log(json!({"event":"vulkan_bridge_failure","error":e.to_string()}));
                1
            }
        }
    })
}
#[no_mangle]
extern "C" fn xess_vk_finish() -> u32 {
    BRIDGE.with_borrow_mut(|b| drop(b.take()));
    let bad = BAD.load(Ordering::Relaxed);
    log(json!({"event":"vulkan_bridge_finished","validation_warnings_errors":bad}));
    bad as u32
}
