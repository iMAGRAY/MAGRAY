# Atom IDE (Rust, Native Desktop) — **Implementation Plan v1.1**
> **Scope**: Реинкарнация GitHub **Atom** c современным **нативным** десктоп‑стеком (Windows/macOS/Linux), **без** Electron/Tauri/WebView для ядра UI и **без** мобильных платформ. Пишем на **Rust** с приоритетами **производительность → безопасность → корректность** без ущерба UX. Интерфейс и ощущение — как у Atom; совместимость c **Legacy Atom** пакетами и **VS Code** экосистемой (через **Open VSX**, не Marketplace Microsoft), плюс новая **собственная** плагинная система (WASM/Native) и глубинная интеграция **Claude Code SDK** и **MCP**.

> Этот документ — детерминированный план для инженерной реализации и для автономной работы ИИ‑агентов. Он **заменяет** ранние черновики, где рассматривались Tauri/Web UI, и фиксирует юридические, технологические и эксплуатационные ограничения. (Ранние материалы на тему веб‑UI/Tauri следует считать **устаревшими**.)

---

## 0) Инварианты и «рамки» проекта (must‑stay‑true)
1. **ОС/архитектуры**: Windows 10/11 (x64/arm64), macOS 13+ (x64/arm64), Linux (Ubuntu 22.04+/Debian 12+/Fedora 40+; x64/arm64).  
2. **Язык/компилятор**: Rust **stable**, **MSRV = 1.82** (повышается только через RFC).  
3. **UI**: **Slint** (Rust‑first) **≥ 1.13** — нативный GPU UI, без браузерного рантайма. Релиз 1.13: 03‑сен‑2025. \[Источник: Slint 1.13 релиз\] citeturn0search8  
4. **Текстовый движок**: rope‑структура (`ropey`), формирование глифов (`cosmic-text`), инкрементальный парсинг — **tree‑sitter 0.25.x** (ABI 15). Выпуск 0.25.0: 01‑фев‑2025. citeturn0search9turn0search1  
5. **WASM‑рантайм**: **Wasmtime 36.x** (**LTS**: 36.0.0 от 20‑авг‑2025; 36.0.1 от 21‑авг‑2025). Поддержка LTS — 24 мес. citeturn1search0turn1search6turn1search12  
6. **VS Code экосистема**: загрузка/поиск расширений — **Open VSX** (`extensionsGallery.serviceUrl/itemUrl`), self‑host/Mirror допустимы. citeturn0search4turn0search12  
7. **Юридические ограничения**: **запрещено** использовать **VS Code Marketplace** для не‑встроенных продуктов (разрешено только для “In‑Scope Products and Services”: Visual Studio/VS Code/Azure DevOps/GitHub Codespaces и т.п.). Используем **Open VSX**. citeturn0search5turn0search13  
8. **Atom legacy**: atom.io/registry закрыт; нужна совместимость + фоллбек загрузки из GitHub Releases/зеркал. citeturn0search6turn0search14  
9. **AI**: интеграция **Claude Code SDK** (Hooks, настройки, обновления), плюс **MCP 2025‑06‑18** (client/server). citeturn0search7turn0search15turn2search10turn2search2  
10. **Производительность (Acceptance Gates)**: cold start **< 300 ms**, открытие проекта 100k файлов **< 200 ms**, индексация 1M файлов **< 2 s** (в фоне), baseline RAM **≤ 200 MB**, latency ввода **≤ 16 ms** при 60 FPS.  

> **Примечание для агентов**: любые попытки подключить Electron/Tauri/браузерные WebView для **ядра UI** — нарушение инвариантов. WebView допустим **только** в изолированных панелях webview API для совместимости с VS Code расширениями.

---

## 1) Процессная архитектура и отказоустойчивость

```
+-------------------+
|   UI (Slint)      |  Командная палитра, вкладки, дерево, статусбар,
|   app: atom-ide   |  чат/AI-док, настройка UI, keymap, темы.
+---------+---------+
          |        Zero-copy framed IPC (bincode/serde, backpressure, cancel)
          v
+---------+---------+
|  Core Service     |  Буферы, индекс, поиск (ripgrep+Tantivy), LSP‑пул,
|  daemon: atomd    |  плагины, безопасность, конфиг, телеметрия.
+--+--------------+-+
   |              |
   |              +---------------------------+-------------------------+
   |                                          |                         |
   v                                          v                         v
+--+------------------+       +---------------+--------------+   +------+------------------+
| VS Code Ext Host    |       | Atom Legacy Bridge           |   |  Native/WASM Plugins  |
| Node LTS (OOP)      |       | CoffeeScript→JS, atom.* shim |   | Wasmtime 36.x (LTS)   |
| .vsix/.vsixgallery  |       | GitHub fetch fallback        |   | Capabilities+signing  |
+---------------------+       +------------------------------+   +------------------------+
```

- **Взаимоизоляция**: каждый хост — **отдельный процесс**. Крах расширения ≠ крах IDE.  
- **IPC**: строго фреймированный протокол, bounded очереди, **обязательная отмена** (cancellation), timeouts SLA на RPC.  
- **FSM перезапуска**: supervise+backoff для LSP и Ext Host; crash‑loop breaker.  

---

## 2) Выбор технологий (обоснованный и актуальный)

### 2.1 UI (нативный GPU)
- **Slint ≥ 1.13** с Live‑Preview/Inspector; рендер: OpenGL/WGPU backend. citeturn0search8  
- **winit** для окон/ввода; IME/HiDPI/Wayland/AppKit/Win32 DPI awareness.  
- **Темы/виджеты**: пакет «Atom UI» (TreeView/Tabs/Panels/Status/Minimap/Palette).  
- **Доступность**: роли/аксессибилити‑каналы ОС; контраст/скейлы шрифтов.  

### 2.2 Текстовый движок
- **Rope**: `ropey` (O(1) разбиения/склейки).  
- **Шейпинг**: `cosmic-text` + кэш глифов по (font,size).  
- **Парсинг**: **tree‑sitter 0.25.x (ABI15)**, инкрементально, за пределы viewport — только в idle. citeturn0search9  
- **Поиск**: on‑demand `ripgrep` для grep‑ов, и **Tantivy** для постоянного символьно‑кодового индекса. citeturn3search0turn3search12  

### 2.3 LSP
- Спецификация **LSP 3.17** (type hierarchy, inlay hints, inline values, notebooks). citeturn0search3turn0search11  
- Транспорт stdio + supervisor; батч‑уведомления; viewport‑batch для diagnostics/semantic tokens.  

### 2.4 Экосистемы расширений
- **VS Code**: OOP **Node LTS (20/22)** хост, совместимость API → мост к нашему ядру.  
- **Регистры**: по умолчанию **Open VSX** — `extensionsGallery.serviceUrl`/`itemUrl`, поддержка зеркал. citeturn0search4  
- **Юридически**: **не** обращаться к **VS Code Marketplace** (ToU ограничивает использование “in‑scope” продуктами Microsoft). citeturn0search5  

### 2.5 Новая плагинная система
- **WASM (предпочтительно)**: **Wasmtime 36.x (LTS)**, fuel metering, preopen FS, network broker, capability‑модель. citeturn1search0turn1search6  
- **Native Rust**: опционально — только с усиленной песочницей (seccomp/Seatbelt/AppContainer) и брокерами I/O.

### 2.6 AI
- **Claude Code SDK**: установка/обновления/настройки; Hooks справочник и гайд. citeturn0search7turn0search15turn2search6  
- **MCP 2025‑06‑18**: клиент + (опционально) сервер; OAuth‑ориентированная авторизация у серверов. citeturn2search10turn2search2  

---

## 3) Workspace и репозиторий (Cargo/Node)

```
/atom-ide/
  Cargo.toml                       # workspace
  /crates/
    atom-ui/                       # Slint UI + палитра, темы, миникарта
    atom-core/                     # буферы, индексы, поиск, LSP, конфиг
    atom-ipc/                      # типы/фрейминг/кансел/ретраи
    atom-index/                    # Tantivy + ripgrep bridge
    atom-lsp/                      # LSP‑пул
    atom-ext-host/                 # мост VS Code (RPC)
    atom-atom-compat/              # Atom legacy bridge
    atom-plugin/                   # SDK плагинов (WASM/Native)
    atom-sandbox/                  # Wasmtime + политики
    atom-ai/                       # Claude Code + MCP
    atom-settings/                 # конфиг/политики/профили
    atom-persistence/              # SQLite кэши/MRU/сессии
  /apps/
    atom-ide/                      # GUI бинарь
    atomd/                         # core daemon
    atom-ext-host-node/            # Node bootstrap (JS/TS)
```

- **Асинхронщина**: Tokio 1.40+, запрет блокировок в async‑тропах; CPU — `spawn_blocking`.  
- **Clippy/lints**: `-D warnings`; формат — rustfmt nightly‑compatible pinned.  

---

## 4) Детали редактора (детерминированно)

### 4.1 Буферы/редактирование
- mmap‑чтение для крупных файлов; потоковая запись; BOM/encodings авто‑детект.  
- Multi‑cursor, прямоугольные выделения, soft‑wrap, колонки‑гайды; undo‑журнал с чекпойнтом (fsync).

### 4.2 Синтаксис/семантика
- `tree-sitter` парсеры по языкам, отменяемые задачи; конф‑тесты на ABI15. citeturn0search9  
- Семантические токены/подсказки через LSP 3.17; debounce/viewport batching. citeturn0search3

### 4.3 Поиск/символы
- `ripgrep` для запросов; **Tantivy** для индекса (символы/рефы/кросс‑файл). citeturn3search0  
- Ограничение I/O/CPU индексатора; инкрементальные обновления по save/rename.

---

## 5) Мост VS Code (90%+ совместимость)

**Цель**: OOP Node‑хост реализует `vscode` API поверх RPC к ядру.  
**Минимум (Phase‑1)**: `workspace`, `window`, `languages`, `commands`, contrib: commands/languages/grammars/themes.  
**Регистры**: **Open VSX** — `serviceUrl/itemUrl`; `.vsix` drag‑drop. citeturn0search4  
**Sandbox**: per‑extension процесс; fs — только через брокера; `child_process`/сеть — по политике (deny‑by‑default).  
**Юридически**: **не** подключаться к Microsoft Marketplace. citeturn0search5

---

## 6) Совместимость с Atom (legacy)

- Реализация поверх `atom.*` API: workspace/commands/config/keymaps/services/grammars/themes.  
- Транспиляция CoffeeScript на установке; совместимость событий.  
- Репозиторий пакетов atom.io закрыт — используем GitHub Releases/зеркала/конвертер. citeturn0search6turn0search14

---

## 7) Новая система плагинов

### Типы и упаковка
- **WASM** плагин (`.apx` zip+sig): декларируемые **capabilities**, ресурсы, UI‑точки.  
- **Native Rust** — через ревью и повышенные гарантии.

### Безопасность
- **Wasmtime 36.x**: fuel metering, SIMD ON, memory bounds, preopen‑FS ограниченный, egress‑broker, лимиты RAM/CPU/время. LTS поддержка 24 мес. citeturn1search0turn1search6  
- Опционально microVM/контейнер: Firecracker/gVisor — для полностью недоверенного нативного кода (enterprise).

---

## 8) AI‑интеграция (Claude Code + MCP)

### Аутентификация и установка
- Следуем “Set up Claude Code” (CLI/SDK), автообновления, переменные окружения. citeturn2search6  
- Поддерживаем **OAuth/подписку** (SDK/CLI) и **API‑key** fallback (Anthropic SDK), с аккуратным хранением секретов. citeturn2search0turn2search11turn2search3

### Hooks (детерминированные автоматизации)
- Полная реализация событий/хуков из справочника/гайда (PreToolUse/PostToolUse/…); матчеры по ключевым словам/ошибкам/инструментам. citeturn0search7turn0search15

### MCP (клиент/сервер)
- Реализация ревизии **2025‑06‑18**: lifecycle, cap‑negotiation, OAuth‑модель для серверов‑ресурсов. citeturn2search10turn2search2

---

## 9) Безопасность (сквозная модель)
- **PoLA**: минимальные привилегии везде; capability‑политики для плагинов/расширений.  
- **Секреты**: OS keychain (DPAPI/Keychain/libsecret). Запрещено логировать токены.  
- **Сеть**: outbound deny‑by‑default c allowlist/ratelimiting.  
- **Файловая система**: только через брокеров; preopen каталоги для WASM.  
- **Подписи**: подпись плагинов, проверка перед активацией.  
- **Обновления**: канал с проверкой подписи/delta‑апдейты; rollbacks.  

---

## 10) Производительность и телеметрия
- **KPI‑гейты** (см. §0.10) enforced в CI на фиксированном эталонном железе/ВМ.  
- **Трейсинг**: `tracing` + OTLP → JSONL; 95‑й перцентиль в отчетах.  
- **Поиск/индекс**: сочетание ripgrep для adhoc и Tantivy для постоянного индекса. citeturn3search0  

---

## 11) Сборка, подпись, выкладки
- GitHub Actions/Buildkite: матрица win/mac/linux x64/arm64; LTO=thin, split‑debuginfo.  
- **Repro**: `cargo vendor`, зафиксированные lockfiles, **SBOM** (CycloneDX).  
- Подпись: Windows (SignTool), macOS (codesign+notary), Linux (`.deb/.rpm` signing).  
- Обновления: дельты; staged rollout; fast rollback.

---

## 12) QA и совместимость
- Набор VS Code расширений с **Open VSX**; conformance suite. citeturn0search4  
- LSP 3.17 — прогон по популярным серверам/языкам. citeturn0search3  
- Atom legacy — smoke‑тесты community‑пакетов из GitHub.  
- Security‑suite: побеги из песочниц (FS/NET/PROC), OOM, fuel exhaustion.

---

## 13) Пошаговый roadmap (минимум рисков)

**A (6–8 нед.)**: каркас UI (Slint), буферы, файл‑дерево, RG‑поиск, LSP‑пул, Open VSX каталог, базовый чат/AI‑док.  
**B (8–12 нед.)**: VS Code bridge (workspace/window/languages/commands), Atom compat (минимум), Wasmtime host, MCP‑клиент, Hooks MVP.  
**C (8–12 нед.)**: AI‑шэйпинг UI (профили), расширенная Atom compat, паблишинг в Open VSX, перф‑харднинг до KPI.  
**D (∞)**: enterprise‑фичи, microVM sandbox, коллаборация.

---

## 14) Антипаттерны (что **нельзя** делать)
- Подключать **VS Code Marketplace** 🚫 (нарушение ToU). Используем **Open VSX**. citeturn0search5  
- Встраивать webview для базового UI. Только для совместимости webview‑панелей расширений.  
- Блокировать event‑loop в async/не обрабатывать отмену.  
- Давать неограниченный FS/NET доступ расширениям/плагинам.

---

## 15) Приложения (манифесты/IDL)

### 15.1 Манифест WASM‑плагина (.apx/`apx.toml`)
```toml
[plugin]
id = "dev.example.tooling"
name = "Example Tooling"
version = "1.0.0"
type = "wasm"
min_host = "1.0.0"

[capabilities]
fs.read  = ["${workspace}", "${config}"]
fs.write = ["${temp}"]
net.out  = ["api.example.com:443"]

[ui]
surfaces = ["command", "status-item", "panel"]
```

### 15.2 RPC ядро ⇄ ext-host (сокращённо)
```rust
enum ExtHostMsg {
  ShowMessage { level: Level, text: String },
  RegisterCommand { id: String },
  ApplyEdit { uri: Uri, range: Range, text: String },
}
```

---

### Источники (ключевые обновлённые факты)
- **Slint 1.13** (03.09.2025): live‑preview, улучшения языка. citeturn0search8  
- **tree‑sitter 0.25.0** (01.02.2025) и ABI15. citeturn0search9  
- **Wasmtime 36.x** (релизы 20–21.08.2025) и **LTS** (24 месяца). citeturn1search0turn1search6  
- **LSP 3.17** спецификация. citeturn0search3  
- **Open VSX** product.json endpoints. citeturn0search4  
- **Marketplace ToU** (“In‑Scope …”). citeturn0search5  
- **Atom sunset / пакеты**. citeturn0search6turn0search14  
- **Claude Code Hooks/Setup**. citeturn0search7turn0search15turn2search6  
- **MCP 2025‑06‑18**. citeturn2search10  
- **Tantivy** доки/крейты. citeturn3search0turn3search12

> **Superseded**: ранние материалы с упором на Tauri/React/Web UI не применимы к этому проекту (смотрели как черновик). fileciteturn0file0 fileciteturn0file1
