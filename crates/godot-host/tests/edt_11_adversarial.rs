// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{flags, FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

const STAT: Tag = Tag(*b"STAT");
const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn open_document(
    base: Plugin,
    middle: Plugin,
    final_plugin: Plugin,
) -> (EditorDocument, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let paths = ["Base.esm", "Middle.esp", "Final.esp"]
        .into_iter()
        .map(|name| directory.path().join(name))
        .collect::<Vec<_>>();
    for (path, plugin) in paths.iter().zip([base, middle, final_plugin]) {
        fs::write(path, plugin.to_bytes()).unwrap();
    }
    let document = EditorDocument::open(&paths, "Final.esp").unwrap();
    (document, directory)
}

fn masters(names: &[&str]) -> FileHeader {
    FileHeader {
        masters: names.iter().map(|name| (*name).to_string()).collect(),
        ..Default::default()
    }
}

#[test]
fn edt_11_chain_skips_plugins_without_a_record_and_keeps_load_order() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "Base"));
    let middle = Plugin::new(masters(&["Base.esm"]));
    let mut final_plugin = Plugin::new(masters(&["Base.esm", "Middle.esp"]));
    final_plugin.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "Final"));
    let (document, _directory) = open_document(base, middle, final_plugin);

    let chain = document.override_chain(FORM);
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].plugin_name, "Base.esm");
    assert_eq!(chain[0].record.editor_id().as_deref(), Some("Base"));
    assert!(!chain[0].is_winner);
    assert_eq!(chain[1].plugin_name, "Final.esp");
    assert_eq!(chain[1].record.editor_id().as_deref(), Some("Final"));
    assert!(chain[1].is_winner);
}

#[test]
fn edt_11_deleted_record_is_still_identified_as_the_winner() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "Live"));
    let middle = Plugin::new(masters(&["Base.esm"]));
    let mut deleted = Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "Deleted");
    deleted.flags |= flags::DELETED;
    let mut final_plugin = Plugin::new(masters(&["Base.esm", "Middle.esp"]));
    final_plugin.push_top(deleted);
    let (document, _directory) = open_document(base, middle, final_plugin);

    let chain = document.override_chain(FORM);
    assert_eq!(chain.len(), 2);
    assert!(!chain[0].is_winner);
    assert!(chain[1].is_winner);
    assert!(chain[1].record.is_deleted());
    assert_eq!(chain[1].record.editor_id().as_deref(), Some("Deleted"));
}

#[test]
fn edt_11_unoverridden_record_is_the_only_winning_version() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "BaseOnly"));
    let middle = Plugin::new(masters(&["Base.esm"]));
    let final_plugin = Plugin::new(masters(&["Base.esm", "Middle.esp"]));
    let (document, _directory) = open_document(base, middle, final_plugin);

    let chain = document.override_chain(FORM);
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].plugin_name, "Base.esm");
    assert!(chain[0].is_winner);
}

#[test]
fn edt_11_godot_bridge_and_dock_keep_all_chain_columns() {
    let bridge = include_str!("../src/godot_forms.rs");
    let dock = include_str!("../../../godot/addons/soe_forms/forms_plugin.gd");
    let view = include_str!("../../../godot/addons/soe_forms/override_chain_view.gd");

    let bridge_method = bridge
        .split("pub fn override_chain(")
        .nth(1)
        .unwrap()
        .split("/// Count selected-form Resources")
        .next()
        .unwrap();
    for field in ["plugin_name", "signature", "editor_id", "is_winner"] {
        assert!(bridge_method.contains(&format!("item.set(\"{field}\"")));
        assert!(view.contains(&format!("version.get(\"{field}\"")));
    }
    assert!(dock.contains("override_chain.show_chain(form_table.override_chain("));
    assert!(view.contains("set_text(3, \"Yes\" if version.get(\"is_winner\", false) else \"No\")"));
}
