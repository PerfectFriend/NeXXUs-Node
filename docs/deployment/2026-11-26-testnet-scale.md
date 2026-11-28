# Testnet NeXXuS: деплой 20–30 серверных нод + 100–200 мобильных клиентов, нагрузка, метрики, тюнинг

Дата: 2026-11-26. Цель: получить воспроизводимые метрики для коммерческого предложения инвесторам
(«что мы получили» на эталонной топологии), до любых публичных шагов.

## 1. Топология

| Слой | Количество | Роль | Хост |
|---|---|---|---|
| Bootstrap/seed-ноды | 3 | первые пиры, bootstrap-реестр | выделенные VPS (EU/US-free zone) |
| Storage-ноды | 20–27 | хранят чанки, PoR, репликация | VPS 2 vCPU / 4 GB / 100 GB SSD |
| Клиентские устройства | 100–200 | Android APK (nexxus-mobile) | реальные устройства / эмуляторы |

Всего серверная часть: **23–30 хостов**. P2P порт `4001` (v4 sniff + v3 mixed), WS API `4010` (только localhost или за reverse-proxy с токеном).

## 2. Установка серверной ноды (одинаково на всех хостах)

```bash
# deb-пакет из CI release или ручная сборка
curl -LO <release-url>/nexxus-node_<ver>_amd64.deb && sudo dpkg -i nexxus-node_*.deb
# или из исходников:
source ~/build/env.sh && cargo build --release -p nexxus-node

sudo systemctl enable --now nexxus-node   # scripts/nexxus-node.service
```

Конфиг на ноде (`~/.nexxus/config.toml` или env):

```bash
NEXXUS_LISTEN=0.0.0.0:4001                 # P2P
NEXXUS_API_ADDR=127.0.0.1:4010              # API только локально
NEXXUS_MAX_STORE_BYTES=$((80*1024*1024*1024))  # лимит стора, 80 GB
NEXXUS_BOOTSTRAP_REGISTRY_PUB=<pubkey seed-ноды>
NEXXUS_TOR=1                                # onion для клиентов за NAT (опционально)
NEXXUS_POR_AUDIT_SECS=300                   # частота PoR
NEXXUS_EPOCH_SECS=3600                      # эпоха консенсуса
NEXXUS_MAX_UP_KBPS / NEXXUS_MAX_DOWN_KBPS   # бенчмарк-лимиты, см. §5
```

Запуск демона: `nexxusd serve <store-path>`; порты — **не** CLI-флагами, а env/конфигом (`NEXXUS_LISTEN`, `NEXXUS_API_ADDR`), иначе биндится дефолт 4001/4010:
```bash
NEXXUS_SEED_PIN=****** NEXXUS_LISTEN=0.0.0.0:4001 NEXXUS_API_ADDR=127.0.0.1:4010 nexxusd serve ~/.nexxus/store.redb
```
Подключение к сети: `nexxusd peer connect <seed-addr:4001>` или bootstrap-реестр. Проверка: `nexxusd status`, `node_peers` через WS API.

## 3. Мобильные клиенты

APK из `nexxus-mobile` (`./gradlew :app:assembleDebug`, имя по правилу `<Проект>-<abi>-debug.apk`).
Клиент подключается к любой storage-ноде по адресу/QR contact link (`node_contact_link`), токен — в EncryptedSharedPreferences.
100–200 устройств: реальные + эмуляторы Android Studio (для массовости), рассаживаем по регионам.

## 4. Нагрузочные тесты

Инструмент: `scripts/load_test.py` (чистый stdlib, WS API, N потоков-клиентов):

```bash
# на каждой storage-ноде — локально:
python3 scripts/load_test.py --api 127.0.0.1:4010 --clients 20 --size-mib 1 --rounds 3 --out /tmp/load.json
# с отдельного нагрузочного хоста против дальней ноды (проверка interhost):
python3 scripts/load_test.py --api 10.0.0.7:4010 --clients 50 --size-mib 4 --rounds 2
# вся сеть сразу: fan-out скриптом по списку хостов (ansible/parallel ssh)
```

Сценарии:
1. **L1 — одиночная нода**: 20 клиентов × 1 MiB × 3 раунда (smoke-нагрузка).
2. **L2 — interhost**: 50 клиентов на удалённую ноду, 4 MiB — p2p-чтение/запись между хостами.
3. **L3 — сеть целиком**: 200 «клиентов» (скрипт) грузят одновременно на все 23–30 нод — смотрим deg Serena: chunk placement, репликации, очереди.
4. **L4 — отказоустойчивость**: убить 3 ноды из 30 (kill -9), дождаться rebalance/репликации, повторить L2 — данные и доступность не теряются.
5. **L5 — sync**: `node_sync_scan` на клиенте с 1000 файлов, замер времени и трафика.
6. **PoR/audit**: `node_por`, `node_audit_peer` между нодами, `node_epoch` — кворум подписей.

## 5. Метрики (что собираем и куда это идёт в инвест-док)

| Метрика | Откуда | Зачем |
|---|---|---|
| upload/download p50/p95/max, s | `load_test.py` report | latency профиля |
| throughput MiB/s (1 нода / сеть) | `load_test.py` + `node_metrics` | пропускная способность |
| chunks count, store bytes | `node_metrics`, `node_chunks` | ёмкость |
| peers count, conn in/out | `node_peers` | топология сети |
| PoR ok/fail, audit latency | `node_por`, `node_audit_peer` | доказуемость хранения |
| epoch quorum, signatures | `node_epoch` | консенсус |
| RAM/CPU ноды под нагрузкой | `node_status` + `pidstat` на хосте | стоимость владения |
| rebalance time после kill-ноды | L4-лог | отказоустойчивость |

Тюнинг-ручки под нагрузкой: `NEXXUS_MAX_UP_KBPS/NEXXUS_MAX_DOWN_KBPS` (per-connection caps), `NEXXUS_CONN_MAX_DOWN_KBPS`, `NEXXUS_SYNC_INTERVAL_SECS`, `NEXXUS_POR_AUDIT_SECS`, размер чанка (256 KiB по умолчанию), `NEXXUS_MAX_STORE_BYTES`.

## 6. Критерии приёмки (что попадёт в инвест-док как «доказано»)

- L1: 20/20 клиентов, sha256 совпали, 0 fail; пороги p95 фиксируются по первому прогону и регрессируются вторым (цель: деградация не >10%).
- L2: 50/50, interhost p95 зафиксирован как эталон cross-host latency.
- L3: 200/200 без потерь данных (sha256), throughput сети = sum(per-host) в пределах дисперсии репликаций; fanout-отчёт `bench-network.json` — единый артефакт.
- L4: потеря 3/30 нод — 0 потерянных чанков, rebalance < T мин (T замеряется на первом L4, регрессия на втором).
- Все метрики — единый JSON (`--out`), per-host метрики — `node_metrics`, графики строятся из этих JSON.

## 6a. Стоимость инфраструктуры тестнета (для инвест-дока, оценка)
- 3 seed + 20–27 storage VPS (2 vCPU/4 GB/100 GB SSD): ~$8–15/мес за хост → **$200–450/мес** на 25 хостов (Hetzner/OVH/DO-tier).
- Нагрузочный хост + мониторинг (node_exporter + Grafana): ~$20–40/мес.
- 100–200 мобильных клиентов: эмуляторы на 2–4 хостах (~$40/мес) + реальные устройства по возможности.
- Итого тестнет: **~$300–500/мес** — это «cost of ownership» для коммерческого предложения.

## 7. Базовые замеры (2026-11-26, одна нода, debug/release 0.228.0, localhost)

`scripts/load_test.py` против живого демона (127.0.0.1:4010):

| Прогон | Клиентов | Размер | Результат | upload p50/p95, с | download p50/p95, с | throughput |
|---|---|---|---|---|---|---|
| 20×2 | 20 | 1 MiB | **20/20 ok**, sha совпали | 17.0 / 25.2 | 4.6 / 13.8 | 0.72 MiB/s |
| 4×1 | 4 | 4 MiB | **4/4 ok** | 14.6 / 14.6 | 0.08 / 9.5 | 1.09 MiB/s |
| 8×2 (fanout) | 8 | 1 MiB | **8/8 ok** | 2.25 / 2.44 | 1.47 / 1.56 | 2.13 MiB/s |
| 20×3 (fanout, L1) | 20 | 2 MiB | **20/20 ok** | 21.24 / 33.94 | 5.57 / 8.79 | 1.45 MiB/s |

Прогон fanout-скриптом (`testnet_fanout.py --local`, демон на 127.0.0.1:14020, debug 0.228.0, `f69eb3e`). Workspace-тесты на том же коммите: **279 passed / 0 failed**.

Выводы для тюнинга: upload ~0.3–0.7 MiB/s на ноду — узкое место (чанкинг+шифрование+запись), download из локального стора быстрый. Это и будем тюнить перед инвест-доком.

## 8. Что дальше

После прогона: тюнинг по метрикам → повторный прогон → `docs/reports/2026-11-26-testnet-bench.md` с цифрами → коммерческое предложение инвесторам (private placement, без ICO; юридика — отдельным треком).
