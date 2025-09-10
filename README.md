Atom IDE (Native, Rust)
=======================

[![Workflow](https://github.com/magray/atom-ide/actions/workflows/toolchain.yml/badge.svg)](https://github.com/magray/atom-ide/actions/workflows/toolchain.yml)

- MSRV: Rust 1.82.0
- UI: Slint (software renderer), без WebView
- Расширения: Open VSX (не VS Marketplace)
- IPC: кастомный протокол (1 MiB кадр, 14‑байтовый заголовок), cancel/deadline/backpressure

Документация
------------

- План: `plan.md`
- Задачи: `TODO.md`
- Политики: `scripts/check_no_webview.sh`, `scripts/check_no_ms_marketplace.sh`

Сборка и тесты (локально)
-------------------------

```
cargo build -p atom-ipc -p atomd -p atom-ide
cargo test  -p atom-ipc --tests -- --test-threads=1
cargo test  -p atomd --test e2e -- --test-threads=1
cargo test  -p atom-ide e2e_ide_headless_starts_and_exits -- --test-threads=1
```

E2E крайних сценариев (deadline/backpressure)
--------------------------------------------

```
cargo test -p atom-ide e2e_headless_deadline -- --test-threads=1
cargo test -p atom-ide e2e_headless_backpressure -- --test-threads=1
```

CI
--

- Linux (`tests-linux`): установка ripgrep, сборка, тесты IPC/daemon/IDE (headless)
- Windows (`tests-windows-ui`): сборка UI, запуск GUI‑smoke `e2e_ui`

