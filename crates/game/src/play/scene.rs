// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless presentation for DLG-18 scene dialogue and DLG-09 phase advancement.
//! Phase conditions and action bounds follow local UESP SCEN and xEdit SCEN.

use soe_esm::records::gameplay::{Condition, Scene, SceneActionData};
use soe_esm::GlobalFormId;

use super::Report;
use crate::conditions::{self, Context};
use crate::Game;

pub(super) fn play(
    game: &mut Game,
    id: GlobalFormId,
    goal: u32,
    max_ticks: u32,
    report: &mut Report,
) -> Result<(), String> {
    if max_ticks > 36_000 {
        return Err("invalid scene playback bound".into());
    }
    let scene = {
        let world = game.world();
        let record = world.record(id).ok_or("scene record unavailable")?;
        Scene::from_record(record).map_err(|error| error.to_string())?
    };
    if goal as usize >= scene.phases.len() {
        return Err("scene playback phase is out of range".into());
    }
    let mut presenting = None;
    for elapsed in 0..=max_ticks {
        let phase = game.world().scene_phase(id).ok_or("scene is not playing")?;
        if phase == goal {
            return Ok(());
        }
        if phase > goal {
            return Err("scene has passed the requested phase".into());
        }
        if elapsed == max_ticks {
            break;
        }
        let log_start = game.vm.log.entries.len();
        if let Some(slot) = presenting {
            if let Some(line) = game
                .advance_scene_dialogue_action(id, slot)
                .map_err(|error| error.to_string())?
            {
                report
                    .trace
                    .push(format!("scene {id} action {slot}: {line:?}"));
            } else {
                presenting = None;
            }
        } else if let Some((slot, _)) = scene.actions.iter().enumerate().find(|(slot, action)| {
            matches!(action.data, SceneActionData::Dialogue { .. })
                && action.start_phase.is_some_and(|start| start <= phase)
                && action.end_phase.is_some_and(|end| phase <= end)
                && game.world().scene_action_slot_complete(id, *slot as u32) == Some(false)
        }) {
            let slot = slot as u32;
            if let Some(line) = game
                .start_scene_dialogue_action(id, slot)
                .map_err(|error| error.to_string())?
            {
                report
                    .trace
                    .push(format!("scene {id} action {slot}: {line:?}"));
                presenting = Some(slot);
            }
        }
        let complete = scene.actions.iter().enumerate().all(|(slot, action)| {
            action.end_phase != Some(phase)
                || game.world().scene_action_slot_complete(id, slot as u32) == Some(true)
        });
        if presenting.is_none()
            && complete
            && phase_conditions(
                game,
                id,
                &scene,
                &scene.phases[phase as usize].completion_conditions,
            )?
        {
            let mut next = phase + 1;
            while let Some(candidate) = scene.phases.get(next as usize) {
                if phase_conditions(game, id, &scene, &candidate.start_conditions)? {
                    break;
                }
                next += 1;
            }
            let world = game.shared_world();
            let mut allocate = |name: &str, class: &str| game.vm.create_object_of(name, class);
            world
                .borrow_mut()
                .advance_scene_phase_to(id, next, &mut allocate);
        }
        super::append_log(game, report, log_start);
        super::tick(game, report);
    }
    Err(format!(
        "timeout playing {id} to phase {goal} after {max_ticks} ticks"
    ))
}

fn phase_conditions(
    game: &mut Game,
    id: GlobalFormId,
    scene: &Scene,
    predicates: &[Condition],
) -> Result<bool, String> {
    let player = game.player();
    let mut world = game.world_mut();
    let quest = scene.quest.and_then(|raw| world.resolve_from(id, raw));
    let context = Context {
        owner: id,
        subject: player,
        target: None,
        quest,
        tempering_enchanted: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        magic_item: None,
        dual_cast: None,
        skill_use: None,
        light_level: None,
    };
    conditions::evaluate(&mut world, &game.vm, context, predicates)
        .map_err(|error| error.to_string())
}
