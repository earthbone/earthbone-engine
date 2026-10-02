// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn document_with_unlabelled_override() -> (EditorDocument, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let base_path = directory.path().join("Base.esm");
    let patch_path = directory.path().join("Patch.esp");

    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), FORM.object).zsub(Tag(*b"EDID"), "Original"));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string()],
        ..Default::default()
    });
    patch.push_top(Record::new(Tag(*b"STAT"), FORM.object));

    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let document = EditorDocument::open(&[base_path, patch_path], "Patch.esp").unwrap();
    (document, directory)
}

#[test]
fn edt_11_bridge_preserves_unlabelled_loser_and_winner_rows() {
    let (document, _directory) = document_with_unlabelled_override();
    let chain = document.override_chain(FORM);

    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].plugin_name, "Base.esm");
    assert_eq!(chain[0].record.editor_id().as_deref(), Some("Original"));
    assert!(!chain[0].is_winner);
    assert_eq!(chain[1].plugin_name, "Patch.esp");
    assert_eq!(chain[1].record.editor_id(), None);
    assert!(chain[1].is_winner);

    let bridge = include_str!("../src/godot_forms.rs");
    let entries = bridge.split("fn override_chain_entries(").nth(1).unwrap();
    assert!(entries.contains(".map(|version| OverrideChainEntry"));
    assert!(entries.contains("editor_id: version.record.editor_id().unwrap_or_default()"));
}

#[test]
fn edt_11_selecting_a_result_populates_the_chain_for_that_form_id() {
    let dock = include_str!("../../../godot/addons/soe_forms/forms_plugin.gd");
    let select = dock.split("func _select_form() -> void:").nth(1).unwrap();

    assert!(select.contains("var item := results.get_selected()"));
    assert!(select.contains(
        "override_chain.show_chain(form_table.override_chain(item.get_metadata(0)))"
    ));
}

#[test]
fn edt_11_search_refresh_clears_stale_chain_before_another_selection() {
    let dock = include_str!("../../../godot/addons/soe_forms/forms_plugin.gd");
    let page = dock.split("func _show_page() -> void:").nth(1).unwrap();

    assert!(page.starts_with("\n\tresults.clear()\n\toverride_chain.clear()"));
}

#[test]
fn edt_11_chain_tree_renders_all_versions_in_input_order_with_source_and_winner() {
    let view = include_str!("../../../godot/addons/soe_forms/override_chain_view.gd");

    assert!(view.contains("for version in versions:"));
    assert!(view.contains("row.set_text(0, version.get(\"plugin_name\", \"\"))"));
    assert!(view.contains("row.set_text(1, version.get(\"signature\", \"\"))"));
    assert!(view.contains("row.set_text(2, version.get(\"editor_id\", \"\"))"));
    assert!(view.contains(
        "row.set_text(3, \"Yes\" if version.get(\"is_winner\", false) else \"No\")"
    ));
}
