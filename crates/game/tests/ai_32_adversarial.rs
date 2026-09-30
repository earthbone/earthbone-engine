// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for AI-32 Flee target identity and route selection.

#[path = "packages.rs"]
mod package_fixtures;

use soe_game::ai::Procedure;

#[test]
fn ai_32_flee_save_load_keeps_the_live_threat_and_away_route() {
    let mut game = package_fixtures::game_at_hour(12.0);
    let actor = game.form("FleeRef").unwrap();
    let threat = game.form("ThreatRef").unwrap();
    {
        let mut world = game.world_mut();
        let state = world.actor(actor).unwrap();
        state.in_combat = true;
        state.combat_target = Some(threat);
    }
    package_fixtures::tick(&mut game, 0.1);
    let save = game.save_state().unwrap();
    let mut restored = package_fixtures::game_at_hour(12.0);
    restored.load_state(&save).unwrap();

    let world = restored.world();
    let state = world.actor_ref(actor).unwrap();
    assert_eq!(state.ai.procedure, Procedure::Flee);
    assert_eq!(state.combat_target, Some(threat));
    let start = world.reference(actor).unwrap().position;
    let route = state.ai.destination.expect("active Flee retains its route");
    let threat_position = world.reference(threat).unwrap().position;
    let distance = |a: [f32; 3], b: [f32; 3]| {
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
    };
    assert!(
        distance(route, threat_position) > distance(start, threat_position),
        "restored Flee route {route:?} does not increase separation from {threat_position:?}"
    );
}

#[test]
fn ai_32_flee_clears_a_dead_threat_without_reacquiring_it() {
    let mut game = package_fixtures::game_at_hour(12.0);
    let actor = game.form("FleeRef").unwrap();
    let threat = game.form("ThreatRef").unwrap();
    {
        let mut world = game.world_mut();
        let state = world.actor(actor).unwrap();
        state.in_combat = true;
        state.combat_target = Some(threat);
    }
    package_fixtures::tick(&mut game, 0.1);
    assert_eq!(
        game.world().actor_ref(actor).unwrap().ai.procedure,
        Procedure::Flee
    );

    game.world_mut().actor(threat).unwrap().dead = true;
    package_fixtures::tick(&mut game, 0.1);

    let state = game.world().actor_ref(actor).unwrap();
    assert!(state.combat_target.is_none());
    assert!(!state.in_combat);
    assert!(state.ai.destination.is_none());
    assert!(state.ai.tree.as_ref().is_some_and(|tree| tree.finished));
    assert_eq!(state.ai.procedure, Procedure::Flee);
}
