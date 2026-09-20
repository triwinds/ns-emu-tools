//! Current-user shared implicit layer. Registry and files are journalled together.
//! The upstream loading check uses ReShade.ini, not ReShadeApps.ini as an allowlist.
use super::*;
use serde::{Deserialize, Serialize};
use transaction::{atomic_write, file_hash, hash, read_optional, safe_path};

const DLL: &str = "ReShade64.dll";
const MANIFEST: &str = "ReShade64.json";
const CONFIG: &[u8] = b"[GENERAL]\r\n";
// Encode binary snapshots compactly; bounded DLLs then fit the bounded journal reader.
mod optional_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    pub fn serialize<S: Serializer>(value: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        value.as_ref().map(|b| STANDARD.encode(b)).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| STANDARD.decode(s).map_err(serde::de::Error::custom))
            .transpose()
    }
}
pub const SCOPE: &str = "HKCU implicit layer 对当前用户全局可见，可能加载到其他已有 ReShade.ini 的 Vulkan 程序；非严格目标白名单。管理员启动的模拟器不读取 HKCU。";

pub(super) fn root() -> PathBuf {
    effective_config_dir()
        .join("graphics-components")
        .join("vulkan-x64")
}

/// Registry adapter is replaceable so tests never change the real registry.
trait Registry {
    fn get(&self, manifest: &Path) -> Result<Option<u32>, String>;
    fn set(&self, manifest: &Path, value: Option<u32>) -> Result<(), String>;
    fn conflicts(&self, manifest: &Path) -> Result<(), String>;
}
struct UserRegistry;
#[cfg(windows)]
const KEY: &str = r"Software\Khronos\Vulkan\ImplicitLayers";
#[cfg(windows)]
impl Registry for UserRegistry {
    fn get(&self, manifest: &Path) -> Result<Option<u32>, String> {
        use winreg::{enums::*, RegKey};
        let key = match RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(KEY, KEY_READ | KEY_WOW64_64KEY)
        {
            Ok(k) => k,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        match key.get_value(manifest.as_os_str()) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("layer 注册值无法读取或不是 DWORD: {e}")),
        }
    }
    fn set(&self, manifest: &Path, value: Option<u32>) -> Result<(), String> {
        use winreg::{enums::*, RegKey};
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey_with_flags(KEY, KEY_WRITE | KEY_WOW64_64KEY)
            .map_err(|e| e.to_string())?;
        if let Some(v) = value {
            key.set_value(manifest.as_os_str(), &v)
                .map_err(|e| e.to_string())
        } else {
            match key.delete_value(manifest.as_os_str()) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
    fn conflicts(&self, own: &Path) -> Result<(), String> {
        use winreg::{enums::*, RegKey};
        for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            for key_path in [KEY, r"Software\Khronos\Vulkan\ExplicitLayers"] {
                let key = match RegKey::predef(hive)
                    .open_subkey_with_flags(key_path, KEY_READ | KEY_WOW64_64KEY)
                {
                    Ok(k) => k,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e.to_string()),
                };
                for entry in key.enum_values() {
                    let (name, _) = entry.map_err(|e| e.to_string())?;
                    if hive == HKEY_CURRENT_USER && key_path == KEY && Path::new(&name) == own {
                        continue;
                    }
                    let bytes = fs::metadata(&name).and_then(|m| {
                        if m.is_file() && m.len() <= 65536 {
                            fs::read(&name)
                        } else {
                            Err(std::io::Error::other("manifest size"))
                        }
                    });
                    let reshade = name.to_lowercase().contains("reshade")
                        || bytes
                            .ok()
                            .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
                            .is_some_and(|v| v["layer"]["name"] == "VK_LAYER_reshade");
                    if reshade {
                        return Err(format!(
                            "发现外部 ReShade layer 注册，须先处理以避免重复 layer: {name}"
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
#[cfg(not(windows))]
impl Registry for UserRegistry {
    fn get(&self, _: &Path) -> Result<Option<u32>, String> {
        Err("仅支持 Windows".into())
    }
    fn set(&self, _: &Path, _: Option<u32>) -> Result<(), String> {
        Err("仅支持 Windows".into())
    }
    fn conflicts(&self, _: &Path) -> Result<(), String> {
        Err("仅支持 Windows".into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Target {
    record: GraphicsInstallationRecord,
    /// Only remove an untouched config created by this tool. Existing configs are never edited.
    created_config: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct State {
    schema: u32,
    dll: String,
    manifest: String,
    targets: Vec<Target>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    path: PathBuf,
    #[serde(with = "optional_bytes")]
    before: Option<Vec<u8>>,
    #[serde(with = "optional_bytes")]
    after: Option<Vec<u8>>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema: u32,
    before: Option<State>,
    after: Option<State>,
    registry_before: Option<u32>,
    registry_after: Option<u32>,
    changes: Vec<Change>,
}
fn json<T: Serialize>(v: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(v).map_err(|e| e.to_string())
}
fn state(root: &Path) -> Result<Option<State>, String> {
    let result: Option<State> = read_optional(&root.join("state.json"))?
        .map(|b| serde_json::from_slice(&b).map_err(|e| format!("Vulkan 安装记录损坏: {e}")))
        .transpose()?;
    if let Some(s) = &result {
        validate(s)?;
    }
    Ok(result)
}
fn validate(s: &State) -> Result<(), String> {
    let digest = |h: &str| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit());
    if s.schema != 1 || !digest(&s.dll) || !digest(&s.manifest) || s.targets.is_empty() {
        return Err("Vulkan 记录 schema 或摘要无效".into());
    }
    let mut seen = std::collections::HashSet::new();
    for t in &s.targets {
        let r = &t.record;
        safe_path(&r.executable)?;
        if r.graphics_api != GraphicsApi::Vulkan
            || r.executable.parent() != Some(r.target_directory.as_path())
            || r.relative_path != DLL
            || r.deployed_sha256 != s.dll
            || r.schema_version != 1
            || !digest(&r.package_sha256)
            || !seen.insert(r.executable.to_string_lossy().to_lowercase())
        {
            return Err("Vulkan 目标记录无效".into());
        }
    }
    Ok(())
}
pub(super) fn manifest(package: &Path) -> Result<Vec<u8>, String> {
    let mut zip = zip::ZipArchive::new(File::open(package).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut matches = Vec::new();
    for i in 0..zip.len() {
        if zip
            .by_index(i)
            .map_err(|e| e.to_string())?
            .name()
            .eq_ignore_ascii_case(MANIFEST)
        {
            matches.push(i);
        }
    }
    if matches.len() != 1 {
        return Err("官方包必须包含唯一 ReShade64.json".into());
    }
    let entry = zip.by_index(matches[0]).map_err(|e| e.to_string())?;
    if entry.size() > 65536 {
        return Err("layer manifest 过大".into());
    }
    let mut bytes = Vec::new();
    entry
        .take(65537)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if v["file_format_version"] != "1.0.0"
        || v["layer"]["name"] != "VK_LAYER_reshade"
        || v["layer"]["type"] != "GLOBAL"
        || v["layer"]["library_path"] != r".\ReShade64.dll"
        || v["layer"]["disable_environment"]["DISABLE_VK_LAYER_reshade_1"] != "1"
    {
        return Err("上游 Vulkan manifest 契约变化，拒绝部署".into());
    }
    Ok(bytes)
}
fn healthy(root: &Path, registry: &impl Registry, s: &Option<State>) -> Result<(), String> {
    if read_optional(&root.join("journal.json"))?.is_some() {
        return Err("Vulkan 存在未完成事务，请先修复".into());
    }
    registry.conflicts(&root.join(MANIFEST))?;
    let expected = s.as_ref();
    if file_hash(&root.join(DLL))?.as_ref() != expected.map(|s| &s.dll)
        || file_hash(&root.join(MANIFEST))?.as_ref() != expected.map(|s| &s.manifest)
        || registry.get(&root.join(MANIFEST))? != expected.map(|_| 0)
    {
        return Err("Vulkan 文件或注册信息与所有权记录不符，保留现状".into());
    }
    Ok(())
}
pub(super) fn affected_targets(exe: &Path) -> Result<Vec<PathBuf>, String> {
    let mut targets: Vec<PathBuf> = state(&root())?
        .map(|s| s.targets.into_iter().map(|t| t.record.executable).collect())
        .unwrap_or_default();
    if !targets.iter().any(|p| p == exe) {
        targets.push(exe.to_path_buf());
    }
    targets.sort();
    Ok(targets)
}

pub(super) fn snapshot(exe: &Path) -> Result<String, String> {
    let root = root();
    let s = state(&root)?;
    healthy(&root, &UserRegistry, &s)?;
    let config = read_optional(&exe.parent().ok_or("缺少目录")?.join("ReShade.ini"))?;
    Ok(hash(&json(&(s, config))?))
}
fn write_optional(path: &Path, value: &Option<Vec<u8>>) -> Result<(), String> {
    if let Some(bytes) = value {
        atomic_write(path, bytes)
    } else if read_optional(path)?.is_some() {
        fs::remove_file(path).map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}
struct Store<R: Registry> {
    root: PathBuf,
    registry: R,
    _lock: File,
}
impl<R: Registry> Store<R> {
    fn open(root: PathBuf, registry: R) -> Result<Self, String> {
        safe_path(&root)?;
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let path = root.join("lock");
        safe_path(&path)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        fs2::FileExt::try_lock_exclusive(&lock).map_err(|_| "其他 Vulkan 操作正在执行")?;
        Ok(Self {
            root,
            registry,
            _lock: lock,
        })
    }
    fn verify_journal(&self, j: &Journal) -> Result<(), String> {
        if j.schema != 1
            || (j.before.is_none() && j.after.is_none())
            || j.registry_before != j.before.as_ref().map(|_| 0)
            || j.registry_after != j.after.as_ref().map(|_| 0)
        {
            return Err("Vulkan 事务日志无效".into());
        }
        let mut allowed = vec![self.root.join(DLL), self.root.join(MANIFEST)];
        for s in [&j.before, &j.after].into_iter().flatten() {
            validate(s)?;
            for t in &s.targets {
                allowed.push(t.record.target_directory.join("ReShade.ini"));
                transaction::ensure_stopped(&t.record.target_directory)?;
            }
        }
        let mut seen = std::collections::HashSet::new();
        for c in &j.changes {
            safe_path(&c.path)?;
            if !allowed.contains(&c.path) || !seen.insert(&c.path) {
                return Err("事务包含非法写入路径".into());
            }
            for (s, bytes) in [(&j.before, &c.before), (&j.after, &c.after)] {
                let expected = if c.path == self.root.join(DLL) {
                    s.as_ref().map(|s| &s.dll)
                } else if c.path == self.root.join(MANIFEST) {
                    s.as_ref().map(|s| &s.manifest)
                } else {
                    continue;
                };
                if bytes.as_ref().map(|b| hash(b)).as_ref() != expected {
                    return Err("事务内容与摘要不一致".into());
                }
            }
            if c.path.file_name().is_some_and(|n| n == "ReShade.ini")
                && [&c.before, &c.after]
                    .into_iter()
                    .flatten()
                    .any(|b| b.as_slice() != CONFIG)
            {
                return Err("不允许通过恢复日志修改用户配置".into());
            }
        }
        Ok(())
    }
    fn repair(&self) -> Result<(), String> {
        let path = self.root.join("journal.json");
        let Some(bytes) = read_optional(&path)? else {
            return Ok(());
        };
        let j: Journal = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        self.verify_journal(&j)?;
        let current = state(&self.root)?;
        if current != j.before && current != j.after {
            return Err("记录被外部修改，停止恢复".into());
        }
        let reg = self.registry.get(&self.root.join(MANIFEST))?;
        if reg != j.registry_before && reg != j.registry_after {
            return Err("注册值被外部修改，停止恢复".into());
        }
        for c in &j.changes {
            let current = read_optional(&c.path)?;
            if current != c.before && current != c.after {
                return Err(format!("文件被外部修改，停止恢复: {}", c.path.display()));
            }
        }
        // Restore the prior registration and files; the journal makes partial recovery repeatable.
        self.registry
            .set(&self.root.join(MANIFEST), j.registry_before)?;
        for c in j.changes.iter().rev() {
            write_optional(&c.path, &c.before)?;
        }
        write_optional(
            &self.root.join("state.json"),
            &j.before.as_ref().map(json).transpose()?,
        )?;
        fs::remove_file(path).map_err(|e| e.to_string())
    }
    fn commit(
        &self,
        before: Option<State>,
        after: Option<State>,
        changes: Vec<Change>,
    ) -> Result<(), String> {
        let j = Journal {
            schema: 1,
            registry_before: before.as_ref().map(|_| 0),
            registry_after: after.as_ref().map(|_| 0),
            before,
            after,
            changes,
        };
        self.verify_journal(&j)?;
        healthy(&self.root, &self.registry, &j.before)?;
        for c in &j.changes {
            if read_optional(&c.path)? != c.before {
                return Err("文件在计划后发生变化".into());
            }
        }
        let path = self.root.join("journal.json");
        atomic_write(&path, &json(&j)?)?;
        let result = (|| {
            if j.after.is_none() {
                self.registry.set(&self.root.join(MANIFEST), None)?;
            }
            for c in &j.changes {
                write_optional(&c.path, &c.after)?;
            }
            self.registry
                .set(&self.root.join(MANIFEST), j.registry_after)?;
            write_optional(
                &self.root.join("state.json"),
                &j.after.as_ref().map(json).transpose()?,
            )?;
            for c in &j.changes {
                if read_optional(&c.path)? != c.after {
                    return Err("部署后文件校验失败".into());
                }
            }
            if self.registry.get(&self.root.join(MANIFEST))? != j.registry_after {
                return Err("注册校验失败".into());
            }
            fs::remove_file(&path).map_err(|e| e.to_string())
        })();
        if let Err(error) = result {
            return Err(match self.repair() {
                Ok(()) => format!("{error}；已回滚"),
                Err(e) => format!("{error}；恢复未完成: {e}"),
            });
        }
        Ok(())
    }
    fn install(
        &self,
        record: GraphicsInstallationRecord,
        dll: Vec<u8>,
        manifest: Vec<u8>,
    ) -> Result<(), String> {
        let before = state(&self.root)?;
        healthy(&self.root, &self.registry, &before)?;
        let mut after = before.clone().unwrap_or(State {
            schema: 1,
            dll: hash(&dll),
            manifest: hash(&manifest),
            targets: vec![],
        });
        // The preview lists all references; the explicit scope confirmation covers the shared update.
        for target in &mut after.targets {
            target.record.version = record.version.clone();
            target.record.source_url = record.source_url.clone();
            target.record.package_sha256 = record.package_sha256.clone();
            target.record.deployed_sha256 = record.deployed_sha256.clone();
        }
        let config_path = record.target_directory.join("ReShade.ini");
        let config = read_optional(&config_path)?;
        let previous = after
            .targets
            .iter()
            .find(|t| t.record.executable == record.executable);
        let created_config = previous.map_or(config.is_none(), |t| t.created_config);
        after
            .targets
            .retain(|t| t.record.executable != record.executable);
        after.targets.push(Target {
            record,
            created_config,
        });
        after.dll = hash(&dll);
        after.manifest = hash(&manifest);
        let mut changes = vec![];
        for (name, bytes) in [(DLL, dll), (MANIFEST, manifest)] {
            changes.push(Change {
                path: self.root.join(name),
                before: read_optional(&self.root.join(name))?,
                after: Some(bytes),
            });
        }
        if config.is_none() {
            changes.push(Change {
                path: config_path,
                before: None,
                after: Some(CONFIG.to_vec()),
            });
        }
        self.commit(before, Some(after), changes)
    }
    fn uninstall(&self, exe: &Path) -> Result<(), String> {
        let before = state(&self.root)?.ok_or("没有 Vulkan 安装记录")?;
        healthy(&self.root, &self.registry, &Some(before.clone()))?;
        let target = before
            .targets
            .iter()
            .find(|t| t.record.executable == exe)
            .ok_or("目标未引用此 layer")?;
        if feeder::has_record_or_pending(exe.parent().ok_or("缺少目标目录")?)? {
            return Err("请先卸载或修复 Feeder".into());
        }
        for addon in ["dlss5-feed.addon64", "renodx-dlss5.addon64"] {
            if target.record.target_directory.join(addon).exists() {
                return Err("请先处理依赖 ReShade 的 Feeder".into());
            }
        }
        let mut after = before.clone();
        after.targets.retain(|t| t.record.executable != exe);
        let mut changes = vec![];
        let config_path = target.record.target_directory.join("ReShade.ini");
        // Multiple executables in one directory share their config.
        if target.created_config
            && !after
                .targets
                .iter()
                .any(|t| t.record.target_directory == target.record.target_directory)
            && read_optional(&config_path)?.as_deref() == Some(CONFIG)
        {
            changes.push(Change {
                path: config_path,
                before: Some(CONFIG.to_vec()),
                after: None,
            });
        } else if target.created_config {
            for t in &mut after.targets {
                if t.record.target_directory == target.record.target_directory {
                    t.created_config = true;
                }
            }
        }
        let after = if after.targets.is_empty() {
            for name in [DLL, MANIFEST] {
                changes.push(Change {
                    path: self.root.join(name),
                    before: read_optional(&self.root.join(name))?,
                    after: None,
                });
            }
            None
        } else {
            Some(after)
        };
        self.commit(Some(before), after, changes)
    }
}
pub(super) fn install(
    record: GraphicsInstallationRecord,
    dll: Vec<u8>,
    manifest: Vec<u8>,
    expected: &str,
) -> Result<GraphicsOperationResult, String> {
    let store = Store::open(root(), UserRegistry)?;
    if snapshot(&record.executable)? != expected {
        return Err("Vulkan 状态或配置变化，请重新预检".into());
    }
    store.install(record.clone(), dll, manifest)?;
    Ok(GraphicsOperationResult {
        record: Some(record),
        message: format!("Vulkan layer 已部署并注册。{SCOPE} 运行加载尚未验证。"),
        compatibility_verified: false,
    })
}
pub fn remove_or_repair(
    executable: PathBuf,
    repair: bool,
) -> Result<GraphicsOperationResult, String> {
    transaction::target_directory(&executable)?;
    let exe = executable.canonicalize().map_err(|e| e.to_string())?;
    if !root().exists() {
        return Err("没有 Vulkan 安装记录".into());
    }
    let store = Store::open(root(), UserRegistry)?;
    if repair {
        store.repair()?;
    } else {
        store.uninstall(&exe)?;
    }
    Ok(GraphicsOperationResult {
        record: read_record(&exe)?,
        message:
            "Vulkan 操作完成；其他目标引用及用户编辑的配置保留。保留的配置仍可能被共享 layer 加载。"
                .into(),
        compatibility_verified: false,
    })
}
pub fn read_record(exe: &Path) -> Result<Option<GraphicsInstallationRecord>, String> {
    Ok(state(&root())?.and_then(|s| {
        s.targets
            .into_iter()
            .find(|t| t.record.executable == exe)
            .map(|t| t.record)
    }))
}
pub(super) fn detect(report: &mut GraphicsComponentDetection) -> Result<(), String> {
    inspect(report, &root(), &UserRegistry)
}
fn inspect(
    report: &mut GraphicsComponentDetection,
    root: &Path,
    registry: &impl Registry,
) -> Result<(), String> {
    report.diagnostics.push(SCOPE.into());
    report.installation_available = false;
    let s = state(root)?;
    if read_optional(&root.join("journal.json"))?.is_some() {
        report.reshade_state = GraphicsComponentState::Incomplete;
        report
            .diagnostics
            .push("共享 layer 存在未完成事务，请先修复。".into());
        return Ok(());
    }
    if let Err(error) = registry.conflicts(&root.join(MANIFEST)) {
        report.reshade_state = GraphicsComponentState::External;
        report.diagnostics.push(error);
        return Ok(());
    }
    let dll = file_hash(&root.join(DLL))?;
    let manifest = file_hash(&root.join(MANIFEST))?;
    let registration = registry.get(&root.join(MANIFEST))?;
    let config = read_optional(
        &report
            .executable
            .parent()
            .ok_or("目标没有目录")?
            .join("ReShade.ini"),
    )?;
    if let Some(s) = s {
        if dll.is_none() || manifest.is_none() || registration.is_none() {
            report.reshade_state = GraphicsComponentState::Incomplete;
        } else if dll.as_ref() != Some(&s.dll)
            || manifest.as_ref() != Some(&s.manifest)
            || registration != Some(0)
        {
            report.reshade_state = GraphicsComponentState::Modified;
        } else {
            let owned = s
                .targets
                .iter()
                .any(|t| t.record.executable == report.executable);
            report.reshade_state = match (owned, config.is_some()) {
                (true, true) => GraphicsComponentState::Installed,
                (true, false) => GraphicsComponentState::Incomplete,
                (false, true) => GraphicsComponentState::External,
                (false, false) => GraphicsComponentState::NotInstalled,
            };
            report.installation_available = true;
        }
    } else if dll.is_some() || manifest.is_some() || registration.is_some() {
        report.reshade_state = GraphicsComponentState::External;
        report
            .diagnostics
            .push("共享目录或注册值已有非本工具拥有的内容，禁止覆盖。".into());
    } else {
        report.reshade_state = GraphicsComponentState::NotInstalled;
        report.installation_available = true;
    }
    Ok(())
}

#[cfg(test)]
pub(super) mod tests;

/// Held during Feeder edits so a shared Vulkan operation cannot replace ReShade concurrently.
pub(super) fn lock_for_feeder() -> Result<impl Sized, String> {
    Store::open(root(), UserRegistry)
}
