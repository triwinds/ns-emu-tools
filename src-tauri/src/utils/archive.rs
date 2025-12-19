//! 压缩文件处理模块
//!
//! 提供 ZIP、7z、tar.xz 等格式的解压缩功能

use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;
use tracing::{debug, error, info, warn};

/// 解压文件（自动检测格式）
pub fn uncompress(
    filepath: &Path,
    target_path: &Path,
    delete_on_error: bool,
) -> AppResult<()> {
    let filename = filepath
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();

    let result = if filename.ends_with(".zip") {
        extract_zip(filepath, target_path)
    } else if filename.ends_with(".7z") {
        extract_7z(filepath, target_path)
    } else if filename.ends_with(".tar.xz") {
        extract_tar_xz(filepath, target_path)
    } else if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") {
        extract_tar_gz(filepath, target_path)
    } else {
        Err(AppError::Extract(format!(
            "不支持的压缩格式: {}",
            filename
        )))
    };

    if let Err(ref e) = result {
        error!("解压失败: {} - {}", filepath.display(), e);
        if delete_on_error {
            warn!("删除损坏的文件: {}", filepath.display());
            let _ = std::fs::remove_file(filepath);
        }
    }

    result
}

/// 解压 ZIP 文件
pub fn extract_zip(filepath: &Path, target_path: &Path) -> AppResult<()> {
    info!("解压 ZIP: {} -> {}", filepath.display(), target_path.display());

    let file = File::open(filepath)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Extract(format!("打开 ZIP 文件失败: {}", e)))?;

    // 创建目标目录
    std::fs::create_dir_all(target_path)?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| AppError::Extract(format!("读取 ZIP 条目失败: {}", e)))?;

        let outpath = match file.enclosed_name() {
            Some(path) => target_path.join(path),
            None => continue,
        };

        if file.is_dir() {
            debug!("创建目录: {}", outpath.display());
            std::fs::create_dir_all(&outpath)?;
        } else {
            debug!("解压文件: {}", outpath.display());
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }

            let mut outfile = File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }

        // 设置 Unix 权限（如果可用）
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                std::fs::set_permissions(&outpath, std::fs::Permissions::from_mode(mode))?;
            }
        }
    }

    info!("ZIP 解压完成");
    Ok(())
}

/// 解压 7z 文件
pub fn extract_7z(filepath: &Path, target_path: &Path) -> AppResult<()> {
    info!("解压 7z: {} -> {}", filepath.display(), target_path.display());

    // 创建目标目录
    std::fs::create_dir_all(target_path)?;

    sevenz_rust::decompress_file(filepath, target_path)
        .map_err(|e| AppError::Extract(format!("解压 7z 失败: {}", e)))?;

    info!("7z 解压完成");
    Ok(())
}

/// 解压 tar.xz 文件
pub fn extract_tar_xz(filepath: &Path, target_path: &Path) -> AppResult<()> {
    info!("解压 tar.xz: {} -> {}", filepath.display(), target_path.display());

    let file = File::open(filepath)?;
    let decoder = xz2::read::XzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    // 创建目标目录
    std::fs::create_dir_all(target_path)?;

    archive
        .unpack(target_path)
        .map_err(|e| AppError::Extract(format!("解压 tar.xz 失败: {}", e)))?;

    info!("tar.xz 解压完成");
    Ok(())
}

/// 解压 tar.gz 文件
pub fn extract_tar_gz(filepath: &Path, target_path: &Path) -> AppResult<()> {
    info!("解压 tar.gz: {} -> {}", filepath.display(), target_path.display());

    let file = File::open(filepath)?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    // 创建目标目录
    std::fs::create_dir_all(target_path)?;

    archive
        .unpack(target_path)
        .map_err(|e| AppError::Extract(format!("解压 tar.gz 失败: {}", e)))?;

    info!("tar.gz 解压完成");
    Ok(())
}

/// 压缩文件夹为 7z
pub fn compress_folder_to_7z(folder_path: &Path, save_path: &Path) -> AppResult<()> {
    info!(
        "压缩文件夹: {} -> {}",
        folder_path.display(),
        save_path.display()
    );

    if !folder_path.exists() {
        return Err(AppError::DirectoryNotFound(
            folder_path.display().to_string(),
        ));
    }

    sevenz_rust::compress_to_path(folder_path, save_path)
        .map_err(|e| AppError::Extract(format!("压缩失败: {}", e)))?;

    info!("压缩完成");
    Ok(())
}

/// 检查是否为有效的 7z 文件
pub fn is_7z_file(filepath: &Path) -> bool {
    if !filepath.exists() {
        return false;
    }

    // 检查 7z 文件魔数
    if let Ok(mut file) = File::open(filepath) {
        let mut magic = [0u8; 6];
        if file.read_exact(&mut magic).is_ok() {
            // 7z 文件签名: 37 7A BC AF 27 1C
            return magic == [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
        }
    }

    false
}

/// 检查是否为有效的 ZIP 文件
pub fn is_zip_file(filepath: &Path) -> bool {
    if !filepath.exists() {
        return false;
    }

    if let Ok(mut file) = File::open(filepath) {
        let mut magic = [0u8; 4];
        if file.read_exact(&mut magic).is_ok() {
            // ZIP 文件签名: 50 4B 03 04 或 50 4B 05 06 (空压缩包)
            return (magic[0] == 0x50 && magic[1] == 0x4B)
                && (magic[2] == 0x03 || magic[2] == 0x05);
        }
    }

    false
}

/// 获取 ZIP 文件中的条目列表
pub fn list_zip_entries(filepath: &Path) -> AppResult<Vec<String>> {
    let file = File::open(filepath)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Extract(format!("打开 ZIP 文件失败: {}", e)))?;

    let entries: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            archive
                .by_index(i)
                .ok()
                .and_then(|f| f.enclosed_name().map(|p| p.to_string_lossy().to_string()))
        })
        .collect();

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_is_7z_file() {
        // 创建一个假的 7z 文件头
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.7z");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(&[0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C]).unwrap();

        assert!(is_7z_file(&file_path));
    }

    #[test]
    fn test_is_zip_file() {
        // 创建一个假的 ZIP 文件头
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.zip");
        let mut file = File::create(&file_path).unwrap();
        file.write_all(&[0x50, 0x4B, 0x03, 0x04]).unwrap();

        assert!(is_zip_file(&file_path));
    }

    #[test]
    fn test_extract_zip() {
        // 创建一个简单的 ZIP 文件
        let dir = tempdir().unwrap();
        let zip_path = dir.path().join("test.zip");
        let extract_path = dir.path().join("extracted");

        // 创建 ZIP
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("test.txt", options).unwrap();
            zip.write_all(b"Hello, World!").unwrap();
            zip.finish().unwrap();
        }

        // 解压
        extract_zip(&zip_path, &extract_path).unwrap();

        // 验证
        let extracted_file = extract_path.join("test.txt");
        assert!(extracted_file.exists());

        let content = std::fs::read_to_string(extracted_file).unwrap();
        assert_eq!(content, "Hello, World!");
    }
}
