// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! CMB-35 done check: a lethal magic hit or a lethal scripted
//! `DamageActorValue` call on an essential actor enters bleedout instead of
//! killing. Mirrors the synthetic-plugin fixtures in combat_bleedout.rs
//! (threshold records) and combat_protected_player_kill.rs (essential/
//! protected ACBS flags), and the effect/spell record shapes proven in
//! magic.rs, so the only new ground covered here is the clamp itself.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, npc::flags};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const TARGET: u32 = 0x900;
const CASTER: u32 = 0x901;
const NPC_TARGET: u32 = 0x800;
const NPC_CASTER: u32 = 0x801;
const EFFECT: u32 = 0x200;
const SPELL: u32 = 0x300;
const KEYWORD: u32 = 0x400;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(data: &mut [u8], at: usize, value: u32) {
    data[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A direct-damage, instant, non-constant magic effect on health (archetype
/// 0, actor value index 24), the same shape `magic.rs`'s
/// `magic_conditions_use_credited_caster_and_delivery_target` already
/// exercises for ordinary (non-essential) targets.
fn effect() -> Record {
    let mut data = vec![0; 152];
    put(&mut data, 0, 4);
    put(&mut data, 16, u32::MAX);
    put(&mut data, 64, 0);
    put(&mut data, 68, 24);
    put(&mut data, 80, 1);
    put(&mut data, 84, 3);
    Record::new(T(b"MGEF"), EFFECT)
        .zsub(T(b"EDID"), "Cmb35ProbeEffect")
        .sub(T(b"DATA"), data)
        .sub(T(b"KWDA"), KEYWORD.to_le_bytes())
}

/// A target-delivery spell (delivery 3) carrying `effect()` at the given
/// magnitude, instant (duration 0).
fn spell(magnitude: f32) -> Record {
    let mut data = vec![0; 36];
    put(&mut data, 16, 1);
    put(&mut data, 20, 3);
    let mut efit = magnitude.to_le_bytes().to_vec();
    efit.extend_from_slice(&0u32.to_le_bytes());
    efit.extend_from_slice(&0u32.to_le_bytes());
    Record::new(T(b"SPEL"), SPELL)
        .sub(T(b"SPIT"), data)
        .sub(T(b"EFID"), EFFECT.to_le_bytes())
        .sub(T(b"EFIT"), efit)
}

/// One essential target (0x900) and one plain caster (0x901), health set to
/// 100 so the built-in `fBleedoutDefault` (0.2, game_settings.rs) gives a
/// deterministic threshold of 20.
fn fixture() -> Game {
    let mut essential = vec![0; 24];
    essential[..4].copy_from_slice(&flags::ESSENTIAL.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(T(b"NPC_"), NPC_TARGET).sub(T(b"ACBS"), essential));
    plugin.push_top(
        Record::new(T(b"ACHR"), TARGET)
            .sub(T(b"NAME"), NPC_TARGET.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(T(b"NPC_"), NPC_CASTER).sub(T(b"ACBS"), vec![0; 24]));
    plugin.push_top(
        Record::new(T(b"ACHR"), CASTER)
            .sub(T(b"NAME"), NPC_CASTER.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(Record::new(T(b"KYWD"), KEYWORD).zsub(T(b"EDID"), "Cmb35ProbeKeyword"));
    plugin.push_top(effect());
    plugin.push_top(spell(500.0));
    let mut order = LoadOrder::new();
    order.add("Cmb35.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [id(TARGET), id(CASTER)] {
        game.world_mut()
            .set_actor_value(actor, "health", 100.)
            .unwrap();
    }
    game
}

/// CMB-35 done check, magic clause: a lethal spell cast by a non-player
/// caster on an essential actor clamps health at the bleedout threshold
/// (20, not 0) and enters bleedout instead of killing.
#[test]
fn cmb_35_lethal_magic_clamps_essential_actor_to_bleedout() {
    let mut game = fixture();
    let shared = game.shared_world();
    let applied = shared
        .borrow_mut()
        .cast_spell(
            id(SPELL),
            id(CASTER),
            Some(id(CASTER)),
            Some(id(TARGET)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert!(
        !applied.is_empty(),
        "the effect must apply for this test to prove anything"
    );
    let mut world = game.world_mut();
    let target = world.actor(id(TARGET)).unwrap();
    assert!(
        !target.dead,
        "a lethal magic hit on an essential actor must not kill it"
    );
    assert!(
        target.unconscious,
        "the essential actor must enter bleedout"
    );
    assert_eq!(
        world.actor_value(id(TARGET), "health"),
        20.,
        "health must clamp at the bleedout threshold, not at zero or below"
    );
}

/// CMB-35 done check, scripted clause: a lethal `DamageActorValue` call
/// (`World::damage_actor_value`, actors.rs) on an essential actor clamps
/// health at the bleedout threshold instead of killing, with no caster
/// identity available to grant the CMB-34 player exception.
#[test]
fn cmb_35_lethal_damage_actor_value_clamps_essential_actor_to_bleedout() {
    let game = fixture();
    let mut world = game.world_mut();
    world
        .damage_actor_value(id(TARGET), "health", 500.)
        .unwrap();
    let target = world.actor(id(TARGET)).unwrap();
    assert!(
        !target.dead,
        "a lethal DamageActorValue call on an essential actor must not kill it"
    );
    assert!(
        target.unconscious,
        "the essential actor must enter bleedout"
    );
    assert_eq!(
        world.actor_value(id(TARGET), "health"),
        20.,
        "health must clamp at the bleedout threshold, not at zero or below"
    );
}
