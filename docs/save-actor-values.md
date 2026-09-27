# Saved actor-value modifiers

SAV-06 remains blocked on SAV-64 as of 2026-09-20. The engine already preserves
its own actor-value components, but the consulted sources do not define how to
locate and decode those components in vanilla ACHR payloads. No modifier decoder
or restoration API is added by this checkpoint.

## Existing engine persistence

`crates/game/src/actor_values.rs` stores base, permanent, temporary and damage
components separately. `Values::write` writes each component and `Values::read`
reads them separately for SOEW versions from 7 onward, including current
version 19. The `write_actor_state` and `read_actor_state` functions in
`world/save.rs` use that representation. Current and permanent queries derive
their results from the components. An older SOEW total cannot recover an
ambiguous component split.

`World::actor` initializes actors from the resolved reference base, NPC template
parts and race data. A vanilla restoration path must establish those identities
and base inputs before applying saved components. Replaying damage or magic
operations would also produce runtime events, so it is not a substitute for
loading saved state. See [actor values](actor-values.md) for runtime component
semantics and existing persistence tests.

## Source audit

The local reference corpus was read before inspecting public updates. The
following revisions and files were checked. Public code was read directly.
No Bethesda executable was inspected or disassembled.

| Source and revision | Relevant observation |
| --- | --- |
| xEdit `6dbbed51d92456e39ffb916eece56040e91114b5`, `Core/wbDefinitionsTES5Saves.pas` | `wbChangeFlags001` names the five actor-modifier flags. `wbChangedCharacter` contains `wbChangedActor`, which defines two 32-bit fields, `wbChangedREFR`, and empty equip data. The five modifier unions are commented out at the actor definition and each active union uses `wbNull` for both alternatives. This provides no modifier field boundaries. |
| Public xEdit file revision `973b80ec555f2fbdc5ee8c1a15113d23ecb6ea2d` | The modifier unions remain placeholders. Reading a newer file does not close the gap. |
| FallrimTools `61bb57d895f023617cb55045a4907849fd1ff566`, `ChangeFormACHR.java` and `ChangeFlagConstantsAchr.java` | The reader handles the common actor prefix, extras, inventory and animation, then retains unparsed bytes. The flag names are present, but no modifier section is decoded. |
| UESP ACHR page, revision `3187377` from 2024-11-19 | The flag table names bits 19 through 23. The structure table ends after animation, with the remaining ACHR fields undocumented. |
| CommonLibSSE-NG `b93280e832f263dbef44e44cbe2936622a02f91a`, `include/RE/A/ActorValues.h` and `ActorValueOwner.h` | Runtime APIs distinguish permanent, temporary and damage modifier domains. They do not define ESS field boundaries or serialization order. |
| skymp `756fb86b05ab5c2fae4f9bc3c6d52ca580b8bdd3`, `savefile/src/SFChangeFormACHR.cpp` | The writer starts from an opaque ACHR template and patches body-morph values. It does not provide an actor-value schema. |
| TiltedEvolution `d2bcf04fd262b32e4a2693a2822a28fb89b03b9c`, `Code/client/Games/Skyrim/Actor.h` and `Actor.cpp` | The code names modifier flags and runtime components. The conditional `Save_Reversed` implementation is incomplete and does not establish the missing serialized sections. |
| Skyrim Save Lab `dbb0b4e3243c7753ff4b5b049cef644e21d294a8`, `app/core/player_payload.py`, `preset_patcher.py` and `live_player.py` | Player editing uses anchor searches, candidate actor-value offsets and plausible-value scoring. Those heuristics do not identify complete component domains or exact general ACHR boundaries. |

The older xEdit NVSE source dump in the local corpus repeats the same five
`wbNull` modifier unions. It is a copied definition, not independent layout
evidence. `crates/save/tests/roundtrip.rs` checks that an ACHR payload remains
byte-exact with each named modifier flag and with all five set together. That
test covers opaque preservation only. It does not assign field boundaries or
decode modifier domains.

The local UESP `ACHR_Changeform.wikitext` has SHA-256
`3f983922f4cbee53337d400113c77d94c4709644e51b189e459c7851742e3d45`.
The public [pinned ACHR page](https://en.uesp.net/w/index.php?title=Skyrim_Mod:Save_File_Format/ACHR_Changeform&oldid=3187377)
was checked against that local description. The updated
[xEdit definition](https://github.com/TES5Edit/TES5Edit/blob/973b80ec555f2fbdc5ee8c1a15113d23ecb6ea2d/Core/wbDefinitionsTES5Saves.pas)
and [Save Lab patcher](https://github.com/xcier/Skyrim-Save-Editor/blob/dbb0b4e3243c7753ff4b5b049cef644e21d294a8/app/core/preset_patcher.py)
were also inspected at the revisions above.

The documented ACHR modifier bits are 19 for disposition, 20 for temporary,
21 for damage, 22 for override and 23 for permanent. The source names alone do
not define which fields they gate, whether additional actor data is always
written, or how override and disposition relate to the engine's components.
The section location, count encoding, actor-value key encoding and domain order
remain unknown. The comment about no damage-modifier data in xEdit's common
reference skeleton does not establish the actor-specific payload layout.

## Corpus coverage

The new `sav_06_corpus_reports_modifier_flags_and_prefix_coverage` test in
`crates/save/tests/actor_value_coverage.rs` reads external saves through
`SOE_TEST_SAVES`. Before its first run, it required at least 50 saves, more than
100,000 ACHR records and exactly one resolved PlayerRef ACHR per save. It
accounts for every ACHR record, checks exact common-header re-encoding and checks
exact contents-prefix re-encoding wherever that decoder succeeds. It reports
every contents error and retains suffix lengths as coverage measurements.

The 2026-09-20 survey passed in 4.52 seconds after compilation.

| Observation | Count |
| --- | ---: |
| Saves | 60 |
| ACHR records, all save form version 78 and change-form version 78 | 243,595 |
| Records setting any of bits 19 through 23 | 0 |
| PlayerRef records, all with modifier mask zero | 60 |
| Successfully decoded contents prefixes | 243,514 |
| Contents prefixes blocked on named package state | 81 |
| Sequence package failures | 72 |
| Stacked package failures | 9 |
| Successfully decoded prefixes with a nonempty suffix | 243,514 |
| Bytes in those unparsed suffixes | 125,575,781 |
| Records with the animation flag | 844 |
| Created ACHR references | 1,572 |
| Explicit header base identities resolving into file `0xFF` | 0 |

Each modifier bit individually had zero occurrences. This is a coverage gap,
not proof that saved actors have no modifiers. Every successfully decoded
contents prefix leaves unparsed actor data, which may also contain animation.
Neither the suffix lengths nor the package failures identify a modifier
boundary. The survey makes no claim that modifier fields follow package state.
No source save was copied into the repository or modified.

## Dependency boundary

SAV-64 must supply a public Skyrim field definition or controlled vanilla
observations that establish exact modifier boundaries and distinguish saved
base overrides and modifier domains. SAV-06 keeps its original completion
check: restored current and permanent actor values retain their separate
modifier components through engine save/load. The coverage test does not
satisfy that check.

SAV-12 owns Sequence decoding and SAV-21 owns Stacked decoding. They account for
the 81 contents-prefix failures, but the unknown modifier location means that
their necessity for an actor-value decoder is not established. They become
actor-value prerequisites if the proven layout requires traversal through those
sections. SAV-22 is the completed classification task, not a package decoder.

SAV-07 supplies the completed created-form identity collision fix used by this
branch. Actor initialization still requires the correct reference and base.
The existing restoration path already handles supported reference and NPC
changes, saved template choices and private bases from leveled creatures.
SAV-17 owns standalone created-base restoration. Zero explicit created bases
in this survey does not exclude another valid save using one. Add that dependency
only if the established actor-value restoration path requires it. This audit
does not add speculative package or created-base dependency edges.

## Evidence needed to continue

No consulted source supplies a complete field definition and no controlled
fixture in the available corpus isolates modifier domains. Search exhaustion
does not finish SAV-64. A useful fixture set needs recorded Skyrim build and
save versions, actor identities, exact operations and independently observed
component values. Vary one component at a time, then compare combinations with
equal current totals but different permanent, temporary and damage splits.
Include base overrides, reload observations and both player and NPC actors.
Account for all intervening fields before assigning offsets to a component.

Once those observations establish a layout, a decoder can preserve exact
components and reject malformed or unsupported sections without guessing.
Restoration must validate identities and component data before changing actor
state, then verify independent current and permanent queries and SOEW reload.
Until then, this checkpoint provides source and corpus evidence only.
