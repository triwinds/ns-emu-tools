//! 金手指数据模型
//!
//! 定义金手指管理相关的数据结构

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 金手指条目
///
/// 代表一个金手指，包含标题、操作码列表和可选的原始内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheatEntry {
    /// 金手指标题
    pub title: String,
    /// 操作码列表（每个操作码为 8 位十六进制字符）
    pub ops: Vec<String>,
    /// 原始内容（保留格式和注释）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_body: Option<String>,
}

impl CheatEntry {
    /// 创建新的金手指条目
    pub fn new(title: impl Into<String>, ops: Vec<String>) -> Self {
        Self {
            title: title.into(),
            ops,
            raw_body: None,
        }
    }

    /// 添加原始内容
    pub fn with_raw_body(mut self, raw_body: impl Into<String>) -> Self {
        self.raw_body = Some(raw_body.into());
        self
    }

    /// 检查操作码是否有效（每个操作码应该是 8 位十六进制）
    pub fn validate_ops(&self) -> bool {
        self.ops.iter().all(|op| {
            op.len() == 8 && op.chars().all(|c| c.is_ascii_hexdigit())
        })
    }
}

/// 金手指文件
///
/// 代表一个解析后的金手指文件，包含多个金手指条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheatFile {
    /// 金手指条目列表
    pub entries: Vec<CheatEntry>,
}

impl CheatFile {
    /// 创建新的金手指文件
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 添加金手指条目
    pub fn add_entry(&mut self, entry: CheatEntry) {
        self.entries.push(entry);
    }

    /// 根据标题查找金手指条目
    pub fn find_by_title(&self, title: &str) -> Option<&CheatEntry> {
        self.entries.iter().find(|e| e.title == title)
    }

    /// 获取所有标题
    pub fn get_all_titles(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.title.clone()).collect()
    }

    /// 过滤启用的金手指
    pub fn filter_enabled(&self, enabled_titles: &[String]) -> Self {
        let entries = self.entries
            .iter()
            .filter(|e| enabled_titles.contains(&e.title))
            .cloned()
            .collect();

        Self { entries }
    }
}

impl Default for CheatFile {
    fn default() -> Self {
        Self::new()
    }
}

/// 游戏金手指文件夹信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameCheatFolder {
    /// 游戏 ID（16 位十六进制字符）
    pub game_id: String,
    /// 金手指文件夹路径
    pub cheats_path: String,
}

impl GameCheatFolder {
    /// 创建新的游戏金手指文件夹信息
    pub fn new(game_id: impl Into<String>, cheats_path: PathBuf) -> Self {
        Self {
            game_id: game_id.into(),
            cheats_path: cheats_path.to_string_lossy().to_string(),
        }
    }
}

/// 金手指文件信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheatFileInfo {
    /// 文件路径
    pub path: String,
    /// 显示名称（包含文件名和第一个金手指标题）
    pub name: String,
}

impl CheatFileInfo {
    /// 创建新的金手指文件信息
    pub fn new(path: PathBuf, name: impl Into<String>) -> Self {
        Self {
            path: path.to_string_lossy().to_string(),
            name: name.into(),
        }
    }
}

/// 金手指块信息
///
/// 代表一个金手指的启用状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheatChunkInfo {
    /// 金手指标题
    pub title: String,
    /// 是否启用
    pub enable: bool,
}

impl CheatChunkInfo {
    /// 创建新的金手指块信息
    pub fn new(title: impl Into<String>, enable: bool) -> Self {
        Self {
            title: title.into(),
            enable,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cheat_entry_validation() {
        let valid_entry = CheatEntry::new(
            "Test Cheat",
            vec!["12345678".to_string(), "ABCDEF00".to_string()],
        );
        assert!(valid_entry.validate_ops());

        let invalid_entry = CheatEntry::new(
            "Test Cheat",
            vec!["1234567".to_string()], // 只有 7 位
        );
        assert!(!invalid_entry.validate_ops());
    }

    #[test]
    fn test_cheat_file_operations() {
        let mut file = CheatFile::new();

        let entry1 = CheatEntry::new("Cheat 1", vec!["12345678".to_string()]);
        let entry2 = CheatEntry::new("Cheat 2", vec!["ABCDEF00".to_string()]);

        file.add_entry(entry1);
        file.add_entry(entry2);

        assert_eq!(file.entries.len(), 2);
        assert_eq!(file.get_all_titles(), vec!["Cheat 1", "Cheat 2"]);

        let found = file.find_by_title("Cheat 1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().title, "Cheat 1");
    }

    #[test]
    fn test_cheat_file_filter() {
        let mut file = CheatFile::new();
        file.add_entry(CheatEntry::new("Cheat 1", vec!["12345678".to_string()]));
        file.add_entry(CheatEntry::new("Cheat 2", vec!["ABCDEF00".to_string()]));
        file.add_entry(CheatEntry::new("Cheat 3", vec!["FEDCBA98".to_string()]));

        let enabled_titles = vec!["Cheat 1".to_string(), "Cheat 3".to_string()];
        let filtered = file.filter_enabled(&enabled_titles);

        assert_eq!(filtered.entries.len(), 2);
        assert_eq!(filtered.get_all_titles(), vec!["Cheat 1", "Cheat 3"]);
    }
}
