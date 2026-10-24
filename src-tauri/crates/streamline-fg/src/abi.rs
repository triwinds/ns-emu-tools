//! Loader-only ABI from Vulkan-Headers v1.4.341 include/vulkan/vk_layer.h.
//! Vulkan application ABI is provided by ash. This probe supports x64 only.
use ash::vk;
use std::ffi::c_void;

#[repr(C)]
pub struct Negotiation {
    pub s_type: u32,
    pub next: *mut c_void,
    pub version: u32,
    pub gipa: Option<vk::PFN_vkGetInstanceProcAddr>,
    pub gdpa: Option<vk::PFN_vkGetDeviceProcAddr>,
    pub physical_gpa: Option<vk::PFN_vkGetInstanceProcAddr>,
}

#[repr(C)]
pub struct InstanceLink {
    pub next: *mut InstanceLink,
    pub gipa: vk::PFN_vkGetInstanceProcAddr,
    pub physical_gpa: Option<vk::PFN_vkGetInstanceProcAddr>,
}

#[repr(C)]
pub struct DeviceLink {
    pub next: *mut DeviceLink,
    pub gipa: vk::PFN_vkGetInstanceProcAddr,
    pub gdpa: vk::PFN_vkGetDeviceProcAddr,
}

// The instance union contains a two-pointer callback pair, unlike the device union.
#[repr(C)]
pub struct InstanceChain {
    pub s_type: vk::StructureType,
    pub next: *const c_void,
    pub function: u32,
    pub data: [usize; 2],
}

#[repr(C)]
pub struct DeviceChain {
    pub s_type: vk::StructureType,
    pub next: *const c_void,
    pub function: u32,
    pub data: usize,
}

pub type SetDeviceLoaderData = unsafe extern "system" fn(vk::Device, *mut c_void) -> vk::Result;

pub unsafe fn find_link(next: *const c_void, ty: vk::StructureType) -> *mut c_void {
    find_function(next, ty, 0)
}

pub unsafe fn find_function(
    mut next: *const c_void,
    ty: vk::StructureType,
    function: u32,
) -> *mut c_void {
    // Valid Vulkan input chains are finite. Bound traversal for this diagnostic DLL.
    for _ in 0..256 {
        if next.is_null() {
            break;
        }
        let base = &*next.cast::<vk::BaseInStructure>();
        if base.s_type == ty {
            let chain = next.cast::<DeviceChain>();
            if (*chain).function == function {
                return next.cast_mut();
            }
        }
        next = base.p_next.cast();
    }
    std::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn x64_loader_layout_and_link_search() {
        if cfg!(target_pointer_width = "64") {
            assert_eq!(size_of::<Negotiation>(), 48);
            assert_eq!(size_of::<InstanceChain>(), 40);
            assert_eq!(size_of::<DeviceChain>(), 32);
            assert_eq!(std::mem::offset_of!(InstanceChain, data), 24);
            assert_eq!(std::mem::offset_of!(Negotiation, gipa), 24);
        }
        let link = DeviceChain {
            s_type: vk::StructureType::LOADER_DEVICE_CREATE_INFO,
            next: std::ptr::null(),
            function: 0,
            data: 123,
        };
        let callback = DeviceChain {
            s_type: link.s_type,
            next: (&link as *const DeviceChain).cast(),
            function: 1,
            data: 0,
        };
        unsafe {
            assert_eq!(
                find_link((&callback as *const DeviceChain).cast(), link.s_type),
                (&link as *const DeviceChain).cast_mut().cast()
            );
            assert!(find_link(std::ptr::null(), link.s_type).is_null());
            assert_eq!(
                find_function((&callback as *const DeviceChain).cast(), link.s_type, 1),
                (&callback as *const DeviceChain).cast_mut().cast()
            );
            assert!(find_function((&link as *const DeviceChain).cast(), link.s_type, 1).is_null());
        }
    }
}
