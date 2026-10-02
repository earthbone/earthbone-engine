// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-75 reference-backed condition functions and unavailable runtime contexts.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"VTCK"), 0x830_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801).sub(Tag(*b"VTCK"), 0x831_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"VTYP"), 0x830));
    plugin.push_top(Record::new(Tag(*b"VTYP"), 0x831));
    for object in [0x900, 0x901, 0x902] {
        plugin.push_top(Record::new(Tag(*b"ACHR"), object).sub(
            Tag(*b"NAME"),
            if object == 0x900 { 0x800_u32 } else { 0x801 }.to_le_bytes(),
        ));
    }
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    plugin.push_top(Record::new(Tag(*b"REFR"), 0x910).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"FLST"), 0x820));
    let mut order = LoadOrder::new();
    order.add("AI75.esm", plugin).unwrap();
    let game = Game::new(order);
    game.world_mut()
        .form_list(id(0x820))
        .unwrap()
        .push(id(0x800));
    game
}

fn condition(function: u16, parameter: u32, expected: f32) -> Condition {
    Condition {
        flags: 0,
        comparison: Comparison::Number(expected),
        function,
        parameters: [parameter, 0],
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
            owner: id(0x800),
            subject,
            target: None,
            quest: None,
            tempering_enchanted: None,

            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

#[test]
fn ai_75_list_exists_voice_and_object_type_match_positive_and_negative_fixtures() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(372, 0x820, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x910), &condition(372, 0x820, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(372, 0x820, 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(414, 0x900, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(426, 0x830, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x901), &condition(426, 0x830, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(432, 43, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(432, 41, 1.0)),
        Ok(false)
    );
    game.world_mut().reference(id(0x901)).unwrap().deleted = true;
    assert_eq!(
        evaluate(&game, id(0x901), &condition(414, 0x901, 1.0)),
        Ok(false)
    );
}

#[test]
fn ai_75_functions_reject_wrong_forms_and_invalid_parameters() {
    let game = fixture();
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(372, 0x810, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(414, 0, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(426, 0x820, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(432, u32::MAX, 1.0)),
        Err(conditions::Error::Parameter(432, u32::MAX))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(391, 1, 1.0)),
        Err(conditions::Error::Parameter(391, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(415, 1, 1.0)),
        Err(conditions::Error::Parameter(415, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(416, 1, 1.0)),
        Err(conditions::Error::Parameter(416, 1))
    );
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(463, 0, 1.0)),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_75_first_person_condition_tracks_host_pov_and_rejects_parameters() {
    let game = fixture();
    assert_eq!(
        evaluate(&game, id(0x900), &condition(391, 0, 1.0)),
        Ok(true)
    );
    game.world_mut().set_player_first_person(false);
    assert_eq!(
        evaluate(&game, id(0x900), &condition(391, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(391, 0, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(391, 1, 1.0)),
        Err(conditions::Error::Parameter(391, 1))
    );
}

#[test]
fn ai_75_combat_group_conditions_count_connected_members_and_targets() {
    let game = fixture();
    game.world_mut().actor(id(0x900)).unwrap().combat_target = Some(id(0x901));
    game.world_mut().actor(id(0x901)).unwrap().combat_target = Some(id(0x902));

    assert_eq!(
        evaluate(&game, id(0x900), &condition(415, 0, 2.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(415, 0, 3.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(416, 0, 2.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(416, 0, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(0x902), &condition(415, 0, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(415, 1, 2.0)),
        Err(conditions::Error::Parameter(415, 1))
    );
    assert!(matches!(
        evaluate(&game, id(0x910), &condition(415, 0, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert!(matches!(
        evaluate(&game, id(0x910), &condition(416, 0, 1.0)),
        Err(conditions::Error::Form(_))
    ));
}

#[test]
fn ai_75_grabbed_reference_condition_matches_current_host_reference() {
    let game = fixture();
    game.world_mut().set_grabbed_reference(Some(id(0x910)));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(463, 0x910, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(463, 0x900, 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(463, 0x900, 1.0)),
        Ok(false)
    );
    assert!(matches!(
        evaluate(&game, id(0x900), &condition(463, 0, 1.0)),
        Err(conditions::Error::Form(_))
    ));
    assert_eq!(
        evaluate(
            &game,
            id(0x900),
            &Condition {
                parameters: [0x910, 1],
                ..condition(463, 0x910, 1.0)
            }
        ),
        Err(conditions::Error::Parameter(463, 1))
    );
}
