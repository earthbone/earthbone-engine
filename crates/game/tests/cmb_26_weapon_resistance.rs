// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verifier pass for CMB-26 (weapon DNAM Resist flags on hit resolution).
//!
//! Done check: known resistance flags choose the documented damage path and
//! reserved flag values still reject safely.
//!
//! `crates/game/tests/cmb_26_resistance_adversarial.rs` already carries a
//! wide breaker sweep (template chains, cross-plugin overrides, save and
//! load, clamping). This file is the row's own positive and negative check,
//! independent of that sweep: a real resist index reduces damage by the
//! target actor value it names (armed and unarmed), the "Ignores Normal
//! Weapon Resistance" flag suppresses that reduction instead of rejecting
//! the weapon, and any index outside -1 and 39 through 44 is reserved and
//! rejects the contact without touching target state.
//!
//! Sources: `uesp-modfileformat/WEAP.wikitext` lines 58 (Flags, 0x01) and
//! 123 (Resist, 39 DamageResist through 44 MagicResist or -1),
//! `TES5Edit/Core/wbDefinitionsTES5.pas` lines 3344 to 3349 (the same
//! index-to-name table), docs/combat.md's "Weapon resistance flags, CMB-26"
//! section.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    combat::{Error, MeleeHit},
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn weapon(object: u32, damage: u16, skill: i32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, skill as u32);
    put(&mut dnam, 88, u32::MAX);
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
}

fn unarmed_weapon() -> Record {
    weapon(0x1F4, 0, -1)
}

/// Overwrite a built WEAP record's DNAM flags (offset 12, "Ignores Normal
/// Weapon Resistance" is bit 0x01) and Resist index (offset 88), matching
/// `WEAP.wikitext` lines 58 and 123 and the byte offsets
/// `Weapon::from_record` decodes in crates/game/src/combat/records.rs.
fn patch_dnam(mut record: Record, resist: i32, flags: u16) -> Record {
    let dnam = &mut record
        .subrecords
        .iter_mut()
        .find(|field| field.tag == Tag(*b"DNAM"))
        .unwrap()
        .data;
    dnam[12..14].copy_from_slice(&flags.to_le_bytes());
    put(dnam, 88, resist as u32);
    record
}

fn race(object: u32, unarmed_damage: f32) -> Record {
    let mut data = vec![0; 124];
    put(&mut data, 96, unarmed_damage.to_bits());
    Record::new(Tag(*b"RACE"), object).sub(Tag(*b"DATA"), data)
}

fn templated_npc(object: u32, template: u32) -> Record {
    let mut acbs = vec![0; 24];
    acbs[18..20].copy_from_slice(&1_u16.to_le_bytes());
    Record::new(Tag(*b"NPC_"), object)
        .sub(Tag(*b"ACBS"), acbs)
        .sub(Tag(*b"TPLT"), template.to_le_bytes())
}

fn npc_with_race(object: u32, race: u32) -> Record {
    Record::new(Tag(*b"NPC_"), object).sub(Tag(*b"RNAM"), race.to_le_bytes())
}

fn settings(plugin: &mut Plugin) {
    for (index, (name, value)) in [
        ("fDamageWeaponMult", 1_f32),
        ("fDamagePCSkillMin", 1.),
        ("fDamagePCSkillMax", 1.5),
        ("fDamageSkillMin", 1.),
        ("fDamageSkillMax", 3.),
        ("fArmorRatingBase", 1.),
        ("fArmorRatingPCMax", 1.4),
        ("fArmorRatingMax", 2.5),
        ("fArmorScalingFactor", 0.12),
        ("fArmorBaseFactor", 0.03),
        ("fMaxArmorRating", 80.),
    ]
    .into_iter()
    .enumerate()
    {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x1000 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
}

fn fixture(extra: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    for record in extra {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 100.).unwrap();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn unarmed_game_with_source(winning_race: Record, source: Option<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race(0x700, 4.));
    plugin.push_top(templated_npc(0x800, 0x801));
    plugin.push_top(npc_with_race(0x801, 0x700));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    if let Some(source) = source {
        plugin.push_top(source);
    }
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Combat.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(winning_race);
    order.add("CombatPatch.esp", patch).unwrap();
    Game::new(order)
}

fn ready_unarmed_game_with_source(winning_race: Record, source: Option<Record>) -> Game {
    let game = unarmed_game_with_source(winning_race, source);
    game.world_mut().actor_base(id(0x800)).unwrap();
    for actor in [0x900, 0x901] {
        game.world_mut()
            .set_actor_value(id(actor), "health", 100.)
            .unwrap();
    }
    game
}

fn unarmed_hit(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0x1F4),
        is_bash: false,
        is_power_attack: false,
    }
}

fn hit_with(attacker: u32, target: u32, weapon: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(weapon),
        is_bash: false,
        is_power_attack: false,
    }
}

fn equip(game: &Game, actor: u32, item: u32) {
    let mut world = game.world_mut();
    world.equip_item(id(actor), id(item)).unwrap();
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// Done check, positive clause, armed path: a real resist index (42,
/// ResistShock) names the target's matching actor value and reduces
/// health_damage by that fraction, matching docs/combat.md's "a percentage
/// divided by 100" statement and `WEAP.wikitext` line 123's index table.
#[test]
fn cmb_26_known_resist_index_reduces_armed_damage_by_the_named_actor_value() {
    let mut game = fixture(vec![patch_dnam(weapon(0xA10, 10, 6), 42, 0)]);
    equip(&game, 0x900, 0xA10);
    game.world_mut()
        .set_actor_value(id(0x901), "resistshock", 50.)
        .unwrap();
    let result = game.melee_hit(hit_with(0x900, 0x901, 0xA10)).unwrap();
    close(result.resistance_reduction, 0.5);
    close(result.health_damage, 15.0);
}

/// Done check, positive clause, ignoring flag: DNAM flags bit 0x01
/// ("Ignores Normal Weapon Resistance", `WEAP.wikitext` line 58) suppresses
/// the reduction even though the index names a live, nonzero actor value,
/// matching docs/combat.md's "suppresses the term instead of rejecting the
/// weapon".
#[test]
fn cmb_26_ignoring_flag_suppresses_the_reduction_for_a_real_index() {
    let mut game = fixture(vec![patch_dnam(weapon(0xA10, 10, 6), 44, 1)]);
    equip(&game, 0x900, 0xA10);
    game.world_mut()
        .set_actor_value(id(0x901), "resistmagic", 90.)
        .unwrap();
    let result = game.melee_hit(hit_with(0x900, 0x901, 0xA10)).unwrap();
    close(result.resistance_reduction, 0.0);
    close(result.health_damage, 30.0);
}

/// Done check, positive clause, unarmed path: CMB-01's unarmed attack form
/// carries its own DNAM, and `unarmed_damage` must read its resist index and
/// flag the same way the armed path does, not skip the mechanic.
#[test]
fn cmb_26_known_resist_index_reduces_unarmed_damage_the_same_way() {
    let source = patch_dnam(unarmed_weapon(), 41, 0);
    let mut game = ready_unarmed_game_with_source(race(0x700, 10.), Some(source));
    game.world_mut()
        .set_actor_value(id(0x901), "resistfire", 70.)
        .unwrap();
    let result = game.melee_hit(unarmed_hit(0x900, 0x901)).unwrap();
    close(result.resistance_reduction, 0.7);
    close(result.health_damage, 3.0);
}

/// Done check, negative clause, armed path: an index outside -1 and 39
/// through 44 is reserved (`WEAP.wikitext` line 123, "CK allows 39
/// DamageResist through 44 MagicResist or -1 for None") and must reject the
/// whole contact with the row's dedicated error, leaving the target's
/// health and equipped set untouched, and leaving the game able to resolve
/// an ordinary hit afterward.
#[test]
fn cmb_26_reserved_resist_index_rejects_armed_hit_without_side_effects() {
    for resist in [0, 1, 38, 45, i32::MIN, i32::MAX] {
        let mut game = fixture(vec![patch_dnam(weapon(0xA10, 10, 6), resist, 0)]);
        equip(&game, 0x900, 0xA10);
        let health_before = game.world_mut().actor_value(id(0x901), "health");
        let equipped_before = game.world_mut().actor(id(0x901)).unwrap().equipped.clone();
        let result = game.melee_hit(hit_with(0x900, 0x901, 0xA10));
        assert!(
            matches!(
                result,
                Err(Error::Unsupported("unsupported weapon resistance value"))
            ),
            "resist {resist}: {result:?}"
        );
        close(
            game.world_mut().actor_value(id(0x901), "health"),
            health_before,
        );
        assert_eq!(
            game.world_mut().actor(id(0x901)).unwrap().equipped,
            equipped_before
        );
        close(
            game.melee_hit(hit_with(0x900, 0x901, 0xA00))
                .unwrap()
                .health_damage,
            30.0,
        );
    }
}

/// Done check, negative clause, unarmed path: the reserved-index rejection
/// applies identically when the resist index comes from the unarmed attack
/// form's own DNAM rather than an equipped WEAP.
#[test]
fn cmb_26_reserved_resist_index_rejects_unarmed_hit_without_changing_health() {
    for resist in [0, 45] {
        let source = patch_dnam(unarmed_weapon(), resist, 0);
        let mut game = ready_unarmed_game_with_source(race(0x700, 10.), Some(source));
        let health_before = game.world_mut().actor_value(id(0x901), "health");
        let result = game.melee_hit(unarmed_hit(0x900, 0x901));
        assert!(
            matches!(
                result,
                Err(Error::Unsupported("unsupported weapon resistance value"))
            ),
            "resist {resist}: {result:?}"
        );
        close(
            game.world_mut().actor_value(id(0x901), "health"),
            health_before,
        );
    }
}
