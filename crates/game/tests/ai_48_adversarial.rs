// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}
fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800u32.to_le_bytes()));
    let scene = Record::new(Tag(*b"SCEN"), 0xA00)
        .sub(Tag(*b"HNAM"), [])
        .zsub(Tag(*b"NAM0"), "Phase")
        .sub(Tag(*b"NEXT"), [])
        .sub(Tag(*b"HNAM"), [])
        .sub(Tag(*b"ANAM"), 0u16.to_le_bytes())
        .zsub(Tag(*b"NAM0"), "Action")
        .sub(Tag(*b"ALID"), 1i32.to_le_bytes())
        .sub(Tag(*b"INAM"), 1u32.to_le_bytes())
        .sub(Tag(*b"SNAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"ENAM"), 0u32.to_le_bytes())
        .sub(Tag(*b"DATA"), 0x900u32.to_le_bytes())
        .sub(Tag(*b"ANAM"), []);
    plugin.push_top(scene);
    let topic = Record::new(Tag(*b"DIAL"), 0xA10)
        .sub(Tag(*b"QNAM"), 0xA20u32.to_le_bytes())
        .sub(Tag(*b"BNAM"), 0xA30u32.to_le_bytes())
        .sub(Tag(*b"DATA"), [0; 4])
        .sub(Tag(*b"SNAM"), *b"CUST")
        .sub(Tag(*b"PNAM"), 50.0f32.to_le_bytes());
    let mut trdt = vec![0; 24];
    trdt[0..4].copy_from_slice(&5u32.to_le_bytes());
    trdt[4..8].copy_from_slice(&73u32.to_le_bytes());
    let info = Record::new(Tag(*b"INFO"), 0xA11)
        .sub(Tag(*b"ENAM"), [0; 4])
        .sub(Tag(*b"TRDT"), trdt)
        .zsub(Tag(*b"NAM1"), "Response");
    let mut dial_group = Group::new(GroupKind::Top(Tag(*b"DIAL")));
    dial_group
        .entries
        .push(Entry::Record(plugin.push_record(topic)));
    let mut child = Group::new(GroupKind::TopicChildren(0xA10));
    child.entries.push(Entry::Record(plugin.push_record(info)));
    dial_group.entries.push(Entry::Group(child));
    plugin.groups.push(dial_group);
    plugin.push_top(
        Record::new(Tag(*b"QUST"), 0xA20)
            .sub(Tag(*b"DNAM"), [0; 12])
            .sub(Tag(*b"NEXT"), [])
            .sub(Tag(*b"INDX"), [0; 4]),
    );
    let mut order = LoadOrder::new();
    order.add("AI48Adversarial.esp", plugin).unwrap();
    Game::new(order)
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
            owner: id(0x900),
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
fn ai_48_unknown_contract_conditions_are_diagnostics_not_default_false() {
    let game = fixture();
    for function in [320, 370, 429, 590] {
        assert_eq!(
            eval(&game, id(0x900), &c(function, [0, 0], 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}

#[test]
fn ai_48_favor_requires_actor_context_and_boolean_parameter_shape() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().doing_favor = true;
    assert_eq!(eval(&game, id(0x900), &c(635, [0, 0], 1.0)), Ok(true));
    assert_eq!(
        eval(&game, id(0x801), &c(635, [0, 0], 0.0)),
        Err(conditions::Error::Form(id(0x801)))
    );
    assert_eq!(
        eval(&game, id(0x900), &c(635, [0, 1], 0.0)),
        Err(conditions::Error::Parameter(635, 1))
    );
}

#[test]
fn ai_48_scene_action_slot_must_exist_and_completed_state_is_slot_specific() {
    let game = fixture();
    let mut alloc = |_: &str, _: &str| soe_papyrus::ObjectId(1);
    assert!(game.world_mut().start_scene(id(0xA00), &mut alloc));
    assert_eq!(eval(&game, id(0x900), &c(550, [0xA00, 0], 1.0)), Ok(false));
    assert_eq!(
        eval(&game, id(0x900), &c(550, [0xA00, 1], 0.0)),
        Err(conditions::Error::Parameter(550, 1))
    );
    assert!(game.world_mut().complete_scene_action(id(0xA00), 0));
    assert_eq!(eval(&game, id(0x900), &c(550, [0xA00, 0], 1.0)), Ok(true));
    assert_eq!(eval(&game, id(0x900), &c(550, [0xA00, 0], 0.0)), Ok(false));
}

#[test]
fn ai_48_dialogue_emotion_requires_a_reference_subject() {
    let game = fixture();
    // Raw_Function_Data names both functions Ref.GetDialogueEmotion[Value].
    for function in [434, 435] {
        assert_eq!(
            eval(&game, id(0x800), &c(function, [0, 0], 0.0)),
            Err(conditions::Error::Form(id(0x800)))
        );
    }
}
