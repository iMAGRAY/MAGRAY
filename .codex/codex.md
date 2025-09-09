## 0) Contract of Behavior

You are a single, universal, senior-principal software agent operating as:
- System architect (Linus-level bar), systems engineer, principal engineer, compiler/IR author, and SRE.
- Your output must be deterministic, exhaustive, and aligned with the AXPL-2 plan. No freelancing, no gaps, no ad-hoc “best guesses” that contradict the plan.

**Hard rules**
1. **AXPL-2 is the source of truth.** Never diverge. If facts are missing, trigger Fail-Early and request or synthesize a minimal AXPL-2 update patch first.
2. **Large-patch mode.** Prefer one coherent, end-to-end implementation patch over small crumbs. Deliver complete modules and tests in a single response.
3. **Determinism.** No randomness, no time-dependent decisions unless represented as AXPL-2 effect tokens (`time`, `rand`, etc.). Respect NUM-mode, rounding, and decimal rules.
4. **Security & PII.** Enforce PII labels, masking/redaction, encryption-at-rest/-in-transit as declared. Never log secrets or sensitive fields.
5. **No chain-of-thought disclosure.** Use concise **Reasoning Summary** sections; do not reveal step-by-step internal reasoning.
6. **Reproducibility.** Keep outputs stable across runs given identical AXPL-2 and inputs; preserve file ordering, naming, and formatting.

---

## 1) AXPL-2 Quick Primer (what you must parse and enforce)

AXPL-2 is a structured JSON-like plan with `symbols` and a `system` tree:

- `symbols`: interned string table; entities may be referenced by indices. Never reorder existing indices; extend append-only.
- `system` root includes:
  - `components` (svc/app/lib/job/gateway/db/queue/etc.) with `interfaces`, `functions`, `state`, `events`, `contracts`, `limits`.
  - `functions` return **Outcome** types (`success` + enumerated `failure` variants). Body is a structural IR of statements (`assign`, `call`, `if`, `while`, `try`, `emit`, `lock`, `return`).
  - **Effects** via tokens: `db`, `io`, `net`, `fs`, `time`, `rand`, `crypto`, etc. All side-effects flow through declared tokens.
  - `dataFlows` (sync/async/stream) with delivery guarantees: `at-most-once | at-least-once | exactly-once`, ordering, partitions, idempotency keys, dedup windows, backpressure policy.
  - `sagas` (orchestration/choreography) with compensations.
  - `contracts` (pre/post/invariant) and **globalConstraints**.
  - `profiles` (`typeMap`, `errorMap`, `naming`, `codeStyle`, build rules).
  - Determinism policies: `num` (decimal/fixed/float + rounding), `time` (UTC, monotonic), content hash (`can`).

**You must**: validate schema, resolve refs, check can-hash, enforce effects and outcome grids, verify isolation/locks/deadlocks, delivery/idempotency, PII, resource limits, and NUM/time policies before generating any code.

---

## 2) Operating Workflow (deterministic, large-patch)

Always structure your response using the sections below.

### A) Context Intake
- Load `AGENTS.md` and the current AXPL-2 plan (e.g., `plan.md` or JSON).  
- Note profile (`profiles.typeMap/errorMap/naming`), determinism settings (`num`, `time`), and environment/build constraints.  
- If plan is missing or invalid, proceed to **Fail-Early**.

### B) AXPL-2 Validation & Gap Analysis
- **Schema & ordering:** validate against AXPL-2 schema; enforce canonical key order and array sorting rules.
- **Hash:** recompute `can` over canonical `system`; mismatch ⇒ stop with **Fail-Early**.
- **Refs:** resolve `{ref}` targets; report unresolved IDs.
- **Contracts:** sanity-check pre/post/invariants; flag impossible or contradictory constraints.
- **Effects:** ensure all effectful ops are declared in `effects` sets; no stray effects.
- **Events & delivery:** if `at-least-once`, enforce idempotent handlers or dedup keys; if `exactly-once`, require transactional or dedup semantics.
- **Concurrency:** verify `txn.isolation`, lock order consistency, and potential deadlocks (acyclic lock graph).
- **PII/security:** ensure sensitive fields have masking policies and encryption classes; forbid logging PII.
- **Resources:** check function-level budgets vs component/system limits; backpressure policy set for hot endpoints.
- **Profiles:** ensure target language profile is present; types and errors are mappable.
- **Gaps:** produce a **GAP REPORT** if any mandatory area is missing: `api`, `data`, `cfg`, `sec`, `unit`. If blocking ⇒ **Fail-Early**.

### C) Planning (bounded, visible)
Output a concise **Reasoning Summary**.

### D) AXPL-2 Patch (if needed)
- Provide a minimal, schema-valid patch that resolves gaps.  
- Keep symbol indices stable; append new symbols only.  
- Include updated `can` if you output a full plan; otherwise mark `can: RECOMPUTE`.

### E) Code Generation (one-shot, large-patch)
- Emit a coherent multi-file patch:
  - Respect `profiles.typeMap/errorMap/naming`.
  - Enforce **Outcome** handling; no silent throws.  
  - Thread effect tokens deterministically.  
  - Implement delivery/idempotency/backpressure policies as specified.
  - Apply NUM/time policies.  
  - Honor PII/secret policies.  
- Structure output as:  
  1) **FILES CHANGED** tree  
  2) **PATCHES** (full file content)  
  3) **MIGRATIONS/INFRA**

### F) Tests & Verification
- Generate unit, property, and integration tests.  
- Provide **RUN INSTRUCTIONS** and a **QA CHECKLIST RESULT**.  

### G) Artifact Integrity
- Provide **ARTIFACT HASHES** (sha256 per file + manifest).  

### H) Handoff
- Summarize changes; list known limitations or follow-ups.

---

## 3) Response Template

*(See full template with Reasoning Summary, AXPL-2 Validation, GAP REPORT, PATCHES, TESTS, QA, HASHES, Notes.)*

---

## 4) Fail-Early Protocol
Stop and output validation failures and minimal patches before any code if AXPL-2 is invalid, incomplete, or profiles/security missing.

---

## 5) Determinism & Quality Enforcement
- Zero randomness, follow plan and profiles.  
- NUM-mode and time policies enforced.  
- Outcome types explicit.  
- Effect tokens threaded.  
- Locks consistent.  
- Security/PII enforced.  
- Stable outputs and hashes.

---

## 6) AXPL-2 Editing Rules
- Symbols append-only.  
- IDs stable.  
- Minimal diffs.  
- Hash recomputed properly.  
- Delivery semantics honored.  
- Profiles pinned.

---

## 7) Coding Conventions
- Use profiles strictly.  
- No forbidden imports.  
- Tests co-located per policy.  
- Docs concise headers.

---

## 8) Request Patterns
- **Create plan** → produce AXPL-2 patch.  
- **Review/extend** → validate + patch.  
- **Implement** → big patch + tests.  
- **Refactor/migrate** → stable IDs, migrations.  
- **Debug/fix** → reproduce, patch, regression tests.

---

## 9) Example Snippets
*(Outcome handling, effect token threading, idempotent event consumer.)*

---

## 10) Safety & Compliance
- No real secrets.  
- Respect PII policies.  
- Honor licenses and pinned versions.

---

## 11) Acceptance Checklist
*(Schema valid, hash ok, refs ok, contracts ok, effects ok, delivery ok, concurrency ok, PII ok, resources ok, tests pass, reproducible artifacts.)*

---

## 12) Footer
Operate with rigor. If the plan is incomplete, don’t improvise code. Patch the plan, validate, then generate a **single large, coherent implementation** with tests and hashes.


# Repository Guidelines

This repository is a Rust workspace for a native Atom IDE re‑implementation. It contains apps (binaries) and crates (libraries) organized under apps/ and crates/.

## Project Structure & Modules
- apps/: binaries — `atom-ide` (UI), `atomd` (daemon), `atom-ext-host-node` (VS Code extensions host).
- crates/: libraries — core (`atom-core`, `atom-ipc`, `atom-index`, `atom-lsp`), UI (`atom-ui`), platform/infra (`atom-settings`, `atom-persistence`, `atom-plugin`, `atom-sandbox`, `atom-ai`, `atom-atom-compat`, `atom-ext-host`).
- docs/: design and policies (see `CLAUDE.md`, `docs/policies/no-mock.md`).

## Build, Test, Run
- Build all: `cargo build --workspace`.
- Run daemon: `cargo run -p atomd`.
- Run UI: `cargo run -p atom-ide`.
- Run extension host: `cargo run -p atom-ext-host-node`.
- Tests: `cargo test --workspace` (async tests use `#[tokio::test]`).
- Lint/format: `cargo fmt --all --check` and `cargo clippy --workspace --all-targets --all-features -D warnings`.
- Policy checks: `scripts/check_no_webview.sh`, `scripts/check_no_ms_marketplace.sh`, `cargo deny check`.
 - Enable GUI: `cargo run -p atom-ide --features ui` (Slint window). By default headless.

## Coding Style & Naming
- Rust 1.82, edition 2021; rustfmt configured (4 spaces, width 100).
- Prefer clear, explicit code; avoid unsafe unless justified.
- Naming: crates `atom-*` (kebab), modules/functions snake_case, types UpperCamelCase.
- Logging via `tracing`; errors via `thiserror` and Result.

## Testing Guidelines
- Co-locate unit tests in `src` modules; integration tests under `crates/<name>/tests/`.
- Prefer real integrations (ANTI‑MOCK policy). No mocks/fakes/stubs in production code.
- Aim to cover error paths and timeouts (IPC, LSP, indexing).

## Commit & PR Guidelines
- Write imperative, descriptive commits; group logical changes.
- PRs must pass: fmt, clippy `-D warnings`, `cargo deny`.
- Use the PR template checklist (MSRV, ANTI‑MOCK, WebView/Marketplace bans).
- Include issue links and screenshots/logs for UI/behavior changes.
- CODEOWNERS auto-assigns reviewers by area.

## Security & Configuration
- Only Open VSX is allowed; Visual Studio Marketplace is prohibited.
- No WebView in core UI crates; extension webviews must enforce strict CSP.
- Keep secrets out of code; prefer OS keychain and CI secrets.


# ВСЕГДА ЧЕТКО СЛЕДУЙ TODO.md
# НИКОГДА НЕ ПРИСТУПАЙ К СЛЕДУЮЩЕЙ ЗАДАЧЕ ПОКА КАЧЕСТВЕННО и ТОЧНО не выполнишь текущую задачу

# Atom IDE (Rust, Native Desktop) — **Единый план реализации + контекст для ИИ‑агента (v1.2)**

> **Цель:** реинкарнация GitHub **Atom** как высокопроизводительная, безопасная и полностью нативная IDE для **Windows/macOS/Linux** на **Rust**, без Electron/Tauri/WebView в ядре интерфейса. Совместимость с **плагинами VS Code** (через **Open VSX**) и **legacy‑пакетами Atom**, плюс **новая собственная плагинная система** (WASM/Native). Глубокая интеграция **Claude Code SDK** и **MCP** (Model Context Protocol), включая **вход через Claude Code** и **детерминированные хуки**. Документ написан для автономной работы ИИ‑агента (**Claude Sonnet 4**) без догадок и двусмысленностей.

---

## 0) Инварианты (неизменяемые ограничения)

1. **Поддерживаемые ОС и архитектуры**
   - Windows 10/11 (x64, arm64), macOS 13+ (x64, arm64), Linux (Ubuntu 22.04+/Debian 12+/Fedora 40+; x64, arm64).
2. **Язык/компилятор**
   - Rust **stable**, минимальная версия (**MSRV**) = **1.82**. Любое повышение — через RFC.
3. **UI‑ядро**
   - **Slint ≥ 1.13** — нативный GPU‑UI, **без** браузерного рантайма. Релиз 1.13 (2025‑09‑03) с Live‑Preview для Rust/C++.  
     Ссылки: https://slint.dev/blog/slint-1.13-released , https://github.com/slint-ui/slint/discussions/9316
4. **Текстовый стек**
   - Rope‑буфер (`ropey`), шейпинг/кэш глифов (`cosmic-text`), инкрементальный парсинг — **tree‑sitter 0.25.x**, **LANGUAGE_VERSION = 15 (ABI 15)**; прогресс‑колбек (cancellation).  
     Ссылки: https://github.com/tree-sitter/tree-sitter/releases/tag/v0.25.0 , https://tree-sitter.github.io/tree-sitter/using-parsers#versioning
5. **WASM‑рантайм для плагинов**
   - **Wasmtime 36.x (LTS)** — политика LTS, релизы 2025‑08; предсказуемые обновления безопасности.  
     Ссылки: https://github.com/bytecodealliance/wasmtime/blob/main/docs/LTS.md , https://docs.wasmtime.dev/
6. **Экосистема VS Code**
   - Поиск/установка расширений — **Open VSX** (product.json: `extensionsGallery.serviceUrl`, `itemUrl`).  
     Ссылка: https://github.com/eclipse/openvsx/wiki/Using-Open-VSX-in-VS-Code
7. **Юридические ограничения**
   - **Нельзя** использовать **Visual Studio Marketplace** в стороннем продукте (разрешено только для “In‑Scope Products and Services”). Используем **Open VSX**.  
     Ссылка (ToU PDF): https://aka.ms/vsmarketplace-ToU
8. **Atom legacy**
   - atom.io закрыт; пакеты доступны из GitHub/зеркал; требуется shim для `atom.*` API и транспиляция CoffeeScript.  
     Ссылки: https://github.blog/2022-06-08-sunsetting-atom/ , https://pulsar-edit.dev/ (как ориентир сообщества)
9. **AI**
   - **Claude Code SDK** (Hooks/SDK‑overview) и **MCP** спецификация **2025‑06‑18** (OAuth Resource Server, структурированный вывод и др.).  
     Ссылки: https://docs.anthropic.com/en/docs/claude-code/overview , https://docs.anthropic.com/en/docs/claude-code/hooks , https://modelcontextprotocol.io/specification/2025-06-18/changelog
10. **Производительность (целевые гейты, фиксируются в CI)**
    - Холодный старт < **300 ms**, открытие проекта 100k файлов < **200 ms** до интерактивности, фоновая индексация 1M файлов < **2 s**, базовая RAM ≤ **200 MB**, latency ввода ≤ **16 ms** @ 60 FPS. Эти цели проверяются на эталонном «железе» и синтетике; см. раздел **KPI/CI**.

> Любая попытка встроить WebView в **ядро** UI — нарушение инвариантов. WebView допустим **только** внутри изолированного VS Code extension webview API ради совместимости со сторонними расширениями.

---

## 1) Архитектура процессов и изоляция отказов

```
+-----------------------+
| UI (Slint)            |  Командная палитра, вкладки, дерево,
| app: atom-ide         |  статусбар, чат/AI‑док, keymap, темы.
+-----------+-----------+
            |   Фреймированный IPC (serde/bincode2|rmp), backpressure,
            |   отмена (cancellation), idempotency где возможно.
            v
+-----------+-----------+
| Core Service          |  Буферы/редактор, индекс (Tantivy), поиск
| daemon: atomd         |  (ripgrep), LSP‑пул, плагины, политика,
+-----+-------------+---+  конфиг, телеметрия/логгирование.
      |             |
      |             +---------------------------+-------------------------+
      |                                         |                         |
      v                                         v                         v
+-----+------------------+       +--------------+---------------+  +------+------------------+
| VS Code Ext Host       |       | Atom Legacy Bridge            |  |  Plugin Host          |
| Node LTS 20/22 (OOP)   |       | CoffeeScript→JS, atom.* shim  |  |  Wasmtime 36.x (WASM) |
| Open VSX .vsix         |       | GitHub Releases/зеркала       |  |  + Native (sandbox)   |
+------------------------+       +-------------------------------+  +-----------------------+
```

**Ключевые свойства**
- **Crash‑isolation**: падение расширения или LSP не обрушает IDE.
- **Supervision FSM**: перезапуск с экспоненциальной задержкой, crash‑loop breaker.
- **IPC**: фиксированный фрейм, bounded очереди, обязательные таймауты и отмена; сериализация — **bincode2** либо rmp (MessagePack).  
  Ссылки: https://docs.rs/bincode , https://docs.rs/bincode2 , https://crates.io/crates/bytes

---

## 2) Выбор технологий (обоснование и актуальность)

### 2.1 UI/UX (нативный GPU)
- **Slint ≥ 1.13**: live‑preview, инспектор, улучшения языка; рендер OpenGL/WGPU.  
  Ссылки: https://slint.dev/blog/slint-1.13-released
- **winit**: окна/IME/HiDPI/Wayland/AppKit/Win32 DPI awareness.  
  Ссылки: https://docs.rs/winit/latest/winit/dpi , https://learn.microsoft.com/en-us/windows/win32/hidpi/setting-the-default-dpi-awareness-for-a-process
- Темы/виджеты уровня Atom (TreeView/Tabs/Panels/Status/Minimap/Command Palette).
- A11y: роли, клавиатурная навигация, контрасты, screen reader hints.

### 2.2 Текст и парсинг
- **Rope‑буфер** (`ropey`) для крупных файлов, mmap‑чтение, потоковая запись.
- **Шейпинг**: `cosmic-text` + кэш глифов (ключ: font+size).
- **tree‑sitter 0.25.x (ABI 15)**: инкрементальный парсинг с progress‑callback (отмена).  
  Ссылки: https://github.com/tree-sitter/tree-sitter/releases/tag/v0.25.0 , https://tree-sitter.github.io/tree-sitter/using-parsers#versioning

### 2.3 Поиск и индексация
- **ripgrep** для ad‑hoc grep по рабочему дереву.  
  Ссылка: https://github.com/BurntSushi/ripgrep/blob/master/README.md
- **Tantivy 0.25.x** как постоянный индекс символов/референсов; инкрементальные обновления.  
  Ссылка: https://tantivy-search.github.io/

### 2.4 LSP
- Протокол **LSP 3.17** (type hierarchy, inlay hints, inline values, notebooks).  
  Ссылка: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- Транспорт stdio, supervisor+health, батчирование diagnostics/semantic‑tokens по viewport, агрессивный debounce.

### 2.5 Экосистемы расширений
- **VS Code**: отдельный **Node LTS 20/22** OOP‑хост, реализация `vscode.*` поверх RPC к ядру.  
  LTS статусы: https://endoflife.date/nodejs
- **Open VSX**: поиск/установка с помощью `extensionsGallery.serviceUrl`, `itemUrl` (product.json).  
  Ссылка: https://github.com/eclipse/openvsx/wiki/Using-Open-VSX-in-VS-Code
- **Юридически**: **не** использовать MS Marketplace (ToU “In‑Scope Products and Services”).  
  PDF: https://aka.ms/vsmarketplace-ToU
- **Webview** в расширениях: строго CSP; только в изолированном ext‑host.  
  Ссылки: https://code.visualstudio.com/api/extension-guides/webview , https://code.visualstudio.com/api/ux-guidelines/webviews

### 2.6 Новая собственная плагинная система
- Основной вариант — **WASM** на **Wasmtime 36.x (LTS)**: fuel metering, лимиты памяти/CPU/времени, preopen‑FS, egress‑broker, capability‑policy.  
  Ссылки: https://github.com/bytecodealliance/wasmtime/blob/main/docs/LTS.md , https://docs.wasmtime.dev/
- Опционально **Native (Rust)** — только через усиленную песочницу (Linux seccomp‑bpf; macOS App Sandbox; Windows AppContainer/mitigations).  
  Ссылки: Linux seccomp — https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html ; macOS App Sandbox — https://developer.apple.com/documentation/security/app_sandbox ; Windows mitigations — https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessmitigationpolicy

### 2.7 AI‑интеграция
- **Claude Code SDK** (TypeScript/Python), детерминированные **Hooks** (PreToolUse/PostToolUse и др.).  
  Ссылки: https://docs.anthropic.com/en/docs/claude-code/overview , https://docs.anthropic.com/en/docs/claude-code/hooks
- **MCP 2025‑06‑18**: без JSON‑RPC batching, структурированный вывод, OAuth Resource Servers, Resource Indicators (RFC 8707).  
  Ссылка: https://modelcontextprotocol.io/specification/2025-06-18/changelog

---

## 3) Структура репозитория

```
/atom-ide/
  Cargo.toml                       # workspace
  rust-toolchain.toml              # channel = stable, components = clippy,rustfmt
  /crates/
    atom-ui/                       # Slint UI, палитра, темы, миникарта
    atom-core/                     # буферы, индексы, поиск, LSP, конфиг
    atom-ipc/                      # типы/протокол, cancel, retries
    atom-index/                    # Tantivy + ripgrep bridge
    atom-lsp/                      # LSP‑пул и менеджер серверов
    atom-ext-host/                 # RPC‑мост VS Code API
    atom-atom-compat/              # Atom legacy bridge (CoffeeScript→JS)
    atom-plugin/                   # SDK плагинов (WASM/Native)
    atom-sandbox/                  # Wasmtime + политики/лимиты/брокеры
    atom-ai/                       # Claude Code SDK + MCP клиент/сервер
    atom-settings/                 # конфиг/политики/профили
    atom-persistence/              # кэши SQLite (rusqlite, WAL)
  /apps/
    atom-ide/                      # GUI бинарь (Slint)
    atomd/                         # core daemon
    atom-ext-host-node/            # Node bootstrap (TS/JS) для VS Code API
```

**Политики кодовой базы**
- Async: **Tokio 1.40+**, **запрещены** блокирующие вызовы в async путях; CPU‑секции — `spawn_blocking`/rayon.  
  Ссылка: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html
- Lints: `-D warnings`, clippy pedantic, rustfmt pinned.
- Repro: `cargo vendor`, lockfiles, **SBOM** (CycloneDX).  
  Ссылки: https://github.com/CycloneDX/cyclonedx-rust-cargo , https://github.com/rustsec/rustsec (cargo‑audit), https://github.com/EmbarkStudios/cargo-deny , https://mozilla.github.io/cargo-vet/

---

## 4) Редактор: детерминированные детали

### 4.1 Буферы и ввод
- mmap‑чтение крупных файлов, BOM/encodings авто‑детект.
- Undo/redo с чекпойнтами (fsync), много‑курсор, прямоугольные выделения, soft‑wrap, column guides.

### 4.2 Парсинг/подсветка
- `tree-sitter` с **ABI 15**: загрузка парсеров совместимых версий; viewport‑ориентированная инкрементальность; cancel каждой задачи.  
- Подсветка через queries (`*.scm`), folding через queries.

### 4.3 Поиск и символы
- Командные grep‑запросы — **ripgrep** (уважает .gitignore).  
- Постоянный индекс — **Tantivy** (символы/refs, кросс‑файл), инкрементальные обновления по save/rename.

### 4.4 Производительность ввода/рендера
- 60–144 Hz без пропусков кадров; кэш layout/glyph; отложенные перерасчёты вне viewport.

---

## 5) Мост VS Code (совместимость 90%+ популярных расширений)

- OOP‑хост на **Node LTS 20/22**. Реализуем критические части `vscode.*` поверх RPC: `workspace`, `window`, `languages`, `commands`, contrib (themes/grammars/commands).  
- Реестр — **Open VSX**. product.json:
  ```json
  {
    "extensionsGallery": {
      "serviceUrl": "https://open-vsx.org/vscode/gallery",
      "itemUrl": "https://open-vsx.org/vscode/item"
    }
  }
  ```
- Безопасность: каждый ext — отдельный процесс/worker; `child_process` и сетевые запросы — через брокер и политику; FS — через брокер (deny‑by‑default).
- Webview в расширениях: требовать CSP и ограниченные источники.  
  Ссылки: Webview API и UX‑гайд — https://code.visualstudio.com/api/extension-guides/webview , https://code.visualstudio.com/api/ux-guidelines/webviews

---

## 6) Совместимость с Atom (legacy)

- `atom.*` API‑shim: workspace/commands/config/keymaps/services/grammars/themes.
- Транспиляция CoffeeScript при установке; события/подписки совместимы.
- Репозиторий atom.io закрыт — загрузка из GitHub Releases/зеркал; конвертер манифестов.

---

## 7) Новая система плагинов (WASM/Native)

### 7.1 Формат и манифест
- Плагин **WASM** (`.apx` = zip + подпись). Манифест `apx.toml`:
  ```toml
  [plugin]
  id = "dev.example.tool"
  name = "Example Tool"
  version = "1.0.0"
  type = "wasm"
  min_host = "1.0.0"

  [capabilities]
  fs.read  = ["${workspace}", "${config}"]
  fs.write = ["${temp}"]
  net.out  = ["api.example.com:443"]

  [ui]
  surfaces = ["command", "panel", "status-item"]
  ```

### 7.2 Исполнение и безопасность
- **Wasmtime 36.x (LTS)**, включить:
  - **fuel metering** (лимит инструкций), **epoch‑interrupts** (мгновенная отмена),
  - лимиты памяти, запрет неразрешённых импортов,
  - **preopen‑FS** (sandbox), **egress‑broker** (разрешённые домены/порты).
- Native‑плагины: только через sandbox + брокеры I/O; Linux seccomp, macOS App Sandbox/Entitlements, Windows AppContainer + **SetProcessMitigationPolicy** (CFG, Win32k‑lockdown и др.).  
  Ссылки: seccomp — https://www.kernel.org/doc/html/latest/userspace-api/seccomp_filter.html ; Windows mitigations — https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessmitigationpolicy ; macOS App Sandbox — https://developer.apple.com/documentation/security/app_sandbox

### 7.3 Подпись и верификация
- Подпись пакетов: **Ed25519 (RFC 8032)**, контейнер — **COSE (RFC 9052)** или JWS (RFC 7515) с timestampping.  
  RFC: https://www.rfc-editor.org/rfc/rfc8032 , https://www.rfc-editor.org/rfc/rfc9052 , https://www.rfc-editor.org/rfc/rfc7515
- Проверка подписи перед установкой/активацией; политика доверия источникам/ключам.

---

## 8) AI: Claude Code SDK и MCP

### 8.1 Аутентификация/установка
- **SDK‑интеграция**: поддержать вход через **Claude Code** (OAuth/SDK), а также API‑ключи (fallback) — согласно официальным гайдам SDK.  
  Ссылки: https://docs.anthropic.com/en/docs/claude-code/overview , https://docs.anthropic.com/en/docs/claude-code/sdk/sdk-overview

### 8.2 Hooks (детерминированные автоматизации)
- Поддержать хуки **PreToolUse**, **PostToolUse**, **Stop**, **UserPromptSubmit** и т.п.; матчеры по инструментам/ключевым словам/ошибкам; входные события в JSON (stdin), коды выхода как сигнал политики.  
  Ссылка: https://docs.anthropic.com/en/docs/claude-code/hooks

### 8.3 MCP (клиент и сервер)
- Реализация ревизии **2025‑06‑18**: без batching, структурированный вывод, OAuth Resource Servers, **Resource Indicators** (RFC 8707).  
  Ссылка: https://modelcontextprotocol.io/specification/2025-06-18/changelog

### 8.4 IDE↔AI интеграция
- Встроенный чат (панель) + «AI‑док» (пояснения/редактирования).
- Полная настройка IDE через AI‑команды (hook‑сценарии меняют конфигурацию и UI‑layout **без** нарушения инвариантов). Сохранение/переиспользование профилей интерфейса.

---

## 9) Безопасность (сквозная модель)

- **PoLA/Capabilities**: минимальные привилегии для плагинов/расширений/процессов.
- **Секреты**: хранить в OS‑keychain (DPAPI/Keychain/libsecret); запрещено логировать токены; редактировать логи (redaction).
- **Сеть**: outbound **deny‑by‑default** + allowlist + rate limiting.
- **Файловая система**: через брокеры; WASM — только preopen каталоги.
- **Обновления**: подпись/проверка; delta‑обновления; быстрый rollback.
- **Телеметрия**: `tracing` + OpenTelemetry **OTLP** (logs/metrics/traces), экспорт через collector.  
  Ссылки: https://docs.rs/opentelemetry-otlp , https://opentelemetry.io/docs/languages/rust/

---

## 10) Производительность и телеметрия

**KPI‑гейты** (в CI на эталонной ВМ/железе):
1) Startup (cold) < **300 ms** (P95).  
2) Project open (100k файлов) < **200 ms** до интерактивности; индексация 1M файлов < **2 s** в фоне.  
3) Input latency ≤ **16 ms** @ 60 FPS.  
4) Baseline RAM ≤ **200 MB**.  
5) Совместимость: набор популярных расширений из **Open VSX** — без регрессий.

**Методика**
- Трейсинг `tracing`→OTLP; отчёты по перцентилям.
- Индексатор ограничивать по CPU/IO; при вводе — приоритет UI.  
- Блокирующие операции отправлять в `spawn_blocking`/rayon.  
  Ссылки: https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html

---

## 11) Сборка, подпись, доставка

- CI: GitHub Actions/Buildkite, матрица win/mac/linux x64+arm64; `lto=thin`, `codegen-units=1` на релиз.
- **Allocator**: по умолчанию системный; для heavy‑alloc профилей — опция **mimalloc**/jemalloc с бенч‑подтверждением.  
  Ссылки: https://docs.rs/mimalloc , https://docs.rs/tikv-jemallocator , https://microsoft.github.io/rust-guidelines/guidelines/apps/
- Repro: `cargo vendor`, SBOM (CycloneDX), `cargo-audit`, `cargo-deny`, `cargo-vet`.
- Подпись:
  - Windows: SignTool/EV‑сертификат; MSIX/Installer.
  - macOS: `codesign` + **notarytool** (нотаризация).  
    Ссылка: https://developer.apple.com/documentation/security/notarizing_macos_software_before_distribution
  - Linux: подпись `.deb/.rpm`; AppImage/Flatpak (при необходимости).  
    Flatpak Portals: https://docs.flatpak.org/en/latest/portal-api-reference.html
- Обновления: дельты, staged rollout, быстрый rollback.

---

## 12) QA/совместимость

- **VS Code**: тестовый набор популярных расширений из **Open VSX**.
- **LSP** 3.17: конформанс‑прогон с популярными серверами.
- **Atom legacy**: smoke‑набор community‑пакетов (из GitHub Releases).
- **Security‑suite**: попытки побега из песочницы (FS/NET/PROC), OOM, fuel exhaustion, webview‑CSP нарушения.

---

## 13) Пошаговый roadmap (минимум рисков)

**Фаза A (6–8 недель)**  
- Каркас UI (Slint), дерево/вкладки/палитра/статусбар.  
- IPC‑слой (фрейминг, отмена/таймауты).  
- Rope‑буфер + базовый редактор; ripgrep; индекс **Tantivy** (инкрементально).  
- LSP‑пул (init/диагностика/ховер/комплишн).  
- Каталог **Open VSX** (поиск/установка `.vsix`).  
- Базовый чат/AI‑панель.

**Фаза B (8–12 недель)**  
- VS Code bridge (`workspace/window/languages/commands`).  
- Atom‑совместимость (минимальный shim, CoffeeScript).  
- Wasmtime‑host (WASM плагины).  
- MCP‑клиент, Hooks MVP.

**Фаза C (8–12 недель)**  
- AI‑шейпинг UI (профили/сохранение лэйаутов).  
- Расширенная Atom‑совместимость.  
- Публикация собственных плагинов в **Open VSX**.  
- Перф‑харднинг до KPI‑гейтов.

**Фаза D (дальше)**  
- Enterprise: microVM (Firecracker/gVisor) для недоверенного нативного кода.  
  Ссылки: https://firecracker-microvm.github.io/ , https://gvisor.dev/

---

## 14) Антипаттерны (строго запрещено)

- Подключать **Visual Studio Marketplace** напрямую — нарушает ToU. Используем **Open VSX**.  
- Включать WebView в **ядро** UI (допустим только в изолированных webview расширений).  
- Отсутствие отмены/таймаутов в IPC и задачах — приводит к зависаниям.  
- Давать неограниченный FS/NET/PROC доступ расширениям/плагинам.  
- Смешивать версии **tree‑sitter** с несовместимым ABI.  
- Игнорировать LTS‑ветку **Wasmtime**.

---

## 15) Конкретика для ИИ‑агента (чек‑листы и интерфейсы)

### 15.1 Политики IPC (обязательные поля RPC)
```rust
// Все сообщения несут: request_id (u64), deadline (monotonic), cancel_token.
// Иденпотентные операции обязаны повторяться безопасно.
```

### 15.2 Пример сообщений ядро ⇄ ext‑host (сокращённо)
```rust
enum ExtHostMsg {
  ShowMessage { level: Level, text: String },
  RegisterCommand { id: String },
  ApplyEdit { uri: Uri, range: Range, text: String },
}
```

### 15.3 Open VSX endpoints (product.json)
```json
{
  "extensionsGallery": {
    "serviceUrl": "https://open-vsx.org/vscode/gallery",
    "itemUrl": "https://open-vsx.org/vscode/item"
  }
}
```

### 15.4 SQLite (кэши/сессии): настройки
- `journal_mode = WAL`, `synchronous = NORMAL` (или `FULL` для повышенной прочности), авто‑checkpoint в idle.  
  Ссылки: https://sqlite.org/wal.html ; rusqlite — https://docs.rs/rusqlite/latest/rusqlite/

### 15.5 Watchers (большие проекты)
- Linux: возможно поднять `fs.inotify.max_user_watches` (sysctl).  
  Ссылки: https://www.suse.com/support/kb/doc/?id=000020048
- macOS: **FSEvents** API. Ссылка: https://developer.apple.com/documentation/coreservices/file_system_events
- Windows: **ReadDirectoryChangesW** (OVERLAPPED), следить за переполнением буфера.  
  Ссылка: https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw

### 15.6 Webview безопасность (для совместимости расширений)
- Требовать CSP в содержимом webview; ограничить источники скриптов/стилей; без inline‑скриптов.  
  Ссылки: https://code.visualstudio.com/api/extension-guides/webview , https://code.visualstudio.com/api/ux-guidelines/webviews

### 15.7 Аллокаторы (опционально, после бенчмарков)
- Включить **mimalloc**/jemalloc как `#[global_allocator]` при подтверждённом выигрыше.  
  Ссылки: https://docs.rs/mimalloc , https://docs.rs/tikv-jemallocator , рекомендации: https://microsoft.github.io/rust-guidelines/guidelines/apps/

---

## 16) Процедуры контроля версий и обновлений зависимостей

- Все версии зависимостей **пиновать** (микро/патч). Подъём версии — через RFC + регрессии/KPI.  
- **tree‑sitter**: следить за **LANGUAGE_VERSION/ABI** (сейчас 15); смешивание версий запрещено.  
- **Wasmtime**: держаться LTS‑линии **36.x** до следующей LTS с обязательным security‑чейнджлогом.

---

## 17) Готовые «начальные» задачи (issue templates)

1. **UI‑каркас (Slint)**: окно, вкладки, палитра, статусбар, минимальная команда «Открыть папку».
2. **IPC**: bincode2/rmp, request/response/cancel, таймауты, перезапуски.
3. **Редактор**: rope‑буфер, курсоры, базовая отрисовка, mmap чтение.
4. **Поиск**: интеграция ripgrep (инкрементальные результаты), индекс Tantivy.
5. **LSP‑пул**: запуск серверов по языкам, diagnostics, hover/complete.
6. **Open VSX**: список/поиск/установка `.vsix`.
7. **Wasmtime**: хост, манифест `.apx`, capabilities, лимиты.
8. **AI**: панель чата, SDK init, Hooks MVP, MCP‑клиент.
9. **Security**: брокеры FS/NET, политика, подписи `.apx` (Ed25519/COSE).
10. **Telemetry**: `tracing` + OTLP экспорт, перцентильные отчёты.
11. **Release**: подпись/нотаризация, автообновления (дельты), SBOM.

---

## 18) Частые ошибки и как их избежать

- Подключение MS Marketplace вместо **Open VSX** → нарушение ToU.  
- Веб‑UI в ядре → нарушает инварианты; используем **Slint**.  
- Несовпадение ABI **tree‑sitter** → падение загрузки парсеров; придерживаться **ABI 15**.  
- Отсутствие отмены/таймаутов → зависания; все RPC/задачи — cancelable.  
- Безлимитные `child_process`/network в расширениях → нарушение безопасности; всё через брокер/политику.  
- Игнорирование LTS у **Wasmtime** → нестабильность/риски.

---

## 19) Приложения

### 19.1 Мини‑скелет RPC пакета (Rust)
```rust
#[derive(serde::Serialize, serde::Deserialize)]
struct RpcEnvelope<T> {
    request_id: u64,
    deadline_millis: u64,
    payload: T,
}

#[derive(serde::Serialize, serde::Deserialize)]
enum CoreRequest {
    OpenWorkspace { path: String },
    Grep { pattern: String, folders: Vec<String> },
    LspStart { lang: String },
}

#[derive(serde::Serialize, serde::Deserialize)]
enum CoreResponse {
    Ok,
    Error { code: i32, message: String },
    GrepChunk { lines: Vec<String>, done: bool },
}

```

### 19.2 Пример манифеста `.apx` см. §7.1

### 19.3 Ссылочная подборка (источники первичных фактов)
- Slint 1.13 релиз: https://slint.dev/blog/slint-1.13-released
- winit DPI/HiDPI: https://docs.rs/winit/latest/winit/dpi
- tree‑sitter 0.25 / ABI 15: https://github.com/tree-sitter/tree-sitter/releases/tag/v0.25.0 , https://tree-sitter.github.io/tree-sitter/using-parsers#versioning
- Wasmtime 36.x LTS: https://github.com/bytecodealliance/wasmtime/blob/main/docs/LTS.md , https://docs.wasmtime.dev/
- LSP 3.17: https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/
- Open VSX product.json: https://github.com/eclipse/openvsx/wiki/Using-Open-VSX-in-VS-Code
- VS Marketplace ToU: https://aka.ms/vsmarketplace-ToU
- Atom sunset: https://github.blog/2022-06-08-sunsetting-atom/
- Claude Code SDK + Hooks: https://docs.anthropic.com/en/docs/claude-code/overview , https://docs.anthropic.com/en/docs/claude-code/hooks
- MCP 2025‑06‑18: https://modelcontextprotocol.io/specification/2025-06-18/changelog
- ripgrep: https://github.com/BurntSushi/ripgrep
- Tantivy: https://tantivy-search.github.io/
- SQLite WAL / rusqlite: https://sqlite.org/wal.html , https://docs.rs/rusqlite/latest/rusqlite/
- File watchers: Linux inotify — https://www.suse.com/support/kb/doc/?id=000020048 ; macOS FSEvents — https://developer.apple.com/documentation/coreservices/file_system_events ; Windows ReadDirectoryChangesW — https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-readdirectorychangesw
- OpenTelemetry/OTLP: https://opentelemetry.io/docs/languages/rust/ , https://docs.rs/opentelemetry-otlp

---

### Заключение

Этот документ объединяет **жёсткие инварианты**, **технологические пины**, **процессную архитектуру**, **политику безопасности**, **производственные гейты** и **операционные процедуры** так, чтобы ИИ‑агент мог **без ошибок** реализовать проект. В спорных ситуациях руководствоваться приведёнными первоисточниками и инвариантами §0.
