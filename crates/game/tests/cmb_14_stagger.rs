// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verifies CMB-14's done check: a hit above the stagger threshold sets the
//! stagger state for its duration. Independent WEAP bytes and a synthetic
//! two-actor plugin, following the fixture pattern in combat_hits.rs.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    combat::{Difficulty, MeleeHit},
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A one-handed WEAP record with a configurable DNAM Stagger fraction
/// (offset 96..100 of the 100-byte DNAM, per crates/game/src/combat/records.rs).
fn weapon(object: u32, damage: u16, stagger: f32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1; // animation: one-hand sword
    put(&mut dnam, 4, 1_f32.to_bits()); // speed
    put(&mut dnam, 8, 1.1_f32.to_bits()); // reach
    put(&mut dnam, 76, 6); // skill: one-handed
    put(&mut dnam, 88, u32::MAX); // resist: -1 (no resistance type)
    put(&mut dnam, 96, stagger.to_bits());
    Record::new(Tag(*b"WEAP"), object)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
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

/// Two ACHR actors sharing one bare NPC_ base, plus a weapon record the
/// fixture equips on both, following combat_hits.rs's `fixture` helper.
fn fixture(weapon_record: Record, weapon_id: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon_record);
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Stagger.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 1000.).unwrap();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(weapon_id), 1);
        world.actor(actor).unwrap().equipped.insert(id(weapon_id));
    }
    game.set_difficulty(Difficulty::Adept);
    game
}

fn hit(attacker: u32, target: u32, weapon_id: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(weapon_id),
        is_bash: false,
        is_power_attack: false,
    }
}

/// The done check's core clause: a hit above the stagger threshold sets the
/// stagger state, and it holds for the configured duration then clears.
///
/// Fixture: a 10-damage one-handed sword (0xA00, Stagger fraction 1.0) swung
/// by a full-skill attacker. Skill scaling (fDamagePCSkillMax 1.5) and the
/// unmodified Adept difficulty multiplier (1.0, no GMST override) against an
/// unarmored target give health_damage = 10 * 1.5 = 15, so the stagger
/// magnitude (health_damage * stagger fraction = 15.0) clears the engine's
/// fStaggerThreshold default of 3.5 (crates/game/src/game_settings.rs).
/// fStaggerDuration defaults to 1.2 seconds from the same table.
#[test]
fn cmb_14_hit_above_threshold_sets_stagger_for_its_duration() {
    let mut game = fixture(weapon(0xA00, 10, 1.0), 0xA00);
    let result = game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();

    assert!(
        result.staggered,
        "hit magnitude {} should cross fStaggerThreshold",
        result.health_damage
    );
    assert!(
        game.world().is_staggered(id(0x901)),
        "target should be staggered immediately after the hit"
    );

    // Still inside the 1.2s window.
    game.tick(1.0);
    assert!(
        game.world().is_staggered(id(0x901)),
        "target should remain staggered before fStaggerDuration elapses"
    );

    // Past the window: the lazy expiry in World::is_staggered clears it.
    game.tick(0.3);
    assert!(
        !game.world().is_staggered(id(0x901)),
        "target should no longer be staggered after fStaggerDuration elapses"
    );
}

/// Negative control: a hit whose magnitude does not cross the threshold
/// leaves the target unstaggered, showing "above the stagger threshold" is
/// an actual gate rather than every hit setting the state.
#[test]
fn cmb_14_hit_below_threshold_does_not_stagger() {
    // Stagger fraction 0.1 against the same 15.0 base damage gives magnitude
    // 1.5, below the 3.5 default threshold.
    let mut game = fixture(weapon(0xA00, 10, 0.1), 0xA00);
    let result = game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();

    assert!(
        !result.staggered,
        "hit magnitude {} should not cross fStaggerThreshold",
        result.health_damage
    );
    assert!(
        !game.world().is_staggered(id(0x901)),
        "target should not be staggered by a sub-threshold hit"
    );
}

/// A second qualifying hit while already staggered refreshes the deadline to
/// a full new duration rather than leaving the earlier, closer deadline in
/// place, matching the field's "sets the stagger state for its duration"
/// wording for each qualifying hit.
#[test]
fn cmb_14_second_qualifying_hit_extends_the_stagger_window() {
    let mut game = fixture(weapon(0xA00, 10, 1.0), 0xA00);
    game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();
    assert!(game.world().is_staggered(id(0x901)));

    // Advance most of the way through the first window, then land a second
    // qualifying hit: the window should extend, not merely hold.
    game.tick(1.0);
    assert!(game.world().is_staggered(id(0x901)));
    game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();

    // 1.0s past the second hit is still within its own 1.2s duration, even
    // though it is 2.0s past the first hit, whose window alone would have
    // expired.
    game.tick(1.0);
    assert!(
        game.world().is_staggered(id(0x901)),
        "a fresh qualifying hit should extend the stagger window"
    );
}
