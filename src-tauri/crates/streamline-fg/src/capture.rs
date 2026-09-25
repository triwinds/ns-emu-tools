use std::{cell::Cell, ffi::CStr};

thread_local! { static ACTIVE: Cell<bool> = const { Cell::new(false) }; }

/// Diagnostic only: selects next for idle GDPA queries on the calling thread.
/// Does not replace Loader tables or intercept calls on other threads.
#[no_mangle]
pub extern "system" fn probeCaptureIdleNext(enabled: u32) -> u32 {
    if !crate::trace::authorized() {
        return 0;
    }
    ACTIVE.with(|v| v.set(enabled != 0));
    crate::trace::event(
        "idle_capture_scope",
        serde_json::json!({"enabled": enabled != 0}),
    );
    1
}

pub fn select_next(name: &CStr) -> bool {
    ACTIVE.with(Cell::get) && name == c"vkDeviceWaitIdle"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capture_is_idle_only_and_thread_local() {
        assert!(!select_next(c"vkDeviceWaitIdle"));
        ACTIVE.with(|v| v.set(true));
        assert!(select_next(c"vkDeviceWaitIdle"));
        assert!(!select_next(c"vkDestroyDevice"));
        assert!(!std::thread::spawn(|| select_next(c"vkDeviceWaitIdle"))
            .join()
            .unwrap());
        ACTIVE.with(|v| v.set(false));
        assert!(!select_next(c"vkDeviceWaitIdle"));
    }
}
