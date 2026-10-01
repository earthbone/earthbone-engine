extends SceneTree

## RND-14's live host check. The NIF fixture is written under
## target/godot-check/fixtures by the Rust rnd_14 test file. It names
## RND14SmokeEmitter, with birth rate 4 per second and life span 0.5 seconds.

var failures := 0


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("RESULT: FAIL: ", message)


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	if not ClassDB.class_exists("ParticleEmitterNode"):
		printerr("RESULT: FAIL: ParticleEmitterNode extension class missing")
		quit(2)
		return

	var fixture := ProjectSettings.globalize_path("res://../target/godot-check/fixtures/rnd_14_emitter.nif")
	if not FileAccess.file_exists(fixture):
		printerr("RESULT: FAIL: fixture missing at ", fixture)
		quit(2)
		return

	var node: Node3D = ClassDB.instantiate("ParticleEmitterNode")
	node.playing = false
	get_root().add_child(node)
	_check(node.call("load_nif", fixture, "RND14SmokeEmitter"), "named emitter did not load")
	if failures == 0:
		node.call("advance", 0.5)
		_check(node.call("emitted_count") == 2, "two births expected after half a second")
		_check(node.call("particle_count") == 2, "two particles expected alive")
		_check(node.call("rendered_count") == 2, "two MultiMesh instances expected visible")
		node.call("advance", 0.5)
		_check(node.call("emitted_count") == 4, "four total births expected after one second")
		_check(node.call("particle_count") == 2, "first two particles should have expired")
		_check(node.call("rendered_count") == 2, "expired particles should leave MultiMesh")
	node.free()
	if failures == 0:
		print("RESULT: PASS: named emitter spawns and expires rendered particles at authored rate and lifetime")
	quit(0 if failures == 0 else 1)
