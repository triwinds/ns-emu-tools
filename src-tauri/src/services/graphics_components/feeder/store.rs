//! Journaled multi-file deployment. Only fixed component paths are writable.
use super::*;
use serde::{Deserialize, Serialize};
use transaction::{atomic_write, file_hash, hash, read_optional, safe_path};
const RECORD: &str = ".ns-emu-tools-feeder/record.json";
const STATE: &str = ".ns-emu-tools-feeder";
type Snapshot = BTreeMap<String, Option<String>>;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnedFile {
    pub original: Option<String>,
    pub deployed: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub schema_version: u32,
    pub bundle: String,
    pub executable: PathBuf,
    pub graphics_api: GraphicsApi,
    pub files: BTreeMap<String, OwnedFile>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    schema_version: u32,
    before: Snapshot,
    after: Snapshot,
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn allowed(path: &str) -> bool {
    path == RECORD || FILES.contains(&path)
}
fn validate_snapshot(snapshot: &Snapshot) -> Result<(), String> {
    if snapshot.is_empty()
        || snapshot.len() > FILES.len() + 1
        || snapshot
            .iter()
            .any(|(p, h)| !allowed(p) || h.as_deref().is_some_and(|h| !valid_hash(h)))
    {
        return Err("Feeder 事务包含未知路径或无效摘要".into());
    }
    Ok(())
}
pub fn record(dir: &Path) -> Result<Option<Record>, String> {
    let Some(bytes) = read_optional(&dir.join(RECORD))? else {
        return Ok(None);
    };
    let record: Record =
        serde_json::from_slice(&bytes).map_err(|e| format!("Feeder 安装记录损坏: {e}"))?;
    if record.schema_version != 1
        || record.bundle != BUNDLE
        || record.executable.parent() != Some(dir)
        || record.files.len() != FILES.len()
        || FILES.iter().any(|p| !record.files.contains_key(*p))
        || record.files.values().any(|f| {
            !valid_hash(&f.deployed) || f.original.as_deref().is_some_and(|h| !valid_hash(h))
        })
    {
        return Err("Feeder 安装记录的 schema、路径或摘要无效".into());
    }
    Ok(Some(record))
}
pub fn pending(dir: &Path) -> Result<bool, String> {
    Ok(read_optional(&dir.join(STATE).join("journal.json"))?.is_some())
}
pub(super) fn snapshot(dir: &Path) -> Result<Snapshot, String> {
    FILES
        .iter()
        .copied()
        .chain(std::iter::once(RECORD))
        .map(|p| Ok((p.into(), file_hash(&dir.join(p))?)))
        .collect()
}
struct Store {
    dir: PathBuf,
    state: PathBuf,
}
impl Store {
    fn new(dir: &Path) -> Result<Self, String> {
        let state = dir.join(STATE);
        safe_path(&state)?;
        fs::create_dir_all(&state).map_err(|e| e.to_string())?;
        Ok(Self {
            dir: dir.into(),
            state,
        })
    }
    fn blob(&self, h: &str) -> Result<PathBuf, String> {
        if !valid_hash(h) {
            return Err("Feeder 备份摘要无效".into());
        }
        Ok(self.state.join(format!("{h}.blob")))
    }
    fn save(&self, bytes: &[u8]) -> Result<String, String> {
        let h = hash(bytes);
        let path = self.blob(&h)?;
        match file_hash(&path)? {
            Some(existing) if existing != h => return Err("Feeder 备份已损坏".into()),
            Some(_) => {}
            None => atomic_write(&path, bytes)?,
        }
        Ok(h)
    }
    fn load(&self, h: &str) -> Result<Vec<u8>, String> {
        let bytes = read_optional(&self.blob(h)?)?.ok_or("缺少 Feeder 恢复备份")?;
        if hash(&bytes) != h {
            return Err("Feeder 恢复备份摘要不匹配".into());
        }
        Ok(bytes)
    }
    fn write(&self, path: &str, value: &Option<String>) -> Result<(), String> {
        if !allowed(path) {
            return Err("Feeder 写入路径无效".into());
        }
        let dest = self.dir.join(path);
        safe_path(&dest)?;
        if let Some(h) = value {
            let bytes = self.load(h)?;
            fs::create_dir_all(dest.parent().ok_or("缺少父目录")?).map_err(|e| e.to_string())?;
            atomic_write(&dest, &bytes)
        } else {
            match fs::remove_file(&dest) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            }
        }
    }
    fn validate(&self, j: &Journal) -> Result<(), String> {
        if j.schema_version != 1 || j.before.keys().ne(j.after.keys()) {
            return Err("Feeder 事务 schema 或路径不一致".into());
        }
        validate_snapshot(&j.before)?;
        validate_snapshot(&j.after)?;
        // Validate every destination and backup before changing the first file.
        for (p, before) in &j.before {
            let current = file_hash(&self.dir.join(p))?;
            if current != *before && current != j.after[p] {
                return Err(format!("Feeder 恢复冲突，文件已被外部修改: {p}"));
            }
            for h in [before.as_ref(), j.after[p].as_ref()].into_iter().flatten() {
                self.load(h)?;
            }
        }
        Ok(())
    }
    fn repair(&self) -> Result<(), String> {
        let path = self.state.join("journal.json");
        let Some(bytes) = read_optional(&path)? else {
            return Ok(());
        };
        let j: Journal = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        self.validate(&j)?;
        for (p, before) in &j.before {
            let current = file_hash(&self.dir.join(p))?;
            if current != *before && current != j.after[p] {
                return Err(format!("Feeder 恢复时发生外部修改: {p}"));
            }
            if current != *before {
                self.write(p, before)?;
            }
        }
        fs::remove_file(path).map_err(|e| e.to_string())
    }
    fn commit(&self, before: Snapshot, after: Snapshot) -> Result<(), String> {
        let j = Journal {
            schema_version: 1,
            before,
            after,
        };
        self.validate(&j)?;
        for (p, b) in &j.before {
            if file_hash(&self.dir.join(p))? != *b {
                return Err(format!("Feeder 目标已变化: {p}"));
            }
        }
        let path = self.state.join("journal.json");
        atomic_write(&path, &serde_json::to_vec(&j).map_err(|e| e.to_string())?)?;
        let result = (|| {
            for (p, a) in &j.after {
                if file_hash(&self.dir.join(p))? != j.before[p] {
                    return Err(format!("Feeder 写入前发生外部修改: {p}"));
                }
                if *a != j.before[p] {
                    self.write(p, a)?;
                }
            }
            fs::remove_file(&path).map_err(|e| e.to_string())
        })();
        if let Err(e) = result {
            return match self.repair() {
                Ok(()) => Err(format!("{e}；已回滚")),
                Err(r) => Err(format!("{e}；恢复待处理: {r}")),
            };
        }
        Ok(())
    }
}
pub(super) fn deploy(
    exe: &Path,
    api: GraphicsApi,
    files: BTreeMap<String, Vec<u8>>,
    expected: &Snapshot,
) -> Result<(), String> {
    let dir = exe.parent().ok_or("缺少目标目录")?;
    if pending(dir)? || snapshot(dir)? != *expected {
        return Err("Feeder 目标已变化或存在未完成事务，请重新预检".into());
    }
    if files.len() != FILES.len() || FILES.iter().any(|p| !files.contains_key(*p)) {
        return Err("Feeder 文件清单不完整".into());
    }
    let old = record(dir)?;
    let store = Store::new(dir)?;
    let mut owned = BTreeMap::new();
    let before = snapshot(dir)?;
    let mut after = before.clone();
    for (p, bytes) in files {
        let current = read_optional(&dir.join(&p))?;
        let original = if let Some(previous) = old.as_ref().and_then(|r| r.files.get(&p)) {
            if current.as_deref().map(hash).as_ref() != Some(&previous.deployed) {
                return Err(format!("已安装文件被修改，拒绝覆盖: {p}"));
            }
            if let Some(h) = &previous.original {
                store.load(h)?;
            }
            previous.original.clone()
        } else {
            if p != "ReShade.ini" && current.is_some() {
                return Err(format!("外部文件冲突: {p}"));
            }
            current.as_deref().map(|b| store.save(b)).transpose()?
        };
        if let Some(b) = current {
            store.save(&b)?;
        }
        let deployed = store.save(&bytes)?;
        after.insert(p.clone(), Some(deployed.clone()));
        owned.insert(p, OwnedFile { original, deployed });
    }
    if let Some(b) = read_optional(&dir.join(RECORD))? {
        store.save(&b)?;
    }
    let record = Record {
        schema_version: 1,
        bundle: BUNDLE.into(),
        executable: exe.into(),
        graphics_api: api,
        files: owned,
    };
    after.insert(
        RECORD.into(),
        Some(store.save(&serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?)?),
    );
    store.commit(before, after)
}
pub(super) fn remove_or_repair(dir: &Path, repair: bool) -> Result<Vec<String>, String> {
    let store = Store::new(dir)?;
    if repair {
        store.repair()?;
        return Ok(vec![]);
    }
    if pending(dir)? {
        return Err("请先恢复未完成的 Feeder 事务".into());
    }
    let Some(record) = record(dir)? else {
        return Err("没有 Feeder 安装记录".into());
    };
    let mut before = Snapshot::new();
    let mut after = Snapshot::new();
    let mut preserved = vec![];
    for (p, owned) in record.files {
        let current = read_optional(&dir.join(&p))?;
        let h = current.as_deref().map(hash);
        if h.as_deref() != Some(owned.deployed.as_str()) && h.is_some() {
            if p == "ReShade.ini" || p == PRESET {
                preserved.push(p);
                continue;
            }
            return Err(format!("Feeder 文件已被修改，保留并拒绝卸载: {p}"));
        }
        if let Some(b) = current {
            store.save(&b)?;
        }
        if let Some(h) = &owned.original {
            store.load(h)?;
        }
        before.insert(p.clone(), h);
        after.insert(p, owned.original);
    }
    let bytes = read_optional(&dir.join(RECORD))?.ok_or("记录缺失")?;
    before.insert(RECORD.into(), Some(store.save(&bytes)?));
    after.insert(RECORD.into(), None);
    store.commit(before, after)?;
    Ok(preserved)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupted_transaction_restores_and_rejects_tampering() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path()).unwrap();
        let old = store.save(b"old").unwrap();
        let new = store.save(b"new").unwrap();
        let j = Journal {
            schema_version: 1,
            before: [("nvngx_dlss.dll".into(), Some(old.clone()))].into(),
            after: [("nvngx_dlss.dll".into(), Some(new.clone()))].into(),
        };
        atomic_write(
            &store.state.join("journal.json"),
            &serde_json::to_vec(&j).unwrap(),
        )
        .unwrap();
        fs::write(dir.path().join("nvngx_dlss.dll"), b"external").unwrap();
        assert!(store.repair().is_err());
        fs::write(dir.path().join("nvngx_dlss.dll"), b"new").unwrap();
        store.repair().unwrap();
        assert_eq!(fs::read(dir.path().join("nvngx_dlss.dll")).unwrap(), b"old");
        assert!(validate_snapshot(&[("../bad".into(), Some(old))].into()).is_err());
    }
    #[test]
    fn uninstall_preserves_edited_config_and_restores_original() {
        for edited in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let exe = dir.path().join("emu.exe");
            fs::write(&exe, b"exe").unwrap();
            fs::write(dir.path().join("ReShade.ini"), b"original").unwrap();
            let payload = FILES
                .iter()
                .map(|p| (p.to_string(), b"installed".to_vec()))
                .collect();
            deploy(
                &exe,
                GraphicsApi::Vulkan,
                payload,
                &snapshot(dir.path()).unwrap(),
            )
            .unwrap();
            if edited {
                fs::write(dir.path().join("ReShade.ini"), b"user edit").unwrap();
            }
            let preserved = remove_or_repair(dir.path(), false).unwrap();
            assert_eq!(preserved.is_empty(), !edited);
            assert_eq!(
                fs::read(dir.path().join("ReShade.ini")).unwrap(),
                if edited {
                    b"user edit".to_vec()
                } else {
                    b"original".to_vec()
                }
            );
            assert!(!dir.path().join("nvngx_dlss.dll").exists());
            assert!(record(dir.path()).unwrap().is_none());
        }
    }
}
