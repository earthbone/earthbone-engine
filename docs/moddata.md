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

## What is deliberately left as written

Two fields keep their text instead of being modelled, because modelling them
badly would be worse than handing the caller a string.

- Keyword Item Distributor's traits field has a different grammar for every
  item type. An armour trait and a weapon trait share no syntax, and `Traits.h`
  dispatches to a different class per type.
- Base Object Swapper's property field is its own small language of `pos`,
  `rot`, `scale` and `flags` calls, with a location based `ChanceL` variant.

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
| `src/*` unit | 60 | Each grammar, each filter rule, the ini dialect, the form reference ambiguity |
| `tests/resolve.rs` | 11 | Resolution against a synthetic load order, including absent forms |

## Not done

- No real mod has been parsed, see above.
- Keyword Item Distributor's exclusive groups, which share the file under other
  keys and are currently reported as skipped lines.
- The two fields left as written, and Open Animation Replacer's `user.json`
  override layer.
- Exact chance selection and actor lifecycle compatibility, detailed below.

## Applying, added 2026-09-18

`crates/game/src/moddata.rs` applies supported rules, and
`Game::apply_mod_data` takes the data roots a profile layers, reads every file
under them, resolves it and applies it. `Game::apply_mod_data_from` takes a
resolved `soe_profile::Loadout` instead, which already knows those roots in
order.

Call it before `Game::start`. This engine applies distributions once at load.
That is a current limitation, not a claim of matching SPID's actor lifecycle.

### Where each format lands, and why there

| Format | Applied | Where |
|---|---|---|
| `_KID.ini` | At load | Merged into `World::keywords` itself |
| `_DISTR.ini` | At load, keyed by base | Read when an actor's state is built |
| `_SWAP.ini` | Held, applied per reference | Read when a reference is built |
| OAR | Counted only | The animation system will read the trees |

The keyword merge is the one worth stating plainly. A distributed keyword goes
into the same list `World::keywords` returns for a record's own `KWDA`, so
`HasKeyword` and every native that reads keywords cannot tell a distributed one
from a shipped one. A side table this module alone consulted would have passed
a weaker test and failed a real mod, which is why the lookup changed rather
than gaining a neighbour.

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
Fractional deterministic rules are skipped and counted in
`skipped_unsupported_chance`. Their per-actor seed and lifecycle are not
implemented. References are SPID `SPID/src/LookupConfigs.h`, `Defs.h` and
`LookupFilters.cpp`, especially `ChanceComponentParser` and the deterministic
branch combining line seed, player ID and actor FormID.

KID uses a different deterministic seed in `src/Data/BaseFilters.cpp`, based
on keyword editor ID and target form. Our KID application still rolls once
per rule with the world RNG. Neither distributor should be described as
matching upstream chance selection. The `world.mod_data` hash covers the
resulting keywords, distributions and swaps for engine replay.

### What is applied and what is only carried

Spells, levelled spells, perks, outfits, sleep outfits and factions reach the
actor. Items reach the inventory. Packages, shouts, skins and bare forms are
resolved, held in `mod_data` and counted in the report, but nothing consumes
them yet: a package needs a third source in the driver's stack and a save format
change, and the other three are not yet connected to distribution.

A distribution's actor level, skill levels, class skill weights, sex, unique,
summonable, child, leveled and teammate filters are checked against the
record-backed NPC base before distribution. A rule that fails one of those
filters is ignored. A `starts-dead` filter remains skipped and counted in
`skipped_unsupported_filter`, because it belongs to a placed actor and this
startup path applies distributions per NPC base. MOD-01's per-actor lifecycle
work is not present in this worktree, so different placed actors using one
base cannot receive different filter results or deterministic chance results.
The checks use SPID's `LookupFilters.cpp` and `LookupNPC.cpp` behavior for
level and traits, with the record fields available to this engine.

### Measured

The gated scan over the installed game reports zero of everything, because the
install is vanilla and ships no mod using any of these formats. It starts
reporting the moment one is installed, which remains the crate's one honest gap:
every fixture is still authored.

| Test suite | Count | What it covers |
|---|---|---|
| `crates/game/src/moddata.rs` unit | 3 | The item type to record tag map, swap fallthrough, empty distribution |
| `crates/game/tests/moddata_apply.rs` | 12 | Each format through the game's own query paths, level and trait filter matches and misses, the actor-only skip, chance, determinism, and the gated scan |
