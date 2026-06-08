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
