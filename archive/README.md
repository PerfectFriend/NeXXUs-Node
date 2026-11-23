# archive/ — выведенные из активной разработки реализации (P1-14, аудит 2026-11-23)

Единственный демон — Rust `nexxusd` (crates/nexxus-node), единственный UI — Tauri-монитор (nexxus-desktop).

- `linux-node/` — Node.js-реализация демона со своим handshake (порт 3999, без auth). В архиве, не собирается, не деплоится.

- `react-ui/` — корневой React-UI (остатки AI Studio: `react-example` 0.0.0, `@google/genai`, `dev --host=0.0.0.0` — доступен из LAN). В архиве; CI больше не гоняет корневой npm (P1-14, `ci-release.yml`). Единственный UI — `nexxus-desktop/` (Tauri + свой vite).
