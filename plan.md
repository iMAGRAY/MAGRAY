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
- [ ] Привести `cargo clippy -D warnings` к зелёному (минимум: убрать `unused import` в `apps/atom-ide/src/main.rs`)
- [ ] IPC сервер в `apps/atomd`: MVP реализован (Ping/Open/Save/Close/Search via ripgrep). Не хватает: обработка `IpcPayload::Cancel`, backpressure/лимиты на уровне очередей, конфиг таймаутов, единый лимит кадра (см. TODO.md §2).
- [ ] UI (Slint): в `apps/atom-ide` есть минимальный `slint::slint!{}`; интеграция с `atom-ui` и полноценные компоненты не подключены. Версия слота в workspace сейчас `slint = 1.5` (см. Cargo.toml) — требуется плановая миграция ≥1.13 по инвариантам.

Обновлено: 2025-09-09
