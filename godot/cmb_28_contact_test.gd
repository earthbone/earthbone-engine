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
const BODY := "meshes/actors/character/character assets/malebody_1.nif"

var live_player: Node3D
var live_visual: Node3D
var authored_markers: Array[String] = []


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
	_check(not player.call("last_melee_obstruction").is_valid(),
		"%s: actor contact also reported an obstruction" % label)
	print("CMB28 %s: swing struck %x, hit resolved %s" % [label, target.object, resolved])
	# A server body halfway down the same segment must stop the actor contact.
	var server := PhysicsServer3D
	var block_shape: RID = server.sphere_shape_create()
	server.shape_set_data(block_shape, 0.18)
	var block_body: RID = server.body_create()
	server.body_set_mode(block_body, PhysicsServer3D.BODY_MODE_STATIC)
	server.body_set_space(block_body, player.get_world_3d().space)
	server.body_add_shape(block_body, block_shape)
	server.body_set_state(block_body, PhysicsServer3D.BODY_STATE_TRANSFORM,
		Transform3D(Basis.IDENTITY, origin.lerp(centre, 0.5)))
	_check(player.call("begin_melee_swing", false), "%s: second swing did not arm" % label)
	player.call("melee_bone_contact", "HitFrame", pose)
	var blocked: Dictionary = player.call("last_melee_target")
	_check(blocked.is_empty(), "%s: blocked swing reported actor %s" % [label, blocked])
	_check(player.call("last_melee_obstruction") == block_body,
		"%s: blocked swing did not report its first RID" % label)
	server.free_rid(block_body)
	server.free_rid(block_shape)
	print("CMB28 %s: obstruction stopped actor contact" % label)


func _on_authored_marker(name: String) -> void:
	authored_markers.append(name)
	if name != "HitFrame":
		return
	var row: Dictionary = live_visual.call("contact_weapon_pose", false)
	_check(row.has("pose"), "animated rig has no weapon pose at HitFrame")
	if not row.has("pose"):
		return
	var pose: Transform3D = row.pose
	var direction: Vector3 = -pose.basis.z.normalized()
	var server := PhysicsServer3D
	var shape: RID = server.sphere_shape_create()
	server.shape_set_data(shape, 0.12)
	var body: RID = server.body_create()
	server.body_set_mode(body, PhysicsServer3D.BODY_MODE_STATIC)
	server.body_set_space(body, live_player.get_world_3d().space)
	server.body_add_shape(body, shape)
	server.body_set_state(body, PhysicsServer3D.BODY_STATE_TRANSFORM,
		Transform3D(Basis.IDENTITY, pose.origin + direction * 0.3))
	live_player.call("melee_skinned_contact", name, live_visual)
	_check(live_player.call("last_melee_obstruction") == body,
		"authored HitFrame did not contact the nearest obstruction")
	_check(live_player.call("last_melee_target").is_empty(),
		"authored obstruction also reported an actor")
	server.free_rid(body)
	server.free_rid(shape)


func _authored_swing(player: Node3D, data: String) -> void:
	authored_markers.clear()
	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	_check(visual != null, "SkinnedActorNode is unavailable")
	if visual == null:
		return
	get_root().add_child(visual)
	_check(visual.call("load_melee_character", data, BODY), "melee character rig did not load")
	_check(player.call("bind_actor_appearance", visual, 0, PLAYER),
		"player rig did not bind to the shared World")
	live_player = player
	live_visual = visual
	visual.connect("animation_event", Callable(self, "_on_authored_marker"))
	visual.set("playing", false)
	_check(visual.call("set_graph_variable", "iRightHandType", 1.0),
		"one-handed graph has no right-hand weapon type")
	_check(player.call("begin_melee_swing", false), "authored swing did not arm")
	var sent: bool = visual.call("send_graph_event", "attackStart")
	_check(sent, "attackStart is missing from graph: %s" % [visual.call("graph_event_names")])
	for i in 180:
		visual.call("advance", 1.0 / 60.0)
		if authored_markers.has("HitFrame"):
			break
	print("CMB28 authored elapsed %.3f markers %s clips %s" % [
		visual.call("elapsed"), authored_markers, visual.call("active_clips")])
	_check(authored_markers.has("HitFrame"), "authored attack emitted no HitFrame")
	visual.queue_free()
	await _frames(2)


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
	await _authored_swing(player, data)
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
		await _authored_swing(player, data)

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
