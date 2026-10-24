//! Bounded, opt-in command evidence. Never changes Vulkan arguments or GPU work.
#![allow(non_snake_case)]
use super::*;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
static FRAMES: AtomicU64 = AtomicU64::new(0);
static EVENTS: AtomicU64 = AtomicU64::new(0);
const MAX_FRAMES: u64 = 120;
const MAX_EVENTS: u64 = 250_000;
fn frame_limit() -> u64 {
    static LIMIT: OnceLock<u64> = OnceLock::new();
    *LIMIT.get_or_init(|| {
        std::env::var("NS_STREAMLINE_SCALE_PROBE_FRAMES")
            .ok()
            .and_then(|s| s.parse().ok())
            .filter(|n| (1..=3600).contains(n))
            .unwrap_or(MAX_FRAMES)
    })
}
fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("NS_STREAMLINE_SCALE_PROBE").as_deref() == Ok("1")
            && std::env::var("NS_STREAMLINE_TARGET_SDK").as_deref() != Ok("1")
    })
}
fn active() -> bool {
    crate::source_auto::collecting()
        || enabled()
            && FRAMES.load(Ordering::Relaxed) < frame_limit()
            && EVENTS.load(Ordering::Relaxed) <= MAX_EVENTS
}
fn record(device: u64, object: u64, name: &str, data: Value) {
    let seq = EVENTS.fetch_add(1, Ordering::Relaxed);
    if seq < MAX_EVENTS {
        let row = json!({"seq":seq,"frame":FRAMES.load(Ordering::Relaxed),"device":device,"object":object,"call":name,"data":data});
        crate::scale_copy::observe(&row);
        trace::record("scale_probe", row);
    } else if seq == MAX_EVENTS {
        trace::record(
            "scale_probe_complete",
            json!({"reason":"event_limit","truncated":true}),
        );
    }
}
pub(crate) unsafe fn items<'a, T>(p: *const T, n: u32) -> &'a [T] {
    if n == 0 || p.is_null() {
        &[]
    } else {
        std::slice::from_raw_parts(p, n as usize)
    }
}
unsafe fn handles<T: Handle + Copy>(p: *const T, n: u32) -> Vec<u64> {
    items(p, n).iter().map(|x| x.as_raw()).collect()
}
pub(super) unsafe fn present(
    queue: vk::Queue,
    info: *const vk::PresentInfoKHR,
    result: vk::Result,
) {
    if !active() || info.is_null() || crate::source_auto::enabled() {
        return;
    }
    let i = &*info;
    record(
        device(queue).map_or(0, |d| d.handle.as_raw()),
        queue.as_raw(),
        "present",
        json!({"chains":handles(i.p_swapchains,i.swapchain_count),"indices":items(i.p_image_indices,i.swapchain_count),"waits":handles(i.p_wait_semaphores,i.wait_semaphore_count),"result":result.as_raw()}),
    );
    if FRAMES.fetch_add(1, Ordering::Relaxed) + 1 == frame_limit() {
        trace::record(
            "scale_probe_complete",
            json!({"reason":"frame_limit","frames":frame_limit(),"truncated":false}),
        );
    }
}
include!("source_hooks.rs");
include!("source_descriptors.rs");
macro_rules! hook {
    ($name:ident,$pfn:ident,($h:ident:$ty:ty $(,$a:ident:$at:ty)*),$ret:ty,$fail:expr,$r:ident,$data:expr)=>{
        unsafe extern "system" fn $name($h:$ty,$($a:$at),*)->$ret{
            let Some(d)=device($h) else{return $fail};
            let Some(f)=(d.gdpa)(d.handle,concat!(stringify!($name),"\0").as_ptr().cast()) else{return $fail};
            #[cfg(all(windows, feature="sdk-bridge"))]
            let f = if target_runtime::enabled() {
                target_runtime::ensure_device(d.handle);
                target_runtime::device_proc(d.handle, CStr::from_bytes_with_nul_unchecked(concat!(stringify!($name),"\0").as_bytes())).unwrap_or(f)
            } else { f };
            let f = crate::present_layout::barrier_intercept(CStr::from_bytes_with_nul_unchecked(concat!(stringify!($name),"\0").as_bytes())).unwrap_or(f);
            let f = if stringify!($name) != "vkDeviceWaitIdle" {
                crate::queue_sync::intercept(CStr::from_bytes_with_nul_unchecked(concat!(stringify!($name),"\0").as_bytes())).unwrap_or(f)
            } else { f };
            let f:vk::$pfn=std::mem::transmute(f);
            #[cfg(all(windows, feature="native-nr"))]
            let $r=crate::validation_context::application(stringify!($name), || f($h,$($a),*));
            #[cfg(not(all(windows, feature="native-nr")))]
            let $r=f($h,$($a),*);
            proxy_record!($name,d.handle.as_raw(),$h,$r $(,$a)*);
            if active() && crate::source_auto::wants_call(stringify!($name)) {
                let started = crate::source_auto::measuring().then(std::time::Instant::now);
                if crate::source_auto::enabled() {
                    online_record!($name,d.handle.as_raw(),$h,$r $(,$a)*);
                } else {
                    record(d.handle.as_raw(),$h.as_raw(),stringify!($name),$data);
                }
                crate::source_auto::measured(stringify!($name), started);
            }
            $r
        }
    }
}
hook!(vkCmdExecuteCommands,PFN_vkCmdExecuteCommands,(h:vk::CommandBuffer,n:u32,c:*const vk::CommandBuffer),(),(),r,json!({}));
hook!(vkCmdPipelineBarrier2,PFN_vkCmdPipelineBarrier2,(h:vk::CommandBuffer,i:*const vk::DependencyInfo),(),(),r,json!({}));
hook!(vkCmdPipelineBarrier2KHR,PFN_vkCmdPipelineBarrier2,(h:vk::CommandBuffer,i:*const vk::DependencyInfo),(),(),r,json!({}));
hook!(vkQueueSubmit2,PFN_vkQueueSubmit2,(h:vk::Queue,n:u32,i:*const vk::SubmitInfo2,f:vk::Fence),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,json!({}));
hook!(vkQueueSubmit2KHR,PFN_vkQueueSubmit2,(h:vk::Queue,n:u32,i:*const vk::SubmitInfo2,f:vk::Fence),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,json!({}));
hook!(vkCmdBeginRendering,PFN_vkCmdBeginRendering,(h:vk::CommandBuffer,i:*const vk::RenderingInfo),(),(),r,json!({}));
hook!(vkCmdBeginRenderingKHR,PFN_vkCmdBeginRendering,(h:vk::CommandBuffer,i:*const vk::RenderingInfo),(),(),r,json!({}));
hook!(vkCreateSampler,PFN_vkCreateSampler,(h:vk::Device,i:*const vk::SamplerCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::Sampler),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,if r==vk::Result::SUCCESS {json!({"sampler":(*o).as_raw(),"mag":(*i).mag_filter.as_raw(),"min":(*i).min_filter.as_raw(),"normalized":(*i).unnormalized_coordinates==0})}else{json!({"result":r.as_raw()})});
hook!(vkDestroySampler,PFN_vkDestroySampler,(h:vk::Device,s:vk::Sampler,a:*const vk::AllocationCallbacks),(),(),r,json!({"sampler":s.as_raw()}));
hook!(vkGetDeviceQueue,PFN_vkGetDeviceQueue,(h:vk::Device,f:u32,index:u32,o:*mut vk::Queue),(),(),r,json!({"queue":(*o).as_raw(),"family":f,"index":index}));
hook!(vkGetDeviceQueue2,PFN_vkGetDeviceQueue2,(h:vk::Device,i:*const vk::DeviceQueueInfo2,o:*mut vk::Queue),(),(),r,json!({"queue":(*o).as_raw(),"family":(*i).queue_family_index,"index":(*i).queue_index}));
hook!(vkCreateImage,PFN_vkCreateImage,(h:vk::Device,i:*const vk::ImageCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::Image),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,if r==vk::Result::SUCCESS {json!({"image":(*o).as_raw(),"extent":[(*i).extent.width,(*i).extent.height,(*i).extent.depth],"format":(*i).format.as_raw(),"usage":(*i).usage.as_raw(),"mips":(*i).mip_levels,"layers":(*i).array_layers,"samples":(*i).samples.as_raw()})}else{json!({"result":r.as_raw()})});
hook!(vkCreateImageView,PFN_vkCreateImageView,(h:vk::Device,i:*const vk::ImageViewCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::ImageView),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,if r==vk::Result::SUCCESS {json!({"view":(*o).as_raw(),"image":(*i).image.as_raw(),"format":(*i).format.as_raw(),"base_mip":(*i).subresource_range.base_mip_level,"base_layer":(*i).subresource_range.base_array_layer})}else{json!({"result":r.as_raw()})});
hook!(vkCreateFramebuffer,PFN_vkCreateFramebuffer,(h:vk::Device,i:*const vk::FramebufferCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::Framebuffer),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,if r==vk::Result::SUCCESS {json!({"framebuffer":(*o).as_raw(),"views":if (*i).flags.contains(vk::FramebufferCreateFlags::IMAGELESS){vec![]}else{handles((*i).p_attachments,(*i).attachment_count)},"extent":[(*i).width,(*i).height],"flags":(*i).flags.as_raw()})}else{json!({"result":r.as_raw()})});
hook!(vkGetSwapchainImagesKHR,PFN_vkGetSwapchainImagesKHR,(h:vk::Device,s:vk::SwapchainKHR,n:*mut u32,o:*mut vk::Image),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,if r==vk::Result::SUCCESS || r==vk::Result::INCOMPLETE {json!({"chain":s.as_raw(),"images":handles(o,*n),"result":r.as_raw()})}else{json!({"result":r.as_raw()})});
hook!(vkBeginCommandBuffer,PFN_vkBeginCommandBuffer,(h:vk::CommandBuffer,i:*const vk::CommandBufferBeginInfo),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,json!({"result":r.as_raw(),"flags":(*i).flags.as_raw()}));
hook!(vkQueueSubmit,PFN_vkQueueSubmit,(h:vk::Queue,n:u32,i:*const vk::SubmitInfo,f:vk::Fence),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,json!({"result":r.as_raw(),"submits":items(i,n).iter().map(|s|json!({"commands":handles(s.p_command_buffers,s.command_buffer_count),"waits":handles(s.p_wait_semaphores,s.wait_semaphore_count),"signals":handles(s.p_signal_semaphores,s.signal_semaphore_count)})).collect::<Vec<_>>()}));
hook!(vkUpdateDescriptorSets,PFN_vkUpdateDescriptorSets,(h:vk::Device,n:u32,w:*const vk::WriteDescriptorSet,c:u32,copies:*const vk::CopyDescriptorSet),(),(),r,json!({"writes":items(w,n).iter().map(|w|json!({"set":w.dst_set.as_raw(),"binding":w.dst_binding,"element":w.dst_array_element,"count":w.descriptor_count,"type":w.descriptor_type.as_raw(),"images":if [vk::DescriptorType::COMBINED_IMAGE_SAMPLER,vk::DescriptorType::SAMPLED_IMAGE,vk::DescriptorType::STORAGE_IMAGE,vk::DescriptorType::INPUT_ATTACHMENT].contains(&w.descriptor_type){items(w.p_image_info,w.descriptor_count).iter().map(|i|json!({"view":i.image_view.as_raw(),"layout":i.image_layout.as_raw()})).collect::<Vec<_>>()}else{vec![]}})).collect::<Vec<_>>(),"copies":items(copies,c).iter().map(|c|json!({"src":c.src_set.as_raw(),"dst":c.dst_set.as_raw(),"count":c.descriptor_count})).collect::<Vec<_>>()}));
hook!(vkCmdBeginRenderPass,PFN_vkCmdBeginRenderPass,(h:vk::CommandBuffer,i:*const vk::RenderPassBeginInfo,c:vk::SubpassContents),(),(),r,json!({"framebuffer":(*i).framebuffer.as_raw(),"renderpass":(*i).render_pass.as_raw(),"area":[(*i).render_area.offset.x,(*i).render_area.offset.y,(*i).render_area.extent.width as i32,(*i).render_area.extent.height as i32]}));
hook!(vkCmdEndRenderPass,PFN_vkCmdEndRenderPass,(h:vk::CommandBuffer),(),(),r,json!({}));
hook!(vkCmdBindPipeline,PFN_vkCmdBindPipeline,(h:vk::CommandBuffer,b:vk::PipelineBindPoint,p:vk::Pipeline),(),(),r,json!({"bind_point":b.as_raw(),"pipeline":p.as_raw()}));
hook!(vkCmdBindDescriptorSets,PFN_vkCmdBindDescriptorSets,(h:vk::CommandBuffer,b:vk::PipelineBindPoint,l:vk::PipelineLayout,first:u32,n:u32,s:*const vk::DescriptorSet,dn:u32,offsets:*const u32),(),(),r,json!({"bind_point":b.as_raw(),"layout":l.as_raw(),"first":first,"sets":handles(s,n),"dynamic_offsets":items(offsets,dn)}));
hook!(vkCmdSetViewport,PFN_vkCmdSetViewport,(h:vk::CommandBuffer,first:u32,n:u32,v:*const vk::Viewport),(),(),r,json!({"first":first,"viewports":items(v,n).iter().map(|v|[v.x,v.y,v.width,v.height,v.min_depth,v.max_depth]).collect::<Vec<_>>()}));
hook!(vkCmdSetScissor,PFN_vkCmdSetScissor,(h:vk::CommandBuffer,first:u32,n:u32,v:*const vk::Rect2D),(),(),r,json!({"first":first,"scissors":items(v,n).iter().map(|v|[v.offset.x,v.offset.y,v.extent.width as i32,v.extent.height as i32]).collect::<Vec<_>>()}));
hook!(vkCmdDraw,PFN_vkCmdDraw,(h:vk::CommandBuffer,v:u32,i:u32,fv:u32,fi:u32),(),(),r,json!({"vertices":v,"instances":i,"first_vertex":fv,"first_instance":fi}));
hook!(vkCmdDrawIndexed,PFN_vkCmdDrawIndexed,(h:vk::CommandBuffer,v:u32,i:u32,fv:u32,vo:i32,fi:u32),(),(),r,json!({"indices":v,"instances":i,"first_index":fv,"vertex_offset":vo,"first_instance":fi}));
hook!(vkCmdBlitImage,PFN_vkCmdBlitImage,(h:vk::CommandBuffer,s:vk::Image,sl:vk::ImageLayout,d:vk::Image,dl:vk::ImageLayout,n:u32,regions:*const vk::ImageBlit,f:vk::Filter),(),(),r,json!({"src":s.as_raw(),"dst":d.as_raw(),"src_layout":sl.as_raw(),"dst_layout":dl.as_raw(),"filter":f.as_raw(),"regions":items(regions,n).iter().map(|r|json!({"src":r.src_offsets.map(|o|[o.x,o.y,o.z]),"dst":r.dst_offsets.map(|o|[o.x,o.y,o.z]),"src_mip":r.src_subresource.mip_level,"dst_mip":r.dst_subresource.mip_level})).collect::<Vec<_>>()}));
hook!(vkCmdCopyImage,PFN_vkCmdCopyImage,(h:vk::CommandBuffer,s:vk::Image,sl:vk::ImageLayout,d:vk::Image,dl:vk::ImageLayout,n:u32,regions:*const vk::ImageCopy),(),(),r,json!({"src":s.as_raw(),"dst":d.as_raw(),"regions":items(regions,n).iter().map(|r|json!({"extent":[r.extent.width,r.extent.height,r.extent.depth],"src":[r.src_offset.x,r.src_offset.y],"dst":[r.dst_offset.x,r.dst_offset.y]})).collect::<Vec<_>>()}));
hook!(vkDestroyPipeline,PFN_vkDestroyPipeline,(h:vk::Device,p:vk::Pipeline,a:*const vk::AllocationCallbacks),(),(),r,json!({"pipeline":p.as_raw()}));
hook!(vkDestroyRenderPass,PFN_vkDestroyRenderPass,(h:vk::Device,p:vk::RenderPass,a:*const vk::AllocationCallbacks),(),(),r,json!({"renderpass":p.as_raw()}));
hook!(vkCmdPushConstants,PFN_vkCmdPushConstants,(h:vk::CommandBuffer,l:vk::PipelineLayout,s:vk::ShaderStageFlags,o:u32,n:u32,v:*const std::ffi::c_void),(),(),r,json!({"layout":l.as_raw(),"stages":s.as_raw(),"offset":o,"bytes":items(v.cast::<u8>(),n)}));

static TEMPLATES: OnceLock<Mutex<HashMap<(u64, u64), Vec<vk::DescriptorUpdateTemplateEntry>>>> =
    OnceLock::new();
fn templates(
) -> std::sync::MutexGuard<'static, HashMap<(u64, u64), Vec<vk::DescriptorUpdateTemplateEntry>>> {
    TEMPLATES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}
unsafe fn template_created(
    device: u64,
    i: *const vk::DescriptorUpdateTemplateCreateInfo,
    o: *mut vk::DescriptorUpdateTemplate,
    r: vk::Result,
) -> Value {
    if r != vk::Result::SUCCESS {
        return json!({"result":r.as_raw()});
    }
    templates().insert(
        (device, (*o).as_raw()),
        items(
            (*i).p_descriptor_update_entries,
            (*i).descriptor_update_entry_count,
        )
        .iter()
        .filter(|e| !crate::source_auto::enabled() || online_entry(e))
        .map(|e| {
            let mut e = *e;
            if crate::source_auto::enabled() {
                e.descriptor_count = 1;
            }
            e
        })
        .collect(),
    );
    json!({"template":(*o).as_raw(),"type":(*i).template_type.as_raw()})
}
fn online_entry(e: &vk::DescriptorUpdateTemplateEntry) -> bool {
    e.dst_array_element == 0
        && e.descriptor_count > 0
        && ((e.dst_binding == 0
            && [
                vk::DescriptorType::SAMPLER,
                vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                vk::DescriptorType::SAMPLED_IMAGE,
            ]
            .contains(&e.descriptor_type))
            || (e.dst_binding == 1 && e.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER))
}
unsafe fn template_write(
    device: u64,
    set: vk::DescriptorSet,
    t: vk::DescriptorUpdateTemplate,
    p: *const std::ffi::c_void,
) -> Value {
    let entries = templates().get(&(device, t.as_raw())).cloned();
    let Some(entries) = entries else {
        return json!({"unknown_template":t.as_raw(),"writes":[]});
    };
    // The verified blit shaders read only texture/sampler binding 0, element 0
    // and the 16-byte coordinate UBO at binding 1. Do not serialize game arrays.
    let online = crate::source_auto::enabled();
    let writes:Vec<_>=entries.iter().filter(|e| !online || (
        e.dst_array_element == 0 && e.descriptor_count > 0 &&
        ((e.dst_binding == 0 && [vk::DescriptorType::SAMPLER,vk::DescriptorType::COMBINED_IMAGE_SAMPLER,vk::DescriptorType::SAMPLED_IMAGE].contains(&e.descriptor_type)) ||
        (e.dst_binding == 1 && e.descriptor_type == vk::DescriptorType::UNIFORM_BUFFER))
    )).map(|e|{
        let count = if online { 1 } else { e.descriptor_count as usize };
        let mut images=vec![];
        if [vk::DescriptorType::COMBINED_IMAGE_SAMPLER,vk::DescriptorType::SAMPLED_IMAGE,vk::DescriptorType::STORAGE_IMAGE,vk::DescriptorType::INPUT_ATTACHMENT].contains(&e.descriptor_type) {
            for n in 0..count {
                let q=p.cast::<u8>().add(e.offset+n*e.stride).cast::<vk::DescriptorImageInfo>();
                // Sampler is ignored for sampled/storage images; read only the valid fields.
                let view=std::ptr::addr_of!((*q).image_view).read_unaligned();
                let layout=std::ptr::addr_of!((*q).image_layout).read_unaligned();
                images.push(json!({"view":view.as_raw(),"layout":layout.as_raw()}));
            }
        }
        let mut samplers=vec![];
        if [vk::DescriptorType::SAMPLER,vk::DescriptorType::COMBINED_IMAGE_SAMPLER].contains(&e.descriptor_type) {
            for n in 0..count {
                let q=p.cast::<u8>().add(e.offset+n*e.stride).cast::<vk::DescriptorImageInfo>();
                samplers.push(std::ptr::addr_of!((*q).sampler).read_unaligned().as_raw());
            }
        }
        let mut buffers=vec![];
        if [vk::DescriptorType::UNIFORM_BUFFER,vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC].contains(&e.descriptor_type) {
            for n in 0..count {
                let info=p.cast::<u8>().add(e.offset+n*e.stride).cast::<vk::DescriptorBufferInfo>().read_unaligned();
                buffers.push(detail::buffer_info(device,info));
            }
        }
        json!({"set":set.as_raw(),"binding":e.dst_binding,"element":e.dst_array_element,"count":e.descriptor_count,"type":e.descriptor_type.as_raw(),"images":images,"buffers":buffers,"samplers":samplers})
    }).collect();
    json!({"template":t.as_raw(),"writes":writes})
}

hook!(vkCreateDescriptorUpdateTemplate,PFN_vkCreateDescriptorUpdateTemplate,(h:vk::Device,i:*const vk::DescriptorUpdateTemplateCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::DescriptorUpdateTemplate),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,template_created(h.as_raw(),i,o,r));
hook!(vkUpdateDescriptorSetWithTemplate,PFN_vkUpdateDescriptorSetWithTemplate,(h:vk::Device,s:vk::DescriptorSet,t:vk::DescriptorUpdateTemplate,p:*const std::ffi::c_void),(),(),r,template_write(h.as_raw(),s,t,p));
hook!(vkDestroyDescriptorUpdateTemplate,PFN_vkDestroyDescriptorUpdateTemplate,(h:vk::Device,t:vk::DescriptorUpdateTemplate,a:*const vk::AllocationCallbacks),(),(),r,{templates().remove(&(h.as_raw(),t.as_raw()));json!({"template":t.as_raw()})});
hook!(vkCreateDescriptorUpdateTemplateKHR,PFN_vkCreateDescriptorUpdateTemplate,(h:vk::Device,i:*const vk::DescriptorUpdateTemplateCreateInfo,a:*const vk::AllocationCallbacks,o:*mut vk::DescriptorUpdateTemplate),vk::Result,vk::Result::ERROR_DEVICE_LOST,r,template_created(h.as_raw(),i,o,r));
hook!(vkUpdateDescriptorSetWithTemplateKHR,PFN_vkUpdateDescriptorSetWithTemplate,(h:vk::Device,s:vk::DescriptorSet,t:vk::DescriptorUpdateTemplate,p:*const std::ffi::c_void),(),(),r,template_write(h.as_raw(),s,t,p));
hook!(vkDestroyDescriptorUpdateTemplateKHR,PFN_vkDestroyDescriptorUpdateTemplate,(h:vk::Device,t:vk::DescriptorUpdateTemplate,a:*const vk::AllocationCallbacks),(),(),r,{templates().remove(&(h.as_raw(),t.as_raw()));json!({"template":t.as_raw()})});
pub(super) unsafe fn intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    if !enabled() && !crate::source_auto::enabled() {
        return None;
    }
    macro_rules! pick { ($($f:ident),*) => { match name.to_bytes() { $(s if s == stringify!($f).as_bytes() => return Some(std::mem::transmute($f as *const ())),)* _ => {} } }; }
    pick!(
        vkCmdExecuteCommands,
        vkCmdPipelineBarrier2,
        vkCmdPipelineBarrier2KHR,
        vkQueueSubmit2,
        vkQueueSubmit2KHR,
        vkCmdBeginRendering,
        vkCmdBeginRenderingKHR,
        vkCreateSampler,
        vkDestroyPipeline,
        vkDestroyRenderPass,
        vkDestroySampler,
        vkGetDeviceQueue,
        vkGetDeviceQueue2,
        vkCreateDescriptorUpdateTemplate,
        vkUpdateDescriptorSetWithTemplate,
        vkDestroyDescriptorUpdateTemplate,
        vkCreateDescriptorUpdateTemplateKHR,
        vkUpdateDescriptorSetWithTemplateKHR,
        vkDestroyDescriptorUpdateTemplateKHR,
        vkCreateImage,
        vkCreateImageView,
        vkCreateFramebuffer,
        vkGetSwapchainImagesKHR,
        vkBeginCommandBuffer,
        vkQueueSubmit,
        vkUpdateDescriptorSets,
        vkCmdBeginRenderPass,
        vkCmdEndRenderPass,
        vkCmdBindPipeline,
        vkCmdBindDescriptorSets,
        vkCmdSetViewport,
        vkCmdSetScissor,
        vkCmdDraw,
        vkCmdDrawIndexed,
        vkCmdBlitImage,
        vkCmdCopyImage,
        vkCmdPushConstants
    );
    detail::intercept(name)
}
#[path = "scale_probe_detail.rs"]
mod detail;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn online_template_excludes_game_bindings_and_array_tail() {
        let mut e = vk::DescriptorUpdateTemplateEntry::default()
            .descriptor_type(vk::DescriptorType::SAMPLED_IMAGE)
            .descriptor_count(4096);
        assert!(online_entry(&e));
        e.dst_array_element = 1;
        assert!(!online_entry(&e));
        e.dst_array_element = 0;
        e.dst_binding = 4;
        assert!(!online_entry(&e));
        e.dst_binding = 1;
        e.descriptor_type = vk::DescriptorType::UNIFORM_BUFFER;
        assert!(online_entry(&e));
        e.descriptor_type = vk::DescriptorType::UNIFORM_BUFFER_DYNAMIC;
        assert!(!online_entry(&e));
    }
    #[test]
    fn template_image_array_respects_unaligned_offset_and_stride() {
        let device = 999;
        let template = vk::DescriptorUpdateTemplate::from_raw(42);
        templates().insert(
            (device, 42),
            vec![vk::DescriptorUpdateTemplateEntry {
                dst_binding: 3,
                dst_array_element: 2,
                descriptor_count: 2,
                descriptor_type: vk::DescriptorType::SAMPLED_IMAGE,
                offset: 1,
                stride: 40,
            }],
        );
        let mut bytes = [0u8; 100];
        unsafe {
            for n in 0..2 {
                bytes
                    .as_mut_ptr()
                    .add(1 + n * 40)
                    .cast::<vk::DescriptorImageInfo>()
                    .write_unaligned(
                        vk::DescriptorImageInfo::default()
                            .image_view(vk::ImageView::from_raw(100 + n as u64))
                            .image_layout(vk::ImageLayout::GENERAL),
                    );
            }
            let value = template_write(
                device,
                vk::DescriptorSet::from_raw(7),
                template,
                bytes.as_ptr().cast(),
            );
            assert_eq!(value["writes"][0]["images"][0]["view"], 100);
            assert_eq!(value["writes"][0]["images"][1]["view"], 101);
            assert_eq!(value["writes"][0]["element"], 2);
        }
        templates().remove(&(device, 42));
    }
}
