use crate::models::{ProgressEvent, ProgressStatus, ProgressStep};
use crate::services::downloader::{create_installation_steps, DownloadProgress};
use std::sync::Arc;
use tauri::{Emitter, Window};

pub const INSTALLATION_EVENT: &str = "installation-event";

pub const STEP_FETCH_VERSION: &str = "fetch_version";
pub const STEP_DOWNLOAD: &str = "download";
pub const STEP_EXTRACT: &str = "extract";
pub const STEP_INSTALL: &str = "install";
pub const STEP_CHECK_ENV: &str = "check_env";

pub const TITLE_FETCH_VERSION: &str = "获取版本信息";
pub const TITLE_EXTRACT: &str = "解压文件";
pub const TITLE_INSTALL: &str = "安装文件";
pub const TITLE_CHECK_ENV: &str = "检查运行环境";

#[derive(Clone)]
pub struct InstallReporter {
    emit: Arc<dyn Fn(ProgressEvent) + Send + Sync>,
}

impl InstallReporter {
    pub fn new<F>(emit: F) -> Self
    where
        F: Fn(ProgressEvent) + Send + Sync + 'static,
    {
        Self {
            emit: Arc::new(emit),
        }
    }

    pub fn from_window(window: Window) -> Self {
        Self::new(move |event| {
            let _ = window.emit(INSTALLATION_EVENT, event);
        })
    }

    pub fn emit(&self, event: ProgressEvent) {
        (self.emit)(event);
    }

    pub fn start(&self, steps: Vec<ProgressStep>) {
        self.emit(ProgressEvent::Started { steps });
    }

    pub fn step(&self, step: ProgressStep) {
        self.emit(ProgressEvent::StepUpdate { step });
    }

    pub fn finish(&self, success: bool, message: Option<String>) {
        self.emit(ProgressEvent::Finished { success, message });
    }

    pub fn finish_success(&self) {
        self.finish(true, None);
    }

    pub fn finish_error(&self, message: impl Into<String>) {
        self.finish(false, Some(message.into()));
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum StepKind {
    Normal,
    Download,
}

impl StepKind {
    fn build(
        self,
        id: impl Into<String>,
        title: impl Into<String>,
        status: ProgressStatus,
    ) -> ProgressStep {
        match self {
            Self::Normal => ProgressStep::normal(id, title, status),
            Self::Download => ProgressStep::download(id, title, status),
        }
    }
}

pub fn step(
    id: impl Into<String>,
    title: impl Into<String>,
    status: ProgressStatus,
    kind: StepKind,
) -> ProgressStep {
    kind.build(id, title, status)
}

pub fn pending_step(id: impl Into<String>, title: impl Into<String>) -> ProgressStep {
    step(id, title, ProgressStatus::Pending, StepKind::Normal)
}

pub fn pending_download_step(id: impl Into<String>, title: impl Into<String>) -> ProgressStep {
    step(id, title, ProgressStatus::Pending, StepKind::Download)
}

pub fn running_step(id: impl Into<String>, title: impl Into<String>) -> ProgressStep {
    step(id, title, ProgressStatus::Running, StepKind::Normal)
}

pub fn running_download_step(id: impl Into<String>, title: impl Into<String>) -> ProgressStep {
    step(id, title, ProgressStatus::Running, StepKind::Download)
}

pub fn success_step(id: impl Into<String>, title: impl Into<String>) -> ProgressStep {
    step(id, title, ProgressStatus::Success, StepKind::Normal)
}

pub fn success_download_step(id: impl Into<String>, title: impl Into<String>) -> ProgressStep {
    step(id, title, ProgressStatus::Success, StepKind::Download)
}

pub fn cancelled_step(
    id: impl Into<String>,
    title: impl Into<String>,
    kind: StepKind,
) -> ProgressStep {
    step(id, title, ProgressStatus::Cancelled, kind)
}

pub fn error_step(
    id: impl Into<String>,
    title: impl Into<String>,
    kind: StepKind,
    error: impl Into<String>,
) -> ProgressStep {
    step(id, title, ProgressStatus::Error, kind).with_error(error)
}

pub fn download_progress_step(
    id: impl Into<String>,
    title: impl Into<String>,
    progress: &DownloadProgress,
    download_source: Option<String>,
) -> ProgressStep {
    let mut step = running_download_step(id, title)
        .with_progress(
            progress.percentage,
            progress.speed_string(),
            progress.eta_string(),
        )
        .with_download_sizes(
            progress.downloaded_string(),
            progress.total_string_or_unknown(),
        );

    if let Some(download_source) = download_source {
        step = step.with_download_source(download_source);
    }

    step
}

pub fn install_steps(download_title: impl Into<String>) -> Vec<ProgressStep> {
    let mut steps = create_installation_steps();
    steps.extend([
        pending_step(STEP_FETCH_VERSION, TITLE_FETCH_VERSION),
        pending_download_step(STEP_DOWNLOAD, download_title),
        pending_step(STEP_EXTRACT, TITLE_EXTRACT),
        pending_step(STEP_INSTALL, TITLE_INSTALL),
        pending_step(STEP_CHECK_ENV, TITLE_CHECK_ENV),
    ]);
    steps
}

pub fn cancelled_install_steps(download_title: impl Into<String>) -> Vec<ProgressStep> {
    vec![
        cancelled_step(STEP_DOWNLOAD, download_title, StepKind::Download),
        cancelled_step(STEP_EXTRACT, TITLE_EXTRACT, StepKind::Normal),
        cancelled_step(STEP_INSTALL, TITLE_INSTALL, StepKind::Normal),
        cancelled_step(STEP_CHECK_ENV, TITLE_CHECK_ENV, StepKind::Normal),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_steps_include_common_install_flow() {
        let steps = install_steps("下载 Eden");
        let ids: Vec<_> = steps.into_iter().map(|step| step.id).collect();

        assert!(ids.contains(&STEP_FETCH_VERSION.to_string()));
        assert!(ids.contains(&STEP_DOWNLOAD.to_string()));
        assert!(ids.contains(&STEP_EXTRACT.to_string()));
        assert!(ids.contains(&STEP_INSTALL.to_string()));
        assert!(ids.contains(&STEP_CHECK_ENV.to_string()));
    }

    #[test]
    fn test_cancelled_install_steps_keep_expected_statuses() {
        let steps = cancelled_install_steps("下载 Ryujinx");

        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].status, ProgressStatus::Cancelled);
        assert_eq!(steps[0].step_type, "download");
        assert_eq!(steps[1].status, ProgressStatus::Cancelled);
        assert_eq!(steps[1].step_type, "normal");
    }
}
