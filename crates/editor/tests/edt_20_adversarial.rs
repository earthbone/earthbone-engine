#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;

use soe_editor::EditorDocument;
use soe_esm::plugin::{FileHeader, Plugin, Record};
use soe_esm::records::gameplay::{Scene, SceneActionData};
use soe_esm::{GlobalFormId, Tag};

#[test]
fn edt_20_second_action_edit_survives_reload_and_preserves_first_action() {
    let root = std::env::temp_dir().join(format!("soe-editor-edt-20-adv-{}", std::process::id()));
    let base = root.with_extension("Base.esm");
    let source_path = root.with_extension("Source.esp");
    let patch = root.with_extension("Patch.esp");
    let mut master = Plugin::new(FileHeader::default());
    master.push_top(Record::new(Tag(*b"QUST"), 0x800));
    master.push_top(Record::new(Tag(*b"DIAL"), 0x801));
    master.push_top(Record::new(Tag(*b"DIAL"), 0x802));
    let mut source = Plugin::new(FileHeader {
        masters: vec![base.file_name().unwrap().to_string_lossy().into_owned()],
        ..Default::default()
    });
    let scene = Record::new(Tag(*b"SCEN"), 0x0200_0820)
        .sub(Tag(*b"HNAM"), Vec::new())
        .zsub(Tag(*b"NAM0"), "Only phase")
        .sub(Tag(*b"NEXT"), Vec::new())
        .sub(Tag(*b"NEXT"), Vec::new())
        .sub(Tag(*b"WNAM"), 100_u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), 0_u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "First")
        .sub(Tag(*b"ALID"), (-1_i32).to_le_bytes())
        .sub(Tag(*b"INAM"), 0_u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0_u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0_u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x0000_0801_u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), Vec::new())
        .sub(Tag(*b"ANAM"), 0_u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Second")
        .sub(Tag(*b"ALID"), (-1_i32).to_le_bytes())
        .sub(Tag(*b"INAM"), 1_u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0_u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0_u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x0000_0801_u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), Vec::new())
        .sub(Tag(*b"ANAM"), Vec::new())
        .sub(Tag(*b"PNAM"), 0x0000_0800_u32.to_le_bytes());
    source.push_top(scene);
    let override_plugin = Plugin::new(FileHeader {
        masters: vec![
            base.file_name().unwrap().to_string_lossy().into_owned(),
            source_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ],
        ..Default::default()
    });
    fs::write(&base, master.to_bytes()).unwrap();
    fs::write(&source_path, source.to_bytes()).unwrap();
    fs::write(&patch, override_plugin.to_bytes()).unwrap();
    let active = patch.file_name().unwrap().to_string_lossy();
    let mut document =
        EditorDocument::open(&[base.clone(), source_path.clone(), patch.clone()], &active).unwrap();
    let scene_id = GlobalFormId {
        file: 1,
        object: 0x820,
    };
    let replacement = GlobalFormId {
        file: 0,
        object: 0x802,
    };

    document
        .set_scene_action_topic(scene_id, 1, replacement)
        .unwrap();
    document
        .set_scene_phase_name(scene_id, 0, "Renamed phase")
        .unwrap();
    document.save(&patch).unwrap();
    let saved = Plugin::parse(&fs::read(&patch).unwrap()).unwrap();
    let record = saved.records_of(Tag(*b"SCEN")).next().unwrap();
    let view = Scene::from_record(record).unwrap();
    assert_eq!(view.phases[0].name.as_deref(), Some("Renamed phase"));
    assert_eq!(view.actions.len(), 2);
    assert_eq!(
        view.actions[0].data,
        SceneActionData::Dialogue {
            topic: Some(0x0000_0801)
        }
    );
    assert_eq!(
        view.actions[1].data,
        SceneActionData::Dialogue {
            topic: Some(0x0000_0802)
        }
    );
    let _ = fs::remove_file(base);
    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(patch);
}
