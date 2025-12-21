use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallationStatus {
    Pending,
    Running,
    Success,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallationStep {
    pub id: String,
    pub title: String,
    pub status: InstallationStatus,
    #[serde(rename = "type")]
    pub step_type: String, // "normal" or "download"
    #[serde(default)]
    pub progress: f64,
    #[serde(default)]
    pub download_speed: String,
    #[serde(default)]
    pub eta: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum InstallationEvent {
    Started { steps: Vec<InstallationStep> },
    StepUpdate { step: InstallationStep },
    Finished { success: bool, message: Option<String> },
}
