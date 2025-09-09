# ANTI-MOCK Policy — Atom IDE

> **Принцип:** Все интеграции должны быть либо **реальными**, либо **fail-closed** с явной пользовательской ошибкой. Никаких моков, заглушек, эмуляторов или "dev bypass" в производственном коде.

## Запрещённые практики

### 1. Mock/Fake/Stub объекты и модули
- `cfg(feature = "mock")`  
- `mod mock`  
- `use mock`  
- Зависимости: `mockall`, `wiremock`, `httpmock`  
- Модули с префиксами: `fake_*`, `stub_*`, `dummy_*`

### 2. Development bypasses
- `insecure-dev-signature` — обход проверки подписи пакетов
- `ai-mock` — фейковые ответы AI/MCP  
- `fs-sandbox-mock` — обход файловых ограничений
- `offline-fallback` — работа без сети вместо ошибки
- Конфигурационные флаги типа `dev-bypass`, `testonly`

### 3. Терминальные заглушки в продукционном коде
- `todo!()`  
- `unimplemented!()`  
- `panic!("TODO")` или `panic!("Not implemented")`  
- Комментарии `// FIXME:`, `// TODO:` в критических путях

### 4. Эмуляция внешних сервисов
- Локальные HTTP серверы вместо реальных API
- In-memory базы данных вместо SQLite/PostgreSQL  
- Fake OAuth endpoints
- Эмуляция Open VSX registry

## Разрешённые альтернативы

### Fail-closed подход
- **Сеть недоступна** → UI показывает "Нет соединения с Open VSX"
- **AI токен отсутствует** → "Требуется вход в Claude Code"  
- **Подпись пакета невалидна** → "Пакет не подписан или подпись повреждена"
- **LSP сервер не найден** → "Установите rust-analyzer для поддержки Rust"

### Реальные интеграции в тестах  
- Интеграционные тесты используют **реальные** LSP серверы
- E2E тесты обращаются к **настоящему** Open VSX (с rate limiting)
- Security тесты проверяют **реальные** песочницы OS

### Конфигурируемые endpoints
```toml
# ~/.atom/config.toml - пример корректной конфигурации
[extensions]
registry_url = "https://open-vsx.org/vscode/gallery"  # Реальный Open VSX

[ai]  
endpoint = "https://api.anthropic.com/v1/messages"     # Реальный Claude API

[telemetry]
otlp_endpoint = "https://api.honeycomb.io"            # Реальный OTLP collector
```

## Enforcement в CI

### Автоматические проверки
1. **no-mock-enforcement** job проверяет отсутствие запрещённых паттернов
2. **dependency-scan** анализирует Cargo.toml на mock-крейты  
3. **dev-deps-isolation** проверяет, что dev-зависимости не попадают в release
4. **fail-closed-tests** запускает offline-сценарии и требует ошибок

### Скрипты проверки
- `scripts/no_mock_enforce.sh` — сканирование кодовой базы
- `scripts/check_dev_deps_in_release.sh` — изоляция dev-deps
- `scripts/fail_closed_test.sh` — проверка offline-поведения

## Исключения

**Единственное исключение:** документация в `docs/policies/no-mock.md` (этот файл) может содержать запрещённые термины для описания политики.

---

*Последнее обновление: 2025-01-09*  
*Статус: Обязательна к соблюдению во всех крейтах*