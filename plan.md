# План (актуальное состояние)

<!-- Обновление статуса: синхронизировано с фактическим состоянием на 2025-09-10 -->

- [x] Анализ структуры репозитория (Cargo.toml, crates/, apps/, CI)
- [x] Оценка текущего состояния (MSRV=1.82, политики, скрипты)
- [x] Добавлен AGENTS.md с гайдом для контрибьюторов
- [x] Обновлён TODO.md: отмечены реально выполненные пункты
- [x] Добавлены CI джобы: `cargo deny`, `audit`, `vet`, SBOM
- [x] Release‑профиль настроен (thin LTO, codegen-units=1, panic=abort)
- [x] Сборка dev‑профиля проходит: на Linux `cargo check -p atom-ipc -p atomd -p atom-ide` и `cargo build -p atom-ipc -p atomd -p atom-ide` — ОК (см. Cargo.lock). Проблема с OS‑lock `atomd.exe` воспроизводится только на Windows.
- [x] Индексация: `tantivy` обновлён до 0.22, `default-features = false`, включены `lz4-compression` и `mmap` (zstd отключён). Предыдущая проблема с `zstd-safe` на Windows нерелевантна.
- [x] Привести `cargo clippy -D warnings` к зелёному (устранены предупреждения в `apps/atom-ide`, `crates/atom-core`, `apps/atomd`, `crates/atom-ipc/tests`).
- [x] IPC сервер в `apps/atomd`: реализована обработка `IpcPayload::Cancel`, backpressure (лимит in‑flight), deadline‑reject; лимит кадра унифицирован до 1 MiB.
- [x] UX: авто‑старт демона из IDE при недоступности сокета (настройка `auto_start`), ожидаем готовность в пределах `connection_timeout`; дружелюбные логи.
- [x] UI (Slint): добавлено окно с строкой поиска, кнопками Search/Cancel, статусом и списком результатов; интеграция с `atom-ui` событиями (SearchStarted/Results/Cancelled/Error).
- [x] IPC лимит кадра и чтение/запись: вынесены cfg‑варианты (`read_ipc_message_cfg`/`write_ipc_message_cfg`) и использованы демоном с лимитом из Settings.
- [x] IPC: лимит кадра (1 MiB), таймауты, и лимиты in‑flight вынесены в Settings и применяются в `atomd`/`atom-ide` (см. `atom_settings::DaemonSettings`).
- [x] Метрики: счётчики `cancels/deadlines/backpressure` в `atomd` + `CoreRequest::GetStats` / `CoreResponse::Stats`.
 - [x] RFC‑0001 (Draft): «Хоткеи в IDE (Esc/F5) и миграция Slint при соблюдении MSRV» — добавлен `docs/rfcs/0001-slint-hotkeys-msrv.md`.
- [x] UI (Slint): в `apps/atom-ide` есть окно на `std-widgets.slint` (поле пути/поиска, Search/Cancel/Open Selected, список, статус), событийная интеграция через `atom-ui::{UiCommand, UiEvent}` подключена. Версия `slint = 1.5` (Cargo.toml). [TODO] Миграция ≥1.13 и расширение виджетов.
  - [ ] Хоткеи (Esc→Cancel, F5→Open Folder): в Slint 1.5 отсутствует стабильный Shortcut/глобальный key‑handling в std‑widgets. Без костылей это требует миграции Slint ≥1.13, что тянет `edition2024` в `i-slint-core-macros` и конфликтует с MSRV=1.82/политикой «без edition2024». Решение: вынести апгрейд Slint в отдельный RFC (перевод CI/toolchain на ed2024 или поиск backport‑варианта). До миграции — хоткеи не внедряем.
  - [x] Дерево проекта: сворачивание/разворачивание по клику, иконки каталогов (▸/▾) и файлов (📄), улучшенные отступы; юнит‑тесты преобразования структуры.
  - [x] Статус‑строка метрик (отдельная строка `metrics_text`) — показывает `cancels/deadlines/backpressure`.

Тесты (факт на 2025‑09‑10):
- [x] `apps/atomd`: E2E `e2e_ping`, `e2e_openbuffer` — зелёные; `e2e_cancel_long_op` — красный (в тесте ожидается закрытие канала, тогда как клиент шлёт `Err(Cancelled)` в ответ; требуется поправить ожидание); `e2e_project_files` не запускался в среде без `rg`.
- [x] `crates/atom-ipc`: unit и интеграционные (`roundtrip`, `cancel`) — зелёные (добавлена обработка handshake‑Ping и корректная проверка отмены).
- [x] `apps/atom-ide`: E2E headless (`e2e_ide_headless_starts_and_exits`) — зелёный; GUI smoke — сборка под Windows + feature `ui` (в тек. среде не запускался). В статус‑строке отображаются метрики демона.
  - [x] Headless E2E IDE крайних сценариев: `e2e_headless_deadline`, `e2e_headless_backpressure` — зелёные.

Обновлено: 2025-09-10
- [x] `apps/atomd`: добавлены E2E `e2e_deadline_reject` и `e2e_backpressure_reject` (использует env‑override `ATOMD_IPC_MAX_INFLIGHT=1`). `e2e_project_files` пропускается при отсутствии `rg`.
- [x] CI Windows: добавлен job `tests-windows-ui` (сборка `atom-ide` с `--features ui` и прогон `e2e_ui`, очистка процессов).
