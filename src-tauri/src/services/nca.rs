//! Nintendo Content Archive (NCA) 文件解析
//!
//! 用于解析 Nintendo Switch 的 NCA 文件格式，提取固件版本信息
//! 支持使用 prod.keys 解密 NCA 头部和内容

use crate::error::{AppError, AppResult};
use crate::services::crypto::{AesCtr, AesXts};
use crate::services::keys;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use tracing::{info, warn};

/// NCA 文件头大小（3072 字节 = 0xC00）
const NCA_HEADER_SIZE: usize = 0xC00;

/// NCA 魔术字节偏移
const NCA_MAGIC_OFFSET: usize = 0x200;

/// NCA Content Type 偏移
const NCA_CONTENT_TYPE_OFFSET: usize = 0x205;

/// NCA Crypto Type 偏移
const NCA_CRYPTO_TYPE_OFFSET: usize = 0x206;

/// NCA Key Index 偏移
const NCA_KEY_INDEX_OFFSET: usize = 0x207;

/// NCA Size 偏移
const NCA_SIZE_OFFSET: usize = 0x208;

/// NCA Title ID 偏移
const NCA_TITLE_ID_OFFSET: usize = 0x210;

/// NCA Crypto Type 2 偏移
const NCA_CRYPTO_TYPE2_OFFSET: usize = 0x220;

/// NCA Rights ID 偏移
const NCA_RIGHTS_ID_OFFSET: usize = 0x230;

/// NCA Section Table 偏移
const NCA_SECTION_TABLE_OFFSET: usize = 0x240;

/// NCA Key Block 偏移
const NCA_KEY_BLOCK_OFFSET: usize = 0x300;

/// NCA Section Header 偏移
const NCA_SECTION_HEADER_OFFSET: usize = 0x400;

/// NCA Section 条目大小
const NCA_SECTION_ENTRY_SIZE: usize = 0x10;

/// NCA Section Header 大小
const NCA_SECTION_HEADER_SIZE: usize = 0x200;

/// Media Unit 大小
const MEDIA_SIZE: u64 = 0x200;

/// NCA Content Type 枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ContentType {
    Program = 0x0,
    Meta = 0x1,
    Control = 0x2,
    Manual = 0x3,
    Data = 0x4,
    PublicData = 0x5,
}

impl ContentType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x0 => Some(ContentType::Program),
            0x1 => Some(ContentType::Meta),
            0x2 => Some(ContentType::Control),
            0x3 => Some(ContentType::Manual),
            0x4 => Some(ContentType::Data),
            0x5 => Some(ContentType::PublicData),
            _ => None,
        }
    }
}

/// 文件系统类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FsType {
    None = 0x0,
    Pfs0 = 0x2,
    RomFs = 0x3,
}

impl FsType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x0 => Some(FsType::None),
            0x2 => Some(FsType::Pfs0),
            0x3 => Some(FsType::RomFs),
            _ => None,
        }
    }
}

/// 加密类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CryptoType {
    None = 0x1,
    Xts = 0x2,
    Ctr = 0x3,
    Bktr = 0x4,
}

impl CryptoType {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x1 => Some(CryptoType::None),
            0x2 => Some(CryptoType::Xts),
            0x3 => Some(CryptoType::Ctr),
            0x4 => Some(CryptoType::Bktr),
            _ => None,
        }
    }
}

/// NCA Section Table 条目
#[derive(Debug, Clone)]
pub struct SectionTableEntry {
    /// 起始偏移（media units, 1 unit = 0x200 bytes）
    pub media_start_offset: u32,
    /// 结束偏移（media units）
    pub media_end_offset: u32,
    /// 是否有效
    pub enabled: bool,
}

/// NCA Section Header
#[derive(Debug, Clone)]
pub struct SectionHeader {
    /// 文件系统类型
    pub fs_type: FsType,
    /// 加密类型
    pub crypto_type: CryptoType,
    /// Section 起始偏移（在 section 内）
    pub section_start: u64,
    /// Crypto Counter
    pub crypto_counter: [u8; 16],
}

/// NCA 文件头信息
#[derive(Debug)]
pub struct NcaHeader {
    /// 魔术字节 ("NCA3" 或 "NCA2")
    pub magic: [u8; 4],
    /// 内容类型
    pub content_type: ContentType,
    /// 加密类型
    pub crypto_type: u8,
    /// 加密类型 2
    pub crypto_type2: u8,
    /// 密钥索引
    pub key_index: u8,
    /// NCA 大小
    pub size: u64,
    /// Title ID（十六进制字符串）
    pub title_id: String,
    /// Rights ID
    pub rights_id: [u8; 16],
    /// Section 表信息
    pub sections: Vec<SectionTableEntry>,
    /// Section Headers
    pub section_headers: Vec<SectionHeader>,
    /// 解密后的密钥块
    pub decrypted_keys: Vec<[u8; 16]>,
    /// 完整的解密头部数据
    pub raw_header: Vec<u8>,
}

impl NcaHeader {
    /// 从文件中读取并解密 NCA 头部
    pub fn read_from_file<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let path = path.as_ref();

        let mut file = File::open(path)?;
        let mut header_data = vec![0u8; NCA_HEADER_SIZE];
        file.read_exact(&mut header_data)?;

        // 尝试先不解密解析
        if let Ok(header) = Self::parse(&header_data, false) {
            return Ok(header);
        }

        // 如果未加密解析失败，尝试解密
        if keys::is_keys_loaded() {
            keys::with_keys(|key_store| {
                let header_key = key_store.get_header_key()?;
                let decrypted = decrypt_nca_header(&header_data, &header_key);
                Self::parse(&decrypted, true)
            })
        } else {
            Err(AppError::InvalidArgument(
                "NCA 文件已加密，但密钥未加载".to_string(),
            ))
        }
    }

    /// 解析 NCA 头部数据
    pub fn parse(data: &[u8], was_encrypted: bool) -> AppResult<Self> {
        if data.len() < NCA_HEADER_SIZE {
            return Err(AppError::InvalidArgument(
                "NCA 头部数据不完整".to_string(),
            ));
        }

        // 读取魔术字节
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&data[NCA_MAGIC_OFFSET..NCA_MAGIC_OFFSET + 4]);

        // 验证魔术字节
        if &magic != b"NCA3" && &magic != b"NCA2" {
            return Err(AppError::InvalidArgument(format!(
                "无效的 NCA 魔术字节: {:?}",
                magic
            )));
        }

        // 读取 content type
        let content_type_byte = data[NCA_CONTENT_TYPE_OFFSET];
        let content_type = ContentType::from_u8(content_type_byte).ok_or_else(|| {
            AppError::InvalidArgument(format!("无效的 content type: {}", content_type_byte))
        })?;

        // 读取 crypto type 和 key index
        let crypto_type = data[NCA_CRYPTO_TYPE_OFFSET];
        let key_index = data[NCA_KEY_INDEX_OFFSET];

        // 读取 NCA 大小
        let size = u64::from_le_bytes(data[NCA_SIZE_OFFSET..NCA_SIZE_OFFSET + 8].try_into().unwrap());

        // 读取 Title ID (8 字节，little-endian)
        let title_id_bytes = &data[NCA_TITLE_ID_OFFSET..NCA_TITLE_ID_OFFSET + 8];
        let title_id_u64 = u64::from_le_bytes(title_id_bytes.try_into().unwrap());
        let title_id = format!("{:016x}", title_id_u64);

        // 读取 crypto type 2
        let crypto_type2 = data[NCA_CRYPTO_TYPE2_OFFSET];

        // 读取 Rights ID
        let mut rights_id = [0u8; 16];
        rights_id.copy_from_slice(&data[NCA_RIGHTS_ID_OFFSET..NCA_RIGHTS_ID_OFFSET + 16]);

        // 读取 Section Table
        let mut sections = Vec::new();
        for i in 0..4 {
            let offset = NCA_SECTION_TABLE_OFFSET + i * NCA_SECTION_ENTRY_SIZE;
            let section_data = &data[offset..offset + NCA_SECTION_ENTRY_SIZE];

            let media_start_offset =
                u32::from_le_bytes(section_data[0..4].try_into().unwrap());
            let media_end_offset =
                u32::from_le_bytes(section_data[4..8].try_into().unwrap());

            let enabled = media_end_offset > media_start_offset;

            sections.push(SectionTableEntry {
                media_start_offset,
                media_end_offset,
                enabled,
            });
        }

        // 读取 Section Headers
        let mut section_headers = Vec::new();
        for i in 0..4 {
            let offset = NCA_SECTION_HEADER_OFFSET + i * NCA_SECTION_HEADER_SIZE;
            let header_data = &data[offset..offset + NCA_SECTION_HEADER_SIZE];

            let fs_type = FsType::from_u8(header_data[0x03]).unwrap_or(FsType::None);
            let crypto_type_val = CryptoType::from_u8(header_data[0x04]).unwrap_or(CryptoType::None);

            // Crypto Counter 在 0x140-0x148
            let mut crypto_counter = [0u8; 16];
            // 前 8 字节为 0，后 8 字节是 counter（大端序）
            crypto_counter[8..16].copy_from_slice(&header_data[0x140..0x148]);
            // 整体反转
            crypto_counter.reverse();

            section_headers.push(SectionHeader {
                fs_type,
                crypto_type: crypto_type_val,
                section_start: 0, // 需要根据具体文件系统类型解析
                crypto_counter,
            });
        }

        // 解密 Key Block
        let mut decrypted_keys = Vec::new();
        if was_encrypted && keys::is_keys_loaded() {
            let master_key_index = std::cmp::max(crypto_type, crypto_type2).saturating_sub(1) as usize;
            let key_block = &data[NCA_KEY_BLOCK_OFFSET..NCA_KEY_BLOCK_OFFSET + 0x40];

            if let Ok(keys_result) = keys::with_keys(|key_store| {
                key_store.unwrap_key_block(key_block, master_key_index)
            }) {
                decrypted_keys = keys_result;
            }
        }

        Ok(NcaHeader {
            magic,
            content_type,
            crypto_type,
            crypto_type2,
            key_index,
            size,
            title_id,
            rights_id,
            sections,
            section_headers,
            decrypted_keys,
            raw_header: data[0..NCA_HEADER_SIZE].to_vec(),
        })
    }

    /// 检查是否有 title rights
    pub fn has_title_rights(&self) -> bool {
        self.rights_id != [0u8; 16]
    }

    /// 获取 master key 索引
    pub fn master_key_index(&self) -> usize {
        std::cmp::max(self.crypto_type, self.crypto_type2).saturating_sub(1) as usize
    }

    /// 获取解密密钥（通常使用 key[2]）
    pub fn get_content_key(&self) -> Option<[u8; 16]> {
        self.decrypted_keys.get(2).copied()
    }

    /// 检查是否是系统版本归档（System Version Archive）
    pub fn is_system_version_archive(&self) -> bool {
        self.title_id == "0100000000000809" && self.content_type == ContentType::Data
    }
}

/// 使用 header_key 解密 NCA 头部
fn decrypt_nca_header(data: &[u8], header_key: &[u8; 32]) -> Vec<u8> {
    let xts = AesXts::new(header_key);
    xts.decrypt_with_sector(data, 0)
}

/// NCA 文件读取器
pub struct NcaReader {
    /// 文件句柄
    file: File,
    /// NCA 头部信息
    pub header: NcaHeader,
}

impl NcaReader {
    /// 打开 NCA 文件
    pub fn open<P: AsRef<Path>>(path: P) -> AppResult<Self> {
        let path = path.as_ref();
        let header = NcaHeader::read_from_file(path)?;
        let file = File::open(path)?;

        Ok(Self { file, header })
    }

    /// 读取 section 数据
    pub fn read_section(&mut self, section_index: usize) -> AppResult<Vec<u8>> {
        if section_index >= self.header.sections.len() {
            return Err(AppError::InvalidArgument(format!(
                "无效的 section 索引: {}",
                section_index
            )));
        }

        let section = &self.header.sections[section_index];
        if !section.enabled {
            return Err(AppError::InvalidArgument(format!(
                "Section {} 未启用",
                section_index
            )));
        }

        let section_header = &self.header.section_headers[section_index];
        let section_start = (section.media_start_offset as u64) * MEDIA_SIZE;
        let section_end = (section.media_end_offset as u64) * MEDIA_SIZE;
        let section_size = section_end - section_start;

        // 读取原始数据
        self.file.seek(SeekFrom::Start(section_start))?;
        let mut data = vec![0u8; section_size as usize];
        self.file.read_exact(&mut data)?;

        // 根据加密类型解密
        match section_header.crypto_type {
            CryptoType::None => Ok(data),
            CryptoType::Ctr => {
                if let Some(key) = self.header.get_content_key() {
                    let ctr = AesCtr::from_counter(&key, &section_header.crypto_counter);
                    Ok(ctr.decrypt(&data, 0))
                } else {
                    warn!("没有可用的解密密钥");
                    Ok(data)
                }
            }
            CryptoType::Xts => {
                // XTS 模式通常用于 NCA0，这里暂不支持
                warn!("暂不支持 XTS 模式的 section 解密");
                Ok(data)
            }
            CryptoType::Bktr => {
                // BKTR 模式用于增量更新，这里暂不支持
                warn!("暂不支持 BKTR 模式的 section 解密");
                Ok(data)
            }
        }
    }

    /// 读取 section 的部分数据
    pub fn read_section_range(
        &mut self,
        section_index: usize,
        offset: u64,
        size: usize,
    ) -> AppResult<Vec<u8>> {
        if section_index >= self.header.sections.len() {
            return Err(AppError::InvalidArgument(format!(
                "无效的 section 索引: {}",
                section_index
            )));
        }

        let section = &self.header.sections[section_index];
        if !section.enabled {
            return Err(AppError::InvalidArgument(format!(
                "Section {} 未启用",
                section_index
            )));
        }

        let section_header = &self.header.section_headers[section_index];
        let section_start = (section.media_start_offset as u64) * MEDIA_SIZE;
        let absolute_offset = section_start + offset;

        // 读取原始数据
        self.file.seek(SeekFrom::Start(absolute_offset))?;
        let mut data = vec![0u8; size];
        self.file.read_exact(&mut data)?;

        // 根据加密类型解密
        match section_header.crypto_type {
            CryptoType::None => Ok(data),
            CryptoType::Ctr => {
                if let Some(key) = self.header.get_content_key() {
                    let ctr = AesCtr::from_counter(&key, &section_header.crypto_counter);
                    // 使用 absolute_offset（section 在文件中的绝对偏移）来计算 counter
                    Ok(ctr.decrypt(&data, absolute_offset))
                } else {
                    warn!("没有可用的解密密钥");
                    Ok(data)
                }
            }
            _ => {
                warn!("暂不支持该加密模式");
                Ok(data)
            }
        }
    }
}

/// 从 NCA 文件中提取固件版本
pub fn extract_firmware_version<P: AsRef<Path>>(nca_path: P) -> AppResult<String> {
    let nca_path = nca_path.as_ref();
    info!("提取固件版本: {}", nca_path.display());

    let mut reader = NcaReader::open(nca_path)?;

    // 验证是否是系统版本归档
    if !reader.header.is_system_version_archive() {
        return Err(AppError::InvalidArgument(
            "不是系统版本归档文件".to_string(),
        ));
    }

    // 检查是否有文件系统 section
    let enabled_sections: Vec<_> = reader
        .header
        .sections
        .iter()
        .enumerate()
        .filter(|(_, s)| s.enabled)
        .collect();

    if enabled_sections.is_empty() {
        return Err(AppError::InvalidArgument(
            "NCA 文件中没有文件系统 section".to_string(),
        ));
    }

    // 读取第一个有效 section
    let (section_idx, _) = enabled_sections[0];

    // 限制读取大小
    let max_read_size = 10 * 1024 * 1024; // 10 MB
    let section = &reader.header.sections[section_idx];
    let section_size = ((section.media_end_offset - section.media_start_offset) as u64) * MEDIA_SIZE;
    let read_size = std::cmp::min(section_size, max_read_size) as usize;

    let section_data = reader.read_section_range(section_idx, 0, read_size)?;

    // 搜索固件版本魔术字节: "NX\x00\x00\x00\x00"
    let magic = b"NX\x00\x00\x00\x00";

    if let Some(idx) = find_pattern(&section_data, magic) {
        // 版本字符串位于魔术字节 + 0x60 处
        let version_offset = idx + 0x60;

        if version_offset + 0x10 <= section_data.len() {
            let version_bytes = &section_data[version_offset..version_offset + 0x10];

            // 读取版本字符串（去除 null 字节）
            let version = version_bytes
                .iter()
                .take_while(|&&b| b != 0)
                .copied()
                .collect::<Vec<u8>>();

            if let Ok(version_str) = String::from_utf8(version) {
                if !version_str.is_empty() {
                    info!("找到固件版本: {}", version_str);
                    return Ok(version_str);
                }
            }
        }
    }

    Err(AppError::FileNotFound("未找到固件版本信息".to_string()))
}

/// 在字节数组中查找模式
fn find_pattern(data: &[u8], pattern: &[u8]) -> Option<usize> {
    data.windows(pattern.len())
        .position(|window| window == pattern)
}

/// 扫描目录中的所有 NCA 文件，找到系统版本归档
pub fn find_system_version_nca<P: AsRef<Path>>(firmware_dir: P) -> AppResult<Option<std::path::PathBuf>> {
    let firmware_dir = firmware_dir.as_ref();
    info!("扫描固件目录: {}", firmware_dir.display());

    if !firmware_dir.exists() {
        return Err(AppError::DirectoryNotFound(format!(
            "固件目录不存在: {}",
            firmware_dir.display()
        )));
    }

    // 遍历目录中的文件
    let entries = std::fs::read_dir(firmware_dir)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // 只处理 .nca 文件（不包括 .cnmt.nca）
        if path.is_file() {
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if name_str.ends_with(".nca") && !name_str.ends_with(".cnmt.nca") {
                    // 尝试读取 NCA 头部
                    match NcaHeader::read_from_file(&path) {
                        Ok(header) => {
                            if header.is_system_version_archive() {
                                info!("找到系统版本归档: {}", path.display());
                                return Ok(Some(path));
                            }
                        }
                        Err(_) => {
                            // 忽略无法解析的文件
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

/// 扫描 Ryujinx 格式的固件目录
pub fn find_system_version_nca_ryujinx<P: AsRef<Path>>(
    firmware_dir: P,
) -> AppResult<Option<std::path::PathBuf>> {
    let firmware_dir = firmware_dir.as_ref();
    info!("扫描 Ryujinx 固件目录: {}", firmware_dir.display());

    if !firmware_dir.exists() {
        return Err(AppError::DirectoryNotFound(format!(
            "固件目录不存在: {}",
            firmware_dir.display()
        )));
    }

    // Ryujinx 的文件结构: firmware_dir/**/00
    use walkdir::WalkDir;

    for entry_result in WalkDir::new(firmware_dir).max_depth(2) {
        // 处理 walkdir 错误
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();

        if path.is_file() && path.file_name() == Some(std::ffi::OsStr::new("00")) {
            // 尝试读取 NCA 头部
            match NcaHeader::read_from_file(path) {
                Ok(header) => {
                    if header.is_system_version_archive() {
                        info!("找到系统版本归档: {}", path.display());
                        return Ok(Some(path.to_path_buf()));
                    }
                }
                Err(_) => {
                    // 忽略无法解析的文件
                }
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_conversion() {
        assert_eq!(ContentType::from_u8(0x0), Some(ContentType::Program));
        assert_eq!(ContentType::from_u8(0x4), Some(ContentType::Data));
        assert_eq!(ContentType::from_u8(0xFF), None);
    }

    #[test]
    fn test_find_pattern() {
        let data = b"Hello NX\x00\x00\x00\x00 World";
        let pattern = b"NX\x00\x00\x00\x00";
        assert_eq!(find_pattern(data, pattern), Some(6));
    }

    #[test]
    fn test_title_id_format() {
        let title_id_bytes: [u8; 8] = [0x09, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        let title_id_u64 = u64::from_le_bytes(title_id_bytes);
        let title_id = format!("{:016x}", title_id_u64);
        assert_eq!(title_id, "0100000000000809");
    }

    #[test]
    fn test_crypto_type_conversion() {
        assert_eq!(CryptoType::from_u8(0x1), Some(CryptoType::None));
        assert_eq!(CryptoType::from_u8(0x3), Some(CryptoType::Ctr));
        assert_eq!(CryptoType::from_u8(0xFF), None);
    }

    #[test]
    fn test_fs_type_conversion() {
        assert_eq!(FsType::from_u8(0x2), Some(FsType::Pfs0));
        assert_eq!(FsType::from_u8(0x3), Some(FsType::RomFs));
        assert_eq!(FsType::from_u8(0xFF), None);
    }
}
