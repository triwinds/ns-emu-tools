use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloaded_size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_size: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_source: Option<String>,
}

impl Default for ProgressStep {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            status: ProgressStatus::Pending,
            step_type: String::new(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        }
    }
}

impl ProgressStep {
    /// 创建一个普通步骤
    pub fn normal(id: impl Into<String>, title: impl Into<String>, status: ProgressStatus) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            status,
            step_type: "normal".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        }
    }

    /// 创建一个下载步骤
    pub fn download(
        id: impl Into<String>,
        title: impl Into<String>,
        status: ProgressStatus,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            status,
            step_type: "download".to_string(),
            progress: 0.0,
            download_speed: String::new(),
            eta: String::new(),
            downloaded_size: None,
            total_size: None,
            error: None,
            download_source: None,
        }
    }

    /// 创建一个带错误的步骤
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// 设置下载源
    pub fn with_download_source(mut self, source: impl Into<String>) -> Self {
        self.download_source = Some(source.into());
        self
    }

    /// 设置进度
    pub fn with_progress(
        mut self,
        progress: f64,
        speed: impl Into<String>,
        eta: impl Into<String>,
    ) -> Self {
        self.progress = progress;
        self.download_speed = speed.into();
        self.eta = eta.into();
        self
    }

    pub fn with_download_sizes(
        mut self,
        downloaded_size: impl Into<String>,
        total_size: impl Into<String>,
    ) -> Self {
        self.downloaded_size = Some(downloaded_size.into());
        self.total_size = Some(total_size.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ProgressEvent {
    Started {
        steps: Vec<ProgressStep>,
    },
    StepUpdate {
        step: ProgressStep,
    },
    Finished {
        success: bool,
        message: Option<String>,
    },
    CorruptedFile {
        path: String,
    },
}
