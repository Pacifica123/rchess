# GUI MVP

Этот документ фиксирует первый рабочий GUI-слой проекта.

Цель MVP — не сделать красивую шахматную программу, а получить инструмент, через который можно играть против движка, смотреть FEN, видеть UCI-лог и проверять, что ядро поиска подключается к интерфейсу без смешивания слоёв.

## Архитектура

GUI находится в отдельном бинарнике:

```text
src/bin/rchess-gui.rs
```

Ядро правил и поиск вынесены в библиотечные модули:

```text
src/lib.rs
src/chess.rs
src/search.rs
src/uci.rs
```

CLI-движок остаётся бинарником `rchess`:

```text
src/main.rs
```

GUI использует библиотечный модуль `chess` для отображения позиции и проверки пользовательских ходов. Сам ход движка запрашивается через UCI-процесс.

По умолчанию GUI запускает копию самого себя в режиме:

```bash
rchess-gui --engine-mode
```

В этом режиме окно не открывается. Бинарник просто запускает `rchess::uci::run()` и ведёт себя как обычный UCI-движок. Для GUI это всё равно отдельный процесс, поэтому граница остаётся честной:

```text
GUI process  <->  stdin/stdout  <->  UCI engine process
```

При необходимости можно выбрать другой backend: vendored Stockfish 10 или произвольный внешний UCI executable. GUI всё равно общается с ним через stdin/stdout по UCI.

## Что уже есть

- Отдельный бинарник `rchess-gui`.
- Окно на `egui/eframe`.
- Доска 8x8 с ручной отрисовкой через `egui::Painter`.
- Выбор фигуры кликом.
- Реальный drag-and-drop для хода фигур: фигура скрывается на исходной клетке, следует за курсором и применяется при отпускании над целевой клеткой.
- Подсветка выбранной фигуры.
- Подсветка доступных ходов.
- Применение легальных ходов через `Position::make_legal_move`.
- Автоматический ответ движка.
- Ручная кнопка `Engine move`.
- Настройка глубины поиска 1..8.
- Выбор стороны игрока.
- Переворот доски.
- Ручная загрузка FEN.
- Копирование текущего FEN в поле ввода.
- Список сыгранных ходов в SAN.
- Прокрутка партии без удаления хвоста: кнопки `|<`, `<`, `>`, `>|` и клавиши Left/Right/Home/End.
- `Undo` / `Redo` через пересборку позиции из истории ходов.
- Панель legal moves для просматриваемой позиции: SAN + UCI.
- Стабильная центральная компоновка: доска и шкала оценки больше не вытесняются разросшимися боковыми панелями.
- Секция `Board appearance`: цвета клеток, подсветок, координат и фигур.
- Встроенные пресеты фигур и пользовательский glyph preset из текстового поля или файла.
- Компактный вывод последней строки UCI `info`.
- Окно выбора promotion-фигуры.
- PGN-блок для импорта, экспорта и копирования партии через текстовое поле.
- Открытие и сохранение PGN по указанному пути.
- UCI-лог.
- Возможность перезапуска дочернего UCI-процесса.
- Выбор backend-движка: встроенный rchess UCI, Stockfish 10 или произвольный внешний UCI executable.

## Что сознательно не входит в MVP

- Красивые координаты вокруг доски.
- Нативный файловый диалог для PGN. Сейчас используется ручное поле пути.
- Контроль времени.
- Анализ нескольких линий.
- Полная таблица анализа principal variation.
- Полноценный engine-grade виджет оценки с PV, depth и несколькими линиями.
- Настройки характера игры.
- Asset-based SVG/PNG темы фигур. Сейчас пользовательский пресет — это glyph-набор.

Эти вещи лучше добавлять после того, как минимальная связь `GUI <-> UCI <-> engine` будет стабильной.

## Promotion

Если пользователь доводит пешку до последней горизонтали, GUI открывает небольшое окно выбора фигуры:

```text
Queen / Rook / Bishop / Knight
```

Это работает и при ходе кликом, и при drag-and-drop.

## Запуск

```bash
cargo run --bin rchess-gui
```

CLI-движок по-прежнему запускается так:

```bash
cargo run --bin rchess
```

Так как в пакете теперь несколько бинарников, в `Cargo.toml` установлен `default-run = "rchess"`. Поэтому старая команда:

```bash
cargo run --release
```

остаётся запуском UCI-движка, а не GUI.

## Следующие шаги

Ближайшие улучшения GUI:

1. Нативный файловый диалог для PGN вместо ручного пути.
2. Нативный файловый диалог для PGN на базе отдельной зависимости или платформенного слоя.
3. Кнопка `perft` / `divide` для текущей позиции.
4. Полный разбор UCI `info`: depth, score, nodes, nps, pv.
5. Передача будущих настроек характера движку через UCI options.

## GUI polish patch: первые исправления после MVP

После первого запуска MVP были замечены несколько практических проблем интерфейса:

- фигуры визуально путались по цвету, потому что цвет текста зависел от цвета клетки;
- две вертикальные `ScrollArea` в правой панели получали одинаковый auto-id, из-за чего `egui` показывал красные отладочные предупреждения поверх интерфейса;
- список ходов занимал лишнее место, потому что белые и чёрные ходы выводились отдельными строками;
- при смене стороны игрока автоответ движка не запускался сам, если очередь хода уже была за движком.

Исправление переводит шахматную доску с набора кнопок на ручную отрисовку через `egui::Painter`. Это даёт нормальный контроль над цветом фигур, подсветкой выбранной клетки, подсветкой легальных ходов, координатами и drag-and-drop.

Правая панель получила явные идентификаторы для scroll-area:

```text
moves_scroll
uci_log_scroll
```

Это важно: в `egui` несколько однотипных scroll-area рядом лучше всегда именовать явно, иначе debug build может показывать предупреждение о повторном использовании widget id.


## PGN в GUI

После первого набора исправлений GUI получил текстовый блок `PGN`.

`Export PGN` экспортирует текущую историю ходов из GUI в SAN. Если партия началась не из стартовой позиции, в PGN добавляются `SetUp` и `FEN`.

`Load PGN` читает текст из поля, передаёт его в `src/pgn.rs`, проверяет каждый ход через ядро правил и ставит доску в финальную позицию партии.

Файловые операции пока сделаны через ручное поле пути. Нативный файловый диалог можно добавить позже отдельной зависимостью или платформенным слоем.

## GUI increment: PGN и управление ходом

Следующий слой GUI добавил `Copy PGN`, ручное поле пути для `Open PGN file` / `Save PGN file`, список ходов в SAN и окно выбора promotion-фигуры. Drag-and-drop затем был доработан отдельно: теперь это не только обработка release-события, но и видимое перетаскивание фигуры с подсветкой клетки под курсором.


## Engine backends

GUI теперь не привязан только к нашему поиску. В правой панели есть `Engine backend`:

```text
rchess internal UCI
Stockfish 10
Custom UCI executable
```

`rchess internal UCI` запускает текущий GUI-бинарник с `--engine-mode`, как и раньше.

`Stockfish 10` ожидает исполняемый файл, собранный из исходников в `third_party/stockfish-sf_10`. GUI умеет искать стандартные пути и подставлять ожидаемый `third_party` path, но не собирает C++-код сам.

`Custom UCI executable` оставлен для любого другого движка. Это первый практический шаг к будущему режиму `engine vs engine`, где будет два независимых UCI-процесса и отдельный контроллер партии.

## Engine-vs-engine foundation

В библиотеке появился модуль `src/matchplay.rs`. Он готовит будущий режим матча движков: два UCI-слота, контроллер партии, PGN-лог и лимиты поиска по глубине или времени. GUI теперь запускает два реальных UCI-процесса из панели `Engine vs engine`. Пустой путь означает встроенный `rchess --engine-mode`, а непустой путь трактуется как внешний UCI executable. `src/matchplay.rs` остаётся владельцем позиции, истории ходов, лимитов поиска и PGN-лога матча.


## Undo/redo и engine info

`Undo` удаляет последний ход из истории и пересобирает позицию из `game_start_fen` плюс оставшиеся ходы. Удалённые ходы складываются в redo-стек. Любой новый пользовательский или движковый ход очищает redo-стек. Во время одиночного поиска или engine-vs-engine матча undo/redo отключены.

Панель `Legal moves` показывает текущие легальные ходы в формате `SAN UCI`. Панель `Engine info` берёт последнюю строку `info ...` от UCI-движка и сжимает её до depth/score/nodes/nps/time/pv, чтобы не читать весь сырой лог.

## Engine vs engine в GUI

В правой панели есть блок `Engine vs engine`. В нём два UCI-слота: White и Black. Если поле пути пустое, слот запускает встроенный rchess UCI через текущий бинарник с `--engine-mode`. Если путь задан, GUI запускает внешний UCI executable.

Матч использует `src/matchplay.rs`: контроллер выдаёт `position fen ...`, выбирает `go depth N` или `go movetime N`, принимает `bestmove`, применяет ход через ядро правил и обновляет PGN-лог. Сейчас есть ограничение `Max plies`, чтобы случайная длинная партия не висела бесконечно.

## Layout pass: menu + left panel

The GUI now splits controls into three zones and keeps the board as the central zone:

- top menu: `File`, `Game`, `View`, `Engine`, `Match`, `Analysis`;
- left panel: common board controls, FEN and legal moves;
- right workspace: PGN text, board appearance, backend setup, engine-vs-engine, game analysis, move history and logs.

This is not a final UX. It is a cleanup pass that prevents the right panel from becoming a dump of unrelated controls.

## Game analysis MVP

The right workspace contains a `Game analysis` section. It can analyse the PGN text field, or the current game if the PGN field is empty. It starts the selected UCI backend as a separate process and evaluates positions before and after every played move.

The first metrics are deliberately simple: centipawn loss and a deterministic accuracy value per move, then average accuracy for both sides.

## History navigation and evaluation bar

The board now has a separate history-view cursor. The actual game remains stored in `played_moves`, while `history_view_ply` decides which ply is drawn.

Controls:

```text
|<  start position
<   previous ply
>   next ply
>|  live/final ply
Left / Right / Home / End keyboard shortcuts
```

When the view is not live, the board is read-only. This prevents accidental moves from a historical position.

The evaluation bar is drawn next to the board. It uses, in order:

1. analysed score for the currently displayed ply, if game analysis has produced one;
2. the latest live UCI score, if it belongs to the live position;
3. the deterministic static evaluation from `src/search.rs`.

Positive values are shown from White's perspective. This makes analysis navigation, finished games and engine-vs-engine games easier to inspect without adding a full analysis dashboard yet.

## Search resource settings

The `Engine backend` section now contains active settings for the internal `rchess` backend:

```text
deterministic_multithread
max_threads
granularity
Hash MB
```

For internal `rchess`, the GUI sends these as UCI `setoption` commands before search. `deterministic_multithread` enables fixed-order root-splitting. `max_threads` limits worker count. `granularity` controls how many root moves go into one work chunk. `Hash MB` controls the shared atomic transposition table size.

External UCI engines are intentionally not configured by these project-specific options yet. That keeps the first implementation focused on our own engine and avoids pretending that all UCI engines expose the same knobs.


## Board appearance

The right workspace now has a `Board appearance` section. It can change square colors, legal-move markers, check/selected/drag colors, coordinate colors, piece colors, piece shadow and piece scale.

Piece rendering is still glyph-based. Built-in presets are `Standard Unicode`, `Filled Unicode` and `Letter pieces`. A custom preset can be loaded from text or from a file. The format is deliberately simple and deterministic:

```text
WK WQ WR WB WN WP BK BQ BR BB BN BP
```

This is not yet an SVG/PNG asset pipeline. That can be added later without touching the chess core.

## Layout invariant

GUI уже содержит много инструментов: PGN, анализ, engine backends, engine-vs-engine, UCI logs и appearance-настройки. Из-за этого доска должна считаться главным закреплённым виджетом, а не обычным элементом потока. Текущий layout выделяет под неё отдельную центральную колонку; боковые панели обязаны скроллиться внутри себя и не должны сжимать доску до исчезновения.

Шкала оценки является частью центральной колонки и должна оставаться рядом с доской при следующих изменениях интерфейса.

## Terminal eval and move animation

The evaluation bar now handles terminal positions before using ordinary analysis or static scores. Checkmate is shown as a decisive score for the winning side; stalemate is shown as equal. This prevents stale UCI output or static material evaluation from showing an advantage for the wrong side after the game is already over.

The board also has an optional last-move animation. It does not delay or modify move application in the core. The legal move is applied first, then the GUI temporarily hides the destination piece and draws it moving from the source square to the destination square. The animation is cancelled when the user browses history, starts a new game, loads FEN or loads PGN.
