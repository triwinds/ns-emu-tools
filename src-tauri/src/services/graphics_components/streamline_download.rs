//! Fixed Release bundle: validated cache, cancellable download, and atomic publication.
use super::{
    runtime_package::{self, member},
    streamline_fg::file_digest,
    streamline_install as install,
};
use crate::services::downloader::{get_download_manager, DownloadManager, DownloadOptions};
use crate::services::installer::{download_progress_step, success_download_step, InstallReporter};
use crate::services::network::resolve_github_download_target;
use std::{
    fs,
    path::{Path, PathBuf},
};

static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub(super) fn cached_source() -> PathBuf {
    install::root()
        .join("packages")
        .join(install::package().version)
}

pub(super) fn available() -> bool {
    cfg!(all(windows, target_arch = "x86_64")) && super::streamline_update::candidate().online()
}

fn single_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['/', '\\', ':']) && !matches!(name, "." | "..")
}

pub(super) fn downloads(
    package: &install::Package,
) -> Result<Vec<(runtime_package::Asset, Vec<install::Artifact>)>, String> {
    if !single_name(&package.version) {
        return Err("组件版本无效".into());
    }
    let mut names = std::collections::HashMap::new();
    for file in &package.files {
        if !single_name(&file.name) || names.insert(file.name.to_ascii_lowercase(), file).is_some()
        {
            return Err("组件清单路径无效或重复".into());
        }
    }
    if package.parts.is_empty() {
        return Ok(vec![(
            package
                .download
                .clone()
                .ok_or("当前工具未配置在线组件包，请更新工具")?,
            package.files.clone(),
        )]);
    }
    if package.download.is_some() || package.parts.len() > 8 {
        return Err("组件分包清单无效".into());
    }
    let mut result = Vec::new();
    for part in &package.parts {
        if part.files.is_empty()
            || part.download.sha256.len() != 64
            || !part.download.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("组件分包校验清单无效".into());
        }
        let mut files = Vec::new();
        for name in &part.files {
            let file = names
                .remove(&name.to_ascii_lowercase())
                .ok_or("组件分包文件未知或重复")?;
            files.push(file.clone());
        }
        result.push((part.download.clone(), files));
    }
    if !names.is_empty() {
        return Err("组件分包清单缺少文件".into());
    }
    Ok(result)
}

fn valid_archive(path: &Path, hash: &str) -> Result<bool, String> {
    runtime_package::safe_path(path)?;
    if !path.exists() {
        return Ok(false);
    }
    install::plain(path, false)?;
    Ok(
        fs::metadata(path).map_err(|e| e.to_string())?.len() <= 256 * 1024 * 1024
            && file_digest(path)? == hash,
    )
}

fn unpack(archive: &Path, destination: &Path, files: &[install::Artifact]) -> Result<(), String> {
    for artifact in files {
        if artifact.name.is_empty()
            || artifact.name.contains(['/', '\\', ':'])
            || artifact.name == "."
            || artifact.name == ".."
        {
            return Err("组件清单路径无效".into());
        }
        let bytes = member(archive, &artifact.name, artifact.name.ends_with(".dll"))?;
        if runtime_package::hash(&bytes) != artifact.sha256 {
            return Err(format!("组件校验失败：{}", artifact.name));
        }
        fs::write(destination.join(&artifact.name), bytes).map_err(|e| e.to_string())?;
    }
    install::verify_artifacts(destination, files)
}

async fn download_at(
    root: &Path,
    package: &install::Package,
    reporter: InstallReporter,
    manager: &dyn DownloadManager,
) -> Result<PathBuf, String> {
    let downloads = downloads(package)?;
    install::safe_dir(root)?;
    let lock_path = root.join(".download.lock");
    runtime_package::safe_path(&lock_path)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| "组件正在下载，请稍后重试")?;
    let destination = root.join(&package.version);
    runtime_package::safe_path(&destination)?;
    if install::verify_artifacts(&destination, &package.files).is_ok() {
        reporter.step(success_download_step("download", "组件缓存已校验"));
        return Ok(destination);
    }
    let stage = tempfile::Builder::new()
        .prefix(".download-")
        .tempdir_in(root)
        .map_err(|e| e.to_string())?;
    let payload = stage.path().join("payload");
    fs::create_dir(&payload).map_err(|e| e.to_string())?;
    for (index, (artifact, files)) in downloads.iter().enumerate() {
        // Content-addressed ZIPs survive layer version changes. Legacy single
        // bundles keep their original repair/download behavior.
        let archive_cache = if package.parts.is_empty() {
            None
        } else {
            let cache = root.join(".archives");
            install::safe_dir(&cache)?;
            Some(cache.join(format!("{}.zip", artifact.sha256.to_ascii_lowercase())))
        };
        if let Some(cache) = archive_cache.as_ref() {
            if valid_archive(cache, &artifact.sha256)? {
                unpack(cache, &payload, files)?;
                continue;
            }
        }
        let filename = format!("package-{index}.zip");
        // Resolve once through the shared GitHub router so both download backends
        // and the source shown in progress use the exact same selected mirror.
        let target = resolve_github_download_target(&artifact.url);
        tracing::info!(url = %target.url, source = %target.source_name, archive = %artifact.name, "下载画面增强组件分包");
        reporter.step(
            crate::services::installer::running_download_step("download", "下载画面增强组件")
                .with_download_source(&target.source_name),
        );
        let download_source = target.source_name;
        let progress = reporter.clone();
        manager
            .download_and_wait(
                &target.url,
                DownloadOptions {
                    save_dir: Some(stage.path().into()),
                    filename: Some(filename.clone()),
                    use_github_mirror: false, // target.url already includes the chosen mirror.
                    ..Default::default()
                },
                Box::new(move |p| {
                    progress.step(download_progress_step(
                        "download",
                        "下载画面增强组件",
                        &p,
                        Some(download_source.clone()),
                    ))
                }),
            )
            .await
            .map_err(|e| e.to_string())?;
        let archive = stage.path().join(filename);
        if !valid_archive(&archive, &artifact.sha256)? {
            return Err("组件下载包 SHA-256 或大小不匹配，请重新下载".into());
        }
        unpack(&archive, &payload, files)?;
        if let Some(cache) = archive_cache {
            if cache.exists() {
                fs::rename(
                    &cache,
                    root.join(format!(".damaged-archive-{}", uuid::Uuid::new_v4())),
                )
                .map_err(|e| e.to_string())?;
            }
            fs::rename(&archive, cache).map_err(|e| e.to_string())?;
        }
    }
    install::verify_artifacts(&payload, &package.files)?;
    if destination.exists() {
        // Retain altered cache files for inspection instead of deleting unknown data.
        install::plain(&destination, true)?;
        fs::rename(
            &destination,
            root.join(format!(".damaged-{}", uuid::Uuid::new_v4())),
        )
        .map_err(|e| e.to_string())?;
    }
    fs::rename(payload, &destination).map_err(|e| e.to_string())?;
    reporter.step(success_download_step(
        "download",
        "画面增强组件已下载并校验",
    ));
    Ok(destination)
}

pub async fn ensure(reporter: InstallReporter) -> Result<PathBuf, String> {
    ensure_package(&super::streamline_update::candidate(), reporter).await
}
pub(super) async fn ensure_package(
    package: &install::Package,
    reporter: InstallReporter,
) -> Result<PathBuf, String> {
    if !cfg!(all(windows, target_arch = "x86_64")) {
        return Err("在线画面增强组件仅支持 Windows x64，请更新工具后重试".into());
    }
    let _guard = DOWNLOAD_LOCK.lock().await;
    // A verified cache does not need a network backend, even when offline.
    let source = if package.online() {
        install::root().join("packages").join(&package.version)
    } else {
        install::source()
    };
    if install::verify_artifacts(&source, &package.files).is_ok() {
        reporter.step(success_download_step("download", "组件缓存已校验"));
        return Ok(source);
    }
    let manager = get_download_manager().await.map_err(|e| e.to_string())?;
    download_at(
        &install::root().join("packages"),
        package,
        reporter,
        manager.as_ref(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::downloader::{DownloadProgress, DownloadResult, ProgressCallback};
    use std::io::{Cursor, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Download {
        bytes: Vec<u8>,
        payloads: std::collections::HashMap<String, Vec<u8>>,
        cancelled: bool,
        calls: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl DownloadManager for Download {
        async fn start(&self) -> crate::AppResult<()> {
            Ok(())
        }
        async fn stop(&self) -> crate::AppResult<()> {
            Ok(())
        }
        async fn download(&self, _: &str, _: DownloadOptions) -> crate::AppResult<String> {
            unreachable!()
        }
        async fn download_and_wait(
            &self,
            url: &str,
            options: DownloadOptions,
            _: ProgressCallback,
        ) -> crate::AppResult<DownloadResult> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            assert!(!options.use_github_mirror);
            let path = options.save_dir.unwrap().join(options.filename.unwrap());
            let bytes = self
                .payloads
                .get(url)
                .or_else(|| {
                    self.payloads
                        .iter()
                        .find(|(original, _)| original.rsplit('/').next() == url.rsplit('/').next())
                        .map(|(_, bytes)| bytes)
                })
                .unwrap_or(&self.bytes);
            fs::write(&path, bytes).unwrap();
            if self.cancelled {
                return Err(crate::AppError::Download("cancelled".into()));
            }
            Ok(DownloadResult {
                path,
                filename: "package.zip".into(),
                size: bytes.len() as u64,
                gid: "fixture".into(),
            })
        }
        async fn pause(&self, _: &str) -> crate::AppResult<()> {
            unreachable!()
        }
        async fn resume(&self, _: &str) -> crate::AppResult<()> {
            unreachable!()
        }
        async fn cancel(&self, _: &str) -> crate::AppResult<()> {
            unreachable!()
        }
        async fn cancel_all(&self, _: bool) -> crate::AppResult<Option<String>> {
            unreachable!()
        }
        async fn get_download_progress(&self, _: &str) -> crate::AppResult<DownloadProgress> {
            unreachable!()
        }
        fn is_started(&self) -> bool {
            true
        }
    }
    fn fixture(names: &[&str]) -> (install::Package, Download) {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in names {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"verified payload").unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let package = serde_json::from_value(serde_json::json!({
            "version":"test-v1", "files":[{"name":"runtime.txt", "sha256":runtime_package::hash(b"verified payload")}],
            "download":{"name":"test.zip", "url":"https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/test-v1/test.zip", "sha256":runtime_package::hash(&bytes)}
        })).unwrap();
        (
            package,
            Download {
                bytes,
                payloads: Default::default(),
                cancelled: false,
                calls: AtomicUsize::new(0),
            },
        )
    }
    #[tokio::test]
    async fn split_updates_reuse_runtime_and_cancel_without_publishing_partial_package() {
        fn split(
            layer: &[u8],
            version: &str,
        ) -> (install::Package, std::collections::HashMap<String, Vec<u8>>) {
            let mut files = Vec::new();
            let mut parts = Vec::new();
            let mut payloads = std::collections::HashMap::new();
            for (name, data) in [
                ("runtime.txt", &b"stable runtime"[..]),
                ("layer.txt", layer),
            ] {
                let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
                zip.start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(data).unwrap();
                let bytes = zip.finish().unwrap().into_inner();
                let hash = runtime_package::hash(&bytes);
                let url = format!("https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/test/{hash}.zip");
                files.push(serde_json::json!({"name":name,"sha256":runtime_package::hash(data)}));
                parts.push(serde_json::json!({"download":{"name":format!("{hash}.zip"),"url":url,"sha256":hash},"files":[name]}));
                payloads.insert(url, bytes);
            }
            (
                serde_json::from_value(
                    serde_json::json!({"version":version,"files":files,"parts":parts}),
                )
                .unwrap(),
                payloads,
            )
        }
        let dir = tempfile::tempdir().unwrap();
        let (package, payloads) = split(b"layer 1", "split-1");
        let mut manager = Download {
            bytes: Vec::new(),
            payloads,
            cancelled: false,
            calls: AtomicUsize::new(0),
        };
        let reporter = InstallReporter::new(|_| {});
        let first = download_at(dir.path(), &package, reporter.clone(), &manager)
            .await
            .unwrap();
        assert_eq!(manager.calls.load(Ordering::Relaxed), 2);
        let (next, payloads) = split(b"layer 2", "split-2");
        manager.payloads = payloads;
        manager.cancelled = true;
        assert!(download_at(dir.path(), &next, reporter.clone(), &manager)
            .await
            .is_err());
        assert!(!dir.path().join("split-2").exists());
        assert_eq!(fs::read(first.join("layer.txt")).unwrap(), b"layer 1");
        manager.cancelled = false;
        let second = download_at(dir.path(), &next, reporter.clone(), &manager)
            .await
            .unwrap();
        assert_eq!(manager.calls.load(Ordering::Relaxed), 4); // Only layer fetched on cancel and retry.
        assert_eq!(
            fs::read(second.join("runtime.txt")).unwrap(),
            b"stable runtime"
        );
        assert_eq!(fs::read(second.join("layer.txt")).unwrap(), b"layer 2");
        fs::write(second.join("runtime.txt"), b"damaged extracted file").unwrap();
        download_at(dir.path(), &next, reporter.clone(), &manager)
            .await
            .unwrap();
        assert_eq!(manager.calls.load(Ordering::Relaxed), 4); // Both ZIPs reused for repair.
        fs::write(
            dir.path()
                .join(".archives")
                .join(format!("{}.zip", next.parts[0].download.sha256)),
            b"damaged zip",
        )
        .unwrap();
        fs::write(second.join("runtime.txt"), b"damaged again").unwrap();
        download_at(dir.path(), &next, reporter, &manager)
            .await
            .unwrap();
        assert_eq!(manager.calls.load(Ordering::Relaxed), 5); // Redownload only damaged runtime ZIP.
    }

    #[test]
    fn split_manifest_requires_exact_disjoint_membership_and_safe_identifiers() {
        let (legacy, _) = fixture(&["runtime.txt"]);
        let asset = legacy.download.as_ref().unwrap().clone();
        for parts in [
            serde_json::json!([{"download":asset,"files":[]}]),
            serde_json::json!([{"download":asset,"files":["missing.txt"]}]),
            serde_json::json!([{"download":asset,"files":["runtime.txt"]},{"download":asset,"files":["RUNTIME.TXT"]}]),
        ] {
            let package = serde_json::from_value(serde_json::json!({"version":"split","files":[{"name":"runtime.txt","sha256":"unused"}],"parts":parts})).unwrap();
            assert!(downloads(&package).is_err());
        }
        let mut package = legacy;
        package.version = "../escape".into();
        assert!(downloads(&package).is_err());
    }

    #[tokio::test]
    async fn cache_reuse_repair_and_cancellation_preserve_published_files() {
        let dir = tempfile::tempdir().unwrap();
        let (package, mut manager) = fixture(&["runtime.txt"]);
        let reporter = InstallReporter::new(|_| {});
        let cache = download_at(dir.path(), &package, reporter.clone(), &manager)
            .await
            .unwrap();
        download_at(dir.path(), &package, reporter.clone(), &manager)
            .await
            .unwrap();
        assert_eq!(manager.calls.load(Ordering::Relaxed), 1);
        fs::write(cache.join("runtime.txt"), b"altered").unwrap();
        fs::write(cache.join("user.txt"), b"keep").unwrap();
        manager.cancelled = true;
        assert!(
            download_at(dir.path(), &package, reporter.clone(), &manager)
                .await
                .is_err()
        );
        assert_eq!(fs::read(cache.join("runtime.txt")).unwrap(), b"altered");
        assert_eq!(fs::read(cache.join("user.txt")).unwrap(), b"keep");
        manager.cancelled = false;
        download_at(dir.path(), &package, reporter, &manager)
            .await
            .unwrap();
        assert_eq!(
            fs::read(cache.join("runtime.txt")).unwrap(),
            b"verified payload"
        );
        let preserved = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".damaged-")
            })
            .unwrap();
        assert_eq!(fs::read(preserved.join("user.txt")).unwrap(), b"keep");
    }
    #[tokio::test]
    async fn tampered_archive_payload_and_unsafe_members_never_publish() {
        for names in [
            vec!["runtime.txt"],
            vec!["../escape", "runtime.txt"],
            vec!["runtime.txt", "RUNTIME.TXT"],
        ] {
            let dir = tempfile::tempdir().unwrap();
            let (mut package, manager) = fixture(&names);
            if names.len() == 1 {
                package.files[0].sha256 = runtime_package::hash(b"different");
            }
            assert!(
                download_at(dir.path(), &package, InstallReporter::new(|_| {}), &manager)
                    .await
                    .is_err()
            );
            assert!(!dir.path().join("test-v1").exists());
            assert!(!dir.path().join("escape").exists());
        }
        let dir = tempfile::tempdir().unwrap();
        let (package, mut manager) = fixture(&["runtime.txt"]);
        manager.bytes = b"damaged archive".to_vec();
        assert!(
            download_at(dir.path(), &package, InstallReporter::new(|_| {}), &manager)
                .await
                .is_err()
        );
        assert!(!dir.path().join("test-v1").exists());
    }
    #[tokio::test]
    #[ignore = "Reads FG_RUNTIME_ARCHIVE or downloads the pinned Release into a temporary cache"]
    async fn pinned_release_download_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let package = if let Some(manifest) = std::env::var_os("FG_RUNTIME_MANIFEST") {
            serde_json::from_slice::<install::Package>(&fs::read(manifest).unwrap()).unwrap()
        } else {
            install::package()
        };
        let cache = if let Some(archive) = std::env::var_os("FG_RUNTIME_ARCHIVE") {
            let archive = PathBuf::from(archive);
            let (bytes, payloads) = if archive.is_dir() {
                let payloads = downloads(&package)
                    .unwrap()
                    .into_iter()
                    .map(|(asset, _)| (asset.url, fs::read(archive.join(asset.name)).unwrap()))
                    .collect();
                (Vec::new(), payloads)
            } else {
                (fs::read(archive).unwrap(), Default::default())
            };
            let manager = Download {
                bytes,
                payloads,
                cancelled: false,
                calls: AtomicUsize::new(0),
            };
            download_at(
                temp.path(),
                &package,
                InstallReporter::new(|_| {}),
                &manager,
            )
            .await
            .unwrap()
        } else {
            let manager = get_download_manager().await.unwrap();
            download_at(
                temp.path(),
                &package,
                InstallReporter::new(|_| {}),
                manager.as_ref(),
            )
            .await
            .unwrap()
        };
        install::verify_artifacts(&cache, &package.files).unwrap();
        let exe = temp.path().join("test.exe");
        fs::write(&exe, b"unchanged emulator").unwrap();
        let base = temp.path().join("managed");
        let expected = file_digest(&exe).unwrap();
        install::install_release_at(&base, &cache, &exe, &expected, &package).unwrap();
        install::uninstall_at(&base, &exe).unwrap();
        assert_eq!(
            file_digest(&exe).unwrap(),
            runtime_package::hash(b"unchanged emulator")
        );
    }
    #[tokio::test]
    #[ignore = "Downloads the published small ZIP through the configured unified backend and mirror"]
    async fn pinned_layer_mirror_download() {
        let config: crate::config::Config = serde_json::from_slice(
            &fs::read(std::env::var_os("FG_DOWNLOAD_CONFIG").expect("FG_DOWNLOAD_CONFIG")).unwrap(),
        )
        .unwrap();
        {
            let mut current = crate::config::CONFIG.write();
            current.setting.network = config.setting.network;
            current.setting.download = config.setting.download;
        }
        let mut package: install::Package = serde_json::from_slice(
            &fs::read(std::env::var_os("FG_RUNTIME_MANIFEST").expect("FG_RUNTIME_MANIFEST"))
                .unwrap(),
        )
        .unwrap();
        package
            .parts
            .retain(|part| part.files.iter().any(|f| f == "streamline-layer-probe.exe"));
        assert_eq!(package.parts.len(), 1);
        package
            .files
            .retain(|f| package.parts[0].files.contains(&f.name));
        let expected = resolve_github_download_target(&package.parts[0].download.url);
        println!(
            "Download mirror: {}; {}",
            expected.source_name, expected.url
        );
        let sources = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = sources.clone();
        let reporter = InstallReporter::new(move |event| {
            if let crate::models::progress::ProgressEvent::StepUpdate { step } = event {
                if let Some(source) = step.download_source {
                    captured.lock().unwrap().push(source);
                }
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let manager = get_download_manager().await.unwrap();
        let started = std::time::Instant::now();
        let cache = tokio::time::timeout(
            std::time::Duration::from_secs(45),
            download_at(dir.path(), &package, reporter, manager.as_ref()),
        )
        .await
        .unwrap()
        .unwrap();
        install::verify_artifacts(&cache, &package.files).unwrap();
        assert!(sources
            .lock()
            .unwrap()
            .iter()
            .any(|source| source == &expected.source_name));
        println!(
            "Small ZIP downloaded and SHA-256 verified in {:?}",
            started.elapsed()
        );
        manager.stop().await.unwrap();
    }
}
