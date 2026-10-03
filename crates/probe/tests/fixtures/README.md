# TOOL-04 synthetic Papyrus trace

Source tool: Python 3.11 or later, standard-library json and tomllib.
Source revision: the TOOL-04 generator in this directory.
License: GPL-3.0-or-later, the workspace license.
Generation command from the repository root:

```text
rtk proxy python3 crates/probe/tests/fixtures/generate.py
rtk proxy python3 crates/probe/tests/fixtures/generate.py --check
```

The fixture records the authored quest_wait model with small scheduler delays,
the synthetic load-order slots and no menus. The model starts at stage zero,
sets stages 10 and 20, waits twice and reads stage 20. Tolerances precede test
execution in `probes/quest_wait.toml`. Version 1.6.1170.0 is synthetic context.
No installed game, asset, save or live log is an input to this generator.
Tests consume the stored trace without executing the generator or computing
its expected results through soe-game.
