# Why the Creation Engine is so moddable, and what this engine keeps

Written 2026-09-06 from section 3 of `../research/skyrim-open-engine-map.md` and the verified
briefs. This is the design the new engine preserves on purpose, because it is what lets a
total conversion the size of Fallout: New Vegas ship in eighteen months and lets a scene of
thousands of authors work on one game without touching its source.

## The engine is a record database with an interpreter on top

Every object in the game is a form: a typed record with a 32-bit FormID, stored in a plugin
file. The engine does not compile a level. It loads every plugin in load order into one form
table and the world is whatever that table says. A mod is therefore just more records, or
replacement records, and the mod author never needs the engine's source, a build step, or a
level editor's project file.

Three consequences follow, and each one is a property the new engine must keep.

1. **Late binding by identity.** The top byte of a FormID is the index of the plugin that
   owns the form in the current load order. That makes every cross-plugin reference a stable
   identity rather than a pointer or a path, so plugins can reference each other's content and
   the load order can change without anything breaking. The engine resolves the byte at load,
   so a mod written against `Skyrim.esm` works whichever slot `Skyrim.esm` occupies.
2. **Winning override.** When two plugins carry the same FormID, the later one in the load
   order wins, field by field for whole records. Conflict resolution is a sort, and the tools
   the scene built (xEdit, LOOT, Wrye Bash, Synthesis) are all views over the same rule.
   Patching is data, not code.
3. **Every system reads forms.** Rendering reads STAT and REFR, AI reads PACK and NAVM,
   quests read QUST and DIAL, scripts read properties bound to forms. There are no
   engine-private structures a modder cannot reach through the file format, which is why the
   data-side reverse engineering (UESP, xEdit's definitions) covers nearly the whole game.

## Scripting is data too

Papyrus scripts compile to bytecode stored as files, attach to forms by name, and keep their
state in the save. A script is a record like any other, so a mod can add behaviour without
patching the engine, and two mods' scripts coexist because they only meet through forms and
events. The limit of this design is the native surface: Papyrus can only do what the engine
exposes as natives, which is why SKSE exists. The new engine keeps the VM and the file format
and makes the native registry an explicit, extensible seam instead of a hook target.

## Assets resolve by path through an archive overlay

Meshes, textures and sounds are referenced by relative path from records. The engine looks up
that path first in loose files, then in archives, in load order. Replacing an asset is placing
a file. No repacking, no manifest, no reimport.

## What the scene had to bolt on, and what the new engine does natively

The workarounds are the engine's missing features made visible. Each is a design requirement.

| Bolt-on | What it worked around | Native answer |
|---|---|---|
| SKSE and Address Library | No native extension API | The native registry and an MIT shim of the CommonLib surface |
| DynDOLOD, xLODGen | LOD is precomputed by a closed tool | LOD generation inside the engine and the editor, from the same form table |
| Nemesis, Pandora, FNIS | Behaviour graphs are compiled Havok data with no merge step | A behaviour runtime that reads graphs as data and merges at load |
| BodySlide, Outfit Studio | No runtime morphing, closed mesh formats | Morph targets in the neutral mesh format and glTF import |
| ENB, Community Shaders | No post-process or shader extension point | A ReShade-shaped hook and shader overrides on materials |
| xEdit, Synthesis | The Creation Kit cannot patch conflicts | Winning-override view and patchers as first-class editor tools |
| CKPE | The Creation Kit crashes and lacks features | A new editor on a maintained host |
| Mod managers with VFS tricks | The game reads one Data folder | The engine reads a loadout directly |

## What stays deliberately compatible

Plugin files, archives, mesh and texture formats, script bytecode and the save format are
inputs the engine reads unchanged, so existing mods, tools and knowledge keep working. New
formats (glTF, PNG, a data behaviour format) are additions with converters, never
replacements, until the scene has moved on its own.

That last clause is a sequencing rule, and it cannot bootstrap on its own. The scene will
not move while the open path is only an alternative way to reach the same result, and the
open path will not improve while it waits for the scene. So the open path has to win
decisively for new content before anyone is asked to port old content. Concretely: an
author who wants a new weapon with a custom animation should ship a glTF and a data
behaviour file, with no Nemesis run, no hkx export and no FNIS registration. If that
works, adoption follows on its own merit, because nobody has to convert anything they
already have. Old content keeps loading unchanged either way, which is what makes the
open path safe to offer before it is complete.
