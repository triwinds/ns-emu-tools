//! Shared build classification for the manager and diagnostic launcher.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
pub const VERIFIED_HASH: &str = "022c6fcbe4741661995b8e6e5f032ae017f874f893a7adaab82c57ec9e89dd85";
pub const YUZU_NAMES: &[&str] = &[
    "yuzu.exe",
    "eden.exe",
    "citron.exe",
    "suzu.exe",
    "suyu.exe",
    "sudachi.exe",
    "torzu.exe",
    "cemu.exe", // Toolbox's optional rename for yuzu-family installations.
];
pub const RYUJINX_NAMES: &[&str] = &["Ryujinx.exe", "Ryujinx.Ava.exe"];

/// A launch-protocol hint, never evidence that a build has been verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetFamily {
    Yuzu,
    Ryujinx,
    Unknown,
}
impl TargetFamily {
    pub fn detect(path: &Path, hash: &str) -> Self {
        if hash == VERIFIED_HASH {
            return Self::Ryujinx;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if YUZU_NAMES.iter().any(|n| name.eq_ignore_ascii_case(n)) {
            Self::Yuzu
        } else if RYUJINX_NAMES.iter().any(|n| name.eq_ignore_ascii_case(n)) {
            Self::Ryujinx
        } else {
            Self::Unknown
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yuzu => "yuzu",
            Self::Ryujinx => "ryujinx",
            Self::Unknown => "unknown",
        }
    }
    // The manager uses only family discovery; the separate launcher uses this.
    #[allow(dead_code)]
    pub fn configure_command(
        self,
        command: &mut std::process::Command,
        enhanced: bool,
        game: Option<&Path>,
    ) {
        // Ryubing applies this after loading global and per-game settings.
        // Qt yuzu forks select Vulkan in their settings and reject this option.
        if enhanced && self == Self::Ryujinx {
            command.args(["--graphics-backend", "Vulkan"]);
        }
        if let Some(game) = game {
            // A single absolute ROM path also works with older Qt builds that
            // interpret newer command-line options themselves as ROM paths.
            command.arg(game);
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compatibility {
    Verified,
    Unverified,
    Incompatible,
}
impl Compatibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Unverified => "unverified",
            Self::Incompatible => "incompatible",
        }
    }
    pub fn authorize(self, allow_unverified: bool) -> Result<(), &'static str> {
        match self {
            Self::Verified => Ok(()),
            Self::Unverified if allow_unverified => Ok(()),
            Self::Unverified => Err("此构建兼容性未验证，需要明确选择尝试后才能启动"),
            Self::Incompatible => Err("目标不是受支持的 Windows x64 EXE，不能尝试启用 FG"),
        }
    }
}
pub fn classify(hash: &str, valid_x64_exe: bool) -> Compatibility {
    if !valid_x64_exe {
        Compatibility::Incompatible
    } else if hash == VERIFIED_HASH {
        Compatibility::Verified
    } else {
        Compatibility::Unverified
    }
}
/// Bounded PE inspection: filenames and MZ alone never establish architecture.
pub fn validate_executable(path: &Path) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let metadata = file.metadata().map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("所选路径不是普通文件".into());
    }
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos).map_err(|_| "EXE 文件头不完整")?;
    if &dos[..2] != b"MZ" {
        return Err("目标不是 Windows EXE".into());
    }
    let offset = u64::from(u32::from_le_bytes(dos[60..64].try_into().unwrap()));
    if offset < 64 || offset + 24 > metadata.len() {
        return Err("PE 文件头偏移无效".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut header = [0u8; 24];
    file.read_exact(&mut header)
        .map_err(|_| "PE 文件头不完整")?;
    let machine = u16::from_le_bytes([header[4], header[5]]);
    let sections = u16::from_le_bytes([header[6], header[7]]);
    let optional = u16::from_le_bytes([header[20], header[21]]);
    let flags = u16::from_le_bytes([header[22], header[23]]);
    if &header[..4] != b"PE\0\0" || flags & 2 == 0 || flags & 0x2000 != 0 {
        return Err("目标不是有效的 PE 可执行程序（不接受 DLL）".into());
    }
    if machine != 0x8664 {
        return Err("仅支持 x64 EXE，不支持 x86 或 ARM64".into());
    }
    if sections == 0
        || optional < 112
        || offset + 24 + u64::from(optional) + u64::from(sections) * 40 > metadata.len()
    {
        return Err("PE 可选头或节表不完整".into());
    }
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic).map_err(|_| "PE 可选头不完整")?;
    if u16::from_le_bytes(magic) != 0x20b {
        return Err("目标不是 PE32+ x64 EXE".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn family_hint_never_verifies_an_unknown_build() {
        for name in YUZU_NAMES {
            assert_eq!(
                TargetFamily::detect(Path::new(name), "new"),
                TargetFamily::Yuzu
            );
            assert_eq!(classify("new", true), Compatibility::Unverified);
        }
        assert_eq!(
            TargetFamily::detect(Path::new("EDEN.EXE"), "new"),
            TargetFamily::Yuzu
        );
        assert_eq!(
            TargetFamily::detect(Path::new("eden-cli.exe"), "new"),
            TargetFamily::Unknown
        );
        assert_eq!(
            TargetFamily::detect(Path::new("other.exe"), "new"),
            TargetFamily::Unknown
        );
        assert_eq!(
            TargetFamily::detect(Path::new("renamed.exe"), VERIFIED_HASH),
            TargetFamily::Ryujinx
        );
    }
    #[test]
    fn launch_arguments_follow_the_emulator_protocol() {
        use std::{ffi::OsString, process::Command};
        let game = Path::new("C:/games/a game.xci");
        for (family, enhanced, expected) in [
            (TargetFamily::Yuzu, true, vec!["C:/games/a game.xci"]),
            (TargetFamily::Yuzu, false, vec!["C:/games/a game.xci"]),
            (
                TargetFamily::Ryujinx,
                true,
                vec!["--graphics-backend", "Vulkan", "C:/games/a game.xci"],
            ),
            (TargetFamily::Ryujinx, false, vec!["C:/games/a game.xci"]),
            (TargetFamily::Unknown, true, vec!["C:/games/a game.xci"]),
        ] {
            let mut command = Command::new("emulator.exe");
            family.configure_command(&mut command, enhanced, Some(game));
            assert_eq!(
                command.get_args().collect::<Vec<_>>(),
                expected.iter().map(OsString::from).collect::<Vec<_>>()
            );
        }
        let mut command = Command::new("eden.exe");
        TargetFamily::Yuzu.configure_command(&mut command, true, None);
        assert_eq!(command.get_args().count(), 0);
    }
    #[test]
    fn explicit_trial_only_bypasses_unknown_build_not_incompatibility() {
        assert_eq!(classify(VERIFIED_HASH, true), Compatibility::Verified);
        assert_eq!(classify("new build", true), Compatibility::Unverified);
        assert_eq!(classify(VERIFIED_HASH, false), Compatibility::Incompatible);
        assert!(Compatibility::Verified.authorize(false).is_ok());
        assert!(Compatibility::Unverified.authorize(false).is_err());
        assert!(Compatibility::Unverified.authorize(true).is_ok());
        assert!(Compatibility::Incompatible.authorize(true).is_err());
    }
}
