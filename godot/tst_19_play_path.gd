# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"
## TST-19 observes the live walkthrough. The Rust parent watches a heartbeat
## from another thread, including while this main thread cannot run a timer.
const Checks := preload("res://tst_19_checks.gd")
const MARKET_MODEL := "wrmarket"
const OWNERS := {
	"npc.drawn": "HST-23", "npc.head_hair": "HST-23", "npc.pose": "HST-23",
	"doors.drawn": "HST-21", "doors.states": "HST-21",
	"fire.additive": "RND-33", "fire.hull": "RND-33",
	"terrain.textures": "RND-01", "object.white": "RND-37", "object.height": "RND-39",
	"compass.offset": "UI-28", "frame.max_ms": "PRF-38", "watchdog.max_ms": "PRF-38",
	"audio.stone": "AUD-14", "audio.grass": "AUD-14",
	"menu.inventory_theme": "UI-29", "menu.container_theme": "UI-29", "menu.dialogue_theme": "UI-30",
}

var mode := "gate"
var duration_s := 180.0
var route_start := 0
var previous_frame := 0
var worst_ms := 0.0
var stalls_ms := 0.0
var over_budget := 0
var measured_frames := 0
var stop_name := "startup"
var finished := false
var sampling := false
var next_sample := 0
var policy: Dictionary = {}
var observations: Dictionary = {}
var previous_poses: Dictionary = {}
var output_directory := ""
var heartbeat_path := ""
var moving := false
var step_requests: Array[Dictionary] = []
var step_materials: Dictionary = {}
var footstep_capture: AudioEffectCapture
var audio_bus := -1
var audio_voices: Array[AudioStreamPlayer3D] = []
var minimum_step_db := 100.0
var stone_steps := 0
var grass_steps := 0
var completed_loops := 0
var expected_failures := 0
var required_failures := 0
var exit_report_removed := false

func _initialize() -> void:
	if OS.get_environment("SOE_PLAY_PATH_SELF_TEST") == "1":
		call_deferred("_self_test")
		return
	mode = OS.get_environment("SOE_PLAY_PATH")
	if mode.is_empty():
		mode = "gate"
	if mode not in ["gate", "gpu"]:
		printerr("PLAY_PATH invalid mode: ", mode)
		quit(2)
		return
	duration_s = 180.0 if mode == "gate" else 600.0
	output_directory = OS.get_environment("SOE_PLAY_PATH_OUTPUT")
	heartbeat_path = OS.get_environment("SOE_PLAY_PATH_HEARTBEAT")
	var policy_path := OS.get_environment("SOE_PLAY_PATH_POLICY")
	if not policy_path.is_empty():
		var value: Variant = JSON.parse_string(FileAccess.get_file_as_string(policy_path))
		if value is Dictionary:
			policy = value
	else:
		var value: Variant = JSON.parse_string(FileAccess.get_file_as_string("res://tst_19_expected_failures.json"))
		if value is Dictionary:
			policy = value
			var backlog_path := ProjectSettings.globalize_path("res://").trim_suffix("/").get_base_dir().path_join("BACKLOG.csv")
			var statuses := {}
			for line in FileAccess.get_file_as_string(backlog_path).split("\n"):
				var fields := line.trim_prefix('"').trim_suffix('"').split('","')
				if fields.size() >= 4:
					statuses[fields[0]] = fields[3]
			for id in policy:
				policy[id].expected = statuses.get(policy[id].owner, "open") != "done"
	Engine.max_fps = 60
	OS.low_processor_usage_mode = false
	startup_position = Vector3(-125.0, -800.0, 120.0)
	super._initialize()

func _configure_player() -> void:
	player.set("capture_mouse", false)

func _start() -> void:
	await super._start()
	if not loaded:
		return
	_check("window", DisplayServer.get_name() != "headless" and root.size == Vector2i(640, 360), {"display": DisplayServer.get_name(), "size": str(root.size), "mode": mode, "DISPLAY": OS.get_environment("DISPLAY")})
	if mode == "gate":
		_check("gate_display", OS.get_environment("DISPLAY") == ":99", {"DISPLAY": OS.get_environment("DISPLAY")})
	if mode == "gpu":
		var adapter := RenderingServer.get_video_adapter_name().to_lower()
		_check("gpu_adapter", not "llvmpipe" in adapter and not "lavapipe" in adapter and not "software" in adapter, {"adapter": adapter})
	_disconnect_exit_report()
	_install_audio_probe()
	root.grab_focus()
	await process_frame
	route_start = Time.get_ticks_usec()
	previous_frame = route_start
	next_sample = route_start + 1_000_000
	_heartbeat()
	if not output_directory.is_empty():
		var image := await _capture()
		_check("capture", image.save_png(output_directory.path_join("startup.png")) == OK, {"path": "startup.png"})
	await _route()

func _process(_delta: float) -> bool:
	_disconnect_exit_report()
	if not loaded or finished or route_start == 0:
		return false
	var now := Time.get_ticks_usec()
	var frame_ms := (now - previous_frame) / 1000.0
	previous_frame = now
	stalls_ms = maxf(stalls_ms, frame_ms)
	if (now - route_start) / 1_000_000.0 >= Checks.WARMUP_S:
		measured_frames += 1
		worst_ms = maxf(worst_ms, frame_ms)
		if frame_ms > Checks.FRAME_MS:
			over_budget += 1
			if over_budget <= 3:
				_check("frame.max_ms", false, {"ms": frame_ms, "frame": measured_frames, "limit_ms": Checks.FRAME_MS})
	_drain_audio()
	_heartbeat()
	if now >= next_sample and not sampling and not player.call("is_menu_open"):
		next_sample = now + 1_000_000
		_sample_view()
	if _elapsed() >= duration_s and not sampling:
		_finish()
	return false

func _disconnect_exit_report() -> void:
	if exit_report_removed or not is_instance_valid(player) or not is_instance_valid(system_overlay):
		return
	var callback := Callable(system_overlay, "_write_on_player_exit")
	if player.is_connected("tree_exiting", callback):
		player.disconnect("tree_exiting", callback)
		exit_report_removed = true

func _finalize() -> void:
	_disconnect_exit_report()
	super._finalize()

func _heartbeat() -> void:
	if heartbeat_path.is_empty():
		return
	var file := FileAccess.open(heartbeat_path, FileAccess.WRITE)
	if file != null:
		file.store_string(JSON.stringify({"frame": measured_frames, "ticks": Time.get_ticks_usec(), "stop": stop_name, "armed": not finished, "elapsed_s": _elapsed()}))
		file.close()

func _elapsed() -> float:
	return (Time.get_ticks_usec() - route_start) / 1_000_000.0

func _check(id: String, passed: bool, measured: Dictionary) -> void:
	var owner := str(policy.get(id, {}).get("owner", OWNERS.get(id, "TST-19")))
	var expected: bool = not passed and bool(policy.get(id, {}).get("expected", false))
	var verdict := "PASS" if passed else ("XFAIL" if expected else "FAIL")
	observations[id] = int(observations.get(id, 0)) + 1
	if not passed:
		if expected:
			expected_failures += 1
		else:
			required_failures += 1
	print("PLAY_PATH ", JSON.stringify({"check": id, "status": verdict, "pass": passed, "owner": owner, "stop": stop_name, "elapsed_s": _elapsed() if route_start > 0 else 0.0, "value": measured}))

func _key(pressed: bool) -> void:
	moving = pressed
	var event := InputEventKey.new()
	event.keycode = KEY_W
	event.physical_keycode = KEY_W
	event.pressed = pressed
	Input.parse_input_event(event)
	Input.flush_buffered_events()

func _look_at(target: Vector3) -> void:
	var eye: Vector3 = player.call("position") + Vector3(0, 0, 115)
	var direction := target - eye
	player.call("set_facing", atan2(direction.x, direction.y), atan2(direction.z, Vector2(direction.x, direction.y).length()))

func _walk_to(target: Vector3, seconds: float = 15.0) -> bool:
	while sampling and not finished:
		await process_frame
	var ground: Dictionary = player.call("play_path_ground", target + Vector3(0, 0, 80))
	if not ground.is_empty():
		target.z = ground.height
	var points: PackedVector3Array = player.call("play_path_navigation", target)
	if points.is_empty():
		points.append(target)
	var start := Time.get_ticks_msec()
	var initial: Vector3 = player.call("position")
	var reached := true
	for point in points:
		while not finished:
			if sampling:
				await process_frame
				continue
			var position: Vector3 = player.call("position")
			var difference := point - position
			if Vector2(difference.x, difference.y).length() < 36.0:
				break
			if Time.get_ticks_msec() - start >= int(seconds * 1000.0):
				reached = false
				break
			player.call("set_facing", atan2(difference.x, difference.y), 0.0)
			_key(true)
			await process_frame
		if not reached or finished:
			break
	_key(false)
	var final: Vector3 = player.call("position")
	_check("route.walk", reached and initial.distance_to(final) > 1.0, {"destination": str(target), "position": str(final), "distance": initial.distance_to(final), "path_points": points.size(), "seconds": (Time.get_ticks_msec() - start) / 1000.0})
	return reached

func _reference(predicate: Callable) -> Dictionary:
	for reference: Dictionary in player.call("play_path_references"):
		if predicate.call(reference):
			return reference
	return {}

func _activate(reference: Dictionary) -> bool:
	if reference.is_empty():
		return false
	return player.call("activate_reference", reference.file, reference.object)

func _stop(name: String) -> void:
	stop_name = name
	_key(false)
	while sampling and not finished:
		await process_frame
	if finished:
		return
	await _sample_view()
	await create_timer(1.05).timeout
	while sampling and not finished:
		await process_frame
	if finished:
		return
	await _sample_view()
	sampling = true
	var image := await _capture()
	if not output_directory.is_empty():
		_check("capture", image.save_png(output_directory.path_join(name + ".png")) == OK, {"path": name + ".png"})
	sampling = false

func _route() -> void:
	stop_name = "hulda"
	var hulda := _reference(func(ref: Dictionary) -> bool: return ref.editor_id == "Hulda")
	if not hulda.is_empty():
		await _walk_to(hulda.position + Vector3(-160, -150, 0), 8.0)
		_look_at(hulda.position + Vector3(0, 0, 115))
	var activated := _activate(hulda)
	await process_frame
	var content: Control = player.call("menu_content")
	var topics: Array = content.find_children("*", "Button", true, false) if content != null else []
	var chosen := false
	if not topics.is_empty():
		topics[0].emit_signal("pressed")
		chosen = true
	await process_frame
	content = player.call("menu_content")
	var responses: Array = content.find_children("*", "Label", true, false) if content != null else []
	var response := ""
	for label: Label in responses:
		if label.is_visible_in_tree() and not label.text.is_empty():
			response += label.text
	_check("route.hulda", activated and chosen and not response.is_empty() and player.call("is_menu_open"), {"reference": hulda.get("id", "missing"), "topics": topics.size(), "activated": activated, "chosen": chosen, "response": response})
	await _menu_theme("dialogue")
	player.call("close_menu")
	await _stop("hulda")

	stop_name = "container"
	var opened := false
	var taken := false
	var containers: Array[Dictionary] = []
	var barrel := _reference(func(ref: Dictionary) -> bool: return ref.kind == "CONT" and ref.file == 0 and ref.object == 0x060359)
	if not barrel.is_empty():
		containers.append(barrel)
	for container: Dictionary in containers:
		var pose: Dictionary = player.call("gameplay_reference_pose", container.file, container.object)
		if not pose.is_empty():
			await _walk_to(pose.position + Vector3(0, -160, 0), 5.0)
		if not player.call("open_container_inventory", container.file, container.object):
			continue
		await process_frame
		var menu: Node = player.call("menu_content").get_node_or_null("ItemMenu")
		if menu != null and not menu.get("visible_rows").is_empty():
			opened = true
			await _menu_theme("container")
			var item: Dictionary = menu.get("visible_rows")[0]
			var before: int = player.call("play_path_item_count", item.file, item.object)
			menu.call("_confirm", item)
			await process_frame
			var after: int = player.call("play_path_item_count", item.file, item.object)
			taken = after > before
			_check("route.container", taken, {"container": str(container), "item": item.id, "before": before, "after": after})
		player.call("close_menu")
		if opened:
			break
	if not opened:
		_check("route.container", false, {"opened": false, "taken": false})
	await _stop("container")

	stop_name = "loose"
	var loose := _reference(func(ref: Dictionary) -> bool: return ref.file == 0 and ref.object == 0x059050 and ref.kind == "ALCH")
	var picked := false
	if not loose.is_empty():
		await _walk_to(loose.position + Vector3(0, -150, 0), 5.0)
		_look_at(loose.position)
		var before: int = player.call("play_path_item_count", loose.base_file, loose.base_object)
		picked = _activate(loose)
		await process_frame
		var after: int = player.call("play_path_item_count", loose.base_file, loose.base_object)
		var count: int = streamer.call("rendered_reference_count", loose.file, loose.object)
		var pose: Dictionary = player.call("gameplay_reference_pose", loose.file, loose.object)
		_check("route.loose", picked and after > before and (count == 0 or not pose.get("visible", true)), {"reference": loose.id, "before": before, "after": after, "drawn": count, "visible": pose.get("visible", true)})
	else:
		_check("route.loose", false, {"reference": "missing"})
	player.call("open_inventory_menu")
	await _menu_theme("inventory")
	player.call("close_menu")
	await _stop("loose")

	stop_name = "hearth"
	var hearth := _reference(func(ref: Dictionary) -> bool: return ref.editor_id == "Campfire01Burning" and ref.file == 0 and ref.object == 0x05D691)
	if not hearth.is_empty():
		await _walk_to(hearth.position + Vector3(0, -180, 0), 8.0)
		_look_at(hearth.position + Vector3(0, 0, 50))
	_check("route.hearth", not hearth.is_empty(), {"reference": hearth.get("id", "missing")})
	await _stop("hearth")
	await _hearth_pixels(hearth)
	await _compass()
	await _swing_doors()

	stop_name = "exit"
	var exit_door: Dictionary = {}
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked and door.get("far_worldspace", "") == "WhiterunWorld":
			exit_door = door
			break
	if not exit_door.is_empty():
		var pose: Dictionary = player.call("gameplay_reference_pose", exit_door.file, exit_door.object)
		await _walk_to(pose.position, 10.0)
	var crossed := _activate(exit_door)
	_check("route.exit", crossed and player.call("detection_worldspace") == "WhiterunWorld", {"crossed": crossed, "world": str(player.call("detection_worldspace")), "cell": str(player.call("gameplay_cell"))})
	await _stop("exit")
	if not crossed:
		_finish()
		return
	var city_start: Vector3 = player.call("position")
	var market := _reference(func(ref: Dictionary) -> bool: return MARKET_MODEL in str(ref.model).to_lower() and ref.kind == "STAT")
	var gate: Dictionary = player.call("detection_gate_reference")
	var gate_pose: Dictionary = player.call("gameplay_reference_pose", gate.get("file", -1), gate.get("object", -1))
	if market.is_empty() or gate_pose.is_empty():
		_check("route.market", false, {"market": str(market), "gate": str(gate_pose)})
		_finish()
		return
	var market_position: Vector3 = market.position
	var gate_position: Vector3 = gate_pose.position
	while not finished:
		stop_name = "market"
		var reached := await _walk_to(market_position, 30.0)
		_check("route.market", reached, {"position": str(player.call("position")), "reference": market.id})
		_look_at(gate_position + Vector3(0, 0, 150))
		await _stop("market")
		stop_name = "gate"
		reached = await _walk_to(gate_position, 30.0)
		_check("route.gate", reached, {"position": str(player.call("position")), "reference": str(gate)})
		_look_at(gate_position + Vector3(0, 0, 160))
		await _stop("gate")
		# CONTEXT: The short outside leg supplies actual grass steps, then returns
		# through the same gate. Door transitions retain their authored landings.
		if completed_loops == 0 and not finished:
			await _grass_leg(gate)
		stop_name = "return"
		reached = await _walk_to(market_position, 30.0)
		reached = await _walk_to(city_start, 30.0) and reached
		_check("route.return", reached, {"position": str(player.call("position")), "start": str(city_start)})
		await _stop("return")
		completed_loops += 1
		_check("route.repeat", completed_loops >= 1, {"loops": completed_loops, "elapsed_s": _elapsed()})

func _grass_leg(gate: Dictionary) -> void:
	stop_name = "grass"
	if not _activate(gate):
		_check("audio.grass", false, {"reason": "gate crossing failed"})
		return
	await process_frame
	var origin: Vector3 = player.call("position")
	var returning: Dictionary = {}
	for door: Dictionary in player.call("gameplay_doors"):
		if door.get("far_worldspace", "") == "WhiterunWorld" and not door.locked:
			returning = door
			break
	await _walk_to(origin + Vector3(-350, -500, 0), 10.0)
	await _stop("grass")
	await _walk_to(origin, 10.0)
	_check("route.grass_return", _activate(returning), {"return_door": str(returning)})

func _capture() -> Image:
	await RenderingServer.frame_post_draw
	return root.get_texture().get_image()

func _analyse(operation: Callable) -> Variant:
	var worker := Thread.new()
	var error := worker.start(operation)
	if error != OK:
		_check("analysis_worker", false, {"error": error})
		return operation.call()
	while worker.is_alive():
		await process_frame
	return worker.wait_to_finish()

func _sample_view() -> void:
	if sampling or finished:
		return
	sampling = true
	_key(false)
	var camera := root.get_camera_3d()
	if camera == null:
		_check("camera", false, {"camera": "missing"})
		sampling = false
		return
	await _npcs(camera)
	await _surface_checks(camera)
	_check("frame.max_ms", over_budget == 0, {"worst_ms": worst_ms, "over_50_ms": over_budget, "frames": measured_frames, "warmup_s": Checks.WARMUP_S})
	_check("watchdog.max_ms", stalls_ms <= Checks.STALL_MS, {"worst_ms": stalls_ms, "limit_ms": Checks.STALL_MS})
	sampling = false

func _npcs(camera: Camera3D) -> void:
	var nodes: Array = root.find_children("*", "SkinnedActorNode", true, false)
	var mapped := {}
	for node in nodes:
		var row: Dictionary = node.call("play_path_observation")
		if row.has("reference"):
			mapped[row.reference] = {"node": node, "row": row}
	var in_view := 0
	var drawn := 0
	var heads := 0
	var hairs := 0
	var changing := 0
	var paired := 0
	var now := Time.get_ticks_usec()
	for reference: Dictionary in player.call("play_path_references"):
		if reference.kind != "NPC_":
			continue
		var position := Checks.skyrim_to_godot(reference.position)
		var bounds := AABB(position + Vector3(-0.5, 0, -0.5), Vector3(1, 2.2, 1))
		if not Checks.bounds_visible(camera, bounds) or not player.call("play_path_line_of_sight", reference.position + Vector3(0, 0, 115)):
			continue
		in_view += 1
		if not mapped.has(reference.id):
			continue
		var row: Dictionary = mapped[reference.id].row
		var actor: Node = mapped[reference.id].node
		actor.set_process(false)
		var shape_groups := {"body": [], "head": [], "hair": []}
		for shape: Dictionary in row.shapes:
			var name := str(shape.name).to_lower()
			shape_groups.body.append(shape.rid)
			if "head" in name or "face" in name:
				shape_groups.head.append(shape.rid)
			if "hair" in name:
				shape_groups.hair.append(shape.rid)
		var shown := await _capture()
		for group in shape_groups:
			for rid: RID in shape_groups[group]:
				RenderingServer.instance_set_visible(rid, false)
			var control := await _capture()
			for rid: RID in shape_groups[group]:
				RenderingServer.instance_set_visible(rid, true)
			var pixels: int = await _analyse(Checks.difference.bind(shown, control, Checks.projected(camera, bounds)))
			if pixels >= Checks.MIN_DRAWN_PIXELS and not shape_groups[group].is_empty():
				if group == "body":
					drawn += 1
				elif group == "head":
					heads += 1
				else:
					hairs += 1
		if previous_poses.has(reference.id):
			var previous: Dictionary = previous_poses[reference.id]
			if now - previous.ticks >= 1_000_000:
				paired += 1
				if Checks.pose_changed(previous.poses, row.poses):
					changing += 1
				previous_poses[reference.id] = {"ticks": now, "poses": row.poses.duplicate()}
		else:
			previous_poses[reference.id] = {"ticks": now, "poses": row.poses.duplicate()}
		actor.set_process(true)
	var encounter := in_view > 0 or stop_name != "hulda"
	_check("npc.drawn", encounter and drawn == in_view, {"in_view": in_view, "drawn": drawn})
	_check("npc.head_hair", encounter and heads == in_view and hairs == in_view, {"in_view": in_view, "heads": heads, "hairs": hairs})
	if paired > 0 or drawn != in_view or not encounter:
		_check("npc.pose", paired > 0 and changing == paired, {"pairs": paired, "changed": changing, "interval_s": 1.0})

func _surface_checks(camera: Camera3D) -> void:
	var terrains := 0
	var untextured := 0
	var object_rids: Array[RID] = []
	var objects := 0
	var lods := 0
	var floating := 0
	var measured := 0
	var highest := 0.0
	for surface: Dictionary in streamer.call("play_path_surfaces"):
		if not Checks.bounds_visible(camera, surface.bounds):
			continue
		var mesh: ArrayMesh = surface.mesh
		for index in range(mesh.get_surface_count()):
			var material := mesh.surface_get_material(index)
			if surface.kind == "terrain":
				terrains += 1
				if not Checks.has_albedo(material):
					untextured += 1
		if surface.kind in ["object", "lod"]:
			object_rids.append(surface.rid)
		if surface.kind == "object":
			objects += 1
		elif surface.kind == "lod":
			lods += 1
		if surface.kind not in ["object", "lod"]:
			continue
		var bounds: AABB = surface.bounds
		# CONTEXT: A merged building batch spans a large footprint. All five
		# probes must miss support before its bottom counts as floating.
		var support := -INF
		for point in [bounds.get_center(), bounds.position, bounds.end, Vector3(bounds.position.x, bounds.position.y, bounds.end.z), Vector3(bounds.end.x, bounds.position.y, bounds.position.z)]:
			var terrain_height: Variant = streamer.call("play_path_terrain_height", point)
			if terrain_height != null:
				support = maxf(support, float(terrain_height))
		if is_finite(support):
			measured += 1
			var height := bounds.position.y * 70.0 - support
			highest = maxf(highest, height)
			if height > Checks.MAX_HEIGHT_UNITS:
				floating += 1
	var white := 0
	var drawn_pixels := 0
	if not object_rids.is_empty():
		var shown := await _capture()
		for rid in object_rids:
			RenderingServer.instance_set_visible(rid, false)
		var control := await _capture()
		for rid in object_rids:
			RenderingServer.instance_set_visible(rid, true)
		white = await _analyse(Checks.plain_white.bind(shown, control))
		drawn_pixels = await _analyse(Checks.difference.bind(shown, control, Rect2i(Vector2i.ZERO, shown.get_size())))
	if stop_name in ["market", "gate", "grass", "return", "exit"]:
		_check("terrain.textures", terrains > 0 and untextured == 0, {"surfaces": terrains, "without_texture": untextured})
		_check("object.white", objects > 0 and drawn_pixels >= Checks.MIN_DRAWN_PIXELS and white == 0, {"objects": objects, "lods": lods, "batches": object_rids.size(), "drawn_pixels": drawn_pixels, "white_pixels": white})
		_check("object.height", measured > 0 and floating == 0, {"measured": measured, "floating": floating, "highest_units": highest, "limit_units": Checks.MAX_HEIGHT_UNITS})
	var doors := 0
	var missing := 0
	for reference: Dictionary in player.call("play_path_references"):
		if reference.kind == "DOOR" and not reference.model.is_empty():
			doors += 1
			if streamer.call("rendered_reference_count", reference.file, reference.object) <= 0:
				missing += 1
	_check("doors.drawn", doors > 0 and missing == 0, {"doors": doors, "missing": missing})

func _hearth_pixels(reference: Dictionary) -> void:
	while sampling and not finished:
		await process_frame
	sampling = true
	stop_name = "hearth"
	var flames: Array[RID] = []
	var additive_contract := true
	var bounds := AABB()
	var first := true
	for surface: Dictionary in streamer.call("play_path_surfaces"):
		if not reference.is_empty() and reference.id in surface.get("references", PackedStringArray()):
			var mesh: ArrayMesh = surface.mesh
			for index in range(mesh.get_surface_count()):
				var material := mesh.surface_get_material(index) as ShaderMaterial
				if material != null and "source_texture" in material.shader.code:
					additive_contract = additive_contract and "blend_add" in material.shader.code and "depth_draw_never" in material.shader.code
					flames.append(surface.rid)
					bounds = surface.bounds if first else bounds.merge(surface.bounds)
					first = false
	var shown := await _capture()
	for rid in flames:
		RenderingServer.instance_set_visible(rid, false)
	var control := await _capture()
	for rid in flames:
		RenderingServer.instance_set_visible(rid, true)
	var result: Dictionary = await _analyse(Checks.additive.bind(shown, control, Checks.projected(root.get_camera_3d(), bounds)))
	result.effect_surfaces = flames.size()
	result.additive_contract = additive_contract
	_check("fire.additive", additive_contract and not flames.is_empty() and result.bright_pixels >= Checks.MIN_DRAWN_PIXELS, result)
	_check("fire.hull", additive_contract and not flames.is_empty() and result.bright_pixels >= Checks.MIN_DRAWN_PIXELS and result.transmitted_pixels > 0 and result.dark_pixels == 0, result)
	if not output_directory.is_empty():
		shown.save_png(output_directory.path_join("hearth-fire.png"))
		control.save_png(output_directory.path_join("hearth-background.png"))
	sampling = false

func _compass() -> void:
	while sampling and not finished:
		await process_frame
	sampling = true
	stop_name = "compass"
	var compass := player.get_node_or_null("HudLayer/Hud/Compass")
	var positions: Array[float] = []
	var missing := 0
	for degree in range(361):
		player.call("set_facing", deg_to_rad(float(degree)), 0.0)
		await process_frame
		var offset: Variant = Checks.compass_offset(compass)
		if offset == null:
			missing += 1
		else:
			positions.append(float(offset))
	var changed := 0
	var error := INF
	if positions.size() == 361:
		var increments: Array[float] = []
		for index in range(360):
			var value := positions[index + 1] - positions[index]
			if absf(value) > 0.001:
				changed += 1
			increments.append(value)
		increments.sort()
		var step := increments[180]
		error = 0.0
		for value in increments:
			# CONTEXT: The one wrap of a repeated strip changes its offset by
			# one full period. All other one-degree increments must be linear.
			if absf(value) <= absf(step) * 2.0:
				error = maxf(error, absf(value - step))
	_check("compass.offset", missing == 0 and changed == 360 and error <= Checks.COMPASS_ERROR_PX, {"degree_samples": 361, "changed_steps": changed, "missing_offset": missing, "maximum_error_px": error if is_finite(error) else -1.0})

	sampling = false

func _swing_doors() -> void:
	stop_name = "doors"
	var doors: Array = player.call("gameplay_swing_doors")
	var states := 0
	var missing := 0
	if not doors.is_empty():
		var door: Dictionary = doors[0]
		var pose: Dictionary = player.call("gameplay_reference_pose", door.file, door.object)
		if not pose.is_empty():
			await _walk_to(pose.position + Vector3(0, -200, 0), 5.0)
			_look_at(pose.position + Vector3(0, 0, 100))
		while sampling and not finished:
			await process_frame
		sampling = true
		for _cycle in range(2):
			_activate(door)
			for _sample in range(5):
				await create_timer(0.2).timeout
				var state: Dictionary = streamer.call("door_state", door.file, door.object)
				var door_node: Node = instance_from_id(int(state.get("node", 0))) if state.has("node") else null
				if door_node != null:
					door_node.set_process(false)
				var shown := await _capture()
				var rids: Array = state.get("instances", [])
				for rid: RID in rids:
					RenderingServer.instance_set_visible(rid, false)
				var control := await _capture()
				for rid: RID in rids:
					RenderingServer.instance_set_visible(rid, true)
				var pixels: int = await _analyse(Checks.difference.bind(shown, control, Rect2i(Vector2i.ZERO, shown.get_size())))
				if door_node != null:
					door_node.set_process(true)
				states += 1
				if pixels < Checks.MIN_DRAWN_PIXELS or not state.get("visible", false):
					missing += 1
				_check("doors.states", pixels >= Checks.MIN_DRAWN_PIXELS and state.get("visible", false), {"fraction": state.get("fraction", -1.0), "opened": state.get("opened", false), "pixels": pixels})
	_check("doors.states", states == 10 and missing == 0, {"states": states, "missing": missing})

	sampling = false

func _menu_theme(kind: String) -> void:
	await process_frame
	var selected := ""
	if not OS.get_environment("SOE_TEST_NORDIC").is_empty():
		selected = "nordic"
	elif not OS.get_environment("SOE_TEST_SKYUI").is_empty():
		selected = "skyui"
	if selected.is_empty():
		_check("menu." + kind + "_theme", true, {"applicable": false, "theme": "installed default"})
		return
	if not output_directory.is_empty():
		var image := await _capture()
		image.save_png(output_directory.path_join("menu-" + kind + ".png"))
	var content: Control = player.call("menu_content")
	var sources: Array[String] = []
	if content != null:
		for node in content.find_children("*", "Control", true, false):
			for property: Dictionary in node.get_property_list():
				if property.name == "art_source":
					sources.append(str(node.get("art_source")))
	var converted := sources.size() > 0 and sources.all(func(source: String) -> bool: return not source.begins_with("res://ui/theme/") and source != "builtin" and not source.is_empty())
	var error := OS.get_environment("SOE_PLAY_PATH_THEME_ERROR")
	_check("menu." + kind + "_theme", content != null and content.is_visible_in_tree() and converted and error.is_empty(), {"applicable": true, "theme": selected, "sources": sources, "conversion_error": error})

func _install_audio_probe() -> void:
	audio_bus = AudioServer.bus_count
	AudioServer.add_bus()
	AudioServer.set_bus_name(audio_bus, "TST19Footsteps")
	footstep_capture = AudioEffectCapture.new()
	footstep_capture.buffer_length = 2.0
	AudioServer.add_bus_effect(audio_bus, footstep_capture)
	var bank := player.get_node_or_null("SoundBank")
	if bank != null:
		for child in bank.get_children():
			if child is AudioStreamPlayer3D:
				audio_voices.append(child)
	player.connect("footstep_requested", _step_requested)
	player.connect("footstep_played", _step_played)

func _step_requested(material: int, right: bool) -> void:
	step_requests.append({"material": material, "right": right, "ticks": Time.get_ticks_usec(), "played": false, "peak": 0.0, "surface": ""})

func _step_played(material: int, slot: int, right: bool) -> void:
	if slot >= 0 and slot < audio_voices.size():
		audio_voices[slot].bus = "TST19Footsteps"
	for index in range(step_requests.size() - 1, -1, -1):
		if step_requests[index].material == material and step_requests[index].right == right:
			step_requests[index].played = true
			break

func _drain_audio() -> void:
	var samples := footstep_capture.get_buffer(footstep_capture.get_frames_available())
	var peak := 0.0
	for sample in samples:
		peak = maxf(peak, maxf(absf(sample.x), absf(sample.y)))
	for step: Dictionary in step_requests:
		step.peak = maxf(float(step.peak), peak)
		if step.surface.is_empty():
			if not step_materials.has(step.material):
				var ground: Dictionary = player.call("play_path_ground", player.call("position"))
				step_materials[step.material] = str(ground.get("material", "unresolved")).to_lower()
			step.surface = step_materials[step.material]
	while not step_requests.is_empty() and Time.get_ticks_usec() - int(step_requests[0].ticks) > 300_000:
		var step: Dictionary = step_requests.pop_front()
		var db := -120.0 if step.peak <= 0.0 else 20.0 * log(float(step.peak)) / log(10.0)
		minimum_step_db = minf(minimum_step_db, db)
		var kind := "grass" if "grass" in step.surface else ("stone" if "stone" in step.surface else "other")
		if kind == "stone":
			stone_steps += 1
		elif kind == "grass":
			grass_steps += 1
		if kind != "other":
			_check("audio." + kind, step.played and db > -60.0, {"material": step.surface, "hash": step.material, "played": step.played, "peak_db": db, "right": step.right})

func _finish() -> void:
	if finished:
		return
	finished = true
	_key(false)
	_check("duration", _elapsed() >= duration_s and completed_loops >= 1, {"elapsed_s": _elapsed(), "required_s": duration_s, "loops": completed_loops})
	_check("frame.max_ms", measured_frames > 0 and over_budget == 0, {"frames": measured_frames, "worst_ms": worst_ms, "over_50_ms": over_budget})
	_check("watchdog.max_ms", stalls_ms <= Checks.STALL_MS, {"worst_ms": stalls_ms})
	_check("audio.stone", stone_steps > 0, {"steps": stone_steps, "minimum_peak_db": minimum_step_db})
	_check("audio.grass", grass_steps > 0, {"steps": grass_steps, "minimum_peak_db": minimum_step_db})
	_heartbeat()
	print("PLAY_PATH_RESULT ", JSON.stringify({"mode": mode, "elapsed_s": _elapsed(), "loops": completed_loops, "required_failures": required_failures, "expected_failures": expected_failures, "checks": observations}))
	quit(0 if required_failures == 0 else 1)

func _self_test() -> void:
	var failures := Checks.self_test()
	for name in failures:
		printerr("TST-19 predicate failed: ", name)
	print("tst_19_pixel_and_pose_predicates: ", "PASS" if failures.is_empty() else "FAIL")
	quit(0 if failures.is_empty() else 1)
