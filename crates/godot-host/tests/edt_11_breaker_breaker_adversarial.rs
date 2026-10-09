// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

fn write_plugin(directory: &tempfile::TempDir, name: &str, plugin: Plugin) -> std::path::PathBuf {
    let path = directory.path().join(name);
    fs::write(&path, plugin.to_bytes()).unwrap();
    path
}

fn master_header(names: &[&str]) -> FileHeader {
    FileHeader {
        masters: names.iter().map(|name| (*name).to_owned()).collect(),
        ..Default::default()
    }
}

#[test]
fn edt_11_same_editor_id_on_different_forms_does_not_join_their_chains() {
    let directory = tempfile::tempdir().unwrap();
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "SharedName"));
    base.push_top(Record::new(Tag(*b"STAT"), 0x801).zsub(Tag(*b"EDID"), "SharedName"));
    let base_path = write_plugin(&directory, "Base.esm", base);
    let mut patch = Plugin::new(master_header(&["Base.esm"]));
    patch.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "SharedName"));
    let patch_path = write_plugin(&directory, "Patch.esp", patch);
    let document = EditorDocument::open(&[base_path, patch_path], "Patch.esp").unwrap();

    let changed = document.override_chain(GlobalFormId { file: 0, object: 0x800 });
    let untouched = document.override_chain(GlobalFormId { file: 0, object: 0x801 });
    assert_eq!(changed.len(), 2);
    assert_eq!(changed.iter().map(|version| version.is_winner).collect::<Vec<_>>(), [false, true]);
    assert_eq!(untouched.len(), 1);
    assert!(untouched[0].is_winner);
}

#[test]
fn edt_11_same_form_id_keeps_versions_when_editor_id_changes() {
    let directory = tempfile::tempdir().unwrap();
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "OldName"));
    let base_path = write_plugin(&directory, "Base.esm", base);
    let mut patch = Plugin::new(master_header(&["Base.esm"]));
    patch.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "NewName"));
    let patch_path = write_plugin(&directory, "Patch.esp", patch);
    let document = EditorDocument::open(&[base_path, patch_path], "Patch.esp").unwrap();

    let chain = document.override_chain(GlobalFormId { file: 0, object: 0x800 });
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].record.editor_id().as_deref(), Some("OldName"));
    assert_eq!(chain[1].record.editor_id().as_deref(), Some("NewName"));
    assert!(!chain[0].is_winner);
    assert!(chain[1].is_winner);
}
