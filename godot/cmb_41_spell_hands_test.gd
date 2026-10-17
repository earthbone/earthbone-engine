extends "res://hst_25_equipment_test.gd"

## Spell hands: a held fire and forget spell plays the casting idle with the
## MGEF hand art in first and third person, release clears it, and a resident
## NPC mage does the same through cast_actor_hand.
## Fails before the change: cast_actor_hand does not exist.

func _check_weapons() -> void:
	await super._check_weapons()
	await _check_player_cast()
	await _check_npc_cast()

func _hand_models(view: Node, left: bool) -> Array:
	var models: Array = []
	for hand: Dictionary in view.call("play_path_observation").get("hands", []):
		if hand.left == left:
			models.append(str(hand.model))
	return models

func _check_player_cast() -> void:
	_check(not str(player.call("console_execute", "player.addspell 12fcc")).contains("error:"), "Healing could not be learned")
	_check(player.call("equip_player_spell", 0, 0x12fcc, false), "Healing could not occupy the right hand")
	await _toggle(0x12eb7)
	_check(player.call("toggle_weapon_ready"), "draw for casting failed")
	await _steps(60)
	var before_third: Array = _hand_models(player_body, false)
	print("CMB-41 before hold clips third=", player_body.call("active_clips"), " first=", player_arms.call("active_clips"))
	graph_events = [[], []]
	Input.action_press("Right Attack/Block")
	var clips_seen := [[], []]
	for _step in 30:
		player.call("advance", 1.0 / 60.0)
		for index in 2:
			var rig: Node3D = player_body if index == 0 else player_arms
			rig.call("advance", 1.0 / 60.0)
			for clip in rig.call("active_clips"):
				var name := str(clip).get_slice("@", 0)
				if not name in clips_seen[index]: clips_seen[index].append(name)
		await _animation_frame(_step)
	print("CMB-41 clips during hold third=", clips_seen[0], " first=", clips_seen[1])
	var held_first: Array = _hand_models(player_arms, false)
	var held_third: Array = _hand_models(player_body, false)
	print("CMB-41 events during hold=", graph_events)
	print("CMB-41 player held first=", held_first, " third=", held_third, " before=", before_third)
	_check(held_first.size() > 0, "no hand art in the first person view while casting")
	_check(held_third.size() > 0, "no hand art in the third person view while casting")
	_check("MRh_SpellReady_Event" in str(graph_events) and "MRh_SpellSelf" in str(graph_events), "the cast did not reach the behavior graph: %s" % str(graph_events))
	Input.action_release("Right Attack/Block")
	await _steps(60)
	_check(_hand_models(player_arms, false).size() <= before_third.size(), "hand art stayed after release")
	player.call("toggle_weapon_ready")
	await _steps(60)

func _check_npc_cast() -> void:
	var npcs: Array = player.call("npc_visuals")
	_check(npcs.size() > 0, "no resident NPC to cast with")
	if npcs.is_empty():
		return
	var npc: Dictionary = npcs[0]
	var file := int(npc.get("file", 0))
	var object := int(npc.get("object", 0))
	_check(not str(player.call("console_execute", "prid %x" % object)).contains("error:"), "could not select the resident mage")
	print("CMB-41 npc addspell: ", player.call("console_execute", "addspell 12fcc"))
	var equipped := str(player.call("console_execute", "equipspell 12fcc right"))
	print("CMB-41 npc equipspell: ", equipped)
	_check(not equipped.contains("error:"), "Healing could not be equipped on the resident mage")
	var started: bool = player.call("cast_actor_hand", file, object, false, true)
	print("CMB-41 npc start accepted=", started, " npc=", npc)
	_check(started, "the resident mage cast was not accepted")
	await _steps(30)
	player.call("cast_actor_hand", file, object, false, false)
	await _steps(30)
	_check(not player.call("cast_actor_hand", 0, 0, false, true), "a cast with no actor was accepted")
