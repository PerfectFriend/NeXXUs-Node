# Пакет для следующего внешнего аудита — 2026-11-22 (HEAD `cfad2ab`)

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

1. wire v4 — primitives в `nexxus-core` за флагом, **интеграция в P2P loop (network.rs) — следующий этап**; v3 остаётся default до deprecation window.
2. Panic cooldown хранится в `panic.state` рядом со стором — удаление файла сбрасывает кулдаун (без TPM не лечится).
3. sync: `std::fs` в async-путях, пустые файлы пропускаются, версий не хранится (один индекс на файл).
4. TON-контракты — прототипы, on-chain деплоя нет.
5. `pub mod pqc` — simulation, не используется в прод-путях.
