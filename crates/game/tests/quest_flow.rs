// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! The round-2 done criterion: a synthetic plugin whose quest advances on
//! activation, end to end through VMAD binding, natives, fragments and the
//! clock. Plus the map's week 6 to 9 gate: a door teleports, a container
//! opens, a notification fires from a quest stage.

use soe_esm::{plugin::flags, Entry, FileHeader, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::harness::{self, Scenario};
use soe_game::quest::{self, entry_flags, stage_flags};
use soe_game::vmad::{ObjectRef, PropertyValue, QuestFragment, ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{b, f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::*;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const EDID: Tag = Tag::new(b"EDID");

const QUEST: u32 = 0x800;
const LEVER_BASE: u32 = 0x801;
const CELL_A: u32 = 0x802;
const LEVER: u32 = 0x803;
const PLAYER_BASE: u32 = 0x804;
const DOOR_BASE: u32 = 0x805;
const CHEST_BASE: u32 = 0x806;
const GOLD: u32 = 0x807;
const COUNTER: u32 = 0x808;
const DOOR_A: u32 = 0x809;
const CHEST: u32 = 0x80A;
const DOOR_B: u32 = 0x80B;
const CELL_B: u32 = 0x80C;
const PLAYER: u32 = 0x14;

fn form_prop(name: &str, form_id: u32) -> (String, PropertyValue) {
    (
        name.to_string(),
        PropertyValue::Object(ObjectRef { form_id, alias: -1 }),
    )
}

fn vmad_for(tag: [u8; 4], scripts: Vec<ScriptEntry>) -> Vec<u8> {
    Vmad {
        scripts,
        ..Default::default()
    }
    .write(T(&tag))
}

fn script_with(name: &str, props: &[(String, PropertyValue)]) -> ScriptEntry {
    let mut e = ScriptEntry::new(name);
    for (n, v) in props {
        e = e.prop(n, v.clone());
    }
    e
}

fn interior_cell(p: &mut Plugin, id: u32, edid: &str, refs: Vec<Record>) -> Group {
    let cell = p.push_record(
        Record::new(T(b"CELL"), id)
            .zsub(EDID, edid)
            .sub(T(b"DATA"), [1u8, 0]),
    );
    let mut temp = Group::new(GroupKind::CellTemporary(id));
    for r in refs {
        let idx = p.push_record(r);
        temp.entries.push(Entry::Record(idx));
    }
    let mut children = Group::new(GroupKind::CellChildren(id));
    children.entries.push(Entry::Group(temp));
    let mut sub = Group::new(GroupKind::InteriorSubBlock(0));
    sub.entries.push(Entry::Record(cell));
    sub.entries.push(Entry::Group(children));
    sub
}

fn test_plugin() -> Plugin {
    let mut p = Plugin::new(FileHeader {
        flags: flags::MASTER,
        author: "soe".into(),
        ..Default::default()
    });
    p.push_top(
        Record::new(T(b"NPC_"), PLAYER_BASE)
            .zsub(EDID, "PlayerBase")
            .zsub(T(b"FULL"), "Prisoner"),
    );
    p.push_top(
        Record::new(T(b"ACTI"), LEVER_BASE)
            .zsub(EDID, "TestLever")
            .zsub(T(b"FULL"), "Lever"),
    );
    p.push_top(
        Record::new(T(b"DOOR"), DOOR_BASE)
            .zsub(EDID, "TestDoor")
            .zsub(T(b"FULL"), "Door"),
    );
    p.push_top(
        Record::new(T(b"MISC"), GOLD)
            .zsub(EDID, "Gold001")
            .zsub(T(b"FULL"), "Gold"),
    );
    let mut cnto = GOLD.to_le_bytes().to_vec();
    cnto.extend_from_slice(&25i32.to_le_bytes());
    p.push_top(
        Record::new(T(b"CONT"), CHEST_BASE)
            .zsub(EDID, "TestChest")
            .zsub(T(b"FULL"), "Chest")
            .sub(T(b"CNTO"), cnto),
    );
    p.push_top(
        Record::new(T(b"GLOB"), COUNTER)
            .zsub(EDID, "TestCounter")
            .sub(T(b"FNAM"), *b"f")
            .sub(T(b"FLTV"), 0f32.to_le_bytes()),
    );

    let quest_vmad = Vmad {
        scripts: vec![
            script_with("MQ01Script", &[form_prop("Counter", COUNTER)]),
            script_with("QF_MQ01", &[form_prop("Gold", GOLD)]),
        ],
        fragment_file: Some("QF_MQ01".into()),
        fragments: vec![QuestFragment {
            stage: 10,
            log_entry: 0,
            script: "QF_MQ01".into(),
            function: "Fragment_10".into(),
        }],
        ..Default::default()
    };
    let q = quest::build_record(QUEST, "MQ01", quest::flags::START_GAME_ENABLED)
        .sub(T(b"VMAD"), quest_vmad.write(T(b"QUST")))
        .zsub(T(b"FULL"), "The Lever");
    let q = quest::with_stage(q, 0, stage_flags::START_UP, 0, "");
    let q = quest::with_stage(q, 10, 0, 0, "Pull the lever");
    let q = quest::with_stage(q, 20, 0, entry_flags::COMPLETE_QUEST, "Done");
    let q = quest::with_objective(q, 10, "Pull the lever");
    let q = quest::with_alias(q, 0, "Lever", Some(LEVER));
    p.push_top(q);

    let mut xtel = DOOR_B.to_le_bytes().to_vec();
    for v in [100.0f32, 200.0, 300.0, 0.0, 0.0, 1.5] {
        xtel.extend_from_slice(&v.to_le_bytes());
    }
    let zero_data = vec![0u8; 24];
    let cell_a = interior_cell(
        &mut p,
        CELL_A,
        "TestCellA",
        vec![
            Record::new(T(b"ACHR"), PLAYER)
                .zsub(EDID, "PlayerRef")
                .sub(T(b"NAME"), PLAYER_BASE.to_le_bytes())
                .sub(T(b"DATA"), zero_data.clone()),
            Record::new(T(b"REFR"), LEVER)
                .zsub(EDID, "LeverRef")
                .sub(
                    T(b"VMAD"),
                    vmad_for(
                        *b"REFR",
                        vec![script_with("LeverScript", &[form_prop("MyQuest", QUEST)])],
                    ),
                )
                .sub(T(b"NAME"), LEVER_BASE.to_le_bytes())
                .sub(T(b"DATA"), zero_data.clone()),
            Record::new(T(b"REFR"), DOOR_A)
                .zsub(EDID, "DoorA")
                .sub(T(b"NAME"), DOOR_BASE.to_le_bytes())
                .sub(T(b"XTEL"), xtel)
                .sub(T(b"DATA"), zero_data.clone()),
            Record::new(T(b"REFR"), CHEST)
                .zsub(EDID, "ChestRef")
                .sub(
                    T(b"VMAD"),
                    vmad_for(*b"REFR", vec![script_with("TimerScript", &[])]),
                )
                .sub(T(b"NAME"), CHEST_BASE.to_le_bytes())
                .sub(T(b"DATA"), zero_data.clone()),
        ],
    );
    let cell_b = interior_cell(
        &mut p,
        CELL_B,
        "TestCellB",
        vec![Record::new(T(b"REFR"), DOOR_B)
            .zsub(EDID, "DoorB")
            .sub(T(b"NAME"), DOOR_BASE.to_le_bytes())
            .sub(T(b"DATA"), zero_data)],
    );
    let mut block = Group::new(GroupKind::InteriorBlock(0));
    block.entries.push(Entry::Group(cell_a));
    block.entries.push(Entry::Group(cell_b));
    let mut top = Group::new(GroupKind::Top(T(b"CELL")));
    top.entries.push(Entry::Group(block));
    p.groups.push(top);
    p
}

fn scripts() -> Vec<Vec<u8>> {
    let lever = ScriptBuilder::new("LeverScript", "ObjectReference")
        .auto_property("MyQuest", "Quest", none())
        .function(
            FunctionBuilder::new("OnActivate", "None")
                .param("akActionRef", "ObjectReference")
                .ins(
                    CallMethod,
                    vec![
                        s("SetStage"),
                        ident("::MyQuest_var"),
                        ident("::nonevar"),
                        i(10),
                    ],
                )
                .ins(
                    CallStatic,
                    vec![
                        s("Debug"),
                        s("Notification"),
                        ident("::nonevar"),
                        s("Lever pulled"),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![s("Disable"), ident("self"), ident("::nonevar"), b(false)],
                )
                .build(),
        )
        .bytes();
    let timer = ScriptBuilder::new("TimerScript", "ObjectReference")
        .function(
            FunctionBuilder::new("OnActivate", "None")
                .param("akActionRef", "ObjectReference")
                .ins(
                    CallMethod,
                    vec![
                        s("RegisterForSingleUpdate"),
                        ident("self"),
                        ident("::nonevar"),
                        f(2.0),
                    ],
                )
                .build(),
        )
        .function(
            FunctionBuilder::new("OnUpdate", "None")
                .ins(
                    CallStatic,
                    vec![
                        s("Debug"),
                        s("Trace"),
                        ident("::nonevar"),
                        s("timer fired"),
                        i(0),
                    ],
                )
                .build(),
        )
        .bytes();
    let mq01 = ScriptBuilder::new("MQ01Script", "Quest")
        .auto_property("Counter", "GlobalVariable", none())
        .function(
            FunctionBuilder::new("OnInit", "None")
                .ins(
                    CallStatic,
                    vec![
                        s("Debug"),
                        s("Trace"),
                        ident("::nonevar"),
                        s("MQ01 init"),
                        i(0),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![s("Mod"), ident("::Counter_var"), ident("::nonevar"), f(1.0)],
                )
                .build(),
        )
        .bytes();
    let fragments = ScriptBuilder::new("QF_MQ01", "Quest")
        .auto_property("Gold", "MiscObject", none())
        .function(
            FunctionBuilder::new("Fragment_10", "None")
                .local("p", "Actor")
                .ins(
                    CallMethod,
                    vec![
                        s("SetObjectiveCompleted"),
                        ident("self"),
                        ident("::nonevar"),
                        i(10),
                        b(true),
                    ],
                )
                .ins(CallStatic, vec![s("Game"), s("GetPlayer"), ident("p")])
                .ins(
                    CallMethod,
                    vec![
                        s("AddItem"),
                        ident("p"),
                        ident("::nonevar"),
                        ident("::Gold_var"),
                        i(5),
                        b(true),
                    ],
                )
                .ins(
                    CallStatic,
                    vec![
                        s("Debug"),
                        s("Trace"),
                        ident("::nonevar"),
                        s("fragment 10"),
                        i(0),
                    ],
                )
                .ins(
                    CallMethod,
                    vec![s("SetStage"), ident("self"), ident("::nonevar"), i(20)],
                )
                .build(),
        )
        .bytes();
    vec![lever, timer, mq01, fragments]
}

pub fn game() -> Game {
    let bytes = test_plugin().to_bytes();
    let mut order = LoadOrder::new();
    order
        .add("Test.esm", Plugin::parse(&bytes).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    for pex in scripts() {
        game.load_script(&pex).unwrap();
    }
    let cell = game.form("TestCellA").unwrap();
    let report = game.load_cell(cell);
    assert_eq!(report.forms_with_scripts, 2, "{report:?}");
    assert_eq!(report.scripts_attached, 2, "{report:?}");
    assert_eq!(report.properties_set, 1, "{report:?}");
    assert!(
        report.scripts_missing.is_empty() && report.properties_missing.is_empty(),
        "{report:?}"
    );
    game
}

fn script_errors(game: &Game) -> Vec<String> {
    game.traces()
        .into_iter()
        .filter(|t| t.contains("no function") || t.contains("no global") || t.contains("error"))
        .collect()
}

#[test]
fn a_quest_advances_on_activation_end_to_end() {
    let mut game = game();
    let report = game.start();
    assert_eq!(report.forms_with_scripts, 1, "{report:?}");
    assert_eq!(report.scripts_attached, 2, "{report:?}");
    assert_eq!(report.properties_set, 2, "{report:?}");
    assert!(
        report.scripts_missing.is_empty() && report.properties_missing.is_empty(),
        "{report:?}"
    );
    assert_eq!(report.quests_started, 1);

    let quest = game.form("MQ01").unwrap();
    let lever = game.form("LeverRef").unwrap();
    let gold = game.form("Gold001").unwrap();
    let counter = game.form("TestCounter").unwrap();
    let player = game.player();
    assert_eq!(
        player.object, PLAYER,
        "the shipped player reference is used"
    );
    {
        let mut w = game.world_mut();
        let q = w.quest_state(quest).unwrap();
        assert!(q.running && !q.completed);
        assert_eq!(q.highest, 0);
        assert_eq!(
            w.global_value(counter),
            1.0,
            "OnInit ran the property getter and the global native"
        );
    }
    assert!(
        game.traces().contains(&"MQ01 init".to_string()),
        "{:?}",
        game.traces()
    );

    assert!(game.activate(lever, player));
    {
        let mut w = game.world_mut();
        let q = w.quest_state(quest).unwrap().clone();
        assert_eq!(q.highest, 20, "{q:?}");
        assert!(q.done.contains(&10) && q.done.contains(&20));
        assert!(q.completed && !q.running, "stage 20 completes the quest");
        assert!(w.objective(quest, 10).unwrap().completed);
        assert_eq!(
            w.item_count(player, gold),
            5,
            "the fragment gave the player gold"
        );
        assert!(
            !w.reference(lever).unwrap().enabled,
            "the lever disabled itself"
        );
        assert_eq!(w.notifications, ["Lever pulled"]);
        assert_eq!(
            w.alias_ref(quest, 0),
            Some(lever),
            "the forced alias holds the lever"
        );
    }
    let traces = game.traces();
    assert!(traces.contains(&"fragment 10".to_string()), "{traces:?}");
    assert!(script_errors(&game).is_empty(), "{traces:?}");
}

#[test]
fn saving_before_the_activation_event_runs_reaches_the_same_end_state_after_load() {
    let make = game;
    let mut game = make();
    game.start();
    let quest = game.form("MQ01").unwrap();
    let lever = game.form("LeverRef").unwrap();
    let gold = game.form("Gold001").unwrap();
    let player = game.player();

    assert!(game.activate_no_settle(lever, player));
    assert!(
        !game.world().pending.is_empty(),
        "the OnActivate event is queued, not yet run"
    );
    let bytes = game.save_state().unwrap();

    let mut loaded = make();
    loaded.load_state(&bytes).unwrap();
    assert!(loaded.run_until_idle(64));

    {
        let mut w = loaded.world_mut();
        let q = w.quest_state(quest).unwrap().clone();
        assert_eq!(q.highest, 20, "{q:?}");
        assert!(q.done.contains(&10) && q.done.contains(&20));
        assert!(q.completed && !q.running, "stage 20 completes the quest");
        assert!(w.objective(quest, 10).unwrap().completed);
        assert_eq!(
            w.item_count(player, gold),
            5,
            "the fragment gave the player gold"
        );
        assert!(
            !w.reference(lever).unwrap().enabled,
            "the lever disabled itself"
        );
        assert_eq!(w.notifications, ["Lever pulled"]);
        assert_eq!(
            w.alias_ref(quest, 0),
            Some(lever),
            "the forced alias holds the lever"
        );
    }
    let traces = loaded.traces();
    assert!(traces.contains(&"fragment 10".to_string()), "{traces:?}");
    assert!(script_errors(&loaded).is_empty(), "{traces:?}");
}

#[test]
fn doors_teleport_containers_open_and_timers_fire_on_the_clock() {
    let mut game = game();
    game.start();
    let player = game.player();
    let door_a = game.form("DoorA").unwrap();
    let chest = game.form("ChestRef").unwrap();
    let cell_b = game.form("TestCellB").unwrap();
    let gold = game.form("Gold001").unwrap();

    assert!(game.activate(door_a, player));
    {
        let mut w = game.world_mut();
        let p = w.reference(player).unwrap();
        assert_eq!(p.cell, Some(cell_b), "the far door's cell");
        assert_eq!(p.position, [100.0, 200.0, 300.0]);
        assert!((p.rotation[2] - 1.5).abs() < 1e-6);
    }

    assert!(game.activate(chest, player));
    {
        let mut w = game.world_mut();
        assert!(w.reference(chest).unwrap().opened);
        assert_eq!(w.item_count(chest, gold), 25, "CNTO seeds the container");
        assert_eq!(w.remove_item(chest, gold, 10, Some(player)), 10);
        assert_eq!(w.item_count(player, gold), 10);
    }
    game.tick(1.0);
    assert!(!game.traces().contains(&"timer fired".to_string()));
    game.tick(1.5);
    assert!(
        game.traces().contains(&"timer fired".to_string()),
        "{:?}",
        game.traces()
    );
    assert!((game.world().clock.game_hours() - 2.5 * 20.0 / 3600.0).abs() < 1e-9);
    assert!(script_errors(&game).is_empty(), "{:?}", game.traces());
}

#[test]
fn scriptless_forms_answer_natives_through_their_base_class() {
    let mut game = game();
    game.start();
    let probe = ScriptBuilder::new("Probe", "")
        .function(
            FunctionBuilder::new("Run", "String")
                .global()
                .param("q", "Quest")
                .local("id", "Int")
                .local("name", "String")
                .local("x", "Form")
                .local("out", "String")
                .ins(CallMethod, vec![s("GetFormID"), ident("q"), ident("id")])
                .ins(CallMethod, vec![s("GetName"), ident("q"), ident("name")])
                .ins(
                    CallStatic,
                    vec![s("Game"), s("GetForm"), ident("x"), i(0x801)],
                )
                .ins(CallMethod, vec![s("GetName"), ident("x"), ident("out")])
                .ins(StrCat, vec![ident("out"), ident("name"), ident("out")])
                .ins(Return, vec![ident("out")])
                .build(),
        )
        .bytes();
    game.load_script(&probe).unwrap();
    let quest = game.form("MQ01").unwrap();
    let qobj = game.object_of_form(quest);
    let tid = game
        .vm
        .call_global("Probe", "Run", &[soe_papyrus::Value::Object(qobj)])
        .unwrap();
    game.run_until_idle(8);
    assert_eq!(
        game.vm.thread_result(tid),
        Some(soe_papyrus::Value::string("The LeverLever"))
    );
    assert_eq!(game.world().runtime_id(quest), 0x0000_0800);
    assert!(script_errors(&game).is_empty(), "{:?}", game.traces());
}

#[test]
fn a_second_native_provider_loses_collisions_and_is_told() {
    let mut game = game();
    let lost = game.add_natives(|vm, _world| {
        vm.natives.register_method(
            "Quest",
            "SetStage",
            Box::new(|_| soe_papyrus::NativeReturn::Value(soe_papyrus::Value::None)),
        );
        vm.natives.register_global(
            "Game",
            "PaperOnlyThing",
            Box::new(|_| soe_papyrus::NativeReturn::Value(soe_papyrus::Value::Bool(true))),
        );
    });
    assert_eq!(
        lost,
        vec![("Quest".to_string(), "SetStage".to_string(), false)]
    );
    assert!(game.vm.natives.has_global("game", "paperonlything"));
}

#[test]
fn the_scenario_harness_runs_the_same_flow_against_a_captured_log() {
    let dir = std::env::temp_dir().join(format!(
        "soe-game-{}-{}",
        module_path!(),
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("Papyrus.0.log");
    std::fs::write(
        &log,
        "[09/14/2026 - 10:00:01AM] MQ01 init\n[09/14/2026 - 10:00:05AM] fragment 10\n",
    )
    .unwrap();
    let text = format!(
        "expect running MQ01\nexpect stage MQ01 == 0\nactivate LeverRef by player\nexpect stage MQ01 == 20\n\
         expect stage-done MQ01 10\nexpect objective MQ01 10 completed\nexpect completed MQ01\n\
         expect item player Gold001 == 5\nexpect enabled LeverRef false\nexpect notification \"Lever pulled\"\n\
         expect trace \"fragment 10\"\nexpect global TestCounter == 1\nactivate DoorA\nexpect cell player TestCellB\n\
         tick 3\nexpect traces-match {}\nexpect stage MQ01 == 999\n",
        log.display()
    );
    let scenario = Scenario::parse(&text).unwrap();
    let mut game = game();
    game.start();
    let report = harness::run(&mut game, &scenario);
    assert_eq!(report.checks, 14);
    assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
    assert!(
        report.failures[0].starts_with("line 17:"),
        "{:?}",
        report.failures
    );
    std::fs::remove_dir_all(&dir).ok();
}
