//! Pinned XeSS Release, isolated cache, shared downloader and atomic publication.
use super::*;
use crate::services::{
    downloader::{get_download_manager, DownloadManager, DownloadOptions},
    installer::{
        download_progress_step, running_download_step, success_download_step, InstallReporter,
    },
    network::resolve_github_download_target,
};

static DOWNLOAD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Release {
    version: String,
    download: runtime_package::Asset,
}

pub(super) fn release(p: &Package) -> Result<Release, String> {
    let release: Release =
        serde_json::from_str(include_str!("xess-release.json")).map_err(|e| e.to_string())?;
    validate_release(p, &release)?;
    Ok(release)
}

fn validate_release(p: &Package, r: &Release) -> Result<(), String> {
    manifest_valid(p)?;
    let tag = format!(
        "xess-{}-v1",
        p.version.strip_prefix("local-xess-").unwrap_or(&p.version)
    );
    let name = format!("{tag}.zip");
    let url =
        format!("https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/{tag}/{name}");
    if r.version != p.version
        || r.download.name != name
        || r.download.url != url
        || r.download.sha256.len() != 64
        || !r.download.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("XeSS 在线组件清单与主程序不匹配，请更新工具".into());
    }
    Ok(())
}

fn unpack(archive: &Path, payload: &Path, p: &Package) -> Result<(), String> {
    // Read only the allowlisted nested members; never extract archive paths.
    let actual: Package =
        serde_json::from_slice(&runtime_package::member(archive, "package.json", false)?)
            .map_err(|e| e.to_string())?;
    if &actual != p {
        return Err("XeSS 下载包清单与主程序不匹配".into());
    }
    for f in &p.files {
        let bytes = runtime_package::member(archive, &f.name, f.name.ends_with(".dll"))?;
        if runtime_package::hash(&bytes) != f.sha256 {
            return Err(format!("XeSS 下载文件未通过校验：{}", f.name));
        }
        let out = payload.join(&f.name);
        safe_dir(out.parent().ok_or("XeSS 文件路径无效")?)?;
        fs::write(out, bytes).map_err(|e| e.to_string())?;
    }
    atomic_json(
        &payload.join("package.json"),
        &serde_json::to_value(p).map_err(|e| e.to_string())?,
    )?;
    drop(verify(payload, p)?);
    Ok(())
}

async fn download_at(
    packages: &Path,
    p: &Package,
    release: &Release,
    reporter: InstallReporter,
    manager: &dyn DownloadManager,
) -> Result<PathBuf, String> {
    validate_release(p, release)?;
    let _store = lock(packages)?;
    let destination = packages.join(&p.version);
    runtime_package::safe_path(&destination)?;
    if verify(&destination, p).is_ok() {
        reporter.step(success_download_step("download", "XeSS 组件缓存已校验"));
        return Ok(destination);
    }
    let stage = tempfile::Builder::new()
        .prefix(".download-")
        .tempdir_in(packages)
        .map_err(|e| e.to_string())?;
    let target = resolve_github_download_target(&release.download.url);
    reporter.step(
        running_download_step("download", "下载 XeSS 组件")
            .with_download_source(&target.source_name),
    );
    let progress = reporter.clone();
    let source = target.source_name;
    manager
        .download_and_wait(
            &target.url,
            DownloadOptions {
                save_dir: Some(stage.path().into()),
                filename: Some("package.zip".into()),
                use_github_mirror: false,
                ..Default::default()
            },
            Box::new(move |p| {
                progress.step(download_progress_step(
                    "download",
                    "下载 XeSS 组件",
                    &p,
                    Some(source.clone()),
                ))
            }),
        )
        .await
        .map_err(|e| e.to_string())?;
    let archive = stage.path().join("package.zip");
    runtime_package::safe_path(&archive)?;
    plain(&archive, false)?;
    if fs::metadata(&archive).map_err(|e| e.to_string())?.len() > 256 * 1024 * 1024
        || file_digest(&archive)? != release.download.sha256
    {
        return Err("XeSS 下载包 SHA-256 或大小不匹配，请重试".into());
    }
    let payload = stage.path().join("payload");
    safe_dir(&payload)?;
    unpack(&archive, &payload, p)?;
    if destination.exists() {
        // Keep changed/unknown files for inspection, and only replace after verification.
        plain(&destination, true)?;
        fs::rename(
            &destination,
            packages.join(format!(".damaged-{}", uuid::Uuid::new_v4())),
        )
        .map_err(|e| e.to_string())?;
    }
    fs::rename(payload, &destination).map_err(|e| e.to_string())?;
    reporter.step(success_download_step("download", "XeSS 组件已下载并校验"));
    Ok(destination)
}

pub(super) async fn ensure(reporter: InstallReporter) -> Result<PathBuf, String> {
    let _guard = DOWNLOAD.lock().await;
    let p = package();
    let packages = root().join("packages");
    let cache = packages.join(&p.version);
    let local = tauri::async_runtime::spawn_blocking(move || {
        [Ok(destination(&root(), &package())), bundled(), Ok(cache)]
            .into_iter()
            .filter_map(Result::ok)
            .find(|dir| verify(dir, &package()).is_ok())
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Some(local) = local {
        reporter.step(success_download_step("download", "本地 XeSS 组件已校验"));
        return Ok(local);
    }
    let release = release(&p)?;
    let manager = get_download_manager().await.map_err(|e| e.to_string())?;
    download_at(&packages, &p, &release, reporter, manager.as_ref()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::downloader::{DownloadProgress, DownloadResult, ProgressCallback};
    use std::{
        io::{Cursor, Write},
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct Download {
        bytes: Vec<u8>,
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
            _: &str,
            options: DownloadOptions,
            _: ProgressCallback,
        ) -> crate::AppResult<DownloadResult> {
            assert!(!options.use_github_mirror);
            self.calls.fetch_add(1, Ordering::Relaxed);
            let path = options.save_dir.unwrap().join(options.filename.unwrap());
            fs::write(&path, &self.bytes).unwrap();
            if self.cancelled {
                return Err(crate::AppError::Download("cancelled".into()));
            }
            Ok(DownloadResult {
                path,
                filename: "package.zip".into(),
                size: self.bytes.len() as u64,
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

    fn fixture(extra: Option<&str>) -> (Package, Release, Download) {
        let mut dll = vec![0u8; 256];
        dll[..2].copy_from_slice(b"MZ");
        dll[60..64].copy_from_slice(&64u32.to_le_bytes());
        dll[64..68].copy_from_slice(b"PE\0\0");
        dll[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
        dll[70..72].copy_from_slice(&1u16.to_le_bytes());
        dll[84..86].copy_from_slice(&112u16.to_le_bytes());
        dll[86..88].copy_from_slice(&0x2002u16.to_le_bytes());
        dll[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
        let mut p = Package {
            schema_version: 1,
            version: "test-v1".into(),
            files: vec![],
        };
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in MEMBERS {
            let data = if name.ends_with(".dll") {
                &dll[..]
            } else {
                name.as_bytes()
            };
            p.files.push(Artifact {
                name: (*name).into(),
                sha256: runtime_package::hash(data),
            });
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data).unwrap();
        }
        zip.start_file("package.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&p).unwrap()).unwrap();
        if let Some(name) = extra {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"unsafe").unwrap();
        }
        let bytes = zip.finish().unwrap().into_inner();
        let r = Release { version: p.version.clone(), download: runtime_package::Asset {
            name: "xess-test-v1-v1.zip".into(),
            url: "https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/xess-test-v1-v1/xess-test-v1-v1.zip".into(),
            sha256: runtime_package::hash(&bytes),
        }};
        (
            p,
            r,
            Download {
                bytes,
                cancelled: false,
                calls: AtomicUsize::new(0),
            },
        )
    }

    #[tokio::test]
    async fn nested_download_cache_repair_cancel_install_and_uninstall() {
        let temp = tempfile::tempdir().unwrap();
        let packages = temp.path().join("packages");
        let (p, r, mut manager) = fixture(None);
        let reporter = InstallReporter::new(|_| {});
        let source = download_at(&packages, &p, &r, reporter.clone(), &manager)
            .await
            .unwrap();
        download_at(&packages, &p, &r, reporter.clone(), &manager)
            .await
            .unwrap();
        assert_eq!(manager.calls.load(Ordering::Relaxed), 1);
        let base = temp.path().join("installed");
        install_at(&base, &source, &p).unwrap();
        fs::write(source.join("user.txt"), b"retain").unwrap();
        manager.cancelled = true;
        assert!(download_at(&packages, &p, &r, reporter.clone(), &manager)
            .await
            .is_err());
        assert_eq!(fs::read(source.join("user.txt")).unwrap(), b"retain");
        assert!(verify(&destination(&base, &p), &p).is_ok());
        manager.cancelled = false;
        download_at(&packages, &p, &r, reporter, &manager)
            .await
            .unwrap();
        let damaged = fs::read_dir(&packages)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".damaged-")
            })
            .unwrap();
        assert_eq!(fs::read(damaged.join("user.txt")).unwrap(), b"retain");
        assert!(verify(&source, &p).is_ok());
        remove_at(&base, &p).unwrap();
        assert!(!destination(&base, &p).exists());
    }

    #[tokio::test]
    async fn unsafe_corrupt_and_mismatched_releases_never_publish() {
        for extra in [Some("../escape"), Some("LIBXESS.DLL"), None] {
            let temp = tempfile::tempdir().unwrap();
            let (mut p, r, manager) = fixture(extra);
            if extra.is_none() {
                p.files[0].sha256 = "0".repeat(64);
            }
            assert!(
                download_at(temp.path(), &p, &r, InstallReporter::new(|_| {}), &manager)
                    .await
                    .is_err()
            );
            assert!(!temp.path().join(&p.version).exists());
        }
        let temp = tempfile::tempdir().unwrap();
        let (p, mut r, manager) = fixture(None);
        r.download.sha256 = "0".repeat(64);
        assert!(
            download_at(temp.path(), &p, &r, InstallReporter::new(|_| {}), &manager)
                .await
                .is_err()
        );
        assert!(!temp.path().join(&p.version).exists());
        r.download.url = "https://example.com/untrusted.zip".into();
        assert!(validate_release(&p, &r).is_err());
        r.version = "../escape".into();
        assert!(validate_release(&p, &r).is_err());
    }

    #[tokio::test]
    #[ignore = "Downloads the published XeSS Release, or uses XESS_RELEASE_ARCHIVE, in an isolated store"]
    async fn published_release_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let p = package();
        let r = release(&p).unwrap();
        let manager: Box<dyn DownloadManager> =
            if let Some(path) = std::env::var_os("XESS_RELEASE_ARCHIVE") {
                Box::new(Download {
                    bytes: fs::read(path).unwrap(),
                    cancelled: false,
                    calls: AtomicUsize::new(0),
                })
            } else {
                Box::new(crate::services::downloader::BytehaulBackend::from_config().unwrap())
            };
        manager.start().await.unwrap();
        let source = download_at(
            &temp.path().join("packages"),
            &p,
            &r,
            InstallReporter::new(|_| {}),
            manager.as_ref(),
        )
        .await
        .unwrap();
        let base = temp.path().join("installed");
        install_at(&base, &source, &p).unwrap();
        assert!(verify(&destination(&base, &p), &p).is_ok());
        remove_at(&base, &p).unwrap();
        manager.stop().await.unwrap();
    }
}
