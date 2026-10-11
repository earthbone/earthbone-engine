extends SceneTree

## CMB-27: a shared melee hit reaches the bound actor's live graph and expires.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
const PLAYER := 0x14
const IRON_WARHAMMER := 0x00013982
var failures := 0


func _initialize() -> void:
	call_deferred("_run")


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)


func _frames(count: int) -> void:
	for i in count:
		await physics_frame


func _run() -> void:
	print("CMB_27 stage=startup")
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		printerr("FAIL: CMB-27 requires SKYRIM_DATA")
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
		"initial cell failed")
	_check(player.call("start_gameplay"), "gameplay startup failed")
	print("CMB_27 stage=gameplay_started")
	_check(player.call("equip_melee_weapon", 0, IRON_WARHAMMER, false), "iron warhammer not equipped")
	await _frames(2)
	player.call("sync_contact_actors")

	var target: Dictionary = {}
	for row: Dictionary in player.call("contact_actor_bodies"):
		if row.file == 0 and row.object != PLAYER:
			target = row
			break
	_check(not target.is_empty(), "no resident NPC to hit")
	if target.is_empty():
		quit(1)
		return

	var visual: Node3D = ClassDB.instantiate("SkinnedActorNode")
	get_root().add_child(visual)
	_check(visual.call("load_character", data, BODY), "human graph failed to load")
	_check(player.call("bind_actor_appearance", visual, target.file, target.object),
		"target rig did not bind to the shared World")
	print("CMB_27 stage=actor_bound")
	for i in 30:
		visual.call("advance", 1.0 / 60.0)
	_check(visual.call("set_graph_variable", "iRightHandType", 1.0),
		"vanilla master graph has no right-hand type variable")
	visual.call("start_walking")
	var locomotion: PackedStringArray = visual.call("active_states")
	_check(not locomotion.is_empty(), "target locomotion state is empty")

	var centre: Vector3 = target.position + Vector3.UP * 0.9
	var origin := centre + Vector3(0.6, 0.0, 0.0)
	var pose := Transform3D(Basis.looking_at(centre - origin, Vector3.UP), origin)
	var health_before: float = player.call("gameplay_actor_value", target.file, target.object, "health")
	_check(player.call("begin_melee_swing", false), "melee swing did not arm")
	var hit_resolved: bool = player.call("melee_bone_contact", "HitFrame", pose)
	_check(hit_resolved, "shared melee hit was rejected")
	var hit: Dictionary = player.call("last_melee_target")
	_check(hit.get("file", -1) == target.file and hit.get("object", -1) == target.object,
		"hit reached %s, expected %d:%x" % [hit, target.file, target.object])
	var health_after_hit: float = player.call("gameplay_actor_value", target.file, target.object, "health")
	_check(health_after_hit < health_before, "shared hit did not damage the bound actor")
	_check(player.call("gameplay_actor_is_staggered", target.file, target.object),
		"shared hit did not set the World stagger deadline")
	_check(not player.call("melee_bone_contact", "HitFrame", pose), "duplicate hit frame resolved twice")
	_check(is_equal_approx(player.call("gameplay_actor_value", target.file, target.object, "health"), health_after_hit),
		"duplicate hit frame damaged the actor twice")

	var reaction := false
	for i in 90:
		player.call("advance", 1.0 / 60.0)
		visual.call("advance", 1.0 / 60.0)
		for state: String in visual.call("active_states"):
			reaction = reaction or state.contains("Stagger_State")
		if reaction:
			break
	_check(reaction, "bound SkinnedActorNode did not enter the authored stagger state")
	_check(is_equal_approx(player.call("gameplay_actor_value", target.file, target.object, "health"), health_after_hit),
		"animation handoff changed health after the hit")

	var returned := false
	for i in 180:
		await physics_frame
		player.call("advance", 1.0 / 60.0)
		visual.call("advance", 1.0 / 60.0)
		if visual.call("active_states") == locomotion:
			returned = true
			break
	_check(returned, "bound graph did not return to locomotion after the World deadline")
	_check(is_equal_approx(player.call("gameplay_actor_value", target.file, target.object, "health"), health_after_hit),
		"stagger expiry changed health after the hit")
	print("CMB_27 stage=reaction_returned")
	visual.queue_free()
	player.queue_free()
	streamer.queue_free()
	if failures == 0:
		print("RESULT: PASS")
	else:
		printerr("RESULT: FAIL: %d checks" % failures)
	quit(0 if failures == 0 else 1)
