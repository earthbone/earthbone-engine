extends SceneTree

const TransitionMetrics := preload("res://transition_metrics.gd")
const SystemOverlay := preload("res://system_overlay.gd")
const MENU_NAMES := [
	"Inventory",
	"Magic",
	"Dialogue",
	"Map",
	"Container",
	"Barter",
	"Crafting",
	"Mod Configuration",
	"Quests",
	"Loot",
]
const ITEM_COUNT := 500

var failures: Array[String] = []
var player: Node3D
var metrics := TransitionMetrics.new()
var overlay: CanvasLayer


func _initialize() -> void:
	call_deferred("_run")


func _run() -> void:
	if DisplayServer.get_name() == "headless" or not ClassDB.class_exists("PlayerNode"):
		_fail("windowed renderer and PlayerNode extension are required")
		_finish()
		return
	var scene := Node3D.new()
	get_root().add_child(scene)
	player = ClassDB.instantiate("PlayerNode") as Node3D
	scene.add_child(player)
	player.transition_requested.connect(_on_transition_requested)
	overlay = SystemOverlay.new()
	overlay.player = player
	get_root().add_child(overlay)
	await process_frame
	for menu_name: String in MENU_NAMES:
		if not player.call("open_menu", menu_name):
			_fail("could not open menu shell: " + menu_name)
			continue
		var content: Control = player.call("menu_content")
		var started := Time.get_ticks_usec()
		var names: Array[String] = []
		for index in ITEM_COUNT:
			var item_name := "%s item %03d" % [menu_name, ITEM_COUNT - index]
			names.append(item_name)
			var label := Label.new()
			label.text = item_name
			content.add_child(label)
		metrics.record_work(menu_name, "list_build", ITEM_COUNT, Time.get_ticks_usec() - started)

		started = Time.get_ticks_usec()
		var filtered: Array[String] = names.filter(func(item: String) -> bool: return item.contains(" 0"))
		metrics.record_work(menu_name, "filter", ITEM_COUNT, Time.get_ticks_usec() - started)
		_check(not filtered.is_empty(), menu_name + " filter returned no items")

		started = Time.get_ticks_usec()
		names.sort()
		metrics.record_work(menu_name, "sort", ITEM_COUNT, Time.get_ticks_usec() - started)
		_check(names.size() == ITEM_COUNT, menu_name + " sort lost list rows")
		await RenderingServer.frame_post_draw
		await process_frame
		_check(player.call("close_menu"), "could not close menu shell: " + menu_name)
		await RenderingServer.frame_post_draw
		await process_frame
		_check(metrics.samples.has("menu_open." + menu_name), menu_name + " open timing missing")
		_check(metrics.samples.has("menu_close." + menu_name), menu_name + " close timing missing")
		_check(metrics.samples.has("%s.list_build.%d" % [menu_name, ITEM_COUNT]), menu_name + " list build timing missing")
		_check(metrics.samples.has("%s.filter.%d" % [menu_name, ITEM_COUNT]), menu_name + " filter timing missing")
		_check(metrics.samples.has("%s.sort.%d" % [menu_name, ITEM_COUNT]), menu_name + " sort timing missing")
	var report: String = metrics.summary_text()
	_check(report.contains("p50_ms worst_ms"), "transition report omitted p50 or worst columns")
	_check(overlay.call("write_transition_summary") == OK, "walkthrough transition summary write failed")
	var saved_report := FileAccess.get_file_as_string("user://soe-transitions.txt")
	_check(saved_report.contains("p50_ms worst_ms"), "walkthrough summary omitted percentile columns")
	for menu_name: String in MENU_NAMES:
		_check(saved_report.contains("menu_open." + menu_name), menu_name + " missing from walkthrough summary")
		_check(saved_report.contains("menu_close." + menu_name), menu_name + " missing from walkthrough summary")
	print("prf_26_menu_transitions_and_500_item_work\n" + report)
	_finish()


func _on_transition_requested(kind: String, name: String) -> void:
	metrics.request(kind, name)


func _check(condition: bool, message: String) -> void:
	if not condition:
		_fail(message)


func _fail(message: String) -> void:
	failures.append(message)
	printerr("FAIL: ", message)


func _finish() -> void:
	if failures.is_empty():
		print("prf_26_menu_transitions_and_500_item_work: PASS")
		quit(0)
	else:
		print("prf_26_menu_transitions_and_500_item_work: FAIL (", failures.size(), ")")
		quit(1)
