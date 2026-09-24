// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Breaker pass for CMB-20 (shield bash: stamina cost and stagger).
//!
//! Contract under test, from docs/combat.md CMB-20 and
//! crates/game/src/world/combat.rs:
//! - A bash source must carry BOD2 shield slot bit 9 (TES5Edit's
//!   `wbBipedObjectFlags`, "39 - Shield"), or the hit is rejected before any
//!   state changes.
//! - `melee_hit`'s own doc comment states that lazy record views may
//!   initialize on any call, but rejected hits do not damage or emit
//!   events, and the bash branch's comment says a bash that fails partway
//!   through never spends stamina. Tests below check that exact contract
//!   (no health change, no stamina change, no OnHit call) rather than full
//!   `save_state()` equality, since the doc comment explicitly excludes
//!   lazy view initialization from the guarantee, confirmed here by a
//!   rejected bash growing the serialized state once on its first attempt
//!   and not on a repeat.
//! - OnHit's abBashAttack argument (ck-wiki `OnHit_-_ObjectReference`,
//!   argument index 5 of 7) must be true for a bash and false otherwise.
//! - A successful bash spends `fBashStaminaCost` once (engine default 25.0,
//!   crates/game/src/game_settings.rs, documented CMB-20 estimate), and that
//!   spend survives a save/load round trip, matching the task's own "Tests
//!   mirror" save/load case.
//! - Bash damage always contributes its full post-reduction magnitude to the
//!   stagger check (stagger_fraction fixed at 1.0, docs/combat.md CMB-20),
//!   using the CMB-14 sourced fStaggerThreshold/fStaggerDuration defaults.
//! - BIDS/BAMT are 4-byte FormID links on a shield ARMO
//!   (uesp-modfileformat/ARMO.wikitext, lines 64 to 68), decoded the same
//!   strict-length way as every other combat record link.
//!
//! This file only adds tests. It does not modify the files under test.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    combat::{records::Armor, Error, MeleeHit},
    Game, Handle,
};
use soe_papyrus::{
    asm::{i, ident, FunctionBuilder, ScriptBuilder},
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

fn gmst(object: u32, name: &str, value: f32) -> Record {
    Record::new(Tag(*b"GMST"), object)
        .zsub(Tag(*b"EDID"), name)
        .sub(Tag(*b"DATA"), value.to_le_bytes())
}

/// A shield-slot ARMO record (BOD2 bit 9 set), with optional BIDS/BAMT
/// FormID links following the UESP layout.
fn shield(object: u32, rating_hundredths: u32, bids: Option<u32>, bamt: Option<u32>) -> Record {
    let mut body = (1_u32 << 9).to_le_bytes().to_vec();
    body.extend_from_slice(&2_u32.to_le_bytes());
    let mut record = Record::new(Tag(*b"ARMO"), object)
        .sub(
            Tag(*b"DATA"),
            [0_i32.to_le_bytes(), 5_f32.to_le_bytes()].concat(),
        )
        .sub(Tag(*b"DNAM"), rating_hundredths.to_le_bytes())
        .sub(Tag(*b"BOD2"), body);
    if let Some(bids) = bids {
        record = record.sub(Tag(*b"BIDS"), bids.to_le_bytes());
    }
    if let Some(bamt) = bamt {
        record = record.sub(Tag(*b"BAMT"), bamt.to_le_bytes());
    }
    record
}

/// An ARMO record that occupies an ordinary body slot, not the shield slot,
/// so it can stand in for a bash source that fails the BOD2 bit 9 check.
fn non_shield_armor(object: u32, rating_hundredths: u32) -> Record {
    let mut body = (1_u32 << 4).to_le_bytes().to_vec();
    body.extend_from_slice(&2_u32.to_le_bytes());
    Record::new(Tag(*b"ARMO"), object)
        .sub(
            Tag(*b"DATA"),
            [0_i32.to_le_bytes(), 5_f32.to_le_bytes()].concat(),
        )
        .sub(Tag(*b"DNAM"), rating_hundredths.to_le_bytes())
        .sub(Tag(*b"BOD2"), body)
}

/// Baseline damage/armor settings, mirrored from combat_hits.rs so an
/// ordinary weapon swing stays deterministic. Block and stagger GMSTs are
/// left to the engine defaults documented in game_settings.rs, the same
/// choice cmb_14_stagger.rs makes.
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
/// so difficulty stays fixed at 1.0 and no skill-scale GMST matters to the
/// bash branch, which never reads the skill-scale settings at all.
fn fixture(extra: Vec<Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    for actor in [0x900, 0x901] {
        plugin.push_top(
            Record::new(Tag(*b"ACHR"), actor).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()),
        );
    }
    plugin.push_top(weapon(0xA00, 10, 6));
    base_settings(&mut plugin);
    for record in extra {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Bash.esm", plugin).unwrap();
    let game = Game::new(order);
    for actor in [id(0x900), id(0x901)] {
        let mut world = game.world_mut();
        world.set_actor_value(actor, "health", 100.).unwrap();
        world.add_item(actor, id(0xA00), 1);
        world.actor(actor).unwrap().equipped.insert(id(0xA00));
    }
    game
}

fn equip(game: &Game, actor: u32, item: u32) {
    let mut world = game.world_mut();
    world.add_item(id(actor), id(item), 1);
    world.actor(id(actor)).unwrap().equipped.insert(id(item));
}

fn hit_bash(attacker: u32, target: u32, shield: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(shield),
        is_bash: true,
        is_power_attack: false,
    }
}

fn hit_weapon(attacker: u32, target: u32) -> MeleeHit {
    MeleeHit {
        attacker: id(attacker),
        target: id(target),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// Attaches a script to `target` that records each OnHit call's `bash`
/// argument into a script variable, so a test can assert on the exact
/// boolean the event carried rather than only that OnHit fired.
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
        .ins(Assign, vec![ident("lastBlocked"), ident("blocked")])
        .ins(IAdd, vec![ident("hits"), ident("hits"), i(1)]);
    let script = ScriptBuilder::new("BashObserver", "Actor")
        .variable("hits", "Int", i(0))
        .variable("lastBash", "Bool", soe_papyrus::asm::b(false))
        .variable("lastBlocked", "Bool", soe_papyrus::asm::b(false))
        .function(function.build());
    game.load_script(&script.bytes()).unwrap();
    let object = game.object_of(Handle::Form(id(target)));
    game.vm.attach(object, "BashObserver").unwrap();
    object
}

/// Sourced from the bash branch's own comment ("a bash that fails partway
/// through never spends stamina") and `melee_hit`'s doc comment ("rejected
/// hits do not damage or emit events", explicitly allowing lazy record
/// views to initialize regardless). A bash rejected for insufficient
/// stamina must leave the attacker's stamina and both actors' health
/// exactly as they were, and must not fire OnHit.
#[test]
fn cmb_20_bash_with_insufficient_stamina_is_rejected_without_state_or_event_changes() {
    let mut game = fixture(vec![shield(0xB00, 1000, Some(0x2000), Some(0x2001))]);
    equip(&game, 0x900, 0xB00);
    let object = bash_observer(&mut game, 0x901);
    // Engine default fBashStaminaCost is 25.0 (game_settings.rs); 5.0 is short.
    game.world_mut()
        .set_actor_value(id(0x900), "stamina", 5.)
        .unwrap();
    assert!(matches!(
        game.melee_hit(hit_bash(0x900, 0x901, 0xB00)),
        Err(Error::Unsupported("insufficient stamina for bash"))
    ));
    assert!(game.run_until_idle(32));
    close(game.world_mut().actor_value(id(0x900), "stamina"), 5.);
    close(game.world_mut().actor_value(id(0x900), "health"), 100.);
    close(game.world_mut().actor_value(id(0x901), "health"), 100.);
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "hits"),
        Some(Value::Int(0)),
        "a rejected bash must not fire OnHit"
    );
}

/// Sourced from TES5Edit's `wbBipedObjectFlags` (bit 9, "39 - Shield") and
/// the bash branch's explicit BOD2 check. An ARMO occupying an ordinary body
/// slot must reject a bash the same way a missing shield would, with the
/// same no-damage, no-event contract as the test above.
#[test]
fn cmb_20_bash_source_lacking_the_shield_slot_is_rejected_atomically() {
    let mut game = fixture(vec![non_shield_armor(0xB01, 1000)]);
    equip(&game, 0x900, 0xB01);
    let object = bash_observer(&mut game, 0x901);
    game.world_mut()
        .set_actor_value(id(0x900), "stamina", 100.)
        .unwrap();
    assert!(matches!(
        game.melee_hit(hit_bash(0x900, 0x901, 0xB01)),
        Err(Error::Unsupported("bash source is not a shield"))
    ));
    assert!(game.run_until_idle(32));
    close(game.world_mut().actor_value(id(0x900), "stamina"), 100.);
    close(game.world_mut().actor_value(id(0x900), "health"), 100.);
    close(game.world_mut().actor_value(id(0x901), "health"), 100.);
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "hits"),
        Some(Value::Int(0)),
        "a rejected bash must not fire OnHit"
    );
}

/// Sourced from ck-wiki `OnHit_-_ObjectReference.wikitext`'s 7-argument
/// signature (abBashAttack at index 5) and `HitResult::bashed`'s doc
/// comment. A bash must report `bash = true`, and an ordinary weapon swing
/// on the same target must report `bash = false`, not a stale leftover.
#[test]
fn cmb_20_onhit_bash_argument_matches_the_hit_kind() {
    let mut game = fixture(vec![shield(0xB00, 1000, None, None)]);
    equip(&game, 0x900, 0xB00);
    game.world_mut()
        .set_actor_value(id(0x900), "stamina", 100.)
        .unwrap();
    let object = bash_observer(&mut game, 0x901);

    game.melee_hit(hit_weapon(0x900, 0x901)).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "hits"),
        Some(Value::Int(1))
    );
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "lastBash"),
        Some(Value::Bool(false)),
        "an ordinary weapon swing must not report a bash"
    );

    game.melee_hit(hit_bash(0x900, 0x901, 0xB00)).unwrap();
    assert!(game.run_until_idle(32));
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "hits"),
        Some(Value::Int(2))
    );
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "lastBash"),
        Some(Value::Bool(true)),
        "a shield bash must report bash = true"
    );
}

/// Sourced from the done check ("consumes stamina once") and the task's own
/// save/load test case. A successful bash spends exactly fBashStaminaCost
/// (engine default 25.0), and that spend is the value a save/load round
/// trip restores, not the pre-bash value.
#[test]
fn cmb_20_successful_bash_consumes_stamina_once_and_survives_save_load() {
    let records = vec![shield(0xB00, 1000, None, None)];
    let mut game = fixture(records.clone());
    equip(&game, 0x900, 0xB00);
    game.world_mut()
        .set_actor_value(id(0x900), "stamina", 100.)
        .unwrap();
    let result = game.melee_hit(hit_bash(0x900, 0x901, 0xB00)).unwrap();
    assert!(result.bashed);
    close(game.world_mut().actor_value(id(0x900), "stamina"), 75.);

    let saved = game.save_state().unwrap();
    let mut loaded = fixture(records);
    equip(&loaded, 0x900, 0xB00);
    loaded.load_state(&saved).unwrap();
    close(loaded.world_mut().actor_value(id(0x900), "stamina"), 75.);
}

/// Sourced from docs/combat.md CMB-20 (stagger_fraction fixed at 1.0 for a
/// bash) and CMB-14's fStaggerThreshold/fStaggerDuration defaults
/// (game_settings.rs: 3.5 and 1.2 seconds). A bash whose shield rating alone
/// clears the threshold must stagger the target for exactly that window,
/// the done check's "configured stagger response".
#[test]
fn cmb_20_bash_crosses_stagger_threshold_using_the_full_stagger_fraction() {
    // Unarmored target: armor_reduction = 0, not blocking: block_reduction =
    // 0, NPC vs NPC: difficulty = 1.0. health_damage = shield rating = 10.0,
    // above the 3.5 default threshold.
    let mut game = fixture(vec![shield(0xB00, 1000, None, None)]);
    equip(&game, 0x900, 0xB00);
    game.world_mut()
        .set_actor_value(id(0x900), "stamina", 100.)
        .unwrap();
    let result = game.melee_hit(hit_bash(0x900, 0x901, 0xB00)).unwrap();
    close(result.health_damage, 10.0);
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

/// Sourced from uesp-modfileformat/ARMO.wikitext (BIDS/BAMT are 4-byte
/// FormID links) and the established truncation-rejection pattern
/// (`weapon_record_truncations_are_rejected` in combat_hits.rs). A shield
/// whose BIDS or BAMT subrecord is short must be rejected at decode time,
/// and a bash that equips such a shield must be rejected atomically too.
#[test]
fn cmb_20_malformed_bash_impact_or_material_subrecord_is_rejected() {
    for tag in [Tag(*b"BIDS"), Tag(*b"BAMT")] {
        let record = shield(0xB00, 1000, Some(0x2000), Some(0x2001));
        let bytes = record.find(tag).unwrap().to_vec();
        for length in 0..bytes.len() {
            let mut bad = record.clone();
            for field in &mut bad.subrecords {
                if field.tag == tag {
                    field.data = bytes[..length].to_vec();
                }
            }
            assert!(
                Armor::from_record(&bad).is_err(),
                "a {length}-byte {tag:?} must be rejected, not silently truncated"
            );
        }
    }

    let mut bad_shield = shield(0xB00, 1000, Some(0x2000), None);
    for field in &mut bad_shield.subrecords {
        if field.tag == Tag(*b"BIDS") {
            field.data.truncate(2);
        }
    }
    let mut game = fixture(vec![bad_shield]);
    equip(&game, 0x900, 0xB00);
    let object = bash_observer(&mut game, 0x901);
    game.world_mut()
        .set_actor_value(id(0x900), "stamina", 100.)
        .unwrap();
    assert!(game.melee_hit(hit_bash(0x900, 0x901, 0xB00)).is_err());
    assert!(game.run_until_idle(32));
    close(game.world_mut().actor_value(id(0x900), "stamina"), 100.);
    close(game.world_mut().actor_value(id(0x901), "health"), 100.);
    assert_eq!(
        game.vm.get_var(object, "BashObserver", "hits"),
        Some(Value::Int(0)),
        "a bash rejected for a malformed BIDS link must not fire OnHit"
    );
}
