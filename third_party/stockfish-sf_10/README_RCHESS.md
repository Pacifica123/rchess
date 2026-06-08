# Stockfish 10 inside rchess

This directory is the uploaded `Stockfish-sf_10` source tree. It is kept as an optional external UCI engine for the Rust GUI.

The Rust project does not compile this code through Cargo. Build Stockfish separately and point the GUI to the resulting executable.

Expected output paths used by the GUI detector:

```text
third_party/stockfish-sf_10/src/stockfish
third_party/stockfish-sf_10/src/stockfish.exe
```

Typical Unix-like build command from this directory's `src` subdirectory:

```bash
make build ARCH=x86-64
```

License: Stockfish is distributed under GNU GPL. See `Copying.txt`.
