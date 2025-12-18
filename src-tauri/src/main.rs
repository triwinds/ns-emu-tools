//! NS Emu Tools - 应用程序入口
//!
//! Nintendo Switch 模拟器管理工具

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ns_emu_tools_lib::commands;
use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn setup_logging() {
    // 设置日志
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,ns_emu_tools=debug"));

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_file(true)
        .with_line_number(true);

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .init();
}

fn main() {
    setup_logging();

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
