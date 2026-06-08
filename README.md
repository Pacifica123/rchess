# rchess

`rchess` — минимальный шахматный движок на Rust. Текущий этап заменяет старый прототип с демонстрационным кодом и случайным выбором хода на рабочее ядро: FEN, генератор легальных ходов, применение ходов, perft-проверки, простая оценка позиции, alpha-beta поиск, детерминированный root-splitting multithread режим, UCI-интерфейс, MVP GUI, начальную работу с PGN/SAN и первый слой анализа партий, навигацию по истории партии и визуальную шкалу оценки позиции.

## Что уже есть

- Доска 8x8 с внутренней индексацией `a1 = 0`.
- Разбор и вывод FEN.
- Генерация легальных ходов с фильтрацией шаха своему королю.
- Пешечные превращения, рокировка, взятие на проходе.
- Определение шаха, мата и пата.
- `perft` и `perft divide` для проверки генератора ходов.
- Набор тестов на стартовую позицию, известные `perft`/`perft divide`-позиции до выбранных depth 3, рокировку, en passant, underpromotion, pinned pieces, discovered/double check, halfmove clock, мат и пат.
- Поиск лучшего хода: negamax + alpha-beta + quiescence на взятиях.
- Опциональный детерминированный многопоточный режим: root-splitting, фиксированный порядок root-ходов, общая атомарная transposition table и replace-by-depth+age.
- Простая статическая оценка: материал, центр, развитие пешек, пара слонов.
- UCI-протокол для подключения к GUI вроде Cute Chess, Arena, Banksia и другим.
- Rust GUI MVP на `egui/eframe`, включая левую панель управления, верхнее меню, drag-and-drop, undo/redo, прокрутку партии стрелками, SAN-историю, legal moves panel, визуальную шкалу оценки и вывод UCI `info`.
- Выбор backend-движка в GUI: наш UCI, vendored Stockfish 10 или любой внешний UCI executable.
- Базовый PGN/SAN: импорт, экспорт, FEN-tag, GUI-блок, CLI-проверка и тесты неоднозначной SAN-нотации.
- Первый анализ PGN/истории партии через UCI: оценка до/после каждого хода, centipawn loss, простая accuracy сторон, текстовый отчёт и переход к позиции по строке анализа.

Ядро правил, поиск, UCI и PGN остаются без внешних зависимостей. GUI использует `egui/eframe` как отдельный интерфейсный слой.

## Команды

Запуск как UCI-движка:

```bash
cargo run --release
```

Проверка генератора ходов:

```bash
cargo run --release -- perft 3
```

Разбивка `perft` по первым ходам:

```bash
cargo run --release -- divide 2
```

Ожидаемые значения из стартовой позиции:

```text
depth 1 = 20
depth 2 = 400
depth 3 = 8902
```

Получить лучший ход из стартовой позиции:

```bash
cargo run --release -- bestmove 4
```

Проверить разбор FEN:

```bash
cargo run --release -- fen "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"
```

Проверить PGN-файл:

```bash
cargo run --bin rchess -- pgn game.pgn
```

Или через stdin:

```bash
cat game.pgn | cargo run --bin rchess -- pgn
```

Тесты:

```bash
cargo test
```

## UCI-минимум

Поддерживаются команды:

- `uci`
- `isready`
- `ucinewgame`
- `setoption name Depth value N`
- `setoption name deterministic_multithread value true|false`
- `setoption name max_threads value N`
- `setoption name granularity value N`
- `setoption name Hash value MB`
- `position startpos [moves ...]`
- `position fen <fen> [moves ...]`
- `go depth N`
- `perft N`
- `d`
- `quit`

## PGN/SAN

В проект добавлен модуль:

```text
src/pgn.rs
```

Он умеет читать обычный SAN, экспортировать SAN, принимать `FEN` tag для партий не из стартовой позиции и проверять каждый ход через легальные ходы `Position`.

В GUI есть блок `PGN`:

- `Export PGN` — собрать PGN из текущей партии;
- `Copy PGN` — скопировать PGN в буфер обмена;
- `Load PGN` — загрузить PGN из поля, проверить ходы и перейти в финальную позицию;
- `Open PGN file` / `Save PGN file` — открыть или сохранить PGN по указанному пути;
- `Clear PGN` — очистить поле.

Подробности: [`docs/PGN.md`](docs/PGN.md).

## Что вычищено

Старые модули с частично завершённой логикой удалены из сборки. В них были `unimplemented!()`, `todo!()`, протекающие строки через `Box::leak`, случайный выбор хода вместо поиска и тестовый `main.rs`. Новый код собран вокруг одного простого ядра и одного интерфейса к нему.

## Проектные документы

- [`docs/PROJECT_PHILOSOPHY.md`](docs/PROJECT_PHILOSOPHY.md) — зачем существует движок, какие решения считаются принципиальными, а какие нет.
- [`docs/GUI_EGUI_PLAN.md`](docs/GUI_EGUI_PLAN.md) — направление для Rust GUI на `egui/eframe` без смешивания интерфейса и ядра.
- [`docs/GUI_MVP.md`](docs/GUI_MVP.md) — текущее состояние GUI MVP.
- [`docs/PGN.md`](docs/PGN.md) — текущий PGN/SAN-слой.
- [`docs/ENGINE_BACKENDS.md`](docs/ENGINE_BACKENDS.md) — подключение внешних UCI-движков, Stockfish 10 и будущий engine-vs-engine режим.
- [`docs/ENGINE_MATCH.md`](docs/ENGINE_MATCH.md) — задел под матч двух UCI-движков.
- [`docs/ANALYSIS.md`](docs/ANALYSIS.md) — первый слой анализа партии через UCI-движок.
- [`docs/SEARCH_PARALLELISM.md`](docs/SEARCH_PARALLELISM.md) — детерминированный многопоточный root-splitting и shared TT.
- [`docs/CORE_TESTING.md`](docs/CORE_TESTING.md) — что сейчас проверяется в ядре правил.

## Дальше

Ближайший разумный порядок разработки:

1. Продолжать добивать надёжность ядра: больше depth 3/4 `perft divide`-позиций, невозможность взятия короля, дополнительные PGN cases и будущие правила результата партии.
2. Улучшить силу: killer/history move ordering, iterative deepening, нормальный time control, более плотная TT и более умное распределение root/subtree работы.
3. Улучшить GUI: нативный файловый диалог вместо ручного пути, perft/divide-панель, более плотная таблица анализа, сохранение анализа в PGN-комментарии и нормальная панель настроек движка.
4. Развивать engine-vs-engine: часы, остановка, adjudication, повторные партии и сохранение матчей.
5. Начать выносить методики поиска в явные модули, чтобы потом можно было сравнивать и комбинировать стратегии.

## Rust GUI MVP

В проект добавлен отдельный GUI-бинарник на `egui/eframe`:

```bash
cargo run --bin rchess-gui
```

GUI не встраивает поиск напрямую в интерфейс. По умолчанию он запускает отдельный UCI-процесс из этого же бинарника через служебный режим `--engine-mode` и общается с ним по `stdin/stdout`. Это сохраняет процессную границу:

```text
rchess-gui window  <->  UCI child process  <->  engine core
```

MVP умеет:

- показывать доску;
- выбирать фигуру кликом или полноценным drag-and-drop;
- подсвечивать доступные ходы;
- делать легальные ходы;
- автоматически отвечать ходом движка;
- запускать отдельный ход движка кнопкой `Engine move`;
- выбирать сторону игрока;
- переворачивать доску;
- задавать глубину поиска;
- загружать и копировать FEN;
- импортировать, экспортировать, копировать, открывать и сохранять PGN;
- отменять и повторять ходы через `Undo` / `Redo`;
- показывать список ходов в SAN;
- прокручивать партию кнопками и клавишами Left/Right, Home/End без удаления хвоста партии;
- показывать legal moves текущей просматриваемой позиции в левой панели;
- показывать визуальную шкалу оценки позиции рядом с доской;
- показывать компактный вывод UCI `info`;
- запускать engine-vs-engine матч из двух реальных UCI-процессов;
- показывать UCI-лог;
- анализировать PGN/текущую историю через выбранный UCI backend и считать первичную accuracy сторон;
- управлять internal rchess resource settings: `deterministic_multithread`, `max_threads`, `granularity` и `Hash` в MB.

Основные действия разнесены: верхняя строка стала меню `File / Game / Engine / Match / Analysis`, частые игровые кнопки и legal moves вынесены в левую панель, а справа остался workspace для PGN, backend, match, анализа и логов.

В GUI теперь есть выбор engine backend: `rchess internal UCI`, `Stockfish 10` или `Custom UCI executable`. Для Stockfish 10 в проект добавлен исходный код в `third_party/stockfish-sf_10`, но он не собирается через Cargo. Нужно собрать исполняемый файл отдельно и указать путь в GUI или нажать `Detect Stockfish 10`, если бинарник лежит в ожидаемом месте.

### Первые исправления GUI

После MVP доска в GUI переведена с сетки кнопок на ручную отрисовку через `egui::Painter`. Это исправляет визуальное смешение цвета фигур, убирает предупреждения `egui` о повторном использовании `ScrollArea` id, добавляет координаты на доску и делает список ходов компактнее.


## Engine-vs-engine foundation

В библиотеке есть `src/matchplay.rs`: небольшой контроллер режима `engine vs engine`. Он хранит два UCI-слота, текущую позицию, историю ходов, PGN-лог и лимит поиска активного движка. GUI теперь умеет запускать два реальных UCI child process и использовать этот контроллер как владельца партии.

## History navigation and evaluation bar

GUI now separates the actual game history from the currently displayed ply. The board can be moved through the game with the left-panel controls or keyboard shortcuts `Left`, `Right`, `Home` and `End` when no text field is focused. While an old ply is displayed the board is read-only; returning to the live ply re-enables normal play.

A compact evaluation bar is drawn next to the board. If analysis data exists for the displayed ply, the bar uses that analysed score converted to White perspective. Otherwise it falls back to the internal deterministic static evaluation. This is only a visual guide, not a replacement for full search.

The engine resource controls are now active for the internal `rchess` backend. `deterministic_multithread` enables deterministic root-splitting, `max_threads` limits worker count, `granularity` controls root chunk size, and `Hash` allocates the shared atomic transposition table. External UCI engines are not configured through these project-specific controls yet.
