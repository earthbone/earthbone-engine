# UI-11 writer report, 2026-10-12

Branch: `draft8/UI-11`. The existing implementation commits are `da5ddebd8`
and `e7655c50c`. This follow-up is uncommitted and unstaged.

The existing bridge and presenter are retained. The follow-up fixes the
adversarial tests' fixture imports and their sparse-slot expectation. It adds
text and toggle callback coverage, including original-framework state options,
and extends the windowed check to select an enabled toggle. The original
framework scripts remain external inputs. `SOE_TEST_SKYUI` can name an
extracted mod root or its BSA, so separate base and manager PEX variables are
not required. No dependencies, saved fields or per-reference nodes are added.
The bridge's finite-value check uses a match guard, and its selection flag
check uses the low two bits' trailing-zero count to satisfy current Clippy.
The invalid-update test also covers NaN and both infinities.
The host presenter snapshots its projected page before acquiring Godot's
mutable base guard, fixing the inherited borrow-check error.

## Files changed in this follow-up

- `crates/game/src/mcm.rs`: equivalent finite-value and flag checks accepted
  by current Clippy.
- `crates/game/tests/support/ui_11_fixture.rs`: state-option child fixture
  using the original base and manager scripts.
- `crates/game/tests/support/ui_05_fixture.rs`: a narrow dead-code allowance
  for the shared helper's host-only fixture writer.
- `crates/game/tests/ui_11_adversarial.rs`: compile fixes, empty-slot identity
  and registration/open callback counts.
- `crates/game/tests/ui_11_mcm.rs`: text and toggle selection counts,
  script-owned values, state callbacks and state restoration.
- `crates/godot-host/tests/ui_11_mcm.rs`: projection of state-option sparse
  IDs and rejection after script flag updates.
- `crates/godot-host/src/godot_player/menu.rs`: page snapshot before the
  mutable Godot base guard, so native control construction type-checks.
- `godot/ui_11_mcm_options_test.gd`: enabled toggle selection and restoration
  of the script-authored checked value.
- `docs/game.md` and `docs/godot-host.md`: descriptions of these checks.
- `crates/game/tests/ui_05_breaker3_adversarial.rs`: formatting only.
- `crates/godot-host/tests/ui_05_breaker_adversarial.rs`: formatting and
  removal of an unnecessary `mut` in the earlier draft's fixture.
- `docs/ui-11-report.md`: this report and external validation commands.

## Sources read

The corpus root is
`/var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs`.

- `skyui/dist/Data/Scripts/Source/SKI_ConfigBase.psc`: text and toggle
  constructors, flag replacement, sparse cursor positions, page IDs,
  state-option methods, callback routing and previous-state restoration.
- `skyui/dist/Data/Scripts/Source/SKI_ConfigManager.psc`: registration,
  mod/page selection and slot-index forwarding to the base script.
- `skyui/src/ModConfigPanel/ConfigPanel.as`: buffer projection, cursor values
  and flag replacement.
- `skyui/src/ModConfigPanel/OptionsListEntry.as`: disabled and hidden flags.

The new fixture calls these public script APIs. It does not replace the
framework scripts or introduce another interpreter. The code and its existing
docs were reviewed against the native Godot UI and MCM compatibility intent.

## Validation

The first filtered game test build exposed missing/private fixture constants.
Those imports are corrected. Initial Clippy runs also found two game bridge
lints and the host presenter's borrow error, corrected in this follow-up.
The full gate is reserved for the reviewer, as requested. No release build,
installed-data test or Godot process was run.

| Check | Exit status | Result |
|---|---|---|
| `rtk cargo fmt --all` | 0 | Formatted |
| Game UI-11 filtered tests before the bridge lint fixes | 0 | 12 passed, 7 exercised and 5 external cases returned early |
| Game UI-11 filtered tests after the bridge lint fixes | 130 | Interrupted after a prolonged rebuild, no final test result |
| Game UI-05 formatting-only test | 130 | Interrupted after a prolonged rebuild, no test result |
| `rtk cargo clippy -p soe-game --all-targets -- -D warnings` | 0 | No issues |
| `rtk cargo clippy -p soe-godot-host --all-targets -- -D warnings` | 130 | Interrupted after a prolonged check, no final result |
| Standalone host Clippy retry with `rtk timeout 120s cargo clippy -p soe-godot-host --all-targets -- -D warnings` | 124 | Timed out without further diagnostics |

The filtered test commands unset `SKYRIM_DATA`, save-corpus inputs, SkyUI
compiled inputs, NORDIC inputs and `SOE_GODOT_BIN`. External-case early
returns are not evidence of original-framework execution.
The environment keeps `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=1`.
The pending test and host-check sessions were interrupted through their
tracked session handles after more than an hour without new diagnostics.
Their exit statuses were collected. The standalone host Clippy retry also
ended with a collected timeout status. No checks remain running from this
follow-up. The final host borrow fix still needs a completed type check.

## Untested

The current-source game rerun did not finish. These synthetic cases passed
before the equivalent bridge lint fixes, but their final rerun is untested:

- `ui_11_script_buffers_preserve_text_toggle_flags_and_sparse_slots`
- `ui_11_script_flag_updates_reach_each_slot_in_call_order`
- `ui_11_script_cursor_updates_keep_text_and_toggle_values_separate`
- `ui_11_selection_rejects_disabled_hidden_stale_and_unknown_ids`
- `ui_11_text_and_toggle_dispatch_once_per_press_without_changing_values`
- `ui_11_bad_updates_and_flushes_preserve_current_rows`
- `ui_11_close_discards_presentation_before_reopening`

The latest non-finite-value assertions and unrelated-flag assertion were
added after that successful run. Run these with external inputs unset:

```bash
rtk cargo test -p soe-game --test it ui_11_ -- --nocapture
```

Complete the interrupted package check with:

```bash
rtk cargo clippy -p soe-godot-host --all-targets -- -D warnings
```

Light mode prohibits all host test binaries, installed-data tests and Godot
runs. The following commands are for the lead after light mode ends, from
this worktree. Set `SOE_TEST_SKYUI` to the installed SkyUI mod root or BSA.
The game command runs these external-framework checks:

- `ui_11_unmodified_skyui_pex_adds_updates_and_selects_once`
- `ui_11_unmodified_skyui_state_options_update_and_restore_state_once`
- `ui_11_page_ids_reject_stale_page_selections`
- `ui_11_config_registration_and_callbacks_are_idempotent_per_open`
- `ui_11_selection_rejects_missing_form_and_out_of_range_id`

```bash
rtk cargo test -p soe-game --test it ui_11_ -- --nocapture
```

The host command exercises every test in `crates/godot-host/tests/ui_11_mcm.rs`:

- `ui_11_adds_text_and_toggle_rows_with_skyui_ids`
- `ui_11_updates_enabled_state_and_preserves_other_flags`
- `ui_11_selection_dispatches_once_and_rejects_disabled_rows`
- `ui_11_page_capacity_matches_skyui_slots`
- `ui_11_ski_config_fixture_builds_rows_updates_flags_and_calls_once`
- `ui_11_unmodified_skyui_sources_define_text_toggle_flags_and_callback`
- `ui_11_runs_unmodified_skyui_manager_and_base_pex_fixture`
- `ui_11_unmodified_skyui_state_options_project_sparse_ids_and_callback_once`
- `ui_11_windowed_unmodified_scripts_render_update_and_callback_once`

For the source check, set `SOE_SKYUI_CONFIG_BASE` and
`SOE_SKYUI_CONFIG_MANAGER` to the corpus PSC paths above. For the windowed
check, set `SOE_GODOT_BIN` to the supplied Godot executable. Before its first
run, build the extension and import the project. These commands must wait
until light mode ends:

```bash
rtk cargo build -p soe-godot-host --release
rtk "$SOE_GODOT_BIN" --headless --path godot --import
rtk cargo test -p soe-godot-host --test ui_11_mcm ui_11_ -- --nocapture
```

The windowed test launches at 640 by 360, captures the viewport and quits
with a checked exit status. Its artifacts go to `target/ui_11_host`.
`godot/soe.gdextension` is unchanged.

The earlier UI-05 host test that received formatting and the `mut` correction
is also untested here:

```bash
rtk cargo test -p soe-godot-host --test ui_05_breaker_adversarial ui_05_godot_renders_declared_pages_in_script_order -- --nocapture
```

The formatting-only game file's real-PEX check needs the same SkyUI input:

```bash
rtk cargo test -p soe-game --test it ui_05_reselecting_config_closes_then_reinitializes_its_page -- --nocapture
```

Slider, menu, key-map and color dialog behavior belongs to other rows.
No claim of a completed installed SkyUI or windowed run is made here.
