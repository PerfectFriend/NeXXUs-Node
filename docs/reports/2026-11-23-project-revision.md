# Ревизия проекта NeXXuS Node — 2026-11-23 (HEAD `7f60904`, origin = github-main, оба remote синхронны)

## 1. Состав репозитория

| Компонент | Что это | Состояние |
|---|---|---|
| `crates/nexxus-core` | wire-протокол: opcodes (ADR-0002, 13 opcodes 0x01–0xFF), frame (4-byte BE len, MAX 4 MiB), v3 signing, Noise v4 (`wire-v4`), RS(4+2), Merkle (+v4 domain-separated), AEAD chunk seal, PoR (+v4), SMP envelope (+seal_v4), KDF/vault keys | реализовано, тесты зелёные |
| `crates/nexxus-node` (`nexxusd`) | демон: P2P (v3 порт 4001 + v4 mixed-mode), Local API WS 4010, redb storage, vault, backup, sync, Tor (arti), update (minisign), seed vault, panic/duress, IOU journal, bootstrap | реализовано, 141 тест + 5 |
| `crates/nexxus-econ` | экономика: IOU (Ed25519-подпись, canonical bytes, verify), vesting, degradation/health, sections (anti-loop invariant), money | реализовано (off-chain), тесты |
| `crates/nexxus-sdk` | Rust SDK-клиент к Local API (status/upload/download/smp/contact) + streaming + dapp | реализовано |
| `crates/nexxus-wasm` | wasm-обёртка | есть |
| `nexxus-desktop` (Tauri 2) | единственный UI-монитор | есть, CSP без unsafe-inline |
| `nexxus-mobile` (Android WebView) | мобильный клиент поверх assets/www | харденинг закрыт (`afa2b86`) |
| `contracts/` (TON Func) | AuditorCouncilBond, EpochController, TreasuryVesting | **прототипы, on-chain деплоя нет** |
| `archive/` | linux-node (Node.js демон), react-ui (AI Studio остатки) | выведены из сборки (P1-14) |

## 2. Реализованный функционал (100% — что реально работает и покрыто тестами)

Гейты на HEAD: `cargo test -p nexxus-node --features wire-v4` — **141 passed / 0 failed** (+5 в остальных suite), `cargo clippy --all-targets -- -D warnings` — 0 ошибок, `check-tree-hygiene.sh` чист, e2e-smoke на живом демоне **58/58**, CI green.

### P2P / wire
- v3 frame-протокол: 13 opcodes (Ping/Pong, NodeAnnounce, Store/Fetch chunk, PoR, Kademlia FindNode, EpochFinalize, SmpEnvelope, Error).
- Аутентификация: Ed25519, `node_id == SHA256(pubkey)` (H-3), replay-защита: combined session nonce `SHA256(min‖max)` + монотонный seq в подписи (P0-4, тест `p0_4_replayed_frame_seq_is_rejected`).
- **wire v4 (ADR-0011)**: Noise_XX_25519_ChaChaPoly_SHA256, prologue = H(tag‖min(id)‖max(id)‖epoch), mixed-mode на одном порту — magic `NXV4` + sniff в `run()` (`1be7a67`, тест `v4_dial_to_main_port_is_sniffed`), отдельный v4-порт `NEXXUS_P2P_V4_PORT` (default 4002, 0=выкл), `NEXXUS_P2P_V4_SNIFF=0` отключает. v4-функции PoR/Merkle/SMP seal (`*_v4`).
- Rate limiting: глобальный + per-connection token bucket, caps `NEXXUS_MAX_UP_KBPS`/`NEXXUS_MAX_DOWN_KBPS`, per-IP лимиты с GC и IPv6 /64, accept backoff, bounded smp_inbox(1024) + метрика `smp_dropped`, HANDSHAKE_TIMEOUT.
- Kademlia routing (`xor_distance` по полному [u8;32]), peer scoring с half-life decay, eviction по PoR-fail.

### Хранилище / vault
- redb ChunkStore, chunk 256 KiB (было 16 KiB), seal-then-RS (C-3): AEAD(XChaCha20-Poly1305-like по chunk) → RS(4+2) 6 шардов, per-file random key в манифесте (zero-knowledge).
- Потоковый upload (tokio::fs чанками) / download (fsync), hedged parallel fetch шардов у пиров (P1-10).
- Сегментированный манифест: >512 чанков → сегменты `manifest_{root}_seg_{i}` с SHA256, legacy inline совместим (`1932079`).
- PoR benchmark локальный + inter-host диагностика, rehydrate из mesh, pin/unpin/TTL/onion-only per chunk, GC, disk cap.

### Local API (WS 127.0.0.1:4010)
~50 методов: ping/auth, node_status/metrics/peers/ping_peer/audit_peer, upload/download/manifest, chunk pin/unpin/ttl/onion_only, connect/dial_onion, contact_link/contact_parse (QR `nexxus:<64hex>`), smp/smp_send, por, backup create/list/verify/restore/delete, seed_backup/restore/vault_status, identity export/import/status, bootstrap get/set, policy set/list/apply/delete, pause/resume, gc, doctor, test_interhost, sync scan/status, econ/econ_export, provider list/pin/status, epoch, iou issue/list/redeem/balance, export/import bundle, tor, name get/set/list/history, metrics, upload limits.
- Безопасность API: auth deadline 5с, idle 120с, Host только loopback (anti DNS-rebinding), /metrics лимит 8KiB, **две роли токена** admin + read-only (P1-9, ct_eq), WS handshake timeout.

### Крипто/keys/at-rest
- seed vault: AES-256-GCM, PBKDF2 600k, mode 0600, create_new без гонки, fsync файла+каталога; Argon2id(64MiB,t=3) PHC для panic.state PIN + миграция legacy; kdf_rounds клемп ≤10M, ct_eq (P1-6); C-9: ключ не лежит в vault-файле.
- Старт без PIN запрещён без `--insecure-plaintext-seed` (P0-2). Duress wipe чистит реестр sensitive_paths (vault, backups, iou, token, tor) (P1-7).
- identity: export/import, мнемоника только в памяти.

### Update / release
- Self-update на minisign SHA256SUMS-манифесте (тот же pubkey, что CI) + sha256 ассета + semver anti-rollback (P0-7). CI: actions запинены по SHA, fmt, cargo-deny (permissive-only), SBOM (cargo-cyclonedx → sbom.cdx.json в ассетах) + SLSA provenance (attest-build-provenance v2 @SHA) (`6447dd0`). Release: linux deb/rpm/AppImage, Windows exe+zip, Android APK, SHA256SUMS+minisig.

### Tor / privacy
- arti: onion service (hidden service), dial через onion, onion-only чанки не отдаются clearnet-пирам, `tor` статус в API.

### Экономика (off-chain)
- IOU journal с Ed25519, checked arithmetic, nonce после fsync (M-19); vesting; degradation/health по offline-часам; sections с anti-loop инвариантом; econ export подписан.

### Безопасность процесса
- gitleaks full-history в CI, история **переписана** 2026-11-23 (`git filter-repo`, 3 bot-токена вычищены, оба remote на `5bb8dcf`→`7f60904`, скан 0 совпадений), DEP-03 обновлён.

## 3. Что НЕ закрыто (честный план)

### broken (регрессии) — нет известных; все P0/P1 аудита 2026-11-22 закрыты или помечены.

### dead (написано, не подключено / заглушки)
- `pub mod pqc` в nexxus-core — simulation, не в прод-путях (C-10 помечено в доках).
- TON contracts — прототипы Func, нужен Tolk/Func миграция и деплой (C-6/7/8 by design).

### unbuilt (задумано, не сделано) — план разработки по приоритетам
1. **Ревок 3 TG bot-токенов** (bot-ID `8853927147`, `8883516682`, `8896772547`) через @BotFather + удалить `~/.hermes/secrets/cathedral_bot_token.bak-tokenrotate-20261117` — владелец, ждёт телегу.
2. **H-3/H-4/H-8 peer-management**: keep_alive rewrite + connection_rate_limit — вне RC1, следующий цикл.
3. **Deprecation v3** после mixed-mode (`1be7a67`): план отключения v3, окно миграции.
4. **Android**: токен в EncryptedSharedPreferences/Keystore вместо поля ввода; CI — `setup-android-sdk` action (сейчас не резолвится).
5. **Windows CI**: webkit2gtk на раннере для Tauri.
6. **P0-3 остаток**: полный vault v3-формат с пароль-фразой (сейчас Argon2id+decoy закрыто частично).
7. **TON**: миграция contracts на Tolk, on-chain деплой, EpochController wiring к `EpochFinalize`.
8. **sync**: убрать `std::fs` из async-путей, версионирование файлов (сейчас один индекс на файл), пустые файлы пропускаются.
9. **panic.state** рядом со стором — без TPM кулдаун сбрасывается удалением файла (задокументировано).

## 4. Риски
- До ревоков токены валидны (хоть и вычищены из истории) — кто скачал старый клон до 2026-11-23, имеет их.
- Старые клоны/зеркала вне контроля — responsabilidad держателей.
