//! 金手指文件解析器
//!
//! 实现 Yuzu/Citron 格式金手指文件的解析和序列化

use crate::error::{AppError, AppResult};
use crate::models::cheats::{CheatEntry, CheatFile};
use std::path::Path;

/// 检查字符是否为十六进制数字
fn is_hex_digit(c: char) -> bool {
    c.is_ascii_hexdigit()
}

/// 检查字符串是否为 8 位十六进制数
fn is_hex8(token: &str) -> bool {
    token.len() == 8 && token.chars().all(is_hex_digit)
}

/// 规范化原始内容
///
/// 移除开头和结尾的空行，保留中间的格式和注释
fn normalize_raw_body(chunks: &[String]) -> Option<String> {
    if chunks.is_empty() {
        return None;
    }

    let mut body = chunks.join("");

    // 移除开头的换行符
    while body.starts_with('\n') {
        body = body[1..].to_string();
    }

    // 移除结尾的换行符
    while body.ends_with('\n') {
        body = body[..body.len() - 1].to_string();
    }

    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

/// 解析金手指文本
///
/// 支持的格式:
/// - {Default} 或任何单大括号名称（仅出现一次，映射为第一个条目）
/// - [Name] 标准条目（可多次出现）
///
/// 主体包含 8 位十六进制令牌，令牌可以用空格和换行符分隔
pub fn parse_text(text: &str, max_ops_per_entry: usize) -> AppResult<CheatFile> {
    if text.is_empty() {
        return Ok(CheatFile::new());
    }

    // 规范化换行符并去除 BOM 和空格
    let data = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string();

    if data.is_empty() {
        return Ok(CheatFile::new());
    }

    let mut entries = Vec::new();
    let mut current_title: Option<String> = None;
    let mut current_ops: Vec<String> = Vec::new();
    let mut current_raw_chunks: Vec<String> = Vec::new();
    let mut seen_default = false;

    let chars: Vec<char> = data.chars().collect();
    let mut i = 0;
    let n = chars.len();

    while i < n {
        let ch = chars[i];

        // 处理空白字符
        if ch.is_whitespace() {
            if current_title.is_some() {
                current_raw_chunks.push(ch.to_string());
            }
            i += 1;
            continue;
        }

        // 处理 {Name} 格式
        if ch == '{' {
            // 提交前一个条目
            if let Some(title) = current_title.take() {
                if title.is_empty() {
                    return Err(AppError::InvalidArgument("空标题不被允许".to_string()));
                }
                entries.push(CheatEntry {
                    title,
                    ops: current_ops.clone(),
                    raw_body: normalize_raw_body(&current_raw_chunks),
                });
                current_ops.clear();
                current_raw_chunks.clear();
            }

            // 查找结束的 }
            let mut end = i + 1;
            while end < n && chars[end] != '}' {
                end += 1;
            }
            if end >= n {
                return Err(AppError::InvalidArgument("缺少标题的闭合 '}'".to_string()));
            }

            let name: String = chars[i + 1..end].iter().collect();
            let name = name.trim();

            if name.is_empty() {
                return Err(AppError::InvalidArgument("'{}' 大括号内的标题为空".to_string()));
            }

            if seen_default {
                return Err(AppError::InvalidArgument(
                    "重复的 '{...}' 默认类条目".to_string(),
                ));
            }

            seen_default = true;
            current_title = Some(name.to_string());
            i = end + 1;
            continue;
        }

        // 处理 [Name] 格式
        if ch == '[' {
            // 提交前一个条目
            if let Some(title) = current_title.take() {
                if title.is_empty() {
                    return Err(AppError::InvalidArgument("空标题不被允许".to_string()));
                }
                entries.push(CheatEntry {
                    title,
                    ops: current_ops.clone(),
                    raw_body: normalize_raw_body(&current_raw_chunks),
                });
                current_ops.clear();
                current_raw_chunks.clear();
            }

            // 查找结束的 ]
            let mut end = i + 1;
            while end < n && chars[end] != ']' {
                end += 1;
            }
            if end >= n {
                return Err(AppError::InvalidArgument("缺少标题的闭合 ']'".to_string()));
            }

            let name: String = chars[i + 1..end].iter().collect();
            let name = name.trim();

            if name.is_empty() {
                return Err(AppError::InvalidArgument("'[]' 方括号内的标题为空".to_string()));
            }

            current_title = Some(name.to_string());
            i = end + 1;
            continue;
        }

        // 处理十六进制令牌流
        if is_hex_digit(ch) {
            // 读取连续的十六进制字符
            let mut j = i;
            while j < n && is_hex_digit(chars[j]) {
                j += 1;
            }

            let token: String = chars[i..j].iter().collect();

            // 某些文件可能连接多个 8 位十六进制组，每 8 位拆分一次
            if token.len() % 8 != 0 {
                return Err(AppError::InvalidArgument(
                    "十六进制令牌长度不是 8 的倍数".to_string(),
                ));
            }

            for k in (0..token.len()).step_by(8) {
                let t = &token[k..k + 8];
                if !is_hex8(t) {
                    return Err(AppError::InvalidArgument("无效的 hex8 令牌".to_string()));
                }

                if current_title.is_none() {
                    // 如果没有打开任何标题，则隐式默认部分
                    current_title = Some("Default".to_string());
                }

                if current_ops.len() >= max_ops_per_entry {
                    return Err(AppError::InvalidArgument("条目中的操作码过多".to_string()));
                }

                current_ops.push(t.to_string());
            }

            // 将原始令牌文本追加到 raw_chunks 以保留格式
            if current_title.is_some() {
                current_raw_chunks.push(token);
            }

            i = j;
            continue;
        }

        // 未知字符
        return Err(AppError::InvalidArgument(format!(
            "金手指文本中出现意外字符 '{}'",
            ch
        )));
    }

    // 提交尾部条目
    if let Some(title) = current_title {
        if title.is_empty() {
            return Err(AppError::InvalidArgument("空标题不被允许".to_string()));
        }
        entries.push(CheatEntry {
            title,
            ops: current_ops,
            raw_body: normalize_raw_body(&current_raw_chunks),
        });
    }

    Ok(CheatFile { entries })
}

/// 从文件解析金手指
pub fn parse_file(path: &Path, max_ops_per_entry: usize) -> AppResult<CheatFile> {
    let data = std::fs::read(path)?;

    // 尝试 UTF-8 解码
    let text = match String::from_utf8(data.clone()) {
        Ok(s) => s,
        Err(_) => {
            // 使用 Latin-1 作为后备
            data.iter().map(|&b| b as char).collect()
        }
    };

    parse_text(&text, max_ops_per_entry)
}

/// 序列化金手指文件为文本
///
/// 操作码每 3 个一行
pub fn serialize(cheat_file: &CheatFile) -> String {
    let mut lines = Vec::new();

    for entry in &cheat_file.entries {
        lines.push(format!("[{}]", entry.title));

        if let Some(raw_body) = &entry.raw_body {
            // 使用原始格式
            let mut body = raw_body.clone();
            // 确保结尾有换行符
            if !body.ends_with('\n') {
                body.push('\n');
            }
            lines.push(body);
        } else {
            // 每 3 个操作码一行
            for chunk in entry.ops.chunks(3) {
                lines.push(chunk.join(" "));
            }
        }

        lines.push(String::new()); // 条目之间的空行
    }

    // 确保最后有换行符
    let mut result = lines.join("\n");
    result = result.trim_end_matches('\n').to_string();
    result.push('\n');
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_cheat() {
        let text = r#"[Test Cheat]
12345678 ABCDEFAB FEDCBA98
11111111 22222222 33333333
"#;

        let result = parse_text(text, 1000).unwrap();
        assert_eq!(result.entries.len(), 1);

        let entry = &result.entries[0];
        assert_eq!(entry.title, "Test Cheat");
        assert_eq!(entry.ops.len(), 6);
        assert_eq!(entry.ops[0], "12345678");
        assert_eq!(entry.ops[5], "33333333");
    }

    #[test]
    fn test_parse_multiple_cheats() {
        let text = r#"[Cheat 1]
12345678

[Cheat 2]
ABCDEFAB
"#;

        let result = parse_text(text, 1000).unwrap();
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].title, "Cheat 1");
        assert_eq!(result.entries[1].title, "Cheat 2");
    }

    #[test]
    fn test_parse_with_default() {
        let text = r#"{Default Cheat}
12345678 ABCDEFAB
"#;

        let result = parse_text(text, 1000).unwrap();
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].title, "Default Cheat");
    }

    #[test]
    fn test_parse_invalid_hex() {
        let text = r#"[Test]
1234567 ABCD
"#; // 7 位，不是 8 位

        let result = parse_text(text, 1000);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize() {
        let mut cheat_file = CheatFile::new();
        cheat_file.add_entry(CheatEntry::new(
            "Test",
            vec!["12345678".to_string(), "ABCDEFAB".to_string()],
        ));

        let serialized = serialize(&cheat_file);
        assert!(serialized.contains("[Test]"));
        assert!(serialized.contains("12345678 ABCDEFAB"));
    }

    #[test]
    fn test_empty_file() {
        let result = parse_text("", 1000).unwrap();
        assert_eq!(result.entries.len(), 0);
    }
}
