use super::*;
use crate::services::downloader::{BytehaulBackend, DownloadManager};

/// 显式联网验收；所有下载和部署均在临时目录，不启动模拟器或安装器。
#[tokio::test]
#[ignore = "需要访问 reshade.me 并下载当前官方安装包"]
async fn official_package_download_and_graphics_roundtrip() {
    let release = latest().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let manager = BytehaulBackend::from_config().unwrap();
    manager.start().await.unwrap();
    let cache = dir.path().join("cache");
    let package = cache_release(&release, &cache, &manager, InstallReporter::new(|_| {}))
        .await
        .unwrap();
    manager.stop().await.unwrap();
    let dll = planning::extract_dll(&package).unwrap();
    let exe = dir.path().join("emu.exe");
    fs::write(&exe, super::super::tests::pe(0x8664, 0x20b)).unwrap();
    vulkan::tests::official_roundtrip(&package);
    let plan = planning::prepare(exe.clone(), GraphicsApi::OpenGl, package, Some(release)).unwrap();
    println!(
        "official version={:?}, DLL bytes={}, DLL sha256={}, package sha256={}",
        plan.version,
        dll.len(),
        plan.dll_sha256,
        plan.package_sha256
    );
    planning::install(plan.plan_id.unwrap(), false).unwrap();
    assert_eq!(fs::read(dir.path().join("opengl32.dll")).unwrap(), dll);
    assert_eq!(
        detect(exe.clone(), Some(GraphicsApi::OpenGl)).reshade_state,
        GraphicsComponentState::Installed
    );
    transaction::remove_or_repair(exe, false).unwrap();
    assert!(!dir.path().join("opengl32.dll").exists());
}

struct FakeDownload {
    body: Vec<u8>,
    fail: bool,
    calls: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl DownloadManager for FakeDownload {
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
        _: crate::services::downloader::ProgressCallback,
    ) -> crate::AppResult<crate::services::downloader::DownloadResult> {
        self.calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(
            options.headers.get("Referer").map(String::as_str),
            Some(OFFICIAL_SITE)
        );
        assert!(!options.use_github_mirror);
        let path = options.save_dir.unwrap().join(options.filename.unwrap());
        fs::write(&path, &self.body).unwrap();
        if self.fail {
            return Err(crate::AppError::Download("cancelled".into()));
        }
        Ok(crate::services::downloader::DownloadResult {
            path,
            filename: "setup.exe".into(),
            size: self.body.len() as u64,
            gid: "fake".into(),
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
    async fn get_download_progress(
        &self,
        _: &str,
    ) -> crate::AppResult<crate::services::downloader::DownloadProgress> {
        unreachable!()
    }
    fn is_started(&self) -> bool {
        true
    }
}

fn setup_bytes() -> Vec<u8> {
    use std::io::{Cursor, Write};
    let mut bytes = super::super::tests::pe(0x8664, 0x20b);
    let mut dll = bytes.clone();
    dll[86..88].copy_from_slice(&0x2002u16.to_le_bytes());
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file("ReShade64.dll", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&dll).unwrap();
    bytes.extend(zip.finish().unwrap().into_inner());
    bytes
}

#[tokio::test]
async fn cache_reuse_corruption_and_cancel_do_not_publish_bad_packages() {
    let dir = tempfile::tempdir().unwrap();
    let release = GraphicsComponentVersion {
        version: "1.0.0".into(),
        source_url: "https://reshade.me/downloads/ReShade_Setup_1.0.0_Addon.exe".into(),
        channel: "stable-addon".into(),
    };
    let manager = FakeDownload {
        body: setup_bytes(),
        fail: false,
        calls: Default::default(),
    };
    let reporter = InstallReporter::new(|_| {});
    let cache = dir.path().join("good");
    let package = cache_release(&release, &cache, &manager, reporter.clone())
        .await
        .unwrap();
    cache_release(&release, &cache, &manager, reporter.clone())
        .await
        .unwrap();
    assert_eq!(manager.calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    fs::write(&package, b"corrupt").unwrap();
    cache_release(&release, &cache, &manager, reporter.clone())
        .await
        .unwrap();
    assert_eq!(manager.calls.load(std::sync::atomic::Ordering::Relaxed), 2);
    assert_eq!(fs::read(&package).unwrap(), manager.body);
    for (name, body, fail) in [
        ("cancelled", setup_bytes(), true),
        ("html", b"<html>error</html>".to_vec(), false),
    ] {
        let cache = dir.path().join(name);
        let manager = FakeDownload {
            body,
            fail,
            calls: Default::default(),
        };
        assert!(cache_release(&release, &cache, &manager, reporter.clone())
            .await
            .is_err());
        assert!(!cache.join("setup.exe").exists());
        assert!(!cache.join("source.json").exists());
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 1); // only cache lock survives
    }
}
