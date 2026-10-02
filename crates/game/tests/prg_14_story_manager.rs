// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    quest::state::{StoryEvent, StoryParameter},
    Game,
};

const fn t(bytes: [u8; 4]) -> Tag {
    Tag(bytes)
}
const QUEST: u32 = 0x800;
const EVENT: u32 = 0x801;
const QUEST_NODE: u32 = 0x802;
const QUEST_TWO: u32 = 0x803;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn story_game(reset: bool, hours: f32) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(t(*b"QUST"), QUEST).sub(t(*b"DNAM"), [0; 12]));
    plugin.push_top(
        Record::new(t(*b"SMEN"), EVENT)
            .sub(t(*b"PNAM"), 0u32.to_le_bytes())
            .sub(t(*b"DNAM"), 0u32.to_le_bytes())
            .sub(t(*b"ENAM"), u32::from_le_bytes(*b"SKIL").to_le_bytes()),
    );
    plugin.push_top(
        Record::new(t(*b"SMQN"), QUEST_NODE)
            .sub(t(*b"PNAM"), EVENT.to_le_bytes())
            .sub(t(*b"SNAM"), 0u32.to_le_bytes())
            .sub(t(*b"DNAM"), 0u32.to_le_bytes())
            .sub(t(*b"XNAM"), 1u32.to_le_bytes())
            .sub(t(*b"MNAM"), 1u32.to_le_bytes())
            .sub(t(*b"NNAM"), QUEST.to_le_bytes())
            .sub(t(*b"FNAM"), [u8::from(reset)])
            .sub(t(*b"RNAM"), hours.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Story.esm", plugin).unwrap();
    Game::new(order)
}

fn event() -> StoryEvent {
    StoryEvent {
        id: u32::from_le_bytes(*b"SKIL"),
        code: *b"SKIL",
        parameters: vec![StoryParameter::Integer(23)],
    }
}

fn do_all_game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(t(*b"QUST"), QUEST).sub(t(*b"DNAM"), [0; 12]));
    plugin.push_top(Record::new(t(*b"QUST"), QUEST_TWO).sub(t(*b"DNAM"), [0; 12]));
    plugin.push_top(
        Record::new(t(*b"SMEN"), EVENT)
            .sub(t(*b"DNAM"), 0u32.to_le_bytes())
            .sub(t(*b"ENAM"), u32::from_le_bytes(*b"SKIL").to_le_bytes()),
    );
    plugin.push_top(
        Record::new(t(*b"SMQN"), QUEST_NODE)
            .sub(t(*b"PNAM"), EVENT.to_le_bytes())
            .sub(t(*b"DNAM"), (1_u32 << 16 | 1 << 18).to_le_bytes())
            .sub(t(*b"XNAM"), 1u32.to_le_bytes())
            .sub(t(*b"MNAM"), 1u32.to_le_bytes())
            .sub(t(*b"NNAM"), QUEST.to_le_bytes())
            .sub(t(*b"FNAM"), [0])
            .sub(t(*b"RNAM"), 0.0f32.to_le_bytes())
            .sub(t(*b"NNAM"), QUEST_TWO.to_le_bytes())
            .sub(t(*b"FNAM"), [0])
            .sub(t(*b"RNAM"), 0.0f32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("StorySelection.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn prg_14_authored_event_tree_starts_eligible_quest_once_with_arguments() {
    let mut game = story_game(false, 0.0);
    let story_event = event();
    assert_eq!(
        game.send_story_event(&story_event).unwrap(),
        vec![id(QUEST)]
    );
    assert!(game.world().quest(id(QUEST)).unwrap().running);
    let world = game.world();
    let run = world
        .quest(id(QUEST))
        .unwrap()
        .runtime
        .run_data
        .as_ref()
        .unwrap();
    assert_eq!(run.event.as_ref().unwrap(), &story_event);
    assert_eq!(run.event_flag, 1);
    let pending = &world.pending;
    let story = pending
        .iter()
        .find_map(|item| match item {
            soe_game::Pending::Event { name, args, .. } if name == "OnStoryIncreaseSkill" => {
                Some(args)
            }
            _ => None,
        })
        .unwrap();
    assert!(matches!(story.as_slice(), [soe_papyrus::Value::Int(23)]));
    drop(world);
    assert!(game.send_story_event(&story_event).unwrap().is_empty());
}

#[test]
fn prg_14_saved_selection_and_reset_deadline_survive_reload() {
    let mut game = story_game(true, 24.0);
    let story_event = event();
    assert_eq!(
        game.send_story_event(&story_event).unwrap(),
        vec![id(QUEST)]
    );
    game.world_mut().stop_quest(id(QUEST));
    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    game.world_mut().clock.game_days = 0.5;
    assert!(game.send_story_event(&story_event).unwrap().is_empty());
    game.world_mut().clock.game_days = 1.0;
    assert_eq!(
        game.send_story_event(&story_event).unwrap(),
        vec![id(QUEST)]
    );
}

#[test]
fn prg_14_do_all_selection_cursor_survives_reload() {
    let mut game = do_all_game();
    let story_event = event();
    let first = game.send_story_event(&story_event).unwrap();
    assert_eq!(first.len(), 1);
    game.world_mut().stop_quest(first[0]);
    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    let second = game.send_story_event(&story_event).unwrap();
    assert_eq!(second.len(), 1);
    assert_ne!(first[0], second[0]);
}
