# SAV-71 writer reply

Branch: `draft8/SAV-71`. This follow-up leaves changes uncommitted. The
existing implementation is in `b90d795f63c17214ad352b47b9a33e52f6906a25`,
partly merged at `989f3ca0`. The earlier draft commits in this worktree are
`b974b201` and `a62a39b8`. Their code and tests are retained.

The culprit is `f16d16ac` (UI-25). Its change to `world/refs.rs` materializes
LVLI contents during reference construction and Reset. An initial chance-none
draw and a later reset draw can differ before any save is written. The
existing game documentation records the historical checks: both old
installed tests pass at its parent `f9db8db7` and fail at `f16d16ac`. Both
already fail at `3547411b`, the start of the requested bisect range. Those
historical installed runs were not repeated during this light-mode task.

The existing fix keeps the resolver and selects fixed CNTO containers for the
two SAV-68 inventory equality checks. SAV-71 separately checks loaded resets
of fixed and leveled inventories, winning overrides, optional targets,
actor resurrection, faction and relationship retention, failed cyclic lists
and saved draw preservation. The earlier draft adds
`sav_71_fixed_equality_fixtures_follow_winning_container_contents` to check
that equality fixtures follow winning base overrides and exclude LVLI
contents. The later adversarial tests cover patch-owned inventory identities,
unresolved targets, absent references and a second reset after loading.
This follow-up corrects that test file's plugin index helper from `u8` to
`u16`, matching `GlobalFormId.file`. It also encodes the patch-owned item's
record and CNTO link with the patch's own master-list index, `0x01`, instead
of the master's index, `0x00`. It restores the missing machine-readable writer
reply and updates the validation report. No runtime fields, save sections,
dependencies or host nodes change.

Sources read: the row's `sav_68_reset.rs`, `sav_68_adversarial.rs` and
`world/reference_restore.rs`, the SAV-68 backlog row, `docs/intent.md`,
`docs/status.md`, `docs/style.md`, `docs/save.md` and the Reset paragraphs in
`docs/game.md`.
The local reference corpus at
`/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs`
supplies `ck-wiki/Reset_-_ObjectReference.wikitext` for inventory reset,
resurrection, retained factions and relationships, and unloaded-target rules,
and `uesp-modfileformat/LVLI.wikitext` for chance-none selection.
`crates/esm/src/load_order.rs` supplies the raw FormID owner mapping used by
the corrected fixture. No external parser or algorithm was added.

Files changed against master: `crates/game/tests/it.rs`,
`crates/game/tests/sav_71_reset.rs`,
`crates/game/tests/sav_71_breaker_breaker_adversarial.rs`, `docs/game.md` and
this report. This follow-up changes the adversarial test helper, its coverage
description in `docs/game.md` and this report, and creates the ignored
worktree-root `writer_reply.json`, which carries the machine-readable reply.

Earlier draft validation, with installed-data variables unset:

- `rtk cargo fmt --all`: exit 0.
- `rtk cargo test -p soe-game --test it sav_71_ -- --nocapture`: exit 0,
  nine synthetic checks pass. The two installed-data cases return early.
- `rtk cargo test -p soe-game --test it sav_68_ -- --nocapture`: exit 0,
  eight synthetic checks pass. The two installed-data cases return early.
- `rtk cargo clippy -p soe-game --all-targets -- -D warnings`: exit 0,
  no warnings.
- `rtk git diff --check`: exit 0.

The earlier harness reports 11 and 10 passes because a gated early return
counts as a pass. There were 17 executed synthetic checks and four untested
installed-data checks. The commands above ran through `rtk proxy env`, unsetting
`SKYRIM_DATA`, `SOE_TEST_SAVE`, `SOE_TEST_SAVES`, `SOE_TEST_SKYUI`,
`SOE_TEST_NORDIC` and `SOE_GODOT_BIN`.

Follow-up validation:

- `rtk cargo fmt --all`: exit 0 after the final fixture changes.
- The first `sav_71_` run exited 101 during compilation because the later
  adversarial helper used `u8` for `GlobalFormId.file`. The helper now uses
  `u16`, and its patch-owned raw FormIDs are corrected.
- `rtk cargo test -p soe-game --test it sav_71_ -- --nocapture`: exit 0,
  13 synthetic checks pass. Two installed-data checks return early.
- `rtk cargo test -p soe-game --test it sav_68_ -- --nocapture`: exit 0,
  eight synthetic checks pass. Two installed-data checks return early.
- `rtk cargo clippy -p soe-game --all-targets -- -D warnings`: exit 0,
  no warnings.
- `rtk git diff --check`: exit 0.

The follow-up executes 21 synthetic checks. The harness counts 25 passes
including four gated early returns. Test and Clippy commands run through
`rtk proxy env` with the six installed-data and Godot variables listed above
unset. Build flags, `CARGO_TARGET_DIR` and `RUST_TEST_THREADS` are unchanged.

Untested under light mode, run later with `SKYRIM_DATA` set:

- `sav_68_installed_changed_vanilla_reference_resets_and_round_trips`:
  `rtk cargo test -p soe-game --test it sav_68_reset::sav_68_installed_changed_vanilla_reference_resets_and_round_trips`
- `sav_68_changed_vanilla_container_resets_to_winning_record_after_load`:
  `rtk cargo test -p soe-game --test it sav_68_adversarial::sav_68_changed_vanilla_container_resets_to_winning_record_after_load`
- `sav_71_installed_changed_fixed_and_leveled_containers_reset_after_load`:
  `rtk cargo test -p soe-game --test it sav_71_reset::sav_71_installed_changed_fixed_and_leveled_containers_reset_after_load`
- `sav_71_installed_vanilla_container_resets_to_authored_state_after_load`:
  `rtk cargo test -p soe-game --test it sav_71_breaker_adversarial::sav_71_installed_vanilla_container_resets_to_authored_state_after_load`

The reviewer runs `rtk tools/lint.sh`. No installed-data tests, Godot runs or
full gate were run here. The save corpus is not required for this reset check.
The row remains partial because its commit attribution must be completed by
the lead and the four installed-data checks have not been rerun.

The existing `b90d795f` commit message does not name `f16d16ac`. This task
requires leaving changes uncommitted, so history has not been rewritten.
The lead's final commit should name the culprit, for example:

```text
SAV-71: Record f16d16ac reset regression and restore the writer reply

f16d16ac (UI-25) introduced leveled inventory selection during construction
and Reset. Keep fixed-inventory equality fixtures separate from leveled draws
and retain the existing loaded-reset regression coverage.
```
