use ash::vk::{self, Handle};
use libloading::Library;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::{c_char, CStr};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::LibraryLoader::{
    GetModuleFileNameW, GetModuleHandleExW, GetModuleHandleW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, WS_OVERLAPPEDWINDOW,
};

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
type Phase = unsafe extern "system" fn(u32);
type Next = unsafe extern "system" fn(u64, u32, *const c_char) -> usize;
type Live = unsafe extern "system" fn() -> u64;
const LAYER: &CStr = c"VK_LAYER_NSEMU_streamline_probe";

pub(crate) fn hash(path: &Path) -> Result<String> {
    let mut source = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub(crate) fn verify(path: &Path, name: &str) -> Result<String> {
    let profile: Value =
        serde_json::from_str(include_str!("../../streamline-fg-preflight/baseline.json"))?;
    let expected = profile["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["path"] == name)
        .ok_or("missing frozen hash")?["sha256"]
        .as_str()
        .unwrap();
    let actual = hash(path)?;
    if actual != expected {
        return Err(format!("frozen hash mismatch: {}", path.display()).into());
    }
    Ok(actual)
}
pub(crate) fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut output = OpenOptions::new().create_new(true).write(true).open(path)?;
    serde_json::to_writer_pretty(&mut output, value)?;
    writeln!(output)?;
    Ok(())
}

pub fn run() -> Result<()> {
    if !cfg!(target_arch = "x86_64") {
        return Err("Windows x64 required".into());
    }
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--target-probe") {
        return crate::target_probe::run(&args[1..]);
    }
    if args.first().is_some_and(|a| a == "--child") {
        #[cfg(feature = "sdk-bridge")]
        if args.len() == 2 && (args[1] == "requirements" || args[1] == "sdk-device") {
            return unsafe { crate::sdk_query::child(args[1] == "sdk-device") };
        }
        if args.len() != 2 || (args[1] != "baseline" && args[1] != "layered") {
            return Err("invalid child mode".into());
        }
        return unsafe { child(args[1] == "layered") };
    }
    if args.len() == 1 && args[0] == "--help" {
        println!("streamline-layer-probe [--sdk-requirements | --sdk-device | --sdk-idle-capture | --sdk-route | --sdk-route-repeat | --sdk-fg] --layer <absolute probe DLL> --interposer <absolute frozen sl.interposer.dll> --session <new absolute directory>\nDefault: uninitialized baseline/layered experiment. Optional sdk-bridge feature: --sdk-requirements queries before Vulkan creation; --sdk-device tests initialized idle dispatch. --sdk-idle-capture tests scoped next-address selection for idle only. --sdk-route-repeat attempts 20 same-process SDK lifecycles and stops on the first failure. Only --sdk-fg enables FG in the diagnostic host. ReShade is disabled only in these children. No registry changes. Sessions are never overwritten.");
        return Ok(());
    }
    let sdk_fg = args.first().is_some_and(|a| a == "--sdk-fg");
    let sdk_repeat = args.first().is_some_and(|a| a == "--sdk-route-repeat");
    let sdk_route = sdk_fg || sdk_repeat || args.first().is_some_and(|a| a == "--sdk-route");
    let idle_capture = args.first().is_some_and(|a| a == "--sdk-idle-capture");
    let sdk_device = sdk_route || idle_capture || args.first().is_some_and(|a| a == "--sdk-device");
    let requirements_only = sdk_device || args.first().is_some_and(|a| a == "--sdk-requirements");
    if requirements_only {
        if !cfg!(feature = "sdk-bridge") {
            return Err("SDK probe modes need --features sdk-bridge at build time".into());
        }
        args.remove(0);
    }
    let (mut layer, mut interposer, mut session) = (None, None, None);
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let slot = match flag.to_str() {
            Some("--layer") => &mut layer,
            Some("--interposer") => &mut interposer,
            Some("--session") => &mut session,
            _ => return Err("unknown argument (use --help)".into()),
        };
        if slot.is_some() {
            return Err("duplicate argument".into());
        }
        let path = PathBuf::from(iter.next().ok_or("missing argument value")?);
        if !path.is_absolute() {
            return Err("all paths must be absolute".into());
        }
        *slot = Some(path);
    }
    let layer = dunce::canonicalize(layer.ok_or("missing --layer")?)?;
    let interposer = interposer.ok_or("missing --interposer")?.canonicalize()?;
    let session = session.ok_or("missing --session")?;
    let loader = PathBuf::from(std::env::var_os("SystemRoot").ok_or("missing SystemRoot")?)
        .join("System32/vulkan-1.dll");
    let loader_hash = verify(&loader, "vulkan-1.dll")?;
    let interposer_hash =
        crate::runtime::verify_runtime(&interposer, "sl.interposer.dll", sdk_route)?;
    let layer_hash = hash(&layer)?;
    let host_hash = hash(&std::env::current_exe()?)?;
    let mut staged_hashes = serde_json::Map::new();
    staged_hashes.insert("sl.interposer.dll".into(), json!(interposer_hash));
    fs::create_dir(&session)?;
    let session = dunce::canonicalize(session)?;
    let executable = std::env::current_exe()?.canonicalize()?;
    let staged_interposer = session.join("sl.interposer.dll");
    fs::copy(&interposer, &staged_interposer)?;
    crate::runtime::verify_runtime(&staged_interposer, "sl.interposer.dll", sdk_route)?;
    #[cfg(feature = "sdk-bridge")]
    if requirements_only {
        for name in crate::sdk_query::PLUGINS
            .iter()
            .filter(|&&n| n != "sl.interposer.dll")
        {
            let source = interposer
                .parent()
                .ok_or("missing plugin directory")?
                .join(name);
            crate::runtime::verify_runtime(&source, name, sdk_route)?;
            fs::copy(&source, session.join(name))?;
            staged_hashes.insert(
                name.to_string(),
                json!(crate::runtime::verify_runtime(
                    &session.join(name),
                    name,
                    sdk_route
                )?),
            );
        }
    }
    let manifests = session.join("manifests");
    fs::create_dir(&manifests)?;
    write_json(
        &manifests.join("probe.json"),
        &json!({"file_format_version": "1.2.0", "layer": {"name": LAYER.to_str()?, "type": "GLOBAL", "library_path": layer, "api_version": "1.3.0", "implementation_version": "1", "description": "NSEmu P0 diagnostic only", "functions": {"vkNegotiateLoaderLayerInterfaceVersion": "vkNegotiateLoaderLayerInterfaceVersion"}}}),
    )?;
    let old_disable = std::env::var("VK_LOADER_LAYERS_DISABLE").unwrap_or_default();
    let disable = if old_disable.is_empty() {
        "VK_LAYER_reshade".into()
    } else {
        format!("{old_disable},VK_LAYER_reshade")
    };
    // Preserve explicit search paths. The probe is enabled in VkInstanceCreateInfo only.
    let mut paths = vec![manifests.clone()];
    if let Some(old) = std::env::var_os("VK_LAYER_PATH") {
        paths.extend(std::env::split_paths(&old));
    }
    let layer_paths = std::env::join_paths(paths)?;
    let environment: Value = [
        "VK_LAYER_PATH",
        "VK_ADD_LAYER_PATH",
        "VK_INSTANCE_LAYERS",
        "VK_LOADER_LAYERS_ENABLE",
        "VK_LOADER_LAYERS_DISABLE",
        "VK_LOADER_LAYERS_ALLOW",
    ]
    .iter()
    .map(|k| {
        (
            k.to_string(),
            json!(std::env::var_os(k).map(|s| s.to_string_lossy().into_owned())),
        )
    })
    .collect();
    // An inherited force-enable/allow can override the experiment's conflict exclusion.
    for name in [
        "VK_INSTANCE_LAYERS",
        "VK_LOADER_LAYERS_ENABLE",
        "VK_LOADER_LAYERS_ALLOW",
    ] {
        if std::env::var_os(name).is_some_and(|s| !s.is_empty()) {
            return Err(format!("unsupported inherited {name}; no child launched").into());
        }
    }
    write_json(
        &session.join("inputs.json"),
        &json!({"loader": loader, "loader_sha256": loader_hash, "source_interposer": interposer, "interposer_sha256": interposer_hash, "layer_sha256": layer_hash, "host_sha256": host_hash, "staged_binary_hashes": staged_hashes, "sdk_header_commit": "e8aaa6eaac968711fb62473d4ae8256dde20919b", "parent_environment": environment, "child_layer_path": layer_paths.to_string_lossy(), "child_disable": disable, "sdk_initialized_at_launch": false, "requested_sdk_device_probe": sdk_device, "requested_sdk_route": sdk_route, "requested_fg_experiment":sdk_fg, "requested_idle_capture": idle_capture, "requested_sdk_requirements": requirements_only, "fg_enabled": false}),
    )?;
    let modes: &[&str] = if requirements_only {
        if sdk_device {
            &["sdk-device"]
        } else {
            &["requirements"]
        }
    } else {
        &["baseline", "layered"]
    };
    for &mode in modes {
        let stdout = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(session.join(format!("{mode}.stdout.log")))?;
        let stderr = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(session.join(format!("{mode}.stderr.log")))?;
        let mut command = Command::new(&executable);
        command
            .args(["--child", mode])
            .current_dir(&session)
            .stdout(stdout)
            .stderr(stderr)
            .env(
                "NS_STREAMLINE_PROBE_IDLE_CAPTURE",
                if idle_capture { "1" } else { "0" },
            )
            .env(
                "NS_STREAMLINE_PROBE_SDK_ROUTE",
                if sdk_route { "1" } else { "0" },
            )
            .env(
                "NS_STREAMLINE_PROBE_SDK_REPEAT",
                if sdk_repeat { "1" } else { "0" },
            )
            .env("NS_STREAMLINE_PROBE_FG", if sdk_fg { "1" } else { "0" })
            .env("NS_STREAMLINE_PROBE_EXE", &executable)
            .env("NS_STREAMLINE_PROBE_SESSION", &session)
            .env("NS_STREAMLINE_PROBE_DLL", &layer)
            .env("NS_STREAMLINE_PROBE_LOADER", &loader)
            .env("NS_STREAMLINE_PROBE_TRACE", session.join("layer.jsonl"))
            .env("VK_LAYER_PATH", &layer_paths)
            .env("VK_LOADER_LAYERS_DISABLE", &disable)
            .env("VK_LOADER_DEBUG", "error,warn,layer");
        let mut process = command.spawn()?;
        let started = Instant::now();
        let status = loop {
            if let Some(status) = process.try_wait()? {
                break status;
            }
            if started.elapsed() > Duration::from_secs(if sdk_route { 180 } else { 40 }) {
                process.kill()?;
                process.wait()?;
                write_json(
                    &session.join(format!("{mode}.timeout.json")),
                    &json!({"timeout_seconds": if sdk_route { 180 } else { 40 }}),
                )?;
                return Err(format!("{mode} timed out; evidence: {}", session.display()).into());
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        if !status.success() {
            return Err(
                format!("{mode} failed ({status}); evidence: {}", session.display()).into(),
            );
        }
    }
    if requirements_only {
        println!(
            "SDK probe evidence saved to {}. Inspect mode-specific results; P0 remains open.",
            session.display()
        );
        return Ok(());
    }
    let trace = fs::read_to_string(session.join("layer.jsonl"))?;
    let events: Vec<Value> = trace
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;
    crate::verify::validate_trace(&events)?;
    let baseline: Value = serde_json::from_slice(&fs::read(session.join("baseline.json"))?)?;
    let layered: Value = serde_json::from_slice(&fs::read(session.join("layered.json"))?)?;
    crate::verify::validate_runs(&baseline, &layered)?;
    write_json(
        &session.join("result.json"),
        &json!({"experiment_passed": true, "sdk_fallback_reenters_layer": true, "sdk_worker_thread_test": "host-created worker calls SDK fallback; NOT an SDK-owned FG worker", "sdk_initialized": false, "sdk_proxy_lifecycle_verified": false, "fg_enabled": false, "p0_passed": false, "trace_events": events.len()}),
    )?;
    println!("Evidence saved to {}. SDK fallback reaches this layer; SDK proxy lifecycle and P0 remain unverified.", session.display());
    Ok(())
}

struct Context {
    entry: ash::Entry,
    instance: ash::Instance,
    device: Option<ash::Device>,
    surface: vk::SurfaceKHR,
    hwnd: HWND,
    swapchain: vk::SwapchainKHR,
}
impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            if let Some(device) = &self.device {
                let _ = device.device_wait_idle();
                if self.swapchain != vk::SwapchainKHR::null() {
                    ash::khr::swapchain::Device::new(&self.instance, device)
                        .destroy_swapchain(self.swapchain, None);
                }
                device.destroy_device(None);
            }
            if self.surface != vk::SurfaceKHR::null() {
                ash::khr::surface::Instance::new(&self.entry, &self.instance)
                    .destroy_surface(self.surface, None);
            }
            self.instance.destroy_instance(None);
            if !self.hwnd.is_null() {
                DestroyWindow(self.hwnd);
            }
        }
    }
}
pub(crate) unsafe fn module_path(address: usize) -> Option<String> {
    let mut module = std::ptr::null_mut();
    if address == 0 || GetModuleHandleExW(0x4 | 0x2, address as *const u16, &mut module) == 0 {
        return None;
    }
    let mut buffer = [0u16; 32768];
    let n = GetModuleFileNameW(module, buffer.as_mut_ptr(), buffer.len() as u32);
    if n == 0 || n as usize == buffer.len() {
        None
    } else {
        Some(String::from_utf16_lossy(&buffer[..n as usize]))
    }
}
pub(crate) unsafe fn load_library(path: &Path) -> Result<Library> {
    Ok(libloading::os::windows::Library::load_with_flags(path, 0x100 | 0x800)?.into())
}
unsafe fn child(layered: bool) -> Result<()> {
    let session = PathBuf::from(
        std::env::var_os("NS_STREAMLINE_PROBE_SESSION")
            .ok_or("child must be launched by parent")?,
    );
    let expected = PathBuf::from(
        std::env::var_os("NS_STREAMLINE_PROBE_EXE").ok_or("missing expected executable")?,
    )
    .canonicalize()?;
    if expected != std::env::current_exe()?.canonicalize()? {
        return Err("executable mismatch".into());
    }
    let loader =
        PathBuf::from(std::env::var_os("NS_STREAMLINE_PROBE_LOADER").ok_or("missing loader")?);
    verify(&loader, "vulkan-1.dll")?;
    let layer = if layered {
        Some(load_library(&PathBuf::from(
            std::env::var_os("NS_STREAMLINE_PROBE_DLL").ok_or("missing layer")?,
        ))?)
    } else {
        None
    };
    let mut results = Vec::new();
    for generation in 0..2 {
        let entry = ash::Entry::load_from(&loader)?;
        let layers = if layered {
            vec![LAYER.as_ptr()]
        } else {
            vec![]
        };
        let extensions = [
            ash::khr::surface::NAME.as_ptr(),
            ash::khr::win32_surface::NAME.as_ptr(),
            ash::khr::get_surface_capabilities2::NAME.as_ptr(),
            ash::ext::surface_maintenance1::NAME.as_ptr(),
        ];
        let app = vk::ApplicationInfo::default()
            .application_name(c"NSEmu P0 probe")
            .api_version(vk::API_VERSION_1_2);
        let instance = entry.create_instance(
            &vk::InstanceCreateInfo::default()
                .application_info(&app)
                .enabled_layer_names(&layers)
                .enabled_extension_names(&extensions),
            None,
        )?;
        let mut ctx = Context {
            entry,
            instance,
            device: None,
            surface: vk::SurfaceKHR::null(),
            hwnd: std::ptr::null_mut(),
            swapchain: vk::SwapchainKHR::null(),
        };

        let set_phase = layer
            .as_ref()
            .map(|l| l.get::<Phase>(b"probeSetPhase\0").map(|f| *f))
            .transpose()?;
        let next_address = layer
            .as_ref()
            .map(|l| l.get::<Next>(b"probeNextAddress\0").map(|f| *f))
            .transpose()?;
        if let Some(phase) = set_phase {
            phase(1);
        }
        let sdk = if layered {
            let p = session.join("sl.interposer.dll");
            verify(&p, "dlss/sl.interposer.dll")?;
            Some(load_library(&p)?)
        } else {
            None
        };
        let sdk_gipa = sdk
            .as_ref()
            .map(|l| {
                l.get::<vk::PFN_vkGetInstanceProcAddr>(b"vkGetInstanceProcAddr\0")
                    .map(|f| *f)
            })
            .transpose()?;
        let sdk_gdpa = sdk
            .as_ref()
            .map(|l| {
                l.get::<vk::PFN_vkGetDeviceProcAddr>(b"vkGetDeviceProcAddr\0")
                    .map(|f| *f)
            })
            .transpose()?;
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let module = GetModuleHandleW(std::ptr::null());
        ctx.hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            320,
            240,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            module,
            std::ptr::null(),
        );
        if ctx.hwnd.is_null() {
            return Err(std::io::Error::last_os_error().into());
        }
        let surface_info = vk::Win32SurfaceCreateInfoKHR::default()
            .hinstance(module as isize)
            .hwnd(ctx.hwnd as isize);
        if let Some(gipa) = sdk_gipa {
            let ptr = gipa(ctx.instance.handle(), c"vkCreateWin32SurfaceKHR".as_ptr())
                .ok_or("SDK surface lookup failed")?;
            let create: vk::PFN_vkCreateWin32SurfaceKHR = std::mem::transmute(ptr);
            set_phase.unwrap()(2);
            let result = create(
                ctx.instance.handle(),
                &surface_info,
                std::ptr::null(),
                &mut ctx.surface,
            );
            if result != vk::Result::SUCCESS {
                return Err(format!("surface: {result:?}").into());
            }
            set_phase.unwrap()(1);
        } else {
            ctx.surface = ash::khr::win32_surface::Instance::new(&ctx.entry, &ctx.instance)
                .create_win32_surface(&surface_info, None)?;
        }
        let surface_api = ash::khr::surface::Instance::new(&ctx.entry, &ctx.instance);
        let mut selection = None;
        for physical in ctx.instance.enumerate_physical_devices()? {
            let properties = ctx.instance.get_physical_device_properties(physical);
            if properties.vendor_id != 0x10de {
                continue;
            }
            for (family, props) in ctx
                .instance
                .get_physical_device_queue_family_properties(physical)
                .iter()
                .enumerate()
            {
                if props.queue_count > 0
                    && props.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && surface_api.get_physical_device_surface_support(
                        physical,
                        family as u32,
                        ctx.surface,
                    )?
                {
                    selection = Some((
                        physical,
                        family as u32,
                        CStr::from_ptr(properties.device_name.as_ptr())
                            .to_string_lossy()
                            .into_owned(),
                    ));
                    break;
                }
            }
        }
        let (physical, family, gpu) = selection.ok_or("no NVIDIA graphics/present queue")?;
        let priorities = [1.0];
        let queues = [vk::DeviceQueueCreateInfo::default()
            .queue_family_index(family)
            .queue_priorities(&priorities)];
        let device_extensions = [
            ash::khr::swapchain::NAME.as_ptr(),
            ash::ext::swapchain_maintenance1::NAME.as_ptr(),
        ];
        let mut maintenance = vk::PhysicalDeviceSwapchainMaintenance1FeaturesEXT::default();
        ctx.instance.get_physical_device_features2(
            physical,
            &mut vk::PhysicalDeviceFeatures2::default().push_next(&mut maintenance),
        );
        if maintenance.swapchain_maintenance1 != vk::TRUE {
            return Err("presentation fences unavailable".into());
        }
        ctx.device = Some(
            ctx.instance.create_device(
                physical,
                &vk::DeviceCreateInfo::default()
                    .queue_create_infos(&queues)
                    .enabled_extension_names(&device_extensions)
                    .push_next(&mut maintenance),
                None,
            )?,
        );
        let device = ctx.device.as_ref().unwrap();
        let queue = device.get_device_queue(family, 0);
        let queue2 = device.get_device_queue2(
            &vk::DeviceQueueInfo2::default()
                .queue_family_index(family)
                .queue_index(0),
        );
        if queue != queue2 {
            return Err("queue entrypoint variants differ".into());
        }
        let mut addresses = Vec::new();
        if let Some(gdpa) = sdk_gdpa {
            for name in [
                c"vkGetDeviceQueue",
                c"vkGetDeviceQueue2",
                c"vkDeviceWaitIdle",
                c"vkCreateSwapchainKHR",
                c"vkAcquireNextImageKHR",
                c"vkAcquireNextImage2KHR",
                c"vkQueuePresentKHR",
                c"vkDestroySwapchainKHR",
            ] {
                let system = ctx
                    .instance
                    .get_device_proc_addr(device.handle(), name.as_ptr())
                    .map_or(0, |f| f as usize);
                let sdk_ptr = gdpa(device.handle(), name.as_ptr()).map_or(0, |f| f as usize);
                let next = next_address.unwrap()(device.handle().as_raw(), 1, name.as_ptr());
                addresses.push(json!({"name": name.to_str()?, "system": system, "sdk": sdk_ptr, "next": next, "sdk_equals_system": sdk_ptr == system, "sdk_equals_next": sdk_ptr == next, "system_module": module_path(system), "sdk_module": module_path(sdk_ptr), "next_module": module_path(next)}));
            }
            for name in [c"vkCreateWin32SurfaceKHR", c"vkDestroySurfaceKHR"] {
                let system = ctx
                    .entry
                    .get_instance_proc_addr(ctx.instance.handle(), name.as_ptr())
                    .map_or(0, |f| f as usize);
                let sdk_ptr = sdk_gipa.unwrap()(ctx.instance.handle(), name.as_ptr())
                    .map_or(0, |f| f as usize);
                let export = sdk
                    .as_ref()
                    .unwrap()
                    .get::<unsafe extern "system" fn()>(name.to_bytes_with_nul())
                    .map(|f| *f as usize)
                    .unwrap_or(0);
                addresses.push(json!({"name": name.to_str()?, "system": system, "sdk": sdk_ptr, "direct_sdk_export": export, "system_module": module_path(system), "sdk_module": module_path(sdk_ptr), "direct_export_module": module_path(export), "next": next_address.unwrap()(ctx.instance.handle().as_raw(), 0, name.as_ptr())}));
            }
            let get_queue: vk::PFN_vkGetDeviceQueue = std::mem::transmute(
                gdpa(device.handle(), c"vkGetDeviceQueue".as_ptr())
                    .ok_or("missing queue function")?,
            );
            set_phase.unwrap()(2);
            let mut sdk_queue = vk::Queue::null();
            get_queue(device.handle(), family, 0, &mut sdk_queue);
            if sdk_queue != queue {
                return Err("SDK fallback changed queue".into());
            }
            set_phase.unwrap()(3);
            let next_queue: vk::PFN_vkGetDeviceQueue = std::mem::transmute(next_address.unwrap()(
                device.handle().as_raw(),
                1,
                c"vkGetDeviceQueue".as_ptr(),
            ));
            let mut direct_queue = vk::Queue::null();
            next_queue(device.handle(), family, 0, &mut direct_queue);
            if direct_queue != queue {
                return Err("next-layer call changed queue".into());
            }
            let raw_device = device.handle();
            let phase = set_phase.unwrap();
            let worker_queue = std::thread::spawn(move || {
                phase(4);
                let mut out = vk::Queue::null();
                get_queue(raw_device, family, 0, &mut out);
                out
            })
            .join()
            .map_err(|_| "diagnostic worker panicked")?;
            if worker_queue != queue {
                return Err("worker fallback changed queue".into());
            }
            set_phase.unwrap()(1);
        }
        let caps = surface_api.get_physical_device_surface_capabilities(physical, ctx.surface)?;
        if !caps
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::TRANSFER_DST)
        {
            return Err("surface lacks transfer destination usage".into());
        }
        let format = surface_api
            .get_physical_device_surface_formats(physical, ctx.surface)?
            .into_iter()
            .find(|f| {
                f.format == vk::Format::B8G8R8A8_UNORM
                    && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
            })
            .ok_or("no SDR format")?;
        let extent = if caps.current_extent.width != u32::MAX {
            caps.current_extent
        } else {
            vk::Extent2D {
                width: 320u32.clamp(caps.min_image_extent.width, caps.max_image_extent.width),
                height: 240u32.clamp(caps.min_image_extent.height, caps.max_image_extent.height),
            }
        };
        if extent.width == 0 || extent.height == 0 {
            return Err("zero extent".into());
        }
        let count = if caps.max_image_count > 0 {
            (caps.min_image_count + 1).min(caps.max_image_count)
        } else {
            caps.min_image_count + 1
        };
        let alpha = [
            vk::CompositeAlphaFlagsKHR::OPAQUE,
            vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
            vk::CompositeAlphaFlagsKHR::INHERIT,
        ]
        .into_iter()
        .find(|a| caps.supported_composite_alpha.contains(*a))
        .ok_or("no alpha mode")?;
        let swapchain_api = ash::khr::swapchain::Device::new(&ctx.instance, device);
        ctx.swapchain = swapchain_api.create_swapchain(
            &vk::SwapchainCreateInfoKHR::default()
                .surface(ctx.surface)
                .min_image_count(count)
                .image_format(format.format)
                .image_color_space(format.color_space)
                .image_extent(extent)
                .image_array_layers(1)
                .image_usage(vk::ImageUsageFlags::TRANSFER_DST)
                .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
                .pre_transform(caps.current_transform)
                .composite_alpha(alpha)
                .present_mode(vk::PresentModeKHR::FIFO)
                .clipped(true),
            None,
        )?;
        present_frames(device, &swapchain_api, ctx.swapchain, queue, family)?;
        results.push(json!({"generation": generation, "gpu": gpu, "queue_family": family, "width": extent.width, "height": extent.height, "native_present_count": 3, "addresses": addresses}));
        drop(ctx);
        if let Some(layer) = &layer {
            let live = layer.get::<Live>(b"probeLiveObjects\0")?();
            if live != 0 {
                return Err(format!("dispatch maps leaked: {live}").into());
            }
        }
    }
    write_json(
        &session.join(if layered {
            "layered.json"
        } else {
            "baseline.json"
        }),
        &json!({"generations": results, "sdk_initialized": false, "fg_enabled": false, "validation_layer_enabled": false}),
    )?;
    Ok(())
}

pub(super) unsafe fn present_frames(
    device: &ash::Device,
    api: &ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    queue: vk::Queue,
    family: u32,
) -> Result<()> {
    let report = present_frames_recoverable(
        device,
        api,
        swapchain,
        queue,
        family,
        FrameFault::None,
        None,
    )?;
    if report.rebuild {
        return Err("baseline swapchain requires recreation".into());
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum FrameFault {
    None,
    AcquireOutOfDate,
    PresentOutOfDate,
}

pub(super) struct FrameReport {
    pub rebuild: bool,
    pub reason: &'static str,
    pub acquired: u32,
    pub acquire2: u32,
    pub presented: u32,
}

pub(super) fn needs_rebuild(
    result: std::result::Result<bool, vk::Result>,
) -> std::result::Result<bool, vk::Result> {
    match result {
        Ok(suboptimal) => Ok(suboptimal),
        Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => Ok(true),
        Err(error) => Err(error),
    }
}

pub(super) unsafe fn present_frames_recoverable(
    device: &ash::Device,
    api: &ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    queue: vk::Queue,
    family: u32,
    fault: FrameFault,
    downstream_completion: Option<unsafe extern "system" fn() -> u64>,
) -> Result<FrameReport> {
    let mut report = FrameReport {
        rebuild: false,
        reason: "complete",
        acquired: 0,
        acquire2: 0,
        presented: 0,
    };
    let images = api.get_swapchain_images(swapchain)?;
    // Presentation fences retire present resources; queue idle retires command resources.
    for frame in 0..3 {
        let acquired = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
        let ready = device.create_semaphore(&vk::SemaphoreCreateInfo::default(), None)?;
        let present_fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;
        let pool = device.create_command_pool(
            &vk::CommandPoolCreateInfo::default().queue_family_index(family),
            None,
        )?;
        let result = (|| -> Result<()> {
            // Inject before the Vulkan call: no image or semaphore has been acquired.
            let acquire = if frame == 0 && fault == FrameFault::AcquireOutOfDate {
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR)
            } else if frame == 1 {
                report.acquire2 += 1;
                api.acquire_next_image2(
                    &vk::AcquireNextImageInfoKHR::default()
                        .swapchain(swapchain)
                        .timeout(5_000_000_000)
                        .semaphore(acquired)
                        .device_mask(1),
                )
            } else {
                report.acquired += 1;
                api.acquire_next_image(swapchain, 5_000_000_000, acquired, vk::Fence::null())
            };
            let (index, suboptimal) = match acquire {
                Ok(value) => value,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    report.rebuild = true;
                    report.reason = "acquire_out_of_date";
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            };
            let command = device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )?[0];
            device.begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())?;
            let range = vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .level_count(1)
                .layer_count(1);
            let barrier = vk::ImageMemoryBarrier::default()
                .image(images[index as usize])
                .subresource_range(range)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
            device.cmd_clear_color_image(
                command,
                images[index as usize],
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &vk::ClearColorValue {
                    float32: [0.1, frame as f32 / 3.0, 0.3, 1.0],
                },
                &[range],
            );
            let barrier = barrier
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::empty());
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[barrier],
            );
            device.end_command_buffer(command)?;
            let waits = [acquired];
            let stages = [vk::PipelineStageFlags::TRANSFER];
            let buffers = [command];
            let signals = [ready];
            device.queue_submit(
                queue,
                &[vk::SubmitInfo::default()
                    .wait_semaphores(&waits)
                    .wait_dst_stage_mask(&stages)
                    .command_buffers(&buffers)
                    .signal_semaphores(&signals)],
                vk::Fence::null(),
            )?;
            let chains = [swapchain];
            let indices = [index];
            let fences = [present_fence];
            let mut fence_info = vk::SwapchainPresentFenceInfoEXT::default().fences(&fences);
            let before = downstream_completion.map(|read| read());
            let mut present_info = vk::PresentInfoKHR::default()
                .wait_semaphores(&signals)
                .swapchains(&chains)
                .image_indices(&indices);
            if downstream_completion.is_none() {
                present_info = present_info.push_next(&mut fence_info);
            }
            let present = api.queue_present(queue, &present_info);
            report.presented += 1;
            // Always retire submitted work, including recoverable presentation failure.
            // Never recycle a signaled semaphore for a replacement swapchain.
            let injected = frame == 0 && fault == FrameFault::PresentOutOfDate && present.is_ok();
            let present_rebuild = needs_rebuild(if injected {
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR)
            } else {
                present
            })?;
            if let Some(read) = downstream_completion {
                if Some(read()) != before.map(|n| n + 1) {
                    return Err(
                        "SDK present did not retire synchronously on the calling thread".into(),
                    );
                }
            } else {
                device.wait_for_fences(&fences, true, 5_000_000_000)?;
            }
            device.queue_wait_idle(queue)?;
            report.rebuild = suboptimal || present_rebuild || injected;
            report.reason = if injected {
                "injected_present_out_of_date"
            } else if present_rebuild {
                "present_out_of_date_or_suboptimal"
            } else if suboptimal {
                "acquire_suboptimal"
            } else {
                "complete"
            };
            Ok(())
        })();
        // On failure the isolated child exits. Do not destroy potentially in-flight objects.
        result?;
        device.destroy_fence(present_fence, None);
        device.destroy_command_pool(pool, None);
        device.destroy_semaphore(ready, None);
        device.destroy_semaphore(acquired, None);
        if report.rebuild {
            break;
        }
    }
    Ok(report)
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    #[test]
    fn recreation_does_not_hide_device_loss_or_surface_loss() {
        assert_eq!(needs_rebuild(Ok(false)), Ok(false));
        assert_eq!(needs_rebuild(Ok(true)), Ok(true));
        assert_eq!(
            needs_rebuild(Err(vk::Result::ERROR_OUT_OF_DATE_KHR)),
            Ok(true)
        );
        for error in [
            vk::Result::ERROR_DEVICE_LOST,
            vk::Result::ERROR_SURFACE_LOST_KHR,
            vk::Result::TIMEOUT,
        ] {
            assert_eq!(needs_rebuild(Err(error)), Err(error));
        }
    }
}
