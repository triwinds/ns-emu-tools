use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallationStatus {
    Pending,
    Running,
    Success,
    Error,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum InstallationEvent {
    Started { steps: Vec<InstallationStep> },
    StepRunning { id: String },
    StepSuccess { id: String },
    StepError { id: String, message: String },
    DownloadProgress { 
        id: String, 
        progress: f64, 
        speed: String, 
        eta: String 
    },
    Finished { success: bool, message: Option<String> },
}
