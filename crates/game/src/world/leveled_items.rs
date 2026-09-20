// SPDX-License-Identifier: GPL-3.0-or-later

//! LVLI quantity selection from xEdit and UESP's LVLF/LVLO/LVLG contracts.
//! Like the actor-template path, selection uses the world's saved RNG.
//! skymp LeveledListUtils informed recursion, with nested counts retained here.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{Error, GlobalFormId};

use super::{le_u32, LeveledList, World, T};

struct List {
    state: LeveledList,
    chance: u8,
}

impl World {
    /// Resolve ordinary LVLI quantities at a caller-supplied selection level.
    /// This does not add items or send events. Repeated calls roll again.
    ///
    /// # Errors
    /// Rejects invalid records, cycles, nonpositive levels, negative quantities,
    /// arithmetic overflow and excessive expansion. Special loot and COED metadata
    /// require separate handling before rolling.
    /// Errors preserve RNG state. Lazy list/global reads may populate caches.
    pub fn roll_leveled_items(
        &mut self,
        list: GlobalFormId,
        level: u32,
        count: i32,
    ) -> Result<BTreeMap<GlobalFormId, i32>, Error> {
        if level == 0 || count < 0 {
            return Err(Error::Invalid("invalid leveled item level or count".into()));
        }
        let mut lists = BTreeMap::new();
        self.prepare_item_lists(list, &mut lists, &mut BTreeSet::new())?;
        let previous_rng = self.rng;
        let result = self.roll_item_list(list, level, count, &lists, &mut 65_536);
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
        if record.find(T(b"COED")).is_some() {
            return Err(Error::Invalid(
                "LVLI ownership metadata is unsupported".into(),
            ));
        }
        let global = match record.find(T(b"LVLG")) {
            Some(bytes) if bytes.len() == 4 => le_u32(bytes).filter(|raw| *raw != 0),
            Some(_) => return Err(Error::Invalid("invalid LVLI chance global".into())),
            None => None,
        };
        let state = self
            .leveled_list(id)
            .ok_or_else(|| Error::Invalid("missing LVLI state".into()))?
            .clone();
        if state.flags & !7 != 0 {
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
                self.prepare_item_lists(entry.form, lists, visiting)?;
            } else if !self.is_inventory_base(entry.form) {
                return Err(Error::Invalid(format!("invalid LVLI item {}", entry.form)));
            }
        }
        visiting.remove(&id);
        lists.insert(id, List { state, chance });
        Ok(())
    }

    fn roll_item_list(
        &mut self,
        id: GlobalFormId,
        level: u32,
        count: i32,
        lists: &BTreeMap<GlobalFormId, List>,
        budget: &mut u32,
    ) -> Result<BTreeMap<GlobalFormId, i32>, Error> {
        let list = lists
            .get(&id)
            .ok_or_else(|| Error::Invalid("missing prepared LVLI".into()))?;
        let mut result = BTreeMap::new();
        if count == 0 {
            return Ok(result);
        }
        let each = list.state.flags & 2 != 0;
        // Runtime CalculateCurrentFormList selects the per-count path first.
        let use_all = list.state.flags & 4 != 0 && !each;
        let highest = list
            .state
            .entries
            .iter()
            .filter(|entry| u32::from(entry.level) <= level)
            .map(|entry| entry.level)
            .max();
        let eligible: Vec<_> = list
            .state
            .entries
            .iter()
            .filter(|entry| {
                use_all
                    || (u32::from(entry.level) <= level
                        && (list.state.flags & 1 != 0 || Some(entry.level) == highest))
            })
            .collect();
        for _ in 0..if each { count } else { 1 } {
            *budget = budget
                .checked_sub(1)
                .ok_or_else(|| Error::Invalid("excessive LVLI expansion".into()))?;
            if eligible.is_empty()
                || list.chance == 100
                || (list.chance > 0 && self.random_u64() % 100 < u64::from(list.chance))
            {
                continue;
            }
            let selected = if use_all {
                eligible.as_slice()
            } else {
                let index = if eligible.len() == 1 {
                    0
                } else {
                    (self.random_u64() % eligible.len() as u64) as usize
                };
                &eligible[index..=index]
            };
            for entry in selected {
                let quantity = i32::from(entry.count);
                let items = if lists.contains_key(&entry.form) {
                    self.roll_item_list(entry.form, level, quantity, lists, budget)?
                } else {
                    BTreeMap::from([(entry.form, quantity)])
                };
                for (item, quantity) in items {
                    let quantity = quantity
                        .checked_mul(if each { 1 } else { count })
                        .ok_or_else(|| Error::Invalid("LVLI quantity overflow".into()))?;
                    let total = result.entry(item).or_insert(0_i32);
                    *total = total
                        .checked_add(quantity)
                        .ok_or_else(|| Error::Invalid("LVLI quantity overflow".into()))?;
                }
            }
        }
        Ok(result)
    }
}
