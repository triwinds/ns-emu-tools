//! Compare a private GPU blit against the same original frame before overwriting one presentation image.
use super::*;
use std::path::Path;
unsafe fn read(r: &Resources, index: usize, count: usize) -> Result<Vec<u8>> {
    let p = checked(r.device.map_memory(
        r.buffers[index].1,
        0,
        count as u64,
        vk::MemoryMapFlags::empty(),
    ))?;
    let bytes = std::slice::from_raw_parts(p.cast::<u8>(), count).to_vec();
    r.device.unmap_memory(r.buffers[index].1);
    Ok(bytes)
}
unsafe fn begin(r: &Resources, command: vk::CommandBuffer) -> Result<()> {
    checked(
        r.device
            .reset_command_pool(r.pool, vk::CommandPoolResetFlags::empty()),
    )?;
    checked(r.device.reset_fences(&[r.fence]))?;
    checked(r.device.begin_command_buffer(
        command,
        &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
    ))
}
unsafe fn finish(r: &Resources, queue: vk::Queue, command: vk::CommandBuffer) -> Result<()> {
    checked(r.device.end_command_buffer(command))?;
    let commands = [command];
    let submit = vk::SubmitInfo::default().command_buffers(&commands);
    if let Err(error) = r
        .device
        .queue_submit(queue, &[submit], r.fence)
        .and_then(|_| r.device.wait_for_fences(&[r.fence], true, 10_000_000_000))
    {
        trace::record("scale_replace_fatal", json!({"error":format!("{error:?}")}));
        std::process::abort();
    }
    Ok(())
}
fn layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .layer_count(1)
}
fn region(size: vk::Extent3D) -> vk::BufferImageCopy {
    vk::BufferImageCopy::default()
        .image_subresource(layers())
        .image_extent(size)
}
fn compare(a: &[u8], b: &[u8]) -> Result<(u8, f64)> {
    if a.len() != b.len() || a.is_empty() || a.len() % 4 != 0 {
        return Err("invalid comparison buffers".into());
    }
    let mut max = 0;
    let mut sum = 0_u64;
    for (a, b) in a.iter().zip(b) {
        let d = a.abs_diff(*b);
        max = max.max(d);
        sum += d as u64;
    }
    Ok((max, sum as f64 / a.len() as f64))
}
fn offsets(c: &Value, sizes: [vk::Extent3D; 2]) -> Result<([vk::Offset3D; 2], [vk::Offset3D; 2])> {
    let uv = c["contract"]["normalized_source_endpoints"]
        .as_array()
        .ok_or("missing source coordinates")?;
    let viewport = c["viewport"]["viewports"][0]
        .as_array()
        .ok_or("missing viewport")?;
    if uv.len() != 4 || viewport.len() != 6 {
        return Err("unsupported coordinates".into());
    }
    let integer = |v: f64| -> Result<i32> {
        if v.is_finite() && (v - v.round()).abs() < 0.0001 && v >= 0.0 && v <= 8192.0 {
            Ok(v.round() as i32)
        } else {
            Err("fractional/out-of-bounds blit endpoints".into())
        }
    };
    let get = |v: &Value| -> Result<f64> { v.as_f64().ok_or_else(|| "invalid coordinate".into()) };
    let sx1 = integer(get(&uv[0])? * sizes[0].width as f64)?;
    let sx2 = integer(get(&uv[1])? * sizes[0].width as f64)?;
    let sy1 = integer(get(&uv[2])? * sizes[0].height as f64)?;
    let sy2 = integer(get(&uv[3])? * sizes[0].height as f64)?;
    let x = integer(get(&viewport[0])?)?;
    let y = integer(get(&viewport[1])?)?;
    let w = integer(get(&viewport[2])?)?;
    let h = integer(get(&viewport[3])?)?;
    if w == 0
        || h == 0
        || x + w > sizes[1].width as i32
        || y + h > sizes[1].height as i32
        || sx1 == sx2
        || sy1 == sy2
        || sx1.max(sx2) > sizes[0].width as i32
        || sy1.max(sy2) > sizes[0].height as i32
    {
        return Err("invalid blit region".into());
    }
    Ok((
        [
            vk::Offset3D {
                x: sx1,
                y: sy1,
                z: 0,
            },
            vk::Offset3D {
                x: sx2,
                y: sy2,
                z: 1,
            },
        ],
        [
            vk::Offset3D { x, y, z: 0 },
            vk::Offset3D {
                x: x + w,
                y: y + h,
                z: 1,
            },
        ],
    ))
}
#[allow(clippy::too_many_arguments)]
pub(super) unsafe fn run(
    r: &mut Resources,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    queue: vk::Queue,
    command: vk::CommandBuffer,
    c: &Value,
    images: [vk::Image; 2],
    sizes: [vk::Extent3D; 2],
    dir: &Path,
) -> Result<Value> {
    let count = sizes[1].width as usize * sizes[1].height as usize * 4;
    let original = read(r, 1, count)?;
    let source = read(r, 0, sizes[0].width as usize * sizes[0].height as usize * 4)?;
    // A transfer blit cannot implement the shader's forced-alpha behavior for nonopaque sources.
    if !source.chunks_exact(4).all(|p| p[3] == 255) {
        return Err("source alpha not opaque; requires shader reconstruction".into());
    }
    let min = source
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter())
        .copied()
        .min()
        .unwrap_or(0);
    let max = source
        .chunks_exact(4)
        .flat_map(|p| p[..3].iter())
        .copied()
        .max()
        .unwrap_or(0);
    if max - min < 32 {
        return Err("reference too uniform for meaningful replacement evidence".into());
    }
    let (src, dst) = offsets(c, sizes)?;
    let src_features = instance
        .get_physical_device_format_properties(physical, vk::Format::R8G8B8A8_UNORM)
        .optimal_tiling_features;
    let dst_features = instance
        .get_physical_device_format_properties(physical, vk::Format::B8G8R8A8_UNORM)
        .optimal_tiling_features;
    if !src_features.contains(
        vk::FormatFeatureFlags::BLIT_SRC | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
    ) || !dst_features.contains(vk::FormatFeatureFlags::BLIT_DST)
    {
        return Err("unsupported linear blit".into());
    }
    let props = instance.get_physical_device_memory_properties(physical);
    buffer(r, &props, count as u64)?;
    buffer(r, &props, count as u64)?;
    let image = checked(
        r.device.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::B8G8R8A8_UNORM)
                .extent(sizes[1])
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC),
            None,
        ),
    )?;
    let req = r.device.get_image_memory_requirements(image);
    let ty = (0..props.memory_type_count).find(|i| req.memory_type_bits & (1 << i) != 0);
    let Some(ty) = ty else {
        r.device.destroy_image(image, None);
        return Err("no image memory type".into());
    };
    let memory = match checked(
        r.device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(ty),
            None,
        ),
    ) {
        Ok(m) => m,
        Err(e) => {
            r.device.destroy_image(image, None);
            return Err(e);
        }
    };
    r.images.push((image, memory));
    checked(r.device.bind_image_memory(image, memory, 0))?;
    begin(r, command)?;
    let before = [
        barrier(
            images[0],
            vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::MEMORY_READ,
            vk::AccessFlags::TRANSFER_READ,
        ),
        barrier(
            image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::empty(),
            vk::AccessFlags::TRANSFER_WRITE,
        ),
    ];
    r.device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &before,
    );
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1);
    r.device.cmd_clear_color_image(
        command,
        image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 1.0],
        },
        &[range],
    );
    r.device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[barrier(
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::TRANSFER_WRITE,
        )],
    );
    let blit = vk::ImageBlit::default()
        .src_subresource(layers())
        .dst_subresource(layers())
        .src_offsets(src)
        .dst_offsets(dst);
    r.device.cmd_blit_image(
        command,
        images[0],
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &[blit],
        vk::Filter::LINEAR,
    );
    r.device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[barrier(
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::TRANSFER_READ,
        )],
    );
    r.device.cmd_copy_image_to_buffer(
        command,
        image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        r.buffers[2].0,
        &[region(sizes[1])],
    );
    let host = vk::MemoryBarrier::default()
        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
        .dst_access_mask(vk::AccessFlags::HOST_READ);
    r.device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::ALL_COMMANDS | vk::PipelineStageFlags::HOST,
        vk::DependencyFlags::empty(),
        &[host],
        &[],
        &[barrier(
            images[0],
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::GENERAL,
            vk::AccessFlags::TRANSFER_READ,
            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
        )],
    );
    finish(r, queue, command)?;
    let rebuilt = read(r, 2, count)?;
    let (max, mean) = compare(&original, &rebuilt)?;
    let _ = std::fs::write(dir.join("gpu-reconstructed.bgra"), &rebuilt);
    if max > 2 || mean > 0.15 {
        return Ok(
            json!({"requested":true,"comparison_passed":false,"max_channel_error":max,"mean_channel_error":mean,"displayed_image_replaced":false}),
        );
    }
    begin(r, command)?;
    r.device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[barrier(
            images[1],
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
            vk::AccessFlags::TRANSFER_WRITE,
        )],
    );
    let copy = vk::ImageCopy::default()
        .src_subresource(layers())
        .dst_subresource(layers())
        .extent(sizes[1]);
    r.device.cmd_copy_image(
        command,
        image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        images[1],
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        &[copy],
    );
    r.device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &[barrier(
            images[1],
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::AccessFlags::TRANSFER_WRITE,
            vk::AccessFlags::TRANSFER_READ,
        )],
    );
    r.device.cmd_copy_image_to_buffer(
        command,
        images[1],
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        r.buffers[3].0,
        &[region(sizes[1])],
    );
    r.device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::TRANSFER,
        vk::PipelineStageFlags::ALL_COMMANDS | vk::PipelineStageFlags::HOST,
        vk::DependencyFlags::empty(),
        &[host],
        &[],
        &[barrier(
            images[1],
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::AccessFlags::TRANSFER_READ,
            vk::AccessFlags::MEMORY_READ,
        )],
    );
    finish(r, queue, command)?;
    // The replacement has completed: any subsequent evidence failure must retain that fact.
    let readback = match read(r, 3, count) {
        Ok(actual) => {
            let identical = actual == rebuilt;
            let saved = std::fs::write(dir.join("replaced.bgra"), actual).is_ok();
            json!({"identical_to_reconstruction":identical,"saved":saved})
        }
        Err(e) => json!({"error":e}),
    };
    Ok(
        json!({"requested":true,"comparison_passed":true,"max_channel_error":max,"mean_channel_error":mean,"displayed_image_replaced":true,"replacement_fence_completed":true,"replacement_readback":readback,"original_draw_retained":true,"sr_evaluated":false,"reduced_application_workload":false,"method":"one_frame_gpu_linear_blit_output_overwrite"}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pixel_comparison_includes_alpha_and_rejects_invalid_buffers() {
        assert_eq!(
            compare(&[0, 1, 2, 255], &[0, 2, 2, 255]).unwrap(),
            (1, 0.25)
        );
        assert_eq!(compare(&[0, 0, 0, 255], &[0, 0, 0, 0]).unwrap().0, 255);
        assert!(compare(&[], &[]).is_err());
        assert!(compare(&[1, 2, 3], &[1, 2, 3]).is_err());
    }
    #[test]
    fn blit_offsets_preserve_flip_and_reject_overflow() {
        let sizes = [
            vk::Extent3D {
                width: 1920,
                height: 1080,
                depth: 1,
            },
            vk::Extent3D {
                width: 2560,
                height: 1440,
                depth: 1,
            },
        ];
        let mut c = json!({"contract":{"normalized_source_endpoints":[0,1,1,0]},"viewport":{"viewports":[[0,0,2560,1440,0,1]]}});
        let (s, d) = offsets(&c, sizes).unwrap();
        assert_eq!((s[0].y, s[1].y, d[1].x), (1080, 0, 2560));
        c["viewport"]["viewports"][0][2] = json!(2561);
        assert!(offsets(&c, sizes).is_err());
    }
}
