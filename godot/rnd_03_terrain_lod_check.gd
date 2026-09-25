extends SceneTree

## Judge-written acceptance check for RND-03's done check: "Terrain LOD
## replaces distant full resolution terrain within a recorded residency
## budget". Declared budget before running: against real BTR data,
## CellStreamerNode.stream_terrain_lod must produce at least one resident LOD
## block with recorded, positive bytes and at least one drawn RenderingServer
## instance (the "replaces distant full resolution terrain" clause, observed
## as something the running extension actually does), and a second call with
## budget_bytes capped at half of what the first call recorded must bring
## terrain_lod_usage_bytes back at or under that cap (the "recorded residency
## budget" clause). Revision note: an earlier version of this script called
## stream_exterior("Tamriel", 2), which is a bug in this test, not in the
## code under test: stream_exterior takes an exterior cell's editor id, see
## DataSource::resolve_exterior_block, not a worldspace id, and "Tamriel"
## names no cell. That call is replaced below with stream_terrain_lod, which
## does take a worldspace id (DataSource::resolve_world).
var failures := 0

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _initialize() -> void:
	call_deferred("_run")

func _run() -> void:
	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: host extension is not loaded, CellStreamerNode missing")
		print("RESULT: FAIL terrain lod streaming")
		quit(1)
		return

	var streamer = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)

	# Declared budget: RND-03 must expose every one of these from a running
	# scene, or TerrainLodStreamer and DataSource::load_lod_block are library
	# code CellStreamerNode never drives.
	var required_methods := [
		"stream_terrain_lod",
		"update_terrain_lod",
		"terrain_lod_resident_count",
		"terrain_lod_usage_bytes",
		"terrain_lod_stats",
		"clear_terrain_lod",
	]
	var missing: Array[String] = []
	for m in required_methods:
		if not streamer.has_method(m):
			missing.append(m)
	_check(
		missing.is_empty(),
		"CellStreamerNode is missing terrain LOD methods: " + str(missing) +
		", so RND-03's TerrainLodStreamer and DataSource::load_lod_block cannot be driven " +
		"from a running scene"
	)

	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("SKYRIM_DATA not set, skipping the live replace-and-budget check")
		streamer.free()
		print("RESULT: ", "PASS" if failures == 0 else "FAIL", " terrain lod streaming")
		quit(0 if failures == 0 else 1)
		return

	_check(streamer.call("load_data", data), "load_data failed on installed data")

	# Cell (-67, -35) sits inside the level 4 Tamriel LOD block confirmed
	# present on this install (see terrain_lod_real_data_adversarial.rs and
	# crates/godot-host/tests/rnd_03_terrain_lod_check.rs, block (-68, -36)).
	# full_res_radius 0 and a generous lod_radius stand LOD in for every block
	# beyond the focus's own cell, unlimited budget so this first call proves
	# blocks are actually read and drawn, not only planned.
	var resident: int = streamer.call("stream_terrain_lod", "Tamriel", -67, -35, 0, 8, 0)
	_check(resident >= 0, "stream_terrain_lod failed on installed data")
	_check(
		resident > 0,
		"stream_terrain_lod found no resident LOD blocks near a confirmed real BTR block, " +
		"so nothing stands in for distant terrain here"
	)

	var stats: Dictionary = streamer.call("terrain_lod_stats")
	var usage: int = stats.get("usage_bytes", 0)
	var instances: int = stats.get("instances", 0)
	_check(usage > 0, "resident LOD blocks recorded 0 bytes, the budget clause has nothing real to enforce")
	_check(
		instances > 0,
		"resident LOD blocks drew 0 RenderingServer instances, nothing was actually drawn in " +
		"place of distant full resolution terrain"
	)
	print("unlimited budget: resident=", resident, " usage_bytes=", usage, " instances=", instances)

	# Second call, same focus, budget capped at half of what the first call
	# actually recorded: declared from a real reading, not a number picked to
	# trivially pass, so eviction has real work to do.
	var budget: int = int(usage / 2)
	_check(budget > 0, "half the real recorded usage must still be a positive budget to be a real cap")
	var capped: int = streamer.call("stream_terrain_lod", "Tamriel", -67, -35, 0, 8, budget)
	_check(capped >= 0, "stream_terrain_lod failed under a real budget cap")
	var capped_stats: Dictionary = streamer.call("terrain_lod_stats")
	var capped_usage: int = capped_stats.get("usage_bytes", 0)
	_check(
		capped_usage <= budget,
		"terrain_lod_usage_bytes " + str(capped_usage) + " exceeds the declared budget " + str(budget) +
		" after stream_terrain_lod: the residency budget was not enforced"
	)
	_check(
		not bool(capped_stats.get("over_budget", true)),
		"over_budget must clear once usage fits the declared cap"
	)
	print("capped budget: resident=", capped, " usage_bytes=", capped_usage, " budget=", budget)

	streamer.call("clear_terrain_lod")
	_check(
		int(streamer.call("terrain_lod_resident_count")) == 0,
		"clear_terrain_lod must leave no LOD blocks resident"
	)

	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " terrain lod streaming")
	quit(0 if failures == 0 else 1)
