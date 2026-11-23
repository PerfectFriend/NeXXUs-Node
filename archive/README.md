# archive/ — выведенные из активной разработки реализации (P1-14, аудит 2026-11-23)

Единственный демон — Rust `nexxusd` (crates/nexxus-node), единственный UI — Tauri-монитор (nexxus-desktop).

- `linux-node/` — Node.js-реализация демона со своим handshake (порт 3999, без auth). В архиве, не собирается, не деплоится.

Симулятор (TS) и React-UI из корня (src/, package.json, vite.config.ts) — остатки AI Studio; см. план миграции UI на Tauri в docs/plans/.
