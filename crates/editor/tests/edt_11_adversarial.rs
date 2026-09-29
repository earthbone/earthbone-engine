#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

#[test]
fn edt_11_winning_deleted_override_remains_the_final_chain_version() {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-11-deleted-{}", std::process::id()));
    let base = root.with_extension("Base.esm");
    let patch = root.with_extension("Patch.esp");
    let mut source = Plugin::new(FileHeader::default());
    source.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "Live"));
    let mut deleted = Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "Deleted");
    deleted.flags |= soe_esm::plugin::flags::DELETED;
    let mut override_plugin = Plugin::new(FileHeader {
        masters: vec![base.file_name().unwrap().to_string_lossy().into_owned()],
        ..Default::default()
    });
    override_plugin.push_top(deleted);
    fs::write(&base, source.to_bytes()).unwrap();
    fs::write(&patch, override_plugin.to_bytes()).unwrap();
    let active = patch.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(&[base.clone(), patch.clone()], &active).unwrap();

    let chain = document.override_chain(GlobalFormId {
        file: 0,
        object: 0x800,
    });
    assert_eq!(chain.len(), 2);
    assert!(!chain[0].is_winner);
    assert!(chain[1].is_winner);
    assert_eq!(chain[1].record.editor_id().as_deref(), Some("Deleted"));
    let _ = fs::remove_file(base);
    let _ = fs::remove_file(patch);
}
