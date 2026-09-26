//! Present-source SR. Dedicated, fence-completed commands leave application state intact.
use crate::{fg_api::*, target_runtime, trace};
use ash::vk::{self, Handle};
use serde_json::json;
use std::ffi::c_void;

pub(super) fn available() -> bool {
    std::env::var("NS_STREAMLINE_TARGET_SR_READY").as_deref() == Ok("1")
}
pub(super) fn initial_mode() -> u32 {
    match std::env::var("NS_STREAMLINE_TARGET_SR_MODE").as_deref() {
        Ok("quality") => 3,
        Ok("balanced") => 2,
        Ok("performance") => 1,
        Ok("dlaa") => 6,
        _ => 0,
    }
}
unsafe extern "C" {
    fn target_sr_options(api: &Api, width: u32, height: u32, mode: u32, input: *mut u32) -> i32;
    fn target_sr_evaluate(
        api: &Api,
        evaluate: *mut c_void,
        command: u64,
        token: u64,
        reset: u32,
        resources: *const Resource,
    ) -> i32;
    fn target_sr_free(function: *mut c_void) -> i32;
}
pub(super) struct Sr {
    device: ash::Device,
    resources: Vec<Resource>,
    images: Vec<vk::Image>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    initialized: bool,
    extent: vk::Extent2D,
}
impl Sr {
    pub(super) unsafe fn new(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        swapchain: vk::SwapchainKHR,
        extent: vk::Extent2D,
        mode: u32,
    ) -> Result<Self> {
        let mut this = Self {
            device: device.clone(),
            resources: Vec::new(),
            images: Vec::new(),
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            initialized: false,
            extent,
        };
        let mut size = [0; 2];
        checked(
            target_sr_options(
                &target_runtime::fg_api()?,
                extent.width,
                extent.height,
                mode,
                size.as_mut_ptr(),
            ),
            "SR options",
        )?;
        if size.contains(&0) || size[0] > extent.width || size[1] > extent.height {
            return Err("invalid SR input size".into());
        }
        let input = vk::Extent2D {
            width: size[0],
            height: size[1],
        };
        for (format, required) in [
            (
                vk::Format::B8G8R8A8_UNORM,
                vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
            ),
            (
                vk::Format::R8G8B8A8_UNORM,
                vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                    | vk::FormatFeatureFlags::STORAGE_IMAGE
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE,
            ),
            (
                vk::Format::R32_SFLOAT,
                vk::FormatFeatureFlags::SAMPLED_IMAGE | vk::FormatFeatureFlags::TRANSFER_DST,
            ),
            (
                vk::Format::R32G32_SFLOAT,
                vk::FormatFeatureFlags::BLIT_SRC
                    | vk::FormatFeatureFlags::BLIT_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE,
            ),
        ] {
            if !instance
                .get_physical_device_format_properties(physical, format)
                .optimal_tiling_features
                .contains(required)
            {
                return Err(format!("SR unsupported format {format:?}").into());
            }
        }
        let props = instance.get_physical_device_memory_properties(physical);
        let usage = vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        for (size, format, usage) in [
            (input, vk::Format::R8G8B8A8_UNORM, usage),
            (
                extent,
                vk::Format::R8G8B8A8_UNORM,
                usage | vk::ImageUsageFlags::STORAGE,
            ),
            (input, vk::Format::R32_SFLOAT, usage),
            (input, vk::Format::R32G32_SFLOAT, usage),
        ] {
            this.resources
                .push(texture_with_usage(device, &props, size, format, usage)?);
        }
        let get: vk::PFN_vkGetSwapchainImagesKHR = std::mem::transmute(
            target_runtime::device_proc(device.handle(), c"vkGetSwapchainImagesKHR")
                .ok_or("missing SR swapchain images")?,
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
        // Target device planner requires application graphics family 0.
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
        trace::event!(
            "target_sr_ready",
            json!({"input":size,"output":[extent.width,extent.height],"mode":mode,"source":"present_source_override","native_render_resolution_changed":false})
        );
        Ok(this)
    }
    pub(super) unsafe fn run(
        &mut self,
        queue: vk::Queue,
        info: &vk::PresentInfoKHR,
        token: u64,
        motion: Option<Resource>,
        reset: bool,
    ) -> Result<()> {
        if info.p_image_indices.is_null()
            || (info.wait_semaphore_count > 0 && info.p_wait_semaphores.is_null())
        {
            return Err("invalid SR present".into());
        }
        let source = *self
            .images
            .get(*info.p_image_indices as usize)
            .ok_or("SR image index")?;
        let d = &self.device;
        let cmd = self.command;
        d.reset_command_pool(self.pool, vk::CommandPoolResetFlags::empty())?;
        d.reset_fences(&[self.fence])?;
        d.begin_command_buffer(cmd, &vk::CommandBufferBeginInfo::default())?;
        for r in &self.resources {
            transition(
                d,
                cmd,
                vk::Image::from_raw(r.image),
                if self.initialized {
                    vk::ImageLayout::GENERAL
                } else {
                    vk::ImageLayout::UNDEFINED
                },
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            );
        }
        transition(
            d,
            cmd,
            source,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        let input = self.resources[0];
        let output = self.resources[1];
        let mv = self.resources[3];
        blit(
            d,
            cmd,
            source,
            self.extent,
            vk::Image::from_raw(input.image),
            vk::Extent2D {
                width: input.width,
                height: input.height,
            },
        );
        d.cmd_clear_color_image(
            cmd,
            vk::Image::from_raw(self.resources[2].image),
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &vk::ClearColorValue { float32: [0.5; 4] },
            &[range()],
        );
        if let Some(motion) = motion {
            let image = vk::Image::from_raw(motion.image);
            transition(
                d,
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            // NVOF stores normalized UV displacement: resizing does not change its scale.
            blit(
                d,
                cmd,
                image,
                vk::Extent2D {
                    width: motion.width,
                    height: motion.height,
                },
                vk::Image::from_raw(mv.image),
                vk::Extent2D {
                    width: mv.width,
                    height: mv.height,
                },
            );
            transition(
                d,
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        } else {
            d.cmd_clear_color_image(
                cmd,
                vk::Image::from_raw(mv.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue { float32: [0.0; 4] },
                &[range()],
            );
        }
        for r in &self.resources {
            transition(
                d,
                cmd,
                vk::Image::from_raw(r.image),
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        checked(
            target_sr_evaluate(
                &target_runtime::fg_api()?,
                target_runtime::sr_function(b"slEvaluateFeature\0")?,
                cmd.as_raw(),
                token,
                u32::from(reset || !self.initialized || motion.is_none()),
                self.resources.as_ptr(),
            ),
            "SR evaluate",
        )?;
        transition(
            d,
            cmd,
            vk::Image::from_raw(output.image),
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        );
        transition(
            d,
            cmd,
            source,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        );
        blit(
            d,
            cmd,
            vk::Image::from_raw(output.image),
            self.extent,
            source,
            self.extent,
        );
        transition(
            d,
            cmd,
            vk::Image::from_raw(output.image),
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
        );
        transition(
            d,
            cmd,
            source,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
        );
        d.end_command_buffer(cmd)?;
        let waits = if info.wait_semaphore_count == 0 {
            &[][..]
        } else {
            std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
        };
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
        d.queue_submit(
            queue,
            &[vk::SubmitInfo::default()
                .command_buffers(&[cmd])
                .wait_semaphores(waits)
                .wait_dst_stage_mask(&stages)],
            self.fence,
        )?;
        // This first version deliberately bounds reuse with a CPU fence; no binary semaphore is reused while present owns it.
        d.wait_for_fences(&[self.fence], true, 5_000_000_000)?;
        crate::live::sr(
            json!({"active":true,"input":[input.width,input.height],"output":[output.width,output.height],"motion":motion.is_some()}),
        );
        self.initialized = true;
        trace::event!(
            "target_sr_frame",
            json!({"evaluated":true,"motion":if motion.is_some(){"nvof"}else{"zero"},"history_reset":reset || motion.is_none(),"input":[input.width,input.height],"output":[output.width,output.height]})
        );
        Ok(())
    }
}
unsafe fn blit(
    d: &ash::Device,
    cmd: vk::CommandBuffer,
    src: vk::Image,
    a: vk::Extent2D,
    dst: vk::Image,
    b: vk::Extent2D,
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
            .dst_subresource(layers)
            .src_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: a.width as i32,
                    y: a.height as i32,
                    z: 1,
                },
            ])
            .dst_offsets([
                vk::Offset3D::default(),
                vk::Offset3D {
                    x: b.width as i32,
                    y: b.height as i32,
                    z: 1,
                },
            ])],
        vk::Filter::LINEAR,
    );
}
impl Drop for Sr {
    fn drop(&mut self) {
        unsafe {
            // Caller only drops after device idle; uncertain GPU failures abort at the presentation boundary.
            if let Ok(f) = target_runtime::sr_function(b"slFreeResources\0") {
                if target_sr_free(f) != 0 {
                    std::process::abort();
                }
            }
            for r in self.resources.drain(..) {
                self.device
                    .destroy_image_view(vk::ImageView::from_raw(r.view), None);
                self.device
                    .destroy_image(vk::Image::from_raw(r.image), None);
                self.device
                    .free_memory(vk::DeviceMemory::from_raw(r.memory), None);
            }
            if self.fence != vk::Fence::null() {
                self.device.destroy_fence(self.fence, None);
            }
            if self.pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.pool, None);
            }
        }
    }
}
