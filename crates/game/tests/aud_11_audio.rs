// SPDX-License-Identifier: GPL-3.0-or-later

//! Ambient speech uses the same DIAL/INFO selection and session as conversations.

// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SPEAKER: u32 = 0x100;
const BASE: u32 = 0x500;
const QUEST: u32 = 0x800;
const TOPIC: u32 = 0x900;
const INFO: u32 = 0xA00;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn greeting_condition(expected: f32, function: u16) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[4..8].copy_from_slice(&expected.to_le_bytes());
    data[8..10].copy_from_slice(&function.to_le_bytes());
    data
}

fn fixture(subtype: u16, signature: Option<[u8; 4]>, expected_greeting: f32) -> Game {
    fixture_condition(subtype, signature, expected_greeting, 123)
}

fn fixture_condition(
    subtype: u16,
    signature: Option<[u8; 4]>,
    expected_greeting: f32,
    function: u16,
) -> Game {
    fixture_package(subtype, signature, expected_greeting, function, None)
}

fn fixture_package(
    subtype: u16,
    signature: Option<[u8; 4]>,
    expected_greeting: f32,
    function: u16,
    interrupts: Option<u16>,
) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"NPC_"), 7)
            .zsub(T(b"EDID"), "Player")
            .sub(T(b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(T(b"NPC_"), BASE)
            .zsub(T(b"EDID"), "Greeter")
            .sub(T(b"ACBS"), vec![0; 24]),
    );
    let mut placement = vec![0; 24];
    placement[..4].copy_from_slice(&20.0f32.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"ACHR"), SPEAKER)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), placement),
    );
    let mut quest_data = vec![0; 12];
    quest_data[2] = 50;
    plugin.push_top(
        Record::new(T(b"QUST"), QUEST)
            .sub(T(b"DNAM"), quest_data)
            .sub(T(b"NEXT"), [])
            .sub(T(b"INDX"), [0, 0, 0, 0]),
    );

    let mut topic_data = [0; 4];
    topic_data[1] = 7;
    topic_data[2..4].copy_from_slice(&subtype.to_le_bytes());
    let topic = Record::new(T(b"DIAL"), TOPIC)
        .zsub(T(b"FULL"), "Hello")
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"BNAM"), (TOPIC + 0x1000).to_le_bytes())
        .sub(T(b"DATA"), topic_data)
        .sub(T(b"PNAM"), 50.0f32.to_le_bytes());
    let topic = if let Some(signature) = signature {
        topic.sub(T(b"SNAM"), signature)
    } else {
        topic
    };
    let branch = Record::new(T(b"DLBR"), TOPIC + 0x1000)
        .sub(T(b"QNAM"), QUEST.to_le_bytes())
        .sub(T(b"TNAM"), 0u32.to_le_bytes())
        .sub(T(b"DNAM"), 1u32.to_le_bytes())
        .sub(T(b"SNAM"), TOPIC.to_le_bytes());
    plugin.push_top(branch);

    let mut response = vec![0; 24];
    response[12] = 1;
    let info = Record::new(T(b"INFO"), INFO)
        .sub(T(b"ENAM"), [0, 0, 0, 0])
        .sub(T(b"CTDA"), greeting_condition(expected_greeting, function))
        .sub(T(b"TRDT"), response)
        .zsub(T(b"NAM1"), "Welcome, traveler.");
    let mut topic_group = Group::new(GroupKind::Top(T(b"DIAL")));
    topic_group
        .entries
        .push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(TOPIC));
    children
        .entries
        .push(Entry::Record(plugin.push_record(info)));
    topic_group.entries.push(Entry::Group(children));
    plugin.groups.push(topic_group);

    if let Some(interrupts) = interrupts {
        let mut data = vec![0; 12];
        data[4] = 18;
        data[8..10].copy_from_slice(&interrupts.to_le_bytes());
        plugin.push_top(Record::new(T(b"PACK"), 0xB00).sub(T(b"PKDT"), data));
    }
    let mut order = LoadOrder::new();
    order.add("Greeting.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.player();
    game.world_mut().drive_actor(id(SPEAKER));
    game.start_quest(id(QUEST));
    if interrupts.is_some() {
        let mut world = game.world_mut();
        world.actor(id(SPEAKER)).unwrap().ai.package = Some(id(0xB00));
        world.debug_ai_enabled = false;
    }
    game
}

#[test]
fn aud_11_nearby_hello_and_idle_topics_start_once_without_activation() {
    for (subtype, signature) in [
        (79, None),
        (94, None),
        (0, Some(*b"HELO")),
        (0, Some(*b"IDLE")),
    ] {
        let mut game = fixture(subtype, signature, 1.0);
        game.tick(0.016);
        assert!(game.ambient_dialogue());
        assert_eq!(game.dialogue_line().unwrap().unwrap().info, id(INFO));
        game.tick(0.016);
        assert_eq!(game.world().dialogue_session().unwrap().response_index, 0);
        game.advance_dialogue().unwrap();
        game.close_dialogue();
        game.tick(0.016);
        assert!(game.world().dialogue_session().is_none());
    }
}

#[test]
fn aud_11_out_of_range_npc_speaks_only_after_entering_range() {
    let mut game = fixture(94, None, 1.0);
    game.world_mut().reference(id(SPEAKER)).unwrap().position = [10_000.0, 0.0, 0.0];
    game.tick(0.016);
    assert!(game.world().dialogue_session().is_none());
    game.world_mut().reference(id(SPEAKER)).unwrap().position = [20.0, 0.0, 0.0];
    game.tick(0.016);
    assert!(game.ambient_dialogue());
    game.world_mut().reference(id(SPEAKER)).unwrap().position = [10_000.0, 0.0, 0.0];
    game.tick(0.016);
    assert!(game.world().dialogue_session().is_none());
}

#[test]
fn aud_11_unavailable_npcs_and_false_conditions_do_not_speak() {
    for state in 0..6 {
        let mut game = fixture(94, None, 1.0);
        {
            let mut world = game.world_mut();
            match state {
                0 => world.actor(id(SPEAKER)).unwrap().dead = true,
                1 => world.actor(id(SPEAKER)).unwrap().in_combat = true,
                2 => world.actor(id(SPEAKER)).unwrap().unconscious = true,
                3 => world.actor(id(SPEAKER)).unwrap().allow_pc_dialogue = false,
                4 => world.reference(id(SPEAKER)).unwrap().enabled = false,
                _ => world.reference(id(SPEAKER)).unwrap().alpha = 0.5,
            }
        }
        game.tick(0.016);
        assert!(game.world().dialogue_session().is_none(), "state {state}");
    }
    let mut game = fixture(94, None, 0.0);
    game.tick(0.016);
    assert!(game.world().dialogue_session().is_none());
}

#[test]
fn aud_11_ambient_topics_do_not_interrupt_an_open_conversation() {
    let mut game = fixture(94, None, 1.0);
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    game.tick(0.016);
    assert_eq!(game.world().dialogue_session().unwrap().current_info, None);
}

fn ambience_world(chance: f32, flags: u32) -> soe_game::World {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x1000)
            .sub(T(b"DATA"), [1, 0])
            .sub(T(b"XCAS"), 0x1001u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ASPC"), 0x1001)
            .sub(T(b"SNAM"), 0x1002u32.to_le_bytes())
            .sub(T(b"RDAT"), 0x1003u32.to_le_bytes()),
    );
    let mut entry = Vec::new();
    entry.extend_from_slice(&0x1004u32.to_le_bytes());
    entry.extend_from_slice(&flags.to_le_bytes());
    entry.extend_from_slice(&chance.to_le_bytes());
    plugin.push_top(
        Record::new(T(b"REGN"), 0x1003)
            .sub(T(b"RDAT"), [7, 0, 0, 0, 0, 50, 0, 0])
            .sub(T(b"RDSA"), entry),
    );
    let mut order = LoadOrder::new();
    order.add("Ambience.esm", plugin).unwrap();
    soe_game::World::new(order)
}

#[test]
fn aud_11_cell_acoustic_space_and_region_select_actual_sound_descriptors() {
    let world = ambience_world(1.0, 1);
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.5).unwrap(),
        [id(0x1002), id(0x1004)]
    );
}

#[test]
fn aud_11_region_weather_mask_and_chance_filter_ineligible_sounds() {
    let world = ambience_world(0.5, 4);
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.1).unwrap(),
        [id(0x1002)]
    );
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 4, 0.6).unwrap(),
        [id(0x1002)]
    );
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 4, 0.1).unwrap(),
        [id(0x1002), id(0x1004)]
    );
}

#[test]
fn aud_11_invalid_ambience_chance_and_draw_return_errors() {
    for chance in [f32::NAN, -1.0, 101.0] {
        let world = ambience_world(chance, 1);
        assert!(soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.1).is_err());
    }
    let world = ambience_world(1.0, 1);
    assert!(soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, f32::NAN).is_err());
}

#[test]
fn aud_11_stopped_quests_other_cells_and_menu_pause_do_not_start_speech() {
    let mut stopped = fixture(94, None, 1.0);
    stopped.world_mut().stop_quest(id(QUEST));
    stopped.tick(0.016);
    assert!(stopped.world().dialogue_session().is_none());
    let mut other_cell = fixture(94, None, 1.0);
    other_cell.world_mut().reference(id(SPEAKER)).unwrap().cell = Some(id(0xFFFF));
    other_cell.tick(0.016);
    assert!(other_cell.world().dialogue_session().is_none());
    let mut paused = fixture(94, None, 1.0);
    paused.tick_menu_mode(1.0, true);
    assert!(paused.world().dialogue_session().is_none());
}

#[test]
fn aud_11_nonambient_misc_topics_do_not_start_as_idle_speech() {
    let mut game = fixture(78, None, 1.0);
    game.tick(0.016);
    assert!(game.world().dialogue_session().is_none());
}

#[test]
fn aud_11_high_priority_region_override_suppresses_lower_priority_sounds() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x1000).sub(T(b"DATA"), [0]).sub(
            T(b"XCLR"),
            [0x1001u32, 0x1002u32]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>(),
        ),
    );
    for (region, priority, override_previous, sound) in [
        (0x1001, 10, false, 0x1003u32),
        (0x1002, 90, true, 0x1004u32),
    ] {
        let mut entry = sound.to_le_bytes().to_vec();
        entry.extend_from_slice(&1u32.to_le_bytes());
        entry.extend_from_slice(&1.0f32.to_le_bytes());
        plugin.push_top(
            Record::new(T(b"REGN"), region)
                .sub(
                    T(b"RDAT"),
                    [7, 0, 0, 0, u8::from(override_previous), priority, 0, 0],
                )
                .sub(T(b"RDSA"), entry),
        );
    }
    let mut order = LoadOrder::new();
    order.add("Regions.esm", plugin).unwrap();
    let world = soe_game::World::new(order);
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.5).unwrap(),
        [id(0x1004)]
    );
}

#[test]
fn aud_11_truncated_regional_sound_entry_returns_an_error() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x1000)
            .sub(T(b"DATA"), [0])
            .sub(T(b"XCLR"), 0x1001u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"REGN"), 0x1001)
            .sub(T(b"RDAT"), [7, 0, 0, 0, 0, 1, 0, 0])
            .sub(T(b"RDSA"), vec![0; 11]),
    );
    let mut order = LoadOrder::new();
    order.add("Truncated.esm", plugin).unwrap();
    let world = soe_game::World::new(order);
    assert!(soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.1).is_err());
}

#[test]
fn aud_11_music_type_follows_cell_then_region_then_world_and_silence() {
    let mut plugin = Plugin::new(FileHeader::default());
    for (cell, music, region) in [
        (0x1000, Some(0x2000u32), Some(0x3000u32)),
        (0x1001, None, Some(0x3000)),
        (0x1002, None, None),
        (0x1003, Some(0), None),
    ] {
        let mut record = Record::new(T(b"CELL"), cell).sub(T(b"DATA"), [0]);
        record.placement.world = Some(0x4000);
        if let Some(music) = music {
            record = record.sub(T(b"XCMO"), music.to_le_bytes());
        }
        if let Some(region) = region {
            record = record.sub(T(b"XCLR"), region.to_le_bytes());
        }
        plugin.push_top(record);
    }
    plugin.push_top(
        Record::new(T(b"REGN"), 0x3000)
            .sub(T(b"RDAT"), [7, 0, 0, 0, 0, 1, 0, 0])
            .sub(T(b"RDMO"), 0x2001u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"WRLD"), 0x4000).sub(T(b"ZNAM"), 0x2002u32.to_le_bytes()));
    let mut order = LoadOrder::new();
    order.add("Music.esm", plugin).unwrap();
    let world = soe_game::World::new(order);
    for (cell, expected) in [
        (0x1000, Some(id(0x2000))),
        (0x1001, Some(id(0x2001))),
        (0x1002, Some(id(0x2002))),
        (0x1003, None),
    ] {
        assert_eq!(
            soe_game::ambience::music_type_for_cell(&world, id(cell)).unwrap(),
            expected
        );
    }
}

#[test]
fn aud_11_explicit_conversation_interrupts_ambient_response() {
    let mut game = fixture(94, None, 1.0);
    game.tick(0.016);
    assert!(game.ambient_dialogue());
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert!(!game.ambient_dialogue());
    assert_eq!(game.world().dialogue_session().unwrap().current_info, None);
}

#[test]
fn aud_11_rejected_conversation_keeps_the_current_ambient_response() {
    let mut game = fixture(94, None, 1.0);
    game.tick(0.016);
    let before = game.world().dialogue_session().cloned();
    let player = game.player();
    assert!(game.begin_dialogue(player, player).is_err());
    assert_eq!(game.world().dialogue_session(), before.as_ref());
}

#[test]
fn aud_11_acoustic_space_region_link_only_applies_to_interiors() {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"CELL"), 0x1000)
            .sub(T(b"DATA"), [0])
            .sub(T(b"XCAS"), 0x1001u32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"ASPC"), 0x1001)
            .sub(T(b"SNAM"), 0x1002u32.to_le_bytes())
            .sub(T(b"RDAT"), 0xFFFFu32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Exterior.esm", plugin).unwrap();
    let world = soe_game::World::new(order);
    assert_eq!(
        soe_game::ambience::descriptors_for_cell(&world, id(0x1000), 1, 0.1).unwrap(),
        [id(0x1002)]
    );
}

#[test]
fn aud_11_idle_eligibility_and_selection_share_dialogue_participant_context() {
    let mut game = fixture_condition(94, None, 1.0, 249);
    game.tick(0.016);
    assert_eq!(game.dialogue_line().unwrap().unwrap().info, id(INFO));
}

#[test]
fn aud_11_package_interrupt_flags_control_hello_and_idle_chatter() {
    for (subtype, interrupts, expected) in [
        (79, 0, false),
        (79, 1, true),
        (79, 128, false),
        (94, 1, false),
        (94, 128, true),
    ] {
        let mut game = fixture_package(subtype, None, 1.0, 123, Some(interrupts));
        game.tick(0.016);
        assert_eq!(
            game.ambient_dialogue(),
            expected,
            "subtype {subtype}, flags {interrupts}"
        );
    }
}
