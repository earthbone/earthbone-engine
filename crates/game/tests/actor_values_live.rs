// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Survey winning NPC and race inputs in the five shipped masters.
//! This checks authored inputs, not agreement with running Skyrim.

use crate::common::installed;

use std::{collections::BTreeMap, path::PathBuf};

use soe_esm::Tag;
use soe_game::{
    npc::{self, Npc},
    race::Attributes,
    Game,
};

#[test]
fn installed_actor_value_sources() {
    let Some(_data) = std::env::var_os("SKYRIM_DATA").map(PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping actor-value survey");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let game = Game::from_shared_order(order);
    let world = game.world();
    let mut races = 0;
    let mut sizes = BTreeMap::<usize, usize>::new();
    for (id, record) in world
        .order()
        .winners_of(Tag(*b"RACE"))
        .filter(|(_, r)| !r.is_deleted())
    {
        let values =
            Attributes::from_record(record).unwrap_or_else(|error| panic!("{id}: {error}"));
        assert!(values.is_some(), "{id}: race without attributes");
        *sizes
            .entry(record.find(Tag(*b"DATA")).unwrap().len())
            .or_default() += 1;
        races += 1;
    }
    // Thresholds set before the first survey, allowing ordinary plugin revisions.
    assert!(races > 100, "expected a full race survey, got {races}");
    let mut npcs = 0;
    let mut resolved = 0;
    let mut cached = 0;
    let mut errors = BTreeMap::<String, usize>::new();
    for (id, record) in world
        .order()
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, r)| !r.is_deleted())
    {
        let winner = world.order().winner_ref(id).unwrap();
        let npc = Npc::from_record(
            record,
            world.order().file(winner.file).plugin.header.is_localized(),
        )
        .unwrap();
        assert!(npc.stats.is_some(), "{id}: NPC without DNAM");
        npcs += 1;
        match world.initial_actor_values(id) {
            Ok(values) => {
                assert!(values.iter().all(|(_, value)| value.is_finite()), "{id}");
                let source = world
                    .npc_component_source(id, npc::template::STATS)
                    .unwrap();
                let source_record = world.record(source).unwrap();
                let source = Npc::from_record(source_record, false).unwrap();
                cached += usize::from(source.config.flags & npc::flags::AUTO_CALC_STATS != 0);
                resolved += 1;
            }
            Err(error) => {
                *errors.entry(error.to_string()).or_default() += 1;
            }
        }
    }
    assert!(npcs > 1000, "expected a full NPC survey, got {npcs}");
    assert!(resolved > 1000, "too few NPC sources resolved: {resolved}");
    assert!(
        errors
            .keys()
            .all(|error| error == "LVLN is not an NPC_ record"),
        "unexpected source failures: {errors:?}"
    );
    let mut examples = Vec::new();
    for name in ["Player", "Hulda", "MQ101Alduin"] {
        let id = world.form(name).unwrap();
        let values = world.initial_actor_values(id).unwrap();
        eprintln!(
            "{name}: health={} magicka={} stamina={}",
            values[0].1, values[1].1, values[2].1
        );
        if name == "Player" {
            assert_eq!([values[0].1, values[1].1, values[2].1], [100.0; 3]);
        }
        examples.push((name, id, values));
    }
    eprintln!(
        "races={races}, DATA sizes={sizes:?}, NPCs={npcs}, resolved={resolved}, cached autocalc={cached}, unresolved={errors:?}"
    );
    drop(world);
    let mut world = game.world_mut();
    let player = world.player();
    assert_eq!(player.object, 0x14);
    assert_eq!(world.base_of(player), Some(examples[0].1));
    for (name, base, values) in examples {
        let actor = if name == "Player" {
            player
        } else {
            world.place_at(base, player, false)
        };
        for (key, value) in values {
            assert_eq!(world.base_actor_value(actor, &key), value, "{name}.{key}");
        }
        let maximum = world.actor_value_maximum(actor, "health");
        world.damage_actor_value(actor, "health", 10.0).unwrap();
        assert_eq!(world.actor_value(actor, "health"), maximum - 10.0);
        let saved = world.save_state();
        let hash = world.state_hash();
        world.restore_actor_value(actor, "health", 10.0).unwrap();
        world.load_state(&saved).unwrap();
        assert_eq!(world.state_hash(), hash);
        assert_eq!(world.actor_value(actor, "health"), maximum - 10.0);
        world.restore_actor_value(actor, "health", 10.0).unwrap();
        assert_eq!(world.actor_value(actor, "health"), maximum);
    }
}
