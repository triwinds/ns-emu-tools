use crate::host::{load_library, module_path, verify, write_json, Result};
use crate::sdk_query::{probe_sl_shutdown, Shutdown};
use ash::vk::{self, Handle};
use libloading::Library;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    ffi::{c_void, CStr, CString},
    path::{Path, PathBuf},
};

#[link(name = "streamline_query_bridge", kind = "static")]
unsafe extern "C" {
    fn probe_sl_set_vulkan(
        function: *mut c_void,
        instance: u64,
        physical: u64,
        device: u64,
        family: u32,
        graphics_start: u32,
        compute_start: u32,
    ) -> i32;
    fn probe_sl_register_route(
        function: *mut c_void,
        instance: u64,
        physical: u64,
        device: u64,
        gipa: vk::PFN_vkGetInstanceProcAddr,
        gdpa: vk::PFN_vkGetDeviceProcAddr,
    ) -> i32;
    fn probe_sl_supported(function: *mut c_void, feature: u32, physical: u64) -> i32;
}
type Address = unsafe extern "C" fn();
type Phase = unsafe extern "system" fn(u32);
type Capture = unsafe extern "system" fn(u32) -> u32;
struct CaptureScope(Capture);
impl CaptureScope {
    unsafe fn enter(toggle: Capture) -> Result<Self> {
        if toggle(1) != 1 {
            return Err("capture authorization failed".into());
        }
        Ok(Self(toggle))
    }
}
impl Drop for CaptureScope {
    fn drop(&mut self) {
        unsafe {
            (self.0)(0);
        }
    }
}
struct Context {
    instance: ash::Instance,
    device: Option<ash::Device>,
    shutdown: *mut c_void,
}
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            // SDK cleanup must run while Vulkan objects are still alive.
            if !self.shutdown.is_null() {
                if probe_sl_shutdown(self.shutdown) != 0 {
                    std::process::abort();
                }
            }
            if let Some(device) = &self.device {
                let _ = device.device_wait_idle();
                device.destroy_device(None);
            }
            self.instance.destroy_instance(None);
        }
    }
}
fn union(features: &[Value], key: &str) -> Result<Vec<CString>> {
    let mut names = BTreeSet::new();
    for feature in features {
        for name in feature["requirements"][key]
            .as_array()
            .ok_or("missing requirement array")?
        {
            names.insert(name.as_str().ok_or("non-string requirement")?.to_owned());
        }
    }
    names.into_iter().map(|s| Ok(CString::new(s)?)).collect()
}
fn queue_counts(features: &[Value]) -> Result<(u32, u32, u32)> {
    let mut graphics = 0u32;
    let mut compute = 0u32;
    for feature in features {
        if feature["result"] != 0 || feature["requirements"]["vulkan_supported"] != true {
            return Err("feature query or Vulkan support failed".into());
        }
        graphics = graphics
            .checked_add(
                feature["requirements"]["graphics_queues"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or("invalid graphics count")?,
            )
            .ok_or("graphics count overflow")?;
        compute = compute
            .checked_add(
                feature["requirements"]["compute_queues"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or("invalid compute count")?,
            )
            .ok_or("compute count overflow")?;
    }
    let total = 1u32
        .checked_add(graphics)
        .and_then(|n| n.checked_add(compute))
        .ok_or("queue count overflow")?;
    if total > 64 {
        return Err("diagnostic queue limit exceeded".into());
    }
    Ok((graphics, compute, total))
}
pub(super) unsafe fn run(
    session: &Path,
    sdk: &Library,
    requirements: &[Value],
    guard: &mut Shutdown,
) -> Result<()> {
    let root_session =
        PathBuf::from(std::env::var_os("NS_STREAMLINE_PROBE_SESSION").ok_or("missing session")?);
    let trace_path = root_session.join("layer.jsonl");
    let trace_start = if trace_path.exists() {
        std::fs::read_to_string(&trace_path)?.len()
    } else {
        0
    };
    let loader =
        PathBuf::from(std::env::var_os("NS_STREAMLINE_PROBE_LOADER").ok_or("missing loader")?);
    verify(&loader, "vulkan-1.dll")?;
    let layer = load_library(&PathBuf::from(
        std::env::var_os("NS_STREAMLINE_PROBE_DLL").ok_or("missing layer")?,
    ))?;
    let phase = *layer.get::<Phase>(b"probeSetPhase\0")?;
    let live = *layer.get::<unsafe extern "system" fn() -> u64>(b"probeLiveObjects\0")?;
    let route_requested = std::env::var("NS_STREAMLINE_PROBE_SDK_ROUTE").as_deref() == Ok("1");
    let capture_requested = std::env::var("NS_STREAMLINE_PROBE_IDLE_CAPTURE").as_deref() == Ok("1");
    let capture = *layer.get::<Capture>(b"probeCaptureIdleNext\0")?;
    let entry = ash::Entry::load_from(&loader)?;
    let mut instance_ext = union(requirements, "instance_extensions")?;
    if route_requested {
        for name in [
            ash::khr::surface::NAME,
            ash::khr::win32_surface::NAME,
            ash::khr::get_surface_capabilities2::NAME,
            ash::ext::surface_maintenance1::NAME,
        ] {
            if !instance_ext.iter().any(|s| s.as_c_str() == name) {
                instance_ext.push(name.to_owned());
            }
        }
    }
    let available = entry.enumerate_instance_extension_properties(None)?;
    for name in &instance_ext {
        if !available
            .iter()
            .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name.as_c_str())
        {
            return Err(format!("missing instance extension {name:?}").into());
        }
    }
    let ext: Vec<_> = instance_ext.iter().map(|s| s.as_ptr()).collect();
    let layers = [c"VK_LAYER_NSEMU_streamline_probe".as_ptr()];
    phase(5);
    let instance = entry.create_instance(
        &vk::InstanceCreateInfo::default()
            .application_info(
                &vk::ApplicationInfo::default()
                    .api_version(vk::API_VERSION_1_3)
                    .application_name(c"NSEmu initialized SDK probe"),
            )
            .enabled_extension_names(&ext)
            .enabled_layer_names(&layers),
        None,
    )?;
    let mut context = Context {
        instance,
        device: None,
        shutdown: guard.0,
    };
    guard.0 = std::ptr::null_mut();
    let (graphics, compute, total) = queue_counts(requirements)?;
    let mut selection = None;
    for physical in context.instance.enumerate_physical_devices()? {
        let props = context.instance.get_physical_device_properties(physical);
        if props.vendor_id != 0x10de || props.api_version < vk::API_VERSION_1_3 {
            continue;
        }
        for (family, queue) in context
            .instance
            .get_physical_device_queue_family_properties(physical)
            .iter()
            .enumerate()
        {
            if queue.queue_count >= total
                && queue
                    .queue_flags
                    .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
            {
                selection = Some((
                    physical,
                    family as u32,
                    CStr::from_ptr(props.device_name.as_ptr())
                        .to_string_lossy()
                        .into_owned(),
                ));
                break;
            }
        }
        if selection.is_some() {
            break;
        }
    }
    let (physical, family, gpu) =
        selection.ok_or("no NVIDIA family meeting disjoint queue requirements")?;
    let supported = *sdk.get::<Address>(b"slIsFeatureSupported\0")? as *mut c_void;
    if !route_requested {
        let support: Vec<_> = [1000, 3, 4]
            .into_iter()
            .map(|id| {
                json!({
                    "feature": id, "result": probe_sl_supported(supported, id, physical.as_raw())
                })
            })
            .collect();
        write_json(
            &session.join("sdk-adapter.json"),
            &json!({"gpu": gpu, "features": support}),
        )?;
        if support.iter().any(|s| s["result"] != 0) {
            return Err("SDK adapter support check failed".into());
        }
    }
    let mut device_ext = union(requirements, "device_extensions")?;
    if route_requested {
        for name in [
            ash::khr::swapchain::NAME,
            ash::ext::swapchain_maintenance1::NAME,
        ] {
            if !device_ext.iter().any(|s| s.as_c_str() == name) {
                device_ext.push(name.to_owned());
            }
        }
    }
    let available = context
        .instance
        .enumerate_device_extension_properties(physical)?;
    for name in &device_ext {
        if !available
            .iter()
            .any(|e| CStr::from_ptr(e.extension_name.as_ptr()) == name.as_c_str())
        {
            return Err(format!("missing device extension {name:?}").into());
        }
    }
    let mut maintenance = vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default();
    let mut available12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut available13 = vk::PhysicalDeviceVulkan13Features::default();
    context.instance.get_physical_device_features2(
        physical,
        &mut vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut maintenance)
            .push_next(&mut available12)
            .push_next(&mut available13),
    );
    if route_requested && maintenance.swapchain_maintenance1 != vk::TRUE {
        return Err("presentation fences unavailable".into());
    }
    maintenance.swapchain_maintenance1 = if route_requested { vk::TRUE } else { vk::FALSE };
    let mut features12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut features13 = vk::PhysicalDeviceVulkan13Features::default();
    // Fail closed on any new name rather than silently dropping an SDK requirement.
    for name in union(requirements, "features12")? {
        let (supported, enabled) = match name.to_str()? {
            "timelineSemaphore" => (
                available12.timeline_semaphore,
                &mut features12.timeline_semaphore,
            ),
            "descriptorIndexing" => (
                available12.descriptor_indexing,
                &mut features12.descriptor_indexing,
            ),
            "bufferDeviceAddress" => (
                available12.buffer_device_address,
                &mut features12.buffer_device_address,
            ),
            _ => return Err(format!("unimplemented required Vulkan 1.2 feature: {name:?}").into()),
        };
        if supported != vk::TRUE {
            return Err(format!("unsupported feature: {name:?}").into());
        }
        *enabled = vk::TRUE;
    }
    for name in union(requirements, "features13")? {
        match name.to_str()? {
            "synchronization2" if available13.synchronization2 == vk::TRUE => {
                features13.synchronization2 = vk::TRUE
            }
            _ => {
                return Err(
                    format!("unsupported or unimplemented Vulkan 1.3 feature: {name:?}").into(),
                )
            }
        }
    }
    let priorities = vec![1.0f32; total as usize];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(family)
        .queue_priorities(&priorities)];
    let extensions: Vec<_> = device_ext.iter().map(|s| s.as_ptr()).collect();
    write_json(
        &session.join("sdk-device-plan.json"),
        &json!({
            "gpu": gpu, "family": family, "host_queue_index": 0,
            "graphics_start": 1, "graphics_count": graphics,
            "compute_start": 1 + graphics, "compute_count": compute, "total_queues": total,
            "native_optical_flow": false, "optical_flow_mode": "documented interop",
            "instance_extensions": instance_ext.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>(),
            "device_extensions": device_ext.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>(),
            "features12": union(requirements, "features12")?.iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "features13": union(requirements, "features13")?.iter().map(|s| s.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "api_version": "1.3", "fg_enabled": false, "swapchain_maintenance1": route_requested
        }),
    )?;
    context.device = Some(
        context.instance.create_device(
            physical,
            &vk::DeviceCreateInfo::default()
                .queue_create_infos(&queues)
                .enabled_extension_names(&extensions)
                .push_next(&mut maintenance)
                .push_next(&mut features12)
                .push_next(&mut features13),
            None,
        )?,
    );
    let device = context.device.as_ref().unwrap();
    // Loader may wrap physical handles above this layer. Downstream instance
    // commands require the handle received by this layer during vkCreateDevice.
    let sdk_physical = if route_requested {
        let get = *layer.get::<unsafe extern "system" fn(vk::Device) -> vk::PhysicalDevice>(
            b"probeRoutePhysical\0",
        )?;
        let native = get(device.handle());
        if native == vk::PhysicalDevice::null() {
            return Err("missing layer physical identity".into());
        }
        write_json(
            &session.join("sdk-route-objects.json"),
            &json!({
                "application_physical":physical.as_raw(), "layer_physical":native.as_raw(),
                "device":device.handle().as_raw(), "instance":context.instance.handle().as_raw()
            }),
        )?;
        native
    } else {
        physical
    };
    // The capture mode first compares direct layer and system GDPA lookup behavior.
    if capture_requested {
        phase(12);
        let direct = *layer.get::<vk::PFN_vkGetDeviceProcAddr>(b"vkGetDeviceProcAddr\0")?;
        let next = *layer
            .get::<unsafe extern "system" fn(u64, u32, *const std::ffi::c_char) -> usize>(
                b"probeNextAddress\0",
            )?;
        let expected = next(device.handle().as_raw(), 1, c"vkDeviceWaitIdle".as_ptr());
        let scope = CaptureScope::enter(capture)?;
        let direct_address =
            direct(device.handle(), c"vkDeviceWaitIdle".as_ptr()).ok_or("no direct idle")? as usize;
        let system_address = context
            .instance
            .get_device_proc_addr(device.handle(), c"vkDeviceWaitIdle".as_ptr())
            .ok_or("no system idle")? as usize;
        drop(scope);
        write_json(
            &session.join("idle-capture-control.json"),
            &json!({
                "direct_layer_address": direct_address, "direct_layer_module": module_path(direct_address),
                "system_address_during_scope": system_address, "system_module": module_path(system_address),
                "next_address": expected, "direct_layer_selected_next": direct_address == expected,
                "system_selected_next": system_address == expected
            }),
        )?;
        if expected == 0 || direct_address != expected {
            return Err("capture positive control failed".into());
        }
    }
    if route_requested {
        let register = *sdk.get::<Address>(b"slRegisterVulkanLayerRouteV1\0")? as *mut c_void;
        let gipa = *layer.get::<vk::PFN_vkGetInstanceProcAddr>(b"probeRouteGipa\0")?;
        let gdpa = *layer.get::<vk::PFN_vkGetDeviceProcAddr>(b"probeRouteGdpa\0")?;
        phase(13);
        let result = probe_sl_register_route(
            register,
            context.instance.handle().as_raw(),
            sdk_physical.as_raw(),
            device.handle().as_raw(),
            gipa,
            gdpa,
        );
        write_json(
            &session.join("sdk-route-register.json"),
            &json!({
                "result":result, "route_version":1, "route_size":48,
                "gipa":gipa as usize, "gdpa":gdpa as usize, "fg_enabled":false
            }),
        )?;
        if result != 0 {
            return Err(format!("route registration failed: {result}").into());
        }
    }
    let set_info = *sdk.get::<Address>(b"slSetVulkanInfo\0")? as *mut c_void;
    phase(6);
    let scope = if capture_requested {
        Some(CaptureScope::enter(capture)?)
    } else {
        None
    };
    let set_result = probe_sl_set_vulkan(
        set_info,
        context.instance.handle().as_raw(),
        sdk_physical.as_raw(),
        device.handle().as_raw(),
        family,
        1,
        1 + graphics,
    );
    drop(scope);
    write_json(
        &session.join("sdk-set-vulkan.json"),
        &json!({"result": set_result, "fg_enabled": false}),
    )?;
    if set_result != 0 {
        return Err(format!("slSetVulkanInfo failed: {set_result}").into());
    }
    if route_requested {
        let support: Vec<_> = [1000, 3, 4]
            .into_iter()
            .map(|id| {
                json!({
                    "feature": id, "result": probe_sl_supported(supported, id, sdk_physical.as_raw())
                })
            })
            .collect();
        write_json(
            &session.join("sdk-adapter.json"),
            &json!({"gpu": gpu, "features": support}),
        )?;
        if support.iter().any(|s| s["result"] != 0) {
            return Err("SDK adapter support check failed".into());
        }
    }
    let gdpa = *sdk.get::<vk::PFN_vkGetDeviceProcAddr>(b"vkGetDeviceProcAddr\0")?;
    if route_requested {
        phase(15);
        let application = crate::sdk_commands::exercise(device, family)?;
        let sdk_device = ash::Device::load_with(
            |name| gdpa(device.handle(), name.as_ptr()).map_or(std::ptr::null(), |f| f as *const _),
            device.handle(),
        );
        phase(16);
        let sdk_rows = crate::sdk_commands::exercise(&sdk_device, family)?;
        crate::sdk_commands::report(session, application, sdk_rows)?;
        phase(17);
        let application = crate::sdk_swapchain::exercise(
            &entry,
            &context.instance,
            physical,
            device,
            family,
            None,
        )?;
        let sdk_gipa = *sdk.get::<vk::PFN_vkGetInstanceProcAddr>(b"vkGetInstanceProcAddr\0")?;
        // Only instance lookups use the SDK. Global functions are never used here.
        let sdk_entry = ash::Entry::from_parts_1_1(
            ash::StaticFn {
                get_instance_proc_addr: sdk_gipa,
            },
            entry.fp_v1_0().clone(),
            entry.fp_v1_1().clone(),
        );
        let sdk_instance = ash::Instance::load(sdk_entry.static_fn(), context.instance.handle());
        phase(18);
        let sdk_rows = crate::sdk_swapchain::exercise(
            &sdk_entry,
            &sdk_instance,
            sdk_physical,
            &sdk_device,
            family,
            Some(
                *layer
                    .get::<unsafe extern "system" fn() -> u64>(b"probeRoutePresentCompletions\0")?,
            ),
        )?;
        write_json(
            &session.join("sdk-swapchain-calls.json"),
            &json!({
                "application":application, "sdk":sdk_rows, "queue_index":0,
                "owner":"diagnostic host", "sdk_owned_worker_tested":false,
                "fg_enabled":false, "resize_tested":true, "presentation_restart_tested":true, "sdk_reinitialization_tested":false
            }),
        )?;
        if std::env::var("NS_STREAMLINE_PROBE_FG").as_deref() == Ok("1") {
            phase(19);
            crate::sdk_fg::exercise(
                session,
                sdk,
                &sdk_entry,
                &sdk_instance,
                sdk_physical,
                &sdk_device,
                family,
            )?;
        }
    }
    let idle: vk::PFN_vkDeviceWaitIdle = std::mem::transmute(
        gdpa(device.handle(), c"vkDeviceWaitIdle".as_ptr()).ok_or("missing SDK idle proxy")?,
    );
    let next_address =
        *layer.get::<unsafe extern "system" fn(u64, u32, *const std::ffi::c_char) -> usize>(
            b"probeNextAddress\0",
        )?;
    let next_idle = next_address(device.handle().as_raw(), 1, c"vkDeviceWaitIdle".as_ptr());
    let system_idle = context
        .instance
        .get_device_proc_addr(device.handle(), c"vkDeviceWaitIdle".as_ptr())
        .ok_or("missing system idle")? as usize;
    if next_idle == 0 || system_idle == idle as usize || next_idle == idle as usize {
        return Err("SDK proxy identity was not distinct from system/next".into());
    }
    if route_requested {
        phase(14);
        device.device_wait_idle()?;
    }
    phase(7);
    let main_result = idle(device.handle());
    let raw = device.handle();
    let worker_result = std::thread::spawn(move || {
        phase(8);
        idle(raw)
    })
    .join()
    .map_err(|_| "SDK proxy worker panicked")?;
    write_json(
        &session.join("sdk-proxy-calls.json"),
        &json!({
            "main_idle": main_result.as_raw(), "worker_idle": worker_result.as_raw(),
            "worker_owner": "diagnostic host (not SDK-owned FG worker)",
            "sdk_proxy_address": idle as usize, "sdk_proxy_module": module_path(idle as usize),
            "system_address": system_idle, "system_module": module_path(system_idle),
            "next_address": next_idle, "next_module": module_path(next_idle), "fg_enabled": false,
            "swapchain_created": route_requested
        }),
    )?;
    if main_result != vk::Result::SUCCESS || worker_result != vk::Result::SUCCESS {
        return Err("initialized SDK idle proxy failed".into());
    }
    phase(9);
    let shutdown = context.shutdown;
    context.shutdown = std::ptr::null_mut();
    let shutdown_result = probe_sl_shutdown(shutdown);
    write_json(
        &session.join("sdk-shutdown.json"),
        &json!({"result": shutdown_result}),
    )?;
    if shutdown_result != 0 {
        std::process::abort();
    }
    drop(context);
    if live() != 0 {
        return Err("dispatch maps leaked after initialized SDK cleanup".into());
    }
    let trace = std::fs::read_to_string(&trace_path)?;
    let current_trace = trace
        .get(trace_start..)
        .ok_or("trace truncated between SDK cycles")?;
    std::fs::write(session.join("cycle-trace.jsonl"), current_trace)?;
    let events: Vec<Value> = current_trace
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    let main_reentries = events
        .iter()
        .filter(|v| v["event"] == "vkDeviceWaitIdle" && v["phase"] == 7)
        .count();
    let worker_reentries = events
        .iter()
        .filter(|v| v["event"] == "vkDeviceWaitIdle" && v["phase"] == 8)
        .count();
    let sdk_log = std::fs::read_to_string(root_session.join("sl.log"))?;
    if route_requested && sdk_log.lines().any(|line| line.contains("[error]")) {
        return Err("SDK runtime error during swapchain experiment; inspect sl.log".into());
    }
    if std::env::var("NS_STREAMLINE_PROBE_FG").as_deref() == Ok("1") {
        let report: Value =
            serde_json::from_slice(&std::fs::read(session.join("fg-result.json"))?)?;
        let verification = crate::sdk_fg::validate(&events, &report)?;
        write_json(&session.join("fg-route-verification.json"), &verification)?;
    }
    if route_requested {
        validate_route_trace(&events)?;
        crate::sdk_commands::validate(&events)?;
        let swapchain_report: Value =
            serde_json::from_slice(&std::fs::read(session.join("sdk-swapchain-calls.json"))?)?;
        crate::sdk_swapchain::validate_report(&events, &swapchain_report)?;
        write_json(
            &session.join("sdk-route-result.json"),
            &json!({
                "idle_routing_verified":true, "host_command_lifecycle_verified":true, "host_sdk_swapchain_lifecycle_verified":true, "main_idle_reentries":main_reentries,
                "worker_idle_reentries":worker_reentries, "sdk_owned_worker_tested":false,
                "runtime_routing_verified":false, "fg_enabled":false, "p0_passed":false
            }),
        )?;
    } else if !capture_requested || main_reentries != 0 || worker_reentries != 0 {
        validate_idle_trace(&events)?;
    }
    if capture_requested {
        validate_capture_trace(&events)?;
        let queries = events
            .iter()
            .filter(|v| v["event"] == "idle_capture_query" && v["phase"] == 6)
            .count();
        write_json(
            &session.join("idle-capture-result.json"),
            &json!({
                "experiment_completed": true, "set_info_idle_capture_queries": queries,
                "main_idle_reentries": main_reentries, "worker_idle_reentries": worker_reentries,
                "scoped_idle_capture_avoided_reentry": main_reentries == 0 && worker_reentries == 0,
                "routing_solution_verified": false, "sdk_owned_worker_tested": false,
                "fg_enabled": false, "p0_passed": false
            }),
        )?;
    }
    if shutdown_result != 0 {
        return Err("SDK shutdown failed".into());
    }
    let sdk_warnings: Vec<_> = sdk_log
        .lines()
        .filter(|line| line.contains("[warn]") || line.contains("[error]"))
        .collect();
    write_json(
        &session.join("sdk-device-result.json"),
        &json!({
            "experiment_passed": true, "sdk_initialized": true, "set_vulkan_succeeded": true,
            "initialized_sdk_idle_reenters_layer": main_reentries != 0 || worker_reentries != 0, "dispatch_live_objects": 0,
            "fg_enabled": false, "fg_experiment_run":std::env::var("NS_STREAMLINE_PROBE_FG").as_deref()==Ok("1"), "p0_passed": false, "sdk_swapchain_lifecycle_verified": route_requested, "validation_layer_enabled": false, "sdk_runtime_warnings_and_errors": sdk_warnings
        }),
    )?;
    Ok(())
}
fn validate_route_trace(events: &[Value]) -> Result<()> {
    let count = |name: &str, phase: u32| {
        events
            .iter()
            .filter(|v| v["event"] == name && v["phase"] == phase)
            .count()
    };
    if count("vkDeviceWaitIdle", 14) != 1
        || count("vkDeviceWaitIdle", 7) != 0
        || count("vkDeviceWaitIdle", 8) != 0
    {
        return Err("application idle control or SDK no-reentry assertion failed".into());
    }
    for name in ["route_gipa", "route_gdpa"] {
        if count(name, 6) == 0 {
            return Err("missing registration callback evidence".into());
        }
    }
    if !events.iter().any(|v| {
        v["event"] == "route_gdpa"
            && v["phase"] == 6
            && v["details"]["name"] == "vkDeviceWaitIdle"
            && v["details"]["next"].as_u64().is_some_and(|p| p != 0)
    }) {
        return Err("missing downstream idle lookup".into());
    }
    Ok(())
}
fn validate_idle_trace(events: &[Value]) -> Result<()> {
    let calls = |phase: u32| {
        events
            .iter()
            .filter(|v| v["event"] == "vkDeviceWaitIdle" && v["phase"] == phase)
            .collect::<Vec<_>>()
    };
    let main = calls(7);
    let worker = calls(8);
    if main.len() != 1 || worker.len() != 1 {
        return Err("missing or duplicated SDK idle reentry".into());
    }
    if main[0]["thread"].as_str().is_none()
        || worker[0]["thread"].as_str().is_none()
        || main[0]["thread"] == worker[0]["thread"]
    {
        return Err("worker reentry not observed on a different thread".into());
    }
    Ok(())
}
fn validate_capture_trace(events: &[Value]) -> Result<()> {
    let matching = |event: &str, phase: u32| {
        events
            .iter()
            .filter(|v| v["event"] == event && v["phase"] == phase)
            .collect::<Vec<_>>()
    };
    for phase in [6, 12] {
        let scopes = matching("idle_capture_scope", phase);
        if scopes.len() != 2
            || scopes[0]["details"]["enabled"] != true
            || scopes[1]["details"]["enabled"] != false
        {
            return Err("capture scope evidence incomplete".into());
        }
    }
    if matching("idle_capture_query", 12).len() != 1 {
        return Err("capture positive-control trace missing or duplicated".into());
    }
    if matching("vkDeviceWaitIdle", 7).is_empty() && matching("idle_capture_query", 6).is_empty() {
        return Err("idle disappeared without an observed capture query".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn route_evidence_requires_control_and_downstream_lookup() {
        let mut rows = vec![
            json!({"event":"vkDeviceWaitIdle","phase":14}),
            json!({"event":"route_gipa","phase":6}),
            json!({"event":"route_gdpa","phase":6,"details":{"name":"vkDeviceWaitIdle","next":123}}),
        ];
        assert!(validate_route_trace(&rows).is_ok());
        rows.push(json!({"event":"vkDeviceWaitIdle","phase":8}));
        assert!(validate_route_trace(&rows).is_err());
        rows.pop();
        rows.remove(0);
        assert!(validate_route_trace(&rows).is_err());
        rows.insert(0, json!({"event":"vkDeviceWaitIdle","phase":14}));
        rows[2]["details"]["next"] = json!(0);
        assert!(validate_route_trace(&rows).is_err());
    }
    #[test]
    fn capture_evidence_requires_scopes_and_positive_control() {
        let scope = |p, enabled| json!({"event":"idle_capture_scope","phase":p,"details":{"enabled":enabled}});
        let mut rows = vec![
            scope(12, true),
            json!({"event":"idle_capture_query","phase":12}),
            scope(12, false),
            scope(6, true),
            scope(6, false),
            json!({"event":"vkDeviceWaitIdle","phase":7}),
        ];
        assert!(validate_capture_trace(&rows).is_ok());
        rows.pop();
        assert!(validate_capture_trace(&rows).is_err());
        rows.push(json!({"event":"idle_capture_query","phase":6}));
        assert!(validate_capture_trace(&rows).is_ok());
        rows.remove(1);
        assert!(validate_capture_trace(&rows).is_err());
    }
    use super::*;
    #[test]
    fn idle_evidence_rejects_missing_duplicate_or_same_thread() {
        let row =
            |phase, thread| json!({"event":"vkDeviceWaitIdle", "phase":phase, "thread":thread});
        let good = [row(7, "main"), row(8, "worker")];
        assert!(validate_idle_trace(&good).is_ok());
        assert!(validate_idle_trace(&good[..1]).is_err());
        assert!(validate_idle_trace(&[good[0].clone(), good[1].clone(), good[1].clone()]).is_err());
        assert!(validate_idle_trace(&[row(7, "main"), row(8, "main")]).is_err());
    }
    #[test]
    fn queue_plan_rejects_failed_features_and_overflow() {
        let req = |g: u64, c: u64| {
            json!({"result":0,"requirements":{
            "vulkan_supported":true,"graphics_queues":g,"compute_queues":c}})
        };
        assert_eq!(queue_counts(&[req(1, 2)]).unwrap(), (1, 2, 4));
        assert!(queue_counts(&[req(u32::MAX as u64, 1)]).is_err());
        assert!(queue_counts(&[json!({"result":31})]).is_err());
    }
}
