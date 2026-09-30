# soe-papyrus: PEX loader and virtual machine

Crate `crates/papyrus`. Round 1 of `ENGINE-PLAN.md`, written 2026-09-06.

## What is in it

- `pex`: the compiled script file format. Header, string table, debug block,
  user flags, objects with variables, properties, states and functions, and
  every instruction with its operands. Line numbers from the debug block are
  attached to functions.
- `asm`: a writer for the same model and a builder on top of it. Tests build a
  script, serialize it, and load the bytes, so the reader is on the path of
  every VM test. It is also the seed of an assembler for tooling.
- `psc`: a bounded Skyrim source compiler using the assembler. It accepts a
  script header with `extends`, `Hidden` and `Conditional`, native functions
  without parameters, scalar `Auto` properties, and functions with literal
  returns. Other declarations and statements return a source line error. It
  checks the source filename
  against `ScriptName` without case, generates Skyrim's `GetState` and
  `GotoState` functions, and emits a Skyrim PEX file. The parser follows the
  Open Papyrus script header, identifier, keyword and `.psc` filename rules
  and Caprica's Skyrim flag
  definitions and generated state functions. The gated SCR-04 tests compare
  the compiled `Activator.psc` and `Armor.psc` objects with their shipped
  Creation Kit PEX objects. A synthetic PSC function compiles and runs in the VM.
- `value`: run-time values and the language's conversion rules.
- `vm`: objects, attached script instances, threads with their own frame
  stacks, a scheduler with sleeping and latent threads, events, states, and the
  native registry.

The VM depends on `soe-trace`. SHA-256 fingerprints use `sha2`. PSC errors
use `thiserror`.

`Vm::loaded_script_fingerprints` lists loaded script names in case-insensitive
order with optional SHA-256 PEX source fingerprints. `load_bytes` records a
fingerprint of the input bytes after a successful parse. `load(Script)` reports
unknown source provenance because its original PEX bytes are unavailable.
Loading another definition with the same name replaces its fingerprint.
`tests/sav_67_fingerprints.rs` covers changed PEX definitions, programmatic
loads and replacement. The hash identifies content for dependency comparison,
not the origin or authenticity of a file.

OpenMW's `apps/openmw/mwstate/savegame.hpp` records content file names and
`loadgame.hpp` maps saved indices on load. Its save path does not supply script
source fingerprints, so the VM records them at the PEX load boundary.

## Format facts, checked against the references

- Skyrim and Skyrim SE files (format 3.1 and 3.2) are big-endian. Fallout 4
  format 3.9 and later files use the extended object layout and little-endian
  fields. The reader chooses byte order from the magic and chooses the object
  layout from the header game ID. The extended layout stores object, variable
  and struct-member const flags, struct definitions, property groups and struct
  member order. These const flags are preserved in the parsed model. They do
  not change VM assignment behavior.
- Object layout for 3.1 and 3.2: name, a size field that counts itself, parent
  class, docstring, user flags, auto state name, variables (name, type, user
  flags, default value), properties (name, type, docstring, user flags, flags
  byte, then either the auto variable name or the getter and setter function
  bodies), states (name, functions).
- Function body: return type, docstring, user flags, flags byte (bit 0 global,
  bit 1 native), parameters, locals, instructions.
- Property flags: bit 0 readable, bit 1 writable, bit 2 auto variable.
- Operand encoding: type byte then payload, 0 None, 1 identifier (string
  index), 2 string (string index), 3 int (four bytes), 4 float (four bytes),
  5 bool (one byte).
- Operand counts per opcode are Champollion's table. The three call opcodes
  carry a trailing integer operand with the count of variable arguments.
- Jump offsets are relative to the jump instruction itself.
- Debug function kinds: 0 method, 1 property getter, 2 property setter.

The 3.9 sections follow Champollion's `Pex/FileReader.cpp` and Caprica's
`pex/PexObject.cpp`, `PexVariable.cpp`, `PexStruct.cpp`,
`PexStructMember.cpp` and `PexDebugInfo.cpp`. The reader stores the added
metadata in the existing Rust PEX model rather than adding a C++ parser
dependency. The 3.9 parser test builds a small byte fixture with each extended
section populated.

Champollion's `Pex/Instruction.cpp` and Caprica's
`Caprica/pex/PexInstruction.cpp` supply the opcode IDs and arities for `Is`
and struct access. Caprica's `PapyrusStruct.h` and
`PapyrusStructMember.h` establish the declared member types and defaults used
when a runtime struct is created.

Checked on the installed game: all 14,026 `.pex` files in `Skyrim - Misc.bsa`
parse, 54,620 functions, zero failures, and every one loads into the VM.

## VM design

**Frames on threads.** A thread owns a stack of frames. A script function call
pushes a frame, `Return` pops it and writes the value into the caller's
destination variable. Because the stack is data rather than the host's call
stack, a native can suspend the thread at any depth and the VM resumes it
later. skymp's papyrus-vm runs calls recursively on the C++ stack and has no
scheduler, which is the gap the map named.

**Scheduler.** `run(budget)` executes runnable threads round-robin in slices of
64 instructions until the budget is spent or nothing is runnable. `advance(dt)`
advances real seconds without menus at the default timescale of 20. Hosts
use `advance_clocks(real_seconds, game_hours, menu_mode)` to supply actual
elapsed game time. `Utility.Wait` counts seconds outside menus,
`WaitMenuMode` counts seconds including menus, and `WaitGameTime` counts
game hours. The latter uses accumulated game time so timescale changes
while sleeping affect the deadline correctly. Sources are the shipped
`skse64/scripts/vanilla/Utility.psc` and CK `Utility_Script.wikitext`.
A native returns `Value`, `Wait(seconds)` or
`Latent(id)`. A latent thread sleeps until the host calls `resolve_latent(id,
value)`, and the value lands in the variable the call site named.

**Lookup order.** A method call on an object walks the attached instances in
attach order. Inside an instance the script chain runs from the most derived
script to the root, checking the current state and then the empty state of
each script before moving to the parent. Parent calls start the walk after the
script that defines the running function. When no loaded script defines the
function, the native registry is asked for each class name in the chain, then
for parents the registry declared for classes whose PEX is not loaded
(`declare_parent("Actor", "ObjectReference")`). With the vanilla scripts
loaded the declared parents are unnecessary, the chain is in the PEX files.

**Natives.** `register_method(class, name, f)` for functions called on an
object, `register_global(class, name, f)` for `Class.Function()`. Names are
case-insensitive. Collision rule: first registration wins, later ones are
recorded in `duplicates` and reported back to the caller, never silently
replaced. A native gets the self object, the arguments, the clock, the log, and
`defer()` to allocate a latent id.

**Base classes.** `create_object_of(name, class)` gives an object a class
with no script attached, and `NativeCall::create_object` does the same from
inside a native. Method lookup falls through the attached scripts to that
class and the parents the registry declared, so a quest form with no script
still answers `SetStage`. This is how the game crate hands forms to scripts
without preloading every form as a VM object.

**Host property writes.** `set_property` writes an auto property's variable
directly. A full property (a compiled `Get` and `Set` pair) has no variable
of its own, so the host runs the `Set` function on a fresh thread with the
value as its argument and drives that thread to completion before returning,
the way the game applies `VMAD` values to such properties. A setter that
waits or calls a latent native is logged and left running. `get_property`
reads auto variables only.

**Name operands.** The compiler writes the function name of `CallMethod` and
`CallParent`, the class and function of `CallStatic`, and the property of
`PropGet` and `PropSet` as identifier operands, the same operand type it uses
for variables. `exec` reads every operand as a value up front, and until
2026-09-18 that turned each of those names into a variable lookup that
resolved to `None`. Every vanilla call then dispatched as `None()`, walked the
class chain, and never reached a native, while the assembler tests passed
because they wrote those names as string operands. `exec` now takes the text
of an identifier or string operand for the name slots without a lookup. The
first `start` of the installed game reached 63 natives through 1,095 calls
after the fix and none before it. `NativeRegistry::calls` counts calls per
`class.function` so a host can see which natives real scripts hit.

**Redeclared natives.** `Alias.pex` declares `RegisterForUpdate` and its
relatives as natives of its own, `ReferenceAlias.pex` declares `GetReference`,
and a host registers each once, on `Form` and `Alias`. `call_method` used to
stop at the declaring script: it found the native on `ReferenceAlias`, asked
the registry for `ReferenceAlias.GetReference`, and logged "no
implementation" (26 such calls in the first `start`, plus 8 alias update
registrations). Since 2026-09-18 a native the declaring class does not
implement is retried on every class above it, the script's parents and the
registry-declared ones, in the order `class_names` gives. A native the
registry knows on a parent resolves, an unknown one still logs once.

**States and events.** `send_event(object, name, args)` starts one thread per
attached script that handles the event, resolved through the current state.
`GotoState` and `GetState` are built in, and a state change delivers
`OnEndState` and `OnBeginState`.

**Properties from outside a thread.** `set_property` and `get_property` are
how a host fills and reads a property without a running script, the way the
game crate applies a form's `VMAD` values. An auto property writes or reads
its backing variable directly. A full property (a compiled `Get`/`Set` pair)
spawns a call to the `setter` or `getter` and runs it to completion instead,
through `run_property_call`, the same spawn-and-drive-`run` loop
`set_property` already used for its setter, bounded to 64 passes of
`SETTER_BUDGET` instructions each. A call still runnable after that budget is
logged and abandoned, and `get_property` reads it back as `None`. A script
error during the call is logged like any other and the call yields `None`, it
never surfaces as a `VmError`. A read-only full property, a getter and no
setter, is reported as `VmError::ReadOnlyProperty` rather than run, so a
caller can count a property that is real but unwritable apart from one that
is missing outright.

**Errors.** Script errors follow the game: they are logged with the script,
function and source line and the thread continues with `None`. Calling a
method on `None`, indexing outside an array, dividing by zero and unknown
functions all take that path. The stack depth limit is 128 frames and a
runaway recursion is logged and the thread dropped. Only host API misuse
returns `VmError`.

**Values.** Strings compare without case, numbers compare across int and
float, arrays are shared references compared by identity, objects compare by
handle. Casts follow the declared type of the destination variable, which is
why locals and script variables keep their type names. Floats print with six
decimals, as in the game's log.

**Fallout 4 array mutation opcodes.** The VM executes `ArrayAdd`, `ArrayInsert`,
`ArrayRemoveLast`, `ArrayRemove` and `ArrayClear` on the existing shared array
reference. Add appends the requested number of typed copies, Insert accepts an
index from zero through the current length, Remove deletes up to the requested
count starting at a valid index, and RemoveLast and Clear are safe on empty
arrays. Nonpositive counts do nothing. Invalid insertion and removal indexes
are logged and leave the array unchanged. Mutations that would exceed
`MAX_ARRAY_LEN` are logged and leave it unchanged. A None array logs an error.
The PEX opcode names and operand layout follow Champollion's
`Pex/Instruction.cpp` and Caprica's `pex/PexFunctionBuilder.h`. Caprica's
`PapyrusFunctionCallExpression.cpp` shows the default Add and Remove count is
one and that these mutation calls do not produce a result value. The state
format already saves array identity and contents through the shared array pool,
so these mutations need no save version change.

**Object reclamation.** `Vm::collect_objects` traces a host-supplied candidate set
from non-candidate objects, frame locals and owners, thread results and explicit
host roots. It follows instance variables and shared arrays, including cycles.
An array with an outstanding mutable borrow defers the pass. Unreachable
candidates are removed together, so two expired effects referring to one another
do not keep either alive. Instance storage is compacted and surviving frame and
object indices are remapped. `InstanceId` is a storage index that callers must
reacquire after collection. `ObjectId` remains stable and is never reused.
The existing state format stores the compacted indices without a version change.

The host must supply retained values and queued callbacks as roots. Bare IDs held
outside the VM are not counted references. Completed thread results remain roots
because `thread_result` can still retrieve them. This is targeted object cleanup,
not collection of threads or arbitrary host allocations. The game host applies it
to expired active effects after each VM run.

Local sources consulted were the shipped
`skse64/scripts/vanilla/ActiveMagicEffect.psc`, whose finish event can outlive native
state, and CommonLibSSE-NG `include/RE/O/Object.h`, which has a reference-counted
object lifetime. The local skymp
`papyrus-vm/src/papyrus-vm-lib/VirtualMachine.cpp` has an empty `RemoveObject`, so
there was no reclamation implementation to port. The trace uses this VM's explicit
frames and values. Five collection unit tests cover object and array cycles,
sleeping and latent frames, compaction across save/load, completed results,
borrowed arrays and 256 allocation/reclamation rounds with one surviving object
and instance throughout.

## Vanilla numbers worth keeping

From the 14,026 shipped scripts, counting call sites by target. The
`OnBeginState` and `OnEndState` counts are the compiler's state-change
prologue in every script, not hand-written calls.

Native declarations in the vanilla PEX files: 686 across 44 classes. Largest:
Actor 184, ObjectReference 121, Game 80, Debug 29, ActiveMagicEffect 25,
Faction 24, Quest 24, Utility 24, Form 22, Alias 19. This is the base game's
own native surface before SKSE and the extenders.

Most called globals: `Game.GetPlayer` 5,746, `Utility.Wait` 2,008,
`Debug.Trace` 395, `CWScript.Log` 255 (a script function), `Utility.RandomFloat`
217, `Utility.RandomInt` 177, `Game.ShakeCamera` 121, `Game.EnablePlayerControls`
120, `Game.DisablePlayerControls` 98, `Game.AddAchievement` 97.

Most called methods by name: `GetOwningQuest` 8,713, `SetStage` 8,467,
`GetReference` 4,527, `GetActorRef` 3,516, `Disable` 2,138,
`SetObjectiveDisplayed` 2,089, `GetActorReference` 2,025, `Enable` 1,978,
`SetObjectiveCompleted` 1,900, `Stop` 1,824, `GotoState` 1,716, `GetRef` 1,714,
`GetLinkedRef` 1,508, `AddItem` 1,305, `MoveTo` 1,262, `EvaluatePackage` 1,241,
`GetStageDone` 1,214, `Start` 1,168, `GetStage` 1,022, `Play` 905, `Activate`
857, `RemoveItem` 806, `SetValue` 801, `ForceRefTo` 747, `PlayAnimation` 650.

The full ranking prints from the gated test, see below. Quest and alias
plumbing dominates, so the first hundred natives for week 6 to 9 of the map
are quest stages and objectives, reference aliases, enable and disable,
inventory, and package evaluation, before combat or magic.

## Save and load

`vm::state` (module `crates/papyrus/src/vm/state.rs`) gives `Vm::save_state(&self)
-> Result<Vec<u8>, VmError>` and `Vm::load_state(&mut self, bytes) ->
Result<(), VmError>`. No serde, an own little-endian binary format read and
written by a small `Writer` and `Reader`, the same shape as `crates/game/src/vmad.rs`.
`load_state` resolves saved script names against definitions already loaded.
The VM itself does not store PEX bytes across a save, only objects and threads
built from them.

Magic `SOEV`, a `u16` version (4), then an array pool, a sorted table of
script source fingerprints, and the VM body. Version 4 checks that a saved
instance or frame still refers to the same PEX bytes. A missing or changed
definition parks the original state until compatible PEX bytes are loaded.
Version 3 and earlier states have no source fingerprints and continue to use
name and frame-function checks.
Everything a script variable can hold round trips: objects by id, strings,
ints, floats, bools, arrays and Fallout 4 struct values. Struct members are
written in name order. An array is a shared reference
(`Rc<RefCell<Vec<Value>>>`), so it is pooled once by first-seen order and
every variable or stack slot that names it writes only its pool index. Two
variables pointing at the same array come back pointing at the same array
after load, checked by a test that shares one array between two variables
and asserts identity, not just equal contents, after the round trip.

The VM body covers everything `load_state` needs to keep running exactly
where `save_state` found it: every object (id, name, class, instance ids),
every instance (its script chain by name, resolved back to `Rc<LoadedScript>`
through the freshly loaded scripts, its state name, its variables with their
declared types), every thread (its state, including a sleeping thread's
clock domain and absolute deadline and a latent thread's pending id, and its
full frame stack, each frame naming its script, its function by a state-plus-name or
getter or setter key, the instruction pointer, locals with their types, and
where the return value goes), the latent and object id counters, the VM's
three clocks, and the trace log. A frame with no script (a native's own call frame)
cannot be saved, `save_state` reports it rather than losing it silently.

Version 1 states still load. Their single clock initializes the real and
menu clocks and implies game hours at timescale 20. A version 1 save cannot
recover which old `Sleeping` states originated from the other wait calls.
Version 2 preserves all three domains and includes them in the state hash.

Version 3 keys instance variables by declaring script and variable name.
Parent and child scripts can have private variables with the same name.
Flattening them into one name map discarded a slot, which appeared when
importing vanilla `defaultOnEnter` and `DefaultSetStageOnEnter` instances.
Frame variable access now starts at the script defining the function. Auto
properties use their declaring script's backing variable. Host variable
queries can name the parent or child declaration through the script argument.
The internal key prefixes the lowercase script name with its byte length,
so names cannot collide through separator characters.

Version 1 to 3 files still load. Their flat value is copied into each matching
declaration. Those files cannot recover a parent value already discarded by
the old VM. Distinct slots then remain independent in version 3. Version 4
adds struct values to the value encoding.

This follows the declaring-object resolution in Caprica's
`PapyrusResolutionContext.cpp`, the inherited slot layout in FallrimTools
`Script.getExtendedMembers`, and CommonLibSSE-NG's
`src/RE/O/Object.cpp` variable indexing. The skymp variable holder was also
read, but its flat name map does not preserve this case. No reference code
was copied.

`Vm::restore_instances` accepts resolved objects, saved state names and
variables carrying their declaring scripts. It validates the entire batch
before replacing instances and runs no initialization or state-change events.
Existing threads are refused because replacing instance indices would
invalidate their frames. Saved state names remain valid even when the loaded
PEX has no body for that state, as the existing `GoToState` path permits.
Function lookup then uses its normal empty-state fallback.

If `Vm::load_state` finds a missing script, changed PEX fingerprint or saved
frame function absent from the loaded PEX, it parks the original SOEV bytes
and clears the live objects and threads. `save_state` returns those bytes
unchanged while parked, and the scheduler executes no work. Loading definitions
retries restoration.
The state becomes live only when all named scripts and frame functions resolve.
The parked state keeps array identity, frames, latent waiters and pending return
destinations without substituting null values.

The save crate supplies the vanilla slot-to-name adapter and converts decoded
active frames into VM frames after object identities and instances are mapped.
`Vm::restore_threads` checks every owner, script, function, instruction body,
program counter and local slot before adding any thread. Caller return targets
come from the saved call instruction. The game import entry point restores
instances and schedules the saved active scripts. Queued messages, suspended
stack tables and latent native waits remain inert. Object identity mapping is
still supplied by the host.

A thread saved mid-`Utility.Wait` is a saved `Sleeping(seconds)` state on a
frame sitting inside the wait, nothing about the scheduler is special-cased,
so loading it back and advancing the clock past the wait resumes and
finishes the thread exactly as if it had never been serialized. A test
builds such a thread, saves and loads it into a fresh `Vm` with the same
script loaded, and checks it stays asleep until the clock passes the
deadline.

`ArrayPool`, `ArrayTable`, `Writer` and `Reader` are public under
`vm::state` so `crates/game/src/world.rs` writes its own state (references,
actors, quests, and the `Value` arguments of a pending event or call) in the
same encoding, with its own array pool separate from the VM's.

## Running the tests

```bash
cargo test -p soe-papyrus
```

The real-file test needs the shipped scripts. They are inside
`Skyrim - Misc.bsa`. The standalone VM corpus test takes an extracted
folder through `SOE_PEX_DIR`. Game integration tests load archives through
`soe-bsa` with `SKYRIM_DATA`:

```bash
SOE_PEX_DIR=/path/to/extracted cargo test -p soe-papyrus --test vm real_scripts -- --nocapture
```

`SKYRIM_DATA=<Data>` works once a loose `Data/Scripts` exists.

## Live run (2026-09-14)

`real_scripts_from_the_game_load_when_present` always skipped on a plain Steam
install, it only looked for a loose `Data/Scripts` folder, but the vanilla scripts
ship packed inside `Skyrim - Misc.bsa` and this install has never had them
extracted. The test now opens that archive with `soe_bsa::Archive` when no
`SOE_PEX_DIR` override and no loose folder are set, the same fix already used for
the hkx and nif gated tests, and reads every `.pex` straight out of it. `soe-bsa`
was added as a dev dependency for this. The live run then parsed 14026 scripts with
54620 functions and 0 failures, found 686 native declarations across 44 classes, 131
distinct static call targets and 2615 distinct method call names, all loaded into a
VM that carries `Actor`, `ObjectReference` and `Form`.

## Not done, and known gaps

- Fallout 4 `Is` and struct create/get/set opcodes run only in Fallout 4 3.9
  scripts. `Is` checks object classes and attached script inheritance. Struct
  creation uses the declared PEX member defaults, member writes enforce the
  member type and const flag, and invalid field operands log an error without
  changing the struct. The remaining Fallout 4 array mutators still abort the
  thread with the line number when executed. Skyrim never emits these opcodes.
- The lookup order inside a chain is per script, state then empty state. The
  original engine's exact precedence when a parent defines a state function
  and the child defines only the empty-state one is not documented and was
  not tested against the game.
- Vanilla startup and selected quest paths have run against natives, as
  recorded in `docs/game.md` and `docs/status.md`. Those checks do not prove
  all native semantics. Original-runtime comparisons remain needed for
  undocumented scheduling and inheritance behavior.
- `String` ordering comparisons use lowercase byte order. The compiler does
  not emit them for strings, so this only matters for hand-built bytecode.
- Object display in `as String` and string concatenation uses the object's
  first script name and the host-given name. The game prints the form id as
  well, which the VM does not know.

## VM maintenance and generated continuation tests (2026-09-19)

The VM keeps its public API and single-threaded execution contract.
`vm/execution.rs` owns instruction execution and casts, `dispatch.rs` owns
method and native calls, `properties.rs` owns property access, and
`scheduler.rs` owns thread lifecycle and scheduling. The parent retains the
shared data types and VM construction. This is a source split, not a new
scheduler or a change to native-call ordering.

`tests/prop_state.rs` generates 128 scripts per run, varies PEX byte order,
integer instructions, wait calls and the instruction at which state is saved.
It checks PEX model and byte round trips, then resumes in a fresh VM and compares
state and results against uninterrupted execution and an independent integer
sum. The test uses the workspace's existing proptest dependency.

State loading now rejects a frame instance index outside the instance table
before replacing live state. A regression test corrupts a saved frame and checks
both rejection and preservation of the original state. The new
`fuzz-vm-state` target loads a known probe script before parsing saved bytes so
valid frame mutations can reach execution. Neither test establishes the behavior
of every possible serialized state or every Papyrus opcode.

Object, variable and latent-waiter maps are serialized in sorted key order,
including before array-pool interning. Generated continuation cases use one to
five objects and multiple variables, and require byte-identical state after
loading into a fresh VM. A waiter-map test covers reversed insertion order.

## Function coverage

`Vm::start_coverage` begins an optional diagnostic interval after scripts load.
The scheduler records a body when it visits that frame, including an empty
body that returns by falling off its end. Merely loading a PEX or scheduling
a call does not count. `coverage_report` snapshots the interval and
`stop_coverage` returns its final report and disables recording.

The denominator is every loaded non-native method and explicit property
accessor. Method identity includes its defining script, state and name.
Getters and setters have separate identities. This reuses the saved-frame
identity resolver already described above, rather than a source-language
coverage tool that cannot observe these VM frames. The local reference search
found C++ coverage wrappers, which do not measure PEX execution. Native calls
remain in the separate native registry counters. Auto properties have no
compiled accessor body to count. Zero bodies reports no percentage.

Hits retain the self object when present, so a host can attribute the same
script body independently to different quest instances. Global calls have no
self object. Reports sort script and function identities. Counts measure
whether a body executed, not branch coverage or successful completion. Several
calls to the same body still count once. A frame resumed partway through a
function counts if it executes during the interval.

Coverage is outside saved state and simulation hashes. Restoring VM state
starts a fresh interval if recording is enabled. Loading more scripts stops
recording, since it changes the denominator. Restart after loading. Frames
and instances retaining an older definition cannot credit the replacement
body. Recording resolves function identity once per entered frame. Reporting
walks the loaded definitions, so enable it for harness runs rather than assume
it has no cost.

`tests/coverage.rs` checks states, inheritance, accessors, uncalled and native
bodies, queued calls, global waits and resume, stable saves/hashes, and stale
instances after script replacement. Quest grouping is in `soe-game` and is
documented in `validation.md`.

## Conditional variable lookup

`Vm::conditional_vars` exposes current conditional variables on attached
instances for game-side CTDA queries. The conditional bit comes from the PEX
user-flag table, not a fixed bit number. Both the declaring script and variable
must carry that flag. Auto properties use their compiled backing name, such
as `::activeHouseLocation_var`, and do not execute a property getter.

Native callbacks receive the same read-only lookup through
`NativeCall::conditional_vars`, so gameplay natives can evaluate live CTDA
variables without borrowing the VM while it is executing the callback.

The lookup follows inheritance and preserves multiple matching values so the
caller can report ambiguity. CK's Papyrus introduction specifies conditional
script and property declarations. The local Caprica property compiler copies
property user flags to the backing variable. `tests/conditional.rs` checks
nonstandard flag positions, current and restored values, inherited variables,
nonconditional declarations and duplicate names. No save format changed.

## Dialogue fragment dispatch and conditional variables

`Vm::call_script` and `has_script_method` resolve the script named by a VMAD
fragment on the object's attached instance chains. An unrelated attachment
with the same function name cannot intercept the call. Resolution uses the
instance's current state, then its empty state. Calls use the existing frame
builder and scheduler. The game dialogue tests attach two colliding fragment
names and verify that only the named script changes the owning quest.
