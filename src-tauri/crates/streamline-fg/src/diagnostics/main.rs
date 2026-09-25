#[cfg(windows)]
mod host;
#[cfg(windows)]
#[path = "../launcher.rs"]
mod launcher;
#[cfg(windows)]
#[path = "../runtime.rs"]
mod runtime;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod sdk_commands;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod sdk_device;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod sdk_fg;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod sdk_query;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod sdk_swapchain;
#[cfg(windows)]
#[path = "../session_verify.rs"]
mod session_verify;
#[cfg(any(windows, test))]
mod verify;

fn main() {
    #[cfg(windows)]
    if let Err(error) = host::run() {
        eprintln!("probe failed: {error}");
        std::process::exit(1);
    }
    #[cfg(not(windows))]
    {
        eprintln!("This diagnostic requires Windows x64.");
        std::process::exit(1);
    }
}

#[cfg(all(windows, feature = "sdk-bridge"))]
#[path = "../fg_api.rs"]
mod fg_api;

#[cfg(windows)]
#[path = "../support.rs"]
mod support;
