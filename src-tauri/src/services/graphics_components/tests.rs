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
fn serializes_api_choices_explicitly() {
    assert_eq!(
        serde_json::to_string(&GraphicsApi::OpenGl).unwrap(),
        "\"openGl\""
    );
    assert!(serde_json::from_str::<GraphicsApi>("\"dxgi\"").is_err());
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn detection_is_read_only_and_does_not_claim_compatibility() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eden.exe");
    fs::write(&path, pe(0x8664, 0x20b)).unwrap();
    let clean = detect(path.clone(), Some(GraphicsApi::OpenGl));
    assert_eq!(clean.reshade_state, GraphicsComponentState::NotInstalled);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    for name in ["opengl32.dll", "dlss5-feed.addon64", "ReShadePreset.ini"] {
        fs::write(dir.path().join(name), b"user content").unwrap();
    }
    let external = detect(path.clone(), Some(GraphicsApi::OpenGl));
    assert_eq!(external.reshade_state, GraphicsComponentState::External);
    assert_eq!(external.feeder_state, GraphicsComponentState::External);
    assert!(!external.compatibility_verified);
    assert!(external.installation_available);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
    assert_eq!(
        fs::read(dir.path().join("ReShadePreset.ini")).unwrap(),
        b"user content"
    );
    let vulkan = detect(path.clone(), Some(GraphicsApi::Vulkan));
    assert!(!vulkan.compatibility_verified);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 4);
    assert_eq!(
        detect(path, None).reshade_state,
        GraphicsComponentState::Unknown
    );
}

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[test]
fn invalid_and_unsupported_targets_are_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("eden.exe");
    assert_eq!(
        detect(path.clone(), None).reshade_state,
        GraphicsComponentState::Error
    );
    fs::write(&path, pe(0x014c, 0x10b)).unwrap();
    let report = detect(path, Some(GraphicsApi::OpenGl));
    assert_eq!(report.reshade_state, GraphicsComponentState::Unsupported);
    assert!(!report.supported_target);
}

#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
#[test]
fn unsupported_host_never_offers_installation() {
    let report = detect(PathBuf::from("missing"), None);
    assert_eq!(report.reshade_state, GraphicsComponentState::Unsupported);
    assert!(!report.installation_available);
}
