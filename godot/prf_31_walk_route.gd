extends RefCounted

## Collision-guided walking for the PRF-31 fixture, in Skyrim coordinates.
const STALL_SECONDS := 0.5
const MIN_PROGRESS := 25.0
const TURN_SECONDS := 5.0
var yaw: float
var anchor: Vector2
var since_probe := 0.0
var since_turn := 0.0

func _init(initial_yaw: float, position: Vector3) -> void:
	yaw = initial_yaw
	anchor = Vector2(position.x, position.y)

func direction() -> Vector3:
	return Vector3(sin(yaw), cos(yaw), 0)

func update(position: Vector3, elapsed: float) -> void:
	if not position.is_finite() or not is_finite(elapsed) or elapsed <= 0:
		return
	since_probe += elapsed
	since_turn += elapsed
	if since_probe < STALL_SECONDS:
		return
	var point := Vector2(position.x, position.y)
	if point.distance_to(anchor) < MIN_PROGRESS or since_turn >= TURN_SECONDS:
		yaw += PI / 2.0
		since_turn = 0.0
	anchor = point
	since_probe = 0.0
