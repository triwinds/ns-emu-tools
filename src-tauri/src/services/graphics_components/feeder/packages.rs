//! Pinned public assets. RHI is a community mirror, not NVIDIA's publisher endpoint.
use super::*;
use crate::services::downloader::{get_download_manager, DownloadOptions};
use crate::services::installer::{download_progress_step, InstallReporter};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub sha256: String,
}
pub fn assets() -> Result<Vec<Asset>, String> {
    serde_json::from_str(include_str!("assets.json")).map_err(|e| e.to_string())
}
static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub async fn download(reporter: InstallReporter) -> Result<PathBuf, String> {
    let _guard = DOWNLOAD_LOCK.lock().await;
    let root = effective_config_dir().join("graphics-components/cache/feeder-rhi-v1");
    transaction::safe_path(&root)?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let lock_path = root.join("lock");
    transaction::safe_path(&lock_path)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|e| e.to_string())?;
    fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| "Feeder 缓存正在使用")?;
    let manager = get_download_manager().await.map_err(|e| e.to_string())?;
    for asset in assets()? {
        let path = root.join(&asset.name);
        transaction::safe_path(&path)?;
        if transaction::file_hash(&path)?.as_deref() == Some(asset.sha256.as_str()) {
            continue;
        }
        let stage = tempfile::tempdir_in(&root).map_err(|e| e.to_string())?;
        let progress = reporter.clone();
        let label = asset.name.clone();
        manager
            .download_and_wait(
                &asset.url,
                DownloadOptions {
                    save_dir: Some(stage.path().into()),
                    filename: Some("asset.bin".into()),
                    use_github_mirror: false,
                    ..Default::default()
                },
                Box::new(move |p| {
                    progress.step(download_progress_step(
                        "download",
                        format!("下载 {label}"),
                        &p,
                        Some("GitHub / RHI".into()),
                    ))
                }),
            )
            .await
            .map_err(|e| e.to_string())?;
        let bytes =
            transaction::read_optional(&stage.path().join("asset.bin"))?.ok_or("下载结果缺失")?;
        if transaction::hash(&bytes) != asset.sha256 {
            return Err(format!("{} 发布 SHA-256 不匹配", asset.name));
        }
        fs::create_dir_all(path.parent().ok_or("缓存路径无效")?).map_err(|e| e.to_string())?;
        transaction::atomic_write(&path, &bytes)?;
    }
    Ok(root)
}

/// Read only named members, normalize the upstream ZIP's Windows separators, never extract paths.
fn member(package: &Path, wanted: &str, dll: bool) -> Result<Vec<u8>, String> {
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

pub(super) fn payload(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    for asset in assets()? {
        if transaction::file_hash(&root.join(&asset.name))?.as_deref()
            != Some(asset.sha256.as_str())
        {
            return Err(format!("{} 缓存摘要不匹配，请重新下载", asset.name));
        }
    }
    let mut files = BTreeMap::new();
    for (zip, member_name, destination, dll) in [
        (
            "feeder.zip",
            "dlss5-feed.addon64",
            "dlss5-feed.addon64",
            true,
        ),
        (
            "feeder.zip",
            "reshade-shaders/Shaders/DLSS5_Feed.fx",
            "ns-emu-tools-feeder/Shaders/DLSS5_Feed.fx",
            false,
        ),
        ("dlss-310.9.1.zip", "nvngx_dlss.dll", "nvngx_dlss.dll", true),
        (
            "dlssnr-310.8.0.zip",
            "nvngx_dlssnr.dll",
            "nvngx_dlssnr.dll",
            true,
        ),
        (
            "renodx-dlss5-4.70.zip",
            "renodx-dlss5.addon64",
            "renodx-dlss5.addon64",
            true,
        ),
    ] {
        files.insert(
            destination.into(),
            member(&root.join(zip), member_name, dll)?,
        );
    }
    for asset in assets()?.into_iter().filter(|a| !a.name.ends_with(".zip")) {
        files.insert(
            format!("ns-emu-tools-feeder/{}", asset.name),
            transaction::read_optional(&root.join(asset.name))?.ok_or("缓存文件缺失")?,
        );
    }
    Ok(files)
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
        zip.start_file("shaders\\feed.fx", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"shader").unwrap();
        zip.finish().unwrap();
        assert_eq!(
            member(f.path(), "shaders/feed.fx", false).unwrap(),
            b"shader"
        );
    }
    #[tokio::test]
    #[ignore = "下载固定官方/RHI 包，验证真实归档和全部依赖"]
    async fn pinned_public_packages() {
        let root = download(InstallReporter::new(|_| {})).await.unwrap();
        let files = payload(&root).unwrap();
        assert_eq!(files.len(), 12);
        // Exercise the same public preview/install/remove entry points against a temporary target.
        // The official ReShade OpenGL proxy avoids touching the real Vulkan registry in this test.
        #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
        {
            let dir = tempfile::tempdir().unwrap();
            let exe = dir.path().join("emu.exe");
            fs::write(&exe, super::super::super::tests::pe(0x8664, 0x20b)).unwrap();
            let original = b"[GENERAL]\r\nPresetPath=user.ini\r\n";
            fs::write(dir.path().join("ReShade.ini"), original).unwrap();
            fs::write(dir.path().join("user.ini"), b"user preset").unwrap();
            let reshade = super::super::super::packages::prepare_official(
                exe.clone(),
                GraphicsApi::OpenGl,
                InstallReporter::new(|_| {}),
            )
            .await
            .unwrap();
            planning::install(reshade.plan_id.unwrap(), false).unwrap();
            let stale =
                super::super::preview(exe.clone(), GraphicsApi::OpenGl, root.clone()).unwrap();
            fs::write(
                dir.path().join("ReShade.ini"),
                b"[GENERAL]\r\nChanged=1\r\n",
            )
            .unwrap();
            assert!(super::super::install(stale.plan_id.unwrap()).is_err());
            assert!(!dir.path().join("dlss5-feed.addon64").exists());
            fs::write(dir.path().join("ReShade.ini"), original).unwrap();
            let plan =
                super::super::preview(exe.clone(), GraphicsApi::OpenGl, root.clone()).unwrap();
            assert!(plan.blockers.is_empty(), "{:?}", plan.blockers);
            let id = plan.plan_id.unwrap();
            super::super::install(id.clone()).unwrap();
            assert!(super::super::install(id).is_err());
            assert_eq!(
                super::super::detection(&dir.path().canonicalize().unwrap()).unwrap(),
                GraphicsComponentState::Installed
            );
            assert!(transaction::remove_or_repair(exe.clone(), false).is_err());
            super::super::remove_or_repair(exe.clone(), false).unwrap();
            assert_eq!(fs::read(dir.path().join("ReShade.ini")).unwrap(), original);
            assert_eq!(
                fs::read(dir.path().join("user.ini")).unwrap(),
                b"user preset"
            );
            assert!(dir.path().join("opengl32.dll").exists());
            assert!(!dir.path().join("nvngx_dlssnr.dll").exists());
            transaction::remove_or_repair(exe, false).unwrap();
        }
        println!(
            "verified {} assets and {} deployable files",
            assets().unwrap().len(),
            files.len()
        );
    }
}
