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
const ALID: Tag = Tag(*b"ALID");
const FNAM: Tag = Tag(*b"FNAM");
const ALUA: Tag = Tag(*b"ALUA");
const ALED: Tag = Tag(*b"ALED");
const CTDA: Tag = Tag(*b"CTDA");

fn fixture(label: &str) -> (EditorDocument, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "soe-editor-edt-18-adversarial-{}-{label}",
        std::process::id()
    ));
    let base_path = root.with_extension("Base.esm");
    let patch_path = root.with_extension("Patch.esp");
    let quest = Record::new(QUST, 0x800)
        .sub(ALST, [1, 0, 0, 0])
        .zsub(ALID, "First")
        .sub(FNAM, [0, 0, 0, 0])
        .sub(ALUA, [0x10, 0, 0, 0])
        .sub(ALED, [])
        .sub(ALST, [2, 0, 0, 0])
        .zsub(ALID, "Second")
        .sub(FNAM, [0, 0, 0, 0])
        .sub(ALUA, [0x20, 0, 0, 0])
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

#[test]
fn edt_18_new_conditions_stay_inside_the_selected_alias() {
    let (mut document, base, patch) = fixture("alias-boundary");
    document
        .set_quest_alias_conditions(FORM, 0, &[Subrecord::new(CTDA, [0; 32])])
        .unwrap();
    document.save(&patch).unwrap();

    let saved = Plugin::parse(&fs::read(&patch).unwrap()).unwrap();
    let quest = saved
        .records
        .iter()
        .find(|record| record.tag == QUST)
        .unwrap();
    let aliases = quest
        .subrecords
        .iter()
        .enumerate()
        .filter_map(|(index, subrecord)| (subrecord.tag == ALST).then_some(index))
        .collect::<Vec<_>>();
    assert_eq!(aliases.len(), 2);
    let first_end = quest
        .subrecords
        .iter()
        .position(|subrecord| subrecord.tag == ALED)
        .unwrap();
    let second_end = quest
        .subrecords
        .iter()
        .rposition(|subrecord| subrecord.tag == ALED)
        .unwrap();
    assert!(quest.subrecords[aliases[0]..=first_end]
        .iter()
        .any(|subrecord| subrecord.tag == CTDA));
    assert!(!quest.subrecords[aliases[1]..=second_end]
        .iter()
        .any(|subrecord| subrecord.tag == CTDA));

    for path in [base, patch] {
        let _ = fs::remove_file(path);
    }
}
