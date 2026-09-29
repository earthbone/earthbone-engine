// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

#[test]
fn edt_02_empty_and_missing_editor_ids_do_not_allocate_resources() {
    let root = std::env::temp_dir().join(format!("soe-edt-02-adversarial-{}", std::process::id()));
    let base_path = root.with_extension("Base.esm");
    let patch_path = root.with_extension("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"STAT"), 0x800));
    base.push_top(Record::new(Tag(*b"MISC"), 0x801).zsub(Tag(*b"EDID"), "StoneToken"));
    let patch = Plugin::new(FileHeader {
        masters: vec![base_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()],
        ..Default::default()
    });
    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&patch_path);
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let active = patch_path.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active).unwrap();

    let ids = document.search_forms(None, "").collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            GlobalFormId {
                file: 0,
                object: 0x800
            },
            GlobalFormId {
                file: 0,
                object: 0x801
            }
        ]
    );
    assert!(document
        .search_forms(Some(Tag(*b"STAT")), "missing")
        .next()
        .is_none());
    assert_eq!(document.load_order().files().len(), 2);

    let _ = fs::remove_file(base_path);
    let _ = fs::remove_file(patch_path);
}
