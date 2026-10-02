//! Owned NGX maps and dynamically resolved, pinned experimental NR entrypoints.
use crate::nr_abi::*;
use libloading::Library;
use std::{
    ffi::{c_char, c_void, CStr},
    path::Path,
    ptr,
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[repr(C)]
pub struct LoggingInfo {
    pub callback: unsafe extern "C" fn(*const c_char, u32, u32),
    pub minimum_level: u32,
    pub disable_other_sinks: u8,
    pub padding: [u8; 3],
}
#[repr(C)]
pub struct CommonInfo {
    pub paths: *const *const u16,
    pub path_count: u32,
    pub internal: *mut c_void,
    pub logging: LoggingInfo,
}

// The frozen static core reads driver registry paths and process ownership.
#[link(name = "advapi32")]
#[link(name = "user32")]
unsafe extern "C" {}

#[link(name = "nvsdk_ngx_d", kind = "static")]
unsafe extern "C" {
    pub fn NVSDK_NGX_VULKAN_Init_with_ProjectID(
        project: *const c_char,
        engine: u32,
        version: *const c_char,
        data: *const u16,
        instance: VkHandle,
        physical: VkHandle,
        device: VkHandle,
        gipa: Address,
        gdpa: Address,
        common: *const CommonInfo,
        api: u32,
    ) -> u32;
    pub fn NVSDK_NGX_VULKAN_GetCapabilityParameters(out: *mut *mut c_void) -> u32;
    pub fn NVSDK_NGX_VULKAN_AllocateParameters(out: *mut *mut c_void) -> u32;
    pub fn NVSDK_NGX_VULKAN_DestroyParameters(parameters: *mut c_void) -> u32;
    // Standalone diagnostic only; shared production sessions defer core shutdown.
    #[allow(dead_code)]
    pub fn NVSDK_NGX_VULKAN_Shutdown1(device: VkHandle) -> u32;
    fn NVSDK_NGX_Parameter_SetUI(parameters: *mut c_void, name: *const c_char, value: u32);
    fn NVSDK_NGX_Parameter_SetI(parameters: *mut c_void, name: *const c_char, value: i32);
    fn NVSDK_NGX_Parameter_SetF(parameters: *mut c_void, name: *const c_char, value: f32);
    fn NVSDK_NGX_Parameter_SetVoidPointer(
        parameters: *mut c_void,
        name: *const c_char,
        value: *mut c_void,
    );
}

pub const EXPORTS: [&[u8]; 6] = [
    b"NVSDK_NGX_VULKAN_Init_Ext2\0",
    b"NVSDK_NGX_VULKAN_PopulateParameters_Impl\0",
    b"NVSDK_NGX_VULKAN_CreateFeature1\0",
    b"NVSDK_NGX_VULKAN_EvaluateFeature\0",
    b"NVSDK_NGX_VULKAN_ReleaseFeature\0",
    b"NVSDK_NGX_VULKAN_Shutdown1\0",
];

pub struct Api {
    _snippet: Library,
    bridge: Option<(Library, Invoke)>,
    functions: [Address; 6],
}
impl Api {
    pub unsafe fn load(path: &Path) -> Result<Self> {
        let library = libloading::os::windows::Library::load_with_flags(path, 0x100 | 0x800)?;
        let snippet: Library = library.into();
        let mut functions = [ptr::null(); 6];
        for (index, name) in EXPORTS.iter().enumerate() {
            functions[index] = *snippet.get::<unsafe extern "C" fn()>(name)? as Address;
        }
        Ok(Self {
            _snippet: snippet,
            bridge: None,
            functions,
        })
    }
    pub unsafe fn load_bridge(&mut self, path: &Path) -> Result<()> {
        let bridge: Library =
            libloading::os::windows::Library::load_with_flags(path, 0x100 | 0x800)?.into();
        let version = *bridge.get::<unsafe extern "C" fn() -> u64>(b"NRBridgeAbiVersion\0")?;
        if version() != bridge_abi() {
            return Err("NR bridge ABI version or layout mismatch".into());
        }
        let invoke = *bridge.get::<Invoke>(b"NRBridgeInvoke\0")?;
        self.bridge = Some((bridge, invoke));
        Ok(())
    }
    pub fn address(&self, operation: usize) -> Address {
        self.functions[operation]
    }
    pub unsafe fn call(&self, mut call: Call) -> u32 {
        let Some(&function) = self.functions.get(call.operation as usize) else {
            return NGX_INVALID_PARAMETER;
        };
        call.function = function;
        match &self.bridge {
            Some((_, invoke)) => invoke(&call),
            None => dispatch(&call),
        }
    }
}

pub fn empty_call(operation: u32) -> Call {
    Call {
        operation,
        function: ptr::null(),
        init: ptr::null(),
        device: ptr::null_mut(),
        command: ptr::null_mut(),
        parameters: ptr::null_mut(),
        feature: ptr::null_mut(),
        output: ptr::null_mut(),
    }
}
pub fn checked(result: u32, stage: &str) -> Result<()> {
    if result != NGX_SUCCESS {
        return Err(format!("{stage}: NGX result 0x{result:08x}").into());
    }
    Ok(())
}

pub unsafe fn set_dimensions(params: *mut c_void, width: u32, height: u32) {
    for (name, value) in [
        (c"CreationNodeMask", 1),
        (c"VisibilityNodeMask", 1),
        (c"Width", width),
        (c"Height", height),
        (c"OutWidth", width),
        (c"OutHeight", height),
        (c"DLSSNR.Width", width),
        (c"DLSSNR.Height", height),
        (c"DLSSNR.OutputWidth", width),
        (c"DLSSNR.OutputHeight", height),
        (c"DLSSNR.Enabled", 1),
    ] {
        NVSDK_NGX_Parameter_SetUI(params, name.as_ptr(), value);
    }
    NVSDK_NGX_Parameter_SetI(params, c"DLSSNR.Hint.Render.Preset".as_ptr(), 0);
}

pub unsafe fn set_frame(
    params: *mut c_void,
    resources: &mut [ResourceVk; 4],
    intensity: f32,
    reset: bool,
) {
    set_frame_scaled(params, resources, intensity, reset, [1.0, 1.0]);
}

/// Motion is current-to-previous UV displacement in the analyzed present image.
/// Cropping its viewport changes the UV basis, independent of the NR extent.
pub unsafe fn set_frame_scaled(
    params: *mut c_void,
    resources: &mut [ResourceVk; 4],
    intensity: f32,
    reset: bool,
    uv_scale: [f32; 2],
) {
    let width = resources[0].width;
    let height = resources[0].height;
    for (index, name) in [
        c"DLSSNR.Color",
        c"DLSSNR.Output",
        c"DLSSNR.Depth",
        c"DLSSNR.MVec",
    ]
    .iter()
    .enumerate()
    {
        NVSDK_NGX_Parameter_SetVoidPointer(
            params,
            name.as_ptr(),
            (&mut resources[index] as *mut ResourceVk).cast(),
        );
    }
    for (name, value) in [
        (c"DLSSNR.ColorSubrectWidth", width),
        (c"DLSSNR.ColorSubrectHeight", height),
        (c"DLSSNR.DepthSubrectWidth", width),
        (c"DLSSNR.DepthSubrectHeight", height),
        (c"DLSSNR.MVecSubrectWidth", width),
        (c"DLSSNR.MVecSubrectHeight", height),
        (c"DLSSNR.OutputSubrectWidth", width),
        (c"DLSSNR.OutputSubrectHeight", height),
        (c"DLSSNR.Reset", u32::from(reset)),
        (c"DLSSNR.DepthInverted", 1),
        (c"DLSSNR.Enabled", 1),
    ] {
        NVSDK_NGX_Parameter_SetUI(params, name.as_ptr(), value);
    }
    for (name, value) in [
        (c"DLSSNR.MVecScaleX", width as f32 * uv_scale[0]),
        (c"DLSSNR.MVecScaleY", height as f32 * uv_scale[1]),
        (c"DLSSNR.Intensity", intensity),
        (c"DLSSNR.GlobalToneStrength", intensity),
        (c"DLSSNR.LocalToneStrength", intensity),
        (c"DLSSNR.LocalStructureStrength", intensity),
    ] {
        NVSDK_NGX_Parameter_SetF(params, name.as_ptr(), value);
    }
}

pub unsafe fn export_names() -> Vec<String> {
    EXPORTS
        .iter()
        .map(|name| {
            CStr::from_bytes_with_nul_unchecked(name)
                .to_string_lossy()
                .into_owned()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_info_matches_windows_x64_header() {
        assert_eq!(std::mem::size_of::<LoggingInfo>(), 16);
        assert_eq!(std::mem::size_of::<CommonInfo>(), 40);
        assert_eq!(std::mem::offset_of!(CommonInfo, internal), 16);
        assert_eq!(std::mem::offset_of!(CommonInfo, logging), 24);
    }
    #[test]
    fn ngx_result_is_not_streamline_result() {
        assert!(checked(1, "init").is_ok());
        assert!(checked(0, "init").is_err());
        assert!(checked(0xbad00002, "init")
            .unwrap_err()
            .to_string()
            .contains("0xbad00002"));
    }
}
