//! Process-scoped transparent-layer launch for the explicitly selected target.
use crate::support::{hash, write_json, Result};
use serde_json::{json, Value};
use std::{ffi::OsString, fs, path::PathBuf, process::Command};
#[path = "../../streamline-sr-preset.rs"]
mod sr_preset;
#[path = "../../streamline-target-policy.rs"]
mod target_policy;
pub(super) fn run(args: &[OsString]) -> Result<()> {
    if args.first().is_some_and(|a| a == "--verify") {
        if args.len() != 2 {
            return Err("--verify requires one session path".into());
        }
        return crate::session_verify::verify(&PathBuf::from(&args[1]));
    }
    let (mut layer, mut session, mut game, mut runtime) = (None, None, None, None);
    let mut target = None;
    let mut expected_target_hash = None;
    let mut allow_unverified = false;
    let mut sdk_off = false;
    let mut fg = false;
    let mut scale_probe = false;
    let mut scale_copy = false;
    let mut scale_replace = false;
    let mut sr_scale: Option<u16> = None;
    let mut sr_mode = String::from("off");
    let mut sr_preset = sr_preset::StreamlineSrPreset::default();
    let mut reflex_ab = false;
    let mut bounded = false;
    let mut reference_params = false;
    let mut motion_estimate = false;
    let mut native_nr = false;
    let mut nr_runtime = None;
    let mut nr_bridge = None;
    let mut validation_dir = None;
    let mut nr_intensity = 1.0f32;
    let mut nr_readback = false;
    let mut nr_performance = false;
    let mut graphics_launch = false;
    let mut nr_initial_off = false;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if flag == "--graphics-launch" {
            graphics_launch = true;
            sdk_off = true;
            continue;
        }
        if flag == "--nr-initial-off" {
            nr_initial_off = true;
            continue;
        }
        if flag == "--nr-performance" {
            nr_performance = true;
            continue;
        }
        if flag == "--nr-readback" {
            nr_readback = true;
            continue;
        }
        if flag == "--native-nr" {
            native_nr = true;
            sdk_off = true;
            motion_estimate = true;
            continue;
        }
        if flag == "--nr-intensity" {
            nr_intensity = args
                .next()
                .and_then(|v| v.to_str())
                .ok_or("missing NR intensity")?
                .parse()?;
            if !nr_intensity.is_finite() || !(0.0..=1.0).contains(&nr_intensity) {
                return Err("NR intensity must be 0..1".into());
            }
            continue;
        }
        if flag == "--scale-replace-probe" {
            scale_replace = true;
            scale_copy = true;
            scale_probe = true;
            continue;
        }
        if flag == "--scale-copy-probe" {
            scale_copy = true;
            scale_probe = true;
            continue;
        }
        if flag == "--scale-probe" {
            scale_probe = true;
            continue;
        }
        if flag == "--sr-scale" {
            let value = args
                .next()
                .and_then(|s| s.to_str())
                .ok_or("missing SR scale")?
                .parse::<u16>()?;
            if !(50..=200).contains(&value) {
                return Err("SR scale must be 50..200 percent".into());
            }
            sr_scale = Some(value);
            continue;
        }
        if flag == "--sr-preset" {
            sr_preset = args
                .next()
                .and_then(|v| v.to_str())
                .and_then(sr_preset::StreamlineSrPreset::parse)
                .ok_or("invalid SR preset")?;
            continue;
        }
        if flag == "--sr-mode" {
            let value = args
                .next()
                .and_then(|s| s.to_str())
                .ok_or("missing SR mode")?;
            if !["off", "quality", "balanced", "performance", "dlaa"].contains(&value) {
                return Err("invalid SR mode".into());
            }
            sr_mode = value.to_owned();
            continue;
        }
        if flag == "--nvof" || flag == "--estimate-motion" {
            motion_estimate = true;
            continue;
        }
        if flag == "--reference-params" {
            reference_params = true;
            continue;
        }
        if flag == "--expected-target-sha256" {
            if expected_target_hash.is_some() {
                return Err("duplicate target hash".into());
            }
            expected_target_hash = Some(
                args.next()
                    .ok_or("missing target hash")?
                    .to_str()
                    .ok_or("invalid target hash")?
                    .to_owned(),
            );
            continue;
        }
        if flag == "--allow-unverified-target" {
            allow_unverified = true;
            continue;
        }
        if flag == "--bounded" {
            bounded = true;
            continue;
        }
        if flag == "--reflex-ab" {
            bounded = true;
            reflex_ab = true;
            continue;
        }
        if flag == "--fg" {
            sdk_off = true;
            fg = true;
            continue;
        }
        if flag == "--sdk-off" {
            sdk_off = true;
            continue;
        }
        let slot = match flag.to_str() {
            Some("--target") => &mut target,
            Some("--layer") => &mut layer,
            Some("--session") => &mut session,
            Some("--game") => &mut game,
            Some("--runtime") => &mut runtime,
            Some("--nr-runtime") => &mut nr_runtime,
            Some("--nr-bridge") => &mut nr_bridge,
            Some("--validation-dir") => &mut validation_dir,
            _ => return Err("unknown target probe argument".into()),
        };
        if slot.is_some() {
            return Err("duplicate argument".into());
        }
        let path = PathBuf::from(args.next().ok_or("missing argument")?);
        if !path.is_absolute() {
            return Err("absolute path required".into());
        }
        *slot = Some(path);
    }
    if scale_probe && sdk_off {
        return Err("--scale-probe requires transparent mode (no --fg/--sdk-off)".into());
    }
    if native_nr && !cfg!(feature = "native-nr") {
        return Err("native NR requires a native-nr build".into());
    }
    if nr_readback && !native_nr {
        return Err("--nr-readback requires --native-nr".into());
    }
    if nr_initial_off && !native_nr {
        return Err("--nr-initial-off requires --native-nr".into());
    }
    if graphics_launch
        && (nr_performance
            || nr_readback
            || validation_dir.is_some()
            || scale_probe
            || bounded
            || reflex_ab)
    {
        return Err(
            "normal graphics launch cannot include diagnostic, readback or validation flags".into(),
        );
    }
    if nr_performance && (!native_nr || nr_readback || validation_dir.is_some()) {
        return Err(
            "--nr-performance requires --native-nr without --nr-readback or --validation-dir"
                .into(),
        );
    }
    if !native_nr && (nr_runtime.is_some() || nr_bridge.is_some() || validation_dir.is_some()) {
        return Err("NR paths require --native-nr".into());
    }
    if sr_mode != "off" && !fg && !native_nr && !graphics_launch {
        return Err("SR currently requires --fg presentation integration".into());
    }
    if motion_estimate && !fg && !native_nr && !graphics_launch {
        return Err("--nvof requires --fg".into());
    }
    if reference_params && !fg {
        return Err("--reference-params requires --fg".into());
    }
    if bounded && !fg {
        return Err("--bounded/--reflex-ab requires --fg".into());
    }
    let baseline: Value = serde_json::from_str(include_str!("../target-profile.json"))?;
    let executable = dunce::canonicalize(
        target.unwrap_or_else(|| PathBuf::from(baseline["executable"].as_str().unwrap())),
    )?;
    target_policy::validate_executable(&executable)?;
    let target_hash = hash(&executable)?;
    if expected_target_hash
        .as_ref()
        .is_some_and(|expected| expected != &target_hash)
    {
        return Err("target changed since installation check".into());
    }
    let compatibility = target_policy::classify(&target_hash, true);
    compatibility.authorize(allow_unverified)?;
    if scale_copy && compatibility != target_policy::Compatibility::Verified {
        return Err("GPU copy trial requires the pinned verified executable".into());
    }
    // An unknown build must not inherit the verified build's version or publisher metadata.
    let profile = if compatibility == target_policy::Compatibility::Verified {
        let mut profile = baseline;
        profile["executable"] = json!(executable);
        profile
    } else {
        json!({"executable":executable,"sha256":target_hash,"version":null,"publisher_authenticity_verified":false})
    };
    let layer = dunce::canonicalize(layer.ok_or("missing --layer")?)?;
    #[cfg(feature = "native-nr")]
    if native_nr {
        unsafe {
            let module = libloading::Library::new(&layer)?;
            let version = *module.get::<unsafe extern "C" fn() -> u64>(b"nrLayerAbi\0")?;
            if version() != 0x0001_0000_0000_0001 {
                return Err("native NR layer ABI mismatch".into());
            }
        }
    }
    let session = session.ok_or("missing --session")?;
    for key in [
        "VK_INSTANCE_LAYERS",
        "VK_LOADER_LAYERS_ENABLE",
        "VK_LOADER_LAYERS_ALLOW",
    ] {
        if std::env::var_os(key).is_some_and(|x| !x.is_empty()) {
            return Err(format!("unsupported inherited {key}").into());
        }
    }
    if let Some(game) = &game {
        if !game.is_file() {
            return Err("game path does not exist".into());
        }
    }
    fs::create_dir(&session)?;
    let session = dunce::canonicalize(session)?;
    if sdk_off && !cfg!(feature = "sdk-bridge") {
        return Err("SDK target mode requires sdk-bridge build".into());
    }
    let runtime_dest = session.join("runtime");
    #[cfg(feature = "sdk-bridge")]
    if sdk_off {
        let runtime = runtime.ok_or("missing --runtime")?;
        fs::create_dir(&runtime_dest)?;
        if fg || graphics_launch || (native_nr && sr_mode != "off") {
            for name in crate::runtime::SR_PLUGINS {
                crate::runtime::verify_sr(&runtime.join(name), name)?;
                fs::copy(runtime.join(name), runtime_dest.join(name))?;
                crate::runtime::verify_sr(&runtime_dest.join(name), name)?;
            }
        }
        for name in crate::runtime::PLUGINS {
            crate::runtime::verify_runtime(&runtime.join(name), name, true)?;
            fs::copy(runtime.join(name), runtime_dest.join(name))?;
            crate::runtime::verify_runtime(&runtime_dest.join(name), name, true)?;
        }
    }
    #[cfg(not(feature = "sdk-bridge"))]
    let _ = runtime;
    #[cfg(feature = "native-nr")]
    let mut nr_files = Value::Null;
    #[cfg(not(feature = "native-nr"))]
    let nr_files = Value::Null;
    #[cfg(feature = "native-nr")]
    if native_nr {
        let source = nr_runtime.as_ref().ok_or("missing --nr-runtime")?;
        let bridge = nr_bridge.as_ref().ok_or("missing --nr-bridge")?;
        if bridge.file_name().and_then(|s| s.to_str()) != Some("nvngx.dll") {
            return Err("NR bridge must be named nvngx.dll".into());
        }
        let nr_hash = crate::nr_package::verify(source, &crate::nr_package::RUNTIMES)?;
        let bridge_hash = crate::nr_package::verify(bridge, &[crate::nr_package::BRIDGE])?;
        let dir = session.join("nr");
        fs::create_dir(&dir)?;
        fs::copy(source, dir.join("nvngx_dlssnr.dll"))?;
        fs::copy(bridge, dir.join("nvngx.dll"))?;
        crate::nr_package::verify(&dir.join("nvngx_dlssnr.dll"), &[nr_hash.as_str()])?;
        crate::nr_package::verify(&dir.join("nvngx.dll"), &[bridge_hash.as_str()])?;
        let mut validation_files = Value::Null;
        if !nr_performance && !graphics_launch {
            let validation = validation_dir
                .as_ref()
                .ok_or("native NR experiment requires --validation-dir")?;
            let manifest = crate::nr_package::verify(
                &validation.join("VkLayer_khronos_validation.json"),
                &["672a281330703083230ff02cabdd1afd524b600e0a11ae8610119e06e4183b02"],
            )?;
            let dll = crate::nr_package::verify(
                &validation.join("VkLayer_khronos_validation.dll"),
                &["2acc317ef880f73a9862f23a964694c03f531b866fab6d70f1412fcbae60caac"],
            )?;
            validation_files = json!({"manifest_sha256":manifest,"dll_sha256":dll});
        }
        nr_files = json!({"runtime_sha256":nr_hash,"bridge_sha256":bridge_hash,"validation_manifest_sha256":validation_files["manifest_sha256"],"validation_dll_sha256":validation_files["dll_sha256"],"synthetic_depth":true});
    }
    let manifests = session.join("manifests");
    fs::create_dir(&manifests)?;
    write_json(
        &manifests.join("probe.json"),
        &json!({"file_format_version":"1.2.0","layer":{"name":"VK_LAYER_NSEMU_streamline_probe","type":"GLOBAL","library_path":layer,"api_version":"1.3.0","implementation_version":"1","description":"Process-scoped transparent target diagnostic","functions":{"vkNegotiateLoaderLayerInterfaceVersion":"vkNegotiateLoaderLayerInterfaceVersion"}}}),
    )?;
    let mut paths = vec![manifests];
    if let Some(dir) = &validation_dir {
        paths.push(dir.clone());
    }
    if let Some(old) = std::env::var_os("VK_LAYER_PATH") {
        paths.extend(std::env::split_paths(&old));
    }
    let disable = if native_nr {
        "~implicit~".into()
    } else {
        match std::env::var("VK_LOADER_LAYERS_DISABLE") {
            Ok(old) if !old.is_empty() => format!("{old},VK_LAYER_reshade"),
            _ => "VK_LAYER_reshade".into(),
        }
    };
    let mut command = Command::new(&executable);
    command
        .current_dir(executable.parent().unwrap())
        .env(
            "NS_STREAMLINE_SCALE_COPY",
            if scale_copy { "1" } else { "0" },
        )
        .env(
            "NS_STREAMLINE_SCALE_REPLACE",
            if scale_replace { "1" } else { "0" },
        )
        .env("VK_LAYER_PATH", std::env::join_paths(paths)?)
        .env(
            "VK_INSTANCE_LAYERS",
            if native_nr && !nr_performance && !graphics_launch {
                "VK_LAYER_NSEMU_streamline_probe;VK_LAYER_KHRONOS_validation"
            } else {
                "VK_LAYER_NSEMU_streamline_probe"
            },
        )
        .env("VK_LOADER_LAYERS_DISABLE", &disable)
        .env("VK_LOADER_DEBUG", "error,warn,layer")
        .env(
            "NS_STREAMLINE_SCALE_PROBE",
            if scale_probe { "1" } else { "0" },
        )
        .env("NS_STREAMLINE_LIVE_DIR", &session)
        .env("NS_STREAMLINE_PROBE_EXE", &executable)
        .env("NS_STREAMLINE_PROBE_TRACE", session.join("layer.jsonl"))
        .env("NS_STREAMLINE_TARGET_SDK", if sdk_off { "1" } else { "0" })
        .env("NS_STREAMLINE_TARGET_RUNTIME", &runtime_dest)
        .env("NS_STREAMLINE_TARGET_FG", if fg { "1" } else { "0" })
        .env(
            "NS_STREAMLINE_GRAPHICS_RUNTIME",
            if graphics_launch { "1" } else { "0" },
        )
        .env("NS_STREAMLINE_NATIVE_NR", if native_nr { "1" } else { "0" })
        .env(
            "NS_STREAMLINE_NR_INITIAL_ENABLED",
            if nr_initial_off { "0" } else { "1" },
        )
        .env(
            "NS_STREAMLINE_NR_VALIDATION",
            if native_nr && !nr_performance && !graphics_launch {
                "1"
            } else {
                "0"
            },
        )
        .env(
            "NS_STREAMLINE_NR_RUNTIME",
            session.join("nr/nvngx_dlssnr.dll"),
        )
        .env("NS_STREAMLINE_NR_BRIDGE", session.join("nr/nvngx.dll"))
        .env("NS_STREAMLINE_NR_INTENSITY", nr_intensity.to_string())
        .env(
            "NS_STREAMLINE_NR_READBACK",
            if nr_readback { "1" } else { "0" },
        )
        .env("NS_STREAMLINE_TARGET_SR_MODE", &sr_mode)
        .env("NS_STREAMLINE_TARGET_SR_PRESET", sr_preset.as_str())
        .env(
            "NS_STREAMLINE_TARGET_SR_SCALE",
            sr_scale.map(|v| v.to_string()).unwrap_or_default(),
        )
        .env(
            "NS_STREAMLINE_SOURCE_AUTO",
            if (fg || native_nr || graphics_launch)
                && compatibility == target_policy::Compatibility::Verified
            {
                "1"
            } else {
                "0"
            },
        )
        .env(
            "NS_STREAMLINE_TARGET_SR_READY",
            if fg || graphics_launch || (native_nr && sr_mode != "off") {
                "1"
            } else {
                "0"
            },
        )
        .env(
            "NS_STREAMLINE_TARGET_REFLEX_AB",
            if reflex_ab { "1" } else { "0" },
        )
        .env(
            "NS_STREAMLINE_TARGET_FRAME_BUDGET",
            if bounded { "600" } else { "0" },
        )
        .env(
            "NS_STREAMLINE_TARGET_REFERENCE_PARAMS",
            if reference_params { "1" } else { "0" },
        )
        .env(
            "NS_STREAMLINE_TARGET_MOTION",
            if motion_estimate { "1" } else { "0" },
        )
        .env("NS_STREAMLINE_PROBE_FG", "0")
        .env("NS_STREAMLINE_PROBE_SDK_ROUTE", "0")
        .stdout(fs::File::create(session.join("target.stdout.log"))?)
        .stderr(fs::File::create(session.join("target.stderr.log"))?);
    // Ryubing applies this override after ReloadConfig, including per-game config.
    // Enhanced launches require Vulkan even when the stored backend is invalid.
    if fg || native_nr || graphics_launch {
        command.args(["--graphics-backend", "Vulkan"]);
    }
    if let Some(game) = &game {
        command.arg(game);
    }
    write_json(
        &session.join("target-inputs.json"),
        &json!({"profile":profile,"compatibility":compatibility.as_str(),"allow_unverified_target":allow_unverified,"layer_sha256":hash(&layer)?,"game":game,"graphics_launch":graphics_launch,"nr_requested":native_nr && !nr_initial_off,"nr_available":native_nr,"nr_performance":nr_performance,"nr_validation_requested":native_nr && !nr_performance && !graphics_launch,"nr_intensity":nr_intensity,"nr_readback_requested":nr_readback,"nr_files":nr_files,"fg_requested":fg,"sr_mode":sr_mode,"sr_preset":sr_preset,"frame_trace_enabled":std::env::var("NS_STREAMLINE_TRACE_FRAMES").as_deref() != Ok("0"),"sr_input":"auto_native_or_present","sr_scale":sr_scale,"reflex_ab_requested":reflex_ab,"reference_parameters":reference_params,"motion_estimate":motion_estimate,"motion_backend":if motion_estimate {"nvof"} else {"zero"},"frame_budget":if bounded {Some(600)} else {None},"scale_probe":scale_probe,"scale_copy_probe":scale_copy,"scale_replace_probe":scale_replace,"layer_only":!sdk_off,"sdk_off_integration":sdk_off && !fg && !native_nr,"fg_experiment_requested":fg,"child_disable":disable}),
    )?;
    write_json(
        &session.join("target-command.json"),
        &json!({"arguments":command.get_args().map(|a|a.to_string_lossy().into_owned()).collect::<Vec<_>>(),"graphics_backend_override":if fg||native_nr||graphics_launch {Some("Vulkan")} else {None}}),
    )?;
    if hash(&executable)? != target_hash {
        return Err("target changed during launch preparation; inspect it again".into());
    }
    let mut child = command.spawn()?;
    write_json(
        &session.join("target-process.json"),
        &json!({"pid":child.id()}),
    )?;
    println!(
        "Target diagnostic running; close Ryujinx normally when finished. Evidence: {}",
        session.display()
    );
    let status = child.wait()?;
    write_json(
        &session.join("target-exit.json"),
        &json!({"success":status.success(),"code":status.code(),"fg_requested":fg}),
    )?;
    if graphics_launch {
        write_json(
            &session.join("launch-result.json"),
            &json!({"target_success":status.success(),"target_exit_code":status.code(),"strict_acceptance_run":false,"validation_requested":false,"graphics_launch":true}),
        )?;
        return if status.success() {
            Ok(())
        } else {
            Err("game process failed; inspect target.stderr.log".into())
        };
    }
    if !status.success() && !native_nr {
        return Err("target process failed; inspect evidence".into());
    }
    crate::session_verify::verify(&session)
}
