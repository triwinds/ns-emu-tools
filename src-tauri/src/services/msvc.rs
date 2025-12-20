//! MSVC 运行库检查和安装服务
//!
//! 提供 Microsoft Visual C++ Redistributable 的检查和安装功能

use crate::error::{AppError, AppResult};
use crate::services::aria2::{get_aria2_manager, Aria2DownloadOptions};
use std::path::PathBuf;
use std::process::Command;
use tracing::{info, warn};

/// MSVC 运行库下载 URL
const MSVC_DOWNLOAD_URL: &str = "https://aka.ms/vs/17/release/VC_redist.x64.exe";

/// 检查关键 DLL 文件名
const MSVC_KEY_DLL: &str = "msvcp140_atomic_wait.dll";

/// 检查 MSVC 运行库是否已安装
///
/// 通过检查系统目录中是否存在关键 DLL 文件来判断
pub fn is_msvc_installed() -> bool {
    if let Ok(windir) = std::env::var("windir") {
        let dll_path = PathBuf::from(windir)
            .join("System32")
            .join(MSVC_KEY_DLL);

        let installed = dll_path.exists();
        if installed {
            info!("MSVC 运行库已安装: {}", dll_path.display());
        } else {
            warn!("MSVC 运行库未安装，缺少文件: {}", dll_path.display());
        }
        installed
    } else {
        warn!("无法获取 Windows 目录");
        false
    }
}

/// 下载 MSVC 运行库安装包
///
/// # 返回
/// 下载的安装包路径
async fn download_msvc_installer() -> AppResult<PathBuf> {
    info!("开始下载 MSVC 运行库安装包");

    let aria2 = get_aria2_manager().await?;
    let options = Aria2DownloadOptions {
        use_github_mirror: false,
        ..Default::default()
    };

    let result = aria2.download_and_wait(
        MSVC_DOWNLOAD_URL,
        options,
        |progress| {
            info!("下载进度: {}%", progress.percentage);
        },
    ).await?;

    info!("MSVC 安装包下载完成: {}", result.path.display());
    Ok(result.path)
}

/// 启动 MSVC 安装程序
///
/// # 参数
/// * `installer_path` - 安装包路径
fn launch_msvc_installer(installer_path: &PathBuf) -> AppResult<()> {
    info!("启动 MSVC 安装程序: {}", installer_path.display());

    Command::new(installer_path)
        .spawn()
        .map_err(|e| AppError::Io(e))?;

    info!("MSVC 安装程序已启动，请按照提示完成安装");
    Ok(())
}

/// 检查并安装 MSVC 运行库
///
/// 如果未安装，将自动下载并启动安装程序
pub async fn check_and_install_msvc() -> AppResult<()> {
    info!("检查 MSVC 运行库");

    if is_msvc_installed() {
        info!("MSVC 运行库检查通过");
        return Ok(());
    }

    warn!("MSVC 运行库未安装，准备下载安装包");

    // 下载安装包
    let installer_path = download_msvc_installer().await?;

    // 启动安装程序
    launch_msvc_installer(&installer_path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_msvc_installed() {
        // 只是确保函数不会 panic
        let _ = is_msvc_installed();
    }
}
