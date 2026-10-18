//! Three immutable packages: shared programs, NVIDIA dependencies, XeSS dependencies.
//! Engines keep their existing settings/session stores and reference one shared program version.
use super::{runtime_package, streamline_fg::file_digest, streamline_install as install};
use crate::{
    config::effective_config_dir,
    services::{
        downloader::{get_download_manager, DownloadManager, DownloadOptions},
        installer::{
            download_progress_step, running_download_step, success_download_step, InstallReporter,
        },
        network::resolve_github_download_target,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

static DOWNLOAD: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
const RELEASES: &str = "https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/";
const PROGRAMS: &[&str] = &[
    "graphics-launcher.exe",
    "streamline_probe_layer.dll",
    "xess-layer.dll",
    "nvngx.dll",
    "LICENSE.txt",
];

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Part {
    pub version: String,
    pub files: Vec<install::Artifact>,
    pub download: runtime_package::Asset,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Distribution {
    schema_version: u32,
    common: Part,
    nvidia: Part,
    xess: Part,
}

fn manifest() -> Result<Distribution, String> {
    let p: Distribution = serde_json::from_str(include_str!("graphics-distribution.json"))
        .map_err(|e| e.to_string())?;
    if p.schema_version != 2 {
        return Err("图形组件分包协议不兼容，请更新工具".into());
    }
    for (kind, part) in [
        ("common", &p.common),
        ("nvidia", &p.nvidia),
        ("xess", &p.xess),
    ] {
        validate_part(kind, part)?;
    }
    Ok(p)
}

fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}

fn relative(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(['\\', ':'])
        && name
            .split('/')
            .all(|v| !v.is_empty() && !matches!(v, "." | ".."))
}

fn validate_part(kind: &str, p: &Part) -> Result<(), String> {
    let prefix = format!("graphics-{kind}-");
    let suffix = p.version.strip_prefix(&prefix).ok_or("图形分包版本无效")?;
    if suffix.len() != 16
        || !suffix.bytes().all(|v| v.is_ascii_hexdigit())
        || p.download.name != format!("{}.zip", p.version)
        || p.download.url != format!("{RELEASES}{}/{}", p.version, p.download.name)
        || !digest(&p.download.sha256)
        || p.files.is_empty()
        || p.files.len() > 64
    {
        return Err("图形分包下载清单无效".into());
    }
    let mut names = BTreeSet::new();
    for f in &p.files {
        if !relative(&f.name) || !digest(&f.sha256) || !names.insert(f.name.to_ascii_lowercase()) {
            return Err("图形分包成员路径或校验值无效".into());
        }
        if kind != "common"
            && (f.name.ends_with(".exe")
                || PROGRAMS.contains(&f.name.as_str()) && f.name != "LICENSE.txt")
        {
            return Err("运行库分包不能包含公共程序".into());
        }
    }
    if kind == "common" && names != PROGRAMS.iter().map(|v| v.to_ascii_lowercase()).collect() {
        return Err("公共程序分包成员不完整".into());
    }
    Ok(())
}

pub(super) fn part(kind: &str) -> Result<Part, String> {
    let p = manifest()?;
    match kind {
        "common" => Ok(p.common),
        "nvidia" => Ok(p.nvidia),
        "xess" => Ok(p.xess),
        _ => Err("未知图形组件分包".into()),
    }
}

fn root() -> PathBuf {
    effective_config_dir().join("graphics/components")
}

pub(super) fn bundled(kind: &str) -> Result<PathBuf, String> {
    let base = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/release")
    } else {
        std::env::current_exe()
            .map_err(|e| e.to_string())?
            .parent()
            .ok_or("主程序目录无效")?
            .to_path_buf()
    };
    Ok(base.join("graphics-components").join(kind))
}

fn verify(dir: &Path, p: &Part) -> Result<Vec<fs::File>, String> {
    runtime_package::safe_path(dir)?;
    install::plain(dir, true)?;
    let paths: Vec<_> = p.files.iter().map(|f| dir.join(&f.name)).collect();
    let handles = install::hold_runtime_files(&paths, false)?;
    let mut allowed = BTreeSet::from([dir.to_path_buf()]);
    for path in &paths {
        for parent in path.ancestors().take_while(|v| v.starts_with(dir)) {
            allowed.insert(parent.to_path_buf());
        }
    }
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|e| e.to_string())?;
        runtime_package::safe_path(entry.path())?;
        if !allowed.contains(entry.path()) {
            return Err("图形分包目录含有额外文件".into());
        }
    }
    for f in &p.files {
        if file_digest(&dir.join(&f.name))? != f.sha256 {
            return Err(format!("图形分包文件校验失败：{}", f.name));
        }
    }
    Ok(handles)
}

pub(super) fn programs(version: &str) -> Result<(PathBuf, Vec<fs::File>), String> {
    let p = part("common")?;
    if p.version != version {
        return Err("公共程序版本与运行库不匹配，请重新安装组件".into());
    }
    let dir = root().join(&p.version);
    verify(&dir, &p).map(|files| (dir, files))
}

pub(super) fn common_version() -> Result<String, String> {
    Ok(part("common")?.version)
}

pub(super) fn available(kind: &str) -> bool {
    part(kind).is_ok()
}

pub(super) fn local_ready(kind: &str) -> bool {
    ["common", kind].into_iter().all(|k| {
        part(k).is_ok_and(|p| {
            verify(&root().join(&p.version), &p).is_ok()
                || bundled(k).is_ok_and(|dir| verify(&dir, &p).is_ok())
        })
    })
}

fn copy(source: &Path, destination: &Path, p: &Part) -> Result<(), String> {
    let _source = verify(source, p)?;
    for f in &p.files {
        let out = destination.join(&f.name);
        install::safe_dir(out.parent().ok_or("分包路径无效")?)?;
        fs::copy(source.join(&f.name), out).map_err(|e| e.to_string())?;
    }
    drop(verify(destination, p)?);
    Ok(())
}

fn unpack(archive: &Path, destination: &Path, p: &Part) -> Result<(), String> {
    for f in &p.files {
        let bytes = runtime_package::member(archive, &f.name, f.name.ends_with(".dll"))?;
        if runtime_package::hash(&bytes) != f.sha256 {
            return Err(format!("分包成员校验失败：{}", f.name));
        }
        let out = destination.join(&f.name);
        install::safe_dir(out.parent().ok_or("分包路径无效")?)?;
        fs::write(out, bytes).map_err(|e| e.to_string())?;
    }
    drop(verify(destination, p)?);
    Ok(())
}

fn archive_valid(path: &Path, p: &Part) -> Result<bool, String> {
    runtime_package::safe_path(path)?;
    if !path.exists() {
        return Ok(false);
    }
    install::plain(path, false)?;
    Ok(
        fs::metadata(path).map_err(|e| e.to_string())?.len() <= 256 * 1024 * 1024
            && file_digest(path)? == p.download.sha256,
    )
}

async fn ensure_at(
    base: &Path,
    local: Option<&Path>,
    p: &Part,
    reporter: &InstallReporter,
    manager: Option<&dyn DownloadManager>,
) -> Result<PathBuf, String> {
    install::safe_dir(base)?;
    let lock_path = base.join(".operation.lock");
    runtime_package::safe_path(&lock_path)?;
    if lock_path.exists() {
        install::plain(&lock_path, false)?;
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| "另一个图形分包安装正在进行")?;
    let dir = base.join(&p.version);
    runtime_package::safe_path(&dir)?;
    if verify(&dir, p).is_ok() {
        return Ok(dir);
    }
    let stage = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(base)
        .map_err(|e| e.to_string())?;
    let payload = stage.path().join("payload");
    install::safe_dir(&payload)?;
    if let Some(local) = local.filter(|d| verify(d, p).is_ok()) {
        copy(local, &payload, p)?;
    } else {
        let archives = base.join(".archives");
        install::safe_dir(&archives)?;
        let cache = archives.join(format!("{}.zip", p.download.sha256));
        if archive_valid(&cache, p)? {
            unpack(&cache, &payload, p)?;
        } else {
            let manager = manager.ok_or("图形分包缺少可用本地文件，需要下载")?;
            let target = resolve_github_download_target(&p.download.url);
            let label = format!("下载 {}", p.version);
            reporter.step(
                running_download_step("download", &label).with_download_source(&target.source_name),
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
                            &label,
                            &p,
                            Some(source.clone()),
                        ))
                    }),
                )
                .await
                .map_err(|e| e.to_string())?;
            let archive = stage.path().join("package.zip");
            if !archive_valid(&archive, p)? {
                return Err("图形分包 ZIP 校验失败，请重试".into());
            }
            unpack(&archive, &payload, p)?;
            if cache.exists() {
                fs::rename(
                    &cache,
                    base.join(format!(".damaged-archive-{}", uuid::Uuid::new_v4())),
                )
                .map_err(|e| e.to_string())?;
            }
            fs::rename(archive, cache).map_err(|e| e.to_string())?;
        }
    }
    if dir.exists() {
        install::plain(&dir, true)?;
        fs::rename(
            &dir,
            base.join(format!(".damaged-{}", uuid::Uuid::new_v4())),
        )
        .map_err(|e| e.to_string())?;
    }
    fs::rename(payload, &dir).map_err(|e| e.to_string())?;
    reporter.step(success_download_step(
        "download",
        "图形分包已安装并通过校验",
    ));
    Ok(dir)
}

pub(super) async fn ensure(kind: &str, reporter: InstallReporter) -> Result<PathBuf, String> {
    let _guard = DOWNLOAD.lock().await;
    let mut result = None;
    for kind in ["common", kind] {
        let p = part(kind)?;
        let local = bundled(kind)?;
        // Offline local delivery and verified ZIP caches do not need a downloader.
        let installed = root().join(&p.version);
        let cached = root()
            .join(".archives")
            .join(format!("{}.zip", p.download.sha256));
        let manager = if verify(&installed, &p).is_ok()
            || verify(&local, &p).is_ok()
            || archive_valid(&cached, &p)?
        {
            None
        } else {
            Some(get_download_manager().await.map_err(|e| e.to_string())?)
        };
        result = Some(ensure_at(&root(), Some(&local), &p, &reporter, manager.as_deref()).await?);
    }
    result.ok_or("缺少图形分包".into())
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
            self.calls.fetch_add(1, Ordering::Relaxed);
            assert!(!options.use_github_mirror);
            let path = options.save_dir.unwrap().join(options.filename.unwrap());
            fs::write(&path, &self.bytes).unwrap();
            if self.cancelled {
                return Err(crate::AppError::Download("cancelled".into()));
            }
            Ok(DownloadResult {
                path,
                filename: "package.zip".into(),
                size: self.bytes.len() as u64,
                gid: "test".into(),
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
    fn fixture(version: &str) -> (Part, Download) {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("data.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"verified runtime").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        (Part { version: version.into(), files: vec![install::Artifact { name: "data.txt".into(), sha256: runtime_package::hash(b"verified runtime") }],
            download: runtime_package::Asset { name: "test.zip".into(), url: "https://github.com/triwinds/ns-emu-tools-runtimes/releases/download/test/test.zip".into(), sha256: runtime_package::hash(&bytes) } },
         Download { bytes, cancelled: false, calls: AtomicUsize::new(0) })
    }

    #[test]
    fn pinned_three_part_manifest_separates_programs_from_vendor_dependencies() {
        let manifest = manifest().unwrap();
        let embedded = install::package();
        assert_eq!(
            embedded.common_version.as_deref(),
            Some(manifest.common.version.as_str())
        );
        assert_eq!(embedded.files.len(), manifest.nvidia.files.len());
        for file in embedded.files {
            assert!(manifest
                .nvidia
                .files
                .iter()
                .any(|f| f.name == file.name && f.sha256 == file.sha256));
        }
        for name in [
            "graphics-launcher.exe",
            "streamline_probe_layer.dll",
            "xess-layer.dll",
            "nvngx.dll",
        ] {
            assert!(manifest.common.files.iter().any(|f| f.name == name));
            assert!(!manifest.nvidia.files.iter().any(|f| f.name == name));
            assert!(!manifest.xess.files.iter().any(|f| f.name == name));
        }
        let mut bad = manifest.common.clone();
        bad.files[0].name = "../escape".into();
        assert!(validate_part("common", &bad).is_err());
        let mut bad = manifest.common.clone();
        bad.download.url = "https://example.com/file.zip".into();
        assert!(validate_part("common", &bad).is_err());
        let mut bad = manifest.xess;
        bad.files[0].name = "graphics-launcher.exe".into();
        assert!(validate_part("xess", &bad).is_err());
    }

    #[tokio::test]
    #[ignore = "Verifies and installs the staged three-part local delivery in an isolated store"]
    async fn local_delivery_roundtrip() {
        let temp = tempfile::tempdir().unwrap();
        let reporter = InstallReporter::new(|_| {});
        let base = temp.path().join("components");
        for kind in ["common", "nvidia", "xess"] {
            let p = part(kind).unwrap();
            let local = bundled(kind).unwrap();
            let dir = ensure_at(&base, Some(&local), &p, &reporter, None)
                .await
                .unwrap();
            assert!(verify(&dir, &p).is_ok());
            assert_eq!(
                ensure_at(&base, None, &p, &reporter, None).await.unwrap(),
                dir
            );
        }
        let common = part("common").unwrap();
        assert_eq!(
            fs::read_dir(base.join(&common.version))
                .unwrap()
                .filter_map(Result::ok)
                .filter(|p| p.path().extension().is_some_and(|v| v == "exe"))
                .count(),
            1
        );
    }

    #[tokio::test]
    #[ignore = "Live Release downloads through production installers; requires an isolated runner config and no bundled components"]
    async fn published_distribution_install_roundtrip() {
        use crate::models::graphics_components::GraphicsApi;
        use serde_json::json;
        use std::io::{Read, Seek, SeekFrom};

        let isolated =
            PathBuf::from(std::env::var_os("GRAPHICS_ONLINE_ROOT").expect("GRAPHICS_ONLINE_ROOT"));
        assert_eq!(crate::config::effective_config_dir(), isolated);
        assert!(isolated.join("config.json").is_file());
        let target = std::env::current_exe().unwrap().canonicalize().unwrap();
        assert_eq!(target.file_name().unwrap(), "Ryujinx.exe");
        assert!(!root().exists(), "test requires an empty component store");
        for kind in ["common", "nvidia", "xess"] {
            assert!(
                !bundled(kind).unwrap().exists(),
                "bundled {kind} must be absent"
            );
        }
        {
            let mut config = crate::config::CONFIG.write();
            // Exercise the published origin, independently of mirror propagation.
            config.setting.network.github_download_mirror = "direct".into();
        }
        let started = std::time::Instant::now();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = events.clone();
        let reporter = InstallReporter::new(move |event| {
            captured
                .lock()
                .unwrap()
                .push(serde_json::to_value(event).unwrap())
        });
        println!("Online install: common + XeSS, no bundled files and no cache");
        super::super::xess::install(reporter.clone()).await.unwrap();
        assert_eq!(
            super::super::xess::status(None, GraphicsApi::Vulkan)["installed"],
            true
        );
        let nvidia = part("nvidia").unwrap();
        assert!(
            !root().join(&nvidia.version).exists(),
            "XeSS must not install NVIDIA dependencies"
        );
        let common = part("common").unwrap();
        let common_dir = root().join(&common.version);
        let common_before = fs::metadata(common_dir.join("graphics-launcher.exe"))
            .unwrap()
            .modified()
            .unwrap();
        println!("Online install: NVIDIA and NR, reusing the shared programs");
        let expected = file_digest(&target).unwrap();
        install::install_latest(
            target.clone(),
            GraphicsApi::Vulkan,
            true,
            expected.clone(),
            reporter.clone(),
        )
        .await
        .unwrap();
        assert_eq!(install::state(&target), "installed");
        assert!(super::super::native_nr::status().installed);
        assert_eq!(
            fs::metadata(common_dir.join("graphics-launcher.exe"))
                .unwrap()
                .modified()
                .unwrap(),
            common_before
        );
        let count = fs::read_dir(root().join(".archives")).unwrap().count();
        assert_eq!(count, 3);
        let mut packages = Vec::new();
        for kind in ["common", "nvidia", "xess"] {
            let p = part(kind).unwrap();
            drop(verify(&root().join(&p.version), &p).unwrap());
            let archive = root()
                .join(".archives")
                .join(format!("{}.zip", p.download.sha256));
            assert!(archive_valid(&archive, &p).unwrap());
            packages.push(json!({"kind":kind,"version":p.version,"url":p.download.url,
                "archiveSha256":file_digest(&archive).unwrap(),"archiveBytes":fs::metadata(&archive).unwrap().len(),"verifiedMembers":p.files.len()}));
        }
        // Reinstall with valid stores: the production downloader must not be entered.
        events.lock().unwrap().clear();
        super::super::xess::install(reporter.clone()).await.unwrap();
        install::install_latest(
            target.clone(),
            GraphicsApi::Vulkan,
            true,
            expected,
            reporter,
        )
        .await
        .unwrap();
        assert!(
            !events
                .lock()
                .unwrap()
                .iter()
                .any(|event| event["step"]["status"] == "running"
                    && event["step"]["id"] == "download")
        );
        println!("Checking rejection of a modified installed DLL and ZIP");
        let library = install::root().join("runtime/sl.common.dll");
        let original = fs::read(&library).unwrap();
        fs::write(&library, b"tampered validation fixture").unwrap();
        assert_eq!(install::state(&target), "damaged");
        fs::write(&library, &original).unwrap();
        assert_eq!(install::state(&target), "installed");
        let archive = root()
            .join(".archives")
            .join(format!("{}.zip", common.download.sha256));
        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&archive)
            .unwrap();
        let mut byte = [0];
        file.read_exact(&mut byte).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&[byte[0] ^ 0xff]).unwrap();
        file.flush().unwrap();
        assert!(!archive_valid(&archive, &common).unwrap());
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(&byte).unwrap();
        file.flush().unwrap();
        drop(file);
        assert!(archive_valid(&archive, &common).unwrap());
        let report = json!({"date":"2026-10-19","bundledComponents":false,"initialCacheEmpty":true,
            "downloadBackend":"production rust/bytehaul","githubSource":"direct","packages":packages,
            "xessInstalled":true,"nvidiaInstalled":true,"nrInstalled":true,"sharedProgramsReused":true,
            "reinstallWithoutDownload":true,"modifiedDllRejected":true,"modifiedZipRejected":true,
            "elapsedSeconds":started.elapsed().as_secs_f64()});
        fs::write(
            isolated.join("online-verification.json"),
            serde_json::to_vec_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
    }

    #[tokio::test]
    async fn offline_install_reuses_common_and_preserves_damaged_packages() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("delivery");
        install::safe_dir(&local).unwrap();
        fs::write(local.join("data.txt"), b"verified runtime").unwrap();
        let base = temp.path().join("components");
        let reporter = InstallReporter::new(|_| {});
        let (common, _) = fixture("common-v1");
        let (xess, _) = fixture("xess-v1");
        let shared = ensure_at(&base, Some(&local), &common, &reporter, None)
            .await
            .unwrap();
        let engine = ensure_at(&base, Some(&local), &xess, &reporter, None)
            .await
            .unwrap();
        assert_eq!(
            ensure_at(&base, None, &common, &reporter, None)
                .await
                .unwrap(),
            shared
        );
        assert!(!base.join("nvidia-v1").exists());
        fs::write(engine.join("data.txt"), b"damaged").unwrap();
        fs::write(engine.join("user.txt"), b"retain").unwrap();
        ensure_at(&base, Some(&local), &xess, &reporter, None)
            .await
            .unwrap();
        assert!(verify(&shared, &common).is_ok());
        assert!(verify(&engine, &xess).is_ok());
        let damaged = fs::read_dir(&base)
            .unwrap()
            .map(|v| v.unwrap().path())
            .find(|v| {
                v.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".damaged-")
            })
            .unwrap();
        assert_eq!(fs::read(damaged.join("user.txt")).unwrap(), b"retain");
    }

    #[tokio::test]
    async fn downloads_reuse_archives_and_cancel_without_publishing_partial_packages() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("components");
        let reporter = InstallReporter::new(|_| {});
        let (p, mut manager) = fixture("vendor-v1");
        let dir = ensure_at(&base, None, &p, &reporter, Some(&manager))
            .await
            .unwrap();
        assert_eq!(manager.calls.load(Ordering::Relaxed), 1);
        fs::write(dir.join("data.txt"), b"damaged").unwrap();
        ensure_at(&base, None, &p, &reporter, None).await.unwrap();
        assert_eq!(manager.calls.load(Ordering::Relaxed), 1);
        let mut next = p.clone();
        next.version = "vendor-v2".into();
        next.download.sha256 = "0".repeat(64);
        manager.cancelled = true;
        assert!(ensure_at(&base, None, &next, &reporter, Some(&manager))
            .await
            .is_err());
        assert!(!base.join(&next.version).exists());
        assert!(verify(&dir, &p).is_ok());
        manager.cancelled = false;
        assert!(ensure_at(&base, None, &next, &reporter, Some(&manager))
            .await
            .is_err());
        assert!(!base.join(&next.version).exists());
    }

    #[tokio::test]
    async fn invalid_members_do_not_publish_or_replace_existing_installations() {
        let temp = tempfile::tempdir().unwrap();
        let (mut p, mut manager) = fixture("invalid-v1");
        p.files[0].sha256 = "0".repeat(64);
        let reporter = InstallReporter::new(|_| {});
        assert!(ensure_at(temp.path(), None, &p, &reporter, Some(&manager))
            .await
            .is_err());
        assert!(!temp.path().join(&p.version).exists());
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for name in ["../escape", "data.txt"] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"verified runtime").unwrap();
        }
        manager.bytes = zip.finish().unwrap().into_inner();
        p.download.sha256 = runtime_package::hash(&manager.bytes);
        assert!(ensure_at(temp.path(), None, &p, &reporter, Some(&manager))
            .await
            .is_err());
        assert!(!temp.path().join(&p.version).exists());
    }
}
