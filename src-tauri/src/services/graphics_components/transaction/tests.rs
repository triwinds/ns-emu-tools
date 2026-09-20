use super::*;

fn record(directory: &Path, bytes: &[u8]) -> GraphicsInstallationRecord {
    GraphicsInstallationRecord {
        schema_version: 1,
        target_directory: directory.into(),
        executable: directory.join("emu.exe"),
        graphics_api: GraphicsApi::OpenGl,
        version: Some("test".into()),
        source_url: None,
        package_sha256: hash(b"package"),
        deployed_sha256: hash(bytes),
        original_sha256: None,
        relative_path: "opengl32.dll".into(),
    }
}

fn install(
    store: &Store,
    bytes: &[u8],
    confirmed: bool,
) -> Result<GraphicsOperationResult, String> {
    store.install(
        record(&store.directory, bytes),
        bytes,
        file_hash(&store.directory.join("opengl32.dll"))?,
        read_record(&store.directory)?,
        confirmed,
    )
}

#[test]
fn upgrade_and_uninstall_preserve_first_backup_and_user_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    fs::write(directory.join("opengl32.dll"), b"original").unwrap();
    fs::write(directory.join("ReShadePreset.ini"), b"user preset").unwrap();
    let store = Store::open(&directory).unwrap();
    assert!(install(&store, b"v1", false).is_err());
    assert!(!pending(&directory).unwrap());
    install(&store, b"v1", true).unwrap();
    install(&store, b"v2", false).unwrap();
    install(&store, b"v2", false).unwrap();
    assert_eq!(
        read_record(&directory).unwrap().unwrap().original_sha256,
        Some(hash(b"original"))
    );
    store.uninstall().unwrap();
    assert_eq!(
        fs::read(directory.join("opengl32.dll")).unwrap(),
        b"original"
    );
    assert_eq!(
        fs::read(directory.join("ReShadePreset.ini")).unwrap(),
        b"user preset"
    );
    assert!(read_record(&directory).unwrap().is_none());
}

#[test]
fn clean_uninstall_and_modified_file_protection() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    let store = Store::open(&directory).unwrap();
    install(&store, b"v1", false).unwrap();
    fs::write(directory.join("opengl32.dll"), b"user edit").unwrap();
    assert!(install(&store, b"v2", true).is_err());
    assert!(store.uninstall().is_err());
    assert_eq!(
        fs::read(directory.join("opengl32.dll")).unwrap(),
        b"user edit"
    );
    fs::write(directory.join("opengl32.dll"), b"v1").unwrap();
    store.uninstall().unwrap();
    assert!(!directory.join("opengl32.dll").exists());
}

#[test]
fn crash_at_each_transaction_boundary_rolls_back_idempotently() {
    for boundary in 0..3 {
        let dir = tempfile::tempdir().unwrap();
        let directory = dir.path().canonicalize().unwrap();
        let store = Store::open(&directory).unwrap();
        install(&store, b"old", false).unwrap();
        let old_record = read_record(&directory).unwrap();
        let new_record = Some(record(&directory, b"new"));
        let journal = Journal {
            schema_version: 1,
            before: Some(store.save_blob(b"old").unwrap()),
            after: Some(store.save_blob(b"new").unwrap()),
            old_record: old_record.clone(),
            new_record: new_record.clone(),
        };
        atomic_write(
            &store.state.join("journal.json"),
            &serde_json::to_vec(&journal).unwrap(),
        )
        .unwrap();
        if boundary >= 1 {
            store.replace(&journal.before, &journal.after).unwrap();
        }
        if boundary >= 2 {
            store.set_record(&new_record).unwrap();
        }
        drop(store);
        let reopened = Store::open(&directory).unwrap();
        reopened.repair().unwrap();
        reopened.repair().unwrap();
        assert_eq!(fs::read(directory.join("opengl32.dll")).unwrap(), b"old");
        assert_eq!(read_record(&directory).unwrap(), old_record);
        assert!(!pending(&directory).unwrap());
    }
}

#[test]
fn recovery_conflict_keeps_journal_and_external_file() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    let store = Store::open(&directory).unwrap();
    let journal = Journal {
        schema_version: 1,
        before: None,
        after: Some(store.save_blob(b"new").unwrap()),
        old_record: None,
        new_record: Some(record(&directory, b"new")),
    };
    atomic_write(
        &store.state.join("journal.json"),
        &serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();
    fs::write(directory.join("opengl32.dll"), b"external").unwrap();
    assert!(store.repair().is_err());
    assert!(pending(&directory).unwrap());
    assert_eq!(
        fs::read(directory.join("opengl32.dll")).unwrap(),
        b"external"
    );
}

#[test]
fn corrupt_backup_and_unknown_schema_are_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    let store = Store::open(&directory).unwrap();
    fs::write(directory.join("opengl32.dll"), b"original").unwrap();
    install(&store, b"v1", true).unwrap();
    fs::write(store.blob(&hash(b"original")).unwrap(), b"corrupt").unwrap();
    assert!(store.uninstall().is_err());
    assert!(install(&store, b"v2", false).is_err());
    assert_eq!(fs::read(directory.join("opengl32.dll")).unwrap(), b"v1");
    let mut invalid = read_record(&directory).unwrap().unwrap();
    invalid.schema_version = 999;
    fs::write(
        store.state.join("record.json"),
        serde_json::to_vec(&invalid).unwrap(),
    )
    .unwrap();
    assert!(read_record(&directory).is_err());
}

#[test]
fn exclusive_lock_and_stale_snapshot_are_enforced() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    let store = Store::open(&directory).unwrap();
    assert!(Store::open(&directory).is_err());
    fs::write(directory.join("opengl32.dll"), b"new external file").unwrap();
    assert!(store
        .install(record(&directory, b"v1"), b"v1", None, None, true)
        .is_err());
}

#[cfg(windows)]
#[test]
fn locked_dll_leaves_consistent_state() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    fs::write(directory.join("opengl32.dll"), b"original").unwrap();
    let store = Store::open(&directory).unwrap();
    let _locked = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(directory.join("opengl32.dll"))
        .unwrap();
    assert!(install(&store, b"new", true).is_err());
    assert_eq!(
        fs::read(directory.join("opengl32.dll")).unwrap(),
        b"original"
    );
    assert!(!pending(&directory).unwrap());
    assert!(read_record(&directory).unwrap().is_none());
}

#[test]
fn current_process_is_blocked() {
    let executable = std::env::current_exe().unwrap().canonicalize().unwrap();
    assert!(ensure_stopped(executable.parent().unwrap()).is_err());
}

#[test]
fn feeder_dependency_blocks_reshade_uninstall() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    let store = Store::open(&directory).unwrap();
    install(&store, b"v1", false).unwrap();
    fs::write(directory.join("dlss5-feed.addon64"), b"addon").unwrap();
    assert!(store.uninstall().unwrap_err().contains("Feeder"));
    assert_eq!(fs::read(directory.join("opengl32.dll")).unwrap(), b"v1");
}

#[cfg(windows)]
#[test]
fn junctions_are_rejected_in_target_and_state_paths() {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    let outside = directory.join("outside");
    fs::create_dir(&outside).unwrap();
    let junction = directory.join(STATE_DIR);
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rejected =
        Store::open(&directory).is_err() && safe_path(&junction.join("nested.dll")).is_err();
    fs::remove_dir(&junction).unwrap();
    assert!(rejected);
    assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
}
