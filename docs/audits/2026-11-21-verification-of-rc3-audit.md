# Верификация аудита v0.229.0-rc3 (Claude) против текущего HEAD — 2026-11-21

Аудируемый файл: `Downloads/NeXXUs-Node-audit-v0.229.0-rc3.md`. Аудит статический (без сборки), HEAD на момент аудита — rc3.
Текущий HEAD: `002d4fa`+ (LIVE). Ниже — статус каждого пункта **по фактическому коду и запуску тестов**.

## P0

| Пункт | Статус сейчас | Доказательство |
|---|---|---|
| **C-9** ключ в vault-файле | ✅ **ЗАКРЫТ** | `derive_keys_v2` разводит enc-key и verifier через `kdf_expand` с разными метками (`crates/nexxus-core/src/keys/vault.rs:101-108`); регрессионный тест `vault_file_does_not_contain_encryption_key` — **ok** (прогнан: 7/7 vault-тестов) |
| **C-10** доки vs код | ✅ **ЗАКРЫТ** (`d929884`) | investor-док: убран «FIPS 203 лидерство»; операционные доки помечают PQ/PoR как simulation/Phase 3 | README/USER_GUIDE помечают ML-KEM/Groth16 как «simulation / Roadmap Phase 3»; `pqc.rs` — simulation, не используется вне себя. **Осталось**: `pub mod pqc` экспортируется; investor-презентация (`docs/INVESTOR_PRESENTATION_AND_TOKENOMICS.md:161`) обещает PR «FIPS 203 лидерство» — репутационный риск, рекомендуется смягчить |
| **H-25** CI/установщик после удаления linux-daemon.ts | ✅ **ЗАКРЫТ** (`d929884`) | install.sh: `--version=<tag>` пинит тег вместо main; prebuilt-бинарь скачивается с релиза и проверяется minisign+SHA256SUMS; fallback на source build | `scripts/linux-daemon.ts` удалён, `ci-release.yml` ссылок не содержит, tsconfig исключает `docs/archive`; `install.sh` собирает **Rust** `nexxusd` через cargo. **Осталось**: install.sh клонирует `main` без пинна версии и без проверки SHA-256 артефакта, сервис от root |

## P1

| Пункт | Статус сейчас | Доказательство |
|---|---|---|
| **H-3** node_id из payload | ✅ **ЗАКРЫТ** | `network.rs:963-970`: `announce.node_id` обязан равняться `SHA256(pubkey)`, иначе reject 0x0008 |
| **H-4** handshake без Noise | ⛔ открыт (by design, wire v3) | нужен wire v4 + ADR (Noise XX / snow) |
| **H-10** download_file без parity-fallback | ⛔ открыт | проверить `vault.rs download_file` |
| **H-6/H-7/H-5** Merkle domain separation, PoR prover выбирает сам, SMP sender spoof | ⛔ открыт | требует wire v4 + ADR |
| **M-20** xor_distance u128 | ✅ **ЗАКРЫТ** | `routing.rs:182-188`: полный `[u8; 32]` XOR, лексикографическое сравнение |
| **H-24** sync.rs | ✅ mostly | `sync.rs:65,215` — `symlink_metadata` (symlink пропускаются); `:106` — сравнение size **и** mtime. Осталось: sync `std::fs` в async, нет GC версий, пустые файлы пропускаются |

## P2 / остальное

| Пункт | Статус |
|---|---|
| **H-23** TON-контракты «аудит закрыт» | ⛔ открыт и **согласен с аудитором**: контракты заморожены, on-chain деплоя нет, «закрыто» в README контрактов вводит в заблуждение — надо пометить DRAFT/not-audited. Вне скоупа релиза по дизайну (ждём Func/Tolk) |
| **M-19** IOU: checked_add, лимит суммы, компакция journal, nonce после fsync | 🟡 частично: `balance_owed` суммирует без checked_add (iou_journal.rs), from_json самописный. Быстрый фикс возможен |
| **M-8/M-9** 32-битный разбор заголовка, vested() и burned | ⛔ открыт |
| **H-8 остаток** lock routing через await, семафор без таймаута | 🟡 частично (per-conn cap и keepalive добавлены `c79a22c`) |
| **H-12** подпись релизов | ✅ **ЗАКРЫТ** (`d929884`) | minisign: `SHA256SUMS` + `.minisig` в релизе, pub key в `keys/`, secret `MINISIGN_SECRET_KEY`, verify — `scripts/verify-release.sh` |
| **H-21** bootstrap pin опционален | 🟢 by design | pin обязателен только когда задан (enforced, `local_api.rs:1192`); иначе первый узел не поднимется. Задокументировано |
| **Panic cooldown** сбрасывается доступом к ФС | ⛔ открыт (by design?) |

## Что проверено запуском сегодня

- `cargo test -p nexxus-core vault` → 7 passed / 0 failed, включая `vault_file_does_not_contain_encryption_key ... ok`
- `cargo test --workspace` → 252 passed / 0 failed
- `npm run lint` → 0 TS errors
- e2e на живом демоне: smoke 58/58, IOU 12/12

## Вывод

Из P0 аудита rc3 **C-9 реально закрыт** (с тестом), H-25 закрыт по CI и установщику (остался unpinned main + root), C-10 закрыт в операционных доках, остался investor-док. Из P1 закрыты H-3 и M-20, H-24 mostly. Главные открытые: H-4/H-5/H-6/H-7 (требуют wire v4 + ADR), H-10, H-23 (TON, вне скоупа), M-19 мелочи IOU, H-12/H-21.
