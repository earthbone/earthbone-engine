// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-45 crime-related CTDA conditions use witnessed crime, faction ledgers,
//! player jail accounting and actor crime factions.

use soe_esm::{
    records::gameplay::{Comparison, Condition},
    FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag,
};
use soe_game::{conditions, Game};

const OWNER: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x800,
};
const FACTION: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x701,
};
const OTHER_FACTION: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x702,
};
const ACTOR: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x900,
};
const OTHER_ACTOR: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x901,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
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

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), OWNER.object));
    for (object, name) in [(0x701, "CrimeFaction"), (0x702, "OtherCrimeFaction")] {
        plugin.push_top(
            Record::new(Tag(*b"FACT"), object)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), 0x40_u32.to_le_bytes()),
        );
    }
    plugin.push_top(Record::new(Tag(*b"MISC"), 0x710).zsub(Tag(*b"EDID"), "Gold001"));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR.object).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), OTHER_ACTOR.object).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("CrimeConditions.esm", plugin).unwrap();
    let game = Game::new(order);
    {
        let mut world = game.world_mut();
        world.player();
        world.actor(ACTOR).unwrap().crime_faction = Some(FACTION);
        world.actor(OTHER_ACTOR).unwrap().crime_faction = Some(FACTION);
        let player = world.player();
        world.actor(player).unwrap();
        world.faction(FACTION).unwrap().crime_gold_violent = 300;
        world.faction(FACTION).unwrap().crime_gold_nonviolent = 200;
        world.faction(FACTION).unwrap().infamy_violent = 700;
        world.faction(FACTION).unwrap().infamy_nonviolent = 300;
        world.faction(OTHER_FACTION).unwrap();
        world.add_item(player, id(0x710), 500);
        world.actor(player).unwrap().days_in_jail = 4;
        world.actor(ACTOR).unwrap().witnessed_crimes.insert(player);
    }
    game
}

fn evaluate(game: &Game, subject: GlobalFormId, c: &Condition) -> Result<bool, conditions::Error> {
    conditions::evaluate_one(
        &mut game.world_mut(),
        &game.vm,
        conditions::Context {
            owner: OWNER,
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
fn ai_45_witnessed_crime_and_shared_faction_conditions_match_actor_state() {
    let game = fixture();
    for function in [458, 652] {
        assert!(conditions::supports_function(function));
    }
    assert_eq!(evaluate(&game, ACTOR, &condition(458, 0, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, ACTOR, &condition(458, 0, 0.0)), Ok(false));
    assert_eq!(
        evaluate(&game, ACTOR, &condition(652, OTHER_ACTOR.object, 1.0)),
        Ok(true)
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(652, OTHER_ACTOR.object, 0.0)),
        Ok(false)
    );

    let player = game.world_mut().player();
    game.world_mut()
        .actor(ACTOR)
        .unwrap()
        .witnessed_crimes
        .remove(&player);
    assert_eq!(evaluate(&game, ACTOR, &condition(458, 0, 1.0)), Ok(false));
    game.world_mut().actor(OTHER_ACTOR).unwrap().crime_faction = Some(OTHER_FACTION);
    assert_eq!(
        evaluate(&game, ACTOR, &condition(652, OTHER_ACTOR.object, 1.0)),
        Ok(false)
    );

    for (function, parameter) in [(458, 1), (652, 0x702)] {
        let expected = if function == 652 {
            Err(conditions::Error::Form(OTHER_FACTION))
        } else {
            Err(conditions::Error::Parameter(function, parameter))
        };
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, parameter, 0.0)),
            expected
        );
    }
    assert_eq!(
        evaluate(
            &game,
            ACTOR,
            &condition(652, OTHER_ACTOR.object, 0.0).with_second_parameter(1)
        ),
        Err(conditions::Error::Parameter(652, 1))
    );
}

#[test]
fn ai_45_faction_bounty_and_infamy_conditions_use_nullable_faction_and_split_ledgers() {
    let game = fixture();
    for function in [459, 533, 534, 535] {
        assert!(conditions::supports_function(function));
    }
    for (function, expected) in [(459, 500.0), (533, 1000.0), (534, 700.0), (535, 300.0)] {
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, FACTION.object, expected)),
            Ok(true)
        );
        assert_eq!(
            evaluate(
                &game,
                ACTOR,
                &condition(function, FACTION.object, expected + 1.0)
            ),
            Ok(false)
        );
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0, expected)),
            Ok(true)
        );
    }
    assert_eq!(
        evaluate(&game, ACTOR, &condition(459, OTHER_FACTION.object, 0.0)),
        Ok(true)
    );
    for function in [459, 533, 534, 535] {
        assert_eq!(
            evaluate(&game, ACTOR, &condition(function, 0x710, 0.0)),
            Err(conditions::Error::Form(id(0x710))),
            "{function} rejects a non-faction form"
        );
        assert_eq!(
            evaluate(&game, OTHER_FACTION, &condition(function, 0, 0.0)),
            Err(conditions::Error::Form(OTHER_FACTION)),
            "{function} needs an actor for its null-faction fallback"
        );
        assert_eq!(
            evaluate(
                &game,
                ACTOR,
                &condition(function, FACTION.object, 0.0).with_second_parameter(1)
            ),
            Err(conditions::Error::Parameter(function, 1)),
            "{function} rejects its unused second parameter"
        );
    }
}

#[test]
fn ai_45_can_pay_and_days_in_jail_read_player_accounting() {
    let game = fixture();
    for function in [497, 499] {
        assert!(conditions::supports_function(function));
    }
    assert_eq!(evaluate(&game, ACTOR, &condition(497, 0, 1.0)), Ok(true));
    assert_eq!(evaluate(&game, ACTOR, &condition(497, 0, 0.0)), Ok(false));
    assert_eq!(evaluate(&game, ACTOR, &condition(499, 0, 4.0)), Ok(true));
    assert_eq!(evaluate(&game, ACTOR, &condition(499, 0, 5.0)), Ok(false));

    let player = game.world_mut().player();
    game.world_mut().remove_item(player, id(0x710), 1, None);
    assert_eq!(evaluate(&game, ACTOR, &condition(497, 0, 1.0)), Ok(false));
    game.world_mut().actor(player).unwrap().days_in_jail = 0;
    assert_eq!(evaluate(&game, ACTOR, &condition(499, 0, 0.0)), Ok(true));

    assert_eq!(
        evaluate(&game, ACTOR, &condition(497, 1, 0.0)),
        Err(conditions::Error::Parameter(497, 1))
    );
    assert_eq!(
        evaluate(&game, OTHER_FACTION, &condition(497, 0, 0.0)),
        Err(conditions::Error::Form(OTHER_FACTION))
    );
    assert_eq!(
        evaluate(&game, ACTOR, &condition(499, 1, 0.0)),
        Err(conditions::Error::Parameter(499, 1))
    );
}

trait SecondParameter {
    fn with_second_parameter(self, value: u32) -> Self;
}

impl SecondParameter for Condition {
    fn with_second_parameter(mut self, value: u32) -> Self {
        self.parameters[1] = value;
        self
    }
}
