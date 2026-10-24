//! One shared runtime directory, replaced only while no process is using it.
use super::{streamline_install as install, streamline_update};
use install::Package;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn destination(base: &Path) -> PathBuf {
    base.join("runtime")
}

pub(super) fn manifest(dir: &Path) -> Result<Package, String> {
    super::runtime_package::safe_path(dir)?;
    install::plain(dir, true)?;
    let package: Package =
        serde_json::from_value(install::read_live_json(&dir.join("installation.json"))?)
            .map_err(|e| e.to_string())?;
    streamline_update::validate_selected(&package)?;
    Ok(package)
}

pub(super) fn verify(dir: &Path, package: &Package) -> Result<(), String> {
    let saved = manifest(dir)?;
    if saved.version != package.version || !streamline_update::same_files(&saved, package) {
        return Err("共享组件安装记录不匹配".into());
    }
    install::verify_artifacts(dir, &package.files)
}

fn members(dir: &Path, package: &Package, partial: bool) -> Result<Vec<PathBuf>, String> {
    if partial {
        let saved = manifest(dir)?;
        if saved.version != package.version || !streamline_update::same_files(&saved, package) {
            return Err("共享组件安装记录不匹配".into());
        }
        for artifact in &package.files {
            let path = dir.join(&artifact.name);
            if path.try_exists().map_err(|e| e.to_string())? {
                install::plain(&path, false)?;
                if super::streamline_fg::file_digest(&path)? != artifact.sha256 {
                    return Err("更新备份组件已改变，保留原文件".into());
                }
            }
        }
    } else {
        verify(dir, package)?;
    }
    let mut files: Vec<_> = package.files.iter().map(|f| dir.join(&f.name)).collect();
    files.push(dir.join("installation.json"));
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if !files.contains(&path) {
            return Err("组件目录含有额外文件，保留原目录并停止操作".into());
        }
    }
    if partial {
        files.retain(|path| path.exists());
    }
    Ok(files)
}

pub(super) fn remove(dir: &Path, package: &Package) -> Result<(), String> {
    remove_members(dir, package, false)
}
fn remove_members(dir: &Path, package: &Package, partial: bool) -> Result<(), String> {
    let files = members(dir, package, partial)?;
    let guard = install::hold_runtime_files(&files, true)?;
    for file in files {
        fs::remove_file(file).map_err(|e| e.to_string())?;
    }
    drop(guard);
    fs::remove_dir(dir).map_err(|e| e.to_string())
}

fn reap_retired(base: &Path) {
    let Ok(entries) = fs::read_dir(base) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_name().to_string_lossy().starts_with(".retired-") {
            continue;
        }
        let result = manifest(&path).and_then(|package| remove_members(&path, &package, true));
        if let Err(error) = result {
            tracing::warn!(%error, path = %path.display(), "保留未能回收的组件更新备份");
        }
    }
}

pub(super) fn install(
    base: &Path,
    source: &Path,
    package: &Package,
    ready: impl Fn() -> Result<(), String>,
) -> Result<(), String> {
    install::safe_dir(base)?;
    let dir = destination(base);
    let previous = if dir.exists() {
        Some(manifest(&dir)?)
    } else {
        None
    };
    if let Some(previous) = &previous {
        if previous.version == package.version {
            if !streamline_update::same_files(previous, package) {
                return Err("同版本组件的校验值发生变化，请使用新的版本清单".into());
            }
            verify(&dir, package)?;
            ready()?;
            reap_retired(base);
            return Ok(());
        }
    }
    let guard = previous
        .as_ref()
        .map(|previous| {
            members(&dir, previous, false)
                .and_then(|files| install::hold_runtime_files(&files, true))
        })
        .transpose()?;
    let stage = tempfile::Builder::new()
        .prefix(".staging-")
        .tempdir_in(base)
        .map_err(|e| e.to_string())?;
    install::copy_package_for(source, stage.path(), package)?;
    install::atomic_json(
        &stage.path().join("installation.json"),
        &serde_json::to_value(package).map_err(|e| e.to_string())?,
    )?;
    ready()?;
    // Windows refuses to rename a directory while its children have open
    // handles, even with FILE_SHARE_DELETE. All toolbox operations hold the
    // store lock; a new external reader instead makes the rename fail safely.
    drop(guard);
    let backup = if previous.is_some() {
        let backup = tempfile::Builder::new()
            .prefix(".retired-")
            .tempdir_in(base)
            .map_err(|e| e.to_string())?;
        let path = backup.keep();
        if path.parent() != Some(base)
            || stage.path().parent() != Some(base)
            || dir.parent() != Some(base)
        {
            return Err("组件更新路径不属于存储目录".into());
        }
        fs::remove_dir(&path).map_err(|e| e.to_string())?;
        fs::rename(&dir, &path).map_err(|e| e.to_string())?;
        Some(path)
    } else {
        None
    };
    if let Err(error) = fs::rename(stage.path(), &dir) {
        if let Some(path) = &backup {
            if let Err(restore) = fs::rename(path, &dir) {
                return Err(format!(
                    "组件更新失败：{error}；原目录保留在 {}，恢复失败：{restore}",
                    path.display()
                ));
            }
        }
        return Err(error.to_string());
    }
    if let (Some(path), Some(previous)) = (backup, previous) {
        if let Err(error) = remove(&path, &previous) {
            tracing::warn!(%error, "旧组件更新备份回收未完成，将在下次安装时重试");
        }
    }
    reap_retired(base);
    Ok(())
}
