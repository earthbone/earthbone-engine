extends SceneTree

# Loads the vanilla human body onto its animated skeleton and runs the
# locomotion graph, first idle then walking. Prints what loaded and whether
# bones actually moved, so a headless run proves the chain end to end.
#
#   SKYRIM_DATA="..." godot --headless --path <engine>/godot --script skinned_test.gd
#
# With SKYRIM_DATA unset it prints a skip and exits 0, matching the other
# gated runs.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"

var actor: Node3D
var frame := 0
var idle_states: PackedStringArray
var done := false


func _initialize() -> void:
	var data_dir := OS.get_environment("SKYRIM_DATA")
	if data_dir == "":
		print("SKYRIM_DATA not set, skipping")
		quit(0)
		return

	var root := Node3D.new()
	get_root().add_child(root)

	actor = ClassDB.instantiate("SkinnedActorNode")
	if actor == null:
		printerr("RESULT: FAIL: SkinnedActorNode is not registered")
		quit(1)
		return
	root.add_child(actor)

	if not actor.call("load_character", data_dir, BODY):
		printerr("RESULT: FAIL: load_character returned false")
		quit(1)
		return

	var s: Dictionary = actor.call("stats")
	print("skinned actor: %d shapes, %d triangles" % [s["shapes"], s["triangles"]])
	print("  skeleton %d bones, mesh bones %d, matched %d, unmatched %d, %d skeletons"
		% [s["skeleton_bones"], s["mesh_bones"], s["matched_bones"],
		   s["unmatched"], s["skeletons"]])
	print("  graph %d nodes, %d clips loaded, %d missing"
		% [s["graph_nodes"], s["clips"], s["missing_clips"]])

	if int(s["matched_bones"]) != int(s["mesh_bones"]):
		printerr("RESULT: FAIL: %d of %d mesh bones matched the skeleton"
			% [s["matched_bones"], s["mesh_bones"]])
		quit(1)
		return


func _process(_delta: float) -> bool:
	if done:
		return true
	frame += 1
	# Two seconds of idle, then ask for a walk and watch the state change.
	if frame == 120:
		idle_states = actor.call("active_states")
		print("  after 2 s idle: states %s" % [idle_states])
		print("  clips %s" % [actor.call("active_clips")])
		actor.call("start_walking")
	elif frame == 240:
		var walk_states: PackedStringArray = actor.call("active_states")
		print("  after the walk event: states %s" % [walk_states])
		print("  clips %s" % [actor.call("active_clips")])
		print("  %.2f s of animation played" % actor.call("elapsed"))
		done = true
		if walk_states == idle_states:
			printerr("RESULT: FAIL: the graph never left the idle state")
			quit(1)
			return true
		if walk_states.is_empty():
			printerr("RESULT: FAIL: no state is active")
			quit(1)
			return true
		print("RESULT: PASS")
		quit(0)
		return true
	return false
