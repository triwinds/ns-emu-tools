//! Tauri 命令模块
//!
//! 定义所有暴露给前端的 Tauri 命令

use tauri::Window;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tracing::warn;

pub mod cheats;
#[cfg(not(test))]
pub mod common;
pub mod ryujinx;
pub mod save_manager;
pub mod yuzu;

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
        let (sender, receiver) = tokio::sync::oneshot::channel();
        window
            .dialog()
            .message(format!(
                "检测到 {} 仍在运行。\n\n请关闭模拟器后选择“重新检测”，或取消本次安装。",
                emulator_name
            ))
            .title(format!("请关闭 {}", emulator_name))
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "重新检测".to_string(),
                "取消安装".to_string(),
            ))
            .show(move |retry| {
                let _ = sender.send(retry);
            });

        let retry = receiver
            .await
            .map_err(|_| "无法获取模拟器进程检测对话框结果".to_string())?;
        if !retry {
            return Err("用户取消安装".to_string());
        }
    }
}
