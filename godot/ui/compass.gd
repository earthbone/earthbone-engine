extends Control

## Repeated native compass strip. Geometry is independent of imported mod art.
const DIRECTIONS := ["N", "NE", "E", "SE", "S", "SW", "W", "NW"]
var scroll_offset := 0.0
var heading_text := "W   NW   [N]   NE   E"

func set_heading(offset: float, description: String) -> void:
	scroll_offset = offset
	heading_text = description
	tooltip_text = description
	queue_redraw()

func _draw() -> void:
	var font := get_theme_default_font()
	var font_size := get_theme_default_font_size()
	var ink := Color(0.94, 0.90, 0.80)
	var center := size.x * 0.5
	for tick in range(-72, 145):
		var x := center + float(tick) * 10.0 + scroll_offset
		if x < -24.0 or x > size.x + 24.0:
			continue
		var major := tick % 9 == 0
		if major:
			var text: String = DIRECTIONS[posmod(floori(float(tick) / 9.0), 8)]
			var width := font.get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1.0, font_size).x
			draw_string(font, Vector2(x - width * 0.5, 18.0), text, HORIZONTAL_ALIGNMENT_LEFT, -1.0, font_size, ink)
		draw_line(Vector2(x, 24.0), Vector2(x, 34.0 if major else 29.0), ink, 1.0)
	draw_colored_polygon(PackedVector2Array([Vector2(center - 4.0, 38.0), Vector2(center, 33.0), Vector2(center + 4.0, 38.0)]), ink)
