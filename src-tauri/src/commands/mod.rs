//! Tauri 命令模块
//!
//! 定义所有暴露给前端的 Tauri 命令

use serde::Serialize;
use tauri::{Emitter, Listener, Window};
use tracing::warn;

pub mod cheats;
#[cfg(not(test))]
pub mod common;
pub mod ryujinx;
pub mod save_manager;
pub mod yuzu;

const EMULATOR_RUNNING_EVENT: &str = "emulator-running";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EmulatorRunningPrompt {
    emulator_name: String,
    response_event: String,
}

pub(crate) async fn wait_for_emulator_exit<F>(
    window: &Window,
    emulator_name: &str,
    is_running: F,
) -> Result<(), String>
where
    F: Fn() -> bool,
{
    loop {
        if !is_running() {
            return Ok(());
        }

        warn!("{} 仍在运行，等待用户关闭模拟器", emulator_name);
        let response_event = format!(
            "emulator-running-response-{}",
            uuid::Uuid::new_v4().simple()
        );
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let listener_id = window.once(response_event.clone(), move |event| {
            let retry = serde_json::from_str::<bool>(event.payload()).unwrap_or(false);
            let _ = sender.send(retry);
        });

        if let Err(error) = window.emit(
            EMULATOR_RUNNING_EVENT,
            EmulatorRunningPrompt {
                emulator_name: emulator_name.to_string(),
                response_event: response_event.clone(),
            },
        ) {
            window.unlisten(listener_id);
            return Err(format!("无法显示模拟器进程检测对话框: {}", error));
        }

        let retry = receiver
            .await
            .map_err(|_| "无法获取模拟器进程检测对话框结果".to_string())?;
        window.unlisten(listener_id);
        if !retry {
            return Err("用户取消安装".to_string());
        }
    }
}
