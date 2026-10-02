// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Breaker pass for CMB-36 (`crates/game/src/world/save.rs::World::save_state`,
//! docs/combat.md "Application and events").
//!
//! CMB-36 made no code change. It named an existing contract explicitly:
//! docs/combat.md now states that a rejected first melee hit which reads
//! `protected`, `essential` or `invulnerable` through `World::actor_base`
//! (`crates/game/src/world/forms.rs`) persists that actor's base into
//! `save_state` even though the hit itself is never applied, "the same
//! contract crafting already documents for rejected ... increment". The
//! save.rs comment at the `actor_bases` serialization site gives the same
//! reason: the map carries mutable native state (`SetProtected`,
//! `SetEssential`, `dead_count`) alongside record-derived fields with no
//! marker for which came from which, so there is no rejected-only subset to
//! exclude.
//!
//! These tests attack whether that documented contract actually holds:
//! whether a base built only to answer a rejected hit's read truly survives
//! a save/load round trip (not just "doesn't crash"), whether that holds
//! regardless of *why* the hit was rejected, and whether a malformed NPC
//! template chain elsewhere in the same record can quietly break the
//! contract instead of erroring per docs/style.md ("malformed input returns
//! a typed error. It never panics.").
//!
//! This file only adds tests. It does not modify the files under test.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{Game, combat::Error, combat::MeleeHit, npc::flags};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

/// Mirrors combat_bleedout_recovery.rs and cmb_34's weapon fixture exactly:
/// 15 damage against the settings block below, with no armor and no block.
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
                .sub(Tag(*b"EDID"), name.as_bytes().to_vec())
                .sub(Tag(*b"DATA"), value.to_le_bytes().to_vec()),
        );
    }
}

/// One target actor (0x900) on base 0x800, plus a baseless NPC attacker
/// (0x901) and the player. `target_flags` seeds the on-disk `ACBS` flags.
/// A fresh `Game` built from this same plugin data is the only way to test
/// whether a value on the loaded side was truly carried by `save_state`
/// rather than freshly recomputed from the winning record: if it was not
/// persisted, `actor_base` on the loaded side falls back to recomputing it
/// from this same on-disk data and reports the *unset* value instead.
fn build_plugin(target_flags: u32) -> Plugin {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&target_flags.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    plugin
}

/// A live, playable `Game`: the player and NPC attacker are equipped and
/// skilled enough to land the weapon fixture's hits.
fn fixture(target_flags: u32) -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Cmb36Save.esm", build_plugin(target_flags))
        .unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    assert_eq!(player, id(0x14));
    for actor in [player, id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "onehanded", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

/// A second `Game` built from the identical plugin bytes, standing in for
/// "the same save file reopened": the target for `load_state`, per
/// `crates/game/src/save.rs`'s own contract that the load side must be
/// built against the same load order.
fn load_target(target_flags: u32) -> Game {
    let mut order = LoadOrder::new();
    order
        .add("Cmb36Save.esm", build_plugin(target_flags))
        .unwrap();
    Game::new(order)
}

fn npc_hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x901),
        target: id(0x900),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

/// The core CMB-36 claim, tested directly rather than trusted: prime the
/// target's base with `invulnerable = true` (the `SetInvulnerable` native's
/// own write, `crates/game/src/natives.rs`) while the on-disk `ACBS` leaves
/// the flag unset. Trigger a melee hit that is rejected *because of* that
/// same read (`Error::Unsupported("invulnerable target")`,
/// `crates/game/src/world/combat.rs`), then save and load into a game built
/// from the same otherwise-unset record. If the persisted contract holds,
/// the loaded side reports `invulnerable == true` straight from the save.
/// If the "no rejected-only cache" comment in save.rs is wrong and this
/// entry never actually reached `save_state`, `actor_base` on the loaded
/// side recomputes it fresh from the unset `ACBS` flag and reports `false`.
#[test]
fn cmb_36_invulnerable_lazy_actor_base_persists_across_save_load() {
    let mut game = fixture(0);
    {
        let mut world = game.world_mut();
        world.actor_base(id(0x800)).unwrap().invulnerable = true;
    }

    let result = game.melee_hit(npc_hit());
    assert!(
        matches!(result, Err(Error::Unsupported("invulnerable target"))),
        "sanity: the primed base must actually reject the hit, got {result:?}"
    );

    let bytes = game.save_state().unwrap();
    let mut loaded = load_target(0);
    loaded.load_state(&bytes).unwrap();

    assert!(
        loaded
            .world_mut()
            .actor_base(id(0x800))
            .unwrap()
            .invulnerable,
        "a lazily-initialized actor base that only ever rejected a hit must \
         still round-trip through save_state, per docs/combat.md's \
         Application and events section; a loaded value of false means the \
         entry was silently dropped and quietly recomputed from the \
         unrelated on-disk ACBS flag instead"
    );
}

/// The same claim, but with the rejection reason deliberately unrelated to
/// the divergent flag: the protected/essential/invulnerable read in
/// `melee_hit` (`crates/game/src/world/combat.rs`, lines ahead of the
/// weapon lookup) runs before the equipment check that actually rejects
/// this hit (`Error::Equipment`, no backing record for the weapon id, the
/// same shape `cmb_34_rejected_hit_leaves_protected_target_state_untouched`
/// uses). docs/combat.md's claim is general, "a rejected first melee hit
/// that reads ... through actor_base", not scoped to invulnerability
/// rejections specifically, so this must persist exactly like the case
/// above.
#[test]
fn cmb_36_protected_lazy_actor_base_persists_through_unrelated_equipment_rejection() {
    let mut game = fixture(0);
    {
        let mut world = game.world_mut();
        world.actor_base(id(0x800)).unwrap().protected = true;
    }

    let result = game.melee_hit(MeleeHit {
        attacker: id(0x901),
        target: id(0x900),
        weapon: id(0xBAD),
        is_bash: false,
        is_power_attack: false,
    });
    assert!(
        matches!(result, Err(Error::Equipment)),
        "sanity: a weapon with no backing record must fail with \
         Error::Equipment, got {result:?}"
    );

    let bytes = game.save_state().unwrap();
    let mut loaded = load_target(0);
    loaded.load_state(&bytes).unwrap();

    assert!(
        loaded.world_mut().actor_base(id(0x800)).unwrap().protected,
        "the divergent protected flag must persist even though the hit was \
         rejected for an unrelated reason (bad weapon, not the protected \
         read itself); docs/combat.md claims no rejected-only cache exists \
         to exclude it regardless of which check rejected the hit"
    );
}

/// `World::actor` (`crates/game/src/world/actors.rs`) resolves the same four
/// template groups `World::actor_base` needs (`BASE_DATA`, `TRAITS` through
/// `actor_component_source`, `STATS`, `INVENTORY`) plus three more
/// (`SPELLS`, `FACTIONS`, `AI_DATA`): every one of `actor_base`'s
/// prerequisites is also one of `actor`'s. Confirmed by reading both
/// builders (`crates/game/src/world/forms.rs::actor_base`,
/// `crates/game/src/world/actors.rs::actor`) rather than assumed: there is
/// no template group whose break isolates `actor_base` failing from `actor`
/// succeeding, because `actor_component_source`'s non-leveled path
/// (`crates/game/src/world/templates.rs::reference_component_source`) falls
/// through to the identical `npc_component_source` walk `actor_base` uses
/// via `npc_part`. A malformed `TPLT` chain on the target therefore breaks
/// the target's `World::actor` build first, at `validate_combat_actor`
/// (`crates/game/src/world/combat.rs`), before `melee_hit` ever reaches the
/// `actor_base` read this file's other tests exercise.
///
/// This test attacks the resulting claim instead: that the target-side
/// failure surfaces as `Error::Actor`, not a panic, and that a load order
/// carrying the malformed record still saves cleanly, per docs/style.md:
/// "malformed input returns a typed error. It never panics." The attacker
/// (0x901) is fully set up (skill, weapon, equip) so the rejection can only
/// come from the target's own build, not from the attacker's.
#[test]
fn cmb_36_malformed_target_template_chain_rejects_without_panic() {
    let mut acbs = vec![0; 24];
    put(&mut acbs, 0, flags::PROTECTED);
    // INVENTORY = 0x0100 (crates/game/src/npc.rs, mod template), gating the
    // TPLT chain walk for the INVENTORY group.
    put16(&mut acbs, 18, 0x0100);
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(Tag(*b"NPC_"), 0x800)
            .sub(Tag(*b"ACBS"), acbs)
            // Points at 0x999, which carries no record of any tag: the
            // INVENTORY group's template walk cannot resolve it.
            .sub(Tag(*b"TPLT"), 0x999_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Cmb36BrokenTemplate.esm", plugin).unwrap();
    let mut game = Game::new(order);
    // Only the attacker (0x901) needs a skill and an equipped weapon to
    // reach the target-side build. Setup never touches the malformed target
    // (0x900), so any failure below comes from the target's own template
    // chain.
    {
        let mut world = game.world_mut();
        world.set_actor_value(id(0x901), "onehanded", 100.).unwrap();
        world.add_item(id(0x901), id(0xA00), 1);
        world.actor(id(0x901)).unwrap().equipped.insert(id(0xA00));
    }

    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| game.melee_hit(npc_hit())))
            .expect("a malformed target template chain must not panic melee_hit");

    assert!(
        matches!(result, Err(Error::Actor(target)) if target == id(0x900)),
        "a target whose own World::actor build cannot resolve its template \
         chain must reject the hit with the typed Error::Actor error \
         validate_combat_actor already returns for a missing actor, not \
         panic and not silently succeed, got {result:?}"
    );

    // The same malformed record must not make save_state itself panic or
    // error, whether or not the target's actor/actor_base cache was ever
    // built.
    let bytes = game.save_state();
    assert!(
        bytes.is_ok(),
        "a target whose actor and actor_base could never be built must \
         still save cleanly, got {bytes:?}"
    );
}

/// "Engine save load continuation": a base populated only through a
/// rejected hit must not just survive one round trip, it must still be
/// there after the save it produced is itself reloaded and resaved,
/// exactly as any other native-written field on `ActorBaseState` would be.
/// Layers `SetInvulnerable`-then-reject (generation 1) with
/// `SetEssential`-after-reload (generation 2, no combat involved) to prove
/// the two divergences accumulate correctly rather than one clobbering or
/// dropping the other across generations.
#[test]
fn cmb_36_actor_base_divergence_survives_two_save_load_generations() {
    let mut gen1 = fixture(0);
    {
        let mut world = gen1.world_mut();
        world.actor_base(id(0x800)).unwrap().invulnerable = true;
    }
    assert!(matches!(
        gen1.melee_hit(npc_hit()),
        Err(Error::Unsupported("invulnerable target"))
    ));
    let bytes1 = gen1.save_state().unwrap();

    let mut gen2 = load_target(0);
    gen2.load_state(&bytes1).unwrap();
    assert!(
        gen2.world_mut().actor_base(id(0x800)).unwrap().invulnerable,
        "generation 1's divergence must survive its own round trip before \
         generation 2 adds anything"
    );
    gen2.world_mut().actor_base(id(0x800)).unwrap().essential = true;
    let bytes2 = gen2.save_state().unwrap();

    let mut gen3 = load_target(0);
    gen3.load_state(&bytes2).unwrap();
    let mut world = gen3.world_mut();
    let base = world.actor_base(id(0x800)).unwrap();
    assert!(
        base.invulnerable,
        "generation 1's SetInvulnerable-then-reject divergence must still \
         be present after a second, unrelated save/load cycle"
    );
    assert!(
        base.essential,
        "generation 2's SetEssential must also be present alongside it, \
         proving the two writes accumulate across generations instead of \
         one save overwriting the other back to record defaults"
    );
}
