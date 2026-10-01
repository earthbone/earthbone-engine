# soe-moddata: the formats mods ship instead of code

`crates/moddata` reads four configuration formats that popular SKSE plugins
invented. A large share of modern mods carry no DLL and no script. They ship an
ini file or a directory tree, and without the plugin that reads it the mod does
nothing at all.

That makes these formats a compatibility surface exactly like the native
function list, and `commonlib-shim` does not cover it, because the thing to be
compatible with here is not a function but a file. So the engine reads the
formats itself and the mods work with no plugin present. This is the project
rule applied once more: replace the workaround with a native system rather than
reimplement the workaround.

| Format | Plugin | What it does | Files |
|---|---|---|---|
| `_DISTR.ini` | Spell Perk Item Distributor | Hands spells, perks, items, shouts, packages, outfits and skins to actors by filter | `src/spid.rs` |
| `_KID.ini` | Keyword Item Distributor | Attaches keywords to records by filter | `src/kid.rs` |
| `_SWAP.ini` | Base Object Swapper | Swaps base objects on placed references, and overrides their properties | `src/bos.rs` |
| directory tree | Open Animation Replacer | Picks animations by condition, and the older Dynamic Animation Replacer layout | `src/oar.rs` |

## Parsing and resolving are separate

A configuration is read with no game data in hand, so a file naming a plugin the
user does not have still parses. Resolution is the second step: `resolve_spid`,
`resolve_kid` and `resolve_bos` take a parsed file and a `soe_esm::LoadOrder`
and produce `Patch` values, each one a form and the change to make to it. A rule
whose form is absent drops out there, which is what the plugins report and skip.

Nothing in this crate touches a running world. Applying a `Patch` belongs to the
game crate, implemented in `crates/game/src/moddata.rs` with limits below.

## What was read, and what it gave

Every format here was taken from its plugin's own source. Nothing was guessed
from a wiki page and nothing was copied.

- **Keyword Item Distributor**, `../engine-refs/Keyword-Item-Distributor/`.
  `LookupConfigs.cpp` gives the field order and that every line uses the key
  `Keyword` in the anonymous section. `EnumDefs.h` gives the nineteen item type
  names including the two word ones. `BaseFilters.cpp` gives the whole filter
  grammar and, more usefully, `DistributableFilterSet::Pass` gives the matching
  rule, which is not what a reader would guess from the syntax. Its `LICENSE`
  is GPL-3.0 even though its `README.md` says MIT, so the licence file is what
  this crate treated as binding.
- **Spell Perk Item Distributor**, `../engine-refs/Spell-Perk-Item-Distributor/`.
  The field order is a comment on line 3 of `Defs.h`, which is the clearest
  specification any of these formats has. `LookupConfigs.h` gives the record
  type names, the `Final` and `Death` key prefixes, and the level, trait and
  count grammars. MIT.
- **Base Object Swapper**, `../engine-refs/BaseObjectSwapper/`. `Manager.cpp`
  gives the section walk and the `SetAllowKeyOnly` call that makes a line's key
  carry the whole payload. `SwapData.cpp` gives the field order on both sides of
  a swap.
- **Open Animation Replacer**, `../engine-refs/OpenAnimationReplacer/`.
  `Parsing.cpp` gives the directory layout, that `priority` is required rather
  than defaulted, and that a submod `user.json` replaces its config data while
  retaining the author config name. Legacy numbered submods also accept a
  `user.json` priority and condition list. GPL-3.0 with modding and linking
  exceptions.
- **dargh**, `../engine-refs/dargh/`, MIT, documents the older condition and
  priority scheme that the legacy layout still uses.

Licence care: this crate is GPL-3.0-or-later like the rest of the core, so
reading the GPL sources above is fine here. Nothing derived from them may reach
the MIT shim crates.

## Two filter grammars, kept apart

The two distributors look similar and are not. Flattening one onto the other
would silently change which mods work, so both are modelled as their authors
wrote them.

Spell Perk Item Distributor sorts every token into one of four flat buckets by
its leading character: `+` joins into `ALL`, `-` goes to `NOT`, `*` goes to
`ANY` as a substring, and a bare token goes to `MATCH`. All four must agree.

Keyword Item Distributor instead builds groups. A comma separated entry that
contains `+` becomes one AND group, groups are OR'd against each other, and `-`
and `*` apply per token inside a group. Its `ANY` bucket has a rule worth
stating plainly, because the syntax does not suggest it: if the bucket holds any
exact token then one exact token must hit, and if it holds any partial token
then one partial token must hit, and both conditions apply at once. So
`Guard, *Mag` needs an exact `Guard` **and** something containing `Mag`, not
either one.

## An ambiguity inherited from the plugins

A bare token made only of hex digits reads as a form id, not an editor id,
because `RawForm` tries a hex parse before falling back. A keyword genuinely
named `Ace` or `Beef` is therefore a form id to all four plugins and to this
crate. `0x` makes it explicit, and any non-hex letter makes it a name. This is
pinned by a test rather than left to be rediscovered inside a mod that quietly
stops working.

## What is deliberately left as written

Two fields keep their text instead of being modelled, because modelling them
badly would be worse than handing the caller a string.

- Keyword Item Distributor's traits field has a different grammar for every
  item type. An armour trait and a weapon trait share no syntax, and `Traits.h`
  dispatches to a different class per type.
- Base Object Swapper's property field is its own small language of `pos`,
  `rot`, `scale` and `flags` calls, with a location based `ChanceL` variant.

Open Animation Replacer's conditions remain structural in `soe-moddata` because
the plugin lets other plugins register new condition types at runtime. The game
runtime evaluates `IsFemale`, `IsActorBase` and `IsRace`. Unknown, unresolved or
plugin-registered condition types do not select a replacement. JSON condition
arrays are ANDed. Legacy `OR` blocks are kept as groups, alternatives within a
group are ORed, and separate groups are ANDed.

For a modern submod, `config.json` must supply its name. A sibling `user.json`
replaces the submod's data, including its required priority, conditions and
optional disabled flag, while retaining the name from `config.json`. The
priority must fit an `i32`, since it orders the scanned submods. A legacy
numbered directory with `_conditions.txt` accepts `user.json` as an OAR
override, using its priority, conditions and optional disabled flag instead of
the directory number and text conditions. `AnimationMod::select_submod`
returns the highest-priority enabled submod accepted by a caller-provided
condition matcher. Equal priorities retain scan order. The condition matcher
remains outside this crate because OAR condition types can be registered by
other plugins. `overrideAnimationsFolder` selects a sibling directory as the
submod's animation source. If that directory is missing, the submod is skipped,
matching `Parsing.cpp`.

## Conflicts resolve by file name

All three ini formats resolve a conflict the same way: the file whose name sorts
last alphabetically wins. `discover` therefore returns files sorted by path, and
a `Patch` carries the file and line it came from so a conflict can be reported
rather than silently applied. Base Object Swapper documents this behaviour, and
a third party tool exists only to override it, which is a fair sign that
alphabetical order is a poor conflict rule. The engine keeps the behaviour
because mods depend on it, and the source on every patch is what a better rule
would be built from later.

## Dependencies

Six crates beyond the workspace: `serde_json` and its four (`serde_core`,
`itoa`, `memchr`, `zmij`), plus `soe-esm` which is in-workspace and free.
`serde_json` is there for Open Animation Replacer's `config.json` and nothing
else. Writing a JSON reader by hand would have been the reimplementation the
project rule forbids, and this is the ecosystem standard at four transitive
crates.

The ini dialect is ours, at about a hundred lines. No crate packages it: these
files use multi-key sections, key-only lines and condition bearing section
headers, and a standard ini crate models none of that.

## Measured

The installed game Data directory has no `_DISTR`, `_KID` or `_SWAP` files, so
the gated real-file tests have nothing to read. Most fixtures in the test suite
are authored from the syntax examples in the sources above. The MOD-07
provenance regression checks that SPID, KID and BOS patches retain their input
file and line.

MOD-07 also carries one real KID rule from DeviousDevicesNG as an inline test
fixture. Its source is `Devious Devices_KID.ini` at revision
`6b89b4160fffe62e38c9d3d777e66fb3a06b8419`, licensed Apache-2.0. The test
parses the exact rule, resolves its keyword against a synthetic load order,
and checks the source path and line. The rule refers to a form supplied by
Schlongs of Skyrim, so the test models that keyword record rather than needing
the external mod installed. No real SPID, BOS or OAR mod fixture is available
yet, and this test does not run the rule through `soe-game`.

| Test suite | Count | What it covers |
|---|---|---|
| `src/*` unit | 64 | Each grammar, BOS property operations, KID exclusive group entries, each filter rule, the ini dialect, the form reference ambiguity |
| `tests/resolve.rs` | 14 | Resolution against a synthetic load order, including absent forms, patch provenance for all three ini formats and the real KID rule |
| `tests/mod_04_adversarial.rs` | 4 | A user priority override changes selection, disabled overrides exclude submods, user conditions replace author conditions, and an empty animation folder keeps the submod path |
| `tests/mod_04_breaker_adversarial.rs` | 4 | Negative and maximum priorities select correctly, missing user priority does not fall back, and out of range priority is rejected |
| `game/tests/mod_04_breaker_2_adversarial.rs` | 2 | User priority selects the game clip and `overrideAnimationsFolder` supplies it from a sibling directory |

## Not done

- MOD-07 covers one real KID rule. Real SPID, BOS and OAR mod fixtures and
  game-side application of a real mod remain untested.
- Keyword Item Distributor's exclusive groups, which share the file under other
  keys and are currently reported as skipped lines.
- The two fields left as written.
- Exact ordinary SPID and KID chance selection, detailed below.

## Applying, added 2026-09-18

`crates/game/src/moddata.rs` applies supported rules, and
`Game::apply_mod_data` takes the data roots a profile layers, reads every file
under them, resolves it and applies it. `Game::apply_mod_data_from` takes a
resolved `soe_profile::Loadout` instead, which already knows those roots in
order.

Call it before `Game::start`. The load pass resolves rules. Deterministic SPID
rules are evaluated for each actor as its state is built.

### Where each format lands, and why there

| Format | Applied | Where |
|---|---|---|
| `_KID.ini` | At load | Merged into `World::keywords` itself |
| `_DISTR.ini` | At load, keyed by base | Read when an actor's state is built |
| `_SWAP.ini` | Held, applied per reference | Read when a reference is built |
| OAR | Parsed submods are retained by `Game`; callers can select the highest-priority enabled submod matching actor conditions | Runtime animation playback will consume the selected paths |

The keyword merge is the one worth stating plainly. A distributed keyword goes
into the same list `World::keywords` returns for a record's own `KWDA`, so
`HasKeyword` and every native that reads keywords cannot tell a distributed one
from a shipped one. A side table this module alone consulted would have passed
a weaker test and failed a real mod, which is why the lookup changed rather
than gaining a neighbour.

`AnimationMod::select_submod` selects among parsed OAR submods using a caller's
condition matcher and skips disabled submods. `Game::select_oar_animation`
binds supported conditions to actor records and returns a matching clip path.
Automatic animation playback does not yet consume that path.

Distribution is keyed by `NPC_` base rather than applied to loaded actors,
because an actor's state is built from its base on first use. A reference that
loads an hour later still receives what its base was given.

A swap is not a load time event at all. The reference it names may not exist
yet, so the swap is held and `RefState`'s base is rewritten when the reference
is built. `World::base_inventory` does the same for a distributed item, so
`GetItemCount` counts it the way it counts a shipped one.

### Chance, and what it costs

`soe_moddata` parses SPID's percent and trailing `!` separately. Missing or
`NONE` chance means 100 percent. Invalid, non-finite or out-of-range values
are reported as unreadable lines instead of becoming guaranteed rules.
Tests cover `0!`, `50!`, `100!`, ordinary percentages and malformed values.

Ordinary SPID chance rolls occur per matching NPC base using the world's
seeded generator. Deterministic `0!` and `100!` work without a random choice.
Fractional deterministic rules are evaluated for each actor when its state is
built. Their seed combines the parsed entry seed, the active save character ID
and the actor runtime FormID, following SPID's `LookupFilters.cpp`. The host
sets the active ID with `World::set_player_character_id`. This input metadata
is not serialized by the engine. SPID obtains the value from its save manager,
or from the character ID in a standard save filename. References are SPID
`SPID/src/LookupConfigs.h`, `Defs.h`, `LookupFilters.cpp` and
`PCLevelMultManager.cpp`, especially `ChanceComponentParser` and
`GetPlayerIDFromSave`.

KID uses a different deterministic seed in `src/Data/BaseFilters.cpp`, based
on keyword editor ID and target form. Our KID application still rolls once
per rule with the world RNG. Ordinary chance and the conversion from a
deterministic SPID seed to a draw remain engine policy, not an exact upstream
random sequence. The `world.mod_data` hash covers the resulting keywords,
distributions and swaps for engine replay.

### What is applied and what is only carried

Spells, levelled spells, perks, outfits, sleep outfits, factions, packages and
shouts reach actor state. SPID package field 5 is a zero-based insertion
position in the actor base's `PKID` list. The actor retains those entries and
positions in a named actor save section, and `ai::candidates` inserts them into
the base package list before schedule and condition evaluation. A package
FormList contributes its package members in list order. A package already in
the list is not inserted again. Repeated shout rules for one form add it once
to the learned shout set, which is included in the engine save format. Items
reach the inventory. SPID `Skin` rules update
the NPC base's `WNAM` override and actor appearance assembly retains the
resolved `ARMO` identity and uses that record's armor addons. Generic `Form`
rules infer the supported type
from the resolved record signature and use that type's consumer. Following
SPID's schema, generic inference treats armor as `Item` and outfits as
`Outfit`, since `Skin` and `SleepOutfit` cannot be inferred from signatures.
Package rules have a runtime consumer. Generic forms with an unsupported
signature are skipped and counted in `skipped_unsupported_consumer`. Explicit
types with a signature outside SPID's declared schema are rejected through the
same counter.

The type list, type inference limits and generic signature groups come from
Spell Perk Item Distributor's `resources/SPID Complete Reference.txt` and
`SPID/src/LookupConfigs.h`. Shout state semantics follow the CK `AddShout - Actor`
page. NPC skin distribution uses the existing `WNAM` override path.

A distribution's actor level, skill levels, class skill weights, sex, unique,
summonable, child, leveled and teammate filters are checked against the
record-backed NPC base before distribution. A rule that fails one of those
filters is ignored. A `starts-dead` filter remains skipped and counted in
`skipped_unsupported_filter`, because it belongs to a placed actor and the
supported filter path reads NPC base data. Deterministic chance is evaluated
per placed actor when its state is built. The checks use SPID's
`LookupFilters.cpp` and `LookupNPC.cpp` behavior for level and traits, with the
record fields available to this engine.

### Measured

The gated scan over the installed game reports zero of everything, because the
install is vanilla and ships no mod using any of these formats. It starts
reporting the moment one is installed. The gated scan reports formats in the
installed game, but the current vanilla installation has no such mod files.

| Test suite | Count | What it covers |
|---|---|---|
| `crates/game/src/moddata.rs` unit | 3 | The item type to record tag map, swap fallthrough, empty distribution |
| `crates/game/tests/moddata_apply.rs` | 27 | Each format through the game's own query paths, level and trait filter matches and misses, the actor-only skip, chance, deterministic actor rules, consumer validation and the gated scan |
| `crates/game/tests/mod_08_distributed_shouts.rs` | 2 | A duplicate shout distribution enters learned state once and survives whole-game save/load |
