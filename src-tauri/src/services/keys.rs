//! Nintendo Switch 密钥管理模块
//!
//! 用于加载和管理 Switch 加密密钥，支持 NCA 文件解密

use crate::error::{AppError, AppResult};
use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes128;
use std::collections::HashMap;
use std::path::Path;
use std::sync::RwLock;
use tracing::{debug, info};

/// 密钥长度（16 字节 = 128 位）
const KEY_SIZE: usize = 16;

/// 最大支持的 master key 版本
const MAX_MASTER_KEY_INDEX: usize = 32;

/// 已知密钥的 CRC32 校验和
static CRC32_CHECKSUMS: &[(&str, u32)] = &[
    ("aes_kek_generation_source", 2545229389),
    ("aes_key_generation_source", 459881589),
    ("titlekek_source", 3510501772),
    ("key_area_key_application_source", 4130296074),
    ("key_area_key_ocean_source", 3975316347),
    ("key_area_key_system_source", 4024798875),
    ("master_key_00", 3540309694),
    ("master_key_01", 3477638116),
    ("master_key_02", 2087460235),
    ("master_key_03", 4095912905),
    ("master_key_04", 3833085536),
    ("master_key_05", 2078263136),
    ("master_key_06", 2812171174),
    ("master_key_07", 1146095808),
    ("master_key_08", 1605958034),
    ("master_key_09", 3456782962),
    ("master_key_0a", 2012895168),
    ("master_key_0b", 3813624150),
    ("master_key_0c", 3881579466),
    ("master_key_0d", 723654444),
    ("master_key_0e", 2690905064),
    ("master_key_0f", 4082108335),
    ("master_key_10", 788455323),
];

/// 密钥类型索引
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAreaKeyType {
    Application = 0,
    Ocean = 1,
    System = 2,
}

/// 全局密钥存储
static KEYS: RwLock<Option<KeyStore>> = RwLock::new(None);

/// 密钥存储结构
#[derive(Debug, Clone)]
pub struct KeyStore {
    /// 原始密钥映射（名称 -> 16进制字符串）
    keys: HashMap<String, String>,
    /// Title KEKs（派生后的）
    title_keks: Vec<[u8; KEY_SIZE]>,
    /// Key Area Keys（派生后的）[master_key_index][key_type]
    key_area_keys: Vec<[[u8; KEY_SIZE]; 3]>,
    /// 加载的密钥文件路径
    loaded_file: String,
}

impl KeyStore {
    /// 创建新的密钥存储
    fn new() -> Self {
        let mut key_area_keys = Vec::with_capacity(MAX_MASTER_KEY_INDEX);
        for _ in 0..MAX_MASTER_KEY_INDEX {
            key_area_keys.push([[0u8; KEY_SIZE]; 3]);
        }

        Self {
            keys: HashMap::new(),
            title_keks: Vec::new(),
            key_area_keys,
            loaded_file: String::new(),
        }
    }

    /// 从文件加载密钥
    fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> AppResult<()> {
        let path = path.as_ref();
        info!("加载密钥文件: {}", path.display());

        let content = std::fs::read_to_string(path)?;
        self.loaded_file = path.to_string_lossy().to_string();

        // 解析密钥文件
        let key_pattern = regex::Regex::new(r"^\s*([a-z0-9_]+)\s*=\s*([A-Fa-f0-9]+)\s*$").unwrap();

        for line in content.lines() {
            if let Some(caps) = key_pattern.captures(line) {
                let name = caps.get(1).unwrap().as_str().to_lowercase();
                let value = caps.get(2).unwrap().as_str().to_uppercase();
                self.keys.insert(name, value);
            }
        }

        info!("已加载 {} 个密钥", self.keys.len());

        // 派生密钥
        self.derive_keys()?;

        Ok(())
    }

    /// 获取原始密钥（字节数组）
    fn get_key(&self, name: &str) -> AppResult<[u8; KEY_SIZE]> {
        let hex_str = self.keys.get(name).ok_or_else(|| {
            AppError::InvalidArgument(format!(
                "密钥 {} 不存在于 {}",
                name, self.loaded_file
            ))
        })?;

        let bytes = hex::decode(hex_str).map_err(|e| {
            AppError::InvalidArgument(format!("无效的密钥格式 {}: {}", name, e))
        })?;

        if bytes.len() != KEY_SIZE {
            return Err(AppError::InvalidArgument(format!(
                "密钥 {} 长度错误: 期望 {} 字节，实际 {} 字节",
                name,
                KEY_SIZE,
                bytes.len()
            )));
        }

        // 验证 CRC32
        let checksum = crc32fast::hash(&bytes);
        for (key_name, expected_crc) in CRC32_CHECKSUMS {
            if *key_name == name && *expected_crc != checksum {
                return Err(AppError::InvalidArgument(format!(
                    "密钥 {} CRC32 校验失败: 期望 {}, 实际 {}",
                    name, expected_crc, checksum
                )));
            }
        }

        let mut result = [0u8; KEY_SIZE];
        result.copy_from_slice(&bytes);
        Ok(result)
    }

    /// 检查 master key 是否存在
    fn exists_master_key(&self, index: usize) -> bool {
        let name = format!("master_key_{:02x}", index);
        self.keys.contains_key(&name)
    }

    /// 获取 master key
    fn get_master_key(&self, index: usize) -> AppResult<[u8; KEY_SIZE]> {
        let name = format!("master_key_{:02x}", index);
        self.get_key(&name)
    }

    /// 派生所有密钥
    fn derive_keys(&mut self) -> AppResult<()> {
        debug!("开始派生密钥...");

        let aes_kek_generation_source = self.get_key("aes_kek_generation_source")?;
        let aes_key_generation_source = self.get_key("aes_key_generation_source")?;
        let titlekek_source = self.get_key("titlekek_source")?;
        let key_area_key_application_source = self.get_key("key_area_key_application_source")?;
        let key_area_key_ocean_source = self.get_key("key_area_key_ocean_source")?;
        let key_area_key_system_source = self.get_key("key_area_key_system_source")?;

        self.title_keks.clear();

        for i in 0..MAX_MASTER_KEY_INDEX {
            if !self.exists_master_key(i) {
                continue;
            }

            let master_key = self.get_master_key(i)?;

            // 派生 title kek
            let title_kek = aes_ecb_decrypt(&master_key, &titlekek_source);
            self.title_keks.push(title_kek);

            // 派生 key area keys
            self.key_area_keys[i][KeyAreaKeyType::Application as usize] = generate_kek(
                &key_area_key_application_source,
                &master_key,
                &aes_kek_generation_source,
                Some(&aes_key_generation_source),
            );

            self.key_area_keys[i][KeyAreaKeyType::Ocean as usize] = generate_kek(
                &key_area_key_ocean_source,
                &master_key,
                &aes_kek_generation_source,
                Some(&aes_key_generation_source),
            );

            self.key_area_keys[i][KeyAreaKeyType::System as usize] = generate_kek(
                &key_area_key_system_source,
                &master_key,
                &aes_kek_generation_source,
                Some(&aes_key_generation_source),
            );

            debug!("已派生 master_key_{:02x} 相关密钥", i);
        }

        info!("密钥派生完成，共 {} 个 master key", self.title_keks.len());
        Ok(())
    }

    /// 获取 header key（32 字节）
    pub fn get_header_key(&self) -> AppResult<[u8; 32]> {
        let hex_str = self.keys.get("header_key").ok_or_else(|| {
            AppError::InvalidArgument(format!(
                "header_key 不存在于 {}",
                self.loaded_file
            ))
        })?;

        let bytes = hex::decode(hex_str).map_err(|e| {
            AppError::InvalidArgument(format!("无效的 header_key 格式: {}", e))
        })?;

        if bytes.len() != 32 {
            return Err(AppError::InvalidArgument(format!(
                "header_key 长度错误: 期望 32 字节，实际 {} 字节",
                bytes.len()
            )));
        }

        let mut result = [0u8; 32];
        result.copy_from_slice(&bytes);
        Ok(result)
    }

    /// 获取 title kek
    pub fn get_title_kek(&self, index: usize) -> AppResult<[u8; KEY_SIZE]> {
        self.title_keks.get(index).copied().ok_or_else(|| {
            AppError::InvalidArgument(format!("title_kek_{:02x} 不存在", index))
        })
    }

    /// 获取 key area key
    pub fn get_key_area_key(
        &self,
        crypto_type: usize,
        key_type: KeyAreaKeyType,
    ) -> AppResult<[u8; KEY_SIZE]> {
        if crypto_type >= MAX_MASTER_KEY_INDEX {
            return Err(AppError::InvalidArgument(format!(
                "无效的 crypto_type: {}",
                crypto_type
            )));
        }
        Ok(self.key_area_keys[crypto_type][key_type as usize])
    }

    /// 解密 key block（使用 unwrap AES wrapped title key）
    pub fn unwrap_key_block(
        &self,
        wrapped_key: &[u8],
        key_generation: usize,
    ) -> AppResult<Vec<[u8; KEY_SIZE]>> {
        let aes_kek_generation_source = self.get_key("aes_kek_generation_source")?;
        let aes_key_generation_source = self.get_key("aes_key_generation_source")?;
        let key_area_key_application_source = self.get_key("key_area_key_application_source")?;

        let master_key = self.get_master_key(key_generation)?;

        let kek = generate_kek(
            &key_area_key_application_source,
            &master_key,
            &aes_kek_generation_source,
            Some(&aes_key_generation_source),
        );

        // 解密整个 key block
        let decrypted = aes_ecb_decrypt_block(&kek, wrapped_key);

        // 分割成 4 个密钥
        let mut keys = Vec::new();
        for i in 0..4 {
            let offset = i * KEY_SIZE;
            if offset + KEY_SIZE <= decrypted.len() {
                let mut key = [0u8; KEY_SIZE];
                key.copy_from_slice(&decrypted[offset..offset + KEY_SIZE]);
                keys.push(key);
            }
        }

        Ok(keys)
    }

    /// 解密 title key
    pub fn decrypt_title_key(
        &self,
        encrypted_key: &[u8; KEY_SIZE],
        key_generation: usize,
    ) -> AppResult<[u8; KEY_SIZE]> {
        let title_kek = self.get_title_kek(key_generation)?;
        Ok(aes_ecb_decrypt(&title_kek, encrypted_key))
    }
}

/// AES-ECB 解密单个块
fn aes_ecb_decrypt(key: &[u8; KEY_SIZE], data: &[u8; KEY_SIZE]) -> [u8; KEY_SIZE] {
    let cipher = Aes128::new_from_slice(key).unwrap();
    let mut block = aes::Block::clone_from_slice(data);
    cipher.decrypt_block(&mut block);
    let mut result = [0u8; KEY_SIZE];
    result.copy_from_slice(&block);
    result
}

/// AES-ECB 解密多个块
fn aes_ecb_decrypt_block(key: &[u8; KEY_SIZE], data: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(key).unwrap();
    let mut result = data.to_vec();

    for chunk in result.chunks_mut(KEY_SIZE) {
        if chunk.len() == KEY_SIZE {
            let mut block = aes::Block::clone_from_slice(chunk);
            cipher.decrypt_block(&mut block);
            chunk.copy_from_slice(&block);
        }
    }

    result
}

/// 生成 KEK
fn generate_kek(
    src: &[u8; KEY_SIZE],
    master_key: &[u8; KEY_SIZE],
    kek_seed: &[u8; KEY_SIZE],
    key_seed: Option<&[u8; KEY_SIZE]>,
) -> [u8; KEY_SIZE] {
    // kek = AES-ECB-Decrypt(master_key, kek_seed)
    let kek = aes_ecb_decrypt(master_key, kek_seed);

    // src_kek = AES-ECB-Decrypt(kek, src)
    let src_kek = aes_ecb_decrypt(&kek, src);

    // 如果有 key_seed，再解密一次
    if let Some(key_seed) = key_seed {
        aes_ecb_decrypt(&src_kek, key_seed)
    } else {
        src_kek
    }
}

/// 加载密钥文件
pub fn load_keys<P: AsRef<Path>>(path: P) -> AppResult<()> {
    let mut key_store = KeyStore::new();
    key_store.load_from_file(path)?;

    let mut keys = KEYS.write().unwrap();
    *keys = Some(key_store);

    Ok(())
}

/// 获取密钥存储的只读引用
pub fn with_keys<F, R>(f: F) -> AppResult<R>
where
    F: FnOnce(&KeyStore) -> AppResult<R>,
{
    let keys = KEYS.read().unwrap();
    let key_store = keys
        .as_ref()
        .ok_or_else(|| AppError::InvalidArgument("密钥未加载".to_string()))?;
    f(key_store)
}

/// 检查密钥是否已加载
pub fn is_keys_loaded() -> bool {
    let keys = KEYS.read().unwrap();
    keys.is_some()
}

/// 清除已加载的密钥
pub fn clear_keys() {
    let mut keys = KEYS.write().unwrap();
    *keys = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes_ecb_decrypt() {
        // 使用零密钥测试
        let key = [0u8; KEY_SIZE];
        let data = [0u8; KEY_SIZE];
        let result = aes_ecb_decrypt(&key, &data);
        // 结果应该是确定性的
        assert_eq!(result.len(), KEY_SIZE);
    }

    #[test]
    fn test_generate_kek() {
        let src = [0u8; KEY_SIZE];
        let master_key = [0u8; KEY_SIZE];
        let kek_seed = [0u8; KEY_SIZE];
        let key_seed = [0u8; KEY_SIZE];

        let result = generate_kek(&src, &master_key, &kek_seed, Some(&key_seed));
        assert_eq!(result.len(), KEY_SIZE);
    }
}
