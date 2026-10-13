// SPDX-License-Identifier: GPL-3.0-or-later

//! Installed Hulda dialogue follows xEdit DIAL/INFO/CTDA and QUST ownership.
//! UESP INFO specifies ILSTRINGS response text and shared response data.

use super::{greeting_entries, voice_asset};
use crate::data_source::DataSource;
use soe_esm::{GlobalFormId, Tag};
use soe_game::{conditions, Game};

fn report_info(game: &mut Game, speaker: GlobalFormId, info: GlobalFormId) {
    let player = game.player();
    let mut world = game.world_mut();
    let topic = world.dialogue_topic_of(info).unwrap();
    let quest = world.owning_quest(topic).unwrap();
    let name = world.record(topic).unwrap().editor_id();
    let state = world.quest_state(quest).unwrap();
    let aliases: std::collections::BTreeMap<_, _> = state.aliases.iter().collect();
    eprintln!(
        "Hulda DIAL={topic} {name:?} INFO={info} QUST={quest} {:?} running={} aliases={:?}",
        state.def.editor_id, state.running, aliases
    );
    assert!(state.running, "selected dialogue quest must be running");
    let info_data = world.dialogue_info(info).unwrap();
    let context = conditions::Context {
        owner: info,
        subject: speaker,
        target: Some(player),
        quest: Some(quest),
        tempering_enchanted: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        magic_item: None,
        dual_cast: None,
        skill_use: None,
        light_level: None,
    };
    for condition in &info_data.conditions {
        let result = conditions::evaluate(
            &mut world,
            &game.vm,
            context,
            std::slice::from_ref(condition),
        );
        eprintln!("  CTDA {condition:?}: {result:?}");
    }
}

fn report_services(game: &mut Game, speaker: GlobalFormId) {
    let player = game.player();
    let topics: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag(*b"DIAL"))
        .filter_map(|(id, record)| {
            let name = record.editor_id()?;
            let lower = name.to_ascii_lowercase();
            ["innkeeper", "barter", "room", "food", "drink"]
                .iter()
                .any(|word| lower.contains(word))
                .then_some((id, name))
        })
        .collect();
    for (topic, name) in topics {
        let mut world = game.world_mut();
        let Some(quest) = world.owning_quest(topic) else {
            continue;
        };
        let Some(state) = world.quest_state(quest) else {
            eprintln!("Hulda service DIAL={topic} {name} QUST={quest}: no quest state");
            continue;
        };
        let running = state.running;
        let quest_name = state.def.editor_id.clone();
        let quest_conditions = state.def.dialogue_conditions.clone();
        let topic_data = world.dialogue_topic(topic).unwrap();
        if quest_name.as_deref() == Some("DialogueGeneric") {
            eprintln!("Hulda service quest CTDA={quest_conditions:?}");
            for raw in [0x03B4A5, 0x09CC97] {
                let id = world.resolve_from(quest, raw).unwrap();
                let record = world.record(id).unwrap();
                eprintln!(
                    "  service predicate form {id} {:?} {:?}",
                    record.tag,
                    record.editor_id()
                );
            }
        }
        let mut eligible = Vec::new();
        let mut exclusions = Vec::new();
        for info in world.dialogue_infos(topic).unwrap() {
            let data = world.dialogue_info(info).unwrap();
            if data
                .speaker
                .filter(|raw| *raw != 0)
                .is_some_and(|raw| world.resolve_from(info, raw) != world.base_of(speaker))
            {
                continue;
            }
            let context = conditions::Context {
                owner: info,
                subject: speaker,
                target: Some(player),
                quest: Some(quest),
                tempering_enchanted: None,
                tempering_item: None,
                alchemy_making_poison: None,
                alchemy_effect: None,
                magic_item: None,
                dual_cast: None,
                skill_use: None,
                light_level: None,
            };
            let quest_match = conditions::evaluate(
                &mut world,
                &game.vm,
                conditions::Context {
                    owner: quest,
                    ..context
                },
                &quest_conditions,
            );
            let info_match = conditions::evaluate(&mut world, &game.vm, context, &data.conditions);
            if quest_match == Ok(true) && info_match == Ok(true) {
                let functions: Vec<_> = data
                    .conditions
                    .iter()
                    .map(|condition| condition.function)
                    .collect();
                eligible.push((info, functions));
            } else if quest_name.as_deref() == Some("DialogueGeneric") {
                exclusions.push((info, quest_match, info_match, data.conditions));
            }
        }
        if !eligible.is_empty()
            || (running && name.contains("Innkeeper"))
            || quest_name.as_deref() == Some("DialogueGeneric")
        {
            eprintln!("Hulda service DIAL={topic} {name} category={:?} subtype={:?} QUST={quest} {quest_name:?} running={running} predicate-matching INFO={eligible:?}", topic_data.category, topic_data.subtype);
            for (info, quest_match, info_match, predicates) in exclusions {
                eprintln!("  excluded INFO={info} quest={quest_match:?} info={info_match:?} CTDA={predicates:?}");
            }
        }
    }
}

#[test]
fn installed_hulda_activation_offers_localized_topics_and_resolved_response_voice() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).unwrap();
    let mut game = source.localized_game().unwrap();
    let scripts = game.load_scripts_from_vfs(source.vfs());
    assert!(scripts.scripts_failed.is_empty());
    let startup = game.start();
    eprintln!(
        "Former foreign waypoint cell: {:?}",
        game.world()
            .record(GlobalFormId {
                file: 0,
                object: 0x02D3E7
            })
            .and_then(soe_esm::Record::editor_id)
    );
    let cell = game.form("WhiterunBanneredMare").unwrap();
    let speaker = game.form("HuldaRef").unwrap();
    let player = game.player();
    let position = game.world_mut().reference(speaker).unwrap().position;
    {
        let mut world = game.world_mut();
        let reference = world.reference(player).unwrap();
        reference.cell = Some(cell);
        reference.position = position;
    }
    game.change_cell(Some(cell));
    report_services(&mut game, speaker);
    eprintln!(
        "Hulda startup quests={} cell={cell} speaker={speaker}",
        startup.quests_started
    );
    assert!(game.activate_deferred(speaker, player));
    let greeting = game.begin_player_dialogue(speaker).unwrap().unwrap();
    assert!(greeting
        .text
        .as_deref()
        .is_some_and(|text| !text.is_empty() && !text.starts_with("<string ")));
    eprintln!("Hulda greeting: {:?}", greeting.text);
    report_info(&mut game, speaker, greeting.info);
    let choices = game.dialogue_choices_after_response().unwrap();
    for choice in &choices.available {
        eprintln!("Hulda topic {:?}: {:?}", choice.topic, choice.prompt);
        report_info(&mut game, speaker, choice.info);
    }
    let entries = greeting_entries(&mut game).unwrap();
    assert!(
        !entries.is_empty(),
        "activation must offer a selectable topic"
    );
    let rumors = game.form("WhiterunInnkeeperRumorsBranchTopic").unwrap();
    assert!(entries.iter().any(|entry| entry.topic == rumors));
    let mut greeting_finished = false;
    for _ in 0..32 {
        if game.advance_dialogue().unwrap().is_none() {
            greeting_finished = true;
            break;
        }
    }
    assert!(
        greeting_finished,
        "greeting did not finish within 32 responses"
    );
    let line = game.choose_dialogue(entries[0].topic).unwrap().unwrap();
    assert!(line
        .text
        .as_deref()
        .is_some_and(|text| !text.is_empty() && !text.starts_with("<string ")));
    let asset = voice_asset(&mut game, &line).unwrap();
    assert!(
        source.vfs().exists(&asset.path),
        "missing voice {}",
        asset.path
    );
    eprintln!(
        "Hulda selected DIAL={} INFO={} response={:?} voice={}",
        line.topic, line.info, line.text, asset.path
    );
    assert_eq!(game.world().tag_of(line.topic), Some(Tag(*b"DIAL")));
    game.close_dialogue();
    for _ in 0..600 {
        game.tick(1.0 / 60.0);
    }
    let actual_cell = game.world_mut().reference(speaker).unwrap().cell;
    eprintln!("Hulda after ten seconds: cell={actual_cell:?}, expected={cell}");
    assert_eq!(
        actual_cell,
        Some(cell),
        "daytime innkeeper must stay in her coordinate space"
    );
    game.begin_player_dialogue(speaker).unwrap();
    assert!(
        greeting_entries(&mut game)
            .unwrap()
            .iter()
            .any(|entry| entry.topic == rumors),
        "GetInCell must still admit Hulda's gossip after AI movement"
    );
}
