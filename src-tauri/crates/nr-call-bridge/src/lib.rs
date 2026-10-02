//! Diagnostic-only Vulkan caller boundary. Not a replacement NGX core library.
#![allow(clippy::missing_safety_doc)]
#[path = "../../streamline-fg/src/nr_abi.rs"]
// The shared host/bridge contract also contains host-only resource metadata.
#[allow(dead_code)]
mod nr_abi;

#[no_mangle]
pub extern "C" fn NRBridgeAbiVersion() -> u64 {
    nr_abi::bridge_abi()
}

#[no_mangle]
#[inline(never)]
pub unsafe extern "C" fn NRBridgeInvoke(call: *const nr_abi::Call) -> u32 {
    if call.is_null() {
        return nr_abi::NGX_INVALID_PARAMETER;
    }
    let result = nr_abi::dispatch(&*call);
    // A real instruction after the snippet call prevents an optimized tail
    // jump. Verify the release DLL's disassembly before trusting caller checks.
    std::sync::atomic::compiler_fence(std::sync::atomic::Ordering::SeqCst);
    std::hint::black_box(result)
}
