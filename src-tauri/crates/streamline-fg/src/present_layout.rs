//! The SDK exposes ordinary transfer-source images as a virtual swapchain.
//! Translate application PRESENT layouts only for images returned by that proxy.
use super::*;
use std::collections::HashSet;

#[derive(Default)]
struct Proxies(HashMap<(u64, u64), HashSet<u64>>, HashSet<(u64, u64)>);
impl Proxies {
    fn contains(&self, device: u64, image: u64) -> bool {
        self.1.contains(&(device, image))
            && self
                .0
                .iter()
                .any(|((d, _), images)| *d == device && images.contains(&image))
    }
}
static PROXIES: OnceLock<Mutex<Proxies>> = OnceLock::new();
#[cfg(all(windows, feature = "sdk-bridge"))]
pub(super) fn created(device: vk::Device, chain: vk::SwapchainKHR) {
    PROXIES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .0
        .entry((device.as_raw(), chain.as_raw()))
        .or_default();
}
pub(super) fn images(device: u64, chain: u64, images: &[vk::Image]) {
    let mut proxies = PROXIES.get_or_init(Default::default).lock().unwrap();
    if let Some(owned) = proxies.0.get_mut(&(device, chain)) {
        let old = owned.len();
        owned.extend(images.iter().map(|image| image.as_raw()));
        if owned.len() != old {
            let proxy_images = &proxies.0[&(device, chain)];
            let ordinary = proxy_images
                .iter()
                .filter(|&&image| proxies.1.contains(&(device, image)))
                .count();
            trace::event!(
                "proxy_present_images",
                json!({"device":device,"swapchain":chain,"images":proxy_images.len(),"sdk_created_ordinary_images":ordinary,"native_images_unchanged":proxy_images.len()-ordinary})
            );
        }
    }
}
pub(super) fn retired(device: vk::Device, chain: vk::SwapchainKHR) {
    PROXIES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .0
        .remove(&(device.as_raw(), chain.as_raw()));
}
pub(super) fn retired_device(device: vk::Device) {
    let mut proxies = PROXIES.get_or_init(Default::default).lock().unwrap();
    proxies.0.retain(|(d, _), _| *d != device.as_raw());
    proxies.1.retain(|(d, _)| *d != device.as_raw());
}
unsafe extern "system" fn create_image(
    handle: vk::Device,
    info: *const vk::ImageCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    output: *mut vk::Image,
) -> vk::Result {
    let Some(d) = device(handle) else {
        return vk::Result::ERROR_DEVICE_LOST;
    };
    let next: vk::PFN_vkCreateImage =
        std::mem::transmute((d.gdpa)(d.handle, c"vkCreateImage".as_ptr()).unwrap());
    let result = next(handle, info, alloc, output);
    if result == vk::Result::SUCCESS {
        crate::sdk_layout_trace::created(handle, *output, &*info);
        crate::sdk_output_layout::created(handle, *output, &*info);
        crate::sdk_transfer_access::created(handle, *output, &*info);
        PROXIES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .1
            .insert((handle.as_raw(), (*output).as_raw()));
    }
    result
}
unsafe extern "system" fn destroy_image(
    handle: vk::Device,
    image: vk::Image,
    alloc: *const vk::AllocationCallbacks,
) {
    let Some(d) = device(handle) else {
        std::process::abort()
    };
    let next: vk::PFN_vkDestroyImage =
        std::mem::transmute((d.gdpa)(d.handle, c"vkDestroyImage".as_ptr()).unwrap());
    next(handle, image, alloc);
    crate::sdk_layout_trace::destroyed(handle, image);
    crate::sdk_output_layout::used(handle, image);
    crate::sdk_transfer_access::destroyed(handle, image);
    PROXIES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .1
        .remove(&(handle.as_raw(), image.as_raw()));
}
fn translated(layout: vk::ImageLayout) -> vk::ImageLayout {
    if layout == vk::ImageLayout::PRESENT_SRC_KHR {
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL
    } else {
        layout
    }
}
fn adapt(barrier: &mut vk::ImageMemoryBarrier, owned: bool) -> bool {
    if !owned
        || (barrier.old_layout != vk::ImageLayout::PRESENT_SRC_KHR
            && barrier.new_layout != vk::ImageLayout::PRESENT_SRC_KHR)
    {
        return false;
    }
    barrier.old_layout = translated(barrier.old_layout);
    barrier.new_layout = translated(barrier.new_layout);
    // The virtual PRESENT transition must order the application's attachment
    // store before the SDK pacer's transfer read, even if the app supplied TOP/0.
    barrier.src_access_mask |= vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE;
    barrier.dst_access_mask |= vk::AccessFlags::MEMORY_READ;
    true
}
fn adapt2(barrier: &mut vk::ImageMemoryBarrier2, owned: bool) -> bool {
    if !owned
        || (barrier.old_layout != vk::ImageLayout::PRESENT_SRC_KHR
            && barrier.new_layout != vk::ImageLayout::PRESENT_SRC_KHR)
    {
        return false;
    }
    barrier.old_layout = translated(barrier.old_layout);
    barrier.new_layout = translated(barrier.new_layout);
    barrier.src_stage_mask |= vk::PipelineStageFlags2::ALL_COMMANDS;
    barrier.dst_stage_mask |= vk::PipelineStageFlags2::ALL_COMMANDS;
    barrier.src_access_mask |= vk::AccessFlags2::MEMORY_READ | vk::AccessFlags2::MEMORY_WRITE;
    barrier.dst_access_mask |= vk::AccessFlags2::MEMORY_READ;
    true
}
unsafe fn barrier2_impl(cmd: vk::CommandBuffer, info: *const vk::DependencyInfo, name: &CStr) {
    let Some(d) = device(cmd) else {
        std::process::abort()
    };
    let next: vk::PFN_vkCmdPipelineBarrier2 =
        std::mem::transmute((d.gdpa)(d.handle, name.as_ptr()).unwrap());
    if info.is_null() {
        next(cmd, info);
        return;
    }
    let source = crate::scale_probe::items(
        (*info).p_image_memory_barriers,
        (*info).image_memory_barrier_count,
    );
    let copy = {
        let proxies = PROXIES.get_or_init(Default::default).lock().unwrap();
        if source.iter().any(|b| {
            proxies.contains(d.handle.as_raw(), b.image.as_raw())
                && (b.old_layout == vk::ImageLayout::PRESENT_SRC_KHR
                    || b.new_layout == vk::ImageLayout::PRESENT_SRC_KHR)
        }) {
            let mut copy = source.to_vec();
            for b in &mut copy {
                adapt2(b, proxies.contains(d.handle.as_raw(), b.image.as_raw()));
            }
            Some(copy)
        } else {
            None
        }
    };
    if let Some(images) = copy {
        let mut dependency = *info;
        dependency.p_image_memory_barriers = images.as_ptr();
        next(cmd, &dependency);
    } else {
        next(cmd, info);
    }
}
unsafe extern "system" fn barrier2(cmd: vk::CommandBuffer, info: *const vk::DependencyInfo) {
    barrier2_impl(cmd, info, c"vkCmdPipelineBarrier2");
}
unsafe extern "system" fn barrier2_khr(cmd: vk::CommandBuffer, info: *const vk::DependencyInfo) {
    barrier2_impl(cmd, info, c"vkCmdPipelineBarrier2KHR");
}
unsafe extern "system" fn barrier(
    cmd: vk::CommandBuffer,
    src: vk::PipelineStageFlags,
    dst: vk::PipelineStageFlags,
    flags: vk::DependencyFlags,
    memory_count: u32,
    memory: *const vk::MemoryBarrier,
    buffer_count: u32,
    buffers: *const vk::BufferMemoryBarrier,
    image_count: u32,
    images: *const vk::ImageMemoryBarrier,
) {
    let Some(d) = device(cmd) else {
        std::process::abort()
    };
    let next: vk::PFN_vkCmdPipelineBarrier =
        std::mem::transmute((d.gdpa)(d.handle, c"vkCmdPipelineBarrier".as_ptr()).unwrap());
    let source = crate::scale_probe::items(images, image_count);
    crate::sdk_layout_trace::buffer_barriers(
        d.handle,
        cmd,
        src,
        dst,
        crate::scale_probe::items(buffers, buffer_count),
    );
    let buffer_copy = crate::sdk_transfer_access::buffer_barriers(
        d.handle,
        cmd,
        src,
        dst,
        crate::scale_probe::items(buffers, buffer_count),
    );
    crate::sdk_layout_trace::barriers(d.handle, cmd, src, dst, source);
    let mut copy = crate::sdk_output_layout::barriers(d.handle, cmd, src, dst, source);
    // First-use layout adaptation must run before widening the captured masks.
    if let Some(changed) = crate::sdk_transfer_access::barriers(
        d.handle,
        cmd,
        src,
        dst,
        copy.as_deref().unwrap_or(source),
    ) {
        copy = Some(changed);
    }
    {
        let proxies = PROXIES.get_or_init(Default::default).lock().unwrap();
        if source.iter().any(|b| {
            proxies.contains(d.handle.as_raw(), b.image.as_raw())
                && (b.old_layout == vk::ImageLayout::PRESENT_SRC_KHR
                    || b.new_layout == vk::ImageLayout::PRESENT_SRC_KHR)
        }) {
            let mut changed = copy.take().unwrap_or_else(|| source.to_vec());
            for b in &mut changed {
                adapt(b, proxies.contains(d.handle.as_raw(), b.image.as_raw()));
            }
            copy = Some(changed);
        }
    }
    let (src, dst, images) = if let Some(copy) = &copy {
        (
            src | vk::PipelineStageFlags::ALL_COMMANDS,
            dst | vk::PipelineStageFlags::ALL_COMMANDS,
            copy.as_ptr(),
        )
    } else {
        (src, dst, images)
    };
    next(
        cmd,
        src,
        dst,
        flags,
        memory_count,
        memory,
        buffer_count,
        buffer_copy.as_ref().map_or(buffers, |copy| copy.as_ptr()),
        image_count,
        images,
    );
}
pub(super) unsafe fn barrier_intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    match name.to_bytes() {
        b"vkCmdPipelineBarrier" => Some(std::mem::transmute(barrier as *const ())),
        b"vkCmdPipelineBarrier2" => Some(std::mem::transmute(barrier2 as *const ())),
        b"vkCmdPipelineBarrier2KHR" => Some(std::mem::transmute(barrier2_khr as *const ())),
        _ => None,
    }
}
pub(super) unsafe fn intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    match name.to_bytes() {
        b"vkCreateImage" => Some(std::mem::transmute(create_image as *const ())),
        b"vkDestroyImage" => Some(std::mem::transmute(destroy_image as *const ())),
        _ => barrier_intercept(name),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn virtual_present_orders_attachment_write_and_maps_both_directions() {
        let mut handoff = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR);
        assert!(adapt(&mut handoff, true));
        assert_eq!(handoff.new_layout, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        assert!(handoff
            .src_access_mask
            .contains(vk::AccessFlags::MEMORY_WRITE));
        assert!(handoff
            .dst_access_mask
            .contains(vk::AccessFlags::MEMORY_READ));
        let mut acquire = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .new_layout(vk::ImageLayout::GENERAL);
        assert!(adapt(&mut acquire, true));
        assert_eq!(acquire.old_layout, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        let mut native =
            vk::ImageMemoryBarrier::default().new_layout(vk::ImageLayout::PRESENT_SRC_KHR);
        assert!(!adapt(&mut native, false));
        assert_eq!(native.new_layout, vk::ImageLayout::PRESENT_SRC_KHR);
        let mut unrelated = vk::ImageMemoryBarrier::default()
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::GENERAL);
        assert!(!adapt(&mut unrelated, true));
    }
    #[test]
    fn membership_is_scoped_to_proxy_and_device_lifetime() {
        let mut proxies = Proxies::default();
        proxies.0.insert((1, 2), HashSet::from([3]));
        assert!(!proxies.contains(1, 3)); // Native swapchain images are not vkCreateImage objects.
        proxies.1.insert((1, 3));
        assert!(proxies.contains(1, 3));
        assert!(!proxies.contains(2, 3));
        proxies.0.remove(&(1, 2));
        assert!(!proxies.contains(1, 3));
    }
    #[test]
    fn synchronization2_preserves_ownership_range_and_other_dependencies() {
        let range = vk::ImageSubresourceRange::default()
            .base_mip_level(2)
            .level_count(1)
            .base_array_layer(3)
            .layer_count(1)
            .aspect_mask(vk::ImageAspectFlags::COLOR);
        let mut b = vk::ImageMemoryBarrier2::default()
            .image(vk::Image::from_raw(42))
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .src_queue_family_index(3)
            .dst_queue_family_index(4)
            .subresource_range(range);
        assert!(!adapt2(&mut b, false));
        assert_eq!(b.new_layout, vk::ImageLayout::PRESENT_SRC_KHR);
        assert!(adapt2(&mut b, true));
        assert_eq!(b.new_layout, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        assert_eq!((b.src_queue_family_index, b.dst_queue_family_index), (3, 4));
        assert_eq!(
            (
                b.subresource_range.base_mip_level,
                b.subresource_range.base_array_layer
            ),
            (2, 3)
        );
        assert!(b.src_stage_mask.contains(
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
                | vk::PipelineStageFlags2::ALL_COMMANDS
        ));
        assert!(b
            .src_access_mask
            .contains(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE | vk::AccessFlags2::MEMORY_WRITE));
        assert!(b.dst_access_mask.contains(vk::AccessFlags2::MEMORY_READ));
        b.old_layout = vk::ImageLayout::PRESENT_SRC_KHR;
        b.new_layout = vk::ImageLayout::GENERAL;
        assert!(adapt2(&mut b, true));
        assert_eq!(b.old_layout, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        assert!(!adapt2(&mut b, true));
    }
}
