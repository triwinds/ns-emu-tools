//! 校验运行库下载包、归档成员和缓存路径。
use super::{pe_architecture, ExecutableArchitecture};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Read,
    path::Path,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct Asset {
    pub name: String,
    pub url: String,
    pub sha256: String,
}

pub(super) fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// 拒绝整个路径链中的链接和 Windows junction，而非只检查最终文件。
pub(super) fn safe_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("必须提供不包含上级跳转的绝对路径".into());
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                let linked = metadata.file_type().is_symlink();
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    linked || metadata.file_attributes() & 0x400 != 0
                };
                if linked {
                    return Err(format!("禁止符号链接或重解析点: {}", ancestor.display()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

/// Read only named members, normalize the upstream ZIP's Windows separators, never extract paths.
pub(super) fn member(package: &Path, wanted: &str, dll: bool) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(File::open(package).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if zip.len() > 4096 {
        return Err("ZIP 条目过多".into());
    }
    let mut found = None;
    let mut seen = std::collections::HashSet::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().replace('\\', "/");
        if name.starts_with('/')
            || name.contains(':')
            || name.split('/').any(|p| p == ".." || p == ".")
            || !seen.insert(name.to_ascii_lowercase())
        {
            return Err("ZIP 路径不安全或重复".into());
        }
        if name.eq_ignore_ascii_case(wanted) {
            if entry.is_dir()
                || entry.size() == 0
                || entry.size() > 256 * 1024 * 1024
                || entry
                    .unix_mode()
                    .is_some_and(|m| !matches!(m & 0o170000, 0 | 0o100000))
            {
                return Err("ZIP 文件类型或大小无效".into());
            }
            found = Some(i);
        }
    }
    let entry = zip
        .by_index(found.ok_or_else(|| format!("ZIP 缺少 {wanted}"))?)
        .map_err(|e| e.to_string())?;
    let expected = entry.size();
    let mut bytes = Vec::new();
    entry
        .take(256 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != expected {
        return Err("ZIP 解压大小不一致".into());
    }
    if dll
        && pe_architecture(&mut std::io::Cursor::new(&bytes), expected, true)?
            != ExecutableArchitecture::X64
    {
        return Err("组件必须是 x64 DLL".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[test]
    fn rejects_unsafe_and_duplicate_members() {
        for names in [
            vec!["../escape", "test"],
            vec!["test", "TEST"],
            vec!["C:\\bad", "test"],
        ] {
            let f = tempfile::NamedTempFile::new().unwrap();
            let mut zip = zip::ZipWriter::new(f.reopen().unwrap());
            for n in names {
                zip.start_file(n, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"data").unwrap();
            }
            zip.finish().unwrap();
            assert!(member(f.path(), "test", false).is_err());
        }
    }
    #[test]
    fn accepts_upstream_windows_separators() {
        let f = tempfile::NamedTempFile::new().unwrap();
        let mut zip = zip::ZipWriter::new(f.reopen().unwrap());
        zip.start_file(
            "licenses\\notice.txt",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"notice").unwrap();
        zip.finish().unwrap();
        assert_eq!(
            member(f.path(), "licenses/notice.txt", false).unwrap(),
            b"notice"
        );
    }

    #[cfg(windows)]
    #[test]
    fn junction_ancestors_are_rejected_without_touching_destination() {
        let dir = tempfile::tempdir().unwrap();
        let directory = dir.path().canonicalize().unwrap();
        let outside = directory.join("outside");
        fs::create_dir(&outside).unwrap();
        let junction = directory.join("cache");
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rejected = safe_path(&junction.join("nested.dll")).is_err();
        fs::remove_dir(&junction).unwrap();
        assert!(rejected);
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
    }
}
