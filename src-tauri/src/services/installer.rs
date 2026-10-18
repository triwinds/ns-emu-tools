use crate::models::{ProgressEvent, ProgressStatus, ProgressStep};
use crate::services::downloader::{create_installation_steps, DownloadProgress};
use std::sync::{Arc, Mutex};
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
    current: Arc<Mutex<Option<ProgressStep>>>,
}

impl InstallReporter {
    pub fn new<F>(emit: F) -> Self
    where
        F: Fn(ProgressEvent) + Send + Sync + 'static,
    {
        Self {
            emit: Arc::new(emit),
            current: Arc::new(Mutex::new(None)),
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
        *self.current.lock().unwrap() = steps
            .iter()
            .find(|step| step.status == ProgressStatus::Running)
            .cloned();
        self.emit(ProgressEvent::Started { steps });
    }

    pub fn step(&self, step: ProgressStep) {
        {
            let mut current = self.current.lock().unwrap();
            if step.status == ProgressStatus::Running {
                *current = Some(step.clone());
            } else if current.as_ref().is_some_and(|active| active.id == step.id) {
                *current = None;
            }
        }
        self.emit(ProgressEvent::StepUpdate { step });
    }

    fn complete_step<T, E: std::fmt::Display>(
        &self,
        id: &str,
        title: &str,
        result: Result<T, E>,
    ) -> Result<T, E> {
        self.step(match &result {
            Ok(_) => success_step(id, title),
            Err(error) => error_step(id, title, StepKind::Normal, error.to_string()),
        });
        result
    }

    pub fn run_step<T, E: std::fmt::Display>(
        &self,
        id: &str,
        title: &str,
        operation: impl FnOnce() -> Result<T, E>,
    ) -> Result<T, E> {
        self.step(running_step(id, title));
        self.complete_step(id, title, operation())
    }

    pub async fn run_blocking_step<T, F>(
        &self,
        id: &str,
        title: &str,
        operation: F,
    ) -> Result<T, String>
    where
        T: Send + 'static,
        F: FnOnce() -> Result<T, String> + Send + 'static,
    {
        self.step(running_step(id, title));
        let result = tauri::async_runtime::spawn_blocking(operation)
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
        self.complete_step(id, title, result)
    }

    pub fn finish(&self, success: bool, message: Option<String>) {
        self.emit(ProgressEvent::Finished { success, message });
    }

    pub fn finish_success(&self) {
        self.finish(true, None);
    }

    pub fn finish_error(&self, message: impl Into<String>) {
        let message = message.into();
        let current = self.current.lock().unwrap().take();
        if let Some(mut step) = current {
            step.status = if is_cancelled_error_message(&message) {
                ProgressStatus::Cancelled
            } else {
                ProgressStatus::Error
            };
            step.error = Some(message.clone());
            self.step(step);
        }
        self.finish(false, Some(message));
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

pub fn is_cancelled_error_message(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    message.contains("取消") || normalized.contains("cancelled") || normalized.contains("canceled")
}

pub fn emit_steps<F, I>(emit: &F, steps: I)
where
    F: Fn(ProgressEvent),
    I: IntoIterator<Item = ProgressStep>,
{
    for step in steps {
        emit(ProgressEvent::StepUpdate { step });
    }
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

    fn record_progress() -> (InstallReporter, Arc<Mutex<Vec<ProgressEvent>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let recorded = events.clone();
        let reporter = InstallReporter::new(move |event| recorded.lock().unwrap().push(event));
        (reporter, events)
    }

    #[tokio::test]
    async fn progress_cached_download_starts_install_before_blocking_work() {
        let (reporter, events) = record_progress();
        reporter.start(vec![
            running_download_step("download", "校验组件缓存"),
            pending_step("install", "安装组件"),
        ]);
        reporter.step(success_download_step("download", "组件缓存已校验"));
        let during_install = events.clone();
        let value = reporter
            .clone()
            .run_blocking_step("install", "安装组件", move || {
                let events = during_install.lock().unwrap();
                assert!(
                    matches!(events.last(), Some(ProgressEvent::StepUpdate { step })
                    if step.id == "install" && step.status == ProgressStatus::Running)
                );
                Ok(42)
            })
            .await
            .unwrap();
        assert_eq!(value, 42);
        reporter.finish_success();
        let events = events.lock().unwrap();
        assert!(matches!(&events[0], ProgressEvent::Started { steps }
            if steps[0].status == ProgressStatus::Running && steps[1].status == ProgressStatus::Pending));
        assert!(matches!(&events[3], ProgressEvent::StepUpdate { step }
            if step.id == "install" && step.status == ProgressStatus::Success));
        assert!(matches!(
            events.last(),
            Some(ProgressEvent::Finished { success: true, .. })
        ));
    }

    #[tokio::test]
    async fn progress_failed_blocking_install_finishes_the_correct_step_once() {
        let (reporter, events) = record_progress();
        let result = reporter
            .run_blocking_step("install", "安装 NR 组件", || {
                Err::<(), _>("文件被占用".into())
            })
            .await;
        reporter.finish_error(result.unwrap_err());
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 3);
        assert!(matches!(&events[1], ProgressEvent::StepUpdate { step }
            if step.id == "install" && step.status == ProgressStatus::Error
                && step.error.as_deref() == Some("文件被占用")));
        assert!(matches!(
            &events[2],
            ProgressEvent::Finished { success: false, .. }
        ));
    }

    #[tokio::test]
    async fn progress_blocking_task_panic_does_not_leave_install_running() {
        let (reporter, events) = record_progress();
        assert!(reporter
            .run_blocking_step("install", "安装组件", || -> Result<(), String> {
                panic!("test worker failure")
            })
            .await
            .is_err());
        assert!(
            matches!(events.lock().unwrap().last(), Some(ProgressEvent::StepUpdate { step })
            if step.status == ProgressStatus::Error)
        );
    }

    #[test]
    fn progress_download_failure_and_cancellation_preserve_the_active_step() {
        for (message, expected) in [
            ("校验失败", ProgressStatus::Error),
            ("download cancelled", ProgressStatus::Cancelled),
        ] {
            let (reporter, events) = record_progress();
            reporter.start(vec![
                running_download_step("download", "校验组件缓存"),
                pending_step("install", "安装组件"),
            ]);
            reporter.clone().finish_error(message);
            let events = events.lock().unwrap();
            assert!(matches!(&events[1], ProgressEvent::StepUpdate { step }
                if step.id == "download" && step.status == expected && step.step_type == "download"));
            assert!(matches!(
                &events[2],
                ProgressEvent::Finished { success: false, .. }
            ));
        }
    }

    #[test]
    fn progress_sync_phases_stop_at_the_failed_stage() {
        let (reporter, events) = record_progress();
        reporter
            .run_step("find_nca", "查找固件文件", || Ok::<_, String>(()))
            .unwrap();
        let during_extract = events.clone();
        let result = reporter.run_step("extract_version", "提取版本信息", || {
            let events = during_extract.lock().unwrap();
            assert!(
                matches!(events.last(), Some(ProgressEvent::StepUpdate { step })
                if step.id == "extract_version" && step.status == ProgressStatus::Running)
            );
            Err::<(), _>("NCA 损坏")
        });
        assert!(result.is_err());
        reporter.finish_error("NCA 损坏");
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 5);
        assert!(matches!(&events[1], ProgressEvent::StepUpdate { step }
            if step.id == "find_nca" && step.status == ProgressStatus::Success));
        assert!(matches!(&events[3], ProgressEvent::StepUpdate { step }
            if step.id == "extract_version" && step.status == ProgressStatus::Error));
    }

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
