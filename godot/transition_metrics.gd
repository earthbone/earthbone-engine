extends RefCounted

## PRF-26 measurements ending after Godot has drawn the resulting frame.
var samples: Dictionary = {}


func request(kind: String, name: String) -> void:
	_record_after_draw("%s.%s" % [kind, name], Time.get_ticks_usec())


func record_work(name: String, operation: String, item_count: int, elapsed_usec: int) -> void:
	var key := "%s.%s.%d" % [name, operation, item_count]
	_add_sample(key, elapsed_usec)


func _record_after_draw(key: String, started_usec: int) -> void:
	await RenderingServer.frame_post_draw
	_add_sample(key, Time.get_ticks_usec() - started_usec)


func _add_sample(key: String, elapsed_usec: int) -> void:
	if not samples.has(key):
		samples[key] = []
	samples[key].append(elapsed_usec)


func summary_text() -> String:
	var keys: Array = samples.keys()
	keys.sort()
	var output := "transition_or_work count p50_ms worst_ms\n"
	for key: String in keys:
		var values: Array = samples[key].duplicate()
		values.sort()
		var middle: int = int((values.size() - 1) / 2)
		output += "%s %d %.3f %.3f\n" % [
			key,
			values.size(),
			float(values[middle]) / 1000.0,
			float(values.back()) / 1000.0,
		]
	return output
