//! P0 host/child isolation. All environment changes apply only to the child.
use crate::{
    nr_abi::*,
    nr_api::{self, Result},
};
use ash::vk::{self, Handle};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, c_void, CStr},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleFileNameW, GetModuleHandleW};

static SESSION: OnceLock<PathBuf> = OnceLock::new();
static LOG_LOCK: Mutex<()> = Mutex::new(());
static LOG_FAILED: AtomicBool = AtomicBool::new(false);
static LOG_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Options {
    pub session: PathBuf,
    pub runtime: PathBuf,
    pub runtime_sha256: String,
    pub bridge: Option<PathBuf>,
    pub bridge_sha256: Option<String>,
    pub validation_dir: Option<PathBuf>,
    pub without_validation: bool,
    pub init_only: bool,
    #[serde(default)]
    pub trace_layouts: bool,
    #[serde(default)]
    pub repair_internal_layouts: bool,
    #[serde(default)]
    pub resize: bool,
    #[serde(default)]
    pub coexist_sr: Option<PathBuf>,
    #[serde(default)]
    pub sr_control: bool,
    #[serde(default)]
    pub repair_sr_resources: bool,
    #[serde(default)]
    pub pause_without_motion: bool,
    #[serde(default)]
    pub record_on_worker: bool,
    #[serde(default)]
    pub two_pass: bool,
    pub frames: u32,
    pub width: u32,
    pub height: u32,
}

pub(super) fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn verify(path: &Path, expected: &str) -> Result<()> {
    if expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("expected SHA256 must contain 64 hexadecimal digits".into());
    }
    if hash(path)? != expected.to_ascii_lowercase() {
        return Err(format!("SHA256 mismatch: {}", path.display()).into());
    }
    let bytes = fs::read(path)?;
    verify_pe(&bytes)
}
fn verify_pe(bytes: &[u8]) -> Result<()> {
    let get_u32 = |at: usize| -> Result<u32> {
        Ok(u32::from_le_bytes(
            bytes.get(at..at + 4).ok_or("truncated PE")?.try_into()?,
        ))
    };
    if bytes.get(..2) != Some(b"MZ") {
        return Err("runtime is not a PE image".into());
    }
    let at = get_u32(0x3c)? as usize;
    if bytes.get(at..at + 4) != Some(b"PE\0\0") || bytes.get(at + 4..at + 6) != Some(&[0x64, 0x86])
    {
        return Err("runtime must be a Windows x64 PE".into());
    }
    let flags = u16::from_le_bytes(
        bytes
            .get(at + 22..at + 24)
            .ok_or("truncated COFF header")?
            .try_into()?,
    );
    if flags & 0x2000 == 0 || bytes.get(at + 24..at + 26) != Some(&[0x0b, 0x02]) {
        return Err("runtime must be a PE32+ DLL".into());
    }
    Ok(())
}
pub(super) fn write_json(path: &Path, value: &Value) -> Result<()> {
    let mut file = OpenOptions::new().create_new(true).write(true).open(path)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    writeln!(file)?;
    Ok(())
}
fn append(name: &str, mut value: Value) -> Result<()> {
    let _guard = LOG_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    value["sequence"] = json!(LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed));
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(SESSION.get().ok_or("missing session")?.join(name))?;
    serde_json::to_writer(&mut file, &value)?;
    writeln!(file)?;
    file.flush()?;
    Ok(())
}
pub(super) fn event(stage: &str, details: Value) -> Result<()> {
    append("nr-events.jsonl", json!({"stage":stage,"details":details}))
}
pub(super) fn ngx_result(stage: &str, result: u32) -> Result<()> {
    event(
        stage,
        json!({"result":result,"result_hex":format!("0x{result:08x}")}),
    )?;
    nr_api::checked(result, stage)
}

unsafe fn module_evidence(name: &str) -> Result<Value> {
    let wide: Vec<_> = name.encode_utf16().chain(Some(0)).collect();
    let module = GetModuleHandleW(wide.as_ptr());
    if module.is_null() {
        return Ok(json!({"name":name,"loaded":false}));
    }
    let mut path = vec![0; 32768];
    let n = GetModuleFileNameW(module, path.as_mut_ptr(), path.len() as u32);
    if n == 0 || n as usize >= path.len() {
        return Err("cannot identify loaded NGX module".into());
    }
    let path = PathBuf::from(String::from_utf16(&path[..n as usize])?);
    Ok(json!({"name":name,"loaded":true,"path":path,"sha256":hash(&path)?}))
}
unsafe extern "C" fn ngx_log(message: *const c_char, level: u32, feature: u32) {
    if message.is_null() {
        return;
    }
    if append("ngx.jsonl", json!({"message":CStr::from_ptr(message).to_string_lossy(),"level":level,"feature":feature})).is_err() {
        LOG_FAILED.store(true, Ordering::Relaxed);
    }
}
unsafe extern "system" fn validation_log(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    kind: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    if data.is_null() || (*data).p_message.is_null() {
        return vk::FALSE;
    }
    if append("validation.jsonl", json!({"severity":severity.as_raw(),"type":kind.as_raw(),
        "id_number":(*data).message_id_number,"message":CStr::from_ptr((*data).p_message).to_string_lossy()})).is_err() {
        LOG_FAILED.store(true, Ordering::Relaxed);
    }
    vk::FALSE
}

pub fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|a| a == "--child-nr") {
        if args.len() != 2 {
            return Err("invalid NR child arguments".into());
        }
        let options: Options =
            serde_json::from_slice(&fs::read(PathBuf::from(&args[1]).join("nr-options.json"))?)?;
        SESSION
            .set(options.session.clone())
            .map_err(|_| "duplicate child session")?;
        if let Err(error) = unsafe { child(&options) } {
            let _ = write_json(
                &options.session.join("nr-failure.json"),
                &json!({"error":error.to_string(),"p0_passed":false}),
            );
            eprintln!("NR child failed: {error}");
            // No unwind through DLL/device destruction on unknown GPU state.
            std::process::abort();
        }
        return Ok(());
    }
    if args.len() == 1 && args[0] == "--help" {
        println!(concat!(
            "streamline-nr-diagnostics --runtime <absolute DLL> --sha256 <digest> --session <new absolute directory>\n",
            "  [--bridge <absolute nvngx.dll> --bridge-sha256 <digest>] [--validation-dir <absolute layer directory>]\n",
            "  [--without-validation] [--init-only] [--trace-layouts] [--repair-internal-layouts]\n",
            "  [--resize] [--coexist-sr <absolute pinned runtime directory>] [--repair-sr-resources] [--sr-control]\n",
            "  [--pause-without-motion] [--record-on-worker] [--frames <300..10000>] [--width <64..1920>] [--height <64..1080>]\n",
            "  [--two-pass] validates independent NR maps/handles, fenced chaining and discarded second recordings.\n",
            "Default: core and synchronization validation required, 300 frames per intensity, 640x360 SDR. Child-only layer environment; files are staged only into a new session.\n",
            "--trace-layouts requires --validation-dir, enables api-dump.txt, and limits each NR cycle to 3 frames.\n",
            "--repair-internal-layouts inserts initial barriers for matching fresh NR-owned images; default off and limited to inspected runtime hashes.\n",
            "--resize adds 960x540, 1280x720 and original-size batches after fence completion. Resize/coexistence require validation and --repair-internal-layouts.\n",
            "--coexist-sr requires the nr-coexistence build feature. SR runs before NR evaluation, with each live NR feature and after NR snippet shutdown.\n",
            "--repair-sr-resources is a separate pinned-runtime experiment for fresh SR motion images and exposure-clear dependencies.\n",
            "--sr-control evaluates SR without initializing NR; requires --coexist-sr and excludes resize/init-only.\n",
            "--pause-without-motion copies the input when motion is unavailable and resets NR on recovery. Default mode explores per-frame reset with zero motion.\n",
            "--record-on-worker tests serialized NR-only recording on a scoped worker; excludes coexistence/init-only.\n",
            "Fence deadline: 10 seconds. Child deadline: 240 seconds, or 600 seconds for resize/coexistence.\n",
            "--without-validation is exploratory and can never pass P0. Output review and bridge audit are external acceptance gates; no mode automatically accepts P0 or game compatibility."
        ));
        return Ok(());
    }
    let mut options = Options {
        session: PathBuf::new(),
        runtime: PathBuf::new(),
        runtime_sha256: String::new(),
        bridge: None,
        bridge_sha256: None,
        validation_dir: None,
        without_validation: false,
        init_only: false,
        trace_layouts: false,
        repair_internal_layouts: false,
        resize: false,
        coexist_sr: None,
        sr_control: false,
        repair_sr_resources: false,
        pause_without_motion: false,
        record_on_worker: false,
        two_pass: false,
        frames: 300,
        width: 640,
        height: 360,
    };
    let mut seen = std::collections::BTreeSet::new();
    let mut iter = args.iter();
    while let Some(flag) = iter.next() {
        let flag = flag.to_str().ok_or("non-UTF8 option")?;
        if !seen.insert(flag) {
            return Err(format!("duplicate option {flag}").into());
        }
        if flag == "--without-validation" {
            options.without_validation = true;
            continue;
        }
        if flag == "--init-only" {
            options.init_only = true;
            continue;
        }
        if flag == "--trace-layouts" {
            options.trace_layouts = true;
            continue;
        }
        if flag == "--repair-internal-layouts" {
            options.repair_internal_layouts = true;
            continue;
        }
        if flag == "--resize" {
            options.resize = true;
            continue;
        }
        if flag == "--sr-control" {
            options.sr_control = true;
            continue;
        }
        if flag == "--repair-sr-resources" {
            options.repair_sr_resources = true;
            continue;
        }
        if flag == "--pause-without-motion" {
            options.pause_without_motion = true;
            continue;
        }
        if flag == "--record-on-worker" {
            options.record_on_worker = true;
            continue;
        }
        if flag == "--two-pass" {
            options.two_pass = true;
            continue;
        }
        let value = iter.next().ok_or("missing option value")?;
        match flag {
            "--runtime" => options.runtime = value.into(),
            "--session" => options.session = value.into(),
            "--sha256" => options.runtime_sha256 = value.to_string_lossy().into_owned(),
            "--bridge" => options.bridge = Some(value.into()),
            "--bridge-sha256" => options.bridge_sha256 = Some(value.to_string_lossy().into_owned()),
            "--validation-dir" => options.validation_dir = Some(value.into()),
            "--coexist-sr" => options.coexist_sr = Some(value.into()),
            "--frames" => options.frames = value.to_str().ok_or("invalid frames")?.parse()?,
            "--width" => options.width = value.to_str().ok_or("invalid width")?.parse()?,
            "--height" => options.height = value.to_str().ok_or("invalid height")?.parse()?,
            _ => return Err(format!("unknown option {flag} (use --help)").into()),
        }
    }
    if !(300..=10000).contains(&options.frames)
        || !(64..=1920).contains(&options.width)
        || !(64..=1080).contains(&options.height)
    {
        return Err("frames or dimensions outside diagnostic bounds".into());
    }
    for path in [&options.session, &options.runtime]
        .into_iter()
        .chain(options.bridge.iter())
        .chain(options.validation_dir.iter())
        .chain(options.coexist_sr.iter())
    {
        if !path.is_absolute() {
            return Err("all paths must be absolute".into());
        }
    }
    verify(&options.runtime, &options.runtime_sha256)?;
    if options.two_pass
        && (options.init_only
            || options.resize
            || options.coexist_sr.is_some()
            || options.pause_without_motion
            || options.trace_layouts
            || options.without_validation
            || !options.repair_internal_layouts)
    {
        return Err("--two-pass requires isolated validated NR execution and --repair-internal-layouts; excludes init-only, resize, SR, pause and layout trace modes".into());
    }
    if options.record_on_worker && (options.coexist_sr.is_some() || options.init_only) {
        return Err("worker recording test requires NR-only GPU execution".into());
    }
    if options.repair_sr_resources && options.coexist_sr.is_none() {
        return Err("--repair-sr-resources requires the pinned --coexist-sr runtime".into());
    }
    if options.sr_control && (options.coexist_sr.is_none() || options.resize || options.init_only) {
        return Err("--sr-control requires --coexist-sr and excludes NR resize/init-only".into());
    }
    if (options.resize || options.coexist_sr.is_some())
        && (options.init_only || options.without_validation || !options.repair_internal_layouts)
    {
        return Err(
            "resize/coexistence require GPU validation and --repair-internal-layouts".into(),
        );
    }
    #[cfg(not(feature = "nr-coexistence"))]
    if options.coexist_sr.is_some() {
        return Err("--coexist-sr requires the nr-coexistence build feature".into());
    }
    if options.repair_internal_layouts && (options.without_validation || options.init_only) {
        return Err("--repair-internal-layouts requires GPU validation".into());
    }
    if options.repair_internal_layouts
        && ![
            "e16bcf15e16e13f527491cdf7845b2fe6521a738d8f7c9c721866a8496e1fc8e",
            "4b8d19bc3eff58a084f5eca7489c921501c203450169fb82ff4f649a4482ba05",
        ]
        .contains(&options.runtime_sha256.to_ascii_lowercase().as_str())
    {
        return Err(
            "internal layout experiment is limited to the two inspected NR runtime digests".into(),
        );
    }
    if options.trace_layouts
        && (options.validation_dir.is_none() || options.without_validation || options.init_only)
    {
        return Err("--trace-layouts requires --validation-dir and GPU validation".into());
    }
    match (&options.bridge, &options.bridge_sha256) {
        (Some(path), Some(digest)) => {
            if path.file_name().and_then(|s| s.to_str()) != Some("nvngx.dll") {
                return Err("caller bridge must be named nvngx.dll".into());
            }
            verify(path, digest)?;
        }
        (None, None) => {}
        _ => return Err("--bridge and --bridge-sha256 must be supplied together".into()),
    }
    let source_runtime = options.runtime.clone();
    let source_bridge = options.bridge.clone();
    fs::create_dir(&options.session)?;
    options.session = dunce::canonicalize(&options.session)?;
    let staged = options.session.join("nvngx_dlssnr.dll");
    fs::copy(&source_runtime, &staged)?;
    verify(&staged, &options.runtime_sha256)?;
    options.runtime = staged;
    let coexist_files = if let Some(source) = &options.coexist_sr {
        #[cfg(feature = "nr-coexistence")]
        {
            let staged = options.session.join("streamline");
            let files = crate::sdk_nr_coexist::stage(source, &staged)?;
            options.coexist_sr = Some(staged);
            files
        }
        #[cfg(not(feature = "nr-coexistence"))]
        {
            let _ = source;
            return Err("nr-coexistence feature missing".into());
        }
    } else {
        Value::Null
    };
    if let Some(source) = &source_bridge {
        let staged = options.session.join("nvngx.dll");
        fs::copy(source, &staged)?;
        verify(&staged, options.bridge_sha256.as_ref().unwrap())?;
        options.bridge = Some(staged);
    }
    let validation_files = if let Some(dir) = &options.validation_dir {
        json!({"manifest_sha256":hash(&dir.join("VkLayer_khronos_validation.json"))?,
            "dll_sha256":hash(&dir.join("VkLayer_khronos_validation.dll"))?})
    } else {
        Value::Null
    };
    let trace_files = if options.trace_layouts {
        let dir = options.validation_dir.as_ref().unwrap();
        json!({"manifest_sha256":hash(&dir.join("VkLayer_api_dump.json"))?,
            "dll_sha256":hash(&dir.join("VkLayer_api_dump.dll"))?})
    } else {
        Value::Null
    };
    write_json(
        &options.session.join("nr-options.json"),
        &serde_json::to_value(&options)?,
    )?;
    write_json(
        &options.session.join("nr-inputs.json"),
        &json!({"host_sha256":hash(&std::env::current_exe()?)?,
        "source_runtime":source_runtime,"source_bridge":source_bridge,"validation_files":validation_files,"trace_files":trace_files,"coexist_files":coexist_files,
        "contract":serde_json::from_str::<Value>(include_str!("../../sdk/nr-contract.json"))?,
        "nr_requested":!options.sr_control,"sr_requested":options.coexist_sr.is_some(),"fg_requested":false,
        "depth":"synthetic_constant","motion":"synthetic_uv_current_to_previous","color":"gamma_encoded_rgba16f"}),
    )?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args(["--child-nr".as_ref(), options.session.as_os_str()])
        .current_dir(&options.session)
        .stdin(Stdio::null())
        .stdout(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(options.session.join("nr.stdout.log"))?,
        )
        .stderr(
            OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(options.session.join("nr.stderr.log"))?,
        )
        .env("VK_LOADER_LAYERS_DISABLE", "~implicit~")
        .env("VK_LOADER_DEBUG", "error,warn,layer");
    for name in [
        "VK_INSTANCE_LAYERS",
        "VK_LOADER_LAYERS_ENABLE",
        "VK_LOADER_LAYERS_ALLOW",
        "VK_ADD_LAYER_PATH",
        "VK_LAYER_SETTINGS_PATH",
    ] {
        command.env_remove(name);
    }
    if let Some(dir) = &options.validation_dir {
        command.env("VK_LAYER_PATH", dir);
    }
    if options.trace_layouts {
        // Pin settings inside this child; never dump shader binaries. This is a
        // short layout reproducer, not a continuous-output acceptance run.
        fs::write(options.session.join("vk_layer_settings.txt"),
            "lunarg_api_dump.file = true\nlunarg_api_dump.log_filename = api-dump.txt\nlunarg_api_dump.output_format = text\nlunarg_api_dump.detailed = true\nlunarg_api_dump.show_shader = false\nlunarg_api_dump.flush = true\n")?;
        command
            .env("VK_LAYER_SETTINGS_PATH", &options.session)
            .env("VK_APIDUMP_LOG_FILENAME", "api-dump.txt")
            .env("VK_APIDUMP_OUTPUT_FORMAT", "text")
            .env("VK_APIDUMP_OUTPUT_RANGE", "0-0")
            .env("VK_APIDUMP_DETAILED", "true")
            .env("VK_APIDUMP_NO_ADDR", "false");
    }
    let timeout = if options.resize || options.coexist_sr.is_some() {
        600
    } else {
        240
    };
    let mut child = command.spawn()?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(timeout) {
            child.kill()?;
            child.wait()?;
            write_json(
                &options.session.join("nr-process.json"),
                &json!({"timeout":true,"timeout_seconds":timeout,"p0_passed":false}),
            )?;
            return Err(format!("NR child timed out; {}", options.session.display()).into());
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let last_event = fs::read_to_string(options.session.join("nr-events.jsonl"))
        .ok()
        .and_then(|log| {
            log.lines()
                .last()
                .and_then(|line| serde_json::from_str::<Value>(line).ok())
        });
    if !status.success() && !options.session.join("nr-failure.json").exists() {
        write_json(
            &options.session.join("nr-failure.json"),
            &json!({
            "error":"NR child terminated before returning an error (possible native exception); no cleanup or safe fallback inferred",
            "exit_code":status.code(),"last_event":last_event,"p0_passed":false}),
        )?;
    }
    write_json(
        &options.session.join("nr-process.json"),
        &json!({"exit_code":status.code(),"success":status.success(),"elapsed_seconds":start.elapsed().as_secs_f64(),
            "last_event":last_event,"p0_passed":false}),
    )?;
    println!("NR evidence: {}", options.session.display());
    if !status.success() {
        return Err("NR child failed; inspect nr-events.jsonl and nr-failure.json".into());
    }
    Ok(())
}

unsafe fn child(options: &Options) -> Result<()> {
    verify(&options.runtime, &options.runtime_sha256)?;
    if let (Some(path), Some(digest)) = (&options.bridge, &options.bridge_sha256) {
        verify(path, digest)?;
    }
    event("load_snippet", json!({"path":options.runtime}))?;
    // Retain the DLL until explicit, quiescent shutdown. On any error below,
    // abort before this object can be dropped and unload pending NR code.
    let mut api = nr_api::Api::load(&options.runtime)?;
    let result = device_and_ngx(options, &mut api);
    if let Err(error) = result {
        let _ = write_json(
            &options.session.join("nr-failure.json"),
            &json!({"error":error.to_string(),"p0_passed":false}),
        );
        eprintln!("NR failed: {error}");
        std::process::abort();
    }
    Ok(())
}

unsafe fn exercise_recording(
    options: &Options,
    api: &nr_api::Api,
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    family: u32,
    params: *mut c_void,
    after_frame: Option<&mut dyn FnMut(u32, u32) -> Result<()>>,
) -> Result<Value> {
    if !options.record_on_worker {
        return crate::sdk_nr_gpu::exercise(
            options,
            api,
            instance,
            physical,
            device,
            family,
            params,
            after_frame,
        );
    }
    event(
        "worker_recording_enter",
        json!({"initializer_thread":format!("{:?}",std::thread::current().id()),"serialized":true}),
    )?;
    // The parent performs no concurrent API/map access and joins before any
    // cleanup or next feature operation. Loader, Api and Vulkan objects remain
    // borrowed and live for the scope. Do not declare general NGX Send/Sync.
    let address = api as *const nr_api::Api as usize;
    let parameters = params as usize;
    let result = std::thread::scope(|scope| {
        scope
            .spawn(move || {
                let result = crate::sdk_nr_gpu::exercise(
                    options,
                    &*(address as *const nr_api::Api),
                    instance,
                    physical,
                    device,
                    family,
                    parameters as *mut c_void,
                    None,
                );
                result.map_err(|error| error.to_string())
            })
            .join()
    })
    .map_err(|_| "NR recording worker panicked")?
    .map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
    event(
        "worker_recording_complete",
        json!({"joined":true,"all_fences_completed":true}),
    )?;
    Ok(result)
}
unsafe fn device_and_ngx(options: &Options, api: &mut nr_api::Api) -> Result<()> {
    let loader = PathBuf::from(std::env::var_os("SystemRoot").ok_or("missing SystemRoot")?)
        .join("System32/vulkan-1.dll");
    event(
        "vulkan_loader",
        json!({"path":loader,"sha256":hash(&loader)?,"exports":nr_api::export_names(),
        "snippet_addresses":(0..6).map(|i| api.address(i) as usize).collect::<Vec<_>>()}),
    )?;
    // Keep the loader resident on every uncertain/error path until the child
    // aborts. Only unload it after explicit Vulkan and NGX shutdown.
    let mut entry = std::mem::ManuallyDrop::new(ash::Entry::load_from(&loader)?);
    #[cfg(feature = "nr-coexistence")]
    let mut consumer = options
        .coexist_sr
        .as_ref()
        .map(|path| {
            crate::sdk_nr_coexist::Consumer::initialize(path).map(std::mem::ManuallyDrop::new)
        })
        .transpose()?;
    let available_layers = entry.enumerate_instance_layer_properties()?;
    let layer = available_layers
        .iter()
        .find(|p| CStr::from_ptr(p.layer_name.as_ptr()) == c"VK_LAYER_KHRONOS_validation");
    event(
        "validation_availability",
        json!({"available":layer.is_some(),"enabled":!options.without_validation,
        "spec_version":layer.map(|p| p.spec_version),"implementation_version":layer.map(|p| p.implementation_version),
        "core_validation":!options.without_validation,"synchronization_validation":!options.without_validation}),
    )?;
    if !options.without_validation && layer.is_none() {
        return Err("VK_LAYER_KHRONOS_validation unavailable; P0 incomplete".into());
    }
    let mut instance_extensions = Vec::new();
    instance_extensions.push(ash::ext::debug_utils::NAME.to_owned());
    #[cfg(feature = "nr-coexistence")]
    if let Some(consumer) = &consumer {
        instance_extensions.extend(consumer.instance_extensions.iter().cloned());
    }
    let ext: Vec<_> = instance_extensions.iter().map(|s| s.as_ptr()).collect();
    let mut layers = vec![c"VK_LAYER_KHRONOS_validation".as_ptr()];
    if options.trace_layouts {
        if !available_layers
            .iter()
            .any(|p| CStr::from_ptr(p.layer_name.as_ptr()) == c"VK_LAYER_LUNARG_api_dump")
        {
            return Err("VK_LAYER_LUNARG_api_dump unavailable".into());
        }
        // Observe application-visible handles above validation's handle wrapping.
        layers.insert(0, c"VK_LAYER_LUNARG_api_dump".as_ptr());
    }
    let enables = [vk::ValidationFeatureEnableEXT::SYNCHRONIZATION_VALIDATION];
    let mut validation = vk::ValidationFeaturesEXT::default().enabled_validation_features(&enables);
    let mut debug = vk::DebugUtilsMessengerCreateInfoEXT::default()
        .message_severity(
            vk::DebugUtilsMessageSeverityFlagsEXT::VERBOSE
                | vk::DebugUtilsMessageSeverityFlagsEXT::INFO
                | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                | vk::DebugUtilsMessageSeverityFlagsEXT::ERROR,
        )
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .pfn_user_callback(Some(validation_log));
    let application = vk::ApplicationInfo::default()
        .api_version(vk::API_VERSION_1_3)
        .application_name(c"NSEmu isolated Vulkan NR P0");
    let mut create = vk::InstanceCreateInfo::default()
        .application_info(&application)
        .enabled_extension_names(&ext)
        .push_next(&mut debug);
    if !options.without_validation {
        create = create
            .enabled_layer_names(&layers)
            .push_next(&mut validation);
    }
    event("create_instance", json!({"api_version":"1.3"}))?;
    let instance = entry.create_instance(&create, None)?;
    let utils = ash::ext::debug_utils::Instance::new(&entry, &instance);
    let messenger = utils.create_debug_utils_messenger(&debug, None)?;
    let physical = instance
        .enumerate_physical_devices()?
        .into_iter()
        .find(|&p| {
            let props = instance.get_physical_device_properties(p);
            props.vendor_id == 0x10de && props.api_version >= vk::API_VERSION_1_3
        })
        .ok_or("no Vulkan 1.3 NVIDIA GPU")?;
    let mut driver = vk::PhysicalDeviceDriverProperties::default();
    let mut identity = vk::PhysicalDeviceIDProperties::default();
    let mut props = vk::PhysicalDeviceProperties2::default()
        .push_next(&mut driver)
        .push_next(&mut identity);
    instance.get_physical_device_properties2(physical, &mut props);
    let mut extensions = Vec::new();
    for name in [
        c"VK_NVX_binary_import",
        c"VK_NVX_image_view_handle",
        c"VK_KHR_push_descriptor",
    ] {
        extensions.push(name.to_owned());
    }
    #[cfg(feature = "nr-coexistence")]
    if let Some(consumer) = &consumer {
        for name in &consumer.device_extensions {
            if !extensions.contains(name) {
                extensions.push(name.clone());
            }
        }
    }
    let supported_ext = instance.enumerate_device_extension_properties(physical)?;
    let missing: Vec<_> = extensions
        .iter()
        .filter(|name| {
            !supported_ext
                .iter()
                .any(|p| CStr::from_ptr(p.extension_name.as_ptr()) == name.as_c_str())
        })
        .map(|s| s.to_string_lossy())
        .collect();
    let mut features12 = vk::PhysicalDeviceVulkan12Features::default();
    let mut features13 = vk::PhysicalDeviceVulkan13Features::default();
    let mut features = vk::PhysicalDeviceFeatures2::default()
        .push_next(&mut features12)
        .push_next(&mut features13);
    instance.get_physical_device_features2(physical, &mut features);
    event(
        "device_capabilities",
        json!({"gpu":CStr::from_ptr(props.properties.device_name.as_ptr()).to_string_lossy(),
        "vendor_id":props.properties.vendor_id,"device_id":props.properties.device_id,"api_version":props.properties.api_version,
        "driver_version":props.properties.driver_version,"driver_name":CStr::from_ptr(driver.driver_name.as_ptr()).to_string_lossy(),
        "driver_info":CStr::from_ptr(driver.driver_info.as_ptr()).to_string_lossy(),"device_uuid":identity.device_uuid,
        "luid":identity.device_luid,"luid_valid":identity.device_luid_valid == vk::TRUE,
        "missing_extensions":missing,"buffer_device_address":features12.buffer_device_address == vk::TRUE,
        "maintenance4":features13.maintenance4 == vk::TRUE,"maintenance4_source":"Vulkan 1.3 core",
        "available_extensions":supported_ext.iter().map(|p| CStr::from_ptr(p.extension_name.as_ptr()).to_string_lossy().into_owned()).collect::<Vec<_>>()}),
    )?;
    if !missing.is_empty()
        || features12.buffer_device_address != vk::TRUE
        || features13.maintenance4 != vk::TRUE
    {
        return Err(
            "NR candidate device requirements not met (requirements remain experimental)".into(),
        );
    }
    let family = instance
        .get_physical_device_queue_family_properties(physical)
        .iter()
        .position(|q| {
            q.queue_count > 0
                && q.queue_flags
                    .contains(vk::QueueFlags::GRAPHICS | vk::QueueFlags::COMPUTE)
        })
        .ok_or("no graphics/compute queue")? as u32;
    let priorities = [1.0];
    let queues = [vk::DeviceQueueCreateInfo::default()
        .queue_family_index(family)
        .queue_priorities(&priorities)];
    let ext: Vec<_> = extensions.iter().map(|s| s.as_ptr()).collect();
    let mut enabled12 = vk::PhysicalDeviceVulkan12Features::default().buffer_device_address(true);
    #[cfg(feature = "nr-coexistence")]
    if let Some(consumer) = &consumer {
        consumer.enable_features(&features12, &mut enabled12)?;
    }
    let mut enabled13 = vk::PhysicalDeviceVulkan13Features::default().maintenance4(true);
    if options.coexist_sr.is_some() {
        if features13.private_data != vk::TRUE {
            return Err("Streamline Vulkan backend requires privateData".into());
        }
        enabled13.private_data = vk::TRUE;
    }
    event(
        "create_device",
        json!({"family":family,"extensions":extensions.iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>()}),
    )?;
    let device = instance.create_device(
        physical,
        &vk::DeviceCreateInfo::default()
            .queue_create_infos(&queues)
            .enabled_extension_names(&ext)
            .push_next(&mut enabled12)
            .push_next(&mut enabled13),
        None,
    )?;
    if options.repair_internal_layouts {
        crate::nr_layout::install(
            entry.static_fn().get_instance_proc_addr,
            instance.fp_v1_0().get_device_proc_addr,
            &device,
            event,
        )?;
    }
    #[cfg(feature = "nr-coexistence")]
    if let Some(consumer) = &consumer {
        consumer.attach(
            &entry,
            &instance,
            physical,
            &device,
            family,
            options.repair_sr_resources,
        )?;
    }
    #[cfg(feature = "nr-coexistence")]
    if options.sr_control {
        let sr_consumer = consumer.as_ref().ok_or("SR control consumer missing")?;
        sr_consumer.exercise(
            &options.session,
            "sr_control",
            &instance,
            physical,
            &device,
            family,
        )?;
        sr_consumer.shutdown()?;
        device.destroy_device(None);
        utils.destroy_debug_utils_messenger(messenger, None);
        instance.destroy_instance(None);
        std::mem::ManuallyDrop::drop(&mut entry);
        if let Some(consumer) = &mut consumer {
            std::mem::ManuallyDrop::drop(consumer);
        }
        let ValidationSummary {
            errors,
            warnings,
            unreviewed,
            reviews,
        } = validation_summary(&options.session)?;
        write_json(
            &options.session.join("nr-result.json"),
            &json!({"sr_control_only":true,"nr_initialized":false,"nr_evaluations":0,"validation_errors":errors,"validation_warnings":warnings,"unreviewed_warnings":unreviewed,"warning_reviews":reviews,"p0_passed":false}),
        )?;
        if errors != 0 || unreviewed != 0 {
            return Err("SR-only control has validation errors".into());
        }
        return Ok(());
    }
    let gipa = entry.static_fn().get_instance_proc_addr as Address;
    let gdpa = instance.fp_v1_0().get_device_proc_addr as Address;
    let data: Vec<_> = options
        .session
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let common = nr_api::CommonInfo {
        paths: std::ptr::null(),
        path_count: 0,
        internal: std::ptr::null_mut(),
        logging: nr_api::LoggingInfo {
            callback: ngx_log,
            minimum_level: 2,
            disable_other_sinks: 1,
            padding: [0; 3],
        },
    };
    let device_handle = device.handle().as_raw() as VkHandle;
    event(
        "core_init_enter",
        json!({"gipa":gipa as usize,"gdpa":gdpa as usize}),
    )?;
    ngx_result(
        "core_init",
        nr_api::NVSDK_NGX_VULKAN_Init_with_ProjectID(
            c"9b633af0-b7c8-4ebe-b3a1-40e6b9d34c1b".as_ptr(),
            0,
            c"0.1.0".as_ptr(),
            data.as_ptr(),
            instance.handle().as_raw() as VkHandle,
            physical.as_raw() as VkHandle,
            device_handle,
            gipa,
            gdpa,
            &common,
            NGX_API_VERSION,
        ),
    )?;
    event(
        "core_modules",
        json!({"modules":[module_evidence("_nvngx.dll")?,
        module_evidence("nvngx.dll")?,module_evidence("nvoglv64.dll")?,module_evidence("nvapi64.dll")?]}),
    )?;
    let mut capabilities = std::ptr::null_mut();
    ngx_result(
        "capability_parameters",
        nr_api::NVSDK_NGX_VULKAN_GetCapabilityParameters(&mut capabilities),
    )?;
    if capabilities.is_null() {
        return Err("successful capability query returned null".into());
    }
    if let Some(bridge) = &options.bridge {
        event(
            "load_caller_bridge",
            json!({"path":bridge,"sha256":options.bridge_sha256}),
        )?;
        api.load_bridge(bridge)?;
    }
    let (snippet_gipa, snippet_gdpa) = if options.repair_internal_layouts {
        (
            crate::nr_layout::gipa as Address,
            crate::nr_layout::gdpa as Address,
        )
    } else {
        (gipa, gdpa)
    };
    let args = InitArgs {
        application_id: 0x1122334455667788,
        data_path: data.as_ptr(),
        instance: instance.handle().as_raw() as VkHandle,
        physical: physical.as_raw() as VkHandle,
        device: device_handle,
        gipa: snippet_gipa,
        gdpa: snippet_gdpa,
        api_version: NGX_API_VERSION,
        parameters: capabilities,
    };
    let mut call = nr_api::empty_call(0);
    call.init = &args;
    event(
        "snippet_init_enter",
        json!({"caller":if options.bridge.is_some() {"rust_bridge"} else {"direct"}}),
    )?;
    ngx_result("snippet_init", api.call(call))?;
    let mut params = std::ptr::null_mut();
    ngx_result(
        "allocate_feature_parameters",
        nr_api::NVSDK_NGX_VULKAN_AllocateParameters(&mut params),
    )?;
    if params.is_null() {
        return Err("successful parameter allocation returned null".into());
    }
    let mut call = nr_api::empty_call(1);
    call.parameters = params;
    ngx_result("populate_nr_parameters", api.call(call))?;
    #[cfg(feature = "nr-coexistence")]
    if let Some(consumer) = &consumer {
        consumer.exercise(
            &options.session,
            "sr_while_nr_active",
            &instance,
            physical,
            &device,
            family,
        )?;
    }
    #[cfg(feature = "nr-coexistence")]
    let mut interleave_index = 0;
    let mut interleave = |_cycle: u32, _frame: u32| -> Result<()> {
        #[cfg(feature = "nr-coexistence")]
        if let Some(consumer) = &consumer {
            consumer.exercise(
                &options.session,
                &format!("sr_with_live_nr_feature-{interleave_index}"),
                &instance,
                physical,
                &device,
                family,
            )?;
            interleave_index += 1;
        }
        Ok(())
    };
    let outputs = if options.init_only {
        Value::Null
    } else {
        exercise_recording(
            options,
            api,
            &instance,
            physical,
            &device,
            family,
            params,
            Some(&mut interleave),
        )?
    };
    let mut resize_outputs = Vec::new();
    if options.resize {
        for (index, (width, height)) in [(960, 540), (1280, 720), (options.width, options.height)]
            .into_iter()
            .enumerate()
        {
            let mut resized = options.clone();
            resized.width = width;
            resized.height = height;
            resized.session = options.session.join(format!("resize-{index}"));
            fs::create_dir(&resized.session)?;
            event(
                "resize_enter",
                json!({"index":index,"extent":[width,height],"previous_gpu_work_fence_completed":true}),
            )?;
            resize_outputs.push(exercise_recording(
                &resized,
                api,
                &instance,
                physical,
                &device,
                family,
                params,
                Some(&mut interleave),
            )?);
            event(
                "resize_complete",
                json!({"index":index,"extent":[width,height]}),
            )?;
        }
    }
    ngx_result(
        "destroy_feature_parameters",
        nr_api::NVSDK_NGX_VULKAN_DestroyParameters(params),
    )?;
    let mut call = nr_api::empty_call(5);
    call.device = device_handle;
    ngx_result("snippet_shutdown", api.call(call))?;
    ngx_result(
        "destroy_capability_parameters",
        nr_api::NVSDK_NGX_VULKAN_DestroyParameters(capabilities),
    )?;
    #[cfg(feature = "nr-coexistence")]
    if let Some(consumer) = &consumer {
        consumer.exercise(
            &options.session,
            "sr_after_nr_shutdown",
            &instance,
            physical,
            &device,
            family,
        )?;
        event(
            "core_shutdown_deferred",
            json!({"owner":"Streamline","reason":"slShutdown closes shared NGX; NR stop only releases its feature/maps/snippet"}),
        )?;
        consumer.shutdown()?;
    }
    if options.coexist_sr.is_none() {
        ngx_result(
            "core_shutdown",
            nr_api::NVSDK_NGX_VULKAN_Shutdown1(device_handle),
        )?;
    }
    device.destroy_device(None);
    // Instance create-info callback also captures instance destruction messages.
    utils.destroy_debug_utils_messenger(messenger, None);
    instance.destroy_instance(None);
    std::mem::ManuallyDrop::drop(&mut entry);
    #[cfg(feature = "nr-coexistence")]
    if let Some(consumer) = &mut consumer {
        std::mem::ManuallyDrop::drop(consumer);
    }
    if LOG_FAILED.load(Ordering::Relaxed) {
        return Err("diagnostic callback log I/O failed".into());
    }
    let ValidationSummary {
        errors,
        warnings,
        unreviewed,
        reviews: warning_reviews,
    } = validation_summary(&options.session)?;
    let mut remaining_gates = vec![
        "external review of synthetic output and optimized caller boundary audit",
        "curated P0 acceptance for the exact pinned runtime/GPU combination",
    ];
    if options.without_validation || errors != 0 || unreviewed != 0 {
        remaining_gates
            .push("core/synchronization validation with zero errors and reviewed warnings");
    }
    if options.coexist_sr.is_none() {
        remaining_gates.push("Streamline coexistence and shared core shutdown ownership");
    }
    if !options.pause_without_motion || options.init_only {
        remaining_gates.push("accepted missing-motion policy and reset on recovery");
    }
    if options.init_only || options.trace_layouts {
        remaining_gates
            .push("full continuous-output run; init/trace-only is not an acceptance sample");
    }
    write_json(
        &options.session.join("nr-result.json"),
        &json!({"initialized":true,"outputs":outputs,
        "validation_enabled":!options.without_validation,"validation_errors":errors,"validation_warnings":warnings,
            "warnings_reviewed":unreviewed == 0,"unreviewed_warnings":unreviewed,"warning_reviews":warning_reviews,
        "streamline_coexistence_verified":options.coexist_sr.is_some()&&errors==0&&unreviewed==0,"coexistence_execution_completed":options.coexist_sr.is_some(),"game_integration_verified":false,
        "resized_and_recreated":options.resize,"resize_outputs":resize_outputs,
        "two_pass":options.two_pass,
        "zero_motion_policy_verified":options.pause_without_motion&&!options.init_only&&errors==0&&unreviewed==0,"zero_motion_policy":if options.two_pass {"synthetic_motion_continuous_history"} else if options.pause_without_motion {"pause_nr_and_reset_on_recovery"} else {"experimental_reset_each_frame"},"p0_passed":false,"layout_trace_only":options.trace_layouts,
        "experimental_internal_layout_repair":options.repair_internal_layouts,
        "remaining_gates":remaining_gates}),
    )?;
    if errors != 0 || unreviewed != 0 {
        return Err(
            "validation errors or unreviewed warnings prevent passing P0; inspect validation.jsonl"
                .into(),
        );
    }
    Ok(())
}

struct ValidationSummary {
    errors: usize,
    warnings: usize,
    unreviewed: usize,
    reviews: Vec<Value>,
}
fn validation_summary(session: &Path) -> Result<ValidationSummary> {
    if LOG_FAILED.load(Ordering::Relaxed) {
        return Err("diagnostic callback log I/O failed".into());
    }
    let mut summary = ValidationSummary {
        errors: 0,
        warnings: 0,
        unreviewed: 0,
        reviews: Vec::new(),
    };
    // A missing/unreadable log is not a successful validation run.
    for line in fs::read_to_string(session.join("validation.jsonl"))?.lines() {
        let row: Value = serde_json::from_str(line)?;
        let severity = row["severity"]
            .as_u64()
            .ok_or("invalid validation severity")? as u32;
        summary.errors +=
            usize::from(severity & vk::DebugUtilsMessageSeverityFlagsEXT::ERROR.as_raw() != 0);
        if severity & vk::DebugUtilsMessageSeverityFlagsEXT::WARNING.as_raw() != 0 {
            summary.warnings += 1;
            let review = review_loader_warning(&row);
            summary.unreviewed += usize::from(review.is_none());
            summary
                .reviews
                .push(json!({"message":row["message"],"review":review}));
        }
    }
    Ok(summary)
}
fn review_loader_warning(row: &Value) -> Option<&'static str> {
    if row["type"] != vk::DebugUtilsMessageTypeFlagsEXT::GENERAL.as_raw() || row["id_number"] != 0 {
        return None;
    }
    let message = row["message"].as_str()?;
    for name in [
        "VK_LAYER_NV_optimus",
        "VK_LAYER_NV_present",
        "VK_LAYER_AMD_switchable_graphics",
        "VK_LAYER_reshade",
    ] {
        if message == format!("Layer \"{name}\" forced disabled because name matches filter of env var 'VK_LOADER_LAYERS_DISABLE'.") {
            return Some("Expected child-only exclusion of implicit layers; this is a loader notice, not a synchronization or lifetime warning.");
        }
    }
    if message == "windows_read_data_files_in_registry: Registry lookup failed to get layer manifest files." {
        return Some("No globally registered explicit layer directory. Layer availability is checked separately before device creation; a validated run fails when the required layer is missing.");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loader_review_does_not_hide_validation_or_unknown_messages() {
        let notice = json!({"type":1,"id_number":0,"message":"Layer \"VK_LAYER_reshade\" forced disabled because name matches filter of env var 'VK_LOADER_LAYERS_DISABLE'."});
        assert!(review_loader_warning(&notice).is_some());
        let mut unknown = notice.clone();
        unknown["message"] = json!("unknown warning");
        assert!(review_loader_warning(&unknown).is_none());
        let mut validation = notice;
        validation["type"] = json!(2);
        assert!(review_loader_warning(&validation).is_none());
    }
    #[test]
    fn rejects_non_dll_and_wrong_architecture_before_load() {
        assert!(verify_pe(b"not a PE").is_err());
        let mut bytes = vec![0; 256];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&128u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&[0x64, 0x86]);
        bytes[150..152].copy_from_slice(&0x2000u16.to_le_bytes());
        bytes[152..154].copy_from_slice(&[0x0b, 0x02]);
        assert!(verify_pe(&bytes).is_ok());
        bytes[132] = 0x4c;
        assert!(verify_pe(&bytes).is_err());
    }
}
