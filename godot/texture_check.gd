extends SceneTree

## Headless check that Godot takes block-compressed bytes as they are: a 4
## by 4 DXT1 image (one 8 byte block) and a 4 by 4 DXT1 image with its mip
## chain (8 + 8 + 8 bytes for 4, 2 and 1 pixel levels) go through
## Image.create_from_data and ImageTexture.create_from_image, and the format
## reads back unchanged. This is what data_source's compressed texture path
## relies on, so it runs before any windowed measurement.
##
##   godot --headless --path godot --script texture_check.gd


func _initialize() -> void:
	var block := PackedByteArray([0x00, 0xF8, 0x1F, 0x00, 0x00, 0x00, 0x00, 0x00])
	var checks := [
		["dxt1_4x4", 4, 4, false, Image.FORMAT_DXT1, block],
		["dxt1_4x4_mips", 4, 4, true, Image.FORMAT_DXT1, block + block + block],
		["dxt5_4x4", 4, 4, false, Image.FORMAT_DXT5, block + block],
		["rgtc_rg_4x4", 4, 4, false, Image.FORMAT_RGTC_RG, block + block],
		["bptc_rgba_4x4", 4, 4, false, Image.FORMAT_BPTC_RGBA, block + block],
		["dxt1_8x8_mips", 8, 8, true, Image.FORMAT_DXT1, block + block + block + block + block + block + block],
	]
	var failed := 0
	for check in checks:
		var image := Image.create_from_data(check[1], check[2], check[3], check[4], check[5])
		if image == null:
			print("texture_check %s create_from_data refused" % check[0])
			failed += 1
			continue
		var texture := ImageTexture.create_from_image(image)
		var ok: bool = image.get_format() == check[4] and texture != null and image.has_mipmaps() == check[3]
		print("texture_check %s format %d mipmaps %d texture %s" % [check[0], image.get_format(), image.get_mipmap_count(), "ok" if ok else "FAIL"])
		if not ok:
			failed += 1
	if failed == 0:
		print("RESULT: PASS")
		quit(0)
	else:
		printerr("RESULT: FAIL: %d checks failed" % failed)
		quit(1)
