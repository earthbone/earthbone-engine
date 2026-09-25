extends SceneTree

## CMB-28: actor bodies and pose-derived weapon segments follow the shared
## World across a cell change, so a live armed swing reaches the nearest
## actor or obstruction without a scene node per reference.
var failures := 0

## Skyrim.esm IronSword, reach 1.0, one-handed.
const IRON_SWORD := 0x00012EB7
## The player reference, which World also lists as a contact actor so NPC
## swings can strike it.
const PLAYER := 0x14


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _frames(count: int) -> void:
	for i in count:
		await physics_frame


## Every actor body must sit at its owner's World position, converted by the
## host axis map, and none may belong to a reference outside the World list.
func _bodies_follow(player: Node3D, label: String) -> int:
	var world_count: int = player.call("sync_contact_actors")
	var rows: Array = player.call("contact_actor_bodies")
	_check(world_count == rows.size(), "%s: %d bodies for %d actors" % [label, rows.size(), world_count])
	var server := PhysicsServer3D
	for row: Dictionary in rows:
		var body: RID = row.body
		_check(body.is_valid(), "%s: invalid body RID" % label)
		var placed: Transform3D = server.body_get_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM)
		var expected: Vector3 = row.position
		_check(placed.origin.distance_to(expected) < 0.01,
			"%s: body for %x at %s, World says %s" % [label, row.object, placed.origin, expected])
	return rows.size()


## Swing from a pose placed beside the nearest actor and pointed at it. The
## pose goes through the same weapon-bone path a HitFrame uses.
func _swing_at_nearest(player: Node3D, label: String) -> void:
	# The player's own body is excluded from its swing, so aim at an NPC.
	var target: Dictionary = {}
	for row: Dictionary in player.call("contact_actor_bodies"):
		if not (row.file == 0 and row.object == PLAYER):
			target = row
			break
	if target.is_empty():
		print("CMB28 %s: no resident NPC to strike" % label)
		return
	var centre: Vector3 = target.position + Vector3.UP * 0.9
	var origin := centre + Vector3(0.6, 0.0, 0.0)
	# The blade runs along the pose's negated basis Z.
	var pose := Transform3D(Basis.looking_at(centre - origin, Vector3.UP), origin)
	_check(player.call("begin_melee_swing", false), "%s: swing did not arm" % label)
	# The return value is the World's hit resolution, which later rows own.
	# This row owns the contact, which last_melee_target reports.
	var resolved: bool = player.call("melee_bone_contact", "HitFrame", pose)
	var struck: Dictionary = player.call("last_melee_target")
	_check(struck.get("object", -1) == target.object,
		"%s: contact went to %s, expected %x" % [label, struck, target.object])
	print("CMB28 %s: swing struck %x, hit resolved %s" % [label, target.object, resolved])
	# The same pose pointed away from every actor and down into the floor
	# meets static collision first and must report no actor.
	var aside := centre + Vector3(1.2, 0.0, 0.0)
	var down := Transform3D(Basis.looking_at(Vector3.DOWN, Vector3.FORWARD), aside)
	_check(player.call("begin_melee_swing", false), "%s: second swing did not arm" % label)
	player.call("melee_bone_contact", "HitFrame", down)
	var blocked: Dictionary = player.call("last_melee_target")
	_check(blocked.is_empty(), "%s: floor swing reported actor %s" % [label, blocked])
	print("CMB28 %s: floor swing struck no actor" % label)


func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		printerr("FAIL: CMB28 requires installed data and the host extension")
		quit(1)
		return
	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)
	_check(player.call("use_streamer_data", streamer), "shared data failed")
	_check(player.call("enter_cell", "WhiterunBanneredMare", 0.0, 0.0, 500.0) > 0,
		"initial collision failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	_check(player.call("equip_melee_weapon", 0, IRON_SWORD, false), "iron sword not equipped")
	await _frames(2)
	var start: String = player.call("gameplay_cell")
	var first := await _bodies_follow(player, start)
	print("CMB28 %s: %d actor bodies" % [start, first])
	_check(first > 0, "%s has no resident actor bodies" % start)
	_swing_at_nearest(player, start)
	var before: Array = player.call("contact_actor_bodies")

	var chosen: Dictionary = {}
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			chosen = door
			break
	_check(not chosen.is_empty(), "no unlocked load door")
	if not chosen.is_empty():
		_check(player.call("activate_reference", chosen.file, chosen.object), "outbound door failed")
		await _frames(2)
		var next: String = player.call("gameplay_cell")
		_check(next != start, "outbound door kept the old cell")
		var after := await _bodies_follow(player, next)
		print("CMB28 %s: %d actor bodies" % [next, after])
		var now: Array = player.call("contact_actor_bodies")
		var kept := 0
		for old: Dictionary in before:
			for row: Dictionary in now:
				if row.object == old.object and row.file == old.file:
					kept += 1
					_check(row.body == old.body, "%x changed body on a cell change" % old.object)
		print("CMB28 %d bodies carried across the door, %d released" % [kept, before.size() - kept])
		for old: Dictionary in before:
			var still := false
			for row: Dictionary in now:
				still = still or row.body == old.body
			if not still:
				_check(not PhysicsServer3D.body_get_space(old.body).is_valid(),
					"released body for %x still in a space" % old.object)
		_swing_at_nearest(player, next)

	player.queue_free()
	await _frames(2)
	for old: Dictionary in before:
		_check(not PhysicsServer3D.body_get_space(old.body).is_valid(),
			"teardown left body for %x in a space" % old.object)
	if failures == 0:
		print("RESULT: PASS")
	else:
		print("RESULT: FAIL: %d checks" % failures)
	quit(0 if failures == 0 else 1)
