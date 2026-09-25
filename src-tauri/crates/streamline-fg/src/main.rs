//! Toolbox launcher. The Vulkan diagnostic host is a separate opt-in binary.
#[cfg(windows)]
mod launcher;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod runtime;
#[cfg(windows)]
mod session_verify;
#[cfg(windows)]
mod support;
fn main() {
    #[cfg(windows)]
    {
        if !cfg!(target_arch = "x86_64") {
            eprintln!("FG requires Windows x64.");
            std::process::exit(1);
        }
        let args: Vec<_> = std::env::args_os().skip(1).collect();
        if args.len() == 1 && args[0] == "--help" {
            println!("streamline-layer-probe --target-probe --fg --target <exe> --layer <dll> --runtime <directory> --session <new directory> [--game <file>] [--reference-params]\nToolbox launcher. SDK host experiments require the separate streamline-fg-diagnostics binary.");
            return;
        }
        let result = if args.first().is_some_and(|a| a == "--target-probe") {
            launcher::run(&args[1..])
        } else {
            Err("expected --target-probe (use --help)".into())
        };
        if let Err(error) = result {
            eprintln!("FG launch failed: {error}");
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    {
        eprintln!("FG requires Windows x64.");
        std::process::exit(1);
    }
}
