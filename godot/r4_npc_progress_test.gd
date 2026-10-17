# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://walkthrough.gd"

## Observe actual progress through the free walk's package and collision path.
const STEP := 1.0 / 60.0
const SECONDS := 30
const STUCK_SECONDS := 5
var failures := 0

func _initialize() -> void:
	call_deferred("_measure")

func _configure_player() -> void:
	player.set("capture_mouse", false)
	player.set("auto_step", false)

func _measure() -> void:
	startup_position = Vector3(-125, -800, 120)
	await super._start()
	if not loaded:
		return
	await _sample("Bannered Mare")
	for door: Dictionary in player.call("gameplay_doors"):
		if not door.locked:
			if not player.call("activate_reference", door.file, door.object):
				failures += 1
			break
	for _frame in 120:
		player.call("advance", STEP)
		await process_frame
	while player.call("is_location_preparing"):
		await process_frame
	player.call("advance", 0.0)
	await _sample("Whiterun")
	print("R4_NPC_RESULT failures=", failures)
	quit(0 if failures == 0 else 1)

func _sample(label: String) -> void:
	var previous := {}
	var initial := {}
	var blocked := {}
	var worst := {}
	var travelled := {}
	var active := {}
	for frame in SECONDS * 60:
		player.call("advance", STEP)
		await process_frame
		if frame % 60 != 59:
			continue
		var packages: Array = player.call("npc_package_states")
		var ground: Array = player.call("npc_grounding_status")
		var states := {}
		for package: Dictionary in packages:
			states[package.actor] = package
		for body: Dictionary in ground:
			var identity := str(body.file) + ":%06X" % int(body.object)
			var position: Vector3 = body.position
			if not initial.has(identity):
				initial[identity] = position
			var moved: float = position.distance_to(previous.get(identity, position))
			travelled[identity] = float(travelled.get(identity, 0.0)) + moved
			var state: Dictionary = states.get(identity, {})
			var routed := int(state.get("path_points", 0)) > 0 and float(state.get("speed", 0)) > 0
			if routed:
				active[identity] = true
			blocked[identity] = int(blocked.get(identity, 0)) + 1 if routed and moved < 1.0 else 0
			worst[identity] = maxi(int(worst.get(identity, 0)), blocked[identity])
			previous[identity] = position
		print("R4_NPC_SAMPLE ", JSON.stringify({"scene": label, "second": (frame + 1) / 60, "ground": ground, "packages": packages, "blocked": blocked, "ai": player.call("ai_breakdown"), "visuals": player.call("npc_visuals")}))
		if frame % 300 == 299:
			var all_routes: String = player.call("debug_view_command", "PrintAiList")
			var resident_routes: Array[String] = []
			for line: String in all_routes.split("\n"):
				if states.has(line.get_slice(" ", 0)):
					resident_routes.append(line)
			print("R4_NPC_ROUTES ", label, " ", "\n".join(resident_routes))
			if label == "Bannered Mare":
				var nearby: Array = []
				for reference: Dictionary in player.call("play_path_references"):
					if reference.position.distance_to(Vector3(319, -467, 69)) < 180.0:
						nearby.append(reference)
				print("R4_NPC_PROPS ", JSON.stringify(nearby))
	var maximum := 0.0
	for identity in active:
		maximum = maxf(maximum, float(travelled.get(identity, 0.0)))
		if int(worst.get(identity, 0)) >= STUCK_SECONDS:
			failures += 1
	if maximum < 64.0:
		failures += 1
	print("R4_NPC_PROGRESS ", JSON.stringify({"scene": label, "travelled_units": travelled, "worst_stuck_seconds": worst, "active": active, "maximum_units": maximum}))
