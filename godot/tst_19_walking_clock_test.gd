# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://tst_19_play_path.gd"

## Godot MainLoop tick semantics, with the inline host's two-step catch-up cap.
class FixturePlayer extends Node3D:
	var feet := Vector3.ZERO
	var blocked := false
	func position() -> Vector3:
		return feet
	func _physics_process(delta: float) -> void:
		if Input.is_physical_key_pressed(KEY_W) and not blocked:
			feet.y += 180.0 * delta
	func set_facing(_yaw: float, _pitch: float) -> void:
		pass
	func play_path_navigation_details(target: Vector3) -> Dictionary:
		return {"points": PackedVector3Array([feet, target]), "reason": ""}

var walks: Array[Dictionary] = []

func _initialize() -> void:
	Engine.max_fps = 10
	Engine.max_physics_steps_per_frame = 2
	player = FixturePlayer.new()
	root.add_child(player)
	call_deferred("_run_fixture")
	_watch_fixture()

func _pause_observation() -> void:
	await create_timer(0.3).timeout
	sampling = true
	_key(false)
	await create_timer(1.1).timeout
	sampling = false

func _interrupt_walk() -> void:
	await create_timer(0.1).timeout
	finished = true

func _run_fixture() -> void:
	var start := Time.get_ticks_msec()
	_pause_observation()
	var reached := await _walk_to(Vector3(0, 150, 0), 1.0)
	var wall_s := (Time.get_ticks_msec() - start) / 1000.0
	var travelled: float = player.call("position").y
	var moving_ok: bool = reached and travelled >= 114.0 and wall_s > 1.0 and walks.size() == 1 and walks[0].walking_s < 1.0
	finished = false
	player.set("feet", Vector3.ZERO)
	player.set("blocked", true)
	var failed := not await _walk_to(Vector3(0, 150, 0), 0.25)
	var blocked_ok: bool = failed and finished and player.call("position") == Vector3.ZERO and walks.size() == 2 and not walks[1].reached and walks[1].walking_s >= 0.25 and walks[1].walking_s <= 0.25 + 2.0 / 60.0
	finished = false
	player.set("blocked", false)
	_interrupt_walk()
	var interrupted := not await _walk_to(Vector3(0, 150, 0), 1.0)
	var interrupted_ok: bool = interrupted and finished and walks.size() == 3 and not walks[2].reached and player.call("position").y < 114.0
	print("TST19_WALKING_CLOCK ", "PASS" if moving_ok and blocked_ok and interrupted_ok else "FAIL", " moving=", moving_ok, " blocked=", blocked_ok, " interrupted=", interrupted_ok, " wall_s=", wall_s, " walks=", walks)
	quit(0 if moving_ok and blocked_ok and interrupted_ok else 1)

func _check(id: String, passed: bool, measured: Dictionary) -> void:
	if id == "route.walk":
		walks.append({"reached": passed, "walking_s": measured.get("walking_s", -1.0)})

func _finish() -> void:
	finished = true
	_key(false)

func _watch_fixture() -> void:
	for _sample in range(8):
		await create_timer(1.0).timeout
		print("TST19_WALKING_CLOCK sample ", {"walking_s": walking_seconds, "position": player.call("position"), "moving": moving, "sampling": sampling})
	printerr("TST19_WALKING_CLOCK FAIL: walker did not finish its finite budget")
	quit(1)
