// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Record, Subrecord};
use soe_esm::{GlobalFormId, Plugin, Tag};

const FORM: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const QUST: Tag = Tag(*b"QUST");
const INDX: Tag = Tag(*b"INDX");
const QSDT: Tag = Tag(*b"QSDT");
const CNAM: Tag = Tag(*b"CNAM");
const QOBJ: Tag = Tag(*b"QOBJ");
const NNAM: Tag = Tag(*b"NNAM");
const QSTA: Tag = Tag(*b"QSTA");
const CTDA: Tag = Tag(*b"CTDA");

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-07-{}-{label}", std::process::id()));
    (
        root.with_extension("Base.esm"),
        root.with_extension("Patch.esp"),
    )
}

fn fixture(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let (base_path, patch_path) = paths(label);
    let mut base = Plugin::new(FileHeader::default());
    let mut quest = Record::new(QUST, 0x800)
        .zsub(Tag(*b"EDID"), "TestQuest")
        .sub(INDX, [10, 0, 2, 0])
        .sub(QSDT, [1])
        .zsub(CNAM, "Old journal text")
        .sub(INDX, [10, 0, 0, 0])
        .sub(QSDT, [0])
        .zsub(CNAM, "Second journal text")
        .sub(QOBJ, [10, 0])
        .sub(Tag(*b"FNAM"), [1, 0, 0, 0])
        .zsub(NNAM, "Old objective text")
        .sub(QSTA, [4, 0, 0, 0, 1, 0, 0, 0])
        .sub(CTDA, [0; 32])
        .sub(QOBJ, [10, 0])
        .sub(Tag(*b"FNAM"), [0, 0, 0, 0])
        .zsub(NNAM, "Second objective text")
        .sub(QSTA, [5, 0, 0, 0, 0, 0, 0, 0])
        .sub(CTDA, [1; 32]);
    quest
        .subrecords
        .push(Subrecord::new(Tag(*b"ANAM"), [6, 0, 0, 0]));
    base.push_top(quest);
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
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document =
        EditorDocument::open(&[base_path.clone(), patch_path.clone()], &active_name).unwrap();
    (document, base_path, patch_path)
}

fn cleanup(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn edt_07_stage_and_objective_override_saves_and_reloads() {
    let (mut document, base, patch) = fixture("roundtrip");
    document
        .edit_quest_stage_log_entry_text(FORM, 1, 0, "Revised journal text")
        .unwrap();
    document
        .edit_quest_objective_text(FORM, 1, "Revised objective text")
        .unwrap();

    let saved = document.save(&patch).unwrap();
    let reloaded = Plugin::parse(&fs::read(&patch).unwrap()).unwrap();
    let override_record = reloaded
        .records
        .iter()
        .find(|record| record.tag == QUST)
        .unwrap();
    assert_eq!(saved.to_bytes(), reloaded.to_bytes());
    assert_eq!(override_record.form_id, 0x0000_0800);
    assert_eq!(
        override_record.all(CNAM).collect::<Vec<_>>(),
        vec![b"Old journal text\0".as_slice(), b"Revised journal text\0"]
    );
    assert_eq!(
        override_record.all(NNAM).collect::<Vec<_>>(),
        vec![
            b"Old objective text\0".as_slice(),
            b"Revised objective text\0"
        ]
    );
    assert_eq!(
        override_record.all(QSTA).collect::<Vec<_>>(),
        vec![&[4, 0, 0, 0, 1, 0, 0, 0][..], &[5, 0, 0, 0, 0, 0, 0, 0]]
    );
    assert_eq!(
        override_record.all(CTDA).collect::<Vec<_>>(),
        vec![&[0; 32][..], &[1; 32]]
    );
    cleanup(&[base, patch]);
}

#[test]
fn edt_07_undo_restores_stage_and_objective_before_save() {
    let (mut document, base, patch) = fixture("undo");
    let original = document.active_plugin().clone();
    document
        .edit_quest_stage_log_entry_text(FORM, 0, 0, "Temporary journal text")
        .unwrap();
    document
        .edit_quest_objective_text(FORM, 0, "Temporary objective text")
        .unwrap();

    assert!(document.undo().unwrap());
    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin(), &original);
    let saved = document.save(&patch).unwrap();
    let reloaded = Plugin::parse(&fs::read(&patch).unwrap()).unwrap();
    assert!(saved.records.is_empty());
    assert!(reloaded.records.is_empty());
    assert_eq!(
        document.load_order().winner(FORM).unwrap().find(CNAM),
        Some(&b"Old journal text\0"[..])
    );
    assert_eq!(
        document.load_order().winner(FORM).unwrap().find(NNAM),
        Some(&b"Old objective text\0"[..])
    );
    cleanup(&[base, patch]);
}

#[test]
fn edt_07_duplicate_stage_and_objective_numbers_edit_selected_occurrences() {
    let (mut document, base, patch) = fixture("duplicates");
    document
        .edit_quest_stage_log_entry_text(FORM, 1, 0, "Second stage changed")
        .unwrap();
    document
        .edit_quest_objective_text(FORM, 1, "Second objective changed")
        .unwrap();

    let record = document.load_order().winner(FORM).unwrap();
    assert_eq!(
        record.all(CNAM).collect::<Vec<_>>(),
        vec![b"Old journal text\0".as_slice(), b"Second stage changed\0"]
    );
    assert_eq!(
        record.all(NNAM).collect::<Vec<_>>(),
        vec![
            b"Old objective text\0".as_slice(),
            b"Second objective changed\0"
        ]
    );
    cleanup(&[base, patch]);
}

#[test]
fn edt_07_localized_text_rejects_without_changing_the_document() {
    let (_initial_document, base, patch) = fixture("localized");
    let mut plugin = Plugin::parse(&fs::read(&base).unwrap()).unwrap();
    plugin.records[0].flags |= soe_esm::plugin::flags::LOCALIZED;
    fs::write(&base, plugin.to_bytes()).unwrap();
    let active_name = patch.file_name().unwrap().to_string_lossy();
    let mut document = EditorDocument::open(&[base.clone(), patch.clone()], &active_name).unwrap();
    let original = document.active_plugin().clone();

    let error = document
        .edit_quest_objective_text(FORM, 0, "Localized text")
        .unwrap_err();
    assert!(matches!(
        error,
        soe_editor::EditorError::LocalizedQuestText(_)
    ));
    assert_eq!(document.active_plugin(), &original);
    cleanup(&[base, patch]);
}
