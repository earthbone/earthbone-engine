// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::world::magic::EffectId;
use soe_game::Game;
use soe_godot_host::detect_life::{effect_shader_requests, DetectionPresentation};
use soe_godot_host::units::GodotPos;
use std::collections::BTreeSet;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn detection(actor: u32, shader: Option<u32>) -> DetectionPresentation {
    DetectionPresentation {
        effect: EffectId(1),
        actor: id(actor),
        position: GodotPos::default(),
        hit_shader: shader.map(id),
    }
}

fn game_with_shaders() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 0x400));
    plugin.push_top(Record::new(Tag::new(b"EFSH"), 0x401));
    let mut order = LoadOrder::new();
    order.add("EffectShaders.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn mag_40_repeated_detection_does_not_restart_the_same_shader() {
    let game = game_with_shaders();
    let world = game.world();
    let targets = [detection(0x101, Some(0x400))];
    let mut active = BTreeSet::new();

    let first = effect_shader_requests(&world, &targets, &mut active);
    assert_eq!(first.len(), 1);
    assert!(first[0].playing);

    let repeated = effect_shader_requests(&world, &targets, &mut active);
    assert!(repeated.is_empty());
    assert_eq!(active, BTreeSet::from([(id(0x400), id(0x101))]));
}

#[test]
fn mag_40_shader_replacement_stops_old_and_starts_new_in_stable_order() {
    let game = game_with_shaders();
    let world = game.world();
    let mut active = BTreeSet::from([(id(0x401), id(0x101))]);

    let requests = effect_shader_requests(&world, &[detection(0x101, Some(0x400))], &mut active);

    assert_eq!(requests.len(), 2);
    assert_eq!(
        (requests[0].shader, requests[0].target),
        (id(0x400), id(0x101))
    );
    assert!(requests[0].playing);
    assert_eq!(
        (requests[1].shader, requests[1].target),
        (id(0x401), id(0x101))
    );
    assert!(!requests[1].playing);
    assert_eq!(active, BTreeSet::from([(id(0x400), id(0x101))]));
}

#[test]
fn mag_40_nullable_hit_shader_stops_a_previous_shader_without_replacement() {
    let game = Game::new(LoadOrder::new());
    let world = game.world();
    let mut active = BTreeSet::from([(id(0x400), id(0x101))]);

    let requests = effect_shader_requests(&world, &[detection(0x101, None)], &mut active);

    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].shader, id(0x400));
    assert_eq!(requests[0].target, id(0x101));
    assert!(!requests[0].playing);
    assert!(active.is_empty());
}

#[test]
fn mag_40_duplicate_targets_collapse_and_emit_in_actor_order() {
    let game = game_with_shaders();
    let world = game.world();
    let targets = [
        detection(0x102, Some(0x400)),
        detection(0x101, Some(0x400)),
        detection(0x101, Some(0x400)),
    ];
    let mut active = BTreeSet::new();

    let requests = effect_shader_requests(&world, &targets, &mut active);

    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].target, id(0x101));
    assert_eq!(requests[1].target, id(0x102));
}
