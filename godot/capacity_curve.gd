extends RefCounted

## PRF-28 doubling ladder. A passing configured maximum is an unreached limit.

var load_count := 1
var maximum := 256
var max_passing := 0
var first_failing := -1
var finished := false


func configure(start: int, limit: int) -> String:
	if start < 1 or limit < start or limit > 100000:
		return "counts must be 1 to 100000 and start <= max"
	load_count = start
	maximum = limit
	return ""


func observe(passed: bool) -> void:
	if finished:
		return
	if not passed:
		first_failing = load_count
		finished = true
		return
	max_passing = load_count
	if load_count == maximum:
		finished = true
	else:
		load_count = mini(load_count * 2, maximum)


func result() -> Dictionary:
	return {"max_passing_rung": max_passing, "first_failing_rung": first_failing if first_failing >= 0 else null, "status": "boundary" if first_failing >= 0 else "limit_not_found" if finished else "incomplete"}
