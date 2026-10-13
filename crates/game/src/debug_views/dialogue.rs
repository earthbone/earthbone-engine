// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only dialogue diagnostics using the shared DIAL/INFO selector and CTDA
//! evaluator. Context follows xEdit INFO and CK dialogue subject/target rules.

use soe_esm::records::gameplay::Condition;
use soe_esm::{GlobalFormId, Tag};

use crate::conditions::{self, Context};
use crate::dialogue::Session;
use crate::{Game, World};

/// All active-quest topics considered for this speaker, including rejected INFOs.
/// Individual predicates are evaluated even when the selector short circuits.
///
/// # Errors
/// Returns an error for a selected form that is not an actor.
pub fn npc_dialogue(game: &mut Game, speaker: GlobalFormId) -> Result<Vec<String>, String> {
    let random_state = game.world().condition_random_state();
    let result = report(game, speaker);
    game.world_mut()
        .restore_condition_random_state(random_state);
    result
}

fn report(game: &mut Game, speaker: GlobalFormId) -> Result<Vec<String>, String> {
    if game.world().actor_ref(speaker).is_none() {
        return Err(format!("{speaker}: no loaded actor"));
    }
    let listener = game.player();
    let session = Session {
        speaker,
        listener,
        topic: None,
        current_info: None,
        last_info: None,
        response_index: 0,
    };
    let topics: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag(*b"DIAL"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, _)| id)
        .collect();
    let mut output = vec![format!("NPC {speaker}, listener {listener}. Conditions below include predicates skipped by short circuiting. Random results are previews and leave the gameplay random stream unchanged.")];
    let (roots, rejected_roots) = game.root_dialogue_topics(&session);
    output.push(format!(
        "root_topics={roots:?} rejected_roots={rejected_roots:?}"
    ));
    for topic in topics {
        let Some(quest) = game.world().owning_quest(topic) else {
            continue;
        };
        let quest_conditions = {
            let mut world = game.world_mut();
            let Some(state) = world.quest_state(quest) else {
                continue;
            };
            if !state.running {
                continue;
            }
            state.def.dialogue_conditions.clone()
        };
        let candidates = game.dialogue_candidates(topic, &session);
        let eligibility = match &candidates {
            Ok(candidates) => format!(
                "selected_candidates={:?}",
                candidates
                    .infos
                    .iter()
                    .map(|(id, _)| id)
                    .collect::<Vec<_>>()
            ),
            Err(error) => format!("rejected={error}"),
        };
        let mut world = game.world_mut();
        let ids = match world.dialogue_infos(topic) {
            Ok(ids) => ids,
            Err(error) => {
                output.push(format!("topic {topic}: {error}"));
                continue;
            }
        };
        output.push(format!(
            "topic {topic} {} quest={quest} root_eligible={} {eligibility}",
            world.editor_id(topic).unwrap_or_default(),
            roots.contains(&topic)
        ));
        let context = condition_context(quest, speaker, listener, quest);
        predicates(
            &mut output,
            &mut world,
            &game.vm,
            context,
            "QUST",
            &quest_conditions,
        );
        for info_id in ids {
            let info = match world.dialogue_info(info_id) {
                Ok(info) => info,
                Err(error) => {
                    output.push(format!("  INFO {info_id}: {error}"));
                    continue;
                }
            };
            let matches_speaker = info
                .speaker
                .filter(|raw| *raw != 0)
                .is_none_or(|raw| world.base_of(speaker) == world.resolve_from(info_id, raw));
            let context = condition_context(info_id, speaker, listener, quest);
            let condition_result =
                conditions::evaluate(&mut world, &game.vm, context, &info.conditions);
            output.push(format!("  INFO {info_id} speaker_match={matches_speaker} flags={:#x} conditions={condition_result:?} responses={} response_link={:?}", info.data.as_ref().map_or(0, |data| data.flags()), info.responses.len(), info.shared_info));
            predicates(
                &mut output,
                &mut world,
                &game.vm,
                context,
                "INFO",
                &info.conditions,
            );
        }
    }
    Ok(output)
}

fn condition_context(
    owner: GlobalFormId,
    subject: GlobalFormId,
    target: GlobalFormId,
    quest: GlobalFormId,
) -> Context {
    Context {
        owner,
        subject,
        target: Some(target),
        quest: Some(quest),
        tempering_enchanted: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        magic_item: None,
        dual_cast: None,
        skill_use: None,
        light_level: None,
    }
}

fn predicates(
    output: &mut Vec<String>,
    world: &mut World,
    vm: &soe_papyrus::Vm,
    context: Context,
    kind: &str,
    conditions: &[Condition],
) {
    for (index, condition) in conditions.iter().enumerate() {
        let result = conditions::evaluate(world, vm, context, std::slice::from_ref(condition));
        output.push(format!("    {kind} CTDA[{index}] function={} params={:?} run_on={} reference={:#x} comparison={:?} operator={} OR={} random_preview={} result={result:?}", condition.function, condition.parameters, condition.run_on, condition.reference, condition.comparison, condition.operator(), condition.or_with_next(), condition.function == 77));
    }
}
