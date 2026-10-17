extends "res://r4_runtime_test.gd"

## The same door traversal must also bound streaming work on the frame path.
const STREAMING_LIMIT_MS := 33.0


func _sample(label: String, fps: int) -> void:
	await super._sample(label, fps)
	if label == "door_walk_60fps":
		var worst := 0.0
		for frame: Dictionary in results[label].frames:
			worst = maxf(worst, frame.streaming_ms)
		for frame: Dictionary in results.door_preparation.frames:
			worst = maxf(worst, frame.streaming_ms)
		print("R4_STREAMING worst_ms=", worst, " limit_ms=", STREAMING_LIMIT_MS)
		_check(worst < STREAMING_LIMIT_MS, "door streaming stays below 33 ms")
