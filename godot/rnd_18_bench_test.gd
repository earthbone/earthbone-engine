extends "res://bench.gd"

func _process(_delta: float) -> bool:
	if not streamed or done:
		return false
	done = true
	var environments := root.find_children("*", "WorldEnvironment", true, false)
	var skies: Array[Node] = []
	for node in root.find_children("*", "Node3D", true, false):
		if node.get_script() == preload("res://record_sky.gd"):
			skies.append(node)
	var passed := environments.size() == 1 and skies.size() == 1
	if passed:
		var sky = skies[0]
		var frame: Dictionary = sky.last_frame
		passed = frame.weather_sky and frame.sky_visible and not frame.has("rotation") \
			and environments[0].environment == sky.environment \
			and sky.environment.background_mode == Environment.BG_SKY \
			and sky.sun.visible and sky.sun.light_color == frame.sunlight
	print("RND-18 BENCH: ", "PASS" if passed else "FAIL")
	quit(0 if passed else 1)
	return false
