//! SDK image hooks feed the scoped first-use adapter and optional read-only trace.
use super::*;
use serde_json::Value;

#[derive(Default)]
struct ImageTrace {
    creation: Value,
    bound: bool,
    name: Option<String>,
    events: Vec<Value>,
    dropped_events: u64,
}
#[derive(Default)]
struct Images(HashMap<(u64, u64), ImageTrace>);
static IMAGES: OnceLock<Mutex<Images>> = OnceLock::new();
static BUFFERS: OnceLock<Mutex<Images>> = OnceLock::new();
pub(super) fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NS_STREAMLINE_SDK_LAYOUT_TRACE").as_deref() == Ok("1"))
}
fn output_name(name: &str) -> bool {
    crate::sdk_transfer_access::selected_name(name)
}
impl Images {
    fn record(&mut self, device: u64, image: u64, event: Value) -> Option<Value> {
        let entry = self.0.get_mut(&(device, image))?;
        if entry.events.len() >= 32 {
            entry.dropped_events += 1;
            return None;
        }
        entry.events.push(event.clone());
        entry.name.as_ref().filter(|s| output_name(s)).map(|name| {
            json!({"device":device,"image":image,"name":name,"ordinal":entry.events.len(),"call":event})
        })
    }
}
fn record(device: vk::Device, image: vk::Image, event: Value) {
    if !enabled() {
        return;
    }
    let details = IMAGES.get_or_init(Default::default).lock().unwrap().record(
        device.as_raw(),
        image.as_raw(),
        event,
    );
    if let Some(details) = details {
        trace::event!("sdk_output_layout_call", details);
    }
}
pub(super) unsafe fn created(device: vk::Device, image: vk::Image, info: &vk::ImageCreateInfo) {
    if enabled() {
        IMAGES.get_or_init(Default::default).lock().unwrap().0.insert(
            (device.as_raw(), image.as_raw()),
            ImageTrace { creation: json!({"initial_layout":info.initial_layout.as_raw(),
                "format":info.format.as_raw(),"extent":[info.extent.width,info.extent.height,info.extent.depth],
                "mips":info.mip_levels,"layers":info.array_layers,"usage":info.usage.as_raw(),
                "flags":info.flags.as_raw(),"samples":info.samples.as_raw(),"tiling":info.tiling.as_raw(),
                "sharing":info.sharing_mode.as_raw(),"has_pnext":!info.p_next.is_null()}), ..Default::default() }
        );
    }
}
pub(super) fn destroyed(device: vk::Device, image: vk::Image) {
    if enabled() {
        let old = IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .remove(&(device.as_raw(), image.as_raw()));
        if let Some(entry) = old.filter(|e| e.name.as_deref().is_some_and(output_name)) {
            trace::event!(
                "sdk_output_layout_destroyed",
                json!({"device":device.as_raw(),"image":image.as_raw(),"name":entry.name,"recorded_calls":entry.events.len(),"dropped_calls":entry.dropped_events})
            );
        }
    }
}
pub(super) fn retired_device(device: vk::Device) {
    if enabled() {
        IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .retain(|(d, _), _| *d != device.as_raw());
        BUFFERS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .retain(|(d, _), _| *d != device.as_raw());
    }
}
fn buffer_record(device: vk::Device, buffer: vk::Buffer, event: Value) {
    if !enabled() {
        return;
    }
    let details = BUFFERS
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .record(device.as_raw(), buffer.as_raw(), event);
    if let Some(details) = details {
        trace::event!("sdk_buffer_transfer_call", details);
    }
}
pub(super) fn buffer_barriers(
    device: vk::Device,
    command: vk::CommandBuffer,
    src: vk::PipelineStageFlags,
    dst: vk::PipelineStageFlags,
    input: &[vk::BufferMemoryBarrier],
) {
    if !enabled() {
        return;
    }
    for b in input {
        buffer_record(
            device,
            b.buffer,
            json!({"kind":"barrier","command":command.as_raw(),"src_stage":src.as_raw(),"dst_stage":dst.as_raw(),"src_access":b.src_access_mask.as_raw(),"dst_access":b.dst_access_mask.as_raw(),"offset":b.offset,"size":b.size}),
        );
    }
}
unsafe extern "system" fn create_buffer(
    handle: vk::Device,
    info: *const vk::BufferCreateInfo,
    alloc: *const vk::AllocationCallbacks,
    out: *mut vk::Buffer,
) -> vk::Result {
    let Some(d) = device(handle) else {
        return vk::Result::ERROR_DEVICE_LOST;
    };
    let next: vk::PFN_vkCreateBuffer =
        std::mem::transmute((d.gdpa)(handle, c"vkCreateBuffer".as_ptr()).unwrap());
    let result = next(handle, info, alloc, out);
    if result == vk::Result::SUCCESS {
        crate::sdk_transfer_access::buffer_created(handle, *out, &*info);
        if enabled() {
            BUFFERS.get_or_init(Default::default).lock().unwrap().0.insert((handle.as_raw(),(*out).as_raw()),ImageTrace{creation:json!({"size":(*info).size,"usage":(*info).usage.as_raw(),"flags":(*info).flags.as_raw(),"has_pnext":!(*info).p_next.is_null()}),..Default::default()});
        }
    }
    result
}
unsafe extern "system" fn bind_buffer(
    handle: vk::Device,
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    offset: u64,
) -> vk::Result {
    let Some(d) = device(handle) else {
        return vk::Result::ERROR_DEVICE_LOST;
    };
    let next: vk::PFN_vkBindBufferMemory =
        std::mem::transmute((d.gdpa)(handle, c"vkBindBufferMemory".as_ptr()).unwrap());
    let result = next(handle, buffer, memory, offset);
    if result == vk::Result::SUCCESS {
        crate::sdk_transfer_access::buffer_bound(handle, buffer);
        if enabled() {
            if let Some(entry) = BUFFERS
                .get_or_init(Default::default)
                .lock()
                .unwrap()
                .0
                .get_mut(&(handle.as_raw(), buffer.as_raw()))
            {
                entry.bound = true;
            }
            buffer_record(
                handle,
                buffer,
                json!({"kind":"bind","memory":memory.as_raw(),"offset":offset}),
            );
        }
    }
    result
}
unsafe extern "system" fn destroy_buffer(
    handle: vk::Device,
    buffer: vk::Buffer,
    alloc: *const vk::AllocationCallbacks,
) {
    let Some(d) = device(handle) else {
        std::process::abort();
    };
    let next: vk::PFN_vkDestroyBuffer =
        std::mem::transmute((d.gdpa)(handle, c"vkDestroyBuffer".as_ptr()).unwrap());
    next(handle, buffer, alloc);
    crate::sdk_transfer_access::buffer_destroyed(handle, buffer);
    if enabled() {
        BUFFERS
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .remove(&(handle.as_raw(), buffer.as_raw()));
    }
}
unsafe extern "system" fn fill_buffer(
    command: vk::CommandBuffer,
    buffer: vk::Buffer,
    offset: u64,
    size: u64,
    data: u32,
) {
    let Some(d) = device(command) else {
        std::process::abort();
    };
    let next: vk::PFN_vkCmdFillBuffer =
        std::mem::transmute((d.gdpa)(d.handle, c"vkCmdFillBuffer".as_ptr()).unwrap());
    if enabled() {
        buffer_record(
            d.handle,
            buffer,
            json!({"kind":"fill","command":command.as_raw(),"offset":offset,"size":size}),
        );
    }
    next(command, buffer, offset, size, data);
}
pub(super) unsafe fn barriers(
    device: vk::Device,
    cmd: vk::CommandBuffer,
    src: vk::PipelineStageFlags,
    dst: vk::PipelineStageFlags,
    images: &[vk::ImageMemoryBarrier],
) {
    if !enabled() {
        return;
    }
    for b in images {
        record(
            device,
            b.image,
            json!({"kind":"barrier","command":cmd.as_raw(),"old":b.old_layout.as_raw(),"new":b.new_layout.as_raw(),
            "src_stage":src.as_raw(),"dst_stage":dst.as_raw(),"src_access":b.src_access_mask.as_raw(),"dst_access":b.dst_access_mask.as_raw(),
            "src_family":b.src_queue_family_index,"dst_family":b.dst_queue_family_index,
            "range":[b.subresource_range.aspect_mask.as_raw(),b.subresource_range.base_mip_level,b.subresource_range.level_count,b.subresource_range.base_array_layer,b.subresource_range.layer_count]}),
        );
    }
}
unsafe extern "system" fn bind(
    handle: vk::Device,
    image: vk::Image,
    memory: vk::DeviceMemory,
    offset: u64,
) -> vk::Result {
    let Some(d) = device(handle) else {
        return vk::Result::ERROR_DEVICE_LOST;
    };
    let next: vk::PFN_vkBindImageMemory =
        std::mem::transmute((d.gdpa)(handle, c"vkBindImageMemory".as_ptr()).unwrap());
    let result = next(handle, image, memory, offset);
    if result == vk::Result::SUCCESS {
        crate::sdk_output_layout::bound(handle, image);
        crate::sdk_transfer_access::bound(handle, image);
        if let Some(entry) = IMAGES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .0
            .get_mut(&(handle.as_raw(), image.as_raw()))
        {
            entry.bound = true;
        }
        record(
            handle,
            image,
            json!({"kind":"bind","memory":memory.as_raw(),"offset":offset}),
        );
    }
    result
}
unsafe extern "system" fn name(
    handle: vk::Device,
    info: *const vk::DebugUtilsObjectNameInfoEXT,
) -> vk::Result {
    let Some(d) = device(handle) else {
        return vk::Result::ERROR_DEVICE_LOST;
    };
    let next: vk::PFN_vkSetDebugUtilsObjectNameEXT =
        std::mem::transmute((d.gdpa)(handle, c"vkSetDebugUtilsObjectNameEXT".as_ptr()).unwrap());
    let result = next(handle, info);
    if result == vk::Result::SUCCESS
        && !info.is_null()
        && matches!(
            (*info).object_type,
            vk::ObjectType::IMAGE | vk::ObjectType::BUFFER
        )
        && !(*info).p_object_name.is_null()
    {
        let name = CStr::from_ptr((*info).p_object_name)
            .to_string_lossy()
            .into_owned();
        if (*info).object_type == vk::ObjectType::BUFFER {
            crate::sdk_transfer_access::buffer_named(handle, (*info).object_handle, &name);
            if enabled() && name == "nv.ngx.dlssg.resource" {
                let details=BUFFERS.get_or_init(Default::default).lock().unwrap().0.get_mut(&(handle.as_raw(),(*info).object_handle)).map(|entry| {
                    entry.name=Some(name.clone());json!({"device":handle.as_raw(),"buffer":(*info).object_handle,"name":name,"creation":entry.creation,"bound":entry.bound,"previous_calls":entry.events})
                });
                trace::event!("sdk_buffer_transfer_named",details.unwrap_or_else(||json!({"device":handle.as_raw(),"buffer":(*info).object_handle,"name":name,"creation_missing":true})));
            }
            return result;
        }
        crate::sdk_output_layout::named(handle, (*info).object_handle, &name);
        crate::sdk_transfer_access::named(handle, (*info).object_handle, &name);
        if !enabled() {
            return result;
        }
        let details = {
            let mut images = IMAGES.get_or_init(Default::default).lock().unwrap();
            images.0.get_mut(&(handle.as_raw(),(*info).object_handle)).map(|entry| {
                entry.name=Some(name.clone());
                json!({"device":handle.as_raw(),"image":(*info).object_handle,"name":name,"creation":entry.creation,"bound":entry.bound,"previous_calls":entry.events,"dropped_calls":entry.dropped_events})
            })
        };
        if output_name(&name) {
            trace::event!("sdk_output_layout_named",details.unwrap_or_else(||json!({"device":handle.as_raw(),"image":(*info).object_handle,"name":name,"creation_missing":true})));
        }
    }
    result
}
unsafe extern "system" fn copy(
    cmd: vk::CommandBuffer,
    src: vk::Image,
    src_layout: vk::ImageLayout,
    dst: vk::Image,
    dst_layout: vk::ImageLayout,
    count: u32,
    regions: *const vk::ImageCopy,
) {
    let Some(d) = device(cmd) else {
        std::process::abort();
    };
    let next: vk::PFN_vkCmdCopyImage =
        std::mem::transmute((d.gdpa)(d.handle, c"vkCmdCopyImage".as_ptr()).unwrap());
    crate::sdk_output_layout::used(d.handle, src);
    crate::sdk_output_layout::used(d.handle, dst);
    record(
        d.handle,
        src,
        json!({"kind":"copy_source","command":cmd.as_raw(),"layout":src_layout.as_raw(),"other":dst.as_raw()}),
    );
    record(
        d.handle,
        dst,
        json!({"kind":"copy_destination","command":cmd.as_raw(),"layout":dst_layout.as_raw(),"other":src.as_raw()}),
    );
    next(cmd, src, src_layout, dst, dst_layout, count, regions);
}
pub(super) unsafe fn intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    if !enabled() && !crate::sdk_output_layout::enabled() && !crate::sdk_transfer_access::enabled()
    {
        return None;
    }
    match name.to_bytes() {
        b"vkBindImageMemory" => Some(std::mem::transmute(bind as *const ())),
        b"vkSetDebugUtilsObjectNameEXT" => Some(std::mem::transmute(self::name as *const ())),
        b"vkCmdCopyImage" => Some(std::mem::transmute(copy as *const ())),
        b"vkCreateBuffer" => Some(std::mem::transmute(create_buffer as *const ())),
        b"vkBindBufferMemory" => Some(std::mem::transmute(bind_buffer as *const ())),
        b"vkDestroyBuffer" => Some(std::mem::transmute(destroy_buffer as *const ())),
        b"vkCmdFillBuffer" if enabled() => Some(std::mem::transmute(fill_buffer as *const ())),
        _ => None,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evidence_requires_sdk_creation_and_is_bounded_per_lifetime() {
        let mut images = Images::default();
        assert!(images.record(1, 2, json!({})).is_none());
        images.0.insert(
            (1, 2),
            ImageTrace {
                name: Some("nv.sl.dlss_g.clone.dlfg-output_0".into()),
                ..Default::default()
            },
        );
        assert!(images.record(2, 2, json!({})).is_none());
        for _ in 0..32 {
            assert!(images.record(1, 2, json!({"kind":"barrier"})).is_some());
        }
        assert!(images.record(1, 2, json!({})).is_none());
        assert_eq!(images.0[&(1, 2)].dropped_events, 1);
        images.0.insert((1, 2), ImageTrace::default());
        assert!(images.record(1, 2, json!({})).is_none());
        assert_eq!(images.0[&(1, 2)].events.len(), 1);
    }
}
