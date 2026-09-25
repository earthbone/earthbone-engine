# soe-profile: load order, INI layering and manager profiles

Turns a folder layout plus the game's list files into what the engine should read: the
Data roots in shadowing order, every plugin with its enabled state and FormID slot, the
archives in opening order, and one merged view of the INI settings. Mod Organizer 2
profiles resolve directly, so a manager loadout is playable without a virtual file
system in between.

## What the conventions are, and where they come from

**`plugins.txt`** lists plugins one per line, an asterisk in front of the active ones,
`#` for comments. Skyrim SE keeps it under `%LOCALAPPDATA%\Skyrim Special Edition\`,
MO2 keeps one per profile. **`loadorder.txt`** is the manager convention (MO2, LOOT)
for the order of every plugin including disabled ones, bare names. Both are read with a
byte order mark and CRLF tolerated, and written with CRLF as the game does.

**The engine's own order.** The five hardcoded masters load first, then the creations
named in `Skyrim.ccc` in the game folder, in that file's order, all active whatever the
list says. A creation that is not on disk is skipped, the shipped `Skyrim.ccc` names
content the install may not own. After those, masters load before plain plugins, each
group keeping the list's relative order. A file is a master by the ESM header flag or by
the `.esm` or `.esl` extension, and light by the ESL flag or the `.esl` extension. An
ESM-flagged `.esp` therefore sorts with the masters, and an ESL-flagged `.esp` stays in
plugin position but takes a light slot. Active regular files take top bytes `00` to
`FD`, light files a twelve-bit slot under `FE`, and going past either count is an error.
Sources: LMO `src/plugins/loadorder.rs` and `ccc.rs`, UESP Mod File Format, the
installed game.

**INI precedence.** `Skyrim_Default.ini` in the game folder, then `Skyrim.ini`,
`SkyrimCustom.ini`, `SkyrimPrefs.ini` from the settings folder, the later one winning
(STEP, Skyrim Configuration Settings). A plugin's INI, `X.ini` next to `X.esp`, applies
above those only while the plugin is active, replaces a list value wholesale, and cannot
change a setting the engine reads at startup, the archive lists first among them (UESP,
INI files). `Settings` ranks layers so the result does not depend on the order files are
applied in, and records every refused plugin write in `refused`.

**Archives.** `sResourceArchiveList` then `sResourceArchiveList2` from `[Archive]`, then
for each active plugin in load order `<stem>.bsa` and `<stem> - *.bsa` found across the
data roots. The second pattern is wider than the documented `<stem> - Textures.bsa`,
since the creations ship `<stem>.bsa` only and the wider match costs nothing. An INI
archive that no root has is kept with `path: None`, the engine skips what it cannot open.

**MO2 profiles.** `modlist.txt` is highest priority first, `+` enabled, `-` disabled,
`*` unmanaged. `Mo2Profile` reverses it so enabled mods read low to high, which is the
data-root order: the game's Data, each enabled mod directory, then `overwrite/`. A
later root shadows an earlier one file by file (`Loadout::find`). `settings.ini`
carries `LocalSettings` (the profile holds its own three INIs) and `LocalSaves`
(`profile/saves`). MO2 publishes no format, this follows what `profile.cpp` writes.

## Packaging

`Loadout::package(dest)` writes a resolved loadout to `dest` as a standalone
install, playable without MO2, Vortex or another manager reading the profile
at run time. Every mod manager stays a build-time step, not a runtime one.

The output:

- `dest/Data`, every data root merged in low to high order, a later root
  shadowing an earlier one file by file, the same rule `Loadout::find` uses.
  This is where mod files, plugin files and archives all land, since they
  already sit inside the data roots.
- `dest/plugins.txt` and `dest/loadorder.txt`, the resolved order rendered
  back with the same conventions they were read with.
- `dest/Skyrim.ccc`, the Creation Club names this loadout treated as early
  loaders, hardcoded masters excluded, so a reload sees the same early group.
- `dest/Skyrim.ini`, `dest/SkyrimCustom.ini`, `dest/SkyrimPrefs.ini`, the
  settings this loadout carries at the `Base`, `Custom` and `Prefs` layers.

A plugin's own INI is not serialized separately, it already sits next to the
plugin in a data root and is copied along with the plugin. `Skyrim_Default.ini`
and whatever it sets are not written either, that file belongs to the game
install the package lands in, not to the profile. Reload the package with a
`LoadoutInput` whose `data_roots` is `[dest.join("Data")]`, `plugins_txt` and
`loadorder_txt` point at the files above, `ini_dir` is `dest`, and `game_dir`
and `ccc` point at wherever `Skyrim_Default.ini` and the install's own
`Skyrim.ccc` live, `dest` itself for a fully standalone copy.

Use case: a headless engine run, a CI job, or a build step that turns a
manager profile into a folder the engine opens directly, closing round 4's
packaging row.

## API

- `PluginHeader::read(path)` reads only the TES4 record: masters, flags, kind.
- `order::resolve(&Inputs)` builds a `LoadOrder` from discovered headers, the two lists
  and the early-loading names, with `Problem`s for missing files, missing, disabled or
  late masters, and a missing hardcoded master.
- `LoadoutInput::vanilla(game_dir, my_games, local_appdata)` or
  `Mo2Instance::portable(root, game_dir, profile).loadout_input()`, then `resolve()`
  gives a `Loadout` with roots, order, settings and archives.
- `Settings::get_bool`, `get_int`, `get_uint`, `get_float`, `get_list`, `archives`,
  `source` (which layer a value came from).

## Tests

24 tests: INI parsing and layering including the startup-only refusal and wholesale
list replacement, header kinds by flag and extension with truncated and foreign files,
`plugins.txt` and `loadorder.txt` round trips, the ccc list, load-order resolution
(early loaders, master partition, slot assignment, missing and late masters, the 254
overflow, case-insensitive matching), an MO2 instance on a temp tree resolving to the
expected roots, order, INI values and archives, and a gated real-install test.

The real test against the Steam install (`SKYRIM_DATA=".../Skyrim Special Edition/Data"`)
resolves ten early loaders, the five masters at regular slots 0 to 4, four creations
and `_ResourcePack.esl` from `Skyrim.ccc`, three of them light at slots 0 to 2, and 23
archives from `Skyrim_Default.ini` plus one per creation. One finding: the shipped
`Skyrim_Default.ini` names `Skyrim - Patch.bsa`, which the install does not contain,
so an engine must tolerate a listed archive that is absent.

## Live run (2026-09-14)

Reran the gated test against the same installed copy. It resolved 7 regular masters
(Skyrim.esm, Update.esm, Dawnguard.esm, HearthFires.esm, Dragonborn.esm,
ccBGSSSE001-Fish.esm and ccBGSSSE025-AdvDSGS.esm), 3 light early loaders
(ccQDRSSE001-SurvivalMode.esl, ccBGSSSE037-Curios.esl and _ResourcePack.esl) and 23
archives, in the correct early loader order. No code change was needed.

## Not done yet

- No `plugins.txt` from `%LOCALAPPDATA%` was tested, the machine has no Windows profile,
  so the vanilla path was exercised with lists on a temp tree only.
- Vortex profiles are not read. Their state is a Redux store, not files, and the lead
  found no format to follow.
- Timestamp-ordered load orders (Skyrim LE and older) are out of scope, the crate is SE.
- The startup-only key list is the documented one plus STEP's examples. The engine's
  true set is not published, so `Settings::with_startup_only` lets a caller replace it.
