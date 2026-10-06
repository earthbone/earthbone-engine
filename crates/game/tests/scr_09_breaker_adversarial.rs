// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::scr_09_actor_queries::{actor_bool, fixture, id};
use soe_papyrus::Value;

#[test]
fn scr_09_mount_and_trespass_queries_are_per_actor_and_follow_saved_state() {
    let mut game = fixture();
    assert!(!actor_bool(&mut game, id(0x900), "IsOnMount", &[]));
    assert!(!actor_bool(&mut game, id(0x900), "IsTrespassing", &[]));

    {
        let mut world = game.world_mut();
        let actor = world.actor(id(0x900)).unwrap();
        actor.riding_mount = true;
        actor.trespassing = true;
    }
    assert!(actor_bool(&mut game, id(0x900), "IsOnMount", &[]));
    assert!(actor_bool(&mut game, id(0x900), "IsTrespassing", &[]));
    assert!(!actor_bool(&mut game, id(0x901), "IsOnMount", &[]));
    assert!(!actor_bool(&mut game, id(0x901), "IsTrespassing", &[]));

    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    assert!(actor_bool(&mut game, id(0x900), "IsOnMount", &[]));
    assert!(actor_bool(&mut game, id(0x900), "IsTrespassing", &[]));
}

#[test]
fn scr_09_no_argument_actor_queries_return_false_for_non_actor_subjects() {
    let mut game = fixture();
    for method in ["IsChild", "IsCommandedActor", "IsOnMount", "IsTrespassing"] {
        assert!(!actor_bool(&mut game, id(0x910), method, &[]), "{method}");
    }
    assert!(game.vm.log.contains("not an actor"));
}

#[test]
fn scr_09_detected_by_rejects_non_actor_argument_without_changing_awareness() {
    let mut game = fixture();
    let observer = game.object_of_form(id(0x901));
    assert!(!actor_bool(
        &mut game,
        id(0x900),
        "IsDetectedBy",
        &[Value::Object(observer)],
    ));
    let before = game.world().save_state();

    assert!(!actor_bool(
        &mut game,
        id(0x900),
        "IsDetectedBy",
        &[Value::Int(0x901)],
    ));
    assert_eq!(game.world().save_state(), before);
}

#[test]
fn scr_09_commanded_summon_query_survives_world_save_load() {
    let mut game = fixture();
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(0x940),
            id(0x900),
            Some(id(0x900)),
            Some(id(0x900)),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    let summoned = game
        .world()
        .active_effects()
        .find_map(|effect| effect.summoned)
        .unwrap();
    let shared = game.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(0x941),
            id(0x900),
            Some(id(0x900)),
            Some(summoned),
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .unwrap();
    assert!(actor_bool(&mut game, summoned, "IsCommandedActor", &[]));

    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    assert!(actor_bool(&mut game, summoned, "IsCommandedActor", &[]));
}

#[test]
fn scr_09_no_argument_predicates_reject_extra_arguments() {
    let mut game = fixture();
    for method in ["IsChild", "IsCommandedActor", "IsOnMount", "IsTrespassing"] {
        let actor = if method == "IsChild" {
            id(0x900)
        } else {
            id(0x901)
        };
        assert!(!actor_bool(&mut game, actor, method, &[Value::Bool(true)],));
        assert!(game
            .vm
            .log
            .contains(&format!("Actor.{method} expects no arguments")));
    }
}
