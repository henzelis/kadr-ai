# Kadr

Настільний застосунок для безпечного впорядкування локальних фото й відео
за схемою `Рік / Місяць / Місце`. Усе працює локально: файли й координати
нікуди не надсилаються, мережа вимкнена.

> **Стан: етап B (індекс і метадані).** Kadr сканує вибрані папки, читає дати й
> GPS (ExifTool, FFprobe або вбудований EXIF-читач), визначає місто чи область за
> офлайн-геобазою і показує файли, що потребують перевірки. Мініатюри — етап C,
> план сортування — етап D, копіювання/переміщення — етап E.

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

## Офлайн-геобаза

Назви міст і областей визначаються локально, без мережі, за файлом
`geobase.sqlite3`, зібраним з даних [GeoNames](https://www.geonames.org/)
(ліцензія CC BY 4.0). Встановлення: **Налаштування → Офлайн-геобаза → Вибрати
файл геобази…**

Зібрати файл самостійно:

```sh
mkdir geonames && cd geonames
for f in cities1000.zip alternateNamesV2.zip admin1CodesASCII.txt countryInfo.txt; do
  curl -fLO https://download.geonames.org/export/dump/$f
done
unzip cities1000.zip && unzip alternateNamesV2.zip
cd .. && cargo run --release -p kadr-geobase -- geonames geobase.sqlite3 geonames-$(date +%F)
```

Результат — близько 8,5 МБ, ~160 тис. населених пунктів з населенням від 1000,
українські назви там, де вони є в GeoNames.

## Дані

Індекс зберігається в каталозі даних застосунку (`kadr.sqlite3`), ніколи — поруч
з оригіналами. Linux: `~/.local/share/ua.kadr.desktop/`, Windows:
`%APPDATA%\ua.kadr.desktop\`.

## Структура

```text
crates/kadr-core/src/
  domain/          media_item, captured_date, location, sort_template, operation
  persistence/     db.rs, migrations/0001_init.sql
  services/        scanner, detect, metadata/ (exiftool, ffprobe, native_image,
                   normalize), place, geocoder, indexer, process, tools
  settings.rs      налаштування з перевіркою
src-tauri/src/
  commands/        settings, library, sources, scan, geobase
  app_state.rs, errors.rs, lib.rs
crates/kadr-geobase/   збирач геобази з дампів GeoNames
src/
  lib/             api.ts, types.ts, format.ts, demo.ts, i18n/uk.ts
  lib/components/  Titlebar, Sidebar, Gallery, Inspector, PlanPreview, SettingsDialog
  routes/          +page.svelte
```

## Ліцензія

MIT — див. [LICENSE](LICENSE). ExifTool і FFmpeg — окремі програми зі своїми ліцензіями; умови їх постачання разом із Kadr перевіряються перед релізом.
