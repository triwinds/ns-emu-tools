//! Toolbox launcher. The Vulkan diagnostic host is a separate opt-in binary.
#[cfg(windows)]
mod launcher;
#[cfg(all(windows, feature = "native-nr"))]
mod nr_package;
#[cfg(all(windows, feature = "sdk-bridge"))]
mod runtime;
#[cfg(windows)]
mod scale_analysis;
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
            println!("Toolbox game launch: --target-probe --graphics-launch --target <exe> --layer <packaged DLL> --runtime <package> --session <new directory> [--fg] [--sr-mode off|quality|balanced|performance|dlaa] [--native-nr --nr-runtime <verified DLL> --nr-bridge <nvngx.dll> --nr-initial-off]. Normal launches omit validation/readback and do not run strict acceptance; diagnostics below retain their gates.");
            println!("Native NR experiment (native-nr build): --target-probe --native-nr --target <exe> --layer <native-nr DLL> --runtime <pinned Streamline directory> --nr-runtime <inspected nvngx_dlssnr.dll> --nr-bridge <audited nvngx.dll> --validation-dir <VVL 1.4.363.0 Bin> --session <new directory> [--game <file>] [--nr-intensity 0..1] [--nr-readback] [--nr-performance (omit --validation-dir; validation and readback disabled, no acceptance)] [--fg] [--sr-mode off|quality|balanced|performance|dlaa]. NR has independent live nrEnabled/nrIntensity/nrRevision fields. Actual FG is off unless --fg is specified. Uses synthetic depth and pauses NR without valid NVOF. Runtime, bridge and validation files are pinned; Toolbox installation and local runtime import are separate from these diagnostics.");
            println!("streamline-layer-probe --target-probe --fg --target <exe> --layer <dll> --runtime <directory> --session <new directory> [--game <file>] [--reference-params] [--nvof] [--sr-mode off|quality|balanced|performance|dlaa] [--sr-scale 50..200 (percent)] [--sr-preset default|j|k|l|m]\nRead-only scaling capture: --target-probe --scale-probe --target <exe> --layer <dll> --session <new directory> [--game <file>] (no SDK). Analyze: --analyze-scale <session>\nExplicit one-frame GPU trials: --scale-copy-probe reads source/output; --scale-replace-probe also compares a private linear blit and overwrites one output only on a match. Pinned executable only, no SDK/FG; uncertain GPU completion aborts the test process.\nToolbox launcher. SDK host experiments require the separate streamline-fg-diagnostics binary.");
            return;
        }
        let result = if args.len() == 2 && args[0] == "--analyze-scale" {
            scale_analysis::run(std::path::Path::new(&args[1]))
        } else if args.first().is_some_and(|a| a == "--target-probe") {
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

#[cfg(windows)]
mod scale_model;
