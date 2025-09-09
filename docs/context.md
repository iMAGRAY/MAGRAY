# ATOM AI IDE — **контекст для Sonnet 4 агента** (детерминированный)

> **Назначение**: дать ИИ‑агенту (Claude **Sonnet 4**) **полный, точный и однозначный** контекст для реализации нативного Atom IDE на Rust. Документ фиксирует **инварианты**, **правила принятия решений**, версии зависимостей, протоколы, граничные условия и критерии приемки. Если какая‑либо информация ниже противоречит чужим источникам/старым черновикам — руководствоваться **этим** документом и указанными ссылками на первоисточники.

## A. Миссия и инварианты

1) **Платформы**: Windows 10/11 (x64/arm64), macOS 13+ (x64/arm64), Linux (Ubuntu 22.04+/Debian 12+/Fedora 40+).  
2) **UI ядро**: **только нативный** GPU UI на **Slint ≥ 1.13**. **Запрещены** Electron/Tauri/Web для ядра. Допустимы webview‑панели **только** в изолированном ext‑host для совместимости c VS Code. citeturn0search8  
3) **Язык/компилятор**: Rust stable, **MSRV=1.82**.  
4) **Парсинг/семантика**: **tree‑sitter 0.25.x (ABI 15)**, инкрементальный, отменяемый. citeturn0search9  
5) **WASM‑плагины**: **Wasmtime 36.x (LTS)** с fuel metering и preopen FS; LTS = 24 мес. citeturn1search0turn1search6  
6) **Поиск/индексация**: `ripgrep` + **Tantivy** для постоянного индекса. citeturn3search0  
7) **LSP**: версия протокола **3.17**. citeturn0search3  
8) **Экосистема расширений**: **Open VSX** (serviceUrl/itemUrl), локальные `.vsix`. **Не** использовать VS Code Marketplace (ToU “In‑Scope Products and Services”). citeturn0search4turn0search5  
9) **AI**: **Claude Code SDK** (Hooks + Setup), **MCP 2025‑06‑18** (client/server). citeturn0search7turn0search15turn2search6turn2search10  
10) **KPI/гейты**: cold start < 300 ms; 100k файлов < 200 ms; 1M файлов индекс < 2 s (фон); RAM ≤ 200 MB; input‑latency ≤ 16 ms.

> **Superseded**: Tauri/React/Web UI из ранних файлов — **не использовать**. fileciteturn0file0 fileciteturn0file1

---

## B. Процессная архитектура (жёстко зафиксировано)

- **UI‑процесс (`atom-ide`)**: Slint UI, палитра, статусбар, док‑панели, чат/AI, keymaps.  
- **Core‑процесс (`atomd`)**: буферы/парсинг, индекс/поиск, LSP‑пул, плагины, безопасность, конфиг, телеметрия.  
- **Ext‑Hosts** (отдельные процессы):  
  **(1)** VS Code (Node LTS 20/22) — `.vsix`; **(2)** Atom Legacy — CoffeeScript→JS + shims; **(3)** Plugins host — Wasmtime/Native.

**IPC**: фреймированный протокол (bincode/serde), **обязательная отмена**, backpressure, timeouts SLA. Любой RPC обязан поддерживать **cancellation** и **idempotency** (по возможности). Краш изолирован доменом.

---

## C. Жёсткие правила для агента (Do/Don’t)

- **Do**: использовать только Open VSX (см. product.json endpoints) и локальные `.vsix`; **Don’t**: вызывать Microsoft Marketplace API. citeturn0search4turn0search5  
- **Do**: собирать UI на Slint; **Don’t**: предлагать Tauri/Web UI. citeturn0search8  
- **Do**: ABI15 для tree‑sitter 0.25.x; обновлять парсеры и запускать их конформанс‑тесты; **Don’t**: смешивать версии ABI. citeturn0search9  
- **Do**: Wasmtime 36.x (**LTS**) и ограниченные capabilities; **Don’t**: запускать неограниченный нативный код без брокеров/песочницы. citeturn1search6  
- **Do**: реализовать Hooks из доков Anthropic; **Don’t**: полагаться на LLM без детерминированных хуков для критичных действий. citeturn0search7turn0search15  
- **Do**: MCP 2025‑06‑18 (OAuth‑модель для серверов); **Don’t**: использовать устаревшие ревизии без неготиации. citeturn2search10

---

## D. Пины и совместимые версии (минимум вариативности)

- Rust **1.82**; Cargo.lock коммитится; `cargo vendor` для репродьюса.  
- Tokio **1.40+** (многопоточный runtime c тюнингом), `tracing`/OTLP.  
- Slint **1.13**; winit ≥ 0.29. citeturn0search8  
- tree‑sitter **0.25.x** (ABI15); источники языков обновлены под ABI15. citeturn0search9  
- Wasmtime **36.x (LTS)** + wasmtime‑wasi preview1/preview2 по необходимости. citeturn1search0turn1search6  
- Tantivy **0.25.x**; ripgrep (последние релизы). citeturn3search12  
- Node **20/22 LTS** для Ext Host.  
- Open VSX endpoints из wiki. citeturn0search4

Все зависимости **фиксировать** до минорных/патч‑релизов; поднятие версий — через RFC и контрольный прогон KPI/регрессий.

---

## E. Подсистемы (детали реализации)

### E1. UI/UX (Slint)
- **Команды/палитра**: строго типизированные `CommandId`, keymaps JSON, конфликт‑резолвер.  
- **Темы**: пакет тем «Atom Classic / One Dark / One Light»; переменные; предпросмотр.  
- **Миникарта** + **пиксель‑перфект скролл** (no jitter, 120/144 Hz).  
- **A11y**: навигация клавиатурой, роли, контрасты, IME, screen‑reader hints.

### E2. Текст/парсинг
- Rope‑буфер, undo‑журнал (LFS‑совместимые чекпойнты).  
- tree‑sitter: парсинг только видимых диапазонов; фоновые обновления; **cancel** на каждую задачу. citeturn0search9  
- Подсветка: queries (*.scm), кэш правил; folding via queries.  
- Кодировки/концы строк/авто‑trim trailing whitespace по настройке.

### E3. Поиск и индекс
- `ripgrep` для ad‑hoc, **Tantivy** для постоянных символов/def/ref; инкрементальные обновления. citeturn3search0  
- Ограничения CPU/IO индексатора, backoff под ввод пользователя.

### E4. LSP‑клиент
- Спецификация **3.17**: inlay hints/inline values/notebooks/type hierarchy. citeturn0search3  
- Транспорт stdio; supervisor (перезапуск/health); батчинги; debounce.  
- DIAGNOSTICS diff‑применяются к буферу; семантика — viewport‑batch; inlay — партиями.

### E5. VS Code bridge
- Node LTS OOP; реализуем `vscode.*` поверх RPC; покрытие ≥ 90% популярных расширений.  
- **Open VSX** для поиска/установки; локальные `.vsix`. **Marketplace MS** — запрещён. citeturn0search4turn0search5  
- Ограничения: `child_process`/network — по политике; FS — только брокером.

### E6. Atom legacy
- `atom.*` API shim; транспиляция CoffeeScript; GitHub Releases/зеркала. citeturn0search6turn0search14

### E7. Новые плагины (WASM/Native)
- **Manifest (TOML)**: id/name/version, `min_host`, declared `capabilities`, UI surfaces.  
- **Wasmtime 36.x**: fuel, лимиты RAM/время, preopen‑FS, host‑функции с проверками; LTS. citeturn1search6  
- **Подписи** `.apx`; верификация при установке и запуске.

### E8. AI (Claude Code + MCP)
- **Setup/Updates**: следуем официальным инструкциям по установке и автообновлениям. citeturn2search6  
- **Auth**: поддерживаем **OAuth‑вход** через SDK/CLI (где доступно) и **API‑ключ** через Anthropic SDK; провайдеры без ключей — через community‑провайдер, когда это применимо. citeturn2search0turn2search11turn2search3  
- **Hooks**: PreToolUse/PostToolUse/UserPromptSubmit/… — детерминированные командные сценарии. citeturn0search7turn0search15  
- **MCP 2025‑06‑18**: client+server; авторизация OAuth‑Resource‑Server. citeturn2search2

---

## F. Безопасность

- **Capabilities/PoLA**: файл манифеста плагина → вычислимые политики доступа (FS, NET, UI).  
- **Секреты**: ОС‑кейчейн; audit что **не** логировать; redaction в логах.  
- **Egress**: deny‑by‑default, allowlist доменов/портов, rate limiting.  
- **FS**: только через брокер; WASM preopen.  
- **Обновления**: подписи и проверка; быстрый rollback.

---

## G. KPI и тесты приёмки (CI gates)

1) **Startup**: холодный запуск < 300 ms (95‑й перцентиль).  
2) **Project open**: 100k файлов < 200 ms (интерактивность), 1M индекс — < 2 s фон.  
3) **Latency**: ввод ≤ 16 ms при 60 FPS.  
4) **Memory**: baseline ≤ 200 MB.  
5) **Compat**: топ‑набор расширений из Open VSX работает без регрессий. citeturn0search4

Метрики фиксируются `tracing`+OTLP (JSONL), воспроизводимое железо.

---

## H. Пошаговый план для агента (чёткие действия)

**H1. Подготовка репо (день 1–2)**  
- Создать Cargo workspace (см. структуру).  
- Настроить CI (lint/clippy/tests/perf), `cargo vendor`, SBOM.  
- Пинning версий (см. §D).

**H2. UI/ядро (нед. 1–3)**  
- Slint каркас: окна, layout, палитра, панели, statusbar. citeturn0search8  
- IPC слой (bincode/serde, cancelable RPC).  
- Rope‑буфер + базовый редактор; mmap для больших файлов.

**H3. Поиск/индекс (нед. 2–4)**  
- Интеграция ripgrep; **Tantivy** индекс + инкрементальные обновления. citeturn3search0

**H4. LSP‑пул (нед. 3–5)**  
- LSP 3.17 init/диагностика/семантика/ховеры/комплиты. citeturn0search3

**H5. Ext Host (нед. 4–7)**  
- Node LTS host + мост `vscode.*`; загрузка из **Open VSX**. citeturn0search4

**H6. Плагины (нед. 6–9)**  
- Wasmtime 36.x host + манифест/подпись/лимиты. citeturn1search6

**H7. AI (нед. 7–10)**  
- Claude Code SDK setup + Hooks; MCP client. citeturn2search6turn0search7turn0search15turn2search10

**H8. QA/Perf (нед. 9–12)**  
- Сuiten тестов совместимости; KPI‑гейты в CI.

---

## I. Частые ошибки и как их избежать (соннет‑чеклист)

- Marketplace MS вместо Open VSX → **ошибка** (ToU). Должно быть Open VSX. citeturn0search5  
- Веб‑UI в ядре → **запрещено** (только Slint). citeturn0search8  
- Неправильный ABI для tree‑sitter → парсеры не грузятся → **использовать ABI15**. citeturn0search9  
- Отсутствие cancel/timeouts в IPC → зависания.  
- Неограниченный `child_process`/network у расширений → проблемы безопасности.
- Игнорирование LTS у Wasmtime → уязвимости/нестабильность. **Использовать 36.x LTS**. citeturn1search6

---

## J. Приложения (образцы)

### J1. Open VSX endpoints (product.json подобие для хоста)
```json
{
  "extensionsGallery": {
    "serviceUrl": "https://open-vsx.org/vscode/gallery",
    "itemUrl": "https://open-vsx.org/vscode/item"
  }
}
```
citeturn0search4

### J2. Манифест плагина (.apx)
```toml
[plugin]
id = "dev.example.tool"
version = "1.0.0"
type = "wasm"
min_host = "1.0.0"

[capabilities]
fs.read  = ["${workspace}"]
net.out  = ["api.example.com:443"]
ui       = ["command","panel","status-item"]
```

### J3. Хуки Claude Code
- `PreToolUse`: валидация намерений перед записью на диск.  
- `PostToolUse`: нормализация/ форматирование результата.  
- `UserPromptSubmit`: фильтр конфиденциальных данных. citeturn0search7turn0search15

---

### Ссылки на первоисточники
- Slint 1.13 (03.09.2025) — релиз/новости. citeturn0search8  
- tree‑sitter 0.25 / ABI15 — релизы и сводка. citeturn0search9turn0search1  
- Wasmtime 36.x + LTS — релизы/политика. citeturn1search0turn1search6  
- LSP 3.17 — спецификация. citeturn0search3  
- Open VSX — endpoints/вики. citeturn0search4  
- Marketplace ToU (In‑Scope) — PDF. citeturn0search5  
- Atom sunset/пакеты. citeturn0search6turn0search14  
- Claude Code Hooks/Setup. citeturn0search7turn0search15turn2search6  
- MCP 2025‑06‑18 — обзор/чейнджлог. citeturn2search10turn2search2

> **Примечание**: ранние документы с Tauri/Web UI признаны устаревшими и не применимыми к этому проекту. fileciteturn0file0 fileciteturn0file1
