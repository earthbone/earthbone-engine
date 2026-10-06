# swf-convert: interface art conversion

The UI-19 converter reads SWF tags with Ruffle `swf` 0.3, tessellates shape
fills and strokes with Lyon, decodes bitmaps and writes Godot 4.4 resources.
The ordinary load-order cache retains movie metadata and JPEXS ActionScript
decompilation outside git checkouts. It does not execute those scripts.

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
