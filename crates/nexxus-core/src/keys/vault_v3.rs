//! P0-3: vault format v3 — пароль-фраза (passphrase) + Argon2id + decoy-записи.
//!
//! Формат: JSON с массивом `entries`. 一つ реальная запись (ключ выведен из
//! Argon2id(passphrase || pin)), остальные — decoy с случайными ключами и
//! верификаторами. Атакующий по файлу не отличает настоящую запись от decoy:
//! все записи имеют одинаковую структуру, verifier проверяется в constant time,
//! неправильная запись даёт такую же ошибку, как и decoy.
//!
//! Требования аудита P0-3: Argon2id (медленный офлайн-перебор), пароль-фраза
//! ≥ 10 символов, decoy entries (пlausible deniability по структуре файла).

use aes_gcm::{Aes256Gcm, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use argon2::{Algorithm, Argon2, Params, Version};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use rand::Rng;
use zeroize::{Zeroize, Zeroizing};

use crate::error::NexxusError;

pub const VAULT_V3_FORMAT: &str = "nexxus-vault-v3";
/// P0-3: минимальная длина пароль-фразы.
pub const MIN_PASSPHRASE_LEN: usize = 10;
/// P0-3: Argon2id(m=64 MiB, t=3, p=1) — как panic PIN (P0-1).
pub const ARGON2_M_KIB: u32 = 65_536;
pub const ARGON2_T: u32 = 3;
pub const ARGON2_P: u32 = 1;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultV3Entry {
    pub ciphertext_hex: String,
    pub iv_hex: String,
    pub salt_hex: String,
    /// HMAC-SHA256(verifier_key, b"nexxus-vault-v3-verifier") — не равен ключу шифрования.
    pub verifier_hex: String,
    #[serde(default)]
    pub decoy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultV3 {
    pub format: String,
    pub entries: Vec<VaultV3Entry>,
    pub created_at: u64,
    pub argon2_m: u32,
    pub argon2_t: u32,
    pub argon2_p: u32,
}

fn argon2_params(m: u32, t: u32, p: u32) -> Result<Params, NexxusError> {
    // argon2 клемпит m к >= 8*p; defend от слишком маленьких значений из файла
    let m = m.max(8 * p.max(1));
    Params::new(m, t.max(1), p.max(1), Some(32))
        .map_err(|e| NexxusError::Vault(format!("argon2 params: {e}")))
}

/// Ключ шифрования + верификатор из пароль-фразы и PIN (оба фактора нужны).
fn derive_v3(passphrase: &str, pin: &str, salt: &[u8], m: u32, t: u32, p: u32) -> Result<(Zeroizing<[u8; 32]>, [u8; 32]), NexxusError> {
    let params = argon2_params(m, t, p)?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    // passphrase || 0x00 || pin — разделитель, чтобы конкатенация была однозначной
    let mut input = Zeroizing::new(Vec::with_capacity(passphrase.len() + pin.len() + 1));
    input.extend_from_slice(passphrase.as_bytes());
    input.push(0);
    input.extend_from_slice(pin.as_bytes());
    let mut master = Zeroizing::new([0u8; 32]);
    argon.hash_password_into(&input, salt, &mut *master)
        .map_err(|e| NexxusError::Vault(format!("argon2id: {e}")))?;
    // domain separation: enc key и verifier криптографически независимы
    let enc_key = hmac_expand(&*master, b"nexxus-vault-v3-encryption-key");
    let verifier = hmac_expand(&*master, b"nexxus-vault-v3-pin-verifier");
    Ok((Zeroizing::new(enc_key), verifier))
}

fn hmac_expand(master: &[u8], info: &[u8]) -> [u8; 32] {
    let mut mac = <HmacSha256 as Mac>::new_from_slice(master).expect("HMAC any size");
    mac.update(info);
    let mut out = [0u8; 32];
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

fn ct_eq_hex(a: &str, b: &str) -> bool {
    let (Ok(x), Ok(y)) = (hex::decode(a), hex::decode(b)) else { return false };
    x.ct_eq(&y).into()
}

/// P0-3: passphrase короче 10 символов — отказ (слабая фраза не допускается).
pub fn validate_passphrase(passphrase: &str) -> Result<(), NexxusError> {
    if passphrase.chars().count() < MIN_PASSPHRASE_LEN {
        return Err(NexxusError::Vault(format!(
            "passphrase too short: {} < {MIN_PASSPHRASE_LEN} chars",
            passphrase.chars().count()
        )));
    }
    Ok(())
}

fn make_entry(
    key: &[u8; 32],
    verifier: &[u8; 32],
    salt: &[u8; SALT_LEN],
    payload: &[u8],
    decoy: bool,
) -> Result<VaultV3Entry, NexxusError> {
    let mut rng = rand::thread_rng();
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rng.fill(&mut nonce_bytes[..]);
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| NexxusError::Vault(format!("cipher init: {e}")))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), payload)
        .map_err(|e| NexxusError::Vault(format!("encrypt: {e}")))?;
    Ok(VaultV3Entry {
        ciphertext_hex: hex::encode(ciphertext),
        iv_hex: hex::encode(nonce_bytes),
        salt_hex: hex::encode(salt),
        verifier_hex: hex::encode(verifier),
        decoy,
    })
}

/// Создаёт vault v3: одна реальная запись + `decoy_count` decoy (по умолчанию 3).
/// decoy шифруются случайными ключами — структурно неотличимы от настоящей.
pub fn create_vault_v3(
    payload: &[u8],
    pin: &str,
    passphrase: &str,
    decoy_count: usize,
) -> Result<String, NexxusError> {
    create_vault_v3_params(payload, pin, passphrase, decoy_count, ARGON2_M_KIB, ARGON2_T, ARGON2_P)
}

pub fn create_vault_v3_params(
    payload: &[u8],
    pin: &str,
    passphrase: &str,
    decoy_count: usize,
    m: u32,
    t: u32,
    p: u32,
) -> Result<String, NexxusError> {
    validate_passphrase(passphrase)?;
    let mut rng = rand::thread_rng();

    let mut salt = [0u8; SALT_LEN];
    rng.fill(&mut salt[..]);
    let (mut key, verifier) = derive_v3(passphrase, pin, &salt, m, t, p)?;
    let real = make_entry(&key, &verifier, &salt, payload, false)?;
    key.zeroize();

    let mut entries = vec![real];
    for _ in 0..decoy_count {
        let mut dkey = [0u8; 32];
        let mut dsalt = [0u8; SALT_LEN];
        let mut dver = [0u8; 32];
        let mut dpayload = vec![0u8; payload.len().max(32)];
        rng.fill(&mut dkey[..]);
        rng.fill(&mut dsalt[..]);
        rng.fill(&mut dver[..]);
        rng.fill(&mut dpayload[..]);
        // decoy шифруем своим случайным ключом, но в ту же структуру
        let entry = make_entry(&dkey, &dver, &dsalt, &dpayload, true)?;
        entries.push(entry);
    }
    // перемешиваем, чтобы реальная запись не была всегда первой
    use rand::seq::SliceRandom;
    entries.shuffle(&mut rng);

    let vault = VaultV3 {
        format: VAULT_V3_FORMAT.to_string(),
        entries,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        argon2_m: m,
        argon2_t: t,
        argon2_p: p,
    };
    serde_json::to_string(&vault).map_err(|e| NexxusError::Vault(format!("serialize: {e}")))
}

/// Открывает vault v3: перебирает все entries, у каждой проверяет verifier (ct_eq),
/// затем расшифровывает. Decoy дают Wrong — indistinguishable от неверного PIN.
pub fn open_vault_v3(vault_json: &str, pin: &str, passphrase: &str) -> Result<Vec<u8>, NexxusError> {
    let vault: VaultV3 = serde_json::from_str(vault_json)
        .map_err(|e| NexxusError::Vault(format!("invalid vault v3 JSON: {e}")))?;
    if vault.format != VAULT_V3_FORMAT {
        return Err(NexxusError::Vault("not a v3 vault".into()));
    }
    if vault.entries.is_empty() {
        return Err(NexxusError::Vault("vault v3 has no entries".into()));
    }
    // DoS-guard: параметры из файла клемпим (как P1-6 для PBKDF2)
    let m = vault.argon2_m.clamp(8, 1_048_576); // ≤ 1 GiB
    let t = vault.argon2_t.clamp(1, 64);
    let p = vault.argon2_p.clamp(1, 8);

    for entry in &vault.entries {
        let salt = match hex::decode(&entry.salt_hex) {
            Ok(s) if s.len() == SALT_LEN => s,
            _ => continue,
        };
        let iv = match hex::decode(&entry.iv_hex) {
            Ok(v) if v.len() == NONCE_LEN => v,
            _ => continue,
        };
        let ct = match hex::decode(&entry.ciphertext_hex) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let (mut key, verifier) = match derive_v3(passphrase, pin, &salt, m, t, p) {
            Ok(k) => k,
            Err(_) => continue,
        };
        if !ct_eq_hex(&hex::encode(verifier), &entry.verifier_hex) {
            key.zeroize();
            continue;
        }
        let cipher = match Aes256Gcm::new_from_slice(&*key) {
            Ok(c) => c,
            Err(_) => { key.zeroize(); continue; }
        };
        match cipher.decrypt(Nonce::from_slice(&iv), ct.as_ref()) {
            Ok(plain) => {
                key.zeroize();
                return Ok(plain);
            }
            Err(_) => {
                key.zeroize();
                continue;
            }
        }
    }
    Err(NexxusError::Vault("wrong pin/passphrase or corrupted vault".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // быстрые параметры для тестов (8 MiB, t=1)
    const TM: u32 = 8 * 1024;

    #[test]
    fn v3_roundtrip_and_decoys_present() {
        let json = create_vault_v3_params(b"secret seed payload", "123456", "correct horse battery", 3, TM, 1, 1).unwrap();
        let v: VaultV3 = serde_json::from_str(&json).unwrap();
        assert_eq!(v.entries.len(), 4);
        assert_eq!(v.entries.iter().filter(|e| e.decoy).count(), 3);
        let opened = open_vault_v3(&json, "123456", "correct horse battery").unwrap();
        assert_eq!(opened, b"secret seed payload");
    }

    #[test]
    fn v3_wrong_passphrase_fails_like_decoy() {
        let json = create_vault_v3_params(b"payload", "123456", "correct horse battery", 2, TM, 1, 1).unwrap();
        let err = open_vault_v3(&json, "123456", "wrong passphrase here").unwrap_err();
        assert!(matches!(err, NexxusError::Vault(_)));
    }

    #[test]
    fn v3_short_passphrase_rejected() {
        let r = create_vault_v3_params(b"x", "123456", "short", 0, TM, 1, 1);
        assert!(r.is_err());
        assert!(validate_passphrase("1234567890").is_ok());
        assert!(validate_passphrase("123456789").is_err());
    }

    #[test]
    fn v3_wrong_pin_fails() {
        let json = create_vault_v3_params(b"payload", "123456", "correct horse battery", 1, TM, 1, 1).unwrap();
        assert!(open_vault_v3(&json, "999999", "correct horse battery").is_err());
    }
}
