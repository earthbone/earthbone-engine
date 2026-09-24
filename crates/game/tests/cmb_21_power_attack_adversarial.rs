// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for CMB-21, power attack damage and stamina cost.
//! Sources: `../engine-refs/TES5Edit/Core/wbDefinitionsTES5.pas`
//! (`wbAttackData`, RACE ATKD layout and Attack Flags),
//! `../engine-refs/ck-wiki/OnHit_-_ObjectReference.wikitext` (the
//! abPowerAttack event parameter), `../engine-refs/ck-wiki/DamageActorValue_-
//! _Actor.wikitext` (stamina cost has no floor), the engine's core design
//! rule that every system reads forms by resolved FormID from the winning
//! load order (skyrim-open-engine/CLAUDE.md), and the row's own done
//! criterion: "A power attack applies its damage multiplier and consumes the
//! recorded stamina once." Every fixture below builds its own plugin bytes.
//! The code under test, `crates/game/src/world/combat.rs` and
//! `crates/game/src/combat/records.rs`, is never edited here.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{Error, MeleeHit},
    Game, Handle,
};
use soe_papyrus::{
    asm::{b, ident, FunctionBuilder, ScriptBuilder},
    Opcode::*,
    Value,
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

/// RACE Attack Flags bits this file needs, matching xEdit `wbAttackData` and
/// `crates/game/src/combat/records.rs`'s own `ATTACK_FLAG_*` constants.
const FLAG_BASH: u32 = 0x02;
const FLAG_POWER: u32 = 0x04;
const FLAG_LEFT: u32 = 0x08;
const FLAG_ROTATING: u32 = 0x10;

/// One 44 byte RACE `ATKD` entry, xEdit `wbAttackData` field order: damage
/// mult, attack chance, attack spell, flags, attack angle, strike angle,
/// stagger, attack type, knockdown, recovery time, stamina mult.
fn atkd_bytes(damage_mult: f32, stamina_mult: f32, flags: u32) -> Vec<u8> {
    let mut data = vec![0_u8; 44];
    put(&mut data, 0, damage_mult.to_bits());
    put(&mut data, 12, flags);
    put(&mut data, 40, stamina_mult.to_bits());
    data
}
fn race_with_atkd(object: u32, entries: &[(f32, f32, u32)]) -> Record {
    let data = vec![0_u8; 124];
    let mut record = Record::new(Tag(*b"RACE"), object).sub(Tag(*b"DATA"), data);
    for &(damage_mult, stamina_mult, flags) in entries {
        record = record.sub(Tag(*b"ATKD"), atkd_bytes(damage_mult, stamina_mult, flags));
    }
    record
}

fn power_hit(attacker: u32, target: u32, is_power_attack: bool) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack,
    }
}
fn unarmed_power_hit(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0x1F4),
        is_bash: false,
        is_power_attack: true,
    }
}

fn ready(game: Game) -> Game {
    for actor in [id(0x900), id(0x901)] {
        game.world_mut()
            .set_actor_value(actor, "health", 100.)
            .unwrap();
        game.world_mut()
            .set_actor_value(actor, "onehanded", 100.)
            .unwrap();
        game.world_mut()
            .set_actor_value(actor, "stamina", 200.)
            .unwrap();
        game.world_mut().add_item(actor, id(0xA00), 1);
        game.world_mut()
            .actor(actor)
            .unwrap()
            .equipped
            .insert(id(0xA00));
    }
    game
}
fn power_game(race: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(race);
    plugin.push_top(templated_npc(0x800, 0x801));
    plugin.push_top(npc_with_race(0x801, 0x700));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    ready(Game::new(order))
}
fn overridden_power_game(base_race: Record, patch_race: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(base_race);
    plugin.push_top(templated_npc(0x800, 0x801));
    plugin.push_top(npc_with_race(0x801, 0x700));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Combat.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(patch_race);
    order.add("CombatPatch.esp", patch).unwrap();
    ready(Game::new(order))
}
fn no_race_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Combat.esm", plugin).unwrap();
    ready(Game::new(order))
}
fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// Attaches an `OnHit` observer to actor 0x901 that copies the event's own
/// `abPowerAttack` parameter into a persistent `power_seen` Bool, so a test
/// can read back the literal value the engine sent, not merely whether the
/// event fired at all.
fn power_observer(game: &mut Game) -> soe_papyrus::ObjectId {
    let function = FunctionBuilder::new("OnHit", "None")
        .param("aggressor", "ObjectReference")
        .param("source", "Form")
        .param("projectile", "Projectile")
        .param("power", "Bool")
        .param("sneak", "Bool")
        .param("bash", "Bool")
        .param("blocked", "Bool")
        .ins(Assign, vec![ident("power_seen"), ident("power")]);
    let script = ScriptBuilder::new("HitObserver", "Actor")
        .variable("power_seen", "Bool", b(false))
        .function(function.build());
    game.load_script(&script.bytes()).unwrap();
    let object = game.object_of(Handle::Form(id(0x901)));
    game.vm.attach(object, "HitObserver").unwrap();
    object
}

#[test]
fn cmb_21_power_attack_applies_damage_mult_and_consumes_stamina_once() {
    // Done criterion: "A power attack applies its damage multiplier and
    // consumes the recorded stamina once." An unarmored non-power hit with
    // this weapon (damage 10, weight 8) and onehanded 100 lands 30 damage
    // (10 * fDamageSkillMax 3.0), the same skill scaling
    // crates/game/tests/combat_hits.rs proves independently for the same
    // weapon and settings. A RACE ATKD entry with damage_mult 2.0 must
    // double that to 60, and its stamina_mult 1.5 must take exactly
    // (40 + weight 8 * 2) * 1.5 = 84 stamina from the attacker, once, per
    // world/combat.rs's own documented formula and its UESP citation.
    let race = race_with_atkd(0x700, &[(2.0, 1.5, FLAG_POWER)]);
    let mut game = power_game(race);
    let result = game.melee_hit(power_hit(0x900, 0x901, true)).unwrap();
    close(result.health_damage, 60.);
    close(game.world_mut().actor_value(id(0x900), "stamina"), 116.);
}

#[test]
fn cmb_21_unarmed_power_attack_rejected_without_partial_state_change() {
    // world/combat.rs documents unarmed power attacks as an unsupported
    // gap, UESP's Combat and Damage pages disagree on the unarmed
    // multiplier, and rejects the hit before either actor's state changes.
    // The done criterion ties a power attack's stamina cost to a swing that
    // actually lands, so a rejected swing must consume none, the same
    // all-or-nothing contract
    // cmb_15_invalid_damage_mult_rejects_without_health_or_event_changes
    // proves on a different reject path in combat_hits.rs.
    let mut game = no_race_game();
    let stamina = game.world_mut().actor_value(id(0x900), "stamina");
    let health = game.world_mut().actor_value(id(0x901), "health");
    assert!(matches!(
        game.melee_hit(unarmed_power_hit(0x900, 0x901)),
        Err(Error::Unsupported("unarmed power attack"))
    ));
    close(game.world_mut().actor_value(id(0x900), "stamina"), stamina);
    close(game.world_mut().actor_value(id(0x901), "health"), health);
}

#[test]
fn cmb_21_default_entry_selection_skips_bash_left_and_rotating_flags() {
    // xEdit wbAttackData's Attack Flags (Bash 0x02, Power 0x04, Left 0x08,
    // Rotating 0x10, TES5Edit/Core/wbDefinitionsTES5.pas) and
    // combat/records.rs's own PowerAttack::from_race doc comment both say
    // the default single-weapon power attack is the first entry with Power
    // set and none of Bash, Left or Rotating set. Three disqualified
    // entries precede the real one, at multipliers a passing test could
    // not mistake for the qualifying 2.0.
    let race = race_with_atkd(
        0x700,
        &[
            (97.0, 1.0, FLAG_BASH | FLAG_POWER),
            (96.0, 1.0, FLAG_LEFT | FLAG_POWER),
            (95.0, 1.0, FLAG_ROTATING | FLAG_POWER),
            (2.0, 1.5, FLAG_POWER),
        ],
    );
    let mut game = power_game(race);
    let result = game.melee_hit(power_hit(0x900, 0x901, true)).unwrap();
    close(result.health_damage, 60.);
}

#[test]
fn cmb_21_load_order_winning_race_atkd_governs_power_attack() {
    // The engine's core design rule (skyrim-open-engine/CLAUDE.md, section
    // 3 of the research map): every system reads forms keyed by resolved
    // FormID from the winning load order, never a cached copy. A RACE
    // override in a later plugin must supply the power attack multipliers
    // that govern, the same winning-override pattern
    // combat_stagger_adversarial.rs already proves for a WEAP DNAM field.
    let base = race_with_atkd(0x700, &[(2.0, 1.5, FLAG_POWER)]);
    let patch = race_with_atkd(0x700, &[(5.0, 1.5, FLAG_POWER)]);
    let mut game = overridden_power_game(base, patch);
    let result = game.melee_hit(power_hit(0x900, 0x901, true)).unwrap();
    close(result.health_damage, 150.);
}

#[test]
fn cmb_21_race_without_a_qualifying_atkd_entry_rejected_atomically() {
    // combat/records.rs's PowerAttack::from_race returns Ok(None) when no
    // ATKD entry has Power set without Bash, Left or Rotating, and
    // world/combat.rs's combat_power_attack maps that to
    // Error::Unsupported("race has no default power attack entry"). The
    // done criterion ties the stamina cost to a swing that lands, so a race
    // without a qualifying entry must reject before spending any stamina or
    // landing any damage, the same atomicity
    // cmb_15_invalid_damage_mult_rejects_without_health_or_event_changes
    // proves for a different reject path in combat_hits.rs.
    let race = race_with_atkd(0x700, &[]);
    let mut game = power_game(race);
    let stamina = game.world_mut().actor_value(id(0x900), "stamina");
    let health = game.world_mut().actor_value(id(0x901), "health");
    assert!(matches!(
        game.melee_hit(power_hit(0x900, 0x901, true)),
        Err(Error::Unsupported("race has no default power attack entry"))
    ));
    close(game.world_mut().actor_value(id(0x900), "stamina"), stamina);
    close(game.world_mut().actor_value(id(0x901), "health"), health);
}

#[test]
fn cmb_21_attacker_without_a_race_rejected_atomically() {
    // world/combat.rs's combat_power_attack requires ActorState::race to be
    // Some before reading any ATKD entry, and returns
    // Error::Unsupported("attacker has no race for power attack data")
    // otherwise. An NPC_ record with no RNAM, the same shape
    // combat_hits.rs's fixture() already relies on npc_race resolving to
    // None for, must reject a power attack without spending stamina or
    // landing damage.
    let mut game = no_race_game();
    let stamina = game.world_mut().actor_value(id(0x900), "stamina");
    let health = game.world_mut().actor_value(id(0x901), "health");
    assert!(matches!(
        game.melee_hit(power_hit(0x900, 0x901, true)),
        Err(Error::Unsupported(
            "attacker has no race for power attack data"
        ))
    ));
    close(game.world_mut().actor_value(id(0x900), "stamina"), stamina);
    close(game.world_mut().actor_value(id(0x901), "health"), health);
}

#[test]
fn cmb_21_truncated_atkd_entry_rejected_atomically() {
    // TES5Edit's wbAttackData is a fixed 44 byte struct
    // (TES5Edit/Core/wbDefinitionsTES5.pas), and esm::cursor::Fields::exact
    // enforces that length exactly. A short ATKD subrecord, the kind a
    // stripped or hand-edited plugin can carry, must reject the power
    // attack rather than read past the end or silently misparse the flags
    // or multiplier fields, and must leave no partial hit behind.
    let mut race = race_with_atkd(0x700, &[]);
    race = race.sub(Tag(*b"ATKD"), vec![0_u8; 20]);
    let mut game = power_game(race);
    let stamina = game.world_mut().actor_value(id(0x900), "stamina");
    let health = game.world_mut().actor_value(id(0x901), "health");
    assert!(matches!(
        game.melee_hit(power_hit(0x900, 0x901, true)),
        Err(Error::Record(_))
    ));
    close(game.world_mut().actor_value(id(0x900), "stamina"), stamina);
    close(game.world_mut().actor_value(id(0x901), "health"), health);
}

#[test]
fn cmb_21_power_attack_stamina_cost_has_no_floor() {
    // ck-wiki/DamageActorValue_-_Actor.wikitext documents DamageActorValue
    // as taking the current value below zero freely, never refusing or
    // clamping at zero. world/combat.rs computes the whole hit, including
    // the stamina cost, before anything commits, so an attacker who started
    // the swing without enough stamina still pays the recorded cost and
    // still lands the swing, the same unclamped contract health_damage
    // itself uses.
    let race = race_with_atkd(0x700, &[(2.0, 1.5, FLAG_POWER)]);
    let mut game = power_game(race);
    game.world_mut()
        .set_actor_value(id(0x900), "stamina", 10.)
        .unwrap();
    let result = game.melee_hit(power_hit(0x900, 0x901, true)).unwrap();
    close(result.health_damage, 60.);
    close(game.world_mut().actor_value(id(0x900), "stamina"), -74.);
}

#[test]
fn cmb_21_onhit_event_reports_the_real_power_attack_flag() {
    // ck-wiki/OnHit_-_ObjectReference.wikitext documents Event
    // OnHit(ObjectReference akAggressor, Form akSource, Projectile
    // akProjectile, bool abPowerAttack, bool abSneakAttack, bool
    // abBashAttack, bool abHitBlocked), "abPowerAttack: Was the hit caused
    // by a power attack?" A power attack hit must report true there.
    let race = race_with_atkd(0x700, &[(2.0, 1.5, FLAG_POWER)]);
    let mut game = power_game(race);
    let object = power_observer(&mut game);
    game.melee_hit(power_hit(0x900, 0x901, true)).unwrap();
    assert_eq!(
        game.vm.get_var(object, "HitObserver", "power_seen"),
        Some(Value::Bool(true))
    );
}

#[test]
fn cmb_21_power_attack_stamina_cost_persists_across_save_and_load_once() {
    // Done criterion: the stamina cost is consumed once. A save taken right
    // after a power attack and loaded into a fresh game must carry that
    // single deduction forward, never reapplying or dropping it, the same
    // persistence combat_hits.rs's cmb_15 save/load tests prove for
    // AttackDamageMult.
    let race = race_with_atkd(0x700, &[(2.0, 1.5, FLAG_POWER)]);
    let mut game = power_game(race);
    game.melee_hit(power_hit(0x900, 0x901, true)).unwrap();
    close(game.world_mut().actor_value(id(0x900), "stamina"), 116.);
    let bytes = game.save_state().unwrap();
    let mut loaded = power_game(race_with_atkd(0x700, &[(2.0, 1.5, FLAG_POWER)]));
    loaded.load_state(&bytes).unwrap();
    close(loaded.world_mut().actor_value(id(0x900), "stamina"), 116.);
}
