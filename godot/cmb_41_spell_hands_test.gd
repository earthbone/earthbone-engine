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
	_check(not str(player.call("console_execute", "player.addspell 12fcd")).contains("error:"), "Flames could not be learned")
	_check(player.call("equip_player_spell", 0, 0x12fcd, false), "Flames could not occupy the right hand")
	await _toggle(0x12eb7)
	_check(player.call("toggle_weapon_ready"), "draw for casting failed")
	await _steps(60)
	var before_third: Array = _hand_models(player_body, false)
	Input.action_press("Right")
	await _steps(30)
	var held_first: Array = _hand_models(player_arms, false)
	var held_third: Array = _hand_models(player_body, false)
	print("CMB-41 player held first=", held_first, " third=", held_third, " before=", before_third)
	_check(held_first.size() > 0, "no hand art in the first person view while casting")
	_check(held_third.size() > 0, "no hand art in the third person view while casting")
	_check("SpellAimedStart" in str(graph_events) or "SpellReady" in str(graph_events) or "SpellCastingLoop" in str(graph_events), "the cast did not reach the behavior graph: %s" % str(graph_events))
	Input.action_release("Right")
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
	var started: bool = player.call("cast_actor_hand", file, object, false, true)
	print("CMB-41 npc start accepted=", started, " npc=", npc)
	await _steps(30)
	player.call("cast_actor_hand", file, object, false, false)
	await _steps(30)
	_check(not player.call("cast_actor_hand", 0, 0, false, true), "a cast with no actor was accepted")
