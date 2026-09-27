// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! AI-29 shout execution uses the equipped SHOU, shared spell delivery and
//! the actor's saved voice recovery deadline.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::ai::Procedure;
use soe_game::npc::{self, Config, Stats};
use soe_game::package;
use soe_game::world::magic::MagicError;
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TEMPLATE: u32 = 0x800;
const PACK: u32 = 0x801;
const BASE: u32 = 0x802;
const ACTOR: u32 = 0x803;
const SHOUT: u32 = 0x804;
const SPELL: u32 = 0x805;
const EFFECT: u32 = 0x806;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn load_order(word_count: usize) -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    let template = package::finish(package::close_header(
        package::build_template(TEMPLATE, "ShoutProcedure"),
        None,
    ));
    plugin.push_top(package::with_procedure(template, "Shout", &[], true));
    let pack = package::finish(package::close_header(
        package::build_record(PACK, "UseAuthoredShout", 0, package::Schedule::default()),
        Some(TEMPLATE),
    ));
    plugin.push_top(pack);
    plugin.push_top(
        npc::build_record(
            BASE,
            "Shouter",
            "Shouter",
            Config::default(),
            Stats::default(),
        )
        .sub(T(b"PKID"), PACK.to_le_bytes()),
    );
    let mut position = vec![0; 24];
    position[8..12].copy_from_slice(&0.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), position),
    );
    let mut spell_data = vec![0; 36];
    spell_data[20..24].copy_from_slice(&0u32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"SPEL"), SPELL)
            .sub(T(b"SPIT"), spell_data)
            .sub(T(b"EFID"), EFFECT.to_le_bytes())
            .sub(
                T(b"EFIT"),
                [
                    10.0f32.to_le_bytes().as_slice(),
                    0u32.to_le_bytes().as_slice(),
                    0u32.to_le_bytes().as_slice(),
                ]
                .concat(),
            ),
    );
    let mut effect_data = vec![0; 152];
    effect_data[0..4].copy_from_slice(&6u32.to_le_bytes());
    effect_data[16..20].copy_from_slice(&u32::MAX.to_le_bytes());
    effect_data[64..68].copy_from_slice(&0u32.to_le_bytes());
    effect_data[68..72].copy_from_slice(&24u32.to_le_bytes());
    effect_data[80..84].copy_from_slice(&0u32.to_le_bytes());
    effect_data[84..88].copy_from_slice(&0u32.to_le_bytes());
    plugin.push_top(Record::new(T(b"MGEF"), EFFECT).sub(T(b"DATA"), effect_data));
    let mut shout = Record::new(T(b"SHOU"), SHOUT);
    for index in 0..word_count {
        let (spell, recovery) = if index + 1 == word_count {
            (SPELL, 4.0f32)
        } else {
            (0, 0.0f32)
        };
        let mut word = vec![0; 12];
        word[4..8].copy_from_slice(&spell.to_le_bytes());
        word[8..12].copy_from_slice(&recovery.to_le_bytes());
        shout = shout.sub(T(b"SNAM"), word);
    }
    plugin.push_top(shout);
    let mut order = LoadOrder::new();
    order
        .add("Ai29.esm", Plugin::parse(&plugin.to_bytes()).unwrap())
        .unwrap();
    order
}

fn game() -> Game {
    let game = Game::new(load_order(3));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.shouts.insert(id(SHOUT));
        actor.equipped_shout = Some(id(SHOUT));
    }
    game
}

#[test]
fn ai_29_package_executes_equipped_shout_and_waits_for_authored_recovery() {
    let mut game = game();
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    game.tick(1.0);
    let (procedure, recovery_until) = {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        (actor.ai.procedure.clone(), actor.voice_recovery_until)
    };
    assert_eq!(procedure, Procedure::Shout);
    assert_eq!(recovery_until, Some(5.0));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 90.0);
    assert_eq!(game.world_mut().voice_recovery_time(id(ACTOR)), 4.0);
    game.tick(4.0);
    assert!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .ai
            .tree
            .as_ref()
            .unwrap()
            .finished
    );
    assert_eq!(game.world_mut().voice_recovery_time(id(ACTOR)), 0.0);
}

#[test]
fn ai_29_package_waits_out_existing_voice_recovery_before_casting() {
    let mut game = game();
    assert!(game.world_mut().set_voice_recovery_time(id(ACTOR), 2.0));
    game.tick(1.0);
    assert_eq!(game.world_mut().voice_recovery_time(id(ACTOR)), 1.0);
    assert_eq!(
        game.world()
            .actor_ref(id(ACTOR))
            .unwrap()
            .voice_recovery_until,
        Some(2.0)
    );
    game.tick(1.0);
    assert_eq!(
        game.world_mut()
            .actor(id(ACTOR))
            .unwrap()
            .voice_recovery_until,
        Some(6.0)
    );
}

#[test]
fn ai_29_voice_recovery_survives_world_save_load() {
    let game = game();
    game.world_mut().set_voice_recovery_time(id(ACTOR), 9.0);
    let bytes = game.world().save_state();
    let mut restored = soe_game::world::World::new(load_order(3));
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        restored.actor_ref(id(ACTOR)).unwrap().voice_recovery_until,
        Some(9.0)
    );
}

#[test]
fn ai_29_malformed_shout_word_count_rejects_cast_without_mutation() {
    let game = Game::new(load_order(2));
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game.world_mut()
        .set_actor_value(id(ACTOR), "health", 100.0)
        .unwrap();
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.shouts.insert(id(SHOUT));
        actor.equipped_shout = Some(id(SHOUT));
    }
    let error = game
        .world_mut()
        .cast_shout(id(ACTOR), None, &mut |_, _| soe_papyrus::ObjectId(0))
        .unwrap_err();
    assert!(matches!(
        error,
        MagicError::Unsupported("malformed shout words")
    ));
    assert_eq!(game.world_mut().actor_value(id(ACTOR), "health"), 100.0);
    assert_eq!(game.world_mut().voice_recovery_time(id(ACTOR)), 0.0);
}
