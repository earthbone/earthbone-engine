// SPDX-License-Identifier: GPL-3.0-or-later

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use soe_editor::{EditorDocument, EditorError};
use soe_esm::plugin::{FileHeader, Record};
use soe_esm::records::gameplay::{Scene, SceneActionData};
use soe_esm::{GlobalFormId, LoadOrder, Plugin, Tag};

const SCENE: GlobalFormId = GlobalFormId {
    file: 2,
    object: 0x820,
};
const NEW_TOPIC: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x802,
};
const WRONG_TOPIC_TYPE: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x803,
};

struct Fixture {
    document: EditorDocument,
    paths: Vec<PathBuf>,
}

fn fixture(label: &str, action_start: u32) -> Fixture {
    let root =
        std::env::temp_dir().join(format!("soe-editor-edt-20-{}-{label}", std::process::id()));
    let base_path = root.with_extension("Base.esm");
    let other_path = root.with_extension("Other.esm");
    let source_path = root.with_extension("SceneSource.esp");
    let patch_path = root.with_extension("Patch.esp");
    let base_name = base_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let other_name = other_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let source_name = source_path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();

    let mut base = Plugin::new(FileHeader::default());
    base.push_top(Record::new(Tag(*b"QUST"), 0x800).zsub(Tag(*b"EDID"), "EditorQuest"));
    base.push_top(Record::new(Tag(*b"DIAL"), 0x801).zsub(Tag(*b"EDID"), "OldTopic"));
    base.push_top(Record::new(Tag(*b"DIAL"), 0x802).zsub(Tag(*b"EDID"), "NewTopic"));
    base.push_top(Record::new(Tag(*b"MISC"), 0x803).zsub(Tag(*b"EDID"), "NotATopic"));

    let other = Plugin::new(FileHeader::default());
    let mut source = Plugin::new(FileHeader {
        masters: vec![other_name, base_name.clone()],
        ..Default::default()
    });
    let scene = Record::new(Tag(*b"SCEN"), 0x0200_0820)
        .zsub(Tag(*b"EDID"), "EditorScene")
        .sub(Tag(*b"HNAM"), Vec::new())
        .zsub(Tag(*b"NAM0"), "First phase")
        .sub(Tag(*b"NEXT"), Vec::new())
        .sub(Tag(*b"NEXT"), Vec::new())
        .sub(Tag(*b"WNAM"), 200u32.to_le_bytes())
        .sub(Tag(*b"HNAM"), Vec::new())
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Talk to the player")
        .sub(Tag(*b"ALID"), (-1i32).to_le_bytes())
        .sub(Tag(*b"INAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), action_start.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x0100_0801u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), Vec::new())
        .sub(Tag(*b"PNAM"), 0x0100_0800u32.to_le_bytes());
    source.push_top(scene);

    let patch = Plugin::new(FileHeader {
        masters: vec![
            base_name,
            other_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            source_name,
        ],
        ..Default::default()
    });

    for (path, plugin) in [
        (&base_path, base),
        (&other_path, other),
        (&source_path, source),
        (&patch_path, patch),
    ] {
        let _ = fs::remove_file(path);
        fs::write(path, plugin.to_bytes()).unwrap();
    }
    let active_name = patch_path.file_name().unwrap().to_string_lossy();
    let document = EditorDocument::open(
        &[
            base_path.clone(),
            other_path.clone(),
            source_path.clone(),
            patch_path.clone(),
        ],
        &active_name,
    )
    .unwrap();
    Fixture {
        document,
        paths: vec![base_path, other_path, source_path, patch_path],
    }
}

fn cleanup(fixture: Fixture) {
    for path in fixture.paths {
        let _ = fs::remove_file(path);
    }
}

#[test]
fn edt_20_scene_phase_and_topic_save_with_relocated_valid_links_after_independent_reload() {
    let mut fixture = fixture("save-reload", 0);
    fixture
        .document
        .set_scene_phase_name(SCENE, 0, "Revised phase")
        .unwrap();
    fixture
        .document
        .set_scene_action_topic(SCENE, 0, NEW_TOPIC)
        .unwrap();

    let saved_path = fixture.paths[3].clone();
    let saved = fixture.document.save(&saved_path).unwrap();
    let independently_parsed = Plugin::parse(&fs::read(&saved_path).unwrap()).unwrap();
    assert_eq!(saved.to_bytes(), independently_parsed.to_bytes());
    let saved_record = independently_parsed
        .records
        .iter()
        .find(|record| record.tag == Tag(*b"SCEN"))
        .unwrap();
    let scene = Scene::from_record(saved_record).unwrap();

    assert_eq!(scene.phases[0].name.as_deref(), Some("Revised phase"));
    assert_eq!(scene.quest, Some(0x0000_0800));
    assert_eq!(
        scene.actions[0].data,
        SceneActionData::Dialogue { topic: Some(0x802) }
    );
    assert_eq!(scene.actions[0].start_phase, Some(0));
    assert_eq!(scene.actions[0].end_phase, Some(0));

    let mut reloaded_order = LoadOrder::new();
    for (path, plugin) in [
        (fixture.paths[0].clone(), None),
        (fixture.paths[1].clone(), None),
        (fixture.paths[2].clone(), None),
        (fixture.paths[3].clone(), Some(independently_parsed)),
    ] {
        let plugin = plugin.unwrap_or_else(|| Plugin::parse(&fs::read(&path).unwrap()).unwrap());
        let name = path.file_name().unwrap().to_string_lossy();
        reloaded_order.add(&name, plugin).unwrap();
    }
    let patch_index = 3;
    let quest_id = reloaded_order.resolve(patch_index, scene.quest.unwrap());
    let topic_id = reloaded_order.resolve(
        patch_index,
        match scene.actions[0].data {
            SceneActionData::Dialogue { topic: Some(topic) } => topic,
            _ => panic!("saved action must retain its Dialogue topic"),
        },
    );
    assert_eq!(reloaded_order.winner(quest_id).unwrap().tag, Tag(*b"QUST"));
    assert_eq!(reloaded_order.winner(topic_id).unwrap().tag, Tag(*b"DIAL"));
    cleanup(fixture);
}

#[test]
fn edt_20_scene_rejects_topic_with_wrong_record_type_without_editing() {
    let mut fixture = fixture("bad-topic", 0);
    let before = fixture.document.active_plugin().clone();
    let error = fixture
        .document
        .set_scene_action_topic(SCENE, 0, WRONG_TOPIC_TYPE)
        .unwrap_err();

    assert!(matches!(error, EditorError::InvalidScene { .. }));
    assert_eq!(fixture.document.active_plugin(), &before);
    cleanup(fixture);
}

#[test]
fn edt_20_scene_rejects_action_phase_outside_the_phase_list() {
    let mut fixture = fixture("bad-phase-link", 1);
    let error = fixture
        .document
        .set_scene_phase_name(SCENE, 0, "Unsafe edit")
        .unwrap_err();

    assert!(matches!(error, EditorError::InvalidScene { .. }));
    assert!(fixture.document.active_plugin().records.is_empty());
    cleanup(fixture);
}
