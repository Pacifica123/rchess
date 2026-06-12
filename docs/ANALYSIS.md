# Game analysis

This document describes the first version of the game-analysis layer in `rchess-gui`.

The current implementation analyses a loaded or pasted PGN by asking a UCI engine to evaluate positions around each played move. For every ply the GUI evaluates two FEN positions:

1. the position before the move;
2. the position after the move.

UCI scores are treated as centipawn scores from the side-to-move perspective. The score after the move is therefore inverted before it is compared with the score before the move. The first accuracy estimate is intentionally simple:

```text
loss_cp = max(0, before_score - after_score_from_mover_view)
accuracy = clamp(100 - loss_cp / 10, 0, 100)
```

This is not meant to be a Chess.com/Lichess clone. It is a deterministic first pass that makes the analysis panel useful and keeps the math easy to inspect.

## Current GUI flow

The analysis panel lives in the right workspace panel under `Game analysis`. The top menu also has an `Analysis` menu.

Current actions:

- start analysis for the PGN text if the PGN field is not empty;
- otherwise analyse the current game history;
- run the selected engine backend as a separate UCI process;
- show progress by analysed FEN count;
- show per-move SAN, score before, score after, centipawn loss and simple accuracy;
- show average accuracy for White and Black;
- show an evaluation-dynamics chart below the summary;
- let the chart and move rows drive the board history cursor by click;
- copy a plain-text analysis report;
- click an analysed ply to show that position on the board;
- reuse analysed scores for the board evaluation bar.

At this stage the selected backend can be the internal `rchess` UCI engine, Stockfish 10, or a custom UCI executable. The internal engine now emits `info ... score cp ...`, so it can be used for the first analysis flow without Stockfish.

## Limits

The current analysis is shallow and deterministic. It does not yet include:

- best-line comparison against the engine's preferred move;
- move classification such as blunder/mistake/inaccuracy;
- multi-PV;
- time-based analysis queue;
- cached analysis;
- saving analysis metadata into PGN comments.

Those should be added as separate layers after the UI and UCI pipeline stop changing.


## Patch: analysis source flow and evaluation chart

The analysis panel now contains the full user flow directly inside `Game analysis`:

1. paste a PGN into the analysis source box, or open it from a file path;
2. optionally load that PGN into the main board/history view;
3. press `Start analysis`;
4. read the summary accuracy for White and Black;
5. inspect the evaluation-dynamics chart under the summary.

The chart is a deterministic line plot of the analysed evaluation after each ply, always converted to White perspective. Positive values mean White is better, negative values mean Black is better. The currently displayed board ply is highlighted in the chart, and clicking a point on the chart moves the board to that ply.

Mate scores are intentionally clamped for chart drawing so that one forced mate does not flatten the rest of the game into a near-horizontal line.

## Board integration

Analysis is no longer only a table in the right panel. When a PGN is analysed, the GUI also keeps the analysed game as the current game history. Clicking a row in the analysis table moves the board's history-view cursor to that ply. The board can then be stepped with Left/Right without changing the actual move list.

For each displayed ply, the evaluation bar first tries to use the corresponding analysis score. If the score is unavailable, the GUI falls back to the current deterministic static evaluation from the internal engine code. Scores are converted to White perspective before drawing the bar.

## Terminal positions

The GUI now treats checkmate and stalemate as terminal evaluation cases before reading ordinary centipawn scores. This matters for live display and for PGN analysis: a completed mating position must not be shown as a small material or positional advantage for the side that is already winning or losing.

For the board evaluation bar, terminal score priority is:

```text
checkmate/stalemate result
analysis score for the displayed ply
live UCI score
static deterministic evaluation
```

When a UCI backend returns `bestmove 0000` without a score for a terminal analysis job, the GUI falls back to a deterministic terminal/static score for that FEN. The internal rchess UCI backend also emits a `score mate -1` line when the side to move has already been checkmated.

## Patch: mate scores and shallow tactical display

The GUI now parses UCI `score mate N` lines and converts them into a mate-class evaluation instead of dropping the score and falling back to static material. The board bar therefore keeps mate information from Stockfish and from the internal engine.

When no analysis score is available, the board bar no longer uses a purely static material/positional number. It first checks terminal positions and immediate mate-in-one for the side to move. This is intentionally small and deterministic, but it removes the most visible case where the scale looked blind one move before mate.

## Patch: draw-aware live status

The GUI live result and evaluation bar now treat rule-based draws as terminal for the current game history. A position with a threefold repetition or a halfmove clock of at least 100 is shown as equal and the game status reports the draw reason.

This matters for long engine games: a repeated checking cycle should not stay as `*` until `Max plies`; it should become `1/2-1/2` when the third occurrence is reached.

## Analysis as experience annotation

Отчёт анализа теперь можно использовать как источник подробных полей для книги опыта. При экспорте engine-vs-engine матча в experience book GUI берёт `before_score_cp`, `after_score_cp`, `loss_cp` и `accuracy` из текущего анализа, если он есть. Поэтому рекомендуемый порядок для качественной записи опыта такой:

1. Сыграть engine-vs-engine матч.
2. Запустить `Game analysis` на этой партии.
3. Нажать `Append match to experience book`.

Без анализа книга всё равно пополняется, но оценки будут fallback-оценками текущего ядра.


## Opening and trap hygiene

Поиск на малой глубине всё ещё может не видеть длинный мат после материального выигрыша. Поэтому статическая оценка получила простые шахматные штрафы, не связанные с ИИ: ранний вывод ферзя при недоразвитии, перегулявшие лёгкие фигуры в дебюте и король, оставшийся в центре с нарушенными центральными пешками. Эти штрафы не заменяют поиск, но уменьшают склонность брать дальнюю ладью ферзём или танцевать одной фигурой, когда позиция требует развития и безопасности короля.

## Patch: SEE, king danger and root-level trap checks

Текущая оценка получила ещё один слой осторожности против типичной ошибки `выиграл материал, но попал под матовую сетку`.

Новые компоненты остаются обычными правилами и не являются ИИ:

```text
SEE-lite              статическая проверка размена на поле взятия;
king danger map       счёт атак по кольцу вокруг короля и отсутствующего пешечного щита;
checking replies      штраф за ход, после которого соперник получает серию шахующих ответов;
root mate threat      проверка на мат в 1 и ограниченную проверку угрозы мата в 2 после root-хода;
greedy queen guard    дополнительный штраф для жадных ходов ферзём за ладьёй/материалом при опасном короле.
```

`SEE-lite` намеренно не является полной таблицей всех разменов. Это дешёвый предохранитель: если после взятия фигура оказывается под очевидным обратным взятием более дешёвой фигурой, ход получает штраф и хуже сортируется. Полный SEE можно добавить позже отдельным патчем.

Root-level проверка мата в 2 также ограничена: она запускается только как защитный слой для уже подозрительных позиций, например при высоком king-danger или жадном ходе ферзём. Это снижает риск зависаний на глубине 5 и выше.
