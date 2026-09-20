use super::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Clone, Default)]
struct FakeRegistry {
    value: Rc<Cell<Option<u32>>>,
    fail_once: Rc<Cell<bool>>,
    conflict: Rc<Cell<bool>>,
}
impl Registry for FakeRegistry {
    fn get(&self, _: &Path) -> Result<Option<u32>, String> {
        Ok(self.value.get())
    }
    fn set(&self, _: &Path, value: Option<u32>) -> Result<(), String> {
        if self.fail_once.replace(false) {
            return Err("injected registry failure".into());
        }
        self.value.set(value);
        Ok(())
    }
    fn conflicts(&self, _: &Path) -> Result<(), String> {
        if self.conflict.get() {
            Err("external layer".into())
        } else {
            Ok(())
        }
    }
}
fn target(base: &Path, name: &str, dll: &[u8]) -> GraphicsInstallationRecord {
    let directory = base.join(name);
    fs::create_dir_all(&directory).unwrap();
    let exe = directory.join("emu.exe");
    fs::write(&exe, b"test").unwrap();
    let executable = exe.canonicalize().unwrap();
    GraphicsInstallationRecord {
        schema_version: 1,
        target_directory: executable.parent().unwrap().into(),
        executable,
        graphics_api: GraphicsApi::Vulkan,
        version: Some("6.8.0".into()),
        source_url: None,
        package_sha256: hash(b"package"),
        deployed_sha256: hash(dll),
        original_sha256: None,
        relative_path: DLL.into(),
    }
}
#[test]
fn references_preserve_shared_layer_and_user_config() {
    let dir = tempfile::tempdir().unwrap();
    let registry = FakeRegistry::default();
    let store = Store::open(dir.path().join("shared"), registry.clone()).unwrap();
    let a = target(dir.path(), "a", b"dll");
    let b = target(dir.path(), "b", b"dll");
    let config_b = b.target_directory.join("ReShade.ini");
    fs::write(&config_b, b"user configuration").unwrap();
    store
        .install(a.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap();
    store
        .install(b.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap();
    store
        .install(a.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap();
    assert_eq!(state(&store.root).unwrap().unwrap().targets.len(), 2);
    assert!(Store::open(store.root.clone(), registry.clone()).is_err());
    store.uninstall(&a.executable).unwrap();
    assert_eq!(registry.value.get(), Some(0));
    assert!(store.root.join(DLL).exists());
    assert!(!a.target_directory.join("ReShade.ini").exists());
    store.uninstall(&b.executable).unwrap();
    assert_eq!(registry.value.get(), None);
    assert!(!store.root.join(DLL).exists());
    assert_eq!(fs::read(config_b).unwrap(), b"user configuration");
}
#[test]
fn shared_upgrade_and_external_changes_are_protected() {
    let dir = tempfile::tempdir().unwrap();
    let registry = FakeRegistry::default();
    let store = Store::open(dir.path().join("shared"), registry.clone()).unwrap();
    let a = target(dir.path(), "a", b"dll");
    let b = target(dir.path(), "b", b"dll");
    store
        .install(a.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap();
    store
        .install(b.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap();
    let mut upgrade = a.clone();
    upgrade.deployed_sha256 = hash(b"new");
    store
        .install(upgrade, b"new".to_vec(), b"manifest".to_vec())
        .unwrap();
    let shared = state(&store.root).unwrap().unwrap();
    assert_eq!(shared.targets.len(), 2);
    assert!(shared
        .targets
        .iter()
        .all(|t| t.record.deployed_sha256 == hash(b"new")));
    store.uninstall(&b.executable).unwrap();
    fs::write(store.root.join(DLL), b"external modification").unwrap();
    assert!(store.uninstall(&a.executable).is_err());
    assert_eq!(registry.value.get(), Some(0));
    fs::write(store.root.join(DLL), b"new").unwrap();
    registry.value.set(Some(1));
    assert!(store.uninstall(&a.executable).is_err());
    registry.value.set(Some(0));
    fs::write(a.target_directory.join("ReShade.ini"), b"user edits").unwrap();
    store.uninstall(&a.executable).unwrap();
    assert_eq!(
        fs::read(a.target_directory.join("ReShade.ini")).unwrap(),
        b"user edits"
    );
}
#[test]
fn registration_failure_rolls_back_all_files() {
    let dir = tempfile::tempdir().unwrap();
    let registry = FakeRegistry::default();
    let store = Store::open(dir.path().join("shared"), registry.clone()).unwrap();
    let a = target(dir.path(), "a", b"dll");
    registry.fail_once.set(true);
    assert!(store
        .install(a.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap_err()
        .contains("已回滚"));
    assert_eq!(registry.value.get(), None);
    assert!(state(&store.root).unwrap().is_none());
    assert!(!store.root.join(DLL).exists());
    assert!(!a.target_directory.join("ReShade.ini").exists());
    assert!(!store.root.join("journal.json").exists());
    registry.conflict.set(true);
    assert!(store
        .install(a, b"dll".to_vec(), b"manifest".to_vec())
        .is_err());
}
#[test]
fn interrupted_transaction_recovery_is_repeatable_and_rejects_tampering() {
    for boundary in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        let registry = FakeRegistry::default();
        let store = Store::open(dir.path().join("shared"), registry.clone()).unwrap();
        let a = target(dir.path(), "a", b"dll");
        let after = State {
            schema: 1,
            dll: hash(b"dll"),
            manifest: hash(b"manifest"),
            targets: vec![Target {
                record: a.clone(),
                created_config: true,
            }],
        };
        let j = Journal {
            schema: 1,
            before: None,
            after: Some(after.clone()),
            registry_before: None,
            registry_after: Some(0),
            changes: vec![
                Change {
                    path: store.root.join(DLL),
                    before: None,
                    after: Some(b"dll".to_vec()),
                },
                Change {
                    path: store.root.join(MANIFEST),
                    before: None,
                    after: Some(b"manifest".to_vec()),
                },
                Change {
                    path: a.target_directory.join("ReShade.ini"),
                    before: None,
                    after: Some(CONFIG.to_vec()),
                },
            ],
        };
        atomic_write(&store.root.join("journal.json"), &json(&j).unwrap()).unwrap();
        for c in j.changes.iter().take(boundary.min(3)) {
            write_optional(&c.path, &c.after).unwrap();
        }
        if boundary >= 3 {
            registry.value.set(Some(0));
        }
        if boundary >= 4 {
            atomic_write(&store.root.join("state.json"), &json(&after).unwrap()).unwrap();
        }
        if boundary == 2 {
            fs::write(store.root.join(DLL), b"user edit").unwrap();
            assert!(store.repair().is_err());
            assert!(store.root.join("journal.json").exists());
            fs::write(store.root.join(DLL), b"dll").unwrap();
        }
        store.repair().unwrap();
        store.repair().unwrap();
        assert_eq!(registry.value.get(), None);
        assert!(state(&store.root).unwrap().is_none());
        assert!(!store.root.join(DLL).exists());
        assert!(!a.target_directory.join("ReShade.ini").exists());
    }
}
#[test]
fn shared_directory_config_ownership_transfers() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("shared"), FakeRegistry::default()).unwrap();
    let a = target(dir.path(), "a", b"dll");
    let mut b = a.clone();
    b.executable = a.target_directory.join("second.exe");
    store
        .install(a.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap();
    store
        .install(b.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap();
    store.uninstall(&a.executable).unwrap();
    assert!(a.target_directory.join("ReShade.ini").exists());
    store.uninstall(&b.executable).unwrap();
    assert!(!a.target_directory.join("ReShade.ini").exists());
}

/// Real official archive, isolated filesystem and fake registry; never register a layer on the host.
pub(in crate::services::graphics_components) fn official_roundtrip(package: &Path) {
    let dll = planning::extract_dll(package).unwrap();
    let manifest = manifest(package).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let registry = FakeRegistry::default();
    let store = Store::open(dir.path().join("shared"), registry.clone()).unwrap();
    let a = target(dir.path(), "a", &dll);
    store
        .install(a.clone(), dll.clone(), manifest.clone())
        .unwrap();
    assert_eq!(fs::read(store.root.join(DLL)).unwrap(), dll);
    assert_eq!(fs::read(store.root.join(MANIFEST)).unwrap(), manifest);
    assert_eq!(registry.value.get(), Some(0));
    store.uninstall(&a.executable).unwrap();
    assert_eq!(registry.value.get(), None);
    assert!(!store.root.join(MANIFEST).exists());
}

#[test]
fn manifest_contract_rejects_missing_duplicate_and_unsafe_library() {
    use std::io::{Cursor, Write};
    let valid = serde_json::json!({"file_format_version":"1.0.0", "layer": {
        "name":"VK_LAYER_reshade", "type":"GLOBAL", "library_path":r".\ReShade64.dll",
        "disable_environment":{"DISABLE_VK_LAYER_reshade_1":"1"}
    }});
    for (count, library, accepted) in [
        (1, r".\ReShade64.dll", true),
        (0, r".\ReShade64.dll", false),
        (2, r".\ReShade64.dll", false),
        (1, r"..\external.dll", false),
    ] {
        let mut v = valid.clone();
        v["layer"]["library_path"] = library.into();
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..count {
            zip.start_file(
                if i == 0 { MANIFEST } else { "reshade64.JSON" },
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(&json(&v).unwrap()).unwrap();
        }
        let file = tempfile::NamedTempFile::new().unwrap();
        fs::write(file.path(), zip.finish().unwrap().into_inner()).unwrap();
        assert_eq!(manifest(file.path()).is_ok(), accepted);
    }
}

#[test]
fn running_shared_target_blocks_all_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("shared"), FakeRegistry::default()).unwrap();
    let mut a = target(dir.path(), "a", b"dll");
    a.executable = std::env::current_exe().unwrap().canonicalize().unwrap();
    a.target_directory = a.executable.parent().unwrap().to_path_buf();
    assert!(store
        .install(a, b"dll".to_vec(), b"manifest".to_vec())
        .is_err());
    assert!(!store.root.join("journal.json").exists());
    assert!(!store.root.join(DLL).exists());
}

#[test]
fn interrupted_last_uninstall_restores_registration_files_and_reference() {
    for boundary in 0..5 {
        let dir = tempfile::tempdir().unwrap();
        let registry = FakeRegistry::default();
        let store = Store::open(dir.path().join("shared"), registry.clone()).unwrap();
        let a = target(dir.path(), "a", b"dll");
        store
            .install(a.clone(), b"dll".to_vec(), b"manifest".to_vec())
            .unwrap();
        let before = state(&store.root).unwrap();
        let paths = [
            a.target_directory.join("ReShade.ini"),
            store.root.join(DLL),
            store.root.join(MANIFEST),
        ];
        let changes = paths
            .into_iter()
            .map(|path| Change {
                before: read_optional(&path).unwrap(),
                path,
                after: None,
            })
            .collect();
        let j = Journal {
            schema: 1,
            before: before.clone(),
            after: None,
            registry_before: Some(0),
            registry_after: None,
            changes,
        };
        atomic_write(&store.root.join("journal.json"), &json(&j).unwrap()).unwrap();
        registry.value.set(None);
        for c in j.changes.iter().take(boundary.min(3)) {
            write_optional(&c.path, &c.after).unwrap();
        }
        if boundary >= 4 {
            fs::remove_file(store.root.join("state.json")).unwrap();
        }
        store.repair().unwrap();
        store.repair().unwrap();
        assert_eq!(state(&store.root).unwrap(), before);
        assert_eq!(registry.value.get(), Some(0));
        assert_eq!(fs::read(store.root.join(DLL)).unwrap(), b"dll");
        assert_eq!(
            fs::read(a.target_directory.join("ReShade.ini")).unwrap(),
            CONFIG
        );
    }
}

#[test]
fn detection_distinguishes_owned_modified_missing_and_external_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let registry = FakeRegistry::default();
    let store = Store::open(dir.path().join("shared"), registry.clone()).unwrap();
    let a = target(dir.path(), "a", b"dll");
    let mut report = GraphicsComponentDetection {
        executable: a.executable.clone(),
        architecture: ExecutableArchitecture::X64,
        graphics_api: Some(GraphicsApi::Vulkan),
        supported_target: true,
        installation_available: false,
        reshade_state: GraphicsComponentState::Unknown,
        feeder_state: GraphicsComponentState::Unknown,
        compatibility_verified: false,
        evidence: vec![],
        diagnostics: vec![],
    };
    inspect(&mut report, &store.root, &registry).unwrap();
    assert_eq!(report.reshade_state, GraphicsComponentState::NotInstalled);
    assert_eq!(fs::read_dir(&store.root).unwrap().count(), 1);
    store
        .install(a.clone(), b"dll".to_vec(), b"manifest".to_vec())
        .unwrap();
    inspect(&mut report, &store.root, &registry).unwrap();
    assert_eq!(report.reshade_state, GraphicsComponentState::Installed);
    fs::write(store.root.join(DLL), b"changed").unwrap();
    inspect(&mut report, &store.root, &registry).unwrap();
    assert_eq!(report.reshade_state, GraphicsComponentState::Modified);
    assert!(!report.installation_available);
    fs::remove_file(store.root.join(DLL)).unwrap();
    inspect(&mut report, &store.root, &registry).unwrap();
    assert_eq!(report.reshade_state, GraphicsComponentState::Incomplete);
    registry.conflict.set(true);
    inspect(&mut report, &store.root, &registry).unwrap();
    assert_eq!(report.reshade_state, GraphicsComponentState::External);
    assert!(!report.compatibility_verified);
}
