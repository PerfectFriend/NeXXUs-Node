# Пакет для следующего внешнего аудита — 2026-11-22 (HEAD `2711232`)

## Что закрыто с момента аудита v0.229.0-rc3

См. `docs/audits/2026-11-21-verification-of-rc3-audit.md` (постатейная верификация) и以下:

| Находка | Статус | Где |
|---|---|---|
| C-9 (ключ в vault-файле) | закрыт + regression test | `crates/nexxus-core/src/keys/vault.rs` (`vault_file_does_not_contain_encryption_key`) |
| C-10 (ложные крипто-claim'ы в доках) | закрыт | README/USER_GUIDE помечают как simulation/Phase 3; investor-док без «FIPS 203 лидерство» (`d929884`) |
| H-3 (node_id из payload) | закрыт | `network.rs` — проверка `node_id == SHA256(pubkey)` |
| H-4 (handshake без Noise) | **реализован за флагом `wire-v4`** | `crates/nexxus-core/src/wire/noise.rs` — Noise_XX_25519_ChaChaPoly_SHA256, prologue = H(tag‖min(id)‖max(id)‖epoch), тесты: roundtrip, wrong prologue, tampered msg3 |
| H-5 (PoR prover сам выбирает) | **v4-функции** | `merkle.rs::PorChallengeV4/por_response_v4/por_verify_v4` — challenge от верификатора (verifier_id+nonce+epoch), response биндится к chunk |
| H-6 (Merkle без domain separation) | **v4-функции** | `merkle_root_v4/merkle_proof_v4/verify_proof_v4` с префиксом `nexxus-merkle-v4` |
| H-7 (SMP sender спуфится) | **v4-функции** | `payloads.rs::SmpEnvelope::seal_v4/open_v4` — sender внутри AEAD-plaintext, тест на tamper cleartext-поля |
| H-12 (подпись релизов) | закрыт | minisign: `SHA256SUMS`+`.minisig` в релизе, pub key в `keys/`, verify — `scripts/verify-release.sh` |
| H-21 (bootstrap pin) | by design | pin обязателен когда задан; задокументировано |
| H-23 (TON «аудит закрыт») | док уже честный | `contracts/README.md` — «ПРОТОТИП / РЕФАКТОРИНГ» |
| H-25 (install.sh без пина) | закрыт | `--version=<tag>` + minisign/SHA256SUMS верификация prebuilt |
| M-19 (IOU overflow) | закрыт | `checked_add`, `MAX_IOU_MICRO`, nonce после fsync (`c6fce8e`) |
| M-20 (xor_distance u128) | закрыт | полный `[u8;32]` в routing.rs и smp |

ADR-0011 (`docs/adr/0011-wire-v4.md`) — дизайн wire v4, вопросы владельцу.

## Гейты на момент пакета

- `cargo test --workspace`: 15 suite ok, 0 failed (включая `--features wire-v4`: 84 passed)
- `cargo clippy --workspace --all-targets -- -D warnings`: 0 ошибок (с флагом и без)
- e2e smoke на живом демоне (caps 512 kbps): **58/58**
- CI: Rust CI + Release Builder green

## Известные ограничения (честно, для аудитора)

1. wire v4 — интегрирован за флагом `wire-v4` (`2711232`): v4-listener на `NEXXUS_P2P_V4_PORT` с Noise_XX handshake, `NoiseStream` (AsyncRead/Write поверх Noise transport) — поверх него работает обычный frame loop; `dial_v4_ping` + e2e `v4_noise_ping_over_v4_port` ок. v3 остаётся default; mixed-mode/auto-upgrade — следующий этап.
2. Panic cooldown хранится в `panic.state` рядом со стором — удаление файла сбрасывает кулдаун (без TPM не лечится).
3. sync: `std::fs` в async-путях, пустые файлы пропускаются, версий не хранится (один индекс на файл).
4. TON-контракты — прототипы, on-chain деплоя нет.
5. `pub mod pqc` — simulation, не используется в прод-путях.


---

# Обновление 2026-11-23 — внешний аудит (Armageddon666), статусы закрытия

Аудит: `Downloads/Аудит NeXXuS Node и ТЗ для кодера.md` (8 P0 + 14 P1). Статусы на HEAD после фиксов:

| Находка | Статус | Коммит / примечание |
| --- | --- | --- |
| P0-6 бинарники в `public/packages/` | **закрыт** | `9c3ce30`: файлы удалены, `check-tree-hygiene.sh` запрещает архивы/бинарники в дереве. История: `git filter-repo` — задача владельца |
| P0-8 `chmod -R 755` в install.sh | **закрыт** | `9c3ce30`: `chmod 750` только на каталог + пост-проверка `find -perm /077` |
| P0-5 README (PoW, «аудировано», версии) | **закрыт** | `9c3ce30`: PoW убран, «аудировано» убрано, версия/число тестов реальные |
| P1-1 `/metrics` (лимит, регистр токена) | **закрыт** | `9c3ce30`: лимит 8 KiB + таймаут 5 с, имя заголовка в lowercase, токен нормализуется при загрузке; тест `api_token_uppercase_hex_is_normalized` |
| P1-13 telegram secrets из `~/.hermes` | **частично закрыт** | `9c3ce30`: fallback удалён из `telegram.rs`. Отзыв токенов в @BotFather + `filter-repo` — задача владельца |
| P0-1 panic.state SHA-256 PIN | **закрыт** | `c7dd9d5`: Argon2id(m=64 MiB,t=3) PHC, legacy-файлы мигрируют при первом входе; тест `panic_state_uses_argon2id_and_migrates_legacy` |
| P0-2 plaintext seed без PIN | **закрыт** | `c7dd9d5`: старт без PIN запрещён без `--insecure-plaintext-seed`; тест `p0_2_refuses_plaintext_without_flag` |
| P1-8 vault tmp гонка прав/fsync | **закрыт** | `c7dd9d5`: `create_new`+mode 0600, fsync файла и каталога; тест `vault_file_is_0600_and_no_tmp_left` |
| P1-7 duress wipe не чистит секреты | **закрыт** | `2e193e9`: реестр `sensitive_paths` + `wipe_all_sensitive` (vault, backups, iou, token, tor); тест `p1_7_wipe_all_sensitive_leaves_no_secret_files` |
| P1-2 WS: auth-timeout, idle, Host | **закрыт** | `2e193e9`: auth deadline 5 с, idle 120 с, Host только loopback (anti DNS-rebinding) |
| P1-6 kdf_rounds/ct_eq в vault | **закрыт** | `10efe91`: клемп сверху 10M (DoS), `ct_eq` сравнение; тест `p1_6_kdf_rounds_clamped_and_ct_eq` |
| P0-4 replay записанной сессии v3 | **закрыт** | `bc646aa`: combined session nonce SHA256(min‖max) + монотонный seq в подписи (`v3_signing_bytes_seq`); тест `p0_4_replayed_frame_seq_is_rejected` |
| P1-3 accept/limits | **закрыт** | `a511b39`: backoff при ошибке accept (v3+v4), GC `ip_limits`, IPv6 группируется по /64 |
| P1-4 smp_inbox unbounded | **закрыт** | `a511b39`: bounded channel(1024), дроп при переполнении + метрика `smp_dropped` |
| P0-3 vault v3 (Argon2id, ≥10 символов, decoy) | **частично** | C-9 закрыт ранее; полный v3-формат с пароль-фразой — следующий цикл |
| P0-7 update.rs vs CI minisign | **закрыт** | `bf88dee`: self-update на minisign SHA256SUMS-манифесте (тот же pubkey, что в CI) + sha256 ассета по манифесту + semver anti-rollback; тест `p0_7_manifest_parsing_and_semver` |
| P1-5 wire-v4 по умолчанию | **открыт** | mixed-mode/auto-upgrade — следующий цикл |
| P1-9 роли токена API | **закрыт** | `226df2b`: admin (`~/.nexxus/api.token`) + read-only (`~/.nexxus/api.token.read`, 0600, env NEXXUS_API_TOKEN_READ_FILE); read-роль: только мониторинг-методы + /metrics, ct_eq; мутации отклоняются; тест `p1_9_read_token_allows_read_rejects_mutation` |
| P1-10 потоковый upload/download | **частично закрыт** | `4e1ad81`: upload читает файл чанками через tokio::fs (RAM не растёт с размером файла), download пишет потоково с fsync, fetch шардов hedged-параллельно у всех пиров (futures_util::join_all). Остаток: чанк 256KiB–1MiB и сегментированный манифест — ломают формат манифеста, отдельный миграционный релиз |
| P1-11/12 CI (deny, SHA-пин, fmt) | **закрыт** | `9493327`: все actions запинены по commit-SHA (проверено через git ls-remote), `cargo fmt --check` + `cargo deny` в rust-ci, `deny.toml` (permissive-only, deny GPL/AGPL), `continue-on-error` снят с android/tor-integration, у audit-ignores владелец+срок (2026-12-31). SBOM/provenance — остаток: `cargo-cyclonedx` + `actions/attest-build-provenance` в release-джобе |
| P1-14 один UI/демон, archive | **открыт** | фаза 5 |

Гейты на `4e1ad81`: `cargo test -p nexxus-node --features wire-v4` 0 failed; `cargo clippy -p nexxus-node --all-targets -- -D warnings` 0 ошибок; `check-tree-hygiene.sh` чист.
