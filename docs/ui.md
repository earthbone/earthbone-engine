# Native UI and SWF execution

The native Godot menus and the later SWF execution track share load-order
asset resolution. UI-19 converts winning art and retains JPEXS ActionScript
outside the checkout. It does not execute ActionScript. UI-20 resolves cached
imports and configured font families. Authored control placement and the
full UI-20 and UI-30 theme comparisons remain unfinished.
The MCM bridge executes original Papyrus scripts through the shared Game VM.

UI-23's execution research lives in
[ui-frameworks.md](ui-frameworks.md#running-mod-swf-menus-through-ruffle-ui-23).
It separates SWF tags and header signatures from Scaleform APIs and Skyrim's
GameDelegate operations. It records the pinned Ruffle gaps, per-part estimates
and proposed work. The Ruffle runtime is not linked into the host.

The local references are SkyUI Community's `source/actionscript`,
`source/scripts` and FFDec tools, Ruffle's `swf` reader, and the research lead
at `research/engine-map/leads/ui-scaleform.md`. SkyUI source and assets remain
read-only references. The lead's license labels are not permission to copy
SkyUI code or art. The implementation and research tooling here are new code.

`survey_interface_extensions` reads the installed vanilla interface archive
and then its SkyUI override. It counts tag envelopes through Ruffle, exports
actual AS2 with JPEXS, and reports literal namespace references and delegate
calls per winning movie. Its output directory must be new and outside git
checkouts. Set SKYRIM_DATA, SOE_TEST_SKYUI and SOE_FFDEC_JAR, then run the tool
with an external directory such as `/tmp/soe-ui23-survey`. The generated
report contains counts and names, while decompiled files stay beside it.
This is a static survey, not a runtime compatibility verdict.

The 2026-10-17 survey covers 59 vanilla and 76 SkyUI-winning movies, including
exported GFX files. It found ExporterInfo (1000) and DefineExternalImage2
(1009), 22 embedded gfx class definitions, and the existing ExternalInterface
host boundary. SetText and Lock come from the movie helper. The linked
research note includes the counts, every vanilla delegate name, confirmed
native gaps at Ruffle fbb4ce8, and the implementation estimates.
