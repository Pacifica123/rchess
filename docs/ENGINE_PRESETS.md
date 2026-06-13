# Engine presets / constructor

The GUI now has `Workspace / Engine constructor`. A constructed engine is not a separate binary. It is a saved parameter preset for the internal `rchess` UCI child process.

A preset stores:

- display name;
- avatar value, either short text/emoji or a png/jpg path;
- description;
- default search depth;
- internal resource options: `deterministic_multithread`, `max_threads`, `granularity`, `Hash`;
- draw style: `AvoidDraws`;
- personality axes: `RiskLevel` and `HumanityLevel`;
- experience-book options;
- additional raw UCI option lines.

The built-in `Default rchess` preset is always inserted at index 0 and is not written to the custom preset file. Custom presets are stored in a simple line-based text file, defaulting to `rchess-engine-presets.txt` in the current working directory.

## Current GUI behaviour

Selecting a preset in the constructor applies its settings to the normal internal `rchess` backend. If an internal UCI child is already running, the GUI sends the updated options immediately. Analysis mode still sends neutral personality values for comparability.

In `Engine vs engine`, each side has its own preset selector. The preset is used only when that side path is empty, because an empty path means the internal `rchess --engine-mode` child. External UCI engines still use the path and the manual per-side UCI option text.

The avatar image path is stored and displayed as a path marker. Actual png/jpg decoding and texture preview are intentionally left for a later rendering-layer patch.
