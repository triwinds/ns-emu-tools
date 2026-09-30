//! Opt-in, one-frame GPU experiment for the pinned emulator. Never enabled by the UI.
use super::*;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
#[path = "scale_replace.rs"]
mod replacement;
static DONE: AtomicBool = AtomicBool::new(false);
#[derive(Default)]
struct Evidence {
    rows: Vec<Value>,
    queues: HashMap<(u64, u64), u32>,
    readable: HashMap<(u64, u64), i32>,
    presents: u32,
}
static EVIDENCE: OnceLock<Mutex<Evidence>> = OnceLock::new();
fn evidence() -> std::sync::MutexGuard<'static, Evidence> {
    EVIDENCE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("NS_STREAMLINE_SCALE_COPY").as_deref() == Ok("1")
            && std::env::var("NS_STREAMLINE_SCALE_PROBE").as_deref() == Ok("1")
            && std::env::var("NS_STREAMLINE_TARGET_SDK").as_deref() != Ok("1")
            && trace::authorized()
    })
}
pub(super) fn observe(row: &Value) {
    if !enabled() || DONE.load(Ordering::Relaxed) {
        return;
    }
    let mut e = evidence();
    if row["call"] == "vkGetDeviceQueue" || row["call"] == "vkGetDeviceQueue2" {
        if let (Some(d), Some(q), Some(f)) = (
            row["device"].as_u64(),
            row["data"]["queue"].as_u64(),
            row["data"]["family"].as_u64(),
        ) {
            e.queues.insert((d, q), f as u32);
        }
    }
    if row["call"] == "present" {
        e.presents += 1;
    }
    if e.rows.len() < 250_000 {
        e.rows.push(row.clone());
    }
}
pub(super) unsafe fn swapchain_usage(
    d: Device,
    info: &vk::SwapchainCreateInfoKHR,
) -> vk::ImageUsageFlags {
    if !enabled() {
        return info.image_usage;
    }
    let Some(i) = instance(d.physical) else {
        return info.image_usage;
    };
    let Some(f) = (i.gipa)(
        i.handle,
        c"vkGetPhysicalDeviceSurfaceCapabilitiesKHR".as_ptr(),
    ) else {
        return info.image_usage;
    };
    let f: vk::PFN_vkGetPhysicalDeviceSurfaceCapabilitiesKHR = std::mem::transmute(f);
    let mut caps = vk::SurfaceCapabilitiesKHR::default();
    if f(d.physical, info.surface, &mut caps) == vk::Result::SUCCESS
        && caps
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_SRC)
    {
        info.image_usage | vk::ImageUsageFlags::TRANSFER_SRC
    } else {
        info.image_usage
    }
}
pub(super) fn swapchain_created(
    d: Device,
    chain: vk::SwapchainKHR,
    info: &vk::SwapchainCreateInfoKHR,
    r: vk::Result,
) {
    if enabled()
        && r == vk::Result::SUCCESS
        && info.image_usage.contains(vk::ImageUsageFlags::TRANSFER_SRC)
    {
        evidence().readable.insert(
            (d.handle.as_raw(), chain.as_raw()),
            info.image_format.as_raw(),
        );
    }
}
struct Resources {
    device: ash::Device,
    pool: vk::CommandPool,
    fence: vk::Fence,
    buffers: Vec<(vk::Buffer, vk::DeviceMemory)>,
    images: Vec<(vk::Image, vk::DeviceMemory)>,
}
impl Drop for Resources {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_command_pool(self.pool, None);
            for (b, m) in self.buffers.drain(..) {
                self.device.destroy_buffer(b, None);
                self.device.free_memory(m, None);
            }
            for (im, m) in self.images.drain(..) {
                self.device.destroy_image(im, None);
                self.device.free_memory(m, None);
            }
            self.device.destroy_fence(self.fence, None);
        }
    }
}
type Result<T> = std::result::Result<T, String>;
fn checked<T>(r: std::result::Result<T, vk::Result>) -> Result<T> {
    r.map_err(|e| format!("{e:?}"))
}
unsafe fn buffer(
    r: &mut Resources,
    props: &vk::PhysicalDeviceMemoryProperties,
    size: u64,
) -> Result<()> {
    let b = checked(
        r.device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(size)
                .usage(vk::BufferUsageFlags::TRANSFER_DST),
            None,
        ),
    )?;
    let req = r.device.get_buffer_memory_requirements(b);
    let ty = (0..props.memory_type_count).find(|i| {
        req.memory_type_bits & (1 << i) != 0
            && props.memory_types[*i as usize].property_flags.contains(
                vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
            )
    });
    let Some(ty) = ty else {
        r.device.destroy_buffer(b, None);
        return Err("no coherent readback memory".into());
    };
    let m = match checked(
        r.device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(req.size)
                .memory_type_index(ty),
            None,
        ),
    ) {
        Ok(m) => m,
        Err(e) => {
            r.device.destroy_buffer(b, None);
            return Err(e);
        }
    };
    r.buffers.push((b, m));
    checked(r.device.bind_buffer_memory(b, m, 0))
}
fn barrier(
    image: vk::Image,
    old: vk::ImageLayout,
    new: vk::ImageLayout,
    src: vk::AccessFlags,
    dst: vk::AccessFlags,
) -> vk::ImageMemoryBarrier<'static> {
    vk::ImageMemoryBarrier::default()
        .image(image)
        .old_layout(old)
        .new_layout(new)
        .src_access_mask(src)
        .dst_access_mask(dst)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1),
        )
}
fn extent(v: &Value) -> Result<vk::Extent3D> {
    let w = v[0].as_u64().unwrap_or(0);
    let h = v[1].as_u64().unwrap_or(0);
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        return Err("unsupported readback extent".into());
    }
    Ok(vk::Extent3D {
        width: w as u32,
        height: h as u32,
        depth: 1,
    })
}
unsafe fn copy(
    d: Device,
    queue: vk::Queue,
    info: &vk::PresentInfoKHR,
    candidate: &Value,
    family: u32,
    format: i32,
) -> Result<()> {
    let i = instance(d.physical).ok_or("missing instance dispatch")?;
    let instance = ash::Instance::load(
        &ash::StaticFn {
            get_instance_proc_addr: i.gipa,
        },
        i.handle,
    );
    let device = ash::Device::load_with(
        |n| (d.gdpa)(d.handle, n.as_ptr()).map_or(std::ptr::null(), |f| f as *const _),
        d.handle,
    );
    let mut r = Resources {
        device,
        pool: vk::CommandPool::null(),
        fence: vk::Fence::null(),
        buffers: vec![],
        images: vec![],
    };
    let source = &candidate["sources"][0];
    if source["image"]["usage"].as_u64().unwrap_or(0) & 1 == 0
        || format != vk::Format::B8G8R8A8_UNORM.as_raw()
    {
        return Err("unsupported transfer usage or output format".into());
    }
    let sizes = [
        extent(&source["image"]["extent"])?,
        extent(&candidate["output_extent"])?,
    ];
    let images = [
        vk::Image::from_raw(source["image"]["image"].as_u64().ok_or("missing source")?),
        vk::Image::from_raw(
            candidate["destinations"][0]
                .as_u64()
                .ok_or("missing destination")?,
        ),
    ];
    let props = instance.get_physical_device_memory_properties(d.physical);
    for size in sizes {
        buffer(&mut r, &props, size.width as u64 * size.height as u64 * 4)?;
    }
    r.pool = checked(r.device.create_command_pool(
        &vk::CommandPoolCreateInfo::default().queue_family_index(family),
        None,
    ))?;
    r.fence = checked(r.device.create_fence(&vk::FenceCreateInfo::default(), None))?;
    let command = checked(
        r.device.allocate_command_buffers(
            &vk::CommandBufferAllocateInfo::default()
                .command_pool(r.pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1),
        ),
    )?[0];
    if let Some(set) = d.set_loader_data {
        let result = set(d.handle, command.as_raw() as *mut _);
        if result != vk::Result::SUCCESS {
            return Err(format!("loader dispatch {result:?}"));
        }
    }
    checked(r.device.begin_command_buffer(
        command,
        &vk::CommandBufferBeginInfo::default().flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
    ))?;
    let layouts = [vk::ImageLayout::GENERAL, vk::ImageLayout::PRESENT_SRC_KHR];
    let before: Vec<_> = images
        .iter()
        .zip(layouts)
        .map(|(im, l)| {
            barrier(
                *im,
                l,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::MEMORY_READ,
                vk::AccessFlags::TRANSFER_READ,
            )
        })
        .collect();
    r.device.cmd_pipeline_barrier(
        command,
        vk::PipelineStageFlags::ALL_COMMANDS,
        vk::PipelineStageFlags::TRANSFER,
        vk::DependencyFlags::empty(),
        &[],
        &[],
        &before,
    );
    for index in 0..2 {
        let region = vk::BufferImageCopy::default()
            .image_subresource(
                vk::ImageSubresourceLayers::default()
                    .aspect_mask(vk::ImageAspectFlags::COLOR)
                    .layer_count(1),
            )
            .image_extent(sizes[index]);
        r.device.cmd_copy_image_to_buffer(
            command,
            images[index],
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            r.buffers[index].0,
            &[region],
        );
    }
    let after: Vec<_> = images
        .iter()
        .zip(layouts)
        .map(|(im, l)| {
            barrier(
                *im,
                vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                l,
                vk::AccessFlags::TRANSFER_READ,
                vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
            )
        })
        .collect();
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
        &after,
    );
    checked(r.device.end_command_buffer(command))?;
    let waits = if info.wait_semaphore_count == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
    };
    let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; waits.len()];
    let commands = [command];
    let submit = vk::SubmitInfo::default()
        .wait_semaphores(waits)
        .wait_dst_stage_mask(&stages)
        .command_buffers(&commands);
    // Explicitly authorized diagnostic failure policy. Do not unwind in-flight resources or reuse consumed waits.
    if let Err(error) = r
        .device
        .queue_submit(queue, &[submit], r.fence)
        .and_then(|_| r.device.wait_for_fences(&[r.fence], true, 10_000_000_000))
    {
        trace::record("scale_copy_fatal", json!({"error":format!("{error:?}")}));
        std::process::abort();
    }
    // From here onward the original waits have been consumed. File/map errors must not request fallback.
    let dir = std::env::var_os("NS_STREAMLINE_PROBE_TRACE")
        .map(std::path::PathBuf::from)
        .and_then(|p| p.parent().map(std::path::Path::to_owned));
    let mut files = vec![];
    if let Some(dir) = dir {
        for (index, name) in ["source.rgba", "present.bgra"].iter().enumerate() {
            let count = sizes[index].width as usize * sizes[index].height as usize * 4;
            match r.device.map_memory(
                r.buffers[index].1,
                0,
                count as u64,
                vk::MemoryMapFlags::empty(),
            ) {
                Ok(ptr) => {
                    let bytes = std::slice::from_raw_parts(ptr.cast::<u8>(), count);
                    let result = std::fs::write(dir.join(name), bytes);
                    r.device.unmap_memory(r.buffers[index].1);
                    files.push(json!({"file":name,"bytes":count,"saved":result.is_ok()}));
                }
                Err(e) => files.push(json!({"file":name,"error":format!("{e:?}")})),
            }
        }
        let replacement = if std::env::var("NS_STREAMLINE_SCALE_REPLACE").as_deref() == Ok("1") {
            match replacement::run(
                &mut r, &instance, d.physical, queue, command, candidate, images, sizes, &dir,
            ) {
                Ok(value) => value,
                Err(reason) => {
                    json!({"requested":true,"displayed_image_replaced":false,"reason":reason})
                }
            }
        } else {
            json!({"requested":false,"displayed_image_replaced":false})
        };
        trace::record("scale_replacement_result", replacement.clone());
        let report = json!({"candidate":candidate,"files":files,"gpu_fence_completed":true,"displayed_image_replaced":replacement["displayed_image_replaced"],"replacement":replacement,"sr_evaluated":false,"source_extent":[sizes[0].width,sizes[0].height],"output_extent":[sizes[1].width,sizes[1].height]});
        let _ = std::fs::write(
            dir.join("scale-copy.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        );
    }
    trace::record(
        "scale_copy_complete",
        json!({"gpu_fence_completed":true,"sr_evaluated":false,"files":files}),
    );
    Ok(())
}
pub(super) unsafe fn before_present(
    d: Device,
    queue: vk::Queue,
    info: &vk::PresentInfoKHR,
) -> bool {
    if !enabled() || DONE.load(Ordering::Relaxed) {
        return false;
    }
    let mut e = evidence();
    let at = std::env::var("NS_STREAMLINE_SCALE_COPY_FRAME")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| (1..=3000).contains(n))
        .unwrap_or(60);
    if e.presents < at {
        return false;
    }
    if DONE.swap(true, Ordering::Relaxed) {
        return false;
    }
    let selected = (|| -> Result<(Value, u32, i32)> {
        if info.swapchain_count != 1 || !info.p_next.is_null() {
            return Err("present extension or multiple swapchains".into());
        }
        let chain = (*info.p_swapchains).as_raw();
        let family = *e
            .queues
            .get(&(d.handle.as_raw(), queue.as_raw()))
            .ok_or("unknown queue")?;
        let format = *e
            .readable
            .get(&(d.handle.as_raw(), chain))
            .ok_or("unreadable swapchain")?;
        let waits: Vec<_> = if info.wait_semaphore_count == 0 {
            vec![]
        } else {
            std::slice::from_raw_parts(info.p_wait_semaphores, info.wait_semaphore_count as usize)
                .iter()
                .map(|s| s.as_raw())
                .collect()
        };
        e.rows.sort_by_key(|r| r["seq"].as_u64().unwrap_or(0));
        let seq = e.rows.last().and_then(|r| r["seq"].as_u64()).unwrap_or(0) + 1;
        e.rows.push(json!({"device":d.handle.as_raw(),"object":queue.as_raw(),"call":"present","seq":seq,"frame":at,"data":{"chains":[chain],"indices":[*info.p_image_indices],"waits":waits}}));
        let report = crate::scale_model::analyze(&e.rows, false);
        let candidates = report["frames"]
            .as_array()
            .and_then(|f| f.last())
            .and_then(|f| f["candidates"].as_array());
        let c = candidates
            .filter(|c| c.len() == 1)
            .and_then(|c| c.first())
            .filter(|c| c["contract"]["recorded_contract_matches"] == true)
            .ok_or("observed contract mismatch")?;
        if report["unknown_templates"] != 0 || report["descriptor_copies_unresolved"] != 0 {
            return Err("unresolved descriptors".into());
        }
        // Diagnostic-only eligibility: also require a recorded GENERAL source dependency.
        let source = c["sources"][0]["image"]["image"]
            .as_u64()
            .ok_or("source absent")?;
        let barrier_seen = c["operations"].as_array().into_iter().flatten().any(|o| {
            o["seq"].as_u64() < c["seq"].as_u64()
                && o["data"]["images"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|b| {
                        b["image"].as_u64() == Some(source)
                            && b["new"] == 1
                            && b["src_family"] == 4294967295_u64
                            && b["dst_family"] == 4294967295_u64
                    })
        });
        if !barrier_seen {
            return Err("source dependency not observed".into());
        }
        if e.rows.iter().any(|r| {
            r["seq"].as_u64() > c["seq"].as_u64()
                && r["call"] == "vkDestroyImage"
                && r["data"]["image"].as_u64() == Some(source)
        }) {
            return Err("source destroyed after draw".into());
        }
        Ok((c.clone(), family, format))
    })();
    e.rows.clear();
    drop(e);
    match selected.and_then(|(c, f, format)| copy(d, queue, info, &c, f, format)) {
        Ok(()) => true,
        Err(reason) => {
            trace::record("scale_copy_skipped", json!({"reason":reason}));
            false
        }
    }
}
