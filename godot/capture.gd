extends SceneTree

## Windowed capture script for the streamed Bannered Mare cell (docs/godot-host.md,
## "Real cell"). Loads SKYRIM_DATA through CellStreamerNode.load_data, streams
## WhiterunBanneredMare with stream_cell, adds a light and an environment, computes
## the AABB of the built MeshInstance3D nodes, and saves three camera views as PNG
## under docs/images/. build_surface (crates/godot-host/src/mesh.rs, see docs/godot-host.md
## "Mesh") already converts positions to metres, so the AABB and the 1.7 m eye height
## below are plain Godot metres, no further unit scaling needed.

const CELL_NAME := "WhiterunBanneredMare"
const OUT_DIR := "res://../docs/images"
const EYE_HEIGHT := 1.7

var frame := 0
var wait_frames := 5
var streamer: Node3D
var scene_root: Node3D
var shots: Array = []
var shot_index := 0

func _initialize() -> void:
	var skyrim_data := OS.get_environment("SKYRIM_DATA")
	print("SKYRIM_DATA=", skyrim_data)
	print("Godot version: ", Engine.get_version_info().string)

	if not ClassDB.class_exists("CellStreamerNode"):
		printerr("FAIL: CellStreamerNode is not registered, the extension did not load")
		quit(1)
		return

	if skyrim_data == "":
		printerr("FAIL: SKYRIM_DATA is not set")
		quit(1)
		return

	scene_root = Node3D.new()
	scene_root.name = "Root"
	get_root().add_child(scene_root)

	streamer = ClassDB.instantiate("CellStreamerNode") as Node3D
	if streamer == null:
		printerr("FAIL: could not instantiate CellStreamerNode")
		quit(1)
		return
	streamer.name = "Streamer"
	scene_root.add_child(streamer)

	if not streamer.call("load_data", skyrim_data):
		printerr("FAIL: load_data could not open %s" % skyrim_data)
		quit(1)
		return

	var built: int = streamer.call("stream_cell", CELL_NAME)
	print("capture: cell = %s, mesh nodes built = %d" % [CELL_NAME, built])
	if built <= 0:
		printerr("FAIL: stream_cell built no meshes for %s" % CELL_NAME)
		quit(1)
		return

	# Ambient light so unlit-material surfaces still read in the capture.
	var sun := DirectionalLight3D.new()
	sun.name = "Sun"
	sun.rotation_degrees = Vector3(-55.0, -35.0, 0.0)
	sun.light_energy = 1.2
	scene_root.add_child(sun)

	var env_node := WorldEnvironment.new()
	env_node.name = "Env"
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.05, 0.05, 0.07)
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.6, 0.6, 0.65)
	env.ambient_light_energy = 0.6
	env_node.environment = env
	scene_root.add_child(env_node)

	var aabb := _mesh_aabb(streamer)
	print("capture: AABB position = %s, size = %s" % [aabb.position, aabb.size])
	var center := aabb.get_center()

	var longest_axis_x := aabb.size.x >= aabb.size.z
	var eye := Vector3(center.x, aabb.position.y + EYE_HEIGHT, center.z)

	var cam := Camera3D.new()
	cam.name = "Cam"
	scene_root.add_child(cam)

	# Camera 1: centre, looking along +X (or +Z if the room is longer on Z).
	var dir_a := Vector3(1, 0, 0) if longest_axis_x else Vector3(0, 0, 1)
	shots.append({"pos": eye, "look": eye + dir_a, "name": "bannered-mare-1"})

	# Camera 2: centre, looking the opposite way along the longest axis.
	shots.append({"pos": eye, "look": eye - dir_a, "name": "bannered-mare-2"})

	# Camera 3: a corner, looking back at the centre.
	var corner := Vector3(
		aabb.position.x + aabb.size.x * 0.1,
		eye.y,
		aabb.position.z + aabb.size.z * 0.1
	)
	shots.append({"pos": corner, "look": Vector3(center.x, eye.y, center.z), "name": "bannered-mare-3"})

	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(OUT_DIR))

	_apply_shot(cam, 0)

func _mesh_aabb(node: Node) -> AABB:
	var result := AABB()
	var first := true
	for child in node.get_children():
		if child is MeshInstance3D:
			var mi := child as MeshInstance3D
			var local_aabb := mi.get_aabb()
			var xform := mi.global_transform
			var corners := [
				xform * local_aabb.position,
				xform * (local_aabb.position + Vector3(local_aabb.size.x, 0, 0)),
				xform * (local_aabb.position + Vector3(0, local_aabb.size.y, 0)),
				xform * (local_aabb.position + Vector3(0, 0, local_aabb.size.z)),
				xform * (local_aabb.position + Vector3(local_aabb.size.x, local_aabb.size.y, 0)),
				xform * (local_aabb.position + Vector3(local_aabb.size.x, 0, local_aabb.size.z)),
				xform * (local_aabb.position + Vector3(0, local_aabb.size.y, local_aabb.size.z)),
				xform * (local_aabb.position + local_aabb.size),
			]
			for c in corners:
				if first:
					result = AABB(c, Vector3.ZERO)
					first = false
				else:
					result = result.expand(c)
	return result

func _apply_shot(cam: Camera3D, index: int) -> void:
	var shot: Dictionary = shots[index]
	cam.global_position = shot["pos"]
	cam.look_at(shot["look"], Vector3.UP)
	cam.current = true
	frame = 0
	shot_index = index

func _process(_delta: float) -> bool:
	frame += 1
	if frame < wait_frames:
		return false

	var shot: Dictionary = shots[shot_index]
	var img := scene_root.get_viewport().get_texture().get_image()
	var out_path := "%s/%s.png" % [OUT_DIR, shot["name"]]
	var err := img.save_png(out_path)
	var abs_path := ProjectSettings.globalize_path(out_path)
	if err == OK:
		print("SAVED: %s" % abs_path)
	else:
		printerr("FAIL: could not save %s (error %d)" % [abs_path, err])

	if shot_index + 1 < shots.size():
		var cam := scene_root.get_viewport().get_camera_3d()
		_apply_shot(cam, shot_index + 1)
		return false

	print("RESULT: DONE")
	quit(0)
	return true
