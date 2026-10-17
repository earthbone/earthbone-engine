extends "res://hst_25_armor_test.gd"

## Weapon readiness and contact through the free walk inventory and input routes.
var graph_events := [[], []]

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
		await process_frame
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
			await process_frame
		_check(attacks[0] and attacks[1], "mouse attack did not select both authored attack clips")
		_check(moved, "mouse attack did not move the selected rig")
		_check("HitFrame" in graph_events[0] and "HitFrame" in graph_events[1], "mouse attack did not emit both hit markers")
		print("HST-25 equipment mouse attack clips=", attacks, " events=", graph_events)
		await _ready(false)
		_attachments(visual, first, false)
		if DisplayServer.get_name() != "headless":
			await _capture("first_sheathed" if first else "third_sheathed")
