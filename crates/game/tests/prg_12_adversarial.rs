// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PRG-12 arrest eligibility and persistence checks.

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_game::crime::{ArrestChoice, ArrestResult};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FACTION: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x701,
};

fn arrest_game() -> Game {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(
        Record::new(T(b"FACT"), FACTION.object)
            .zsub(T(b"EDID"), "ArrestFaction")
            .sub(T(b"DATA"), 0x40_u32.to_le_bytes())
            .sub(T(b"CRVA"), crime_values()),
    );
    plugin.push_top(Record::new(T(b"MISC"), 0x702).zsub(T(b"EDID"), "Gold001"));
    let mut order = LoadOrder::new();
    order
        .add("Base.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().actor(player).unwrap().crime_faction = Some(FACTION);
    game
}

fn crime_values() -> [u8; 20] {
    let mut values = [0; 20];
    values[0] = 1;
    values[2..4].copy_from_slice(&250_u16.to_le_bytes());
    values
}

#[test]
fn prg_12_jail_resolution_survives_engine_save_load_without_reapplying() {
    let game = arrest_game();
    {
        let mut world = game.world_mut();
        let faction = world.faction(FACTION).unwrap();
        faction.crime_gold_violent = 250;
        faction.crime_gold_nonviolent = 50;
    }

    assert_eq!(
        game.world_mut().resolve_arrest(ArrestChoice::Jail),
        Ok(ArrestResult::Jailed { days: 3 })
    );
    let saved = game.save_state().unwrap();
    let mut loaded = arrest_game();
    loaded.load_state(&saved).unwrap();

    let mut world = loaded.world_mut();
    let player = world.player();
    assert_eq!(world.actor(player).unwrap().days_in_jail, 3);
    assert_eq!(world.clock.game_days, 3.0);
    let faction = world.faction(FACTION).unwrap();
    assert_eq!(faction.crime_gold_violent, 0);
    assert_eq!(faction.crime_gold_nonviolent, 0);
    assert_eq!(
        world.resolve_arrest(ArrestChoice::Jail),
        Err(soe_game::crime::Error::NoBounty)
    );
}
