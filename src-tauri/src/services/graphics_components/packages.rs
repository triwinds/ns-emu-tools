//! 官方包缓存；下载和取消复用统一下载管理器。
use super::*;
use crate::repositories::graphics_components::{latest, OFFICIAL_SITE};
use crate::services::downloader::{get_download_manager, DownloadManager, DownloadOptions};
use crate::services::installer::{
    download_progress_step, running_step, success_download_step, success_step, InstallReporter,
};
use serde::{Deserialize, Serialize};
use transaction::{atomic_write, file_hash, safe_path};

static CACHE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheRecord {
    schema_version: u32,
    package_sha256: String,
    source_url: String,
}

pub async fn prepare_official(
    executable: PathBuf,
    graphics_api: GraphicsApi,
    reporter: InstallReporter,
) -> Result<GraphicsInstallPreview, String> {
    transaction::target_directory(&executable)?;
    let _guard = CACHE_LOCK.lock().await;
    reporter.step(running_step("source", "解析官方稳定版"));
    let release = latest().await?;
    reporter.step(success_step(
        "source",
        format!("ReShade {} full addon", release.version),
    ));
    let cache = effective_config_dir()
        .join("graphics-components")
        .join("cache")
        .join(format!("reshade-{}-x64-addon", release.version));
    let manager = get_download_manager().await.map_err(|e| e.to_string())?;
    let package = cache_release(&release, &cache, manager.as_ref(), reporter.clone()).await?;
    reporter.step(success_download_step("download", "官方包已缓存并校验"));
    planning::prepare(executable, graphics_api, package, Some(release))
}

async fn cache_release(
    release: &GraphicsComponentVersion,
    cache: &Path,
    manager: &dyn DownloadManager,
    reporter: InstallReporter,
) -> Result<PathBuf, String> {
    safe_path(cache)?;
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let lock_path = cache.join("lock");
    safe_path(&lock_path)?;
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| "其他进程正在更新此缓存，请稍后重试")?;
    let package = cache.join("setup.exe");
    let record_path = cache.join("source.json");
    let cached = match transaction::read_optional(&record_path)? {
        Some(bytes) => serde_json::from_slice::<CacheRecord>(&bytes)
            .ok()
            .is_some_and(|record| {
                record.schema_version == 1
                    && record.source_url == release.source_url
                    && file_hash(&package).ok().flatten().as_ref() == Some(&record.package_sha256)
            }),
        None => false,
    };
    if !cached {
        let staging = tempfile::tempdir_in(cache).map_err(|e| e.to_string())?;
        let options = DownloadOptions {
            save_dir: Some(staging.path().into()),
            filename: Some("setup.exe".into()),
            use_github_mirror: false,
            headers: [("Referer".into(), OFFICIAL_SITE.into())]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let progress = reporter.clone();
        manager
            .download_and_wait(
                &release.source_url,
                options,
                Box::new(move |p| {
                    progress.step(download_progress_step(
                        "download",
                        "下载 ReShade",
                        &p,
                        Some("reshade.me".into()),
                    ));
                }),
            )
            .await
            .map_err(|e| e.to_string())?;
        let staged = staging.path().join("setup.exe");
        // 验证真正的安装器 PE 和附加归档，不把 HTML 错误页或普通 ZIP 当作官网安装器。
        executable_architecture(&staged)?;
        planning::extract_dll(&staged)?;
        let bytes = transaction::read_optional(&staged)?.ok_or("下载结果缺失")?;
        let record = CacheRecord {
            schema_version: 1,
            package_sha256: transaction::hash(&bytes),
            source_url: release.source_url.clone(),
        };
        atomic_write(&package, &bytes)?;
        atomic_write(
            &record_path,
            &serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?,
        )?;
    }
    // 复用缓存时也验证 PE 和归档，不仅验证 sidecar 中的本地哈希。
    executable_architecture(&package)?;
    planning::extract_dll(&package)?;
    Ok(package)
}

#[cfg(test)]
#[path = "packages_tests.rs"]
mod tests;
