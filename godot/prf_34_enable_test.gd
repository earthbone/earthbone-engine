extends "res://rnd_17_enable_parent_test.gd"

## PRF-34 exercises independent rendering and collision notifications using
## RND-17's installed barricade manifest and actual rendered pixel check.
func _run() -> void:
    await super._run()
    print("prf_34_installed_parent_enable_updates_rendering_and_collision failures=", failures)
