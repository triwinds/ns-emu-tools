//! Opt-in bounded same-frame readback. Captures perturb timing, not a performance mode.
use crate::fg_api::*;
use ash::vk::{self, Handle};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

#[derive(Default)]
struct Requests {
    revision: u64,
    remaining: u32,
    reserved: u32,
    last: Option<(u32, bool)>,
}
impl Requests {
    fn apply(&mut self, revision: u64, count: u32) {
        if revision > self.revision && (1..=8).contains(&count) && self.reserved + count <= 64 {
            self.revision = revision;
            self.remaining += count;
            self.reserved += count;
        }
    }
    fn selected(&mut self, frame: u32) -> bool {
        if let Some((previous, selected)) = self.last {
            if previous == frame {
                return selected;
            }
        }
        let selected = self.remaining > 0;
        if selected {
            self.remaining -= 1;
        }
        self.last = Some((frame, selected));
        selected
    }
}
static REQUESTS: Mutex<Requests> = Mutex::new(Requests {
    revision: 0,
    remaining: 0,
    reserved: 0,
    last: None,
});
fn requests_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NS_STREAMLINE_CAPTURE_REQUESTS").as_deref() == Ok("1"))
}
/// Called only by the session control worker. All stages select the same frame.
pub(super) fn request(value: &serde_json::Value) {
    if !requests_enabled() {
        return;
    }
    if let (Some(revision), Some(count)) = (
        value["captureRevision"].as_u64(),
        value["captureFrames"].as_u64(),
    ) {
        if let Ok(count) = u32::try_from(count) {
            REQUESTS.lock().unwrap().apply(revision, count);
        }
    }
}

fn parse_frames(value: &str) -> Vec<u32> {
    let frames: Option<Vec<u32>> = value.split(',').map(|v| v.trim().parse().ok()).collect();
    let Some(mut frames) = frames else {
        return Vec::new();
    };
    if frames.len() > 64 || frames.contains(&0) {
        return Vec::new();
    }
    frames.sort_unstable();
    frames.dedup();
    frames
}
fn frames() -> &'static [u32] {
    static FRAMES: OnceLock<Vec<u32>> = OnceLock::new();
    FRAMES.get_or_init(|| {
        std::env::var("NS_STREAMLINE_CAPTURE_FRAMES")
            .map(|v| parse_frames(&v))
            .unwrap_or_default()
    })
}
pub(super) fn selected(frame: u32) -> bool {
    let requested = requests_enabled() && REQUESTS.lock().unwrap().selected(frame);
    frames().binary_search(&frame).is_ok() || requested
}
fn pixel_bytes(format: vk::Format) -> Option<u64> {
    match format {
        vk::Format::R8G8B8A8_UNORM | vk::Format::B8G8R8A8_UNORM | vk::Format::R16G16_SFLOAT => {
            Some(4)
        }
        vk::Format::R16G16B16A16_SFLOAT | vk::Format::R32G32_SFLOAT => Some(8),
        vk::Format::R16_SFLOAT => Some(2),
        _ => None,
    }
}

pub(super) struct Capture {
    device: ash::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    bytes: u64,
    resources: Vec<(Resource, u64, u64)>,
    directory: PathBuf,
}
impl Capture {
    pub(super) unsafe fn new(
        device: &ash::Device,
        props: &vk::PhysicalDeviceMemoryProperties,
        resources: &[Resource],
    ) -> Result<Option<Self>> {
        if frames().is_empty() && !requests_enabled() {
            return Ok(None);
        }
        let directory = PathBuf::from(
            std::env::var_os("NS_STREAMLINE_LIVE_DIR")
                .ok_or("frame capture requires session directory")?,
        )
        .join("frame-captures");
        std::fs::create_dir_all(&directory)?;
        let mut capture = Self {
            device: device.clone(),
            buffer: vk::Buffer::null(),
            memory: vk::DeviceMemory::null(),
            bytes: 0,
            resources: Vec::new(),
            directory,
        };
        for &r in resources {
            let pixel = pixel_bytes(vk::Format::from_raw(r.format as i32))
                .ok_or("unsupported capture format")?;
            if r.usage & vk::ImageUsageFlags::TRANSFER_SRC.as_raw() == 0 {
                return Err("capture image lacks transfer source usage".into());
            }
            let offset = (capture.bytes + 7) & !7;
            let bytes = u64::from(r.width) * u64::from(r.height) * pixel;
            capture.resources.push((r, offset, bytes));
            capture.bytes = offset + bytes;
        }
        capture.buffer = device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(capture.bytes)
                .usage(vk::BufferUsageFlags::TRANSFER_DST),
            None,
        )?;
        let req = device.get_buffer_memory_requirements(capture.buffer);
        let flags = vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;
        let index = memory_type(
            props,
            req.memory_type_bits,
            flags | vk::MemoryPropertyFlags::HOST_CACHED,
        )
        .or_else(|_| memory_type(props, req.memory_type_bits, flags))?;
        capture.memory = device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(index),
            None,
        )?;
        device.bind_buffer_memory(capture.buffer, capture.memory, 0)?;
        Ok(Some(capture))
    }
    pub(super) unsafe fn record(&self, cmd: vk::CommandBuffer) {
        for &(r, offset, _) in &self.resources {
            let image = vk::Image::from_raw(r.image);
            transition(
                &self.device,
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            self.device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                self.buffer,
                &[vk::BufferImageCopy::default()
                    .buffer_offset(offset)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: r.width,
                        height: r.height,
                        depth: 1,
                    })],
            );
            transition(
                &self.device,
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        self.device.cmd_pipeline_barrier(
            cmd,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::HOST,
            vk::DependencyFlags::empty(),
            &[vk::MemoryBarrier::default()
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::HOST_READ)],
            &[],
            &[],
        );
    }
    /// The owner must complete its submission fence before reading or reusing.
    pub(super) unsafe fn save(&self, stage: &str, frame: u32) -> Result<()> {
        let pointer =
            self.device
                .map_memory(self.memory, 0, self.bytes, vk::MemoryMapFlags::empty())?;
        let data = std::slice::from_raw_parts(pointer.cast::<u8>(), self.bytes as usize).to_vec();
        self.device.unmap_memory(self.memory);
        let mut files = Vec::new();
        for (index, &(r, offset, bytes)) in self.resources.iter().enumerate() {
            let pixels = &data[offset as usize..(offset + bytes) as usize];
            let name = format!("{frame:06}-{stage}-{index}");
            std::fs::write(self.directory.join(format!("{name}.bin")), pixels)?;
            let format = vk::Format::from_raw(r.format as i32);
            if matches!(
                format,
                vk::Format::R8G8B8A8_UNORM
                    | vk::Format::B8G8R8A8_UNORM
                    | vk::Format::R16G16B16A16_SFLOAT
            ) {
                let mut ppm = format!("P6\n{} {}\n255\n", r.width, r.height).into_bytes();
                if matches!(
                    format,
                    vk::Format::R8G8B8A8_UNORM | vk::Format::B8G8R8A8_UNORM
                ) {
                    for p in pixels.chunks_exact(4) {
                        if format == vk::Format::B8G8R8A8_UNORM {
                            ppm.extend_from_slice(&[p[2], p[1], p[0]]);
                        } else {
                            ppm.extend_from_slice(&p[..3]);
                        }
                    }
                } else {
                    for p in pixels.chunks_exact(8) {
                        for c in p[..6].chunks_exact(2) {
                            ppm.push(
                                (half(u16::from_le_bytes([c[0], c[1]])).clamp(0.0, 1.0) * 255.0)
                                    .round() as u8,
                            );
                        }
                    }
                }
                std::fs::write(self.directory.join(format!("{name}.ppm")), ppm)?;
            }
            files.push(serde_json::json!({"name":name,"format":r.format,"width":r.width,"height":r.height,"bytes":bytes}));
        }
        let metadata = serde_json::json!({"frame":frame,"stage":stage,"resources":files,"fence_completed":true});
        std::fs::write(
            self.directory.join(format!("{frame:06}-{stage}.json")),
            serde_json::to_vec_pretty(&metadata)?,
        )?;
        crate::trace::event!("target_frame_capture", metadata);
        Ok(())
    }
}
pub(crate) fn half(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = u32::from((bits >> 10) & 31);
    let fraction = u32::from(bits & 1023);
    f32::from_bits(
        sign | match exponent {
            0 => ((fraction as f32) * f32::from_bits(0x3380_0000)).to_bits(),
            31 => 0x7f80_0000 | (fraction << 13),
            _ => ((exponent + 112) << 23) | (fraction << 13),
        },
    )
}
impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requested_captures_are_bounded_and_keep_all_stages_on_the_same_frame() {
        let mut requests = Requests::default();
        assert!(!requests.selected(10));
        requests.apply(1, 2);
        assert!(!requests.selected(10)); // Never start part way through a frame.
        assert!(requests.selected(11));
        assert!(requests.selected(11));
        requests.apply(1, 2); // The worker reads the same control file repeatedly.
        assert!(requests.selected(12));
        assert!(!requests.selected(13));
        for count in [0, 9, u32::MAX] {
            requests.apply(2, count);
        }
        assert!(!requests.selected(14));
        for revision in 2..=8 {
            requests.apply(revision, 8);
        }
        requests.apply(9, 6);
        requests.apply(10, 1); // 64 total captures have already been reserved.
        assert_eq!(requests.reserved, 64);
        assert_eq!(requests.remaining, 62);
    }
    #[test]
    fn capture_formats_preserve_color_and_guidance_byte_sizes() {
        assert_eq!(pixel_bytes(vk::Format::B8G8R8A8_UNORM), Some(4));
        assert_eq!(pixel_bytes(vk::Format::R16G16_SFLOAT), Some(4));
        assert_eq!(pixel_bytes(vk::Format::R16_SFLOAT), Some(2));
        assert_eq!(pixel_bytes(vk::Format::R32G32_SFLOAT), Some(8));
        assert_eq!(pixel_bytes(vk::Format::UNDEFINED), None);
    }
    #[test]
    fn schedule_is_explicit_bounded_and_deduplicated() {
        assert_eq!(parse_frames("60, 1,60,120"), vec![1, 60, 120]);
        for input in ["", "0", "1,bad", "-1", "4294967296"] {
            assert!(parse_frames(input).is_empty());
        }
        assert!(parse_frames(&vec!["1"; 65].join(",")).is_empty());
    }
    #[test]
    fn half_conversion_preserves_black_white_and_specials() {
        assert_eq!(half(0), 0.0);
        assert_eq!(half(0x3c00), 1.0);
        assert_eq!(half(0xbc00), -1.0);
        assert!(half(0x7c00).is_infinite());
        assert!(half(0x7e00).is_nan());
    }
}
