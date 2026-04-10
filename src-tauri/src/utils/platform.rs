//! 平台识别工具模块

#[cfg(target_os = "macos")]
use crate::error::{AppError, AppResult};
use std::env::consts::{ARCH, OS};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
#[cfg(target_os = "macos")]
use tracing::{debug, warn};

/// 当前平台信息
#[derive(Debug, Clone, PartialEq)]
pub struct Platform {
    pub os: PlatformOS,
    pub arch: PlatformArch,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformOS {
    Windows,
    MacOS,
    Linux,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlatformArch {
    X86_64,
    Aarch64,
    Other(String),
}

impl Platform {
    /// 获取当前运行平台
    pub fn current() -> Self {
        let os = match OS {
            "windows" => PlatformOS::Windows,
            "macos" => PlatformOS::MacOS,
            "linux" => PlatformOS::Linux,
            _ => PlatformOS::Linux, // fallback
        };

        let arch = match ARCH {
            "x86_64" => PlatformArch::X86_64,
            "aarch64" => PlatformArch::Aarch64,
            other => PlatformArch::Other(other.to_string()),
        };

        Self { os, arch }
    }

    pub fn is_macos(&self) -> bool {
        matches!(self.os, PlatformOS::MacOS)
    }

    pub fn is_windows(&self) -> bool {
        matches!(self.os, PlatformOS::Windows)
    }

    pub fn is_linux(&self) -> bool {
        matches!(self.os, PlatformOS::Linux)
    }
}

/// 读取 macOS .app bundle 的可执行文件名
#[cfg(target_os = "macos")]
pub fn read_macos_bundle_executable_name(app_path: &Path) -> AppResult<Option<String>> {
    let plist_path = app_path.join("Contents/Info.plist");
    if !plist_path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read(&plist_path)?;
    let plist: plist::Dictionary = plist::from_bytes(&contents)
        .map_err(|e| AppError::Emulator(format!("解析 Info.plist 失败: {}", e)))?;

    if let Some(plist::Value::String(executable_name)) = plist.get("CFBundleExecutable") {
        if executable_name.trim().is_empty() {
            warn!(
                "Info.plist 中的 CFBundleExecutable 为空，忽略该值: {}",
                app_path.display()
            );
            return Ok(None);
        }

        debug!(
            "从 Info.plist 读取到 bundle 可执行文件名: {} -> {}",
            app_path.display(),
            executable_name
        );
        return Ok(Some(executable_name.clone()));
    }

    Ok(None)
}

/// 获取 macOS .app bundle 内部可执行文件路径
#[cfg(target_os = "macos")]
pub fn get_macos_bundle_executable_path(
    app_path: &Path,
    fallback_name: Option<&str>,
) -> AppResult<PathBuf> {
    if !app_path.exists() {
        return Err(AppError::FileNotFound(app_path.display().to_string()));
    }

    let macos_dir = app_path.join("Contents/MacOS");
    if let Some(executable_name) = read_macos_bundle_executable_name(app_path)? {
        let executable_path = macos_dir.join(executable_name);
        if executable_path.exists() && executable_path.is_file() {
            return Ok(executable_path);
        }

        warn!(
            "Info.plist 指定的 bundle 可执行文件不存在，尝试回退: {}",
            executable_path.display()
        );
    }

    if let Some(fallback_name) = fallback_name {
        let fallback_path = macos_dir.join(fallback_name);
        if fallback_path.exists() && fallback_path.is_file() {
            return Ok(fallback_path);
        }

        warn!(
            "fallback bundle 可执行文件不存在: {}",
            fallback_path.display()
        );
    }

    Err(AppError::FileNotFound(format!(
        "{} 内未找到可执行文件名信息",
        app_path.display()
    )))
}

/// 对 macOS .app bundle 执行安装后收尾处理
#[cfg(target_os = "macos")]
pub fn finalize_macos_app_install(
    app_path: &Path,
    fallback_name: Option<&str>,
) -> AppResult<PathBuf> {
    let executable_path = get_macos_bundle_executable_path(app_path, fallback_name)?;
    if !executable_path.exists() {
        return Err(AppError::FileNotFound(
            executable_path.display().to_string(),
        ));
    }

    let xattr_result = Command::new("xattr")
        .args(["-r", "-d", "com.apple.quarantine"])
        .arg(app_path)
        .output();

    match xattr_result {
        Ok(output) if output.status.success() => {
            debug!("已移除 quarantine 属性: {}", app_path.display());
        }
        Ok(_) => {
            debug!(
                "xattr 返回非零状态，bundle 可能本就没有 quarantine 属性: {}",
                app_path.display()
            );
        }
        Err(error) => {
            warn!(
                "移除 quarantine 属性失败: {} ({})",
                app_path.display(),
                error
            );
        }
    }

    let chmod_bundle_result = Command::new("chmod").args(["755"]).arg(app_path).output();
    match chmod_bundle_result {
        Ok(output) if output.status.success() => {
            debug!("已设置 bundle 权限为 755: {}", app_path.display());
        }
        Ok(output) => {
            warn!(
                "设置 bundle 权限失败: {} ({})",
                app_path.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(error) => {
            warn!("执行 chmod 失败: {} ({})", app_path.display(), error);
        }
    }

    let chmod_executable_result = Command::new("chmod")
        .args(["+x"])
        .arg(&executable_path)
        .output();
    match chmod_executable_result {
        Ok(output) if output.status.success() => {
            debug!("已设置可执行文件权限: {}", executable_path.display());
        }
        Ok(output) => {
            warn!(
                "设置可执行文件权限失败: {} ({})",
                executable_path.display(),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(error) => {
            warn!("执行 chmod 失败: {} ({})", executable_path.display(), error);
        }
    }

    Ok(executable_path)
}

/// 从当前可执行文件路径向上查找 `.app` bundle 根目录
///
/// macOS 应用内二进制位于 `Foo.app/Contents/MacOS/binary`，
/// 此函数从 `current_exe()` 向上遍历祖先目录，返回第一个以 `.app` 结尾的目录。
#[cfg(target_os = "macos")]
pub fn find_current_macos_app_bundle() -> AppResult<PathBuf> {
    use tracing::info;

    let exe = std::env::current_exe()?;
    info!("当前可执行文件路径: {}", exe.display());

    let mut current = exe.as_path();
    while let Some(parent) = current.parent() {
        if let Some(name) = parent.file_name().and_then(|n| n.to_str()) {
            if name.ends_with(".app") {
                info!("找到当前应用 bundle: {}", parent.display());
                return Ok(parent.to_path_buf());
            }
        }
        current = parent;
    }

    Err(AppError::Unknown(format!(
        "无法从当前可执行文件路径识别 .app bundle: {}",
        exe.display()
    )))
}

/// 在目录中递归查找 `.app` bundle
#[cfg(target_os = "macos")]
pub fn find_app_bundle_in_dir(dir: &Path) -> AppResult<PathBuf> {
    use tracing::info;

    for entry in walkdir::WalkDir::new(dir).max_depth(3) {
        let entry = entry.map_err(|e| AppError::Unknown(format!("遍历目录失败: {}", e)))?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".app") {
                    info!("在目录中找到 .app bundle: {}", path.display());
                    return Ok(path.to_path_buf());
                }
            }
        }
    }

    Err(AppError::FileNotFound(format!(
        "在 {} 中未找到 .app bundle",
        dir.display()
    )))
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use super::*;

    #[cfg(target_os = "macos")]
    use tempfile::tempdir;

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_bundle_executable_path_prefers_plist_value() {
        let dir = tempdir().unwrap();
        let app_path = dir.path().join("Eden.app");
        let contents_dir = app_path.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        std::fs::create_dir_all(&macos_dir).unwrap();
        std::fs::write(
            contents_dir.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>EdenLauncher</string>
</dict>
</plist>"#,
        )
        .unwrap();

        let executable_path = get_macos_bundle_executable_path(&app_path, Some("Eden")).unwrap();
        assert_eq!(
            executable_path,
            app_path.join("Contents/MacOS/EdenLauncher")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_bundle_executable_path_errors_without_plist_and_fallback() {
        let dir = tempdir().unwrap();
        let app_path = dir.path().join("Broken.app");
        std::fs::create_dir_all(app_path.join("Contents/MacOS")).unwrap();

        let error = get_macos_bundle_executable_path(&app_path, None).unwrap_err();
        assert!(matches!(error, AppError::FileNotFound(_)));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_get_macos_bundle_executable_path_falls_back_when_plist_value_empty() {
        let dir = tempdir().unwrap();
        let app_path = dir.path().join("Eden.app");
        let contents_dir = app_path.join("Contents");
        let macos_dir = contents_dir.join("MacOS");
        std::fs::create_dir_all(&macos_dir).unwrap();
        std::fs::write(
            contents_dir.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string></string>
</dict>
</plist>"#,
        )
        .unwrap();
        std::fs::write(macos_dir.join("eden"), b"").unwrap();

        let executable_path = get_macos_bundle_executable_path(&app_path, Some("eden")).unwrap();
        assert_eq!(executable_path, macos_dir.join("eden"));
    }
}
