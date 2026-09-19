# Governing intent

Restated on 2026-09-19 from the user's recorded intent, later decisions in
`ENGINE-PLAN.md`, and the repository's working rules. This is the operating
interpretation for future work, not a claim that these features exist.
Implementation status belongs in [status.md](status.md).

## Purpose and scope

Build an open-source Skyrim Special Edition runtime that can play existing
content and preserve the mod ecosystem, while removing limitations of the
Creation Engine and its authoring tools. Playing the game is the destination.
Readers, benchmarks and isolated demonstrations are milestones toward it.

Skyrim SE, Update, Dawnguard, HearthFires and Dragonborn belong in the normal
validation load order. DLC scripting and gameplay are engine work. Creation
Club content loads through the ordinary plugin/profile route, with native
coverage extended in measured call-count order. These are scope commitments,
not claims of complete compatibility.

The mod manager is a separate workspace track. The engine should consume a
resolved loadout and existing profile conventions without requiring a manager
to emulate a single Data directory. Engine work does not authorize changing
or publishing the manager.

## Preserve the contracts mods depend on

The engine is a record database with an interpreter on top. Every subsystem
reads forms keyed by resolved FormID from the load order. Preserve record
identity, winning overrides, cross-plugin references, asset path resolution,
archive and loose-file precedence, script attachment, and observable event
semantics. Runtime mutations belong in the shared form-backed world model.
Do not give a subsystem a parallel authoritative copy of game state.

Existing plugins, xEdit output and Synthesis patches must retain their
meaning. Keep the Creation Engine's useful authoring model: a load order,
one active plugin, record edits, references and reusable data-driven behavior.
Modding should remain possible without rebuilding the engine.

Read the formats existing content requires: ESM/ESP/ESL, BSA/BA2, NIF, DDS,
HKX, PEX, ESS and the vanilla audio containers. Reading a format does not
establish full execution fidelity. Keep those claims separate in reports.

Papyrus remains a compatibility path. Implement natives in the order vanilla
and DLC scripts actually call them, with semantics grounded in local CK/UESP
documentation, public reference code and observed behavior. A registered stub
is not an implemented mechanic. A successful startup is not a playable quest.

The goal includes SKSE-dependent mods, within the API boundary the research
established: source-level adaptation or recompilation against a
CommonLib-shaped API and native replacements for popular Papyrus extensions.
Existing Windows SKSE DLLs that call Skyrim executable addresses cannot simply
load into this runtime. Report unsupported hooks and adaptations explicitly.

Preserve behavior on which mods rely, and validate changes where event order,
timing or saved state is observable. The user accepts limits to backward
compatibility. That permits documented improvements, not silent breakage or
an unsupported promise that every mod works.

## Modernize the runtime and authoring workflow

Use existing open tooling and host systems where they solve the problem.
OpenMW and the modding ecosystem are references and sources of reusable work.
Do not invent a new low-level renderer or duplicate a subsystem Godot provides.
Keep working engine code when an outside project overlaps it. Reuse fills
gaps, supplies evidence and removes unnecessary work.

Replace workaround steps with native systems while accepting the data mods
already ship. This includes distribution and object swapping, animation
replacement and behavior processing, navmesh generation, conflict inspection,
lip-sync generation, and eventually the broader authoring pipeline. Parsing
SPID, KID, BOS or OAR files alone does not complete their runtime integration.

Give new authors standard formats and familiar tools. Add glTF and ordinary
image imports, open audio formats and modern scripting alongside legacy
formats. Automate necessary conversion rather than requiring fragile manual
tool chains. Preserve legacy inputs as authors adopt new paths.

The Creation Kit replacement is part of the goal. Host it in the Godot editor,
with Rust operations over the same form table and renderer as the runtime.
Retain forms, cells, quest stages, aliases, dialogue, conditions and placement
workflows. Add override-chain inspection, undo, text exports for version
control, standard asset imports, navmesh regeneration and play-from-here.
The editor design is not an implemented editor.

Use a native Godot UI and an MCM-compatible configuration API for the
`SKI_ConfigBase` interface. Scaleform SWF execution is outside the chosen route.
ENB's closed injection protocol is out of scope. The extensible, configurable
post-processing that preset authors need remains in scope.

Consume existing LOD output first, including BTO/BTR and DynDOLOD output.
LOD generation is deferred by the first-year plan, even though it is a longer
term editor objective. Improved door transitions must retain cell identities,
coordinates and event order. Portal doorways remain a prototype to measure,
not permission to relocate every interior or silently tick far-side actors.

## Settled implementation choices

- Runtime crates stay Rust. Go is an option for a tooling component only when
  it is easier to maintain there, behind the planned protobuf boundary.
- Stock Godot hosts the runtime through GDExtension, with the `api-4-4` floor.
  The recorded host runs used 4.4.1 and 4.7.2. Treat these as tested versions,
  not a statement of what is currently the newest release.
- Drive bulk rendering and host physics through server resource IDs rather
  than one scene node per placed reference. Positional audio uses
  `AudioStreamPlayer3D` because the host lacks an equivalent server-level path.
  A small host/control node does not change the bulk-world rule.
- Use Godot's bundled Jolt for host physics and Godot's audio facilities.
  Keep simulation testable without Godot. The current player uses
  `SimpleWorld`, so agreement between physics paths needs verification.
- Keep parser outputs independent of rendering and physics dependencies.
  Use centered coordinates and origin shifting where worldspace size needs it.
- WebAssembly through wasmtime and the component model is the planned new
  mod extension ABI, using the same form table and native surface as Papyrus.
  Rust natives remain the engine path. GDScript is for editor tooling and UI.
  Lua is deferred unless authors need it. These decisions do not imply WASM
  integration has shipped.

## Sources, licensing and user data

Read the local `../engine-refs/` corpus before implementing a format, layout,
parser or algorithm. Load the `soe-corpus` skill for that work. Start with
xEdit for record layouts, UESP for format descriptions, CK pages for native
semantics, Mutagen for overrides, and the relevant existing implementation.
Credit what was used. If writing a replacement, say why the existing option
did not fit. Keep dependencies small and report new dependency trees.

Core crates are GPL-3.0-or-later. The CommonLib shim is MIT. No GPL-derived
code may enter the shim. Reading a repository is not permission to copy it,
and a repository without a license is not a source of freely reusable code.
The project's method uses published formats, public source and observed
behavior for semantics, not disassembly of Bethesda's executable.

Game assets and the user's save corpus are external test inputs. Read them
in place through the documented environment variables. Never commit them or
copy a save into this repository. An open engine does not include Skyrim data.

## Evidence and working discipline

Measure before claiming success. Set thresholds before runs, preserve seeds
and reproductions, survey real content as well as synthetic fixtures, and
report what was not exercised. Frame percentiles, CPU time, GPU time, stream
time and transition time are different measurements. Do not interchange them.

Malformed data must return typed errors. Keep determinism, parser fuzzing,
conformance scenarios and the feature-range checks part of development.
`tools/lint.sh` is the commit gate. Gated game-data and save-corpus runs are
separate evidence. A skipped gated test is not a successful corpus run.

One writer per checkout, one bounded task per branch and worktree. Use Serena
and the language servers for symbol navigation, direct reads and RTK searches
for named files and literal text, and prefix shell commands with `rtk`.
Consult relevant docs before building. Preserve unrelated user changes.
For GPT agents, explain each action before taking it, including edits,
checks and commits, as the user requested on 2026-09-19.
The agent reviews every completed step and runs the gate. Before any commit
or merge, show a diff summary naming the files, changes and reasons, then
wait for explicit user approval of that summary. The latest global
instructions require this approval even when the agent is the reviewer.
Leave changes unstaged while awaiting approval and commit approved steps.
Avoid another writer's tree. No AI attribution trailers. Follow the prose
and Rust rules in [style.md](style.md).

## Source precedence

The current user's instructions govern the task. The transcript extraction in
`../research/project-intent.md` preserves early statements and questions.
`ENGINE-PLAN.md` records later decisions, including Rust runtime scope,
licensing, WASM, DLC and ENB scope. Those later decisions resolve several
questions still marked open in the earlier extraction.

The research map and verified briefs explain the original reasoning. The
subsystem docs and code establish what has been built. Read their dates and
limitations. Old plans, agent reports and merged branch names do not establish
that every original done check passed. Update this restatement when the user
changes direction, and keep `CLAUDE.md` and `AGENTS.md` consistent with it.
