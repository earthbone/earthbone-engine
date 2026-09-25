extends SceneTree

## RND-08's done check, exercised from a live Godot process against the
## compiled soe-godot-host extension: an authored NIF transform controller
## moves the target along its track and respects its cycle mode.
##
## Fixture: target/godot-check/fixtures/rnd_08_animated_static.nif, written
## by `cargo test -p soe-nif --test rnd_08_write_godot_fixture`
## (crates/nif/tests/rnd_08_write_godot_fixture.rs), run once before this
## script. It is a Root over "Target", an NiNode carrying an
## NiTransformController (Loop, [0, 10]) that moves translation from the
## origin to (100, 0, 0), with one BSTriShape child offset to (2, 0, 0) in
## Target's own local space.
##
## AnimatedStaticNode::load_nif resolves "Target"'s controller chain and
## builds one MeshInstance3D per child mesh, in the node's own local space
## (soe_nif::mesh::meshes_under_children), the same split the Rust level
## check in crates/godot-host/src/data_source/animated_static_mesh_tests.rs
## proves against synthetic geometry. advance(delta) evaluates the
## controller at the accumulated elapsed time and writes the result onto
## every child, the same way _process would each frame while playing is
## true. This script drives advance() directly instead, so each step is
## exactly the delta the check asks for.
##
## Clause 1 (moves along the track): the child mesh's world X position must
## increase between elapsed 1 and elapsed 2, both inside [0, 10].
## Clause 2 (respects its cycle mode): elapsed 13 is one full 10 second Loop
## cycle past elapsed 3, so the pose at elapsed 13 must match the pose at
## elapsed 3, not the held end pose a Clamp controller would show instead.
##
## Run headless:
## Godot_v4.7.2-stable_linux.x86_64 --headless --path <godot-check project> --script rnd_08_test.gd

var failures := 0


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("RESULT: FAIL: ", message)


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	if not ClassDB.class_exists("AnimatedStaticNode"):
		printerr("RESULT: FAIL: soe-godot-host extension did not load (AnimatedStaticNode missing)")
		quit(2)
		return

	var fixture_path := ProjectSettings.globalize_path("res://fixtures/rnd_08_animated_static.nif")
	if not FileAccess.file_exists(fixture_path):
		printerr("RESULT: FAIL: fixture not found at ", fixture_path,
			", run cargo test -p soe-nif --test rnd_08_write_godot_fixture first")
		quit(2)
		return

	var node: Node3D = ClassDB.instantiate("AnimatedStaticNode")
	node.playing = false
	get_root().add_child(node)

	var loaded: bool = node.call("load_nif", fixture_path, "Target")
	_check(loaded, "load_nif could not open %s" % fixture_path)
	if failures > 0:
		node.free()
		quit(1)
		return

	var mesh_count: int = node.call("mesh_count")
	_check(mesh_count == 1, "expected 1 child mesh, got %d" % mesh_count)
	if failures > 0 or node.get_child_count() == 0:
		node.free()
		quit(1)
		return

	var mesh: Node3D = node.get_child(0)

	# Clause 1: the target moves along its track.
	node.call("advance", 1.0)
	var p1: Vector3 = mesh.transform.origin
	node.call("advance", 1.0)
	var p2: Vector3 = mesh.transform.origin
	_check(p2.x > p1.x,
		"expected the mesh to move further along +X between elapsed 1 and 2, got p1.x=%f p2.x=%f" % [p1.x, p2.x])

	# Clause 2: a Loop cycle wraps the pose back into the track. Elapsed is
	# 2 here, advance 1 to reach elapsed 3, record it, then advance a full
	# 10 second cycle to elapsed 13 and compare.
	node.call("advance", 1.0)
	var p3: Vector3 = mesh.transform.origin
	node.call("advance", 10.0)
	var p13: Vector3 = mesh.transform.origin
	_check(p3.distance_to(p13) < 0.0005,
		"expected elapsed 13 (one Loop cycle past elapsed 3) to match elapsed 3's pose, got p3=%s p13=%s" % [p3, p13])

	# And the wrapped pose must actually differ from the held end pose a
	# Clamp controller would show, so this is not vacuously true because the
	# controller stopped moving.
	# Sanity: elapsed accumulates across calls, it does not reset.
	var end: float = node.call("elapsed")
	_check(is_equal_approx(end, 13.0), "expected elapsed() to report 13.0, got %f" % end)
	_check(not p13.is_equal_approx(p2), "elapsed 13 should not sit at the same pose as elapsed 2")

	print("RND-08 SMOKE: p1=", p1, " p2=", p2, " p3=", p3, " p13=", p13, " failures=", failures)
	if failures == 0:
		print("RESULT: PASS: the authored transform controller moves the target along its track and respects its Loop cycle")
	else:
		printerr("RESULT: FAIL: %d check(s) failed, see above" % failures)
	node.free()
	quit(0 if failures == 0 else 1)
