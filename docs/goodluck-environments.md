# Режимы сборки RustLM (фронт + GoodLuck API)

## Три сценария

| Что нужно | Команда | Фронт | GoodLuck API |
|-----------|---------|-------|----------------|
| **Дев, фронт в реальном времени (HMR), прод** | `npm run tauri:dev` | Next на `localhost:3000` в WebView | `gltournament.ru` |
| **Дев, фронт в реальном времени, тестовый бэкенд** | `npm run tauri:dev:test` | то же | `test.gltournament.ru` (фича `goodluck-test`) |
| **Как у релиза, но тестовый API** | `npm run tauri:build:test` | статический `out/` в бинаре | `test.gltournament.ru` |
| **Прод-релиз** | `npm run tauri:build` | статический `out/` | `gltournament.ru` |

В дев-режиме UI всегда с **localhost:3000** — так устроен Tauri (`beforeDevCommand` + `devUrl`). К **тесту** или **проду** ходит только **Rust** (`goodluck_auth.rs` + фича `goodluck-test`).

## Запуск тестового приложения (собранного exe)

1. Один раз собрать: `npm run tauri:build:test`
2. Запуск без переустановки: `npm run tauri:run:test-build`  
   (ищет `src-tauri/target/release/rustlm.exe` или `RustLM.exe`)

Собрать и сразу открыть: `npm run tauri:build:test:run`

GitHub Release (workflow `Release`, теги `v*`) — только **прод**, без `goodluck-test`.

## Сверка перед релизом

1. Интеграция с тестом: `tauri:dev:test` и/или `tauri:build:test` + смоук на `test.gltournament.ru`.
2. Перед выкладкой: `tauri:build` и те же сценарии на `gltournament.ru`.

Проверка, что домены только в Rust:

`rg gltournament src-tauri`
