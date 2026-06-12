# Engine vs engine foundation

Этот документ фиксирует первый технический слой для будущего режима `engine vs engine`.

Изначально это был только библиотечный слой. Теперь к нему подключена GUI-панель, которая запускает два реальных UCI child process и использует `EngineMatchController` как владельца партии.

## Новый модуль

Добавлен модуль:

```text
src/matchplay.rs
```

В нём есть три базовые сущности.

`UciEngineSlot` описывает один слот движка: имя, путь/команду запуска, аргументы и лимит поиска.

`SearchLimit` описывает ограничение на ход. Сейчас поддержаны два режима:

```text
go depth N
go movetime N
```

`EngineMatchController` владеет партией: стартовым FEN, текущей позицией, списком ходов, белым слотом, чёрным слотом, текущим статусом и результатом.

## Что уже умеет контроллер

Контроллер умеет:

- определить, чей сейчас слот должен ходить;
- выдать UCI-команду `position fen ...` для текущей позиции;
- выдать UCI-команду `go depth ...` или `go movetime ...` по лимиту активного слота;
- принять `bestmove ...` или чистый UCI-ход;
- проверить ход через `Position::parse_uci_move` и `Position::make_legal_move`;
- обновить список ходов;
- определить мат/пат и результат;
- экспортировать текущую партию в PGN с тегами `White`, `Black`, `Result`, а при нестандартном старте — `SetUp` и `FEN`.

## GUI-раннер

GUI теперь имеет панель `Engine vs engine`:

```text
White path
Black path
Depth
Movetime ms
Max plies
Start match / Stop match / Copy match PGN
```

Пустой путь означает встроенный `rchess --engine-mode`. Непустой путь означает внешний UCI executable. Так уже можно запускать партии вида:

```text
rchess vs rchess
rchess vs Stockfish 10
Stockfish 10 vs custom UCI
```

Текущий раннер пока минимален: он отправляет `uci`, `isready`, затем для активного цвета `position fen ...` и `go depth N` или `go movetime N`, ждёт `bestmove`, применяет ход через контроллер и запускает следующий ход. Есть ограничение `Max plies`.

## Чего ещё нет

Пока не добавлены:

- настоящие шахматные часы с остатком времени;
- `stop`/timeout на зависшем движке;
- adjudication по материалу/оценке;
- турнирный формат из нескольких партий;
- сохранение настроек матчей;
- отдельная таблица результата.

Главное ограничение остаётся тем же: GUI и матч-контроллер не должны знать, как именно движок ищет ход. Они знают только UCI.

## Shared direction with analysis

The match runner and the analysis runner both use independent UCI child processes. This keeps the GUI prepared for future workflows where a game can be played by two engines and then analysed by a third selected engine.

## Patch: asymmetric engine power

The GUI match panel now has separate limits for White and Black. Each side can use its own depth and movetime; movetime greater than zero overrides depth for that side. This makes handicap testing less blind: for example, `rchess` can stay at depth 3 while Stockfish is forced to depth 1 or a very small movetime.

The panel also accepts per-side UCI option lines. Both raw UCI lines and compact `Name=value` lines are accepted, for example:

```text
Skill Level=0
Threads=1
Hash=16
```

or:

```text
setoption name Skill Level value 0
```

These options are sent to the corresponding child process at match startup. Internal `rchess` children still receive the project resource options (`deterministic_multithread`, `max_threads`, `granularity`, `Hash`) automatically.

## Patch: draw adjudication and history-aware UCI positions

Engine matches now adjudicate the two draw rules that were missing from long games:

- threefold repetition, counted from the start FEN and the full played move list;
- the 50-move rule, using the FEN halfmove clock and stopping when it reaches 100 halfmoves.

Checkmate still has priority over draw adjudication. Stalemate, threefold repetition and the 50-move rule produce `1/2-1/2`; match PGN also gets a `Termination` tag such as `threefold repetition` or `50-move rule`.

The match runner no longer sends only the current FEN to UCI engines. It now sends the start FEN plus the full move list:

```text
position fen <start-fen> moves <uci-move> <uci-move> ...
```

That keeps repetition history visible to external engines such as Stockfish, instead of making every move look like an isolated FEN position.
