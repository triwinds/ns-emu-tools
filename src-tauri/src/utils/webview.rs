//! WebView2 检测和安装模块
//!
//! 提供 WebView2 运行时检测和自动安装功能

use tracing::{info, warn, error};
use std::path::PathBuf;

/// 检查 WebView2 是否已安装
///
/// 在 Windows 上检查注册表,确定 WebView2 Runtime 是否已安装
/// 在其他平台上始终返回 true（因为它们使用系统 webview）
#[cfg(target_os = "windows")]
pub fn check_webview2_installed() -> bool {
    use windows::Win32::System::Registry::*;
    use windows::core::w;
    use windows::Win32::Foundation::ERROR_SUCCESS;

    // WebView2 Runtime 的注册表位置
    let registry_paths = [
        w!("SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"),
        w!("SOFTWARE\\Microsoft\\EdgeUpdate\\Clients\\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"),
    ];

    for path in &registry_paths {
        let mut key = HKEY::default();
        let result = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                *path,
                0,
                KEY_READ,
                &mut key,
            )
        };

        if result == ERROR_SUCCESS {
            unsafe { let _ = RegCloseKey(key); }
            info!("检测到 WebView2 Runtime 已安装");
            return true;
        }
    }

    // 也检查用户级别的安装
    for path in &registry_paths {
        let mut key = HKEY::default();
        let result = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                *path,
                0,
                KEY_READ,
                &mut key,
            )
        };

        if result == ERROR_SUCCESS {
            unsafe { let _ = RegCloseKey(key); }
            info!("检测到 WebView2 Runtime 已安装（用户级别）");
            return true;
        }
    }

    warn!("未检测到 WebView2 Runtime");
    false
}

#[cfg(not(target_os = "windows"))]
pub fn check_webview2_installed() -> bool {
    // 非 Windows 平台使用系统 webview，无需检查
    true
}

/// 下载 WebView2 Bootstrapper
///
/// 从微软官方下载 WebView2 Runtime 安装程序
#[cfg(target_os = "windows")]
async fn download_webview2_bootstrapper() -> anyhow::Result<PathBuf> {
    use std::io::Write;

    info!("正在下载 WebView2 Runtime 安装程序...");

    // WebView2 Bootstrapper 官方下载链接
    let url = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

    // 保存到临时目录
    let temp_dir = std::env::temp_dir();
    let installer_path = temp_dir.join("MicrosoftEdgeWebview2Setup.exe");

    // 下载文件
    let response = reqwest::get(url).await?;
    let bytes = response.bytes().await?;

    // 写入文件
    let mut file = std::fs::File::create(&installer_path)?;
    file.write_all(&bytes)?;

    info!("WebView2 安装程序下载完成: {:?}", installer_path);
    Ok(installer_path)
}

/// 运行 WebView2 安装程序
///
/// 运行下载的安装程序并等待完成
#[cfg(target_os = "windows")]
fn install_webview2(installer_path: &PathBuf) -> anyhow::Result<()> {
    info!("正在运行 WebView2 安装程序...");
    info!("安装过程可能需要几分钟，请耐心等待...");

    // 运行安装程序
    let status = std::process::Command::new(installer_path)
        .arg("/install")  // 静默安装
        .status()?;

    if status.success() {
        info!("WebView2 Runtime 安装成功");
        Ok(())
    } else {
        error!("WebView2 Runtime 安装失败");
        Err(anyhow::anyhow!("安装程序返回错误代码: {:?}", status.code()))
    }
}

/// 显示消息对话框
#[cfg(target_os = "windows")]
fn show_message_box(title: &str, message: &str, is_question: bool) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::PCWSTR;

    let title_wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let message_wide: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();

    let flags = if is_question {
        MB_YESNO | MB_ICONQUESTION
    } else {
        MB_OK | MB_ICONINFORMATION
    };

    let result = unsafe {
        MessageBoxW(
            None,
            PCWSTR(message_wide.as_ptr()),
            PCWSTR(title_wide.as_ptr()),
            flags,
        )
    };

    result == IDYES
}

/// 在应用启动时检查并安装 WebView2
///
/// 如果未安装，会提示用户并自动下载安装
#[cfg(target_os = "windows")]
pub fn check_and_install_on_startup() -> anyhow::Result<()> {
    info!("正在检查 WebView2 Runtime 状态...");

    if check_webview2_installed() {
        info!("✓ WebView2 Runtime 检查通过");
        return Ok(());
    }

    info!("✗ 未检测到 WebView2 Runtime");

    // 询问用户是否下载安装
    let user_agreed = show_message_box(
        "需要安装 WebView2 Runtime",
        "NS Emu Tools 需要 Microsoft Edge WebView2 Runtime 才能运行。\n\n是否现在下载并安装？\n\n安装过程需要网络连接，大约需要几分钟时间。",
        true,
    );

    if !user_agreed {
        error!("用户取消了 WebView2 安装");
        show_message_box(
            "无法启动应用",
            "没有 WebView2 Runtime，应用无法启动。\n\n您可以稍后从以下地址手动下载安装：\nhttps://developer.microsoft.com/en-us/microsoft-edge/webview2/",
            false,
        );
        return Err(anyhow::anyhow!("用户取消安装"));
    }

    // 创建运行时环境来执行异步操作
    let rt = tokio::runtime::Runtime::new()?;

    // 下载安装程序
    let installer_path = rt.block_on(async {
        download_webview2_bootstrapper().await
    })?;

    // 运行安装程序
    install_webview2(&installer_path)?;

    // 清理临时文件
    let _ = std::fs::remove_file(&installer_path);

    // 验证安装
    if check_webview2_installed() {
        info!("✓ WebView2 Runtime 安装并验证成功");
        show_message_box(
            "安装成功",
            "WebView2 Runtime 安装成功！\n\n应用即将启动...",
            false,
        );
        Ok(())
    } else {
        error!("安装完成但未检测到 WebView2 Runtime");
        Err(anyhow::anyhow!("安装验证失败"))
    }
}

#[cfg(not(target_os = "windows"))]
pub fn check_and_install_on_startup() -> anyhow::Result<()> {
    // 非 Windows 平台无需处理
    Ok(())
}
