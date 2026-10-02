//! 原生画面增强使用的图形接口、PE 架构与模拟器候选目标。

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsTargetCandidate {
    pub family: String,
    pub executable: PathBuf,
}
