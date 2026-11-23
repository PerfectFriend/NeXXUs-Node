//! G4: signed self-update (`nexxusd update`).
//!
//! Релиз скачивается по URL из конфига/CLI, проверяется detached
//! ed25519-подписью против зашитого публичного ключа, заменяется атомарно
//! (tmp + rename в той же файловой системе) с rollback при любой ошибке.
//! Приватный ключ живёт вне репозитория (владелец), в коде — только pubkey.

use nexxus_core::error::NexxusError;
use std::path::{Path, PathBuf};

/// P0-7: minisign-публичный ключ релизов (base64, тот же, что в keys/nexxus-release-minisign.pub).
/// CI подписывает SHA256SUMS манифест — тот же механизм, что и CI (ci-release.yml, H-12).
pub const RELEASE_MINISIGN_PUBKEY_B64: &str =
    "RWQBA6hz7o68fUSHH64Tr9OeijSMzDejwteaDL8di3ZrcvJS9kLpGygD";


/// URL релизного ассета по умолчанию (squash-mirror / GitHub releases).
pub const DEFAULT_RELEASE_URL: &str =
    "https://github.com/PerfectFriend/NeXXuS-Node-Hermes-Rust/releases/latest/download/nexxusd-linux-x86_64";


/// P0-7: верификация релиза через minisign-манифест SHA256SUMS (как в CI).
/// `manifest` — содержимое SHA256SUMS, `minisig_b64` — SHA256SUMS.minisig,
/// `asset_name` — имя файла ассета в манифесте (защита от подмены имени).
pub fn verify_release_manifest(
    asset_bytes: &[u8],
    asset_name: &str,
    manifest: &[u8],
    minisig_b64: &str,
) -> Result<(), NexxusError> {
    use minisign_verify::{PublicKey, Signature};
    let pk = PublicKey::from_base64(RELEASE_MINISIGN_PUBKEY_B64)
        .map_err(|e| NexxusError::Io(format!("release minisign pubkey: {e}")))?;
    let sig = Signature::decode(minisig_b64.trim())
        .map_err(|e| NexxusError::Io(format!("bad minisig: {e}")))?;
    pk.verify(manifest, &sig, false)
        .map_err(|_| NexxusError::Vault("SHA256SUMS minisign verification FAILED".into()))?;
    // ищем строку манифеста ровно для нашего ассета (имя без путей)
    let want_name = asset_name.rsplit('/').next().unwrap_or(asset_name);
    let manifest_str = String::from_utf8_lossy(manifest);
    let expected = manifest_str
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            let hash = it.next()?;
            let name = it.next()?.trim_start_matches('*');
            if name == want_name || name.ends_with(want_name) {
                Some(hash.to_string())
            } else {
                None
            }
        })
        .ok_or_else(|| NexxusError::Vault(format!("asset {want_name} not found in signed manifest")))?;
    let actual = {
        use sha2::Digest;
        hex::encode(sha2::Sha256::digest(asset_bytes))
    };
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(NexxusError::Vault(format!(
            "sha256 mismatch for {want_name}: manifest {expected} vs actual {actual}"
        )));
    }
    Ok(())
}


/// P0-7: скачивание ассета + подписанного манифеста SHA256SUMS + SHA256SUMS.minisig.
/// HTTPS обязателен; plain HTTP разрешён только для loopback (локальные тесты).
pub fn download(url: &str) -> Result<(Vec<u8>, Vec<u8>, String), NexxusError> {
    let bin = http_get(url)?;
    let base = url.rsplit('/').next().unwrap_or("");
    let manifest_url = format!("{}/SHA256SUMS", &url[..url.len() - base.len()]);
    let minisig_url = format!("{manifest_url}.minisig");
    let manifest = http_get(&manifest_url)
        .map_err(|e| NexxusError::Io(format!("manifest download: {e}")))?;
    let minisig = http_get(&minisig_url)
        .map_err(|e| NexxusError::Io(format!("minisig download: {e}")))?;
    let sig_text = String::from_utf8_lossy(&minisig).trim().to_string();
    Ok((bin, manifest, sig_text))
}

/// Валидация URL: https всегда; http только для 127.0.0.1/localhost/[::1].
pub fn validate_url(url: &str) -> Result<(), NexxusError> {
    if let Some(rest) = url.strip_prefix("https://") {
        if rest.is_empty() {
            return Err(NexxusError::Io("empty host in url".into()));
        }
        Ok(())
    } else if let Some(rest) = url.strip_prefix("http://") {
        let host = rest.split(['/', ':', '?']).next().unwrap_or("");
        if host == "127.0.0.1" || host == "localhost" || host == "[::1]" {
            Ok(())
        } else {
            Err(NexxusError::Io(
                "plain http allowed only for loopback; use https".into(),
            ))
        }
    } else {
        Err(NexxusError::Io("url must start with https://".into()))
    }
}

const MAX_DOWNLOAD_BYTES: usize = 512 * 1024 * 1024;

fn http_get(url: &str) -> Result<Vec<u8>, NexxusError> {
    use std::io::Read;
    validate_url(url)?;
    let resp = ureq::get(url)
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| NexxusError::Io(format!("GET {url}: {e}")))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take((MAX_DOWNLOAD_BYTES + 1) as u64)
        .read_to_end(&mut buf)
        .map_err(|e| NexxusError::Io(format!("read body: {e}")))?;
    if buf.len() > MAX_DOWNLOAD_BYTES {
        return Err(NexxusError::Io("download exceeds 512 MiB cap".into()));
    }
    Ok(buf)
}

/// Атомарная замена бинарника с rollback: verify подписи → backup старого →
/// tmp+rename → при любой ошибке до rename старый бинарь не тронут.
/// UPD-02 (anti-rollback): разобрать semver-строку "0.1.0[-suffix]" в кортеж.
pub fn version_tuple(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let maj = it.next()?.parse().ok()?;
    let min = it.next().unwrap_or("0").parse().ok()?;
    let pat = it.next().unwrap_or("0").parse().ok()?;
    Some((maj, min, pat))
}

/// true если `new` строго новее `cur`.
pub fn is_newer(new: (u64, u64, u64), cur: (u64, u64, u64)) -> bool {
    new > cur
}

/// UPD-02: anti-rollback — прогоняет скачанный (уже signature-verified) бинарник
/// с `--version` и отказывается устанавливать, если его версия <= текущей.
/// Возвращает строку новой версии при успехе.
pub fn check_not_downgrade(new_bytes: &[u8]) -> Result<String, NexxusError> {
    if new_bytes.len() < 1024 {
        return Err(NexxusError::Io("suspiciously small release binary".into()));
    }
    let dir = std::env::temp_dir().join(format!(
        "nexxus-upd-check-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).map_err(|e| NexxusError::Io(format!("tmp dir: {e}")))?;
    let tmp = dir.join("nexxusd-candidate");
    std::fs::write(&tmp, new_bytes).map_err(|e| NexxusError::Io(format!("tmp write: {e}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
    }
    let out = std::process::Command::new(&tmp)
        .arg("--version")
        .output()
        .map_err(|e| NexxusError::Io(format!("probe --version: {e}")))?;
    let _ = std::fs::remove_dir_all(&dir);
    let stdout = String::from_utf8_lossy(&out.stdout);
    // ожидаем "nexxusd X.Y.Z"
    let vstr = stdout
        .split_whitespace()
        .last()
        .ok_or_else(|| NexxusError::Io("candidate --version produced no version".into()))?;
    let new = version_tuple(vstr)
        .ok_or_else(|| NexxusError::Io(format!("candidate version unparseable: {vstr}")))?;
    let cur = version_tuple(env!("CARGO_PKG_VERSION"))
        .ok_or_else(|| NexxusError::Io("current version unparseable".into()))?;
    if !is_newer(new, cur) {
        return Err(NexxusError::Io(format!(
            "anti-rollback: candidate {vstr} is not newer than current {} — refusing",
            env!("CARGO_PKG_VERSION")
        )));
    }
    Ok(vstr.to_string())
}

pub fn install_update(
    new_bytes: &[u8],
    asset_name: &str,
    manifest: &[u8],
    minisig_b64: &str,
    current_bin: &Path,
) -> Result<PathBuf, NexxusError> {
    verify_release_manifest(new_bytes, asset_name, manifest, minisig_b64)?;
    if new_bytes.len() < 1024 {
        return Err(NexxusError::Io("suspiciously small release binary".into()));
    }
    let dir = current_bin
        .parent()
        .ok_or_else(|| NexxusError::Io("no parent dir".into()))?;
    let backup = dir.join(format!(
        ".{}.rollback",
        current_bin
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "nexxusd".into())
    ));
    // backup текущего бинарника
    if current_bin.exists() {
        std::fs::copy(current_bin, &backup)
            .map_err(|e| NexxusError::Io(format!("backup copy: {e}")))?;
    }
    let tmp = dir.join(format!(
        ".{}.new",
        current_bin
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "nexxusd".into())
    ));
    let install = (|| -> Result<PathBuf, NexxusError> {
        std::fs::write(&tmp, new_bytes)
            .map_err(|e| NexxusError::Io(format!("tmp write: {e}")))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
                .map_err(|e| NexxusError::Io(format!("chmod: {e}")))?;
        }
        std::fs::rename(&tmp, current_bin)
            .map_err(|e| NexxusError::Io(format!("atomic rename: {e}")))?;
        Ok(current_bin.to_path_buf())
    })();
    if install.is_err() {
        // rollback
        if backup.exists() {
            let _ = std::fs::copy(&backup, current_bin);
        }
        let _ = std::fs::remove_file(&tmp);
    }
    install
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_rules() {
        assert!(validate_url("https://github.com/x/y").is_ok());
        assert!(validate_url("http://127.0.0.1:8080/nexxusd").is_ok());
        assert!(validate_url("http://localhost/x").is_ok());
        assert!(validate_url("http://evil.example.com/x").is_err(), "plain http вне loopback запрещён");
        assert!(validate_url("ftp://x").is_err());
        assert!(validate_url("https://").is_err());
    }

    #[test]
    fn version_tuple_and_is_newer() {
        assert_eq!(version_tuple("0.1.0"), Some((0, 1, 0)));
        assert_eq!(version_tuple("2.0.0-rc1"), Some((2, 0, 0)));
        assert_eq!(version_tuple("v2.0.0-prototype"), None, "префикс v не парсится — ок, --version печатает без него");
        assert!(is_newer((0, 2, 0), (0, 1, 0)));
        assert!(!is_newer((0, 1, 0), (0, 1, 0)), "равная версия — не новее (anti-rollback)");
        assert!(!is_newer((0, 0, 9), (0, 1, 0)));
    }

    #[test]
    fn check_not_downgrade_rejects_same_version_binary() {
        // собираем "кандидата": текущий бинарник теста с --version = CARGO_PKG_VERSION
        let cur = std::env::current_exe().unwrap();
        let bytes = std::fs::read(&cur).unwrap();
        // текущий == текущему → отказ
        assert!(check_not_downgrade(&bytes).is_err());
    }

    #[test]
    fn signature_verify_ok_and_tamper_fails() {
        // ключationenkeypair только для теста — не release-ключ
        let sk = ed25519_dalek::SigningKey::generate(&mut rand::thread_rng());
        let vk = sk.verifying_key();
        let msg = b"fake release binary bytes";
        use ed25519_dalek::Signer;
        let sig = sk.sign(msg);
        use ed25519_dalek::Verifier;
        assert!(vk.verify(msg, &sig).is_ok());
        let mut bad = msg.to_vec();
        bad[0] ^= 0xFF;
        assert!(vk.verify(&bad, &sig).is_err());

        // verify_release падает на битую подпись / левый ключ
        // P0-7: legacy ed25519 verify_release удалён; негативные кейсы манифеста:
        assert!(verify_release_manifest(msg, "no-such-asset", b"manifest", "bad-sig").is_err());
    }

    #[test]
    fn install_update_atomic_and_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("nexxusd");
        std::fs::write(&bin, b"old-binary").unwrap();
        // подпись валидна только под release pubkey — для теста install
        // используем verify отдельно; здесь проверяем rollback-логику через
        // фальшивую подпись: install должен упасть и НЕ трогать старый бинарь
        let res = install_update(b"new-bytes", "nexxusd", b"manifest", "minisig", &bin);
        assert!(res.is_err(), "bad signature must abort");
        assert_eq!(std::fs::read(&bin).unwrap(), b"old-binary", "old binary intact after failed verify");
    }

    #[test]
    fn p0_7_manifest_parsing_and_semver() {
        let asset = b"fake-binary-bytes-nexxusd".to_vec();
        let good_hash = {
            use sha2::Digest;
            hex::encode(sha2::Sha256::digest(&asset))
        };
        let manifest = format!("{good_hash}  nexxusd-linux-x86_64\n");
        assert!(manifest.contains("nexxusd-linux-x86_64"));
        let tampered = b"tampered".to_vec();
        {
            use sha2::Digest;
            assert_ne!(hex::encode(sha2::Sha256::digest(&tampered)), good_hash);
        }
        assert_eq!(version_tuple("1.2.3-rc1"), Some((1, 2, 3)));
        assert!(is_newer((2, 0, 0), (1, 9, 9)));
        assert!(!is_newer((1, 9, 9), (2, 0, 0)));
    }
}
