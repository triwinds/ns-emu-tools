//! 本地包预检：仅从归档读取固定 DLL，不执行安装器，也不向磁盘解包。
use super::*;
use std::io::Cursor;

const MAX_DLL_SIZE: u64 = 64 * 1024 * 1024;

pub(super) fn extract_dll(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|e| format!("无法读取安装包: {e}"))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("安装包不是普通文件".into());
    }
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("安装包没有可读取的 ZIP 归档: {e}"))?;
    if archive.len() > 4096 {
        return Err("归档条目过多".into());
    }
    let mut selected = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|e| e.to_string())?;
        // 无论当前系统如何解析路径，均拒绝 Windows 路径逃逸和备用数据流。
        let name = entry.name();
        if entry.enclosed_name().is_none()
            || name.contains(['\\', ':'])
            || name.split('/').any(|part| part == "..")
        {
            return Err(format!("归档包含不安全路径: {name}"));
        }
        if name.eq_ignore_ascii_case("ReShade64.dll") {
            if selected.is_some() {
                return Err("归档包含重复的 ReShade64.dll".into());
            }
            if entry.is_dir()
                || entry.unix_mode().is_some_and(|mode| {
                    let kind = mode & 0o170000;
                    kind != 0 && kind != 0o100000
                })
                || entry.size() == 0
                || entry.size() > MAX_DLL_SIZE
            {
                return Err("ReShade64.dll 类型或大小无效".into());
            }
            selected = Some(index);
        }
    }
    let index = selected.ok_or("归档根目录缺少 ReShade64.dll")?;
    let entry = archive.by_index(index).map_err(|e| e.to_string())?;
    let expected_size = entry.size();
    let mut bytes = Vec::new();
    entry
        .take(MAX_DLL_SIZE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("DLL 解压或 CRC 校验失败: {e}"))?;
    let size = bytes.len() as u64;
    if size != expected_size || size > MAX_DLL_SIZE {
        return Err("DLL 实际大小与归档声明不一致".into());
    }
    if pe_architecture(&mut Cursor::new(&bytes), size, true)? != ExecutableArchitecture::X64 {
        return Err("安装包 DLL 不是 x64 架构".into());
    }
    Ok(bytes)
}

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use transaction::{file_hash, hash};

struct Prepared {
    expires: Instant,
    vulkan_snapshot: Option<String>,
    executable: PathBuf,
    executable_hash: String,
    package: PathBuf,
    package_hash: String,
    dll_hash: String,
    expected: Option<String>,
    record: Option<GraphicsInstallationRecord>,
    release: Option<GraphicsComponentVersion>,
}
static PLANS: Lazy<Mutex<HashMap<String, Prepared>>> = Lazy::new(|| Mutex::new(HashMap::new()));
const PLAN_LIFETIME: Duration = Duration::from_secs(15 * 60);

pub fn preview(
    executable: PathBuf,
    graphics_api: GraphicsApi,
    package: PathBuf,
) -> Result<GraphicsInstallPreview, String> {
    prepare(executable, graphics_api, package, None)
}

pub(super) fn prepare(
    executable: PathBuf,
    graphics_api: GraphicsApi,
    package: PathBuf,
    release: Option<GraphicsComponentVersion>,
) -> Result<GraphicsInstallPreview, String> {
    let directory = transaction::target_directory(&executable)?;
    let executable = executable.canonicalize().map_err(|e| e.to_string())?;
    transaction::safe_path(&package)?;
    let package = package.canonicalize().map_err(|e| e.to_string())?;
    let package_sha256 = file_hash(&package)?.ok_or("安装包缺失")?;
    let dll = extract_dll(&package)?;
    if file_hash(&package)?.as_ref() != Some(&package_sha256) {
        return Err("预检期间安装包发生变化".into());
    }
    let dll_sha256 = hash(&dll);
    let vulkan_snapshot = if graphics_api == GraphicsApi::Vulkan {
        vulkan::manifest(&package)?;
        Some(vulkan::snapshot(&executable)?)
    } else {
        None
    };
    let destination = if vulkan_snapshot.is_some() {
        vulkan::root().join("ReShade64.dll")
    } else {
        directory.join("opengl32.dll")
    };
    let mut blockers = Vec::new();
    let record = if vulkan_snapshot.is_some() {
        vulkan::read_record(&executable)?
    } else {
        transaction::read_record(&directory)?
    };
    if transaction::pending(&directory)? {
        blockers.push("存在未完成事务，请先恢复".into());
    }
    let expected = match file_hash(&destination) {
        Ok(value) => value,
        Err(error) => {
            blockers.push(error);
            None
        }
    };
    if let Some(record) = &record {
        if expected.as_ref() != Some(&record.deployed_sha256) {
            blockers.push("受管理 DLL 已缺失或被外部修改".into());
        }
    }
    // 不叠加其他 API 的代理安装，也不擅自删除它们。
    for name in [
        "dxgi.dll",
        "d3d9.dll",
        "d3d10.dll",
        "d3d11.dll",
        "d3d12.dll",
    ] {
        if fs::symlink_metadata(directory.join(name)).is_ok() {
            blockers.push(format!("发现可能冲突的代理 {name}，请先手动检查"));
        }
    }
    let requires_external_overwrite_confirmation =
        vulkan_snapshot.is_none() && record.is_none() && expected.is_some();
    let version = release.as_ref().map(|r| r.version.clone());
    let source_url = release.as_ref().map(|r| r.source_url.clone());
    let plan_id = if blockers.is_empty() {
        let mut plans = PLANS.lock();
        plans.retain(|_, p| Instant::now() < p.expires);
        if plans.len() >= 64 {
            return Err("待执行计划过多，请稍后重试".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        plans.insert(
            id.clone(),
            Prepared {
                expires: Instant::now() + PLAN_LIFETIME,
                vulkan_snapshot,
                executable: executable.clone(),
                executable_hash: file_hash(&executable)?.ok_or("目标缺失")?,
                package: package.clone(),
                package_hash: package_sha256.clone(),
                dll_hash: dll_sha256.clone(),
                expected,
                record,
                release,
            },
        );
        Some(id)
    } else {
        None
    };
    let affected_targets = if graphics_api == GraphicsApi::Vulkan {
        vulkan::affected_targets(&executable)?
    } else {
        vec![executable.clone()]
    };
    Ok(GraphicsInstallPreview {
        affected_targets,
        target: detect(executable, Some(graphics_api)), package, plan_id,
        package_sha256, dll_sha256, version, source_url, dll_size: dll.len() as u64, destination,
        requires_external_overwrite_confirmation, requires_vulkan_scope_confirmation: graphics_api == GraphicsApi::Vulkan, blockers,
        diagnostics: vec![
            "计划有效期为 15 分钟；安装时重新校验包、目标和安装记录。Vulkan 更新会影响 affectedTargets 中全部引用目标，需全部退出。".into(),
            if graphics_api == GraphicsApi::Vulkan { vulkan::SCOPE.into() } else { "只部署 OpenGL 核心 DLL；保留已有 ini、preset、shader 和 addon，不自动安装效果包。".into() },
            "本地包的来源、版本及 addon 支持未认证；本地 SHA-256 仅用于变更检测。运行兼容性尚未验证。".into(),
        ],
    })
}

pub fn install(
    plan_id: String,
    confirm_external_overwrite: bool,
) -> Result<GraphicsOperationResult, String> {
    install_with_scope(plan_id, confirm_external_overwrite, false)
}

pub fn install_with_scope(
    plan_id: String,
    confirm_external_overwrite: bool,
    confirm_vulkan_scope: bool,
) -> Result<GraphicsOperationResult, String> {
    let plan = PLANS
        .lock()
        .remove(&plan_id)
        .ok_or("计划不存在、已使用或已过期，请重新预检")?;
    if Instant::now() >= plan.expires {
        return Err("计划已过期，请重新预检".into());
    }
    let directory = transaction::target_directory(&plan.executable)?;
    if plan.vulkan_snapshot.is_some() && !confirm_vulkan_scope {
        return Err("必须确认当前用户全局 Vulkan layer 影响范围".into());
    }
    if file_hash(&plan.executable)?.as_ref() != Some(&plan.executable_hash) {
        return Err("模拟器 EXE 已变化，请重新预检".into());
    }
    if file_hash(&plan.package)?.as_ref() != Some(&plan.package_hash) {
        return Err("安装包已变化，请重新预检".into());
    }
    for name in [
        "dxgi.dll",
        "d3d9.dll",
        "d3d10.dll",
        "d3d11.dll",
        "d3d12.dll",
    ] {
        if fs::symlink_metadata(directory.join(name)).is_ok() {
            return Err(format!("发现可能冲突的代理 {name}，请重新检查目标"));
        }
    }
    let dll = extract_dll(&plan.package)?;
    if hash(&dll) != plan.dll_hash {
        return Err("DLL 已变化，请重新预检".into());
    }
    // 后端构造固定写入列表；不接收来自前端的目标相对路径或记录。
    let record = GraphicsInstallationRecord {
        schema_version: 1,
        target_directory: directory,
        executable: plan.executable,
        graphics_api: if plan.vulkan_snapshot.is_some() {
            GraphicsApi::Vulkan
        } else {
            GraphicsApi::OpenGl
        },
        version: plan.release.as_ref().map(|r| r.version.clone()),
        source_url: plan.release.map(|r| r.source_url),
        package_sha256: plan.package_hash,
        deployed_sha256: plan.dll_hash,
        original_sha256: None,
        relative_path: if plan.vulkan_snapshot.is_some() {
            "ReShade64.dll"
        } else {
            "opengl32.dll"
        }
        .into(),
    };
    if let Some(expected) = plan.vulkan_snapshot {
        let manifest = vulkan::manifest(&plan.package)?;
        if file_hash(&plan.package)?.as_ref() != Some(&record.package_sha256) {
            return Err("安装包已变化".into());
        }
        return vulkan::install(record, dll, manifest, &expected);
    }
    let store = transaction::Store::open(&record.target_directory)?;
    store.install(
        record,
        &dll,
        plan.expected,
        plan.record,
        confirm_external_overwrite,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn package(entries: &[(&str, Vec<u8>)], prefix: bool) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        if prefix {
            file.write_all(&vec![0x4d; 1024]).unwrap();
        }
        // ZIP 独立生成后追加，覆盖安装器附加归档的偏移修正路径。
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        file.write_all(&writer.finish().unwrap().into_inner())
            .unwrap();
        file
    }

    fn dll() -> Vec<u8> {
        let mut bytes = super::super::tests::pe(0x8664, 0x20b);
        bytes[86..88].copy_from_slice(&0x2002u16.to_le_bytes());
        bytes
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    #[test]
    fn vulkan_scope_confirmation_is_required_before_any_write() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("emu.exe");
        fs::write(&exe, super::super::tests::pe(0x8664, 0x20b)).unwrap();
        let id = uuid::Uuid::new_v4().to_string();
        PLANS.lock().insert(
            id.clone(),
            Prepared {
                expires: Instant::now() + PLAN_LIFETIME,
                vulkan_snapshot: Some("snapshot".into()),
                executable: exe,
                executable_hash: String::new(),
                package: dir.path().join("unused"),
                package_hash: String::new(),
                dll_hash: String::new(),
                expected: None,
                record: None,
                release: None,
            },
        );
        assert!(install_with_scope(id, true, false)
            .unwrap_err()
            .contains("必须确认"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn reads_appended_zip_without_extracting() {
        for prefix in [false, true] {
            let file = package(&[("ReShade64.dll", dll())], prefix);
            assert_eq!(extract_dll(file.path()).unwrap().len(), 512);
        }
    }

    #[test]
    fn rejects_unsafe_ambiguous_and_fake_packages() {
        for entries in [
            vec![("../escape", vec![]), ("ReShade64.dll", dll())],
            vec![("C:\\escape", vec![]), ("ReShade64.dll", dll())],
            vec![("ReShade64.dll", dll()), ("reshade64.DLL", dll())],
            vec![("ReShade64.dll", b"MZ fake".to_vec())],
            vec![("ReShade64.dll", super::super::tests::pe(0x8664, 0x20b))],
            vec![("nested/ReShade64.dll", dll())],
        ] {
            assert!(extract_dll(package(&entries, false).path()).is_err());
        }
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    #[test]
    fn previews_conflicts_without_modifying_target() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("eden.exe");
        fs::write(&exe, super::super::tests::pe(0x8664, 0x20b)).unwrap();
        let file = package(&[("ReShade64.dll", dll())], true);
        let plan = || preview(exe.clone(), GraphicsApi::OpenGl, file.path().into()).unwrap();
        assert!(!plan().requires_external_overwrite_confirmation);
        fs::write(dir.path().join("opengl32.dll"), b"external").unwrap();
        assert!(plan().requires_external_overwrite_confirmation);
        assert_eq!(
            fs::read(dir.path().join("opengl32.dll")).unwrap(),
            b"external"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
        fs::remove_file(dir.path().join("opengl32.dll")).unwrap();
        fs::create_dir(dir.path().join("opengl32.dll")).unwrap();
        assert_eq!(plan().blockers.len(), 1);
        assert!(preview(exe, GraphicsApi::Vulkan, file.path().into()).is_err());
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    #[test]
    fn frozen_plans_reject_mutations_and_require_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("emu.exe");
        fs::write(&exe, super::super::tests::pe(0x8664, 0x20b)).unwrap();
        let file = package(&[("ReShade64.dll", dll())], false);
        let make = || {
            preview(exe.clone(), GraphicsApi::OpenGl, file.path().into())
                .unwrap()
                .plan_id
                .unwrap()
        };
        assert!(install("forged-plan".into(), true).is_err());
        let id = make();
        fs::write(dir.path().join("opengl32.dll"), b"appeared after preview").unwrap();
        assert!(install(id, true).is_err());
        let id = make();
        assert!(install(id, false).is_err());
        assert_eq!(
            fs::read(dir.path().join("opengl32.dll")).unwrap(),
            b"appeared after preview"
        );
        let id = make();
        PLANS.lock().get_mut(&id).unwrap().expires = Instant::now();
        assert!(install(id, true).is_err());
        let id = make();
        let mut changed = super::super::tests::pe(0x8664, 0x20b);
        changed[500] = 42;
        fs::write(&exe, changed).unwrap();
        assert!(install(id, true).is_err());
        let id = make();
        fs::write(file.path(), b"changed package").unwrap();
        assert!(install(id, true).is_err());
    }
}
