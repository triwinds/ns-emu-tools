//! Nonblocking window guard for the bounded, fixed-window FG diagnostic.
//! Window changes are cancelled while active; the render thread turns FG off.
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use windows_sys::Win32::{Foundation::*, System::LibraryLoader::*, UI::WindowsAndMessaging::*};
static WINDOW: AtomicIsize = AtomicIsize::new(0);
static ROOT: AtomicIsize = AtomicIsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static REASON: AtomicI32 = AtomicI32::new(-1);
static HOOKS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
unsafe extern "system" fn callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0
        && ACTIVE.load(Ordering::Acquire)
        && (wparam as isize == WINDOW.load(Ordering::Relaxed)
            || wparam as isize == ROOT.load(Ordering::Relaxed))
        && matches!(code as u32, HCBT_MOVESIZE | HCBT_MINMAX | HCBT_DESTROYWND)
    {
        REASON.store(code, Ordering::Relaxed);
        STOP.store(true, Ordering::Release);
        return 1;
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}
pub(super) unsafe fn install(hwnd: isize) -> Result<(), String> {
    let mut hooks = HOOKS.lock().unwrap();
    if !hooks.is_empty() {
        return Ok(());
    }
    let root = GetAncestor(hwnd as HWND, GA_ROOT);
    WINDOW.store(hwnd, Ordering::Relaxed);
    ROOT.store(root as isize, Ordering::Relaxed);
    let mut module = std::ptr::null_mut();
    if GetModuleHandleExW(
        GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
        callback as *const () as *const u16,
        &mut module,
    ) == 0
    {
        return Err("window guard module lookup failed".into());
    }
    let mut threads = vec![
        GetWindowThreadProcessId(hwnd as HWND, std::ptr::null_mut()),
        GetWindowThreadProcessId(root, std::ptr::null_mut()),
    ];
    threads.sort_unstable();
    threads.dedup();
    // Publish only a complete set. Roll back every installed hook if a later install fails.
    let mut pending = Vec::new();
    for thread in threads {
        let hook = if thread == 0 {
            std::ptr::null_mut()
        } else {
            SetWindowsHookExW(WH_CBT, Some(callback), module, thread)
        };
        if hook.is_null() {
            let error = if thread == 0 {
                "invalid window thread".to_owned()
            } else {
                std::io::Error::last_os_error().to_string()
            };
            for installed in pending {
                UnhookWindowsHookEx(installed as HHOOK);
            }
            WINDOW.store(0, Ordering::Relaxed);
            ROOT.store(0, Ordering::Relaxed);
            return Err(error);
        }
        pending.push(hook as usize);
    }
    *hooks = pending;
    Ok(())
}
pub(super) fn active(enabled: bool) {
    ACTIVE.store(enabled, Ordering::Release);
}
pub(super) fn stopping() -> bool {
    STOP.load(Ordering::Acquire)
}
pub(super) fn reason() -> i32 {
    REASON.load(Ordering::Relaxed)
}
pub(super) unsafe fn uninstall() {
    ACTIVE.store(false, Ordering::Release);
    for hook in HOOKS.lock().unwrap().drain(..) {
        UnhookWindowsHookEx(hook as HHOOK);
    }
    WINDOW.store(0, Ordering::Relaxed);
    ROOT.store(0, Ordering::Relaxed);
}

// Call only after the old swapchain has been destroyed and its device is idle.
// A successor has its own warmup and frame-history reset; an old window event
// must not poison that new chain. Bounded diagnostics keep the terminal latch.
pub(super) fn retired(allow_successor: bool) {
    ACTIVE.store(false, Ordering::Release);
    if allow_successor {
        REASON.store(-1, Ordering::Relaxed);
        STOP.store(false, Ordering::Release);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retirement_rearms_only_ordinary_sessions() {
        STOP.store(true, Ordering::Release);
        REASON.store(HCBT_MOVESIZE as i32, Ordering::Relaxed);
        retired(false);
        assert!(stopping());
        retired(true);
        assert!(!stopping());
        assert_eq!(reason(), -1);
        assert!(!ACTIVE.load(Ordering::Acquire));
    }
}
