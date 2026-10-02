// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{package, Game};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const ACTOR: u32 = 0x801;
const PACKAGE: u32 = 0x802;
const TOPIC: u32 = 0x804;

fn id(form: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: form,
    }
}

fn order() -> LoadOrder {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), 0x800)
            .zsub(T(b"EDID"), "ActorBase")
            .sub(T(b"PKID"), PACKAGE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), ACTOR)
            .sub(T(b"NAME"), 0x800u32.to_le_bytes())
            .sub(T(b"DATA"), [0; 24]),
    );
    let mut topic_data = 0u32.to_le_bytes().to_vec();
    topic_data.extend_from_slice(&TOPIC.to_le_bytes());
    plugin.push_top(
        package::build_record(PACKAGE, "TopicBoundary", 0, package::Schedule::default())
            .sub(T(b"POBA"), [])
            .sub(T(b"INAM"), 0u32.to_le_bytes())
            .sub(T(b"PDTO"), topic_data),
    );
    plugin.push_top(Record::new(T(b"DIAL"), TOPIC).zsub(T(b"EDID"), "BoundaryTopic"));
    let mut load_order = LoadOrder::new();
    load_order.add("Ai16.esp", plugin).unwrap();
    load_order
}

fn game() -> Game {
    let mut game = Game::new(order());
    game.player();
    assert!(game.world_mut().drive_actor(id(ACTOR)));
    game
}

#[test]
fn ai_16_begin_topic_is_suppressed_in_combat_or_when_pc_dialogue_is_disabled() {
    for (combat, allow_dialogue) in [(true, true), (false, false)] {
        let mut game = game();
        let mut world = game.world_mut();
        let actor = world.actor(id(ACTOR)).unwrap();
        actor.in_combat = combat;
        actor.allow_pc_dialogue = allow_dialogue;
        drop(world);
        game.tick(0.1);
        assert!(game.world().dialogue_session().is_none());
    }
}

#[test]
fn ai_16_begin_topic_does_not_replace_an_existing_dialogue_session() {
    let mut game = game();
    let player = game.world_mut().player();
    game.begin_dialogue(player, id(ACTOR)).unwrap();
    game.tick(0.1);
    assert_eq!(game.world().dialogue_session().unwrap().speaker, player);
}

#[test]
fn ai_16_queued_boundary_fragment_keeps_its_script_function_and_actor_after_save_load() {
    let mut original = game();
    let package_object = original.object_of_form(id(0x802));
    let actor_object = original.object_of_form(id(ACTOR));
    original
        .world_mut()
        .pending
        .push(soe_game::Pending::CallScript {
            object: package_object,
            script: "PackFragments".into(),
            function: "OnBegin".into(),
            args: vec![Value::Object(actor_object)],
        });

    let saved = original.save_state().unwrap();
    let mut restored = Game::new(order());
    restored.load_state(&saved).unwrap();
    assert!(matches!(
        restored.world().pending.last(),
        Some(soe_game::Pending::CallScript { object, script, function, args })
            if *object == package_object
                && script == "PackFragments"
                && function == "OnBegin"
                && args == &[Value::Object(actor_object)]
    ));
}
