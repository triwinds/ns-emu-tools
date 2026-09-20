//! 图形组件只读检测契约。文件存在不代表组件已加载或兼容性已验证。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GraphicsApi {
    OpenGl,
    Vulkan,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExecutableArchitecture {
    X86,
    X64,
    Arm64,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GraphicsComponentState {
    NotInstalled,
    Installed,
    Incomplete,
    Modified,
    External,
    Unknown,
    Unsupported,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsTargetCandidate {
    pub family: String,
    pub executable: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsComponentEvidence {
    pub relative_path: String,
    /// 本阶段不推断文件版本、来源或所有权。
    pub is_regular_file: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsComponentDetection {
    pub executable: PathBuf,
    pub architecture: ExecutableArchitecture,
    /// 由用户选择；没有证据时不猜测模拟器实际使用的 API。
    pub graphics_api: Option<GraphicsApi>,
    pub supported_target: bool,
    pub installation_available: bool,
    pub reshade_state: GraphicsComponentState,
    pub feeder_state: GraphicsComponentState,
    pub compatibility_verified: bool,
    pub evidence: Vec<GraphicsComponentEvidence>,
    pub diagnostics: Vec<String>,
}

/// 本地安装包预检结果，不构成来源认证或运行兼容性证明。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsInstallPreview {
    pub target: GraphicsComponentDetection,
    pub package: PathBuf,
    pub plan_id: Option<String>,
    pub package_sha256: String,
    pub dll_sha256: String,
    pub version: Option<String>,
    pub source_url: Option<String>,
    pub dll_size: u64,
    pub destination: PathBuf,
    pub requires_external_overwrite_confirmation: bool,
    pub requires_vulkan_scope_confirmation: bool,
    pub affected_targets: Vec<PathBuf>,
    pub blockers: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphicsInstallationRecord {
    pub schema_version: u32,
    pub target_directory: PathBuf,
    pub executable: PathBuf,
    pub graphics_api: GraphicsApi,
    pub version: Option<String>,
    pub source_url: Option<String>,
    pub package_sha256: String,
    pub deployed_sha256: String,
    pub original_sha256: Option<String>,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsOperationResult {
    pub record: Option<GraphicsInstallationRecord>,
    pub message: String,
    pub compatibility_verified: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsComponentVersion {
    pub version: String,
    pub source_url: String,
    pub channel: String,
}
