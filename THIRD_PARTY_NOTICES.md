# Сторонні компоненти Kadr

Kadr поширюється за ліцензією MIT (див. `LICENSE`). Інсталятор для Windows
містить окремі сторонні програми й дані. Kadr запускає ці програми як
окремі процеси і не змінює їх.

## ExifTool 13.59

© Phil Harvey. https://exiftool.org
Ліцензія: умови Perl — Artistic License або GNU GPL (на вибір).
Збірка для Windows містить Strawberry Perl; ліцензії її компонентів — у
`helpers/exiftool/exiftool_files/LICENSE` та `Licenses_Strawberry_Perl.zip`.
Оригінальний архів: https://sourceforge.net/projects/exiftool/files/exiftool-13.59_64.zip

## FFmpeg 8.1.3

https://ffmpeg.org — ліцензія GNU LGPL 2.1 або новіша (текст — у
`helpers/ffmpeg/LICENSE.txt`). Використано збірку LGPL (shared) від BtbN
(https://github.com/BtbN/FFmpeg-Builds) від 2026-10-16.
Вихідний код: https://github.com/FFmpeg/FFmpeg/tree/n8.1.3; скрипти збірки
й конфігурація: https://github.com/BtbN/FFmpeg-Builds. Бібліотеки FFmpeg
постачаються як окремі DLL і можуть бути замінені користувачем.

## Офлайн-геобаза

Дані © GeoNames, https://www.geonames.org, ліцензія CC BY 4.0
(https://creativecommons.org/licenses/by/4.0/). Змінено: відібрано населені
пункти з населенням від 1000, вибрано українські назви, нормалізовано регістр.
