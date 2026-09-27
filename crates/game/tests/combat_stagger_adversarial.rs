// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial coverage for CMB-14, "Apply stagger and recoil from hit
//! magnitude" (`crates/game/src/world/combat.rs`, `crates/game/src/combat.rs`,
//! `crates/game/src/game_settings.rs`, `crates/game/src/world/save.rs`).
//!
//! Done check: "A hit above the stagger threshold sets the stagger state
//! for its duration." Sources for every expectation below: `Weapon::stagger`
//! and `HitResult::staggered`'s doc comments (`crates/game/src/combat.rs`,
//! `crates/game/src/combat/records.rs`), `World::melee_hit`'s stagger block
//! (`stagger_magnitude > stagger_threshold`, both `combat_setting` calls
//! unconditional), `World::game_setting`'s plugin-GMST-then-engine-default
//! lookup (`crates/game/src/world/forms.rs`), and `read_actor_state`'s two
//! independent version gates, `exclusive_branch` at `version >= 24` and
//! `stagger_end_time` at `version >= 29` (`crates/game/src/world/save.rs`).
//!
//! `crates/game/tests/combat_hits.rs`'s own `weapon()` fixture never writes
//! a nonzero DNAM Stagger byte, so none of the suite's existing tests
//! exercise this field. This file builds its own weapon record with a
//! configurable Stagger fraction instead.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// A one-handed WEAP with a configurable DNAM Stagger fraction at byte
/// offset 96, the field `Weapon::from_record` reads last
/// (`crates/game/src/combat/records.rs`).
fn weapon(object: u32, damage: u16, skill: i32, stagger: f32) -> Record {
    let mut data = vec![0; 10];
    put(&mut data, 4, 8_f32.to_bits());
    data[8..10].copy_from_slice(&damage.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    put(&mut dnam, 4, 1_f32.to_bits());
    put(&mut dnam, 8, 1.1_f32.to_bits());
    put(&mut dnam, 76, skill as u32);
    put(&mut dnam, 88, u32::MAX);
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

/// `weapon_a` is always object `0xA00`, equipped by both fixture actors.
/// `weapon_b`, when given, must be object `0xA01`, also equipped by
/// attacker `0x900` alongside `0xA00`, so a hit can name either one
/// directly (`World::melee_hit` requires the named weapon to be in the
/// attacker's `equipped` set, not merely carried).
fn fixture(weapon_a: Record, weapon_b: Option<Record>, gmsts: &[(&str, f32)]) -> Game {
    let has_b = weapon_b.is_some();
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon_a);
    if let Some(weapon_b) = weapon_b {
        plugin.push_top(weapon_b);
    }
    settings(&mut plugin);
    for (index, (name, value)) in gmsts.iter().enumerate() {
        plugin.push_top(
            Record::new(Tag(*b"GMST"), 0x2000 + index as u32)
                .zsub(Tag(*b"EDID"), name)
                .sub(Tag(*b"DATA"), value.to_le_bytes()),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Stagger.esm", plugin).unwrap();
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
    if has_b {
        let mut world = game.world_mut();
        world.add_item(id(0x900), id(0xA01), 1);
        world.actor(id(0x900)).unwrap().equipped.insert(id(0xA01));
    }
    game
}

fn hit(attacker: u32, target: u32, weapon: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(weapon),
        is_bash: false,
        is_power_attack: false,
    }
}

/// A weapon whose DNAM Stagger fraction is 0.0 can never stagger:
/// `stagger_magnitude = health_damage * stagger_fraction` is always 0.0
/// regardless of `health_damage` (`Weapon::stagger`'s "zero means the
/// weapon never staggers" contract). But `World::melee_hit` still calls
/// `combat_setting("fStaggerThreshold")?` and `combat_setting("fStaggerDuration")?`
/// unconditionally, before checking whether the fraction is zero. A
/// malformed value for either setting, present anywhere in the load
/// order, must not stop a hit that could never have staggered from
/// resolving at all.
#[test]
fn cmb_14_zero_stagger_weapon_ignores_malformed_stagger_settings() {
    for setting in ["fStaggerThreshold", "fStaggerDuration"] {
        let mut game = fixture(weapon(0xA00, 10, 6, 0.0), None, &[(setting, -1.0)]);
        let result = game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap_or_else(|error| {
            panic!("a zero-stagger weapon's hit must resolve despite a malformed {setting}: {error}")
        });
        assert!(
            !result.staggered,
            "a zero-stagger weapon must never stagger"
        );
    }
}

/// `staggered = stagger_magnitude > stagger_threshold` is a strict
/// inequality, and the done check requires stagger "above" the threshold.
/// A hit landing exactly on the threshold must not stagger. The same hit
/// with the threshold nudged down by 0.01 must.
#[test]
fn cmb_14_boundary_hit_exactly_at_threshold_does_not_stagger() {
    let measured = fixture(
        weapon(0xA00, 10, 6, 0.0),
        None,
        &[("fStaggerThreshold", 1.0e9)],
    )
    .melee_hit(hit(0x900, 0x901, 0xA00))
    .unwrap()
    .health_damage;
    assert!(
        measured > 0.0,
        "the fixture must deal nonzero damage for this boundary to mean anything"
    );

    let mut at_threshold = fixture(
        weapon(0xA00, 10, 6, 1.0),
        None,
        &[("fStaggerThreshold", measured)],
    );
    let result = at_threshold.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();
    assert!(
        !result.staggered,
        "magnitude exactly equal to the threshold must not stagger"
    );

    let mut just_below = fixture(
        weapon(0xA00, 10, 6, 1.0),
        None,
        &[("fStaggerThreshold", measured - 0.01)],
    );
    let result = just_below.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();
    assert!(
        result.staggered,
        "magnitude above the threshold by 0.01 must stagger"
    );
}

/// No branch in `World::melee_hit` clears `stagger_end_time` for a later
/// hit that does not qualify. A hit from a zero-stagger weapon (never
/// qualifies, per the test above) must leave an already active stagger
/// window on the same target completely untouched, not shortened or reset.
#[test]
fn cmb_14_non_qualifying_hit_does_not_clear_active_stagger() {
    let mut game = fixture(
        weapon(0xA00, 10, 6, 1.0),
        Some(weapon(0xA01, 10, 6, 0.0)),
        &[("fStaggerThreshold", 0.01)],
    );
    let first = game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();
    assert!(
        first.staggered,
        "fixture setup must stagger on the first hit"
    );
    let deadline = game.world().actor_ref(id(0x901)).unwrap().stagger_end_time;
    assert!(deadline.is_some());

    let second = game.melee_hit(hit(0x900, 0x901, 0xA01)).unwrap();
    assert!(
        !second.staggered,
        "the zero-stagger weapon must not itself stagger"
    );
    assert_eq!(
        game.world().actor_ref(id(0x901)).unwrap().stagger_end_time,
        deadline,
        "a non-qualifying hit must not clear or shorten an active stagger window"
    );
}

/// docs/combat.md CMB-14: "Both stay overridable through the existing
/// plugin and live-setting GMST precedence World::combat_setting already
/// gives every other engine default here." A plugin-defined
/// `fStaggerThreshold` GMST record must be the value `World::game_setting`
/// resolves, not the 3.5 engine-default estimate in `game_settings.rs`.
#[test]
fn cmb_14_plugin_gmst_overrides_engine_default_threshold() {
    let mut low = fixture(
        weapon(0xA00, 10, 6, 1.0),
        None,
        &[("fStaggerThreshold", 0.001)],
    );
    assert!(
        low.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap().staggered,
        "a plugin threshold near zero must be read from the GMST record and stagger"
    );

    let mut high = fixture(
        weapon(0xA00, 10, 6, 1.0),
        None,
        &[("fStaggerThreshold", 1.0e9)],
    );
    assert!(
        !high.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap().staggered,
        "a plugin threshold far above any hit must be read from the GMST record and suppress stagger"
    );
}

/// `SAVE_VERSION` 29's doc history and `docs/combat.md` both commit to an
/// active stagger deadline surviving a save/load round trip exactly.
#[test]
fn cmb_14_stagger_state_survives_save_load_round_trip() {
    let mut game = fixture(
        weapon(0xA00, 10, 6, 1.0),
        None,
        &[("fStaggerThreshold", 0.001)],
    );
    let result = game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();
    assert!(result.staggered);
    assert!(game.world().is_staggered(id(0x901)));
    let deadline = game.world().actor_ref(id(0x901)).unwrap().stagger_end_time;

    let saved = game.save_state().unwrap();
    let mut loaded = fixture(
        weapon(0xA00, 10, 6, 1.0),
        None,
        &[("fStaggerThreshold", 0.001)],
    );
    loaded.load_state(&saved).unwrap();
    assert_eq!(loaded.world().state_hash(), game.world().state_hash());
    assert_eq!(
        loaded
            .world()
            .actor_ref(id(0x901))
            .unwrap()
            .stagger_end_time,
        deadline,
        "an active stagger deadline must survive a save/load round trip exactly"
    );
    assert!(loaded.world().is_staggered(id(0x901)));
}

/// The same unconditional-fetch defect the first test in this file proves
/// with a zero-fraction weapon also reaches a weapon that CAN stagger.
/// `fStaggerDuration` only feeds `stagger_deadline`, and `stagger_deadline`
/// is discarded whenever `stagger_magnitude` does not clear the threshold
/// (`World::melee_hit`, `if staggered { target.stagger_end_time = ... }`).
/// A hit that lands below the threshold must resolve even when
/// `fStaggerDuration` is malformed, because a deadline that is never
/// stored cannot need a valid duration to compute.
#[test]
fn cmb_14_sub_threshold_hit_ignores_malformed_stagger_duration() {
    let mut game = fixture(
        weapon(0xA00, 10, 6, 0.5),
        None,
        &[("fStaggerThreshold", 1.0e9), ("fStaggerDuration", -1.0)],
    );
    let result = game
        .melee_hit(hit(0x900, 0x901, 0xA00))
        .unwrap_or_else(|error| {
            panic!(
                "a hit that cannot cross an unreachable threshold must resolve despite \
             a malformed fStaggerDuration: {error}"
            )
        });
    assert!(
        !result.staggered,
        "fStaggerThreshold of 1e9 must not be crossed by this fixture's damage"
    );
}

/// Ordinary melee damage already resolves through the winning WEAP
/// record's DATA/DNAM bytes (docs/combat.md). The new stagger field must
/// follow the same winner resolution: a later plugin overriding the same
/// weapon FormID with a different DNAM Stagger fraction must have its own
/// value govern, not the base master's.
#[test]
fn cmb_14_winning_weapon_override_governs_stagger_fraction() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6, 0.0));
    settings(&mut plugin);
    plugin.push_top(
        Record::new(Tag(*b"GMST"), 0x2000)
            .zsub(Tag(*b"EDID"), "fStaggerThreshold")
            .sub(Tag(*b"DATA"), 0.001_f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Stagger.esm", plugin).unwrap();

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Stagger.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(weapon(0xA00, 10, 6, 1.0));
    order.add("StaggerPatch.esp", patch).unwrap();

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

    let result = game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();
    assert!(
        result.staggered,
        "the winning patch record's Stagger 1.0 must govern, not the base master's 0.0"
    );
}

/// `read_actor_state`'s `stagger_end_time` field is gated on `version >= 29`
/// (`crates/game/src/world/save.rs`), the version this row's SOEW bump
/// introduced. A save written by the prior build, version 28, carries no
/// bytes for the field at all and must still load, with every actor
/// defaulting to not staggered, the same "older saves without the new
/// state" contract `exclusive_branch`'s own `version >= 24` gate already
/// keeps for version 23 saves.
#[test]
fn cmb_14_version_28_save_loads_with_no_actor_staggered() {
    let mut game = fixture(
        weapon(0xA00, 10, 6, 1.0),
        None,
        &[("fStaggerThreshold", 0.001)],
    );
    let result = game.melee_hit(hit(0x900, 0x901, 0xA00)).unwrap();
    assert!(result.staggered, "fixture setup must stagger the target");
    assert!(game.world().is_staggered(id(0x901)));

    // Overwrite every actor's stagger_end_time with a sentinel unique enough
    // that its exact little-endian bytes cannot occur anywhere else in the
    // save, then cut the leading `Some` bool and the 8 sentinel bytes back
    // out, reproducing exactly the byte stream a version 28 engine (one
    // build before this row) would have written for the same world.
    let sentinels = [
        (id(0x14), 918_273_645.5_f64),
        (id(0x900), 918_273_646.5),
        (id(0x901), 918_273_647.5),
    ];
    {
        let mut world = game.world_mut();
        for (actor, value) in sentinels {
            world.actor(actor).unwrap().stagger_end_time = Some(value);
        }
    }
    let mut bytes = game.world().save_state();
    for (_, value) in sentinels {
        let mut marker = vec![1_u8];
        marker.extend_from_slice(&value.to_le_bytes());
        let positions: Vec<_> = bytes
            .windows(marker.len())
            .enumerate()
            .filter_map(|(offset, window)| (window == marker).then_some(offset))
            .collect();
        assert_eq!(
            positions.len(),
            1,
            "stagger sentinel {value} must be unique in the save byte stream"
        );
        // Version 31 writes each actor's witnessed crime set right after the
        // stagger slot. This fixture witnessed nothing, so the set is a zero
        // count, which a version 28 layout never carried either. Version 32
        // writes the left and right hand spell slots right after that, one
        // presence byte each, both unset here.
        let end = positions[0] + marker.len();
        assert_eq!(
            bytes[end..end + 4],
            [0, 0, 0, 0],
            "an actor with no witnessed crimes must write an empty set after its stagger slot"
        );
        assert_eq!(
            bytes[end + 4..end + 6],
            [0, 0],
            "an actor with no hand spells must write two unset presence bytes after its crime set"
        );
        bytes.drain(positions[0]..end + 6);
    }
    assert_eq!(
        u16::from_le_bytes([bytes[4], bytes[5]]),
        soe_game::world::SAVE_VERSION,
        "the save must carry the current world state version before the patch"
    );
    // Version 28 predates the learned-enchantment set at the save tail.
    assert_eq!(bytes[bytes.len() - 4..], [0; 4]);
    bytes.truncate(bytes.len() - 4);
    bytes[4..6].copy_from_slice(&28_u16.to_le_bytes());

    let loaded = fixture(
        weapon(0xA00, 10, 6, 1.0),
        None,
        &[("fStaggerThreshold", 0.001)],
    );
    loaded.world_mut().load_state(&bytes).unwrap_or_else(|error| {
        panic!("a version 28 save, one version below stagger_end_time's gate, must still load: {error}")
    });
    for actor in [id(0x14), id(0x900), id(0x901)] {
        assert!(
            !loaded.world().is_staggered(actor),
            "a version 28 save carries no stagger bytes, every actor must default to not staggered"
        );
        assert_eq!(
            loaded.world().actor_ref(actor).unwrap().stagger_end_time,
            None,
            "a version 28 save carries no stagger bytes, the field must default to None"
        );
    }
}
