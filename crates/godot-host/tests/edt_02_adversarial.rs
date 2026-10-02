// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

fn form_id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn loaded_forms() -> (EditorDocument, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let base_path = directory.path().join("Base.esm");
    let patch_path = directory.path().join("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "FirTree"));
    base.push_top(Record::new(Tag(*b"STAT"), 0x801));
    base.push_top(Record::new(Tag(*b"MISC"), 0x802).zsub(Tag(*b"EDID"), "FirToken"));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_owned()],
        ..Default::default()
    });
    patch.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "WinningOak"));
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    (
        EditorDocument::open(&[base_path, patch_path], "Patch.esp").unwrap(),
        directory,
    )
}

#[test]
fn edt_02_signature_and_query_filters_only_the_winning_loaded_forms() {
    let (document, _directory) = loaded_forms();

    assert_eq!(
        document
            .search_forms(Some(Tag(*b"STAT")), "oAk")
            .collect::<Vec<_>>(),
        vec![form_id(0x800)]
    );
    assert_eq!(
        document
            .load_order()
            .winner(form_id(0x800))
            .unwrap()
            .editor_id()
            .as_deref(),
        Some("WinningOak")
    );
    assert!(document
        .search_forms(Some(Tag(*b"MISC")), "oAk")
        .next()
        .is_none());
}

#[test]
fn edt_02_empty_query_includes_missing_editor_ids_but_text_query_excludes_them() {
    let (document, _directory) = loaded_forms();

    assert_eq!(
        document
            .search_forms(Some(Tag(*b"STAT")), "")
            .collect::<Vec<_>>(),
        vec![form_id(0x800), form_id(0x801)]
    );
    assert!(document
        .search_forms(Some(Tag(*b"STAT")), "fir")
        .all(|id| id != form_id(0x801)));
}

#[test]
fn edt_02_search_and_tree_population_do_not_construct_inspector_resources() {
    let bridge = include_str!("../src/godot_forms.rs");
    let dock = include_str!("../../../godot/addons/soe_forms/forms_plugin.gd");
    let search = bridge
        .split("pub fn search(")
        .nth(1)
        .unwrap()
        .split("/// Build an inspector Resource")
        .next()
        .unwrap();
    let show_page = dock
        .split("func _show_page()")
        .nth(1)
        .unwrap()
        .split("func _previous_page()")
        .next()
        .unwrap();
    let selection = dock.split("func _select_form()").nth(1).unwrap();

    assert!(search.contains("search_form_ids("));
    assert!(!search.contains("from_init_fn"));
    assert!(show_page.contains("form_table.search("));
    assert!(!show_page.contains("selected_form("));
    assert!(selection.contains("form_table.selected_form("));
}

#[test]
fn edt_02_invalid_signature_and_missing_selection_do_not_create_resources() {
    let bridge = include_str!("../src/godot_forms.rs");
    let search = bridge
        .split("pub fn search(")
        .nth(1)
        .unwrap()
        .split("/// Build an inspector Resource")
        .next()
        .unwrap();
    let selection = bridge
        .split("pub fn selected_form(")
        .nth(1)
        .unwrap()
        .split("fn search_form_ids(")
        .next()
        .unwrap();

    assert!(search.contains("signature.len() != 4"));
    assert!(search.contains("return results;"));
    assert!(selection.contains("let record_ref = document.load_order().winner_ref(id)?;"));
    assert!(selection.contains("resources_created += 1;"));
}
