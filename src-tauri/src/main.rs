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

    // 检查并安装 WebView2 运行时
    // 如果未安装，会提示用户并自动下载安装
    // 如果用户取消或安装失败，程序将退出
    if let Err(e) = ns_emu_tools_lib::utils::webview::check_and_install_on_startup() {
        tracing::error!("WebView2 检查/安装失败: {}", e);
        std::process::exit(1);
    }

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
            commands::common::delete_history_path,
            commands::common::check_update,
            commands::common::load_change_log,
            commands::common::get_available_firmware_sources,
            commands::common::get_github_mirrors,
            commands::common::get_game_data,
            commands::common::get_available_firmware_infos,
            commands::common::load_history_path,
            commands::common::detect_firmware_version,
            // Yuzu commands
            commands::yuzu::get_all_yuzu_versions,
            commands::yuzu::install_yuzu_by_version,
            commands::yuzu::detect_yuzu_version_command,
            commands::yuzu::start_yuzu_command,
            commands::yuzu::get_yuzu_exe_path_command,
            commands::yuzu::open_yuzu_keys_folder_command,
            commands::yuzu::get_yuzu_user_path_command,
            commands::yuzu::get_yuzu_nand_path_command,
            commands::yuzu::get_yuzu_load_path_command,
            commands::yuzu::update_yuzu_path_command,
            commands::yuzu::get_yuzu_change_logs_command,
            commands::yuzu::install_firmware_to_yuzu_command,
            commands::yuzu::switch_yuzu_branch,
            commands::yuzu::cancel_yuzu_download_command,
            // Ryujinx commands
            commands::ryujinx::get_all_ryujinx_versions_command,
            commands::ryujinx::install_ryujinx_by_version_command,
            commands::ryujinx::start_ryujinx_command,
            commands::ryujinx::open_ryujinx_keys_folder_command,
            commands::ryujinx::get_ryujinx_user_folder_command,
            commands::ryujinx::update_ryujinx_path_command,
            commands::ryujinx::get_ryujinx_change_logs_command,
            commands::ryujinx::install_firmware_to_ryujinx_command,
            commands::ryujinx::detect_ryujinx_branch_command,
            commands::ryujinx::ask_and_update_ryujinx_path_command,
            commands::ryujinx::detect_ryujinx_version_command,
            commands::ryujinx::cancel_ryujinx_download_command,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用程序时出错");
}
