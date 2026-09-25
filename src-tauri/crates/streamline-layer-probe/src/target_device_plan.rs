//! Owned creation parameters for the frozen target. Application pNext memory is never patched.
use crate::abi::DeviceChain;
use ash::vk;
use std::ffi::c_void;

pub(super) struct FeatureChain {
    storage: Vec<Box<[u64]>>,
    head: *const c_void,
}
impl FeatureChain {
    pub unsafe fn for_target(mut node: *const c_void) -> Result<Self, &'static str> {
        let mut result = Self {
            storage: Vec::new(),
            head: std::ptr::null(),
        };
        let mut saw12 = false;
        while !node.is_null() {
            if result.storage.len() >= 64 {
                return Err("device feature chain too long");
            }
            let base = &*node.cast::<vk::BaseInStructure>();
            macro_rules! sizes { ($($t:ty),*)=>{{ let mut size=None; $(if base.s_type==<$t as vk::TaggedStructure>::STRUCTURE_TYPE {size=Some(std::mem::size_of::<$t>());})* size }}; }
            let size = if base.s_type == vk::StructureType::LOADER_DEVICE_CREATE_INFO {
                std::mem::size_of::<DeviceChain>()
            } else {
                sizes!(
                    vk::PhysicalDeviceVulkan11Features,
                    vk::PhysicalDeviceVulkan12Features,
                    vk::PhysicalDeviceTransformFeedbackFeaturesEXT,
                    vk::PhysicalDevicePrimitiveTopologyListRestartFeaturesEXT,
                    vk::PhysicalDeviceRobustness2FeaturesEXT,
                    vk::PhysicalDeviceExtendedDynamicStateFeaturesEXT,
                    vk::PhysicalDeviceIndexTypeUint8FeaturesEXT,
                    vk::PhysicalDeviceFragmentShaderInterlockFeaturesEXT,
                    vk::PhysicalDeviceCustomBorderColorFeaturesEXT,
                    vk::PhysicalDeviceDepthClipControlFeaturesEXT,
                    vk::PhysicalDeviceAttachmentFeedbackLoopLayoutFeaturesEXT,
                    vk::PhysicalDeviceAttachmentFeedbackLoopDynamicStateFeaturesEXT
                )
                .ok_or("unsupported target pNext structure")?
            };
            let mut copy = vec![0u64; size.div_ceil(8)].into_boxed_slice();
            std::ptr::copy_nonoverlapping(node.cast::<u8>(), copy.as_mut_ptr().cast(), size);
            if base.s_type == vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_2_FEATURES {
                if saw12 {
                    return Err("duplicate Vulkan 1.2 feature structure");
                }
                saw12 = true;
                let features = &mut *copy
                    .as_mut_ptr()
                    .cast::<vk::PhysicalDeviceVulkan12Features>();
                features.timeline_semaphore = vk::TRUE;
                features.buffer_device_address = vk::TRUE;
                features.descriptor_indexing = vk::TRUE;
            }
            result.storage.push(copy);
            node = base.p_next.cast();
        }
        if !saw12 {
            return Err("frozen target Vulkan 1.2 features missing");
        }
        let sync = vk::PhysicalDeviceVulkan13Features::default().synchronization2(true);
        let maintenance = vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default()
            .swapchain_maintenance1(true);
        result.prepend_copy(&sync);
        result.prepend_copy(&maintenance);
        for index in 0..result.storage.len() {
            let next = if index + 1 < result.storage.len() {
                result.storage[index + 1].as_ptr().cast()
            } else {
                std::ptr::null()
            };
            let base = &mut *result.storage[index]
                .as_mut_ptr()
                .cast::<vk::BaseInStructure>();
            base.p_next = next;
        }
        result.head = result.storage[0].as_ptr().cast();
        Ok(result)
    }
    unsafe fn prepend_copy<T>(&mut self, value: &T) {
        let size = std::mem::size_of::<T>();
        let mut data = vec![0u64; size.div_ceil(8)].into_boxed_slice();
        std::ptr::copy_nonoverlapping(
            (value as *const T).cast::<u8>(),
            data.as_mut_ptr().cast(),
            size,
        );
        self.storage.insert(0, data);
    }
    pub fn head(&self) -> *const c_void {
        self.head
    }
}
#[derive(Debug, PartialEq)]
pub(super) struct QueuePlan {
    pub application_count: u32,
    pub graphics_start: u32,
    pub compute_start: u32,
    pub total: u32,
}
pub(super) fn queues(
    application_count: u32,
    available: u32,
    graphics: u32,
    compute: u32,
) -> Result<QueuePlan, &'static str> {
    if application_count == 0 || graphics != 1 || compute != 2 {
        return Err("unexpected target/SDK queue requirements");
    }
    let compute_start = application_count
        .checked_add(graphics)
        .ok_or("queue overflow")?;
    let total = compute_start.checked_add(compute).ok_or("queue overflow")?;
    if total > available {
        return Err("insufficient disjoint queues");
    }
    Ok(QueuePlan {
        application_count,
        graphics_start: application_count,
        compute_start,
        total,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reserves_sdk_queues_after_both_target_queues() {
        assert_eq!(
            queues(2, 16, 1, 2).unwrap(),
            QueuePlan {
                application_count: 2,
                graphics_start: 2,
                compute_start: 3,
                total: 5
            }
        );
        assert!(queues(2, 4, 1, 2).is_err());
        assert!(queues(u32::MAX, 16, 1, 2).is_err());
    }
    #[test]
    fn clones_features_without_modifying_the_application() {
        unsafe {
            let mut eleven =
                vk::PhysicalDeviceVulkan11Features::default().shader_draw_parameters(true);
            let mut twelve =
                vk::PhysicalDeviceVulkan12Features::default().draw_indirect_count(true);
            twelve.p_next = (&mut eleven as *mut vk::PhysicalDeviceVulkan11Features).cast();
            let saved = twelve.p_next;
            let chain = FeatureChain::for_target(
                (&twelve as *const vk::PhysicalDeviceVulkan12Features).cast(),
            )
            .unwrap();
            assert_eq!(twelve.p_next, saved);
            assert_eq!(twelve.timeline_semaphore, vk::FALSE);
            assert_eq!(twelve.buffer_device_address, vk::FALSE);
            let maintenance = &*chain
                .head()
                .cast::<vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT>();
            assert_eq!(maintenance.swapchain_maintenance1, vk::TRUE);
            let thirteen = &*maintenance
                .p_next
                .cast::<vk::PhysicalDeviceVulkan13Features>();
            assert_eq!(thirteen.synchronization2, vk::TRUE);
            let copy = &*thirteen.p_next.cast::<vk::PhysicalDeviceVulkan12Features>();
            assert_eq!(copy.timeline_semaphore, vk::TRUE);
            assert_eq!(copy.buffer_device_address, vk::TRUE);
            assert_eq!(copy.draw_indirect_count, vk::TRUE);
            assert_ne!(copy.p_next, saved);
            let unknown = vk::BaseInStructure {
                s_type: vk::StructureType::APPLICATION_INFO,
                ..Default::default()
            };
            assert!(
                FeatureChain::for_target((&unknown as *const vk::BaseInStructure).cast()).is_err()
            );
        }
    }
}
