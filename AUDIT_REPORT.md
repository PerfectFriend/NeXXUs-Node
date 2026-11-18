# 📋 Отчёт независимого технического аудита NeXXUs

> **Аудитор**: Grok (Гриша)  
> **Исполнитель / кодер**: AI Studio Engine / Женя (GAIS)  
> **Куратор**: Главный архитектор  
> **Канонический репозиторий**: https://github.com/PerfectFriend/NeXXUs-Node  
> **HEAD**: `ca89956` (docs) / код RC: `e611ad5`  
> **Дата**: 18 ноября 2026  
> **Тип**: **Аудит №11 — повторный срез + gap к продуктовой модели**  
> **Кодекс**: [Манифест v1.1](./MANIFEST_CANONICAL_AI_DEVELOPER.md)

---

## 0. Что изменилось с Аудита №10

| Область | Изменение |
|---------|-----------|
| **Код crates / nexxusd** | **Нет** — последний code-fix RC: `e611ad5` |
| **Документация** | Добавлены: quality audit, tech docs, user guide, TZ, investor pack, risk report, pronóstico ES (`9c3d35b`…`ca89956`) |
| **Теги / release** | Тег `v2.0.0-beta.1` в git **не проверен как annotated release** в этом срезе — рекомендуется выставить явно |
| **Продуктовая модель (вне репо)** | Уточнена куратором: onion-нода, телефон 24/7+Wi‑Fi, $0 сейф, restore по ключу, fan/reels, SDK/магазин |

**Вердикт по коду:** beta RC **не откатился** и **не продвинулся**. Технический долг №10 закрыт; **новый долг — gap между RC и целевой архитектурой сервиса**.

Аудиты №1–№10 остаются **ЗАКРЫТЫМИ**.  
**№11 ОТКРЫТ** как roadmap-аудит (не регрессия).

---

## 1. Состояние LIVE (без регрессий)

Подтверждено повторным просмотром маркеров и API:

- put/get/repair, RS(4+2), manifest, mesh `--peers`, bootstrap, auth, enc-at-rest (opt-in), reputation gate, challenge, smoke, CHANGELOG, SIM banner.
- ~9k+ LOC crates, честные `[SIM]` на Tor/PQC/ZK/io_uring/XDP/Kad RPC.

---

## 2. Найденные проблемы и разрывы (приоритет)

### P0 — блокеры целевого продукта «домашний onion-сейф»

| ID | Проблема | Где | Рекомендация |
|----|----------|-----|--------------|
| **PROD-ONION-01** | Tor/Arti **только [SIM]** (`arti_tor.rs`: fake bootstrap 100%, `connect_onion_stream` возвращает строку-заглушку). Адрес v3 есть, **hidden service / circuit — нет**. | `nexxus-network` | Интегрировать реальный backend: минимум **system Tor SOCKS + onion service** *или* `arti-client`. DoD: с другой машины открывается поток к ноде по `.onion`, put/get или status через onion. Разделить в status: `onion_address` (LIVE derive) vs `onion_transport` (LIVE/SIM). |
| **PROD-ENC-01** | Encrypt-at-rest **opt-in**. Для модели «параноик / избранное» plaintext по умолчанию — риск. | `put` / file_ingest | Флаг `--encrypt` default on **или** предупреждение + `NEXXUS_ENCRYPT=1` default в profile `paranoid`. Ключ из identity/BIP39 path, не только ручной hex. |
| **PROD-RESTORE-01** | Нет UX/CLI «импорт identity → дотянуть шарды за N дней». | daemon / storage | `nexxusd restore --identity node_identity.json` / import keys → queue fetch missing shards from peers/manifest list; прогресс %. |

### P1 — устойчивость mesh и честность ops

| ID | Проблема | Рекомендация |
|----|----------|--------------|
| **PROD-APPLIANCE-01** | Нет профиля «нода-appliance»: только Wi‑Fi/eth, запрет «карманного» режима в docs/flags | `serve --appliance`: require storage path, optional reject mobile-radio heuristics in docs; health: power/network assumed |
| **NET-TLS-01** | Wire без transport crypto (MITM на path) | Noise или TLS на P2P *или* документировать «только через onion/VPN» как обязательный профиль |
| **NET-RELAY-01** | Нет relay для CGNAT (даже при Wi‑Fi) | Простой relay/intro design; для onion — intro points Tor |
| **SEC-TOFU-01** | Trust on first NodeAnnounce | `peers.allowlist` / pin verifying keys |
| **REL-TAG-01** | Нет явного git tag beta.1 на remote (проверить/создать) | `git tag -a v2.0.0-beta.1` + release notes = CHANGELOG |

### P2 — платформа (магазин / SDK / Linux-провайдер) — не блок beta node

| ID | Тема |
|----|------|
| **PLAT-SDK-01** | Каркас SDK: transport client, BIP39 onboarding stub |
| **PLAT-STORE-01** | Манифест формата app package (позже) |
| **PLAT-PROVIDER-01** | CLI/учётка «Linux node sells pin» (учёт услуги, не только CAS) |

### Известные [SIM] без изменений (ок, но не путать с LIVE onion)

PQC, ZK-SNARK PoR, io_uring SQPOLL, eBPF/XDP, Kademlia WAN RPC, **Tor transport**.

---

## 3. Письмо кодеру (Женя / GAIS)

```text
Женя, привет!

Повторный аудит №11 (18.11.2026).

КОД с RC e611ad5 не менялся — регрессий нет, Аудиты №1–№10 остаются закрытыми.
Добавились только docs (риски, TZ, инвестор, прогноз ES). Это хорошо для внешних,
но продуктовый gap не закрыт кодом.

Продуктовая цель (от куратора), под которую нужно целиться:

  телефон/коробка 24/7 + Wi-Fi
  + onion-доступ к ноде
  + тот же ключ → сейф приложений
  + гибель ноды → импорт ключей → медленный restore из сети
  + $0 за личный архив; деньги позже с fan/reels/store

Сейчас в репо: отличный TCP mesh storage RC, но onion_transport = [SIM]
(arti_tor.rs — заглушка circuit/stream). Домашний тест куратора с «живым onion»
НЕ отражён в каноническом дереве — либо вынести в main как LIVE, либо
не заявлять onion_transport production.

Приоритет спринта (Аудит №11 DoD):

P0
1) PROD-ONION-01 — реальный onion publish + connect (system tor или arti).
   Тест: dual-machine или CI с tor service; status честно LIVE vs SIM.
2) PROD-ENC-01 — профиль paranoid: encrypt by default / identity-derived key path.
3) PROD-RESTORE-01 — nexxusd restore/import-identity + прогресс дотягивания шардов.

P1
4) REL-TAG-01 — annotated tag v2.0.0-beta.1
5) SEC-TOFU-01 — allowlist пиров
6) NET-TLS-01 или жёсткий docs: «cleartext wire только за onion/VPN»

P2 (не блокирует)
7) Черновик SDK surface (transport + BIP39) — можно отдельным crate/docs

Не надо в этом спринте: полноценный app store, OnlyFans-слой, ML-KEM live.

Критерий приёмки №11:
- [ ] onion_transport не [SIM] OR status явно split address vs transport
- [ ] documented restore path works on fresh storage_dir with same identity
- [ ] encrypt default policy decided and implemented or explicitly deferred with CLI warning
- [ ] cargo test --workspace green
- [ ] DEVELOPMENT_LOG этап «Аудит №11»

Жду коммит с fix(PROD-ONION-01,...) и короткий отчёт.

— Гриша (Grok)
```

---

## 4. Definition of Done — Аудит №11

1. PROD-ONION-01: live path **или** честный split в status + issue «blocked on arti».  
2. PROD-ENC-01: policy implemented or deferred with loud warning on plaintext put.  
3. PROD-RESTORE-01: minimal import+rehydrate CLI.  
4. Тесты зелёные; DEVELOPMENT_LOG.  

После закрытия №11 — снова оценка готовности к **public beta** уже под onion-сейф narrative (не только TCP mesh RC).

---

## 5. Резолюция

| Аудит | Статус |
|-------|--------|
| №1–№10 | ✅ закрыты (код `e611ad5`) |
| **№11** | 🔴 **открыт** — gap onion/encrypt-default/restore |

— Grok (Гриша)  
18 ноября 2026
