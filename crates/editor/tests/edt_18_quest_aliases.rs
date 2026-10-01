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
const ALST: Tag = Tag(*b"ALST");
const ALFI: Tag = Tag(*b"ALFI");
const ALID: Tag = Tag(*b"ALID");
const FNAM: Tag = Tag(*b"FNAM");
const ALED: Tag = Tag(*b"ALED");
const CTDA: Tag = Tag(*b"CTDA");
const CIS1: Tag = Tag(*b"CIS1");
const CIS2: Tag = Tag(*b"CIS2");

fn paths(label: &str) -> (PathBuf, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-18-{}-{label}", std::process::id()));
    (
        root.with_extension("Base.esm"),
        root.with_extension("Patch.esp"),
    )
}

fn fixture(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let (base_path, patch_path) = paths(label);
    let quest = Record::new(QUST, 0x800)
        .zsub(Tag(*b"EDID"), "AliasQuest")
        .sub(Tag(*b"ANAM"), [2, 0, 0, 0])
        .sub(ALST, [1, 0, 0, 0])
        .zsub(ALID, "Guard")
        .sub(FNAM, [2, 0, 0, 0])
        .sub(ALFI, [3, 0, 0, 0])
        .sub(Tag(*b"ALUA"), [0x34, 0x12, 0, 0])
        .sub(CTDA, [0; 32])
        .zsub(CIS1, "GetIsID")
        .sub(CTDA, [1; 32])
        .zsub(CIS2, "ActorBase")
        .sub(Tag(*b"KSIZ"), [0, 0, 0, 0])
        .sub(ALED, []);
    let mut base = Plugin::new(FileHeader::default());
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
fn edt_18_alias_fill_rule_and_conditions_round_trip() {
    let (mut document, base, patch) = fixture("roundtrip");
    document
        .set_quest_alias_fill_rule(FORM, 0, Some(7))
        .unwrap();
    let conditions = [
        Subrecord::new(CTDA, [0x22; 32]),
        Subrecord::new(CIS1, b"GetIsID\0".to_vec()),
    ];
    document
        .set_quest_alias_conditions(FORM, 0, &conditions)
        .unwrap();

    let saved = document.save(&patch).unwrap();
    let reloaded = Plugin::parse(&fs::read(&patch).unwrap()).unwrap();
    let edited = reloaded
        .records
        .iter()
        .find(|record| record.tag == QUST)
        .unwrap();
    assert_eq!(saved.to_bytes(), reloaded.to_bytes());
    assert_eq!(edited.find(ALFI), Some(&7_i32.to_le_bytes()[..]));
    let tags = edited
        .subrecords
        .iter()
        .map(|subrecord| subrecord.tag)
        .collect::<Vec<_>>();
    let flags_index = tags.iter().position(|tag| *tag == FNAM).unwrap();
    let fill_rule_index = tags.iter().position(|tag| *tag == ALFI).unwrap();
    let fill_type_index = tags.iter().position(|tag| *tag == Tag(*b"ALUA")).unwrap();
    assert!(flags_index < fill_rule_index && fill_rule_index < fill_type_index);
    let alias_start = edited
        .subrecords
        .iter()
        .position(|subrecord| subrecord.tag == ALST)
        .unwrap();
    let alias_end = edited
        .subrecords
        .iter()
        .position(|subrecord| subrecord.tag == ALED)
        .unwrap();
    let alias_subrecords = &edited.subrecords[alias_start..=alias_end];
    assert_eq!(
        alias_subrecords
            .iter()
            .filter(|subrecord| matches!(subrecord.tag, CTDA | CIS1 | CIS2))
            .cloned()
            .collect::<Vec<_>>(),
        conditions
    );
    assert!(alias_subrecords
        .iter()
        .any(|subrecord| subrecord.tag == ALID));
    assert!(alias_subrecords
        .iter()
        .any(|subrecord| subrecord.tag == FNAM));
    assert!(alias_subrecords
        .iter()
        .any(|subrecord| subrecord.tag == ALED));
    cleanup(&[base, patch]);
}

#[test]
fn edt_18_alias_fill_rule_and_conditions_undo_together() {
    let (mut document, base, patch) = fixture("undo");
    let original = document.active_plugin().clone();
    document.set_quest_alias_fill_rule(FORM, 0, None).unwrap();
    document
        .set_quest_alias_conditions(FORM, 0, &[Subrecord::new(CTDA, [9; 32])])
        .unwrap();
    assert!(document.undo().unwrap());
    assert!(document.undo().unwrap());
    assert_eq!(document.active_plugin(), &original);
    cleanup(&[base, patch]);
}

#[test]
fn edt_18_alias_conditions_reject_orphan_string_without_editing() {
    let (mut document, base, patch) = fixture("invalid");
    let original = document.active_plugin().clone();
    let error = document
        .set_quest_alias_conditions(FORM, 0, &[Subrecord::new(CIS1, b"orphan\0".to_vec())])
        .unwrap_err();
    assert!(matches!(
        error,
        soe_editor::EditorError::InvalidQuestConditions { .. }
    ));
    assert_eq!(document.active_plugin(), &original);
    cleanup(&[base, patch]);
}
