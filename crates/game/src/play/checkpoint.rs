// SPDX-License-Identifier: GPL-3.0-or-later

//! PLY-03 route checkpoints over live forms. Quest flags follow CommonLib
//! TESQuest.h and the existing xEdit-based ESS importer. No saved state is added.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;

use crate::Game;

/// Quest progress and flags compared at a route checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestCheckpoint {
    pub current: u16,
    pub highest: u16,
    pub done: Vec<u16>,
    pub running: bool,
    pub active: bool,
    pub completed: bool,
    pub failed: bool,
    pub other_flags: u16,
    pub form_flags: Option<(u32, u16)>,
}

/// Quest and player inventory fields at one route checkpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub quests: BTreeMap<GlobalFormId, QuestCheckpoint>,
    pub inventory: BTreeMap<GlobalFormId, i32>,
}

impl Checkpoint {
    /// Capture quest stages, flags and player quantities from the shared world.
    /// Include loaded quests and the supplied comparison scope. Unloaded quests
    /// use their record defaults without changing the world or dispatching events.
    ///
    /// # Errors
    /// Returns an error for a missing player or an invalid quest in the scope.
    pub fn capture(game: &mut Game, scope: &[GlobalFormId]) -> Result<Self, super::Error> {
        let player = game.player();
        let world = game.world();
        let mut quests = BTreeMap::new();
        for id in scope
            .iter()
            .copied()
            .chain(world.loaded_quests())
            .collect::<BTreeSet<_>>()
        {
            let initial;
            let state = if let Some(state) = world.quest(id) {
                state
            } else {
                initial = world
                    .initial_quest(id)
                    .ok_or_else(|| super::Error::Checkpoint(format!("invalid quest {id}")))?;
                &initial
            };
            quests.insert(
                id,
                QuestCheckpoint {
                    current: state.current,
                    highest: state.highest,
                    done: state.done.iter().copied().collect(),
                    running: state.running,
                    active: state.active,
                    completed: state.completed,
                    failed: state.failed,
                    other_flags: state.runtime.other_flags,
                    form_flags: state.runtime.form_flags,
                },
            );
        }
        let inventory = world
            .reference_ref(player)
            .ok_or_else(|| super::Error::Checkpoint("player reference unavailable".into()))?
            .inventory
            .iter()
            .filter(|(_, count)| *count != 0)
            .map(|(item, count)| (*item, count))
            .collect();
        Ok(Self { quests, inventory })
    }

    /// List every differing field, sorted by quest or item identity.
    /// Inventory entries absent on one side have quantity zero.
    pub fn differences(&self, expected: &Self) -> Vec<String> {
        differences(self, expected)
    }
}

fn differences(before: &Checkpoint, expected: &Checkpoint) -> Vec<String> {
    let mut differences = Vec::new();
    for id in before
        .quests
        .keys()
        .chain(expected.quests.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        match (before.quests.get(&id), expected.quests.get(&id)) {
            (Some(left), Some(right)) => {
                for (field, old, new) in [
                    (
                        "current",
                        format!("{}", left.current),
                        format!("{}", right.current),
                    ),
                    (
                        "highest",
                        format!("{}", left.highest),
                        format!("{}", right.highest),
                    ),
                    (
                        "done",
                        format!("{:?}", left.done),
                        format!("{:?}", right.done),
                    ),
                    (
                        "running",
                        format!("{}", left.running),
                        format!("{}", right.running),
                    ),
                    (
                        "active",
                        format!("{}", left.active),
                        format!("{}", right.active),
                    ),
                    (
                        "completed",
                        format!("{}", left.completed),
                        format!("{}", right.completed),
                    ),
                    (
                        "failed",
                        format!("{}", left.failed),
                        format!("{}", right.failed),
                    ),
                    (
                        "quest_flags",
                        format!("{:#06x}", left.other_flags),
                        format!("{:#06x}", right.other_flags),
                    ),
                    (
                        "form_flags",
                        format!("{:?}", left.form_flags),
                        format!("{:?}", right.form_flags),
                    ),
                ] {
                    if old != new {
                        differences.push(format!("quest {id:?}.{field}: {old} -> {new}"));
                    }
                }
            }
            (left, right) => differences.push(format!(
                "quest {id:?}.present: {} -> {}",
                left.is_some(),
                right.is_some()
            )),
        }
    }
    for id in before
        .inventory
        .keys()
        .chain(expected.inventory.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let old = before.inventory.get(&id).copied().unwrap_or_default();
        let new = expected.inventory.get(&id).copied().unwrap_or_default();
        if old != new {
            differences.push(format!("player.inventory {id:?}: {old} -> {new}"));
        }
    }
    differences
}
