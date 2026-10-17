# Сторонні компоненти Kadr

Kadr поширюється за ліцензією MIT (див. `LICENSE`). Інсталятори для Windows,
macOS і Linux містять окремі сторонні програми й дані. Kadr запускає ці програми як
окремі процеси і не змінює їх.

## ExifTool 13.59

© Phil Harvey. https://exiftool.org
Ліцензія: умови Perl — Artistic License або GNU GPL (на вибір).
Збірка для Windows містить Strawberry Perl; ліцензії її компонентів — у
`helpers/exiftool/exiftool_files/LICENSE` та `Licenses_Strawberry_Perl.zip`.
Оригінальний архів: https://sourceforge.net/projects/exiftool/files/exiftool-13.59_64.zip
На macOS і Linux використано Perl-дистрибутив Image-ExifTool-13.59, що працює
з системним Perl: https://sourceforge.net/projects/exiftool/files/Image-ExifTool-13.59.tar.gz

## FFmpeg 8.1.3

https://ffmpeg.org — ліцензія GNU LGPL 2.1 або новіша (текст — у
`helpers/ffmpeg/LICENSE.txt`). Використано збірку LGPL (shared) від BtbN
(https://github.com/BtbN/FFmpeg-Builds) від 2026-10-16.
Вихідний код: https://github.com/FFmpeg/FFmpeg/tree/n8.1.3; скрипти збірки
й конфігурація: https://github.com/BtbN/FFmpeg-Builds. На Windows бібліотеки
FFmpeg постачаються як окремі DLL і можуть бути замінені користувачем.

На macOS і Linux FFmpeg 8.1.3 зібрано з вихідного коду
(https://ffmpeg.org/releases/ffmpeg-8.1.3.tar.xz) скриптом
`scripts/build-ffmpeg.sh` з репозиторію Kadr, лише з LGPL-компонентами, разом
із dav1d 1.5.4 (BSD-2-Clause, https://code.videolan.org/videolan/dav1d).
`ffmpeg` і `ffprobe` — окремі програми, які можна замінити власною збіркою:
вкажіть шлях у Налаштуваннях Kadr → Додатково.

## Офлайн-геобаза

Дані © GeoNames, https://www.geonames.org, ліцензія CC BY 4.0
(https://creativecommons.org/licenses/by/4.0/). Змінено: відібрано населені
пункти з населенням від 1000, вибрано українські назви, нормалізовано регістр.
