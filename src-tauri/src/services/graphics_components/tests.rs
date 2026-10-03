use super::*;

pub(super) fn pe(machine: u16, magic: u16) -> Vec<u8> {
    let mut bytes = vec![0; 512];
    bytes[..2].copy_from_slice(b"MZ");
    bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
    bytes[64..68].copy_from_slice(b"PE\0\0");
    bytes[68..70].copy_from_slice(&machine.to_le_bytes());
    bytes[70..72].copy_from_slice(&1u16.to_le_bytes());
    bytes[84..86].copy_from_slice(&240u16.to_le_bytes());
    bytes[86..88].copy_from_slice(&2u16.to_le_bytes());
    bytes[88..90].copy_from_slice(&magic.to_le_bytes());
    bytes
}

#[test]
fn checks_machine_and_optional_header() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("emulator.exe");
    for (machine, magic, expected) in [
        (0x8664, 0x20b, ExecutableArchitecture::X64),
        (0x014c, 0x10b, ExecutableArchitecture::X86),
        (0xaa64, 0x20b, ExecutableArchitecture::Arm64),
    ] {
        fs::write(&path, pe(machine, magic)).unwrap();
        assert_eq!(executable_architecture(&path).unwrap(), expected);
    }
    fs::write(&path, pe(0x8664, 0x10b)).unwrap();
    assert!(executable_architecture(&path).is_err());
}

#[test]
fn rejects_truncated_forged_and_dll_targets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("emulator.exe");
    let valid = pe(0x8664, 0x20b);
    for length in [0, 2, 63, 80, 89, 367] {
        fs::write(&path, &valid[..length]).unwrap();
        assert!(executable_architecture(&path).is_err());
    }
    let mut dll = valid.clone();
    dll[86..88].copy_from_slice(&0x2002u16.to_le_bytes());
    let mut bad_offset = valid.clone();
    bad_offset[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut bad_signature = valid;
    bad_signature[64] = 0;
    for bytes in [dll, bad_offset, bad_signature] {
        fs::write(&path, bytes).unwrap();
        assert!(executable_architecture(&path).is_err());
    }
}

#[test]
fn enumerates_all_installations_and_deduplicates_paths() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["eden.exe", "citron.exe", "Ryujinx.Ava.exe", "unrelated.exe"] {
        fs::write(dir.path().join(name), b"").unwrap();
    }
    fs::create_dir(dir.path().join("yuzu.exe")).unwrap();
    let targets = collect_targets(vec![
        ("yuzu", dir.path().into()),
        ("yuzu", dir.path().join(".")),
        ("ryujinx", dir.path().into()),
        ("yuzu", dir.path().join("missing")),
    ])
    .unwrap();
    assert_eq!(targets.len(), 3);
    assert!(targets.iter().all(|target| target.executable.is_absolute()));
}

#[test]
fn discovers_yuzu_forks_and_renamed_installations_but_not_helper_programs() {
    let dir = tempfile::tempdir().unwrap();
    for name in YUZU_NAMES.iter().copied().chain([
        "eden-cli.exe",
        "yuzu-room.exe",
        "citron-cmd.exe",
        "citron-room.exe",
    ]) {
        fs::write(dir.path().join(name), b"").unwrap();
    }
    let targets = collect_targets(vec![("yuzu", dir.path().into())]).unwrap();
    assert_eq!(targets.len(), YUZU_NAMES.len());
    assert!(targets.iter().all(|t| t.family == "yuzu"));
}

#[test]
fn serializes_api_choices_explicitly() {
    assert_eq!(
        serde_json::to_string(&GraphicsApi::OpenGl).unwrap(),
        "\"openGl\""
    );
    assert!(serde_json::from_str::<GraphicsApi>("\"dxgi\"").is_err());
}
