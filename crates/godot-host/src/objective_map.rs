// SPDX-License-Identifier: GPL-3.0-or-later

//! Quest objective map targets resolved from QUST aliases and live world state.
//!
//! QUST target layout follows xEdit's `wbDefinitionsTES5.pas` and the local
//! `uesp-modfileformat/QUST.wikitext` description.

use soe_esm::GlobalFormId;
use soe_game::Game;

/// One mapped target belonging to a quest objective.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub quest: GlobalFormId,
    pub objective: u16,
    pub alias: u32,
    pub reference: GlobalFormId,
    pub position: [f32; 3],
    pub compass_marker_ignores_locks: bool,
    pub visible: bool,
}

/// Resolve objective targets for active quests in stable quest and target order.
///
/// Targets with unresolved aliases, missing location world markers, or
/// conditions that are false or unsupported are omitted. Visibility reflects
/// the displayed objective and the target reference's current enabled state.
pub fn markers(game: &mut Game) -> Vec<Marker> {
    let quests = game.world().loaded_quests();
    let mut markers = Vec::new();
    for quest in quests {
        let definitions = {
            let world = game.world();
            let Some(state) = world.quest(quest).filter(|state| state.active) else {
                continue;
            };
            state
                .def
                .objectives
                .iter()
                .map(|objective| (objective.index, objective.targets.clone()))
                .collect::<Vec<_>>()
        };
        for (objective, targets) in definitions {
            let displayed = game
                .world()
                .quest(quest)
                .and_then(|state| state.objectives.get(&objective))
                .is_some_and(|state| state.displayed);
            for (target_index, target) in targets.iter().enumerate() {
                let Ok(alias) = u32::try_from(target.alias) else {
                    continue;
                };
                if !game
                    .objective_target_conditions_met(quest, objective, target_index)
                    .unwrap_or(false)
                {
                    continue;
                }
                let Some(reference) = resolve_target(game, quest, alias) else {
                    continue;
                };
                let placement = {
                    let mut world = game.world_mut();
                    let Some(reference_state) = world.reference(reference) else {
                        continue;
                    };
                    (
                        reference_state.position,
                        reference_state.enabled && !reference_state.deleted,
                    )
                };
                markers.push(Marker {
                    quest,
                    objective,
                    alias,
                    reference,
                    position: placement.0,
                    compass_marker_ignores_locks: target.compass_marker_ignores_locks,
                    visible: displayed && placement.1,
                });
            }
        }
    }
    markers
}

fn resolve_target(game: &mut Game, quest: GlobalFormId, alias: u32) -> Option<GlobalFormId> {
    let mut world = game.world_mut();
    let is_location = world.alias_is_location(quest, alias);
    let target = world.alias_ref(quest, alias)?;
    if is_location {
        world.location_marker(target)
    } else {
        Some(target)
    }
}
