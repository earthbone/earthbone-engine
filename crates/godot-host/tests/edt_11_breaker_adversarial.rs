// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

const STAT: Tag = Tag(*b"STAT");
const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};

fn open_document(base: Plugin, middle: Plugin, final_plugin: Plugin) -> EditorDocument {
    let directory = tempfile::tempdir().unwrap();
    let paths = ["Base.esm", "Middle.esp", "Final.esp"]
        .into_iter()
        .map(|name| directory.path().join(name))
        .collect::<Vec<_>>();
    for (path, plugin) in paths.iter().zip([base, middle, final_plugin]) {
        fs::write(path, plugin.to_bytes()).unwrap();
    }
    // Keep the fixture directory alive for the duration of EditorDocument.
    // EditorDocument owns the parsed load order, so the files can be removed.
    EditorDocument::open(&paths, "Final.esp").unwrap()
}

fn master_header(names: &[&str]) -> FileHeader {
    FileHeader {
        masters: names.iter().map(|name| (*name).to_owned()).collect(),
        ..Default::default()
    }
}

#[test]
fn edt_11_unknown_form_has_no_override_rows() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "Known"));
    let middle = Plugin::new(master_header(&["Base.esm"]));
    let final_plugin = Plugin::new(master_header(&["Base.esm", "Middle.esp"]));
    let document = open_document(base, middle, final_plugin);
    let unknown = GlobalFormId {
        file: 0,
        object: 0x801,
    };

    assert!(document.override_chain(unknown).is_empty());
}

#[test]
fn edt_11_repeated_chain_reads_keep_the_complete_load_order_history() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "Original"));
    let mut middle = Plugin::new(master_header(&["Base.esm"]));
    middle.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "PatchOne"));
    let mut final_plugin = Plugin::new(master_header(&["Base.esm", "Middle.esp"]));
    final_plugin.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "PatchTwo"));
    let document = open_document(base, middle, final_plugin);

    for _ in 0..3 {
        let chain = document.override_chain(FORM);
        assert_eq!(chain.len(), 3);
        assert_eq!(chain[0].plugin_name, "Base.esm");
        assert_eq!(chain[0].record.editor_id().as_deref(), Some("Original"));
        assert!(!chain[0].is_winner);
        assert_eq!(chain[1].plugin_name, "Middle.esp");
        assert_eq!(chain[1].record.editor_id().as_deref(), Some("PatchOne"));
        assert!(!chain[1].is_winner);
        assert_eq!(chain[2].plugin_name, "Final.esp");
        assert_eq!(chain[2].record.editor_id().as_deref(), Some("PatchTwo"));
        assert!(chain[2].is_winner);
    }
}

#[test]
fn edt_11_missing_editor_id_does_not_drop_a_record_version() {
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(STAT, FORM.object));
    let middle = Plugin::new(master_header(&["Base.esm"]));
    let mut final_plugin = Plugin::new(master_header(&["Base.esm", "Middle.esp"]));
    final_plugin.push_top(Record::new(STAT, FORM.object).zsub(Tag(*b"EDID"), "Named"));
    let document = open_document(base, middle, final_plugin);

    let chain = document.override_chain(FORM);
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].plugin_name, "Base.esm");
    assert_eq!(chain[0].record.tag, STAT);
    assert_eq!(chain[0].record.editor_id(), None);
    assert!(!chain[0].is_winner);
    assert_eq!(chain[1].plugin_name, "Final.esp");
    assert_eq!(chain[1].record.editor_id().as_deref(), Some("Named"));
    assert!(chain[1].is_winner);
}
