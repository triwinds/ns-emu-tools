//! Read-only preflight for the pinned downloadable graphics package.
//! Deliberately separate target identity from GPU support and live session state.
use super::streamline_install;
use crate::models::graphics_components::GraphicsApi;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[path = "../../../crates/streamline-target-policy.rs"]
pub(super) mod target_policy;
use target_policy::{classify, validate_executable, Compatibility, TargetFamily};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckStatus {
    Passed,
    Blocked,
    Pending,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightCheck {
    id: &'static str,
    label: &'static str,
    status: CheckStatus,
    detail: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FgPreflight {
    executable: PathBuf,
    checked_at: String,
    target_version: Option<&'static str>,
    compatibility: &'static str,
    requires_trial_confirmation: bool,
    target_sha256: Option<String>,
    checks: Vec<PreflightCheck>,
    package_available: bool,
    package_message: String,
    planned_destination: PathBuf,
    /// File installation state is separate from live frame-generation state.
    installation_state: &'static str,
    runtime_state: &'static str,
}

pub(super) fn file_digest(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|e| format!("无法读取主程序：{e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("所选路径不是普通文件".into());
    }
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let size = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if size == 0 {
            break;
        }
        digest.update(&buffer[..size]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub async fn detect_online(
    executable: PathBuf,
    graphics_api: GraphicsApi,
) -> Result<FgPreflight, String> {
    let started = std::time::Instant::now();
    let (local, update) = tokio::join!(
        tauri::async_runtime::spawn_blocking(move || detect(executable, graphics_api)),
        super::streamline_update::check(),
    );
    let mut report = local.map_err(|e| e.to_string())??;
    report.apply_update(update);
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis(),
        "画面增强组件前台检查已完成"
    );
    Ok(report)
}
impl FgPreflight {
    fn apply_update(&mut self, update: super::streamline_update::Update) {
        let installed = streamline_install::installed_package(&self.executable).ok();
        let verified_latest = update.fresh
            && update.package.as_ref().is_some_and(|latest| {
                installed.as_ref().is_some_and(|current| {
                    current.version == latest.version
                        && super::streamline_update::same_files(current, latest)
                })
            });
        let detail = match update.package {
            Some(latest) => {
                self.package_available = true;
                self.planned_destination =
                    streamline_install::planned_package(&self.executable, &latest);
                let current = installed
                    .as_ref()
                    .map(|p| p.version.as_str())
                    .unwrap_or("未知");
                if self.installation_state == "installed"
                    && current == latest.version
                    && installed
                        .as_ref()
                        .is_some_and(|p| !super::streamline_update::same_files(p, &latest))
                {
                    format!(
                        "远端同版本组件的校验值发生变化：{}。请等待发布新版本；当前安装保持可用。",
                        latest.version
                    )
                } else if self.installation_state == "installed" && current != latest.version {
                    self.installation_state = "outdated";
                    format!("发现组件小包更新：{current} → {}。点击下载并安装全部组件即可更新；稳定包缓存将复用，当前游戏会话保持不变。", latest.version)
                } else if self.installation_state == "installed" && update.fresh {
                    format!("已安装最新组件小包：{}。文件校验已通过。", latest.version)
                } else if self.installation_state == "installed" {
                    format!("已安装组件小包：{}。{}", latest.version, update.message)
                } else {
                    format!(
                        "可安装组件小包：{}。自动下载并校验小包和配套稳定包。",
                        latest.version
                    )
                }
            }
            None => format!("{} {}", self.package_message, update.message),
        };
        self.package_message = detail.clone();
        self.checks.push(PreflightCheck {
            id: "component-version",
            label: "组件小包版本",
            status: if self.installation_state == "installed" && verified_latest {
                CheckStatus::Passed
            } else {
                CheckStatus::Pending
            },
            detail,
        });
    }
}

pub fn detect(executable: PathBuf, graphics_api: GraphicsApi) -> Result<FgPreflight, String> {
    let executable = executable
        .canonicalize()
        .map_err(|e| format!("无法访问模拟器：{e}"))?;
    let mut checks = Vec::new();
    let mut check = |id, label, status, detail| {
        checks.push(PreflightCheck {
            id,
            label,
            status,
            detail,
        })
    };
    check(
        "host",
        "操作系统",
        if cfg!(all(windows, target_arch = "x86_64")) {
            CheckStatus::Passed
        } else {
            CheckStatus::Blocked
        },
        "首版支持 Windows x64".into(),
    );
    let hash = file_digest(&executable)?;
    let family = TargetFamily::detect(&executable, &hash);
    let validation = validate_executable(&executable);
    let compatibility = classify(&hash, validation.is_ok());
    let requires_trial_confirmation =
        compatibility == Compatibility::Unverified && compatibility.authorize(false).is_err();
    check(
        "target",
        "模拟器版本",
        match compatibility {
            Compatibility::Verified => CheckStatus::Passed,
            Compatibility::Unverified => CheckStatus::Pending,
            Compatibility::Incompatible => CheckStatus::Blocked,
        },
        match compatibility {
            Compatibility::Verified => "已验证构建：Ryujinx Canary 1.3.351".into(),
            Compatibility::Unverified => {
                "兼容性未验证。可明确选择尝试，启动时仍需通过显卡能力和运行条件检查".into()
            }
            Compatibility::Incompatible => validation.unwrap_err(),
        },
    );
    check(
        "api",
        "图形接口",
        if graphics_api == GraphicsApi::Vulkan {
            CheckStatus::Passed
        } else {
            CheckStatus::Blocked
        },
        if family == TargetFamily::Yuzu {
            "需要在 Eden / yuzu 系列的全局和游戏专属设置中选择 Vulkan；专用启动不会改写配置。请关闭模拟器内置插帧，避免与 FG 重复处理".into()
        } else {
            "需要 Vulkan；此处按页面选择检查，启动前仍需核对模拟器设置".into()
        },
    );
    if family == TargetFamily::Yuzu {
        check(
            "source",
            "画面输入",
            CheckStatus::Pending,
            format!("{} 系列使用最终呈现画面试运行 NR、SR / DLAA 和 FG，包含黑边与叠加界面。尚未接入游戏原生纹理、深度或运动矢量；SR 不降低模拟器渲染分辨率，实际能力由运行时检查。需要更新增强组件以使用此启动方式", family.as_str()),
        );
    }
    let directory = executable.parent().ok_or("主程序没有父目录")?;
    let mut conflicts = Vec::new();
    for name in [
        "ReShade64.dll",
        "opengl32.dll",
        "dlss5-feed.addon64",
        "renodx-dlss5.addon64",
    ] {
        match directory.join(name).symlink_metadata() {
            Ok(_) => conflicts.push(name),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(format!("无法检查 {name}：{e}")),
        }
    }
    check(
        "layers",
        "其他图形组件",
        CheckStatus::Pending,
        if conflicts.is_empty() {
            "目标目录未发现已知冲突文件；全局 Vulkan 图层仍需在启动前检查".into()
        } else {
            format!(
                "发现 {}。组合使用尚未验证，安装器不会自动移除这些文件",
                conflicts.join("、")
            )
        },
    );
    check(
        "gpu",
        "显卡与驱动",
        CheckStatus::Pending,
        "需要支持 DLSS 帧生成的 NVIDIA 显卡；由运行时检查实际能力，当前尚未检测".into(),
    );
    let package = streamline_install::availability();
    Ok(FgPreflight {
        planned_destination: streamline_install::planned(&executable),
        installation_state: streamline_install::state(&executable),
        executable,
        checked_at: chrono::Utc::now().to_rfc3339(),
        target_version: (compatibility == Compatibility::Verified)
            .then_some("Ryujinx Canary 1.3.351"),
        compatibility: compatibility.as_str(),
        requires_trial_confirmation,
        target_sha256: Some(hash),
        checks,
        package_available: package.is_ok() || super::streamline_download::available(),
        package_message: match package {
            Ok(()) => "画面增强组件已缓存并通过完整校验。普通游戏不设帧数上限；失焦时暂停、回到前台可恢复；窗口操作或运行条件不满足仍可能停用 FG。".into(),
            Err(_) if super::streamline_download::available() => "安装时自动从 runtimes 仓库下载并校验 NR、SR / DLAA、FG 与配套图层，无需手动寻找 DLL。".into(),
            Err(error) => error,
        },
        runtime_state: "unknown",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Read-only preflight timing using FG_CHECK_TARGET and optional FG_CHECK_CONFIG"]
    async fn local_preflight_timing() {
        if let Some(path) = std::env::var_os("FG_CHECK_CONFIG") {
            let config: crate::config::Config =
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
            crate::config::CONFIG.write().setting.network = config.setting.network;
        }
        let target = PathBuf::from(std::env::var_os("FG_CHECK_TARGET").expect("FG_CHECK_TARGET"));
        let started = std::time::Instant::now();
        let report = detect_online(target.clone(), GraphicsApi::Vulkan)
            .await
            .unwrap();
        println!(
            "First preflight: {:?}; {}",
            started.elapsed(),
            report
                .checks
                .iter()
                .find(|c| c.id == "component-version")
                .unwrap()
                .detail
        );
        tokio::time::sleep(std::time::Duration::from_secs(8)).await;
        let started = std::time::Instant::now();
        let report = detect_online(target, GraphicsApi::Vulkan).await.unwrap();
        println!(
            "Cached preflight: {:?}; {}",
            started.elapsed(),
            report
                .checks
                .iter()
                .find(|c| c.id == "component-version")
                .unwrap()
                .detail
        );
    }
    #[test]
    fn remote_update_exposes_install_action_while_offline_report_retains_installed_state() {
        let make_report = || FgPreflight {
            executable: std::env::temp_dir().join("unused-preflight-test.exe"),
            checked_at: String::new(),
            target_version: None,
            compatibility: "verified",
            requires_trial_confirmation: false,
            target_sha256: None,
            checks: vec![],
            package_available: false,
            package_message: String::new(),
            planned_destination: PathBuf::new(),
            installation_state: "installed",
            runtime_state: "unknown",
        };
        let latest = super::super::streamline_update::tests::fixture();
        let mut report = make_report();
        report.apply_update(super::super::streamline_update::Update {
            package: Some(latest),
            message: String::new(),
            fresh: true,
        });
        assert_eq!(report.installation_state, "outdated");
        assert!(report.package_available && report.package_message.contains("发现组件小包更新"));
        assert!(matches!(
            report.checks.last().unwrap().status,
            CheckStatus::Pending
        ));
        let mut report = make_report();
        report.apply_update(super::super::streamline_update::Update {
            package: None,
            message: "未能检查远端组件更新".into(),
            fresh: false,
        });
        assert_eq!(report.installation_state, "installed");
        assert!(matches!(
            report.checks.last().unwrap().status,
            CheckStatus::Pending
        ));
    }
    #[test]
    fn new_x64_build_is_unverified_but_dll_and_x86_are_incompatible() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Ryujinx.exe");
        let mut bytes = vec![0u8; 512];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
        bytes[70..72].copy_from_slice(&1u16.to_le_bytes());
        bytes[84..86].copy_from_slice(&112u16.to_le_bytes());
        bytes[86..88].copy_from_slice(&2u16.to_le_bytes());
        bytes[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
        std::fs::write(&exe, &bytes).unwrap();
        let result = detect(exe.clone(), GraphicsApi::Vulkan).unwrap();
        assert_eq!(result.compatibility, "unverified");
        assert!(result.requires_trial_confirmation);
        assert!(result.target_version.is_none());
        assert!(matches!(
            result
                .checks
                .iter()
                .find(|c| c.id == "target")
                .unwrap()
                .status,
            CheckStatus::Pending
        ));
        bytes[86..88].copy_from_slice(&0x2002u16.to_le_bytes());
        std::fs::write(&exe, &bytes).unwrap();
        assert_eq!(
            detect(exe.clone(), GraphicsApi::Vulkan)
                .unwrap()
                .compatibility,
            "incompatible"
        );
        bytes[86..88].copy_from_slice(&2u16.to_le_bytes());
        bytes[68..70].copy_from_slice(&0x14cu16.to_le_bytes());
        std::fs::write(&exe, &bytes).unwrap();
        assert_eq!(
            detect(exe, GraphicsApi::Vulkan).unwrap().compatibility,
            "incompatible"
        );
    }
    #[test]
    fn rejects_missing_target() {
        let dir = tempfile::tempdir().unwrap();
        assert!(detect(dir.path().join("missing.exe"), GraphicsApi::Vulkan).is_err());
    }
    #[test]
    fn eden_trial_reports_present_input_without_claiming_a_verified_build() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("eden.exe");
        std::fs::write(&exe, super::super::tests::pe(0x8664, 0x20b)).unwrap();
        let report = detect(exe, GraphicsApi::Vulkan).unwrap();
        assert_eq!(report.compatibility, "unverified");
        assert!(report.requires_trial_confirmation);
        assert!(report.target_version.is_none());
        let source = report.checks.iter().find(|c| c.id == "source").unwrap();
        assert!(matches!(source.status, CheckStatus::Pending));
        assert!(source.detail.contains("最终呈现画面"));
        assert!(report
            .checks
            .iter()
            .find(|c| c.id == "api")
            .unwrap()
            .detail
            .contains("游戏专属设置"));
    }
    #[test]
    fn unknown_executable_cannot_pass_identity_or_claim_gpu_support() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Ryujinx.exe");
        std::fs::write(&exe, b"not the frozen executable").unwrap();
        std::fs::write(dir.path().join("ReShade64.dll"), b"user file").unwrap();
        let result = detect(exe, GraphicsApi::OpenGl).unwrap();
        assert!(matches!(
            result
                .checks
                .iter()
                .find(|c| c.id == "target")
                .unwrap()
                .status,
            CheckStatus::Blocked
        ));
        assert!(matches!(
            result.checks.iter().find(|c| c.id == "api").unwrap().status,
            CheckStatus::Blocked
        ));
        assert!(matches!(
            result.checks.iter().find(|c| c.id == "gpu").unwrap().status,
            CheckStatus::Pending
        ));

        assert_eq!(result.runtime_state, "unknown");
        assert_eq!(
            std::fs::read(dir.path().join("ReShade64.dll")).unwrap(),
            b"user file"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
