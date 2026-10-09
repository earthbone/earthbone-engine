#![cfg(feature = "godot")]
#![allow(clippy::expect_used, clippy::unwrap_used)]

//! Adversarial checks for authored stagger handoff and reactivation.

use soe_bsa::Vfs;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::{
    actor::{ActorAssets, MASTER_BEHAVIOR},
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
    let mut order = LoadOrder::new();
    order.add("Stagger.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn cmb_27_authored_graph_receives_world_magnitude_and_returns_without_health_change() {
    let Ok(data_dir) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping installed authored graph check");
        return;
    };
    let target = id(0x900);
    let mut game = game_with_actor();
    game.world_mut().set_actor_value(target, "health", 100.0).unwrap();
    let shared = game.shared_world();
    {
        let mut world = shared.borrow_mut();
        world.actor(target).unwrap().stagger_end_time = Some(1.2);
        world.actor(target).unwrap().stagger_magnitude = 1.75;
    }

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
    assert!(vm.variables.set_by_name("iRightHandType", 1.0));
    assert!(vm.variables.set_by_name("Speed", 1.0));
    assert!(vm.send_event_named("moveStart"));
    for _ in 0..30 {
        vm.update(1.0 / 60.0);
    }
    let locomotion = vm.active_states();
    assert!(!locomotion.is_empty());

    let magnitude_index = vm.variables.index("staggerMagnitude").unwrap();
    let mut tracker = StaggerTracker::default();
    let (event, magnitude) = tracker
        .poll(&mut shared.borrow_mut(), target)
        .expect("active shared World deadline starts the graph reaction");
    assert_eq!(event, StaggerGraphEvent::Start);
    assert_eq!(magnitude, 1.75);
    assert!(send_stagger_start(&mut vm, magnitude));
    assert_eq!(vm.variables.real(magnitude_index), Some(1.75));
    let reaction_played = (0..90).any(|_| {
        vm.update(1.0 / 60.0);
        vm.active_clips().iter().any(|(node, _)| {
            assets.graph.node(*node).name.to_ascii_lowercase().contains("stagger")
        })
    });
    assert!(reaction_played, "authored stagger clip should play");
    assert_eq!(game.world_mut().actor_value(target, "health"), 100.0);

    shared.borrow_mut().clock.real_seconds = 1.2;
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Stop, 1.75))
    );
    assert!(vm.send_event_named("staggerStop"));
    for _ in 0..180 {
        vm.update(1.0 / 60.0);
    }
    assert_eq!(vm.active_states(), locomotion);
    assert_eq!(game.world_mut().actor_value(target, "health"), 100.0);
}

#[test]
fn cmb_27_completed_reaction_can_start_again_without_duplicate_transitions() {
    let target = id(0x900);
    let game = game_with_actor();
    let shared = game.shared_world();
    let mut tracker = StaggerTracker::default();

    {
        let mut world = shared.borrow_mut();
        world.actor(target).unwrap().stagger_end_time = Some(1.0);
        world.actor(target).unwrap().stagger_magnitude = 0.5;
    }
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Start, 0.5))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);

    shared.borrow_mut().clock.real_seconds = 1.0;
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Stop, 0.5))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);

    {
        let mut world = shared.borrow_mut();
        world.actor(target).unwrap().stagger_end_time = Some(2.0);
        world.actor(target).unwrap().stagger_magnitude = 1.25;
    }
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Start, 1.25))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);
    shared.borrow_mut().clock.real_seconds = 2.0;
    assert_eq!(
        tracker.poll(&mut shared.borrow_mut(), target),
        Some((StaggerGraphEvent::Stop, 1.25))
    );
    assert_eq!(tracker.poll(&mut shared.borrow_mut(), target), None);
}
