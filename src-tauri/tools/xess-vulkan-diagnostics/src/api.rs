//! XeSS 3.0.2 public C ABI (pack(8)); no Streamline/NGX dependency.
use ash::vk;
use libloading::Library;
use std::ffi::{c_char, c_void};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub type Context = *mut c_void;
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Size {
    pub x: u32,
    pub y: u32,
}
#[repr(C)]
#[derive(Default)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub reserved: u16,
}
#[repr(C)]
#[derive(Default)]
pub struct Properties {
    pub descriptors: u32,
    pub buffer_bytes: u64,
    pub texture_bytes: u64,
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct ImageInfo {
    pub view: vk::ImageView,
    pub image: vk::Image,
    pub range: vk::ImageSubresourceRange,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
}
#[repr(C)]
#[derive(Default)]
pub struct Execute {
    pub color: ImageInfo,
    pub motion: ImageInfo,
    pub depth: ImageInfo,
    pub exposure: ImageInfo,
    pub responsive: ImageInfo,
    pub output: ImageInfo,
    pub jitter_x: f32,
    pub jitter_y: f32,
    pub exposure_scale: f32,
    pub reset: u32,
    pub width: u32,
    pub height: u32,
    pub color_base: Size,
    pub motion_base: Size,
    pub depth_base: Size,
    pub responsive_base: Size,
    pub reserved: Size,
    pub output_base: Size,
}
#[repr(C)]
#[derive(Default)]
pub struct Init {
    pub output: Size,
    pub quality: i32,
    pub flags: u32,
    pub creation_mask: u32,
    pub visible_mask: u32,
    pub buffer_heap: vk::DeviceMemory,
    pub buffer_offset: u64,
    pub texture_heap: vk::DeviceMemory,
    pub texture_offset: u64,
    pub pipeline_cache: vk::PipelineCache,
}

// Expected sizes/offsets from the pinned public xess.h / xess_vk.h, Windows x64.
const _: () = {
    assert!(std::mem::size_of::<ImageInfo>() == 48);
    assert!(std::mem::size_of::<Execute>() == 360);
    assert!(std::mem::offset_of!(Execute, jitter_x) == 288);
    assert!(std::mem::offset_of!(Execute, output_base) == 352);
    assert!(std::mem::size_of::<Init>() == 64);
    assert!(std::mem::offset_of!(Init, buffer_heap) == 24);
    assert!(std::mem::size_of::<Properties>() == 24);
};

pub struct Api {
    pub version: unsafe extern "C" fn(*mut Version) -> i32,
    pub instance_extensions:
        unsafe extern "C" fn(*mut u32, *mut *const *const c_char, *mut u32) -> i32,
    pub device_extensions: unsafe extern "C" fn(
        vk::Instance,
        vk::PhysicalDevice,
        *mut u32,
        *mut *const *const c_char,
    ) -> i32,
    pub features: unsafe extern "C" fn(vk::Instance, vk::PhysicalDevice, *mut *mut c_void) -> i32,
    pub create:
        unsafe extern "C" fn(vk::Instance, vk::PhysicalDevice, vk::Device, *mut Context) -> i32,
    pub destroy: unsafe extern "C" fn(Context) -> i32,
    pub init: unsafe extern "C" fn(Context, *const Init) -> i32,
    pub execute: unsafe extern "C" fn(Context, vk::CommandBuffer, *const Execute) -> i32,
    pub optimal:
        unsafe extern "C" fn(Context, *const Size, i32, *mut Size, *mut Size, *mut Size) -> i32,
    pub properties: unsafe extern "C" fn(Context, *const Size, *mut Properties) -> i32,
    pub logging:
        unsafe extern "C" fn(Context, i32, unsafe extern "C" fn(*const c_char, i32)) -> i32,
    _library: Library,
}
impl Api {
    pub unsafe fn load(path: &std::path::Path) -> Result<Self> {
        let lib = Library::new(path)?;
        Ok(Self {
            version: *lib.get(b"xessGetVersion\0")?,
            instance_extensions: *lib.get(b"xessVKGetRequiredInstanceExtensions\0")?,
            device_extensions: *lib.get(b"xessVKGetRequiredDeviceExtensions\0")?,
            features: *lib.get(b"xessVKGetRequiredDeviceFeatures\0")?,
            create: *lib.get(b"xessVKCreateContext\0")?,
            destroy: *lib.get(b"xessDestroyContext\0")?,
            init: *lib.get(b"xessVKInit\0")?,
            execute: *lib.get(b"xessVKExecute\0")?,
            optimal: *lib.get(b"xessGetOptimalInputResolution\0")?,
            properties: *lib.get(b"xessGetProperties\0")?,
            logging: *lib.get(b"xessSetLoggingCallback\0")?,
            _library: lib,
        })
    }
}
pub fn check(code: i32, operation: &str) -> Result<()> {
    // Warnings are evidence, not successful validation.
    if code != 0 {
        return Err(format!("{operation}: XeSS result {code}").into());
    }
    Ok(())
}
pub struct Session<'a> {
    pub handle: Context,
    pub api: &'a Api,
}
impl Session<'_> {
    pub fn close(mut self) -> Result<()> {
        let result = unsafe { (self.api.destroy)(self.handle) };
        self.handle = std::ptr::null_mut();
        check(result, "destroy context")
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        let result = unsafe { (self.api.destroy)(self.handle) };
        if result != 0 {
            eprintln!("xessDestroyContext: {result}");
        }
    }
}
