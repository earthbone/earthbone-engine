extends SceneTree

## MAG-16 checks that live resident actor body RIDs resolve to their owners.
var failures := 0


func _initialize() -> void:
	call_deferred("mag_16_live_godot_actor_body_acquisition")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func mag_16_live_godot_actor_body_acquisition() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: MAG-16 requires installed data and the host extension, data_empty=",
			data.is_empty(), " PlayerNode=", ClassDB.class_exists("PlayerNode"))
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	root.add_child(streamer)
	_check(streamer.call("load_data", data), "installed data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	root.add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data source failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"Bannered Mare collision load failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	await physics_frame
	_check(player.call("sync_contact_actors") > 1, "resident actor bodies were not acquired")
	var rows: Array = player.call("contact_actor_bodies")
	var owners: Dictionary = {}
	var excluded: Array[RID] = []
	for row: Dictionary in rows:
		owners[row.body] = Vector2i(row.file, row.object)
		if row.file == 0 and row.object == 0x14:
			excluded.append(row.body)
	var state: PhysicsDirectSpaceState3D = player.get_world_3d().direct_space_state
	var acquired := false
	var acquired_owner := Vector2i(-1, -1)
	for row: Dictionary in rows:
		if row.file == 0 and row.object == 0x14:
			continue
		for height in [0.6, 0.9, 1.2]:
			var center: Vector3 = row.position + Vector3.UP * height
			for axis in [Vector3.RIGHT, Vector3.FORWARD]:
				var query := PhysicsRayQueryParameters3D.create(center - axis, center + axis)
				query.exclude = excluded
				var result := state.intersect_ray(query)
				var body: RID = result.get("rid", RID())
				if owners.has(body):
					acquired = true
					acquired_owner = owners[body]
					break
			if acquired:
				break
		if acquired:
			break
	_check(acquired, "live physics ray did not acquire a registered actor body")
	if acquired:
		_check(acquired_owner.x >= 0 and acquired_owner.y >= 0,
			"acquired actor RID has no owning FormID")
		print("MAG-16 live actor body resolved to %04x:%06x" % [
			acquired_owner.x, acquired_owner.y])
	player.free()
	streamer.free()
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " MAG-16 actor body acquisition")
	quit(0 if failures == 0 else 1)
