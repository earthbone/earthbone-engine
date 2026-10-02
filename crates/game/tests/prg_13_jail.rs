// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRG-13 sentence accounting over the shared world and actor save state.

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FACTION: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x701,
};

fn jail_game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"FACT"), FACTION.object)
            .zsub(T(b"EDID"), "JailFaction")
            .sub(T(b"DATA"), 0x40_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    Game::new(order)
}

#[test]
fn prg_13_jail_advances_sentence_and_applies_progress_penalty() {
    let game = jail_game();
    let mut world = game.world_mut();
    let player = world.player();
    world
        .actor(player)
        .unwrap()
        .skill_experience
        .insert("alchemy".to_owned(), 17.0);
    {
        let faction = world.faction(FACTION).unwrap();
        faction.crime_gold_violent = 250;
        faction.crime_gold_nonviolent = 1;
    }
    let before = world.clock.game_days;

    let served = world.serve_jail_time(FACTION).unwrap();

    assert_eq!(served, 3, "the sentence rounds up by 100 bounty gold");
    assert_eq!(
        world.clock.game_days - before,
        3.0,
        "serving advances three game days"
    );
    assert!(
        world.actor(player).unwrap().skill_experience.is_empty(),
        "serving clears partial skill progress"
    );
    let faction = world.faction(FACTION).unwrap();
    assert_eq!(
        faction.crime_gold_violent, 0,
        "served time clears violent bounty"
    );
    assert_eq!(
        faction.crime_gold_nonviolent, 0,
        "served time clears nonviolent bounty"
    );
}

#[test]
fn prg_13_jail_caps_a_long_sentence_at_seven_days() {
    let game = jail_game();
    let mut world = game.world_mut();
    let player = world.player();
    world.actor(player).unwrap();
    world.faction(FACTION).unwrap().crime_gold_violent = 50_000;

    let served = world.serve_jail_time(FACTION).unwrap();

    assert_eq!(served, 7, "a sentence cannot exceed seven game days");
    assert_eq!(
        world.clock.game_days, 7.0,
        "the capped sentence advances seven game days"
    );
}

#[test]
fn prg_13_jail_days_persist_in_the_actor_section() {
    let game = jail_game();
    let save = {
        let mut world = game.world_mut();
        let player = world.player();
        world.actor(player).unwrap().days_in_jail = 4;
        world.save_state()
    };
    let restored = jail_game();
    let mut world = restored.world_mut();
    let player = world.player();
    world.load_state(&save).unwrap();

    assert_eq!(
        world.actor(player).unwrap().days_in_jail,
        4,
        "the PRG-13 actor section restores served days"
    );
    world.clear_prison();
    assert_eq!(
        world.actor(player).unwrap().days_in_jail,
        0,
        "ClearPrison clears the served-day counter"
    );
}
