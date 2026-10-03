//! Owned creation parameters for emulator targets. Borrowed pNext memory is never patched.
use crate::abi::DeviceChain;
use ash::vk;
use std::collections::HashSet;
use std::ffi::c_void;

pub(super) struct FeatureChain {
    storage: Vec<Box<[u64]>>,
    head: *const c_void,
}
impl FeatureChain {
    pub unsafe fn for_target(node: *const c_void) -> Result<Self, &'static str> {
        Self::for_target_with_nr(node, false)
    }
    pub unsafe fn for_target_with_nr(
        mut node: *const c_void,
        nr: bool,
    ) -> Result<Self, &'static str> {
        let mut result = Self {
            storage: Vec::new(),
            head: std::ptr::null(),
        };
        let mut seen = HashSet::new();
        while !node.is_null() {
            if result.storage.len() >= 64 {
                return Err("device feature chain too long");
            }
            let base = &*node.cast::<vk::BaseInStructure>();
            if base.s_type != vk::StructureType::LOADER_DEVICE_CREATE_INFO
                && !seen.insert(base.s_type)
            {
                return Err("duplicate device feature structure");
            }
            macro_rules! sizes { ($($t:ty),*)=>{{ let mut size=None; $(if base.s_type==<$t as vk::TaggedStructure>::STRUCTURE_TYPE {size=Some(std::mem::size_of::<$t>());})* size }}; }
            let size = if base.s_type == vk::StructureType::LOADER_DEVICE_CREATE_INFO {
                std::mem::size_of::<DeviceChain>()
            } else {
                sizes!(
                    vk::PhysicalDeviceFeatures2,
                    vk::PhysicalDeviceVulkan11Features,
                    vk::PhysicalDeviceVulkan12Features,
                    vk::PhysicalDeviceVulkan13Features,
                    vk::PhysicalDevice16BitStorageFeatures,
                    vk::PhysicalDevice8BitStorageFeatures,
                    vk::PhysicalDeviceShaderAtomicInt64Features,
                    vk::PhysicalDeviceShaderDrawParametersFeatures,
                    vk::PhysicalDeviceShaderFloat16Int8Features,
                    vk::PhysicalDeviceUniformBufferStandardLayoutFeatures,
                    vk::PhysicalDeviceVariablePointersFeatures,
                    vk::PhysicalDeviceDescriptorIndexingFeatures,
                    vk::PhysicalDeviceHostQueryResetFeatures,
                    vk::PhysicalDeviceTimelineSemaphoreFeatures,
                    vk::PhysicalDeviceBufferDeviceAddressFeatures,
                    vk::PhysicalDeviceImageRobustnessFeatures,
                    vk::PhysicalDeviceShaderDemoteToHelperInvocationFeatures,
                    vk::PhysicalDeviceSubgroupSizeControlFeatures,
                    vk::PhysicalDeviceMaintenance4Features,
                    vk::PhysicalDevicePrivateDataFeatures,
                    vk::PhysicalDeviceSynchronization2Features,
                    vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT,
                    vk::PhysicalDeviceTransformFeedbackFeaturesEXT,
                    vk::PhysicalDevicePrimitiveTopologyListRestartFeaturesEXT,
                    vk::PhysicalDeviceRobustness2FeaturesEXT,
                    vk::PhysicalDeviceExtendedDynamicStateFeaturesEXT,
                    vk::PhysicalDeviceExtendedDynamicState2FeaturesEXT,
                    vk::PhysicalDeviceExtendedDynamicState3FeaturesEXT,
                    vk::PhysicalDevice4444FormatsFeaturesEXT,
                    vk::PhysicalDeviceIndexTypeUint8FeaturesEXT,
                    vk::PhysicalDeviceLineRasterizationFeaturesKHR,
                    vk::PhysicalDeviceProvokingVertexFeaturesEXT,
                    vk::PhysicalDeviceVertexInputDynamicStateFeaturesEXT,
                    vk::PhysicalDeviceMaintenance5FeaturesKHR,
                    vk::PhysicalDeviceMaintenance6FeaturesKHR,
                    vk::PhysicalDevicePipelineExecutablePropertiesFeaturesKHR,
                    vk::PhysicalDeviceWorkgroupMemoryExplicitLayoutFeaturesKHR,
                    vk::PhysicalDeviceDepthBiasControlFeaturesEXT,
                    vk::PhysicalDeviceFragmentShadingRateFeaturesKHR,
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
                let features = &mut *copy
                    .as_mut_ptr()
                    .cast::<vk::PhysicalDeviceVulkan12Features>();
                features.timeline_semaphore = vk::TRUE;
                features.buffer_device_address = vk::TRUE;
                features.descriptor_indexing = vk::TRUE;
            }
            macro_rules! enable {
                ($ty:ty, $field:ident, $on:expr) => {
                    if base.s_type == <$ty as vk::TaggedStructure>::STRUCTURE_TYPE && $on {
                        (*copy.as_mut_ptr().cast::<$ty>()).$field = vk::TRUE;
                    }
                };
            }
            enable!(
                vk::PhysicalDeviceTimelineSemaphoreFeatures,
                timeline_semaphore,
                true
            );
            enable!(
                vk::PhysicalDeviceBufferDeviceAddressFeatures,
                buffer_device_address,
                true
            );
            enable!(vk::PhysicalDeviceVulkan13Features, synchronization2, true);
            enable!(vk::PhysicalDeviceVulkan13Features, maintenance4, nr);
            enable!(vk::PhysicalDeviceVulkan13Features, private_data, nr);
            enable!(
                vk::PhysicalDeviceSynchronization2Features,
                synchronization2,
                true
            );
            enable!(vk::PhysicalDeviceMaintenance4Features, maintenance4, nr);
            enable!(vk::PhysicalDevicePrivateDataFeatures, private_data, nr);
            enable!(
                vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT,
                swapchain_maintenance1,
                true
            );
            result.storage.push(copy);
            node = base.p_next.cast();
        }
        let aggregate12 = seen.contains(&vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_2_FEATURES);
        let individual12 = [
            vk::StructureType::PHYSICAL_DEVICE_TIMELINE_SEMAPHORE_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_BUFFER_DEVICE_ADDRESS_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_DESCRIPTOR_INDEXING_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_8BIT_STORAGE_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_SHADER_ATOMIC_INT64_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_SHADER_FLOAT16_INT8_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_UNIFORM_BUFFER_STANDARD_LAYOUT_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_HOST_QUERY_RESET_FEATURES,
        ]
        .iter()
        .any(|ty| seen.contains(ty));
        let aggregate13 = seen.contains(&vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_3_FEATURES);
        let individual13 = [
            vk::StructureType::PHYSICAL_DEVICE_IMAGE_ROBUSTNESS_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_SHADER_DEMOTE_TO_HELPER_INVOCATION_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_SUBGROUP_SIZE_CONTROL_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_MAINTENANCE_4_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_PRIVATE_DATA_FEATURES,
            vk::StructureType::PHYSICAL_DEVICE_SYNCHRONIZATION_2_FEATURES,
        ]
        .iter()
        .any(|ty| seen.contains(ty));
        if (aggregate12 && individual12) || (aggregate13 && individual13) {
            return Err("mixed aggregate and individual Vulkan features");
        }
        if !aggregate12 {
            if !seen.contains(&vk::StructureType::PHYSICAL_DEVICE_TIMELINE_SEMAPHORE_FEATURES) {
                result.prepend_copy(
                    &vk::PhysicalDeviceTimelineSemaphoreFeatures::default()
                        .timeline_semaphore(true),
                );
            }
            if !seen.contains(&vk::StructureType::PHYSICAL_DEVICE_BUFFER_DEVICE_ADDRESS_FEATURES) {
                result.prepend_copy(
                    &vk::PhysicalDeviceBufferDeviceAddressFeatures::default()
                        .buffer_device_address(true),
                );
            }
        }
        if !aggregate13 {
            if individual13 {
                if !seen.contains(&vk::StructureType::PHYSICAL_DEVICE_SYNCHRONIZATION_2_FEATURES) {
                    result.prepend_copy(
                        &vk::PhysicalDeviceSynchronization2Features::default()
                            .synchronization2(true),
                    );
                }
                if nr {
                    if !seen.contains(&vk::StructureType::PHYSICAL_DEVICE_MAINTENANCE_4_FEATURES) {
                        result.prepend_copy(
                            &vk::PhysicalDeviceMaintenance4Features::default().maintenance4(true),
                        );
                    }
                    if !seen.contains(&vk::StructureType::PHYSICAL_DEVICE_PRIVATE_DATA_FEATURES) {
                        result.prepend_copy(
                            &vk::PhysicalDevicePrivateDataFeatures::default().private_data(true),
                        );
                    }
                }
            } else {
                result.prepend_copy(
                    &vk::PhysicalDeviceVulkan13Features::default()
                        .synchronization2(true)
                        .maintenance4(nr)
                        .private_data(nr),
                );
            }
        }
        if !seen.contains(&vk::StructureType::PHYSICAL_DEVICE_SWAPCHAIN_MAINTENANCE_1_FEATURES_EXT)
        {
            result.prepend_copy(
                &vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default()
                    .swapchain_maintenance1(true),
            );
        }
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
    /// Features2 and pEnabledFeatures are mutually exclusive in DeviceCreateInfo.
    pub fn enable_storage_image_formats(&mut self) -> bool {
        for data in &mut self.storage {
            unsafe {
                let base = &*data.as_ptr().cast::<vk::BaseInStructure>();
                if base.s_type == vk::StructureType::PHYSICAL_DEVICE_FEATURES_2 {
                    (*data.as_mut_ptr().cast::<vk::PhysicalDeviceFeatures2>())
                        .features
                        .shader_storage_image_extended_formats = vk::TRUE;
                    return true;
                }
            }
        }
        false
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
    unsafe fn feature<T: vk::TaggedStructure>(chain: &FeatureChain) -> &T {
        let data = chain
            .storage
            .iter()
            .find(|data| (*data.as_ptr().cast::<vk::BaseInStructure>()).s_type == T::STRUCTURE_TYPE)
            .unwrap();
        &*data.as_ptr().cast::<T>()
    }

    #[test]
    fn eden_features2_preserves_individual_features_without_adding_aggregate_duplicates() {
        unsafe {
            let mut maintenance =
                vk::PhysicalDeviceMaintenance4Features::default().maintenance4(true);
            let mut timeline = vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
            let mut indexing = vk::PhysicalDeviceDescriptorIndexingFeatures::default()
                .descriptor_binding_partially_bound(true);
            let mut root = vk::PhysicalDeviceFeatures2::default()
                .features(vk::PhysicalDeviceFeatures::default().geometry_shader(true));
            timeline.p_next =
                (&mut maintenance as *mut vk::PhysicalDeviceMaintenance4Features).cast();
            indexing.p_next =
                (&mut timeline as *mut vk::PhysicalDeviceTimelineSemaphoreFeatures).cast();
            root.p_next =
                (&mut indexing as *mut vk::PhysicalDeviceDescriptorIndexingFeatures).cast();
            let original_next = root.p_next;
            let mut chain = FeatureChain::for_target_with_nr(
                (&root as *const vk::PhysicalDeviceFeatures2).cast(),
                true,
            )
            .unwrap();
            assert_eq!(root.p_next, original_next);
            assert_eq!(timeline.timeline_semaphore, vk::FALSE);
            assert_eq!(
                feature::<vk::PhysicalDeviceTimelineSemaphoreFeatures>(&chain).timeline_semaphore,
                vk::TRUE
            );
            assert_eq!(
                feature::<vk::PhysicalDeviceMaintenance4Features>(&chain).maintenance4,
                vk::TRUE
            );
            assert_eq!(
                feature::<vk::PhysicalDevicePrivateDataFeatures>(&chain).private_data,
                vk::TRUE
            );
            assert_eq!(
                feature::<vk::PhysicalDeviceSynchronization2Features>(&chain).synchronization2,
                vk::TRUE
            );
            assert_eq!(
                feature::<vk::PhysicalDeviceBufferDeviceAddressFeatures>(&chain)
                    .buffer_device_address,
                vk::TRUE
            );
            assert_eq!(
                feature::<vk::PhysicalDeviceDescriptorIndexingFeatures>(&chain)
                    .descriptor_binding_partially_bound,
                vk::TRUE
            );
            assert!(chain.enable_storage_image_formats());
            let core = &feature::<vk::PhysicalDeviceFeatures2>(&chain).features;
            assert_eq!(core.geometry_shader, vk::TRUE);
            assert_eq!(core.shader_storage_image_extended_formats, vk::TRUE);
            assert_eq!(
                root.features.shader_storage_image_extended_formats,
                vk::FALSE
            );
            let mut seen = HashSet::new();
            for data in &chain.storage {
                let ty = (*data.as_ptr().cast::<vk::BaseInStructure>()).s_type;
                assert!(seen.insert(ty));
            }
            assert!(!seen.contains(&vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_2_FEATURES));
            assert!(!seen.contains(&vk::StructureType::PHYSICAL_DEVICE_VULKAN_1_3_FEATURES));
            // The chain still borrows no part of the application's nodes.
            root.features.geometry_shader = vk::FALSE;
            assert_eq!(root.features.geometry_shader, vk::FALSE);
            assert_eq!(
                feature::<vk::PhysicalDeviceFeatures2>(&chain)
                    .features
                    .geometry_shader,
                vk::TRUE
            );
        }
    }

    #[test]
    fn citron_fragment_shading_rate_request_is_preserved_in_owned_chain() {
        unsafe {
            let mut rate = vk::PhysicalDeviceFragmentShadingRateFeaturesKHR::default()
                .pipeline_fragment_shading_rate(true)
                .primitive_fragment_shading_rate(false)
                .attachment_fragment_shading_rate(true);
            let mut subgroup = vk::PhysicalDeviceSubgroupSizeControlFeatures::default()
                .subgroup_size_control(true);
            let mut root = vk::PhysicalDeviceFeatures2::default();
            subgroup.p_next =
                (&mut rate as *mut vk::PhysicalDeviceFragmentShadingRateFeaturesKHR).cast();
            root.p_next =
                (&mut subgroup as *mut vk::PhysicalDeviceSubgroupSizeControlFeatures).cast();
            let chain = FeatureChain::for_target_with_nr(
                (&root as *const vk::PhysicalDeviceFeatures2).cast(),
                true,
            )
            .unwrap();
            let copied = feature::<vk::PhysicalDeviceFragmentShadingRateFeaturesKHR>(&chain);
            assert_eq!(copied.pipeline_fragment_shading_rate, vk::TRUE);
            assert_eq!(copied.primitive_fragment_shading_rate, vk::FALSE);
            assert_eq!(copied.attachment_fragment_shading_rate, vk::TRUE);
            assert_ne!(copied as *const _, &rate as *const _);
            assert_eq!(
                root.p_next,
                (&subgroup as *const vk::PhysicalDeviceSubgroupSizeControlFeatures)
                    .cast_mut()
                    .cast()
            );
            assert_eq!(
                feature::<vk::PhysicalDeviceSubgroupSizeControlFeatures>(&chain)
                    .subgroup_size_control,
                vk::TRUE
            );
            assert_eq!(
                feature::<vk::PhysicalDeviceSynchronization2Features>(&chain).synchronization2,
                vk::TRUE
            );
            assert_eq!(
                feature::<vk::PhysicalDeviceMaintenance4Features>(&chain).maintenance4,
                vk::TRUE
            );
        }
    }

    #[test]
    fn rejects_duplicates_and_mixed_promoted_features() {
        unsafe {
            let mut timeline = vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
            let mut duplicate = vk::PhysicalDeviceTimelineSemaphoreFeatures::default();
            duplicate.p_next =
                (&mut timeline as *mut vk::PhysicalDeviceTimelineSemaphoreFeatures).cast();
            assert!(FeatureChain::for_target(
                (&duplicate as *const vk::PhysicalDeviceTimelineSemaphoreFeatures).cast()
            )
            .is_err());
            let mut twelve = vk::PhysicalDeviceVulkan12Features::default();
            twelve.p_next =
                (&mut timeline as *mut vk::PhysicalDeviceTimelineSemaphoreFeatures).cast();
            assert!(FeatureChain::for_target(
                (&twelve as *const vk::PhysicalDeviceVulkan12Features).cast()
            )
            .is_err());
            let mut maintenance = vk::PhysicalDeviceMaintenance4Features::default();
            let mut thirteen = vk::PhysicalDeviceVulkan13Features::default();
            thirteen.p_next =
                (&mut maintenance as *mut vk::PhysicalDeviceMaintenance4Features).cast();
            assert!(FeatureChain::for_target(
                (&thirteen as *const vk::PhysicalDeviceVulkan13Features).cast()
            )
            .is_err());
            // SDK flags are merged into existing aggregate nodes, not duplicated.
            thirteen.p_next = std::ptr::null_mut();
            twelve.p_next = std::ptr::null_mut();
            twelve.p_next = (&mut thirteen as *mut vk::PhysicalDeviceVulkan13Features).cast();
            let mut chain = FeatureChain::for_target_with_nr(
                (&twelve as *const vk::PhysicalDeviceVulkan12Features).cast(),
                true,
            )
            .unwrap();
            assert!(!chain.enable_storage_image_formats());
            assert_eq!(
                feature::<vk::PhysicalDeviceVulkan13Features>(&chain).synchronization2,
                vk::TRUE
            );
            assert_eq!(thirteen.synchronization2, vk::FALSE);
        }
    }
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
            assert_eq!(thirteen.maintenance4, vk::FALSE);
            assert_eq!(thirteen.private_data, vk::FALSE);
            let nr = FeatureChain::for_target_with_nr(
                (&twelve as *const vk::PhysicalDeviceVulkan12Features).cast(),
                true,
            )
            .unwrap();
            let nr_maintenance = &*nr
                .head()
                .cast::<vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT>();
            let nr_thirteen = &*nr_maintenance
                .p_next
                .cast::<vk::PhysicalDeviceVulkan13Features>();
            assert_eq!(nr_thirteen.maintenance4, vk::TRUE);
            assert_eq!(nr_thirteen.private_data, vk::TRUE);
            assert_eq!(twelve.p_next, saved);
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
