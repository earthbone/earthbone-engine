// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Conversations over authored DIAL groups, INFO conditions and VMAD fragments.
//! Assertions cover quest state changes and response timing, not just parsing.

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::dialogue::Error;
use soe_game::vmad::{InfoFragment, InfoFragments, ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, IAdd};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SPEAKER: u32 = 0x100;
const BASE: u32 = 0x500;
const QUEST: u32 = 0x800;
const OTHER_QUEST: u32 = 0x801;
const TOPIC: u32 = 0x900;
const LINKED_TOPIC: u32 = 0x901;
const INFO: u32 = 0xA00;
const SHARED: u32 = 0xA01;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn condition(function: u16, parameter: u32, expected: f32, run_on: u32) -> Vec<u8> {
    let mut data = vec![0; 32];
    data[4..8].copy_from_slice(&expected.to_le_bytes());
    data[8..10].copy_from_slice(&function.to_le_bytes());
    data[12..16].copy_from_slice(&parameter.to_le_bytes());
    data[20..24].copy_from_slice(&run_on.to_le_bytes());
    data
}

fn quest(form: u32, priority: u8) -> Record {
    let mut data = vec![0; 12];
    data[2] = priority;
    Record::new(T(b"QUST"), form)
        .sub(T(b"DNAM"), data)
        .sub(T(b"NEXT"), [])
        .sub(T(b"INDX"), [0, 0, 0, 0])
        .sub(T(b"INDX"), [5, 0, 0, 0])
        .sub(T(b"INDX"), [10, 0, 0, 0])
        .sub(T(b"INDX"), [20, 0, 0, 0])
}

fn topic(form: u32, quest: u32) -> Record {
    Record::new(T(b"DIAL"), form)
        .zsub(T(b"FULL"), "A question")
        .sub(T(b"QNAM"), quest.to_le_bytes())
        .sub(T(b"BNAM"), (form + 0x1000).to_le_bytes())
        .sub(T(b"DATA"), [0, 0, 0, 0])
        .sub(T(b"SNAM"), *b"CUST")
        .sub(T(b"PNAM"), 50.0f32.to_le_bytes())
}

fn add_topic(plugin: &mut Plugin, topic: Record, infos: Vec<Record>, top_level: bool) {
    let raw = topic.form_id;
    let quest = topic.find(T(b"QNAM")).unwrap().to_vec();
    if top_level {
        plugin.push_top(
            Record::new(T(b"DLBR"), raw + 0x1000)
                .sub(T(b"QNAM"), quest)
                .sub(T(b"TNAM"), 0u32.to_le_bytes())
                .sub(T(b"DNAM"), 1u32.to_le_bytes())
                .sub(T(b"SNAM"), raw.to_le_bytes()),
        );
    }
    let mut group = Group::new(GroupKind::Top(T(b"DIAL")));
    group.entries.push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(raw));
    for info in infos {
        children
            .entries
            .push(Entry::Record(plugin.push_record(info)));
    }
    group.entries.push(Entry::Group(children));
    plugin.groups.push(group);
}

fn info(form: u32, flags: u16, lines: &[&str], scripts: bool) -> Record {
    let mut enam = flags.to_le_bytes().to_vec();
    enam.extend_from_slice(&0u16.to_le_bytes());
    let mut record = Record::new(T(b"INFO"), form).sub(T(b"ENAM"), enam);
    if scripts {
        let fragment = |function: &str| InfoFragment {
            marker: 1,
            script: "TopicFragments".into(),
            function: function.into(),
        };
        let vmad = Vmad {
            scripts: vec![
                ScriptEntry::new("WrongFragments"),
                ScriptEntry::new("TopicFragments"),
            ],
            info_fragments: Some(InfoFragments {
                version: 2,
                file: "TopicFragments".into(),
                begin: Some(fragment("Fragment_0")),
                end: Some(fragment("Fragment_1")),
            }),
            ..Vmad::default()
        };
        record = record.sub(T(b"VMAD"), vmad.write(T(b"INFO")));
    }
    for (index, text) in lines.iter().enumerate() {
        let mut response = vec![0; 24];
        response[12] = index as u8 + 1;
        record = record.sub(T(b"TRDT"), response).zsub(T(b"NAM1"), text);
    }
    record
}

fn plugin(infos: Vec<Record>) -> Plugin {
    let mut plugin = Plugin::new(FileHeader::default());
    for base in [7, BASE] {
        plugin.push_top(
            Record::new(T(b"NPC_"), base)
                .zsub(T(b"EDID"), if base == 7 { "Player" } else { "Speaker" })
                .sub(T(b"ACBS"), vec![0; 24]),
        );
    }
    plugin.push_top(
        Record::new(T(b"ACHR"), SPEAKER)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    plugin.push_top(quest(QUEST, 50));
    plugin.push_top(quest(OTHER_QUEST, 60));
    add_topic(&mut plugin, topic(TOPIC, QUEST), infos, true);
    plugin
}

fn game(plugin: Plugin) -> Game {
    let mut order = LoadOrder::new();
    order.add("Dialogue.esm", plugin).unwrap();
    game_with_order(order)
}

fn game_with_order(order: LoadOrder) -> Game {
    let mut game = Game::new(order);
    let fragment = |name: &str, stage: i32, counter: &str| {
        FunctionBuilder::new(name, "None")
            .param("akSpeakerRef", "ObjectReference")
            .local("quest", "Quest")
            .local("unused", "None")
            .ins(IAdd, vec![ident(counter), ident(counter), i(1)])
            .ins(
                CallMethod,
                vec![s("GetOwningQuest"), ident("self"), ident("quest")],
            )
            .ins(
                CallMethod,
                vec![s("SetStage"), ident("quest"), ident("unused"), i(stage)],
            )
            .build()
    };
    let scripts = [
        ScriptBuilder::new("TopicFragments", "TopicInfo")
            .variable("begins", "Int", i(0))
            .variable("ends", "Int", i(0))
            .function(fragment("Fragment_0", 10, "begins"))
            .function(fragment("Fragment_1", 20, "ends"))
            .bytes(),
        ScriptBuilder::new("WrongFragments", "TopicInfo")
            .variable("wrong", "Int", i(0))
            .function(
                FunctionBuilder::new("Fragment_0", "None")
                    .param("speaker", "ObjectReference")
                    .ins(IAdd, vec![ident("wrong"), ident("wrong"), i(1)])
                    .build(),
            )
            .bytes(),
    ];
    for script in scripts {
        game.load_script(&script).unwrap();
    }
    game.bind();
    game
}

fn open(game: &mut Game) {
    let player = game.player();
    game.start_quest(id(QUEST));
    game.begin_dialogue(id(SPEAKER), player).unwrap();
}

fn counter(game: &mut Game, variable: &str) -> i32 {
    let object = game.object_of_form(id(INFO));
    game.vm
        .get_var(object, "TopicFragments", variable)
        .unwrap()
        .as_int()
}

#[test]
fn fragments_run_on_the_named_script_and_the_group_owning_quest() {
    let mut plugin = plugin(vec![
        info(INFO, 0, &["First", "Second"], true).sub(T(b"TPIC"), LINKED_TOPIC.to_le_bytes())
    ]);
    add_topic(&mut plugin, topic(LINKED_TOPIC, OTHER_QUEST), vec![], false);
    let mut game = game(plugin);
    open(&mut game);
    assert_eq!(game.world().owning_quest(id(INFO)), Some(id(QUEST)));
    assert_eq!(
        game.choose_dialogue(id(TOPIC))
            .unwrap()
            .unwrap()
            .response
            .number,
        1
    );
    assert_eq!(counter(&mut game, "begins"), 1);
    assert_eq!(counter(&mut game, "ends"), 0);
    assert_eq!(game.world_mut().quest_state(id(QUEST)).unwrap().highest, 10);
    assert_eq!(game.advance_dialogue().unwrap().unwrap().response.number, 2);
    assert_eq!(counter(&mut game, "ends"), 0);
    assert!(game.advance_dialogue().unwrap().is_none());
    assert_eq!(counter(&mut game, "ends"), 1);
    assert_eq!(game.world_mut().quest_state(id(QUEST)).unwrap().highest, 20);
    assert_eq!(
        game.world_mut()
            .quest_state(id(OTHER_QUEST))
            .unwrap()
            .highest,
        0
    );
    let object = game.object_of_form(id(INFO));
    assert_eq!(
        game.vm.get_var(object, "WrongFragments", "wrong"),
        Some(Value::Int(0))
    );
}

#[test]
fn inactive_quest_suppresses_its_topic() {
    let mut game = game(plugin(vec![info(INFO, 0, &["Hidden"], false)]));
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert!(game.dialogue_choices().unwrap().available.is_empty());
    game.start_quest(id(QUEST));
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
}

#[test]
fn info_conditions_use_speaker_and_listener_contexts() {
    let record = info(INFO, 0, &["For you"], false)
        .sub(T(b"CTDA"), condition(72, BASE, 1.0, 0))
        .sub(T(b"CTDA"), condition(72, 7, 1.0, 1));
    let mut game = game(plugin(vec![record]));
    open(&mut game);
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
}

#[test]
fn quest_dialogue_conditions_do_not_include_story_manager_conditions() {
    let mut plugin = plugin(vec![info(INFO, 0, &["Allowed"], false)]);
    let index = plugin
        .records
        .iter()
        .position(|r| r.form_id == QUEST)
        .unwrap();
    plugin.records[index] = Record::new(T(b"QUST"), QUEST)
        .sub(T(b"DNAM"), vec![0; 12])
        .sub(T(b"CTDA"), condition(72, BASE, 1.0, 0))
        .sub(T(b"NEXT"), [])
        .sub(T(b"CTDA"), condition(72, BASE, 0.0, 0));
    let mut game = game(plugin);
    open(&mut game);
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
}

#[test]
fn shared_response_data_does_not_transfer_fragments_or_ownership() {
    let mut plugin = plugin(vec![
        info(INFO, 0, &["Discarded local text"], true).sub(T(b"DNAM"), SHARED.to_le_bytes())
    ]);
    add_topic(
        &mut plugin,
        topic(LINKED_TOPIC, OTHER_QUEST),
        vec![info(SHARED, 1, &["Shared response"], false)],
        false,
    );
    let mut game = game(plugin);
    open(&mut game);
    let line = game.choose_dialogue(id(TOPIC)).unwrap().unwrap();
    assert_eq!(line.response_info, id(SHARED));
    assert_eq!(line.info, id(INFO));
    assert_eq!(counter(&mut game, "begins"), 1);
    game.advance_dialogue().unwrap();
    assert!(
        game.world().dialogue_session().is_some(),
        "shared INFO goodbye flag is not inherited"
    );
}

#[test]
fn goodbye_closes_only_after_the_last_response_and_end_fragment() {
    let mut game = game(plugin(vec![info(INFO, 1, &["Bye"], true)]));
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    assert!(game.world().dialogue_session().is_some());
    game.advance_dialogue().unwrap();
    assert!(game.world().dialogue_session().is_none());
    assert_eq!(counter(&mut game, "ends"), 1);
}

#[test]
fn interrupted_response_does_not_run_end_fragment() {
    let mut game = game(plugin(vec![info(INFO, 0, &["Interrupted"], true)]));
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.close_dialogue();
    assert_eq!(counter(&mut game, "ends"), 0);
}

#[test]
fn stale_choice_is_rechecked_before_running_its_fragment() {
    let record =
        info(INFO, 0, &["Only at stage zero"], true).sub(T(b"CTDA"), condition(58, QUEST, 0.0, 0));
    let mut game = game(plugin(vec![record]));
    open(&mut game);
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
    assert!(game.set_stage(id(QUEST), 5));
    assert!(game.choose_dialogue(id(TOPIC)).is_err());
    assert_eq!(counter(&mut game, "begins"), 0);
}

#[test]
fn topic_links_replace_the_root_choices_after_completion() {
    let mut plugin = plugin(vec![
        info(INFO, 0, &["Choose next"], false).sub(T(b"TCLT"), LINKED_TOPIC.to_le_bytes())
    ]);
    add_topic(
        &mut plugin,
        topic(LINKED_TOPIC, QUEST),
        vec![info(SHARED, 0, &["Next"], false)],
        false,
    );
    let mut game = game(plugin);
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.advance_dialogue().unwrap();
    let choices = game.dialogue_choices().unwrap();
    assert_eq!(
        choices
            .available
            .iter()
            .map(|c| c.topic)
            .collect::<Vec<_>>(),
        vec![id(LINKED_TOPIC)]
    );
}

#[test]
fn save_restores_response_cursor_without_replaying_begin_fragment() {
    let fixture = || plugin(vec![info(INFO, 4, &["First", "Second"], true)]);
    let mut original = game(fixture());
    open(&mut original);
    original.choose_dialogue(id(TOPIC)).unwrap();
    original.advance_dialogue().unwrap();
    let bytes = original.save_state().unwrap();
    let mut restored = game(fixture());
    restored.load_state(&bytes).unwrap();
    assert_eq!(
        restored.dialogue_line().unwrap(),
        original.dialogue_line().unwrap()
    );
    assert_eq!(restored.world().state_hash(), original.world().state_hash());
    assert_eq!(counter(&mut restored, "begins"), 1);
    restored.advance_dialogue().unwrap();
    assert_eq!(counter(&mut restored, "ends"), 1);
    assert!(restored.dialogue_choices().unwrap().available.is_empty());
}

#[test]
fn save_restores_dialogue_and_magic_without_reapplying_the_buff() {
    let fixture = || {
        let mut plugin = plugin(vec![info(INFO, 0, &["First", "Second"], true)]);
        let mut effect = vec![0; 152];
        for (offset, value) in [(0, 2u32), (16, u32::MAX), (68, 24), (80, 1), (84, 3)] {
            effect[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        plugin.push_top(Record::new(T(b"MGEF"), 0xB00).sub(T(b"DATA"), effect));
        let mut spell = vec![0; 36];
        spell[16..20].copy_from_slice(&1u32.to_le_bytes());
        spell[20..24].copy_from_slice(&3u32.to_le_bytes());
        let mut magnitude = 20.0f32.to_le_bytes().to_vec();
        magnitude.extend_from_slice(&0u32.to_le_bytes());
        magnitude.extend_from_slice(&10u32.to_le_bytes());
        plugin.push_top(
            Record::new(T(b"SPEL"), 0xB01)
                .sub(T(b"SPIT"), spell)
                .sub(T(b"EFID"), 0xB00u32.to_le_bytes())
                .sub(T(b"EFIT"), magnitude),
        );
        plugin
    };
    let mut original = game(fixture());
    open(&mut original);
    original
        .world_mut()
        .set_actor_value(id(SPEAKER), "health", 100.0)
        .unwrap();
    original
        .world_mut()
        .mod_actor_value(id(SPEAKER), "health", -15.0)
        .unwrap();
    original
        .world_mut()
        .damage_actor_value(id(SPEAKER), "health", 30.0)
        .unwrap();
    let shared = original.shared_world();
    shared
        .borrow_mut()
        .cast_spell(
            id(0xB01),
            id(SPEAKER),
            Some(id(SPEAKER)),
            Some(id(SPEAKER)),
            &mut |name, class| original.vm.create_object_of(name, class),
        )
        .unwrap();
    original.choose_dialogue(id(TOPIC)).unwrap();
    original.advance_dialogue().unwrap();
    let bytes = original.save_state().unwrap();
    let mut restored = game(fixture());
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.world().state_hash(), original.world().state_hash());
    assert_eq!(
        restored.dialogue_line().unwrap(),
        original.dialogue_line().unwrap()
    );
    assert_eq!(counter(&mut restored, "begins"), 1);
    assert_eq!(
        restored.world_mut().actor_value(id(SPEAKER), "health"),
        75.0
    );
    restored
        .world_mut()
        .restore_actor_value(id(SPEAKER), "health", 100.0)
        .unwrap();
    assert_eq!(
        restored.world_mut().actor_value(id(SPEAKER), "health"),
        105.0
    );
    restored.tick(10.0);
    assert_eq!(restored.world().active_effects().count(), 0);
    assert_eq!(
        restored.world_mut().actor_value(id(SPEAKER), "health"),
        85.0
    );
    restored.advance_dialogue().unwrap();
    assert_eq!(counter(&mut restored, "ends"), 1);
}

#[test]
fn info_vmad_round_trips_begin_end_and_marker_bytes() {
    let record = info(INFO, 0, &[], true);
    let bytes = record.find(T(b"VMAD")).unwrap();
    let mut parsed = Vmad::parse(bytes, T(b"INFO")).unwrap();
    assert_eq!(parsed.write(T(b"INFO")), bytes);
    parsed.info_fragments = None;
    let scripts_end = parsed.write(T(b"INFO")).len();
    for cut in 7..bytes.len() {
        let result = Vmad::parse(&bytes[..cut], T(b"INFO"));
        assert_eq!(result.is_ok(), cut == scripts_end, "cut at {cut}");
    }
}

#[test]
fn missing_fragment_rejects_without_consuming_say_once() {
    let mut record = info(INFO, 4, &["Still available"], true);
    let vmad = record
        .subrecords
        .iter_mut()
        .find(|s| s.tag == T(b"VMAD"))
        .unwrap();
    let mut parsed = Vmad::parse(&vmad.data, T(b"INFO")).unwrap();
    parsed
        .info_fragments
        .as_mut()
        .unwrap()
        .begin
        .as_mut()
        .unwrap()
        .function = "Missing".into();
    vmad.data = parsed.write(T(b"INFO"));
    let mut game = game(plugin(vec![record]));
    open(&mut game);
    assert!(matches!(
        game.choose_dialogue(id(TOPIC)),
        Err(Error::Script(_))
    ));
    assert!(game
        .world()
        .dialogue_session()
        .unwrap()
        .current_info
        .is_none());
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
}

#[test]
fn pnam_orders_inserted_info_and_keeps_deleted_anchors() {
    let base = plugin(vec![
        info(INFO, 0, &["Anchor"], false),
        info(INFO + 1, 0, &["Last"], false),
    ]);
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Dialogue.esm".into()],
        ..FileHeader::default()
    });
    let mut group = Group::new(GroupKind::Top(T(b"DIAL")));
    let mut children = Group::new(GroupKind::TopicChildren(TOPIC));
    children.entries.push(Entry::Record(
        patch.push_record(info(INFO, 0, &[], false).with_flags(0x20)),
    ));
    let added = 0x0100_0A02;
    children.entries.push(Entry::Record(patch.push_record(
        info(added, 0, &["Inserted"], false).sub(T(b"PNAM"), INFO.to_le_bytes()),
    )));
    group.entries.push(Entry::Group(children));
    patch.groups.push(group);
    let mut order = LoadOrder::new();
    order.add("Dialogue.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut game = game_with_order(order);
    let added = GlobalFormId {
        file: 1,
        object: 0xA02,
    };
    assert_eq!(
        game.world().dialogue_infos(id(TOPIC)).unwrap(),
        vec![id(INFO), added, id(INFO + 1)]
    );
    open(&mut game);
    assert_eq!(
        game.choose_dialogue(id(TOPIC)).unwrap().unwrap().info,
        added
    );
}

#[test]
fn cyclic_info_order_is_reported_without_hanging() {
    let mut game = game(plugin(vec![
        info(INFO, 0, &[], false).sub(T(b"PNAM"), (INFO + 1).to_le_bytes()),
        info(INFO + 1, 0, &[], false).sub(T(b"PNAM"), INFO.to_le_bytes()),
    ]));
    open(&mut game);
    let choices = game.dialogue_choices().unwrap();
    assert!(choices.available.is_empty());
    assert!(choices
        .rejected
        .iter()
        .any(|(_, reason)| reason.contains("cyclic")));
}

#[test]
fn response_reading_and_choice_listing_do_not_advance_world_rng() {
    let mut game = game(plugin(vec![
        info(INFO, 2, &["Random A"], false),
        info(INFO + 1, 2 | 32, &["Random B"], false),
    ]));
    open(&mut game);
    let before = game.world().state_hash().rng;
    game.dialogue_choices().unwrap();
    game.dialogue_choices().unwrap();
    assert_eq!(before, game.world().state_hash().rng);
    game.choose_dialogue(id(TOPIC)).unwrap();
    assert_ne!(before, game.world().state_hash().rng);
    let before = game.world().state_hash().rng;
    game.dialogue_line().unwrap();
    assert_eq!(before, game.world().state_hash().rng);
}

#[test]
fn actor_dialogue_target_keeps_the_player_native_exception() {
    let mut game = game(plugin(vec![info(INFO, 0, &["Hello"], false)]));
    open(&mut game);
    let player = game.player();
    assert_eq!(game.world().dialogue_target(id(SPEAKER)), Some(player));
    assert_eq!(game.world().dialogue_target(player), None);
}

#[test]
fn failed_end_dispatch_preserves_a_saveable_response_cursor() {
    let mut game = game(plugin(vec![info(INFO, 0, &["Still playing"], true)]));
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    let line = game.dialogue_line().unwrap();
    game.vm = soe_papyrus::Vm::new();
    assert!(matches!(game.advance_dialogue(), Err(Error::Script(_))));
    assert_eq!(game.dialogue_line().unwrap(), line);
    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    assert_eq!(game.dialogue_line().unwrap(), line);
}

#[test]
fn old_world_save_starts_without_dialogue_state() {
    let mut game = game(plugin(vec![info(INFO, 0, &["New conversation"], false)]));
    let mut old = game.world().save_state();
    // CONTEXT: Legacy state ends before the empty dialogue and magic tables.
    old.truncate(old.len() - 37);
    old[4..6].copy_from_slice(&4u16.to_le_bytes());
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.world_mut().load_state(&old).unwrap();
    assert!(game.world().dialogue_session().is_none());
}

#[test]
fn reset_hours_uses_game_time_and_survives_closing_the_conversation() {
    let mut record = info(INFO, 0, &["Come back later"], false);
    record.subrecords[0].data[2..4].copy_from_slice(&65535u16.to_le_bytes());
    let mut game = game(plugin(vec![record]));
    let reset_hours = game
        .world()
        .dialogue_info(id(INFO))
        .unwrap()
        .data
        .unwrap()
        .reset_hours();
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.advance_dialogue().unwrap();
    game.close_dialogue();
    open(&mut game);
    assert!(game.dialogue_choices().unwrap().available.is_empty());
    game.world_mut().clock.game_days += f64::from(reset_hours) / 24.0 + 0.001;
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
}

#[test]
fn cyclic_shared_response_rejects_before_consuming_say_once() {
    let mut game = game(plugin(vec![
        info(INFO, 4, &[], false).sub(T(b"DNAM"), SHARED.to_le_bytes()),
        info(SHARED, 0, &[], false).sub(T(b"DNAM"), INFO.to_le_bytes()),
    ]));
    open(&mut game);
    assert!(matches!(
        game.choose_dialogue(id(TOPIC)),
        Err(Error::State("cyclic shared INFO"))
    ));
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
    assert!(game
        .world()
        .dialogue_session()
        .unwrap()
        .current_info
        .is_none());
}

#[test]
fn cell_and_location_keyword_conditions_follow_current_world_state() {
    let mut keyword_condition = condition(606, 0x600, 2.0, 0);
    keyword_condition[16..20].copy_from_slice(&0x601u32.to_le_bytes());
    let mut plugin = plugin(vec![info(INFO, 0, &["Here and now"], false)
        .sub(T(b"CTDA"), condition(67, 0x602, 1.0, 0))
        .sub(T(b"CTDA"), keyword_condition)]);
    plugin.push_top(Record::new(T(b"LCTN"), 0x600));
    plugin.push_top(Record::new(T(b"KYWD"), 0x601));
    plugin.push_top(Record::new(T(b"CELL"), 0x602));
    let mut game = game(plugin);
    open(&mut game);
    assert!(game.dialogue_choices().unwrap().available.is_empty());
    game.world_mut().reference(id(SPEAKER)).unwrap().cell = Some(id(0x602));
    assert!(game.dialogue_choices().unwrap().available.is_empty());
    game.world_mut()
        .location(id(0x600))
        .unwrap()
        .keyword_data
        .insert(id(0x601), 2.0);
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
}

#[test]
fn installed_dialogue_preserves_ownership_fragments_and_order() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA unset, skipping installed dialogue survey");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        let bytes = std::fs::read(data.join(name)).unwrap();
        order.add(name, Plugin::parse(&bytes).unwrap()).unwrap();
    }
    let mut game = Game::new(order);
    let mut infos = 0;
    let mut fragments = 0;
    let mut topics = std::collections::BTreeSet::new();
    {
        let world = game.world();
        for (id, record) in world.order().winners_of(T(b"INFO")) {
            if record.is_deleted() {
                continue;
            }
            world.dialogue_info(id).unwrap();
            let topic = world
                .dialogue_topic_of(id)
                .unwrap_or_else(|| panic!("no parent for {id:?}"));
            assert_eq!(world.tag_of(topic), Some(T(b"DIAL")), "{id:?}");
            let quest = world
                .owning_quest(id)
                .unwrap_or_else(|| panic!("no quest for {id:?}"));
            assert_eq!(world.tag_of(quest), Some(T(b"QUST")), "{id:?}");
            topics.insert(topic);
            infos += 1;
            if let Some(bytes) = record.find(T(b"VMAD")) {
                let mut vmad =
                    Vmad::parse(bytes, T(b"INFO")).unwrap_or_else(|e| panic!("{id:?}: {e}"));
                let encoded = vmad.write(T(b"INFO"));
                // The existing writer canonicalizes the script header to v5/format2.
                vmad.version = 5;
                vmad.object_format = 2;
                assert_eq!(Vmad::parse(&encoded, T(b"INFO")).unwrap(), vmad, "{id:?}");
                let mut scripts_only = vmad.clone();
                scripts_only.info_fragments = None;
                let tail = &encoded[scripts_only.write(T(b"INFO")).len()..];
                assert!(bytes.ends_with(tail), "fragment bytes differ for {id:?}");
                if let Some(tail) = vmad.info_fragments {
                    fragments +=
                        usize::from(tail.begin.is_some()) + usize::from(tail.end.is_some());
                }
            }
        }
        for topic in &topics {
            world
                .dialogue_infos(*topic)
                .unwrap_or_else(|e| panic!("{topic:?}: {e}"));
        }
    }
    assert!(infos > 10_000 && fragments > 1_000 && topics.len() > 1_000);
    eprintln!(
        "dialogue survey: {infos} INFO, {fragments} fragments, {} ordered topics",
        topics.len()
    );
    let scripts = game.load_scripts_from_data(&data);
    assert!(scripts.scripts_failed.is_empty());
    game.start();
    let speaker = game.world().form("HuldaRef").unwrap();
    let player = game.player();
    game.begin_dialogue(speaker, player).unwrap();
    let choices = game.dialogue_choices().unwrap();
    eprintln!(
        "Hulda choices: {} available, {} rejected",
        choices.available.len(),
        choices.rejected.len()
    );
    for choice in &choices.available {
        eprintln!("  offered {:?}: {:?}", choice.topic, choice.prompt);
    }
    let rumors = game
        .world()
        .form("WhiterunInnkeeperRumorsBranchTopic")
        .unwrap();
    assert!(choices
        .available
        .iter()
        .any(|choice| choice.topic == rumors));
    let before_traces = game.traces().len();
    let before_calls: u64 = game.vm.natives.calls.values().sum();
    let mut line = game.choose_dialogue(rumors).unwrap();
    let mut responses = 0;
    while let Some(response) = line {
        responses += 1;
        assert!(
            responses < 32,
            "conversation failed to reach topic selection"
        );
        eprintln!(
            "  response {:?}/{}: {:?}",
            response.info, response.response.number, response.response.text
        );
        let info = game.world().dialogue_info(response.info).unwrap();
        if let Some(bytes) = info.vmad {
            eprintln!(
                "  fragments: {:?}",
                Vmad::parse(&bytes, T(b"INFO")).unwrap().info_fragments
            );
        }
        line = game.advance_dialogue().unwrap();
    }
    assert!(responses > 0);
    let after_calls: u64 = game.vm.natives.calls.values().sum();
    eprintln!(
        "  completed {responses} responses, {} native calls",
        after_calls - before_calls
    );
    for trace in game.traces().iter().skip(before_traces) {
        eprintln!("  fragment trace: {trace}");
    }
    if game.world().dialogue_session().is_some() {
        let linked = game.dialogue_choices().unwrap();
        eprintln!("  follow-ups: {:?}", linked.available);
    }

    let mut rejected = std::collections::BTreeMap::new();
    for (_, reason) in choices.rejected {
        *rejected.entry(reason).or_insert(0usize) += 1;
    }
    for (reason, count) in rejected {
        eprintln!("  rejected {count}: {reason}");
    }
}
