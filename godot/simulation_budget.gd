extends RefCounted

## PRF-37/TST-18 limits, declared before the first installed-data measurement.
const MAIN_P50_MS := 2.0
const MAIN_P99_MS := 4.0
const MAX_BEHIND_SECONDS := 1.0


static func evaluate(samples: Array[float], behind_seconds: float, ticks: int) -> Dictionary:
	if samples.is_empty():
		return {"passed": false, "reason": "no rendered frame samples"}
	var sorted := samples.duplicate()
	sorted.sort()
	var p50: float = sorted[ceili(sorted.size() * 0.50) - 1]
	var p99: float = sorted[ceili(sorted.size() * 0.99) - 1]
	var finite := is_finite(p50) and is_finite(p99) and is_finite(behind_seconds)
	for sample in sorted:
		finite = finite and is_finite(sample) and sample >= 0.0
	return {
		"passed": finite and p50 <= MAIN_P50_MS and p99 <= MAIN_P99_MS and behind_seconds <= MAX_BEHIND_SECONDS and ticks > 0,
		"p50_ms": p50,
		"p99_ms": p99,
		"worst_ms": sorted[-1],
		"behind_seconds": behind_seconds,
		"ticks": ticks,
	}
