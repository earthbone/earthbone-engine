// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! SAV-10 lifecycle restoration edge checks.

use soe_esm::{GlobalFormId, LoadOrder, Plugin, plugin::FileHeader};
use soe_game::{Game, quest};
use soe_papyrus::{
    Opcode, Value,
    asm::{FunctionBuilder, ScriptBuilder, ident, s},
};
use soe_save::{
    ChangeForm, Compression, FileLocationTable, GlobalData, Header, RefId, Save, WString,
    quest::{QuestChange, flags as quest_flags},
};

fn reference(raw: u32) -> RefId {
    RefId::from_bytes([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn game() -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Skyrim.esm", Plugin::new(FileHeader::default()))
        .unwrap();
    let mut plugin = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..Default::default()
    });
    let mut record = quest::build_record(0x0100_0900, "RestoredQuest", 0x0100);
    record = quest::with_stage(record, 10, 0, 0, "");
    plugin.push_top(record);
    order.add("QuestPatch.esp", plugin).unwrap();
    Game::new(order)
}

fn save(flags: u16) -> Save {
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
    let mut plugins = vec![2];
    WString::from_text("Skyrim.esm").write(&mut plugins);
    WString::from_text("QuestPatch.esp").write(&mut plugins);
    plugins.extend_from_slice(&0_u16.to_le_bytes());
    bytes.push(78);
    bytes.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&plugins);
    let at = bytes.len() as u32 + 100;
    FileLocationTable {
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
    save.plugins = ["Skyrim.esm", "QuestPatch.esp"]
        .map(WString::from_text)
        .to_vec();
    save.form_id_array = vec![0x0100_0900];
    let change = QuestChange {
        change_flags: quest_flags::QUEST,
        form_flags: None,
        quest_flags: Some(flags),
        script_delay: None,
        stages: None,
        objectives: None,
        runtime: None,
        instances: None,
        already_run: None,
    };
    save.change_forms.push(ChangeForm {
        form: reference(1),
        change_flags: quest_flags::QUEST,
        type_field: 8,
        version: 0,
        uncompressed_len: 0,
        data: change.data_bytes().unwrap(),
    });
    save
}

fn lifecycle(game: &mut Game, name: &str) -> Value {
    game.load_script(
        &ScriptBuilder::new("LifecycleProbe", "Quest")
            .function(
                FunctionBuilder::new("Run", "Bool")
                    .global()
                    .param("target", "Quest")
                    .local("result", "Bool")
                    .ins(
                        Opcode::CallMethod,
                        vec![s(name), ident("target"), ident("result")],
                    )
                    .ins(Opcode::Return, vec![ident("result")])
                    .build(),
            )
            .bytes(),
    )
    .unwrap();
    let target = Value::Object(game.object_of_form(GlobalFormId {
        file: 1,
        object: 0x900,
    }));
    let thread = game
        .vm
        .call_global("LifecycleProbe", "Run", &[target])
        .unwrap();
    game.run_until_idle(16);
    game.vm.thread_result(thread).unwrap()
}

#[test]
fn sav_10_transitional_start_task_resumes_after_restore() {
    let mut game = game();
    game.world_mut()
        .restore_vanilla_quests(&save(u16::MAX))
        .unwrap();
    assert_eq!(lifecycle(&mut game, "IsStarting"), Value::Bool(true));
    assert_eq!(lifecycle(&mut game, "IsRunning"), Value::Bool(false));
    game.tick(0.25);
    assert_eq!(
        game.world()
            .quest(GlobalFormId {
                file: 1,
                object: 0x900
            })
            .unwrap()
            .lifecycle_task,
        None,
        "a restored startup task should be consumed by the lifecycle scheduler"
    );
    assert!(
        game.world()
            .quest(GlobalFormId {
                file: 1,
                object: 0x900
            })
            .unwrap()
            .running
    );
}

#[test]
fn sav_10_transitional_shutdown_task_resumes_after_restore() {
    let mut game = game();
    game.world_mut()
        .restore_vanilla_quests(&save(u16::MAX - 1))
        .unwrap();
    assert_eq!(lifecycle(&mut game, "IsStopping"), Value::Bool(true));
    assert_eq!(lifecycle(&mut game, "IsRunning"), Value::Bool(false));
    game.tick(0.25);
    assert_eq!(
        game.world()
            .quest(GlobalFormId {
                file: 1,
                object: 0x900
            })
            .unwrap()
            .lifecycle_task,
        None,
        "a restored shutdown task should be consumed by the lifecycle scheduler"
    );
    assert!(
        !game
            .world()
            .quest(GlobalFormId {
                file: 1,
                object: 0x900
            })
            .unwrap()
            .running
    );
}
