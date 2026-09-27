// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x900;
const PLAYER_BASE: u32 = 7;
const SCENE: u32 = 0x800;
const QUEST: u32 = 0x810;
const TOPIC: u32 = 0x820;
const INFO: u32 = 0x830;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, parameters: [u32; 2], expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters,
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(ACTOR),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
        },
        c,
    )
}

fn game() -> Game {
    let scene = Record::new(T(b"SCEN"), SCENE)
        .sub(T(b"HNAM"), [])
        .zsub(T(b"NAM0"), "Phase")
        .sub(T(b"NEXT"), [])
        .sub(T(b"HNAM"), [])
        .sub(T(b"ANAM"), 0u16.to_le_bytes())
        .zsub(T(b"NAM0"), "Action")
        .sub(T(b"ALID"), 1i32.to_le_bytes())
        .sub(T(b"INAM"), 1u32.to_le_bytes())
        .sub(T(b"SNAM"), 0u32.to_le_bytes())
        .sub(T(b"ENAM"), 0u32.to_le_bytes())
        .sub(T(b"DATA"), 0x900u32.to_le_bytes())
        .sub(T(b"ANAM"), []);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), PLAYER_BASE).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(Record::new(T(b"NPC_"), 0x901).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x901u32.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(scene);
    let mut quest_data = vec![0; 12];
    quest_data[2] = 50;
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), quest_data)
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0, 0, 0, 0]),
    );
    let topic = Record::new(T(b"DIAL"), TOPIC)
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"BNAM"), 0x920u32.to_le_bytes())
        .sub(T(b"DATA"), [0; 4])
        .sub(T(b"SNAM"), *b"CUST")
        .sub(T(b"PNAM"), 50.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"DLBR"), 0x921)
            .sub(T(b"QNAM"), QUEST.to_le_bytes())
            .sub(T(b"TNAM"), 0u32.to_le_bytes())
            .sub(T(b"DNAM"), 1u32.to_le_bytes())
            .sub(T(b"SNAM"), TOPIC.to_le_bytes()),
    );
    let mut trdt = vec![0; 24];
    trdt[0..4].copy_from_slice(&5u32.to_le_bytes());
    trdt[4..8].copy_from_slice(&73u32.to_le_bytes());
    let info = Record::new(T(b"INFO"), INFO)
        .sub(T(b"ENAM"), [0; 4])
        .sub(T(b"TRDT"), trdt)
        .zsub(T(b"NAM1"), "Hello");
    let mut dial_group = Group::new(GroupKind::Top(T(b"DIAL")));
    dial_group
        .entries
        .push(Entry::Record(plugin.push_record(topic)));
    let mut child = Group::new(GroupKind::TopicChildren(TOPIC));
    child.entries.push(Entry::Record(plugin.push_record(info)));
    dial_group.entries.push(Entry::Group(child));
    plugin.groups.push(dial_group);
    let mut order = LoadOrder::new();
    order.add("AI48.esp", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ai_48_implemented_dialogue_and_scene_predicates_match_runtime_state() {
    let mut game = game();
    let actor = id(ACTOR);
    let scene = condition(550, [SCENE, 0], 1.0);
    assert!(conditions::supports_function(550));
    assert_eq!(
        evaluate(&game, actor, &condition(550, [SCENE, 0], 0.0)),
        Ok(true)
    );
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    assert!(game.world_mut().start_scene(id(SCENE), &mut alloc));
    assert!(game.world_mut().complete_scene_action(id(SCENE), 0));
    assert_eq!(evaluate(&game, actor, &scene), Ok(true));
    assert_eq!(
        evaluate(&game, actor, &condition(550, [SCENE, 0], 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, actor, &condition(550, [SCENE, 1], 1.0)),
        Err(conditions::Error::Parameter(550, 1))
    );
    assert_eq!(
        evaluate(&game, actor, &condition(550, [0xfff, 0], 1.0)),
        Err(conditions::Error::Form(id(0xfff)))
    );

    assert!(conditions::supports_function(635));
    assert_eq!(
        evaluate(&game, actor, &condition(635, [0, 0], 0.0)),
        Ok(true)
    );
    game.world_mut().actor(actor).unwrap().doing_favor = true;
    assert_eq!(
        evaluate(&game, actor, &condition(635, [0, 0], 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, actor, &condition(635, [0, 0], 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, actor, &condition(635, [1, 0], 0.0)),
        Err(conditions::Error::Parameter(635, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x800), &condition(635, [0, 0], 0.0)),
        Err(conditions::Error::Form(id(0x800)))
    );

    game.start_quest(id(QUEST));
    let player = game.player();
    game.begin_dialogue(actor, player).unwrap();
    assert!(game
        .dialogue_choices()
        .unwrap()
        .available
        .iter()
        .any(|c| c.topic == id(TOPIC)));
    game.choose_dialogue(id(TOPIC)).unwrap();
    assert!(conditions::supports_function(434));
    assert!(conditions::supports_function(435));
    assert_eq!(
        evaluate(&game, actor, &condition(434, [0, 0], 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, actor, &condition(434, [0, 0], 5.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, actor, &condition(434, [0, 0], 4.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, actor, &condition(435, [0, 0], 73.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, actor, &condition(435, [0, 0], 74.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, actor, &condition(435, [1, 0], 73.0)),
        Err(conditions::Error::Parameter(435, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x800), &condition(434, [0, 0], 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x800), &condition(435, [0, 0], 0.0)),
        Ok(true)
    );
}

#[test]
fn ai_48_functions_without_a_usable_local_contract_report_diagnostics() {
    let game = game();
    for function in [320, 370, 429, 590] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(ACTOR), &condition(function, [0, 0], 0.0)),
            Err(conditions::Error::Function(function)),
            "function {function}"
        );
        assert_eq!(
            evaluate(&game, id(0x800), &condition(function, [0, 0], 0.0)),
            Err(conditions::Error::Function(function)),
            "invalid context for function {function}"
        );
    }
}
