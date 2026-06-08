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

The internal `rchess` UCI backend now advertises these options:

```text
option name deterministic_multithread type check default false
option name max_threads type spin default 1 min 1 max 64
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
