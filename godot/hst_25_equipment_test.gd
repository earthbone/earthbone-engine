extends "res://hst_25_armor_test.gd"

## Weapon readiness and contact through the free walk inventory and input routes.
var graph_events := [[], []]
var contact_target: Dictionary = {}
var contact_body: StaticBody3D
var contact_health := 0.0
var contact_seen := false
var contact_after := 0.0

func _check_weapons() -> void:
	await _toggle(0x12eb7)
	await _toggle(0x12eb6)
	print("HST-25 equipment shield=", _models(player_body))
	_check(player_body.call("stats").get("hand_shapes", 0) >= 2, "sword and shield are not attached as hand models")
	_check(player.call("toggle_weapon_ready"), "R draw failed")
	await _steps(90)
	print("HST-25 equipment drawn=", player_body.call("active_clips"), " first=", player_arms.call("active_clips"))
	if DisplayServer.get_name() != "headless":
		await _capture("drawn")
	_check(player.call("attack_with_hand", false, false), "sword attack rejected")
	await _steps(60)
	_check(not player.call("toggle_weapon_ready"), "R sheathe failed")
	await _steps(90)
	if DisplayServer.get_name() != "headless":
		await _capture("sheathed")
	await _proof_both_views()
	await _check_spell_replacement()

func _animation(name: String, view: int) -> void:
	graph_events[view].append(name)

func _ready(drawn: bool) -> void:
	graph_events = [[], []]
	Input.action_press("Ready Weapon")
	player.call("advance", 1.0 / 60.0)
	Input.action_release("Ready Weapon")
	player.call("advance", 0.0)
	var played := [false, false]
	var event := "weaponDraw" if drawn else "weaponSheathe"
	for _step in 120:
		for index in 2:
			var visual: Node3D = player_body if index == 0 else player_arms
			visual.call("advance", 1.0 / 60.0)
			for clip in visual.call("active_clips"):
				if "equip" in str(clip).to_lower():
					played[index] = true
			var observation: Dictionary = visual.call("play_path_observation")
			_check(observation.get("hands_drawn", not drawn) == (drawn if event in graph_events[index] else not drawn), "attachment changed before " + event)
		await _animation_frame(_step)
	for index in 2:
		_check(played[index], "view %d did not play equip/unequip clip" % index)
		_check(event in graph_events[index], "view %d did not emit %s" % [index, event])
	print("HST-25 equipment ready drawn=", drawn, " events=", graph_events)

func _attachments(visual: Node3D, first: bool, drawn: bool) -> void:
	var observation: Dictionary = visual.call("play_path_observation")
	var hands: Array = observation.get("hands", [])
	_check(hands.size() == 2, "sword and shield attachment observations missing")
	for hand: Dictionary in hands:
		var expected := "ShieldBack" if hand.left else "WeaponSword"
		if drawn:
			expected = "shield" if hand.left else "weapon"
		_check(str(hand.bone).to_lower() == expected.to_lower() if drawn else hand.bone == expected or first, "wrong attachment " + str(hand))
		_check(hand.shapes > 0, "attachment has no geometry " + str(hand))
		_check(hand.visible == (drawn or not first), "wrong attachment visibility " + str(hand))
		if hand.visible:
			_check(hand.has("pose") and hand.pose.is_finite(), "attachment pose is missing or invalid")
	print("HST-25 equipment attachments first=", first, " drawn=", drawn, " ", hands)

func _proof_both_views() -> void:
	_setup_contact_fixture()
	player_body.connect("animation_event", _animation.bind(0))
	player_arms.connect("animation_event", _animation.bind(1))
	for first in [false, true]:
		if first:
			Input.action_press("Toggle POV")
			player.call("advance", 1.0 / 60.0)
			Input.action_release("Toggle POV")
			player.call("advance", 0.0)
		_check(player.call("is_third_person") != first, "camera did not select requested view")
		var visual: Node3D = player_arms if first else player_body
		_attachments(visual, first, false)
		await _ready(true)
		_attachments(visual, first, true)
		if DisplayServer.get_name() != "headless":
			await _capture("first_drawn" if first else "third_drawn")
		graph_events = [[], []]
		contact_seen = false
		contact_health = _health()
		var before: Array = visual.call("uploaded_skin_poses")
		Input.action_press("Right Attack/Block")
		player.call("advance", 0.1)
		Input.action_release("Right Attack/Block")
		player.call("advance", 1.0 / 60.0)
		var attacks := [false, false]
		var moved := false
		for _step in 120:
			for index in 2:
				var rig: Node3D = player_body if index == 0 else player_arms
				rig.call("advance", 1.0 / 60.0)
				for clip in rig.call("active_clips"):
					attacks[index] = attacks[index] or "attack" in str(clip).to_lower()
			moved = moved or before != visual.call("uploaded_skin_poses")
			if contact_seen and contact_after == contact_health:
				contact_after = _health()
			await _animation_frame(_step)
		_check(attacks[0] and attacks[1], "mouse attack did not select both authored attack clips")
		_check(moved, "mouse attack did not move the selected rig")
		_check(contact_seen and contact_after < contact_health, "authored hit frame did not apply shared combat damage")
		_check(is_equal_approx(_health(), contact_after), "attack applied damage more than once")
		print("HST-25 contact first=", first, " health=", contact_health, " -> ", _health())
		_check("HitFrame" in graph_events[0] and "HitFrame" in graph_events[1], "mouse attack did not emit both hit markers")
		print("HST-25 equipment mouse attack clips=", attacks, " events=", graph_events)
		await _ready(false)
		_attachments(visual, first, false)
		if DisplayServer.get_name() != "headless":
			await _capture("first_sheathed" if first else "third_sheathed")

func _health() -> float:
	if contact_target.is_empty(): return 0.0
	return float(player.call("gameplay_actor_value", contact_target.file, contact_target.object, "health"))

func _setup_contact_fixture() -> void:
	for row: Dictionary in player.call("play_path_references"):
		if row.kind == "NPC_" and row.file == 0 and row.base_file == 0 and float(player.call("gameplay_actor_value", row.file, row.object, "health")) > 0:
			contact_target = row
			break
	_check(not contact_target.is_empty(), "no resident actor for the contact fixture")
	if contact_target.is_empty(): return
	for command in ["setessential %x 0" % contact_target.base_object, "setprotected %x 0" % contact_target.base_object, "prid %x" % contact_target.object, "setav health 1000"]:
		_check(not str(player.call("console_execute", command)).contains("error:"), "contact fixture command failed: " + command)
	contact_body = StaticBody3D.new()
	var collision := CollisionShape3D.new()
	var sphere := SphereShape3D.new()
	sphere.radius = 0.4
	collision.shape = sphere
	contact_body.add_child(collision)
	get_root().add_child(contact_body)
	contact_body.position = Vector3(10000, 10000, 10000)
	_check(player.call("register_melee_body", contact_body.get_rid(), contact_target.file, contact_target.object), "contact collider did not bind to the resident actor")
	for visual in [player_body, player_arms]:
		for connection: Dictionary in visual.get_signal_connection_list("weapon_contact"):
			var callback: Callable = connection.callable
			if callback.get_object() == player:
				visual.disconnect("weapon_contact", callback)
				visual.connect("weapon_contact", _contact_fixture.bind(visual == player_arms))
				visual.connect("weapon_contact", callback)

func _contact_fixture(event: String, left: bool, pose: Transform3D, first: bool) -> void:
	if event != "HitFrame" or left or first == bool(player.call("is_third_person")): return
	if contact_body == null: return
	if not contact_seen:
		_check(is_equal_approx(_health(), contact_health), "damage preceded the authored hit marker")
		contact_seen = true
		contact_after = contact_health
	PhysicsServer3D.body_set_state(contact_body.get_rid(), PhysicsServer3D.BODY_STATE_TRANSFORM, Transform3D(Basis.IDENTITY, pose.origin))

func _check_spell_replacement() -> void:
	await _toggle(0x12eb7)
	_check(not str(player.call("console_execute", "player.addspell 12fcd")).contains("error:"), "Flames fixture could not be learned")
	_check(player.call("equip_player_spell", 0, 0x12fcd, false), "Flames could not occupy the right hand")
	await _toggle(0x12eb7)
	var sword := false
	for hand: Dictionary in player_arms.call("play_path_observation").get("hands", []):
		if not hand.left:
			sword = "sword" in str(hand.model).to_lower()
	_check(sword, "inventory sword did not replace the right hand spell model")
	player.call("unequip_player_hand", false)
	player.call("unequip_player_hand", false)
	await _toggle(0x12eb7)
