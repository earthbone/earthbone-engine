extends SceneTree

## UI-03 acceptance check: "Known spells list and selecting one equips it to
## a hand," run inside a live Godot process so PlayerNode's magic presenter
## (crates/godot-host/src/godot_player/magic.rs) opens, reads and closes
## through the real Node tree, Input and per-frame `advance` loop, the one
## thing no `cargo test` can do (see hst_04_placement_sync_test.gd for the
## same rationale on the streaming row).
##
## What this proves: the extension loads real vanilla data through
## CellStreamerNode, PlayerNode exposes open_magic_menu, select_magic_hand
## and equip_magic_spell to GDScript, the menu refuses to open before a Game
## exists, opens once gameplay starts, both hand buttons stay selectable
## while it is open, an unknown spell is refused end to end through
## magic_menu::equip_spell -> World::equip_hand_spell (the same rejection
## `ui_03_a_spell_the_actor_does_not_know_is_rejected` proves at the World
## level), the menu stays open and responsive across many real engine ticks,
## and closing it disables the hand and equip calls the way the inventory
## menu's closed-handler check already does for its own presenter.
##
## What this does NOT prove: a real vanilla save's player character knowing
## a hand-castable spell and clicking a rendered button for it, because
## nothing exposes a vanilla .ess load or the Console's `addspell` command
## to GDScript (`godot_player/saves.rs` only round-trips the engine's own
## generation format). That gap is the same shape HST-04 documented for its
## own missing enable/disable/moveto wiring, and closing it needs a new
## GDScript entry point, outside a test writer's remit. The full known
## spell to equipped hand path, including a real SPEL record and the save
## round trip, is proven without a live Godot process by
## `game/tests/ui_03_magic_menu.rs` and the shared-native-path test in
## `game/tests/natives_dlc.rs`.
##
## Run with:
##   SKYRIM_DATA="/path/to/Data" godot --headless --path target/godot-check --script ../../godot/ui_03_magic_menu_test.gd

var failures := 0
const CELL := "WhiterunBanneredMare"
const TICKS := 120
const UNKNOWN_SPELL_FILE := 0
const UNKNOWN_SPELL_OBJECT := 0x00999999

func _initialize() -> void:
	call_deferred("_run")

func _check(ok: bool, message: String) -> void:
	if not ok:
		failures += 1
		printerr("FAIL: ", message)

func _run() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty() or not ClassDB.class_exists("PlayerNode"):
		print("SKYRIM_DATA not set or extension not loaded, skipping")
		print("RESULT: PASS ui_03 magic menu (skipped)")
		quit(0)
		return

	var player: Node3D = ClassDB.instantiate("PlayerNode")
	player.set("capture_mouse", false)
	player.set("auto_step", false)
	get_root().add_child(player)

	_check(not player.call("open_magic_menu"), "magic menu opened without a Game")
	_check(not player.call("is_menu_open"), "missing Game left an empty modal")

	var streamer: Node3D = ClassDB.instantiate("CellStreamerNode")
	get_root().add_child(streamer)
	_check(streamer.call("load_data", data), "data load failed")
	_check(player.call("use_streamer_data", streamer), "shared data failed")
	_check(player.call("enter_cell", CELL, 0.0, 0.0, 500.0) > 0,
		"cell collision did not load")
	_check(player.call("start_gameplay"), "runtime start failed")

	_check(player.call("open_magic_menu"), "magic menu did not open with an active Game")
	_check(player.call("is_menu_open"), "open_magic_menu left the modal closed")
	_check(player.call("select_magic_hand", 0), "left hand selection failed while open")
	_check(player.call("select_magic_hand", 1), "right hand selection failed while open")
	_check(
		not player.call("equip_magic_spell", UNKNOWN_SPELL_FILE, UNKNOWN_SPELL_OBJECT),
		"an unknown spell equipped through the live menu"
	)
	_check(player.call("is_menu_open"), "a rejected equip closed the menu")

	# Drive the real tick hook with the magic menu open, the way HST-04
	# drives it with the world streaming, so a presenter that reads World
	# every frame cannot silently start erroring or leaking nodes.
	for i in range(TICKS):
		player.call("advance", 1.0 / 60.0)
	_check(player.call("is_menu_open"), "the magic menu closed on its own across idle ticks")

	player.call("close_menu")
	_check(not player.call("is_menu_open"), "close_menu left the modal open")
	_check(not player.call("select_magic_hand", 0), "hand selection worked after close")
	_check(
		not player.call("equip_magic_spell", UNKNOWN_SPELL_FILE, UNKNOWN_SPELL_OBJECT),
		"equip worked after close"
	)

	player.free()
	streamer.free()
	await process_frame
	print("RESULT: ", "PASS" if failures == 0 else "FAIL", " ui_03 magic menu")
	quit(0 if failures == 0 else 1)
