extends "res://hst_25_equipment_test.gd"

## The inventory and control-map route must also select block, power and bow clips.

func _check_weapons() -> void:
	await super._check_weapons()
	await _ready(true)
	Input.action_press("Left Attack/Block")
	var blocked := [false, false]
	for _step in 45:
		player.call("advance", 1.0 / 60.0)
		for index in 2:
			var rig: Node3D = player_body if index == 0 else player_arms
			rig.call("advance", 1.0 / 60.0)
			for clip in rig.call("active_clips"):
				blocked[index] = blocked[index] or "block" in str(clip).to_lower()
		await _animation_frame(_step)
	Input.action_release("Left Attack/Block")
	player.call("advance", 1.0 / 60.0)
	_check(blocked[0] and blocked[1], "shield mouse hold did not play block clips in both views")
	print("HST-25 block clips=", blocked)
	await _steps(60)
	await _toggle(0x12eb6)
	await _toggle(0x12eb7)
	for weapon in [0x1397e, 0x13981]:
		player.call("console_execute", "player.additem %x 1" % weapon)
		await _toggle(weapon)
		if weapon == 0x13981:
			Input.action_press("Left Attack/Block")
			var two_hand_block := [false, false]
			for _step in 45:
				player.call("advance", 1.0 / 60.0)
				for index in 2:
					var rig: Node3D = player_body if index == 0 else player_arms
					rig.call("advance", 1.0 / 60.0)
					for clip in rig.call("active_clips"):
						two_hand_block[index] = two_hand_block[index] or "block" in str(clip).to_lower()
				await _animation_frame(_step)
			Input.action_release("Left Attack/Block")
			player.call("advance", 1.0 / 60.0)
			_check(two_hand_block[0] and two_hand_block[1], "two handed mouse hold did not play both block clips")
			await _steps(60)
		player.call("console_execute", "player.setav stamina 1000")
		contact_seen = false
		contact_health = _health()
		contact_after = contact_health
		graph_events = [[], []]
		Input.action_press("Right Attack/Block")
		var powers := [false, false]
		var clip_names := [[], []]
		for _step in 100:
			player.call("advance", 1.0 / 60.0)
			for index in 2:
				var rig: Node3D = player_body if index == 0 else player_arms
				rig.call("advance", 1.0 / 60.0)
				for clip in rig.call("active_clips"):
					powers[index] = powers[index] or "power" in str(clip).to_lower()
					var name := str(clip).get_slice("@", 0)
					if not name in clip_names[index]: clip_names[index].append(name)
			if contact_seen and contact_after == contact_health: contact_after = _health()
			await _animation_frame(_step)
		Input.action_release("Right Attack/Block")
		player.call("advance", 1.0 / 60.0)
		_check(powers[0] and powers[1], "weapon %x hold did not play both power clips" % weapon)
		_check("HitFrame" in graph_events[0] and "HitFrame" in graph_events[1], "weapon %x power contact missing" % weapon)
		_check(contact_seen and _health() < contact_health, "weapon %x power marker did not apply damage" % weapon)
		_check(is_equal_approx(contact_after, _health()), "weapon %x power applied damage twice" % weapon)
		print("HST-25 power weapon=%x clips=" % weapon, clip_names, " events=", graph_events)
		if weapon == 0x13981:
			_check("2hm" in str(clip_names).to_lower() or "2hw" in str(clip_names).to_lower(), "two handed weapon selected one handed power clips")
		await _steps(90)
		await _toggle(weapon)
	for object in [0x13985, 0x1397d]:
		if object == 0x1397d:
			var previous: int = player.call("play_path_item_count", 0, object)
			if previous > 0:
				_check(not str(player.call("console_execute", "player.removeitem %x %d" % [object, previous])).contains("error:"), "starting arrows could not be cleared")
		player.call("console_execute", "player.additem %x 10" % object)
		await _toggle(object)
	_check(player.call("play_path_item_count", 0, 0x1397d) == 10, "bow fixture must begin with exactly ten arrows")
	print("HST-25 arrows before=", player.call("play_path_item_count", 0, 0x1397d))
	for rig in [player_body, player_arms]:
		rig.connect("weapon_contact", _bow_contact.bind(rig == player_arms))
	graph_events = [[], []]
	Input.action_press("Right Attack/Block")
	var bows := [false, false]
	var bow_clips := [[], []]
	for _step in 90:
		player.call("advance", 1.0 / 60.0)
		for index in 2:
			var rig: Node3D = player_body if index == 0 else player_arms
			rig.call("advance", 1.0 / 60.0)
			for clip in rig.call("active_clips"):
				bows[index] = bows[index] or "bow_draw" in str(clip).to_lower() or "bowdraw" in str(clip).to_lower()
				var name := str(clip).get_slice("@", 0)
				if not name in bow_clips[index]: bow_clips[index].append(name)
		await _animation_frame(_step)
	Input.action_release("Right Attack/Block")
	player.call("advance", 1.0 / 60.0)
	await _steps(90)
	_check(bows[0] and bows[1], "bow mouse hold did not play both draw clips")
	_check("attackRelease" in graph_events[0] and "attackRelease" in graph_events[1] and ("arrowRelease" in graph_events[0] or "arrowRelease" in graph_events[1]), "bow release marker missing")
	_check(player.call("play_path_item_count", 0, 0x1397d) == 9, "authored bow release did not consume one arrow")
	print("HST-25 arrows after=", player.call("play_path_item_count", 0, 0x1397d))
	print("HST-25 bow clips=", bow_clips, " events=", graph_events)

func _bow_contact(event: String, _left: bool, _pose: Transform3D, first: bool) -> void:
	if event == "arrowRelease":
		print("HST-25 arrow callback first=", first, " third_view=", player.call("is_third_person"), " arrows=", player.call("play_path_item_count", 0, 0x1397d))
