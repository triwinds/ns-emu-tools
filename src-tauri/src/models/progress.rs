use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgressStatus {
    Pending,
    Running,
    Success,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressStep {
    pub id: String,
    pub title: String,
    pub status: ProgressStatus,
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
pub enum ProgressEvent {
    Started { steps: Vec<ProgressStep> },
    StepUpdate { step: ProgressStep },
    Finished { success: bool, message: Option<String> },
}
