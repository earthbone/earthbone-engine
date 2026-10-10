extends "res://walkthrough.gd"

var scr_75_ready_msec := 0

func _process(delta: float) -> bool:
	super(delta)
	if loaded:
		if scr_75_ready_msec == 0:
			scr_75_ready_msec = Time.get_ticks_msec()
		if Time.get_ticks_msec() - scr_75_ready_msec >= 20_000:
			var log_path := OS.get_environment("SOE_SCR_75_LOG")
			if not FileAccess.file_exists(log_path):
				printerr("scr_75_installed_walkthrough: SOE_SCR_75_LOG must capture console output")
				quit(1)
				return false
			var log := FileAccess.get_file_as_string(log_path)
			if log.is_empty() or log.to_lower().contains("has no property value"):
				printerr("scr_75_installed_walkthrough: missing log or property lookup error")
				quit(1)
				return false
			print("scr_75_installed_walkthrough: ready, frames=", frames)
			quit(0)
	elif Time.get_ticks_msec() > 120_000:
		printerr("scr_75_installed_walkthrough: startup timed out")
		quit(1)
	return false
