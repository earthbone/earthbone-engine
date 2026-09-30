// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for crime conditions and their persisted inputs.

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
    plugin.push_top(Record::new(Tag(*b"FACT"), 0x801).sub(Tag(*b"DATA"), 0x40_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x802));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x802_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("CrimeAdversarial.esm", plugin).unwrap();
    Game::new(order)
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
            skill_use: None,
            light_level: None,
        },
        c,
    )
}

#[test]
fn witnessed_crime_and_jail_days_survive_engine_save_load() {
    let game = fixture();
    let player = game.world_mut().player();
    {
        let mut world = game.world_mut();
        world
            .actor(id(0x900))
            .unwrap()
            .witnessed_crimes
            .insert(player);
        world.actor(player).unwrap().days_in_jail = 6;
    }
    let bytes = game.save_state().unwrap();
    let mut restored = fixture();
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        evaluate(&restored, id(0x900), &condition(458, 0, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&restored, id(0x900), &condition(499, 0, 6.0)),
        Ok(true)
    );
    restored
        .world_mut()
        .actor(id(0x900))
        .unwrap()
        .witnessed_crimes
        .clear();
    restored.world_mut().actor(player).unwrap().days_in_jail = 0;
    assert_eq!(
        evaluate(&restored, id(0x900), &condition(458, 0, 1.0)),
        Ok(false)
    );
    assert_eq!(
        evaluate(&restored, id(0x900), &condition(499, 0, 0.0)),
        Ok(true)
    );
}

#[test]
fn crime_conditions_reject_non_actor_subjects_and_missing_crime_factions() {
    let game = fixture();
    for function in [458, 459, 497, 533, 534, 535, 652] {
        let result = evaluate(&game, id(0x800), &condition(function, 0, 1.0));
        assert!(
            matches!(result, Err(conditions::Error::Form(_))),
            "{function}: {result:?}"
        );
    }
    game.world_mut().actor(id(0x900)).unwrap().crime_faction = None;
    for function in [459, 497, 533, 534, 535] {
        assert!(matches!(
            evaluate(&game, id(0x900), &condition(function, 0, 1.0)),
            Err(conditions::Error::Form(_))
        ));
    }
}
