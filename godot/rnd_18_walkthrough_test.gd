extends "res://walkthrough.gd"

var sky_frames := 0
var lighting_checked := false

func _process(delta: float) -> bool:
	if is_instance_valid(player):
		player.set("auto_step", false)
	if loaded:
		sky_frames += 1
		for node in root.find_children("*", "Node3D", true, false):
			if node.get_script() == RecordSky:
				var frame: Dictionary = node.last_frame
				lighting_checked = frame.has("rotation") and frame.has("ambient") and node.environment.ambient_light_color == frame.ambient
		if not lighting_checked:
			printerr("RND-18 WALKTHROUGH: interior lighting did not reach the environment")
			quit(1)
			return false
		if sky_frames == 8:
			frames = 119
	return super._process(delta)

func _finalize() -> void:
	if loaded and lighting_checked:
		print("RND-18 WALKTHROUGH: PASS")
	super._finalize()
