# swf-convert: interface art conversion

The UI-19 converter reads SWF tags with Ruffle `swf` 0.3, tessellates shape
fills and strokes with Lyon, decodes bitmaps and writes Godot 4.4 resources.
The ordinary load-order cache retains movie metadata and JPEXS ActionScript
decompilation outside git checkouts. It does not execute those scripts.

DefineFont2 and DefineFont3 outlines rasterize at 64 pixels into indexed
atlas pages, each at most 1024 by 2048 pixels. Glyphs retain SWF index order,
character codes, advances, offsets and outline meshes. The FontFile writer
assigns each page to its Godot texture index and each glyph to its page's
UV rectangle. Fonts exceeding 256 MiB of atlas storage return an invalid-art
error before allocating the pages. The content-addressed cache format is
`soe-interface-3`, so older generations are rebuilt.

The atlas resource layout follows the local `godot-docs` FontFile
`set_texture_image`, `set_glyph_texture_idx` and `set_glyph_uv_rect` contracts.
The bounded tag reader follows the tag lengths and typed readers in
[Ruffle swf 0.3](https://docs.rs/swf/latest/swf/). Root movies require a
zero-length End tag, as does Ruffle's `read_tag_list`. The installed SkyUI icon
sprites omit End and some also omit their final ShowFrame, so bounded nested
sprites may end at their declared byte boundary and retain that final display
list as a frame. A present End tag must have an empty payload at every level.
Truncated tag payloads remain errors. The full Ruffle clone is absent from the
local corpus.

JPEXS's local `SkyUI-Community/tools/FFDec/ffdec-cli` launcher and JAR supply
the reference decompiler. The installed-data checks build native caches for
vanilla and winning SkyUI inventory and magic menus with that actual JAR,
check the exported ActionScript functions, and verify cache reuse. Other
UI-19 checks compare source tags with text, names, placements and glyphs,
convert all winning vanilla and SkyUI movies, and load a paged font in a
windowed Godot viewport to check pixels from two pages. Generated resources,
decompiled scripts and captures are external test or ignored build outputs.

No dependency was added for paging. `cargo tree -p soe-swf-convert` includes
the existing Ruffle parser's bitstream, enum-map, encoding, zlib, LZMA and
ASN.1 support. ASN.1 brings num-bigint and time. Lyon supplies tessellation,
tiny-skia supplies rasterization, image supplies PNG, JPEG and GIF decoding,
and serde, SHA-256 and tempfile supply cache encoding, hashing and publication.
The Ruffle runtime and Java libraries are not linked into the engine.

UI-27 supplies each FFDec process with a rewound temporary file as stdin,
still addressed through `/dev/stdin`. This preserves the existing CLI input
contract and avoids writing to a pipe after an early child exit, which would
terminate a Godot host through SIGPIPE. Nonzero exit status remains a typed
decompiler error, allowing the host to select its bundled theme. The temporary
input is removed when the process releases it. The `ui_27_` early-exit test
checks that an executable rejecting input returns that error.

`write_art` writes only a supplied Movie's ArrayMesh and ImageTexture
definitions. It emits no fonts, linkage names, text, buttons, timelines,
scripts or movie metadata. Callers select the permitted dictionary before
writing it. UI-24 uses this entry point for its checked NORDIC theme.

The implementation shares the existing UI-19 resource writer, whose sources
are Ruffle's shape utilities and Godot's ArrayMesh, FontFile and ImageTexture
resource encoding. UI-24 adds no parser or resource format to this crate.

## Installed validation, 2026-10-17

The winning vanilla and SkyUI survey converts 70 interface movies. The full
cache check publishes all 70 with real JPEXS exports. Vanilla inventory and
magic exports contain 68 and 55 source files, and SkyUI winners contain 71
and 69. The Chinese font retains all 17,514 glyphs across 30 atlas pages.
The paged font render check uses an isolated Godot project so host diagnostics
autoloads do not require an imported GDExtension. It keeps the original pixel
thresholds and saves the frame before checking them, including on failure.
Generated art and decompiled scripts remain external inputs and outputs.

UI-30 adds Java executable discovery for the existing JPEXS process. It reads
SOE_JAVA, JAVA_HOME, PATH and conventional Linux install paths, following the
local FFDec launcher. A selected JAR uses that absolute executable instead of
requiring java on PATH. No parser or native cache format changes.

UI-20 retains the winning `interface/fontconfig.txt` beside the generated
resources and includes its bytes in the generation key. A changed alias or
font library therefore rebuilds the cache even when the SWFs stay unchanged.
The file remains external user data. The native host reads its fontlib and map
declarations using the local SkyUI `build/fontconfig.txt` as the reference.
The converter does not parse these declarations.

FontFile style uses numeric Godot FontStyle flags, bold bit 0 and italic bit 1.
Godot 4.4.1's ResourceSaver and `get_font_style` confirm that representation.
The earlier string `font_style` and unsupported `font_italic` property did not
preserve those flags. The writer now emits the numeric field with the existing
family, weight, metrics and glyph atlas data. No dependency was added.

UI-30 preserves explicit End tags in the bounded reader. A nonempty sprite
closed by End may contain fewer frames than declared. Its timeline retains
the declared count and the actual display lists without padding. Strict
`convert` still rejects root frame mismatches. The cache uses `convert_art`,
which also retains closed short roots. NORDIC's loading menu declares two
root frames but closes after one. Excess frames, truncated payloads and
unterminated short timelines remain errors in both paths. The existing
malformed-input assertions are unchanged.

The local NORDIC 2.4.1 survey found four closed short sprites: loading menu
IDs 4 and 50 at 438 of 466 and 150 of 300 frames, journal ID 281 at 81 of 90,
and bottom-bar ID 24 at 200 of 250. The corpus has no Ruffle runtime clone.
[Ruffle's MovieClip preloader](https://github.com/ruffle-rs/ruffle/blob/master/core/src/display_object/movie_clip.rs)
was read on 2026-10-17. It finishes on End or the bounded stream and retains
separate loaded and declared frame information. The native converter uses
the narrower closed-short-sprite case needed by these fixtures. It does not
run ActionScript or copy the runtime implementation. The existing resource
writer remains the output path.

NORDIC's HUD sprite 874 also contains one matrix-only PlaceObject2 with both
action bits clear. Its depth is already occupied. The local CommonLib
`GFxPlaceObject2.h` confirms the flag layout. Ruffle 0.3 rejects that action
combination, while
[JPEXS's timeline](https://github.com/jindrapetrik/jpexs-decompiler/blob/master/libsrc/ffdec_lib/src/com/jpexs/decompiler/flash/timeline/Timeline.java)
ignores it on an occupied depth. The converter validates its complete matrix
through Ruffle with a temporary move flag, then retains tag 26 as a diagnostic
without changing the display list. It accepts only the matrix-only form.
The temporary buffer's bound of 29 bytes follows the flag and depth fields plus
the matrix's maximum 203 bits. Truncated and trailing data remain errors.
The regression uses authored synthetic transforms and checks that the prior
placement survives. No installed layout bytes are included in the test.

The final 2026-10-17 run passes all four UI-30 regressions, including the
35-movie NORDIC survey, ten existing adversarial checks and three library
tests. Exactly four short sprites and the loading menu's short root remain
visible in metadata. All-target clippy and crate formatting pass. The earlier
seven UI-20 checks also pass, including the Godot font-style round trip.
This validates conversion and resource loading, not complete menu rendering.
