// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

#[test]
fn edt_02_repeated_queries_keep_winning_ids_and_page_boundaries_stable() {
    let directory = tempfile::tempdir().unwrap();
    let base_path = directory.path().join("Base.esm");
    let patch_path = directory.path().join("Patch.esp");
    let mut base = Plugin::new(FileHeader::default());
    for index in 0..405 {
        let editor_id = format!("Form{index:03}");
        base.push_top(Record::new(Tag(*b"STAT"), 0x800 + index).zsub(Tag(*b"EDID"), &editor_id));
    }
    let patch = Plugin::new(FileHeader {
        masters: vec!["Base.esm".to_string()],
        ..Default::default()
    });
    fs::write(&base_path, base.to_bytes()).unwrap();
    fs::write(&patch_path, patch.to_bytes()).unwrap();
    let document = EditorDocument::open(&[base_path, patch_path], "Patch.esp").unwrap();

    let first = document
        .search_forms(Some(Tag(*b"STAT")), "FORM")
        .collect::<Vec<_>>();
    let repeat = document
        .search_forms(Some(Tag(*b"STAT")), "form")
        .collect::<Vec<_>>();
    let first_page = first.iter().take(200).copied().collect::<Vec<_>>();
    let second_page = first
        .iter()
        .skip(200)
        .take(200)
        .copied()
        .collect::<Vec<_>>();
    let third_page = first
        .iter()
        .skip(400)
        .take(200)
        .copied()
        .collect::<Vec<_>>();

    assert_eq!(first, repeat);
    assert_eq!(first.len(), 405);
    assert_eq!(first_page.first().unwrap().object, 0x800);
    assert_eq!(second_page.first().unwrap().object, 0x8c8);
    assert_eq!(third_page.len(), 5);
    assert_eq!(third_page.last().unwrap().object, 0x994);
    assert!(first_page
        .iter()
        .chain(&second_page)
        .chain(&third_page)
        .all(|id| id.file == 0));
}

#[test]
fn edt_02_unmatched_or_invalid_signatures_do_not_leak_other_record_types() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("Only.esp");
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800).zsub(Tag(*b"EDID"), "Stone"));
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x801).zsub(Tag(*b"EDID"), "StoneMisc"));
    fs::write(&path, plugin.to_bytes()).unwrap();
    let document = EditorDocument::open(&[path], "Only.esp").unwrap();

    assert_eq!(
        document.search_forms(Some(Tag(*b"STAT")), "stone").count(),
        1
    );
    assert_eq!(
        document.search_forms(Some(Tag(*b"MISC")), "stone").count(),
        1
    );
    assert!(document
        .search_forms(Some(Tag(*b"NAVM")), "stone")
        .next()
        .is_none());
    assert_eq!(document.search_forms(None, "stone").count(), 2);
}

#[test]
fn edt_02_empty_query_includes_missing_editor_ids_but_text_query_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("MissingId.esp");
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x800));
    fs::write(&path, plugin.to_bytes()).unwrap();
    let document = EditorDocument::open(&[path], "MissingId.esp").unwrap();
    let id = GlobalFormId {
        file: 0,
        object: 0x800,
    };

    assert_eq!(
        document
            .search_forms(Some(Tag(*b"STAT")), "")
            .collect::<Vec<_>>(),
        vec![id]
    );
    assert!(document
        .search_forms(Some(Tag(*b"STAT")), "missing")
        .next()
        .is_none());
}
