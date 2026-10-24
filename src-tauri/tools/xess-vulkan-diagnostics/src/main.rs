mod api;
mod gpu;
mod quality;
mod scene;

use api::{check, Size};
use ash::vk;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    ffi::{c_char, CStr},
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Mutex},
    time::Instant,
};
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const DLL_HASH: &str = "251659dd84a3e84de67c886a4186e01f3eca49b00641906fe38bb6b807e5d5b7";
const USAGE: &str = "xess-vulkan-diagnostics --dll <absolute pinned libxess.dll> --output <new absolute directory>\nOptions: --width 1920 --height 1080 --frames 144 --mode all|aa|quality|balanced --gpu-index 0\n         --validation --no-jitter --high-motion --flat-depth --temporal --benchmark-only\n         --zero-motion-reset (AA only; disables jitter and resets every frame)\n         --jitter-sign-x 1|-1 --jitter-sign-y 1|-1";
static SDK_MESSAGES: Mutex<Vec<Value>> = Mutex::new(Vec::new());
unsafe extern "C" fn sdk_log(message: *const c_char, level: i32) {
    let message = CStr::from_ptr(message).to_string_lossy().into_owned();
    if level >= 2 {
        eprintln!("XeSS {level}: {message}");
    }
    if let Ok(mut rows) = SDK_MESSAGES.lock() {
        if rows.len() < 200 {
            rows.push(json!({"level":level,"message":message}));
        }
    }
}
struct Options {
    dll: PathBuf,
    output: PathBuf,
    size: Size,
    frames: u32,
    validation: bool,
    jitter: bool,
    high_motion: bool,
    flat_depth: bool,
    adapter: usize,
    mode: String,
    api_jitter: [f32; 2],
    temporal: bool,
    benchmark_only: bool,
    zero_motion_reset: bool,
}
impl Options {
    fn parse() -> Result<Self> {
        let mut o = Self {
            dll: PathBuf::new(),
            output: PathBuf::new(),
            size: Size { x: 1920, y: 1080 },
            frames: 144,
            validation: false,
            jitter: true,
            high_motion: false,
            flat_depth: false,
            adapter: 0,
            mode: "all".into(),
            api_jitter: [1.0, 1.0],
            temporal: false,
            benchmark_only: false,
            zero_motion_reset: false,
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--dll" => o.dll = args.next().ok_or("--dll value required")?.into(),
                "--output" => o.output = args.next().ok_or("--output value required")?.into(),
                "--width" => o.size.x = args.next().ok_or("--width value required")?.parse()?,
                "--height" => o.size.y = args.next().ok_or("--height value required")?.parse()?,
                "--frames" => o.frames = args.next().ok_or("--frames value required")?.parse()?,
                "--gpu-index" => {
                    o.adapter = args.next().ok_or("--gpu-index value required")?.parse()?
                }
                "--mode" => o.mode = args.next().ok_or("--mode value required")?,
                "--jitter-sign-x" => {
                    o.api_jitter[0] = args
                        .next()
                        .ok_or("--jitter-sign-x value required")?
                        .parse()?
                }
                "--jitter-sign-y" => {
                    o.api_jitter[1] = args
                        .next()
                        .ok_or("--jitter-sign-y value required")?
                        .parse()?
                }
                "--temporal" => o.temporal = true,
                "--benchmark-only" => o.benchmark_only = true,
                "--zero-motion-reset" => {
                    o.zero_motion_reset = true;
                    o.jitter = false;
                    o.high_motion = true;
                }
                "--validation" => o.validation = true,
                "--no-jitter" => o.jitter = false,
                "--high-motion" => o.high_motion = true,
                "--flat-depth" => o.flat_depth = true,
                "--help" => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                _ => return Err(USAGE.into()),
            }
        }
        if !o.dll.is_absolute() || !o.output.is_absolute() {
            return Err("--dll and --output must be absolute".into());
        }
        if !(128..=3840).contains(&o.size.x)
            || !(128..=2160).contains(&o.size.y)
            || !(144..=2000).contains(&o.frames)
        {
            return Err("size out of bounds or fewer than 144 frames".into());
        }
        if !["all", "aa", "quality", "balanced"].contains(&o.mode.as_str()) {
            return Err("invalid mode".into());
        }
        if o.api_jitter.iter().any(|v| ![-1.0, 1.0].contains(v)) {
            return Err("jitter signs must be -1 or 1".into());
        }
        if o.zero_motion_reset {
            if o.mode != "aa" {
                return Err("--zero-motion-reset is restricted to --mode aa".into());
            }
            o.jitter = false;
            o.high_motion = true;
        }
        Ok(o)
    }
}
fn main() {
    let result = (|| -> Result<()> {
        let o = Options::parse()?;
        std::fs::create_dir(&o.output)?;
        match unsafe { run(&o) } {
            Ok(report) => {
                write(&o.output.join("report.json"), &report)?;
                println!("report: {}", o.output.join("report.json").display());
                if report["passed"] != true {
                    return Err("validation or output checks failed; inspect report.json".into());
                }
                Ok(())
            }
            Err(e) => {
                write(
                    &o.output.join("failure.json"),
                    &json!({"error":e.to_string(),"validation_errors":gpu::VALIDATION_ERRORS.load(Ordering::Relaxed),"validation_messages":*gpu::VALIDATION_MESSAGES.lock().unwrap(),"sdk_messages":*SDK_MESSAGES.lock().unwrap()}),
                )?;
                Err(e)
            }
        }
    })();
    if let Err(error) = result {
        eprintln!("XeSS diagnostics failed: {error}");
        std::process::exit(1);
    }
}
fn write(path: &Path, value: &Value) -> Result<()> {
    std::fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}
unsafe fn run(o: &Options) -> Result<Value> {
    let hash = format!("{:x}", Sha256::digest(std::fs::read(&o.dll)?));
    if hash != DLL_HASH {
        return Err(format!("pinned XeSS 3.0.2 DLL hash mismatch: {hash}").into());
    }
    let api = api::Api::load(&o.dll)?;
    let mut version = api::Version::default();
    check((api.version)(&mut version), "version")?;
    let mut rows = Vec::new();
    let hardware;
    {
        let gpu = gpu::Gpu::new(&api, o.validation, o.adapter)?;
        let name = CStr::from_ptr(gpu.props.device_name.as_ptr())
            .to_string_lossy()
            .into_owned();
        println!(
            "{name}; XeSS {}.{}.{}; {}x{}; validation={}",
            version.major, version.minor, version.patch, o.size.x, o.size.y, o.validation
        );
        let adapters:Vec<_>=gpu.instance.enumerate_physical_devices()?.iter().map(|&p|{let props=gpu.instance.get_physical_device_properties(p);json!({"name":CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy(),"vendor_id":props.vendor_id,"device_id":props.device_id})}).collect();
        hardware = json!({"gpu":name,"adapter_index":o.adapter,"adapters":adapters,"vendor_id":gpu.props.vendor_id,"device_id":gpu.props.device_id,"driver_version_raw":gpu.props.driver_version,"vulkan_api_raw":gpu.props.api_version,"queue_family":gpu.family,"timestamp_period_ns":gpu.props.limits.timestamp_period,"timestamp_valid_bits":gpu.timestamp_bits,"requirements":gpu.requirements});
        for (name, preset) in [("aa", 106), ("quality", 103), ("balanced", 102)] {
            if o.mode != "all" && o.mode != name {
                continue;
            }
            let result = case(o, &api, &gpu, name, preset)?;
            println!(
                "{name}: input={}, GPU p50={} ms, p95={} ms",
                result["input"], result["xess_gpu_ms"]["p50"], result["xess_gpu_ms"]["p95"]
            );
            write(&o.output.join(format!("{name}.json")), &result)?;
            rows.push(result);
        }
    } // Validation also observes context, device and instance destruction.
    let errors = gpu::VALIDATION_ERRORS.load(Ordering::Relaxed);
    let warnings = gpu::VALIDATION_WARNINGS.load(Ordering::Relaxed);
    let sdk_bad = SDK_MESSAGES
        .lock()
        .unwrap()
        .iter()
        .filter(|m| m["level"].as_i64().unwrap_or(0) >= 2)
        .count();
    Ok(
        json!({"schema_version":1,"passed":errors==0&&warnings==0&&sdk_bad==0,"hardware":hardware,
        "sdk":{"release":"v3.0.2","sr_version":format!("{}.{}.{}",version.major,version.minor,version.patch),"dll_sha256":hash,"dll":o.dll},
        "scene_shader_sha256":format!("{:x}",Sha256::digest(include_bytes!("../scene.comp"))),
        "conditions":{"output":[o.size.x,o.size.y],"frames":o.frames,"warmup":32,"jitter":o.jitter,"api_jitter_sign":o.api_jitter,"motion_units":"current-to-previous pixels on the motion texture grid, no jitter","high_resolution_motion":o.high_motion,"flat_depth":o.flat_depth,"zero_motion":o.zero_motion_reset,"reset_every_frame":o.zero_motion_reset,"validation":o.validation,"temporal_crops":o.temporal&&!o.benchmark_only,"benchmark_only":o.benchmark_only,"scene":"static 0-47, camera/object translation 48+, explicit color cut/reset 112","timing":"GPU timestamps enclose XeSS commands only; serialized fenced submissions; no rendering/upload/readback/PNG/presentation in interval","reference":"64 spatial samples per output pixel, linear RGB; analytic synthetic scene, not emulator/game acceptance"},
        "cases":rows,"validation_errors":errors,"validation_warnings":warnings,"validation_messages":*gpu::VALIDATION_MESSAGES.lock().unwrap(),"loader_isolation_notices":*gpu::LOADER_NOTICES.lock().unwrap(),"sdk_messages":*SDK_MESSAGES.lock().unwrap()}),
    )
}
fn halton(mut i: u32, base: u32) -> f32 {
    let mut r = 0.0;
    let mut f = 1.0;
    while i > 0 {
        f /= base as f32;
        r += f * (i % base) as f32;
        i /= base;
    }
    r
}
unsafe fn case(
    o: &Options,
    api: &api::Api,
    gpu: &gpu::Gpu,
    name: &str,
    preset: i32,
) -> Result<Value> {
    let mut handle = std::ptr::null_mut();
    check(
        (api.create)(
            gpu.instance.handle(),
            gpu.physical,
            gpu.device.handle(),
            &mut handle,
        ),
        "create context",
    )?;
    let session = api::Session { handle, api };
    check((api.logging)(handle, 1, sdk_log), "logging")?;
    let mut input = Size::default();
    let mut min = Size::default();
    let mut max = Size::default();
    check(
        (api.optimal)(handle, &o.size, preset, &mut input, &mut min, &mut max),
        "optimal resolution",
    )?;
    if input.x == 0 || input.y == 0 || input.x > o.size.x || input.y > o.size.y {
        return Err("invalid optimal input resolution".into());
    }
    let mut properties = api::Properties::default();
    check(
        (api.properties)(handle, &o.size, &mut properties),
        "properties",
    )?;
    let started = Instant::now();
    check(
        (api.init)(
            handle,
            &api::Init {
                output: o.size,
                quality: preset,
                flags: (1 << 6) | u32::from(o.high_motion),
                ..Default::default()
            },
        ),
        "initialize",
    )?;
    let init_ms = started.elapsed().as_secs_f64() * 1000.0;
    let color = gpu::Image::new(gpu, input, vk::Format::R16G16B16A16_SFLOAT)?;
    let motion = gpu::Image::new(
        gpu,
        if o.high_motion { o.size } else { input },
        vk::Format::R16G16_SFLOAT,
    )?;
    let depth = gpu::Image::new(gpu, input, vk::Format::R32_SFLOAT)?;
    let output = gpu::Image::new(gpu, o.size, vk::Format::R16G16B16A16_SFLOAT)?;
    let reference = gpu::Image::new(gpu, o.size, vk::Format::R16G16B16A16_SFLOAT)?;
    let baseline = gpu::Image::new(gpu, o.size, vk::Format::R16G16B16A16_SFLOAT)?;
    let images = [&color, &motion, &depth, &output, &reference, &baseline];
    let allocated: u64 = images.iter().map(|i| i.allocated).sum();
    let scene = scene::Scene::new(gpu, [&color, &motion, &depth, &reference, &baseline])?;
    gpu.begin()?;
    for image in &images {
        image.barrier(gpu, vk::ImageLayout::UNDEFINED, vk::ImageLayout::GENERAL);
    }
    gpu.submit()?;
    let mut timings = Vec::new();
    let mut cpu = Vec::new();
    let mut captures = Vec::new();
    let mut per_frame = Vec::new();
    let mut previous_static: Option<Vec<f32>> = None;
    let mut static_delta = Vec::new();
    let sequence = (8.0 * (o.size.x as f32 / input.x as f32).powi(2)).ceil() as u32;
    for frame in 0..o.frames {
        let jitter = if o.jitter {
            [
                halton(frame % sequence + 1, 2) - 0.5,
                halton(frame % sequence + 1, 3) - 0.5,
            ]
        } else {
            [0.0, 0.0]
        };
        let capture = !o.benchmark_only && [47, 95, 112, o.frames - 1].contains(&frame);
        gpu.begin()?;
        // Also orders the previous frame's scene writes / SDK reads before reuse.
        for image in [&color, &motion, &depth, &reference, &baseline] {
            image.barrier(gpu, vk::ImageLayout::GENERAL, vk::ImageLayout::GENERAL);
        }
        scene.record(
            gpu,
            &scene::Parameters {
                input,
                output: o.size,
                frame,
                capture: u32::from(capture),
                jitter,
                high_motion: u32::from(o.high_motion),
                flat_depth: u32::from(o.flat_depth),
                zero_motion: u32::from(o.zero_motion_reset),
            },
        );
        for image in [&color, &motion, &depth] {
            image.barrier(
                gpu,
                vk::ImageLayout::GENERAL,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            );
        }
        output.barrier(gpu, vk::ImageLayout::GENERAL, vk::ImageLayout::GENERAL);
        // Complete scene/reference generation before starting an otherwise empty
        // timed submission; TOP_OF_PIPE cannot include an earlier render pass.
        gpu.submit()?;
        gpu.begin()?;
        gpu.device.cmd_reset_query_pool(gpu.cmd, gpu.queries, 0, 2);
        gpu.device.cmd_write_timestamp(
            gpu.cmd,
            vk::PipelineStageFlags::TOP_OF_PIPE,
            gpu.queries,
            0,
        );
        let started = Instant::now();
        check(
            (api.execute)(
                handle,
                gpu.cmd,
                &api::Execute {
                    color: color.info(),
                    motion: motion.info(),
                    depth: if o.high_motion {
                        Default::default()
                    } else {
                        depth.info()
                    },
                    output: output.info(),
                    jitter_x: jitter[0] * o.api_jitter[0],
                    jitter_y: jitter[1] * o.api_jitter[1],
                    exposure_scale: 1.0,
                    reset: u32::from(o.zero_motion_reset || frame == 0 || frame == 112),
                    width: input.x,
                    height: input.y,
                    ..Default::default()
                },
            ),
            "execute",
        )?;
        let cpu_ms = started.elapsed().as_secs_f64() * 1000.0;
        gpu.device.cmd_write_timestamp(
            gpu.cmd,
            vk::PipelineStageFlags::BOTTOM_OF_PIPE,
            gpu.queries,
            1,
        );
        for image in [&color, &motion, &depth] {
            image.barrier(
                gpu,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                vk::ImageLayout::GENERAL,
            );
        }
        output.barrier(gpu, vk::ImageLayout::GENERAL, vk::ImageLayout::GENERAL);
        gpu.submit()?;
        let gpu_ms = gpu.milliseconds()?;
        if frame >= 32 {
            timings.push(gpu_ms);
            cpu.push(cpu_ms);
        }
        per_frame.push(json!({"frame":frame,"gpu_ms":gpu_ms,"cpu_record_ms":cpu_ms,"jitter":jitter,"reset":o.zero_motion_reset||frame==0||frame==112}));
        if o.temporal
            && !o.benchmark_only
            && ((40..=47).contains(&frame) || (80..=95).contains(&frame))
        {
            let pixels = output.read(gpu)?;
            if frame < 48 {
                if let Some(previous) = &previous_static {
                    let total: f64 = pixels
                        .chunks_exact(4)
                        .zip(previous.chunks_exact(4))
                        .map(|(p, q)| (0..3).map(|c| (p[c] - q[c]).abs() as f64).sum::<f64>())
                        .sum();
                    static_delta.push(total / (o.size.x as f64 * o.size.y as f64 * 3.0));
                }
                previous_static = Some(pixels);
            } else {
                let (size, crop) = quality::crops(&pixels, o.size);
                quality::png(
                    &o.output.join(format!("{name}-motion-{frame:04}.png")),
                    size,
                    &crop,
                )?;
            }
        }
        if capture {
            let result = output.read(gpu)?;
            let truth = reference.read(gpu)?;
            let raw = baseline.read(gpu)?;
            let low = color.read(gpu)?;
            if result.iter().any(|v| !v.is_finite()) {
                return Err(format!("{name} frame {frame}: nonfinite output").into());
            }
            let spatial = quality::bilinear(&low, input, o.size, jitter);
            let prefix = format!("{name}-{frame:04}");
            for (suffix, size, pixels) in [
                ("xess", o.size, &result),
                ("reference", o.size, &truth),
                ("native-no-aa", o.size, &raw),
                ("input", input, &low),
                ("bilinear", o.size, &spatial),
            ] {
                quality::png(
                    &o.output.join(format!("{prefix}-{suffix}.png")),
                    size,
                    pixels,
                )?;
            }
            let finite_min = result
                .chunks_exact(4)
                .flat_map(|p| p[..3].iter().copied())
                .fold(f32::INFINITY, f32::min);
            let finite_max = result
                .chunks_exact(4)
                .flat_map(|p| p[..3].iter().copied())
                .fold(f32::NEG_INFINITY, f32::max);
            if finite_max - finite_min < 0.05 {
                return Err(format!("{name} frame {frame}: flat/unwritten output").into());
            }
            captures.push(json!({"frame":frame,"prefix":prefix,"xess":quality::metric(&result,&truth,o.size),"native_no_aa":quality::metric(&raw,&truth,o.size),"bilinear":quality::metric(&spatial,&truth,o.size),"rgb_min":finite_min,"rgb_max":finite_max}));
        }
    }
    // Fence completed before every read/reuse. Retire SDK before destroying inputs.
    session.close()?;
    Ok(
        json!({"mode":name,"preset":preset,"input":[input.x,input.y],"output":[o.size.x,o.size.y],"init_ms":init_ms,"jitter_sequence_length":sequence,
        "xess_gpu_ms":quality::distribution(&timings),"cpu_record_ms":quality::distribution(&cpu),"per_frame":per_frame,
        "static_consecutive_linear_rgb_mae":quality::distribution(&static_delta),"sdk_temporary_buffer_bytes":properties.buffer_bytes,"sdk_temporary_texture_bytes":properties.texture_bytes,"sdk_persistent_memory":"not exposed by xessGetProperties; not included", "harness_image_allocated_bytes":allocated,"captures":captures}),
    )
}
