//! Yuzu 存档管理命令
//!
//! 提供存档备份、还原等 Tauri 命令

use crate::models::response::ApiResponse;
use crate::services::save_manager::{
    backup_folder, get_users_in_save, get_yuzu_save_backup_folder, list_all_games_by_user_folder,
    list_all_yuzu_backups, restore_yuzu_save_from_backup, update_yuzu_save_backup_folder,
    BackupInfo, GameSaveInfo, UserInfo,
};
use tauri::{command, Emitter};

/// 获取所有存档中的用户
#[command]
pub async fn get_users_in_save_cmd() -> ApiResponse<Vec<UserInfo>> {
    match get_users_in_save() {
        Ok(users) => ApiResponse::success(users),
        Err(e) => ApiResponse::fail(&e.to_string()),
    }
}

/// 列出指定用户的所有游戏存档
#[command]
pub async fn list_all_games_by_user_folder_cmd(folder: String) -> ApiResponse<Vec<GameSaveInfo>> {
    match list_all_games_by_user_folder(&folder) {
        Ok(games) => ApiResponse::success(games),
        Err(e) => ApiResponse::fail(&e.to_string()),
    }
}

/// 备份 Yuzu 存档文件夹
#[command]
pub async fn backup_yuzu_save_folder_cmd(
    folder: String,
    window: tauri::Window,
) -> ApiResponse<()> {
    use crate::services::notifier;
    use crate::services::save_manager::sizeof_fmt;

    // 发送开始备份日志
    let _ = window.emit(
        notifier::events::LOG_MESSAGE,
        format!("正在备份文件夹 [{}]...", folder),
    );

    match backup_folder(&folder) {
        Ok((backup_filepath, file_size)) => {
            // 发送备份完成日志，包含文件大小
            let _ = window.emit(
                notifier::events::LOG_MESSAGE,
                format!(
                    "{} 备份完成，大小: {}",
                    backup_filepath.display(),
                    sizeof_fmt(file_size)
                ),
            );
            let _ = notifier::send_notify(&window, "备份完成");
            ApiResponse::success(())
        }
        Err(e) => {
            let error_msg = e.to_string();
            let _ = window.emit(
                notifier::events::LOG_MESSAGE,
                format!("备份失败: {}", error_msg),
            );
            let _ = notifier::send_notify(&window, &format!("备份失败: {}", error_msg));
            ApiResponse::fail(&error_msg)
        }
    }
}

/// 获取当前 Yuzu 存档备份文件夹
#[command]
pub async fn get_yuzu_save_backup_folder_cmd() -> ApiResponse<String> {
    match get_yuzu_save_backup_folder() {
        Ok(path) => ApiResponse::success(path),
        Err(e) => ApiResponse::fail(&e.to_string()),
    }
}

/// 更新 Yuzu 存档备份文件夹
#[command]
pub async fn update_yuzu_save_backup_folder_cmd(
    folder: String,
    window: tauri::Window,
) -> ApiResponse<()> {
    match update_yuzu_save_backup_folder(&folder) {
        Ok(_) => {
            let _ = crate::services::notifier::send_notify(
                &window,
                &format!("yuzu 存档备份文件夹更改为: {}", folder),
            );
            ApiResponse::success(())
        }
        Err(e) => {
            let error_msg = e.to_string();
            let _ = crate::services::notifier::send_notify(&window, &error_msg);
            ApiResponse::fail(&error_msg)
        }
    }
}

/// 列出所有 Yuzu 备份
#[command]
pub async fn list_all_yuzu_backups_cmd() -> ApiResponse<Vec<BackupInfo>> {
    match list_all_yuzu_backups() {
        Ok(backups) => ApiResponse::success(backups),
        Err(e) => ApiResponse::fail(&e.to_string()),
    }
}

/// 从备份还原 Yuzu 存档
#[command]
pub async fn restore_yuzu_save_from_backup_cmd(
    user_folder_name: String,
    backup_path: String,
    window: tauri::Window,
) -> ApiResponse<()> {
    use crate::services::notifier;

    // 发送开始还原日志
    let _ = window.emit(
        notifier::events::LOG_MESSAGE,
        format!("正在还原存档从 [{}]...", backup_path),
    );

    match restore_yuzu_save_from_backup(&user_folder_name, &backup_path) {
        Ok(_) => {
            let _ = window.emit(
                notifier::events::LOG_MESSAGE,
                "存档还原完成".to_string(),
            );
            let _ = notifier::send_notify(&window, "存档还原完成");
            ApiResponse::success(())
        }
        Err(e) => {
            let error_msg = e.to_string();
            let _ = window.emit(
                notifier::events::LOG_MESSAGE,
                format!("还原失败: {}", error_msg),
            );
            let _ = notifier::send_notify(&window, &format!("还原失败: {}", error_msg));
            ApiResponse::fail(&error_msg)
        }
    }
}

/// 打开 Yuzu 存档备份文件夹
#[command]
pub async fn open_yuzu_save_backup_folder_cmd(window: tauri::Window) -> ApiResponse<()> {
    use std::process::Command;

    match get_yuzu_save_backup_folder() {
        Ok(path) => {
            let path_buf = std::path::PathBuf::from(&path);

            // 确保目录存在
            if !path_buf.exists() {
                if let Err(e) = std::fs::create_dir_all(&path_buf) {
                    let error_msg = format!("创建备份目录失败: {}", e);
                    let _ = crate::services::notifier::send_notify(&window, &error_msg);
                    return ApiResponse::fail(&error_msg);
                }
            }

            // 在资源管理器中打开
            #[cfg(target_os = "windows")]
            {
                match Command::new("explorer").arg(&path).spawn() {
                    Ok(_) => ApiResponse::success(()),
                    Err(e) => ApiResponse::fail(&format!("打开文件夹失败: {}", e)),
                }
            }

            #[cfg(target_os = "macos")]
            {
                match Command::new("open").arg(&path).spawn() {
                    Ok(_) => ApiResponse::success(()),
                    Err(e) => ApiResponse::fail(&format!("打开文件夹失败: {}", e)),
                }
            }

            #[cfg(target_os = "linux")]
            {
                match Command::new("xdg-open").arg(&path).spawn() {
                    Ok(_) => ApiResponse::success(()),
                    Err(e) => ApiResponse::fail(&format!("打开文件夹失败: {}", e)),
                }
            }
        }
        Err(e) => ApiResponse::fail(&e.to_string()),
    }
}
