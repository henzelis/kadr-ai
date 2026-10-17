# Kadr

Настільний застосунок для безпечного впорядкування локальних фото й відео
за схемою `Рік / Місяць / Місце`. Усе працює локально: файли й координати
нікуди не надсилаються, мережа вимкнена.

> **Стан: етап A (каркас).** Галерея показує демонстраційні записи; реальне
> сканування, метадані, план і файлові операції з'являться на наступних етапах.

## Стек

- **Rust** — уся логіка. `crates/kadr-core` — домен, налаштування, індекс SQLite;
  не залежить від Tauri й UI.
- **Tauri 2** (`src-tauri`) — вікно та типізовані команди; фронтенд не має
  прямого доступу до файлової системи.
- **SvelteKit (SPA) + Svelte 5 + TypeScript** — інтерфейс.
- **SQLite** (rusqlite, вбудована збірка) — індекс, налаштування, журнал.
- **ExifTool / FFprobe** — зовнішні інструменти для метаданих (з етапу B).

## Розробка

Потрібні Rust stable, Node 22+, pnpm 9.

Linux (Ubuntu 24.04):

```sh
sudo apt install libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev \
  libayatana-appindicator3-dev librsvg2-dev libimage-exiftool-perl ffmpeg
```

Windows: WebView2 (є в Windows 10/11), Visual Studio Build Tools з C++;
ExifTool і FFmpeg — у `PATH` або шлях у налаштуваннях застосунку.

```sh
pnpm install
pnpm tauri dev        # застосунок з гарячим перезавантаженням
pnpm dev              # лише UI в браузері, без Rust (для швидкої верстки)
```

## Перевірки

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm check && pnpm build
```

Ті самі перевірки запускає CI на Linux і Windows.

## Дані

Індекс зберігається в каталозі даних застосунку (`kadr.sqlite3`), ніколи — поруч
з оригіналами. Linux: `~/.local/share/ua.kadr.desktop/`, Windows:
`%APPDATA%\ua.kadr.desktop\`.

## Структура

```text
crates/kadr-core/src/
  domain/          media_item, captured_date, location, sort_template, operation
  persistence/     db.rs, migrations/0001_init.sql
  services/        tools.rs (виявлення ExifTool / FFprobe)
  settings.rs      налаштування з перевіркою
src-tauri/src/
  commands/        settings.rs, library.rs
  app_state.rs, errors.rs, lib.rs
src/
  lib/             api.ts, types.ts, format.ts, demo.ts, i18n/uk.ts
  lib/components/  Titlebar, Sidebar, Gallery, Inspector, PlanPreview, SettingsDialog
  routes/          +page.svelte
```
