//! Serialize native queue entry points shared by the application and SDK route.
//! Never hold this lock across SDK hooks, worker joins, or CPU fence waits.
#![allow(non_snake_case)]
use super::*;
use std::sync::Arc;

#[derive(Default)]
struct Locks(Mutex<HashMap<u64, Arc<Mutex<()>>>>);
impl Locks {
    fn device(&self, device: u64) -> Arc<Mutex<()>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(device)
            .or_default()
            .clone()
    }
    fn remove(&self, device: u64) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&device);
    }
}
static LOCKS: OnceLock<Locks> = OnceLock::new();

pub(super) fn native<T>(device: vk::Device, call: impl FnOnce() -> T) -> T {
    let lock = LOCKS.get_or_init(Default::default).device(device.as_raw());
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    call()
}
pub(super) fn retired(device: vk::Device) {
    LOCKS.get_or_init(Default::default).remove(device.as_raw());
}

macro_rules! forward {
    ($name:ident,$pfn:ident,($h:ident:$ty:ty $(,$a:ident:$at:ty)*)) => {
        unsafe extern "system" fn $name($h:$ty,$($a:$at),*) -> vk::Result {
            let Some(d) = device($h) else { return vk::Result::ERROR_DEVICE_LOST };
            let Some(f) = (d.gdpa)(d.handle, concat!(stringify!($name),"\0").as_ptr().cast()) else {
                return vk::Result::ERROR_EXTENSION_NOT_PRESENT;
            };
            let f:vk::$pfn = std::mem::transmute(f);
            native(d.handle, || f($h,$($a),*))
        }
    }
}
forward!(vkQueueSubmit,PFN_vkQueueSubmit,(queue:vk::Queue,count:u32,submits:*const vk::SubmitInfo,fence:vk::Fence));
forward!(vkQueueSubmit2,PFN_vkQueueSubmit2,(queue:vk::Queue,count:u32,submits:*const vk::SubmitInfo2,fence:vk::Fence));
forward!(vkQueueSubmit2KHR,PFN_vkQueueSubmit2,(queue:vk::Queue,count:u32,submits:*const vk::SubmitInfo2,fence:vk::Fence));
forward!(vkQueueBindSparse,PFN_vkQueueBindSparse,(queue:vk::Queue,count:u32,binds:*const vk::BindSparseInfo,fence:vk::Fence));
forward!(vkQueueWaitIdle,PFN_vkQueueWaitIdle,(queue:vk::Queue));
forward!(vkDeviceWaitIdle,PFN_vkDeviceWaitIdle,(handle:vk::Device));

pub(super) unsafe fn intercept(name: &CStr) -> vk::PFN_vkVoidFunction {
    match name.to_bytes() {
        b"vkQueueSubmit" => Some(std::mem::transmute(vkQueueSubmit as *const ())),
        b"vkQueueSubmit2" => Some(std::mem::transmute(vkQueueSubmit2 as *const ())),
        b"vkQueueSubmit2KHR" => Some(std::mem::transmute(vkQueueSubmit2KHR as *const ())),
        b"vkQueueBindSparse" => Some(std::mem::transmute(vkQueueBindSparse as *const ())),
        b"vkQueueWaitIdle" => Some(std::mem::transmute(vkQueueWaitIdle as *const ())),
        b"vkDeviceWaitIdle" => Some(std::mem::transmute(vkDeviceWaitIdle as *const ())),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn application_and_worker_use_one_lock_per_device() {
        let locks = Locks::default();
        let application = locks.device(1);
        let worker = locks.device(1);
        assert!(Arc::ptr_eq(&application, &worker));
        let busy = application.lock().unwrap();
        let thread = std::thread::spawn(move || worker.try_lock().is_err());
        assert!(thread.join().unwrap());
        assert!(locks.device(2).try_lock().is_ok());
        drop(busy);
        assert!(locks.device(1).try_lock().is_ok());
        locks.remove(1);
        assert!(!Arc::ptr_eq(&application, &locks.device(1)));
    }
}
