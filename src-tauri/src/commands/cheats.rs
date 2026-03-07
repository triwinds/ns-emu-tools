//! 金手指管理命令
//!
//! 提供金手指管理相关的 Tauri 命令

use crate::models::cheats::{CheatChunkInfo, CheatFileInfo, GameCheatFolder};
use crate::models::response::ApiResponse;
use crate::services::cheats::CheatsService;
use crate::services::yuzu::get_yuzu_load_path;
use std::path::PathBuf;
use tauri::command;
use tracing::{error, info};

/// 扫描所有金手指文件夹
///
/// 扫描模拟器的 load 目录，找到所有包含金手指的游戏文件夹
#[command]
pub async fn scan_all_cheats_folder() -> Result<ApiResponse<Vec<GameCheatFolder>>, String> {
    let load_path = get_yuzu_load_path();
    info!("扫描金手指文件夹: {}", load_path.display());

    let service = CheatsService::new();

    match service.scan_all_cheats_folder(&load_path) {
        Ok(folders) => {
            info!("找到 {} 个游戏的金手指", folders.len());
            Ok(ApiResponse::success(folders))
        }
        Err(e) => {
            error!("扫描金手指文件夹失败: {:?}", e);
            Err(format!("扫描金手指文件夹失败: {}", e))
        }
    }
}

/// 列出文件夹中的所有金手指文件
///
/// 返回指定文件夹中所有有效的金手指文件列表
#[command]
pub async fn list_all_cheat_files_from_folder(
    folder_path: String,
) -> Result<ApiResponse<Vec<CheatFileInfo>>, String> {
    info!("列出金手指文件: {}", folder_path);

    let service = CheatsService::new();
    let path = PathBuf::from(folder_path);

    match service.list_all_cheat_files_from_folder(&path) {
        Ok(files) => {
            info!("找到 {} 个金手指文件", files.len());
            Ok(ApiResponse::success(files))
        }
        Err(e) => {
            error!("列出金手指文件失败: {:?}", e);
            Err(format!("列出金手指文件失败: {}", e))
        }
    }
}

/// 加载金手指块信息
///
/// 解析金手指文件，创建或更新 chunk 文件（所有可用的金手指仓库），
/// 返回每个金手指的标题和启用状态
#[command]
pub async fn load_cheat_chunk_info(
    cheat_file_path: String,
) -> Result<ApiResponse<Vec<CheatChunkInfo>>, String> {
    info!("加载金手指块信息: {}", cheat_file_path);

    let service = CheatsService::new();
    let path = PathBuf::from(cheat_file_path);

    match service.load_cheat_chunk_info(&path) {
        Ok(chunk_info) => {
            info!("加载了 {} 个金手指块", chunk_info.len());
            Ok(ApiResponse::success(chunk_info))
        }
        Err(e) => {
            error!("加载金手指块信息失败: {:?}", e);
            Err(format!("加载金手指块信息失败: {}", e))
        }
    }
}

/// 更新当前金手指
///
/// 根据用户选择的金手指标题列表，从 chunk 文件中提取对应的金手指，
/// 备份原文件，写入新的金手指文件
#[command]
pub async fn update_current_cheats(
    enable_titles: Vec<String>,
    cheat_file_path: String,
    window: tauri::Window,
) -> Result<ApiResponse<()>, String> {
    info!(
        "更新当前金手指: {} 个已启用, 文件: {}",
        enable_titles.len(),
        cheat_file_path
    );

    let service = CheatsService::new();
    let path = PathBuf::from(cheat_file_path);

    match service.update_current_cheats(&enable_titles, &path, Some(&window)) {
        Ok(_) => {
            info!("金手指更新成功");
            Ok(ApiResponse::ok_with_msg("金手指更新成功"))
        }
        Err(e) => {
            error!("更新金手指失败: {:?}", e);
            Err(format!("更新金手指失败: {}", e))
        }
    }
}

/// 打开金手指文件夹
///
/// 在系统文件管理器中打开金手指文件夹的父目录（Cheats Mod 文件夹）
#[command]
pub async fn open_cheat_mod_folder(folder_path: String) -> Result<ApiResponse<()>, String> {
    info!("打开金手指文件夹: {}", folder_path);

    let service = CheatsService::new();
    let path = PathBuf::from(folder_path);

    match service.open_cheat_mod_folder(&path) {
        Ok(_) => {
            info!("金手指文件夹已打开");
            Ok(ApiResponse::ok())
        }
        Err(e) => {
            error!("打开金手指文件夹失败: {:?}", e);
            Err(format!("打开金手指文件夹失败: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    // 测试将在这里添加
}
