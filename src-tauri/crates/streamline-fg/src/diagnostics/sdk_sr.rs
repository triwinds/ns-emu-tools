//! Isolated native Vulkan SR smoke test. Synthetic guides do not establish game image quality.
use crate::fg_api::{self, Api, Resource};
use crate::host::{hash, write_json, Result};
use ash::vk::{self, Handle};
use libloading::Library;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{ffi::c_void, path::Path};

pub(super) const PLUGINS: &[&str] = &["sl.dlss.dll", "nvngx_dlss.dll"];
pub(super) fn verify_plugin(path: &Path, name: &str) -> Result<String> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("../../sdk-route/sr-runtime.json"))?;
    let expected = manifest["files"]
        .as_array()
        .ok_or("missing SR manifest")?
        .iter()
        .find(|v| v["name"] == name)
        .and_then(|v| v["sha256"].as_str())
        .ok_or("unknown SR plugin")?;
    let actual = hash(path)?;
    if actual != expected {
        return Err(format!("SR runtime hash mismatch: {}", path.display()).into());
    }
    Ok(actual)
}
unsafe extern "C" {
    fn probe_sr_options(api: &Api, width: u32, height: u32, input: *mut u32) -> i32;
    fn probe_sr_evaluate(
        api: &Api,
        evaluate: *mut c_void,
        command: u64,
        frame: u32,
        reset: u32,
        resources: *const Resource,
    ) -> i32;
    fn target_sr_options(
        api: &Api,
        width: u32,
        height: u32,
        mode: u32,
        preset: u32,
        input: *mut u32,
    ) -> i32;
    fn target_sr_evaluate(
        api: &Api,
        evaluate: *mut c_void,
        command: u64,
        token: u64,
        reset: u32,
        resources: *const Resource,
    ) -> i32;
    fn target_sr_free(function: *mut c_void) -> i32;
    fn probe_sr_free(function: *mut c_void) -> i32;
}
pub(super) unsafe fn exercise(
    session: &Path,
    sdk: &Library,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
) -> Result<()> {
    exercise_scoped(session, sdk, instance, physical, device, family, None)
}
pub(super) type RecordingScope = fn(vk::CommandBuffer, u32, u32) -> Box<dyn std::any::Any>;
pub(super) unsafe fn exercise_scoped(
    session: &Path,
    sdk: &Library,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
    scope: Option<RecordingScope>,
) -> Result<()> {
    // A failed submit/wait may leave GPU work in flight. The parent bounds this
    // child process; do not unwind through SDK/device cleanup on uncertain state.
    if let Err(error) = run(session, sdk, instance, physical, device, family, scope) {
        let _ = write_json(
            &session.join("sr-failure.json"),
            &json!({"error":error.to_string(),"sr_verified":false}),
        );
        eprintln!("SR experiment failed: {error}");
        std::process::abort();
    }
    Ok(())
}
unsafe fn buffer(
    device: &ash::Device,
    props: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
    usage: vk::BufferUsageFlags,
) -> Result<(vk::Buffer, vk::DeviceMemory)> {
    let buffer = device.create_buffer(
        &vk::BufferCreateInfo::default().size(size).usage(usage),
        None,
    )?;
    let req = device.get_buffer_memory_requirements(buffer);
    let memory = device.allocate_memory(
        &vk::MemoryAllocateInfo::default()
            .allocation_size(req.size)
            .memory_type_index(fg_api::memory_type(
                props,
                req.memory_type_bits,
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )?),
        None,
    )?;
    device.bind_buffer_memory(buffer, memory, 0)?;
    Ok((buffer, memory))
}
fn region(size: vk::Extent2D) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .image_subresource(
            vk::ImageSubresourceLayers::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1),
        )
        .image_extent(vk::Extent3D {
            width: size.width,
            height: size.height,
            depth: 1,
        })
}
unsafe fn run(
    session: &Path,
    sdk: &Library,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
    scope: Option<RecordingScope>,
) -> Result<()> {
    type Address = unsafe extern "C" fn();
    let api = Api {
        feature: *sdk.get::<Address>(b"slGetFeatureFunction\0")? as *mut _,
        token: *sdk.get::<Address>(b"slGetNewFrameToken\0")? as *mut _,
        constants: *sdk.get::<Address>(b"slSetConstants\0")? as *mut _,
        tags: *sdk.get::<Address>(b"slSetTagForFrame\0")? as *mut _,
    };
    let evaluate = *sdk.get::<Address>(b"slEvaluateFeature\0")? as *mut _;
    let free = *sdk.get::<Address>(b"slFreeResources\0")? as *mut _;
    let props = instance.get_physical_device_memory_properties(physical);
    let pool = device.create_command_pool(
        &vk::CommandPoolCreateInfo::default()
            .queue_family_index(family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
        None,
    )?;
    let cmd = device.allocate_command_buffers(
        &vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1),
    )?[0];
    let queue = device.get_device_queue(family, 0);
    let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
    let mut rows = Vec::new();
    let mut frame = 0;
    let magpie = std::env::var("NS_STREAMLINE_SR_MAGPIE_TEST").as_deref() == Ok("1");
    for (cycle, output) in if magpie {
        [
            vk::Extent2D {
                width: 2688,
                height: 1512,
            },
            vk::Extent2D {
                width: 960,
                height: 540,
            },
        ]
    } else {
        [
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
            vk::Extent2D {
                width: 960,
                height: 540,
            },
        ]
    }
    .into_iter()
    .enumerate()
    {
        let mut input_size = if magpie {
            if cycle == 0 {
                [1920, 1080]
            } else {
                [960, 540]
            }
        } else {
            [0; 2]
        };
        fg_api::checked(
            if magpie {
                target_sr_options(
                    &api,
                    output.width,
                    output.height,
                    2,
                    10,
                    input_size.as_mut_ptr(),
                )
            } else {
                probe_sr_options(&api, output.width, output.height, input_size.as_mut_ptr())
            },
            "SR options/optimal settings",
        )?;
        if input_size.contains(&0)
            || input_size[0] > output.width
            || (!magpie && input_size[0] == output.width)
            || input_size[1] > output.height
            || (!magpie && input_size[1] == output.height)
        {
            return Err("SR returned invalid/non-upscaling optimal size".into());
        }
        let input = vk::Extent2D {
            width: input_size[0],
            height: input_size[1],
        };
        write_json(
            &session.join(format!("sr-plan-{cycle}.json")),
            &json!({"input":input_size,"output":[output.width,output.height],"mode":if magpie {"balanced"} else {"quality"},"preset":if magpie {"j"} else {"default"},"depth":if magpie {0.0} else {0.5},"motion_format":"fp16_input_pixels"}),
        )?;
        let usage = vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
        let mut resources = Vec::new();
        for (extent, format) in [
            (input, vk::Format::R8G8B8A8_UNORM),
            (output, vk::Format::R8G8B8A8_UNORM),
            (input, vk::Format::R32_SFLOAT),
            (input, vk::Format::R16G16_SFLOAT),
        ] {
            let supported = instance
                .get_physical_device_format_properties(physical, format)
                .optimal_tiling_features;
            if !supported.contains(
                vk::FormatFeatureFlags::SAMPLED_IMAGE
                    | vk::FormatFeatureFlags::STORAGE_IMAGE
                    | vk::FormatFeatureFlags::TRANSFER_SRC
                    | vk::FormatFeatureFlags::TRANSFER_DST,
            ) {
                return Err(format!("SR diagnostic format unsupported: {format:?}").into());
            }
            resources.push(fg_api::texture_with_usage(
                device, &props, extent, format, usage,
            )?);
        }
        let input_bytes = u64::from(input.width) * u64::from(input.height) * 4;
        let output_bytes = u64::from(output.width) * u64::from(output.height) * 4;
        let (upload, upload_memory) = buffer(
            device,
            &props,
            input_bytes,
            vk::BufferUsageFlags::TRANSFER_SRC,
        )?;
        let (readback, readback_memory) = buffer(
            device,
            &props,
            output_bytes,
            vk::BufferUsageFlags::TRANSFER_DST,
        )?;
        let mut hashes = Vec::new();
        for local_frame in 0..3 {
            let reset = local_frame != 1;
            let pixels = std::slice::from_raw_parts_mut(
                device.map_memory(upload_memory, 0, input_bytes, vk::MemoryMapFlags::empty())?
                    as *mut u8,
                input_bytes as usize,
            );
            for y in 0..input.height {
                for x in 0..input.width {
                    let bright = ((x / 24 + y / 24) % 2 == 0) ^ (local_frame == 2);
                    let i = ((y * input.width + x) * 4) as usize;
                    pixels[i..i + 4].copy_from_slice(if bright {
                        &[220, 180, 80, 255]
                    } else {
                        &[24, 40, 100, 255]
                    });
                }
            }
            device.unmap_memory(upload_memory);
            device.reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())?;
            device.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )?;
            for (i, res) in resources.iter().enumerate() {
                let image = vk::Image::from_raw(res.image);
                fg_api::transition(
                    device,
                    cmd,
                    image,
                    if local_frame == 0 {
                        vk::ImageLayout::UNDEFINED
                    } else {
                        vk::ImageLayout::GENERAL
                    },
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                );
                if i == 0 {
                    device.cmd_copy_buffer_to_image(
                        cmd,
                        upload,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &[region(input)],
                    );
                } else {
                    device.cmd_clear_color_image(
                        cmd,
                        image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue {
                            float32: if i == 2 && !magpie {
                                [0.5; 4]
                            } else if i == 1 {
                                [1.0, 0.0, 1.0, 1.0]
                            } else {
                                [0.0; 4]
                            },
                        },
                        &[fg_api::range()],
                    );
                }
                fg_api::transition(
                    device,
                    cmd,
                    image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::GENERAL,
                );
            }
            let recording = scope.map(|scope| scope(cmd, input.width, input.height));
            let result = if magpie {
                // SR-only diagnostics have no Reflex context. Request only the
                // frame token, without the game path's Reflex sleep marker.
                let mut token = std::ptr::null_mut();
                let get: unsafe extern "C" fn(*mut *mut c_void, *const u32) -> i32 =
                    std::mem::transmute(api.token);
                fg_api::checked(get(&mut token, &frame), "SR frame token")?;
                target_sr_evaluate(
                    &api,
                    evaluate,
                    cmd.as_raw(),
                    token as u64,
                    u32::from(reset),
                    resources.as_ptr(),
                )
            } else {
                probe_sr_evaluate(
                    &api,
                    evaluate,
                    cmd.as_raw(),
                    frame,
                    u32::from(reset),
                    resources.as_ptr(),
                )
            };
            fg_api::checked(result, "Vulkan SR evaluate")?;
            drop(recording);
            let image = vk::Image::from_raw(resources[1].image);
            fg_api::transition(
                device,
                cmd,
                image,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            );
            device.cmd_copy_image_to_buffer(
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                readback,
                &[region(output)],
            );
            fg_api::transition(
                device,
                cmd,
                image,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
            device.cmd_pipeline_barrier(
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
            device.end_command_buffer(cmd)?;
            device.reset_fences(&[fence])?;
            device.queue_submit(
                queue,
                &[vk::SubmitInfo::default().command_buffers(&[cmd])],
                fence,
            )?;
            device.wait_for_fences(&[fence], true, 10_000_000_000)?;
            let mapped = device.map_memory(
                readback_memory,
                0,
                output_bytes,
                vk::MemoryMapFlags::empty(),
            )?;
            let output_data =
                std::slice::from_raw_parts(mapped as *const u8, output_bytes as usize).to_vec();
            device.unmap_memory(readback_memory);
            let written = output_data
                .chunks_exact(4)
                .filter(|p| p[..3] != [255, 0, 255])
                .count();
            let min = output_data.chunks_exact(4).map(|p| p[0]).min().unwrap();
            let max = output_data.chunks_exact(4).map(|p| p[0]).max().unwrap();
            if written < (output.width * output.height) as usize * 9 / 10
                || max.saturating_sub(min) < 32
            {
                return Err("SR output remained sentinel or lacked input contrast".into());
            }
            // Sample cell interiors so a random/nonuniform write cannot pass as SR.
            let mut matches = 0usize;
            let mut samples = 0usize;
            for y in (12..input.height).step_by(24) {
                for x in (12..input.width).step_by(24) {
                    let ox = x * output.width / input.width;
                    let oy = y * output.height / input.height;
                    let red = output_data[((oy * output.width + ox) * 4) as usize];
                    let bright = ((x / 24 + y / 24) % 2 == 0) ^ (local_frame == 2);
                    matches += usize::from(if bright { red > 160 } else { red < 90 });
                    samples += 1;
                }
            }
            if matches * 100 < samples * 95 {
                return Err("SR output did not preserve the input checker pattern".into());
            }
            let digest = format!("{:x}", Sha256::digest(&output_data));
            hashes.push(digest.clone());
            let mut ppm = format!("P6\n{} {}\n255\n", output.width, output.height).into_bytes();
            for pixel in output_data.chunks_exact(4) {
                ppm.extend_from_slice(&pixel[..3]);
            }
            std::fs::write(
                session.join(format!("sr-output-{cycle}-{local_frame}.ppm")),
                ppm,
            )?;
            rows.push(json!({"cycle":cycle,"frame":frame,"reset":reset,"input":input_size,
                "output":[output.width,output.height],"evaluate_result":0,"fence_completed":true,"written_pixels":written,"red_range":[min,max],"pattern_matches":matches,"pattern_samples":samples,"sha256":digest}));
            write_json(
                &session.join(format!("sr-frame-{frame}.json")),
                &json!(rows),
            )?;
            frame += 1;
        }
        if hashes[0] == hashes[2] {
            return Err("SR output did not respond to changed input".into());
        }
        device.device_wait_idle()?;
        fg_api::checked(
            if magpie {
                target_sr_free(free)
            } else {
                probe_sr_free(free)
            },
            "SR free resources",
        )?;
        for res in resources {
            device.destroy_image_view(vk::ImageView::from_raw(res.view), None);
            device.destroy_image(vk::Image::from_raw(res.image), None);
            device.free_memory(vk::DeviceMemory::from_raw(res.memory), None);
        }
        for (b, m) in [(upload, upload_memory), (readback, readback_memory)] {
            device.destroy_buffer(b, None);
            device.free_memory(m, None);
        }
    }
    device.destroy_fence(fence, None);
    device.destroy_command_pool(pool, None);
    write_json(
        &session.join("sr-result.json"),
        &json!({"native_vulkan_sr_verified":true,"frames":rows,
        "resized_and_recreated":true,"synthetic_inputs":true,"game_integration_verified":false,"image_quality_verified":false}),
    )
}
