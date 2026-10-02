// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::GlobalFormId;
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

#[test]
fn mag_40_missing_efsh_record_does_not_request_a_fallback_visual() {
    let game = Game::new(soe_esm::LoadOrder::new());
    let world = game.world();
    let mut active = BTreeSet::new();

    let requests = effect_shader_requests(&world, &[detection(0x101, Some(0x400))], &mut active);

    assert!(requests.is_empty());
    assert!(active.is_empty());
}

#[test]
fn mag_40_overlapping_effects_for_one_shader_start_only_one_target_visual() {
    let game = Game::new(soe_esm::LoadOrder::new());
    let world = game.world();
    let mut active = BTreeSet::new();
    let detections = [detection(0x101, Some(0x400)), detection(0x101, Some(0x400))];

    let requests = effect_shader_requests(&world, &detections, &mut active);

    assert_eq!(requests.len(), 1);
    assert_eq!((requests[0].shader, requests[0].target), (id(0x400), id(0x101)));
    assert!(requests[0].playing);
}

#[test]
fn mag_40_missing_hit_shader_stops_every_shader_for_the_departed_actor() {
    let game = Game::new(soe_esm::LoadOrder::new());
    let world = game.world();
    let mut active = BTreeSet::from([(id(0x400), id(0x101)), (id(0x401), id(0x101))]);

    let requests = effect_shader_requests(&world, &[detection(0x101, None)], &mut active);

    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|request| !request.playing));
    assert!(active.is_empty());
}
