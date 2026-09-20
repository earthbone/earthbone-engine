# ANM work review, 2026-09-20

The active goal remains all ANM backlog work. ANM-01, ANM-04, ANM-05,
ANM-06 and ANM-23 have tested implementation commits on separate branches.
None has been merged. The consolidated `BACKLOG.csv` records 53 ANM rows,
with those five marked `done` on their branches and ANM-07, ANM-08, ANM-12
and ANM-52 claimed as `wip`.
All dependency IDs exist and the complete backlog dependency graph is acyclic.
The original checkout and its unrelated changes were preserved.

## Tested implementation commits

| Item | Branch | Full gate | Installed-data evidence |
| --- | --- | --- | --- |
| ANM-01 | `behavior-conditions` | 1,181 Rust and 11 Python passes | 67 HKX tests, 428 graphs and 814 guards |
| ANM-04 | `behavior-bonemask` | 1,175 Rust and 11 Python passes | 61 HKX tests, 428 graphs and 1,094 masked children |
| ANM-05 | `behavior-motion` | 1,171 Rust and 11 Python passes | 7,699 packfiles, 6,126 animations, 13 motion frames and 1,069 samples |
| ANM-06 | `behavior-ragdoll` | 1,180 Rust and 11 Python passes | 48 ragdolls, 1,009 bodies and 961 constraints |
| ANM-23 | `task-anm-23` | 1,166 Rust and 11 Python passes | 428 graphs, 4,176 unexecuted modifier-path nodes and 32 classes |

Each candidate ran the complete `tools/lint.sh`. No optional tool was skipped.
Compiler checks passed. Comment-lint emitted informational similarity findings,
including findings in unchanged code. These were not warning-free runs.
The ANM-23 worker's earlier 1,163 figure was a test-list inventory. The
evidence-commit rerun retained a full log and passed 1,166 Rust tests and
11 Python tests.

Builds used isolated targets to avoid stale artifacts across worktrees.
Quota failures in the default temporary filesystem were addressed with private
scratch directories. The ANM-05 compiled target was cleaned after validation
to reclaim 1.8 GiB. Sources and gate logs remain available.

## Reviewed changes

ANM-01 changes `crates/hkx/src/behavior.rs`, `expression.rs`, `interp.rs`,
`link.rs`, `lib.rs`, `crates/hkx/tests/conditions.rs`, `real_files.rs`,
`docs/hkx.md` and `BACKLOG.csv`. It decodes scalar conditions, evaluates typed
guards, handles condition-only transitions and preserves variable/property
identity through linked graphs. Independent review caught and verified a fix
for dropped child character properties and namespace collisions.

ANM-04 changes `crates/hkx/src/behavior.rs`, `interp.rs`,
`crates/hkx/tests/common/mod.rs`, `interp.rs`, `real_files.rs`, `docs/hkx.md`
and `BACKLOG.csv`. It decodes authored bone masks and propagates per-bone
contributions through nested blends. Review fixes cover masked transition
endpoints, malformed nonempty arrays and exact first-contributor copying.

ANM-05 changes `crates/hkx/src/animation.rs`, `lib.rs`, new `motion.rs`,
`crates/hkx/tests/interp.rs`, new `motion.rs`, `docs/hkx.md` and `BACKLOG.csv`.
It decodes and samples optional extracted motion with checked inputs and
retains authored axes and samples. Its initial population assumption failed.
The final survey checks the complete previously documented packfile and
animation corpus, including every non-null motion pointer.

ANM-06 changes `crates/hkx/src/lib.rs`, new `ragdoll.rs`,
`ragdoll/shape.rs`, `ragdoll/constraint.rs`, `crates/hkx/tests/ragdoll.rs`,
`docs/hkx.md` and `BACKLOG.csv`. It exposes typed bodies, shapes, joint frames,
limits and motors. Independent review checked offsets, reference validation,
recursion bounds and the uncapped installed survey.

ANM-23 changes `crates/hkx/tests/real_files.rs`, `docs/hkx.md` and `BACKLOG.csv`.
It inventories every unexecuted modifier path, verifies row links and adds
bounded class-specific follow-ups. Review expanded its initial generic-only
inventory to include decoded but unexecuted modifier variants.

The `anm-progress` worktree changes only `BACKLOG.csv` and this report. It
consolidates the completed rows, active claims and reviewed dependencies. Candidate
branches have been checked separately. No combined runtime gate has run.

## Dependencies and limits

ANM-52 supplies modifier-generator dispatch. ANM-51 supplies ordered modifier
lists on top of ANM-52. Individual modifier rows require ANM-51 because the
installed graphs place them in those lists. Their old blanket dependency on
transition expressions was insufficient. ANM-40 also needs scalar expressions
and the function support in ANM-49. Ragdoll consumers retain ANM-06 where its
typed data is required. ANM-19 and ANM-53 require ANM-04. ANM-13 requires
ANM-10 and AUD-01. Exact dependencies are recorded in the backlog.

ANM-49 retains expression function and random evaluation scope. ANM-50 tracks
the authored troll comma-expression contract that needs runtime evidence.
Two draugr guards reference an undeclared SpeedSampled symbol and are reported
closed. No original-runtime expression capture was performed.

ANM-53 retains dynamic bone-mask binding scope. Short-mask zero extension in
ANM-04 is an unverified compatibility choice, documented in its crate notes.
Ragdoll physics execution and applying extracted motion to a controller are
outside the decoder rows. These readers do not establish a playable animation
runtime or complete the ANM goal.

The corpus inventory added 27 rows for uncovered modifier classes and dispatch
paths. The expression and bone-mask splits added three further rows. This
preserves discovered scope instead of hiding it behind successful narrow tests.

The user approved these commits and gave standing approval to commit after
task completion. Implementation evidence is recorded below and in each
completed backlog row. No worktree has been removed.

| Item | Implementation commit |
| --- | --- |
| ANM-01 | `84e992add53d470bf0438292be3862ffbe0418e7` |
| ANM-04 | `efa837bf439ce3609658721395244e8967fd9ca9` |
| ANM-05 | `5e07169b5eae4f0da4ec7f7c974207c010047535` |
| ANM-06 | `a021ced54760f1925ea8e6c7b0ac27aef250d8b6` |
| ANM-23 | `c8248a6d9c715a63abbc0d2180afff66f1103692` |
