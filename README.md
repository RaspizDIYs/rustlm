# RustLM

Менеджер аккаунтов League of Legends для Windows: хранение аккаунтов, автоматический вход и автоматизация чемп-селекта.

Десктопное приложение на Tauri v2 + Next.js + Rust.

## Возможности

- **Хранилище аккаунтов** — пароли шифруются через Windows DPAPI, экспорт/импорт зашифрованных бэкапов
- **Автоматический вход** — машина состояний логина (`detect_login_phase`) + Windows UI Automation; вход при уже запущенном клиенте без лишних перезапусков
- **Авто-принятие матча** — WebSocket / polling листенеры LCU
- **Автоматизация чемп-селекта** — авто-бан, авто-пик чемпиона, заклинаний и рун, редактор страниц рун
- **Конфиги** — управление `PersistedSettings.json`: read-only toggle, пресеты настроек, экспорт/импорт `.lolcfg`
- **Разведка (Spy)** — данные по игрокам лобби через Riot API
- **Облако** — синхронизация аккаунтов, настроек и рун через GoodLuck API (с 2FA / TOTP)
- **Системный трей** — быстрый доступ к авто-принятию и аккаунтам, режим запуска в фоне, автозагрузка с Windows
- **Авто-обновления** — тихая установка через Tauri updater

## Стек

- [Tauri v2](https://tauri.app/) — десктоп-оболочка, Rust backend
- [Next.js 16](https://nextjs.org/) (App Router) + React 19
- [shadcn/ui](https://ui.shadcn.com/) + Tailwind CSS v4
- Прозрачное окно с кастомным тайтлбаром

## Разработка

Требования: Node.js, Rust toolchain, [зависимости Tauri](https://tauri.app/start/prerequisites/) для Windows.

```bash
npm install
npm run tauri:dev      # запуск в режиме разработки
npm run tauri:build    # сборка релизного установщика (NSIS)
```

## Структура

```
src/                  # Next.js фронтенд
src-tauri/
  src/commands/        # Tauri-команды (login, accounts, ...)
  src/services/        # riot_client, uia_login, auto_accept, ...
```

## Лицензия

[MIT](LICENSE) © 2026 Alexandr Nazarenko

---

> RustLM не аффилирован с Riot Games. League of Legends — товарный знак Riot Games, Inc.
