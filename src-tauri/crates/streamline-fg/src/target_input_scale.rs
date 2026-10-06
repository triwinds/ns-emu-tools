//! Shared SDR input downsampling before NR/SR and full-size presentation for FG.
use crate::{advanced_settings::InputSizing, fg_api::*, source_auto::Source, target_runtime};
use ash::vk::{self, Handle};
use serde_json::json;
use std::hash::{Hash, Hasher};

pub(super) fn extent(window: vk::Extent2D, native: Option<Source>) -> vk::Extent2D {
    native.map_or(window, |n| vk::Extent2D {
        width: n.offsets[0].x.abs_diff(n.offsets[1].x),
        height: n.offsets[0].y.abs_diff(n.offsets[1].y),
    })
}
fn resized(size: vk::Extent2D, sizing: InputSizing) -> Result<vk::Extent2D> {
    if !sizing.valid() {
        return Err("invalid shared input sizing".into());
    }
    let (percent, cap) = (sizing.scale_percent, sizing.max_edge);
    let longest = size.width.max(size.height);
    if size.width == 0 || size.height == 0 || longest > 8192 {
        return Err("input scaling extent exceeds limits".into());
    }
    let (n, d) = if cap == 0 {
        (percent, 100)
    } else {
        (cap.min(longest), longest)
    };
    let low = vk::Extent2D {
        width: (u64::from(size.width) * u64::from(n) / u64::from(d)) as u32,
        height: (u64::from(size.height) * u64::from(n) / u64::from(d)) as u32,
    };
    if low != size && (low.width < 320 || low.height < 180) {
        return Err("scaled input must be at least 320x180".into());
    }
    Ok(low)
}
fn offsets(size: vk::Extent2D) -> [vk::Offset3D; 2] {
    [
        vk::Offset3D::default(),
        vk::Offset3D {
            x: size.width as i32,
            y: size.height as i32,
            z: 1,
        },
    ]
}
fn viewport(window: vk::Extent2D, native: Option<Source>) -> Result<[f32; 4]> {
    let v = native.map_or([0.0, 0.0, window.width as f32, window.height as f32], |n| {
        n.viewport
    });
    if !v.iter().all(|x| x.is_finite())
        || v[0] < 0.0
        || v[1] < 0.0
        || v[2] < 1.0
        || v[3] < 1.0
        || v[0] + v[2] > window.width as f32 + 0.5
        || v[1] + v[3] > window.height as f32 + 0.5
    {
        return Err("input scaling viewport is outside the presented image".into());
    }
    Ok(v)
}
unsafe fn blit(
    d: &ash::Device,
    cmd: vk::CommandBuffer,
    src: vk::Image,
    from: [vk::Offset3D; 2],
    dst: vk::Image,
    to: [vk::Offset3D; 2],
) {
    let layers = vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1);
    d.cmd_blit_image(
        cmd,
        src,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        dst,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &[vk::ImageBlit::default()
            .src_subresource(layers)
            .src_offsets(from)
            .dst_subresource(layers)
            .dst_offsets(to)],
        vk::Filter::LINEAR,
    );
}
pub(super) struct InputScale {
    sizing: InputSizing,
    device: ash::Device,
    swapchain: vk::SwapchainKHR,
    window: vk::Extent2D,
    original: vk::Extent2D,
    low: vk::Extent2D,
    raw_extent: Option<vk::Extent2D>,
    textures: Vec<Resource>,
    images: Vec<vk::Image>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    initialized: bool,
    in_flight: bool,
    previous_identity: Option<u64>,
}
impl InputScale {
    pub(super) unsafe fn new(
        inst: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        swapchain: vk::SwapchainKHR,
        window: vk::Extent2D,
        native: Option<Source>,
        sdr: bool,
        sizing: InputSizing,
    ) -> Result<Option<Self>> {
        let original = extent(window, native);
        let low = resized(original, sizing)?;
        if low == original {
            return Ok(None);
        }
        if !sdr {
            return Err("shared input scaling requires SDR sRGB".into());
        }
        viewport(window, native)?;
        let required = vk::FormatFeatureFlags::BLIT_SRC
            | vk::FormatFeatureFlags::BLIT_DST
            | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
        for format in [
            vk::Format::R16G16B16A16_SFLOAT,
            vk::Format::R8G8B8A8_UNORM,
            vk::Format::B8G8R8A8_UNORM,
        ] {
            if !inst
                .get_physical_device_format_properties(physical, format)
                .optimal_tiling_features
                .contains(required)
            {
                return Err(format!("unsupported input scaling format {format:?}").into());
            }
        }
        let mut this = Self {
            sizing,
            device: device.clone(),
            swapchain,
            window,
            original,
            low,
            raw_extent: native.filter(|n| n.raw_copy).map(|n| n.extent),
            textures: Vec::new(),
            images: Vec::new(),
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            initialized: false,
            in_flight: false,
            previous_identity: None,
        };
        let memory = inst.get_physical_device_memory_properties(physical);
        let usage = vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        this.textures.push(texture_with_usage(
            device,
            &memory,
            low,
            vk::Format::R16G16B16A16_SFLOAT,
            usage,
        )?);
        if let Some(raw) = this.raw_extent {
            this.textures.push(texture_with_usage(
                device,
                &memory,
                raw,
                vk::Format::R8G8B8A8_UNORM,
                usage,
            )?);
        }
        let get: vk::PFN_vkGetSwapchainImagesKHR = std::mem::transmute(
            target_runtime::device_proc(device.handle(), c"vkGetSwapchainImagesKHR")
                .ok_or("input scaling swapchain query missing")?,
        );
        let mut count = 0;
        get(device.handle(), swapchain, &mut count, std::ptr::null_mut()).result()?;
        this.images.resize(count as usize, vk::Image::null());
        get(
            device.handle(),
            swapchain,
            &mut count,
            this.images.as_mut_ptr(),
        )
        .result()?;
        this.images.truncate(count as usize);
        this.pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default()
                .queue_family_index(0)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
            None,
        )?;
        this.command = device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(this.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        )?[0];
        this.fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        Ok(Some(this))
    }
    pub(super) fn matches(&self, native: Option<Source>) -> bool {
        self.original == extent(self.window, native)
            && self.raw_extent == native.filter(|n| n.raw_copy).map(|n| n.extent)
    }
    pub(super) fn source(&self, native: Option<Source>) -> Result<Source> {
        let v = viewport(self.window, native)?;
        let mut identity = std::collections::hash_map::DefaultHasher::new();
        self.swapchain.as_raw().hash(&mut identity);
        if let Some(n) = native {
            n.history_identity.hash(&mut identity);
            n.raw_copy.hash(&mut identity);
            [n.extent.width, n.extent.height].hash(&mut identity);
            for o in n.offsets {
                [o.x, o.y, o.z].hash(&mut identity);
            }
        }
        for x in v {
            x.to_bits().hash(&mut identity);
        }
        [self.low.width, self.low.height].hash(&mut identity);
        let r = self.textures[0];
        Ok(Source {
            image: vk::Image::from_raw(r.image),
            generation: r.image,
            history_identity: identity.finish(),
            history_members: 1,
            history_paths: 1,
            history_update: "shared_scaled_input",
            history_route: [0; 4],
            usage: (vk::ImageUsageFlags::SAMPLED
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::TRANSFER_DST)
                .as_raw(),
            extent: self.low,
            raw_copy: false,
            offsets: offsets(self.low),
            viewport: v,
        })
    }
    unsafe fn begin(&mut self) -> Result<()> {
        self.finish()?;
        self.device
            .reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())?;
        self.device.reset_fences(&[self.fence])?;
        self.device
            .begin_command_buffer(self.command, &vk::CommandBufferBeginInfo::default())?;
        Ok(())
    }
    unsafe fn submit(&mut self, queue: vk::Queue, waits: &[vk::Semaphore]) -> Result<()> {
        self.device.end_command_buffer(self.command)?;
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        self.device.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[self.command])
                .wait_semaphores(waits)
                .wait_dst_stage_mask(&stages)],
            self.fence,
        )?;
        self.in_flight = true;
        self.finish()
    }
    pub(super) unsafe fn finish(&mut self) -> Result<()> {
        if self.in_flight {
            self.device
                .wait_for_fences(&[self.fence], true, 10_000_000_000)?;
            self.in_flight = false;
        }
        Ok(())
    }
    pub(super) unsafe fn prepare(
        &mut self,
        queue: vk::Queue,
        info: &vk::PresentInfoKHR,
        native: Option<Source>,
    ) -> Result<bool> {
        let processed = self.source(native)?;
        if info.p_image_indices.is_null()
            || (info.wait_semaphore_count != 0 && info.p_wait_semaphores.is_null())
        {
            return Err("invalid input scaling present".into());
        }
        let present = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("input scaling image index")?;
        self.begin()?;
        let d = &self.device;
        let cmd = self.command;
        let low = vk::Image::from_raw(self.textures[0].image);
        transition(
            d,
            cmd,
            low,
            if self.initialized {
                vk::ImageLayout::GENERAL
            } else {
                vk::ImageLayout::UNDEFINED
            },
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        let src = native.map_or(present, |n| n.image);
        let old = if native.is_some() {
            vk::ImageLayout::GENERAL
        } else {
            vk::ImageLayout::PRESENT_SRC_KHR
        };
        transition(d, cmd, src, old, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        let from = native.map_or(offsets(self.window), |n| n.offsets);
        if let Some(raw_extent) = self.raw_extent {
            let raw = vk::Image::from_raw(self.textures[1].image);
            transition(
                d,
                cmd,
                raw,
                if self.initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
            let layers = vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1);
            d.cmd_copy_image(
                cmd,
                src,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                raw,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &[vk::ImageCopy::default()
                    .src_subresource(layers)
                    .dst_subresource(layers)
                    .extent(vk::Extent3D {
                        width: raw_extent.width,
                        height: raw_extent.height,
                        depth: 1,
                    })],
            );
            transition(
                d,
                cmd,
                raw,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            blit(d, cmd, raw, from, low, offsets(self.low));
            transition(
                d,
                cmd,
                raw,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        } else {
            blit(d, cmd, src, from, low, offsets(self.low));
        }
        transition(d, cmd, src, vk::ImageLayout::TRANSFER_SRC_OPTIMAL, old);
        transition(
            d,
            cmd,
            low,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        let waits = if info.wait_semaphore_count == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
        };
        self.submit(queue, waits)?;
        self.initialized = true;
        let boundary = self.previous_identity != Some(processed.history_identity);
        self.previous_identity = Some(processed.history_identity);
        Ok(boundary)
    }
    pub(super) unsafe fn present(
        &mut self,
        queue: vk::Queue,
        info: &vk::PresentInfoKHR,
        native: Option<Source>,
    ) -> Result<()> {
        let v = viewport(self.window, native)?;
        let present = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("input scaling output index")?;
        self.begin()?;
        let d = &self.device;
        let cmd = self.command;
        let low = vk::Image::from_raw(self.textures[0].image);
        transition(
            d,
            cmd,
            low,
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        transition(
            d,
            cmd,
            present,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        if native.is_some() {
            d.cmd_clear_color_image(
                cmd,
                present,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
                &[range()],
            );
            transition(
                d,
                cmd,
                present,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
        }
        let to = [
            vk::Offset3D {
                x: v[0].round() as i32,
                y: v[1].round() as i32,
                z: 0,
            },
            vk::Offset3D {
                x: (v[0] + v[2]).round() as i32,
                y: (v[1] + v[3]).round() as i32,
                z: 1,
            },
        ];
        blit(d, cmd, low, offsets(self.low), present, to);
        transition(
            d,
            cmd,
            low,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        transition(
            d,
            cmd,
            present,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        self.submit(queue, &[])
    }
    pub(super) fn status(&self, nr: bool, sr: bool, revision: u64) -> serde_json::Value {
        let (percent, cap) = (self.sizing.scale_percent, self.sizing.max_edge);
        json!({"active":true,"mode":if cap == 0 {"percentage"} else {"max_edge"},"scalePercent":percent,"maxEdge":cap,
            "originalExtent":[self.original.width,self.original.height],"inputExtent":[self.low.width,self.low.height],"outputExtent":[self.window.width,self.window.height],
            "nrInputScaled":nr,"srInputScaled":sr,"presentationResampled":!nr && !sr,"fgExtent":"presentation_output","textureCount":self.textures.len(),"startupOnly":false,"appliedRevision":revision})
    }
}
impl Drop for InputScale {
    fn drop(&mut self) {
        unsafe {
            if self.finish().is_err() {
                std::process::abort();
            }
            let d = &self.device;
            d.destroy_fence(self.fence, None);
            d.destroy_command_pool(self.pool, None);
            for r in self.textures.drain(..) {
                d.destroy_image_view(vk::ImageView::from_raw(r.view), None);
                d.destroy_image(vk::Image::from_raw(r.image), None);
                d.free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
        }
    }
}
