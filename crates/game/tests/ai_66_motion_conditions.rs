// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-66 motion conditions with shared references and explicit unsupported diagnostics.

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
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"STAT"), 0x810));
    for object in [0x910, 0x911] {
        plugin.push_top(
            Record::new(Tag(*b"REFR"), object).sub(Tag(*b"NAME"), 0x810_u32.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Ai66.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.reference(id(0x900)).unwrap();
        let near = world.reference(id(0x910)).unwrap();
        near.position = [3.0, 4.0, 0.0];
        let far = world.reference(id(0x911)).unwrap();
        far.position = [6.0, 8.0, 0.0];
    }
    game
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

fn evaluate(
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
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        },
        condition,
    )
}

#[test]
fn ai_66_within_distance_uses_reference_positions_and_inclusive_float_threshold() {
    let game = fixture();
    let exactly_five = condition(639, [0x910, 5.0_f32.to_bits()], 1.0);
    assert!(conditions::supports_function(639));
    assert_eq!(evaluate(&game, id(0x911), &exactly_five), Ok(true));

    let too_close_for_six = condition(639, [0x910, 4.99_f32.to_bits()], 1.0);
    assert_eq!(evaluate(&game, id(0x911), &too_close_for_six), Ok(false));

    let invert_match = condition(639, [0x910, 5.0_f32.to_bits()], 0.0);
    assert_eq!(evaluate(&game, id(0x911), &invert_match), Ok(false));
}

#[test]
fn ai_66_within_distance_rejects_invalid_references_and_ranges() {
    let game = fixture();
    assert_eq!(
        evaluate(
            &game,
            id(0x911),
            &condition(639, [0xffff, 5.0_f32.to_bits()], 1.0)
        ),
        Err(conditions::Error::Form(id(0xffff)))
    );
    assert_eq!(
        evaluate(
            &game,
            id(0xffff),
            &condition(639, [0x910, 5.0_f32.to_bits()], 1.0)
        ),
        Err(conditions::Error::Form(id(0xffff)))
    );
    assert_eq!(
        evaluate(
            &game,
            id(0x911),
            &condition(639, [0x910, (-1.0_f32).to_bits()], 1.0)
        ),
        Err(conditions::Error::Parameter(639, (-1.0_f32).to_bits()))
    );
    assert_eq!(
        evaluate(
            &game,
            id(0x911),
            &condition(639, [0x910, f32::NAN.to_bits()], 1.0)
        ),
        Err(conditions::Error::NonFinite)
    );
}

#[test]
fn ai_66_unavailable_motion_state_conditions_report_function_diagnostics() {
    let game = fixture();
    for function in [614, 620, 621, 622] {
        assert!(!conditions::supports_function(function), "{function}");
        assert_eq!(
            evaluate(&game, id(0x900), &condition(function, [0, 0], 0.0)),
            Err(conditions::Error::Function(function)),
            "{function}"
        );
    }
}

#[test]
fn ai_66_pathing_target_offset_reports_missing_contract_after_validating_axis_and_actor() {
    let game = fixture();
    let valid = condition(619, [2, 0], 1.0);
    assert!(conditions::supports_function(619));
    assert_eq!(
        evaluate(&game, id(0x900), &valid),
        Err(conditions::Error::Contract(619))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(619, [3, 0], 1.0)),
        Err(conditions::Error::Parameter(619, 3))
    );
    assert_eq!(
        evaluate(&game, id(0x910), &valid),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_66_movement_speed_reports_missing_contract_for_valid_actor_only() {
    let game = fixture();
    let valid = condition(623, [0, 0], 0.0);
    assert!(conditions::supports_function(623));
    assert_eq!(
        evaluate(&game, id(0x900), &valid),
        Err(conditions::Error::Contract(623))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(623, [0, 1], 0.0)),
        Err(conditions::Error::Parameter(623, 1))
    );
    assert_eq!(
        evaluate(&game, id(0x910), &valid),
        Err(conditions::Error::Form(id(0x910)))
    );
}

#[test]
fn ai_66_flying_state_matches_documented_values_and_checks_context() {
    let game = fixture();
    let hover = condition(633, [0, 0], 3.0);
    assert!(conditions::supports_function(633));
    assert_eq!(
        evaluate(&game, id(0x900), &hover),
        Err(conditions::Error::Unavailable(633))
    );

    game.world_mut().actor(id(0x900)).unwrap().flying_state = Some(3);
    assert_eq!(evaluate(&game, id(0x900), &hover), Ok(true));
    assert_eq!(
        evaluate(&game, id(0x900), &condition(633, [0, 0], 2.0)),
        Ok(false)
    );
    for state in 0..=4 {
        game.world_mut().actor(id(0x900)).unwrap().flying_state = Some(state);
        assert_eq!(
            evaluate(&game, id(0x900), &condition(633, [0, 0], f32::from(state))),
            Ok(true),
            "state {state}"
        );
    }
    assert_eq!(
        evaluate(&game, id(0x910), &hover),
        Err(conditions::Error::Form(id(0x910)))
    );

    game.world_mut().actor(id(0x900)).unwrap().flying_state = Some(5);
    assert_eq!(
        evaluate(&game, id(0x900), &hover),
        Err(conditions::Error::StateContract(633))
    );
    assert_eq!(
        evaluate(&game, id(0x900), &condition(633, [1, 0], 3.0)),
        Err(conditions::Error::Parameter(633, 1))
    );
}
