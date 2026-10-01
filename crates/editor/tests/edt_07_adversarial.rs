#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

#[test]
fn edt_07_repeated_same_field_edits_undo_to_each_prior_state() {
    let root = std::env::temp_dir().join(format!("soe-editor-edt-07-adv-{}", std::process::id()));
    let base = root.with_extension("Base.esm");
    let patch = root.with_extension("Patch.esp");
    let mut source = Plugin::new(FileHeader::default());
    source.push_top(
        Record::new(Tag(*b"QUST"), 0x800)
            .sub(Tag(*b"QOBJ"), [1, 0])
            .sub(Tag(*b"FNAM"), [0, 0, 0, 0])
            .zsub(Tag(*b"NNAM"), "Original"),
    );
    let override_plugin = Plugin::new(FileHeader {
        masters: vec![base.file_name().unwrap().to_string_lossy().into_owned()],
        ..Default::default()
    });
    fs::write(&base, source.to_bytes()).unwrap();
    fs::write(&patch, override_plugin.to_bytes()).unwrap();
    let active = patch.file_name().unwrap().to_string_lossy();
    let mut document =
        EditorDocument::open(&[PathBuf::from(&base), PathBuf::from(&patch)], &active).unwrap();
    let form = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    let original = document.active_plugin().clone();

    document
        .edit_quest_objective_text(form, 0, "First edit")
        .unwrap();
    document
        .edit_quest_objective_text(form, 0, "Second edit")
        .unwrap();
    assert!(document.undo().unwrap());
    assert_eq!(
        document
            .load_order()
            .winner(form)
            .unwrap()
            .find(Tag(*b"NNAM")),
        Some(&b"First edit\0"[..])
    );
    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin(), &original);
    document.save(&patch).unwrap();
    assert!(Plugin::parse(&fs::read(&patch).unwrap())
        .unwrap()
        .records
        .is_empty());
    let _ = fs::remove_file(base);
    let _ = fs::remove_file(patch);
}
