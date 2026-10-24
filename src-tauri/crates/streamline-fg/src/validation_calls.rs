//! Attribute strict validation callbacks without enabling optional source probes.
//! This context names the application entry point, not the ultimate fault owner.
use ash::vk;
use std::ffi::CStr;

macro_rules! hook {
    ($name:ident, $pfn:ident, ($handle:ident: $ty:ty $(, $arg:ident: $argty:ty)*), $ret:ty, $failure:expr) => {
        #[allow(non_snake_case)]
        unsafe extern "system" fn $name($handle: $ty, $($arg: $argty),*) -> $ret {
            let Some(dispatch) = crate::device($handle) else { return $failure; };
            let name = CStr::from_bytes_with_nul_unchecked(concat!(stringify!($name), "\0").as_bytes());
            let Some(next) = (dispatch.gdpa)(dispatch.handle, name.as_ptr()) else { return $failure; };
            let next = if crate::target_runtime::enabled() {
                crate::target_runtime::ensure_device(dispatch.handle);
                crate::target_runtime::device_proc(dispatch.handle, name).unwrap_or(next)
            } else { next };
            let next: vk::$pfn = std::mem::transmute(next);
            crate::validation_context::application(stringify!($name), || next($handle, $($arg),*))
        }
    };
}

hook!(vkCreateShaderModule, PFN_vkCreateShaderModule, (handle: vk::Device, info: *const vk::ShaderModuleCreateInfo, alloc: *const vk::AllocationCallbacks, out: *mut vk::ShaderModule), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);
hook!(vkCreateGraphicsPipelines, PFN_vkCreateGraphicsPipelines, (handle: vk::Device, cache: vk::PipelineCache, count: u32, info: *const vk::GraphicsPipelineCreateInfo, alloc: *const vk::AllocationCallbacks, out: *mut vk::Pipeline), vk::Result, vk::Result::ERROR_INITIALIZATION_FAILED);
hook!(vkCmdBeginRenderPass, PFN_vkCmdBeginRenderPass, (handle: vk::CommandBuffer, info: *const vk::RenderPassBeginInfo, contents: vk::SubpassContents), (), ());
hook!(vkCmdBeginRenderPass2, PFN_vkCmdBeginRenderPass2, (handle: vk::CommandBuffer, info: *const vk::RenderPassBeginInfo, begin: *const vk::SubpassBeginInfo), (), ());
hook!(vkCmdBeginRenderPass2KHR, PFN_vkCmdBeginRenderPass2, (handle: vk::CommandBuffer, info: *const vk::RenderPassBeginInfo, begin: *const vk::SubpassBeginInfo), (), ());
hook!(vkCmdBeginRendering, PFN_vkCmdBeginRendering, (handle: vk::CommandBuffer, info: *const vk::RenderingInfo), (), ());
hook!(vkCmdBeginRenderingKHR, PFN_vkCmdBeginRendering, (handle: vk::CommandBuffer, info: *const vk::RenderingInfo), (), ());
hook!(vkCmdClearAttachments, PFN_vkCmdClearAttachments, (handle: vk::CommandBuffer, count: u32, attachments: *const vk::ClearAttachment, rect_count: u32, rects: *const vk::ClearRect), (), ());
hook!(vkCmdClearDepthStencilImage, PFN_vkCmdClearDepthStencilImage, (handle: vk::CommandBuffer, image: vk::Image, layout: vk::ImageLayout, value: *const vk::ClearDepthStencilValue, count: u32, ranges: *const vk::ImageSubresourceRange), (), ());
hook!(vkCmdEndQuery, PFN_vkCmdEndQuery, (handle: vk::CommandBuffer, pool: vk::QueryPool, query: u32), (), ());
hook!(vkCmdDraw, PFN_vkCmdDraw, (handle: vk::CommandBuffer, vertices: u32, instances: u32, first_vertex: u32, first_instance: u32), (), ());
hook!(vkCmdDrawIndexed, PFN_vkCmdDrawIndexed, (handle: vk::CommandBuffer, indices: u32, instances: u32, first_index: u32, vertex_offset: i32, first_instance: u32), (), ());

pub(super) unsafe fn intercept(name: &CStr, strict: bool) -> vk::PFN_vkVoidFunction {
    if !strict {
        return None;
    }
    macro_rules! pick {
        ($($f:ident),*) => {
            match name.to_bytes() {
                $(s if s == stringify!($f).as_bytes() => Some(std::mem::transmute($f as *const ())),)*
                _ => None,
            }
        };
    }
    pick!(
        vkCreateShaderModule,
        vkCreateGraphicsPipelines,
        vkCmdBeginRenderPass,
        vkCmdBeginRenderPass2,
        vkCmdBeginRenderPass2KHR,
        vkCmdBeginRendering,
        vkCmdBeginRenderingKHR,
        vkCmdClearAttachments,
        vkCmdClearDepthStencilImage,
        vkCmdEndQuery,
        vkCmdDraw,
        vkCmdDrawIndexed
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;
    use std::cell::RefCell;

    thread_local! {
        static FORWARDED: RefCell<Vec<(u64, u64, u32, Option<&'static str>)>> = const { RefCell::new(Vec::new()) };
    }

    unsafe extern "system" fn next_end_query(
        command: vk::CommandBuffer,
        pool: vk::QueryPool,
        query: u32,
    ) {
        FORWARDED.with(|calls| {
            calls.borrow_mut().push((
                command.as_raw(),
                pool.as_raw(),
                query,
                crate::validation_context::current(),
            ))
        });
    }

    unsafe extern "system" fn next_proc(
        _: vk::Device,
        name: *const std::ffi::c_char,
    ) -> vk::PFN_vkVoidFunction {
        if CStr::from_ptr(name) == c"vkCmdEndQuery" {
            Some(std::mem::transmute(next_end_query as *const ()))
        } else {
            None
        }
    }

    #[test]
    fn callback_context_and_query_arguments_forward_without_advertising_missing_commands() {
        // Two dispatchable objects share one device dispatch-table key.
        let device_storage = Box::new(0usize);
        let dispatch_key = (&*device_storage as *const usize) as usize;
        let command_storage = Box::new(dispatch_key);
        let device_storage = Box::new(dispatch_key);
        let device = vk::Device::from_raw((&*device_storage as *const usize) as u64);
        let command = vk::CommandBuffer::from_raw((&*command_storage as *const usize) as u64);
        crate::state().devices.insert(
            dispatch_key,
            crate::Device {
                set_loader_data: None,
                physical: vk::PhysicalDevice::null(),
                handle: device,
                gdpa: next_proc,
            },
        );
        struct Retire(usize);
        impl Drop for Retire {
            fn drop(&mut self) {
                crate::state().devices.remove(&self.0);
            }
        }
        let _retire = Retire(dispatch_key);
        FORWARDED.with(|calls| calls.borrow_mut().clear());
        unsafe {
            let function: vk::PFN_vkCmdEndQuery =
                std::mem::transmute(intercept(c"vkCmdEndQuery", true).unwrap());
            function(command, vk::QueryPool::from_raw(123), 17);
            assert!(crate::vkGetDeviceProcAddr(device, c"vkCreateShaderModule".as_ptr()).is_none());
        }
        FORWARDED.with(|calls| {
            assert_eq!(
                *calls.borrow(),
                vec![(command.as_raw(), 123, 17, Some("vkCmdEndQuery"))]
            )
        });
        assert_eq!(crate::validation_context::current(), None);
    }

    #[test]
    fn normal_profile_has_no_extra_call_wrappers_and_unknown_commands_stay_absent() {
        unsafe {
            for name in [
                c"vkCreateShaderModule",
                c"vkCmdEndQuery",
                c"vkCmdDrawIndexed",
                c"vkCmdBeginRenderPass2KHR",
                c"vkCmdBeginRenderingKHR",
                c"vkCmdClearAttachments",
            ] {
                assert!(intercept(name, false).is_none());
                assert!(intercept(name, true).is_some());
            }
            assert!(intercept(c"vkUnsupportedAttributionCommand", true).is_none());
        }
    }
}
