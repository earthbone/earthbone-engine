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
`soe-interface-2`, so generations from the single-atlas writer are rebuilt.

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
