// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::needless_pass_by_value
)]

//! AI-63 motion CTDA functions, using xEdit function identities, CK behavior
//! pages and the runtime race and furniture state.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const OWNER: u32 = 0x800;
const RACE: u32 = 0x810;
const MOVEMENT: u32 = 0x811;
const ACTOR_BASE: u32 = 0x820;
const ACTOR: u32 = 0x900;
const FURNITURE_BASE: u32 = 0x830;
const FURNITURE: u32 = 0x910;
const OTHER_FURNITURE: u32 = 0x911;
const OTHER_FURNITURE_REF: u32 = 0x912;
const TARGET: u32 = 0x913;

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

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"STAT"), OWNER));
    let mut walk = [0; 24];
    walk[16..20].copy_from_slice(&72.0_f32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MOVT"), MOVEMENT).sub(T(b"SPED"), walk));
    plugin.push_top(Record::new(T(b"RACE"), RACE).sub(T(b"WKMV"), MOVEMENT.to_le_bytes()));
    plugin.push_top(Record::new(T(b"NPC_"), ACTOR_BASE).sub(T(b"RNAM"), RACE.to_le_bytes()));
    plugin.push_top(Record::new(T(b"ACHR"), ACTOR).sub(T(b"NAME"), ACTOR_BASE.to_le_bytes()));
    let mut furniture =
        Record::new(T(b"FURN"), FURNITURE_BASE).sub(T(b"MNAM"), 3_u32.to_le_bytes());
    for marker_type in [1_u16, 2_u16] {
        furniture = furniture.sub(
            T(b"FNPR"),
            [marker_type as u8, (marker_type >> 8) as u8, 0, 0],
        );
    }
    plugin.push_top(furniture);
    plugin
        .push_top(Record::new(T(b"REFR"), FURNITURE).sub(T(b"NAME"), FURNITURE_BASE.to_le_bytes()));
    plugin.push_top(Record::new(T(b"FURN"), OTHER_FURNITURE).sub(T(b"MNAM"), 1_u32.to_le_bytes()));
    plugin.push_top(Record::new(T(b"STAT"), 0x831));
    plugin.push_top(
        Record::new(T(b"REFR"), OTHER_FURNITURE_REF).sub(T(b"NAME"), OTHER_FURNITURE.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"REFR"), TARGET).sub(T(b"NAME"), 0x831_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("AI63.esm", plugin).unwrap();
    Game::new(order)
}

fn evaluate(
    game: &Game,
    subject: GlobalFormId,
    value: Condition,
) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: id(OWNER),
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
        &value,
    )
}

#[test]
fn ai_63_heading_angle_uses_reference_heading_and_reports_invalid_targets() {
    let game = fixture();
    game.world_mut().reference(id(ACTOR)).unwrap().rotation[2] = 0.0;
    let target = id(TARGET);
    game.world_mut().reference(target).unwrap().position = [10.0, 10.0, 0.0];
    assert!(conditions::supports_function(99));
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(99, [TARGET, 0], -45.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(99, [TARGET, 0], -46.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(99, [0xfff, 0], 0.0)),
        Err(conditions::Error::Form(id(0xfff)))
    );
}

#[test]
fn ai_63_walk_speed_reads_race_movement_and_rejects_nonactors() {
    let game = fixture();
    assert!(conditions::supports_function(142));
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(142, [0, 0], 72.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(142, [0, 0], 71.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(FURNITURE), condition(142, [0, 0], 72.0)),
        Err(conditions::Error::Form(id(FURNITURE)))
    );
}

#[test]
fn ai_63_furniture_conditions_follow_reserved_marker_identity_and_kind() {
    let mut game = fixture();
    assert!(conditions::supports_function(159));
    assert!(conditions::supports_function(162));
    assert!(conditions::supports_function(163));
    assert!(conditions::supports_function(175));
    {
        let mut world = game.world_mut();
        world.activate_furniture(id(FURNITURE), id(ACTOR));
    }
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(159, [0, 0], 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(159, [0, 0], 0.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(162, [0x910, 0], 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(
            &game,
            id(ACTOR),
            condition(162, [OTHER_FURNITURE_REF, 0], 1.0)
        ),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(163, [0x830, 0], 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(163, [0x911, 0], 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(175, [0, 0], 0.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(175, [0, 0], 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(FURNITURE), condition(159, [0, 0], 0.0)),
        Err(conditions::Error::Form(id(FURNITURE)))
    );
    assert_eq!(
        evaluate(&game, id(FURNITURE), condition(163, [0x830, 0], 0.0)),
        Err(conditions::Error::Form(id(FURNITURE)))
    );
    assert_eq!(
        evaluate(&game, id(FURNITURE), condition(162, [0x910, 0], 0.0)),
        Err(conditions::Error::Form(id(FURNITURE)))
    );

    let player = game.player();
    game.world_mut().actor(id(ACTOR)).unwrap().furniture_seat = Some((id(FURNITURE), 1));
    game.world_mut().actor(player).unwrap().furniture_seat = Some((id(FURNITURE), 1));
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(159, [0, 0], 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(175, [0, 0], 1.0)),
        Ok(true)
    );
}

#[test]
fn ai_63_unavailable_idle_and_weather_contracts_return_typed_diagnostics() {
    let game = fixture();
    for function in [112, 147] {
        assert!(!conditions::supports_function(function));
        assert_eq!(
            evaluate(&game, id(ACTOR), condition(function, [0, 0], 0.0)),
            Err(conditions::Error::Function(function))
        );
    }
}

#[test]
fn ai_63_heading_angle_counterclockwise_is_negative_per_ck() {
    let game = fixture();
    game.world_mut().reference(id(ACTOR)).unwrap().rotation[2] = 0.0;
    game.world_mut().reference(id(TARGET)).unwrap().position = [0.0, 10.0, 0.0];
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(99, [TARGET, 0], -90.0)),
        Ok(true),
        "CK GetHeadingAngle defines counterclockwise as negative"
    );
    assert_eq!(
        evaluate(&game, id(ACTOR), condition(99, [TARGET, 0], 90.0)),
        Ok(false)
    );
}
