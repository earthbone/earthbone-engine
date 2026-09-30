// SPDX-License-Identifier: GPL-3.0-or-later

//! LVLI quantity selection from xEdit and UESP's LVLF/LVLO/LVLG contracts.
//! Like the actor-template path, selection uses the world's saved RNG.
//! skymp LeveledListUtils informed recursion, with nested counts retained here.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{Error, GlobalFormId};

use super::{le_u32, LeveledList, World, T};
use crate::inventory::{Extra, Inventory, Stack};

mod extras;

mod special_loot;
use special_loot::SpecialLoot;

struct List {
    state: LeveledList,
    chance: u8,
    special: Option<SpecialLoot>,
    extras: Vec<Vec<Extra>>,
}

impl World {
    /// Resolve LVLI quantities at a caller-supplied selection level.
    /// This does not add items or send events. Repeated calls roll again.
    ///
    /// # Errors
    /// Rejects invalid records, cycles, nonpositive levels, negative quantities,
    /// arithmetic overflow and excessive expansion. Mixed Use All flags follow
    /// the runtime's precedence, Special Loot uses the current player level and
    /// live game settings, and COED metadata rides along with each entry.
    /// Errors preserve RNG state. Lazy list/global reads may populate caches.
    pub fn roll_leveled_items(
        &mut self,
        list: GlobalFormId,
        level: u32,
        count: i32,
    ) -> Result<BTreeMap<GlobalFormId, i32>, Error> {
        self.roll_leveled_inventory(list, level, count)
            .map(|items| items.iter().map(|(item, count)| (*item, count)).collect())
    }

    /// Resolve LVLI entries into inventory quantities and distinct extra-data stacks.
    /// Nested lists supply their own leaf metadata. Intermediate COED is not inherited.
    /// This does not mutate any reference inventory or send inventory events.
    ///
    /// # Errors
    /// Invalid graphs, metadata, flags, levels, counts or excessive expansion.
    /// Errors preserve RNG state, while lazy record reads may populate caches.
    pub fn roll_leveled_inventory(
        &mut self,
        list: GlobalFormId,
        level: u32,
        count: i32,
    ) -> Result<Inventory, Error> {
        self.roll_leveled_inventory_context(list, level, count, None)
    }

    pub(crate) fn roll_leveled_inventory_context(
        &mut self,
        list: GlobalFormId,
        level: u32,
        count: i32,
        player_level: Option<u32>,
    ) -> Result<Inventory, Error> {
        self.roll_leveled_inventory_mode(list, level, count, player_level, true)
    }

    /// Outfit generation uses the same rolls but does not apply COED extras.
    pub(super) fn roll_outfit_items(
        &mut self,
        list: GlobalFormId,
        level: u32,
    ) -> Result<Inventory, Error> {
        self.roll_leveled_inventory_mode(list, level, 1, None, false)
    }

    fn roll_leveled_inventory_mode(
        &mut self,
        list: GlobalFormId,
        level: u32,
        count: i32,
        player_level: Option<u32>,
        metadata: bool,
    ) -> Result<Inventory, Error> {
        if level == 0 || count < 0 {
            return Err(Error::Invalid("invalid leveled item level or count".into()));
        }
        let mut lists = BTreeMap::new();
        self.prepare_item_lists(list, &mut lists, &mut BTreeSet::new(), metadata)?;
        let previous_rng = self.rng;
        let result = self.roll_item_list(list, level, count, player_level, &lists, &mut 65_536);
        if result.is_err() {
            self.rng = previous_rng;
        }
        result
    }

    fn prepare_item_lists(
        &mut self,
        id: GlobalFormId,
        lists: &mut BTreeMap<GlobalFormId, List>,
        visiting: &mut BTreeSet<GlobalFormId>,
        metadata: bool,
    ) -> Result<(), Error> {
        if visiting.contains(&id) {
            return Err(Error::Invalid(format!("cyclic leveled item list {id}")));
        }
        if lists.contains_key(&id) {
            return Ok(());
        }
        if visiting.len() >= 128 || lists.len() >= 4096 {
            return Err(Error::Invalid("excessive leveled item graph".into()));
        }
        visiting.insert(id);
        let record = self
            .record(id)
            .filter(|record| !record.is_deleted() && record.tag == T(b"LVLI"))
            .ok_or_else(|| Error::Invalid(format!("missing leveled item list {id}")))?;
        for tag in [T(b"LVLD"), T(b"LVLF"), T(b"LLCT")] {
            if record.find(tag).is_some_and(|bytes| bytes.len() != 1) {
                return Err(Error::Invalid(format!("invalid LVLI {tag}")));
            }
        }
        if record.all(T(b"LVLO")).any(|bytes| bytes.len() != 12)
            || record
                .find(T(b"LLCT"))
                .is_some_and(|bytes| usize::from(bytes[0]) != record.all(T(b"LVLO")).count())
        {
            return Err(Error::Invalid("invalid LVLI entries".into()));
        }
        let mut extras = if metadata {
            self.leveled_item_extras(id)?
        } else {
            Vec::new()
        };
        let global = match record.find(T(b"LVLG")) {
            Some(bytes) if bytes.len() == 4 => le_u32(bytes).filter(|raw| *raw != 0),
            Some(_) => return Err(Error::Invalid("invalid LVLI chance global".into())),
            None => None,
        };
        let state = self
            .leveled_list(id)
            .ok_or_else(|| Error::Invalid("missing LVLI state".into()))?
            .clone();
        if state.flags & !15 != 0 {
            return Err(Error::Invalid(format!(
                "unsupported LVLI flags {:#04x}",
                state.flags
            )));
        }
        let chance = if let Some(raw) = global {
            let global = self
                .resolve_from(id, raw)
                .filter(|global| {
                    self.record(*global)
                        .is_some_and(|record| record.tag == T(b"GLOB") && !record.is_deleted())
                })
                .ok_or_else(|| Error::Invalid("missing LVLI chance global".into()))?;
            self.global_value(global)
        } else {
            f32::from(state.chance_none)
        };
        if !chance.is_finite() || !(0.0..=100.0).contains(&chance) {
            return Err(Error::Invalid("invalid LVLI chance-none percentage".into()));
        }
        // CommonLib TESLeveledList::GetChanceNone returns a byte, including LVLG.
        let chance = chance as u8;
        for entry in &state.entries {
            if entry.count == 0 {
                return Err(Error::Invalid("zero LVLI entry count".into()));
            }
            if self.tag_of(entry.form) == Some(T(b"LVLI")) {
                self.prepare_item_lists(entry.form, lists, visiting, metadata)?;
            } else if !self.is_inventory_base(entry.form) {
                return Err(Error::Invalid(format!("invalid LVLI item {}", entry.form)));
            }
        }
        let use_all = state.flags & 4 != 0 && state.flags & 2 == 0;
        let special = if state.flags & 8 != 0 && !use_all {
            Some(SpecialLoot::read(self)?)
        } else {
            None
        };
        visiting.remove(&id);
        extras.resize_with(state.entries.len(), Vec::new);
        lists.insert(
            id,
            List {
                state,
                chance,
                special,
                extras,
            },
        );
        Ok(())
    }

    fn roll_item_list(
        &mut self,
        id: GlobalFormId,
        level: u32,
        count: i32,
        player_level: Option<u32>,
        lists: &BTreeMap<GlobalFormId, List>,
        budget: &mut u32,
    ) -> Result<Inventory, Error> {
        let list = lists
            .get(&id)
            .ok_or_else(|| Error::Invalid("missing prepared LVLI".into()))?;
        let mut result = Inventory::default();
        if count == 0 {
            return Ok(result);
        }
        let each = list.state.flags & 2 != 0;
        // Runtime CalculateCurrentFormList selects the per-count path first.
        let use_all = list.state.flags & 4 != 0 && !each;
        for _ in 0..if each { count } else { 1 } {
            *budget = budget
                .checked_sub(1)
                .ok_or_else(|| Error::Invalid("excessive LVLI expansion".into()))?;
            // The runtime tests chance-none before the Special Loot draw.
            if list.special.is_some() && self.item_list_chance_none(list.chance) {
                continue;
            }
            let used_level = if let Some(special) = &list.special {
                let input = if list.state.flags & 1 != 0 {
                    level.min(
                        list.state
                            .entries
                            .iter()
                            .map(|entry| u32::from(entry.level))
                            .max()
                            .unwrap_or(0),
                    )
                } else {
                    level
                };
                i64::from(special.level(
                    self.player_level,
                    input,
                    self.random_u64_for(super::RngStream::LeveledItems) as u32,
                ))
            } else {
                // Only ordinary selection applies AddItem's player-level ceiling.
                i64::from(player_level.map_or(level, |player| level.min(player.max(1))))
            };
            let highest = list
                .state
                .entries
                .iter()
                .filter(|entry| i64::from(entry.level) <= used_level)
                .map(|entry| entry.level)
                .max();
            // Entry positions ride along so each draw finds its COED extras.
            let eligible: Vec<_> = list
                .state
                .entries
                .iter()
                .enumerate()
                .filter(|(_, entry)| {
                    use_all
                        || (i64::from(entry.level) <= used_level
                            && (list.state.flags & 1 != 0 || Some(entry.level) == highest))
                })
                .collect();
            if eligible.is_empty()
                || (list.special.is_none() && self.item_list_chance_none(list.chance))
            {
                continue;
            }
            let selected = if use_all {
                eligible.as_slice()
            } else {
                let index = if eligible.len() == 1 {
                    0
                } else {
                    (self.random_u64_for(super::RngStream::LeveledItems) % eligible.len() as u64)
                        as usize
                };
                &eligible[index..=index]
            };
            for &(index, entry) in selected {
                let quantity = i32::from(entry.count);
                let items = if lists.contains_key(&entry.form) {
                    self.roll_item_list(entry.form, level, quantity, player_level, lists, budget)?
                } else {
                    let mut items = Inventory::default();
                    let extra = list.extras.get(index).cloned().unwrap_or_default();
                    if extra.is_empty() {
                        items
                            .add(entry.form, quantity)
                            .map_err(|error| inventory_error(&error))?;
                    } else {
                        items
                            .add_selected_stack(
                                entry.form,
                                Stack::new(quantity, extra)
                                    .map_err(|error| inventory_error(&error))?,
                            )
                            .map_err(|error| inventory_error(&error))?;
                    }
                    items
                };
                for (item, quantity) in items.iter() {
                    let multiplier = if each { 1 } else { count };
                    let quantity = quantity
                        .checked_mul(multiplier)
                        .ok_or_else(|| Error::Invalid("LVLI quantity overflow".into()))?;
                    if items.stacks(*item).is_empty() {
                        result
                            .add(*item, quantity)
                            .map_err(|error| inventory_error(&error))?;
                    } else {
                        for stack in items.stacks(*item) {
                            let count = stack
                                .count()
                                .checked_mul(multiplier)
                                .ok_or_else(|| Error::Invalid("LVLI quantity overflow".into()))?;
                            result
                                .add_selected_stack(
                                    *item,
                                    Stack::new(count, stack.extras().to_vec())
                                        .map_err(|error| inventory_error(&error))?,
                                )
                                .map_err(|error| inventory_error(&error))?;
                        }
                        let represented: i32 = items.stacks(*item).iter().map(Stack::count).sum();
                        result
                            .add(*item, (items.count(*item) - represented) * multiplier)
                            .map_err(|error| inventory_error(&error))?;
                    }
                }
            }
        }
        Ok(result)
    }

    fn item_list_chance_none(&mut self, chance: u8) -> bool {
        chance == 100
            || (chance > 0
                && self.random_u64_for(super::RngStream::LeveledItems) % 100 < u64::from(chance))
    }
}

fn inventory_error(error: &crate::inventory::Error) -> Error {
    Error::Invalid(format!("LVLI inventory: {error}"))
}
