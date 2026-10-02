//! Per-present suspension without freeing the Streamline FG resources.
//! The version-5 ABI below is checked against the pinned C++ SDK at build time.
use crate::fg_api::{checked, Api, Result};
use ash::vk;
use std::ffi::c_void;

#[repr(C)]
struct Base {
    next: *const c_void,
    guid: [u8; 16],
    version: usize,
}
impl Base {
    fn new(guid: [u8; 16], version: usize) -> Self {
        Self {
            next: std::ptr::null(),
            guid,
            version,
        }
    }
}
#[repr(C)]
struct Viewport {
    base: Base,
    value: u32,
}
#[repr(C)]
struct ReflexOptions {
    base: Base,
    mode: u32,
    frame_limit_us: u32,
    optimize_markers: u8,
    virtual_key: u16,
    thread: u32,
}
#[repr(C)]
struct Options {
    base: Base,
    mode: u32,
    generated: u32,
    flags: u32,
    dynamic_width: u32,
    dynamic_height: u32,
    backbuffers: u32,
    motion_width: u32,
    motion_height: u32,
    color_width: u32,
    color_height: u32,
    color_format: u32,
    motion_format: u32,
    depth_format: u32,
    hudless_format: u32,
    ui_format: u32,
    callback: *const c_void,
    reserved: u8,
    queue_parallelism: u32,
    recompose_ui: u8,
    dynamic_fps: f32,
}
impl Options {
    fn paused(extent: vk::Extent2D, count: u32, motion: vk::Format) -> Self {
        Self {
            base: Base::new(
                [
                    0xcb, 0xf1, 0xc5, 0xfa, 0xfd, 0x2d, 0x36, 0x4f, 0xa1, 0xe6, 0x3a, 0x9e, 0x86,
                    0x52, 0x56, 0xc5,
                ],
                5,
            ),
            mode: 0,
            generated: 1,
            flags: 1 << 3,
            dynamic_width: 0,
            dynamic_height: 0,
            backbuffers: count,
            motion_width: extent.width,
            motion_height: extent.height,
            color_width: extent.width,
            color_height: extent.height,
            color_format: vk::Format::B8G8R8A8_UNORM.as_raw() as u32,
            motion_format: motion.as_raw() as u32,
            depth_format: vk::Format::R32_SFLOAT.as_raw() as u32,
            hudless_format: 0,
            ui_format: 0,
            callback: std::ptr::null(),
            reserved: 2,
            queue_parallelism: 0,
            recompose_ui: 0,
            dynamic_fps: 0.0,
        }
    }
}
pub(super) unsafe fn suspend(
    api: &Api,
    extent: vk::Extent2D,
    count: u32,
    motion: vk::Format,
    frame_limit_us: u32,
) -> Result<()> {
    configure(api, false, true, extent, count, motion, frame_limit_us)
}
pub(super) unsafe fn configure(
    api: &Api,
    enabled: bool,
    retain: bool,
    extent: vk::Extent2D,
    count: u32,
    motion: vk::Format,
    frame_limit_us: u32,
) -> Result<()> {
    type Get = unsafe extern "system" fn(u32, *const i8, *mut *mut c_void) -> i32;
    type Set = unsafe extern "system" fn(&Viewport, &Options) -> i32;
    if api.feature.is_null() {
        return Err("missing FG feature function resolver".into());
    }
    let get: Get = std::mem::transmute(api.feature);
    let mut function = std::ptr::null_mut();
    checked(
        get(3, c"slReflexSetOptions".as_ptr(), &mut function),
        "FG suspension Reflex resolver",
    )?;
    if function.is_null() {
        return Err("missing Reflex options function".into());
    }
    let reflex: unsafe extern "system" fn(&ReflexOptions) -> i32 = std::mem::transmute(function);
    checked(
        reflex(&ReflexOptions {
            base: Base::new(
                [
                    0x1a, 0xf8, 0x3a, 0xf0, 0x0b, 0x6d, 0x02, 0x49, 0xa6, 0x51, 0xc4, 0x96, 0x5e,
                    0x21, 0x54, 0x34,
                ],
                1,
            ),
            mode: 1,
            frame_limit_us,
            optimize_markers: 0,
            virtual_key: 0,
            thread: 0,
        }),
        "FG suspension Reflex options",
    )?;
    function = std::ptr::null_mut();
    checked(
        get(1000, c"slDLSSGSetOptions".as_ptr(), &mut function),
        "FG suspension resolver",
    )?;
    if function.is_null() {
        return Err("missing FG suspension function".into());
    }
    let set: Set = std::mem::transmute(function);
    let viewport = Viewport {
        base: Base::new(
            [
                0x35, 0x64, 0x1b, 0x17, 0x3c, 0x9b, 0xc8, 0x4f, 0x99, 0x94, 0xfb, 0xe5, 0x25, 0x69,
                0xaa, 0xa4,
            ],
            1,
        ),
        value: 0,
    };
    let mut options = Options::paused(extent, count, motion);
    options.mode = u32::from(enabled);
    options.flags = if retain { 8 } else { 0 };
    checked(set(&viewport, &options), "FG options / retained suspension")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pinned_x64_sdk_layout_and_suspension_flags() {
        use std::mem::{offset_of, size_of};
        assert_eq!(size_of::<Base>(), 32);
        assert_eq!(size_of::<Viewport>(), 40);
        assert_eq!(size_of::<ReflexOptions>(), 48);
        assert_eq!(offset_of!(ReflexOptions, virtual_key), 42);
        assert_eq!(offset_of!(ReflexOptions, thread), 44);
        assert_eq!(size_of::<Options>(), 120);
        assert_eq!(offset_of!(Options, mode), 32);
        assert_eq!(offset_of!(Options, flags), 40);
        assert_eq!(offset_of!(Options, callback), 96);
        assert_eq!(offset_of!(Options, queue_parallelism), 108);
        assert_eq!(offset_of!(Options, dynamic_fps), 116);
        let options = Options::paused(
            vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            3,
            vk::Format::R32G32_SFLOAT,
        );
        assert_eq!(
            (options.mode, options.flags, options.base.version),
            (0, 8, 5)
        );
        assert_eq!(
            (
                options.motion_width,
                options.color_height,
                options.backbuffers
            ),
            (1920, 1080, 3)
        );
    }
}
