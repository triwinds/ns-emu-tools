#[cfg(windows)]
mod host;
#[cfg(windows)]
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
mod target_probe;
#[cfg(windows)]
mod target_verify;
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
mod fg_api;
