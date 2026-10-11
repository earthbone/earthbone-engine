# Performance

## Fixed-tick budget gate, PRF-37 draft

`godot/tst_18_frame_budget_test.gd` starts the PRF-12 walkthrough, warms it
for two wall-clock seconds and samples 240 completed render frames in both
the Bannered Mare and the Tamriel `WhiterunExterior04` cell. It then quits.
`simulation_budget.gd` declares the limits before measurement: main-thread
engine scope time at most 2 ms p50 and 4 ms p99, with no continuous simulation
lag above one second. Empty samples, non-finite times and no simulation
progress fail. Per-frame raw main, tick and worker samples and mean exclusive
system times are written to `SOE_PRF_37_REPORT`, with an F3 capture at each location.

Select the draft adapter with `SOE_THREADED_SIMULATION=1`. The host tests
`prf_37_tst_18_rejects_main_thread_percentiles_and_simulation_lag` and
`prf_37_installed_windowed_tst_18_gate` exercise the gate contract and
installed-data rendering separately. Build the release extension and import
the project before running them, as for the other host checks.

The 2 ms budget applies jointly to player controller work, Jolt synchronization,
snapshot application and the remaining instrumented PlayerNode callbacks.
There is no separate allowance per catch-up physics callback. Simulation
ticks and navigation-worker busy time are reported outside this total.
Worker busy time overlaps simulation wall time. Host/GDScript work outside
engine scopes is outside this measurement and remains visible through the
host frame denominator. This draft measures snapshot placement, contact and
sky presentation. The legacy gameplay presentation adapters are not part of
the draft path's timing evidence. This is not a comparison with the user's
interactive mod profile. The gate uses five installed masters and the fixed
resident block, without walking across its boundary.

### PRF-37 open gap, round 7 check

The threaded simulation is opt-in through `SOE_THREADED_SIMULATION=1`. The
default `start_gameplay` path, which is the one the user plays, still runs the
game inline in `physics_process` with Godot's catch-up ticks, so a slow frame is
followed by up to eight physics callbacks and the spiral the row describes is
still live there. The TST-18 gate above passes only on the draft path. The
default path is neither restructured nor measured by it, so the 2 ms main-thread
clause cannot be read as the only open piece of PRF-37. Moving the default path
onto the worker and measuring it with ordinary input are both still missing.

### PRF-37 installed measurement, 2026-10-06

The release extension with Godot 4.4.1, windowed software Vulkan at 640 by 360,
passed both location gates. Neither measured window overlapped a Rust build
or another Rust test.
Each location supplied 240 completed render frames. Bannered Mare published
seven contacts. The plains published two, including the player and a resident
NPC from Tamriel's persistent cell. Both ran package evaluations.

| Location | Main p50 ms | Main p99 ms | Maximum lag s | Simulation ticks during sample |
|---|---|---|---|---|
| Bannered Mare | 0.152 | 0.243 | 0.050 | 357 |
| Whiterun plains | 0.214 | 0.341 | 0.140 | 890 |

The shared 2 ms main-thread allowance had this measured mean cost split.
These exclusive means add to the total mean, not to either percentile.

| Main-thread engine work | Bannered Mare mean ms | Plains mean ms |
|---|---|---|
| Player controller and Jolt synchronization | 0.100647 | 0.116801 |
| Input | 0.029796 | 0.047130 |
| Snapshot rendering and process presentation | 0.012331 | 0.037016 |
| Other covered callbacks | 0.009411 | 0.010260 |
| Streaming | 0.000106 | 0.000074 |
| Total | 0.152291 | 0.211281 |

Simulation tick p50/p99 was 0.459/23.218 ms in Bannered Mare and
1.972/5.729 ms on the plains. The occasional long interior tick did not
sustain overdue simulation for one second. Mean reported navigation-worker
busy time was 0.000315 ms and 0.000137 ms respectively. Those worker values
are included in simulation wall time and excluded from the main total.

The raw timing JSON, two F3 PNGs and process logs are under `target/prf-37-*`.
The process exited zero and printed `WALKTHROUGH stopped`. Its logs include
unsupported conditions and Papyrus calls against None objects. The timing
pass does not establish gameplay fidelity. Initial gate setup was corrected
to clear interior geometry, attach renderer residency and include the
worldspace persistent cell before this measurement.

All 20 new PRF-37 Rust checks passed: six simulation unit tests, nine game
integration tests, two timing-total tests, one VM deadline test and two host
checks. Both installed game cases and the windowed host case ran with
SKYRIM_DATA. The six trace frame tests and 23 existing VM tests also passed.
Formatting and all-target package Clippy exited zero for game, godot-host,
papyrus and trace. Cargo reported a future Rust compatibility notice for
`proc-macro-error2` 2.0.1. The reviewer owns the full workspace gate.
No dependencies, saved fields or reference scene-node paths were added.

### PRF-37 on hardware, 2026-10-09

Run on the user's own machine (NVIDIA GeForce RTX 5070 Ti, Vulkan 1.4.351
Forward+), release extension, run master c0b3bb622, 640x360 window.
`tst_18_frame_budget_test.gd` under `SOE_THREADED_SIMULATION=1` passes, main
thread 0.051 ms p50 and 0.101 ms p99 over 100 ticks. On the default path it
fails with "simulation never published a first snapshot", since the test
drives the threaded adapter. The PRF-36 walk under
`SOE_THREADED_SIMULATION=1` fails navigation readiness, the interior exit and
the Whiterun gate, so door and cell transitions are missing from the
threaded adapter. It stays opt-in until those work.

## Recorded sessions and profile differences, PRF-22

Record a fixed-step workload through the PLY-01 input layer. In Rust, call
`Driver::start_recording(&mut game, seed)`, queue and execute commands, then
write the returned `Driver::finish_recording(&game)` session with
`Session::write(path)`. A Wait command represents an idle simulation frame.
The recording includes the seed, initial world and VM snapshot, transient
residency and binding state, inputs, results and a state hash every 60 frames.
It also hashes the initial state and the final partial interval.

Enable `soe_trace::Profile` and clear it after setup. Export the first run
with `Profile::export_summary(before_path)`. Load the same content definitions
and providers in a fresh Game, call `Driver::start_replay`, then step its
queued commands. Check `Driver::replay_error` after each step. Changed plugin
or script provenance rejects replay before replacing state. An outcome or
hash divergence stops execution and reports its frame. Export the replay's
summary as `after.json`, then compare:

```text
rtk cargo run -p soe-bench --release -- profile-diff before.json after.json
```

The command accepts the PRF-13 `soe-entities.json` counter summary, rather
than its scope trace. It needs no SKYRIM_DATA. Save a copy of each summary
after ending capture. Repeat the same session after changing the engine to
compare per-entity behavior and cost with the same inputs. Content changes
require a new recording. Keep the CPU and workload fixed when interpreting
wall-clock differences. The diff ranks actors, scripts, quests, plugins and
systems independently for self time, inclusive time, calls, waits and each
work counter, with signed percentages. Script functions and entity phases
are grouped. Each metric lists the ten largest absolute changes, including
decreases. New entities have a zero baseline, so their percentage is reported
as new. Game and host system intervals are reported separately and excluded
from plugin sums. Older PRF-13 files lack system totals, which compare as zero.

PlayerNode exposes the same workflow with `record_player_session(seed)`,
`write_player_session(absolute_path)` and `replay_player_session(absolute_path)`.
Set `auto_step` to false, feed `queue_player_input` while recording, and use
`step_player_input` for both runs. The method reports replay divergence as
false, and `player_session_error` supplies its diagnostic. Only PLY-01 inputs
are recorded. Arbitrary device controls and renderer state are outside the
session hash, which covers Game and VM state.

The automated test
`prf_22_600_frame_session_replays_twice_and_injected_slow_script_tops_profile_diff`
records 600 frames, replays twice with matching hashes and injects a
five-millisecond native delay into SlowScript during the second replay.
It compares actual PRF-13 exports through the CLI and requires SlowScript
to head the script self-time diff. The delay affects execution time without
changing gameplay state. After building and importing the extension, export
the synthetic PLY-01 fixture and run the host replay check:

```text
rtk proxy env SOE_PLY_01_FIXTURE="$PWD/target/prf-22-fixture" cargo test -p soe-game --test it ply_01_sequence_opens -- --nocapture
rtk proxy env SOE_PLY_01_FIXTURE="$PWD/target/prf-22-fixture" "$SOE_GODOT_BIN" --headless --path godot --script prf_22_session_test.gd
```

The host check records and replays 600 steps twice, checks eleven hashes and
complete event logs, and quits. This is a simulation correctness check.
Rendering frame comparisons continue to use the windowed bench.gd workflow.

## Capacity probes, PRF-28, 2026-10-05

The CPU and light probes are described in [bench.md](bench.md) and
[godot-host.md](godot-host.md). These measurements use the Ryzen 9 7900X,
with 24 online logical CPUs. CPU curves use one worker in a release build,
ten warm-up ticks and 120 measured ticks per rung. Each curve doubles from
16 through at most 65,536 actors, stopping on the first p99 above 16.7 ms
or worst frame above 33 ms. The table records sampled passing rungs,
not exact maxima between those rungs. Setup and reports are excluded.
No local Cargo build overlapped these CPU measurements. Other machine
activity was not controlled. CPU subsystem curves do not measure a complete
rendered gameplay frame.

| Workload | Highest passing rung | First failing rung | Passing p99 | Failing p99 | Failing worst |
|---|---:|---:|---:|---:|---:|
| Synthetic high-process AI | 4,096 | 8,192 | 15.524 ms | 33.560 ms | 50.656 ms |
| Whiterun high-process AI | 2,048 | 4,096 | 12.199 ms | 22.473 ms | 24.429 ms |
| Synthetic CPU animation | 2,048 | 4,096 | 15.972 ms | 24.711 ms | 24.877 ms |
| Whiterun CPU animation | 512 | 1,024 | 7.540 ms | 18.970 ms | 21.755 ms |

The synthetic AI workload retains the existing Travel/Sandbox records.
Whiterun uses the five installed masters and repeats enabled placed actor
bases with their authored packages and positions. The synthetic animation
fixture blends two constant 99-bone clips. Whiterun's first selected actor
uses a 99-bone rig, 822 graph nodes, 205 loaded clips and 94 skin matrices
across its visible surfaces. Its loader reports 14 missing clips and 13
unresolved behavior references. The log records unsupported interpreter
work at every rung. Both animation curves measure graph evaluation and CPU
skin matrices, without GPU vertex skinning or render submissions.

Reproduction, after building `soe-bench --release`:

```text
rtk proxy target/release/soe-bench capacity ai synthetic --start 16 --max 65536
rtk proxy env SKYRIM_DATA=<Data> target/release/soe-bench capacity ai whiterun --start 16 --max 65536
rtk proxy target/release/soe-bench capacity animation synthetic --start 16 --max 65536
rtk proxy env SKYRIM_DATA=<Data> target/release/soe-bench capacity animation whiterun --start 16 --max 65536
rtk proxy env SKYRIM_DATA=<Data> target/release/soe-bench capacity settings
```

### CPU p99 curves

Every cell below is a measured p99 in milliseconds. A dash means the
curve stopped at an earlier breach. These are separate subsystem workloads.

| Actors | Synthetic AI | Whiterun AI | Synthetic animation | Whiterun animation |
|---:|---:|---:|---:|---:|
| 16 | 0.056 | 0.203 | 0.093 | 0.112 |
| 32 | 0.093 | 0.592 | 0.194 | 0.335 |
| 64 | 0.197 | 1.086 | 0.328 | 0.732 |
| 128 | 0.572 | 1.663 | 0.835 | 1.727 |
| 256 | 0.738 | 3.394 | 1.564 | 4.312 |
| 512 | 1.808 | 7.467 | 3.274 | 7.540 |
| 1024 | 2.736 | 8.914 | 6.522 | 18.970 |
| 2048 | 7.382 | 12.199 | 15.972 | - |
| 4096 | 15.524 | 22.473 | 24.711 | - |
| 8192 | 33.560 | - | - | - |

### Windowed light curves

The light runs use Godot 4.4.1 Forward+ with llvmpipe LLVM 22.1.8 software
Vulkan. They render 1,600 by 900 inside a 640 by 360 window, with vsync off.
Every rung measures at least 600 frames and five seconds after pipeline
warm-up. Lights move every frame through server RIDs and add zero nodes per
light. The positional shadow atlas is 4,096 pixels with 1,024 map slots.
The maximum configured light count is 512, matching the clustered-element
limit. These runs measure this software renderer, not a physical GPU.

| Scene and lights | Highest passing rung | First failing rung | Baseline p99 | Passing p99 | Failing p99 | Failing worst |
|---|---:|---:|---:|---:|---:|---:|
| Synthetic dynamic | 256 | 512 | 6.922 ms | 16.544 ms | 21.686 ms | 23.592 ms |
| Synthetic shadow | 8 | 16 | 8.437 ms | 16.441 ms | 20.520 ms | 24.459 ms |
| Whiterun dynamic | 0 | 1 | 87.372 ms | - | 82.796 ms | 89.030 ms |
| Whiterun shadow | 0 | 1 | 79.790 ms | - | 251.902 ms | 336.052 ms |

Each cell below is frame-wall p99 in milliseconds. The zero-light rung is
the baseline. A dash means that the curve stopped at an earlier breach.

| Added lights | Synthetic dynamic | Synthetic shadow | Whiterun dynamic | Whiterun shadow |
|---:|---:|---:|---:|---:|
| 0 | 6.922 | 8.437 | 87.372 | 79.790 |
| 1 | 9.963 | 9.854 | 82.796 | 251.902 |
| 2 | 7.830 | 11.509 | - | - |
| 4 | 7.899 | 13.128 | - | - |
| 8 | 9.243 | 16.441 | - | - |
| 16 | 9.274 | 20.520 | - | - |
| 32 | 8.967 | - | - | - |
| 64 | 11.071 | - | - | - |
| 128 | 13.477 | - | - | - |
| 256 | 16.544 | - | - | - |
| 512 | 21.686 | - | - | - |

The synthetic scene draws a plane and 64 boxes. Whiterun streams nine cells,
957 references, 287 render instances and 282 parsed meshes with no failed
meshes. Its baseline draws 388,160 triangles and 289 draw calls. The probe
checks that more than the boxes' 768 triangles reach rendering. Buried
geometry extends the raw scene bounds far below the city, so the camera and
light grid use the upper eight metres of those bounds. Pixel checks confirm
that lights change the image and disabling shadows changes each shadow rung.

Both Whiterun baselines already exceed the budgets. Zero is the measured
capacity for added lights in these runs. The dynamic baseline has a 1,354.335 ms
worst frame, despite pipeline warm-up, and the following one-light window has
an 89.030 ms worst frame. The shadow run's one-light rung increases drawn
primitives to 900,092 and draw calls to 578. These are separate scene runs,
not estimates of light-only time. They do not establish physical GPU capacity.

Build the release extension and import once before these commands. Set
`SOE_CAPACITY_WORLD` to `synthetic` or `whiterun`, and
`SOE_CAPACITY_LIGHTS` to `dynamic` or `shadow` for each curve:

```text
rtk cargo build -p soe-godot-host --release
rtk proxy "$SOE_GODOT_BIN" --headless --path godot --import
rtk proxy env SKYRIM_DATA=<Data> SOE_CAPACITY_WORLD=whiterun SOE_CAPACITY_LIGHTS=shadow SOE_CAPACITY_MAX=512 "$SOE_GODOT_BIN" --path godot --resolution 640x360 --script capacity_lights.gd
rtk proxy "$SOE_GODOT_BIN" --headless --path godot --script prf_28_capacity_test.gd
```

The corrected Whiterun dynamic run set `SOE_CAPACITY_MAX=1` and failed at that
first rung. The other three curves used 512. Each completed windowed process
exited with status zero after releasing the scene and light RIDs. Raw reports
are retained locally under `target/prf-28-{ai,animation,lights}-*.log`, with
settings in `target/prf-28-settings-vanilla.log`. The light logs contain p50, p99, p99.9 and worst values. CPU logs contain
p99 and worst values, workload counts and unsupported animation work.
`target/prf-28-report.json` combines them with source hashes for this checkout.

Validation ran the installed-data Rust cases: 22 bench tests passed and one
existing stress survey remained ignored. All ten new `prf_28_` Rust tests
passed. The six headless light-ladder checks and four windowed light curves
completed successfully. Bench and host clippy with all targets passed. The
full gate is left to the reviewer, as requested.

### Stored settings and source boundaries

The Skyrim.esm-only survey finds 35 numeric name candidates containing actor,
process, light or shadow. The stored detection settings include
`iLightLevelInteriorMod=25`, `iLightLevelMax=300`,
`fDetectionSneakLightMod=15`, `fSneakLightMult=0.33` and
`fSneakLightExteriorMult=0.5`. These are light-level inputs, not counts of
renderable lights. No numeric processing or rendered-light count cap is
stored among those candidates.

Local Wrye Bash `Mopy/bash/patcher/patchers/multitweak_settings.py` names
`iAINumberActorsComplexScene` as the active-AI limit and
`iNumberActorsInCombatPlayer` as the combatant limit. It lists 20 for both
TES5 defaults. `iNumberActorsAllowedToFollowPlayer` is a follower limit.
All three are absent from this installed Skyrim.esm. Those published defaults
are not values read from its DATA fields. UESP's local GMST page states that
many used GMST defaults are omitted from plugins and that editor IDs supply
setting identity. The synthetic patch test verifies that a later record with
the same editor ID can raise a setting through the existing World reader.

Local Community Shaders `src/Features/LightLimitFix.cpp::DataLoaded` names
`iMagicLightMaxCount` and raises it to the maximum signed integer. This name
is also absent from the stored numeric survey. `LightLimitFix.h` describes
a vanilla four-light renderer restriction. Its cluster and total-light
constants belong to that mod's renderer, not to Skyrim.esm records or to
this engine's Godot renderer.

The installed `Skyrim_Default.ini` contains `[Display]`
`fLightLODMaxStartFade=3500`, `fLightLODStartFade=3500`,
`fShadowLODMaxStartFade=1000` and `iShadowMapResolutionPrimary=2048`.
They control fade distances and shadow resolution. They do not state actor
process-level counts or a general light count cap. INI values do not come
from Skyrim.esm. The local CK dump has no page defining those processing or
light-count settings. No game binary was read for this survey.

PRF-28 is incomplete. This checkout has no middle-high, middle-low or low
process scheduler and no PRF-17 worker implementation, so those curves cannot
be measured through an executing engine path. The probes reject those modes.
GPU actor capacity and runtime application of GMST/INI caps also remain
unimplemented. The measured rungs are not installed as runtime limits.

The two breaker tests that need the missing pieces are marked `#[ignore]` with the
reason: `prf_28_measures_each_vanilla_process_level` and
`prf_28_each_vanilla_process_level_executes_a_capacity_measurement` wait for
PRF-37, `prf_28_measures_ai_with_all_available_workers` and
`prf_28_capacity_accepts_one_and_core_count_ai_workers` wait for the PRF-17
worker pool. Run them with `--include-ignored` once those rows land.

## Permanent AI breakdown, PRF-27

F5 in the walkthrough shows the always-on AI breakdown from
`PlayerNode::ai_breakdown`, retaining the diagnostic phases formerly used only
by the PRF-11 planning script. The headless data lives on `AiWorld` and is
independent of optional trace/entity capture. Procedure and decision durations
are inclusive, path latency uses wall time, process cadence and the five-second
stuck threshold use the non-menu world clock. See [game.md](game.md) for the
measured boundaries, bounded history and currently inactive categories.

The `prf_27_` Rust tests exercise permanent phases, procedure categories,
request slicing, expansion counts, failures, cached results, cancellations,
re-plans, reaction pairing, stuck progress and process counts. The installed
Bannered Mare case is gated on SKYRIM_DATA. The ignored release test
`prf_27_retains_prf_12_accumulator_budget` applies PRF-12's existing median
on/off threshold of 1.01 and checks identical actor positions. That calibration
suspends system timing only. It does not isolate the new always-on AI
measurements from a pre-change binary.

The separate `prf_27_combined_measurements_stay_under_one_percent` release-probe
comparison uses the same 1,000-actor fixture, ten warmup ticks and 240 measured
ticks. The pre-change baseline uses `c3e2d1de` with system timing suspended.
The current probe keeps system timing and AI measurements on. Both disable
optional entity capture and tracing. Fifteen alternating process pairs must
have equal final-position checksums and a median current/baseline ratio below
1.01, fixed before the first comparison. The probe source is
`crates/game/examples/prf_27_probe.rs`, copied into the baseline source snapshot
before building it. Its `suspended` argument disables system timing.

```text
rtk cargo build -p soe-game --release --example prf_27_probe
rtk cargo test -p soe-game --test it prf_27_combined_measurements_stay_under_one_percent -- --ignored --nocapture
```

The comparison takes executable paths through SOE_PRF_27_BASELINE and
SOE_PRF_27_CURRENT. Keep both compiled probes outside Cargo's shared public
example filename and verify that the current binary contains the new procedure
slots. Run timing tests after all local builds and Godot checks have exited.

## Entity capture workflow, PRF-13

The ignored release check `prf_13_disabled_capture_retains_prf_12_frame_budget`
compares two optimized timing probes through `SOE_PRF_13_BASELINE` and
`SOE_PRF_13_CURRENT`. Both use `prf_12_frame::actors(1000)`, ten warmup ticks
and 240 measured ticks. Each prints elapsed nanoseconds and a checksum of the
actors' final positions. The baseline uses the pre-change crates with system
timing suspended. The current probe keeps system timing on and entity capture
off. Fifteen alternating pairs must have equal checksums and a median ratio
below 1.01. The existing PRF-12 calibration also runs in that test.

Press F4 in the walkthrough to enable entity capture and its rolling rankings.
Press it again to finish capture and write `user://soe-profile.json` and
`user://soe-entities.json`. Player exit also writes both reports. F3 continues
to show system frame timings. Headless Game runs accept `SOE_PROFILE=1` and
`SOE_PROFILE_DIR=<existing-directory>` to write the same reports on drop.
Open either JSON file in Perfetto. The scope trace carries entity and plugin
arguments, while the summary contains session counter events per entity phase
and plugin. Script identities include functions. The overlay groups functions
by script and ranks actors, scripts, quests and plugins by self time.

Nested scopes report inclusive time and self time. Plugin totals use self
time, and operation counts belong to the innermost entity. Latent waits
report real scheduler intervals separately from running time. Forms charge
their defining plugin, regardless of the winner. Archived PEX providers map
to plugin names, loose PEX files retain their source paths as provider labels.
The trace retains the newest `MAX_EVENTS` scopes, while summary totals cover
the complete session. Disabled factories do not construct strings or read
clocks. Attribution adds no saved state and uses no new runtime dependency.

The harness is `crates/bench` (binary `soe-bench`) plus `crates/trace`
(scoped spans) and `godot/bench.gd` (frame time). Every number below comes
from one machine and one commit, so a later run on the same machine can be
diffed line by line. Vanilla numbers are a separate problem, see the end.

Capacity sweeps, enforced frame budgets and sustained workload checks are
documented in [performance-limits.md](performance-limits.md). Historical
measurements below remain tied to their recorded revisions.

## Menu and save transitions, PRF-26

The walkthrough records modal-menu open and close requests, dialogue and
load-door activation, native generation saves, quick saves, native quick-load
recovery and full generation loads through the first
`RenderingServer.frame_post_draw`. Its exit summary is written to
`user://soe-transitions.txt` with p50 and worst milliseconds. The windowed
`godot/prf_26_menu_transitions_test.gd` exercises the named menu shells and a
500-row list workload, with per-menu list construction, filtering and sorting
times. The rows are synthetic labels, not timings of each record-backed
presenter. Door and dialogue timings end at first draw, rather than measuring
the first resumed control callback. Fast travel has no host transition entry
point in this checkout. PRF-16 has no transition budgets until the remaining
paths and native presenter measurements are implemented.

## TST-18 gameplay frame gates

The `tst_18_` timing checks use fixed limits: p99 at most 16.7 ms and
every measured frame at most 33 ms. Limits are checked directly in Rust and
in the Godot script, with no environment override. These headless gameplay
measurements include simulation and host callbacks, with no GPU throughput
claim. They do not use `Performance.TIME_PROCESS` as a frame denominator.

`crates/bench/tests/tst_18_walkthrough.rs` launches
`godot/tst_18_walkthrough.gd`, which inherits the real walkthrough startup,
once for WhiterunBanneredMare and once for WhiterunOrigin's radius-one block.
Passing children run at least 60 wall seconds and 3,600 fixed 1/60-second
steps. A run with a frame breach reports failure after 60 measured wall
seconds, even if it has not reached 3,600 steps.
Setup and a 20-wall-second gameplay settling phase are excluded. Wall
intervals between process callbacks include the explicit player advance and
the other host callbacks. Once per simulated second, the measured frame also
probes the navigation graph at world positions of NPCs with supported packages,
matching AI's package evaluation interval so retained planner results
cannot hide an expensive lookup. At least five actors, a nonempty navigation
graph, package evaluations, selected packages throughout measurement and
successful probes at every scheduled navigation frame are required. These
read-only probes add a fixed cadence to the headless walkthrough gate. They
are excluded from settling.
The script counts every frame above 33 ms, reports p99 and worst, then quits
with failure on any breach. The parent checks the report and exit status,
drains both pipes and kills and reaps a child after 240 seconds. Each child
also has a 210-second script watchdog. Measurements and diagnostics are
retained under the ignored `performance-results/tst-18/` directory.

The PRF-07 fixture retains its Travel and Sandbox routes and adds three
overlapping NAVM floors, each with 20,000 triangles, giving 60,002 total.
Graph construction and package selection occur in setup. A cold 120-tick
work-count check includes the first package tick and permits at most 4,000
triangle examinations per tick. The optimized PRF-07 test measures a cold
120-tick window including the first package tick, then checks the Python
runner's 120-tick window after one warm-up tick. Both assert numeric p99 and
worst against the same limits, with process RSS below 2,048 MiB. The timing
test is ignored in the ordinary workspace gate and uses Cargo's benchmark
executable from the explicitly selected optimized profile. It does not start
a nested build. Run it as documented in [performance-limits.md](performance-limits.md).
Every actor selects its expected procedure. Travel actors must advance. Both
reports are checked for numeric timings, graph size and package
evaluations. The cold child has a 120-second deadline and is killed and reaped
on timeout. Its complete output is retained alongside the walkthrough logs.

With `SKYRIM_DATA` set, build the release extension and import the project
before running the installed gate. `SOE_GODOT_BIN` names the host executable:

```text
rtk cargo build -p soe-godot-host --release
rtk proxy "$SOE_GODOT_BIN" --headless --path godot --import
rtk cargo test -p soe-bench --test tst_18_walkthrough -- --nocapture
rtk cargo test -p soe-bench --bin soe-bench tst_18_ -- --nocapture
rtk cargo test -p soe-bench --test prf_07_adversarial -- --nocapture
```

Unset `SKYRIM_DATA` skips only the installed walkthrough cases. The synthetic
thousand-actor limits remain active in the normal Cargo test gate.
Sources are the local `godotwind/docs/systems/benchmarking.md` for separate
setup, its 10 to 20 second settling interval and workload counters, and
`godot-docs/classes/class_time.rst` for
monotonic frame intervals. The headless gameplay requirement is separate from
that manual's windowed rendering acceptance. NAVM fixtures use this engine's
geometry writer, following `TES5Edit/Core/wbDefinitionsTES5.pas`'s `wbNVNM`,
including 16-bit vertices and signed 16-bit edge and grid triangle indices.
Each added floor stays within those index widths.

## Per-system frames, PRF-12

PRF-20 cell cost reports, texture inventories, plugin record bodies and counting
allocator ownership are documented in [godot-host.md](godot-host.md) and
[trace.md](trace.md). Optimized allocation captures require the host's
`profile-memory` feature. The benchmark enables that feature and reads the
shared counters as described in [bench.md](bench.md). Streaming diagnostics
name the slowest asset. PRF-18's dump integration is not present in this branch.

`soe-trace` records fixed-slot exclusive system time with optional Chrome
tracing off. `Game::frame_history` provides headless tick frames and rolling
240-frame statistics. PlayerNode provides presentation frames through
`SOE/<system>_ms` custom monitors in Godot's Debugger Monitors tab, alongside
Godot's built-in process, physics and rendering monitors. F3 in the walkthrough
shows milliseconds and percentages of the wall frame. On exit it prints and
writes `user://soe-systems.txt` with per-system session totals, means, worst
times and rolling p50 and p99. Idle systems report zero.

The tick denominator is tick wall time. The host denominator is wall time
between completed PlayerNode process callbacks, including rendering waits.
Main-thread scope durations include waits inside their calls and do not
measure GPU execution. Details and source paths are in
[trace.md](trace.md), [game.md](game.md) and [godot-host.md](godot-host.md).
Percentiles are computed when queried, outside the simulation tick.

## Reference synchronization timing, PRF-34, 2026-10-06

The original Whiterun plains F3 capture on `8ae73582` recorded
`host_render_submission` at 121.7 ms p50, 355.4 ms p99 and 370 ms worst.
The capture attributed five of ten stack samples to reference collision
synchronization and two to reference change synchronization, but did not
separate their phase times. After the PRF-34 changes, the breaker walk ran
for 60 seconds in Bannered Mare and on the Whiterun plains with release
Godot 4.4.1 at 640 by 360 on llvmpipe software Vulkan.

| Location | Collision sync p50 / max ms | Change sync p50 / max ms | Host scene CPU p50 / p99 ms |
|---|---:|---:|---:|
| Bannered Mare | 0.054 / 1.296 | 0.033 / 0.362 | 0.324 / 1.424 |
| Whiterun plains | 0.559 / 2.460 | 0.212 / 1.226 | 1.142 / 3.326 |

`godot/prf_34_walk_test.gd` reports per-frame phase samples and applies the
0.2 ms sync maximum and 0.5 ms p50, 1 ms p99 host scene CPU limits. It runs
windowed against installed data and no cargo test starts it, so the gate does not
cover it.

Before the round 7 fixes the script reported 271 failed checks. Whiterun plains
had reference_collision_sync worst 1.07 ms, reference_change_sync worst 0.457 ms
and host_scene_cpu p50 0.613 ms, p99 1.363 ms. The causes were three passes
that touched every body each time a few bodies changed:

- The reference to bodies lookup was rebuilt from scratch inside the timed
  sync. It is now rebuilt once where `collision_references` is replaced.
- `sync_collision_state` rescanned every static and movable body whenever the
  list grew. Bodies only join at the end, so it now resumes from a cursor and
  visits only the new ones. A removal or rebuilt body restarts the pass.
  Both passes stop on a 12 microsecond budget and resume next frame.
- The placement index and collision state passes read the clock every 16 items,
  which let a cold run of 16 overshoot. They now read it every 4.

After these, three consecutive windowed runs of the script on this machine
reported failures=1, 0 and 1. The two failing runs missed one check each, the
reference_collision_sync worst frame on Whiterun plains at 0.20 to 0.22 ms. The
passing run had a worst of 0.17 ms. p50 and p99 of both sync functions and of
host_scene_cpu pass in all runs (reference_collision_sync p50 0.009 ms on the
Bannered Mare, reference_change_sync worst 0.09 ms, host_scene_cpu p50 0.20 ms,
p99 0.34 ms). Checkpoint timing inside the sync located the outlier frames in a
single `watch` resolving an enable state for a cold reference, which costs about
140 microseconds once and cannot be sliced further without deferring the
reference's first collision.

PRF-34 is partial. The timed clause is met on p50 and p99 and on one of three
runs for the worst frame. What is missing is a worst frame under 0.2 ms on every
run, which needs the first enable state lookup of a newly resident reference
moved off the frame that streams its cell, or a longer-lived cache of that state.
The 14 `game it prf_34` tests prove idle queries perform zero record lookups and
pass, but they do not replace the installed windowed timing.

### PRF-34 on hardware, 2026-10-09

Run on the user's own machine (NVIDIA GeForce RTX 5070 Ti, Vulkan 1.4.351
Forward+), release extension, run master c0b3bb622, 640x360 window. Every
p50 and p99 clause passes. Bannered Mare collision sync worst 0.073 ms,
change sync worst 0.075 ms, host scene CPU 0.098 ms p50 and 0.171 ms p99.
Whiterun plains host scene CPU 0.029 ms p50 and 0.181 ms p99. The any-frame
0.2 ms clause still misses on 4 plains frames, collision sync worst 0.344 ms
and change sync worst 0.702 ms, which line up with cell attaches. That clause
is the only remainder.

## Magic tick work, PRF-35, 2026-10-06

The user's original Whiterun plains F3 capture on `8ae73582` recorded magic
at 25.3 ms p50 and 28.7 ms p99, with no spell being cast. The measurements
here compare `0cfd0df3` with this PRF-35 change on the same fixed route.
They use the release extension, installed Skyrim data and windowed Godot
4.4.1 at 640 by 360 on the private software Vulkan display. They measure
one simulation tick per windowed sample, rather than the user's original
session with automatic physics catch-up. The result is a fixed-route measurement only. It
does not show the cost under the eight catch-up ticks per frame of the default
path (see PRF-37). Those two workloads have different
timing distributions.

`godot/prf_35_magic_test.gd` starts the actual walkthrough in the Bannered
Mare, prepares navigation and warms 30 ticks. It measures 360 ticks of
forward and backward walking at a simulated 60 Hz, exits through the authored
tavern door and Whiterun gate, then repeats on the Tamriel plains.
Each location checks movement, one tick per sample and running NPC packages.
The CPU window surrounds `PlayerNode::advance` and closes before rendering
waits or reporting. Magic's exclusive PRF-12 slot includes its nested work.
The fixed acceptance limits, declared before measurement, are below 0.2 ms
p50 and below 0.5 ms p99 at both locations.

The old translation tick sorted every materialized reference, even without
a translation. Active effects repeatedly decoded MGEF and SPEL records and
called the actor builder, which rechecked passive spell lists. Grab Actor
selected from all contact actors, resolving their controller scales even
without a grab. These paths now use translation candidates, immutable decoded
definitions, existing target state and active-effect identity indexes.
Runtime MGEF overrides bypass the definition cache. Changed light transforms
and fades update their existing RIDs. No per-reference nodes, dependencies
or saved fields were added. Source paths and retained effect semantics are
documented in [game.md](game.md) and [godot-host.md](godot-host.md).

The report and F3 overlay subdivide the Game magic slot into image-space,
translations, shader particles, effect shaders, visual effects, active effects,
concentration, and a casting/dispatch remainder. The remainder accounts for
reactive VM casts and bookkeeping outside the seven tick calls. These values
are included in the existing system total. The percentile calculations stay
outside the measured tick.

The baseline raw samples are `target/prf-35-before-walk.json`. The gated
`prf_35_installed_windowed_magic_budget` writes `target/prf-35-after.json`,
its process log and an F3 PNG at each location, then quits Godot. Run it
after building the release extension and importing the project:

```text
rtk proxy env SKYRIM_DATA="/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data" cargo test -p soe-game --test it prf_35_installed_windowed_magic_budget -- --nocapture
```

The completed 2026-10-06 after run on Godot 4.4.1 and llvmpipe software
Vulkan measured 360 samples per location. Bannered Mare magic was 0.021 ms
p50 and 0.056 ms p99. Whiterun plains magic was 0.030 ms p50 and 0.063 ms
p99. The check passed both locations and the vanilla Candlelight light
creation, reuse and expiry checks. These fixed-step samples follow a
different workload from the original automatic-physics F3 capture.

## Process cadence and waypoint timing, PRF-36

The driver reports inclusive scheduling, actor processing, procedure-tree,
marker, navigation and waypoint phases through `ai_planning_details`. F3 keeps
exclusive PRF-12 system totals above these overlapping details. Navigation and
waypoint job counts and worker busy time are separate. Caller dispatch and join
waits remain part of main-thread AI time.

Actors outside the player's high-process range use coarser updates, with
accumulated simulation delta and actor-owned saved clocks. Immutable winning
PACK definitions and procedure trees are cached by resolved FormID, while live
overrides and restored package markers remain visible. Waypoint following uses
the PRF-17 pool and applies joined results in FormID order. The distances and
cadences are engine policy. Local CommonLibSSE-NG `AIProcess.h` and
`ProcessLists.h` name the four process levels, and OpenMW
`mwmechanics/actors.cpp` supplies the distance-processing example.

After building and importing the release extension, run:

```text
rtk proxy env SKYRIM_DATA=/path/to/Data cargo test -p soe-godot-host --test prf_36_ai_budget -- --nocapture
```

The windowed installed walkthrough samples 3,600 fixed simulation steps in
Bannered Mare, exits through the city gate, and samples another 3,600 while
walking in Tamriel. Fixed samples yield to rendering every 60 steps, as the
retained PRF-11 script does. A further 60-wall-second plains walk samples the
actual F3 presentation frames with automatic simulation and ordinary device
input. Both plains samples require combined main-thread `ai_movement` and
`ai_planning` below 0.6 ms p50 and 1.2 ms p99. The complete fixed-step driver
must stay below 8 ms. Evaluation counts, simulation progress, player movement
and worker activity guard against idle samples. The host test also runs the
unchanged PRF-11 Bannered Mare check, retains both logs under `target`, collects
both exit statuses and leaves neither process running.

The pre-change 120-step windowed probe on this worktree measured combined
AI p50/p99 of 3.167/12.156 ms in Bannered Mare and 2.784/8.440 ms in the
plains. These short probes do not establish a sustained budget. An intermediate
3,600-step Bannered Mare run measured 0.746/2.228 ms and a 10.079 ms driver
maximum, exceeding the 8 ms limit. That run rendered between every sample and
the launcher's 300-second limit terminated it before the plains completed.
Empty paths still dispatched waypoint jobs in that intermediate extension.
The driver now skips those jobs. No dependency or per-reference scene node
was added.

The completed 2026-10-06 Godot 4.4.1 llvmpipe run measured the fixed 3,600
step checks at 0.218/0.612 ms combined AI p50/p99 in Bannered Mare and
0.085/0.436 ms on the plains. Driver worst was 5.046 ms in Bannered Mare
and 0.842 ms on the plains. The retained PRF-11 60-second Bannered Mare
check passed with 5.598 ms worst AI time. The ordinary-input 60-second
plains walk only completed 30.27 simulation seconds across 227 presentation
frames because Godot reached eight physics catch-up ticks per frame. Its
combined AI was 1.145 ms p50 and 2.568 ms p99, above the 0.6 and 1.2 ms
limits. The PRF-36 host test therefore failed its live-walk checks. Fixed-step
success does not establish the live presentation budget.

After the PRF-11 steering scope was no longer measured twice and the per-actor
evaluation gate stopped re-checking actors that already hold a package, the
same ordinary-input plains walk (227 frames, 30.27 simulation seconds, eight
catch-up ticks per frame) measured 0.709 ms p50 and 1.474 ms p99 combined AI,
still above the 0.6 and 1.2 ms limits. The fixed-step checks pass, so the
live clause stays open until a hardware-rendered session of the user's own
shows whether the catch-up ticks per frame drop below eight.

Round 7 check, 2026-10-08. The reviewer's windowed run of
`cargo test -p soe-godot-host --test prf_36_ai_budget` on installed data failed
the live plains clause with 209 presentation frames, 0.690 ms p50, 2.140 ms p99
and 8.0 simulation ticks per frame, while the fixed-step checks passed (Bannered
Mare 0.177 ms p50 and 1.231 ms p99, plains 0.068 ms p50 and 0.317 ms p99). A
rerun on this machine afterwards passed: live plains 236 frames, 0.172 ms p50,
0.442 ms p99 and 2.0 ticks per frame. The two runs differ in how many catch-up
ticks Godot ran per frame, 8 against 2, and the budget tracks that count. The
live clause therefore passes or fails with host load, which is the PRF-37
catch-up spiral on the default path, so PRF-36 is partial until PRF-37 removes
the dependence. It is not a fixed-step cost problem.

### PRF-36 on hardware, 2026-10-09

Run on the user's own machine (NVIDIA GeForce RTX 5070 Ti, Vulkan 1.4.351
Forward+), release extension, run master c0b3bb622, 640x360 window. Every
clause of `godot/prf_36_ai_budget_test.gd` passes with failures=0. Bannered
Mare 0.165 ms p50 and 0.385 ms p99, plains 0.065 ms p50 and 0.217 ms p99,
live plains 0.279 ms p99 at 0.42 catch-up ticks per frame. The llvmpipe
misses (0.690 ms p50 and 2.140 ms p99 at 8 ticks per frame) came from the
software renderer driving the catch-up spiral, which hardware does not
reach. PRF-36 is done on this run. `prf_11_ai_planning_check.gd` passes on
the same run, worst AI 1.42 ms over 7 actors.

## Music decode frame cost, AUD-12, 2026-10-05

The installed PRF-12 walkthrough previously recorded 6,740.54 ms in the audio
system's worst frame when music first started. Music and ambience now decode on
a worker, and the Godot stream is created on the main thread after decoding.
The current installed-data `aud_12_installed_prf12_walkthrough_keeps_audio_under_two_ms`
run recorded a worst audio frame of 0.061 ms at its frame-12 report. The
acceptance threshold is 2 ms. This is one run on the machine below, not a
distribution across repeated launches. `aud_12_music_and_ambience_requests_do_not_block_and_reuse_decodes`
also checks synthetic music and ambience requests return under 2 ms, discard a
superseded result, preserve playing music until replacement, reuse cached paths
and keep cache accounting within 64 MiB.

## Runtime metrics, PRF-24

The F3 overlay and `user://soe-systems.txt` append a PRF-24 report. The
report includes stable fields for AI detection checks and pairs, line-of-sight
raycasts, path-cache hits and misses, physics bodies and contacts, audio voice
count and decode time, audio latency categories, animation phases and event
timing, and input-to-screen latency. The overlay also samples presentation
frame deltas in a 240-frame ring and reports standard deviation, p50, p99 and
worst. These frame values describe intervals between Godot overlay callbacks.

At this checkpoint, only detection observations, active sound voices,
uncached audio decode time, and presentation frame pacing have live producers.
Detection observations carry a caller-supplied visibility result and do not
perform a raycast. There is no path cache or live behavior-graph actor
evaluator. Physics contact counts, audio event-to-first-sample and output
latency, animation phase timings and gameplay annotation drift, and
input-to-screen latency remain zero pending instrumentation at those sites.
The field presence is a report contract, not evidence those systems have been
measured. `prf_24_metrics_report_overlay_and_summary` checks the field set,
frame statistics, overlay and written summary.

The metric set and rolling report shape follow the local
`engine-refs/godotwind/src/core/world/performance_profiler.gd` and
`src/tools/performance_report_contract.gd`. The engine uses atomic Rust
metrics for simulation and audio-host values, rather than copying the
godotwind dynamic GDScript counters.

## Physics frame details, PRF-25

The F3 overlay includes physics callback count, inferred catch-up callbacks,
Godot's `TIME_PHYSICS_PROCESS` per-step time and PHYSICS_3D active-object, collision-pair
and island monitors. A continuous catch-up streak lasting one second logs one
warning. Wrapped host raycasts report elapsed call time and count under the
active system. The report reserves shape-cast and overlap categories, but no
host caller currently records those kinds. `TIME_PHYSICS_PROCESS` reports the
last physics-step time and does not split Jolt integration among
body classes. Per-player/NPC controller time, ragdoll and dropped-item body
time/count, and cell-load collision-shape build time are not yet reported.
The synthetic report scene is `godot/prf_25_physics_test.gd`.

Run the declared overhead gate, which alternates recorded and suspended
eight-tick batches in one warmed 1,000-actor travel/sandbox world, then
asserts the median paired on/off ratio is below 1.01. A second world replays
the same tick count with timing suspended to verify identical actor positions:

```text
rtk cargo test -p soe-game --release --test it prf_12_accumulator_under_one_percent -- --ignored --nocapture
```

The Rust `prf_12_` checks cover complete slots, exclusive nesting, idle
frames, percentile eviction and overlapping game/host snapshots. After the
release extension build and import, run the live monitor and overlay check:

```text
rtk proxy "$SOE_GODOT_BIN" --path godot --resolution 640x360 --script prf_12_monitors_test.gd
```

## Hitch recorder, PRF-18

The default hitch threshold is twice the 16.7 ms frame budget, 33.4 ms.
`soe-trace` retains five seconds of named and exclusive system spans, frame spans
and frame work counters with optional tracing off. A hitch captures one
second afterward and writes Chrome trace JSON with a one-line slowest-span
summary. Dumps are separated by at least ten seconds. The walkthrough and
headless Godot use `user://hitches`. Headless Rust uses the same Linux
application-data path, or `SOE_HITCH_DIR`. See [trace.md](trace.md) for
capacity, output and shutdown behavior.

The release calibration limit is fixed at an on/off ratio below 1.01:

```text
rtk cargo test -p soe-trace --release --test prf_18_hitches prf_18_recorder_stays_inside_one_percent -- --ignored --nocapture
rtk cargo test -p soe-game --release --test it prf_12_accumulator_under_one_percent -- --ignored --nocapture
```

`prf_18_slow_synthetic_tick_dumps_prior_frames_with_optional_tracing_off`
forces a slow tick and verifies earlier frames and the one-second tail.
The `prf_18_` unit tests check the bounded ring, exact threshold, pinned
history, cooldown, counter export, one-line summary and I/O errors. After
the release extension build and import, the host acceptance script runs
without installed game records:

```text
rtk proxy "$SOE_GODOT_BIN" --path godot --resolution 640x360 --script prf_18_hitches_test.gd
```

`crates/godot-host/tests/prf_18_hitches_host.rs` runs the same script headless
in an isolated project when SKYRIM_DATA and SOE_GODOT_BIN are set, so a stale
script fails `cargo test` instead of only a hand run.

### PRF-18 validation, 2026-10-05

The uncommitted candidate based on `a69bee42` passed 23 ordinary
`soe-trace` tests. On the Ryzen 9 7900X with 24 logical CPUs and 61 GiB RAM,
the release synthetic on/off ratio was 1.008173. The 1,000-actor PRF-12
calibration ratio was 0.989503 and its suspended replay produced identical
actor positions. Both paired medians were below the declared 1.01 limit.

Godot 4.4.1 passed the headless and windowed host checks. The windowed run
used Forward+ with llvmpipe, LLVM 22.1.8, at 640 by 360. An 80 ms scripted
pause produced a 96.822 ms headless frame and a 96.672 ms windowed frame.
Both dumps retained the five-second preceding window and one-second tail,
contained counter events, matched the slowest-span summary and reported zero
overwritten events. Both processes exited zero. These are recorder
correctness checks.

## Tracy and editor profiling, PRF-23

The optional `soe-trace/tracy` feature streams `Span` scopes to a running Tracy
viewer, plots the ten `WorkCounts` metrics, and marks completed system frames.
It is disabled by default. The client captures on demand and advertises only
on the local host. Inspect its dependency tree with:

```text
rtk cargo tree -p soe-trace --features tracy
```

Build the Godot extension with `cargo build -p soe-godot-host --release
--features tracy` to enable Tracy in a host run.

The Godot extension registers an `EngineProfiler` named `soe_systems`.
When enabled by the editor debugger, it receives a frame duration and
fourteen system duration pairs in seconds for every PlayerNode presentation
frame. The current editor script verifies registration. It does not verify
that Godot displays the custom values in its built-in Profiler tab.
The feature adapter test is
`prf_23_tracy_adapter_exports_scopes_counters_and_frame_marks`. After building
the release extension and importing the project, run the editor registration
check:

```text
rtk proxy "$SOE_GODOT_BIN" --editor --path godot --script prf_23_profiler_test.gd
```

## Fixed installed-data CPU suite, PRF-16

`tools/perf-budget.sh` builds the release extension and benchmark, imports
the Godot project and runs three isolated headless scenarios. The first
starts the actual walkthrough in WhiterunBanneredMare, warms navigation and
120 fixed steps, then measures 3,600 steps with NPC packages for 60 seconds.
The second starts the same walkthrough, walks toward an unlocked authored
load door from its return marker, activates it through Game and replaces the
host collision and geometry with WhiterunWorld. It measures the approach,
the crossing and 3,600 exterior steps. Physical residency at the fresh city
exit includes the player and one NPC after PRF-31. The walk requires both
actor bodies and package evaluations during the exterior steps.
The third drives PRF-07's 1,000 travel
and sandbox actors on their record-backed NAVM for 3,600 steps after ten
warm-up steps. Each scenario paces at 60 steps per second and retains every
CPU sample. TST-18's installed walkthrough workload is the first two scenarios.

The frame is CPU wall time inside a fixed simulation step, including door
activation on the crossing step. Pacing, setup, telemetry and report writing
are excluded. PRF-12's exclusive slots supply per-system p50, p99 and worst
over the whole measured run, including idle zero samples. Headless rendering
slots may be zero. These runs establish CPU costs, not rendering throughput.
The thresholds below were fixed before the first PRF-16 run. Every reported
statistic must meet its corresponding limit, including the overall frame.

<!-- PRF-16 budgets -->

| System | p50 ms | p99 ms | Worst ms |
|---|---:|---:|---:|
| frame | 8 | 16.7 | 33 |
| papyrus_vm | 2 | 4 | 8 |
| ai_planning | 2 | 4 | 8 |
| ai_movement | 2 | 4 | 8 |
| combat | 1 | 2 | 4 |
| magic | 1 | 2 | 4 |
| quests_story_manager | 1 | 2 | 4 |
| dialogue | 1 | 2 | 4 |
| physics_sync | 2 | 4 | 8 |
| cell_streaming | 2 | 8 | 33 |
| audio | 1 | 2 | 4 |
| save | 2 | 8 | 33 |
| input | 1 | 2 | 4 |
| host_render_submission | 2 | 4 | 8 |
| other | 2 | 4 | 8 |

<!-- /PRF-16 budgets -->

```text
rtk proxy env SKYRIM_DATA=/path/to/Data tools/perf-budget.sh
rtk proxy env SKYRIM_DATA=/path/to/Data tools/perf-budget.sh --sweep
rtk proxy env SKYRIM_DATA=/path/to/Data python3 tools/performance.py final-sweep --out target/final-budget
```

Normal mode returns 2 for a budget breach, 1 for incomplete or invalid
execution, and 0 when every scenario passes. It runs all scenarios even after
a failure. `tools/performance.py final-sweep` calls this suite with `--sweep`.
It returns zero after recording failures so a group can finish. Both modes write raw logs,
`report.json` and `follow-up-rows.csv` under `target/perf-budget`. The latter
contains one complete new backlog row per failed scenario, using the next
unused PRF ids. The orchestrator imports those rows into BACKLOG.csv after
reconciling ids with any concurrent additions. The runner does not edit the
backlog. `--no-build` as the first argument reuses previously built binaries,
while still importing the project before launching the scenarios.

Sources: the local Godotwind benchmarking manual for fixed routes, separate
setup and full-run statistics, xEdit's REFR XTEL for the authored return
marker, this repository's PRF-12 timing API, and the parent project's
`.claude/skills/soe-perf/SKILL.md`. The current timing contract supersedes
that skill's historical process-monitor wording.

## PRF-16 installed run, 2026-10-05

Uncommitted PRF-16 changes on `draft4/PRF-16`, based on `a69bee42`.
Release builds on the AMD Ryzen 9 7900X, 24 logical CPUs and 61.9 GiB
RAM, using stock Godot 4.4.1 headless. These CPU checks perform no GPU
rendering. The load order contains the five installed Special Edition
masters. No compiler or other check launched by this agent overlapped
the measured scenarios.

| Scenario | Samples | Game ticks | Simulation seconds | Wall seconds | NPCs | Evaluations |
|---|---:|---:|---:|---:|---:|---:|
| bannered-mare | 3600 | 3600 | 60.000 | 60.069 | 7 | 12412 |
| walk-out | 3624 | 3624 | 60.400 | 61.119 | 26 | 14489 |
| thousand-actors | 3600 | 3600 | 60.000 | 60.000 | 1000 | 14440 |

| Scenario | System | p50 ms | p99 ms | Worst ms |
|---|---|---:|---:|---:|
| bannered-mare | frame | 3.089889 | 4.648324 | 104.465270 |
| bannered-mare | ai_movement | 0.829303 | 1.237961 | 1.718361 |
| bannered-mare | ai_planning | 1.096298 | 1.832864 | 5.068457 |
| bannered-mare | audio | 0.000090 | 0.000311 | 0.004729 |
| bannered-mare | cell_streaming | 0.002495 | 0.130498 | 0.359499 |
| bannered-mare | combat | 0.002544 | 0.004390 | 0.015118 |
| bannered-mare | dialogue | 0.060515 | 0.096082 | 1.481378 |
| bannered-mare | host_render_submission | 0.333993 | 0.527116 | 2.374088 |
| bannered-mare | input | 0.016982 | 0.030678 | 0.259441 |
| bannered-mare | magic | 0.448046 | 0.824949 | 1.552254 |
| bannered-mare | other | 0.049963 | 0.082946 | 1.032108 |
| bannered-mare | papyrus_vm | 0.096683 | 0.158129 | 2.479175 |
| bannered-mare | physics_sync | 0.112193 | 0.164040 | 0.251706 |
| bannered-mare | quests_story_manager | 0.011583 | 0.023656 | 100.462013 |
| bannered-mare | save | 0.000000 | 0.000000 | 0.000000 |
| walk-out | frame | 5.178666 | 7.577209 | 680.799649 |
| walk-out | ai_movement | 1.028892 | 1.579179 | 4.816025 |
| walk-out | ai_planning | 2.395355 | 3.569947 | 19.459391 |
| walk-out | audio | 0.000090 | 0.000221 | 0.000642 |
| walk-out | cell_streaming | 0.004137 | 0.521446 | 461.143531 |
| walk-out | combat | 0.015866 | 0.025094 | 0.626454 |
| walk-out | dialogue | 0.103426 | 0.155835 | 1.161656 |
| walk-out | host_render_submission | 0.724226 | 1.160052 | 203.558696 |
| walk-out | input | 0.017544 | 0.031599 | 0.050525 |
| walk-out | magic | 0.556332 | 1.007745 | 2.247467 |
| walk-out | other | 0.038083 | 0.066496 | 2.443009 |
| walk-out | papyrus_vm | 0.101600 | 0.160081 | 2.145214 |
| walk-out | physics_sync | 0.120688 | 0.185711 | 6.853982 |
| walk-out | quests_story_manager | 0.012055 | 0.024517 | 103.755979 |
| walk-out | save | 0.000000 | 0.000000 | 0.000000 |
| thousand-actors | frame | 3.557162 | 7.269708 | 11.816029 |
| thousand-actors | papyrus_vm | 0.004318 | 0.006843 | 0.023554 |
| thousand-actors | ai_planning | 0.009165 | 0.049494 | 0.914478 |
| thousand-actors | ai_movement | 2.905501 | 5.810900 | 9.936738 |
| thousand-actors | combat | 0.000000 | 0.000000 | 0.000000 |
| thousand-actors | magic | 0.074491 | 0.141528 | 0.441815 |
| thousand-actors | quests_story_manager | 0.000390 | 0.000811 | 0.011631 |
| thousand-actors | dialogue | 0.351975 | 0.882028 | 3.321977 |
| thousand-actors | physics_sync | 0.000000 | 0.000000 | 0.000000 |
| thousand-actors | cell_streaming | 0.000000 | 0.000000 | 0.000000 |
| thousand-actors | audio | 0.000000 | 0.000000 | 0.000000 |
| thousand-actors | save | 0.000000 | 0.000000 | 0.000000 |
| thousand-actors | input | 0.000000 | 0.000000 | 0.000000 |
| thousand-actors | host_render_submission | 0.000000 | 0.000000 | 0.000000 |
| thousand-actors | other | 0.174651 | 0.409134 | 4.366021 |

Every scenario completed and breached at least one fixed budget. The
sweep recorded proposed rows PRF-34, PRF-35 and PRF-36 without changing
BACKLOG.csv. Raw logs, the complete report and the rows are in
`target/prf-16-installed-test`. The gated
`prf_16_installed_sweep_runs_fixed_scenarios_and_records_budget_breaches`
test passed. The earlier report in `target/prf-16-before-tick-fix` records
the failing doubled Game ticks before removing the closed-menu tick.

No vanilla-runtime timing comparison or rendering acceptance was run.

## Machine

- AMD Ryzen 9 7900X, 12 cores, 24 threads, up to 5.7 GHz
- 61 GB RAM, the game data on the page cache after the first run
- NVIDIA GeForce RTX 5070 Ti, Vulkan 1.4, Godot 4.4.1 Forward+
- Fedora Atomic, release build, the installed Skyrim SE under Steam

## Startup quest candidate index, PRF-33, 2026-10-04

`crates/game/examples/startup_timing.rs` adapts the lead's 2026-10-03 host
harness from the external `crash-1004-1945/wt2.patch` run log. It times opening
the five masters and script archives, constructing Game, reading and
registering PEX, binding,
each start-game-enabled quest and script settling. It opens the script VFS
directly so this game-crate example needs no host dependency or rendering
assets. Its opening time is not a `DataSource::open` measurement.

The comparison ran sequentially in this worktree with the release profile,
fat LTO and `--trace`, before and after the candidate index change. Both runs
loaded 14,026 scripts, bound 18,225 attachments and attempted 423 startup
quests, of which 421 started. The two required-alias failures stayed failures.
The lead's earlier play-session overlay had 14,302 scripts and recorded
12.8 to 14.0 seconds starting quests. The table below is this comparison's
own before and after pair, with times in milliseconds.

| Measurement | Before | After |
|---|---:|---:|
| Startup quest loop, total | 15,370.476 | 4.423 |
| Per quest, median | 36.218251 | 0.001993 |
| Per quest, nearest-rank p99 | 39.872904 | 0.017263 |
| Per quest, maximum | 42.742893 | 0.373156 |
| Candidate construction, call count | 423 | 1 |
| Candidate construction, total | 15,360.197 | 0.366 |
| FreeformRiften07 | 36.435211 | 0.002184 |
| PlayerVampireQuest | 37.134945 | 0.000732 |
| CreatureDialogueHorse | 36.283464 | 0.000962 |
| DialogueSolitude | 36.351603 | 0.006001 |

Phase spans inside `World::start_quest` identified candidate construction as
the repeated cost. The old path collected all winning ACHR and REFR records,
sorted and deduplicated them on every attempt, including quests without a
searched alias. The replacement skips construction for those quests and
derives one bounded index from `LoadOrder::winners`, whose identities are
already unique and sorted. It caches either the complete candidate set or
the fact that there are more than 4,096 candidates. A partial set is never
evaluated. Conditions, live eligibility and alias reuse still run at each
start. The cache adds no saved field.

All World hash fields matched before and after the change, both immediately
after startup and after `run_until_idle(64)`. This includes alias assignments,
stages, quest flags and queued events. The quest hash before script settling
was `12244036582383919460`, and after settling it was
`16996129545838252704`, equal in the two processes. Script settling took
280.275 ms before and 94.298 ms after. That phase was not changed, and these
single-process samples do not establish a script-execution speedup.

`prf_33_installed_startup_quests_finish_within_release_budget` loads scripts
and binds before timing. Its timed loop includes quest state construction
and the first candidate lookup. It excludes data opening, script loading,
binding and script settling. The 1.5 second release limit was fixed before
the first test run. With tracing off, the release case started the same 421
quests in 3.604097 ms, with a 1.383 microsecond median and a 301.119 microsecond
maximum. Debug builds check the startup result without enforcing a release
timing limit. These measurements cover this machine and installed cohort,
not frame time or complete quest execution.

The final release rerun passed all seven PRF-33 tests and measured 3.817829 ms
for the same 423 attempts and 421 starts. Existing quest regressions and 73
additional alias cases passed. `cargo fmt --all` and
`cargo clippy -p soe-game --all-targets -- -D warnings` exited zero.

The alias fields and startup-stage flags follow local
`TES5Edit/Core/wbDefinitionsTES5.pas` and `uesp-modfileformat/QUST.wikitext`.
`ck-wiki/Quest_Script.wikitext` supplies the quest state query contracts.
PRF-33 reuses the existing quest interpreter and load-order winner iterator
rather than adding another record or alias model.

```text
SKYRIM_DATA="<Data>" cargo run --release -p soe-game --example startup_timing -- --trace
SKYRIM_DATA="<Data>" cargo test --release -p soe-game --lib --test it prf_33_ -- --nocapture
```

## PRF-11 navigation baseline, 2026-10-03

Before changing navigation, `prf_11_installed_bannered_mare_and_whiterun_path_timings`
loaded the five installed masters and queried the first and last triangle
centroids of the largest mesh in each graph. These are single release queries,
with a first query followed by a warm query, rather than frame measurements.

| Graph | Meshes | Triangles | Route nodes | First | Warm |
|---|---:|---:|---:|---:|---:|
| WhiterunBanneredMare | 1 | 318 | 30 | 38.102 us | 28.423 us |
| WhiterunWorld | 44 | 4,705 | 55 | 102.924 us | 68.509 us |

Bannered Mare endpoints were `[-250.43564, 477.0016, 70.85016]` and
`[580.056, -261.84796, 67.956154]`, cost 1914.888. WhiterunWorld endpoints
were `[20497.994, -7813.914, -3635.1064]` and
`[21203.918, -4165.7085, -2998.261]`, cost 2723.3533.
`find_path_between` resolves each endpoint once. Its A* expansion loop does
not call `locate`. The package driver can issue repeated path requests.

## TST-16 game integration test binary, 2026-10-01

The change recorded 440 `soe-game` integration test executables before
the change and one `it` executable after it. On the same machine, warm
`cargo test -p soe-game --no-run` took 341.869 seconds before and 131.178
seconds after. These measurements come from the TST-16 run at commit
`0dfc2cae`, whose temporary build records were removed after the comparison.
The before run had warmed dependencies before timing the test build. The
after run built the consolidated integration test target.

The committed `tst_16_test_names_match_the_pre_consolidation_inventory` test
compares all 2,289 before and after names after removing the `unit::` prefix
and TST-16 guard tests. The retained pre-change installed run
stopped with a failure, and targeted retries recorded two more failed test
cases. The consolidated installed capture passed, so the available records do
not establish matching installed results test for test.

## Parsed plugin cache, 2026-09-29

`crates/esm/tests/fmt_29_cache.rs::fmt_29_installed_skyrim_master_cache_hit_matches_and_times_second_load`
loaded the installed `Skyrim.esm` through `read_plugin_cached` twice with an
empty cache directory before the first call. In the test profile, the first
load parsed and wrote the entry in 15.226 seconds. The second load read and
decoded the cached plugin in 10.743 seconds. Both returned plugins equaled a
fresh parse. The same test checked cache equality for Update.esm, Dawnguard.esm,
HearthFires.esm and Dragonborn.esm. This is one run, including the cache write
on the first load. It is not a release benchmark.

`soe-bench open-data` opens the installed data folder with its archives and
reports the parsed-plugin cache cold and warm separately. On 2026-09-29, a
release run with 24 threads and three samples measured a 4.713 s cold median
(4.596 s minimum) and a 3.173 s warm median (3.091 s minimum). Each cold sample
started with an empty cache directory. The warm samples reused the entries
from the cold run. For comparison, the 2026-09-17 open-data result below was
9.18 s median before the parsed-plugin cache integration.

## How to run

```text
SKYRIM_DATA="<Data>" cargo run -p soe-bench --release -- [--runs N] [--out DIR] <subcommand>
```

Subcommands: `load-order`, `open-data`, `archives`, `cell <editor id>`, `cell-index <editor
id>`, `exterior <editor id>`, `scripts`, `navmesh`, `save`, `overhead <editor
id>`, `trace <subcommand
...>` (writes `soe-bench-trace-<name>.json` for Perfetto or `chrome://tracing`
and a `.txt` summary next to it), `all`. Each measurement prints as
`name value unit`, timings as the median and the minimum of `--runs`
(default 3). The Godot frame benchmark:

`soe-bench save <file.ess>` instead parses one existing Special Edition save
and reports parse timing, file and global-table sizes, Papyrus block size and
counts, and exact change-form bytes by plugin and record type. The plugin
rows include created, invalid and unresolved form buckets. Plugin names can
hold spaces, so a plugin is always the last field of a row.

- `save_section <name> <bytes>` and `save_section_time <name> <ns>` for
  `container` (the uncompressed header block, whose time includes inflating
  the body), `plugin_info`, `location_table`, `form_id_array`,
  `visited_worldspaces`, `table_1`, `table_2`, `change_forms` and `table_3`,
  plus `papyrus_decode` and `papyrus_attribution` times. The body sections
  tile the body, so `save_section_bytes` equals the body length and
  `save_section_unaccounted_bytes` is 0. `save_section_load_ns` is the sum of
  the section times. Times are from the last of the `--runs` parses.
- `save_plugin_load <forms> <ns> <plugin>` is the decode time of each change
  form, summed by the plugin of the form. `save_plugin_load_ns` is their sum.
  `save_plugin_section <section> <entries> <bytes> <plugin>` splits
  `change_forms` and `form_id_array` by plugin.
- `save_section_save_ns <name> <ns>` reports the ESS encode time for every
  body section. Native SOEG saves report the `world`, `papyrus_vm` and
  `envelope` section sizes and write times, including when the input argument
  is a native save. `save_section_load_ns <name> <ns>` reports their native
  validation and restore times when loading a native input.
- `save_papyrus_script <script> <instances> <bytes> <plugin>` is the variable
  data of a script's instances, grouped by the plugin of the form each instance
  runs on. An ESS does not record which plugin ships a script, so the owner is
  the plugin of the form it is attached to. `save_papyrus_script_plugin
  <instances> <bytes> <plugin>` totals the same rows, `save_papyrus_script_bytes`
  is the grand total and `save_papyrus_unattributed_bytes` is the rest of the
  value data, which is reference and array data.

A native SOEG file argument reports full load timing and file size.
With no file argument, `save` retains the generated native-world save and load
measurement.

```text
SKYRIM_DATA="<Data>" godot --path godot --script bench.gd
```

The first run of a fresh checkout needs `godot --headless --path godot --import`
once, see `docs/godot-host.md`.

## PRF-08 first-use rendering

The 2026-09-23 exterior log recorded a 69.089 ms worst frame on run one,
but did not record its frame number or pipeline counters. The code path left
first-use pipeline compilation outside streaming: `add_instance`,
`add_multimesh` and `lod_instance` uploaded geometry without a surface material,
then assigned an instance material override. The benchmark created its camera
on its first process frame and waited ten frames without a completion check.
Godot identifies instance overrides as unavailable to mesh-stage precompilation.
Material combinations and view-dependent rendering were first exercised after
the load call. Texture decode completes inside `stream_exterior_each` and
`add_cell` uploads textures before returning. The benchmark does not request
collision or assign a streaming focus, so neither collision construction nor
cell streaming runs on its first exterior process frame.

PRF-08 binds bulk materials to mesh surfaces before instancing and preserves
them across placement rebakes. Benchmark and walkthrough loading draw their
configured camera and lights behind a cover, synchronize submitted GPU work,
and wait for three drawn frames and 500 ms with unchanged pipeline counters.
An unsettled load fails at 30 seconds. The covered frames warm first-use
pipelines and uploads before the cover is removed. The helper reports loading
time, its worst frame and compilation counts independently of measurement.
It uses stock Godot 4.4 APIs. The default disk shader cache stays enabled.

This names and removes the deferred first-use path found in the source.
It does not establish that compilation caused the historical 69.089 ms frame,
because that log lacks a frame trace. The windowed rerun must verify the fix.
The helper observes quiet counters, not pending background compiler jobs.
Specialization can resume later. No headless timing is rendering evidence.

Build and import the extension as described in [godot-host.md](godot-host.md),
then run these three windowed exterior radius 1 starts with one command:

```text
rtk proxy env SKYRIM_DATA="/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data" SOE_BENCH_CELL=WhiterunOrigin SOE_BENCH_RADIUS=1 SOE_BENCH_BATCH=1 SOE_BENCH_OCCLUDERS=1 SOE_BENCH_SECONDS=5 SOE_BENCH_P99_MS=16.7 SOE_BENCH_WORST_MS=33 SOE_BENCH_SOAK=0 SOE_BENCH_CAPTURE= XDG_DATA_HOME="$PWD/target/prf-08-user" /var/home/pengolodh/Data/Projects/Programming/skyrim-modclients/tools/godot/Godot_v4.7.2-stable_linux.x86_64 --headless --path godot --script bench.gd -- --cold-starts 3
```

The launcher is headless, while each child uses a window and the project's
renderer with the same executable. Each child opens data and builds the scene
in a fresh process, measures at least 600 frames and five seconds, prints
`godot_run N worst X ms frame F` and a `SOE_SAMPLE` row, then exits before
the next launch. Frame indices start at one for measurement. Loading frame
indices are separate. Capture and reporting are outside measured intervals.
The launcher runs every requested child even after a breach. Exit 2 preserves
any frame-budget failure, exit 1 preserves any invalid run, and exit 0 requires
all children to pass. A soak cannot be combined with cold-start mode.

Cold means fresh process caches, as in performance-limits.md. Disk shader,
driver and operating-system file caches are retained. This command does not
delete the user's caches. `pipelines_before_render`, `pipelines_after_load`
and `pipelines_during_measurement` distinguish load-time compilation from
later compilation. Compare these with `load_render_worst_ms` and the measured
worst frame. Thresholds remain 16.7 ms p99 and 33 ms worst.
The executable acceptance check is
`prf_08_three_windowed_cold_whiterun_runs_under_33_ms`, gated by
`SKYRIM_DATA`, `SOE_GODOT` and `SOE_PRF_08_WINDOWED=1`.
Windowed acceptance is left to the lead in this sandbox.

Local validation on 2026-10-02: the deterministic loading and reporting checks
pass on stock Godot 4.4.1 and 4.7.2. The installed-data headless material check
passes on 4.4.1 with nine cells, 287 batched instances and 805 unbatched
instances, all with surface materials before drawing. Clearing releases both
sets. These are correctness results, with no measured rendering throughput.

References: local Godot pipeline, RenderingServer and ProjectSettings docs,
checked against the [Godot 4.4 pipeline guide](https://docs.godotengine.org/en/4.4/tutorials/performance/pipeline_compilations.html)
and [shader cache setting](https://docs.godotengine.org/en/4.4/classes/class_projectsettings.html#class-projectsettings-property-rendering-shader-compiler-shader-cache-enabled).
The `soe-perf` skill's old frame-monitor description is superseded by this
repository's wall-clock measurement contract.

The PLY-04 windowed route smoke run uses installed game data and writes a PNG
for every named checkpoint plus a JSON frame log with the worst sampled wall
frame for each step:

```text
SKYRIM_DATA="<Data>" godot --path godot --resolution 640x360 --script ply_04_route_test.gd
```

`SOE_PLY_04_ROUTE` selects a route JSON file and `SOE_PLY_04_OUTPUT_DIR`
selects the output directory. The default route starts in
`WhiterunBanneredMare`, executes one wait command, and captures that
checkpoint. A zero process status means all route commands ran and all
checkpoints and the frame log were saved.

## PRF-15 core operation baselines

`tools/perf.sh` runs the Divan target `crates/bench/benches/prf_15.rs` in
release mode with 30 samples and a 100 ms minimum per benchmark. Short
operations run in fixed batches, and the stored values divide each batch
median by its operation count. It measures form lookup, winning-override
resolution, navmesh locate and path search, a Papyrus call and native call,
NIF parsing, BC7 DDS decode and a BSA file read. The fixtures are generated
in memory. If
`SKYRIM_DATA` is set, the target also parses an installed NIF and reads the
corresponding file from a mesh archive under the data directory.

The current medians live in `docs/perf-baselines.json`, in nanoseconds, tagged
with host, CPU model, architecture and data mode. Run `tools/perf.sh` to
compare the nine synthetic operation medians on the same machine. When
`SKYRIM_DATA` is set, its two installed asset medians are stored and compared
as well. It exits nonzero when an operation is more than 15 percent slower or a
measurement is missing, or when the baseline belongs to another CPU identity.
After reviewing a measured change, run `tools/perf.sh refresh` to replace the
medians for the current CPU. A refresh writes the current host and CPU
identity into the baseline. Divan 0.1.21 is licensed MIT or Apache-2.0.
`cargo tree -p soe-bench -i divan` shows its dependency
tree. `cargo tree -p divan` includes cfg-if, clap, condtype, divan-macros,
libc and regex-lite. Clap also brings terminal_size and rustix. The benchmark
API and its median table follow the
[Divan 0.1.21 documentation](https://docs.rs/divan/0.1.21/divan/).

## Run of 2026-09-17, branch `perf`

### Top level

| Measurement | Value |
|---|---|
| Load order, five masters, 1 thread | 2.15 s median, 1.67 s min, 545k forms/s |
| Load order, five masters, 24 threads | 1.42 s median, 1.13 s min, 824k forms/s, 1.51x |
| Forms in the table | 1,168,671 |
| Archive index, 23 archives, 172,914 files | 11.1 s median, 8.5 s min, 15.6k files/s |
| Archive read, 1,000 random files, 133 MB | 23 ms, 5.7 GB/s from the page cache |
| Open data (masters plus archives), 1 thread | 9.18 s median |
| Open data, 24 threads for the masters | 6.62 s median, 5.34 s min |
| Stream WhiterunBanneredMare, 1 thread | 0.442 s median, 282k triangles/s |
| Stream WhiterunBanneredMare, 24 threads | 0.400 s median, 312k triangles/s, 1.10x |
| Bannered Mare content | 690 references, 318 statics, 124,924 triangles, 763 surfaces |
| Stream the 3 by 3 exterior block around WhiterunOrigin, 1 thread | 3.28 s, 87k triangles/s |
| Same, 24 threads | 3.30 s, 0.99x |
| Exterior block content | 9 cells, 957 references, 242 meshes, 285,187 triangles |
| Scripts from `Skyrim - Misc.bsa` | 14,026 loaded, 0 failed, 1.83 s including the load order |
| Bind | 28,143 instances, 71,318 properties, 0.137 s |
| Start | 423 quests started, 0.044 s |
| VM hot loop, one thread by contract | 12.7M instructions/s (800k instructions in 63 ms) |
| Save state after bind | 27 ms, 12.97 MB |
| Load state | 72 ms |
| Navmesh graph, Tamriel | 13,011 meshes, 2,271,709 triangles, built in 0.149 s |
| Navmesh local queries, both ends in one mesh | 80.6k queries/s, 1,000 of 1,000 found |
| Navmesh province queries, random pairs across Tamriel | 3.7 queries/s, 40 of 100 found |

Godot, Bannered Mare through the real data path, 300 frames after 10 warm-up frames:

| Measurement | Headless | Windowed, vsync off |
|---|---|---|
| load_data | 6.55 s | 5.79 s |
| stream_cell | 0.455 s | 0.399 s |
| Mesh nodes built | 763 | 763 |
| Objects drawn per frame | 0 | 763 |
| Primitives drawn per frame | 0 | 124,924 |
| Process step, median | 0.03 ms | 56.1 ms |
| Physics step, median | 0.02 ms | 0.22 ms |
| Render setup CPU, median | 0 | 4.0 ms |

Headless has no renderer, so its process time is the scene tree alone. The
windowed frame is 56 ms with 763 separate `MeshInstance3D` nodes and one
material each. That is the first rendering number the project has and it is
poor for a single interior: the surfaces are unbatched, so the next step on
the host is merging static surfaces per material into one mesh or a
`MultiMesh`, and measuring again.

### Where the time goes

From `soe-bench trace cell WhiterunBanneredMare` (16,617 events over the
3 plus 3 open and stream runs):

| Span | Count | Total ms | Mean | Note |
|---|---|---|---|---|
| host.open | 6 | 45,960 | 7.66 s | opening the data folder |
| host.index_archives | 6 | 35,264 | 5.88 s | 77 percent of open |
| bsa.archive_open | 102 | 34,980 | 343 ms | 17 archives per open, the two texture archives take over a second each |
| esm.plugin_parse | 30 | 11,288 | 376 ms | Skyrim.esm 1.1 to 1.3 s, the others under 150 ms |
| host.parse_masters | 6 | 9,548 | 1.59 s | the threaded parse, bounded by Skyrim.esm |
| esm.load_order_add | 30 | 1,147 | 38 ms | form table insert |
| host.stream_cell | 6 | 2,221 | 370 ms | |
| host.scan_references | 6 | 2,164 | 361 ms | 97 percent of stream: a linear pass over every REFR in the load order |
| host.build_all | 6 | 56 | 9.4 ms | NIF parse plus mesh build, all 318 statics |
| host.static | 1,908 | 125 | 66 us | per static, across the workers |
| nif.parse | 1,908 | 59 | 31 us | 47 MB of NIF bytes |
| host.build_surface | 4,578 | 24 | 5 us | |
| bsa.vfs_read | 1,908 | 29 | 15 us | |

Two conclusions follow. First, streaming a cell is not the mesh work at
all: 361 of 370 ms is `scan_references` walking the whole reference table
to find the 690 references in one cell. An index from cell to its references,
built once at open, turns that into a lookup, after which the threaded mesh
path (9 ms for the whole cell) is what remains. Second, opening the data
folder is the archive index, and inside it the texture archives. The bench
does not need textures, and a lazy archive index (open on first read) would
cut open from 6.6 s to about 1.5 s.

From `soe-bench trace scripts` (105,313 events):

| Span | Count | Total ms | Note |
|---|---|---|---|
| esm.plugin_parse | 20 | 7,357 | four load orders for the four runs |
| esm.load_order_add | 20 | 705 | |
| vm.run | 256 | 148 | 2.4M instructions over start plus the hot loop |
| vm.slice | 37,961 | 140 | 3.7 us per 64-instruction slice |
| game.bind | 1 | 93 | |
| game.bind_form | 23,443 | 89 | 3.8 us per form with scripts |
| bsa.vfs_read | 42,078 | 21 | 0.5 us per script read |
| game.pump | 1 | 0.5 | 611 queued events and calls after start |
| native calls | 1,183 | 0.06 | every one named `<class>.None` |

The native rows are a finding, not a measurement: during `start` the VM
reaches `call_native` 1,183 times with the function name `None` and walks
the whole class chain each time (script names appear as classes), which
means a method-call operand decodes to `None` somewhere in the start path,
most likely `CallParent` or a call on a `None` object. No real native by
name shows up in the trace at all, so the 423 quest starts ran their
`OnInit` bodies without any native reaching the game crate. That is a VM
bug the trace exposed, filed in `docs/papyrus.md` under known gaps and left
for a VM change, since this branch adds spans only.

### Threads

| Phase | 1 thread | 24 threads | Speedup | Verdict |
|---|---|---|---|---|
| Parse the five masters | 2.15 s | 1.42 s | 1.51x | pays, bounded by Skyrim.esm at 1.1 s on one thread |
| Bannered Mare, parse and build | 0.442 s | 0.400 s | 1.10x | the mesh work is 9 ms, the scan is single threaded |
| Exterior 3 by 3 block | 3.28 s | 3.30 s | 0.99x | same reason, nine scans of the reference table |
| VM | one thread | one thread | | by contract, scripts see one scheduler |

The threaded paths are `std::thread::scope` over chunked work lists, no
runtime. Threads only pay where the trace shows the time, and for cell
streaming it shows it in the scan, so the index comes first.

### Tracing overhead

Bannered Mare streamed with the switch off and on, 3 runs each, 24 threads:

| | Off | On | Ratio |
|---|---|---|---|
| Stream median | 0.374 s | 0.368 s | 0.98x |
| Events per run | 0 | 2,751 | below the run-to-run noise |

A span costs two `Instant::now` calls and one push into the global list
under its mutex. At 2,751 events per cell the difference is inside the
noise between runs. A first design buffered per thread and flushed on
thread exit, which measured 1.10x and lost the tail of a scoped worker's
events when the join returned before the thread-local destructor ran, so
the global list stayed. At this checkpoint a disabled span cost one relaxed
atomic load. PRF-18 adds the always-on bounded ring, so the switch now controls
only the optional event list. Named spans retain their timing and payloads
with that switch off, using category names for dynamic spans.

## Run of 2026-09-18, branch `batching`

Same machine, release build without the `lto = "fat"` and `codegen-units = 1`
profile `master` gained the same day, so every before and after pair here
was measured inside this worktree. The branch changes the Godot layer
(batching, `RenderingServer` instances, compressed texture upload), the
material path (a texture cache on `DataSource`, reads on the workers, no
BCn decode) and adds exterior streaming with terrain. The design is in
`docs/godot-host.md`, the section of the same date.

### A correction to the 2026-09-17 frame numbers

The 56 ms "process step" for the Bannered Mare above was
`Performance.TIME_PROCESS`, which this run showed reading a constant 80 to
113 ms across every frame of a scene that the wall clock draws in half a
millisecond. It is not a per-frame number. `godot/bench.gd` now records the
wall clock between two `_process` calls and the viewport's measured GPU and
CPU render time, over at least five seconds and 600 frames, as p50, p99,
p99.9 and worst. The 763 unbatched nodes were never the frame's cost.
"Where the time goes" above stands, its Godot row does not.

### Frame time, windowed, 1600 by 900, vsync off, X11 display driver

| Scene | Instances | Frame p50 | p99 | p99.9 | Worst | GPU p50 | Render CPU p50 | Frames/s |
|---|---|---|---|---|---|---|---|---|
| Bannered Mare, one instance per surface | 763 | 0.480 ms | 0.950 | 1.492 | 5.651 | 0.081 ms | 0.177 ms | 1,996 |
| Bannered Mare, batched per material | 48 | 0.235 ms | 0.397 | 0.903 | 2.282 | 0.062 ms | 0.039 ms | 4,004 |
| Whiterun 3 by 3 (9 cells, 303,619 triangles), per surface | 802 | 0.758 ms | 1.934 | 2.749 | 3.747 | 0.093 ms | 0.280 ms | 1,098 |
| Whiterun 3 by 3, batched | 195 | 0.288 ms | 0.519 | 1.045 | 1.698 | 0.087 ms | 0.066 ms | 3,284 |
| Whiterun 5 by 5 (25 cells, 624,408 triangles), per surface | 1,499 | 0.889 ms | 1.782 | 3.013 | 7.567 | 0.156 ms | 0.454 ms | 1,058 |
| Whiterun 5 by 5, batched | 390 | 0.359 ms | 0.686 | 1.319 | 3.191 | 0.142 ms | 0.105 ms | 2,713 |
| WhiterunWorld, every cell (59, 825,587 triangles), batched | 563 | 0.440 ms | 0.935 | 1.576 | 2.246 | 0.171 ms | 0.147 ms | 2,090 |

Draw calls equal instances. The frame wall clock is presentation bound at
these sizes (Xwayland), the GPU and render CPU rows are the scene. Batching
buys a 4x on render CPU and about 2x on the frame. The GPU row grows with
triangles, 0.06 ms per 125k to 0.17 ms per 825k, and would need about five
million triangles in view to reach one millisecond on this GPU. The
`Performance.TIME_PROCESS` row is gone.

Godot 4.7.2 on the same 3 by 3 block, batched: frame p50 0.276 ms, p99
0.498, p99.9 1.071, worst 1.677, GPU 0.084 ms, render CPU 0.063 ms on the
single-safe model, and 0.281, 0.492, 1.110, 1.849, 0.084, 0.064 on the
separate render thread. Unbatched on 4.7.2: 0.517, 1.193, 1.675, 22.399,
0.091, 0.196. Batched, the binaries are within noise of each other,
unbatched 4.7.2 has the lower median and the session's one 22 ms hitch, see
`docs/godot-host.md`.

Against the bar `ENGINE-PLAN.md` set for the 3 by 3 block (p99 at or under
16.7 ms, worst hitch under 33 ms, a cell streamed in under 250 ms on
workers): the frame criteria pass at 0.519 ms and 1.698 ms batched (1.934
and 3.747 unbatched), the stream criterion fails on this branch at 406 ms
per cell, 356 of them the reference scan that `master`'s cell index
replaces. Measure again after the merge.

### Stream time, Rust side, `soe-bench cell WhiterunBanneredMare`

| | 2026-09-17 (`perf`) | 2026-09-18 (`batching`) |
|---|---|---|
| Stream, 1 thread, median | 0.442 s | 0.402 s |
| Stream, 24 threads, median | 0.400 s | 0.391 s |
| `host.material` total over 6 streams | 1,954 ms (97 percent, `master` trace of 2026-09-18) | 46 ms (2 percent) |
| `host.texture_decode`, cold reads | decoded every stream on the calling thread | 88 reads, 44.7 ms summed over the workers, once per source |
| `host.scan_references` mean | 361 ms | 356 ms (the `cell-index` branch replaces it, not carried here) |

The 1,954 ms row is the lead's trace of `master` after `cell-index` landed,
where the material path was all that remained. Here a block-compressed
texture is never decoded, its BC blocks and mip chain are copied out of the
file and uploaded as they are, and the copy happens on the worker thread
that parsed the NIF, into a cache the `DataSource` owns. So the first stream
of a cell pays 45 ms of reads spread over the workers and a second stream of
any cell that shares its textures pays nothing. On this branch the stream
is the reference scan again. With the index from `master` on top of this
branch, the Bannered Mare should stream in about 20 ms warm.

Godot side, `stream_cell` in `bench.gd` (data path plus upload): 1.11 to
1.19 s with the RGBA8 decode and a generated mip chain, 0.50 to 0.52 s with
the compressed upload. Texture memory 592 MB (RGBA8, no mips), 783 MB (RGBA8
with generated mips), 175 MB (the files' own blocks and chains).

### Spans added

`host.batch` (payload: surfaces in), `host.add_cell`, `host.upload_mesh`
(payload: vertices), `host.upload_texture` (payload: pixels),
`host.texture_decode` (a cache miss, read plus copy or decode),
`host.scan_cells`, `host.scan_land`, `host.terrain_surface`,
`host.stream_exterior`. `host.material` now wraps the cache lookup on the
worker.

## Vanilla reference points

We do not have vanilla numbers for any row above. Nothing here says the
engine is faster or slower than Skyrim SE. What we can measure of vanilla,
and how:

- Loading screen times: a Proton launch of Skyrim SE with
  `[Papyrus] bEnableLogging=1 bEnableTrace=1` in `Skyrim.ini` writes
  timestamped `Papyrus.0.log` lines. The first `OnInit` timestamp minus the
  process start gives cold start to script start. Coc into
  WhiterunBanneredMare from the main menu (`coc WhiterunBanneredMare` in the
  console) and read the `OnCellLoad` or `OnLoad` timestamps for the cell load
  time.
- Frame time: `ENB` or `MangoHud` under Proton for a frame time histogram in
  the Bannered Mare and on the Whiterun plains, on this GPU, at the same
  resolution the Godot window uses.
- Script throughput: no vanilla counter exists. A script with a counting loop
  and `Debug.Trace` timestamps at the start and end gives instructions per
  second within the log's second granularity.
- Save size and time: the vanilla `.ess` for the same point, from the Saves
  folder, with its file size and the loading screen timing.

Until a session captures those, the tables above are internal baselines for
regression checks, not a comparison with the shipped game.

## Run of 2026-09-18, branch `cell-index`

The reference scan is gone. `DataSource::open` builds `soe_esm::CellIndex`
once (the `esm.cell_index_build` span) and `stream_cell_id` walks the ids the
index files under the cell instead of every `REFR` in the load order. Same
machine, release build, `--runs 3`. New subcommand: `cell-index <editor id>`.

| Measurement | Before (2026-09-17) | After | Note |
|---|---|---|---|
| Cell index build | | 0.309 s median, 0.273 s min | 872,220 placed objects, 2.8M objects/s, once per open |
| One `references` lookup | | 9.9 ns | 1M lookups of the Bannered Mare |
| `host.scan_references`, Bannered Mare | 361 ms per stream | 0.5 ms per stream | 697 ids walked instead of the whole table |
| Stream WhiterunBanneredMare, 1 thread | 0.442 s | 0.320 s median, 0.306 s min | 391k triangles/s |
| Stream WhiterunBanneredMare, 24 threads | 0.400 s | 0.301 s median | 416k triangles/s |
| Stream the 3 by 3 block around WhiterunOrigin, 1 thread | 3.28 s | 4.06 s | 957 references, 285,187 triangles, see below |
| `host.scan_references`, exterior block | nine scans | 2.0 ms over 18 streams | |

The stream did not drop to the 9 ms of mesh work the earlier trace promised,
and the exterior block got slower. Both have the same cause: the 2026-09-17
numbers came from branch `perf` before the materials branch merged, and
texture decode now fills the time the scan used to. From
`trace cell WhiterunBanneredMare` (21,729 events over 3 plus 3 runs):

| Span | Count | Total ms | Mean | Note |
|---|---|---|---|---|
| host.stream_cell | 6 | 2,005 | 334 ms | |
| host.material | 4,578 | 1,954 | 427 us | 97 percent of stream: `Vfs::read` plus `Dds::decode`, 88 textures per stream |
| esm.cell_index_build | 6 | 1,676 | 279 ms | once per open, not per stream |
| bsa.vfs_read | 2,436 | 319 | 131 us | texture reads dominate, mesh reads were 15 us each |
| host.build_all | 6 | 46 | 7.7 ms | the mesh work, unchanged |
| host.scan_references | 6 | 3.0 | 0.5 ms | was 361 ms |

And from `trace exterior WhiterunOrigin` (one run, 18 streams):

| Span | Count | Total ms | Note |
|---|---|---|---|
| host.stream_cell | 18 | 9,219 | |
| host.material | 1,586 | 9,166 | 99 percent, exterior textures are larger than interior ones |
| bsa.vfs_read | 1,144 | 1,091 | |
| host.build_all | 18 | 50 | |
| host.scan_references | 18 | 2.0 | |

So the next hot spot is texture decode, not the index and not the mesh path.
`MaterialCache` lives for one `stream_cell_id` call, so every stream decodes
its textures again, and the decode runs on the calling thread after the mesh
workers join. A texture cache on `DataSource` that outlives the call, and
decoding on the worker threads, are the two obvious moves, in that order.
The thread speedup rows stay near 1.0x for the same reason: the timed work
is now single-threaded texture decode.

The exterior block's 957 references and 285,187 triangles are the cells'
own temporary children only. WhiterunWorld's persistent cell holds another
495 placed objects (doors, markers, the city's persistent statics) that the
index keeps apart under `world_persistent_cell`, an exterior loader has to
add those and filter them by position.

## After the round 3 batch (2026-09-18, master 0874286 and later)

The cell index and the batching branch merged the same day, measured together on master with
`soe-bench --runs 3`, release profile with fat LTO and line tables, 24 threads.

| Measurement | Before the batch | After |
|---|---|---|
| Bannered Mare stream, 24 threads, median | 0.400 s | 0.004 s |
| Bannered Mare stream, 1 thread, min | 0.442 s | 0.012 s |
| Whiterun 3 by 3 block, 24 threads, median | 4.06 s | 0.013 s |
| Whiterun 3 by 3 block, 1 thread, median | 3.28 s | 0.022 s |

The exterior block keeps its 957 references, 242 meshes and 285,187 triangles. Against the bar
fixed before the numbers arrived (p99 at or under 16.7 ms, worst hitch under 33 ms, a cell under
250 ms on workers): frame p99 0.519 ms and worst 1.698 ms on the batched 3 by 3 block, both pass,
and the cell stream passes once the index and the compressed texture path are on the same tree.
The 56 ms frame figure recorded earlier was `Performance.TIME_PROCESS`, which reads a constant 80
to 113 ms on a scene the wall clock draws in 0.48 ms, so nodes were never the frame cost. The
host question is settled for now: Godot at this scale is not the bottleneck.

## Explicit timing gates (2026-09-19)

Each timed phase now reports median, minimum, nearest-rank p99, maximum and sample
count. The p99 of fewer than 100 samples is the maximum, not evidence about a
population tail. Most phases use `--runs`, but the province navigation cohort
still runs once. Its samples describe the complete 100-query cohort, not
individual query latencies.

Repeat `--max-seconds metric=value` before the subcommand to set explicit upper
bounds, for example:

```text
SKYRIM_DATA="<Data>" soe-bench --runs 100 --max-seconds cell_stream_WhiterunBanneredMare_1_thread_p99=0.05 cell WhiterunBanneredMare
```

Use the exact printed metric name. Invalid, duplicate and unmeasured names fail
the command, as do exceeded limits. Choose machine-specific limits before the
first measured run. No measured number in this report automatically becomes a
threshold.

Navigation also reports lazy point-index construction separately from repeated
1,000-point lookup cohorts. The synthetic CPU stress harness counts allocation
and reallocation requests per timed tick in an isolated child process. It does
not alter the installed-game timings below and does not measure live heap size.

The cell benchmark reports its cold texture-cache stream separately, then
compares warm-cache one-worker and multi-worker streams. Each timing sample
drops the previous result before starting its timer, so startup measurements
do not retain two complete archive sets. A unit test checks the drop order.

### Audit measurements, 2026-09-19

Baseline `75cfb55` and the audit branch used the same installed five-master load
order and Cargo `thin` profile. Before and after processes ran sequentially.
Other project work was active on the machine, and startup timings varied.
These are observations, not portable thresholds.

| Workload | Baseline | Audit branch |
|---|---:|---:|
| 100 fixed province path queries | 27.934 s | 6.135 s |
| Paths found in that cohort | 40 | 40 |
| 1,000 local path queries, median of 3 | 0.013 s | 0.010 s |
| Local paths found | 1,000 | 1,000 |
| 24-worker startup, separate process pair 1 | 9.168 s | 3.776 s |
| 24-worker startup, separate process pair 2 | 9.352 s | 6.991 s |
| 24-worker startup, separate process pair 3 | 5.769 s | 3.449 s |

The province cohort is 4.55 times faster in this comparison, with the same
reachability count. It calls `find_path` directly, so the gain is from component
rejection and stale-queue handling, not the point index. The new point-query
cohort found 1,000 of 1,000 points in 0.008 s after a separately measured
0.016 s lazy index build. There is no matching old-harness point-query timing.

The startup samples have medians of 9.168 s before and 3.776 s after, but the
one-worker control samples also varied widely. All paired threaded samples
improved, which supports retaining bounded parallel archive opening. A repeat
on an idle machine is needed for a stable regression threshold.

Every cell run retained 690 references, 318 parsed meshes, zero failed meshes,
124,924 triangles and 763 surfaces. Streaming numbers from the old harness are
not compared because its first one-worker stream had a cold texture cache and
its multi-worker stream had a warm cache.

Persistent worker pools and lazy archive payload storage remain unimplemented.
They need separate measurements of worker startup, first-use latency and peak
memory. The failed repeated startup run that retained two full archive sets is
not included in the table. The allocation-count gate covers synthetic CPU
scenarios only, as documented in [performance-limits.md](performance-limits.md).

## Persistent data-source workers, PRF-05

The worker pool change was measured on the same machine and with the release
`soe-bench cell WhiterunBanneredMare` command, five warm stream samples at one
and 24 workers. The baseline ran before the pool change, then the command ran
again after it. Both runs reported 690 references, 318 statics with models,
393 meshes parsed and no failed meshes.

| Measurement | Scoped workers before | Persistent workers after |
|---|---:|---:|
| Warm stream, 1 worker | 20 ms | 22 ms |
| Warm stream, 24 workers | 6 ms | 5 ms |

The focused startup test measured 21 repeated batches of 512 integer items.
Its scoped-worker median was 100 microseconds per batch. Creating the four
thread pool and running its first batch took 344 microseconds, while later
batches on that pool had a 30 microsecond median. This synthetic measurement
isolates worker creation. It is not a substitute for the real-cell stream
measurements above. The stream medians are close to the observed run-to-run
variation, so this sample does not show a reliable stream-time change. The
gated debug test also streamed Bannered Mare five times with 24 workers and
recorded a 50 ms median. That figure is kept separate from the release
benchmark.

## Cold interior texture preparation, 2026-09-20

HST-06 fixed a 50 ms cold texture preparation budget before its first timing
run. OpenMW's `apps/openmw/mwgui/loadingscreen.cpp` uses the same initial delay
before drawing a loading screen. The earlier Bannered Mare trace above reported
44.7 ms of cold texture work, which put the declared boundary just above that
known sample.

The installed-data test clears `DataSource`'s prepared texture entries before
each cell while retaining its load order and VFS. Cold therefore means a miss
in the engine texture cache. It does not mean an empty operating-system page
cache. The release test used one worker and one allowed CPU because a parallel
data-source open peaked above the available memory during concurrent project
work. `preparation` is the sum already recorded by
`CellStats::texture_decode_time`. It is cumulative preparation work and can
exceed elapsed wall time when cell work runs in parallel. This corpus fixed
the worker count at one. `wall` includes the rest of `stream_cell_id`, but
excludes Godot resource upload and collision building.

| Fixed interior | References | Textures | Preparation ms | Wall ms | Fade |
|---|---:|---:|---:|---:|---|
| MarkarthSilverBloodInn | 794 | 114 | 48.417 | 75.657 | no |
| WhiterunBanneredMare | 690 | 118 | 68.865 | 89.433 | yes |
| RiftenBeeandBarb | 619 | 111 | 69.926 | 86.826 | yes |
| FalkreathDeadMansDrink | 349 | 69 | 75.502 | 86.355 | yes |
| BleakFallsBarrow01 | 2,465 | 143 | 78.721 | 230.207 | yes |
| SolitudeWinkingSkeever | 975 | 146 | 90.991 | 115.431 | yes |
| MorthalMoorsideInn | 348 | 81 | 97.145 | 105.614 | yes |
| WinterholdTheFrozenHearth | 326 | 67 | 98.595 | 106.962 | yes |
| WindhelmCandlehearthHall | 700 | 134 | 101.570 | 123.862 | yes |
| DawnstarWindpeakInn | 353 | 83 | 106.797 | 115.210 | yes |
| WhiterunDragonsreach | 1,432 | 174 | 158.486 | 193.273 | yes |
| RiverwoodSleepingGiantInn | 481 | 97 | 241.598 | 254.647 | yes |

Nearest-rank preparation percentiles are 90.991 ms at p50 and 158.486 ms at
p90, with a 241.598 ms maximum. The test completed in 18.65 seconds after the
release build and asserted the fade decision for every sample. Its monitored
process peaked at 14,556,104 KiB RSS during data-source open, then exited zero.

The separate headless Godot gameplay acceptance measured 367.029 ms of cold
outbound preparation and activated the overlay. Warm return and input
crossings each measured 0 ms and left it inactive. The process exited zero
after 46.0 seconds, with a peak RSS of 15,273,864 KiB.

## Cell lighting, RND-21

The 2026-10-04 RND-21 change removes the benchmark's fixed directional sun and
ambient color. Interior runs now use the cell's own XCLL or LGTM ambient and
directional lighting, fog and light fade values, matching the walkthrough.
Exterior cells use the dark background until RND-18 supplies weather lighting.
Benchmark results before and after this lighting change are not directly
comparable as image-quality measurements. The timing procedure and frame-time
limits are unchanged.

## Detection neighborhoods, PRF-31

The 2026-10-11 follow-up removes another whole-reference scan from actor
selection. Contact queries retain sorted actor FormIDs and recheck membership
only for invalidated references. Prefetch prepares that index, and native-save
restoration and vanilla reference import rebuild it. Current placements and
effective enable state still come from World. The synthetic
`prf_31_contact_index_bounds_unchanged_frames_independently_of_scenery` check
puts 5,000 non-actor references beside one actor. Its fixed work check requires
60 unchanged queries to perform no additional membership examinations and
one changed actor to add exactly one examination.

The light membership index also groups loaded identities by cell and refreshes
only invalidated references. It no longer scans every loaded light for each
queried cell, and runtime base changes to or from LIGH refresh membership.
`prf_31_light_membership_bounds_unchanged_frames_and_cell_moves` fixes the work
threshold before running: 60 queries of both cells with 5,000 scenery references
add no membership examinations, and moving one light adds exactly one.
The design retains xEdit REFR/NAME identity and shared World placement. The
local OpenMW `components/sceneutil/clusteredlighting.hpp` supplies a rendering
light grid model. Its OSG light payload does not fit this CPU gameplay lookup,
which stores only derived FormIDs and uses the existing light sampler.

The walking fixture now turns on stalled horizontal progress instead of
repeatedly pushing along a blocked gate axis. It retains both 100-unit movement
thresholds, the two 30-second phases and the maximum 2,000-microsecond detection
limit. Its windowed Rust launcher also rejects script errors and a failed
process exit. The prior judge's failed windowed sample, 2,616 microseconds and
13.3 units of Tamriel movement, remains a failed measurement. Light mode
prevents new installed-data or Godot runs for this follow-up, so the changes
have no new before/after timings or successful windowed acceptance measurement.
Earlier measurements below describe their original builds.

### Follow-up local verification

The 2026-10-11 follow-up retains draft commits `836a0228b` and `ab27b7705`
on `draft8/PRF-31`. Further changes are uncommitted. Installed-data variables
are unset for local test runs, and the configured build flags and test thread
count are retained.

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all` | Exit 0 |
| `rtk cargo test -p soe-game --lib prf_31` | Exit 0, 10 passed |
| `rtk cargo test -p soe-game --test it prf_31` | Exit 0, 7 passed |
| `rtk cargo test -p soe-game --test it cmb_28_contact_actors` | Exit 0, 11 passed |
| `rtk ./tools/prose-lint.sh` | Exit 0 |
| `rtk env cargo clippy -p soe-game --all-targets -- -D warnings` | Interrupted, exit 1 |
| `rtk env cargo clippy -p soe-godot-host --all-targets -- -D warnings` | Interrupted, exit 1 |

Clippy ran through `rtk env`, with installed-data variables unset. Both package
checks reached a suspended Cargo process, state T, with no compiler child and
the build lock held. Cancellation released that lock. The initial RTK game
check and its queued retry exited 130 when canceled. The later direct game
and host checks exited 1 when canceled after suspension. These are incomplete
checks, with no compiler diagnostic or successful clippy result. Rerun the two
package commands above through `rtk env` when the environment permits them to
finish.

The full workspace gate is left to the reviewer.

### Untested follow-up checks

Light mode prohibits host test builds, Godot runs and installed-data checks.
The following cases have no result on this follow-up. Commands are for the
lead after light mode ends, with installed-data variables set for corpus
cases. No new successful 2 ms acceptance measurement is claimed.

`rtk cargo test -p soe-godot-host --lib prf_31` runs these untested cases:

- `prf_31_spatial_light_matches_full_scan_and_current_reference_state`
- `prf_31_spatial_light_matches_full_scan_after_runtime_base_changes`
- `prf_31_pairs_use_range_and_one_light_sample_per_actor`
- `prf_31_sound_source_keeps_distant_owner_without_duplicate_pairs`
- `prf_31_sparse_population_avoids_distant_pair_queries`
- `prf_31_vanilla_light_results_match_full_scan`
- `prf_31_range_boundary_and_negative_buckets_keep_near_pairs`
- `prf_31_invalid_range_performs_no_sight_or_light_queries`
- `prf_31_candidates_preserve_npc_pairs_away_from_player_and_sound_owner_pose`
- `prf_31_candidates_follow_cell_and_position_each_frame`
- `prf_31_candidates_reject_invalid_ranges`
- `prf_31_vanilla_candidate_observations_match_resident_population`
- `prf_31_light_sampler_follows_current_enable_parent`
- `prf_31_vanilla_persistent_actors_require_attached_tamriel_grid`
- `prf_31_exterior_pairs_share_worldspace_and_keep_original_light_cells`

`rtk cargo test -p soe-godot-host --lib cmb_32` runs these untested existing
detection checks:

- `cmb_32_occlusion_light_and_sound_change_observations`
- `cmb_32_cell_change_retains_actor_form_identity`
- `cmb_32_sound_source_can_have_an_owner_in_another_cell`
- `cmb_32_cell_ambient_and_placed_light_follow_records`
- `cmb_32_cell_inherits_ambient_from_lighting_template`
- `cmb_32_vanilla_light_records_are_read_when_installed`

The existing adversarial targets also remain untested. Each command is
`rtk cargo test -p soe-godot-host --test TARGET prf_31`, using the named target:

- `prf_31_adversarial`: `prf_31_light_sample_uses_current_enabled_and_moved_reference_state`,
  `prf_31_malformed_or_nonlight_placed_references_do_not_panic_or_add_light`,
  `prf_31_spatial_pairs_include_exact_range_and_negative_bucket_boundary`,
  `prf_31_detection_event_is_sampled_at_its_location_and_owner_can_be_distant`,
  `prf_31_sparse_actor_neighborhood_never_queries_distant_sight_or_light`
- `prf_31_breaker_adversarial`: `prf_31_destroyed_runtime_light_stops_contributing`
- `prf_31_breaker_breaker_adversarial`:
  `prf_31_sound_from_sibling_exterior_cell_uses_source_and_owner_separately`,
  `prf_31_equal_coordinates_in_different_worldspaces_do_not_share_sounds`
- `prf_31_breaker_breaker_breaker_adversarial`:
  `prf_31_off_by_default_light_stays_off_when_reference_state_is_loaded`,
  `prf_31_runtime_spawned_light_joins_the_next_cell_sample`,
  `prf_31_spatial_light_grid_matches_full_scan_at_negative_and_radius_edges`,
  `prf_31_observation_batch_samples_light_once_per_target_form`

After building with `rtk cargo build -p soe-godot-host --release` and importing
with `rtk "$SOE_GODOT_BIN" --headless --path godot --import`, run
`rtk cargo test -p soe-godot-host --test prf_31_walk prf_31`. Both launchers
remain untested:

- `prf_31_walk_route_turns_on_stalls_without_moving_the_player`
- `prf_31_installed_windowed_gate_walk_keeps_every_detection_update_under_two_ms`

The existing live checks also remain untested:

- `prf_31_body_sync_preserves_stationary_rids_and_follows_world_changes`, run
  `rtk "$SOE_GODOT_BIN" --path godot --resolution 640x360 --script prf_31_body_sync_test.gd`
- `cmb_32_live_detection_test.gd::_run`, run
  `rtk "$SOE_GODOT_BIN" --path godot --resolution 640x360 --script cmb_32_live_detection_test.gd`

### Earlier measurements

The threshold fixed before the first run is 2 ms for the whole of
`PlayerNode::advance_detection`, including body synchronization, sight rays,
light lookup and `World::evaluate_detection`. The installed-data checks use
Godot 4.4.1, the release extension and the five-master load order. The walking
harness holds NPC placements fixed and omits AI planning and VM ticks. It steps
the host capsule, updates shared player placement, streams exterior residency
and calls the whole gameplay detection updater. A zero-time gameplay frame
after each load-door transition settles deferred unload before walking.

The initial implementation keeps light-reference identity in a derived World
index, bins current light influence for CPU queries, and builds observer/target
pairs within the winning GMST detection range. Sound source neighborhoods remain
separate from actor neighborhoods. Light is shared per target in a frame, and
actor senses and combat awareness are shared within an evaluation batch.
Previously known distant pairs advance loss and recovery timers without creating
new awareness for distant strangers. These indexes add no saved state or
reference scene nodes.

Earlier exterior comparisons measured 266 observations in 3,203 microseconds
before spatial sampling, then 248 observations in 1,410 microseconds with 32
actors. The previous corrected 60-second headless walk still failed. It sampled
446 actors and 530 observations in Tamriel, with a 6,657-microsecond maximum
and 1,813 over-budget updates out of 3,623. The lead's round-four measurement
on `af106218` also failed, with a 4,540-microsecond maximum and 1,809
over-budget updates out of 3,624. Those checkpoints had sparse pair and light
lookup, but still synchronized the worldspace's entire built actor population.

The 2026-10-05 pass separates cheap contact placements from controller-height
resolution. Effective XESP enable state, deletion and destruction gate that
view. Prefetch does not attach host resources. Exterior references also need an
attached grid at their current position, including references authored under a
worldspace's persistent cell. This retains their parent-cell identity without
treating every built persistent actor as physically resident. Exterior grid
and persistent cells share the worldspace's pair coordinates. Light lookup
still receives the target's original cell.

A per-frame candidate pass retains actors near another observer, including NPC
pairs away from the player, and observers near sound sources. Selected actors
resolve controller height. Existing eligible contact bodies remain available,
while unchanged feet positions skip physics transform writes. Contact rays
share a direct space state, query parameters and actor-to-body exclusions for
one update. The implementation reads current shared World state and introduces
no dependency, saved state or per-reference nodes.

`detection_stage_times` separates placement selection and body sync,
`LightSamples::new`, observations including sight queries, and world evaluation.
The following measurements show that split, in microseconds. Windowed whole
samples enclose the GDScript `sample_detection` call. Walking samples read the
outer Rust detection timer, which also includes settings, event filtering and
bookkeeping outside the four stages.

| Sample | Actors | Observations | Sync | Lights | Observations and rays | Evaluation | Whole |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Before this pass, Whiterun windowed | 26 | 218 | 63 | 56 | 779 | 255 | 1198 |
| Partial pass before physical residency, Tamriel headless | 364 | 226 | 1482 | 29 | 853 | 614 | 3085 |
| Final Whiterun windowed | 2 | 0 | 28 | 0 | 8 | 5 | 71 |
| Final Tamriel walk, first frame | 2 | 1 | 222 | 24 | 15 | 12 | 286 |

The partial pass already used effective enable state and reused sight queries,
but left distant persistent placements eligible. It failed its 60-second walk
with 1,730 over-budget updates out of 3,623, including a 56,580-microsecond
maximum. Body sync, observations and evaluation all remained substantial,
whereas light construction was 29 microseconds in the reported Tamriel sample.
This failure led to physical grid residency filtering.

The final fresh startup has one living resident NPC besides the player in each
gate phase. Those NPCs are beyond the winning 2,500-unit detection range along
the gate route. The unchanged passive route therefore produces no nearby sight
pairs. The timed fixture queues a controlled sound at the player's reference,
owned by the resident NPC, before each update. It requires observations in both
phases and exercises the CK separate source and owner contract, including a
distant owner. One untimed update primes that sound's actor before each phase.
Sound creation, startup and gate resource replacement are outside detection's
timer. Event consumption, candidate and body sync, light construction,
observations and evaluation remain inside it.

The final 60-second headless check passed and exited zero. It performed 3,624
updates, 1,815 in WhiterunWorld and 1,809 in Tamriel, with one actor-owned sound
observation per update. Its maximum was 1,171 microseconds and no update exceeded
2,000 microseconds. Maximum horizontal displacement was 471.087 units inside
Whiterun and 1,508.425 units in Tamriel. Mean stage costs were as follows.

| Phase | Sync microseconds | Lights microseconds | Observations microseconds | Evaluation microseconds |
| --- | --- | --- | --- | --- |
| WhiterunWorld | 52.0 | 12.4 | 13.5 | 9.0 |
| Tamriel | 187.7 | 19.3 | 14.5 | 11.9 |

The separate live interior detection check passed with seven actors and 42
ordered pairs. A PhysicsServer wall reduced detection from 40.985 to -12.832,
and a sound raised it to 38.581. Toggling placed light `0:0C8FBD` changed its
pair's value from -2.713 to 26.873. Forty full contact sync and detection samples
took 5,660 microseconds, with no scene-node increase, and awareness survived
a load door. The PRF-31 body check passed stationary RID reuse, moved placement,
disable/re-enable, source/owner validation and unchanged scene-node count.

Validation passed five game unit tests, ten contact integration tests, 23
existing game detection integration tests and 20 host detection tests. The host
tests ran with installed data, including indexed/full-scan light equality,
candidate/full resident observation equality and Tamriel persistent-grid
residency. Formatting and both changed packages' all-target Clippy checks exited
zero. The full workspace gate was left to the reviewer as requested.

The timing check does not exercise NPC AI, VM scheduling or resumed saves.
An attempted straight approach to the city NPC fell through terrain, while the
Tamriel approach was blocked, so the acceptance retains the gate route. An
extra CMB-28 host run passed pose, body, obstruction and removal checks but failed
its two authored HitFrame checks. Its graph reported `tailMTState`, `MTState`
and `arrowDetach`, without HitFrame. Terrain route repair and authored animation
markers are outside this row. The earlier full windowed walk also failed its
Tamriel movement threshold, with only 73.319 units of displacement. The final
windowed check is an exterior sample, and the 60-second acceptance is headless.

## Deterministic operation budgets, PRF-14

`soe_trace::WorkFrame` captures operation counts without using elapsed time.
Game ticks retain their last counts through `Game::last_work_counts`.
`PlayerNode.work_counts` includes its enclosing host advance. Explicit work
windows cover setup or queued work outside a tick. Nested ticks contribute
once to their enclosing window. Data-source workers enter the caller's
recorder before processing a chunk and join before the snapshot.

Every fixed test compares all ten counters, including zero limits. These
limits are fixed before running the scenarios:

| Scenario | Locate | Triangles | Path nodes | Instructions | Natives | Conditions | Rays | Shapes | Records | Instances |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| AI-25 orbit, 8,192 triangles, each of 60 ticks at 0.1 s | 2,000 | 8,000 | 256 | 0 | 0 | 0 | 0 | 0 | 2,048 | 0 |
| PRF-07, 1,000 actors, ten warm-up ticks then 120 ticks at 60 Hz | 8,000 | 16,000 | 4,000 | 0 | 0 | 0 | 0 | 0 | 200,000 | 0 |
| Mixed stress, 16 actors, scripts and paths, each of five ticks | 128 | 256 | 8,192 | 544 | 0 | 0 | 0 | 0 | 10,000 | 0 |
| Burst of 32 quest stages, fragments and OR conditions | 0 | 0 | 0 | 96 | 32 | 32 | 0 | 0 | 8,192 | 0 |
| One installed Bannered Mare NAVM point query | 1 | 256 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

The orbit fixture keeps AI-25's authored bounds, height, arc samples and
movement. Triangle candidates use the graph's bounds tree at triangle level.
A full scan of its 8,192 triangles for every orbit sample exceeds its
triangle limit by hundreds of times. The gate checks work rather than CPU
speed. A* remains synchronous, with no scheduling or path-budget change.

The stress CLI prints `work_peak_<operation> <count> count` after each
measurement window. These are per-counter maxima across measured ticks,
excluding setup, warm-up and report work. Scope instrumentation is independent
of the global timing trace switch. Counter definitions and worker scope
behavior are in [trace.md](trace.md). The fixed tests are in
`game/tests/ai_25_orbit.rs`, `game/tests/prf_14_work.rs` and
`bench/src/stress/mod.rs`. The installed NAVM case uses all five masters.
The Godot correctness test loads the Bannered Mare, checks actual RID creation
counts against streamed instances, and verifies a new empty window is zero.
It does not measure renderer throughput.

### PRF-11 after triangle indexing and sliced A*

The same installed graphs and endpoints measured 40.387 us first and 25.539 us
warm for Bannered Mare, and 93.216 us first and 60.665 us warm for WhiterunWorld.
The route node counts and costs remained 30 / 1914.888 and 55 / 2723.3533.
The warm before/after pairs are 28.423 / 25.539 us and 68.509 / 60.665 us.
These single-query timings do not establish a general speedup. The frame
scheduler bounds work independently: 64 nodes per request and 512 across
requests per frame. Pending requests retain their heap and predecessor state.

An intermediate Godot 4.4.1 headless run, with triangle indexing and sliced A*
but before the closed-door lookup index, completed 3,600 frames and failed
all 3,600 against the fixed 8 ms limit. Seven contact NPCs were present.
It reported 53,030 package evaluations and a worst AI tick of 154.985 ms.
A three-frame diagnostic then measured 1.235 ms of door refresh within a
43.023 ms first AI tick, with 64 instantiated actors. This evidence led to
indexing closed-door blocking by triangle, reusing the existing cell reference
index and narrowing combat candidates through the existing awareness table.

### PRF-11 full-phase diagnosis, 2026-10-03

The continuation measured the complete AI tick and nested phases on Godot
4.4.1, release build, five installed masters, headless Bannered Mare,
3,600 ticks at 60 Hz. The first diagnostic run reported seven contact actors,
24,208 evaluations, 295 ticks above 8 ms and a 13.199 ms maximum. The finer
second run reported 1,180 failures and a 13.709 ms maximum. These runs were
sequential and their wall-clock maxima varied.

At frame 600 of the finer run, total actor processing took 6.571 ms.
Request setup used 2.116 ms and route-result fallback used 3.496 ms.
The first tick spent 9.414 ms scanning sandbox markers. Resumed search work
was below one microsecond in these samples, so the node budget did not cover
these costs. The previous two-phase report exposed none of them.

Nearest lookup now carries the existing best radius into each mesh's triangle
tree. The synthetic overlapping-mesh regression failed before the fix after
testing all 80,002 triangles, and passes after testing two. Boolean containment
stops at the first match rather than visiting every overlapping interior.
With these query changes, frame 600 fallback time fell to 22.683 us. That
intermediate run still failed six early ticks, with a 14.683 ms maximum.

Marker filtering now reads only the live base or the winning NAME, XLCN and
cell fields, sharing the reference initializer's BOS base selection. It avoids
full placement decoding and inventory or actor-template initialization for
irrelevant references. This changes no saved fields and adds no dependencies.

The final marker subphase diagnostic found 5.742 ms in base filtering,
3.375 ms in cell-reference lookup and 1.267 ms in bed classification during
the first tick. Reference initialization accounted for 0.508 ms. That run
failed three ticks and reached 15.404 ms. The candidate scan therefore gained
its own retained cursor, with 64 reference examinations per request and 256
across the frame. Pending actors keep their step until candidates are ready.
The query still returns candidates in FormID order before choosing with RNG.

The first host run with sliced marker queries reported one failure at frame
1561, reaching 12.319 ms. Its failing frame did not record nested phases.
The script now prints phase details for failures. A repeat on the same
extension did not reproduce it. Its cause is unknown. That repeat exited zero
and measured a 5.885 ms maximum, with all 3,600 frames below 8 ms. The original
71 percent failure rate is gone in these runs, but this is a measured check
on this machine, rather than a portable wall-clock guarantee.

```text
PRF-11 actors=7 frames=3600 seconds=60 evaluations=24900 worst_ai_us=5885.331 failures=0
```

The final run's first tick spent 1.844 ms in cell-reference lookup and
1.874 ms in the entire new marker request phase. At frame 1800, request setup
used 3.097 ms, fallback used 28.409 us, marker request work used 138.662 us
and resumed marker work used 0.471 us. Package and actor state keep their
existing saved layout. No actor scene-node path was added.

The installed-data release Rust checks also passed both 60-second Bannered
Mare runs, with maxima of 1.560 ms and 2.696 ms. All 24 PRF-11 Rust tests passed.
The final path survey retained both baseline corridors and costs. Bannered
Mare measured 35.437 us first and 23.705 us warm, versus 38.102 us and
28.423 us before indexing. WhiterunWorld measured 89.209 us first and
57.709 us warm, versus 102.924 us and 68.509 us before indexing.
The 498 library tests across game, navmesh and godot-host passed. Package
clippy with all targets and warnings denied passed for all three crates.

### PRF-11 repeated-request diagnosis, pass 3

The lead's pass 2 release check reported 637 failing frames and a 24.893 ms
maximum. Additional actor-driver phases separate actor state, Calm and
evaluation eligibility from movement and its existing nested query timings.
The initial pass 3 diagnostic, before changing request reuse or door refresh,
reported 778 failures and a 39.097 ms maximum over 3,600 frames. It used Godot
4.4.1 headless with the installed five-master gameplay load order.

At frame 600, movement used 7.088 ms, including 6.254 ms in request setup.
At frame 2400, setup used 8.461 ms of 9.092 ms in movement. A failed route was
discarded on completion, so unchanged actors resolved the same endpoints
again on later ticks. At frame 304, a sandbox request also spent 4.361 ms
collecting and filtering cell references. Door refresh ranged from 1.257 to
2.060 ms at the periodic samples. The worst frame, 2289, spent 19.516 ms in
request setup, 5.260 ms in procedure-tree entry and 6.034 ms in door refresh.
These are inclusive wall-clock phases, including time spent descheduled.

Completed failures now remain in the transient planner until their inputs or
the NAVM graph revision change. Unchanged requests perform no new query work.
Door refresh reads live references each tick and reuses unchanged footprint
bindings. Neither cache changes saved actor state or adds a scene-node path.

With route reuse and unchanged-door filtering, periodic samples fell to 23.136
us of setup and 129.645 us of door refresh at frame 600. That run overlapped
the worktree's Rust test build. It still failed 184 frames and reached 118.969
ms, with 81.543 ms in package selection on its worst tick. Its first tick
also spent 7.142 ms collecting cell references, outside the marker scan budget.
Sandbox request setup now reads only index slice lengths, and each bounded
scan merges the sorted persistent, temporary and deleted slices. It no longer
copies, filters and sorts the whole cell before its first slice.

The installed Rust checks passed their three 60-second runs at 967.218,
698.921 and 749.527 us maximum, but the cold first-tick regression failed at
699.073 ms. Ordinary cell loading had left the world NAVM graph lazy.
`World::load_cell` now prepares the graph after reference prefetch and before
attachment, so this work occurs during loading rather than the first AI tick.

The navmesh test command reported 83 passes with `SKYRIM_DATA` unset. A separate
installed path survey passed on both graphs. Bannered Mare measured 34.095 us
first and 24.877 us warm, and WhiterunWorld 91.253 us first and 60.033 us warm.
Both baseline corridors and costs remain unchanged. The warm before/after
pairs are 28.423 / 24.877 us and 68.509 / 60.033 us.

After moving preparation into loading, all 173 game library tests passed.
The installed integration selection passed 83 tests, including the cold
first-tick check, all PRF-11 timed runs, door invalidation and cell transitions.
The three Bannered Mare 60-second maxima were 703.269, 699.803 and 724.780 us.
Package Clippy passed for game, navmesh and godot-host with all targets and
warnings denied. No saved fields, dependencies or actor scene nodes were added.

The final release extension passed the same Godot 4.4.1 headless check, with
no compiler running in this worktree during measurement. The process exited
zero. First-tick cell-reference work was 3.778 us. At frame 600, request setup
was 11.439 us and door refresh 59.322 us, compared with 6.254 ms and 1.899 ms
in the initial pass 3 diagnostic. The worst tick was frame 1800 at 1.756 ms,
including 1.582 ms of actor processing, 264.609 us of request setup and
153.110 us of door refresh. All 3,600 samples met the unchanged 8 ms limit.
These are wall-clock observations on this machine, not a portable scheduling
guarantee. Earlier failed diagnostic runs remain recorded above.

```text
PRF-11 actors=7 frames=3600 seconds=60 evaluations=24900 worst_ai_us=1756.340 failures=0
```

### TST-18 installed gameplay frame budget

`godot/tst_18_frame_budget_test.gd` follows the walkthrough's installed-data
startup in the Bannered Mare, then streams a three by three block around
Tamriel's WhiterunExterior04. It starts the PRF-37 simulation in each location,
waits for a first snapshot and warms up for two wall-clock seconds. It samples
240 completed rendered frames per location, requires resident NPCs and package
evaluations, and checks the main-thread engine total against 2 ms p50 and
4 ms p99. Continuous simulation lag above one second also fails the gate.
These limits are declared in `godot/simulation_budget.gd` before measurement.
The synthetic `prf_37_timing_gate_test.gd` checks all three failure thresholds,
empty samples, invalid durations and a simulation that makes no progress.

The installed check uses a window, captures the F3 overlay at both locations,
writes raw main-thread, simulation and worker samples plus mean system times
to `SOE_PRF_37_REPORT`, and quits with a failing exit status when a bound fails.
Simulation and worker samples are the latest published values at each render
frame, so several frames may observe the same tick. The Rust
`prf_37_installed_windowed_tst_18_gate` launcher retains process output and
requires successful exit and the walkthrough shutdown marker. Build the
release extension and import once before running:

```text
rtk proxy env SOE_THREADED_SIMULATION=1 SKYRIM_DATA=<installed-data> "$SOE_GODOT_BIN" --path godot --resolution 640x360 --script tst_18_frame_budget_test.gd
```

The exterior soak found one frame of 123 to 217 ms. A story event rebuilt the
node list by walking every winning record in the load order, about 20 ms per
event, and a burst of events in one frame cost 150 to 260 ms. The parsed nodes
depend only on the load order, so `World` now builds them on the first event
and reuses them (`story_nodes` in `crates/game/src/world/story_manager.rs`).
After the change the exterior soak has no frame over 33 ms on the installed
data.

The interior soak then failed far more often than the three frames the
script prints (it prints only the first three breaches). Two runs on a lightly
loaded host reported over_budget_frames 57 and 83 over about 2,600 samples,
p99 35.0 and 39.2 ms, worst 48.4 and 81.3 ms, a median frame of about 21 ms
and about 45 fps against about 145 fps in the exterior. The FAIL line now
prints over_budget_frames, p99, worst and the sample count. SOE systems summed
to only about 4 ms of that frame, and the interior was three times slower per
frame than the exterior under the same renderer, so the renderer was not the
cause. The single frame worst values in the systems table (dialogue,
audio, host render submission, Papyrus VM) come from startup and settle
frames, not the measured window. A frame profile of the interior walkthrough
found `World::sky_frame`: for an interior cell it walked every winning WRLD
record and asked the cell index for the persistent cell of each, once per
tick, costing 16.9 ms per call. `CellIndex::persistent_cell_world` now keeps
the reverse map, so the owner is one hash lookup. After that change the
interior walkthrough has over_budget_frames 0, p99 9.1 ms and worst 15.6 ms
over 8,691 samples, and the exterior p99 8.3 ms and worst 14.9 ms over 8,695.

The interior log carries repeated Papyrus errors, `cannot call IsInFaction()
on a None object` from `DialogueSwapGovernmentAliasesScript.TestForSwap`
line 66. The log reports each distinct error once with a count (about 100
lines per run), so printing is not a per-frame cost, and the fixed run meets
the budget with them still present. The None actor in that alias is a
separate defect and is not part of the frame budget.

The pre-PRF-11 comparison was run with the linear scan in
`NavGraph::nearest` restored: the exterior walkthrough fails
(over_budget_frames 123, p99 88.7 ms, worst 98.1 ms), and so does the
thousand-actor runner test. Navigation work in the warm 600-tick PRF-14 window
is no longer asserted because the fixture setup now performs it, so
navigation is covered by the cold-start tests
(`tst_18_cold_actor_packages_bound_lookup_on_overlapping_navmesh_floors` and
`tst_18_prf_07_runner_asserts_thousand_actor_p99_and_worst_limits`).

This gate measures the opt-in snapshot adapter. It does not establish complete
playable-runtime integration or include startup and cell replacement in the
steady-frame percentile window. The draft limits are listed above.
