use age::{Decryptor, Encryptor};
use secrecy::SecretString;
use std::fmt;
use std::io::{Read, Write};

#[derive(Debug)]
pub enum CryptoError {
    EncryptionFailed(String),
    DecryptionFailed(String),
    InvalidFormat,
    WrongMasterPassword,
}

impl fmt::Display for CryptoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CryptoError::EncryptionFailed(msg) => write!(f, "encryption failed: {}", msg),
            CryptoError::DecryptionFailed(msg) => write!(f, "decryption failed: {}", msg),
            CryptoError::InvalidFormat => write!(f, "invalid encrypted data format"),
            CryptoError::WrongMasterPassword => write!(f, "wrong master password"),
        }
    }
}

impl std::error::Error for CryptoError {}

pub fn encrypt(plaintext: &str, passphrase: &str) -> Result<String, CryptoError> {
    let passphrase = SecretString::new(passphrase.to_string());
    let encryptor = Encryptor::with_user_passphrase(passphrase);

    let mut encrypted = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut encrypted)
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;
    writer
        .write_all(plaintext.as_bytes())
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;
    writer
        .finish()
        .map_err(|e| CryptoError::EncryptionFailed(e.to_string()))?;

    Ok(base64_encode(&encrypted))
}

pub fn decrypt(encrypted: &str, passphrase: &str) -> Result<String, CryptoError> {
    let data = base64_decode(encrypted)?;

    let decryptor = Decryptor::new(data.as_slice())
        .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;

    match decryptor {
        Decryptor::Passphrase(d) => {
            let passphrase = SecretString::new(passphrase.to_string());
            let mut reader = d
                .decrypt(&passphrase, None)
                .map_err(|e| CryptoError::DecryptionFailed(format!("{} (wrong master password?)", e)))?;
            let mut plaintext = String::new();
            reader
                .read_to_string(&mut plaintext)
                .map_err(|e| CryptoError::DecryptionFailed(e.to_string()))?;
            Ok(plaintext)
        }
        Decryptor::Recipients(_) => Err(CryptoError::InvalidFormat),
    }
}

pub trait PasswordProvider: Send + Sync {
    fn save_password(&self, key: &str, password: &str, master_password: &str) -> Result<(), CryptoError>;
    fn get_password(&self, key: &str, master_password: &str) -> Result<String, CryptoError>;
    fn delete_password(&self, key: &str) -> Result<(), CryptoError>;
}

pub struct AgeProvider {
    store: std::collections::HashMap<String, String>,
}

impl AgeProvider {
    pub fn new() -> Self {
        Self {
            store: std::collections::HashMap::new(),
        }
    }

    pub fn load_encrypted_entries(&mut self, entries: std::collections::HashMap<String, String>) {
        self.store = entries;
    }

    pub fn encrypted_entries(&self) -> &std::collections::HashMap<String, String> {
        &self.store
    }
}

impl PasswordProvider for AgeProvider {
    fn save_password(&self, _key: &str, _password: &str, _master_password: &str) -> Result<(), CryptoError> {
        Ok(())
    }

    fn get_password(&self, key: &str, master_password: &str) -> Result<String, CryptoError> {
        let encrypted = self
            .store
            .get(key)
            .ok_or_else(|| CryptoError::DecryptionFailed(format!("key '{}' not found", key)))?;
        decrypt(encrypted, master_password)
    }

    fn delete_password(&self, _key: &str) -> Result<(), CryptoError> {
        Ok(())
    }
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        result.push(
            if chunk.len() > 1 {
                CHARS[((triple >> 6) & 0x3F) as usize] as char
            } else {
                '='
            },
        );
        result.push(if chunk.len() > 2 {
            CHARS[(triple & 0x3F) as usize] as char
        } else {
            '='
        });
    }
    result
}

fn base64_decode(input: &str) -> Result<Vec<u8>, CryptoError> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = input.trim();
    let mut result = Vec::with_capacity(input.len() * 3 / 4);
    let chars: Vec<u8> = input
        .bytes()
        .filter(|&b| b != b'=' && b != b'\n' && b != b'\r')
        .collect();

    for chunk in chars.chunks(4) {
        let mut acc: u32 = 0;
        let mut bits = 0;
        for &b in chunk {
            let val = CHARS
                .iter()
                .position(|&c| c == b)
                .ok_or(CryptoError::InvalidFormat)?;
            acc = (acc << 6) | val as u32;
            bits += 6;
        }
        if bits >= 8 {
            result.push(((acc >> (bits - 8)) & 0xFF) as u8);
        }
        if bits >= 16 {
            result.push(((acc >> (bits - 16)) & 0xFF) as u8);
        }
        if bits >= 24 {
            result.push(((acc >> (bits - 24)) & 0xFF) as u8);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let encrypted = encrypt("ssh_password_123", "master_pass").unwrap();
        let decrypted = decrypt(&encrypted, "master_pass").unwrap();
        assert_eq!(decrypted, "ssh_password_123");
    }

    #[test]
    fn test_encrypt_decrypt_different_passwords() {
        let encrypted = encrypt("secret", "password1").unwrap();
        let result = decrypt(&encrypted, "password2");
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let e1 = encrypt("same_password", "master").unwrap();
        let e2 = encrypt("same_password", "master").unwrap();
        assert_ne!(e1, e2);
    }

    #[test]
    fn test_decrypt_invalid_base64() {
        let result = decrypt("not valid base64!!!", "master");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_invalid_age_data() {
        let short = base64_encode(&[0u8; 10]);
        let result = decrypt(&short, "master");
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_empty_string() {
        let encrypted = encrypt("", "master").unwrap();
        let decrypted = decrypt(&encrypted, "master").unwrap();
        assert_eq!(decrypted, "");
    }

    #[test]
    fn test_encrypt_unicode_password() {
        let plaintext = "密码测试🔐";
        let encrypted = encrypt(plaintext, "master").unwrap();
        let decrypted = decrypt(&encrypted, "master").unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_long_password() {
        let long_pw = "a".repeat(10000);
        let encrypted = encrypt(&long_pw, "master").unwrap();
        let decrypted = decrypt(&encrypted, "master").unwrap();
        assert_eq!(decrypted, long_pw);
    }

    #[test]
    fn test_age_provider_encrypt_decrypt() {
        let mut provider = AgeProvider::new();
        let encrypted = encrypt("my_ssh_pass", "master123").unwrap();
        provider.store.insert("prod-web".to_string(), encrypted);

        let password = provider.get_password("prod-web", "master123").unwrap();
        assert_eq!(password, "my_ssh_pass");
    }

    #[test]
    fn test_age_provider_key_not_found() {
        let provider = AgeProvider::new();
        let result = provider.get_password("nonexist", "master");
        assert!(result.is_err());
    }

    #[test]
    fn test_age_provider_wrong_master() {
        let mut provider = AgeProvider::new();
        let encrypted = encrypt("secret", "correct").unwrap();
        provider.store.insert("key".to_string(), encrypted);

        let result = provider.get_password("key", "wrong");
        assert!(result.is_err());
    }

    #[test]
    fn test_age_provider_load_entries() {
        let mut provider = AgeProvider::new();
        let encrypted = encrypt("password1", "master").unwrap();

        let mut entries = std::collections::HashMap::new();
        entries.insert("alias".to_string(), encrypted);
        provider.load_encrypted_entries(entries);

        let result = provider.get_password("alias", "master").unwrap();
        assert_eq!(result, "password1");
    }

    #[test]
    fn test_age_provider_encrypted_entries() {
        let mut provider = AgeProvider::new();
        let encrypted = encrypt("pw", "master").unwrap();
        provider.store.insert("key".to_string(), encrypted.clone());

        let entries = provider.encrypted_entries();
        assert_eq!(entries.get("key").unwrap(), &encrypted);
    }

    #[test]
    fn test_base64_roundtrip() {
        let data = vec![0u8, 1, 2, 255, 254, 253];
        let encoded = base64_encode(&data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_base64_empty() {
        let encoded = base64_encode(&[]);
        let decoded = base64_decode(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_base64_single_byte() {
        let data = vec![42u8];
        let encoded = base64_encode(&data);
        let decoded = base64_decode(&encoded).unwrap();
        assert_eq!(data, decoded);
    }

    #[test]
    fn test_crypto_error_display() {
        assert!(CryptoError::EncryptionFailed("test".into())
            .to_string()
            .contains("test"));
        assert!(CryptoError::DecryptionFailed("test".into())
            .to_string()
            .contains("test"));
        assert!(!CryptoError::InvalidFormat.to_string().is_empty());
        assert!(!CryptoError::WrongMasterPassword.to_string().is_empty());
    }
}
