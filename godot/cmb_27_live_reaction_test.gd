extends SceneTree

## CMB-27: a shared melee hit reaches the bound actor's live graph and expires.

const BODY := "meshes/actors/character/character assets/malebody_1.nif"
const PLAYER := 0x14
const IRON_SWORD := 0x00012EB7
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
	_check(player.call("equip_melee_weapon", 0, IRON_SWORD, false), "iron sword not equipped")
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
	visual.call("start_walking")
	for i in 30:
		visual.call("advance", 1.0 / 60.0)
	var locomotion: PackedStringArray = visual.call("active_states")
	_check(not locomotion.is_empty(), "target locomotion state is empty")

	var centre: Vector3 = target.position + Vector3.UP * 0.9
	var origin := centre + Vector3(0.6, 0.0, 0.0)
	var pose := Transform3D(Basis.looking_at(centre - origin, Vector3.UP), origin)
	_check(player.call("begin_melee_swing", false), "melee swing did not arm")
	_check(player.call("melee_bone_contact", "HitFrame", pose), "shared melee hit did not resolve")
	var hit: Dictionary = player.call("last_melee_target")
	_check(hit.get("object", -1) == target.object, "hit reached %s, expected %x" % [hit, target.object])

	var reaction := false
	for i in 90:
		visual.call("advance", 1.0 / 60.0)
		for clip: String in visual.call("active_clips"):
			reaction = reaction or clip.to_lower().contains("stagger")
		if reaction:
			break
	_check(reaction, "bound SkinnedActorNode did not play the authored stagger clip")

	var returned := false
	for i in 180:
		await physics_frame
		visual.call("advance", 1.0 / 60.0)
		if visual.call("active_states") == locomotion:
			returned = true
			break
	_check(returned, "bound graph did not return to locomotion after the World deadline")
	visual.queue_free()
	player.queue_free()
	streamer.queue_free()
	if failures == 0:
		print("RESULT: PASS")
	else:
		printerr("RESULT: FAIL: %d checks" % failures)
	quit(0 if failures == 0 else 1)
