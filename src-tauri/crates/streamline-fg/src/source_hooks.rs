// Each online hook consumes native Vulkan values. The diagnostic JSON expression
// in hook! is evaluated only when the offline recorder is selected.
macro_rules! online_record {
    (@model $body:expr) => { crate::source_auto::update($body) };
    (vkCmdBindPipeline,$d:expr,$h:ident,$r:ident,$b:ident,$p:ident) => {
        if $b==vk::PipelineBindPoint::GRAPHICS {online_record!(@model |m|m.pipeline($d,$h.as_raw(),$p.as_raw()));}
    };
    (vkCmdBindDescriptorSets,$d:expr,$h:ident,$r:ident,$b:ident,$l:ident,$first:ident,$n:ident,$s:ident,$dn:ident,$offsets:ident) => {
        if $b==vk::PipelineBindPoint::GRAPHICS {online_record!(@model |m|m.sets($d,$h.as_raw(),$first,items($s,$n)));}
    };
    (vkCmdSetViewport,$d:expr,$h:ident,$r:ident,$first:ident,$n:ident,$v:ident) => {online_record!(@model |m|m.viewport($d,$h.as_raw(),$first,items($v,$n)))};
    (vkCmdSetScissor,$d:expr,$h:ident,$r:ident,$first:ident,$n:ident,$v:ident) => {online_record!(@model |m|m.scissor($d,$h.as_raw(),$first,items($v,$n)))};
    (vkCmdDraw,$d:expr,$h:ident,$r:ident,$v:ident,$i:ident,$fv:ident,$fi:ident) => {online_record!(@model |m|m.draw($d,$h.as_raw(),false,[$v,$i,$fv,$fi]))};
    (vkCmdDrawIndexed,$d:expr,$h:ident,$r:ident,$v:ident,$i:ident,$fv:ident,$vo:ident,$fi:ident) => {online_record!(@model |m|m.draw($d,$h.as_raw(),true,[$v,$i,$fv,$fi]))};
    (vkCmdPipelineBarrier,$dev:expr,$h:ident,$r:ident,$s:ident,$d:ident,$f:ident,$mn:ident,$mem:ident,$bn:ident,$b:ident,$n:ident,$i:ident) => {online_record!(@model |m|m.barriers($dev,$h.as_raw(),items($i,$n)))};
    (vkCmdBeginRenderPass,$d:expr,$h:ident,$r:ident,$i:ident,$c:ident) => {online_record!(@model |m|m.begin_pass($d,$h.as_raw(),(*$i).framebuffer.as_raw(),(*$i).render_pass.as_raw()))};
    (vkCmdEndRenderPass,$d:expr,$h:ident,$r:ident) => {online_record!(@model |m|m.end_pass($d,$h.as_raw()))};
    (vkCmdClearAttachments,$d:expr,$h:ident,$r:ident,$n:ident,$a:ident,$rn:ident,$rect:ident) => {online_record!(@model |m|m.clear($d,$h.as_raw(),items($a,$n),items($rect,$rn)))};
    (vkBeginCommandBuffer,$d:expr,$h:ident,$r:ident,$i:ident) => {online_record!(@model |m|m.begin($d,$h.as_raw(),$r==vk::Result::SUCCESS))};
    (vkResetCommandBuffer,$d:expr,$h:ident,$r:ident,$f:ident) => {online_record!(@model |m|m.begin($d,$h.as_raw(),$r==vk::Result::SUCCESS))};
    (vkEndCommandBuffer,$d:expr,$h:ident,$r:ident) => {online_record!(@model |m|m.end($d,$h.as_raw(),$r==vk::Result::SUCCESS))};
    (vkQueueSubmit,$d:expr,$h:ident,$r:ident,$n:ident,$i:ident,$f:ident) => {online_record!(@model |m|m.submit($d,$h.as_raw(),items($i,$n),$r==vk::Result::SUCCESS))};
    (vkCreateSampler,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {
        if $r==vk::Result::SUCCESS {online_record!(@model |m|{m.samplers.insert(($d,(*$o).as_raw()),(*$i).min_filter==vk::Filter::LINEAR&&(*$i).mag_filter==vk::Filter::LINEAR&&(*$i).unnormalized_coordinates==0);});}
    };
    (vkDestroySampler,$d:expr,$h:ident,$r:ident,$s:ident,$a:ident) => {online_record!(@model |m|{m.samplers.remove(&($d,$s.as_raw()));})};
    (vkCreateImage,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {
        if $r==vk::Result::SUCCESS {online_record!(@model |m|{let i=&*$i;m.images.insert(($d,(*$o).as_raw()),crate::source_model::Image{
            extent:[i.extent.width,i.extent.height,i.extent.depth],format:i.format.as_raw(),usage:i.usage.as_raw(),mips:i.mip_levels,layers:i.array_layers,samples:i.samples.as_raw(),generation:m.serial});});}
    };
    (vkCreateImageView,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {
        if $r==vk::Result::SUCCESS {online_record!(@model |m|{let i=&*$i;m.views.insert(($d,(*$o).as_raw()),crate::source_model::View{
            image:i.image.as_raw(),format:i.format.as_raw(),mip:i.subresource_range.base_mip_level,layer:i.subresource_range.base_array_layer});});}
    };
    (vkCreateFramebuffer,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {
        if $r==vk::Result::SUCCESS {online_record!(@model |m|{let i=&*$i;m.framebuffers.insert(($d,(*$o).as_raw()),crate::source_model::Framebuffer{
            views:if i.flags.contains(vk::FramebufferCreateFlags::IMAGELESS){vec![]}else{handles(i.p_attachments,i.attachment_count)},size:[i.width,i.height]});});}
    };
    (vkDestroyImage,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident) => {online_record!(@model |m|{m.images.remove(&($d,$i.as_raw()));})};
    (vkDestroyImageView,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident) => {online_record!(@model |m|{m.views.remove(&($d,$i.as_raw()));})};
    (vkDestroyFramebuffer,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident) => {online_record!(@model |m|{m.framebuffers.remove(&($d,$i.as_raw()));})};
    (vkGetSwapchainImagesKHR,$d:expr,$h:ident,$r:ident,$s:ident,$n:ident,$o:ident) => {
        if ($r==vk::Result::SUCCESS||$r==vk::Result::INCOMPLETE)&&!$o.is_null(){online_record!(@model |m|{m.chains.insert(($d,$s.as_raw()),handles($o,*$n));});}
    };
    (vkCreateShaderModule,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {
        if $r==vk::Result::SUCCESS {let hash=crate::scale_probe::online_shader(&*$i);online_record!(@model |m|{m.shaders.insert(($d,(*$o).as_raw()),hash);});}
    };
    (vkCreateGraphicsPipelines,$d:expr,$h:ident,$r:ident,$c:ident,$n:ident,$i:ident,$a:ident,$o:ident) => {
        if $r==vk::Result::SUCCESS {online_record!(@model |m|{for (info,p) in items($i,$n).iter().zip(items($o,$n)){
            let stages=items(info.p_stages,info.stage_count);
            let known=stages.len()==2&&[(vk::ShaderStageFlags::VERTEX,1),(vk::ShaderStageFlags::FRAGMENT,2)].iter().all(|(stage,hash)|
                stages.iter().any(|s|s.stage==*stage&&m.shaders.get(&($d,s.module.as_raw()))==Some(hash)&&!s.p_name.is_null()&&CStr::from_ptr(s.p_name)==c"main"));
            m.pipelines.insert(($d,p.as_raw()),known);
        }});}
    };
    (vkCreateRenderPass,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {
        if $r==vk::Result::SUCCESS {online_record!(@model |m|{let a=items((*$i).p_attachments,(*$i).attachment_count);
            m.renderpasses.insert(($d,(*$o).as_raw()),a.len()==1&&a[0].initial_layout==vk::ImageLayout::GENERAL&&a[0].final_layout==vk::ImageLayout::GENERAL);});}
    };
    (vkUpdateDescriptorSets,$d:expr,$h:ident,$r:ident,$n:ident,$w:ident,$c:ident,$copies:ident) => {
        crate::scale_probe::online_writes($d,items($w,$n),$c);
    };
    (vkCreateDescriptorUpdateTemplate,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {crate::scale_probe::online_template_created($d,$i,$o,$r)};
    (vkCreateDescriptorUpdateTemplateKHR,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {crate::scale_probe::online_template_created($d,$i,$o,$r)};
    (vkUpdateDescriptorSetWithTemplate,$d:expr,$h:ident,$r:ident,$s:ident,$t:ident,$p:ident) => {crate::scale_probe::online_template_write($d,$s,$t,$p)};
    (vkUpdateDescriptorSetWithTemplateKHR,$d:expr,$h:ident,$r:ident,$s:ident,$t:ident,$p:ident) => {crate::scale_probe::online_template_write($d,$s,$t,$p)};
    (vkDestroyDescriptorUpdateTemplate,$d:expr,$h:ident,$r:ident,$t:ident,$a:ident) => {crate::scale_probe::templates().remove(&($d,$t.as_raw()));};
    (vkDestroyDescriptorUpdateTemplateKHR,$d:expr,$h:ident,$r:ident,$t:ident,$a:ident) => {crate::scale_probe::templates().remove(&($d,$t.as_raw()));};
    (vkCreateBuffer,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {if $r==vk::Result::SUCCESS{crate::scale_probe::detail::memory().buffers.insert(($d,(*$o).as_raw()),(*$i).size);}};
    (vkAllocateMemory,$d:expr,$h:ident,$r:ident,$i:ident,$a:ident,$o:ident) => {if $r==vk::Result::SUCCESS{crate::scale_probe::detail::memory().allocations.insert(($d,(*$o).as_raw()),(*$i).allocation_size);}};
    (vkBindBufferMemory,$d:expr,$h:ident,$r:ident,$b:ident,$mem:ident,$o:ident) => {if $r==vk::Result::SUCCESS{crate::scale_probe::detail::memory().bindings.insert(($d,$b.as_raw()),($mem.as_raw(),$o));}};
    (vkMapMemory,$d:expr,$h:ident,$r:ident,$mem:ident,$o:ident,$s:ident,$f:ident,$p:ident) => {if $r==vk::Result::SUCCESS{let mut data=crate::scale_probe::detail::memory();let size=if $s==vk::WHOLE_SIZE{data.allocations.get(&($d,$mem.as_raw())).and_then(|n|n.checked_sub($o)).unwrap_or(0)}else{$s};data.maps.insert(($d,$mem.as_raw()),(*$p as usize,$o,size));}};
    (vkUnmapMemory,$d:expr,$h:ident,$r:ident,$mem:ident) => {crate::scale_probe::detail::memory().maps.remove(&($d,$mem.as_raw()));};
    (vkFreeMemory,$d:expr,$h:ident,$r:ident,$mem:ident,$a:ident) => {let mut data=crate::scale_probe::detail::memory();data.maps.remove(&($d,$mem.as_raw()));data.allocations.remove(&($d,$mem.as_raw()));};
    (vkDestroyBuffer,$d:expr,$h:ident,$r:ident,$b:ident,$a:ident) => {let mut data=crate::scale_probe::detail::memory();data.buffers.remove(&($d,$b.as_raw()));data.bindings.remove(&($d,$b.as_raw()));};
    (vkGetDeviceQueue,$($arg:tt)*) => {};
    (vkGetDeviceQueue2,$($arg:tt)*) => {};
    (vkCmdBlitImage,$($arg:tt)*) => {};
    (vkCmdCopyImage,$($arg:tt)*) => {};
    (vkCmdPushConstants,$($arg:tt)*) => {};
    // Unsupported submission/rendering paths must never authorize a source.
    ($name:ident,$($arg:tt)*) => {crate::source_auto::update(|m|m.invalid=true)};
}
