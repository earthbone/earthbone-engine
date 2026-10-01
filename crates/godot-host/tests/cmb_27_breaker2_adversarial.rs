// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! CMB-27 persistence and authored reaction edge case.

use std::path::Path;

use soe_bsa::Vfs;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::Hand, Game};
use soe_godot_host::{actor::{ActorAssets, MASTER_BEHAVIOR}, combat::MeleeContact};
use soe_hkx::interp::Interpreter;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for (slot, parents) in [
        (0xb00_u32, None),
        (0xb01, None),
        (0xb02, Some((0xb00_u32, 0xb01_u32))),
    ] {
        let mut record = Record::new(Tag(*b"EQUP"), slot).sub(Tag(*b"DATA"), 0_u32.to_le_bytes());
        if let Some((left, right)) = parents {
            let mut bytes = left.to_le_bytes().to_vec();
            bytes.extend_from_slice(&right.to_le_bytes());
            record = record.sub(Tag(*b"PNAM"), bytes);
        }
        plugin.push_top(record);
    }
    let mut defaults = b"LHEQ".to_vec();
    defaults.extend_from_slice(&0xb00_u32.to_le_bytes());
    defaults.extend_from_slice(b"RHEQ");
    defaults.extend_from_slice(&0xb01_u32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"DOBJ"), 0x31).sub(Tag(*b"DNAM"), defaults));

    let mut data = vec![0; 10];
    data[8..10].copy_from_slice(&10_u16.to_le_bytes());
    let mut dnam = vec![0; 100];
    dnam[0] = 1;
    dnam[4..8].copy_from_slice(&1_f32.to_le_bytes());
    dnam[8..12].copy_from_slice(&1_f32.to_le_bytes());
    dnam[76..80].copy_from_slice(&(-1_i32).to_le_bytes());
    dnam[88..92].copy_from_slice(&(-1_i32).to_le_bytes());
    dnam[96..100].copy_from_slice(&1_f32.to_le_bytes());
    plugin.push_top(Record::new(Tag(*b"WEAP"), 0xa07)
        .sub(Tag(*b"DATA"), data)
        .sub(Tag(*b"DNAM"), dnam)
        .sub(Tag(*b"ETYP"), 0xb02_u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Reaction.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    for actor in [player, id(0x900)] {
        game.world_mut().set_actor_value(actor, "health", 100.0).unwrap();
    }
    game.world_mut().add_item(player, id(0xa07), 1);
    game.world_mut().equip_hand_weapon(player, Hand::Right, id(0xa07)).unwrap();
    game
}

#[test]
fn staggered_hit_is_applied_once_and_deadline_survives_save_load() {
    let attacker = id(0x14);
    let target = id(0x900);
    let mut game = fixture();
    let mut contact = MeleeContact::default();
    contact.begin(&game, attacker, Hand::Right).unwrap();
    let hit = contact.event(&mut game, "HitFrame", |_| Ok(Some(target))).unwrap().unwrap();
    assert_eq!(hit.health_damage, 10.0);
    assert_eq!(game.world_mut().actor_value(target, "health"), 90.0);
    let deadline = game.world_mut().actor(target).unwrap().stagger_end_time;
    assert!(deadline.is_some());
    let magnitude = game.world_mut().actor(target).unwrap().stagger_magnitude;
    assert!(magnitude > 1.0, "the hit's computed magnitude must remain available to the renderer");
    assert!(contact.event(&mut game, "HitFrame", |_| panic!("duplicate frame queried")).unwrap().is_none());

    let saved = game.save_state().unwrap();
    game.load_state(&saved).unwrap();
    assert_eq!(game.world_mut().actor_value(target, "health"), 90.0);
    assert_eq!(game.world_mut().actor(target).unwrap().stagger_end_time, deadline);
    assert_eq!(game.world_mut().actor(target).unwrap().stagger_magnitude, magnitude);
}

#[test]
fn moving_locomotion_returns_after_authored_stagger_stop() {
    let Ok(data_dir) = std::env::var("SKYRIM_DATA") else { return };
    let mut vfs = Vfs::new();
    for archive in ["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa", "Skyrim - Animations.bsa"] {
        vfs.add_archive_path(&Path::new(&data_dir).join(archive)).unwrap();
    }
    let assets = ActorAssets::load_with_behavior(
        &vfs,
        "meshes/actors/character/character assets/malebody_1.nif",
        MASTER_BEHAVIOR,
    ).unwrap();
    let mut vm = Interpreter::new(&assets.graph, &assets.clips, assets.skeleton.reference_pose.clone());
    vm.variables.set_by_name("iRightHandType", 1.0);
    vm.variables.set_by_name("Speed", 1.0);
    assert!(vm.send_event_named("moveStart"));
    for _ in 0..30 { vm.update(1.0 / 60.0); }
    let locomotion = vm.active_states();
    assert!(!locomotion.is_empty());
    vm.variables.set_by_name("staggerMagnitude", 1.0);
    assert!(vm.send_event_named("staggerStart"));
    let played_reaction = (0..90).any(|_| {
        vm.update(1.0 / 60.0);
        vm.active_clips().iter().any(|(node, _)| assets.graph.node(*node).name.to_ascii_lowercase().contains("stagger"))
    });
    assert!(played_reaction);
    assert!(vm.send_event_named("staggerStop"));
    for _ in 0..180 { vm.update(1.0 / 60.0); }
    assert_eq!(vm.active_states(), locomotion);
}
