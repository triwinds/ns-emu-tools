//! 金手指管理服务
//!
//! 提供金手指管理相关的业务逻辑

use crate::error::{AppError, AppResult};
use crate::models::cheats::{
    CheatChunkInfo, CheatEntry, CheatFile, CheatFileInfo, GameCheatFolder,
};
use crate::services::cheats_parser::{parse_file, serialize};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

/// 金手指服务
pub struct CheatsService;

impl CheatsService {
    /// 创建新的金手指服务实例
    pub fn new() -> Self {
        Self
    }

    /// 扫描所有金手指文件夹
    ///
    /// 扫描模拟器的 load 目录，找到所有包含金手指的游戏文件夹
    pub fn scan_all_cheats_folder(&self, mod_path: &Path) -> AppResult<Vec<GameCheatFolder>> {
        info!("扫描金手指目录: {:?}", mod_path);

        if !mod_path.exists() {
            return Err(AppError::DirectoryNotFound(
                mod_path.to_string_lossy().to_string(),
            ));
        }

        let game_id_re = Regex::new(r"^[\dA-Za-z]{16}$").unwrap();
        let cheat_file_re = Regex::new(r"^[\dA-Za-z]{16}\.txt$").unwrap();

        let mut result = Vec::new();

        // 查找所有 cheats 文件夹
        for entry in walkdir::WalkDir::new(mod_path)
            .max_depth(5)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();

            // 检查是否是 cheats 文件夹
            if !path.is_dir() || path.file_name() != Some(std::ffi::OsStr::new("cheats")) {
                continue;
            }

            // 获取游戏 ID（cheats 文件夹的祖父文件夹名称）
            let game_id = match path.parent().and_then(|p| p.parent()) {
                Some(p) => match p.file_name() {
                    Some(name) => name.to_string_lossy().to_string(),
                    None => continue,
                },
                None => continue,
            };

            // 验证游戏 ID 格式
            if !game_id_re.is_match(&game_id) {
                continue;
            }

            // 检查是否有有效的金手指文件（16 位十六进制命名的 .txt 文件）
            let has_cheat_file = fs::read_dir(path)
                .ok()
                .map(|entries| {
                    entries
                        .filter_map(|e| e.ok())
                        .any(|e| {
                            if let Some(name) = e.file_name().to_str() {
                                cheat_file_re.is_match(name)
                            } else {
                                false
                            }
                        })
                })
                .unwrap_or(false);

            if has_cheat_file {
                result.push(GameCheatFolder::new(game_id, path.to_path_buf()));
            }
        }

        info!("找到 {} 个游戏的金手指", result.len());
        Ok(result)
    }

    /// 列出文件夹中的所有金手指文件
    pub fn list_all_cheat_files_from_folder(
        &self,
        folder_path: &Path,
    ) -> AppResult<Vec<CheatFileInfo>> {
        if !folder_path.exists() {
            return Err(AppError::DirectoryNotFound(
                folder_path.to_string_lossy().to_string(),
            ));
        }

        let cheat_file_re = Regex::new(r"^[\dA-Za-z]{16}\.txt$").unwrap();
        let mut result = Vec::new();

        for entry in fs::read_dir(folder_path)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            // 只接受 16 位十六进制命名的 .txt 文件
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if cheat_file_re.is_match(filename) {
                    let name = self.read_cheat_name(&path)?;
                    result.push(CheatFileInfo::new(path, name));
                }
            }
        }

        Ok(result)
    }

    /// 读取金手指文件的名称
    fn read_cheat_name(&self, txt_file: &Path) -> AppResult<String> {
        // 尝试解析文件获取第一个条目的标题
        match parse_file(txt_file, 1000) {
            Ok(cheat_file) => {
                if !cheat_file.entries.is_empty() {
                    let filename = txt_file
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    return Ok(format!("{} - {}", filename, cheat_file.entries[0].title));
                }
            }
            Err(e) => {
                warn!("解析金手指文件失败: {:?}", e);
            }
        }

        // 回退到文件名
        Ok(txt_file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string())
    }

    /// 加载金手指块信息
    ///
    /// 解析金手指文件，创建或更新 chunk 文件（所有可用的金手指仓库），
    /// 返回每个金手指的标题和启用状态
    pub fn load_cheat_chunk_info(&self, cheat_file_path: &Path) -> AppResult<Vec<CheatChunkInfo>> {
        if !cheat_file_path.exists() {
            return Err(AppError::FileNotFound(
                cheat_file_path.to_string_lossy().to_string(),
            ));
        }

        // 获取 chunk 文件夹和文件路径
        let chunk_folder = cheat_file_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| AppError::InvalidArgument("无效的文件路径".to_string()))?
            .join("cheats_chunk");

        if !chunk_folder.exists() {
            fs::create_dir_all(&chunk_folder)?;
        }

        let filename = cheat_file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| AppError::InvalidArgument("无效的文件名".to_string()))?;

        let chunk_filename = format!("{}_chunk.txt", &filename[..16]);
        let chunk_file = chunk_folder.join(chunk_filename);

        // 解析当前金手指文件
        let current_cheat_map = self.parse_yuzu_cheat_file(cheat_file_path)?;
        debug!(
            "当前金手指数量: {}, 标题: {:?}",
            current_cheat_map.len(),
            current_cheat_map.keys().collect::<Vec<_>>()
        );

        // 加载或创建 chunk 文件
        let chunk_cheat_map = if chunk_file.exists() {
            let chunk_map = self.parse_yuzu_cheat_file(&chunk_file)?;
            debug!("chunk 金手指标题: {:?}", chunk_map.keys().collect::<Vec<_>>());

            // 合并当前金手指到 chunk
            let mut merged = chunk_map;
            for (title, entry) in current_cheat_map.iter() {
                merged.insert(title.clone(), entry.clone());
            }
            info!("chunk 金手指已更新");
            merged
        } else {
            info!("chunk 金手指已初始化");
            current_cheat_map.clone()
        };

        debug!(
            "chunk 金手指总数: {}, 标题: {:?}",
            chunk_cheat_map.len(),
            chunk_cheat_map.keys().collect::<Vec<_>>()
        );

        // 生成结果
        let mut result = Vec::new();
        for title in chunk_cheat_map.keys() {
            let enable = current_cheat_map.contains_key(title);
            result.push(CheatChunkInfo::new(title, enable));
        }

        // 保存 chunk 文件
        info!("保存 chunk 金手指到 {:?}...", chunk_file);
        self.save_cheat_map_to_file(&chunk_cheat_map, &chunk_file)?;

        Ok(result)
    }

    /// 更新当前金手指
    ///
    /// 根据用户选择的金手指标题列表，从 chunk 文件中提取对应的金手指，
    /// 备份原文件，写入新的金手指文件
    pub fn update_current_cheats(
        &self,
        enable_titles: &[String],
        cheat_file_path: &Path,
        window: Option<&tauri::Window>,
    ) -> AppResult<()> {
        if !cheat_file_path.exists() {
            return Err(AppError::FileNotFound(
                cheat_file_path.to_string_lossy().to_string(),
            ));
        }

        // 获取 chunk 文件路径
        let chunk_folder = cheat_file_path
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| AppError::InvalidArgument("无效的文件路径".to_string()))?
            .join("cheats_chunk");

        if !chunk_folder.exists() {
            return Err(AppError::DirectoryNotFound(
                "仓库目录不存在".to_string(),
            ));
        }

        let filename = cheat_file_path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| AppError::InvalidArgument("无效的文件名".to_string()))?;

        let chunk_filename = format!("{}_chunk.txt", &filename[..16]);
        let chunk_file = chunk_folder.join(chunk_filename);

        if !chunk_file.exists() {
            return Err(AppError::FileNotFound("仓库文件不存在".to_string()));
        }

        // 备份原文件
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let backup_filename = format!("{}_{}.txt", &filename[..16], timestamp);
        let backup_file = chunk_folder.join(backup_filename);

        fs::copy(cheat_file_path, &backup_file)?;
        info!("备份 {:?} 到 {:?}", cheat_file_path, backup_file);

        // 发送通知
        if let Some(window) = window {
            let _ = crate::services::notifier::send_notify(
                window,
                &format!("原文件已备份至 {}", backup_file.to_string_lossy()),
            );
        }

        // 从 chunk 文件中提取启用的金手指
        let chunk_map = self.parse_yuzu_cheat_file(&chunk_file)?;
        debug!(
            "chunk 金手指数量: {}, 标题: {:?}",
            chunk_map.len(),
            chunk_map.keys().collect::<Vec<_>>()
        );

        let mut cheat_map = HashMap::new();
        for title in enable_titles {
            if let Some(entry) = chunk_map.get(title) {
                cheat_map.insert(title.clone(), entry.clone());
            } else {
                warn!("标题 [{}] 在 chunk 中不存在", title);
            }
        }

        debug!(
            "选中的金手指数量: {}, 标题: {:?}",
            cheat_map.len(),
            cheat_map.keys().collect::<Vec<_>>()
        );

        // 保存金手指文件
        info!("保存金手指到 {:?}...", cheat_file_path);
        self.save_cheat_map_to_file(&cheat_map, cheat_file_path)?;

        Ok(())
    }

    /// 打开金手指文件夹
    pub fn open_cheat_mod_folder(&self, folder_path: &Path) -> AppResult<()> {
        if !folder_path.exists() {
            return Err(AppError::DirectoryNotFound(
                folder_path.to_string_lossy().to_string(),
            ));
        }

        // 获取父文件夹（Cheats Mod 文件夹）
        let parent_folder = folder_path
            .parent()
            .ok_or_else(|| AppError::InvalidArgument("无效的文件夹路径".to_string()))?;

        info!("在资源管理器中打开文件夹 [{:?}]", parent_folder);

        #[cfg(target_os = "windows")]
        {
            // 在 Windows 上使用绝对路径
            let abs_path = parent_folder.canonicalize()?;
            std::process::Command::new("explorer")
                .arg(abs_path)
                .spawn()?;
        }

        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(parent_folder)
                .spawn()?;
        }

        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(parent_folder)
                .spawn()?;
        }

        Ok(())
    }

    /// 解析 Yuzu 金手指文件
    fn parse_yuzu_cheat_file(&self, cheat_file: &Path) -> AppResult<HashMap<String, CheatEntry>> {
        let cheat_file_obj = parse_file(cheat_file, 1000)?;

        let mut result = HashMap::new();
        for entry in cheat_file_obj.entries {
            result.insert(entry.title.clone(), entry);
        }

        Ok(result)
    }

    /// 保存金手指映射到文件
    fn save_cheat_map_to_file(
        &self,
        cheats_map: &HashMap<String, CheatEntry>,
        file_path: &Path,
    ) -> AppResult<()> {
        // 将 HashMap 转换为 CheatFile，保持原始顺序
        // 注意: HashMap 没有顺序保证，但这与 Python 行为一致
        let entries: Vec<_> = cheats_map.values().cloned().collect();

        let cheat_file = CheatFile { entries };

        // 序列化并写入文件
        let content = serialize(&cheat_file);
        fs::write(file_path, content)?;

        Ok(())
    }
}

impl Default for CheatsService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_cheat_structure() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();

        // 创建测试结构
        let game_path = base_path.join("0100000000000001").join("Cheat Manager Patch");
        let cheats_path = game_path.join("cheats");
        fs::create_dir_all(&cheats_path).unwrap();

        // 创建测试金手指文件
        let cheat_file = cheats_path.join("0100000000000001.txt");
        fs::write(
            &cheat_file,
            "[Test Cheat 1]\n12345678 ABCDEFAB\n\n[Test Cheat 2]\nFEDCBA98 11111111\n",
        )
        .unwrap();

        (temp_dir, base_path)
    }

    #[test]
    fn test_scan_all_cheats_folder() {
        let (_temp_dir, base_path) = create_test_cheat_structure();
        let service = CheatsService::new();

        let result = service.scan_all_cheats_folder(&base_path).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].game_id, "0100000000000001");
    }

    #[test]
    fn test_list_all_cheat_files_from_folder() {
        let (_temp_dir, base_path) = create_test_cheat_structure();
        let service = CheatsService::new();

        let cheats_path = base_path
            .join("0100000000000001")
            .join("Cheat Manager Patch")
            .join("cheats");

        let result = service
            .list_all_cheat_files_from_folder(&cheats_path)
            .unwrap();
        assert_eq!(result.len(), 1);
        assert!(result[0].name.contains("Test Cheat 1"));
    }
}
