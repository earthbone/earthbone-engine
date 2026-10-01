// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! CMB-13 done check: a protected or essential target enters bleedout instead
//! of dying, then recovers. Mirrors the synthetic-plugin fixtures in
//! combat_hits.rs and combat_bleedout.rs, keeping the entry/recovery cases
//! independent of the threshold-only tests already in combat_bleedout.rs.

use crate::common::installed;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::MeleeHit, npc::flags, Game};
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

/// One protected/essential target actor, plus a baseless NPC attacker (0x901)
/// and the player, sharing the same weapon fixture the combat hit tests use.
/// The NPC attacker, not the player, exercises the entry/recovery path here:
/// since CMB-34 gives the player an outright-kill exception for a merely
/// protected, non-essential target, only a non-player attacker keeps
/// clamping every flagged target into bleedout the way this fixture checks.
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
    order.add("Bleedout.esm", plugin).unwrap();
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

fn hit() -> MeleeHit {
    MeleeHit {
        attacker: id(0x901),
        target: id(0x900),
        weapon: id(0xA00),
        is_bash: false,
        is_power_attack: false,
    }
}

fn close(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
}

/// Core CMB-13 done check, run for both protected and essential targets: a
/// lethal hit clamps health at the bleedout threshold instead of killing the
/// target, and a later health restore that crosses the threshold again clears
/// `unconscious`.
#[test]
fn cmb_13_protected_essential_target_enters_bleedout_and_recovers() {
    for target_flags in [flags::PROTECTED, flags::ESSENTIAL] {
        let mut game = fixture(target_flags);
        // No CLAS/override record: threshold is fbleedoutdefault (0.2) times
        // permanent health, per game_settings.rs and combat_bleedout.rs. This
        // weapon fixture deals 15 damage (verified against a debug probe),
        // so health 15 (threshold 3) is low enough for one hit to be lethal.
        game.world_mut()
            .set_actor_value(id(0x900), "health", 15.)
            .unwrap();
        assert!(!game.world_mut().actor(id(0x900)).unwrap().unconscious);

        let result = game.melee_hit(hit()).unwrap();
        assert!(!result.killed, "protected/essential target must not die");
        {
            let mut world = game.world_mut();
            let target = world.actor(id(0x900)).unwrap();
            assert!(target.unconscious, "target must enter bleedout");
            assert!(!target.dead);
        }
        close(game.world_mut().actor_value(id(0x900), "health"), 3.);

        // A positive health change that crosses the cached threshold (3.)
        // recovers the target, matching the health-driven path in
        // World::resolve_bleedout_recovery (docs/combat.md, CMB-13).
        game.world_mut()
            .restore_actor_value(id(0x900), "health", 10.)
            .unwrap();
        assert!(
            !game.world_mut().actor(id(0x900)).unwrap().unconscious,
            "target must recover once health clears the threshold"
        );
        close(game.world_mut().actor_value(id(0x900), "health"), 13.);
    }
}

/// `SetNoBleedoutRecovery` (flag bit 5) blocks the health-triggered recovery
/// path, per ck-wiki's SetNoBleedoutRecovery/GetNoBleedoutRecovery pages.
#[test]
fn cmb_13_no_bleedout_recovery_blocks_health_triggered_recovery() {
    let mut game = fixture(flags::ESSENTIAL);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();
    game.melee_hit(hit()).unwrap();
    assert!(game.world_mut().actor(id(0x900)).unwrap().unconscious);
    game.world_mut()
        .actor(id(0x900))
        .unwrap()
        .no_bleedout_recovery = true;

    game.world_mut()
        .restore_actor_value(id(0x900), "health", 100.)
        .unwrap();
    assert!(
        game.world_mut().actor(id(0x900)).unwrap().unconscious,
        "NoBleedoutRecovery must hold the target down despite full health"
    );
}

fn bleedout_observer(game: &mut Game) -> soe_papyrus::ObjectId {
    let function = FunctionBuilder::new("OnEnterBleedout", "None")
        .local("next", "Int")
        .ins(IAdd, vec![ident("sequence"), ident("sequence"), i(1)])
        .build();
    let script = ScriptBuilder::new("BleedoutObserver", "Actor")
        .variable("sequence", "Int", i(0))
        .function(function);
    game.load_script(&script.bytes()).unwrap();
    let object = game.object_of(soe_game::Handle::Form(id(0x900)));
    game.vm.attach(object, "BleedoutObserver").unwrap();
    object
}

/// Entering bleedout queues `OnEnterBleedout` for the target, per
/// docs/combat.md's CMB-13 section and the ck-wiki event page.
#[test]
fn cmb_13_enter_bleedout_queues_on_enter_bleedout_event() {
    let mut game = fixture(flags::PROTECTED);
    let object = bleedout_observer(&mut game);
    game.world_mut()
        .set_actor_value(id(0x900), "health", 15.)
        .unwrap();
    game.melee_hit(hit()).unwrap();
    assert_eq!(
        game.vm.get_var(object, "BleedoutObserver", "sequence"),
        Some(Value::Int(1)),
        "OnEnterBleedout must fire exactly once on entry"
    );
    // A second lethal hit while already unconscious is rejected by
    // validate_combat_actor, so the event does not fire again.
    assert!(game.melee_hit(hit()).is_err());
    assert_eq!(
        game.vm.get_var(object, "BleedoutObserver", "sequence"),
        Some(Value::Int(1))
    );
}

/// SKYRIM_DATA gated: a real essential or protected NPC_ winner from the
/// installed load order reaches the same bleedout threshold math the
/// synthetic fixtures above exercise, tying CMB-13 to actual game records
/// instead of only the hand-built ACBS bytes.
#[test]
fn cmb_13_installed_essential_npc_reaches_a_finite_bleedout_threshold() {
    let Some(_data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA not set, skipping installed essential NPC check");
        return;
    };
    let mut order = installed::owned_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let mut found = None;
    for (form, record) in order
        .winners_of(Tag(*b"NPC_"))
        .filter(|(_, record)| !record.is_deleted())
    {
        let Ok(npc) = soe_game::npc::Npc::from_record(record, false) else {
            continue;
        };
        if npc.config.flags & (flags::ESSENTIAL | flags::PROTECTED) != 0 {
            let name = npc.editor_id.clone().unwrap_or_default();
            if !name.is_empty() {
                found = Some((form, name));
                break;
            }
        }
    }
    let (base, name) = found.expect("installed load order carries a named essential/protected NPC");
    eprintln!("cmb_13: using installed essential/protected NPC {base} {name}");

    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Skyrim.esm".into()],
        ..FileHeader::default()
    });
    let reference = GlobalFormId {
        file: 1,
        object: 0x900,
    };
    patch.push_top(
        Record::new(Tag(*b"ACHR"), reference.object).sub(Tag(*b"NAME"), base.object.to_le_bytes()),
    );
    order.add("EssentialSurvey.esp", patch).unwrap();
    let file = order.files().len() - 1;
    let reference = GlobalFormId {
        file: file as u16,
        object: 0x900,
    };
    let game = Game::new(order);
    let threshold = game.world_mut().actor_bleedout_threshold(reference);
    assert!(
        threshold.as_ref().is_ok_and(|value| value.is_finite() && *value >= 0.),
        "installed essential/protected NPC {name} must resolve a finite bleedout threshold: {threshold:?}"
    );
}
