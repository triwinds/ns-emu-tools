//! First-use adapter for the pinned SDK's two fresh BGRA8 DLFG clone outputs.
//! The captured first SDK barrier is TRANSFER_SRC -> GENERAL, before NGX work;
//! vkCreateImage actually starts at UNDEFINED. Correct only that barrier once.
//! Neither used images nor application/native swapchain images are candidates.
use super::*;

#[derive(Default)]
struct Fresh {
    bound: bool,
    selected: bool,
}
#[derive(Default)]
struct Images(HashMap<(u64, u64), Fresh>);
static IMAGES: OnceLock<Mutex<Images>> = OnceLock::new();
pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NS_STREAMLINE_SDK_OUTPUT_INIT").as_deref() != Ok("0"))
}
fn matches(info: &vk::ImageCreateInfo) -> bool {
    info.p_next.is_null()
        && info.flags.is_empty()
        && info.image_type == vk::ImageType::TYPE_2D
        && info.format == vk::Format::B8G8R8A8_UNORM
        && info.extent.width > 0
        && info.extent.height > 0
        && info.extent.depth == 1
        && info.mip_levels == 1
        && info.array_layers == 1
        && info.samples == vk::SampleCountFlags::TYPE_1
        && info.tiling == vk::ImageTiling::OPTIMAL
        && info.usage.as_raw() == 31
        && info.sharing_mode == vk::SharingMode::EXCLUSIVE
        && info.initial_layout == vk::ImageLayout::UNDEFINED
}
impl Images {
    fn adapt(
        &mut self,
        device: u64,
        b: &mut vk::ImageMemoryBarrier,
        src: vk::PipelineStageFlags,
        dst: vk::PipelineStageFlags,
    ) -> bool {
        // Any observed barrier consumes freshness, including an unsupported one.
        let Some(fresh) = self.0.remove(&(device, b.image.as_raw())) else {
            return false;
        };
        if !fresh.bound
            || !fresh.selected
            || !b.p_next.is_null()
            || b.old_layout != vk::ImageLayout::TRANSFER_SRC_OPTIMAL
            || b.new_layout != vk::ImageLayout::GENERAL
            || b.src_access_mask != vk::AccessFlags::TRANSFER_READ
            || b.dst_access_mask != (vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            || src != vk::PipelineStageFlags::ALL_COMMANDS
            || dst != vk::PipelineStageFlags::ALL_COMMANDS
            || b.src_queue_family_index != vk::QUEUE_FAMILY_IGNORED
            || b.dst_queue_family_index != vk::QUEUE_FAMILY_IGNORED
            || b.subresource_range.aspect_mask != vk::ImageAspectFlags::COLOR
            || b.subresource_range.base_mip_level != 0
            || b.subresource_range.level_count != vk::REMAINING_MIP_LEVELS
            || b.subresource_range.base_array_layer != 0
            || b.subresource_range.layer_count != vk::REMAINING_ARRAY_LAYERS
        {
            return false;
        }
        b.old_layout = vk::ImageLayout::UNDEFINED;
        b.src_access_mask = vk::AccessFlags::empty();
        true
    }
}
pub(super) fn created(device: vk::Device, image: vk::Image, info: &vk::ImageCreateInfo) {
    if enabled() && matches(info) {
        IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .insert((device.as_raw(), image.as_raw()), Fresh::default());
    }
}
pub(super) fn bound(device: vk::Device, image: vk::Image) {
    if enabled() {
        if let Some(entry) = IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .get_mut(&(device.as_raw(), image.as_raw()))
        {
            entry.bound = true;
        }
    }
}
pub(super) fn named(device: vk::Device, image: u64, name: &str) {
    if enabled() {
        if let Some(entry) = IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .get_mut(&(device.as_raw(), image))
        {
            entry.selected = matches!(
                name,
                "nv.sl.dlss_g.clone.dlfg-output_0" | "nv.sl.dlss_g.clone.dlfg-output_1"
            );
        }
    }
}
pub(super) fn used(device: vk::Device, image: vk::Image) {
    if enabled() {
        IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .remove(&(device.as_raw(), image.as_raw()));
    }
}
pub(super) fn retired_device(device: vk::Device) {
    if enabled() {
        IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .retain(|(d, _), _| *d != device.as_raw());
    }
}
pub(super) fn barriers<'a>(
    device: vk::Device,
    cmd: vk::CommandBuffer,
    src: vk::PipelineStageFlags,
    dst: vk::PipelineStageFlags,
    images: &[vk::ImageMemoryBarrier<'a>],
) -> Option<Vec<vk::ImageMemoryBarrier<'a>>> {
    if !enabled() {
        return None;
    }
    let mut state = IMAGES.get_or_init(Default::default).lock().unwrap();
    let mut copy = None;
    for (index, b) in images.iter().enumerate() {
        let mut adapted = *b;
        if state.adapt(device.as_raw(), &mut adapted, src, dst) {
            let copy = copy.get_or_insert_with(|| images.to_vec());
            copy[index] = adapted;
            trace::event!(
                "sdk_output_initial_layout",
                json!({"device":device.as_raw(),"image":b.image.as_raw(),"command":cmd.as_raw(),"original_old":b.old_layout.as_raw(),"old":adapted.old_layout.as_raw(),"new":adapted.new_layout.as_raw(),"fresh_bound_named_clone":true})
            );
        }
    }
    copy
}
#[cfg(test)]
mod tests {
    use super::*;
    fn barrier() -> vk::ImageMemoryBarrier<'static> {
        vk::ImageMemoryBarrier::default()
            .image(vk::Image::from_raw(2))
            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
            .dst_access_mask(vk::AccessFlags::SHADER_READ | vk::AccessFlags::SHADER_WRITE)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .subresource_range(
                vk::ImageSubresourceRange::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .level_count(vk::REMAINING_MIP_LEVELS)
                    .layer_count(vk::REMAINING_ARRAY_LAYERS),
            )
    }
    #[test]
    fn adapts_only_first_bound_named_sdk_clone_barrier() {
        let mut images = Images::default();
        let stage = vk::PipelineStageFlags::ALL_COMMANDS;
        let mut b = barrier();
        assert!(!images.adapt(1, &mut b, stage, stage));
        images.0.insert(
            (1, 2),
            Fresh {
                bound: true,
                selected: true,
            },
        );
        assert!(!images.adapt(3, &mut b, stage, stage));
        assert!(images.adapt(1, &mut b, stage, stage));
        assert_eq!(b.old_layout, vk::ImageLayout::UNDEFINED);
        assert!(b.src_access_mask.is_empty());
        assert_eq!(b.new_layout, vk::ImageLayout::GENERAL);
        assert!(!images.adapt(1, &mut barrier(), stage, stage));
    }
    #[test]
    fn unsupported_first_barrier_consumes_freshness() {
        let stage = vk::PipelineStageFlags::ALL_COMMANDS;
        for fresh in [
            Fresh::default(),
            Fresh {
                bound: true,
                selected: false,
            },
            Fresh {
                bound: false,
                selected: true,
            },
        ] {
            let mut images = Images::default();
            images.0.insert((1, 2), fresh);
            assert!(!images.adapt(1, &mut barrier(), stage, stage));
            assert!(images.0.is_empty());
        }
        let mut images = Images::default();
        images.0.insert(
            (1, 2),
            Fresh {
                bound: true,
                selected: true,
            },
        );
        let mut b = barrier();
        b.old_layout = vk::ImageLayout::GENERAL;
        assert!(!images.adapt(1, &mut b, stage, stage));
        assert!(!images.adapt(1, &mut barrier(), stage, stage));
    }
}
