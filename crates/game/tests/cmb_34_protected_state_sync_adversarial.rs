// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Breaker pass for CMB-34 (`crates/game/src/world/combat.rs::melee_hit`,
//! docs/combat.md "Player-only kill exception for protected actors, CMB-34").
//!
//! `player_exception` and `bleedout_target` are computed from
//! `ActorState::protected`, one field on the per-reference runtime state
//! `World::actor` builds. That field is set once, from the winning `ACBS`
//! record's flags (`crates/game/src/world/actors.rs`, `a.protected =
//! flags.is_protected()`), and nothing else ever writes it.
//!
//! Contrast with `ActorState::essential` two lines above it in the same
//! function: it is read through `self.actor_bases.get(&flags_source)` first,
//! falling back to the record only when no runtime override exists. The
//! `ActorBase.SetEssential` Papyrus native goes through
//! `World::set_actor_base_essential` (`crates/game/src/world/death.rs`),
//! which explicitly walks every already-built `ActorState` sharing that base
//! and updates `essential` live. `combat_bleedout_recovery_adversarial.rs`
//! documents the defect that function was added to fix.
//!
//! `ActorBase.SetProtected` (`crates/game/src/natives.rs`, the `SetProtected`
//! arm of the `SetEssential`/`SetProtected`/`SetInvulnerable` native group)
//! has no such counterpart: it only writes `World::actor_base(me).protected`,
//! the per-base cache `ActorBase.IsProtected` reads back. It never touches
//! `self.actors`. ck-wiki's ActorBase_Script page
//! (`../engine-refs/ck-wiki/ActorBase_Script.wikitext`, `SetProtected`)
//! states the setter "Flags this actor as protected or not", with no
//! carve-out for actors already loaded. These tests call the native's own
//! Rust body directly (`World::actor_base(id).protected = ...`, exactly what
//! `natives.rs` runs), then drive `melee_hit`, CMB-34's own contract, to show
//! `IsProtected()` and the actual combat outcome disagree.
//!
//! This file only adds tests. It does not modify the files under test.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::Error, combat::MeleeHit, npc::flags, Game};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// Mirrors combat_bleedout_recovery.rs's weapon fixture exactly: 15 damage
/// against the settings block below, with no armor and no block.
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
/// (0x901) and the player. `target_flags` seeds the on-disk `ACBS` flags;
/// tests then layer a runtime `SetProtected` call on top through
/// `World::actor_base`.
fn fixture(target_flags: u32) -> Game {
    let mut acbs = vec![0; 24];
    acbs[..4].copy_from_slice(&target_flags.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800).sub(Tag(*b"ACBS"), acbs));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x801));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x901).sub(Tag(*b"NAME"), 0x801_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Cmb34Sync.esm", plugin).unwrap();
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

fn player_hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x14),
        target: id(0x900),
        weapon: id(0xA00),
        is_bash: false,
    }
}

fn npc_hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x901),
        target: id(0x900),
        weapon: id(0xA00),
        is_bash: false,
    }
}

/// A script that calls `SetProtected(true)` on a target's base before that
/// target has ever been hit (so `World::actor` has never built its
/// `ActorState`) must make the target protected: an NPC hit that would
/// otherwise be lethal must clamp into bleedout, and the CMB-34 exception
/// must then let only the player finish it. `ActorBase.IsProtected()` agrees
/// immediately (it reads the same cache the setter wrote), so a mod checking
/// its own state back sees success while combat still treats the target as
/// unprotected: `a.protected = flags.is_protected()` in
/// `crates/game/src/world/actors.rs` never consults `self.actor_bases`.
#[test]
fn cmb_34_set_protected_before_first_build_reaches_live_combat_state() {
    let mut game = fixture(0);
    {
        let mut world = game.world_mut();
        // This is exactly natives.rs's SetProtected native body: the
        // "field == 1" arm writes only `World::actor_base(me).protected`.
        world.actor_base(id(0x800)).unwrap().protected = true;
        assert!(
            world.actor_base(id(0x800)).unwrap().protected,
            "sanity: the setter must land on the base cache IsProtected reads"
        );
    }
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let result = game.melee_hit(npc_hit()).unwrap();

    assert!(
        !result.killed,
        "SetProtected(true) must protect the target from an NPC's lethal hit \
         even though the target was never touched before the script call"
    );
    let mut world = game.world_mut();
    let target = world.actor(id(0x900)).unwrap();
    assert!(
        target.unconscious,
        "a protected target enters bleedout on an NPC hit, per CMB-34 and CMB-13"
    );
    assert!(!target.dead);
}

/// The reverse of the case above, and the one CMB-34 itself depends on
/// directly: `SetProtected(false)` on a target whose on-disk `ACBS` flags
/// set `PROTECTED` is the "scene is over, they can die now" pattern the
/// sibling `essential` case in `combat_bleedout_recovery_adversarial.rs`
/// documents. Once lifted, the target must behave like any other actor: an
/// NPC hit kills it outright, and the player gets no special exception
/// because there is no more protection to except from. `melee_hit` still
/// reads a live `target.protected` of `true` (the value the first build
/// derived from the record and cached, since nothing after that ever
/// rewrites it), so the target stays gated into bleedout for every attacker,
/// and the player-only kill exception fires on a target the script tried to
/// fully unprotect.
#[test]
fn cmb_34_set_protected_false_after_build_does_not_lift_the_bleedout_gate() {
    let mut game = fixture(flags::PROTECTED);
    // Build the ActorState once, at its on-disk value, before the script
    // call. A player hit that misses this window (test above) already shows
    // SetProtected is ignored before a build too, so this isolates the
    // "already active" half of the gap.
    game.world_mut().actor(id(0x900)).unwrap();
    assert!(
        game.world_mut().actor(id(0x900)).unwrap().protected,
        "sanity: the record flag must have landed on the live actor first"
    );

    game.world_mut().actor_base(id(0x800)).unwrap().protected = false;
    assert!(
        !game.world_mut().actor_base(id(0x800)).unwrap().protected,
        "sanity: IsProtected() must now report false"
    );

    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();
    let result = game.melee_hit(npc_hit()).unwrap();

    assert!(
        result.killed,
        "SetProtected(false) must let a plain NPC hit kill the target once \
         protection is lifted; instead the stale live flag still clamps it \
         into bleedout"
    );
    assert!(game.world_mut().actor(id(0x900)).unwrap().dead);
}

/// `melee_hit`'s own doc comment states "rejected hits do not damage or emit
/// events." CMB-34 inserted `player_exception` and `bleedout_target` ahead of
/// every existing validation in the function, both read-only against
/// `target`. A hit that is rejected afterward, for an unrelated reason (here,
/// a weapon `GlobalFormId` with no backing record at all), must still leave
/// the target's health, `dead` and `unconscious` fields completely
/// untouched, proving the new CMB-34 computation did not open a partial-
/// mutation window ahead of the existing atomicity guarantee.
#[test]
fn cmb_34_rejected_hit_leaves_protected_target_state_untouched() {
    let mut game = fixture(flags::PROTECTED);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();

    let before = {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        (
            actor.values.get("health").current(),
            actor.dead,
            actor.unconscious,
        )
    };

    let bad_weapon = id(0xBAD);
    let result = game.melee_hit(MeleeHit {
        attacker: id(0x14),
        target: id(0x900),
        weapon: bad_weapon,
        is_bash: false,
    });

    assert!(
        matches!(result, Err(Error::Equipment)),
        "a weapon with no backing record must fail with Error::Equipment, got {result:?}"
    );
    let mut world = game.world_mut();
    let actor = world.actor(id(0x900)).unwrap();
    assert_eq!(
        (
            actor.values.get("health").current(),
            actor.dead,
            actor.unconscious
        ),
        before,
        "a rejected hit must not change health, dead or unconscious state"
    );
}

/// A malformed or half-uninstalled load order can leave an `ACHR`'s `NAME`
/// pointing at a form that carries no `NPC_` record at all (a missing
/// master, a deleted base, a cut mod). docs/style.md states "malformed input
/// returns a typed error. It never panics." CMB-34's own new code
/// (`player_exception`, `bleedout_target`) sits between the actor lookups
/// and the rest of `melee_hit`, so a dangling target base must still be
/// caught by `validate_combat_actor` before either read runs, not partway
/// through them.
#[test]
fn cmb_34_dangling_target_base_errors_instead_of_panicking() {
    let mut plugin = Plugin::new(FileHeader::default());
    // 0x900's NAME resolves to 0x800, which carries no record of any tag.
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    plugin.push_top(weapon(0xA00, 10, 6));
    settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Cmb34Dangling.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "onehanded", 100.)
        .unwrap();
    game.world_mut().add_item(player, id(0xA00), 1);
    game.world_mut()
        .actor(player)
        .unwrap()
        .equipped
        .insert(id(0xA00));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        game.melee_hit(player_hit())
    }));

    let result = result.expect("a dangling target base must not panic melee_hit");
    assert!(
        matches!(result, Err(Error::Actor(_))),
        "a dangling target base must fail validate_combat_actor with Error::Actor, got {result:?}"
    );
}
