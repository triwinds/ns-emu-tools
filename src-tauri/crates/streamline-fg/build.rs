use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    if (env::var_os("CARGO_FEATURE_NR_DIAGNOSTICS").is_some()
        || env::var_os("CARGO_FEATURE_NATIVE_NR").is_some())
        && env::var("TARGET").unwrap() == "x86_64-pc-windows-msvc"
    {
        nr_sdk();
    }
    println!("cargo:rerun-if-changed=shaders/motion.json");
    let shaders: serde_json::Value =
        serde_json::from_str(include_str!("shaders/motion.json")).unwrap();
    for (name, expected) in shaders.as_object().unwrap() {
        let path = PathBuf::from("shaders").join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected.as_str().unwrap(),
            "optical-flow shader changed; recompile and update paired hashes"
        );
    }
    println!("cargo:rerun-if-env-changed=STREAMLINE_SDK_DIR");
    println!("cargo:rerun-if-changed=bridge/query.cpp");
    println!("cargo:rerun-if-changed=bridge/sr.cpp");
    println!("cargo:rerun-if-changed=sdk/baseline.json");
    if env::var_os("CARGO_FEATURE_SDK_BRIDGE").is_none() {
        return;
    }
    assert_eq!(env::var("TARGET").unwrap(), "x86_64-pc-windows-msvc");
    let root = env::var_os("STREAMLINE_SDK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../target/streamline-sdk-v2.12.0"));
    let frozen = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("frozen-headers");
    fs::create_dir_all(&frozen).unwrap();
    let baseline: serde_json::Value =
        serde_json::from_str(include_str!("sdk/baseline.json")).unwrap();
    // Compile only copied, hash-verified headers. Unknown files in the SDK include
    // directory cannot silently participate in this build.
    for artifact in baseline["artifacts"].as_array().unwrap() {
        let name = artifact["path"].as_str().unwrap();
        if artifact["scope"] != "sdk" || !name.starts_with("include/") {
            continue;
        }
        let source = root.join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        let normalized = fs::read_to_string(&source)
            .unwrap_or_else(|e| panic!("{}: {e}", source.display()))
            .replace("\r\n", "\n");
        let actual = format!("{:x}", Sha256::digest(normalized.as_bytes()));
        assert_eq!(
            actual,
            artifact["sha256"].as_str().unwrap(),
            "SDK header mismatch: {name}"
        );
        fs::write(frozen.join(source.file_name().unwrap()), normalized).unwrap();
    }
    println!("cargo:rerun-if-env-changed=VULKAN_HEADERS_DIR");
    println!("cargo:rerun-if-changed=bridge/vulkan-headers.json");
    let vk_root = env::var_os("VULKAN_HEADERS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../target/vulkan-headers-v1.4.341/include"));
    let vk_frozen = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("frozen-vulkan");
    let vk_manifest: serde_json::Value =
        serde_json::from_str(include_str!("bridge/vulkan-headers.json")).unwrap();
    for item in vk_manifest["files"].as_array().unwrap() {
        let name = item["path"].as_str().unwrap();
        let source = vk_root.join(name);
        println!("cargo:rerun-if-changed={}", source.display());
        let normalized = fs::read_to_string(&source).unwrap().replace("\r\n", "\n");
        assert_eq!(
            format!("{:x}", Sha256::digest(normalized.as_bytes())),
            item["sha256"].as_str().unwrap(),
            "Vulkan header mismatch: {name}"
        );
        let dest = vk_frozen.join(name);
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        fs::write(dest, normalized).unwrap();
    }
    let fg_pause_abi = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("fg-pause-abi.cpp");
    fs::write(
        &fg_pause_abi,
        r#"
#include <cstddef>
#include "sl_dlss_g.h"
#include "sl_reflex.h"
static_assert(sizeof(sl::BaseStructure) == 32);
static_assert(sizeof(sl::ViewportHandle) == 40);
static_assert(sizeof(sl::DLSSGOptions) == 120);
static_assert(offsetof(sl::DLSSGOptions, mode) == 32);
static_assert(offsetof(sl::DLSSGOptions, flags) == 40);
static_assert(offsetof(sl::DLSSGOptions, onErrorCallback) == 96);
static_assert(offsetof(sl::DLSSGOptions, queueParallelismMode) == 108);
static_assert(offsetof(sl::DLSSGOptions, dynamicTargetFrameRate) == 116);
static_assert(uint32_t(sl::DLSSGFlags::eRetainResourcesWhenOff) == 8);
static_assert(sl::kFeatureReflex == 3);
static_assert(sizeof(sl::ReflexOptions) == 48);
static_assert(offsetof(sl::ReflexOptions, virtualKey) == 42);
static_assert(offsetof(sl::ReflexOptions, idThread) == 44);
"#,
    )
    .unwrap();
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .flag("/EHsc")
        .flag("/permissive-")
        .include(frozen)
        .include(vk_frozen)
        .file("bridge/query.cpp")
        .file(fg_pause_abi)
        .warnings_into_errors(true)
        .compile("streamline_query_bridge");
}

fn nr_sdk() {
    println!("cargo:rerun-if-env-changed=NGX_SDK_DIR");
    println!("cargo:rerun-if-changed=sdk/nr-contract.json");
    let root = env::var_os("NGX_SDK_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../target/streamline-sdk-v2.12.0/external/ngx-sdk"));
    let contract: serde_json::Value =
        serde_json::from_str(include_str!("sdk/nr-contract.json")).unwrap();
    for artifact in contract["artifacts"].as_array().unwrap() {
        let path = root.join(artifact["path"].as_str().unwrap());
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(
            format!("{:x}", Sha256::digest(bytes)),
            artifact["sha256"].as_str().unwrap(),
            "NR SDK contract mismatch: {}",
            path.display()
        );
    }
    println!(
        "cargo:rustc-link-search=native={}",
        root.join("lib/Windows_x86_64").display()
    );
}
