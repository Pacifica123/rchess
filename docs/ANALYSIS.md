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
- chart view;
- saving analysis metadata into PGN comments.

Those should be added as separate layers after the UI and UCI pipeline stop changing.

## Board integration

Analysis is no longer only a table in the right panel. When a PGN is analysed, the GUI also keeps the analysed game as the current game history. Clicking a row in the analysis table moves the board's history-view cursor to that ply. The board can then be stepped with Left/Right without changing the actual move list.

For each displayed ply, the evaluation bar first tries to use the corresponding analysis score. If the score is unavailable, the GUI falls back to the current deterministic static evaluation from the internal engine code. Scores are converted to White perspective before drawing the bar.
