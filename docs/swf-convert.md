# swf-convert: interface art conversion

The UI-19 converter reads SWF tags with Ruffle `swf` 0.3, tessellates shape
fills and strokes with Lyon, decodes bitmaps and writes Godot 4.4 resources.
The ordinary load-order cache retains movie metadata and JPEXS ActionScript
decompilation outside git checkouts. It does not execute those scripts.

`write_art` writes only a supplied Movie's ArrayMesh and ImageTexture
definitions. It emits no fonts, linkage names, text, buttons, timelines,
scripts or movie metadata. Callers select the permitted dictionary before
writing it. UI-24 uses this entry point for its checked NORDIC theme.

The implementation shares the existing UI-19 resource writer, whose sources
are Ruffle's shape utilities and Godot's ArrayMesh, FontFile and ImageTexture
resource encoding. UI-24 adds no parser or resource format to this crate.
