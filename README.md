# Elysium Launcher

Лаунчер для клиентского мода **Elysium Visuals** (Minecraft 26.2, Fabric).
Стек: **Tauri 2** (Rust) + **React 19 / TypeScript / Vite**, zustand, framer-motion, lucide-react, Inter.

## Возможности
- Вход через Microsoft (Device Code → Xbox Live → XSTS → Minecraft) и офлайн-аккаунты, несколько аккаунтов, головы скинов.
- Сам скачивает Minecraft 26.2, библиотеки, ассеты, нативы, Fabric Loader 0.19.5, Fabric API и последнюю версию мода из GitHub Releases.
- Нет Java 25 → скачивает портативную Temurin 25 JRE в `%APPDATA%\ElysiumLauncher\runtime`.
- Проверка по SHA-1, докачивается только недостающее; работает и офлайн, если файлы уже скачаны.
- Консоль с логами в реальном времени, диалог при краше, автообновление лаунчера.

## Структура
```
src/                      UI (React)
  api/                    типизированные вызовы Rust-команд и событий
  store/                  состояние: settings, accounts, launch, console, toasts, nav
  components/             TitleBar, Sidebar, PlayButton, AccountPicker, SkinHead, RamSlider, …
  pages/                  Play, Settings, Console, Accounts
  styles/                 tokens.css (тема), glass.css (Liquid Glass), app.css
src-tauri/src/
  config.rs               версии MC/Fabric, Client ID, репозиторий мода
  auth/                   microsoft.rs, xbox.rs, minecraft.rs, offline.rs, store.rs (Credential Manager)
  download/               параллельная загрузка + SHA-1, manifest, libraries, assets, java
  fabric/                 профиль Fabric, Fabric API, установка мода
  launch/                 конвейер установки, аргументы запуска, процесс игры и логи
  settings.rs, system.rs, updates.rs
.github/workflows/        сборка и публикация релизов
```

## Требования (один раз)
1. **Node.js** 20+.
2. **Rust**: <https://rustup.rs> (toolchain `stable-x86_64-pc-windows-msvc`).
3. **Visual Studio Build Tools** с компонентом «Разработка классических приложений на C++».
4. WebView2: в Windows 10/11 уже есть.

```powershell
npm install
```

## Режим разработки
```powershell
npm run tauri dev
```
Откроется окно лаунчера с горячей перезагрузкой UI. В dev-сборке мод берётся прямо из
`../elysium-visuals-template-26.2/build/libs/` — после `gradlew build` в проекте мода
достаточно снова нажать «Играть».

Тесты Rust:
```powershell
cd src-tauri
cargo test                                  # быстрые юнит-тесты
cargo test e2e -- --ignored --nocapture     # полная установка в %APPDATA%\.elysium и запуск игры
```

## Скачать
Установщик — в [последнем релизе](https://github.com/kirillyalovenko463-hash/elysium-launcher/releases/latest):
`Elysium Launcher_<версия>_x64-setup.exe`.

## Сборка установщика .exe локально
Обновления подписываются, поэтому для сборки нужен ключ подписи:
```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content -Raw "$HOME\.tauri\elysium-launcher.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Get-Content -Raw "$HOME\.tauri\elysium-launcher.key.password"
npm run tauri build
```
Готовый установщик и его подпись:
`src-tauri\target\release\bundle\nsis\Elysium Launcher_<версия>_x64-setup.exe` (+ `.sig`).

Мод в установщик не входит — лаунчер скачивает его сам.
Версия меняется в `package.json`, `src-tauri/tauri.conf.json` и `src-tauri/Cargo.toml`.

## Вход через Microsoft: Client ID
Microsoft требует, чтобы у каждого лаунчера был свой зарегистрированный ID приложения:
1. <https://portal.azure.com> → **App registrations** → **New registration**.
   Supported account types: **Personal Microsoft accounts only**. Redirect URI можно не указывать.
2. В приложении: **Authentication** → **Allow public client flows** → **Yes** → Save.
3. Скопируйте **Application (client) ID**.
4. Подайте заявку на доступ к Minecraft API: <https://aka.ms/mce-reviewappid>
   (пока Mojang её не одобрит, вход завершится ошибкой «Client ID ещё не одобрен Mojang»).
5. Подставьте ID одним из способов:
   - в `src-tauri/src/config.rs` (константа `MS_CLIENT_ID`), или
   - переменной окружения при сборке: `$env:ELYSIUM_MS_CLIENT_ID="ваш-id"; npm run tauri build`.

Пока ID не задан, кнопка «Войти через Microsoft» неактивна, офлайн-режим работает.

## Обновления
**Мод.** При каждом запуске игры лаунчер смотрит последний релиз
[elysium-visuals](https://github.com/kirillyalovenko463-hash/elysium-visuals/releases) (`MOD_REPO` в
`config.rs`) и, если версия новее, скачивает `.jar` (кроме `*-sources.jar`) в
`%APPDATA%\ElysiumLauncher\mod\`. Без интернета используется уже скачанная копия.
Чтобы выпустить новую версию мода, создайте в его репозитории релиз с тегом `vX.Y.Z`
(совпадает с `version` в `fabric.mod.json`) и приложите jar.

**Лаунчер.** Tauri updater читает `releases/latest/download/latest.json` этого репозитория,
скачивает установщик и проверяет подпись публичным ключом из `tauri.conf.json`.
Кнопка «Обновить» в настройках ставит обновление и перезапускает лаунчер.

Выпуск новой версии лаунчера:
1. Поднимите версию в `package.json`, `src-tauri/tauri.conf.json` и `src-tauri/Cargo.toml`.
2. Закоммитьте и запушьте тег: `git tag v1.1.0; git push origin main v1.1.0`.
3. GitHub Actions (`.github/workflows/release.yml`) соберёт подписанный установщик и опубликует
   релиз с `latest.json` — установленные лаунчеры предложат обновиться.

Секреты репозитория (Settings → Secrets and variables → Actions):

| Секрет | Что это |
|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | содержимое `~/.tauri/elysium-launcher.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | пароль к ключу (`~/.tauri/elysium-launcher.key.password`) |
| `ELYSIUM_MS_CLIENT_ID` | необязательно: Azure Client ID для входа через Microsoft |

> Приватный ключ есть только у вас и в секретах GitHub. Если его потерять, установленные
> лаунчеры не смогут принять обновления — сохраните резервную копию.

## Где лежат данные
| Что | Где |
|---|---|
| Настройки, список аккаунтов | `%APPDATA%\ElysiumLauncher\settings.json`, `accounts.json` |
| Refresh-токены Microsoft | Диспетчер учётных данных Windows (`ElysiumLauncher` / `msa:<uuid>`) |
| Токены доступа Minecraft | только в памяти, в логах консоли скрываются |
| Java | `%APPDATA%\ElysiumLauncher\runtime\temurin-25` |
| Игра | `%APPDATA%\.elysium` (меняется в настройках) |
