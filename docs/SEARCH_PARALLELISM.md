# Deterministic multithreaded search

This document describes the first real CPU-parallel search layer in `rchess`.

The project still treats the single-threaded search as the baseline. Multithreading is opt-in and deliberately conservative: it should make the engine faster on root positions without turning the search into an opaque subsystem.

## Current design

The current implementation uses deterministic root splitting.

1. The root legal moves are generated and ordered once.
2. The ordered root list is split into fixed-size chunks controlled by `granularity`.
3. Chunks are assigned to worker threads in a fixed round-robin order.
4. Every worker searches its assigned root moves with the same deterministic negamax/alpha-beta code.
5. Results are collected by the original root move index.
6. The best move is selected by score; ties are resolved by the original ordered move index.

This means the selected move is not allowed to depend on operating-system thread scheduling. The shared transposition table may affect speed, but the final root decision is still collected in a fixed order.

## UCI options

The internal `rchess` UCI backend now advertises these options. The exact default for the first two options is detected at startup:

```text
option name deterministic_multithread type check default <true when more than one hardware thread is available>
option name max_threads type spin default <available hardware threads, clamped to 1..64> min 1 max 64
option name granularity type spin default 1 min 1 max 64
option name Hash type spin default 64 min 1 max 4096
```

Example:

```text
setoption name deterministic_multithread value true
setoption name max_threads value 4
setoption name granularity value 2
setoption name Hash value 128
go depth 5
```

`Hash` controls the shared transposition table size in megabytes. The option is named `Hash` for GUI compatibility, but internally it is still part of the transparent deterministic search layer.

## Shared transposition table

The table is shared by all search workers. Entries are atomic and split into an atomic key plus atomic packed data. A read verifies that the key is stable before trusting the entry.

Replacement policy is intentionally simple:

```text
replace if new depth is at least old depth, or if the old entry belongs to an older search age
```

This is the current `replace-by-depth+age` policy. It is not a final high-performance table, but it is easy to inspect and sufficient for the first parallel-search step.

## Limits

The current implementation does not yet include:

- iterative deepening;
- aspiration windows;
- killer/history heuristics;
- time manager;
- NUMA-aware table layout;
- per-thread local history tables;
- memory-optimized packed board representation.

Those are separate topics. This patch only introduces the first deterministic CPU parallelism layer while keeping the original project philosophy: no NNUE, no black-box AI, reproducible behaviour first, strength second.

## Patch: tactical horizon and default CPU use

The low-depth search now runs a cheap mate-in-one probe before falling back to quiescence at the depth frontier. This specifically fixes the worst shallow-analysis failure mode where a non-capture mating move is invisible to the static evaluator. The same probe is also exposed to the GUI evaluation bar, so a live board position with an immediate mate no longer displays as an ordinary material score before full PGN analysis is run.

The internal engine now defaults to deterministic root splitting when the machine reports more than one hardware thread. `max_threads` is initialized from `std::thread::available_parallelism()` and remains clamped by the existing UCI option bounds. The GUI mirrors that default in its resource panel, so game analysis and internal-engine searches use the implemented parallel root layer unless the user turns it off.

The UCI backend now emits `score mate N` for mate-distance scores instead of huge centipawn values. `go movetime N` is still not a real clock manager, but the internal backend maps movetime buckets to a bounded depth so engine-vs-engine matches can use rough per-side power limits without silently ignoring `movetime`.

## Patch: 50-move draw in search leaves

The search and tactical fallback now score positions with a halfmove clock of at least 100 as drawn unless the position is checkmate. This does not give the engine full repetition-aware search yet, because the internal search still stores only the parsed `Position`, not the whole move-history stack. It does prevent the internal evaluator from treating an already claimable 50-move draw as a normal advantage.
