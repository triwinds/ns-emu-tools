//! NS Emu Tools - 应用程序入口
//!
//! Nintendo Switch 模拟器管理工具

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ns_emu_tools_lib::{commands, logging};
use tracing::info;

fn main() {
    // 初始化日志系统
    // 使用 Box::leak 确保 WorkerGuard 在整个程序运行期间保持活跃
    // 这样日志可以持续写入文件
    let _guard = Box::leak(Box::new(logging::init()));

    info!(
        "启动 NS Emu Tools v{}",
        ns_emu_tools_lib::CURRENT_VERSION
    );

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // Common commands
            commands::common::get_config,
            commands::common::save_config,
            commands::common::get_storage,
            commands::common::get_app_version,
            commands::common::open_folder,
            commands::common::open_url,
            commands::common::update_setting,
            commands::common::update_last_open_emu_page,
            commands::common::update_dark_state,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用程序时出错");
}
