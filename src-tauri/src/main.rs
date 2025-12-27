//! NS Emu Tools - 应用程序入口
//!
//! Nintendo Switch 模拟器管理工具

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ns_emu_tools_lib::{commands, logging};
use tauri::{Manager, WebviewWindow};
use tracing::info;

/// 设置窗口大小并监听窗口变化事件
fn setup_window(window: &WebviewWindow) {
    // 从配置读取窗口大小
    let config = ns_emu_tools_lib::config::get_config();
    let width = config.setting.ui.width;
    let height = config.setting.ui.height;

    // 设置窗口大小
    if let Err(e) = window.set_size(tauri::Size::Physical(tauri::PhysicalSize {
        width,
        height,
    })) {
        tracing::warn!("设置窗口大小失败: {}", e);
    }

    // 监听窗口大小变化事件
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Resized(size) = event {
            let width = size.width;
            let height = size.height;

            // 更新配置中的窗口大小
            if let Err(e) = ns_emu_tools_lib::config::update_window_size(width, height) {
                tracing::warn!("保存窗口大小失败: {}", e);
            }
        }
    });
}

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
        .setup(|app| {
            // 获取主窗口
            if let Some(window) = app.get_webview_window("main") {
                setup_window(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Common commands
            commands::common::get_config,
            commands::common::save_config,
            commands::common::get_storage,
            commands::common::get_app_version,
            commands::common::get_platform,
            commands::common::open_folder,
            commands::common::open_url,
            commands::common::update_setting,
            commands::common::update_last_open_emu_page,
            commands::common::update_dark_state,
            commands::common::update_window_size,
            commands::common::delete_history_path,
            commands::common::delete_path,
            commands::common::check_update,
            commands::common::load_change_log,
            commands::common::get_available_firmware_sources,
            commands::common::get_github_mirrors,
            commands::common::get_game_data,
            commands::common::get_available_firmware_infos,
            commands::common::load_history_path,
            commands::common::detect_firmware_version,
            commands::common::download_app_update,
            commands::common::install_app_update,
            commands::common::update_self_by_tag,
            commands::common::cancel_download_command,
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
            // Cheats commands
            commands::cheats::scan_all_cheats_folder,
            commands::cheats::list_all_cheat_files_from_folder,
            commands::cheats::load_cheat_chunk_info,
            commands::cheats::update_current_cheats,
            commands::cheats::open_cheat_mod_folder,
            // Save manager commands
            commands::save_manager::get_users_in_save_cmd,
            commands::save_manager::list_all_games_by_user_folder_cmd,
            commands::save_manager::backup_yuzu_save_folder_cmd,
            commands::save_manager::get_yuzu_save_backup_folder_cmd,
            commands::save_manager::update_yuzu_save_backup_folder_cmd,
            commands::save_manager::list_all_yuzu_backups_cmd,
            commands::save_manager::restore_yuzu_save_from_backup_cmd,
            commands::save_manager::open_yuzu_save_backup_folder_cmd,
        ])
        .run(tauri::generate_context!())
        .expect("运行 Tauri 应用程序时出错");
}
