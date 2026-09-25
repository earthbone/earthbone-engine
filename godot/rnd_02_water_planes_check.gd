extends SceneTree

# RND-02 acceptance: a cell with water draws its surface at the recorded
# height with its material. Streams DLC1VampireCastleExterior01 from the
# installed data (SKYRIM_DATA), reads the drawn water plane back through
# CellStreamerNode.water_planes and the RenderingServer, and checks its
# height against the worldspace default (the cell's XCLW is the "Default"
# marker) and its override material against the WATR colors and opacity.
# Prints one RESULT line and exits non-zero on failure.

const CELL := "DLC1VampireCastleExterior01"
const METRES_PER_UNIT := 0.0142875
# Tamriel's DNAM default water height in game units, which this cell takes.
const WORLD_WATER_UNITS := -14000.0
const TOLERANCE := 0.01

var failures: Array[String] = []


func _check(ok: bool, what: String) -> void:
	if not ok:
		failures.append(what)
		printerr("FAIL: ", what)


func _initialize() -> void:
	var data := OS.get_environment("SKYRIM_DATA")
	if data.is_empty():
		print("RESULT: FAIL SKYRIM_DATA is not set")
		quit(1)
		return
	var host = ClassDB.instantiate("CellStreamerNode")
	if host == null:
		print("RESULT: FAIL CellStreamerNode is not registered")
		quit(1)
		return
	root.add_child(host)
	_run(host, data)
	host.queue_free()
	if failures.is_empty():
		print("RESULT: PASS")
		quit(0)
	else:
		print("RESULT: FAIL %d check(s): %s" % [failures.size(), ", ".join(failures)])
		quit(1)


func _run(host, data: String) -> void:
	if not host.load_data(data):
		_check(false, "load_data opens the installed data")
		return
	var drawn: int = host.stream_cell(CELL)
	print("stream_cell(%s) drew %d instances" % [CELL, drawn])
	_check(drawn > 0, "the cell draws instances")
	var stats: Dictionary = host.last_stats()
	_check(int(stats.get("water_planes", 0)) == 1, "last_stats counts one water plane")

	var planes: Array = host.water_planes()
	_check(planes.size() == 1, "exactly one water plane instance is drawn")
	if planes.size() != 1:
		return
	var plane: Dictionary = planes[0]
	var want_y := WORLD_WATER_UNITS * METRES_PER_UNIT
	var height: float = plane["height"]
	print("water plane: cell %s, height %.3f m (want %.3f), opacity %d, shallow %s"
		% [plane["cell"], height, want_y, plane["opacity"], plane["shallow_color"]])
	_check(absf(height - want_y) < TOLERANCE, "the plane sits at the recorded water height")

	var rid: RID = plane["rid"]
	_check(rid.is_valid(), "the water instance has a valid RID")

	var material = plane["material"]
	_check(material is StandardMaterial3D, "the water material is a StandardMaterial3D")
	if not (material is StandardMaterial3D):
		return
	var mat := material as StandardMaterial3D
	var shallow: Color = plane["shallow_color"]
	var alpha: float = float(plane["opacity"]) / 255.0
	_check(mat.albedo_color.is_equal_approx(Color(shallow.r, shallow.g, shallow.b, alpha)),
		"the material albedo is the WATR shallow color at ANAM opacity")
	if int(plane["opacity"]) < 255:
		_check(mat.transparency == BaseMaterial3D.TRANSPARENCY_ALPHA, "a translucent water type blends")
	_check(mat.cull_mode == BaseMaterial3D.CULL_DISABLED, "the plane is visible from below")
