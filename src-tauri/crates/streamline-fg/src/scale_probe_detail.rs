//! Extra diagnostic evidence; never a source of authorization to replace a draw.
use super::*;
use sha2::{Digest, Sha256};
type Key = (u64, u64);
#[derive(Default)]
pub(super) struct Memory {
    pub(super) allocations: HashMap<Key, u64>,
    pub(super) buffers: HashMap<Key, u64>,
    pub(super) bindings: HashMap<Key, (u64, u64)>,
    pub(super) maps: HashMap<Key, (usize, u64, u64)>,
}
static MEMORY: OnceLock<Mutex<Memory>> = OnceLock::new();
pub(super) fn memory() -> std::sync::MutexGuard<'static, Memory> {
    MEMORY
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
fn mapped_offset(
    binding: u64,
    offset: u64,
    size: u64,
    map_offset: u64,
    map_size: u64,
) -> Option<usize> {
    let start = binding.checked_add(offset)?.checked_sub(map_offset)?;
    (start.checked_add(size)? <= map_size).then_some(start as usize)
}
pub(super) unsafe fn buffer_info(device: u64, info: vk::DescriptorBufferInfo) -> Value {
    let mut result = json!({"buffer":info.buffer.as_raw(),"offset":info.offset,"range":info.range});
    // This version's blit coordinates occupy exactly 16 bytes. Do not dump game UBOs.
    if info.range != 16 {
        return result;
    }
    let m = memory();
    let key = (device, info.buffer.as_raw());
    let Some(size) = m.buffers.get(&key) else {
        return result;
    };
    if info.offset.checked_add(16).is_none_or(|end| end > *size) {
        return result;
    }
    let Some((allocation, binding)) = m.bindings.get(&key) else {
        return result;
    };
    let Some((ptr, offset, size)) = m.maps.get(&(device, *allocation)) else {
        return result;
    };
    if let Some(start) = mapped_offset(*binding, info.offset, 16, *offset, *size) {
        let bytes = std::slice::from_raw_parts((*ptr as *const u8).add(start), 16);
        result["host_snapshot"] = json!(bytes);
    }
    result
}
pub(super) unsafe fn coordinates(device: u64, info: vk::DescriptorBufferInfo) -> Option<[f32; 4]> {
    if info.range != 16 {
        return None;
    }
    let m = memory();
    let key = (device, info.buffer.as_raw());
    if info.offset.checked_add(16)? > *m.buffers.get(&key)? {
        return None;
    }
    let (allocation, binding) = m.bindings.get(&key)?;
    let (ptr, offset, size) = m.maps.get(&(device, *allocation))?;
    let start = mapped_offset(*binding, info.offset, 16, *offset, *size)?;
    let bytes = std::slice::from_raw_parts((*ptr as *const u8).add(start), 16);
    let uv =
        std::array::from_fn(|i| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()));
    Some(uv)
}
unsafe fn shader(
    i: *const vk::ShaderModuleCreateInfo,
    o: *mut vk::ShaderModule,
    r: vk::Result,
) -> Value {
    if r != vk::Result::SUCCESS {
        return json!({"result":r.as_raw()});
    }
    let bytes = std::slice::from_raw_parts((*i).p_code.cast::<u8>(), (*i).code_size);
    let hash = format!("{:x}", Sha256::digest(bytes));
    // Bound artifact size independently from the event limit.
    let saved =
        if bytes.len() <= 1024 * 1024 && trace::authorized() && !crate::source_auto::enabled() {
            std::env::var_os("NS_STREAMLINE_PROBE_TRACE")
                .and_then(|p| {
                    let parent = std::path::PathBuf::from(p);
                    let dir = parent.parent()?.join("shaders");
                    std::fs::create_dir_all(&dir).ok()?;
                    let path = dir.join(format!("{hash}.spv"));
                    if !path.exists() {
                        use std::io::Write;
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(path)
                            .ok()?
                            .write_all(bytes)
                            .ok()?;
                    }
                    Some(true)
                })
                .unwrap_or(false)
        } else {
            false
        };
    json!({"module":(*o).as_raw(),"sha256":hash,"bytes":bytes.len(),"saved":saved})
}
hook!(vkCreateShaderModule,PFN_vkCreateShaderModule,(h:vk::Device,i:*const vk::ShaderModuleCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::ShaderModule),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,shader(i,o,r));
hook!(vkCreateGraphicsPipelines,PFN_vkCreateGraphicsPipelines,(h:vk::Device,c:vk::PipelineCache,n:u32,i:*const vk::GraphicsPipelineCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::Pipeline),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,json!({"result":r.as_raw(),"pipelines":if r==vk::Result::SUCCESS {items(i,n).iter().zip(items(o,n)).map(|(i,p)|json!({"pipeline":p.as_raw(),"layout":i.layout.as_raw(),"renderpass":i.render_pass.as_raw(),"subpass":i.subpass,"flags":i.flags.as_raw(),"stages":items(i.p_stages,i.stage_count).iter().map(|s|json!({"stage":s.stage.as_raw(),"module":s.module.as_raw(),"entry":CStr::from_ptr(s.p_name).to_string_lossy()})).collect::<Vec<_>>()})).collect::<Vec<_>>()}else{vec![]}}));
hook!(vkCreateBuffer,PFN_vkCreateBuffer,(h:vk::Device,i:*const vk::BufferCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::Buffer),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,{if r==vk::Result::SUCCESS{memory().buffers.insert((h.as_raw(),(*o).as_raw()),(*i).size);}json!({"result":r.as_raw()})});
hook!(vkAllocateMemory,PFN_vkAllocateMemory,(h:vk::Device,i:*const vk::MemoryAllocateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::DeviceMemory),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,{if r==vk::Result::SUCCESS{memory().allocations.insert((h.as_raw(),(*o).as_raw()),(*i).allocation_size);}json!({"result":r.as_raw()})});
hook!(vkBindBufferMemory,PFN_vkBindBufferMemory,(h:vk::Device,b:vk::Buffer,m:vk::DeviceMemory,o:vk::DeviceSize),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,{if r==vk::Result::SUCCESS{memory().bindings.insert((h.as_raw(),b.as_raw()),(m.as_raw(),o));}json!({"result":r.as_raw()})});
hook!(vkMapMemory,PFN_vkMapMemory,(h:vk::Device,m:vk::DeviceMemory,o:vk::DeviceSize,s:vk::DeviceSize,f:vk::MemoryMapFlags,p:*mut *mut std::ffi::c_void),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,{if r==vk::Result::SUCCESS{let mut data=memory();let size=if s==vk::WHOLE_SIZE{data.allocations.get(&(h.as_raw(),m.as_raw())).and_then(|n|n.checked_sub(o)).unwrap_or(0)}else{s};data.maps.insert((h.as_raw(),m.as_raw()),(*p as usize,o,size));}json!({"result":r.as_raw()})});
hook!(vkUnmapMemory,PFN_vkUnmapMemory,(h:vk::Device,m:vk::DeviceMemory),(),(),r,{memory().maps.remove(&(h.as_raw(),m.as_raw()));json!({})});
hook!(vkFreeMemory,PFN_vkFreeMemory,(h:vk::Device,m:vk::DeviceMemory,a:*const vk::AllocationCallbacks),(),(),r,{let mut data=memory();data.maps.remove(&(h.as_raw(),m.as_raw()));data.allocations.remove(&(h.as_raw(),m.as_raw()));json!({})});
hook!(vkDestroyBuffer,PFN_vkDestroyBuffer,(h:vk::Device,b:vk::Buffer,a:*const vk::AllocationCallbacks),(),(),r,{let mut data=memory();data.buffers.remove(&(h.as_raw(),b.as_raw()));data.bindings.remove(&(h.as_raw(),b.as_raw()));json!({"buffer":b.as_raw()})});
hook!(vkDestroyImage,PFN_vkDestroyImage,(h:vk::Device,i:vk::Image,a:*const vk::AllocationCallbacks),(),(),r,json!({"image":i.as_raw()}));
hook!(vkDestroyImageView,PFN_vkDestroyImageView,(h:vk::Device,i:vk::ImageView,a:*const vk::AllocationCallbacks),(),(),r,json!({"view":i.as_raw()}));
hook!(vkDestroyFramebuffer,PFN_vkDestroyFramebuffer,(h:vk::Device,i:vk::Framebuffer,a:*const vk::AllocationCallbacks),(),(),r,json!({"framebuffer":i.as_raw()}));
hook!(vkResetCommandBuffer,PFN_vkResetCommandBuffer,(h:vk::CommandBuffer,f:vk::CommandBufferResetFlags),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,json!({"result":r.as_raw()}));
hook!(vkEndCommandBuffer,PFN_vkEndCommandBuffer,(h:vk::CommandBuffer),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,json!({"result":r.as_raw()}));
hook!(vkCmdPipelineBarrier,PFN_vkCmdPipelineBarrier,(h:vk::CommandBuffer,s:vk::PipelineStageFlags,d:vk::PipelineStageFlags,f:vk::DependencyFlags,mn:u32,m:*const vk::MemoryBarrier,bn:u32,b:*const vk::BufferMemoryBarrier,n:u32,i:*const vk::ImageMemoryBarrier),(),(),r,json!({"src_stage":s.as_raw(),"dst_stage":d.as_raw(),"memory":items(m,mn).iter().map(|m|json!({"src_access":m.src_access_mask.as_raw(),"dst_access":m.dst_access_mask.as_raw()})).collect::<Vec<_>>(),"images":items(i,n).iter().map(|i|json!({"image":i.image.as_raw(),"old":i.old_layout.as_raw(),"new":i.new_layout.as_raw(),"src_access":i.src_access_mask.as_raw(),"dst_access":i.dst_access_mask.as_raw(),"src_family":i.src_queue_family_index,"dst_family":i.dst_queue_family_index,"aspect":i.subresource_range.aspect_mask.as_raw(),"mip":i.subresource_range.base_mip_level,"mips":i.subresource_range.level_count,"layer":i.subresource_range.base_array_layer,"layers":i.subresource_range.layer_count})).collect::<Vec<_>>()}));
hook!(vkCreateRenderPass,PFN_vkCreateRenderPass,(h:vk::Device,i:*const vk::RenderPassCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::RenderPass),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,if r==vk::Result::SUCCESS{json!({"renderpass":(*o).as_raw(),"attachments":items((*i).p_attachments,(*i).attachment_count).iter().map(|a|json!({"initial":a.initial_layout.as_raw(),"final":a.final_layout.as_raw(),"load":a.load_op.as_raw(),"store":a.store_op.as_raw(),"format":a.format.as_raw()})).collect::<Vec<_>>(),"subpasses":items((*i).p_subpasses,(*i).subpass_count).iter().map(|s|json!({"colors":items(s.p_color_attachments,s.color_attachment_count).iter().map(|a|json!({"attachment":a.attachment,"layout":a.layout.as_raw()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})}else{json!({"result":r.as_raw()})});
hook!(vkCmdClearAttachments,PFN_vkCmdClearAttachments,(h:vk::CommandBuffer,n:u32,a:*const vk::ClearAttachment,rn:u32,rect:*const vk::ClearRect),(),(),r,json!({"attachments":items(a,n).iter().map(|a|json!({"aspect":a.aspect_mask.as_raw(),"index":a.color_attachment,"color":if a.aspect_mask.contains(vk::ImageAspectFlags::COLOR){Some(a.clear_value.color.float32)}else{None}})).collect::<Vec<_>>(),"rects":items(rect,rn).iter().map(|r|[r.rect.offset.x,r.rect.offset.y,r.rect.extent.width as i32,r.rect.extent.height as i32]).collect::<Vec<_>>()}));
pub(super) unsafe fn intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    macro_rules! pick { ($($f:ident),*) => { match name.to_bytes() { $(s if s == stringify!($f).as_bytes() => return Some(std::mem::transmute($f as *const ())),)* _ => {} } }; }
    pick!(
        vkCreateShaderModule,
        vkCreateGraphicsPipelines,
        vkCreateBuffer,
        vkAllocateMemory,
        vkBindBufferMemory,
        vkMapMemory,
        vkUnmapMemory,
        vkFreeMemory,
        vkDestroyBuffer,
        vkDestroyImage,
        vkDestroyImageView,
        vkDestroyFramebuffer,
        vkResetCommandBuffer,
        vkEndCommandBuffer,
        vkCmdPipelineBarrier,
        vkCreateRenderPass,
        vkCmdClearAttachments
    );
    None
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mapped_range_rejects_partial_mapping_and_overflow() {
        assert_eq!(mapped_offset(64, 32, 16, 80, 32), Some(16));
        assert_eq!(mapped_offset(64, 32, 16, 97, 32), None);
        assert_eq!(mapped_offset(64, 32, 16, 80, 31), None);
        assert_eq!(mapped_offset(u64::MAX, 32, 16, 0, 32), None);
    }
}
