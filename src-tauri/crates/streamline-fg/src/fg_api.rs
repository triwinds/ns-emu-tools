//! Shared fixed-width FG bridge and immutable Vulkan guide resources.
use ash::vk::{self, Handle};
use serde_json::json;
use std::ffi::c_void;
pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[repr(C)]
pub(crate) struct Api {
    pub(crate) feature: *mut c_void,
    pub(crate) token: *mut c_void,
    pub(crate) constants: *mut c_void,
    pub(crate) tags: *mut c_void,
}
#[repr(C)]
#[derive(Default)]
pub(crate) struct State {
    pub(crate) status: u32,
    pub(crate) minimum: u32,
    pub(crate) presented: u32,
    pub(crate) maximum: u32,
    pub(crate) fence: u64,
    pub(crate) value: u64,
}
impl State {
    pub(crate) fn json(&self) -> serde_json::Value {
        json!({"status":self.status,"minimum":self.minimum,"presented":self.presented,"maximum":self.maximum,"fence":self.fence,"value":self.value})
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Resource {
    pub(crate) image: u64,
    pub(crate) memory: u64,
    pub(crate) view: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: u32,
    pub(crate) usage: u32,
}
unsafe extern "C" {
    pub(crate) fn probe_fg_state(api: &Api, state: &mut State) -> i32;
    pub(crate) fn probe_fg_options(
        api: &Api,
        enabled: u32,
        width: u32,
        height: u32,
        count: u32,
        frame_limit_us: u32,
    ) -> i32;
    pub(crate) fn probe_fg_begin(api: &Api, frame: u32, token: &mut u64) -> i32;
    pub(crate) fn probe_fg_marker(api: &Api, token: u64, marker: u32) -> i32;
    pub(crate) fn probe_fg_inputs(
        api: &Api,
        token: u64,
        reset: u32,
        resources: *const Resource,
    ) -> i32;
}
pub(crate) fn checked(result: i32, operation: &str) -> Result<()> {
    if result != 0 {
        return Err(format!("{operation}: SDK result {result}").into());
    }
    Ok(())
}
pub(crate) unsafe fn state(api: &Api) -> Result<State> {
    let mut state = State::default();
    checked(probe_fg_state(api, &mut state), "FG state")?;
    Ok(state)
}
pub(crate) unsafe fn wait_inputs(device: &ash::Device, s: &State) -> Result<()> {
    if s.value != 0 {
        if s.fence == 0 {
            return Err("SDK completion value without semaphore".into());
        }
        device.wait_semaphores(
            &vk::SemaphoreWaitInfo::default()
                .semaphores(&[vk::Semaphore::from_raw(s.fence)])
                .values(&[s.value]),
            5_000_000_000,
        )?;
    }
    Ok(())
}
pub(crate) fn memory_type(
    props: &vk::PhysicalDeviceMemoryProperties,
    bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Result<u32> {
    (0..props.memory_type_count)
        .find(|&i| {
            bits & (1 << i) != 0
                && props.memory_types[i as usize]
                    .property_flags
                    .contains(flags)
        })
        .ok_or_else(|| "no compatible memory type".into())
}
pub(crate) unsafe fn texture(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    extent: vk::Extent2D,
    format: vk::Format,
) -> Result<Resource> {
    texture_with_usage(
        device,
        props,
        extent,
        format,
        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
    )
}
pub(crate) unsafe fn texture_with_usage(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    extent: vk::Extent2D,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
) -> Result<Resource> {
    let image = device.create_image(
        &vk::ImageCreateInfo::default()
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
            .sharing_mode(vk::SharingMode::EXCLUSIVE),
        None,
    )?;
    let req = device.get_image_memory_requirements(image);
    let memory = device.allocate_memory(
        &vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(memory_type(
                props,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?),
        None,
    )?;
    device.bind_image_memory(image, memory, 0)?;
    let view = device.create_image_view(
        &vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(range()),
        None,
    )?;
    Ok(Resource {
        image: image.as_raw(),
        memory: memory.as_raw(),
        view: view.as_raw(),
        width: extent.width,
        height: extent.height,
        format: format.as_raw() as u32,
        usage: usage.as_raw(),
    })
}
pub(crate) fn range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}
pub(crate) unsafe fn transition(
    device: &ash::Device,
    cmd: vk::CommandBuffer,
    image: vk::Image,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
) {
    device.cmd_pipeline_barrier(
        cmd,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[vk::ImageMemoryBarrier::default()
            .image(image)
            .subresource_range(range())
            .old_layout(old)
            .new_layout(new)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .src_access_mask(if old == vk::ImageLayout::UNDEFINED {
                vk::AccessFlags::empty()
            } else {
                vk::AccessFlags::MEMORY_WRITE
            })
            .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)],
    );
}
