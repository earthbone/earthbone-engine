// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{plugin::FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{quest, Game};
use soe_papyrus::{
    asm::{i, ident, s, FunctionBuilder, ScriptBuilder},
    Opcode,
};
use soe_save::{ChangeForm, Compression, GlobalData, Header, RefId, Save, WString};

const QUEST: u32 = 0x900;

fn id(file: u16, object: u32) -> GlobalFormId {
    GlobalFormId { file, object }
}

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn fixture() -> Game {
    let mut order = LoadOrder::new();
    for (name, light) in [
        ("Skyrim.esm", false),
        ("Extra.esl", true),
        ("B.esp", false),
        ("A.esp", false),
        ("Light.esl", true),
    ] {
        let mut plugin = Plugin::new(FileHeader {
            flags: if light {
                soe_esm::plugin::flags::LIGHT_MASTER
            } else {
                0
            },
            ..Default::default()
        });
        plugin
            .push_top(Record::new(Tag(*b"GLOB"), 0x800).sub(Tag(*b"FLTV"), 11.0_f32.to_le_bytes()));
        order.add(name, plugin).unwrap();
    }
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into(), "A.esp".into()],
        ..Default::default()
    });
    let mut record = quest::build_record(0x0100_0900, "Sav20Quest", 0x0100);
    for stage in [0, 10, 20] {
        record = quest::with_stage(record, stage, 0, 0, "");
    }
    for objective in 0..6 {
        record = quest::with_objective(record, objective, "");
    }
    record = quest::with_alias(record, 5, "Target", Some(0x0100_0800))
        .sub(Tag(*b"ALLS"), 6_u32.to_le_bytes())
        .sub(Tag(*b"ALED"), []);
    patch.push_top(record);
    patch.push_top(
        Record::new(Tag(*b"GLOB"), 0x0100_0800).sub(Tag(*b"FLTV"), 15.0_f32.to_le_bytes()),
    );
    order.add("QuestPatch.esp", patch).unwrap();
    Game::new(order)
}

fn save() -> Save {
    let header = Header {
        version: 12,
        save_number: 1,
        player_name: WString::from_text("Prisoner"),
        player_level: 1,
        player_location: WString::default(),
        game_date: WString::default(),
        player_race: WString::default(),
        player_sex: 0,
        current_experience: 0.0,
        needed_experience: 0.0,
        filetime: 0,
        screenshot_width: 0,
        screenshot_height: 0,
        compression: Compression::None,
        screenshot: vec![],
    };
    let mut bytes = vec![];
    header.write(&mut bytes);
    let mut plugins = vec![1];
    WString::from_text("Skyrim.esm").write(&mut plugins);
    plugins.extend_from_slice(&0_u16.to_le_bytes());
    bytes.push(78);
    bytes.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&plugins);
    let at = bytes.len() as u32 + 100;
    soe_save::FileLocationTable {
        form_id_array_count_offset: at + 8,
        unknown_table_3_offset: at + 16,
        table_1_offset: at,
        table_2_offset: at,
        change_forms_offset: at,
        table_3_offset: at,
        table_1_count: 0,
        table_2_count: 0,
        table_3_count: 0,
        change_form_count: 0,
        unused: [0; 15],
    }
    .write(&mut bytes);
    GlobalData {
        kind: 1000,
        data: vec![],
    }
    .write(&mut bytes);
    bytes.extend_from_slice(&[0; 8]);
    let mut save = Save::parse(&bytes).unwrap();
    save.plugins = ["skyrim.ESM", "a.ESP", "B.esp"]
        .map(WString::from_text)
        .to_vec();
    save.light_plugins = vec![WString::from_text("LIGHT.esl")];
    save.form_id_array = vec![0x0100_0900, 0xFF00_1234, 0x0100_0800, 0xFE00_0800];
    save.change_forms.push(ChangeForm {
        form: reference(1),
        change_flags: soe_save::quest::flags::KNOWN,
        type_field: 8,
        version: 0,
        uncompressed_len: 0,
        data: payload(),
    });
    save
}

fn payload() -> Vec<u8> {
    let mut out = vec![];
    out.extend(0x8000_0100_u32.to_le_bytes());
    out.extend(0x1234_u16.to_le_bytes());
    out.extend(0x2983_u16.to_le_bytes());
    out.extend(1.25_f32.to_le_bytes());
    out.push(3 << 2);
    for (stage, done) in [(0_u16, 1_u8), (20, 1), (10, 0)] {
        out.extend(stage.to_le_bytes());
        out.push(done);
    }
    out.push(6 << 2);
    for status in 0_u32..6 {
        out.extend(status.to_le_bytes());
        out.extend(status.to_le_bytes());
    }
    out.push(0xA5);
    out.extend(1_u32.to_le_bytes());
    out.extend(5_u32.to_le_bytes());
    out.push(1);
    for raw in [2, 3, 0x400000, 0x400000, 4] {
        reference(raw).write(&mut out);
    }
    out.extend(1_u32.to_le_bytes());
    out.extend(6_u32.to_le_bytes());
    reference(4).write(&mut out);
    out.push(1);
    out.extend(8_u32.to_le_bytes());
    out.extend(b"SCPT");
    out.extend(2_u32.to_le_bytes());
    out.extend(0_u32.to_le_bytes());
    reference(2).write(&mut out);
    out.extend(3_u32.to_le_bytes());
    out.extend(42_u32.to_le_bytes());
    out.extend(7_u32.to_le_bytes());
    out.push(2 << 2);
    for (instance, value) in [(3_u32, -0.0_f32), (7, 9.5)] {
        out.extend(instance.to_le_bytes());
        out.push(1 << 2);
        out.extend(5_u32.to_le_bytes());
        reference(3).write(&mut out);
        out.push(1 << 2);
        reference(3).write(&mut out);
        out.extend(value.to_le_bytes());
        out.extend(20_u16.to_le_bytes());
        out.push(0xFF);
    }
    out.push(1);
    out
}

fn event_handler() -> Vec<u8> {
    let mut function = FunctionBuilder::new("OnStoryScript", "None")
        .param("keyword", "Var")
        .param("location", "Var")
        .param("reference1", "Var")
        .param("reference2", "Var")
        .param("value1", "Var")
        .param("value2", "Var");
    for argument in [
        "keyword",
        "location",
        "reference1",
        "reference2",
        "value1",
        "value2",
    ] {
        function = function.ins(
            Opcode::CallStatic,
            vec![
                s("Debug"),
                s("Trace"),
                ident("::nonevar"),
                ident(argument),
                i(0),
            ],
        );
    }
    ScriptBuilder::new("Sav20StoryProbe", "Quest")
        .function(function.build())
        .bytes()
}

#[test]
fn sav_20_all_six_mapped_story_arguments_survive_reload_and_dispatch_once() {
    let mut game = fixture();
    let mut save = save();
    let mut change = save.change_forms[0].quest_change().unwrap().unwrap();
    let event = change.runtime.as_mut().unwrap().event.as_mut().unwrap();
    event.parameters = vec![
        soe_save::quest::EventParameter::Reference {
            kind: 0,
            reference: reference(3),
        },
        soe_save::quest::EventParameter::Reference {
            kind: 1,
            reference: reference(4),
        },
        soe_save::quest::EventParameter::Reference {
            kind: 2,
            reference: reference(3),
        },
        soe_save::quest::EventParameter::Reference {
            kind: 4,
            reference: reference(4),
        },
        soe_save::quest::EventParameter::Integer(u32::MAX - 15),
        soe_save::quest::EventParameter::Integer(42),
    ];
    save.change_forms[0].data = change.data_bytes().unwrap();
    game.world_mut().restore_vanilla_quests(&save).unwrap();
    let quest_id = id(3, QUEST);
    assert_eq!(
        game.world()
            .quest(quest_id)
            .unwrap()
            .runtime
            .run_data
            .as_ref()
            .unwrap()
            .event
            .as_ref()
            .unwrap()
            .parameters
            .len(),
        6
    );
    let world_bytes = game.world().save_state();
    game = fixture();
    game.world_mut().load_state(&world_bytes).unwrap();
    assert_eq!(
        game.world()
            .quest(quest_id)
            .unwrap()
            .runtime
            .run_data
            .as_ref()
            .unwrap()
            .event
            .as_ref()
            .unwrap()
            .parameters
            .len(),
        6
    );
    let quest_object = game.object_of_form(quest_id);
    game.load_script(&event_handler()).unwrap();
    game.vm.attach(quest_object, "Sav20StoryProbe").unwrap();
    assert!(game
        .vm
        .has_script_method(quest_object, "Sav20StoryProbe", "OnStoryScript"));
    assert!(game.world().exists(id(3, 0x800)));
    assert!(game.world().exists(id(4, 0x800)));

    assert_eq!(game.resume_vanilla_quest_story_events(), 1);
    let messages = game.vm.log.messages();
    for form in [id(3, 0x800), id(4, 0x800)] {
        let runtime = game.world().runtime_id(form);
        assert!(messages
            .iter()
            .any(|message| message.to_string().contains(&format!("{runtime:08X}"))));
    }
    assert!(messages
        .iter()
        .any(|message| message.to_string().contains("-16")));
    assert!(messages
        .iter()
        .any(|message| message.to_string().contains("42")));
    assert_eq!(game.resume_vanilla_quest_story_events(), 0);
    assert!(game
        .world()
        .quest(quest_id)
        .unwrap()
        .runtime
        .run_data
        .as_ref()
        .unwrap()
        .event
        .is_none());
}
