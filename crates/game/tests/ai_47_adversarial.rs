// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag,
    records::gameplay::{Comparison, Condition},
};
use soe_game::{Game, conditions};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"SCEN"), 0xA00));
    let mut order = LoadOrder::new();
    order.add("AI47Adversarial.esp", plugin).unwrap();
    Game::new(order)
}

fn automatic_greeting_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), 7)
            .zsub(Tag(*b"EDID"), "Player")
            .sub(Tag(*b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), 0x500)
            .zsub(Tag(*b"EDID"), "Greeter")
            .sub(Tag(*b"ACBS"), vec![0; 24]),
    );
    let mut placement = vec![0; 24];
    placement[..4].copy_from_slice(&20.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 0x100)
            .sub(Tag(*b"NAME"), 0x500u32.to_le_bytes())
            .sub(Tag(*b"DATA"), placement),
    );
    let mut quest_data = vec![0; 12];
    quest_data[2] = 50;
    plugin.push_top(
        Record::new(Tag(*b"QUST"), 0x800)
            .sub(Tag(*b"DNAM"), quest_data)
            .sub(Tag(*b"NEXT"), [])
            .sub(Tag(*b"INDX"), [0; 4]),
    );
    let mut topic_data = [0; 4];
    topic_data[2..4].copy_from_slice(&1u16.to_le_bytes());
    let topic = Record::new(Tag(*b"DIAL"), 0x900)
        .zsub(Tag(*b"FULL"), "Hello")
        .sub(Tag(*b"QNAM"), 0x800u32.to_le_bytes())
        .sub(Tag(*b"BNAM"), 0x1900u32.to_le_bytes())
        .sub(Tag(*b"DATA"), topic_data)
        .sub(Tag(*b"SNAM"), *b"PFGT")
        .sub(Tag(*b"PNAM"), 50.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"DLBR"), 0x1900)
            .sub(Tag(*b"QNAM"), 0x800u32.to_le_bytes())
            .sub(Tag(*b"TNAM"), 0u32.to_le_bytes())
            .sub(Tag(*b"DNAM"), 1u32.to_le_bytes())
            .sub(Tag(*b"SNAM"), 0x900u32.to_le_bytes()),
    );
    let mut condition = vec![0; 32];
    condition[4..8].copy_from_slice(&1.0f32.to_le_bytes());
    condition[8..10].copy_from_slice(&123u16.to_le_bytes());
    let mut response = vec![0; 24];
    response[12] = 1;
    let info = Record::new(Tag(*b"INFO"), 0xA00)
        .sub(Tag(*b"ENAM"), [0; 4])
        .sub(Tag(*b"CTDA"), condition)
        .sub(Tag(*b"TRDT"), response)
        .zsub(Tag(*b"NAM1"), "Welcome");
    let mut topic_group = Group::new(GroupKind::Top(Tag(*b"DIAL")));
    topic_group
        .entries
        .push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(0x900));
    children
        .entries
        .push(Entry::Record(plugin.push_record(info)));
    topic_group.entries.push(Entry::Group(children));
    plugin.groups.push(topic_group);

    let mut order = LoadOrder::new();
    order.add("AI47Greeting.esp", plugin).unwrap();
    let mut game = Game::new(order);
    game.player();
    game.world_mut().drive_actor(id(0x100));
    game.start_quest(id(0x800));
    game
}
fn c(function: u16, params: [u32; 2], expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: params,
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    }
}
fn eval(
    game: &Game,
    subject: GlobalFormId,
    condition: &Condition,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(0x800),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,
            skill_use: None,
            light_level: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
        },
        condition,
    )
}

#[test]
fn ai_47_unsupported_talk_counters_and_talking_predicate_fail_explicitly() {
    let game = game();
    for function in [50, 141, 172] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            eval(&game, id(0x900), &c(function, [0, 0], 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}

#[test]
fn ai_47_greeting_requires_the_same_actor_and_no_parameter() {
    let game = game();
    assert_eq!(eval(&game, id(0x900), &c(123, [0, 0], 1.0)), Ok(false));
    assert_eq!(eval(&game, id(0x800), &c(123, [0, 0], 1.0)), Ok(false));
    assert_eq!(
        eval(&game, id(0x900), &c(123, [1, 0], 1.0)),
        Err(conditions::Error::Parameter(123, 1))
    );
}

#[test]
fn ai_47_is_greeting_player_allows_automatic_force_greet_selection() {
    let mut game = automatic_greeting_game();
    game.tick(0.016);
    assert_eq!(
        game.world().dialogue_session().unwrap().current_info,
        Some(id(0xA00))
    );
}

#[test]
fn ai_47_persuasion_functions_use_current_speechcraft_and_reject_parameters() {
    let game = game();
    game.world_mut()
        .set_actor_value(id(0x900), "speechcraft", 37.0)
        .unwrap();
    for function in [225, 315] {
        assert_eq!(eval(&game, id(0x900), &c(function, [0, 0], 37.0)), Ok(true));
        assert_eq!(
            eval(&game, id(0x900), &c(function, [1, 0], 37.0)),
            Err(conditions::Error::Parameter(function, 1))
        );
        assert_eq!(
            eval(&game, id(0x800), &c(function, [0, 0], 0.0)),
            Err(conditions::Error::Form(id(0x800)))
        );
    }
}

#[test]
fn ai_47_scene_and_dialogue_require_valid_identity_and_active_player_pair() {
    let mut game = game();
    assert_eq!(eval(&game, id(0x900), &c(248, [0xA00, 0], 1.0)), Ok(false));
    game.world_mut().scenes.insert(id(0xA00));
    assert_eq!(eval(&game, id(0x900), &c(248, [0xA00, 0], 1.0)), Ok(true));
    assert_eq!(
        eval(&game, id(0x900), &c(248, [0xA00, 1], 1.0)),
        Err(conditions::Error::Parameter(248, 1))
    );
    let player = game.world_mut().player();
    game.begin_dialogue(id(0x900), player).unwrap();
    assert_eq!(eval(&game, id(0x900), &c(249, [0, 0], 1.0)), Ok(true));
    game.close_dialogue();
    assert_eq!(eval(&game, id(0x900), &c(249, [0, 0], 1.0)), Ok(false));
}
