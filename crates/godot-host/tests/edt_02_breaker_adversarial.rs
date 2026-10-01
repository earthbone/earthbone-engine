// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

const STAT: Tag = Tag(*b"STAT");
const MISC: Tag = Tag(*b"MISC");

fn fixture() -> (EditorDocument, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let base_path = directory.path().join("Base.esm");
    let patch_path = directory.path().join("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    for index in 0..205 {
        let editor_id = format!("Stone{index:03}");
        base.push_top(Record::new(STAT, 0x800 + index).zsub(Tag(*b"EDID"), &editor_id));
    }
    base.push_top(Record::new(STAT, 0x900));
    base.push_top(Record::new(MISC, 0x901).zsub(Tag(*b"EDID"), "StoneMisc"));

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string()],
        ..Default::default()
    });
    patch.push_top(Record::new(STAT, 0x800).zsub(Tag(*b"EDID"), "ModdedStone000"));
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let document = EditorDocument::open(&[base_path, patch_path], "Patch.esp").unwrap();
    (document, directory)
}

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

#[test]
fn edt_02_search_filters_by_signature_and_the_winning_editor_id() {
    let (document, _directory) = fixture();
    let matches = document
        .search_forms(Some(STAT), "mOdDeDsToNe")
        .collect::<Vec<_>>();

    assert_eq!(matches, vec![id(0x800)]);
    assert_eq!(
        document
            .load_order()
            .winner(id(0x800))
            .unwrap()
            .editor_id()
            .as_deref(),
        Some("ModdedStone000")
    );
    assert!(document
        .search_forms(Some(MISC), "ModdedStone")
        .next()
        .is_none());
}

#[test]
fn edt_02_missing_editor_ids_match_only_an_empty_query() {
    let (document, _directory) = fixture();

    assert_eq!(document.search_forms(Some(STAT), "").count(), 206);
    assert!(document
        .search_forms(Some(STAT), "stone")
        .all(|form| form != id(0x900)));
    assert!(document
        .search_forms(None, "stone")
        .all(|form| form != id(0x900)));
}

#[test]
fn edt_02_search_pages_are_stable_unique_and_cover_all_winners() {
    let (document, _directory) = fixture();
    let forms = document
        .search_forms(Some(STAT), "stone")
        .collect::<Vec<_>>();
    let first_page = forms.iter().take(200).copied().collect::<Vec<_>>();
    let second_page = forms
        .iter()
        .skip(200)
        .take(200)
        .copied()
        .collect::<Vec<_>>();

    assert_eq!(first_page.len(), 200);
    assert_eq!(second_page.len(), 5);
    assert!(first_page.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(second_page.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(first_page.last(), Some(&id(0x8c7)));
    assert_eq!(second_page.first(), Some(&id(0x8c8)));
    assert!(first_page.iter().all(|form| !second_page.contains(form)));
    assert_eq!(forms.len(), 205);
}

#[test]
fn edt_02_dock_search_builds_ids_and_selection_builds_the_resource() {
    let dock = include_str!("../../../godot/addons/soe_forms/forms_plugin.gd");
    let bridge = include_str!("../src/godot_forms.rs");
    let show_page = dock
        .split("func _show_page()")
        .nth(1)
        .unwrap()
        .split("func _previous_page()")
        .next()
        .unwrap();
    let select_form = dock.split("func _select_form()").nth(1).unwrap();
    let search_bridge = bridge
        .split("pub fn search(")
        .nth(1)
        .unwrap()
        .split("/// Build an inspector Resource")
        .next()
        .unwrap();
    let selected_bridge = bridge
        .split("pub fn selected_form(")
        .nth(1)
        .unwrap()
        .split("fn search_form_ids(")
        .next()
        .unwrap();

    assert!(show_page.contains("form_table.search("));
    assert!(!show_page.contains("selected_form("));
    assert!(select_form.contains("form_table.selected_form("));
    assert!(search_bridge.contains("search_form_ids("));
    assert!(!search_bridge.contains("from_init_fn"));
    assert!(selected_bridge.contains("from_init_fn"));
    assert!(selected_bridge.contains("resources_created += 1"));
}
