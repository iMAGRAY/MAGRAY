<!--
Обновлено: 2025-09-09
Назначение: детерминированный снапшот текущего состояния проекта для продолжения работ.
Этот файл дополняет docs/context.md и фиксирует фактическое состояние кода/зависимостей/CI.
-->

# Atom IDE — Контекст (детерминированный снапшот)

## Резюме
- Репозиторий — Rust workspace (apps + crates), MSRV=1.82.0.
- CI настроен (toolchain/clippy/fmt, anti‑mock, cargo‑deny/audit/vet, SBOM).
- Сборка всего воркспейса на Windows сейчас падает из‑за `tantivy -> zstd-safe` (ошибки E0432/E0433).
- UI ещё плейсхолдер (без реального Slint), IPC‑клиент есть, сервер в `atomd` пока «echo» по TCP без фрейминга `atom-ipc`.
- LSP‑менеджер есть (spawn stdio серверов), но не интегрирован в `atomd`/IPC.

## Среда (локально проверено)
- OS: Windows (путь вида `C:\Users\...`).
- `rustc -V`: `rustc 1.82.0 (f6e511eec 2024-10-15)`.
- `node -v`: `v24.4.1` (ок, но целевая LTS 20/22 по плану).
- `rg --version`: `ripgrep 13.0.0`.

## Структура воркспейса (фактически)
- apps: `atom-ide` (UI-процесс), `atomd` (daemon), `atom-ext-host-node` (Node bootstrap).
- crates: `atom-core`, `atom-ipc`, `atom-index`, `atom-lsp`, `atom-ui`, `atom-settings`, `atom-persistence`, `atom-plugin`, `atom-sandbox`, `atom-ai`, `atom-ext-host`, `atom-atom-compat`.

## Пиннутые версии (из корневого `Cargo.toml`)
- Tokio 1.40, Serde 1.0, Bincode 1.3.
- Slint = 1.5 (в планах ≥1.13, пока не обновлено).
- Ropey 1.6; cosmic-text 0.14.
- tree-sitter = 0.25; языковые парсеры: rust 0.24, js 0.25, ts 0.23, python 0.23, json 0.24.
- Tantivy = 0.21 (в планах 0.25.x; источник текущей ошибки сборки на Win из‑за zstd-safe/zstd-sys).
- Wasmtime = 25.0 (в планах 36.x LTS).
- rusqlite 0.32; tracing 0.1/0.3.
- Release‑профиль: `lto = "thin"`, `codegen-units = 1`, `panic = "abort"`, `strip = true`.

## Состояние ключевых модулей
- `crates/atom-ipc`: есть IPC‑клиент с фрейм‑хедером (magic/version/len/CRC32). `MAX_MESSAGE_SIZE = 64MB`, Cancel поддержан, Deadline пока нет. Серверной части в `atomd` нет (см. ниже).
- `apps/atomd`: инициализация настроек/буферов; условный индекс по фиче `index`. IPC‑сервер — простой TCP echo (без bincode/фрейминга и без маршрутизации `CoreRequest/CoreResponse`).
- `crates/atom-core`: менеджер буферов на Rope, базовый парсинг tree‑sitter, сохранение с валидацией пути.
- `crates/atom-index`: движок индексации (Tantivy 0.21) + ripgrep. Сборка ломается на Windows из‑за `zstd-safe`.
- `crates/atom-lsp`: пул LSP (spawn stdio, init, базовые cap), супервизор; пока не связан с `atomd`.
- `apps/atom-ide` + `crates/atom-ui`: UI — плейсхолдер без реальных Slint‑компонентов; фича `ui` включает зависимости, по умолчанию headless.
- `apps/atom-ext-host-node`: bootstrap Node, вывод логов, авто‑поиск `node`, но без реализации VS Code bridge.
- `crates/atom-settings`: загрузка/сохранение JSON/TOML, merge, validate; дефолты корректны.

## Политики/CI
- ANTI‑MOCK: есть job `no-mock-enforcement` (grep‑проверки в workflow), политика задокументирована в `docs/policies/no-mock.md`.
- Запрет WebView в ядре: `scripts/check_no_webview.sh`.
- Запрет VS Marketplace: `scripts/check_no_ms_marketplace.sh` (ориентир — Open VSX).
- CI: `toolchain.yml` — fmt, clippy `-D warnings`, metadata, deny/audit/vet, SBOM.

## Текущее фактическое состояние сборки
- `cargo metadata` — ок.
- `cargo build --workspace` — падает (Windows): ошибки в `zstd-safe` при сборке `tantivy 0.21`.
- `cargo test --workspace` — аналогично падает на том же месте.

Фрагмент ошибки:
```
error[E0432]: unresolved import `zstd_sys::ZSTD_cParameter::ZSTD_c_experimentalParam6`
error[E0433]: could not find `ZSTD_paramSwitch_e` in `zstd_sys`
```

## Расхождения с инвариантами (что ещё не выполнено)
- Slint < 1.13; Tantivy < 0.25.x; Wasmtime < 36.x LTS.
- IPC на стороне демона не соответствует `atom-ipc` (нет фрейминга/serde/bincode, нет deadline/cancellation‑маршрута сервером).
- UI ещё без нативных компонентов Slint.
- Open VSX интеграция/установка `.vsix` не реализованы.
- LSP‑пул не подключён к IPC/ядру.

## Опции для разблокировки сборки (детерминированно)
1) Временно исключить `crates/atom-index` из сборки воркспейса (нежелательно: нарушит policy "Build all").
2) Оставить членом воркспейса, но собирать таргетно (`cargo build -p ...`) — не решает CI.
3) Обновить `tantivy` до серии, совместимой с MSRV=1.82 и `zstd-sys` на Win (предпочтительно, требует теста).
4) Отключить zstd в tantivy и перейти на lz4‑компрессию, если фичи позволяют (нужно проверить флаги фичей tantivy 0.21/≥0.25).

Рекомендация: пункт (3) или (4), затем удерживать версию пиннутой и добавить проверку в CI.

## Мини‑дорожная карта ближайших шагов
1) Починить сборку воркспейса на Win (см. опции выше).
2) Реализовать серверную сторону IPC в `atomd` (bincode‑фрейминг, CoreRequest/CoreResponse, таймауты/отмена).
3) Протянуть базовые команды UI↔Core: OpenBuffer/Save/Search.
4) Включить реальный Slint‑окно за фичей `ui` (миграция с плейсхолдера).
5) Интегрировать LSP‑пул через IPC (hover/diag/complete на MVP).

— Конец снапшота —

