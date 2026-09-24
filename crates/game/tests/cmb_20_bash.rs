// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Verifies CMB-20's done check: a bash consumes stamina once and raises a
//! hit plus the configured stagger response. Independent ARMO bytes and a
//! synthetic two-actor plugin, following the fixture pattern in
//! combat_hits.rs and cmb_14_stagger.rs.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, Game, Handle};
use soe_papyrus::{
    asm::{i, ident, FunctionBuilder, ScriptBuilder},
    Opcode::*,
    Value,
};

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

/// A shield-slot ARMO record (BOD2 bit 9 set, TES5Edit's `wbBipedObjectFlags`
/// "39 - Shield"), with a configurable DNAM rating in hundredths.
fn shield(object: u32, rating_hundredths: u32) -> Record {
    let mut body = (1_u32 << 9).to_le_bytes().to_vec();
    body.extend_from_slice(&2_u32.to_le_bytes());
    Record::new(Tag(*b"ARMO"), object)
        .sub(
            Tag(*b"DATA"),
            [0_i32.to_le_bytes(), 5_f32.to_le_bytes()].concat(),
        )
        .sub(Tag(*b"DNAM"), rating_hundredths.to_le_bytes())
        .sub(Tag(*b"BOD2"), body)
}

fn gmst(object: u32, name: &str, value: f32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

/// Baseline damage/armor settings, mirrored from combat_hits.rs so an
/// unarmored, unblocked bash stays deterministic. Stagger GMSTs are left at
/// the engine defaults documented in game_settings.rs (fStaggerThreshold
/// 3.5, fStaggerDuration 1.2), the same choice cmb_14_stagger.rs makes.
fn base_settings(plugin: &mut Plugin) {
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
        plugin.push_top(gmst(0x1000 + index as u32, name, value));
    }
}

/// Two plain NPC actors (0x900 attacker, 0x901 target), neither the player,
/// so difficulty stays fixed at 1.0. `shield_rating` is the DNAM rating in
/// hundredths for the shield the attacker starts equipped with at 0xB00.
fn fixture(shield_rating: u32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(shield(0xB00, shield_rating));
    base_settings(&mut plugin);
    let mut order = LoadOrder::new();
    order.add("Bash.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 1000.).unwrap();
    }
    {
        let mut world = game.world_mut();
        world.set_actor_value(id(0x900), "stamina", 100.).unwrap();
        world.add_item(id(0x900), id(0xB00), 1);
        world.actor(id(0x900)).unwrap().equipped.insert(id(0xB00));
    }
    game
}

fn hit_bash(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0xB00),
        is_bash: true,
    }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// Attaches a script to `target` that records each OnHit call's `bash`
/// argument and a running count, so a test can assert the hit actually
/// reached the event pipeline rather than only that `melee_hit` returned Ok.
fn bash_observer(game: &mut Game, target: u32) -> soe_papyrus::ObjectId {
    let function = FunctionBuilder::new("OnHit", "None")
        .param("aggressor", "ObjectReference")
        .param("source", "Form")
        .param("projectile", "Projectile")
        .param("power", "Bool")
        .param("sneak", "Bool")
        .param("bash", "Bool")
        .param("blocked", "Bool")
        .ins(Assign, vec![ident("lastBash"), ident("bash")])
        .ins(IAdd, vec![ident("hits"), ident("hits"), i(1)]);
    let script = ScriptBuilder::new("BashObserver", "Actor")
        .variable("hits", "Int", i(0))
        .variable("lastBash", "Bool", soe_papyrus::asm::b(false))
        .function(function.build());
    game.load_script(&script.bytes()).unwrap();
    let object = game.object_of(Handle::Form(id(target)));
    game.vm.attach(object, "BashObserver").unwrap();
    object
}

/// Done check clause 1: "a bash consumes stamina once." A single successful
/// bash must reduce the attacker's stamina by exactly `fBashStaminaCost`
/// (engine default 25.0, game_settings.rs), not by zero and not by double.
#[test]
fn cmb_20_bash_consumes_stamina_exactly_once() {
    // Rating 100 (below fStaggerThreshold 3.5? no: 1.0 -> well below any
    // stagger concern) keeps this test focused on stamina alone.
    let mut game = fixture(100);
    game.melee_hit(hit_bash(0x900, 0x901)).unwrap();
    close(game.world_mut().actor_value(id(0x900), "stamina"), 75.);

    // A second bash consumes the cost again, confirming the first spend was
    // exactly one charge, not an accidental double or missing deduction that
    // a single before/after snapshot could not distinguish.
    game.melee_hit(hit_bash(0x900, 0x901)).unwrap();
    close(game.world_mut().actor_value(id(0x900), "stamina"), 50.);
}

/// Done check clause 2: "raises a hit." A successful bash must land through
/// the same event pipeline an ordinary hit uses: `HitResult::bashed` true,
/// finite positive damage, target health reduced, and exactly one `OnHit`
/// delivered to the target with `abBashAttack` true (ck-wiki
/// `OnHit_-_ObjectReference.wikitext`, argument index 5 of 7).
#[test]
fn cmb_20_bash_raises_a_hit() {
    let mut game = fixture(1000);
    let object = bash_observer(&mut game, 0x901);
    let result = game.melee_hit(hit_bash(0x900, 0x901)).unwrap();

    assert!(result.bashed, "HitResult::bashed must report the bash flag");
    assert!(result.health_damage > 0.0, "a bash must deal damage");
    close(result.health_damage, 10.0);

    assert!(game.run_until_idle(32));
    close(game.world_mut().actor_value(id(0x901), "health"), 990.0);
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "hits"),
        Some(Value::Int(1)),
        "a landed bash must fire exactly one OnHit"
    );
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "lastBash"),
        Some(Value::Bool(true)),
        "OnHit's abBashAttack argument must be true for a bash"
    );
}

/// Done check clause 3: "the configured stagger response." A bash whose
/// shield rating alone clears `fStaggerThreshold` (3.5, game_settings.rs)
/// must stagger the target for exactly `fStaggerDuration` (1.2s), the same
/// threshold-and-duration mechanism CMB-14 established for weapon swings,
/// not an unconditional stagger on every bash regardless of magnitude.
#[test]
fn cmb_20_bash_above_threshold_triggers_configured_stagger_response() {
    // Unarmored target, no block, NPC vs NPC difficulty 1.0: health_damage
    // equals the shield rating, 10.0, above the 3.5 default threshold.
    let mut game = fixture(1000);
    let result = game.melee_hit(hit_bash(0x900, 0x901)).unwrap();
    assert!(
        result.staggered,
        "bash magnitude {} should cross fStaggerThreshold",
        result.health_damage
    );
    assert!(game.world().is_staggered(id(0x901)));

    game.tick(1.0);
    assert!(
        game.world().is_staggered(id(0x901)),
        "target should remain staggered before fStaggerDuration elapses"
    );
    game.tick(0.3);
    assert!(
        !game.world().is_staggered(id(0x901)),
        "target should no longer be staggered after fStaggerDuration elapses"
    );
}

/// Negative control for the same clause: a bash whose magnitude does not
/// cross `fStaggerThreshold` must not stagger, showing the bash's fixed
/// stagger_fraction of 1.0 still routes through a real threshold gate
/// rather than staggering unconditionally.
#[test]
fn cmb_20_bash_below_threshold_does_not_stagger() {
    // Rating 100 hundredths -> health_damage 1.0, below the 3.5 threshold.
    let mut game = fixture(100);
    let result = game.melee_hit(hit_bash(0x900, 0x901)).unwrap();
    assert!(
        !result.staggered,
        "bash magnitude {} should not cross fStaggerThreshold",
        result.health_damage
    );
    assert!(!game.world().is_staggered(id(0x901)));
}
