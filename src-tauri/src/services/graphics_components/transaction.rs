//! 单个 OpenGL DLL 的持久化事务。日志先于文件替换，恢复始终回滚到事务前。
//! 首次备份按 SHA-256 保存，升级不覆盖；不管理用户的 preset / shader / ini。
use super::*;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::io::Write;

const STATE_DIR: &str = ".ns-emu-tools-reshade";
const MAX_FILE: u64 = 256 * 1024 * 1024;

pub(super) fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// 拒绝整个路径链中的链接和 Windows junction，而非只检查最终文件。
pub(super) fn safe_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("必须提供不包含上级跳转的绝对路径".into());
    }
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                let linked = metadata.file_type().is_symlink();
                #[cfg(windows)]
                let linked = {
                    use std::os::windows::fs::MetadataExt;
                    linked || metadata.file_attributes() & 0x400 != 0
                };
                if linked {
                    return Err(format!("禁止符号链接或重解析点: {}", ancestor.display()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

pub(super) fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    safe_path(path)?;
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("读取 {} 失败: {e}", path.display())),
    };
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_FILE {
        return Err(format!("文件类型或大小无效: {}", path.display()));
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_FILE {
        return Err("文件超过大小限制".into());
    }
    Ok(Some(bytes))
}

pub(super) fn file_hash(path: &Path) -> Result<Option<String>, String> {
    Ok(read_optional(path)?.map(|bytes| hash(&bytes)))
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    safe_path(path)?;
    if fs::symlink_metadata(path).is_ok() {
        read_optional(path)?;
    }
    let parent = path.parent().ok_or("没有父目录")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(bytes).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    safe_path(path)?;
    temp.persist(path)
        .map_err(|e| format!("原子替换 {} 失败: {e}", path.display()))?;
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_record(record: &GraphicsInstallationRecord, directory: &Path) -> Result<(), String> {
    if record.schema_version != 1
        || record.target_directory != directory
        || record.executable.parent() != Some(directory)
        || record.graphics_api != GraphicsApi::OpenGl
        || record.relative_path != "opengl32.dll"
        || !valid_hash(&record.deployed_sha256)
        || !valid_hash(&record.package_sha256)
        || record
            .original_sha256
            .as_ref()
            .is_some_and(|v| !valid_hash(v))
    {
        return Err("安装记录的 schema、目标或哈希无效；拒绝修改".into());
    }
    Ok(())
}

pub fn read_record(directory: &Path) -> Result<Option<GraphicsInstallationRecord>, String> {
    let Some(bytes) = read_optional(&directory.join(STATE_DIR).join("record.json"))? else {
        return Ok(None);
    };
    let record = serde_json::from_slice(&bytes).map_err(|e| format!("安装记录损坏: {e}"))?;
    validate_record(&record, directory)?;
    Ok(Some(record))
}

pub(super) fn pending(directory: &Path) -> Result<bool, String> {
    Ok(read_optional(&directory.join(STATE_DIR).join("journal.json"))?.is_some())
}

pub(super) fn ensure_stopped(directory: &Path) -> Result<(), String> {
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    for process in system.processes().values() {
        if let Some(path) = process.exe() {
            if path.canonicalize().ok().as_deref().and_then(Path::parent) == Some(directory) {
                return Err(format!(
                    "请先关闭目标目录中的进程: {}",
                    process.name().to_string_lossy()
                ));
            }
        }
    }
    Ok(())
}

pub(super) struct Store {
    directory: PathBuf,
    state: PathBuf,
    _lock: File,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    before: Option<String>,
    after: Option<String>,
    old_record: Option<GraphicsInstallationRecord>,
    new_record: Option<GraphicsInstallationRecord>,
}

impl Store {
    pub(super) fn open(directory: &Path) -> Result<Self, String> {
        safe_path(directory)?;
        let state = directory.join(STATE_DIR);
        safe_path(&state)?;
        fs::create_dir_all(&state).map_err(|e| e.to_string())?;
        let lock_path = state.join("lock");
        safe_path(&lock_path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        lock.try_lock_exclusive()
            .map_err(|_| "该目标有其他图形组件操作正在执行")?;
        Ok(Self {
            directory: directory.into(),
            state,
            _lock: lock,
        })
    }

    fn blob(&self, digest: &str) -> Result<PathBuf, String> {
        if !valid_hash(digest) {
            return Err("备份哈希无效".into());
        }
        Ok(self.state.join(format!("{digest}.blob")))
    }

    fn save_blob(&self, bytes: &[u8]) -> Result<String, String> {
        let digest = hash(bytes);
        let path = self.blob(&digest)?;
        match file_hash(&path)? {
            Some(existing) if existing != digest => return Err("备份已损坏，禁止覆盖".into()),
            Some(_) => {}
            None => atomic_write(&path, bytes)?,
        }
        Ok(digest)
    }

    fn load_blob(&self, digest: &str) -> Result<Vec<u8>, String> {
        let bytes = read_optional(&self.blob(digest)?)?.ok_or("缺少恢复备份")?;
        if hash(&bytes) != digest {
            return Err("恢复备份哈希校验失败".into());
        }
        Ok(bytes)
    }

    fn set_record(&self, record: &Option<GraphicsInstallationRecord>) -> Result<(), String> {
        let path = self.state.join("record.json");
        if let Some(record) = record {
            validate_record(record, &self.directory)?;
            atomic_write(
                &path,
                &serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?,
            )
        } else if read_optional(&path)?.is_some() {
            fs::remove_file(path).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }

    fn replace(&self, expected: &Option<String>, desired: &Option<String>) -> Result<(), String> {
        let path = self.directory.join("opengl32.dll");
        if &file_hash(&path)? != expected {
            return Err("目标文件已变化，保留当前文件；请重新检测".into());
        }
        if expected == desired {
            return Ok(());
        }
        ensure_stopped(&self.directory)?;
        if let Some(digest) = desired {
            atomic_write(&path, &self.load_blob(digest)?)?;
        } else {
            fs::remove_file(&path).map_err(|e| format!("无法移除 DLL: {e}"))?;
        }
        if &file_hash(&path)? != desired {
            return Err("部署后哈希校验失败".into());
        }
        Ok(())
    }

    fn commit(&self, journal: Journal) -> Result<(), String> {
        let path = self.state.join("journal.json");
        if pending(&self.directory)? {
            return Err("存在未完成事务，请先恢复".into());
        }
        atomic_write(
            &path,
            &serde_json::to_vec_pretty(&journal).map_err(|e| e.to_string())?,
        )?;
        let result = (|| {
            self.replace(&journal.before, &journal.after)?;
            self.set_record(&journal.new_record)?;
            fs::remove_file(&path).map_err(|e| e.to_string())
        })();
        if let Err(error) = result {
            return match self.repair() {
                Ok(_) => Err(format!("{error}；已回滚到操作前状态")),
                Err(recovery) => Err(format!(
                    "{error}；恢复未完成: {recovery}。请执行 repair_graphics_components"
                )),
            };
        }
        Ok(())
    }

    pub(super) fn install(
        &self,
        mut record: GraphicsInstallationRecord,
        bytes: &[u8],
        expected: Option<String>,
        expected_record: Option<GraphicsInstallationRecord>,
        confirmed: bool,
    ) -> Result<GraphicsOperationResult, String> {
        if pending(&self.directory)? {
            return Err("存在未完成事务，请先恢复".into());
        }
        let old_record = read_record(&self.directory)?;
        if old_record != expected_record {
            return Err("安装记录已变化，请重新生成计划".into());
        }
        let current = read_optional(&self.directory.join("opengl32.dll"))?;
        let before = current.as_deref().map(hash);
        if before != expected {
            return Err("目标文件已变化，请重新生成计划".into());
        }
        if let Some(old) = &old_record {
            if before.as_ref() != Some(&old.deployed_sha256) {
                return Err("受管理 DLL 被外部修改，禁止覆盖".into());
            }
            record.original_sha256 = old.original_sha256.clone();
            if let Some(original) = &record.original_sha256 {
                self.load_blob(original)?;
            }
        } else {
            if current.is_some() && !confirmed {
                return Err("覆盖外部 DLL 需要显式确认".into());
            }
            record.original_sha256 = before.clone();
        }
        if hash(bytes) != record.deployed_sha256 {
            return Err("待部署 DLL 哈希变化".into());
        }
        if let Some(current) = current {
            self.save_blob(&current)?;
        }
        self.save_blob(bytes)?;
        self.commit(Journal {
            schema_version: 1,
            before,
            after: Some(record.deployed_sha256.clone()),
            old_record,
            new_record: Some(record.clone()),
        })?;
        Ok(GraphicsOperationResult {
            record: Some(record),
            message: "OpenGL ReShade DLL 已部署并校验；运行加载和兼容性尚未验证。".into(),
            compatibility_verified: false,
        })
    }

    fn uninstall(&self) -> Result<GraphicsOperationResult, String> {
        if pending(&self.directory)? {
            return Err("存在未完成事务，请先恢复".into());
        }
        if feeder::has_record_or_pending(&self.directory)? {
            return Err("请先卸载或修复 Feeder".into());
        }
        for addon in ["dlss5-feed.addon64", "renodx-dlss5.addon64"] {
            if fs::symlink_metadata(self.directory.join(addon)).is_ok() {
                return Err(format!(
                    "发现依赖 ReShade 的 {addon}，请先处理 Feeder 后再卸载"
                ));
            }
        }
        let record = read_record(&self.directory)?.ok_or("没有本工具安装记录，禁止移除外部文件")?;
        let before = file_hash(&self.directory.join("opengl32.dll"))?;
        if before.as_ref() != Some(&record.deployed_sha256) {
            return Err("DLL 缺失或被用户修改，保留当前文件和安装记录".into());
        }
        // 确保删除后仍可回滚，即使缓存已被用户清理。
        let current = read_optional(&self.directory.join("opengl32.dll"))?.ok_or("DLL 缺失")?;
        self.save_blob(&current)?;
        if let Some(original) = &record.original_sha256 {
            self.load_blob(original)?;
        }
        self.commit(Journal {
            schema_version: 1,
            before,
            after: record.original_sha256.clone(),
            old_record: Some(record),
            new_record: None,
        })?;
        Ok(GraphicsOperationResult {
            record: None,
            message: "已卸载并恢复首次备份；用户配置、preset 和 shader 保留。".into(),
            compatibility_verified: false,
        })
    }

    fn repair(&self) -> Result<GraphicsOperationResult, String> {
        let path = self.state.join("journal.json");
        if let Some(bytes) = read_optional(&path)? {
            let journal: Journal =
                serde_json::from_slice(&bytes).map_err(|e| format!("事务日志损坏: {e}"))?;
            if journal.schema_version != 1
                || [&journal.before, &journal.after]
                    .iter()
                    .any(|v| v.as_ref().is_some_and(|h| !valid_hash(h)))
            {
                return Err("不支持的事务日志".into());
            }
            for record in [&journal.old_record, &journal.new_record]
                .into_iter()
                .flatten()
            {
                validate_record(record, &self.directory)?;
            }
            if journal
                .old_record
                .as_ref()
                .is_some_and(|r| journal.before.as_ref() != Some(&r.deployed_sha256))
                || journal
                    .new_record
                    .as_ref()
                    .is_some_and(|r| journal.after.as_ref() != Some(&r.deployed_sha256))
                || (journal.old_record.is_none() && journal.new_record.is_none())
                || (journal.new_record.is_none()
                    && journal
                        .old_record
                        .as_ref()
                        .is_some_and(|r| r.original_sha256 != journal.after))
            {
                return Err("事务日志与所有权记录不一致".into());
            }
            let current_record = read_record(&self.directory)?;
            if current_record != journal.old_record && current_record != journal.new_record {
                return Err("安装记录被外部修改，停止恢复".into());
            }
            let current = file_hash(&self.directory.join("opengl32.dll"))?;
            if current != journal.before && current != journal.after {
                return Err("文件被外部修改，停止恢复并保留日志".into());
            }
            self.replace(&current, &journal.before)?;
            self.set_record(&journal.old_record)?;
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(GraphicsOperationResult {
            record: read_record(&self.directory)?,
            message: "恢复检查完成；未完成的事务已回滚。".into(),
            compatibility_verified: false,
        })
    }
}

pub(super) fn target_directory(executable: &Path) -> Result<PathBuf, String> {
    if !cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        return Err("仅支持 Windows x64".into());
    }
    safe_path(executable)?;
    let executable = executable.canonicalize().map_err(|e| e.to_string())?;
    if executable_architecture(&executable)? != ExecutableArchitecture::X64 {
        return Err("目标不是 x64 EXE".into());
    }
    let directory = executable.parent().ok_or("没有目标目录")?.to_path_buf();
    ensure_stopped(&directory)?;
    Ok(directory)
}

pub fn remove_or_repair(
    executable: PathBuf,
    repair: bool,
) -> Result<GraphicsOperationResult, String> {
    let directory = target_directory(&executable)?;
    // 不为没有安装记录的目标创建状态目录。
    if !directory.join(STATE_DIR).exists() {
        return Err("没有可恢复或卸载的安装记录".into());
    }
    let store = Store::open(&directory)?;
    if repair {
        store.repair()
    } else {
        store.uninstall()
    }
}

#[cfg(test)]
mod tests;
