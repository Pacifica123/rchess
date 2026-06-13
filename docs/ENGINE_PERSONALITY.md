# Engine personality controls

The GUI now has `Workspace / Engine personality` for the internal `rchess` backend.

The controls are stored as GUI values from `-1.00` to `1.00` and are sent to the internal UCI child as spin options from `-100` to `100`:

- `RiskLevel`: negative values make root move selection more cautious; positive values give small bonuses to checks, king attacks and speculative sacrifices that create pressure.
- `HumanityLevel`: positive values can deterministically choose occasional inaccurate human-style alternatives within a bounded centipawn loss window. Negative values are currently treated as engine-like neutral behaviour.

The default is neutral: `RiskLevel=0`, `HumanityLevel=0`. Analysis mode sends neutral personality values so accuracy reports stay comparable.
