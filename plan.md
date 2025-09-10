# План (актуальное состояние)

<!-- Обновление статуса: синхронизировано с фактическим состоянием на 2025-09-09 -->

- [x] Анализ структуры репозитория (Cargo.toml, crates/, apps/, CI)
- [x] Оценка текущего состояния (MSRV=1.82, политики, скрипты)
- [x] Добавлен AGENTS.md с гайдом для контрибьюторов
- [x] Обновлён TODO.md: отмечены реально выполненные пункты
- [x] Добавлены CI джобы: `cargo deny`, `audit`, `vet`, SBOM
- [x] Release‑профиль настроен (thin LTO, codegen-units=1, panic=abort)
- [x] Сборка dev‑профиля проходит: `cargo check --workspace` OK; `cargo build --workspace` компилирует все таргеты, но завершился ошибкой удаления `target/debug/atomd.exe` (OS‑lock, os error 5). Код компилируется, требуется устранить блокировку файла.
- [x] Индексация: `tantivy` обновлён до 0.22, `default-features = false`, включены `lz4-compression` и `mmap` (zstd отключён). Предыдущая проблема с `zstd-safe` на Windows нерелевантна.
- [x] Привести `cargo clippy -D warnings` к зелёному (устранены предупреждения в `apps/atom-ide`, `crates/atom-core`, `apps/atomd`, `crates/atom-ipc/tests`).
- [x] IPC сервер в `apps/atomd`: реализована обработка `IpcPayload::Cancel`, backpressure (лимит in‑flight), deadline‑reject; лимит кадра унифицирован до 1 MiB.
- [x] UX: авто‑старт демона из IDE при недоступности сокета (настройка `auto_start`), ожидаем готовность в пределах `connection_timeout`; дружелюбные логи.
- [x] UI (Slint): добавлено окно с строкой поиска, кнопками Search/Cancel, статусом и списком результатов; интеграция с `atom-ui` событиями (SearchStarted/Results/Cancelled/Error).
- [x] IPC лимит кадра и чтение/запись: вынесены cfg‑варианты (`read_ipc_message_cfg`/`write_ipc_message_cfg`) и использованы демоном с лимитом из Settings.
- [ ] IPC: вынести лимит кадра и таймауты в конфиг Settings (сейчас константы), добавить метрики отмен/таймаутов.
- [ ] UI (Slint): в `apps/atom-ide` есть минимальный `slint::slint!{}`; интеграция с `atom-ui` и полноценные компоненты не подключены. Версия слота в workspace сейчас `slint = 1.5` (см. Cargo.toml) — требуется плановая миграция ≥1.13 по инвариантам.

Обновлено: 2025-09-09
