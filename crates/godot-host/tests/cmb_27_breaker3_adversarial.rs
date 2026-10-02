// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Adversarial checks for shared stagger events delivered to actor graphs.

use soe_bsa::Vfs;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{combat::Hand, Game};
use soe_godot_host::{
    actor::{ActorAssets, MASTER_BEHAVIOR},
    combat::MeleeContact,
    godot_actor::{send_stagger_start, StaggerGraphEvent, StaggerTracker},
};
use soe_hkx::interp::Interpreter;
use std::path::Path;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game_with_actor() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x800));
    plugin.push_top(Record::new(Tag(*b"ACHR"), 0x900).sub(Tag(*b"NAME"), 0x800_u32.to_le_bytes()));
    for slot in [0xb00_u32, 0xb01] {
        plugin.push_top(Record::new(Tag(*b"EQUP"), slot).sub(Tag(*b"DATA"), 0_u32.to_le_bytes()));
    }
    let mut parents = 0xb00_u32.to_le_bytes().to_vec();
    parents.extend_from_slice(&0xb01_u32.to_le_bytes());
    plugin.push_top(
        Record::new(Tag(*b"EQUP"), 0xb02)
            .sub(Tag(*b"DATA"), 0_u32.to_le_bytes())
            .sub(Tag(*b"PNAM"), parents),
    );
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
    plugin.push_top(
        Record::new(Tag(*b"WEAP"), 0xa07)
            .sub(Tag(*b"DATA"), data)
            .sub(Tag(*b"DNAM"), dnam)
            .sub(Tag(*b"ETYP"), 0xb02_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Stagger.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert_eq!(game.player(), id(0x14));
    for actor in [id(0x14), id(0x900)] {
        game.world_mut()
            .set_actor_value(actor, "health", 100.0)
            .unwrap();
    }
    game.world_mut().add_item(id(0x14), id(0xa07), 1);
    game.world_mut()
        .equip_hand_weapon(id(0x14), Hand::Right, id(0xa07))
        .unwrap();
    game
}

#[test]
fn cmb_27_shared_hit_plays_authored_reaction_then_returns_without_redamage() {
    let attacker = id(0x14);
    let target = id(0x900);
    let mut game = game_with_actor();
    let mut contact = MeleeContact::default();
    contact.begin(&game, attacker, Hand::Right).unwrap();
    let hit = contact
        .event(&mut game, "HitFrame", |_| Ok(Some(target)))
        .unwrap()
        .expect("the authored hit frame resolves one hit");
    assert_eq!(hit.health_damage, 10.0);
    assert!(hit.staggered);
    assert_eq!(game.world_mut().actor_value(target, "health"), 90.0);
    assert!(contact
        .event(&mut game, "HitFrame", |_| {
            Err(soe_game::combat::Error::Unsupported(
                "duplicate frame queried",
            ))
        })
        .unwrap()
        .is_none());

    let Ok(data_dir) = std::env::var("SKYRIM_DATA") else {
        eprintln!("skipping authored graph check: set SKYRIM_DATA");
        return;
    };
    let mut vfs = Vfs::new();
    for archive in [
        "Skyrim - Meshes0.bsa",
        "Skyrim - Meshes1.bsa",
        "Skyrim - Animations.bsa",
    ] {
        vfs.add_archive_path(&Path::new(&data_dir).join(archive))
            .expect("vanilla actor archive");
    }
    let assets = ActorAssets::load_with_behavior(
        &vfs,
        "meshes/actors/character/character assets/malebody_1.nif",
        MASTER_BEHAVIOR,
    )
    .expect("vanilla actor assets");
    let mut vm = Interpreter::new(
        &assets.graph,
        &assets.clips,
        assets.skeleton.reference_pose.clone(),
    );
    vm.variables.set_by_name("iRightHandType", 1.0);
    vm.variables.set_by_name("Speed", 1.0);
    assert!(vm.send_event_named("moveStart"));
    for _ in 0..30 {
        vm.update(1.0 / 60.0);
    }
    let locomotion = vm.active_states();
    assert!(!locomotion.is_empty());

    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();
    let (event, magnitude) = tracker
        .poll(&mut shared.borrow_mut(), target)
        .expect("shared hit state starts a reaction");
    assert_eq!(event, StaggerGraphEvent::Start);
    assert!(send_stagger_start(&mut vm, magnitude));
    let played = (0..90).any(|_| {
        vm.update(1.0 / 60.0);
        vm.active_clips().iter().any(|(node, _)| {
            assets
                .graph
                .node(*node)
                .name
                .to_ascii_lowercase()
                .contains("stagger")
        })
    });
    assert!(played, "an authored stagger clip should play");
    assert_eq!(game.world_mut().actor_value(target, "health"), 90.0);

    game.tick(10.0);
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Stop, magnitude))
    );
    assert!(vm.send_event_named("staggerStop"));
    for _ in 0..180 {
        vm.update(1.0 / 60.0);
    }
    assert_eq!(vm.active_states(), locomotion);
    assert_eq!(game.world_mut().actor_value(target, "health"), 90.0);
}

#[test]
fn cmb_27_tracker_emits_each_start_and_expiry_transition_once() {
    let target = id(0x900);
    let game = game_with_actor();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();
    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 2.0;
        let actor = world.actor(target).unwrap();
        actor.stagger_end_time = Some(3.0);
        actor.stagger_magnitude = 0.75;
    }

    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Start, 0.75))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);

    shared.borrow_mut().clock.real_seconds = 3.0;
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Stop, 0.75))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);
}

#[test]
fn cmb_27_extended_deadline_restarts_once_with_latest_magnitude() {
    let target = id(0x900);
    let game = game_with_actor();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();
    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 2.0;
        let actor = world.actor(target).unwrap();
        actor.stagger_end_time = Some(3.0);
        actor.stagger_magnitude = 0.75;
    }
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Start, 0.75))
    );

    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 2.25;
        let actor = world.actor(target).unwrap();
        actor.stagger_end_time = Some(4.0);
        actor.stagger_magnitude = 1.5;
    }
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Start, 1.5))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);

    shared.borrow_mut().clock.real_seconds = 4.0;
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Stop, 1.5))
    );
}

#[test]
fn cmb_27_expired_deadline_never_starts_a_late_reaction() {
    let target = id(0x900);
    let game = game_with_actor();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();
    {
        let mut world = shared.borrow_mut();
        world.clock.real_seconds = 5.0;
        let actor = world.actor(target).unwrap();
        actor.stagger_end_time = Some(4.0);
        actor.stagger_magnitude = 1.5;
    }
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);
}
