// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass for CMB-26 (weapon DNAM Resist flags on hit resolution).
//!
//! Contract under test, from docs/combat.md's "Weapon resistance flags,
//! CMB-26" section and `combat_resistance`/`unarmed_damage` in
//! crates/game/src/world/combat.rs:
//! - A DNAM Resist index of 39 DamageResist through 44 MagicResist
//!   (`uesp-modfileformat/WEAP.wikitext` line 123,
//!   `TES5Edit/Core/wbDefinitionsTES5.pas` lines 3344 to 3349) names a
//!   target actor value. Its current value, read the same way
//!   `combat_armor` already reads DamageResist, becomes a percentage
//!   divided by 100 and clamped to 0.0 through 1.0, applied as its own
//!   multiplicative reduction alongside armor reduction and shield block,
//!   before the difficulty multiplier.
//! - DNAM flags bit 0x01, "Ignores Normal Weapon Resistance"
//!   (`WEAP.wikitext` line 58), suppresses the term instead of rejecting
//!   the weapon.
//! - -1 is a no-op. Any other index outside 39 through 44 is reserved and
//!   rejects the whole contact with `Error::Unsupported("unsupported
//!   weapon resistance value")`, leaving saved state untouched.
//! - The unarmed attack form (CMB-01) carries its own DNAM the same way,
//!   so `unarmed_damage` applies the identical index and flag reading, an
//!   unarmed contact is not a special case.
//! - Index 39 DamageResist names the very actor value `combat_armor`
//!   already folds into its own armor-rating formula, so a weapon using
//!   that index composes two independently documented reductions rather
//!   than replacing one with the other.
//! - The record read for a hit follows the same load-order winning
//!   override and CNAM template chain every other combat record uses, WEAP
//!   carries no CMB-26 specific exception for either.
//!
//! This file only adds tests. It does not modify the files under test.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{Error, MeleeHit},
    Game,
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
/// `WEAP.wikitext` lines 58 and 123 and the byte offsets `Weapon::from_record`
/// decodes in crates/game/src/combat/records.rs.
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

fn weapon_template(object: u32, template: u32) -> Record {
    Record::new(Tag(*b"WEAP"), object).sub(Tag(*b"CNAM"), template.to_le_bytes())
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

fn template_fixture(
    template: Record,
    sources: Vec<Record>,
    patch_records: Vec<Record>,
) -> (Game, GlobalFormId) {
    let mut master = Plugin::new(FileHeader::default());
    master.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    master.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for source in sources {
        master.push_top(source);
    }
    settings(&mut master);
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Combat.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(template);
    for record in patch_records {
        patch.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Combat.esm", master).unwrap();
    order.add("Template.esp", patch).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    for actor in [player, id(0x901)] {
        game.world_mut()
            .set_actor_value(actor, "health", 100.)
            .unwrap();
        game.world_mut()
            .set_actor_value(actor, "onehanded", 100.)
            .unwrap();
    }
    let item = GlobalFormId {
        file: 1,
        object: 0xA01,
    };
    game.world_mut().add_item(id(0x14), item, 1);
    game.world_mut()
        .actor(id(0x14))
        .unwrap()
        .equipped
        .insert(item);
    (game, item)
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// Sourced from docs/combat.md CMB-26 ("a resist index of 39 DamageResist
/// through 44 MagicResist names a further target actor value that reduces
/// this weapon's damage... a percentage divided by 100 and clamped to 0.0
/// to 1.0") and the index-to-name table at
/// `TES5Edit/Core/wbDefinitionsTES5.pas` lines 3344 to 3349. Index 39
/// (DamageResist) composes with `combat_armor`'s own read of the same
/// actor value, so it is covered separately below. Every other index in
/// range must apply in isolation.
#[test]
fn armed_weapon_known_resistance_indices_apply_the_documented_actor_value() {
    for (resist, av) in [
        (40, "poisonresist"),
        (41, "resistfire"),
        (42, "resistshock"),
        (43, "resistfrost"),
        (44, "resistmagic"),
    ] {
        let mut game = fixture(vec![patch_dnam(weapon(0xA10, 10, 6), resist, 0)]);
        equip(&game, 0x900, 0xA10);
        game.world_mut()
            .set_actor_value(id(0x901), av, 60.)
            .unwrap();
        let result = game.melee_hit(hit_with(0x900, 0x901, 0xA10)).unwrap();
        close(result.resistance_reduction, 0.6);
        close(result.health_damage, 12.0);
    }
}

/// Sourced from docs/combat.md CMB-26 alongside the existing "Armor rating
/// and reduction" section: `combat_armor` reads the target's raw
/// `"damageresist"` actor value into its own rating before any equipped
/// armor is added, and CMB-26 reads that same actor value again through
/// its own, differently scaled formula when the weapon's Resist index is
/// 39. Neither section states the other supersedes it, so a weapon using
/// index 39 against a nonzero DamageResist target must show both
/// reductions applied together: armor_reduction from `combat_armor`'s
/// formula and resistance_reduction from CMB-26's, stacked
/// multiplicatively per the row's own ordering statement.
#[test]
fn armed_weapon_damage_resist_index_composes_with_existing_armor_reduction() {
    let mut game = fixture(vec![patch_dnam(weapon(0xA10, 10, 6), 39, 0)]);
    equip(&game, 0x900, 0xA10);
    game.world_mut()
        .set_actor_value(id(0x901), "damageresist", 60.)
        .unwrap();
    let result = game.melee_hit(hit_with(0x900, 0x901, 0xA10)).unwrap();
    close(result.armor_rating, 60.0);
    close(result.armor_reduction, 0.072);
    close(result.resistance_reduction, 0.6);
    close(result.health_damage, 11.136);
}

/// Sourced from docs/combat.md CMB-26 and `WEAP.wikitext` line 58: DNAM
/// flags bit 0x01 "Ignores Normal Weapon Resistance" "suppresses the term
/// instead of rejecting the weapon". A weapon carrying both the flag and a
/// real resist index must leave resistance_reduction at 0.0 and
/// health_damage at the unmodified baseline, not partially apply the term.
#[test]
fn armed_weapon_ignoring_flag_suppresses_the_resistance_term() {
    let mut game = fixture(vec![patch_dnam(weapon(0xA10, 10, 6), 44, 1)]);
    equip(&game, 0x900, 0xA10);
    game.world_mut()
        .set_actor_value(id(0x901), "resistmagic", 90.)
        .unwrap();
    let result = game.melee_hit(hit_with(0x900, 0x901, 0xA10)).unwrap();
    close(result.resistance_reduction, 0.0);
    close(result.health_damage, 30.0);
}

/// Sourced from docs/combat.md CMB-26: "An index outside -1 and 39 through
/// 44 is reserved and still rejects the contact, now with its own
/// `\"unsupported weapon resistance value\"` error". This is the armed path,
/// which had no coverage for any resist index before this row, unlike the
/// unarmed path's existing rejection test. A rejected contact must leave
/// the target's health and equipped set exactly as they were, and the
/// game must still resolve an ordinary hit correctly afterward, matching
/// every other rejection's "does not damage or emit events" meaning
/// (CMB-01) rather than the byte-identical `save_state()` comparison CMB-10's
/// read-only quality pass alone documents itself as keeping (that check
/// runs before the lazy target actor-base read this function performs
/// earlier for its protected/essential lookup, so its atomicity claim does
/// not extend to a check running after that read).
#[test]
fn armed_weapon_reserved_resist_index_rejects_without_changing_target_health_or_equipment() {
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

/// Sourced from docs/combat.md CMB-26's clamp statement ("a percentage
/// divided by 100 and clamped to 0.0 to 1.0"). An actor value outside 0 to
/// 100, reachable through ordinary `ModActorValue`/`SetActorValue` calls
/// with no validation tied to this row, must clamp instead of amplifying
/// damage past the full hit or reducing it below zero.
#[test]
fn weapon_resistance_reduction_clamps_out_of_range_actor_values() {
    for (value, expected_reduction, expected_damage) in
        [(500.0_f32, 1.0_f32, 0.0_f32), (-80.0, 0.0, 30.0)]
    {
        let mut game = fixture(vec![patch_dnam(weapon(0xA10, 10, 6), 44, 0)]);
        equip(&game, 0x900, 0xA10);
        game.world_mut()
            .set_actor_value(id(0x901), "resistmagic", value)
            .unwrap();
        let result = game.melee_hit(hit_with(0x900, 0x901, 0xA10)).unwrap();
        close(result.resistance_reduction, expected_reduction);
        close(result.health_damage, expected_damage);
    }
}

/// Sourced from docs/combat.md CMB-26 ("the weapon's DNAM Resist field")
/// combined with the template chain contract the existing cross-plugin
/// tests already establish for damage and armor rating: a fieldless
/// template's own DNAM is absent, so the resist index used for a hit must
/// come from following CNAM to the terminal record, the same as every
/// other DNAM field this row shares a record with.
#[test]
fn armed_weapon_resistance_reads_the_terminal_template_record() {
    let (mut game, weapon_item) = template_fixture(
        weapon_template(0x0100_0A01, 0x0000_0A02),
        vec![patch_dnam(weapon(0xA02, 10, 6), 41, 0)],
        vec![],
    );
    game.world_mut()
        .set_actor_value(id(0x901), "resistfire", 40.)
        .unwrap();
    let result = game
        .melee_hit(MeleeHit {
            attacker: id(0x14),
            target: id(0x901),
            weapon: weapon_item,
            is_bash: false,
            is_power_attack: false,
        })
        .unwrap();
    close(result.resistance_reduction, 0.4);
    close(result.health_damage, 9.0);
}

/// Sourced from the winning-override contract
/// `a_fieldless_cross_plugin_weapon_template_uses_the_winning_master_record`
/// already establishes for damage in combat_hits.rs, applied to the
/// Resist field CMB-26 added: when a later plugin redefines the same
/// terminal record, its DNAM must govern the hit, including the Resist
/// index, not the master's losing copy.
#[test]
fn armed_weapon_resistance_uses_the_winning_cross_plugin_override() {
    let (mut game, weapon_item) = template_fixture(
        weapon_template(0x0100_0A01, 0x0000_0A02),
        vec![patch_dnam(weapon(0xA02, 10, 6), -1, 0)],
        vec![patch_dnam(weapon(0xA02, 10, 6), 44, 0)],
    );
    game.world_mut()
        .set_actor_value(id(0x901), "resistmagic", 60.)
        .unwrap();
    let result = game
        .melee_hit(MeleeHit {
            attacker: id(0x14),
            target: id(0x901),
            weapon: weapon_item,
            is_bash: false,
            is_power_attack: false,
        })
        .unwrap();
    close(result.resistance_reduction, 0.6);
    close(result.health_damage, 6.0);
}

/// Sourced from docs/combat.md CMB-26: "The unarmed attack form (CMB-01)
/// carries its own DNAM the same way, so `unarmed_damage` reads its resist
/// index and ignoring flag and applies them identically, an unarmed
/// contact is not a special case." The only existing unarmed resist test
/// never exercises a real 39 through 44 index, only -1 and the flag
/// together, leaving this positive path and the flag's unarmed suppression
/// untested.
#[test]
fn unarmed_contact_applies_and_the_ignoring_flag_suppresses_resistance() {
    for (flags, expected_reduction, expected_damage) in [(0_u16, 0.7_f32, 3.0_f32), (1, 0.0, 10.0)]
    {
        let source = patch_dnam(unarmed_weapon(), 41, flags);
        let mut game = ready_unarmed_game_with_source(race(0x700, 10.), Some(source));
        game.world_mut()
            .set_actor_value(id(0x901), "resistfire", 70.)
            .unwrap();
        let result = game.melee_hit(unarmed_hit(0x900, 0x901)).unwrap();
        close(result.resistance_reduction, expected_reduction);
        close(result.health_damage, expected_damage);
    }
}

/// Sourced from the "engine save and load continuation" concern this row's
/// state falls under. docs/combat.md CMB-26 states the Resist index and
/// ignoring flag "are read from the WEAP record on every hit, not cached",
/// but the target actor value the index names is ordinary saved state
/// (established for a different actor value by
/// `cmb_15_attack_damage_mult_uses_shared_components_and_survives_save`).
/// A resistance reduction computed before a save must still apply
/// identically after a load into a fresh game built from the same records.
#[test]
fn weapon_resistance_and_its_live_actor_value_survive_a_save_load_round_trip() {
    let extra = vec![patch_dnam(weapon(0xA10, 10, 6), 44, 0)];
    let game = fixture(extra.clone());
    equip(&game, 0x900, 0xA10);
    game.world_mut()
        .set_actor_value(id(0x901), "resistmagic", 60.)
        .unwrap();
    let saved = game.save_state().unwrap();
    let mut loaded = fixture(extra);
    loaded.load_state(&saved).unwrap();
    let result = loaded.melee_hit(hit_with(0x900, 0x901, 0xA10)).unwrap();
    close(result.resistance_reduction, 0.6);
    close(result.health_damage, 12.0);
}
