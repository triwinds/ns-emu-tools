//! 压缩文件处理模块
//!
//! 提供 ZIP、7z、tar.xz 等格式的解压缩功能

use crate::error::{AppError, AppResult};
use std::fs::File;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
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

    // 验证文件存在和大小
    let file_metadata = filepath.metadata()
        .map_err(|e| AppError::Extract(format!("无法读取文件元数据: {}", e)))?;

    let file_size = file_metadata.len();
    debug!("tar.gz 文件大小: {} bytes ({:.2} MB)", file_size, file_size as f64 / 1024.0 / 1024.0);

    if file_size == 0 {
        return Err(AppError::Extract("文件大小为0，可能下载不完整".to_string()));
    }

    if file_size < 100 {
        return Err(AppError::Extract(format!("文件过小 ({} bytes)，可能是错误页面或下载失败", file_size)));
    }

    // 验证 gzip 魔数
    let mut file = File::open(filepath)?;
    let mut magic = [0u8; 2];
    if let Err(e) = file.read_exact(&mut magic) {
        return Err(AppError::Extract(format!("读取文件头失败: {}", e)));
    }

    // gzip 文件签名: 1F 8B
    if magic[0] != 0x1F || magic[1] != 0x8B {
        warn!("文件头: {:02X} {:02X}, 不是有效的 gzip 文件", magic[0], magic[1]);

        // 尝试读取前100字节查看内容
        file.rewind()?;
        let mut preview = vec![0u8; 100.min(file_size as usize)];
        file.read_exact(&mut preview)?;
        let preview_str = String::from_utf8_lossy(&preview);

        // 检查是否是 HTML 错误页面
        if preview_str.to_lowercase().contains("<html") || preview_str.to_lowercase().contains("<!doctype") {
            return Err(AppError::Extract(
                format!("下载的文件不是 tar.gz，而是 HTML 页面，可能是下载链接错误或需要认证")
            ));
        }

        return Err(AppError::Extract(
            format!("不是有效的 gzip 文件 (魔数: {:02X} {:02X})，文件可能已损坏或下载不完整", magic[0], magic[1])
        ));
    }

    debug!("gzip 文件头验证通过");

    // 重新打开文件进行解压
    file.rewind()?;
    let decoder = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(decoder);

    // 创建目标目录
    std::fs::create_dir_all(target_path)?;

    // 解压时提供详细的错误信息
    archive
        .unpack(target_path)
        .map_err(|e| {
            error!("解压 tar.gz 失败，文件: {}, 大小: {} bytes, 错误: {}",
                   filepath.display(), file_size, e);

            // 提供更友好的错误信息
            let error_msg = e.to_string();
            if error_msg.contains("failed to iterate") {
                AppError::Extract(format!(
                    "无法读取 tar 归档内容 ({}), 可能原因:\n\
                    1. 文件下载不完整，请重试\n\
                    2. 文件在下载过程中损坏\n\
                    3. 磁盘空间不足或权限问题\n\
                    4. 网络连接在下载时中断\n\
                    建议: 删除下载的文件并重新下载",
                    error_msg
                ))
            } else {
                AppError::Extract(format!("解压 tar.gz 失败: {}", error_msg))
            }
        })?;

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

/// 挂载并提取 DMG 文件中的 .app (仅 macOS)
#[cfg(target_os = "macos")]
pub fn extract_dmg(dmg_path: &Path, target_path: &Path) -> AppResult<PathBuf> {
    use std::process::Command;

    info!("挂载 DMG: {} -> {}", dmg_path.display(), target_path.display());

    // 创建临时挂载点
    let mount_point = std::env::temp_dir().join(format!("dmg_mount_{}", std::process::id()));
    std::fs::create_dir_all(&mount_point)?;

    // 挂载 DMG
    let mount_result = Command::new("hdiutil")
        .args(["attach"])
        .arg(dmg_path)
        .args(["-nobrowse", "-readonly", "-mountpoint"])
        .arg(&mount_point)
        .output()?;

    if !mount_result.status.success() {
        let _ = std::fs::remove_dir_all(&mount_point);
        return Err(AppError::Extract(format!(
            "DMG 挂载失败: {}",
            String::from_utf8_lossy(&mount_result.stderr)
        )));
    }

    // 查找 .app
    let app_path = find_app_in_dir(&mount_point)?;
    let app_name = app_path.file_name()
        .ok_or_else(|| AppError::Extract("无法获取 .app 名称".to_string()))?;

    // 确保目标目录存在
    std::fs::create_dir_all(target_path)?;

    let target_app = target_path.join(app_name);

    // 如果目标已存在，先删除
    if target_app.exists() {
        std::fs::remove_dir_all(&target_app)?;
    }

    // 使用 ditto 复制 .app（保留权限和扩展属性）
    let copy_result = Command::new("ditto")
        .args(["--rsrc", "--extattr"])
        .arg(&app_path)
        .arg(&target_app)
        .output()?;

    // 卸载 DMG（无论复制是否成功都要卸载）
    let _ = Command::new("hdiutil")
        .args(["detach", "-quiet"])
        .arg(&mount_point)
        .output();

    // 清理挂载点目录
    let _ = std::fs::remove_dir_all(&mount_point);

    if !copy_result.status.success() {
        return Err(AppError::Extract(format!(
            "复制 .app 失败: {}",
            String::from_utf8_lossy(&copy_result.stderr)
        )));
    }

    info!("DMG 提取完成: {}", target_app.display());
    Ok(target_app)
}

/// 在目录中查找 .app 包
#[cfg(target_os = "macos")]
fn find_app_in_dir(dir: &Path) -> AppResult<PathBuf> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name() {
                if name.to_string_lossy().ends_with(".app") {
                    return Ok(path);
                }
            }
        }
    }
    Err(AppError::Extract("DMG 中未找到 .app 文件".to_string()))
}

/// 从 tar.gz 中提取 .app 并安装 (仅 macOS)
#[cfg(target_os = "macos")]
pub fn extract_and_install_app_from_tar_gz(
    tar_gz_path: &Path,
    target_path: &Path,
    app_name: &str, // 例如 "Eden.app"
) -> AppResult<PathBuf> {
    use std::process::Command;

    info!("从 tar.gz 提取 .app: {}", tar_gz_path.display());

    // 解压到临时目录
    let tmp_dir = std::env::temp_dir().join(format!("app_extract_{}", std::process::id()));
    if tmp_dir.exists() {
        std::fs::remove_dir_all(&tmp_dir)?;
    }
    std::fs::create_dir_all(&tmp_dir)?;

    // 使用现有的 extract_tar_gz
    extract_tar_gz(tar_gz_path, &tmp_dir)?;

    // 在解压目录中递归查找目标 .app
    let app_path = find_app_recursive(&tmp_dir, app_name)?;

    // 确保目标目录存在
    std::fs::create_dir_all(target_path)?;

    let target_app = target_path.join(app_name);

    // 如果目标已存在，先删除
    if target_app.exists() {
        std::fs::remove_dir_all(&target_app)?;
    }

    // 使用 ditto 复制（保留权限和扩展属性）
    let copy_result = Command::new("ditto")
        .args(["--rsrc", "--extattr"])
        .arg(&app_path)
        .arg(&target_app)
        .output()?;

    // 清理临时目录
    let _ = std::fs::remove_dir_all(&tmp_dir);

    if !copy_result.status.success() {
        return Err(AppError::Extract(format!(
            "复制 .app 失败: {}",
            String::from_utf8_lossy(&copy_result.stderr)
        )));
    }

    // 验证安装结果
    let info_plist = target_app.join("Contents/Info.plist");
    if !info_plist.exists() {
        return Err(AppError::Extract(
            ".app 安装验证失败: Contents/Info.plist 不存在".to_string()
        ));
    }

    info!(".app 安装完成: {}", target_app.display());
    Ok(target_app)
}

/// 递归查找指定名称的 .app
#[cfg(target_os = "macos")]
fn find_app_recursive(dir: &Path, app_name: &str) -> AppResult<PathBuf> {
    // 首先在当前目录查找
    let direct_path = dir.join(app_name);
    if direct_path.exists() && direct_path.is_dir() {
        return Ok(direct_path);
    }

    // 递归查找子目录
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string());
            if name.as_deref() == Some(app_name) {
                return Ok(path);
            }
            // 继续在子目录中查找
            if let Ok(found) = find_app_recursive(&path, app_name) {
                return Ok(found);
            }
        }
    }

    Err(AppError::Extract(format!("未找到 {}", app_name)))
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
