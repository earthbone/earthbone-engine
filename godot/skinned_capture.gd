extends SceneTree

## Windowed capture of the vanilla body on its animated skeleton
## (docs/godot-host.md, "Skinned characters"). Loads the character, plays the
## idle, then the walk, and saves a PNG of each under docs/images/. The body
## renders unlit grey because a character's materials are not wired yet, so
## what these show is the silhouette and the pose, which is the point.
##
##   SKYRIM_DATA="..." godot --path <engine>/godot --script skinned_capture.gd \
##     --resolution 1600x900
##
## Do not pass --headless, there is nothing to read back from.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
const OUT_DIR := "res://../docs/images"

var actor: Node3D
var scene_root: Node3D
var frame := 0
var shots: Array = []
var shot_index := 0
var settle := 5


func _initialize() -> void:
	var data_dir := OS.get_environment("SKYRIM_DATA")
	if data_dir == "":
		printerr("FAIL: SKYRIM_DATA is not set")
		quit(1)
		return
	if not ClassDB.class_exists("SkinnedActorNode"):
		printerr("FAIL: SkinnedActorNode is not registered")
		quit(1)
		return

	scene_root = Node3D.new()
	scene_root.name = "Root"
	get_root().add_child(scene_root)

	actor = ClassDB.instantiate("SkinnedActorNode") as Node3D
	actor.name = "Actor"
	scene_root.add_child(actor)
	if not actor.call("load_character", data_dir, BODY):
		printerr("FAIL: load_character returned false")
		quit(1)
		return

	var s: Dictionary = actor.call("stats")
	print("capture: %d shapes, %d triangles, %d of %d bones matched"
		% [s["shapes"], s["triangles"], s["matched_bones"], s["mesh_bones"]])
	print("  %d partitions, %d of %d shapes remapped"
		% [s["partitions"], s["remapped_shapes"], s["shapes"]])

	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-35.0, -40.0, 0.0)
	sun.light_energy = 1.3
	scene_root.add_child(sun)

	var env_node := WorldEnvironment.new()
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.06, 0.07, 0.09)
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.7, 0.7, 0.75)
	env.ambient_light_energy = 0.7
	env_node.environment = env
	scene_root.add_child(env_node)

	# A character is about 1.8 m tall standing at the origin, so frame it from
	# the front at chest height a couple of metres back.
	var cam := Camera3D.new()
	cam.name = "Cam"
	scene_root.add_child(cam)
	# look_at needs the node in the tree, which it is not yet during
	# _initialize, so aim from the position instead.
	cam.look_at_from_position(Vector3(0.0, 1.0, 2.6), Vector3(0.0, 0.9, 0.0), Vector3.UP)
	cam.current = true

	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(OUT_DIR))
	shots = [
		{"name": "skinned-idle", "walk": false, "frames": 60},
		{"name": "skinned-walk", "walk": true, "frames": 40},
	]


func _process(_delta: float) -> bool:
	if shots.is_empty():
		return true
	frame += 1
	var shot: Dictionary = shots[shot_index]
	if frame == 1 and shot["walk"]:
		actor.call("start_walking")
	if frame < int(shot["frames"]) + settle:
		return false

	var img := scene_root.get_viewport().get_texture().get_image()
	var out_path := "%s/%s.png" % [OUT_DIR, shot["name"]]
	var err := img.save_png(out_path)
	var abs_path := ProjectSettings.globalize_path(out_path)
	if err == OK:
		print("SAVED: %s  clips %s" % [abs_path, actor.call("active_clips")])
	else:
		printerr("FAIL: could not save %s (error %d)" % [abs_path, err])

	if shot_index + 1 < shots.size():
		shot_index += 1
		frame = 0
		return false
	print("RESULT: DONE")
	quit(0)
	return true
