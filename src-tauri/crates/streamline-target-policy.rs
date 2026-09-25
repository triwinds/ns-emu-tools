//! Shared build classification for the manager and diagnostic launcher.
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};
pub const VERIFIED_HASH: &str = "022c6fcbe4741661995b8e6e5f032ae017f874f893a7adaab82c57ec9e89dd85";
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
