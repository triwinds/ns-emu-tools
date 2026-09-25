//! 图形组件命令：只读检测不联网；显式准备官方包才下载，执行计划才修改模拟器目录。
//!
//! OpenGL/Vulkan 调用流程：`prepare_official_graphics_component_install`（官方包）或
//! `plan_graphics_component_install`（本地包）返回预览。检查 blockers 和外部覆盖提示后，
//! 将 planId 与 confirmExternalOverwrite 传给 `install_graphics_components`。
//! 计划为后端持有的一次性快照，有效期 15 分钟，不接受前端写入列表。
//! 下载取消沿用 `cancel_download_command`；写入阶段完成当前事务或回滚。
//! `get_graphics_component_installation` 可查看版本、来源、哈希和首次备份摘要。
//! `uninstall_graphics_components` 恢复首次备份，`repair_graphics_components`
//! 回滚未完成事务。Vulkan 需传 graphicsApi=vulkan，安装还需 confirmVulkanScope=true。
//! Vulkan 创建缺失的 ReShade.ini 并管理自有 HKCU 注册；保留用户已有和修改过的配置。

use crate::models::graphics_components::{
    GraphicsApi, GraphicsComponentDetection, GraphicsTargetCandidate,
};
use crate::models::response::ApiResponse;
use crate::services::graphics_components;
use std::path::PathBuf;

#[tauri::command]
pub async fn list_graphics_component_targets() -> ApiResponse<Vec<GraphicsTargetCandidate>> {
    match tauri::async_runtime::spawn_blocking(graphics_components::list_targets).await {
        Ok(Ok(targets)) => ApiResponse::success(targets),
        Ok(Err(error)) => ApiResponse::fail(error),
        Err(error) => ApiResponse::fail(format!("目标检测任务失败: {error}")),
    }
}

#[tauri::command]
pub async fn detect_graphics_components(
    executable: PathBuf,
    graphics_api: Option<GraphicsApi>,
) -> ApiResponse<GraphicsComponentDetection> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::detect(executable, graphics_api)
    })
    .await
    {
        Ok(report) => ApiResponse::success(report),
        Err(error) => ApiResponse::fail(format!("组件检测任务失败: {error}")),
    }
}

/// 预检本地包；后续安装不得直接信任前端传回的预览或写入列表。
#[tauri::command]
pub async fn plan_graphics_component_install(
    executable: PathBuf,
    graphics_api: GraphicsApi,
    package: PathBuf,
) -> ApiResponse<crate::models::graphics_components::GraphicsInstallPreview> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::planning::preview(executable, graphics_api, package)
    })
    .await
    {
        Ok(Ok(plan)) => ApiResponse::success(plan),
        Ok(Err(error)) => ApiResponse::fail(error),
        Err(error) => ApiResponse::fail(format!("安装预检任务失败: {error}")),
    }
}

#[tauri::command]
pub async fn get_graphics_component_versions(
) -> ApiResponse<crate::models::graphics_components::GraphicsComponentVersion> {
    match crate::repositories::graphics_components::latest().await {
        Ok(release) => ApiResponse::success(release),
        Err(error) => ApiResponse::fail(error),
    }
}

#[tauri::command]
pub async fn prepare_official_graphics_component_install(
    window: tauri::Window,
    executable: PathBuf,
    graphics_api: GraphicsApi,
) -> ApiResponse<crate::models::graphics_components::GraphicsInstallPreview> {
    use crate::services::installer::*;
    let reporter = InstallReporter::from_window(window);
    reporter.start(vec![
        pending_step("source", "解析官方版本"),
        pending_download_step("download", "下载 ReShade"),
    ]);
    let result =
        graphics_components::packages::prepare_official(executable, graphics_api, reporter.clone())
            .await;
    match result {
        Ok(plan) => {
            reporter.finish_success();
            ApiResponse::success(plan)
        }
        Err(error) => {
            reporter.finish_error(&error);
            ApiResponse::fail(error)
        }
    }
}

async fn run_graphics_operation(
    window: tauri::Window,
    operation: impl FnOnce() -> Result<crate::models::graphics_components::GraphicsOperationResult, String>
        + Send
        + 'static,
) -> ApiResponse<crate::models::graphics_components::GraphicsOperationResult> {
    use crate::services::installer::*;
    let reporter = InstallReporter::from_window(window);
    reporter.start(vec![pending_step(
        "graphics_transaction",
        "备份、部署与验证",
    )]);
    reporter.step(running_step("graphics_transaction", "执行可恢复事务"));
    match tauri::async_runtime::spawn_blocking(operation).await {
        Ok(Ok(result)) => {
            reporter.step(success_step("graphics_transaction", "事务完成"));
            reporter.finish_success();
            ApiResponse::success(result)
        }
        result => {
            let error = match result {
                Ok(Err(error)) => error,
                Err(error) => error.to_string(),
                _ => unreachable!(),
            };
            reporter.step(error_step(
                "graphics_transaction",
                "事务未完成",
                StepKind::Normal,
                &error,
            ));
            reporter.finish_error(&error);
            ApiResponse::fail(error)
        }
    }
}

#[tauri::command]
pub async fn install_graphics_components(
    window: tauri::Window,
    plan_id: String,
    confirm_external_overwrite: bool,
    confirm_vulkan_scope: Option<bool>,
) -> ApiResponse<crate::models::graphics_components::GraphicsOperationResult> {
    run_graphics_operation(window, move || {
        graphics_components::planning::install_with_scope(
            plan_id,
            confirm_external_overwrite,
            confirm_vulkan_scope.unwrap_or(false),
        )
    })
    .await
}

#[tauri::command]
pub async fn uninstall_graphics_components(
    window: tauri::Window,
    executable: PathBuf,
    graphics_api: Option<GraphicsApi>,
) -> ApiResponse<crate::models::graphics_components::GraphicsOperationResult> {
    run_graphics_operation(window, move || {
        if graphics_api == Some(GraphicsApi::Vulkan) {
            graphics_components::vulkan::remove_or_repair(executable, false)
        } else {
            graphics_components::transaction::remove_or_repair(executable, false)
        }
    })
    .await
}

#[tauri::command]
pub async fn repair_graphics_components(
    window: tauri::Window,
    executable: PathBuf,
    graphics_api: Option<GraphicsApi>,
) -> ApiResponse<crate::models::graphics_components::GraphicsOperationResult> {
    run_graphics_operation(window, move || {
        if graphics_api == Some(GraphicsApi::Vulkan) {
            graphics_components::vulkan::remove_or_repair(executable, true)
        } else {
            graphics_components::transaction::remove_or_repair(executable, true)
        }
    })
    .await
}

#[tauri::command]
pub async fn get_graphics_component_installation(
    executable: PathBuf,
    graphics_api: Option<GraphicsApi>,
) -> ApiResponse<Option<crate::models::graphics_components::GraphicsInstallationRecord>> {
    match tauri::async_runtime::spawn_blocking(move || {
        let exe = executable.canonicalize().map_err(|e| e.to_string())?;
        if graphics_api == Some(GraphicsApi::Vulkan) {
            graphics_components::vulkan::read_record(&exe)
        } else {
            graphics_components::transaction::read_record(exe.parent().ok_or("目标没有父目录")?)
        }
    })
    .await
    {
        Ok(Ok(record)) => ApiResponse::success(record),
        Ok(Err(error)) => ApiResponse::fail(error),
        Err(error) => ApiResponse::fail(error.to_string()),
    }
}

/// 固定 RHI 镜像组合：仅准备和预检，不直接修改模拟器。
#[tauri::command]
pub async fn prepare_feeder_install(
    window: tauri::Window,
    executable: PathBuf,
    graphics_api: GraphicsApi,
) -> ApiResponse<graphics_components::feeder::Preview> {
    use crate::services::installer::*;
    let reporter = InstallReporter::from_window(window);
    reporter.start(vec![pending_download_step(
        "download",
        "下载并校验 Feeder / RHI 组件",
    )]);
    match graphics_components::feeder::prepare(executable, graphics_api, reporter.clone()).await {
        Ok(plan) => {
            reporter.finish_success();
            ApiResponse::success(plan)
        }
        Err(e) => {
            reporter.finish_error(&e);
            ApiResponse::fail(e)
        }
    }
}
async fn run_feeder_operation(
    window: tauri::Window,
    operation: impl FnOnce() -> Result<graphics_components::feeder::Operation, String> + Send + 'static,
) -> ApiResponse<graphics_components::feeder::Operation> {
    use crate::services::installer::*;
    let reporter = InstallReporter::from_window(window);
    reporter.start(vec![pending_step(
        "feeder_transaction",
        "部署或恢复 Feeder",
    )]);
    match tauri::async_runtime::spawn_blocking(operation).await {
        Ok(Ok(result)) => {
            reporter.finish_success();
            ApiResponse::success(result)
        }
        result => {
            let error = match result {
                Ok(Err(e)) => e,
                Err(e) => e.to_string(),
                _ => unreachable!(),
            };
            reporter.finish_error(&error);
            ApiResponse::fail(error)
        }
    }
}
#[tauri::command]
pub async fn install_feeder(
    window: tauri::Window,
    plan_id: String,
) -> ApiResponse<graphics_components::feeder::Operation> {
    run_feeder_operation(window, move || {
        graphics_components::feeder::install(plan_id)
    })
    .await
}
#[tauri::command]
pub async fn uninstall_feeder(
    window: tauri::Window,
    executable: PathBuf,
) -> ApiResponse<graphics_components::feeder::Operation> {
    run_feeder_operation(window, move || {
        graphics_components::feeder::remove_or_repair(executable, false)
    })
    .await
}
#[tauri::command]
pub async fn repair_feeder(
    window: tauri::Window,
    executable: PathBuf,
) -> ApiResponse<graphics_components::feeder::Operation> {
    run_feeder_operation(window, move || {
        graphics_components::feeder::remove_or_repair(executable, true)
    })
    .await
}

#[tauri::command]
pub async fn get_feeder_installation(
    executable: PathBuf,
) -> ApiResponse<Option<graphics_components::feeder::Installation>> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::feeder::installation(executable)
    })
    .await
    {
        Ok(Ok(record)) => ApiResponse::success(record),
        Ok(Err(e)) => ApiResponse::fail(e),
        Err(e) => ApiResponse::fail(e.to_string()),
    }
}

/// Read-only FG preflight; never infers live FG state from installed files.
#[tauri::command]
pub async fn detect_streamline_fg(
    executable: PathBuf,
    graphics_api: GraphicsApi,
) -> ApiResponse<graphics_components::streamline_fg::FgPreflight> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::streamline_fg::detect(executable, graphics_api)
    })
    .await
    {
        Ok(Ok(report)) => ApiResponse::success(report),
        Ok(Err(error)) => ApiResponse::fail(error),
        Err(error) => ApiResponse::fail(format!("FG 检测任务失败：{error}")),
    }
}

#[tauri::command]
pub async fn install_streamline_fg(
    executable: PathBuf,
    graphics_api: GraphicsApi,
    allow_unverified: bool,
    expected_sha256: String,
) -> ApiResponse<graphics_components::streamline_install::Operation> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::streamline_install::install(
            executable,
            graphics_api,
            allow_unverified,
            expected_sha256,
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
) -> ApiResponse<serde_json::Value> {
    match tauri::async_runtime::spawn_blocking(move || {
        graphics_components::streamline_install::live(executable, enabled)
    })
    .await
    {
        Ok(Ok(v)) => ApiResponse::success(v),
        Ok(Err(e)) => ApiResponse::fail(e),
        Err(e) => ApiResponse::fail(e.to_string()),
    }
}
