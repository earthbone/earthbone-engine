extends RefCounted

## Fresh-process repetitions for bench.gd. OS.execute reaps each child before
## the next launch. Process caches are cold, disk and driver caches are retained.


static func configure(args: PackedStringArray) -> Dictionary:
	var result := {"runs": 1, "run": 1, "launcher": false, "error": ""}
	var seen := {}
	var index := 0
	while index < args.size():
		var option := args[index]
		if option not in ["--cold-starts", "--cold-run"] or seen.has(option):
			result.error = "expected one --cold-starts N or --cold-run N"
			return result
		seen[option] = true
		index += 1
		if index >= args.size() or not args[index].is_valid_int():
			result.error = "%s requires an integer" % option
			return result
		var value := int(args[index])
		if value < 1 or value > 128:
			result.error = "%s must be 1 to 128" % option
			return result
		if option == "--cold-starts":
			result.runs = value
			result.launcher = true
		else:
			result.run = value
		index += 1
	if seen.size() > 1:
		result.error = "cold-start launcher and child options cannot be combined"
	return result


static func child_args(project: String, run: int) -> PackedStringArray:
	return PackedStringArray(["--path", project, "--script", "bench.gd", "--windowed", "--", "--cold-run", str(run)])


static func child_status(exit_code: int, output: String) -> int:
	if exit_code not in [0, 2] or output.contains("SCRIPT ERROR:") or output.contains("ERROR:"):
		return 1
	if exit_code == 0 and not output.contains("RESULT: PASS"):
		return 1
	if exit_code == 2 and not output.contains("RESULT: FAIL: frame budget exceeded"):
		return 1
	return exit_code


static func combined_status(previous: int, child: int) -> int:
	if previous == 1 or child == 1:
		return 1
	return maxi(previous, child)


static func launch(project: String, runs: int) -> int:
	var status := 0
	for index in runs:
		var output: Array = []
		var code := OS.execute(OS.get_executable_path(), child_args(project, index + 1), output, true)
		var log := ""
		for chunk in output:
			log += str(chunk)
		print(log)
		var child := child_status(code, log)
		print("godot_cold_run %d exit %d" % [index + 1, child])
		status = combined_status(status, child)
	print("godot_cold_starts %d runs exit %d" % [runs, status])
	return status
