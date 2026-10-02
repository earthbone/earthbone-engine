// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

fn fixture() -> (EditorDocument, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let base_path = directory.path().join("Base.esm");
    let patch_path = directory.path().join("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "OldOak"));
    base.push_top(Record::new(Tag(*b"STAT"), 0x801).zsub(Tag(*b"EDID"), "Pine"));
    base.push_top(Record::new(Tag(*b"MISC"), 0x802).zsub(Tag(*b"EDID"), "OakToken"));

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
fn edt_02_query_returns_only_current_winners_for_signature_and_ascii_casefold() {
    let (document, _directory) = fixture();
    let ids = document
        .search_forms(Some(Tag(*b"STAT")), "wInNiNg")
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![GlobalFormId {
            file: 0,
            object: 0x800,
        }]
    );
    assert!(document
        .search_forms(Some(Tag(*b"STAT")), "OldOak")
        .next()
        .is_none());
    assert!(document
        .search_forms(Some(Tag(*b"MISC")), "Winning")
        .next()
        .is_none());
}

#[test]
fn edt_02_bridge_rejects_invalid_signatures_and_bounds_untrusted_page_arguments() {
    let bridge = include_str!("../src/godot_forms.rs");
    let search = bridge
        .split("pub fn search(")
        .nth(1)
        .unwrap()
        .split("/// Build an inspector Resource")
        .next()
        .unwrap();

    assert!(search.contains("signature.len() != 4"));
    assert!(search.contains("!signature.as_bytes().iter().all(u8::is_ascii)"));
    assert!(search.contains("return results;"));
    assert!(search.contains("offset.max(0)"));
    assert!(search.contains("limit.clamp(0, 200)"));
}

#[test]
fn edt_02_search_path_never_builds_resources_and_missing_selection_exits_before_counting() {
    let bridge = include_str!("../src/godot_forms.rs");
    let dock = include_str!("../../../godot/addons/soe_forms/forms_plugin.gd");
    let search = bridge
        .split("pub fn search(")
        .nth(1)
        .unwrap()
        .split("/// Build an inspector Resource")
        .next()
        .unwrap();
    let selected = bridge
        .split("pub fn selected_form(")
        .nth(1)
        .unwrap()
        .split("fn search_form_ids(")
        .next()
        .unwrap();
    let page = dock
        .split("func _show_page()")
        .nth(1)
        .unwrap()
        .split("func _previous_page()")
        .next()
        .unwrap();
    let selection = dock.split("func _select_form()").nth(1).unwrap();

    assert!(!search.contains("from_init_fn"));
    assert!(page.contains("form_table.search("));
    assert!(!page.contains("selected_form("));
    assert!(selection.contains("form_table.selected_form("));
    assert!(selected.find("winner_ref(id)?").unwrap() < selected.find("from_init_fn").unwrap());
    assert!(selected.find("from_init_fn").unwrap() < selected.find("resources_created += 1").unwrap());
}
