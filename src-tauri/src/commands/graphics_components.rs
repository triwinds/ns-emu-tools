//! 原生画面增强的目标枚举、自动下载、安装与会话控制。
use crate::models::graphics_components::{GraphicsApi, GraphicsTargetCandidate};
use crate::models::response::ApiResponse;
use crate::services::graphics_components;
use std::path::PathBuf;

#[tauri::command]
pub fn validate_nr_preset(json: String) -> ApiResponse<crate::config::nr_presets::ImportResult> {
    match crate::config::nr_presets::import(&json) {
        Ok(result) => ApiResponse::success(result),
        Err(error) => ApiResponse::fail(error),
    }
}

#[tauri::command]
pub async fn nr_preset_environment(
    executable: PathBuf,
) -> ApiResponse<crate::config::nr_presets::Environment> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::nr_presets::environment(&executable)
    })
    .await
    {
        Ok(result) => ApiResponse::success(result),
        Err(error) => ApiResponse::fail(error.to_string()),
    }
}

#[tauri::command]
pub async fn list_graphics_component_targets() -> ApiResponse<Vec<GraphicsTargetCandidate>> {
    match tauri::async_runtime::spawn_blocking(graphics_components::list_targets).await {
        Ok(Ok(targets)) => ApiResponse::success(targets),
        Ok(Err(error)) => ApiResponse::fail(error),
        Err(error) => ApiResponse::fail(format!("目标检测任务失败: {error}")),
    }
}

/// Read-only FG preflight; never infers live FG state from installed files.
#[tauri::command]
pub async fn detect_streamline_fg(
    executable: PathBuf,
    graphics_api: GraphicsApi,
) -> ApiResponse<graphics_components::streamline_fg::FgPreflight> {
    match graphics_components::streamline_fg::detect_online(executable, graphics_api).await {
        Ok(report) => ApiResponse::success(report),
        Err(error) => ApiResponse::fail(error),
    }
}

#[tauri::command]
pub async fn install_streamline_fg(
    window: tauri::Window,
    executable: PathBuf,
    graphics_api: GraphicsApi,
    allow_unverified: bool,
    expected_sha256: String,
) -> ApiResponse<graphics_components::streamline_install::Operation> {
    use crate::services::installer::*;
    let reporter = InstallReporter::from_window(window);
    reporter.start(vec![
        pending_download_step("download", "下载并校验画面增强组件"),
        pending_step("install", "安装画面增强组件"),
    ]);
    let validation_exe = executable.clone();
    let validation_hash = expected_sha256.clone();
    let result = async {
        tauri::async_runtime::spawn_blocking(move || {
            graphics_components::streamline_install::authorize(
                &validation_exe,
                graphics_api,
                allow_unverified,
                &validation_hash,
            )
        })
        .await
        .map_err(|e| e.to_string())??;
        graphics_components::streamline_install::install_latest(
            executable,
            graphics_api,
            allow_unverified,
            expected_sha256,
            reporter.clone(),
        )
        .await
    }
    .await;
    match result {
        Ok(value) => {
            reporter.step(success_step("install", "画面增强组件已安装"));
            reporter.finish_success();
            ApiResponse::success(value)
        }
        Err(error) => {
            reporter.finish_error(&error);
            ApiResponse::fail(error)
        }
    }
}

#[tauri::command]
pub async fn launch_streamline_fg(
    executable: PathBuf,
    graphics_api: GraphicsApi,
    allow_unverified: bool,
    expected_sha256: String,
    game: Option<PathBuf>,
) -> ApiResponse<graphics_components::streamline_install::Operation> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::streamline_install::launch(
            executable,
            graphics_api,
            allow_unverified,
            expected_sha256,
            game,
        )
    })
    .await
    {
        Ok(Ok(v)) => ApiResponse::success(v),
        Ok(Err(e)) => ApiResponse::fail(e),
        Err(e) => ApiResponse::fail(e.to_string()),
    }
}

#[tauri::command]
pub async fn uninstall_streamline_fg(
    executable: PathBuf,
) -> ApiResponse<graphics_components::streamline_install::Operation> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::streamline_install::uninstall(executable)
    })
    .await
    {
        Ok(Ok(v)) => ApiResponse::success(v),
        Ok(Err(e)) => ApiResponse::fail(e),
        Err(e) => ApiResponse::fail(e.to_string()),
    }
}

#[tauri::command]
pub async fn live_streamline_fg(
    executable: PathBuf,
    enabled: Option<bool>,
    sr_mode: Option<String>,
    sr_scale: Option<u16>,
    sr_preset: Option<String>,
    nr_enabled: Option<bool>,
    nr_intensity: Option<f32>,
    advanced: Option<crate::config::advanced_settings::AdvancedUpdate>,
) -> ApiResponse<serde_json::Value> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::streamline_install::live(
            executable,
            enabled,
            sr_mode,
            sr_scale,
            sr_preset,
            nr_enabled,
            nr_intensity,
            advanced,
        )
    })
    .await
    {
        Ok(Ok(v)) => ApiResponse::success(v),
        Ok(Err(e)) => ApiResponse::fail(e),
        Err(e) => ApiResponse::fail(e.to_string()),
    }
}

#[tauri::command]
pub async fn get_native_nr_component() -> ApiResponse<graphics_components::native_nr::Component> {
    match tauri::async_runtime::spawn_blocking(graphics_components::native_nr::status).await {
        Ok(status) => ApiResponse::success(status),
        Err(error) => ApiResponse::fail(error.to_string()),
    }
}
#[tauri::command]
pub async fn install_native_nr_runtime(
    window: tauri::Window,
) -> ApiResponse<graphics_components::native_nr::Component> {
    use crate::services::installer::*;
    let reporter = InstallReporter::from_window(window);
    reporter.start(vec![
        pending_download_step("download", "下载并校验 NR 组件"),
        pending_step("install", "安装 NR 组件"),
    ]);
    match graphics_components::native_nr::install(reporter.clone()).await {
        Ok(status) => {
            reporter.step(success_step("install", "NR 组件已安装"));
            reporter.finish_success();
            ApiResponse::success(status)
        }
        Err(error) => {
            reporter.finish_error(&error);
            ApiResponse::fail(error)
        }
    }
}
#[tauri::command]
pub async fn uninstall_native_nr_runtime() -> ApiResponse<graphics_components::native_nr::Component>
{
    match tauri::async_runtime::spawn_blocking(graphics_components::native_nr::uninstall).await {
        Ok(Ok(status)) => ApiResponse::success(status),
        Ok(Err(error)) => ApiResponse::fail(error),
        Err(error) => ApiResponse::fail(error.to_string()),
    }
}
