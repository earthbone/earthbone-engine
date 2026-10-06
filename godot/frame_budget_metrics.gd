extends RefCounted

## Full-run PRF-12 CPU samples. Reporting stays outside the measured window.
## Nearest-rank percentiles follow bench_metrics.gd and godotwind's manual.

var frames: Array[float] = []
var systems := {}


func observe(sample: Dictionary) -> void:
	frames.append(sample.frame_ms)
	for system: String in sample.systems:
		if not systems.has(system):
			systems[system] = []
		systems[system].append(sample.systems[system])


static func summarize(values: Array) -> Dictionary:
	var sorted := values.duplicate()
	sorted.sort()
	return {
		"p50_ms": sorted[ceili(sorted.size() * 0.5) - 1],
		"p99_ms": sorted[ceili(sorted.size() * 0.99) - 1],
		"worst_ms": sorted[-1],
	}


func report(extra: Dictionary) -> Dictionary:
	var row := extra.duplicate()
	row["samples"] = frames.size()
	row["frame"] = summarize(frames)
	row["systems"] = {}
	for system: String in systems:
		row.systems[system] = summarize(systems[system])
	print("SOE_BUDGET " + JSON.stringify(row))
	return row
