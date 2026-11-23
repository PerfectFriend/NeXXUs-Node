//! Panic PIN (Phase A3): dual-PIN (основной + duress) с экспоненциальным
//! кулдауном. Состояние — в `panic.state` рядом со store (0600), не в vault.
//!
//! Кулдаун: 60с → 120с → 240с … cap 24ч, сброс только верным PIN.
//! Duress-PIN → ложный успех + необратимый wipe стора (seed внутри стора).
//! Формат хеша PIN — как в BIP39-ABOARD-SDK: SHA-256(PIN || salt) hex
//! (nexxus_core::keys::vault::hash_pin), поэтому vault и panic.state
//! совместимы по hash_pin.

use nexxus_core::error::NexxusError;
use nexxus_core::keys::vault::hash_pin;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PANIC_STATE_FILE: &str = "panic.state";
const SALT_LEN: usize = 16;
/// Первый кулдаун после неверного PIN (сек).
pub const COOLDOWN_START_SECS: u64 = 60;
/// Потолок кулдауна: 24 часа.
pub const COOLDOWN_CAP_SECS: u64 = 24 * 3600;

#[derive(Serialize, Deserialize, Clone)]
struct StateFile {
    salt_hex: String,
    /// P0-1 (аудит 2026-11-23): legacy SHA-256(PIN||salt) — оставлен только для
    /// миграции старых файлов; новые arm() пишут только phc-поля.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pin_hash_hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duress_hash_hex: Option<String>,
    /// P0-1: Argon2id PHC-строка (m=64 MiB, t=3) — медленный офлайн-перебор.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pin_phc: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    duress_phc: Option<String>,
    fail_count: u64,
    /// Unix-секунды до какого момента кулдаун; 0 = нет кулдауна.
    cooldown_until: u64,
}

/// P0-1: Argon2id(m=64 MiB, t=3, p=1) PHC-хеш PIN. ~64 MiB RAM на проверку —
/// 4-8-значный PIN больше не подбирается "за секунды".
fn argon2_phc(pin: &str, salt: &[u8]) -> Result<String, NexxusError> {
    use argon2::{password_hash::{PasswordHasher, SaltString}, Argon2, Params};
    let params = Params::new(65536, 3, 1, Some(32))
        .map_err(|e| NexxusError::Vault(format!("argon2 params: {e}")))?;
    let salt_str = SaltString::encode_b64(salt)
        .map_err(|e| NexxusError::Vault(format!("argon2 salt: {e}")))?;
    let argon = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let hash = argon.hash_password(pin.as_bytes(), &salt_str)
        .map_err(|e| NexxusError::Vault(format!("argon2 hash: {e}")))?;
    Ok(hash.to_string())
}

/// Проверка PIN по phc (constant-time внутри argon2-крейта).
fn argon2_verify(pin: &str, phc: &str) -> bool {
    use argon2::password_hash::{PasswordHash, PasswordVerifier};
    match PasswordHash::new(phc) {
        Ok(h) => argon2::Argon2::default().verify_password(pin.as_bytes(), &h).is_ok(),
        Err(_) => false,
    }
}

/// Результат `enter`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnterOutcome {
    /// Верный PIN: кулдаун сброшен.
    Ok,
    /// Введён duress-PIN: надо wipe + отвечать ложным успехом.
    Duress,
    /// Неверный PIN, начался/продолжился кулдаун.
    Wrong { cooldown_secs: u64 },
    /// Кулдаун ещё активен, PIN не проверялся.
    Cooldown { remaining_secs: u64 },
}

/// Статус для `panic_status`.
#[derive(Debug, Clone, Serialize)]
pub struct PanicStatus {
    pub armed: bool,
    pub fail_count: u64,
    pub cooldown_remaining_secs: u64,
}

pub struct PanicManager {
    path: PathBuf,
    state: Option<StateFile>,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// PIN: 4–8 цифр.
pub fn validate_pin(pin: &str) -> Result<(), NexxusError> {
    if pin.len() >= 4 && pin.len() <= 8 && pin.bytes().all(|b| b.is_ascii_digit()) {
        Ok(())
    } else {
        Err(NexxusError::Vault("PIN must be 4-8 digits".into()))
    }
}

impl PanicManager {
    /// Загружает состояние из файла (или создаёт "не armed").
    pub fn load(path: PathBuf) -> Result<Self, NexxusError> {
        let state = match std::fs::read_to_string(&path) {
            Ok(content) => Some(serde_json::from_str(&content).map_err(|e| {
                NexxusError::Vault(format!("panic.state повреждён: {e}"))
            })?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(NexxusError::Io(format!("read panic.state: {e}"))),
        };
        Ok(Self { path, state })
    }

    /// Путь по умолчанию: рядом со store (`~/.nexxus/panic.state`).
    pub fn default_path(store_path: &Path) -> PathBuf {
        store_path
            .parent()
            .map(|p| p.join(PANIC_STATE_FILE))
            .unwrap_or_else(|| PathBuf::from(PANIC_STATE_FILE))
    }

    pub fn armed(&self) -> bool {
        self.state.is_some()
    }

    fn save(&self, state: &StateFile) -> Result<(), NexxusError> {
        let json = serde_json::to_string(state)
            .map_err(|e| NexxusError::Vault(format!("сериализация panic.state: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&self.path)
                .map_err(|e| NexxusError::Io(format!("write panic.state: {e}")))?;
            use std::io::Write;
            f.write_all(json.as_bytes())
                .map_err(|e| NexxusError::Io(format!("write panic.state: {e}")))?;
        }
        #[cfg(not(unix))]
        {
            std::fs::write(&self.path, json).map_err(|e| NexxusError::Io(format!("write panic.state: {e}")))?;
        }
        Ok(())
    }

    /// Вооружает panic PIN (или меняет, если уже armed). duress_pin — опционально.
    pub fn arm(&mut self, pin: &str, duress_pin: Option<&str>) -> Result<(), NexxusError> {
        validate_pin(pin)?;
        if let Some(d) = duress_pin {
            validate_pin(d)?;
            if d == pin {
                return Err(NexxusError::Vault("duress PIN must differ from main PIN".into()));
            }
        }
        let mut salt = [0u8; SALT_LEN];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
        let state = StateFile {
            salt_hex: hex::encode(salt),
            // P0-1: новые файлы — только Argon2id, без быстрого SHA-256 верификатора
            pin_hash_hex: None,
            duress_hash_hex: None,
            pin_phc: Some(argon2_phc(pin, &salt)?),
            duress_phc: match duress_pin {
                Some(d) => Some(argon2_phc(d, &salt)?),
                None => None,
            },
            fail_count: 0,
            cooldown_until: 0,
        };
        self.save(&state)?;
        self.state = Some(state);
        Ok(())
    }

    pub fn status(&self) -> PanicStatus {
        let now = now_unix();
        let armed = self.armed();
        match &self.state {
            None => PanicStatus { armed, fail_count: 0, cooldown_remaining_secs: 0 },
            Some(s) => PanicStatus {
                armed,
                fail_count: s.fail_count,
                cooldown_remaining_secs: s.cooldown_until.saturating_sub(now),
            },
        }
    }

    /// Проверяет PIN. `now` — unix-сек, вынесено для тестируемости.
    pub fn enter(&mut self, pin: &str, now: u64) -> Result<EnterOutcome, NexxusError> {
        let state = match &self.state {
            Some(s) => s.clone(),
            None => return Err(NexxusError::Vault("panic PIN is not armed".into())),
        };
        if now < state.cooldown_until {
            return Ok(EnterOutcome::Cooldown {
                remaining_secs: state.cooldown_until - now,
            });
        }
        let salt = hex::decode(&state.salt_hex)
            .map_err(|e| NexxusError::Vault(format!("невалидный salt: {e}")))?;
        let mut new_state = state.clone();
        // P0-1: сначала phc (Argon2id), legacy hash — только для миграции старого файла
        let duress_hit = match &new_state.duress_phc {
            Some(phc) => argon2_verify(pin, phc),
            None => new_state.duress_hash_hex.as_ref().is_some_and(|h| *h == hash_pin(pin, &salt)),
        };
        let pin_hit = match &new_state.pin_phc {
            Some(phc) => argon2_verify(pin, phc),
            None => new_state.pin_hash_hex.as_ref().is_some_and(|h| *h == hash_pin(pin, &salt)),
        };
        let outcome = if duress_hit && new_state.duress_phc.is_some() {
            // Duress: кулдаун не трогаем, отвечаем ложным успехом снаружи.
            EnterOutcome::Duress
        } else if pin_hit {
            // миграция legacy-файла: успешный вход → перезаписываем на Argon2id
            if new_state.pin_phc.is_none() {
                new_state.pin_phc = Some(argon2_phc(pin, &salt)?);
                new_state.pin_hash_hex = None;
                new_state.duress_hash_hex = None;
            }
            new_state.fail_count = 0;
            new_state.cooldown_until = 0;
            EnterOutcome::Ok
        } else {
            new_state.fail_count += 1;
            let shift = (new_state.fail_count - 1).min(20);
            let cooldown = COOLDOWN_START_SECS
                .saturating_mul(1u64 << shift)
                .min(COOLDOWN_CAP_SECS);
            new_state.cooldown_until = now + cooldown;
            EnterOutcome::Wrong { cooldown_secs: cooldown }
        };
        self.save(&new_state)?;
        self.state = Some(new_state);
        Ok(outcome)
    }
}

/// Необратимый wipe файла: перезапись нулями + удаление (best-effort zeroize
/// на диске перед remove — на SSD это best-effort, но файл исчезает гарантированно).
pub fn wipe_file(path: &Path) -> Result<(), NexxusError> {
    if let Ok(meta) = std::fs::metadata(path) {
        if let Ok(mut f) = std::fs::OpenOptions::new().write(true).open(path) {
            use std::io::Write;
            let zeros = vec![0u8; meta.len().min(1 << 20) as usize];
            let mut remaining = meta.len();
            while remaining > 0 {
                let n = remaining.min(zeros.len() as u64) as usize;
                if f.write_all(&zeros[..n]).is_err() {
                    break;
                }
                remaining -= n as u64;
            }
            let _ = f.sync_all();
        }
    }
    std::fs::remove_file(path).map_err(|e| NexxusError::Io(format!("wipe {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    #[test]
    fn panic_state_uses_argon2id_and_migrates_legacy() {
        // P0-1 регрессионный тест: arm() пишет phc (argon2id), без pin_hash_hex;
        // legacy-файл с pin_hash_hex после успешного enter мигрирует на phc.
        let path = tmp_state("argon2");
        let mut pm = PanicManager::load(path.clone()).unwrap();
        pm.arm("1234", Some("9999")).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("$argon2id$"), "ожидался argon2id phc: {raw}");
        assert!(!raw.contains("pin_hash_hex"), "legacy-хеш не должен писаться: {raw}");
        assert_eq!(pm.enter("1234", 1000).unwrap(), EnterOutcome::Ok);
        assert_eq!(pm.enter("9999", 1001).unwrap(), EnterOutcome::Duress);
        assert!(matches!(pm.enter("0000", 1002).unwrap(), EnterOutcome::Wrong { .. }));

        // legacy-файл (только pin_hash_hex) открывается и мигрирует
        let mut salt = [0u8; SALT_LEN];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut salt);
        let legacy = StateFile {
            salt_hex: hex::encode(salt),
            pin_hash_hex: Some(hash_pin("4321", &salt)),
            duress_hash_hex: None,
            pin_phc: None,
            duress_phc: None,
            fail_count: 0,
            cooldown_until: 0,
        };
        let path2 = tmp_state("legacy");
        let mut pm2 = PanicManager::load(path2.clone()).unwrap();
        pm2.save(&legacy).unwrap();
        pm2.state = Some(legacy);
        assert_eq!(pm2.enter("4321", 2000).unwrap(), EnterOutcome::Ok);
        let raw2 = std::fs::read_to_string(&path2).unwrap();
        assert!(raw2.contains("$argon2id$"), "legacy должен мигрировать на argon2id: {raw2}");
        assert!(!raw2.contains("pin_hash_hex"));
        assert_eq!(pm2.enter("4321", 2001).unwrap(), EnterOutcome::Ok);
    }

    use super::*;

    fn tmp_state(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nexxus-panic-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(PANIC_STATE_FILE)
    }

    #[test]
    fn arm_enter_ok_resets_cooldown() {
        let path = tmp_state("ok");
        let mut m = PanicManager::load(path.clone()).unwrap();
        assert!(!m.armed());
        m.arm("1234", Some("9999")).unwrap();
        assert!(m.armed());
        assert_eq!(m.enter("1234", 1000).unwrap(), EnterOutcome::Ok);
        assert_eq!(m.status().cooldown_remaining_secs, 0);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn wrong_pin_grows_exponential_cooldown() {
        let path = tmp_state("cool");
        let mut m = PanicManager::load(path.clone()).unwrap();
        m.arm("1234", None).unwrap();
        assert_eq!(m.enter("0000", 1000).unwrap(), EnterOutcome::Wrong { cooldown_secs: 60 });
        // Во время кулдауна PIN не проверяется вообще:
        assert_eq!(m.enter("1234", 1001).unwrap(), EnterOutcome::Cooldown { remaining_secs: 59 });
        // После кулдауна: снова неверный → 120с.
        assert_eq!(m.enter("0000", 1061).unwrap(), EnterOutcome::Wrong { cooldown_secs: 120 });
        assert_eq!(m.enter("0000", 1181).unwrap(), EnterOutcome::Wrong { cooldown_secs: 240 });
        // Верный PIN после кулдауна сбрасывает счётчик:
        assert_eq!(m.enter("1234", 1421).unwrap(), EnterOutcome::Ok);
        assert_eq!(m.enter("0000", 1422).unwrap(), EnterOutcome::Wrong { cooldown_secs: 60 });
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn cooldown_capped_at_24h() {
        let path = tmp_state("cap");
        let mut m = PanicManager::load(path.clone()).unwrap();
        m.arm("1234", None).unwrap();
        let mut now = 1000u64;
        for _ in 0..30 {
            match m.enter("0000", now).unwrap() {
                EnterOutcome::Wrong { cooldown_secs } => {
                    assert!(cooldown_secs <= COOLDOWN_CAP_SECS);
                    now += cooldown_secs;
                }
                other => panic!("expected Wrong, got {other:?}"),
            }
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn duress_pin_detected() {
        let path = tmp_state("duress");
        let mut m = PanicManager::load(path.clone()).unwrap();
        m.arm("1234", Some("9999")).unwrap();
        assert_eq!(m.enter("9999", 1000).unwrap(), EnterOutcome::Duress);
        // Основной PIN при этом тоже не палится:
        assert_eq!(m.enter("1234", 1000).unwrap(), EnterOutcome::Ok);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn state_persists_across_reload() {
        let path = tmp_state("persist");
        {
            let mut m = PanicManager::load(path.clone()).unwrap();
            m.arm("1234", None).unwrap();
            assert_eq!(m.enter("0000", 5000).unwrap(), EnterOutcome::Wrong { cooldown_secs: 60 });
        }
        let mut m = PanicManager::load(path.clone()).unwrap();
        assert!(m.armed());
        assert_eq!(m.enter("1234", 5001).unwrap(), EnterOutcome::Cooldown { remaining_secs: 59 });
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn enter_without_arm_is_error() {
        let path = tmp_state("unarmed");
        let mut m = PanicManager::load(path.clone()).unwrap();
        assert!(m.enter("1234", 0).is_err());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn pin_validation() {
        assert!(validate_pin("1234").is_ok());
        assert!(validate_pin("12345678").is_ok());
        assert!(validate_pin("123").is_err());
        assert!(validate_pin("123456789").is_err());
        assert!(validate_pin("12a4").is_err());
    }

    #[test]
    fn wipe_file_removes_content() {
        let dir = std::env::temp_dir().join(format!("nexxus-wipe-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("store.redb");
        std::fs::write(&f, b"seed-material-here").unwrap();
        wipe_file(&f).unwrap();
        assert!(!f.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn state_file_is_private() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = tmp_state("perms");
            let mut m = PanicManager::load(path.clone()).unwrap();
            m.arm("1234", None).unwrap();
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
            let _ = std::fs::remove_dir_all(path.parent().unwrap());
        }
    }
}
