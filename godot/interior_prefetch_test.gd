extends SceneTree

## Installed host acceptance for authored-door CPU prefetch and resource reuse.
const STEP_BUDGET_US := 50000
const READY_TIMEOUT_MS := 30000
# Matches crate::interior_prefetch::MAX_PAYLOAD_BYTES. WhiterunBanneredMare's
# real payload measured 186.7 MiB on 2026-09-24, above the previous 128 MiB
# budget, so the Rust-side cap moved to 384 MiB and this check follows it.
const PAYLOAD_BUDGET := 384 * 1024 * 1024
var failures := 0

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: interior_prefetch_test requires SKYRIM_DATA and the extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", "WhiterunBanneredMare") > 0, "initial render failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"initial collision failed")
	_check(player.call("start_gameplay"), "runtime start failed")
	var start: String = player.call("gameplay_cell")
	var chosen: Dictionary = {}
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			chosen = door
			break
	_check(not chosen.is_empty(), "no unlocked load door")
	if not chosen.is_empty():
		_check(player.call("activate_reference", chosen.file, chosen.object), "outbound activation failed")
		var outside: String = player.call("gameplay_cell")
		_check(outside != start, "outbound door did not change cells")
		var instances: int = streamer.call("instance_count")
		var state: Dictionary = player.call("interior_prefetch_status")
		_check(state.started == 0, "activation speculatively loaded a destination")
		var begin := Time.get_ticks_msec()
		var max_step_us := 0
		var frames := 0
		while state.ready.is_empty() and Time.get_ticks_msec() - begin < READY_TIMEOUT_MS:
			var tick := Time.get_ticks_usec()
			player.call("advance", 1.0 / 60.0)
			max_step_us = maxi(max_step_us, Time.get_ticks_usec() - tick)
			frames += 1
			await process_frame
			state = player.call("interior_prefetch_status")
		_check(state.ready == start, "nearby return door did not prepare its linked interior")
		_check(state.started == 1 and not state.pending, "prefetch exceeded the one-job budget")
		_check(state.payload_bytes > 0 and state.payload_bytes <= PAYLOAD_BUDGET,
			"prepared destination exceeded payload budget or was empty")
		_check(max_step_us <= STEP_BUDGET_US, "prefetch blocked gameplay step: %d us" % max_step_us)
		_check(player.call("gameplay_cell") == outside, "prefetch moved the logical player")
		_check(streamer.call("instance_count") == instances, "prefetch replaced active geometry")
		var prepared_bytes: int = state.payload_bytes
		_check(player.call("activate_reference", chosen.far_file, chosen.far_object), "return activation failed")
		state = player.call("interior_prefetch_status")
		_check(state.hits == 1, "transition rebuilt resources instead of consuming prefetch")
		_check(state.ready.is_empty(), "consumed destination was retained")
		_check(player.call("gameplay_cell") == start, "return did not restore interior identity")
		_check(streamer.call("instance_count") > 0, "prefetched interior has no draw instances")
		for i in range(120):
			player.call("advance", 1.0 / 60.0)
		_check(player.call("is_on_ground"), "prefetched collision did not support the landing")
		print("INTERIOR PREFETCH frames=", frames, " max_step_us=", max_step_us,
			" ready_ms=", Time.get_ticks_msec() - begin, " payload_bytes=", prepared_bytes,
			" status=", state)
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " interior prefetch")
	quit(0 if failures == 0 else 1)
