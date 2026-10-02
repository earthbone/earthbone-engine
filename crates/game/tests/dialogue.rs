// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Conversations over authored DIAL groups, INFO conditions and VMAD fragments.
//! Assertions cover quest state changes and response timing, not just parsing.

use crate::common::installed;
use crate::common::legacy;

use soe_esm::{Entry, FileHeader, GlobalFormId, Group, GroupKind, LoadOrder, Plugin, Record, Tag};
use soe_game::dialogue::Error;
use soe_game::vmad::{InfoFragment, InfoFragments, ScriptEntry, Vmad};
use soe_game::Game;
use soe_papyrus::asm::{i, ident, none, s, FunctionBuilder, ScriptBuilder};
use soe_papyrus::Opcode::{CallMethod, IAdd, Return};
use soe_papyrus::Value;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const SPEAKER: u32 = 0x100;
const BASE: u32 = 0x500;
const QUEST: u32 = 0x800;
const OTHER_QUEST: u32 = 0x801;
const GREETING_QUEST: u32 = 0x802;
const TOPIC: u32 = 0x900;
const LINKED_TOPIC: u32 = 0x901;
const BLOCKING_TOPIC: u32 = 0x902;
const EXCLUSIVE_A_TOPIC: u32 = 0x903;
const EXCLUSIVE_B_TOPIC: u32 = 0x904;
const OUTSIDE_TOPIC: u32 = 0x905;
const GREETING_TOPIC: u32 = 0x906;
const INFO: u32 = 0xA00;
const SHARED: u32 = 0xA01;
const GREETING_INFO: u32 = 0xA02;
const ADDED_INFO: u32 = 0xA03;

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

fn enable_do_all_before_repeating(plugin: &mut Plugin) {
    let data = plugin
        .records
        .iter_mut()
        .find(|record| record.tag == T(b"DIAL") && record.form_id == TOPIC)
        .unwrap()
        .subrecords
        .iter_mut()
        .find(|subrecord| subrecord.tag == T(b"DATA"))
        .unwrap();
    data.data[0] = 1;
}

fn add_topic(plugin: &mut Plugin, topic: Record, infos: Vec<Record>, top_level: bool) {
    let raw = topic.form_id;
    let quest = topic.find(T(b"QNAM")).unwrap().to_vec();
    if top_level {
        add_branch(plugin, raw, quest, 1);
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

fn add_branch(plugin: &mut Plugin, topic: u32, quest: Vec<u8>, flags: u32) {
    plugin.push_top(
        Record::new(T(b"DLBR"), topic + 0x1000)
            .sub(T(b"QNAM"), quest)
            .sub(T(b"TNAM"), 0u32.to_le_bytes())
            .sub(T(b"DNAM"), flags.to_le_bytes())
            .sub(T(b"SNAM"), topic.to_le_bytes()),
    );
}

fn add_force_greet(plugin: &mut Plugin, priority: u8, condition: Option<Vec<u8>>) {
    plugin.push_top(quest(GREETING_QUEST, priority));
    let mut topic = topic(GREETING_TOPIC, GREETING_QUEST);
    topic
        .subrecords
        .iter_mut()
        .find(|subrecord| subrecord.tag == T(b"SNAM"))
        .unwrap()
        .data = b"PFGT".to_vec();
    add_branch(
        plugin,
        GREETING_TOPIC,
        GREETING_QUEST.to_le_bytes().to_vec(),
        1,
    );
    let mut greeting_info = info(GREETING_INFO, 0, &["Hello"], true);
    if let Some(condition) = condition {
        greeting_info = greeting_info.sub(T(b"CTDA"), condition);
    }
    let mut group = Group::new(GroupKind::Top(T(b"DIAL")));
    group.entries.push(Entry::Record(plugin.push_record(topic)));
    let mut children = Group::new(GroupKind::TopicChildren(GREETING_TOPIC));
    children
        .entries
        .push(Entry::Record(plugin.push_record(greeting_info)));
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

#[test]
fn scr_48_topic_add_offers_owned_topic_and_saves_the_known_set() {
    let mut plugin = plugin(vec![info(INFO, 0, &["Existing"], false)]);
    add_topic(
        &mut plugin,
        topic(LINKED_TOPIC, OTHER_QUEST),
        vec![info(ADDED_INFO, 0, &["Added topic response"], false)],
        false,
    );
    let mut game = game(plugin);
    let player = game.player();
    game.start_quest(id(QUEST));
    game.start_quest(id(OTHER_QUEST));
    game.begin_dialogue(id(SPEAKER), player).unwrap();

    assert!(game.vm.natives.has_method("Topic", "Add"));
    assert!(!game
        .dialogue_choices()
        .unwrap()
        .available
        .iter()
        .any(|choice| choice.topic == id(LINKED_TOPIC)));
    scr_48_call_topic_add(&mut game, LINKED_TOPIC);
    let choices = game.dialogue_choices().unwrap();
    assert!(
        choices
            .available
            .iter()
            .any(|choice| choice.topic == id(LINKED_TOPIC)),
        "choices: {:?}, rejected: {:?}",
        choices.available,
        choices.rejected
    );
    let line = game.choose_dialogue(id(LINKED_TOPIC)).unwrap().unwrap();
    assert_eq!(line.info, id(ADDED_INFO));
    assert_eq!(line.text.as_deref(), Some("Added topic response"));

    game.close_dialogue();
    let saved = game.world().save_state();
    game.world_mut().load_state(&saved).unwrap();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert!(game
        .dialogue_choices()
        .unwrap()
        .available
        .iter()
        .any(|choice| choice.topic == id(LINKED_TOPIC)));
}

#[test]
fn scr_48_topic_add_reports_invalid_forms_without_adding_them() {
    let invalid_topic = 0x907;
    let mut plugin = plugin(vec![info(INFO, 0, &["Existing"], false)]);
    plugin.push_top(
        Record::new(T(b"DIAL"), invalid_topic)
            .sub(T(b"DATA"), [0, 0, 0, 0])
            .sub(T(b"SNAM"), *b"CUST"),
    );
    let mut game = game(plugin);
    open(&mut game);
    assert!(matches!(
        game.world_mut().add_dialogue_topic(id(0xDEAD)),
        Err(Error::Record(form)) if form == id(0xDEAD)
    ));
    assert!(matches!(
        game.world_mut().add_dialogue_topic(id(BASE)),
        Err(Error::Data(_))
    ));
    scr_48_call_topic_add(&mut game, invalid_topic);
    assert!(game
        .traces()
        .iter()
        .any(|trace| trace.contains("Topic.Add")));
    assert!(game
        .dialogue_choices()
        .unwrap()
        .available
        .iter()
        .all(|choice| choice.topic == id(TOPIC)));
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

fn scr_48_call_topic_add(game: &mut Game, topic: u32) {
    let script = ScriptBuilder::new("Scr48TopicCall", "Topic")
        .function(
            FunctionBuilder::new("Run", "None")
                .global()
                .param("topic", "Topic")
                .local("unused", "None")
                .ins(CallMethod, vec![s("Add"), ident("topic"), ident("unused")])
                .ins(Return, vec![none()])
                .build(),
        )
        .bytes();
    game.load_script(&script).unwrap();
    let object = game.object_of_form(id(topic));
    let thread = game
        .vm
        .call_global("Scr48TopicCall", "Run", &[Value::Object(object)])
        .unwrap();
    assert!(game.run_until_idle(64));
    assert_eq!(game.vm.thread_result(thread), Some(Value::None));
}

fn open(game: &mut Game) {
    let player = game.player();
    game.start_quest(id(QUEST));
    game.begin_dialogue(id(SPEAKER), player).unwrap();
}

fn counter(game: &mut Game, variable: &str) -> i32 {
    counter_on(game, INFO, variable)
}

fn counter_on(game: &mut Game, form: u32, variable: &str) -> i32 {
    let object = game.object_of_form(id(form));
    game.vm
        .get_var(object, "TopicFragments", variable)
        .unwrap()
        .as_int()
}

#[test]
fn scr_62_topic_info_gets_owning_quest_from_its_dialogue_group() {
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

fn alias_dialogue() -> Game {
    let mut base = plugin(vec![info(INFO, 0, &["Base response"], false)]);
    base.push_top(
        Record::new(T(b"ACHR"), SPEAKER + 1)
            .sub(T(b"NAME"), BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Dialogue.esm".into()],
        ..FileHeader::default()
    });
    for (owner, reference) in [(QUEST, SPEAKER), (OTHER_QUEST, SPEAKER + 1)] {
        patch.push_top(
            quest(owner, 50)
                .sub(T(b"ALST"), 0u32.to_le_bytes())
                .zsub(T(b"ALID"), "Speaker")
                .sub(T(b"FNAM"), 2u32.to_le_bytes())
                .sub(T(b"ALFR"), reference.to_le_bytes())
                .sub(T(b"ALED"), []),
        );
    }
    add_topic(
        &mut patch,
        topic(TOPIC, QUEST),
        vec![info(INFO, 4, &["Alias response"], true).sub(T(b"CTDA"), condition(566, 0, 1.0, 0))],
        false,
    );
    let mut order = LoadOrder::new();
    order.add("Dialogue.esm", base).unwrap();
    order.add("AliasOverride.esp", patch).unwrap();
    game_with_order(order)
}

#[test]
fn filled_actor_alias_controls_dialogue_for_same_base_references() {
    let mut game = alias_dialogue();
    open(&mut game);
    assert_eq!(game.world_mut().alias_ref(id(QUEST), 0), Some(id(SPEAKER)));
    assert_eq!(
        game.world_mut().alias_ref(id(OTHER_QUEST), 0),
        Some(id(SPEAKER + 1))
    );
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
    game.close_dialogue();
    let player = game.player();
    game.begin_dialogue(id(SPEAKER + 1), player).unwrap();
    assert!(game.dialogue_choices().unwrap().available.is_empty());
    assert!(game
        .world_mut()
        .force_alias(id(QUEST), 0, Some(id(SPEAKER + 1))));
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
    let line = game.choose_dialogue(id(TOPIC)).unwrap().unwrap();
    assert_eq!(line.speaker, id(SPEAKER + 1));
    assert_eq!(line.text.as_deref(), Some("Alias response"));
}

#[test]
fn alias_clear_invalidates_offered_choice_before_fragments() {
    let mut game = alias_dialogue();
    open(&mut game);
    assert_eq!(game.dialogue_choices().unwrap().available.len(), 1);
    assert!(game.world_mut().force_alias(id(QUEST), 0, None));
    assert!(game.dialogue_choices().unwrap().available.is_empty());
    assert!(matches!(
        game.choose_dialogue(id(TOPIC)),
        Err(Error::State(_))
    ));
    assert_eq!(counter(&mut game, "begins"), 0);
    assert!(game.dialogue_line().unwrap().is_none());
    assert!(game
        .world_mut()
        .force_alias(id(QUEST), 0, Some(id(SPEAKER))));
    assert!(game.choose_dialogue(id(TOPIC)).unwrap().is_some());
    assert_eq!(counter(&mut game, "begins"), 1);
}

#[test]
fn dialogue_alias_assignment_survives_save_and_reload() {
    let mut original = alias_dialogue();
    open(&mut original);
    assert!(original.world_mut().force_alias(id(QUEST), 0, None));
    let bytes = original.save_state().unwrap();
    let mut restored = alias_dialogue();
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.world_mut().alias_ref(id(QUEST), 0), None);
    assert!(restored.dialogue_choices().unwrap().available.is_empty());
    assert!(restored
        .world_mut()
        .force_alias(id(QUEST), 0, Some(id(SPEAKER))));
    let bytes = restored.save_state().unwrap();
    let mut filled = alias_dialogue();
    filled.load_state(&bytes).unwrap();
    assert_eq!(
        filled.world_mut().alias_ref(id(QUEST), 0),
        Some(id(SPEAKER))
    );
    assert_eq!(filled.dialogue_choices().unwrap().available.len(), 1);
    let line = filled.choose_dialogue(id(TOPIC)).unwrap().unwrap();
    assert_eq!(line.speaker, id(SPEAKER));
    assert_eq!(counter(&mut filled, "begins"), 1);
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
fn dlg_15_invisible_continue_chains_to_eligible_link_without_menu_or_replay() {
    let first = info(INFO, 1 << 6, &["Continue"], true).sub(T(b"TCLT"), LINKED_TOPIC.to_le_bytes());
    let ineligible =
        info(INFO + 1, 0, &["Hidden"], true).sub(T(b"CTDA"), condition(72, BASE, 0.0, 0));
    let eligible =
        info(INFO + 2, 0, &["Follow-up"], true).sub(T(b"CTDA"), condition(72, BASE, 1.0, 0));
    let mut plugin = plugin(vec![first]);
    add_topic(
        &mut plugin,
        topic(LINKED_TOPIC, QUEST),
        vec![ineligible, eligible],
        false,
    );
    let mut game = game(plugin);
    open(&mut game);

    let line = game.choose_dialogue(id(TOPIC)).unwrap().unwrap();
    assert_eq!(line.info, id(INFO));
    let continuation = game.advance_dialogue().unwrap().unwrap();
    assert_eq!(continuation.info, id(INFO + 2));
    assert_eq!(continuation.text.as_deref(), Some("Follow-up"));
    assert!(matches!(
        game.dialogue_choices().unwrap_err(),
        Error::State("response still playing")
    ));

    let fragment_count = |game: &mut Game, info: u32, variable: &str| {
        let object = game.object_of_form(id(info));
        game.vm
            .get_var(object, "TopicFragments", variable)
            .unwrap()
            .as_int()
    };
    assert_eq!(fragment_count(&mut game, INFO, "begins"), 1);
    assert_eq!(fragment_count(&mut game, INFO, "ends"), 1);
    assert_eq!(fragment_count(&mut game, INFO + 2, "begins"), 1);
    assert_eq!(fragment_count(&mut game, INFO + 2, "ends"), 0);

    assert!(game.advance_dialogue().unwrap().is_none());
    assert_eq!(fragment_count(&mut game, INFO, "begins"), 1);
    assert_eq!(fragment_count(&mut game, INFO, "ends"), 1);
    assert_eq!(fragment_count(&mut game, INFO + 2, "begins"), 1);
    assert_eq!(fragment_count(&mut game, INFO + 2, "ends"), 1);
}

fn localized_table(text: &str, prefixed: bool) -> Vec<u8> {
    let mut data = Vec::new();
    if prefixed {
        data.extend_from_slice(&((text.len() + 1) as u32).to_le_bytes());
    }
    data.extend_from_slice(text.as_bytes());
    data.push(0);
    let mut bytes = 1u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend(data);
    bytes
}

#[test]
fn localized_choice_and_shared_response_keep_distinct_winning_owners() {
    let root = std::env::temp_dir().join(format!("soe-dialogue-localized-{}", std::process::id()));
    std::fs::create_dir_all(root.join("Strings")).unwrap();
    for (stem, prompt, response) in [
        ("Dialogue", "Wrong base prompt", "Wrong base response"),
        (
            "Override",
            "Winning topic prompt",
            "Winning shared response",
        ),
    ] {
        for (extension, text, prefixed) in
            [("strings", prompt, false), ("ilstrings", response, true)]
        {
            std::fs::write(
                root.join(format!("Strings/{stem}_english.{extension}")),
                localized_table(text, prefixed),
            )
            .unwrap();
        }
    }
    let mut base = plugin(vec![
        info(INFO, 0, &[], true).sub(T(b"DNAM"), SHARED.to_le_bytes())
    ]);
    base.header.flags |= soe_esm::plugin::flags::LOCALIZED;
    add_topic(
        &mut base,
        topic(LINKED_TOPIC, OTHER_QUEST),
        vec![info(SHARED, 0, &["Unused"], false)],
        false,
    );
    let mut patch = Plugin::new(FileHeader {
        flags: soe_esm::plugin::flags::LOCALIZED,
        masters: vec!["Dialogue.esm".into()],
        ..FileHeader::default()
    });
    let localized_topic = |form: u32, quest: u32| {
        Record::new(T(b"DIAL"), form)
            .sub(T(b"FULL"), 1u32.to_le_bytes())
            .sub(T(b"QNAM"), quest.to_le_bytes())
            .sub(T(b"BNAM"), (form + 0x1000).to_le_bytes())
            .sub(T(b"DATA"), [0, 0, 0, 0])
            .sub(T(b"SNAM"), *b"CUST")
            .sub(T(b"PNAM"), 50.0f32.to_le_bytes())
    };
    add_topic(&mut patch, localized_topic(TOPIC, QUEST), vec![], false);
    let mut response = vec![0; 24];
    response[12] = 1;
    add_topic(
        &mut patch,
        localized_topic(LINKED_TOPIC, OTHER_QUEST),
        vec![info(SHARED, 0, &[], false)
            .sub(T(b"TRDT"), response)
            .sub(T(b"NAM1"), 1u32.to_le_bytes())],
        false,
    );
    let mut order = LoadOrder::new();
    order.add("Dialogue.esm", base).unwrap();
    order.add("Override.esp", patch).unwrap();
    let mut game = game_with_order(order);
    let report = game.load_scripts_from_data(&root);
    assert!(report.scripts_failed.is_empty());
    open(&mut game);
    assert_eq!(
        game.dialogue_choices().unwrap().available[0]
            .prompt
            .as_deref(),
        Some("Winning topic prompt")
    );
    let line = game.choose_dialogue(id(TOPIC)).unwrap().unwrap();
    assert_eq!(line.text.as_deref(), Some("Winning shared response"));
    assert_eq!(line.info, id(INFO));
    assert_eq!(line.response_info, id(SHARED));
    assert_eq!(counter(&mut game, "begins"), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn mixed_case_interface_archive_errors_reach_load_report() {
    let root = std::env::temp_dir().join(format!("soe-dialogue-archive-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Dialogue - iNtErFaCe.BSA"), b"invalid archive").unwrap();
    let mut game = game(plugin(vec![]));
    let report = game.load_scripts_from_data(&root);
    assert_eq!(report.scripts_failed.len(), 1);
    assert!(report.scripts_failed[0]
        .0
        .ends_with("Dialogue - iNtErFaCe.BSA"));
    std::fs::remove_dir_all(root).unwrap();
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
fn dlg_21_higher_priority_eligible_greeting_interrupts_once_and_finishes_callbacks() {
    let mut plugin = plugin(vec![info(INFO, 0, &["Current line"], true)]);
    add_force_greet(&mut plugin, 80, None);
    let mut game = game(plugin);
    open(&mut game);
    game.start_quest(id(GREETING_QUEST));
    game.world_mut().drive_actor(id(SPEAKER));
    game.choose_dialogue(id(TOPIC)).unwrap();

    game.tick(0.0);
    let (speaker, topic, current_info) = {
        let world = game.world();
        let session = world.dialogue_session().unwrap();
        (session.speaker, session.topic, session.current_info)
    };
    assert_eq!(speaker, id(SPEAKER));
    assert_eq!(topic, Some(id(GREETING_TOPIC)));
    assert_eq!(current_info, Some(id(GREETING_INFO)));
    assert_eq!(counter(&mut game, "ends"), 1);
    assert_eq!(
        game.world_mut()
            .quest_state(id(GREETING_QUEST))
            .unwrap()
            .highest,
        10
    );
    assert_eq!(counter_on(&mut game, GREETING_INFO, "begins"), 1);

    game.tick(0.0);
    assert_eq!(counter(&mut game, "ends"), 1);
    assert_eq!(counter_on(&mut game, GREETING_INFO, "begins"), 1);
    assert_eq!(
        game.world_mut()
            .quest_state(id(GREETING_QUEST))
            .unwrap()
            .highest,
        10,
        "the greeting edge must not replay its begin fragment"
    );
}

#[test]
fn dlg_21_lower_priority_or_ineligible_greeting_keeps_current_conversation() {
    for (priority, greeting_condition) in [
        (40, None),
        (50, None),
        (80, Some(condition(72, BASE + 1, 1.0, 0))),
    ] {
        let mut plugin = plugin(vec![info(INFO, 0, &["Current line"], true)]);
        add_force_greet(&mut plugin, priority, greeting_condition);
        let mut game = game(plugin);
        open(&mut game);
        game.start_quest(id(GREETING_QUEST));
        game.world_mut().drive_actor(id(SPEAKER));
        game.choose_dialogue(id(TOPIC)).unwrap();
        game.tick(0.0);

        {
            let world = game.world();
            let session = world.dialogue_session().unwrap();
            assert_eq!(session.topic, Some(id(TOPIC)));
            assert_eq!(session.current_info, Some(id(INFO)));
        }
        assert_eq!(counter(&mut game, "ends"), 0);
        assert_eq!(
            game.world_mut()
                .quest_state(id(GREETING_QUEST))
                .unwrap()
                .highest,
            0
        );
    }
}

#[test]
fn dlg_06_leaving_range_ends_conversation_and_fires_recorded_end_once() {
    for response_index in [0, 1] {
        let mut game = game(plugin(vec![info(INFO, 0, &["First", "Second"], true)]));
        open(&mut game);
        game.choose_dialogue(id(TOPIC)).unwrap();
        if response_index == 1 {
            game.advance_dialogue().unwrap();
        }
        let player = game.player();
        game.world_mut().reference(player).unwrap().position = [300.0, 400.0, 0.0];
        game.tick(0.0);
        assert!(
            game.world().dialogue_session().is_some(),
            "the exact boundary is in range"
        );
        assert_eq!(
            counter(&mut game, "ends"),
            0,
            "in-range ticks must not end a line"
        );
        game.world_mut().reference(player).unwrap().position[2] = 1.0;
        game.tick(0.0);
        assert!(
            game.world().dialogue_session().is_none(),
            "leaving range closes the session"
        );
        assert_eq!(
            game.world().dialogue_target(id(SPEAKER)),
            None,
            "the actor loses its target"
        );
        assert!(
            game.dialogue_line().unwrap().is_none(),
            "no response remains after walking away"
        );
        assert_eq!(
            game.world_mut().quest_state(id(QUEST)).unwrap().highest,
            20,
            "the recorded end fragment runs"
        );
        game.tick(0.0);
        assert!(
            !game.update_dialogue_range().unwrap(),
            "closure is idempotent"
        );
        assert_eq!(
            counter(&mut game, "begins"),
            1,
            "walking away must not restart the INFO"
        );
        assert_eq!(
            counter(&mut game, "ends"),
            1,
            "the end fragment runs exactly once"
        );
    }
}

#[test]
fn dlg_06_between_responses_closes_without_replaying_end() {
    for completed in [false, true] {
        let mut game = game(plugin(vec![info(INFO, 0, &["Line"], true)]));
        open(&mut game);
        if completed {
            game.choose_dialogue(id(TOPIC)).unwrap();
            game.advance_dialogue().unwrap();
        }
        game.world_mut().reference(id(SPEAKER)).unwrap().position = [501.0, 0.0, 0.0];
        game.tick(0.0);
        assert!(
            game.world().dialogue_session().is_none(),
            "an idle conversation closes too"
        );
        assert_eq!(
            counter(&mut game, "ends"),
            i32::from(completed),
            "only a playing INFO needs an end fragment"
        );
    }
}

#[test]
fn dlg_06_record_and_runtime_distance_overrides_control_range() {
    let mut plugin = plugin(vec![info(INFO, 0, &["Line"], true)]);
    plugin.push_top(
        Record::new(T(b"GMST"), 0xB00)
            .zsub(T(b"EDID"), "fAIInDialogueModeWithPlayerDistance")
            .sub(T(b"DATA"), 1000.0f32.to_le_bytes()),
    );
    let mut game = game(plugin);
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.world_mut().reference(id(SPEAKER)).unwrap().position = [750.0, 0.0, 0.0];
    assert!(
        !game.update_dialogue_range().unwrap(),
        "the record overrides the default"
    );
    game.world_mut().settings.insert(
        "faiindialoguemodewithplayerdistance".into(),
        Value::Float(600.0),
    );
    game.tick(0.0);
    assert!(
        game.world().dialogue_session().is_none(),
        "runtime setting overrides the record"
    );
    assert_eq!(
        counter(&mut game, "ends"),
        1,
        "the override exit dispatches the end fragment"
    );
}

#[test]
fn dlg_06_invalid_range_or_position_preserves_the_response() {
    let mut game = game(plugin(vec![info(INFO, 0, &["Line"], true)]));
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    for invalid in [
        Value::Float(-1.0),
        Value::Float(f32::NAN),
        Value::Float(f32::INFINITY),
        Value::string("bad"),
    ] {
        game.world_mut()
            .settings
            .insert("faiindialoguemodewithplayerdistance".into(), invalid);
        assert!(
            matches!(game.update_dialogue_range().unwrap_err(), Error::State(_)),
            "invalid settings must be reported"
        );
    }
    game.world_mut().settings.clear();
    game.world_mut().reference(id(SPEAKER)).unwrap().position[0] = f32::NAN;
    assert!(
        matches!(game.update_dialogue_range().unwrap_err(), Error::State(_)),
        "invalid positions must be reported"
    );
    assert_eq!(
        game.world().dialogue_session().unwrap().current_info,
        Some(id(INFO)),
        "errors retain the active INFO"
    );
    assert_eq!(
        counter(&mut game, "ends"),
        0,
        "errors must not consume its fragment"
    );
}

#[test]
fn dlg_06_menu_pause_defers_walk_away_until_simulation_resumes() {
    let mut game = game(plugin(vec![info(INFO, 0, &["Line"], true)]));
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.world_mut().reference(id(SPEAKER)).unwrap().position = [501.0, 0.0, 0.0];
    game.tick_menu_mode(0.0, true);
    assert!(
        game.world().dialogue_session().is_some(),
        "menu pause preserves the session"
    );
    assert_eq!(
        counter(&mut game, "ends"),
        0,
        "menu pause defers the fragment"
    );
    game.tick(0.0);
    assert!(
        game.world().dialogue_session().is_none(),
        "simulation resume applies the exit"
    );
    assert_eq!(counter(&mut game, "ends"), 1, "resume dispatches once");
}

#[test]
fn dlg_06_shared_response_uses_selected_info_end_fragment() {
    let mut plugin = plugin(vec![
        info(INFO, 0, &[], true).sub(T(b"DNAM"), SHARED.to_le_bytes())
    ]);
    add_topic(
        &mut plugin,
        topic(LINKED_TOPIC, OTHER_QUEST),
        vec![info(SHARED, 0, &["Shared"], true)],
        false,
    );
    let mut game = game(plugin);
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.world_mut().reference(id(SPEAKER)).unwrap().position = [0.0, 0.0, 501.0];
    game.tick(0.0);
    assert_eq!(
        counter(&mut game, "ends"),
        1,
        "the selected INFO owns the end fragment"
    );
    let shared = game.object_of_form(id(SHARED));
    assert_eq!(
        game.vm.get_var(shared, "TopicFragments", "ends"),
        Some(Value::Int(0)),
        "shared text must not transfer fragment ownership"
    );
    assert_eq!(
        game.world_mut()
            .quest_state(id(OTHER_QUEST))
            .unwrap()
            .highest,
        0,
        "the shared response quest is unchanged"
    );
}

#[test]
fn dlg_06_separate_interiors_do_not_share_dialogue_range() {
    let mut plugin = plugin(vec![info(INFO, 0, &["Line"], true)]);
    for cell in [0xC00, 0xC01] {
        plugin.push_top(Record::new(T(b"CELL"), cell).sub(T(b"DATA"), [1, 0]));
    }
    let mut game = game(plugin);
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    let player = game.player();
    game.world_mut().reference(player).unwrap().cell = Some(id(0xC00));
    game.world_mut().reference(id(SPEAKER)).unwrap().cell = Some(id(0xC01));
    game.tick(0.0);
    assert!(
        game.world().dialogue_session().is_none(),
        "matching interior coordinates are not the same space"
    );
    assert_eq!(
        counter(&mut game, "ends"),
        1,
        "the spatial exit dispatches its end fragment"
    );
}

#[test]
fn dlg_06_installed_distance_record_drives_walk_away() {
    let Some(_data) = installed::data_dir() else {
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let world = soe_game::World::from_shared_order(order);
    let setting = world.form("fAIInDialogueModeWithPlayerDistance").unwrap();
    assert_eq!(
        world.game_setting("fAIInDialogueModeWithPlayerDistance"),
        Some(Value::Float(500.0)),
        "the installed setting establishes the default"
    );
    let mut record = world.record(setting).unwrap().clone();
    record.form_id = 0xB00;
    let mut plugin = plugin(vec![info(INFO, 0, &["Line"], true)]);
    plugin.push_top(record);
    let mut game = game(plugin);
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.world_mut().reference(id(SPEAKER)).unwrap().position = [501.0, 0.0, 0.0];
    game.tick(0.0);
    assert!(
        game.world().dialogue_session().is_none(),
        "the authored range controls the exit"
    );
    assert_eq!(
        counter(&mut game, "ends"),
        1,
        "the recorded fragment runs after leaving the authored range"
    );
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
fn do_all_before_repeating_exhausts_eligible_infos_and_survives_a_save() {
    let fixture = || {
        let mut plugin = plugin(vec![
            info(INFO, 2, &["A"], false),
            info(INFO + 1, 2, &["B"], false),
            info(INFO + 2, 2 | 32, &["C"], false),
        ]);
        enable_do_all_before_repeating(&mut plugin);
        plugin
    };
    let select = |game: &mut Game| {
        let info = game.choose_dialogue(id(TOPIC)).unwrap().unwrap().info;
        game.advance_dialogue().unwrap();
        info
    };

    let mut original = game(fixture());
    open(&mut original);
    let first = select(&mut original);
    let second = select(&mut original);
    assert_ne!(first, second);
    let bytes = original.save_state().unwrap();

    let mut restored = game(fixture());
    restored.load_state(&bytes).unwrap();
    assert_eq!(restored.world().state_hash(), original.world().state_hash());
    let third = select(&mut restored);
    assert!(![first, second].contains(&third));
    let repeated = select(&mut restored);
    assert!([first, second, third].contains(&repeated));
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
    let fixture = || game(plugin(vec![info(INFO, 0, &["New conversation"], false)]));
    let mut game = fixture();
    let mut old = game.world().save_state();
    legacy::truncate_to_version(&mut old, 4, |candidate| {
        self::game(plugin(vec![info(INFO, 0, &["New conversation"], false)]))
            .world_mut()
            .load_state(candidate)
            .is_ok()
    });
    open(&mut game);
    game.choose_dialogue(id(TOPIC)).unwrap();
    game.world_mut().load_state(&old).unwrap();
    assert!(game.world().dialogue_session().is_none());
}

#[test]
fn blocking_branch_waits_for_its_quest_and_info_conditions() {
    let blocked = info(INFO + 1, 0, &["Blocked until stage 10"], false)
        .sub(T(b"CTDA"), condition(58, OTHER_QUEST, 10.0, 0));
    let mut plugin = plugin(vec![info(INFO, 0, &["Normal"], false)]);
    add_topic(
        &mut plugin,
        topic(BLOCKING_TOPIC, OTHER_QUEST),
        vec![blocked],
        false,
    );
    add_branch(
        &mut plugin,
        BLOCKING_TOPIC,
        OTHER_QUEST.to_le_bytes().to_vec(),
        2,
    );
    let mut game = game(plugin);
    game.start_quest(id(QUEST));
    game.start_quest(id(OTHER_QUEST));
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert_eq!(
        game.dialogue_choices()
            .unwrap()
            .available
            .iter()
            .map(|choice| choice.topic)
            .collect::<Vec<_>>(),
        vec![id(TOPIC)]
    );
    assert!(game.set_stage(id(OTHER_QUEST), 10));
    assert_eq!(
        game.dialogue_choices()
            .unwrap()
            .available
            .iter()
            .map(|choice| choice.topic)
            .collect::<Vec<_>>(),
        vec![id(BLOCKING_TOPIC)]
    );
}

#[test]
fn highest_priority_eligible_blocking_branch_wins_deterministically() {
    let mut plugin = plugin(vec![info(INFO, 0, &["Normal"], false)]);
    add_topic(
        &mut plugin,
        topic(BLOCKING_TOPIC, QUEST),
        vec![info(INFO + 1, 0, &["Lower blocker"], false)],
        false,
    );
    add_topic(
        &mut plugin,
        topic(LINKED_TOPIC, OTHER_QUEST),
        vec![info(SHARED, 0, &["Higher blocker"], false)],
        false,
    );
    add_branch(&mut plugin, BLOCKING_TOPIC, QUEST.to_le_bytes().to_vec(), 2);
    add_branch(
        &mut plugin,
        LINKED_TOPIC,
        OTHER_QUEST.to_le_bytes().to_vec(),
        2,
    );
    let mut game = game(plugin);
    game.start_quest(id(QUEST));
    game.start_quest(id(OTHER_QUEST));
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    let choices = game.dialogue_choices().unwrap();
    assert_eq!(choices.available.len(), 1);
    assert_eq!(choices.available[0].topic, id(LINKED_TOPIC));
}

#[test]
fn unknown_dialogue_branch_flags_are_rejected() {
    let mut plugin = plugin(vec![info(INFO, 0, &["Normal"], false)]);
    add_topic(
        &mut plugin,
        topic(BLOCKING_TOPIC, QUEST),
        vec![info(INFO + 1, 0, &["Unknown flags"], false)],
        false,
    );
    add_branch(&mut plugin, BLOCKING_TOPIC, QUEST.to_le_bytes().to_vec(), 8);
    let mut game = game(plugin);
    open(&mut game);
    let choices = game.dialogue_choices().unwrap();
    assert_eq!(choices.available[0].topic, id(TOPIC));
    assert!(choices.rejected.iter().any(|(form_id, reason)| {
        *form_id == id(BLOCKING_TOPIC + 0x1000) && reason.contains("unknown")
    }));
}

fn exclusive_game(first_links: Option<u32>, outside_is_branch: bool) -> Game {
    let mut first = info(INFO + 1, 0, &["Exclusive A"], false);
    if let Some(link) = first_links {
        first = first.sub(T(b"TCLT"), link.to_le_bytes());
    }
    let mut plugin = plugin(vec![info(INFO, 0, &["Normal"], false)]);
    add_topic(
        &mut plugin,
        topic(EXCLUSIVE_A_TOPIC, QUEST),
        vec![first],
        false,
    );
    add_topic(
        &mut plugin,
        topic(EXCLUSIVE_B_TOPIC, QUEST),
        vec![info(INFO + 2, 0, &["Exclusive B"], false)],
        false,
    );
    add_topic(
        &mut plugin,
        topic(OUTSIDE_TOPIC, QUEST),
        vec![info(INFO + 3, 0, &["Outside"], false)],
        false,
    );
    add_branch(
        &mut plugin,
        EXCLUSIVE_A_TOPIC,
        QUEST.to_le_bytes().to_vec(),
        5,
    );
    add_branch(
        &mut plugin,
        EXCLUSIVE_B_TOPIC,
        QUEST.to_le_bytes().to_vec(),
        5,
    );
    if outside_is_branch {
        add_branch(&mut plugin, OUTSIDE_TOPIC, QUEST.to_le_bytes().to_vec(), 1);
    }
    game(plugin)
}

#[test]
fn exclusive_branch_survives_closure_and_an_empty_link_list() {
    let mut game = exclusive_game(None, true);
    open(&mut game);
    game.choose_dialogue(id(EXCLUSIVE_A_TOPIC)).unwrap();
    game.advance_dialogue().unwrap();
    game.close_dialogue();
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert_eq!(
        game.dialogue_choices()
            .unwrap()
            .available
            .iter()
            .map(|choice| choice.topic)
            .collect::<Vec<_>>(),
        vec![id(EXCLUSIVE_A_TOPIC)]
    );
}

#[test]
fn exclusive_branch_updates_for_another_exclusive_and_releases_for_a_normal_one() {
    let mut game = exclusive_game(Some(EXCLUSIVE_B_TOPIC), true);
    open(&mut game);
    game.choose_dialogue(id(EXCLUSIVE_A_TOPIC)).unwrap();
    game.advance_dialogue().unwrap();
    game.choose_dialogue(id(EXCLUSIVE_B_TOPIC)).unwrap();
    game.advance_dialogue().unwrap();
    game.close_dialogue();
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert_eq!(
        game.dialogue_choices()
            .unwrap()
            .available
            .iter()
            .map(|choice| choice.topic)
            .collect::<Vec<_>>(),
        vec![id(EXCLUSIVE_B_TOPIC)]
    );

    game.close_dialogue();
    let mut release = exclusive_game(Some(OUTSIDE_TOPIC), true);
    open(&mut release);
    release.choose_dialogue(id(EXCLUSIVE_A_TOPIC)).unwrap();
    release.advance_dialogue().unwrap();
    release.choose_dialogue(id(OUTSIDE_TOPIC)).unwrap();
    release.advance_dialogue().unwrap();
    release.close_dialogue();
    let player = release.player();
    release.begin_dialogue(id(SPEAKER), player).unwrap();
    let topics: Vec<_> = release
        .dialogue_choices()
        .unwrap()
        .available
        .iter()
        .map(|choice| choice.topic)
        .collect();
    assert!(topics.contains(&id(TOPIC)));
    assert!(topics.contains(&id(EXCLUSIVE_A_TOPIC)));
}

#[test]
fn unbranched_link_does_not_release_an_exclusive_branch() {
    let mut game = exclusive_game(Some(OUTSIDE_TOPIC), false);
    open(&mut game);
    game.choose_dialogue(id(EXCLUSIVE_A_TOPIC)).unwrap();
    game.advance_dialogue().unwrap();
    game.choose_dialogue(id(OUTSIDE_TOPIC)).unwrap();
    game.advance_dialogue().unwrap();
    game.close_dialogue();
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert_eq!(
        game.dialogue_choices().unwrap().available[0].topic,
        id(EXCLUSIVE_A_TOPIC)
    );
}

#[test]
fn active_exclusive_branch_yields_to_an_eligible_higher_priority_blocker() {
    let blocked = info(INFO + 2, 0, &["Higher blocker"], false)
        .sub(T(b"CTDA"), condition(58, OTHER_QUEST, 10.0, 0));
    let mut plugin = plugin(vec![info(INFO, 0, &["Normal"], false)]);
    add_topic(
        &mut plugin,
        topic(EXCLUSIVE_A_TOPIC, QUEST),
        vec![info(INFO + 1, 0, &["Exclusive"], false)],
        false,
    );
    add_topic(
        &mut plugin,
        topic(BLOCKING_TOPIC, OTHER_QUEST),
        vec![blocked],
        false,
    );
    add_branch(
        &mut plugin,
        EXCLUSIVE_A_TOPIC,
        QUEST.to_le_bytes().to_vec(),
        5,
    );
    add_branch(
        &mut plugin,
        BLOCKING_TOPIC,
        OTHER_QUEST.to_le_bytes().to_vec(),
        2,
    );
    let mut game = game(plugin);
    game.start_quest(id(QUEST));
    game.start_quest(id(OTHER_QUEST));
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    game.choose_dialogue(id(EXCLUSIVE_A_TOPIC)).unwrap();
    game.advance_dialogue().unwrap();
    game.close_dialogue();
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert_eq!(
        game.dialogue_choices().unwrap().available[0].topic,
        id(EXCLUSIVE_A_TOPIC),
        "an ineligible blocker leaves the active exclusive branch in control"
    );
    assert!(game.set_stage(id(OTHER_QUEST), 10));
    assert_eq!(
        game.dialogue_choices().unwrap().available[0].topic,
        id(BLOCKING_TOPIC)
    );
}

#[test]
fn ineligible_active_exclusive_branch_yields_without_clearing_its_state() {
    let gated = info(INFO + 1, 0, &["Exclusive at stage 10"], false)
        .sub(T(b"CTDA"), condition(58, QUEST, 10.0, 0));
    let mut plugin = plugin(vec![info(INFO, 0, &["Normal"], false)]);
    add_topic(
        &mut plugin,
        topic(EXCLUSIVE_A_TOPIC, QUEST),
        vec![gated],
        false,
    );
    add_branch(
        &mut plugin,
        EXCLUSIVE_A_TOPIC,
        QUEST.to_le_bytes().to_vec(),
        5,
    );
    let mut game = game(plugin);
    game.start_quest(id(QUEST));
    game.world_mut()
        .actor(id(SPEAKER))
        .unwrap()
        .exclusive_branch = Some(id(EXCLUSIVE_A_TOPIC + 0x1000));
    let player = game.player();
    game.begin_dialogue(id(SPEAKER), player).unwrap();
    assert_eq!(
        game.dialogue_choices().unwrap().available[0].topic,
        id(TOPIC)
    );
    assert_eq!(
        game.world()
            .actor_ref(id(SPEAKER))
            .unwrap()
            .exclusive_branch,
        Some(id(EXCLUSIVE_A_TOPIC + 0x1000))
    );
    assert!(game.set_stage(id(QUEST), 10));
    assert_eq!(
        game.dialogue_choices().unwrap().available[0].topic,
        id(EXCLUSIVE_A_TOPIC)
    );
}

#[test]
fn save_restores_the_active_exclusive_branch() {
    let mut original = exclusive_game(None, true);
    open(&mut original);
    original.choose_dialogue(id(EXCLUSIVE_A_TOPIC)).unwrap();
    original.advance_dialogue().unwrap();
    original.close_dialogue();
    let saved = original.save_state().unwrap();

    let mut restored = exclusive_game(None, true);
    restored.load_state(&saved).unwrap();
    let player = restored.player();
    restored.begin_dialogue(id(SPEAKER), player).unwrap();
    assert_eq!(
        restored.dialogue_choices().unwrap().available[0].topic,
        id(EXCLUSIVE_A_TOPIC)
    );
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
    let Some(data) = installed::data_dir() else {
        eprintln!("SKYRIM_DATA unset, skipping installed dialogue survey");
        return;
    };
    let order = installed::load_order(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ])
    .unwrap();
    let mut game = Game::from_shared_order(order);
    let mut infos = 0;
    let mut fragments = 0;
    let mut topics = std::collections::BTreeSet::new();
    let mut branch_flags = std::collections::BTreeMap::new();
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
                let vmad = Vmad::parse(bytes, T(b"INFO")).unwrap_or_else(|e| panic!("{id:?}: {e}"));
                let encoded = vmad.write(T(b"INFO"));
                // The writer keeps the authored script header, so the record
                // round-trips byte for byte.
                assert_eq!(encoded, bytes, "{id:?}");
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
        for (id, record) in world.order().winners_of(T(b"DLBR")) {
            if record.is_deleted() {
                continue;
            }
            let flags = u32::from_le_bytes(
                record
                    .find(T(b"DNAM"))
                    .unwrap_or_else(|| panic!("DLBR {id:?} has no DNAM"))
                    .try_into()
                    .unwrap_or_else(|_| panic!("DLBR {id:?} has invalid DNAM")),
            );
            *branch_flags.entry(flags).or_insert(0usize) += 1;
        }
    }
    assert!(infos > 10_000 && fragments > 1_000 && topics.len() > 1_000);
    eprintln!(
        "dialogue survey: {infos} INFO, {fragments} fragments, {} ordered topics",
        topics.len()
    );
    eprintln!("dialogue branch flags: {branch_flags:?}");
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
    assert!(
        choices
            .available
            .iter()
            .filter_map(|choice| choice.prompt.as_deref())
            .any(|prompt| !prompt.starts_with("<string ")),
        "localized shipped dialogue choices stayed unresolved"
    );
    let rumors = game
        .world()
        .form("WhiterunInnkeeperRumorsBranchTopic")
        .unwrap();
    let rumor_choice = choices
        .available
        .iter()
        .find(|choice| choice.topic == rumors)
        .unwrap();
    assert_eq!(
        rumor_choice.prompt.as_deref(),
        Some("Give me the latest gossip.")
    );
    let before_traces = game.traces().len();
    let before_calls: u64 = game.vm.natives.calls.values().sum();
    let mut line = game.choose_dialogue(rumors).unwrap();
    assert!(
        line.as_ref()
            .and_then(|line| line.text.as_deref())
            .is_some_and(|text| !text.starts_with("<string ")),
        "rumors response stayed unresolved"
    );
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

#[test]
fn tst_14_dialogue_reuses_the_installed_parse_and_isolates_games() {
    let masters = [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ];
    let Some(order) = installed::load_order(&masters) else {
        eprintln!("SKYRIM_DATA not set, skipping TST-14 installed parse check");
        return;
    };
    let cached = installed::load_order(&masters).unwrap();
    assert!(std::sync::Arc::ptr_eq(&order, &cached));

    let first = Game::from_shared_order(order);
    let second = Game::from_shared_order(cached);
    first.world_mut().perk_points = 9;
    assert_eq!(first.world().perk_points, 9);
    assert_eq!(second.world().perk_points, 0);
}
