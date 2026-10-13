// SPDX-License-Identifier: GPL-3.0-or-later

//! Quest console operations from UESP Skyrim Console, using the existing fragment path.

use super::{Args, Game, GlobalFormId, Outcome};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "sqs"
            | "sqo"
            | "sqt"
            | "completequest"
            | "resetquest"
            | "saq"
            | "caqs"
            | "movetoqt"
            | "setobjectivedisplayed"
            | "setobjectivecompleted"
            | "setobjectivefailed"
            | "getobjectivedisplayed"
            | "getobjectivecompleted"
            | "getobjectivefailed"
            | "getquestcompleted"
            | "getquestrunning"
            | "completeallobjectives"
            | "failallobjectives"
    )
}

pub(super) fn run(game: &mut Game, name: &str, args: &Args<'_>) -> Result<Outcome, String> {
    if name == "saq" || name == "caqs" {
        return all(game, name);
    }
    if name == "sqo" || name == "sqt" {
        return journal(game, name);
    }
    let id = args.form(game, 0)?;
    if game.world_mut().quest_state(id).is_none() {
        return Err("requires a quest".into());
    }
    if name == "movetoqt" {
        let targets = targets(game, id)?;
        let reference = targets
            .first()
            .copied()
            .ok_or("quest has no current target")?;
        game.world_mut().debug_player_move_to_reference(reference);
        return Ok(Outcome::Form(reference));
    }
    if name.contains("objective") {
        return objective(game, id, name, args);
    }
    let mut world = game.world_mut();
    let quest = world.quest_state(id).ok_or("requires a quest")?;
    match name {
        "sqs" => Ok(Outcome::List(
            quest
                .def
                .stages
                .iter()
                .map(|s| {
                    format!(
                        "Stage {}: {}",
                        s.index,
                        u8::from(quest.done.contains(&s.index))
                    )
                })
                .collect(),
        )),
        "completequest" => {
            quest.completed = true;
            quest.running = false;
            Ok(Outcome::Done)
        }
        "resetquest" => {
            quest.running = false;
            quest.completed = false;
            quest.failed = false;
            quest.current = 0;
            quest.highest = 0;
            quest.done.clear();
            quest.objectives.clear();
            Ok(Outcome::Done)
        }
        "getquestcompleted" => Ok(Outcome::Bool(quest.completed)),
        _ => Ok(Outcome::Bool(quest.running)),
    }
}

fn objective(
    game: &mut Game,
    id: GlobalFormId,
    name: &str,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    let mut world = game.world_mut();
    if matches!(name, "completeallobjectives" | "failallobjectives") {
        let indices: Vec<_> = world
            .quest_state(id)
            .ok_or("requires a quest")?
            .def
            .objectives
            .iter()
            .map(|o| o.index)
            .collect();
        for index in indices {
            let objective = world.objective(id, index).ok_or("missing objective")?;
            if name == "completeallobjectives" {
                objective.completed = true;
            } else {
                objective.failed = true;
            }
        }
        return Ok(Outcome::Done);
    }
    let index = u16::try_from(args.int(1)?).map_err(|_| "invalid objective index")?;
    let objective = world.objective(id, index).ok_or("missing objective")?;
    let field = if name.ends_with("displayed") {
        &mut objective.displayed
    } else if name.ends_with("completed") {
        &mut objective.completed
    } else {
        &mut objective.failed
    };
    if name.starts_with("get") {
        Ok(Outcome::Bool(*field))
    } else {
        *field = args.flag(2, true)?;
        Ok(Outcome::Done)
    }
}

fn all(game: &mut Game, name: &str) -> Result<Outcome, String> {
    let ids: Vec<_> = game
        .world()
        .order()
        .winners_of(soe_esm::Tag(*b"QUST"))
        .map(|(id, _)| id)
        .collect();
    for id in ids {
        game.start_quest(id);
        if name == "caqs" {
            let stages: Vec<_> = game
                .world_mut()
                .quest_state(id)
                .ok_or("invalid quest")?
                .def
                .stages
                .iter()
                .map(|s| s.index)
                .collect();
            for stage in stages {
                game.set_stage(id, stage);
                game.run_until_idle(64);
            }
        }
    }
    game.run_until_idle(64);
    Ok(Outcome::Done)
}

fn journal(game: &mut Game, name: &str) -> Result<Outcome, String> {
    let mut lines = Vec::new();
    let quests = game.world().loaded_quests();
    for id in quests {
        let (active, objectives) = {
            let world = game.world();
            let quest = world.quest(id).ok_or("missing quest")?;
            (quest.running || quest.completed, quest.objectives.clone())
        };
        if !active {
            continue;
        }
        let label = game.world().editor_id(id).unwrap_or_else(|| id.to_string());
        if name == "sqo" {
            for (index, state) in objectives {
                lines.push(format!(
                    "{label} {index}: displayed={} completed={} failed={}",
                    state.displayed, state.completed, state.failed
                ));
            }
        } else {
            for target in targets(game, id)? {
                lines.push(format!("{label}: {:08X}", game.world().runtime_id(target)));
            }
        }
    }
    Ok(Outcome::List(lines))
}

fn targets(game: &mut Game, id: GlobalFormId) -> Result<Vec<GlobalFormId>, String> {
    let objectives = game
        .world_mut()
        .quest_state(id)
        .ok_or("requires a quest")?
        .def
        .objectives
        .clone();
    let mut result = Vec::new();
    for objective in objectives {
        if !game
            .world()
            .quest(id)
            .and_then(|q| q.objectives.get(&objective.index))
            .is_some_and(|o| o.displayed && !o.completed && !o.failed)
        {
            continue;
        }
        for (index, target) in objective.targets.iter().enumerate() {
            if !game
                .objective_target_conditions_met(id, objective.index, index)
                .map_err(|e| e.to_string())?
            {
                continue;
            }
            let alias = u32::try_from(target.alias).map_err(|_| "invalid target alias")?;
            let mut world = game.world_mut();
            if let Some(target) = world.alias_ref(id, alias) {
                let reference = if world.alias_is_location(id, alias) {
                    world.location_marker(target)
                } else {
                    Some(target)
                };
                if let Some(reference) = reference {
                    result.push(reference);
                }
            }
        }
    }
    Ok(result)
}
