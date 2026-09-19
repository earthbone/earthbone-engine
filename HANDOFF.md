# Skyrim open engine: handoff

Written 2026-09-14. This is the state of the engine track for whoever picks it up next, human
or agent. The plan it executes is `ENGINE-PLAN.md`, the research behind the plan is
`../research/skyrim-open-engine-map.md` with verified briefs under `../research/engine-map/`,
and the reference checkouts are under `../engine-refs/`. Read the plan's preamble and the
map's sections 1, 3, 10 and 12 before touching code. The design rule from the map holds
everywhere: the engine is a record database with an interpreter on top, every system consumes
forms keyed by resolved FormID.

## Where the repository stands

Local git only, no remote, branch `master` at `1615481`. Commits need an explicit identity:

```text
set -x GIT_AUTHOR_NAME pengolodh
set -x GIT_AUTHOR_EMAIL lysander.deloore@gmail.com
set -x GIT_COMMITTER_NAME $GIT_AUTHOR_NAME
set -x GIT_COMMITTER_EMAIL $GIT_AUTHOR_EMAIL
```

Round 1 of the plan is merged in full: `esm`, `bsa`, `nif`, `papyrus`, `havok`, `godot-host`.
Round 2 is merged for six of its seven items: `dds`, `physics`, `navmesh`, `commonlib-shim`,
`profile`, and the `editor` design document. Each crate has a matching `docs/<crate>.md`.

The one unstarted item is **game** (`crates/game`, branch `game`). The worktree `../soe-game`
exists on that branch, but the branch sits at the old papyrus merge (`899308b`), eight commits
behind master, with no commits of its own and no `crates/game` directory. Fast-forward it before
doing anything:

```text
cd ../soe-game
git merge --ff-only master
```

## Test state on 2026-09-14

The whole workspace was run with the real game unlocked:

```text
set -x SKYRIM_DATA '/run/host/media/pengolodh/Data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data'
cargo test --workspace
```

Result: 270 tests passed, 0 failed, 0 ignored. The gated real-file tests ran this time and
passed, among them every real NIF parsing to meshes, the real compiled scripts loading, and the
BSA texture extraction. Earlier fork reports that said "gated test written but not run" are
superseded by this run. Skyrim SE on this machine was updated by Steam on 2026-09-05 (build
24914197), so any hard-coded record counts from an older dump may drift.

Rules that still apply to every change, from the plan: `cargo check --workspace`, clippy clean
on the touched crate, `cargo test -p <crate>`, malformed-input tests for every parser, no
rendering or physics dependency inside a parser crate, house style in prose (no em dashes, no
semicolons in prose, no AI trailers), one worktree per fork, touch only `crates/<crate>/` plus
`docs/<crate>.md`.

## What the game crate has to do

From the round-2 entry in `ENGINE-PLAN.md`, restated with what the merged crates now provide:

- Load a load order through `soe-esm` into resolved forms. `soe-profile` already reads
  `plugins.txt`, `loadorder.txt`, the INIs and an MO2 profile layout, so the crate should take a
  resolved loadout from it rather than reading those files again.
- Define the object model Papyrus sees: Form, ObjectReference, Actor, Cell, Quest, with the base
  fields every native reads. Keep these as views over the form table keyed by FormID, not as
  engine-private copies.
- Register the first hundred natives by vanilla call count into the `soe-papyrus` native
  registry (Debug, Utility, Game, Form, ObjectReference activation and enable state, Quest stages
  and objectives, Actor inventory and values). The registry trait, its collision rule and the
  scheduler with latent calls are in `crates/papyrus`, see `docs/papyrus.md` for the host side.
- A game clock that drives the VM scheduler. `soe-physics` already steps a Jolt world from a
  clock, so one clock should feed both.
- Done criterion: a synthetic plugin with a quest script that advances stages on activation runs
  end to end under test, with the real-file variant gated on `SKYRIM_DATA`.

## After the game crate

No round 3 is written. The plan's own deferred list and the critic pass in
`../research/engine-map/critic.md` point at these, in the order they unblock each other:

1. The behavior-graph interpreter that `docs/havok.md` designs, with the replay harness it
   describes. This is the blocker the map calls out for animation, and nothing else on the
   list needs it yet, so it can run in parallel with the game crate.
2. The CommonLib shim from skeleton to something a plugin can link: `docs/shim.md` groups the
   349 types by whether the engine has the concept. Start with the group the engine already has
   once the game crate exists, since the object model is what most of those types are.
3. Streaming and rendering on the Godot host: `CellStreamer` exists but has never had a Godot
   binary to run in. No Godot is installed on this machine, so the host crate is checked by
   cargo alone. Installing one is the first step of that item.
4. Back-porting the BSA embedded-name fix has already landed in LMO (`726ef00`), so that item
   from the manager plan is closed.

## Environment notes

- Cargo and clippy run under rustc 1.98 here. Homebrew's cargo links `libbz2.so.1.0`, which is
  only a concern for the mod manager binary, not for the engine crates.
- `.rag/` under the workspace root indexes the engine references and this repository. Rebuild
  it after a round merges, the last build predates round 2.
- The mod manager track lives in `../linux-mod-organizer` on its local `master`, ahead of the
  Codeberg upstream, and must not be pushed there without asking. Its state is in
  `../BUILD-PLAN.md`.
