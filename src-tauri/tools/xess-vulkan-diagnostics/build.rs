use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=scene.comp");
    println!("cargo:rerun-if-env-changed=GLSLANG_VALIDATOR");
    let compiler = env::var_os("GLSLANG_VALIDATOR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
                .join("../../target/vulkan-sdk-1.4.363.0/Bin/glslangValidator.exe")
        });
    let result = Command::new(compiler)
        .args(["-V", "--target-env", "vulkan1.1", "scene.comp", "-o"])
        .arg(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("scene.spv"))
        .output()
        .expect("glslangValidator (or GLSLANG_VALIDATOR) is required");
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
