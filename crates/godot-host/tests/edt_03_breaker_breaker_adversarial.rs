// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::panic)]

const LAYER: &str = include_str!("../src/godot_layer.rs");
const SCENE_CHECK: &str = include_str!("../../../godot/edt_03_cell_scene_test.gd");

#[test]
fn edt_03_cell_selection_validates_before_replacing_the_scene() {
    let open_cell = LAYER
        .split("pub fn open_cell_id(")
        .nth(1)
        .unwrap()
        .split("fn draw_editor_cell(")
        .next()
        .unwrap();
    let invalid_cell = open_cell
        .split("let Some(record) = order.winner(cell_id) else {")
        .next()
        .unwrap();

    assert!(invalid_cell.contains("return -1;"));
    assert!(invalid_cell.contains("cell_id"));
    assert!(!invalid_cell.contains("self.clear()"));
    assert!(open_cell.contains("if record.tag != soe_esm::tag::CELL"));
    assert!(open_cell
        .contains("self.draw_editor_cell(cell_id, &loaded, &cell_form_id.to_string(), true)"));
}

#[test]
fn edt_03_scene_check_opens_references_and_replaces_the_prior_cell() {
    assert!(SCENE_CHECK.contains("streamer.open_cell_id(selected.id)"));
    assert!(SCENE_CHECK.contains("streamer.last_stats().references > 0"));
    assert!(SCENE_CHECK.contains("streamer.instance_count() > 0"));
    assert!(SCENE_CHECK.contains("streamer.open_cell_id(replacement.id)"));
    assert!(SCENE_CHECK.contains("not replacement_scene.has(selected.id)"));
}
