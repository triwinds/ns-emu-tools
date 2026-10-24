//! Experimental snippet ABI, separate from Streamline feature IDs.
//! Layouts are derived from the hash-pinned NGX headers in sdk/nr-contract.json.
//! Undocumented NR entrypoints/keys were independently bound after inspecting
//! bevy_dlss5 ed5ea626 and AIO 09301f55; no upstream implementation is copied.
use std::ffi::c_void;

pub const NGX_SUCCESS: u32 = 1;
pub const NGX_INVALID_PARAMETER: u32 = 0xbad00005;
pub const NGX_API_VERSION: u32 = 0x15;
pub const NR_FEATURE: u32 = 18;
pub type VkHandle = *mut c_void;
pub type Address = *const c_void;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct InitArgs {
    pub application_id: u64,
    pub data_path: *const u16,
    pub instance: VkHandle,
    pub physical: VkHandle,
    pub device: VkHandle,
    pub gipa: Address,
    pub gdpa: Address,
    pub api_version: u32,
    pub parameters: *const c_void,
}

#[repr(C)]
pub struct Call {
    pub operation: u32,
    pub function: Address,
    pub init: *const InitArgs,
    pub device: VkHandle,
    pub command: VkHandle,
    pub parameters: *mut c_void,
    pub feature: *mut c_void,
    pub output: *mut *mut c_void,
}

pub type Init = unsafe extern "C" fn(
    u64,
    *const u16,
    VkHandle,
    VkHandle,
    VkHandle,
    Address,
    Address,
    u32,
    *const c_void,
) -> u32;
pub type Populate = unsafe extern "C" fn(*mut c_void) -> u32;
pub type Create =
    unsafe extern "C" fn(VkHandle, VkHandle, u32, *const c_void, *mut *mut c_void) -> u32;
pub type Evaluate =
    unsafe extern "C" fn(VkHandle, *const c_void, *const c_void, *const c_void) -> u32;
pub type Release = unsafe extern "C" fn(*mut c_void) -> u32;
pub type Shutdown = unsafe extern "C" fn(VkHandle) -> u32;
pub type Invoke = unsafe extern "C" fn(*const Call) -> u32;
pub fn bridge_abi() -> u64 {
    (1u64 << 48)
        | ((std::mem::size_of::<Call>() as u64) << 32)
        | ((std::mem::size_of::<InitArgs>() as u64) << 16)
        | std::mem::size_of::<ResourceVk>() as u64
}

/// Caller must own the target DLL, live Vulkan handles and parameter objects.
pub unsafe fn dispatch(call: &Call) -> u32 {
    if call.function.is_null() {
        return NGX_INVALID_PARAMETER;
    }
    match call.operation {
        0 if !call.init.is_null() => {
            let args = &*call.init;
            let f: Init = std::mem::transmute(call.function);
            f(
                args.application_id,
                args.data_path,
                args.instance,
                args.physical,
                args.device,
                args.gipa,
                args.gdpa,
                args.api_version,
                args.parameters,
            )
        }
        1 => std::mem::transmute::<Address, Populate>(call.function)(call.parameters),
        2 => std::mem::transmute::<Address, Create>(call.function)(
            call.device,
            call.command,
            NR_FEATURE,
            call.parameters,
            call.output,
        ),
        3 => std::mem::transmute::<Address, Evaluate>(call.function)(
            call.command,
            call.feature,
            call.parameters,
            std::ptr::null(),
        ),
        4 => std::mem::transmute::<Address, Release>(call.function)(call.feature),
        5 => std::mem::transmute::<Address, Shutdown>(call.function)(call.device),
        _ => NGX_INVALID_PARAMETER,
    }
}

// The image-view union occupies 48 bytes on Windows x64. ReadWrite is a C++
// bool (one byte), followed by explicit zero padding; it is not a u32 field.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ResourceVk {
    pub view: u64,
    pub image: u64,
    pub aspect: u32,
    pub base_mip: u32,
    pub level_count: u32,
    pub base_layer: u32,
    pub layer_count: u32,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub resource_type: u32,
    pub read_write: u8,
    pub padding: [u8; 3],
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn pinned_vulkan_image_view_layout() {
        assert_eq!(size_of::<ResourceVk>(), 56);
        assert_eq!(align_of::<ResourceVk>(), 8);
        assert_eq!(offset_of!(ResourceVk, format), 36);
        assert_eq!(offset_of!(ResourceVk, resource_type), 48);
        assert_eq!(offset_of!(ResourceVk, read_write), 52);
        assert_eq!(NGX_SUCCESS, 1); // Streamline uses a different success value.
        assert_eq!(NR_FEATURE, 18);
        assert_eq!(std::mem::size_of::<Call>(), 64);
        assert_eq!(std::mem::size_of::<InitArgs>(), 72);
        assert_eq!(bridge_abi(), 0x0001_0040_0048_0038);
    }

    #[test]
    fn rejects_missing_function_without_calling() {
        let call = Call {
            operation: 0,
            function: std::ptr::null(),
            init: std::ptr::null(),
            device: std::ptr::null_mut(),
            command: std::ptr::null_mut(),
            parameters: std::ptr::null_mut(),
            feature: std::ptr::null_mut(),
            output: std::ptr::null_mut(),
        };
        assert_eq!(unsafe { dispatch(&call) }, NGX_INVALID_PARAMETER);
    }
}
