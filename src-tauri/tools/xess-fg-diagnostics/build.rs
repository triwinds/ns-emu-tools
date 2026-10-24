use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};
fn main() {
    assert_eq!(env::var("TARGET").unwrap(), "x86_64-pc-windows-msvc");
    let root = PathBuf::from("../../target/xess-fg-sdk-v3.0.2");
    let frozen = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("headers");
    let manifest: serde_json::Value = serde_json::from_str(include_str!("sdk.json")).unwrap();
    for file in manifest["files"].as_array().unwrap() {
        let name = file["path"].as_str().unwrap();
        if !name.starts_with("inc/") {
            continue;
        }
        let source = root.join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        let bytes = fs::read(source).unwrap();
        assert_eq!(
            format!("{:x}", Sha256::digest(&bytes)),
            file["sha256"].as_str().unwrap()
        );
        let target = frozen.join(name.strip_prefix("inc/").unwrap());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, bytes).unwrap();
    }
    println!("cargo:rerun-if-changed=sdk.json");
    println!("cargo:rerun-if-changed=bridge.cpp");
    println!("cargo:rerun-if-changed=bridge_scene.comp");
    let shader = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("bridge_scene.spv");
    let compiler = PathBuf::from("../../target/vulkan-sdk-1.4.363.0/Bin/glslangValidator.exe");
    assert!(std::process::Command::new(compiler)
        .args([
            "-V",
            "--target-env",
            "vulkan1.2",
            "-S",
            "comp",
            "bridge_scene.comp",
            "-o"
        ])
        .arg(shader)
        .status()
        .unwrap()
        .success());
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .flag("/EHsc")
        .flag("/permissive-")
        .include(frozen)
        .file("bridge.cpp")
        .warnings_into_errors(true)
        .compile("xess_fg_test");
    for library in ["d3d12", "dxgi", "dxguid", "user32"] {
        println!("cargo:rustc-link-lib={library}");
    }
}
