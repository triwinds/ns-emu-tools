use super::*;
use crate::fg_api::Result;
use std::ffi::c_void;

// Copy only application features. No Streamline/NVIDIA requirements are added.
pub(super) unsafe fn copy_features(mut node: *const c_void) -> Result<Vec<Box<[u64]>>> {
    let mut storage = Vec::new();
    while !node.is_null() {
        if storage.len() >= 64 {
            return Err("feature chain exceeds 64 nodes".into());
        }
        let base = &*node.cast::<vk::BaseInStructure>();
        macro_rules! sizes {($($ty:ty),*)=>{{let mut n=None;$(if base.s_type==<$ty as vk::TaggedStructure>::STRUCTURE_TYPE{n=Some(std::mem::size_of::<$ty>());})*n}};}
        let size = if base.s_type == vk::StructureType::LOADER_DEVICE_CREATE_INFO {
            Some(std::mem::size_of::<DeviceChain>())
        } else {
            sizes!(
                vk::PhysicalDeviceFeatures2,
                vk::PhysicalDeviceVulkan11Features,
                vk::PhysicalDeviceVulkan12Features,
                vk::PhysicalDeviceVulkan13Features,
                vk::PhysicalDeviceAttachmentFeedbackLoopDynamicStateFeaturesEXT,
                vk::PhysicalDeviceAttachmentFeedbackLoopLayoutFeaturesEXT,
                vk::PhysicalDeviceDepthClipControlFeaturesEXT,
                vk::PhysicalDeviceCustomBorderColorFeaturesEXT,
                vk::PhysicalDeviceFragmentShaderInterlockFeaturesEXT,
                vk::PhysicalDeviceIndexTypeUint8FeaturesEXT,
                vk::PhysicalDeviceExtendedDynamicStateFeaturesEXT,
                vk::PhysicalDeviceRobustness2FeaturesEXT,
                vk::PhysicalDevicePrimitiveTopologyListRestartFeaturesEXT,
                vk::PhysicalDeviceTransformFeedbackFeaturesEXT,
                vk::PhysicalDeviceShaderFloat16Int8Features,
                vk::PhysicalDevice8BitStorageFeatures,
                vk::PhysicalDevice16BitStorageFeatures,
                vk::PhysicalDeviceTimelineSemaphoreFeatures,
                vk::PhysicalDeviceSynchronization2Features,
                vk::PhysicalDeviceDynamicRenderingFeatures,
                vk::PhysicalDeviceDescriptorIndexingFeatures,
                vk::PhysicalDeviceBufferDeviceAddressFeatures,
                vk::PhysicalDeviceHostQueryResetFeatures,
                vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT
            )
        }
        .ok_or_else(|| format!("unsupported application feature {:?}", base.s_type))?;
        let mut copy = vec![0u64; size.div_ceil(8)].into_boxed_slice();
        std::ptr::copy_nonoverlapping(node.cast::<u8>(), copy.as_mut_ptr().cast(), size);
        storage.push(copy);
        node = base.p_next.cast();
    }
    for i in 0..storage.len() {
        let next = storage
            .get(i + 1)
            .map_or(std::ptr::null_mut(), |b| b.as_ptr().cast_mut().cast());
        (*storage[i].as_mut_ptr().cast::<vk::BaseOutStructure>()).p_next = next;
    }
    Ok(storage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_feature_chain_owns_nodes_and_preserves_application_requests() {
        unsafe {
            let mut storage16 =
                vk::PhysicalDevice16BitStorageFeatures::default().storage_buffer16_bit_access(true);
            let mut original =
                vk::PhysicalDeviceVulkan12Features::default().timeline_semaphore(true);
            original.p_next =
                (&mut storage16 as *mut vk::PhysicalDevice16BitStorageFeatures).cast();
            let copy =
                copy_features((&original as *const vk::PhysicalDeviceVulkan12Features).cast())
                    .unwrap();
            let head = &*copy[0]
                .as_ptr()
                .cast::<vk::PhysicalDeviceVulkan12Features>();
            assert_eq!(head.timeline_semaphore, vk::TRUE);
            assert_ne!(head.p_next, original.p_next);
            let tail = &*head.p_next.cast::<vk::PhysicalDevice16BitStorageFeatures>();
            assert_eq!(tail.storage_buffer16_bit_access, vk::TRUE);
            assert!(tail.p_next.is_null());
            assert_eq!(
                original.p_next.cast_const(),
                &storage16 as *const _ as *const c_void
            );
        }
    }

    #[test]
    fn unknown_feature_is_rejected_instead_of_copying_an_unknown_size() {
        let unknown = vk::BaseInStructure {
            s_type: vk::StructureType::from_raw(i32::MAX),
            ..Default::default()
        };
        assert!(unsafe { copy_features((&unknown as *const vk::BaseInStructure).cast()) }.is_err());
    }
    #[test]
    fn intel_mutable_descriptor_requirement_survives_combined_fg_copy() {
        unsafe {
            let mut timeline =
                vk::PhysicalDeviceTimelineSemaphoreFeatures::default().timeline_semaphore(true);
            let mut mutable = vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT::default()
                .mutable_descriptor_type(true);
            mutable.p_next =
                (&mut timeline as *mut vk::PhysicalDeviceTimelineSemaphoreFeatures).cast();
            let copy = copy_features(
                (&mutable as *const vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT).cast(),
            )
            .unwrap();
            let head = &*copy[0]
                .as_ptr()
                .cast::<vk::PhysicalDeviceMutableDescriptorTypeFeaturesEXT>();
            assert_eq!(head.mutable_descriptor_type, vk::TRUE);
            assert_ne!(head.p_next, mutable.p_next);
            assert_eq!(
                (*head
                    .p_next
                    .cast::<vk::PhysicalDeviceTimelineSemaphoreFeatures>())
                .timeline_semaphore,
                vk::TRUE
            );
            assert!(timeline.p_next.is_null());
        }
    }
}
