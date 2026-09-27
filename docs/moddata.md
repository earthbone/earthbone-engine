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
| `_KID.ini` | Keyword Item Distributor | Attaches keywords to records by filter and enforces exclusive keyword groups | `src/kid.rs` |
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
  a swap. `ObjectProperties.cpp` and `.h` define transform, scale and flag
  operations. `RNG.cpp` gives the reference seed and chance behavior.
- **Open Animation Replacer**, `../engine-refs/OpenAnimationReplacer/`.
  `Parsing.cpp` gives the directory layout, that `priority` is required rather
  than defaulted, and that `user.json` is read after `config.json`. GPL-3.0 with
  modding and linking exceptions.
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

## What remains as written

One field keeps its text instead of being modelled, because its grammar varies
by item type.

- Keyword Item Distributor's traits field has a different grammar for every
  item type. An armour trait and a weapon trait share no syntax, and `Traits.h`
  dispatches to a different class per type.

Base Object Swapper's property field is parsed into `pos`, `rot`, `scale` and
`flags` operations. Position and rotation support exact values and ranges.
`R` makes position or rotation relative. Rotation input uses degrees and is
stored in radians. Scale multiplies the reference scale unless suffixed with
`A`, which sets it. `flags` adds hexadecimal bits and `flagsC` clears them.
Ranges are sampled deterministically from the reference ID. `ChanceL` uses the
location and base IDs for a stable chance decision. These values are not
claimed to match BOS's C++ random sequence. Conditional section filters are
not evaluated by this runtime.

A swap line's fourth-field `chance(...)` is checked when its reference is
built. A failed check leaves both the base and attached properties unchanged.

Open Animation Replacer's conditions are structural for a different reason: the
plugin lets other plugins register new condition types at runtime, so the set is
open ended and no fixed list would stay correct. Each condition keeps its name,
its negation, the plugin it needs and its raw parameters, which is enough to
report what a mod asks for and to implement the common ones later.

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

No mods are installed on this machine, so the gated real-file tests have nothing
to read. Every fixture in the test suite is authored from the syntax examples in
the sources above and is marked as authored rather than real. That is the one
honest gap in this crate: the grammars are taken from the parsers that consume
them, but no shipped mod has been run through them yet.

| Test suite | Count | What it covers |
|---|---|---|
| `src/*` unit | 64 | Each grammar, BOS property operations, KID exclusive group entries, each filter rule, the ini dialect, the form reference ambiguity |
| `tests/resolve.rs` | 12 | Resolution against a synthetic load order, including absent forms |

## Not done

- No real mod has been parsed, see above.
- The KID traits field and Open Animation Replacer's `user.json` override
  layer.
- Exact chance selection and actor lifecycle compatibility, detailed below.

## Applying, added 2026-09-18

`crates/game/src/moddata.rs` applies supported rules, and
`Game::apply_mod_data` takes the data roots a profile layers, reads every file
under them, resolves it and applies it. `Game::apply_mod_data_from` takes a
resolved `soe_profile::Loadout` instead, which already knows those roots in
order.

Call it before `Game::start`. This engine applies distributions before scripts
run. Fractional SPID rules marked with `!` are kept until an actor reference is
built.

### Where each format lands, and why there

| Format | Applied | Where |
|---|---|---|
| `_KID.ini` | At load | Merged into `World::keywords` itself |
| `_DISTR.ini` | At load | Base rules are read when actor state is built, deterministic fractional rules are selected per actor reference |
| `_SWAP.ini` | Base swaps and property overrides are held, applied per reference | Read when a reference is built |
| OAR | Counted only | The animation system will read the trees |

The keyword merge is the one worth stating plainly. A distributed keyword goes
into the same list `World::keywords` returns for a record's own `KWDA`, so
`HasKeyword` and every native that reads keywords cannot tell a distributed one
from a shipped one. A side table this module alone consulted would have passed
a weaker test and failed a real mod, which is why the lookup changed rather
than gaining a neighbour.

KID `ExclusiveGroup` entries are gathered across all discovered KID files
before keyword rules run. Lines with the same group name add included keywords
and remove excluded keywords. When a rule would add a keyword, the engine skips
it if another member of any shared group is already on the target record,
including a keyword from the plugin or an earlier KID rule.

Ordinary SPID distribution results are keyed by `NPC_` base because actor
state is built from its base on first use. Fractional deterministic rules
marked with `!` instead wait for an actor reference, where the same base can
produce different results for different actors.

A swap is not a load time event at all. The reference it names may not exist
yet, so the swap and property overrides are held until the reference is built.
An explicit reference rule takes precedence over a rule for the original base,
and a rule for the original base applies when no reference rule exists. The
runtime writes transform and flag changes to `RefState`. Applied flags persist
in SOEW version 36. `World::base_inventory` does the same for a distributed
item, so `GetItemCount` counts it the way it counts a shipped one.

### Chance, and what it costs

`soe_moddata` parses SPID's percent and trailing `!` separately. Missing or
`NONE` chance means 100 percent. Invalid, non-finite or out-of-range values
are reported as unreadable lines instead of becoming guaranteed rules.
Tests cover `0!`, `50!`, `100!`, ordinary percentages and malformed values.

Ordinary SPID chance rolls occur per matching NPC base using the world's
seeded generator. Deterministic `0!` and `100!` work without a random choice.
Fractional deterministic rules retain the parsed entry value's line seed and
are evaluated when `World::actor` first builds the matching reference. The
roll seed combines that line seed with the player runtime FormID and the actor
runtime FormID, following the combine order in SPID's `LookupFilters.cpp`.
`PCLevelMultManager.cpp` uses the save character ID for the player seed, which
this runtime does not yet track. The engine uses FNV-1a for the entry value,
then expands the combined seed with SplitMix64 before the xorshift draw. This
gives stable per-entry and per-actor results, but does not reproduce upstream
per-character selection or SPID's C++ random sequence.
`world.mod_data` includes the retained actor rules and their selection inputs.

KID chance uses the upstream seed inputs from
`Keyword-Item-Distributor/src/Data/BaseFilters.cpp`: FNV-1a 32 of the keyword
editor ID paired with the target runtime FormID using Szudzik pairing. The
engine derives a stable unit value from that seed for each keyword and target,
so KID chance does not consume the world RNG. Its random generator is not
claimed to reproduce upstream output bit for bit. The `world.mod_data` hash
covers the resulting keywords, distributions and swaps for engine replay.

### What is applied and what is only carried

Spells, levelled spells, perks, outfits, sleep outfits and factions reach the
actor. Items reach the inventory. Packages, shouts, skins and bare forms are
resolved, held in `mod_data` and counted in the report, but nothing consumes
them yet: a package needs a third source in the driver's stack and a save format
change, and the other three are not yet connected to distribution.

A distribution carrying a level or trait filter is **skipped** and counted in
`skipped_unsupported_filter`, never applied. Those filters are not evaluated
yet, and applying such a rule to every actor would be worse than not applying
it. A test pins that direction.

### Measured

The gated scan over the installed game reports zero of everything, because the
install is vanilla and ships no mod using any of these formats. It starts
reporting the moment one is installed, which remains the crate's one honest gap:
every fixture is still authored.

| Test suite | Count | What it covers |
|---|---|---|
| `crates/game/src/moddata.rs` unit | 3 | The item type to record tag map, swap fallthrough, empty distribution |
| `crates/game/tests/moddata_apply.rs` | 15 | Each format through the game's own query paths, KID group merging and exclusions, seeded chance, the skip direction, determinism, and the gated scan |
