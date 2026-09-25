extends SceneTree

## HST-04 acceptance check: "Disabling or moving a reference updates
## rendering and collision consistently at the next host tick," run inside a
## live Godot process so `CellStreamerNode::sync_reference_changes` and
## `CellBodies::sync_placements` execute against real RenderingServer and
## PhysicsServer3D singletons, the one thing no `cargo test` can do.
##
## What this proves: across many real ticks of a loaded cell, the new tick
## hook (`gameplay.rs`'s `advance_game`, inserted before `advance_exterior`)
## runs every frame without erroring, and repeatedly re-applying "nothing
## changed" leaves `collision_stats` and `instance_count` exactly where they
## started, so the cache comparisons in `ref_cache` and `StaticEntry::synced`
## are not spuriously rebuilding, freeing or leaking a body on a tick where
## nothing moved.
##
## What this does NOT prove: no `#[func]` on `CellStreamerNode` or
## `PlayerNode` lets GDScript flip a reference's `enabled` flag or call
## `move_to` at runtime (confirmed by reading every `#[func]` in
## crates/godot-host/src/godot_layer.rs and godot_player.rs), and the
## `Console` debug command layer that does support `enable`/`disable`/
## `moveto` directly on `World` (crates/game/src/debug.rs) is documented
## there as "later an in-game console, a separate plan item" and is not
## wired into this crate. So the causal claim, that a live disable or move
## actually changes what is drawn and collided with, is not exercised by any
## test here or in the breaker's suite. `hst_04_placement_sync_check.rs`
## proves the value both sync functions read (`World::ref_placement`)
## reflects a disable, a re-enable and a move correctly, and this script
## proves the two consumers of that value run safely, tick after tick,
## inside the engine that would apply it. Closing the remaining gap needs a
## new `#[func]` (or a Console binding) on the production side, which is
## outside a test writer's remit.
##
## Run with:
##   SKYRIM_DATA="/path/to/Data" godot --headless --path godot --script hst_04_placement_sync_test.gd

var failures := 0
const CELL := "WhiterunBanneredMare"
const TICKS := 180

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		print("SKYRIM_DATA not set or extension not loaded, skipping")
		print("RESULT: PASS hst_04 placement sync (skipped)")
		quit(0)
		return

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(streamer.call("stream_cell", CELL) > 0, "initial render failed")
	_check(streamer.call("stream_cell_collision", CELL) > 0, "initial collision failed")

	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared source failed")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0, "player collision failed")
	_check(player.call("start_gameplay"), "runtime start failed")

	if failures == 0:
		var before_instances: int = streamer.call("instance_count")
		var before_collision: Dictionary = streamer.call("collision_stats")

		# Drive the real tick hook repeatedly. Each `advance` call runs
		# `PlayerNode::physics_process` -> `advance_game`, which now calls
		# `sync_reference_changes` before anything else reads `World` this
		# tick (gameplay.rs, HST-04). Nothing here mutates a reference, so
		# every call exercises the "nothing changed" branch of both sync
		# functions inside a live RenderingServer/PhysicsServer3D.
		for i in range(TICKS):
			player.call("advance", 1.0 / 60.0)

		var after_instances: int = streamer.call("instance_count")
		var after_collision: Dictionary = streamer.call("collision_stats")
		_check(after_instances == before_instances,
			"instance_count drifted with no reference changed: %d -> %d" %
			[before_instances, after_instances])
		_check(after_collision == before_collision,
			"collision_stats drifted with no reference changed: %s -> %s" %
			[before_collision, after_collision])
		_check(player.call("is_on_ground"), "player lost ground support across %d idle ticks" % TICKS)
		print("hst_04 idle-tick stability: instances=", after_instances,
			" collision=", after_collision, " ticks=", TICKS)

	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " hst_04 placement sync")
	quit(0 if failures == 0 else 1)
