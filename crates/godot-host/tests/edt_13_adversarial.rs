// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

const FORMS_PLUGIN: &str = include_str!("../../../godot/addons/soe_forms/forms_plugin.gd");
const INSPECTOR: &str = include_str!("../../../godot/addons/soe_forms/form_inspector.gd");
const INSPECTOR_TEST: &str = include_str!("../../../godot/edt_13_inspector_test.gd");
const PROJECT: &str = include_str!("../../../godot/project.godot");
const BRIDGE: &str = include_str!("../src/godot_forms.rs");

#[test]
fn edt_13_enabled_forms_plugin_registers_and_removes_inspector() {
    assert!(PROJECT.contains("res://addons/soe_forms/plugin.cfg"));
    assert!(FORMS_PLUGIN.contains("add_inspector_plugin(form_inspector)"));
    assert!(FORMS_PLUGIN.contains("remove_inspector_plugin(form_inspector)"));
}

#[test]
fn edt_13_inspector_handles_only_forms_and_displays_live_winner_metadata() {
    let can_handle = INSPECTOR
        .split("func _can_handle(object: Object) -> bool:")
        .nth(1)
        .unwrap()
        .split("func _parse_begin")
        .next()
        .unwrap();
    let control = INSPECTOR
        .split("func winning_source_control(object: Object) -> Control:")
        .nth(1)
        .unwrap();
    let parse_begin = INSPECTOR
        .split("func _parse_begin(object: Object) -> void:")
        .nth(1)
        .unwrap()
        .split("func winning_source_control")
        .next()
        .unwrap();

    assert!(can_handle.contains("object.get_class() == \"SoeFormResource\""));
    assert!(parse_begin.contains("add_custom_control(winning_source_control(object))"));
    assert!(control.contains("title.text = \"Winning source:\""));
    assert!(control.contains("source.text = object.get(\"winning_plugin\")"));
}

#[test]
fn edt_13_selected_form_uses_winning_record_plugin_for_inspector_value() {
    let selected_form = BRIDGE
        .split("pub fn selected_form(")
        .nth(1)
        .unwrap()
        .split("fn search_form_ids(")
        .next()
        .unwrap();

    assert!(selected_form.contains("winner_ref(id)"));
    assert!(selected_form.contains("file(record_ref.file)"));
    assert!(selected_form.contains("winning_plugin: GString::from(winning_plugin.as_str())"));
    assert!(!selected_form.contains("active_plugin"));
}

#[test]
fn edt_13_godot_check_selects_resource_and_checks_displayed_winner_text() {
    assert!(FORMS_PLUGIN.contains("SOE_EDT13_INSPECTOR_TEST"));
    assert!(FORMS_PLUGIN.contains("_run_edt_13_inspector_test()"));
    assert!(FORMS_PLUGIN.contains("get_tree().quit(1)"));
    assert!(INSPECTOR_TEST.contains("EditorInterface.edit_resource(form)"));
    assert!(INSPECTOR_TEST.contains("await Engine.get_main_loop().process_frame"));
    assert!(INSPECTOR_TEST.contains("SoeWinningSourceValue"));
    assert!(INSPECTOR_TEST.contains("source.text != \"Base.esm\""));
}
