// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::EditorDocument;
use soe_esm::plugin::{Entry, FileHeader, Group, GroupKind, Record};
use soe_esm::{GlobalFormId, Plugin, Tag};

const TOPIC: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const INFO: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x810,
};
const QUEST: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x900,
};
const BRANCH: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x901,
};

struct Fixture {
    document: EditorDocument,
    base: PathBuf,
    patch: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.base);
        let _ = fs::remove_file(&self.patch);
    }
}

fn fixture(label: &str) -> Fixture {
    let stem =
        std::env::temp_dir().join(format!("soe-editor-edt-19-{}-{label}", std::process::id()));
    let base = stem.with_extension("Base.esm");
    let patch = stem.with_extension("Patch.esp");

    let mut source = Plugin::new(FileHeader::default());
    source.push_top(Record::new(Tag(*b"QUST"), 0x900).zsub(Tag(*b"EDID"), "DialogueQuest"));
    source.push_top(Record::new(Tag(*b"DLBR"), 0x901).zsub(Tag(*b"EDID"), "DialogueBranch"));
    source.push_top(
        Record::new(Tag(*b"DIAL"), 0x800)
            .zsub(Tag(*b"EDID"), "DialogueTopic")
            .sub(Tag(*b"PNAM"), 50.0_f32.to_le_bytes())
            .sub(Tag(*b"BNAM"), 0x901_u32.to_le_bytes())
            .sub(Tag(*b"QNAM"), 0x900_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0, 0, 0, 0]),
    );
    let mut info = Record::new(Tag(*b"INFO"), 0x810)
        .sub(Tag(*b"ENAM"), [0, 0, 0, 0])
        .sub(Tag(*b"TRDT"), [0; 24])
        .zsub(Tag(*b"NAM1"), "First response")
        .sub(Tag(*b"TRDT"), [1; 24])
        .zsub(Tag(*b"NAM1"), "Second response");
    info.placement.group = Some(GroupKind::TopicChildren(0x800));
    let info_index = source.push_record(info);
    let mut topic_group = Group::new(GroupKind::TopicChildren(0x800));
    topic_group.entries.push(Entry::Record(info_index));
    source
        .find_group_mut(GroupKind::Top(Tag(*b"DIAL")))
        .unwrap()
        .entries
        .push(Entry::Group(topic_group));

    let patch_plugin = Plugin::new(FileHeader {
        masters: vec![base.file_name().unwrap().to_string_lossy().into_owned()],
        ..Default::default()
    });
    let _ = fs::remove_file(&base);
    let _ = fs::remove_file(&patch);
    fs::write(&base, source.to_bytes()).unwrap();
    fs::write(&patch, patch_plugin.to_bytes()).unwrap();
    let active_name = patch.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(&[base.clone(), patch.clone()], &active_name).unwrap();
    Fixture {
        document,
        base,
        patch,
    }
}

#[test]
fn edt_19_topic_override_saves_with_quest_and_branch_links() {
    let mut fixture = fixture("topic");
    let source = fixture.document.load_order().winner(TOPIC).unwrap().clone();
    let source_quest = u32::from_le_bytes(source.find(Tag(*b"QNAM")).unwrap().try_into().unwrap());
    let source_branch = u32::from_le_bytes(source.find(Tag(*b"BNAM")).unwrap().try_into().unwrap());
    let quest = fixture.document.load_order().resolve(0, source_quest);
    let branch = fixture.document.load_order().resolve(0, source_branch);
    assert_eq!(quest, QUEST);
    assert_eq!(branch, BRANCH);

    fixture
        .document
        .set_subrecord(TOPIC, Tag(*b"PNAM"), 75.0_f32.to_le_bytes().to_vec())
        .unwrap();
    let saved = fixture.document.save(&fixture.patch).unwrap();
    let reloaded = Plugin::parse(&fs::read(&fixture.patch).unwrap()).unwrap();
    let topic_override = reloaded.records_of(Tag(*b"DIAL")).next().unwrap();

    assert_eq!(
        topic_override.find(Tag(*b"PNAM")),
        Some(&75.0_f32.to_le_bytes()[..])
    );
    assert_eq!(
        fixture.document.load_order().resolve(
            1,
            u32::from_le_bytes(
                topic_override
                    .find(Tag(*b"QNAM"))
                    .unwrap()
                    .try_into()
                    .unwrap()
            )
        ),
        quest
    );
    assert_eq!(
        fixture.document.load_order().resolve(
            1,
            u32::from_le_bytes(
                topic_override
                    .find(Tag(*b"BNAM"))
                    .unwrap()
                    .try_into()
                    .unwrap()
            )
        ),
        branch
    );
    assert_eq!(saved.to_bytes(), reloaded.to_bytes());
}

#[test]
fn edt_19_response_override_saves_under_owning_topic_group() {
    let mut fixture = fixture("response");
    fixture
        .document
        .set_dialogue_response_text(INFO, 1, b"Edited response\0".to_vec())
        .unwrap();
    let saved = fixture.document.save(&fixture.patch).unwrap();
    let reloaded = Plugin::parse(&fs::read(&fixture.patch).unwrap()).unwrap();
    let info_override = reloaded.records_of(Tag(*b"INFO")).next().unwrap();
    let texts = info_override.all(Tag(*b"NAM1")).collect::<Vec<_>>();

    assert_eq!(
        texts,
        vec![&b"First response\0"[..], &b"Edited response\0"[..]]
    );
    let topic_group = reloaded
        .groups
        .iter()
        .find(|group| group.kind == GroupKind::Top(Tag(*b"DIAL")))
        .unwrap()
        .entries
        .iter()
        .find_map(|entry| match entry {
            Entry::Group(group) if group.kind == GroupKind::TopicChildren(0x800) => Some(group),
            _ => None,
        })
        .unwrap();
    assert!(topic_group
        .entries
        .iter()
        .any(|entry| matches!(entry, Entry::Record(_))));
    assert_eq!(saved.to_bytes(), reloaded.to_bytes());
}
