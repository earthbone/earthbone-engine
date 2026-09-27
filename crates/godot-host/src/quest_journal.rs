// SPDX-License-Identifier: GPL-3.0-or-later

//! Read-only quest journal rows from active, form-backed quest state.
//!
//! QUST layout follows `uesp-modfileformat/QUST.wikitext` and
//! `TES5Edit/Core/wbDefinitionsTES5.pas`: type 0 is absent from the journal,
//! type 6 hides its name, and objective visibility comes from runtime state.

use soe_esm::GlobalFormId;
use soe_game::Game;

const MISCELLANEOUS_QUEST_TYPE: u32 = 6;

/// One displayed objective in a quest journal entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectiveEntry {
    pub index: u16,
    pub text: String,
    pub completed: bool,
    pub failed: bool,
}

/// One active quest shown in the journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub quest: GlobalFormId,
    /// None for miscellaneous quests, whose record type hides the quest name.
    pub title: Option<String>,
    pub objectives: Vec<ObjectiveEntry>,
}

/// List visible active quests and their displayed objectives in FormID order.
pub fn entries(game: &Game) -> Vec<Entry> {
    let world = game.world();
    world
        .loaded_quests()
        .into_iter()
        .filter_map(|id| {
            let state = world.quest(id)?;
            if !state.active || state.def.quest_type == 0 {
                return None;
            }
            let title = (state.def.quest_type != MISCELLANEOUS_QUEST_TYPE)
                .then(|| world.name_of(id).unwrap_or_else(|| format!("{id}")));
            let objectives = state
                .def
                .objectives
                .iter()
                .filter_map(|definition| {
                    let runtime = state.objectives.get(&definition.index)?;
                    if !runtime.displayed {
                        return None;
                    }
                    let text = definition
                        .text
                        .as_ref()
                        .map(|text| world.resolve_text(id, soe_esm::Tag(*b"NNAM"), text))
                        .unwrap_or_default();
                    Some(ObjectiveEntry {
                        index: definition.index,
                        text,
                        completed: runtime.completed,
                        failed: runtime.failed,
                    })
                })
                .collect();
            Some(Entry {
                quest: id,
                title,
                objectives,
            })
        })
        .collect()
}
