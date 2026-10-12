// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! HST-28 dialogue availability and empty-response edge cases.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for base in [7, 20] {
        plugin.push_top(
            Record::new(Tag(*b"NPC_"), base)
                .zsub(Tag(*b"EDID"), if base == 7 { "Player" } else { "Speaker" })
                .sub(Tag(*b"ACBS"), [0; 24]),
        );
    }
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), 21)
            .sub(Tag(*b"NAME"), 20_u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Dialogue.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn hst_28_blocked_disabled_dead_and_hostile_actors_cannot_start_dialogue() {
    let mut game = game();
    assert!(game.player_dialogue_available(id(21)));
    game.world_mut()
        .reference(id(21))
        .unwrap()
        .activation_blocked = true;
    assert!(!game.player_dialogue_available(id(21)));
    game.world_mut()
        .reference(id(21))
        .unwrap()
        .activation_blocked = false;
    game.world_mut().set_reference_enabled(id(21), false);
    assert!(!game.player_dialogue_available(id(21)));
    game.world_mut().set_reference_enabled(id(21), true);
    game.world_mut().actor(id(21)).unwrap().dead = true;
    assert!(!game.player_dialogue_available(id(21)));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(21)).unwrap();
        actor.dead = false;
        actor.in_combat = true;
    }
    assert!(!game.player_dialogue_available(id(21)));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(21)).unwrap();
        actor.in_combat = false;
        actor.unconscious = true;
    }
    assert!(!game.player_dialogue_available(id(21)));
    {
        let mut world = game.world_mut();
        let actor = world.actor(id(21)).unwrap();
        actor.unconscious = false;
        actor.allow_pc_dialogue = false;
    }
    assert!(!game.player_dialogue_available(id(21)));
}

#[test]
fn hst_28_speaker_without_eligible_greeting_returns_to_play_without_session() {
    let mut game = game();
    assert!(game.begin_player_dialogue(id(21)).unwrap().is_none());
    assert!(game.world().dialogue_session().is_none());
    assert!(game.begin_player_dialogue(id(21)).unwrap().is_none());
    assert!(game.world().dialogue_session().is_none());
}
