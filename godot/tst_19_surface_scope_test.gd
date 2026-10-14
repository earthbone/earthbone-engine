# SPDX-License-Identifier: GPL-3.0-or-later
extends "res://tst_19_play_path.gd"

## Observe the real surface checker before and after a load-door transition.
class FixturePlayer extends Node3D:
	var worldspace := ""
	func detection_worldspace() -> String:
		return worldspace
	func play_path_references() -> Array[Dictionary]:
		return [{"kind": "DOOR", "model": "door.nif", "file": 0, "object": 1}]

class FixtureStreamer extends Node3D:
	func play_path_surfaces() -> Array[Dictionary]:
		return []
	func rendered_reference_count(_file: int, _object: int) -> int:
		return 1

var checks := {}
var camera: Camera3D

func _initialize() -> void:
	player = FixturePlayer.new()
	streamer = FixtureStreamer.new()
	camera = Camera3D.new()
	root.add_child(player)
	root.add_child(streamer)
	root.add_child(camera)
	stop_name = "exit"
	call_deferred("_run_fixture")

func _check(id: String, passed: bool, _value: Dictionary) -> void:
	checks[id] = passed

func _run_fixture() -> void:
	await _surface_checks(camera)
	var inside: bool = checks.get("doors.drawn", false) and not checks.has("terrain.textures") and not checks.has("object.white") and not checks.has("object.height")
	checks.clear()
	player.set("worldspace", "WhiterunWorld")
	await _surface_checks(camera)
	var outside: bool = checks.get("doors.drawn", false) and checks.has("terrain.textures") and not checks["terrain.textures"] and checks.has("object.white") and not checks["object.white"] and checks.has("object.height") and not checks["object.height"]
	print("TST19_SURFACE_SCOPE ", "PASS" if inside and outside else "FAIL", " inside=", inside, " exterior_missing_surfaces_fail=", outside)
	quit(0 if inside and outside else 1)
