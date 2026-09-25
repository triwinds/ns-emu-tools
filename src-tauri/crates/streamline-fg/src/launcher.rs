//! Process-scoped transparent-layer launch for the explicitly selected target.
use crate::support::{hash, write_json, Result};
use serde_json::{json, Value};
use std::{ffi::OsString, fs, path::PathBuf, process::Command};
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
    let mut reflex_ab = false;
    let mut bounded = false;
    let mut reference_params = false;
    let mut motion_estimate = false;
    let mut args = args.iter();
    while let Some(flag) = args.next() {
        if flag == "--estimate-motion" {
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
    if motion_estimate && !fg {
        return Err("--estimate-motion requires --fg".into());
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
    // An unknown build must not inherit the verified build's version or publisher metadata.
    let profile = if compatibility == target_policy::Compatibility::Verified {
        let mut profile = baseline;
        profile["executable"] = json!(executable);
        profile
    } else {
        json!({"executable":executable,"sha256":target_hash,"version":null,"publisher_authenticity_verified":false})
    };
    let layer = dunce::canonicalize(layer.ok_or("missing --layer")?)?;
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
        for name in crate::runtime::PLUGINS {
            crate::runtime::verify_runtime(&runtime.join(name), name, true)?;
            fs::copy(runtime.join(name), runtime_dest.join(name))?;
            crate::runtime::verify_runtime(&runtime_dest.join(name), name, true)?;
        }
    }
    #[cfg(not(feature = "sdk-bridge"))]
    let _ = runtime;
    let manifests = session.join("manifests");
    fs::create_dir(&manifests)?;
    write_json(
        &manifests.join("probe.json"),
        &json!({"file_format_version":"1.2.0","layer":{"name":"VK_LAYER_NSEMU_streamline_probe","type":"GLOBAL","library_path":layer,"api_version":"1.3.0","implementation_version":"1","description":"Process-scoped transparent target diagnostic","functions":{"vkNegotiateLoaderLayerInterfaceVersion":"vkNegotiateLoaderLayerInterfaceVersion"}}}),
    )?;
    let mut paths = vec![manifests];
    if let Some(old) = std::env::var_os("VK_LAYER_PATH") {
        paths.extend(std::env::split_paths(&old));
    }
    let disable = match std::env::var("VK_LOADER_LAYERS_DISABLE") {
        Ok(old) if !old.is_empty() => format!("{old},VK_LAYER_reshade"),
        _ => "VK_LAYER_reshade".into(),
    };
    let mut command = Command::new(&executable);
    command
        .current_dir(executable.parent().unwrap())
        .env("VK_LAYER_PATH", std::env::join_paths(paths)?)
        .env("VK_INSTANCE_LAYERS", "VK_LAYER_NSEMU_streamline_probe")
        .env("VK_LOADER_LAYERS_DISABLE", &disable)
        .env("VK_LOADER_DEBUG", "error,warn,layer")
        .env("NS_STREAMLINE_LIVE_DIR", &session)
        .env("NS_STREAMLINE_PROBE_EXE", &executable)
        .env("NS_STREAMLINE_PROBE_TRACE", session.join("layer.jsonl"))
        .env("NS_STREAMLINE_TARGET_SDK", if sdk_off { "1" } else { "0" })
        .env("NS_STREAMLINE_TARGET_RUNTIME", &runtime_dest)
        .env("NS_STREAMLINE_TARGET_FG", if fg { "1" } else { "0" })
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
    if let Some(game) = &game {
        command.arg(game);
    }
    write_json(
        &session.join("target-inputs.json"),
        &json!({"profile":profile,"compatibility":compatibility.as_str(),"allow_unverified_target":allow_unverified,"layer_sha256":hash(&layer)?,"game":game,"fg_requested":fg,"reflex_ab_requested":reflex_ab,"reference_parameters":reference_params,"motion_estimate":motion_estimate,"frame_budget":if bounded {Some(600)} else {None},"layer_only":!sdk_off,"sdk_off_integration":sdk_off && !fg,"fg_experiment_requested":fg,"child_disable":disable}),
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
    if !status.success() {
        return Err("target process failed; inspect evidence".into());
    }
    crate::session_verify::verify(&session)
}
