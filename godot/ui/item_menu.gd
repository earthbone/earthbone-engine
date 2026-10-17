# SPDX-License-Identifier: GPL-3.0-or-later
extends Control
## New native presenter informed by SkyUI-Community's InventoryLists,
## NameFilter, SortFilter and config.txt. No ActionScript runs here.

signal activate_item(row: Dictionary, amount: int, side: int)
signal favorite_item(row: Dictionary)

const Icon = preload("res://ui/item_icon.gd")
const DefaultTheme = preload("res://ui/theme/theme.gd")
const InterfaceCache = preload("res://ui/interface_cache.gd")
const ArtLibrary = preload("res://ui/item_art_library.gd")
const ROW_BUILD_US := 2000
const INVENTORY_CATEGORIES = ["All", "Weapons", "Apparel", "Potions", "Scrolls", "Food", "Ingredients", "Books", "Keys", "Misc"]
const FAVORITE_CATEGORIES = ["All", "Gear", "Aid", "Magic"]
const MAGIC_CATEGORIES = ["All", "Spells"]
const LABELS = {"equipped": "", "icon": "", "name": "NAME", "category": "TYPE", "weight": "WGT", "value": "VAL", "ratio": "V/W", "damage": "DMG", "armor": "ARM", "cost": "COST"}

const UiTrace = preload("res://ui/trace.gd")

var mode := "inventory"
var source_rows: Array = []
var visible_rows: Array = []
var category := "All"
var side := 0
var sort_key := "name"
var descending := false
var selected_id := ""
var query := ""
var cache_directory := ""
var icon_sources: Dictionary = {}
var art_library: Node
var async_menu := false
var next_row := 0
var chrome_pending := false
var sort_pending := false
var selection_pending := false
var row_spacer: Control
var art_updates: Array = []
var art_cache = InterfaceCache.new()
var categories: VBoxContainer
var header: HBoxContainer
var rows: VBoxContainer
var card: VBoxContainer
var search: LineEdit
var quantity: SpinBox
var primary: Button
var favorite: Button
var tabs: HBoxContainer
var debounce: Timer
var columns: Array = []
var gold_label: Label
var player_gold := 0
var merchant_gold := 0

func _ready() -> void:
	var _span = UiTrace.begin("ui.items.ready")
	name = "ItemMenu"
	set_meta("hand", 1)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	size_flags_vertical = Control.SIZE_EXPAND_FILL
	custom_minimum_size = Vector2(0, 180)
	add_theme_font_size_override("font_size", 12)
	theme = DefaultTheme.controls()
	_apply_cache_font()
	var background := ColorRect.new()
	background.color = Color(0.025, 0.03, 0.035, 0.94)
	background.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(background)
	background.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for decoration_name in ["background", "frame"]:
		var decoration := Icon.new()
		decoration.name = decoration_name.capitalize()
		decoration.icon_name = decoration_name
		decoration.stretch_art = true
		decoration.art_color = Color(0.035, 0.045, 0.05, 0.96) if decoration_name == "background" else Color(0.10, 0.13, 0.15, 0.85)
		decoration.cache_directory = cache_directory
		decoration.cache = art_cache
		decoration.cache_movie = "itemcard.swf"
		decoration.cache_symbol = "ItemCard"
		decoration.cache_instance = "background"
		add_child(decoration)
		decoration.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	var body := HBoxContainer.new()
	body.name = "Body"
	add_child(body)
	body.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	body.add_theme_constant_override("separation", 10)
	categories = VBoxContainer.new()
	categories.name = "Categories"
	categories.add_theme_constant_override("separation", 0)
	categories.custom_minimum_size.x = 100
	body.add_child(categories)
	var list := VBoxContainer.new()
	list.name = "List"
	list.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(list)
	tabs = HBoxContainer.new()
	tabs.name = "Tabs"
	list.add_child(tabs)
	gold_label = Label.new()
	gold_label.name = "Gold"
	list.add_child(gold_label)
	search = LineEdit.new()
	search.name = "Search"
	search.placeholder_text = "Search"
	search.clear_button_enabled = true
	search.text_changed.connect(_search_changed)
	list.add_child(search)
	debounce = Timer.new()
	debounce.one_shot = true
	debounce.wait_time = 0.3
	debounce.timeout.connect(refresh)
	add_child(debounce)
	header = HBoxContainer.new()
	header.name = "Columns"
	header.add_theme_constant_override("separation", 0)
	list.add_child(header)
	var scroll := ScrollContainer.new()
	scroll.name = "Scroll"
	scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	list.add_child(scroll)
	rows = VBoxContainer.new()
	rows.name = "Rows"
	rows.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	rows.add_theme_constant_override("separation", 0)
	scroll.add_child(rows)
	card = VBoxContainer.new()
	card.name = "ItemCard"
	card.custom_minimum_size.x = 132
	card.size_flags_horizontal = Control.SIZE_SHRINK_END
	body.add_child(card)
	resized.connect(_resize_card)
	refresh()

func configure(menu_mode: String, data: Array, cache: String = "", sources: Dictionary = {}, library: Node = null) -> void:
	var _span = UiTrace.begin("ui.items.configure")
	mode = menu_mode
	source_rows = data
	icon_sources = sources
	art_library = library
	async_menu = library != null
	if library != null:
		library.connect("prepared", Callable(self, "_prepared_art"))
		var prepared = library.get("cache")
		set_art_cache(str(prepared.directory), prepared)
	else:
		set_art_cache(cache)

func _prepared_art(prepared) -> void:
	set_art_cache(str(prepared.directory), prepared)

func set_art_cache(cache: String, prepared = null) -> void:
	var _span = UiTrace.begin("ui.items.art")
	cache_directory = cache
	if prepared != null:
		art_cache = prepared
	if is_node_ready():
		_apply_cache_font()
		if async_menu:
			art_updates = find_children("*", "Control", true, false).filter(func(control): return control.has_method("use_cache"))
			set_process(true)
		else:
			for decoration_name in ["Background", "Frame"]:
				get_node(decoration_name).use_cache(art_cache)
			refresh()

func _apply_cache_font() -> void:
	if art_cache.directory != cache_directory:
		art_cache = InterfaceCache.new(cache_directory)
	theme.default_font = art_cache.font("$EverywhereMediumFont")

func replace_rows(data: Array) -> void:
	source_rows = data
	refresh()

func set_gold(player_amount: int, merchant_amount: int) -> void:
	player_gold = player_amount
	merchant_gold = merchant_amount
	gold_label.text = "Your gold: %d    Merchant gold: %d" % [player_gold, merchant_gold]

func _resize_card() -> void:
	card.custom_minimum_size.x = maxf(112.0, size.x * 0.24)

func _clear(parent: Node) -> void:
	for child in parent.get_children():
		parent.remove_child(child)
		child.queue_free()

func _button(text: String, parent: Node, callback: Callable) -> Button:
	var button := Button.new()
	button.text = text
	button.add_theme_font_size_override("font_size", 12)
	button.pressed.connect(callback)
	parent.add_child(button)
	return button

func _search_changed(text: String) -> void:
	query = text
	if text.is_empty():
		debounce.stop()
		refresh()
	else:
		debounce.start()

static func normalized(text: String) -> String:
	var groups := {"a": "àáâãäåæąă", "c": "çćč", "d": "ðď", "e": "èéêëęě", "g": "ğ", "i": "ìíîïıį", "l": "łĺľ", "n": "ñńň", "o": "òóôõöøő", "r": "řŕ", "s": "ßśšş", "t": "þťţ", "u": "ùúûüůűū", "y": "ýÿ", "z": "źżž", "г": "ґ", "е": "ёє", "и": "ії", "й": "ј", "с": "ѕ", "у": "ў"}
	var result := ""
	for character in text.to_lower():
		var code := character.unicode_at(0)
		if (code >= 161 and code <= 191) or code == 215 or code == 247:
			continue
		var mapped := character
		for key in groups:
			if character in groups[key]:
				mapped = key
				break
		result += mapped
	return result

func _matches(row: Dictionary) -> bool:
	if int(row.get("side", 0)) != side:
		return false
	if category == "Favorites" and not row.get("favorite", false):
		return false
	elif mode == "favorites":
		if not row.get("favorite", false):
			return false
		var group := "Other"
		if row.category in ["Weapons", "Apparel", "Lights"]:
			group = "Gear"
		elif row.category in ["Potions", "Food", "Ingredients"]:
			group = "Aid"
		elif row.category in ["Spells", "Scrolls", "Shouts"]:
			group = "Magic"
		if category != "All" and category != group:
			return false
	elif category not in ["All", "Favorites"] and row.category != category:
		return false
	return query.is_empty() or normalized(str(row.name)).contains(normalized(query))

func _value(row: Dictionary, key: String) -> Variant:
	if key == "ratio":
		var weight = row.get("weight")
		return float(row.get("value", 0)) / float(weight) if weight != null and weight > 0 else null
	return row.get(key)

func _less(left: Dictionary, right: Dictionary) -> bool:
	var a = _value(left, sort_key)
	var b = _value(right, sort_key)
	if a == null or b == null:
		if a != b:
			return a != null
	if bool(left.get("enabled", true)) != bool(right.get("enabled", true)):
		return bool(left.get("enabled", true))
	if a is String:
		a = a.to_lower()
		b = str(b).to_lower()
	elif a is bool:
		a = int(a)
		b = int(b)
	if a != b:
		return a > b if descending else a < b
	var left_name := str(left.name).to_lower()
	var right_name := str(right.name).to_lower()
	if left_name != right_name:
		return left_name < right_name
	return str(left.id) < str(right.id)

func sort_by(key: String) -> void:
	if key == sort_key:
		descending = not descending
	else:
		sort_key = key
		descending = key in ["damage", "armor", "weight", "value", "ratio", "cost", "equipped"]
	refresh()

func select_category(value: String) -> void:
	if category == value:
		return
	category = value
	refresh()

func select_side(value: int) -> void:
	side = value
	selected_id = ""
	refresh()

func column_order() -> Array:
	if mode == "magic":
		return ["equipped", "icon", "name", "category", "cost"]
	var order := ["equipped", "icon", "name"]
	if category != "Keys":
		order.append("category")
	if category == "Weapons":
		order.append("damage")
	elif category == "Apparel":
		order.append("armor")
	order.append_array(["weight", "value"])
	return order

func _cell_width(key: String) -> float:
	if key == "category":
		return 76.0
	return 20.0 if key in ["equipped", "icon"] else 34.0

func _cell(control: Control, key: String) -> void:
	control.custom_minimum_size.x = _cell_width(key)
	if key == "name":
		control.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		control.custom_minimum_size.x = 55
	control.mouse_filter = Control.MOUSE_FILTER_IGNORE

func refresh() -> void:
	var _span = UiTrace.begin("ui.items.refresh")
	if not is_node_ready():
		return
	gold_label.visible = mode == "barter"
	_clear(categories)
	_clear(header)
	_clear(rows)
	row_spacer = null
	next_row = 0
	art_updates = []
	_clear(tabs)
	columns = column_order()
	chrome_pending = async_menu
	if not async_menu:
		_build_chrome()
	visible_rows = source_rows.filter(_matches)
	sort_pending = async_menu
	selection_pending = async_menu
	if not async_menu:
		_sort_visible()
	if async_menu:
		row_spacer = Control.new()
		row_spacer.mouse_filter = Control.MOUSE_FILTER_IGNORE
		rows.add_child(row_spacer)
	else:
		for row in visible_rows:
			_append_row(row)
		next_row = visible_rows.size()
	set_process(chrome_pending or sort_pending or selection_pending or (async_menu and next_row < visible_rows.size()))
	if async_menu:
		_clear(card)
	else:
		_finish_selection()

func _sort_visible() -> void:
	var _span = UiTrace.begin("ui.items.sort")
	var started := Time.get_ticks_usec()
	visible_rows.sort_custom(_less)
	set_meta("sort_build_ms", (Time.get_ticks_usec() - started) / 1000.0)

func _finish_selection() -> void:
	var _span = UiTrace.begin("ui.items.selection")
	if visible_rows.is_empty():
		var empty := Label.new()
		empty.text = "No matching items"
		rows.add_child(empty)
		selected_id = ""
		_clear(card)
	else:
		var selection = visible_rows.filter(func(row): return str(row.id) == selected_id)
		_select(selection[0] if not selection.is_empty() else visible_rows[0])

func _build_chrome() -> void:
	var choices: Array = INVENTORY_CATEGORIES.duplicate()
	if mode == "favorites":
		choices = FAVORITE_CATEGORIES
	elif mode == "magic":
		choices = MAGIC_CATEGORIES
	elif mode == "inventory":
		choices.insert(1, "Favorites")
	for choice in choices:
		var button := _button(str(choice), categories, select_category.bind(str(choice)))
		button.name = str(choice)
		button.toggle_mode = true
		button.button_pressed = category == choice
		button.add_theme_font_size_override("font_size", 10)
		button.custom_minimum_size.y = 20
		var category_art := Icon.new()
		category_art.name = "CategoryIcon"
		category_art.icon_name = DefaultTheme.category_icon(str(choice))
		category_art.cache_directory = cache_directory
		category_art.cache = art_cache
		category_art.cache_movie = str(icon_sources.get("category", ""))
		category_art.cache_icon_name = DefaultTheme.installed_category(str(choice))
		category_art.custom_minimum_size = Vector2(20, 20)
		button.add_child(category_art)
		category_art.position = Vector2(1, 1)
		category_art.size = Vector2(20, 20)
		button.alignment = HORIZONTAL_ALIGNMENT_RIGHT
	if mode in ["container", "barter"]:
		var sides := ["Take", "Store"] if mode == "container" else ["Buy", "Sell"]
		for index in range(2):
			var tab := _button(sides[index], tabs, select_side.bind(index))
			tab.toggle_mode = true
			tab.button_pressed = side == index
	elif mode == "magic":
		var left := _button("Left hand", tabs, _choose_hand.bind(0))
		left.name = "LeftHand"
		var right := _button("Right hand", tabs, _choose_hand.bind(1))
		right.name = "RightHand"
	columns = column_order()
	for key in columns:
		var title: String = LABELS[key]
		if key == sort_key:
			title += " ▼" if descending else " ▲"
		var button := _button(title, header, sort_by.bind(str(key)))
		button.name = str(key)
		button.tooltip_text = "Equipped" if key == "equipped" else str(LABELS[key])
		button.custom_minimum_size.x = _cell_width(key)
		if key == "name":
			button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			button.custom_minimum_size.x = 55

func _append_row(row: Dictionary) -> void:
	var _span = UiTrace.begin("ui.items.row")
	var button := _button("", rows, _activate.bind(row))
	button.name = "Item_" + str(row.id).replace(":", "_")
	button.custom_minimum_size.y = 24
	button.disabled = not row.get("enabled", true)
	button.mouse_entered.connect(_select.bind(row))
	button.focus_entered.connect(_select.bind(row))
	var cells := HBoxContainer.new()
	cells.add_theme_constant_override("separation", 0)
	cells.mouse_filter = Control.MOUSE_FILTER_IGNORE
	button.add_child(cells)
	cells.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for key in columns:
		if key == "icon":
			var icon := Icon.new()
			icon.icon_name = row.get("icon", "default_misc")
			icon.cache_directory = cache_directory
			icon.cache = art_cache
			icon.cache_movie = str(icon_sources.get("category" if row.category == "Spells" else "item", ""))
			_cell(icon, key)
			cells.add_child(icon)
		elif key == "equipped" and row.get("equipped", false):
			var marker := Icon.new()
			marker.icon_name = "marker"
			marker.cache_directory = cache_directory
			marker.cache = art_cache
			marker.cache_movie = "inventorylists.swf"
			marker.cache_symbol = "EquipIcon"
			_cell(marker, key)
			cells.add_child(marker)
		else:
			var label := Label.new()
			label.add_theme_font_size_override("font_size", 12)
			_cell(label, key)
			label.text = _display(row, key)
			label.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
			cells.add_child(label)

func _append_rows(deadline: int) -> void:
	var _span = UiTrace.begin("ui.items.build_rows")
	while next_row < visible_rows.size() and Time.get_ticks_usec() < deadline:
		_append_row(visible_rows[next_row])
		next_row += 1
	if row_spacer != null:
		row_spacer.custom_minimum_size.y = (visible_rows.size() - next_row) * 24.0
		rows.move_child(row_spacer, rows.get_child_count() - 1)
		if next_row == visible_rows.size():
			rows.remove_child(row_spacer)
			row_spacer.queue_free()
			row_spacer = null

func _process(_delta: float) -> void:
	var started := Time.get_ticks_usec()
	if chrome_pending:
		_build_chrome()
		chrome_pending = false
		set_meta("chrome_build_ms", (Time.get_ticks_usec() - started) / 1000.0)
		return
	if sort_pending:
		_sort_visible()
		sort_pending = false
		return
	var deadline := started + ROW_BUILD_US
	if selection_pending:
		_finish_selection()
		selection_pending = false
	while not art_updates.is_empty() and Time.get_ticks_usec() < deadline:
		var icon = art_updates.pop_back()
		if is_instance_valid(icon) and not icon.is_queued_for_deletion():
			icon.use_cache(art_cache)
	if next_row < visible_rows.size():
		_append_rows(deadline)
	var elapsed_ms := (Time.get_ticks_usec() - started) / 1000.0
	set_meta("rows_batch_max_ms", maxf(float(get_meta("rows_batch_max_ms", 0.0)), elapsed_ms))
	set_process(selection_pending or not art_updates.is_empty() or next_row < visible_rows.size())

func _display(row: Dictionary, key: String) -> String:
	if key == "equipped":
		return "E" if row.get(key, false) else ""
	if key == "name":
		var text := str(row.name)
		if row.get("count", 1) > 1:
			text += " (%d)" % row.count
		if row.get("favorite", false):
			text += " *"
		return text
	var value = _value(row, key)
	if value == null:
		return ""
	if value is float:
		return "%.1f" % value
	return str(value)

func _choose_hand(hand: int) -> void:
	set_meta("hand", hand)

func _select(row: Dictionary) -> void:
	var stages := {}
	var stamp := Time.get_ticks_usec()
	set_meta("selection_name", str(row.name))
	set_meta("selection_description_chars", str(row.get("description", "")).length())
	selected_id = str(row.id)
	_clear(card)
	var title := Label.new()
	title.name = "Name"
	title.text = str(row.name)
	title.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	title.add_theme_font_size_override("font_size", 16)
	card.add_child(title)
	stages["title"] = (Time.get_ticks_usec() - stamp) / 1000.0
	stamp = Time.get_ticks_usec()
	var details := Label.new()
	details.name = "Details"
	details.text = str(row.category)
	for key in ["damage", "armor", "weight", "value", "cost"]:
		if row.get(key) != null:
			details.text += "\n%s  %s" % [LABELS[key], _display(row, key)]
	details.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	details.add_theme_font_size_override("font_size", 12)
	card.add_child(details)
	stages["details"] = (Time.get_ticks_usec() - stamp) / 1000.0
	stamp = Time.get_ticks_usec()
	var description := Label.new()
	description.name = "Description"
	description.text = str(row.get("description", ""))
	description.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	description.size_flags_vertical = Control.SIZE_EXPAND_FILL
	description.add_theme_font_size_override("font_size", 12)
	card.add_child(description)
	stages["description"] = (Time.get_ticks_usec() - stamp) / 1000.0
	stamp = Time.get_ticks_usec()
	quantity = SpinBox.new()
	quantity.name = "Quantity"
	quantity.min_value = 1
	quantity.max_value = maxi(1, int(row.get("count", 1)))
	quantity.value = 1
	quantity.visible = mode in ["container", "barter"]
	card.add_child(quantity)
	stages["quantity"] = (Time.get_ticks_usec() - stamp) / 1000.0
	stamp = Time.get_ticks_usec()
	var action := "Equip"
	if mode == "container":
		action = "Take" if side == 0 else "Store"
	elif mode == "barter":
		action = "Buy" if side == 0 else "Sell"
	primary = _button(action, card, _confirm.bind(row))
	primary.name = "Confirm"
	primary.disabled = not row.get("enabled", true)
	favorite = _button("Unfavorite" if row.get("favorite", false) else "Favorite", card, favorite_item.emit.bind(row))
	favorite.name = "Favorite"
	favorite.visible = mode in ["inventory", "magic", "favorites"]
	stages["actions"] = (Time.get_ticks_usec() - stamp) / 1000.0
	stamp = Time.get_ticks_usec()

	set_meta("card_stages_ms", stages)

func _confirm(row: Dictionary) -> void:
	activate_item.emit(row, int(quantity.value), side)

func _activate(row: Dictionary) -> void:
	_select(row)
	if mode in ["inventory", "magic", "favorites"]:
		_confirm(row)
	elif mode == "container" and side == 0 and row.get("weight") == 0.0:
		activate_item.emit(row, int(row.get("count", 1)), side)
	elif row.get("count", 1) < 6:
		_confirm(row)
	else:
		quantity.grab_focus()
