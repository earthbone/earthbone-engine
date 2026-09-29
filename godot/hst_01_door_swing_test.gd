extends SceneTree

## HST-01's Godot check for an authored rotation and attached collision.
## Generate target/godot-check/fixtures/rnd_08_animated_static.nif with
## `cargo test -p soe-nif --test rnd_08_write_godot_fixture` first.

var failures := 0


func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("RESULT: FAIL: ", message)


func _initialize() -> void:
	call_deferred("_hst_01_door_swing_and_collision_follow")


func _hst_01_door_swing_and_collision_follow() -> void:
	if not ClassDB.class_exists("AnimatedStaticNode"):
		printerr("RESULT: FAIL: soe-godot-host extension did not load")
		quit(2)
		return

	var fixture_path := ProjectSettings.globalize_path("res://fixtures/rnd_08_animated_static.nif")
	if not FileAccess.file_exists(fixture_path):
		printerr("RESULT: FAIL: fixture not found at ", fixture_path)
		quit(2)
		return

	var node: Node3D = ClassDB.instantiate("AnimatedStaticNode")
	node.playing = false
	node.collision_enabled = true
	get_root().add_child(node)
	var loaded: bool = node.call("load_nif", fixture_path, "Target")
	_check(loaded, "animated door fixture did not load")
	if not loaded or node.get_child_count() == 0:
		node.free()
		quit(1)
		return

	var mesh := node.get_child(0) as MeshInstance3D
	var body: StaticBody3D
	for child in mesh.get_children():
		if child is StaticBody3D:
			body = child
	_check(body != null, "collision_enabled did not create a StaticBody3D")
	if body == null:
		node.free()
		quit(1)
		return
	_check(body.get_child_count() > 0, "generated static body has no collision shape")

	# The authored rotation moves from identity to 90 degrees over its 10-second
	# track. At five seconds both the render child and its collider must turn.
	var before: Basis = mesh.global_transform.basis
	node.call("open")
	node.call("advance", 5.0)
	var after: Basis = mesh.global_transform.basis
	_check(not before.is_equal_approx(after), "authored swing did not rotate the mesh")
	_check(body.global_transform.is_equal_approx(mesh.global_transform),
		"door collision transform did not follow its visual mesh")
	_check(body.get_child(0).get_global_transform().is_equal_approx(mesh.global_transform),
		"door collision shape did not follow its visual mesh")

	print("HST-01 SMOKE: rotated=", not before.is_equal_approx(after),
		" collision_follows=", body.global_transform.is_equal_approx(mesh.global_transform),
		" failures=", failures)
	node.free()
	quit(0 if failures == 0 else 1)
