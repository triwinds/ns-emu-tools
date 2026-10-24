use sha2::{Digest, Sha256};
mod vulkan_bridge;
use std::{
    fs::OpenOptions, io::Write, os::windows::fs::OpenOptionsExt, path::PathBuf, sync::Mutex,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
static EVENTS: Mutex<Vec<serde_json::Value>> = Mutex::new(Vec::new());
static OUTPUT: Mutex<Option<PathBuf>> = Mutex::new(None);
static JOURNAL: Mutex<Option<std::fs::File>> = Mutex::new(None);
fn record(value: serde_json::Value) {
    let line = value.to_string();
    println!("{line}");
    let _ = std::io::stdout().flush();
    if let Some(file) = JOURNAL.lock().unwrap().as_mut() {
        // Preserve diagnostics even when uncertain GPU completion aborts the child.
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
    EVENTS.lock().unwrap().push(value);
}
#[no_mangle]
unsafe extern "C" fn xess_fg_readback(
    frame: u32,
    width: u32,
    height: u32,
    pitch: u32,
    data: *const u8,
) -> i32 {
    let result = (|| -> Result<()> {
        let source = std::slice::from_raw_parts(data, pitch as usize * height as usize);
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        for y in 0..height as usize {
            for x in 0..width as usize {
                let at = y * pitch as usize + x * 4;
                rgb.extend_from_slice(&source[at..at + 3]);
            }
        }
        let directory = OUTPUT.lock().unwrap();
        let file = std::fs::File::create(
            directory
                .as_ref()
                .ok_or("output unavailable")?
                .join(format!("vulkan-source-{frame:04}.png")),
        )?;
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.write_header()?.write_image_data(&rgb)?;
        Ok(())
    })();
    if result.is_ok() {
        0
    } else {
        1
    }
}
unsafe extern "C" fn event(line: *const std::ffi::c_char) {
    let line = std::ffi::CStr::from_ptr(line).to_string_lossy();
    if let Ok(value) = serde_json::from_str(&line) {
        record(value);
    }
}
extern "C" {
    fn xess_fg_run(
        sdk: *const u16,
        adapter: u32,
        frames: u32,
        width: u32,
        height: u32,
        validation: u32,
        capabilities: u32,
        interop: u32,
        source_fps: u32,
        callback: unsafe extern "C" fn(*const std::ffi::c_char),
    ) -> i32;
}
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut sdk = PathBuf::new();
    let mut output = PathBuf::new();
    let (mut adapter, mut frames, mut width, mut height) = (u32::MAX, 180, 640, 360);
    let mut source_fps = 40;
    let (mut validation, mut capabilities, mut interop) = (false, false, false);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--sdk" => sdk = args.next().ok_or("SDK path required")?.into(),
            "--output" => output = args.next().ok_or("output path required")?.into(),
            "--adapter" => adapter = args.next().ok_or("adapter required")?.parse()?,
            "--frames" => frames = args.next().ok_or("frames required")?.parse()?,
            "--width" => width = args.next().ok_or("width required")?.parse()?,
            "--height" => height = args.next().ok_or("height required")?.parse()?,
            "--validation" => validation = true,
            "--capabilities" => capabilities = true,
            "--vulkan" => interop = true,
            "--source-fps" => source_fps=args.next().ok_or("source FPS required")?.parse()?,
            _ => return Err("Expected --sdk <absolute pinned SDK> --output <new absolute dir> [--adapter N] [--frames 180] [--width 640 --height 360] [--source-fps 40] [--vulkan] [--validation] [--capabilities]".into()),
        }
    }
    if !sdk.is_absolute()
        || !output.is_absolute()
        || output.exists()
        || !(90..=600).contains(&frames)
        || !(320..=2560).contains(&width)
        || !(180..=1440).contains(&height)
        || !(24..=120).contains(&source_fps)
    {
        return Err(
            "absolute paths, new output directory and bounded size/frame count required".into(),
        );
    }
    let manifest: serde_json::Value = serde_json::from_str(include_str!("../sdk.json"))?;
    let mut pinned = Vec::new();
    let agility: serde_json::Value = serde_json::from_str(include_str!("../agility.json"))?;
    let binary_dir = std::env::current_exe()?
        .parent()
        .ok_or("missing binary directory")?
        .to_path_buf();
    for file in agility["files"]
        .as_array()
        .ok_or("missing Agility manifest")?
    {
        let name = file["path"].as_str().ok_or("invalid Agility path")?;
        let mut handle = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(binary_dir.join("D3D12").join(name))?;
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut handle, &mut bytes)?;
        if format!("{:x}", Sha256::digest(bytes)) != file["sha256"] {
            return Err(format!("pinned Agility DLL mismatch: {name}").into());
        }
        pinned.push(handle);
    }
    for file in manifest["files"].as_array().ok_or("missing SDK manifest")? {
        let name = file["path"].as_str().ok_or("invalid SDK path")?;
        if name.ends_with(".dll") {
            let mut handle = OpenOptions::new()
                .read(true)
                .share_mode(1)
                .open(sdk.join(name))?;
            let mut bytes = Vec::new();
            std::io::Read::read_to_end(&mut handle, &mut bytes)?;
            if format!("{:x}", Sha256::digest(bytes)) != file["sha256"] {
                return Err(format!("pinned DLL mismatch: {name}").into());
            }
            pinned.push(handle);
        }
    }
    std::fs::create_dir_all(&output)?;
    *OUTPUT.lock().unwrap() = Some(output.clone());
    *JOURNAL.lock().unwrap() = Some(std::fs::File::create(output.join("events.jsonl"))?);
    let wide: Vec<u16> = sdk
        .to_str()
        .ok_or("SDK path must be Unicode")?
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let code = unsafe {
        xess_fg_run(
            wide.as_ptr(),
            adapter,
            frames,
            width,
            height,
            u32::from(validation),
            u32::from(capabilities),
            u32::from(interop),
            source_fps,
            event,
        )
    };
    let report = serde_json::json!({"schema_version":1,"sdk":manifest,"exit_code":code,
        "binary_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe()?)?)),
        "vulkan_shader_sha256":format!("{:x}",Sha256::digest(include_bytes!(concat!(env!("OUT_DIR"),"/bridge_scene.spv")))),
        "agility":agility,"conditions":{"width":width,"height":height,"frames":frames,"target_source_fps":source_fps,"capabilities_only":capabilities,"d3d12_debug_requested":validation,"streamline":false,"nvof":false,"vulkan_bridge":interop,"vulkan_validation_requested":std::env::var("NS_XESS_FG_VULKAN_VALIDATION").as_deref()==Ok("1"),"scene":"analytic whole-frame translation, no jitter; true pixel motion and constant depth; synthetic, not game acceptance"},"events":*EVENTS.lock().unwrap()});
    std::fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    drop(pinned);
    if code != 0 {
        return Err(format!("FG diagnostic failed with code {code}; see report.json").into());
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
