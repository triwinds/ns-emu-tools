//! 文件名解析
//!
//! 从 HTTP 响应头和 URL 中解析文件名

use reqwest::header::CONTENT_DISPOSITION;
use reqwest::Response;
use std::path::Path;
use tracing::debug;
use url::Url;

/// 从响应中解析文件名
///
/// 优先级：
/// 1. 用户指定的文件名
/// 2. Content-Disposition 头
/// 3. URL 路径
/// 4. 默认名称
pub fn resolve_filename(response: &Response, url: &str, user_filename: Option<&str>) -> String {
    // 1. 优先使用用户指定的文件名
    if let Some(filename) = user_filename {
        if !filename.is_empty() {
            debug!("使用用户指定的文件名: {}", filename);
            return sanitize_filename(filename);
        }
    }

    // 2. 从 Content-Disposition 头解析
    if let Some(cd) = response.headers().get(CONTENT_DISPOSITION) {
        if let Ok(cd_str) = cd.to_str() {
            if let Some(filename) = parse_content_disposition(cd_str) {
                debug!("从 Content-Disposition 解析文件名: {}", filename);
                return sanitize_filename(&filename);
            }
        }
    }

    // 3. 从 URL 路径提取
    if let Some(filename) = extract_filename_from_url(url) {
        debug!("从 URL 提取文件名: {}", filename);
        return sanitize_filename(&filename);
    }

    // 4. 使用默认名称
    let default_name = format!("download_{}", uuid::Uuid::new_v4());
    debug!("使用默认文件名: {}", default_name);
    default_name
}

/// 从 URL 解析文件名（不需要响应）
pub fn resolve_filename_from_url(url: &str, user_filename: Option<&str>) -> String {
    // 1. 优先使用用户指定的文件名
    if let Some(filename) = user_filename {
        if !filename.is_empty() {
            return sanitize_filename(filename);
        }
    }

    // 2. 从 URL 路径提取
    if let Some(filename) = extract_filename_from_url(url) {
        return sanitize_filename(&filename);
    }

    // 3. 使用默认名称
    format!("download_{}", uuid::Uuid::new_v4())
}

/// 解析 Content-Disposition 头
///
/// 支持两种格式：
/// - filename="example.zip"
/// - filename*=UTF-8''%E4%B8%AD%E6%96%87.zip (RFC 5987)
fn parse_content_disposition(header: &str) -> Option<String> {
    // 优先解析 filename* 格式（支持非 ASCII 字符）
    if let Some(filename) = parse_filename_star(header) {
        return Some(filename);
    }

    // 解析普通 filename 格式
    parse_filename_simple(header)
}

/// 解析 filename*=UTF-8''xxx 格式 (RFC 5987)
fn parse_filename_star(header: &str) -> Option<String> {
    // 查找 filename*=
    let lower = header.to_lowercase();
    let start = lower.find("filename*=")?;
    let value_start = start + "filename*=".len();
    let remaining = &header[value_start..];

    // 解析编码格式，如 UTF-8''xxx
    let parts: Vec<&str> = remaining.splitn(2, "''").collect();
    if parts.len() != 2 {
        return None;
    }

    let encoded = parts[1].split(';').next()?.trim();

    // URL 解码
    urlencoding::decode(encoded).ok().map(|s| s.into_owned())
}

/// 解析 filename="xxx" 格式
fn parse_filename_simple(header: &str) -> Option<String> {
    // 查找 filename=（但不是 filename*=）
    let lower = header.to_lowercase();

    // 找到所有 filename= 的位置
    let mut search_start = 0;
    while let Some(pos) = lower[search_start..].find("filename=") {
        let abs_pos = search_start + pos;

        // 检查是否是 filename*=
        if abs_pos > 0 && header.as_bytes()[abs_pos - 1] == b'*' {
            search_start = abs_pos + 1;
            continue;
        }

        let value_start = abs_pos + "filename=".len();
        let remaining = &header[value_start..];

        // 处理带引号的值
        if remaining.starts_with('"') {
            let end = remaining[1..].find('"')?;
            return Some(remaining[1..end + 1].to_string());
        }

        // 处理不带引号的值
        let end = remaining
            .find(|c: char| c == ';' || c.is_whitespace())
            .unwrap_or(remaining.len());
        return Some(remaining[..end].to_string());
    }

    None
}

/// 从 URL 提取文件名
fn extract_filename_from_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let path = parsed.path();

    // 获取路径的最后一部分
    let filename = path.rsplit('/').next()?;

    if filename.is_empty() {
        return None;
    }

    // URL 解码
    urlencoding::decode(filename).ok().map(|s| s.into_owned())
}

/// 清理文件名中的非法字符
pub fn sanitize_filename(filename: &str) -> String {
    let mut result = filename.to_string();

    // Windows 非法字符
    #[cfg(windows)]
    {
        const ILLEGAL_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        for c in ILLEGAL_CHARS {
            result = result.replace(*c, "_");
        }

        // Windows 保留名称
        const RESERVED_NAMES: &[&str] = &[
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
        ];

        let upper = result.to_uppercase();
        for name in RESERVED_NAMES {
            if upper == *name || upper.starts_with(&format!("{}.", name)) {
                result = format!("_{}", result);
                break;
            }
        }
    }

    // Unix 非法字符
    #[cfg(not(windows))]
    {
        result = result.replace('/', "_");
        result = result.replace('\0', "_");
    }

    // 通用清理
    result = result.trim().to_string();

    // 移除开头的点（隐藏文件）
    while result.starts_with('.') {
        result = result[1..].to_string();
    }

    if result.is_empty() {
        result = "download".to_string();
    }

    // 限制长度（考虑 .part 和 .download 后缀）
    const MAX_FILENAME_LEN: usize = 200;
    if result.len() > MAX_FILENAME_LEN {
        let ext = Path::new(&result)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        if ext.is_empty() {
            result = result[..MAX_FILENAME_LEN].to_string();
        } else {
            let stem_max = MAX_FILENAME_LEN - ext.len() - 1;
            let stem = &result[..stem_max.min(result.len() - ext.len() - 1)];
            result = format!("{}.{}", stem, ext);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_content_disposition_simple() {
        assert_eq!(
            parse_content_disposition("attachment; filename=\"test.zip\""),
            Some("test.zip".to_string())
        );

        assert_eq!(
            parse_content_disposition("attachment; filename=test.zip"),
            Some("test.zip".to_string())
        );

        assert_eq!(
            parse_content_disposition("inline; filename=\"file name.txt\""),
            Some("file name.txt".to_string())
        );
    }

    #[test]
    fn test_parse_content_disposition_rfc5987() {
        assert_eq!(
            parse_content_disposition("attachment; filename*=UTF-8''%E4%B8%AD%E6%96%87.zip"),
            Some("中文.zip".to_string())
        );

        assert_eq!(
            parse_content_disposition(
                "attachment; filename=\"fallback.zip\"; filename*=UTF-8''%E4%B8%AD%E6%96%87.zip"
            ),
            Some("中文.zip".to_string())
        );
    }

    #[test]
    fn test_extract_filename_from_url() {
        assert_eq!(
            extract_filename_from_url("https://example.com/path/to/file.zip"),
            Some("file.zip".to_string())
        );

        assert_eq!(
            extract_filename_from_url("https://example.com/path/to/%E4%B8%AD%E6%96%87.zip"),
            Some("中文.zip".to_string())
        );

        assert_eq!(
            extract_filename_from_url("https://example.com/"),
            None
        );

        assert_eq!(
            extract_filename_from_url("https://example.com/path/"),
            None
        );
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal.zip"), "normal.zip");
        assert_eq!(sanitize_filename("  spaces  "), "spaces");
        assert_eq!(sanitize_filename(".hidden"), "hidden");
        assert_eq!(sanitize_filename("...dots"), "dots");
        assert_eq!(sanitize_filename(""), "download");
    }

    #[cfg(windows)]
    #[test]
    fn test_sanitize_filename_windows() {
        assert_eq!(sanitize_filename("file<>:\"|?*.zip"), "file________.zip");
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("LPT1"), "_LPT1");
    }

    #[cfg(not(windows))]
    #[test]
    fn test_sanitize_filename_unix() {
        assert_eq!(sanitize_filename("file/name.zip"), "file_name.zip");
        assert_eq!(sanitize_filename("file\0name.zip"), "file_name.zip");
    }

    #[test]
    fn test_sanitize_filename_long() {
        let long_name = "a".repeat(300) + ".zip";
        let sanitized = sanitize_filename(&long_name);
        assert!(sanitized.len() <= 200);
        assert!(sanitized.ends_with(".zip"));
    }

    #[test]
    fn test_resolve_filename_from_url() {
        // 用户指定的文件名优先
        assert_eq!(
            resolve_filename_from_url("https://example.com/file.zip", Some("custom.zip")),
            "custom.zip"
        );

        // 从 URL 提取
        assert_eq!(
            resolve_filename_from_url("https://example.com/path/to/file.zip", None),
            "file.zip"
        );

        // 默认名称
        let default = resolve_filename_from_url("https://example.com/", None);
        assert!(default.starts_with("download_"));
    }
}
