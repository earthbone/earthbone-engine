# SPDX-License-Identifier: GPL-3.0-or-later
extends Node
## One resource worker owns each immutable cache until the main thread publishes it.
## Godot thread_safe_apis.rst permits resource loading away from the active tree.

signal prepared(cache)
const Cache = preload("res://ui/interface_cache.gd")
const UiTrace = preload("res://ui/trace.gd")

var cache = Cache.new()
var worker: Thread
var requested_directory := ""
var requested_sources: Dictionary = {}
var requested_key := ""
var working_key := ""
var completed_key := ""

func _init() -> void:
	cache.prepared_resources = true

func request(directory: String, sources: Dictionary) -> void:
	var key := directory + JSON.stringify(sources)
	if key == requested_key:
		return
	requested_key = key
	requested_directory = directory
	requested_sources = sources.duplicate()
	if directory != cache.directory:
		var pending = Cache.new()
		pending.prepared_resources = true
		pending.fallbacks = cache.fallbacks
		cache = pending
		prepared.emit(cache)
	if is_node_ready() and worker == null:
		_start_worker()

func _ready() -> void:
	_start_worker()

func _start_worker() -> void:
	if completed_key == requested_key:
		return
	working_key = requested_key
	worker = Thread.new()
	var directory := requested_directory
	var sources := requested_sources.duplicate()
	var error := worker.start(func():
		var result = Cache.new(directory)
		result.prepare_resources(sources)
		return result)
	if error != OK:
		worker = null
		printerr("Item art resource worker failed: ", error)

func _process(_delta: float) -> void:
	if worker == null or worker.is_alive():
		return
	var result = worker.wait_to_finish()
	worker = null
	if working_key == requested_key:
		var _span = UiTrace.begin("ui.cache.publish_resources")
		cache = result
		completed_key = working_key
		prepared.emit(cache)
	_start_worker()

func _exit_tree() -> void:
	if worker != null:
		worker.wait_to_finish()
		worker = null
