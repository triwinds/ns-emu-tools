//! Protect the pinned SDK's printf-style validation sink without filtering errors.
//! Only messengers created synchronously by slSetVulkanInfo are adapted. The
//! application's dispatch and our raw validation recorder remain unchanged.
use ash::vk::{self, Handle};
use std::{
    cell::Cell,
    collections::HashMap,
    ffi::{c_void, CStr, CString},
    sync::{Mutex, OnceLock},
};

thread_local! {
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn during_sdk_init<T>(f: impl FnOnce() -> T) -> T {
    struct Restore(bool);
    impl Drop for Restore {
        fn drop(&mut self) {
            CAPTURE.with(|v| v.set(self.0));
        }
    }
    let _restore = Restore(CAPTURE.with(|v| v.replace(true)));
    f()
}

struct Callback {
    function: vk::PFN_vkDebugUtilsMessengerCallbackEXT,
    // Store the opaque pointer as an integer; the adapter never dereferences it.
    user_data: usize,
}
type Key = (u64, u64);
fn callbacks() -> &'static Mutex<HashMap<Key, Box<Callback>>> {
    static CALLBACKS: OnceLock<Mutex<HashMap<Key, Box<Callback>>>> = OnceLock::new();
    CALLBACKS.get_or_init(Mutex::default)
}

unsafe extern "system" fn callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    user_data: *mut c_void,
) -> vk::Bool32 {
    let original = &*user_data.cast::<Callback>();
    let Some(function) = original.function else {
        return vk::FALSE;
    };
    if data.is_null() || (*data).p_message.is_null() {
        return function(severity, kind, data, original.user_data as *mut c_void);
    }
    let message = CStr::from_ptr((*data).p_message).to_bytes();
    if !message.contains(&b'%') {
        return function(severity, kind, data, original.user_data as *mut c_void);
    }
    // sl.common's debugUtilsMessengerCallback passes pMessage directly to
    // SL_LOG_ERROR. Escape every literal percent, including non-UTF-8 bytes.
    // printf then prints the original message; Vulkan metadata is untouched.
    let mut escaped = Vec::with_capacity(message.len());
    for &byte in message {
        escaped.push(byte);
        if byte == b'%' {
            escaped.push(byte);
        }
    }
    let escaped = CString::new(escaped).expect("CStr contains no interior NUL");
    let mut safe = *data;
    safe.p_message = escaped.as_ptr();
    function(severity, kind, &safe, original.user_data as *mut c_void)
}

unsafe fn create_with(
    next: vk::PFN_vkCreateDebugUtilsMessengerEXT,
    instance: vk::Instance,
    info: *const vk::DebugUtilsMessengerCreateInfoEXT,
    alloc: *const vk::AllocationCallbacks,
    output: *mut vk::DebugUtilsMessengerEXT,
) -> vk::Result {
    if !CAPTURE.with(Cell::get) || info.is_null() || (*info).pfn_user_callback.is_none() {
        return next(instance, info, alloc, output);
    }
    let mut owned = Box::new(Callback {
        function: (*info).pfn_user_callback,
        user_data: (*info).p_user_data as usize,
    });
    let mut safe = *info;
    safe.pfn_user_callback = Some(callback);
    safe.p_user_data = (&mut *owned as *mut Callback).cast();
    let result = next(instance, &safe, alloc, output);
    if result == vk::Result::SUCCESS {
        callbacks()
            .lock()
            .unwrap()
            .insert((instance.as_raw(), (*output).as_raw()), owned);
    }
    result
}

unsafe fn destroy_with(
    next: vk::PFN_vkDestroyDebugUtilsMessengerEXT,
    instance: vk::Instance,
    messenger: vk::DebugUtilsMessengerEXT,
    alloc: *const vk::AllocationCallbacks,
) {
    // Keep callback storage alive during downstream destruction, without
    // holding a mutex across code that may invoke a callback.
    let owned = callbacks()
        .lock()
        .unwrap()
        .remove(&(instance.as_raw(), messenger.as_raw()));
    next(instance, messenger, alloc);
    drop(owned);
}

unsafe fn downstream(instance: vk::Instance, name: &CStr) -> vk::PFN_vkVoidFunction {
    let dispatch = crate::state()
        .instances
        .values()
        .find(|d| d.handle == instance)
        .copied()?;
    (dispatch.gipa)(instance, name.as_ptr())
}

unsafe extern "system" fn create(
    instance: vk::Instance,
    info: *const vk::DebugUtilsMessengerCreateInfoEXT,
    alloc: *const vk::AllocationCallbacks,
    output: *mut vk::DebugUtilsMessengerEXT,
) -> vk::Result {
    let Some(next) = downstream(instance, c"vkCreateDebugUtilsMessengerEXT") else {
        return vk::Result::ERROR_EXTENSION_NOT_PRESENT;
    };
    create_with(std::mem::transmute(next), instance, info, alloc, output)
}

unsafe extern "system" fn destroy(
    instance: vk::Instance,
    messenger: vk::DebugUtilsMessengerEXT,
    alloc: *const vk::AllocationCallbacks,
) {
    if let Some(next) = downstream(instance, c"vkDestroyDebugUtilsMessengerEXT") {
        destroy_with(std::mem::transmute(next), instance, messenger, alloc);
    }
}

pub(super) unsafe fn intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    match name.to_bytes() {
        b"vkCreateDebugUtilsMessengerEXT" => Some(std::mem::transmute(create as *const ())),
        b"vkDestroyDebugUtilsMessengerEXT" => Some(std::mem::transmute(destroy as *const ())),
        _ => None,
    }
}

/// Called only after downstream instance destruction has completed.
pub(super) fn after_destroy_instance(instance: vk::Instance) {
    callbacks()
        .lock()
        .unwrap()
        .retain(|key, _| key.0 != instance.as_raw());
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Seen {
        messages: Vec<Vec<u8>>,
        ids: Vec<i32>,
    }
    unsafe extern "system" fn record(
        severity: vk::DebugUtilsMessageSeverityFlagsEXT,
        kind: vk::DebugUtilsMessageTypeFlagsEXT,
        data: *const vk::DebugUtilsMessengerCallbackDataEXT,
        user_data: *mut c_void,
    ) -> vk::Bool32 {
        assert_eq!(severity, vk::DebugUtilsMessageSeverityFlagsEXT::ERROR);
        assert_eq!(kind, vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION);
        let seen = &mut *user_data.cast::<Seen>();
        seen.messages
            .push(CStr::from_ptr((*data).p_message).to_bytes().to_vec());
        seen.ids.push((*data).message_id_number);
        vk::TRUE
    }
    unsafe fn emit(info: &vk::DebugUtilsMessengerCreateInfoEXT) {
        let data = vk::DebugUtilsMessengerCallbackDataEXT::default()
            .message(c"%SubgroupEqMaskKHR %s %% 100%")
            .message_id_number(4744);
        assert_eq!(
            (info.pfn_user_callback.unwrap())(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
                &data,
                info.p_user_data,
            ),
            vk::TRUE
        );
    }
    unsafe extern "system" fn mock_create(
        _: vk::Instance,
        info: *const vk::DebugUtilsMessengerCreateInfoEXT,
        _: *const vk::AllocationCallbacks,
        output: *mut vk::DebugUtilsMessengerEXT,
    ) -> vk::Result {
        emit(&*info); // A callback can happen before creation returns.
        *output = vk::DebugUtilsMessengerEXT::from_raw(42);
        vk::Result::SUCCESS
    }
    unsafe extern "system" fn mock_fail(
        _: vk::Instance,
        info: *const vk::DebugUtilsMessengerCreateInfoEXT,
        _: *const vk::AllocationCallbacks,
        _: *mut vk::DebugUtilsMessengerEXT,
    ) -> vk::Result {
        emit(&*info);
        vk::Result::ERROR_OUT_OF_HOST_MEMORY
    }
    unsafe extern "system" fn mock_destroy(
        _: vk::Instance,
        _: vk::DebugUtilsMessengerEXT,
        _: *const vk::AllocationCallbacks,
    ) {
    }
    #[test]
    fn scoped_creation_escapes_only_sdk_message_and_preserves_result_and_metadata() {
        let mut seen = Seen {
            messages: vec![],
            ids: vec![],
        };
        let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .pfn_user_callback(Some(record))
            .user_data((&mut seen as *mut Seen).cast());
        let instance = vk::Instance::from_raw(101);
        let mut output = vk::DebugUtilsMessengerEXT::null();
        unsafe {
            assert_eq!(
                during_sdk_init(|| create_with(
                    mock_create,
                    instance,
                    &info,
                    std::ptr::null(),
                    &mut output
                )),
                vk::Result::SUCCESS
            );
            let map = callbacks().lock().unwrap();
            let owned = map.get(&(101, 42)).unwrap();
            let live = vk::DebugUtilsMessengerCreateInfoEXT::default()
                .pfn_user_callback(Some(callback))
                .user_data((&**owned as *const Callback as *mut Callback).cast());
            emit(&live);
            drop(map);
            destroy_with(mock_destroy, instance, output, std::ptr::null());
            assert_eq!(
                create_with(mock_create, instance, &info, std::ptr::null(), &mut output),
                vk::Result::SUCCESS
            );
        }
        assert_eq!(
            seen.messages,
            vec![
                b"%%SubgroupEqMaskKHR %%s %%%% 100%%".to_vec(),
                b"%%SubgroupEqMaskKHR %%s %%%% 100%%".to_vec(),
                b"%SubgroupEqMaskKHR %s %% 100%".to_vec()
            ]
        );
        assert_eq!(seen.ids, vec![4744; 3]);
        assert!(!callbacks().lock().unwrap().contains_key(&(101, 42)));
    }
    #[test]
    fn failed_creation_and_scope_unwind_do_not_retain_callback_or_capture() {
        let mut seen = Seen {
            messages: vec![],
            ids: vec![],
        };
        let info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .pfn_user_callback(Some(record))
            .user_data((&mut seen as *mut Seen).cast());
        let mut output = vk::DebugUtilsMessengerEXT::null();
        unsafe {
            assert_eq!(
                during_sdk_init(|| create_with(
                    mock_fail,
                    vk::Instance::from_raw(102),
                    &info,
                    std::ptr::null(),
                    &mut output
                )),
                vk::Result::ERROR_OUT_OF_HOST_MEMORY
            );
        }
        assert!(!callbacks().lock().unwrap().contains_key(&(102, 0)));
        let _ = std::panic::catch_unwind(|| during_sdk_init(|| panic!("test scope")));
        assert!(!CAPTURE.with(Cell::get));
        during_sdk_init(|| {
            during_sdk_init(|| assert!(CAPTURE.with(Cell::get)));
            assert!(CAPTURE.with(Cell::get));
        });
        assert!(!CAPTURE.with(Cell::get));
    }
    #[test]
    fn messages_without_percent_and_non_utf8_bytes_are_preserved() {
        let mut seen = Seen {
            messages: vec![],
            ids: vec![],
        };
        let mut original = Callback {
            function: Some(record),
            user_data: (&mut seen as *mut Seen) as usize,
        };
        for message in [b"ordinary error".as_slice(), b"\xff%\xfe".as_slice()] {
            let message = CString::new(message).unwrap();
            let data = vk::DebugUtilsMessengerCallbackDataEXT::default()
                .message(&message)
                .message_id_number(1);
            unsafe {
                callback(
                    vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
                    vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION,
                    &data,
                    (&mut original as *mut Callback).cast(),
                );
            }
            assert_eq!(data.p_message, message.as_ptr());
        }
        assert_eq!(
            seen.messages,
            vec![b"ordinary error".to_vec(), b"\xff%%\xfe".to_vec()]
        );
    }
}
