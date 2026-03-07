//! Nintendo Switch 加密模块
//!
//! 实现 NCA 文件所需的各种加密模式：AES-XTS、AES-CTR、AES-ECB

use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes128;

/// AES 块大小
const BLOCK_SIZE: usize = 16;

/// XTS 扇区大小
const XTS_SECTOR_SIZE: usize = 0x200;

/// XOR 两个字节数组
fn xor_bytes(a: &[u8], b: &[u8]) -> Vec<u8> {
    a.iter().zip(b.iter()).map(|(x, y)| x ^ y).collect()
}

/// AES-XTS 解密器
///
/// 用于解密 NCA 头部（使用 header_key）
pub struct AesXts {
    /// 第一个密钥（用于数据加密）
    cipher1: Aes128,
    /// 第二个密钥（用于 tweak 加密）
    cipher2: Aes128,
    /// 当前扇区号
    sector: u64,
    /// 扇区大小
    sector_size: usize,
}

impl AesXts {
    /// 创建新的 AES-XTS 解密器
    ///
    /// keys: 32 字节密钥，前 16 字节用于数据，后 16 字节用于 tweak
    pub fn new(keys: &[u8; 32]) -> Self {
        let cipher1 = Aes128::new_from_slice(&keys[0..16]).unwrap();
        let cipher2 = Aes128::new_from_slice(&keys[16..32]).unwrap();

        Self {
            cipher1,
            cipher2,
            sector: 0,
            sector_size: XTS_SECTOR_SIZE,
        }
    }

    /// 设置扇区号
    pub fn set_sector(&mut self, sector: u64) {
        self.sector = sector;
    }

    /// 计算 tweak 值
    fn get_tweak(&self, sector: u64) -> u128 {
        let mut tweak: u128 = 0;
        let mut s = sector;
        for i in 0..BLOCK_SIZE {
            tweak |= ((s & 0xFF) as u128) << (i * 8);
            s >>= 8;
        }
        tweak
    }

    /// 解密数据
    pub fn decrypt(&mut self, data: &[u8]) -> Vec<u8> {
        self.decrypt_with_sector(data, self.sector)
    }

    /// 使用指定扇区号解密数据
    pub fn decrypt_with_sector(&self, data: &[u8], mut sector: u64) -> Vec<u8> {
        if data.len() % BLOCK_SIZE != 0 {
            panic!("数据长度必须是 16 的倍数");
        }

        let mut result = Vec::with_capacity(data.len());
        let mut pos = 0;

        while pos < data.len() {
            let sector_end = std::cmp::min(pos + self.sector_size, data.len());
            let sector_data = &data[pos..sector_end];
            let decrypted_sector = self.decrypt_sector(sector_data, sector);
            result.extend_from_slice(&decrypted_sector);
            pos = sector_end;
            sector += 1;
        }

        result
    }

    /// 解密单个扇区
    fn decrypt_sector(&self, data: &[u8], sector: u64) -> Vec<u8> {
        if data.len() % BLOCK_SIZE != 0 {
            panic!("扇区数据长度必须是 16 的倍数");
        }

        let mut result = Vec::with_capacity(data.len());

        // 计算初始 tweak（小端序表示的 sector number）
        // 按照 Python 实现：tweak = unhexlify('%032X' % sector_as_le_int)
        // 对于 sector = 0: tweak_bytes = [0; 16]
        // 对于 sector = 1: tweak_bytes = [0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1]
        let tweak_value = self.get_tweak(sector);
        let tweak_bytes = tweak_value.to_be_bytes();

        // 加密 tweak（使用 K2/cipher2）
        let mut tweak_block = aes::Block::clone_from_slice(&tweak_bytes);
        self.cipher2.encrypt_block(&mut tweak_block);
        let mut tweak: Vec<u8> = tweak_block.to_vec();

        for chunk in data.chunks(BLOCK_SIZE) {
            // XOR with tweak
            let xored: Vec<u8> = xor_bytes(chunk, &tweak);

            // Decrypt
            let mut block = aes::Block::clone_from_slice(&xored);
            self.cipher1.decrypt_block(&mut block);

            // XOR with tweak again
            let decrypted = xor_bytes(&block, &tweak);
            result.extend_from_slice(&decrypted);

            // 更新 tweak (GF(2^128) 乘法)
            tweak = gf128_mul_by_2(&tweak);
        }

        result
    }
}

/// GF(2^128) 中乘以 2
///
/// 按照 XTS 标准实现 GF(2^128) 乘法
/// tweak 被当作小端序整数处理（byte[0] 是最低有效位）
fn gf128_mul_by_2(tweak: &[u8]) -> Vec<u8> {
    let mut result = vec![0u8; BLOCK_SIZE];
    let mut carry = 0u8;

    // 从最低有效字节开始处理（小端序中是第一个字节）
    for i in 0..BLOCK_SIZE {
        let new_carry = (tweak[i] >> 7) & 1;
        result[i] = (tweak[i] << 1) | carry;
        carry = new_carry;
    }

    // 如果最高位溢出，XOR 0x87 在最低有效字节（byte[0]）
    if carry != 0 {
        result[0] ^= 0x87;
    }

    result
}

/// AES-CTR 解密器
///
/// 用于解密 NCA section 数据
pub struct AesCtr {
    /// AES 密钥
    cipher: Aes128,
    /// Nonce（前 8 字节）
    nonce: [u8; 8],
}

impl AesCtr {
    /// 创建新的 AES-CTR 解密器
    pub fn new(key: &[u8; 16], nonce: &[u8]) -> Self {
        let cipher = Aes128::new_from_slice(key).unwrap();
        let mut nonce_arr = [0u8; 8];
        nonce_arr.copy_from_slice(&nonce[0..8]);

        Self {
            cipher,
            nonce: nonce_arr,
        }
    }

    /// 从 crypto counter 创建
    pub fn from_counter(key: &[u8; 16], counter: &[u8; 16]) -> Self {
        let cipher = Aes128::new_from_slice(key).unwrap();
        let mut nonce = [0u8; 8];
        // counter 是大端序，前 8 字节是 nonce
        nonce.copy_from_slice(&counter[0..8]);

        Self { cipher, nonce }
    }

    /// 解密数据
    pub fn decrypt(&self, data: &[u8], offset: u64) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        let block_offset = offset / BLOCK_SIZE as u64;
        let byte_offset = (offset % BLOCK_SIZE as u64) as usize;

        let mut counter = self.get_counter(block_offset);
        let mut keystream_pos = byte_offset;
        let mut keystream = self.generate_keystream(&counter);

        for &byte in data {
            if keystream_pos >= BLOCK_SIZE {
                // 移动到下一个块
                counter = self.increment_counter(&counter);
                keystream = self.generate_keystream(&counter);
                keystream_pos = 0;
            }

            result.push(byte ^ keystream[keystream_pos]);
            keystream_pos += 1;
        }

        result
    }

    /// 获取指定块的 counter
    fn get_counter(&self, block_offset: u64) -> [u8; 16] {
        let mut counter = [0u8; 16];
        counter[0..8].copy_from_slice(&self.nonce);

        // 后 8 字节是大端序的块偏移
        let offset_bytes = block_offset.to_be_bytes();
        counter[8..16].copy_from_slice(&offset_bytes);

        counter
    }

    /// 递增 counter
    fn increment_counter(&self, counter: &[u8; 16]) -> [u8; 16] {
        let mut result = *counter;

        // 从最后一个字节开始递增（大端序）
        for i in (8..16).rev() {
            result[i] = result[i].wrapping_add(1);
            if result[i] != 0 {
                break;
            }
        }

        result
    }

    /// 生成密钥流
    fn generate_keystream(&self, counter: &[u8; 16]) -> [u8; 16] {
        let mut block = aes::Block::clone_from_slice(counter);
        self.cipher.encrypt_block(&mut block);
        let mut result = [0u8; 16];
        result.copy_from_slice(&block);
        result
    }
}

/// AES-ECB 解密
pub fn aes_ecb_decrypt(key: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let cipher = Aes128::new_from_slice(key).unwrap();
    let mut result = data.to_vec();

    for chunk in result.chunks_mut(BLOCK_SIZE) {
        if chunk.len() == BLOCK_SIZE {
            let mut block = aes::Block::clone_from_slice(chunk);
            cipher.decrypt_block(&mut block);
            chunk.copy_from_slice(&block);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gf128_mul_by_2() {
        let tweak = vec![0x01u8; 16];
        let result = gf128_mul_by_2(&tweak);
        assert_eq!(result.len(), 16);
    }

    #[test]
    fn test_aes_xts_basic() {
        let keys = [0u8; 32];
        let xts = AesXts::new(&keys);

        let data = [0u8; 512]; // 一个完整扇区
        let decrypted = xts.decrypt_with_sector(&data, 0);
        assert_eq!(decrypted.len(), 512);
    }

    #[test]
    fn test_aes_ctr_basic() {
        let key = [0u8; 16];
        let nonce = [0u8; 8];
        let ctr = AesCtr::new(&key, &nonce);

        let data = [0u8; 32];
        let decrypted = ctr.decrypt(&data, 0);
        assert_eq!(decrypted.len(), 32);
    }

    #[test]
    fn test_aes_ecb_decrypt() {
        let key = [0u8; 16];
        let data = [0u8; 16];
        let result = aes_ecb_decrypt(&key, &data);
        assert_eq!(result.len(), 16);
    }
}
