// SDK-owned pointers never escape the C++ bridge or survive a query.
use crate::host::{load_library, write_json, Result};
use serde_json::{json, Value};
use std::{ffi::c_void, path::PathBuf};

#[repr(C)]
struct Requirements {
    flags: u32,
    cpu_threads: u32,
    viewports: u32,
    graphics_queues: u32,
    compute_queues: u32,
    optical_flow_queues: u32,
    counts: [u32; 5],
    tags: [u32; 64],
    names: [[[u8; 128]; 64]; 4],
    versions: [[u32; 3]; 4],
}
#[link(name = "streamline_query_bridge", kind = "static")]
unsafe extern "C" {
    fn probe_sl_init(function: *mut c_void, directory: *const u16) -> i32;
    fn probe_sl_requirements(function: *mut c_void, feature: u32, out: *mut Requirements) -> i32;
    pub(super) fn probe_sl_shutdown(function: *mut c_void) -> i32;
    fn probe_sl_abi(out: *mut u64);
}
pub(super) struct Shutdown(pub(super) *mut c_void);
impl Drop for Shutdown {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                if probe_sl_shutdown(self.0) != 0 {
                    std::process::abort();
                }
            }
        }
    }
}
fn names(req: &Requirements, group: usize) -> Result<Vec<String>> {
    let count = req.counts[group] as usize;
    if count > 64 {
        return Err("bridge count out of bounds".into());
    }
    req.names[group][..count]
        .iter()
        .map(|bytes| {
            let end = bytes
                .iter()
                .position(|&b| b == 0)
                .ok_or("unterminated bridge string")?;
            Ok(std::str::from_utf8(&bytes[..end])?.to_owned())
        })
        .collect()
}
fn as_json(req: &Requirements) -> Result<Value> {
    let tags = req.counts[4] as usize;
    if tags > 64 {
        return Err("bridge tag count out of bounds".into());
    }
    Ok(json!({
        "flags": req.flags,
        "vulkan_supported": req.flags & 4 != 0,
        "vsync_off_required": req.flags & 8 != 0,
        "hardware_scheduling_required": req.flags & 16 != 0,
        "max_cpu_threads": req.cpu_threads, "max_viewports": req.viewports,
        "graphics_queues": req.graphics_queues, "compute_queues": req.compute_queues,
        "optical_flow_queues": req.optical_flow_queues,
        "instance_extensions": names(req, 0)?, "device_extensions": names(req, 1)?,
        "features12": names(req, 2)?, "features13": names(req, 3)?,
        "required_tags": &req.tags[..tags],
        "os_detected": req.versions[0], "os_required": req.versions[1],
        "driver_detected": req.versions[2], "driver_required": req.versions[3]
    }))
}
pub unsafe fn child(device_probe: bool) -> Result<()> {
    let result = run_child(device_probe);
    if let Err(error) = &result {
        if std::env::var("NS_STREAMLINE_PROBE_SDK_REPEAT").as_deref() == Ok("1") {
            let root = PathBuf::from(
                std::env::var_os("NS_STREAMLINE_PROBE_SESSION").ok_or("missing session")?,
            );
            let completed = (0..20)
                .take_while(|cycle| {
                    root.join(format!("cycle-{cycle:02}/sdk-device-result.json"))
                        .exists()
                })
                .count();
            write_json(
                &root.join("sdk-repeat-failure.json"),
                &json!({"completed_cycles":completed,"requested_cycles":20,"same_process":true,"pid":std::process::id(),"error":error.to_string(),"sdk_reinitialization_verified":false,"fg_enabled":false,"p0_passed":false}),
            )?;
        }
    }
    result
}
unsafe fn run_child(device_probe: bool) -> Result<()> {
    let session =
        PathBuf::from(std::env::var_os("NS_STREAMLINE_PROBE_SESSION").ok_or("missing session")?);
    for name in PLUGINS {
        crate::runtime::verify_runtime(
            &session.join(name),
            name,
            std::env::var("NS_STREAMLINE_PROBE_SDK_ROUTE").as_deref() == Ok("1"),
        )?;
    }
    let expected = PathBuf::from(
        std::env::var_os("NS_STREAMLINE_PROBE_EXE").ok_or("missing expected executable")?,
    )
    .canonicalize()?;
    if expected != std::env::current_exe()?.canonicalize()? {
        return Err("executable mismatch".into());
    }
    // Keep callback providers and the Vulkan Loader alive across SDK cycles.
    // Dropping the last host module reference can invalidate cached SDK/NGX addresses.
    let _loader = if device_probe {
        let path =
            PathBuf::from(std::env::var_os("NS_STREAMLINE_PROBE_LOADER").ok_or("missing loader")?);
        crate::host::verify(&path, "vulkan-1.dll")?;
        Some(load_library(&path)?)
    } else {
        None
    };
    let _layer = if device_probe {
        Some(load_library(&PathBuf::from(
            std::env::var_os("NS_STREAMLINE_PROBE_DLL").ok_or("missing layer")?,
        ))?)
    } else {
        None
    };
    let library = load_library(&session.join("sl.interposer.dll"))?;
    type Address = unsafe extern "C" fn();
    let init = *library.get::<Address>(b"slInit\0")? as *mut c_void;
    let shutdown = *library.get::<Address>(b"slShutdown\0")? as *mut c_void;
    let query = *library.get::<Address>(b"slGetFeatureRequirements\0")? as *mut c_void;
    // Directory lives until after shutdown, including error cleanup.
    use std::os::windows::ffi::OsStrExt;
    let directory: Vec<u16> = session.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut abi = [0u64; 8];
    probe_sl_abi(abi.as_mut_ptr());
    if abi[5] != std::mem::size_of::<Requirements>() as u64 {
        return Err("C/Rust bridge ABI mismatch".into());
    }
    let cycles =
        if device_probe && std::env::var("NS_STREAMLINE_PROBE_SDK_REPEAT").as_deref() == Ok("1") {
            20
        } else {
            1
        };
    let root_session = &session;
    for cycle in 0..cycles {
        let session = if cycles > 1 {
            root_session.join(format!("cycle-{cycle:02}"))
        } else {
            root_session.clone()
        };
        if cycles > 1 {
            std::fs::create_dir(&session)?;
        }
        let modules_before_init = module_snapshot();
        let init_result = probe_sl_init(init, directory.as_ptr());
        let mut guard = Shutdown(if init_result == 0 {
            shutdown
        } else {
            std::ptr::null_mut()
        });
        write_json(
            &session.join("sdk-init.json"),
            &json!({
                "result": init_result, "sdk_version": abi[0], "modules_before_init":modules_before_init, "modules_after_init":module_snapshot(),
                "preferences_size": abi[1], "preferences_version": abi[2],
                "requirements_size": abi[3], "requirements_version": abi[4],
                "bridge_requirements_size": abi[5], "vulkan_info_size": abi[6], "vulkan_info_version": abi[7],
                "feature_ids": [1000,3,4], "preference_flags": 133,
                "application_id": 0, "engine_version": "NSEmu-P0-Probe-0.1",
                "project_id": "fd3bfdaf-72d1-48b9-a4ae-4dcb031c535b",
                "ota_enabled": false, "vulkan_objects_created": false, "fg_enabled": false
            }),
        )?;
        if init_result != 0 {
            return Err(format!("slInit failed in cycle {cycle}: {init_result}").into());
        }

        let mut results = Vec::new();
        let mut all_ok = true;
        for (feature, label) in [(1000, "DLSS-G"), (3, "Reflex"), (4, "PCL")] {
            let mut req: Requirements = std::mem::zeroed();
            let result = probe_sl_requirements(query, feature, &mut req);
            all_ok &= result == 0;
            results.push(json!({"feature": feature, "name": label, "result": result,
            "requirements": if result == 0 { as_json(&req)? } else { Value::Null }}));
        }
        write_json(
            &session.join("sdk-requirements.json"),
            &json!({
                "stage": "before_instance_creation", "features": results, "all_queries_succeeded": all_ok,
                "sdk_initialized": true, "vulkan_objects_created": false, "fg_enabled": false,
                "p0_passed": false, "sdk_proxy_lifecycle_verified": false
            }),
        )?;
        if device_probe && all_ok {
            crate::sdk_device::run(&session, &library, &results, &mut guard)?;
            if cycles > 1 {
                std::fs::copy(root_session.join("sl.log"), session.join("sl.log"))?;
            }
            continue;
        }
        guard.0 = std::ptr::null_mut();
        let shutdown_result = probe_sl_shutdown(shutdown);
        write_json(
            &session.join("sdk-shutdown.json"),
            &json!({"result": shutdown_result}),
        )?;
        if shutdown_result != 0 || !all_ok {
            return Err(format!("SDK query failed or shutdown failed ({shutdown_result})").into());
        }
    }
    if cycles > 1 {
        write_json(
            &session.join("sdk-repeat-result.json"),
            &json!({"completed_cycles":cycles,"same_process":true,"interposer_kept_loaded":true,"sdk_reinitialization_verified":true,"fg_enabled":false,"p0_passed":false}),
        )?;
    }
    Ok(())
}
unsafe fn module_snapshot() -> Vec<Value> {
    PLUGINS
        .iter()
        .chain(["vulkan-1.dll", "streamline_probe_layer.dll"].iter())
        .map(|name| {
            let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
            let base = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(wide.as_ptr());
            json!({"name":name,"base":base as usize})
        })
        .collect()
}
pub const PLUGINS: &[&str] = &[
    "sl.interposer.dll",
    "sl.common.dll",
    "sl.dlss_g.dll",
    "sl.reflex.dll",
    "sl.pcl.dll",
    "nvngx_dlssg.dll",
    "NvLowLatencyVk.dll",
];
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copied_requirements_reject_overflow_and_invalid_strings() {
        assert_eq!(std::mem::size_of::<Requirements>(), 33116);
        assert_eq!(std::mem::offset_of!(Requirements, names), 300);
        let mut req: Requirements = unsafe { std::mem::zeroed() };
        req.counts[0] = 65;
        assert!(as_json(&req).is_err());
        req.counts[0] = 1;
        req.names[0][0] = [b'x'; 128];
        assert!(as_json(&req).is_err());
        req.names[0][0][127] = 0;
        assert!(as_json(&req).is_ok());
    }
}
