# `save`: Skyrim Special Edition save games

`crates/save` reads an `.ess` file into a typed structure and turns the part of
it that describes progress into scenario expectations the harness in
`crates/game` can run. It does not apply a save onto a running world, that is a
separate item.

The format was not reverse engineered. It is documented and it has a mature
open implementation, so both were read first.

## Sources, and what each gave

- `../engine-refs/uesp-savefile/` holds 37 wikitext pages fetched from the UESP
  wiki by `fetch_uesp_savefile.py` next to them. `Save_File_Format.wikitext` is
  the top level layout, `File_Format_Conventions.wikitext` defines `wstring`,
  `vsval` and `RefID`, and one page per change form and global data type covers
  the rest. Rerunning the script picks up pages added since.
- `../engine-refs/FallrimTools/` is ReSaver, Apache 2.0, the save editor the
  modding community uses to repair broken saves. Its `resaver.ess` package is
  the authority used here for everything the wiki states loosely: which fields
  sit inside `headerSize`, that Special Edition is told from Legendary by the
  header `version` alone, that the third global data table count is short by
  one, and that Special Edition widens Papyrus string counts and element ids.
  The Papyrus second pass follows `papyrus/Papyrus.java`,
  `DefinedElement.java`, `ScriptInstance.java`, `Reference.java`,
  `ArrayInfo.java`, `ActiveScript.java`, `Variable.java` and `Type.java`.
  These supply table order, optional fields, ID widths and value tags. The
  Java reader is not a Rust library, so this crate implements the documented
  layouts locally. No source code was copied.
  Runtime variable import uses `Script.getExtendedMembers` for parent-first
  slot order and CommonLibSSE-NG `src/RE/O/Object.cpp` for distinct inherited
  slots. The VM scope rules also follow Caprica's identifier resolution.
- The npm `@save-ward/save-parser-skyrim` was not used. It states that it does
  not support Special Edition, which is the only edition this crate targets.

## Where the two sources are wrong

A `vsval` packs its value into the bits above a two bit width tag, so one byte
holds at most `0x3F`. The UESP page says a value up to `0x40` fits in one byte
and ReSaver's encoder follows it, but `0x40` shifted left by two is `0x100` and
does not survive the cast to a byte, so that path writes zero. The same off by
one sits at `0x4000`, and the three byte cap is `0x3FFFFF` rather than the
`0x40000000` both sources give, which would need a fourth byte. This crate uses
the bit widths and refuses a value that does not fit. Decoding is unaffected,
only encoding, which is why it survives in a tool that mostly reads.

## The layout, and its two traps

A save is an uncompressed header block, then a body that Special Edition
compresses with LZ4 in block format.

The header block is the magic, a `uint32` size, the fields that size covers,
and then the screenshot. The screenshot is outside the declared size and
Special Edition writes four bytes per pixel. Ending the header in the wrong
place does not fail at the header: the body then starts four or more bytes off,
and the failure surfaces much later as a location table full of nonsense.

Inside the body the plugin lists sit behind a `uint32` byte length. It is easy
to read past, because the plugin count immediately after it looks like a
plausible count on its own. Skipping it shifts every later field by four bytes
with the same late, confusing failure.

Both are pinned by `tests/probe.rs`, since both cost a full parse of the corpus
when they were wrong and neither announces itself.

## What is decoded

- The header, every field, including the screenshot kept as raw pixels.
- The plugin list and the light plugin list.
- The file location table, with the third table count corrected.
- All three global data tables, as typed blocks with raw payloads.
- Type 3, the global variable table, as references and floats.
- Every change form header, and the `QUST` payload far enough to read quest
  stages and their done flags.
- Type 1001, the Papyrus block: the string table, the script definitions with
  their member descriptors, script instances that bind scripts to forms,
  reference and array headers, runtime ID and active-script headers. The
  second pass decodes instance and reference variables and array elements.

Papyrus values include null, object and struct references, strings, integers,
floats, booleans, nested variants and array references. String indices are
checked, counts are bounded by remaining bytes, and variant nesting is limited
to 128 levels. Each second-pass ID must occur exactly once in its header table.
Payloads retain file order, which can differ from header order. Null and boolean
payloads and float bit patterns survive re-encoding. Unresolved object and
array references remain IDs, since a dangling reference is readable save data.

## What is left raw

Change form payloads other than `QUST` stay as bytes, decoded on demand. The
Papyrus block is decoded through array elements. Active-script bodies, stack
frames, function messages, suspended stacks and the remaining tables stay raw.
Only active-script headers precede instance variables. Stack bodies follow
the decoded second-pass values, so they do not block reading those values.

This is inspection, not VM restoration. Values are not applied to the running
world, and neither stack execution nor pending calls can be resumed from a
vanilla save. The explicit instance-import API below can apply decoded
variables to a prepared VM, but it is not a complete save loader.

## Runtime instance import

`Papyrus::restore_instances` joins parent-first member descriptors to saved
slots by ID. The host must supply an explicit map from each saved instance or
reference ID to an existing VM object. A value-only reference may map to
`None` when the host explicitly determines it is unresolved. Missing mappings
are errors. Saved form/script pairs are not used as unique identities, since
the corpus contains repeated pairs and unbound instances.

Arrays are allocated once per saved ID, then filled and shared by every
variable naming that ID. Scalar, object and array references become VM values.
Both the script-instance and reference tables carry variable state. In the
corpus, `critterFish` reference entries include 95 slots and the state
`schooling`, despite FallrimTools naming that payload field `TYPE`.

Each variable retains its declaring script, including private parent and
child variables with the same name. The VM imports the saved state without
calling `OnInit`, `OnEndState` or `OnBeginState`. It validates declarations
before replacing any live instance. State names with no matching PEX body
are retained, as the VM's existing state transition path allows.

The returned instance and array ID maps are available for later stack import.
The save crate now depends on the existing `soe-papyrus` crate, whose only
runtime dependency is the existing `soe-trace` crate. `soe-bsa` is a test
dependency for reading installed PEX files. No new third-party package is
required.

The import refuses mismatched slots or declarations, duplicate target
instances, inheritance cycles, missing identities, invalid array elements
and unsupported variant/struct runtime values. Arrays use the existing VM
allocation ceiling. It also refuses to replace instances beneath existing
threads. It does not invent a form or alias binding from an opaque saved ID.

## Round trip

Everything not decoded is kept as raw bytes, so the body can be written back
exactly. `Save::split` gives the decompressed body and `Save::body_bytes` gives
the body this reader would write, and the two are compared byte for byte over
the whole corpus.

That raw-body check alone cannot validate decoded Papyrus values. The corpus
also re-encodes the typed second pass with `Papyrus::value_data_bytes` and
compares it with the original slice from `data_start` to `decoded_len`. This
method is a diagnostic encoder for the decoded values, not a save editor.

The file as a whole is deliberately not compared. Rebuilding it re-runs the
compressor and nothing promises an encoder reproduces the game's exact output,
so a mismatch there would say nothing about the parse.

## Expectations

`Save::to_expectations` renders quest stages and globals in the grammar
`validation.md` documents, as `expect stage-done <quest> <n> true` and
`expect global <name> <op> <value>`.

The save stores FormIDs and the harness resolves editor ids, so the caller
supplies the mapping. It comes from a loaded load order, which this crate does
not depend on: a save reader should not need the game's plugins to parse, only
to name what it found. A form the mapping does not cover becomes a comment
carrying its FormID rather than being dropped, so the output accounts for
everything in the save.

## Live run (2026-09-19)

Sixty real Special Edition saves, supplied by the user and held outside this
repository under `../engine-refs/saves/`. Saves are user data: none is
committed, and the tests skip cleanly when the corpus is absent.
`SOE_TEST_SAVE` names one file, `SOE_TEST_SAVES` names a directory.

Two sets. "Clean Saves" is nineteen files named after the main quest stage each
follows. "Main Quest + Dawnguard(Vampire) + Dragonborn" is forty one files
numbered in play order across the main quest, the Dawnguard vampire path and
Dragonborn.

| Measure | Result |
|---|---|
| Saves parsed | 60 of 60 |
| Bodies round tripped byte for byte | 60 of 60 |
| Typed Papyrus value payloads round tripped byte for byte | 60 of 60 |
| Papyrus value slots decoded, including array elements | 15,900,669 |
| Papyrus arrays decoded | 19,194 |
| Saves whose instance variables imported against installed PEX | 60 of 60 |
| Script-instance and reference entries imported | 2,649,854 |
| Shared arrays imported | 19,194 |
| Typed second-pass bytes re-encoded | 211,771,719 |
| Change forms read | 1,344,172 |
| Quest stage entries read | 92,077 |
| Globals per save | 933, the same in every save |
| Papyrus block | 4.87 MB at character creation, 6.16 MB at Miraak defeated |
| Change forms per save | 10,000 at character creation, 33,463 at Miraak defeated |

Every count rises with story progress, which is the shape a real playthrough
should have.

### Progression

The numbered set is in play order, so a stage marked done in one save must
still be done in every later one. Over the 41 step sequence, 29,572 carry overs
held and 14 cleared.

All 14 are stage 200 or 255, the conventional shutdown stages, on eight forms.
That is `ResetQuest` clearing a radiant quest so it can run again for a new
target, which is the game doing what it is supposed to do. The test reports
them and fails only if the cleared share passes five percent, since a decode
error would clear most carry overs rather than a handful.

This is the check a single file cannot give. It tests the decode against the
shape of a real playthrough instead of against itself.

### Not covered

The player level reads 1 in all sixty saves. These are prepared saves driven
through the story with console commands rather than played, so that is the
data rather than a misread, and the header size check passing on all sixty
rules out a misaligned field.

No save with a mod load order was available, so the light plugin list is
exercised only as an empty list, and `RefID` resolution through the form id
array is exercised but not against a case where a mod supplies the form.

The instance-import survey requires both `SKYRIM_DATA` and `SOE_TEST_SAVES`.
It uses distinct standalone VM objects for saved IDs, not live game forms.
It validates every saved instance and reference against installed PEX
declarations and requires zero scheduled instructions after import.
Synthetic tests execute functions against restored state, distinguish parent
and child values, preserve shared arrays through VM save/load, and check that
late errors do not replace existing state. The external corpus is read in
place and never copied.

Game-world object bindings, alias and active-effect identities, change-form
application, saved stack conversion, pending native calls and scheduler clocks
remain required for full vanilla-save restoration.
