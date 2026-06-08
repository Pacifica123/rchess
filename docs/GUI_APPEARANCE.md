# GUI appearance

Этот документ фиксирует первый слой визуальной кастомизации GUI.

## Зачем

После добавления меню, анализа и настроек поиска интерфейс стал теснее. В одном из следующих состояний доска фактически потерялась в компоновке. Этот слой делает две вещи:

1. возвращает доску как центральный и устойчивый элемент окна;
2. добавляет настраиваемую визуальную тему доски и фигур без вмешательства в шахматное ядро.

Шкала оценки остаётся рядом с доской. Это отдельный виджет, и его нельзя терять при дальнейших изменениях layout.

## Компоновка

Текущая структура GUI:

```text
верхнее меню
left controls  |  evaluation bar + board  |  right workspace
```

Левая колонка содержит частые игровые действия, навигацию, FEN и legal moves. Центр зарезервирован под доску и шкалу оценки. Правая колонка остаётся workspace для PGN, анализа, backend-настроек, engine-vs-engine, истории и логов.

Это не финальный интерфейс, но теперь доска не зависит от того, насколько разрослись правые панели.

## Настройки доски

В правом workspace добавлена секция `Board appearance`.

Можно менять:

- цвет светлых клеток;
- цвет тёмных клеток;
- цвет выбранной клетки;
- цвет клетки под drag-and-drop;
- цвет клетки короля под шахом;
- цвет маркера тихого легального хода;
- цвет маркера взятия;
- цвета координат;
- включение/выключение координат;
- цвет белых фигур;
- цвет чёрных фигур;
- цвет тени фигур;
- масштаб фигур.

## Пресеты фигур

Есть несколько встроенных наборов:

```text
Standard Unicode
Filled Unicode
Letter pieces
Custom glyph set
```

`Standard Unicode` использует обычные шахматные Unicode-фигуры. `Filled Unicode` использует более плотные символы. `Letter pieces` полезен как запасной режим, если шрифт системы плохо показывает шахматные символы.

## Пользовательский пресет

Пользовательский набор фигур задаётся как 12 разделённых пробелами glyph-значений:

```text
WK WQ WR WB WN WP BK BQ BR BB BN BP
```

Пример:

```text
♔ ♕ ♖ ♗ ♘ ♙
♚ ♛ ♜ ♝ ♞ ♟
```

GUI умеет загрузить такой пресет из текстового поля или из файла по пути. Файловый диалог пока не добавлен; используется ручное поле пути, как и для PGN.

## Ограничения

На этом этапе пользовательский пресет — это glyph preset, а не набор SVG/PNG-спрайтов. Это осознанное ограничение: без новой зависимости и без отдельного asset pipeline. В будущем можно добавить asset-based пресеты, но это уже отдельный слой: загрузка файлов, кэш текстур, масштабирование и fallback при ошибках.

## Layout hardening

После первого appearance-патча выяснилось, что простой `horizontal_top` с `Frame::group` и только `set_min_width`/`set_max_width` недостаточно жёстко ограничивает дочерние элементы. Длинные FEN/PGN/логовые строки и вложенные `ScrollArea` могли растянуть боковые зоны и вытеснить доску из центральной области.

Текущая компоновка использует явные `allocate_ui`-области для трёх колонок:

```text
left controls | eval bar + board | workspace
```

Каждая колонка получает фиксированную ширину в рамках текущего окна, свой clip rect и внутреннюю вертикальную прокрутку. Доска рисуется только внутри центральной области, а FEN под доской вынесен в горизонтальный скролл, чтобы длинная строка не ломала раскладку.

Главное правило для следующих GUI-патчей: новые панели анализа, матчей, логов и настроек не должны увеличивать ширину центральной области и не должны располагаться в одной строке с доской.

## Layout hardening note

The board is now drawn in a fixed center viewport. The side panels may scroll internally, but they must not be allowed to resize the board out of the visible area. The evaluation bar belongs to the same center viewport as the board.

This is intentional: visual customization is allowed to change colors and glyphs, not the basic visibility contract of the board.

## Move animation and terminal evaluation

The board now has a small optional animation layer for the last applied move. It is intentionally simple: after a legal move is already accepted by the core, the GUI draws the moved piece between the source and target squares for a short fixed duration. This keeps the chess state deterministic and avoids making animation part of the rules layer.

The `Board appearance` panel contains:

```text
Animate moves
Move animation ms
```

The animation is a visual overlay only. PGN, FEN, engine search and history navigation still use the already-applied board state. Moving the history cursor cancels the animation.

The evaluation bar now checks terminal positions before using analysis scores, live UCI scores or static evaluation. A checkmated side is displayed as a decisive result for the winner, so a board where Black is checkmated is shown as winning for White even if the last non-terminal search score was stale.
