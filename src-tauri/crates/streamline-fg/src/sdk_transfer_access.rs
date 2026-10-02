//! Pinned FG images used by NGX shaders and transfer clear/copy operations in GENERAL.
//! Complete the existing ALL_COMMANDS barriers' shader-only access masks. Layouts,
//! subresources, command count and submission/wait behavior stay unchanged.
use super::*;

#[derive(Default)]
struct Image {
    usage: vk::ImageUsageFlags,
    bound: bool,
    selected: bool,
    samples: u32,
}
#[derive(Default)]
struct Buffer {
    usage: vk::BufferUsageFlags,
    bound: bool,
    selected: bool,
    samples: u32,
}
#[derive(Default)]
struct Images {
    images: HashMap<(u64, u64), Image>,
    buffers: HashMap<(u64, u64), Buffer>,
    adapted: HashMap<u64, u64>,
}
static IMAGES: OnceLock<Mutex<Images>> = OnceLock::new();
pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var("NS_STREAMLINE_SDK_TRANSFER_ACCESS").as_deref() != Ok("0"))
}
pub(super) fn selected_name(name: &str) -> bool {
    matches!(
        name,
        "nv.ngx.dlssg.resource"
            | "nv.sl.dlss_g.tex2d.fake-swapchain-buffer"
            | "nv.sl.dlss_g.clone.dlfg-output_0"
            | "nv.sl.dlss_g.clone.dlfg-output_1"
    )
}
fn adapt(
    image: &Image,
    b: &mut vk::ImageMemoryBarrier,
    src: vk::PipelineStageFlags,
    dst: vk::PipelineStageFlags,
) -> bool {
    if !image.bound
        || !image.selected
        || !b.p_next.is_null()
        || src != vk::PipelineStageFlags::ALL_COMMANDS
        || dst != vk::PipelineStageFlags::ALL_COMMANDS
        || b.src_queue_family_index != vk::QUEUE_FAMILY_IGNORED
        || b.dst_queue_family_index != vk::QUEUE_FAMILY_IGNORED
        || b.subresource_range.aspect_mask != vk::ImageAspectFlags::COLOR
    {
        return false;
    }
    let shader = vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE;
    let mut transfer = vk::AccessFlags::empty();
    if image.usage.contains(vk::ImageUsageFlags::TRANSFER_SRC) {
        transfer |= vk::AccessFlags::TRANSFER_READ;
    }
    if image.usage.contains(vk::ImageUsageFlags::TRANSFER_DST) {
        transfer |= vk::AccessFlags::TRANSFER_WRITE;
    }
    let mut changed = false;
    if matches!(
        b.old_layout,
        vk::ImageLayout::GENERAL | vk::ImageLayout::UNDEFINED
    ) && b.src_access_mask == shader
        && !transfer.is_empty()
    {
        b.src_access_mask |= transfer;
        changed = true;
    }
    if b.new_layout == vk::ImageLayout::GENERAL
        && b.dst_access_mask == shader
        && !transfer.is_empty()
    {
        b.dst_access_mask |= transfer;
        changed = true;
    }
    changed
}
pub(super) fn created(device: vk::Device, image: vk::Image, info: &vk::ImageCreateInfo) {
    if !enabled() {
        return;
    }
    let mut state = IMAGES.get_or_init(Default::default).lock().unwrap();
    let key = (device.as_raw(), image.as_raw());
    state.images.remove(&key);
    if info.p_next.is_null()
        && info.flags.is_empty()
        && info.image_type == vk::ImageType::TYPE_2D
        && info.samples == vk::SampleCountFlags::TYPE_1
        && info.tiling == vk::ImageTiling::OPTIMAL
        && info
            .usage
            .intersects(vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST)
    {
        state.images.insert(
            key,
            Image {
                usage: info.usage,
                ..Default::default()
            },
        );
    }
}
pub(super) fn bound(device: vk::Device, image: vk::Image) {
    if enabled() {
        if let Some(image) = IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .images
            .get_mut(&(device.as_raw(), image.as_raw()))
        {
            image.bound = true;
        }
    }
}
pub(super) fn named(device: vk::Device, image: u64, name: &str) {
    if enabled() {
        if let Some(image) = IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .images
            .get_mut(&(device.as_raw(), image))
        {
            image.selected = selected_name(name);
        }
    }
}
pub(super) fn destroyed(device: vk::Device, image: vk::Image) {
    if enabled() {
        IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .images
            .remove(&(device.as_raw(), image.as_raw()));
    }
}
pub(super) fn retired_device(device: vk::Device) {
    if !enabled() {
        return;
    }
    let count = {
        let mut state = IMAGES.get_or_init(Default::default).lock().unwrap();
        state.images.retain(|(d, _), _| *d != device.as_raw());
        state.buffers.retain(|(d, _), _| *d != device.as_raw());
        state.adapted.remove(&device.as_raw()).unwrap_or(0)
    };
    trace::event!(
        "sdk_transfer_access_summary",
        json!({"device":device.as_raw(),"adapted_barriers":count,"extra_commands":0,"extra_submissions":0,"extra_cpu_waits":0})
    );
}
pub(super) fn buffer_created(device: vk::Device, buffer: vk::Buffer, info: &vk::BufferCreateInfo) {
    if !enabled() {
        return;
    }
    let mut state = IMAGES.get_or_init(Default::default).lock().unwrap();
    let key = (device.as_raw(), buffer.as_raw());
    state.buffers.remove(&key);
    if info.p_next.is_null()
        && info.flags.is_empty()
        && info
            .usage
            .intersects(vk::BufferUsageFlags::TRANSFER_SRC | vk::BufferUsageFlags::TRANSFER_DST)
    {
        state.buffers.insert(
            key,
            Buffer {
                usage: info.usage,
                ..Default::default()
            },
        );
    }
}
pub(super) fn buffer_bound(device: vk::Device, buffer: vk::Buffer) {
    if enabled() {
        if let Some(buffer) = IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .buffers
            .get_mut(&(device.as_raw(), buffer.as_raw()))
        {
            buffer.bound = true;
        }
    }
}
pub(super) fn buffer_named(device: vk::Device, buffer: u64, name: &str) {
    if enabled() {
        if let Some(buffer) = IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .buffers
            .get_mut(&(device.as_raw(), buffer))
        {
            buffer.selected = name == "nv.ngx.dlssg.resource";
        }
    }
}
pub(super) fn buffer_destroyed(device: vk::Device, buffer: vk::Buffer) {
    if enabled() {
        IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .buffers
            .remove(&(device.as_raw(), buffer.as_raw()));
    }
}
fn adapt_buffer(
    buffer: &Buffer,
    b: &mut vk::BufferMemoryBarrier,
    src: vk::PipelineStageFlags,
    dst: vk::PipelineStageFlags,
) -> bool {
    if !buffer.bound
        || !buffer.selected
        || !b.p_next.is_null()
        || src != vk::PipelineStageFlags::ALL_COMMANDS
        || dst != vk::PipelineStageFlags::ALL_COMMANDS
        || b.src_queue_family_index != vk::QUEUE_FAMILY_IGNORED
        || b.dst_queue_family_index != vk::QUEUE_FAMILY_IGNORED
    {
        return false;
    }
    let shader = vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE;
    let mut transfer = vk::AccessFlags::empty();
    if buffer.usage.contains(vk::BufferUsageFlags::TRANSFER_SRC) {
        transfer |= vk::AccessFlags::TRANSFER_READ;
    }
    if buffer.usage.contains(vk::BufferUsageFlags::TRANSFER_DST) {
        transfer |= vk::AccessFlags::TRANSFER_WRITE;
    }
    let mut changed = false;
    if !transfer.is_empty() && b.src_access_mask == shader {
        b.src_access_mask |= transfer;
        changed = true;
    }
    if !transfer.is_empty() && b.dst_access_mask == shader {
        b.dst_access_mask |= transfer;
        changed = true;
    }
    changed
}
pub(super) fn buffer_barriers<'a>(
    device: vk::Device,
    command: vk::CommandBuffer,
    src: vk::PipelineStageFlags,
    dst: vk::PipelineStageFlags,
    input: &[vk::BufferMemoryBarrier<'a>],
) -> Option<Vec<vk::BufferMemoryBarrier<'a>>> {
    if !enabled() {
        return None;
    }
    let mut state = IMAGES.get_or_init(Default::default).lock().unwrap();
    let mut copy = None;
    let mut count = 0;
    for (index, original) in input.iter().enumerate() {
        let Some(buffer) = state
            .buffers
            .get_mut(&(device.as_raw(), original.buffer.as_raw()))
        else {
            continue;
        };
        let mut b = *original;
        if adapt_buffer(buffer, &mut b, src, dst) {
            copy.get_or_insert_with(|| input.to_vec())[index] = b;
            count += 1;
            if buffer.samples < 8 {
                buffer.samples += 1;
                trace::event!(
                    "sdk_transfer_buffer_access",
                    json!({"device":device.as_raw(),"buffer":b.buffer.as_raw(),"command":command.as_raw(),"offset":b.offset,"size":b.size,"original_src_access":original.src_access_mask.as_raw(),"original_dst_access":original.dst_access_mask.as_raw(),"src_access":b.src_access_mask.as_raw(),"dst_access":b.dst_access_mask.as_raw(),"sdk_created_bound_named_buffer":true})
                );
            }
        }
    }
    *state.adapted.entry(device.as_raw()).or_default() += count;
    copy
}
pub(super) fn barriers<'a>(
    device: vk::Device,
    command: vk::CommandBuffer,
    src: vk::PipelineStageFlags,
    dst: vk::PipelineStageFlags,
    input: &[vk::ImageMemoryBarrier<'a>],
) -> Option<Vec<vk::ImageMemoryBarrier<'a>>> {
    if !enabled() {
        return None;
    }
    let mut state = IMAGES.get_or_init(Default::default).lock().unwrap();
    let mut copy = None;
    let mut count = 0;
    for (index, original) in input.iter().enumerate() {
        let Some(image) = state
            .images
            .get_mut(&(device.as_raw(), original.image.as_raw()))
        else {
            continue;
        };
        let mut b = *original;
        if adapt(image, &mut b, src, dst) {
            copy.get_or_insert_with(|| input.to_vec())[index] = b;
            count += 1;
            if image.samples < 8 {
                image.samples += 1;
                trace::event!(
                    "sdk_transfer_access",
                    json!({"device":device.as_raw(),"image":b.image.as_raw(),"command":command.as_raw(),"old_layout":b.old_layout.as_raw(),"new_layout":b.new_layout.as_raw(),"original_src_access":original.src_access_mask.as_raw(),"original_dst_access":original.dst_access_mask.as_raw(),"src_access":b.src_access_mask.as_raw(),"dst_access":b.dst_access_mask.as_raw(),"layout_unchanged":true,"sdk_created_bound_named_image":true})
                );
            }
        }
    }
    *state.adapted.entry(device.as_raw()).or_default() += count;
    copy
}
#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> Image {
        Image {
            usage: vk::ImageUsageFlags::TRANSFER_SRC | vk::ImageUsageFlags::TRANSFER_DST,
            bound: true,
            selected: true,
            samples: 0,
        }
    }
    fn barrier() -> vk::ImageMemoryBarrier<'static> {
        vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(1)
                    .layer_count(1),
            )
    }
    #[test]
    fn general_transfer_dependencies_preserve_layout_and_are_idempotent() {
        let mut b = barrier();
        let stage = vk::PipelineStageFlags::ALL_COMMANDS;
        assert!(adapt(&image(), &mut b, stage, stage));
        assert!(b.src_access_mask.contains(vk::AccessFlags::TRANSFER_WRITE));
        assert!(b
            .dst_access_mask
            .contains(vk::AccessFlags::TRANSFER_READ | vk::AccessFlags::TRANSFER_WRITE));
        assert_eq!(b.old_layout, vk::ImageLayout::GENERAL);
        assert_eq!(b.new_layout, vk::ImageLayout::GENERAL);
        assert!(!adapt(&image(), &mut b, stage, stage));
    }
    #[test]
    fn requires_proven_owner_and_compatible_stage_usage() {
        let stage = vk::PipelineStageFlags::ALL_COMMANDS;
        for image in [
            Image::default(),
            Image {
                bound: false,
                ..image()
            },
            Image {
                selected: false,
                ..image()
            },
        ] {
            assert!(!adapt(&image, &mut barrier(), stage, stage));
        }
        assert!(!adapt(
            &image(),
            &mut barrier(),
            vk::PipelineStageFlags::COMPUTE_SHADER,
            stage
        ));
        let mut b = barrier();
        let mut owned = image();
        owned.usage = vk::ImageUsageFlags::TRANSFER_DST;
        assert!(adapt(&owned, &mut b, stage, stage));
        assert!(!b.src_access_mask.contains(vk::AccessFlags::TRANSFER_READ));
        assert!(!selected_name("nv.ngx.dlssnr.resource"));
        assert!(!selected_name("application texture"));
        let mut b = barrier();
        b.old_layout = vk::ImageLayout::TRANSFER_SRC_OPTIMAL;
        b.src_access_mask = vk::AccessFlags::TRANSFER_READ;
        assert!(adapt(&image(), &mut b, stage, stage));
        assert_eq!(b.src_access_mask, vk::AccessFlags::TRANSFER_READ);
    }
    #[test]
    fn buffer_transfer_dependency_requires_bound_named_owner_and_preserves_range() {
        let stage = vk::PipelineStageFlags::ALL_COMMANDS;
        let mut b = vk::BufferMemoryBarrier::default()
            .buffer(vk::Buffer::from_raw(7))
            .offset(128)
            .size(1024)
            .src_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
        assert!(!adapt_buffer(&Buffer::default(), &mut b, stage, stage));
        let buffer = Buffer {
            usage: vk::BufferUsageFlags::TRANSFER_DST,
            bound: true,
            selected: true,
            samples: 0,
        };
        assert!(adapt_buffer(&buffer, &mut b, stage, stage));
        assert_eq!(b.offset, 128);
        assert_eq!(b.size, 1024);
        assert_eq!(b.buffer.as_raw(), 7);
        assert!(b.src_access_mask.contains(vk::AccessFlags::TRANSFER_WRITE));
        assert!(!b.dst_access_mask.contains(vk::AccessFlags::TRANSFER_READ));
        assert!(!adapt_buffer(&buffer, &mut b, stage, stage));
    }
}
