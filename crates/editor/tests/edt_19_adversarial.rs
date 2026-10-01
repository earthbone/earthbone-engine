#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{Entry, FileHeader, Group, GroupKind, Plugin, Record};
use soe_esm::{GlobalFormId, Tag};

#[test]
fn edt_19_combined_topic_and_response_edits_preserve_owner_links() {
    let root = std::env::temp_dir().join(format!("soe-editor-edt-19-adv-{}", std::process::id()));
    let base = root.with_extension("Base.esm");
    let patch = root.with_extension("Patch.esp");
    let mut source = Plugin::new(FileHeader::default());
    source.push_top(Record::new(Tag(*b"QUST"), 0x900));
    source.push_top(Record::new(Tag(*b"DLBR"), 0x901));
    source.push_top(
        Record::new(Tag(*b"DIAL"), 0x800)
            .sub(Tag(*b"PNAM"), 50.0_f32.to_le_bytes())
            .sub(Tag(*b"BNAM"), 0x901_u32.to_le_bytes())
            .sub(Tag(*b"QNAM"), 0x900_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0, 0, 0, 0]),
    );
    let mut info = Record::new(Tag(*b"INFO"), 0x810)
        .sub(Tag(*b"TRDT"), [0; 24])
        .zsub(Tag(*b"NAM1"), "Original response");
    info.placement.group = Some(GroupKind::TopicChildren(0x800));
    let index = source.push_record(info);
    let mut children = Group::new(GroupKind::TopicChildren(0x800));
    children.entries.push(Entry::Record(index));
    source
        .find_group_mut(GroupKind::Top(Tag(*b"DIAL")))
        .unwrap()
        .entries
        .push(Entry::Group(children));
    let override_plugin = Plugin::new(FileHeader {
        masters: vec![base.file_name().unwrap().to_string_lossy().into_owned()],
        ..Default::default()
    });
    fs::write(&base, source.to_bytes()).unwrap();
    fs::write(&patch, override_plugin.to_bytes()).unwrap();
    let active = patch.file_name().unwrap().to_string_lossy();
    let mut document = EditorDocument::open(&[base.clone(), patch.clone()], &active).unwrap();
    let topic = GlobalFormId {
        file: 0,
        object: 0x800,
    };
    let response = GlobalFormId {
        file: 0,
        object: 0x810,
    };

    document
        .set_subrecord(topic, Tag(*b"PNAM"), 88.0_f32.to_le_bytes().to_vec())
        .unwrap();
    document
        .set_dialogue_response_text(response, 0, b"Edited response\0".to_vec())
        .unwrap();
    let saved = document.save(&patch).unwrap();
    let reloaded = Plugin::parse(&fs::read(&patch).unwrap()).unwrap();
    let topic_record = reloaded.records_of(Tag(*b"DIAL")).next().unwrap();
    let info_record = reloaded.records_of(Tag(*b"INFO")).next().unwrap();
    assert_eq!(
        topic_record.find(Tag(*b"PNAM")),
        Some(&88.0_f32.to_le_bytes()[..])
    );
    let patch_quest = u32::from_le_bytes(
        topic_record
            .find(Tag(*b"QNAM"))
            .unwrap()
            .try_into()
            .unwrap(),
    );
    let patch_branch = u32::from_le_bytes(
        topic_record
            .find(Tag(*b"BNAM"))
            .unwrap()
            .try_into()
            .unwrap(),
    );
    assert_eq!(
        document.load_order().resolve(1, patch_quest),
        GlobalFormId {
            file: 0,
            object: 0x900
        }
    );
    assert_eq!(
        document.load_order().resolve(1, patch_branch),
        GlobalFormId {
            file: 0,
            object: 0x901
        }
    );
    assert_eq!(
        info_record.find(Tag(*b"NAM1")),
        Some(&b"Edited response\0"[..])
    );
    assert!(saved.groups.iter().any(|group| group.kind == GroupKind::Top(Tag(*b"DIAL")) && group.entries.iter().any(|entry| matches!(entry, Entry::Group(child) if child.kind == GroupKind::TopicChildren(0x800)))));
    let _ = fs::remove_file(base);
    let _ = fs::remove_file(patch);
}
