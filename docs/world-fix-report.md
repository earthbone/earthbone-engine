# World fix checkpoint, 2026-10-13

This is a partial checkpoint on detached integration scratch HEAD, based on
`735176dff`, in `lead-play-world`. HST-44, HST-49 and HST-33 remain wip.
It is not a merge-ready result. No master branch or other worktree is changed.

## Changes

Physics retains compressed chunk instances and triangle shell radii. Ordered
convex face boundaries replace a fan that left gaps. Location replacement and
exterior streaming preserve movable bodies and install static query bodies.
LAND collision excludes grass and restores exact native XY sample spacing.
World initializes first exterior package locations through the existing AI
selector and resolver. Actor models still use the existing contact, appearance
and SkinnedActorNode path, selected by loaded physical grids. NPC walking is
unchanged. No dependency, saved field or new per-reference node path is added.

The subsystem documents credit the local nifxml, nifly, xEdit and Jolt sources,
and the residency and navigation comparison reports. References were read from
`/home/pengolodh/Data/Projects/Programming/skyrim-modclients/engine-refs`.
Installed assets were read in place, with no asset or save copied into git.

## Completed measurements

The initial installed model survey covered 1,550 enabled references carrying
collision objects in WhiterunWorld and the Bannered Mare. The old location
replacement omitted 351 movable references. Three additional references have
water phantoms and no rigid body: `0:0587B0`, `0:0587B1` and `0:085FCB`.
Thus 354 references lacked bodies under that replacement policy. Model
collection produces bodies for all but those three water phantoms. This is a
preparation count, not a completed live-body count after this checkpoint.
`docs/physics.md` gives the movable shape groups and water model names.

The physics library test run passed 20 tests, with zero failures or ignored
tests. An independent 225-point old/new convex face probe measured 49 holes
before and zero after. The installed inn floor grid passed all 1,008 samples
after retaining authored triangle radii. The square's NAVM-height diagnostic
found 43 misses among 2,898 samples. Independent visible-mesh and deep-ray
inspection found existing support at stairs, rocks and building bases whose
render height differs from NAVM. The final visible-surface anchored grid test
has not run. Its XY samples and original vertical support limits are retained.

The initial exterior record/contact survey found 32 enabled authored exterior
ACHRs and 32 contacts. It does not establish a drawn NPC count after package
initialization. The installed market record survey passed its owned-item and
prepared-shape checks. The live scene ray, Take and Steal checks have not run.

A release build completed successfully before the last terrain preparation
change. Production clippy also passed before that change. Final changed-file Rust formatting, comment
lint, prose lint, TOML formatting and Markdown lint passed. Comment lint also
reported advisory duplicate-function suggestions. No existing check was weakened or removed, and no lint exemption was added.

## Validation blocked

`tools/lint.sh` was run through the run Cargo wrapper. Earlier attempts caught
and corrected the clippy-driver selection and missing WithBaseField import.
The final gate could not start its format stage through the shared wrapper.
Its tracked process was interrupted with exit 130 after the shared Cargo and
installed locks stayed held. No full gate pass is claimed. Cargo deny and
cargo audit are absent. The other optional lint tools are installed and were
run directly while waiting.

Installed runs used SKYRIM_DATA, DISPLAY `:99`, the run Cargo wrapper and
serial test execution. The final headless survey and live-scene test remained
queued and were interrupted with exit 130. No test window was launched on
`:0`, and Skyrim was never launched. Queued jobs were not allowed to replace
a loaded extension. No tracked live walkthrough remains open.

The shared integration log also stopped inside an audio scene test. Wrapper
inspection suggests a lock-order cycle: an installed Cargo job acquires its
installed slot before waiting for a build slot, while a running build-slot
owner can start a Godot fixture that needs the installed slot. Nested Cargo
lookup is another possible queue contributor. Only this worktree's queued
sessions were interrupted. Shared wrappers and other agents' jobs were not
changed or killed. This diagnosis requires confirmation by the run owner.

## Required follow-up

Run the complete gate on this checkpoint once shared execution is available.
Run collision::world_tests serially with installed data and no default
features. Rebuild the release extension under the release lock. Then run
installed_exterior_scene_restores_npcs_and_stall_activation with installed
data and SOE_GODOT_BIN on Xvfb `:99`. Record the spawned and independent
expected NPC counts, every stall ray and pickup result, and final square grid
coverage before marking the backlog rows done. Package placement and restored
live bodies remain unverified in Godot. Walking is owned by the other fixer.
