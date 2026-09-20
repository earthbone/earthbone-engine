// SPDX-License-Identifier: GPL-3.0-or-later

//! LVSP initialization over the shared spell and leveled-list state.
//! Layouts and flags follow xEdit LVSP and Mutagen LeveledSpell. Recursive
//! count handling follows skymp LeveledListUtils, with the highest-eligible
//! level restriction also used by OpenMW and this engine's actor templates.

use std::collections::BTreeSet;

use soe_esm::{Error, GlobalFormId};

use super::{LeveledList, World, T};

const MAX_DEPTH: usize = 128;
const MAX_VISITS: usize = 65_536;

impl World {
    /// Select concrete spells from a leveled spell list at the actor's level.
    /// Duplicate spells collapse in the actor's set, while duplicate entries
    /// retain their selection weight. Failed selection restores the RNG.
    ///
    /// # Errors
    /// Rejects malformed records, unsupported flags, invalid spell links,
    /// cycles, zero quantities or levels, and excessive recursive expansion.
    pub fn select_leveled_spells(
        &mut self,
        list: GlobalFormId,
        level: u32,
    ) -> Result<BTreeSet<GlobalFormId>, Error> {
        if level == 0 || self.tag_of(list) != Some(T(b"LVSP")) {
            return Err(Error::Invalid(
                "spell selection requires LVSP and a positive level".into(),
            ));
        }
        let previous_rng = self.rng;
        let mut result = BTreeSet::new();
        let mut path = BTreeSet::new();
        let mut visits = MAX_VISITS;
        match self.select_spell_entry(list, level, 1, &mut path, &mut visits, &mut result) {
            Ok(()) => Ok(result),
            Err(error) => {
                self.rng = previous_rng;
                Err(error)
            }
        }
    }

    pub(super) fn expand_actor_spell_lists(
        &mut self,
        roots: &BTreeSet<GlobalFormId>,
        level: u32,
    ) -> Result<BTreeSet<GlobalFormId>, Error> {
        let previous_rng = self.rng;
        let mut spells = BTreeSet::new();
        for root in roots {
            if self.tag_of(*root) == Some(T(b"LVSP")) {
                match self.select_leveled_spells(*root, level) {
                    Ok(selected) => spells.extend(selected),
                    Err(error) => {
                        self.rng = previous_rng;
                        return Err(error);
                    }
                }
            } else {
                spells.insert(*root);
            }
        }
        Ok(spells)
    }

    fn spell_list(&mut self, id: GlobalFormId) -> Result<LeveledList, Error> {
        let record = self
            .record(id)
            .filter(|record| record.tag == T(b"LVSP") && !record.is_deleted())
            .ok_or_else(|| Error::Invalid(format!("missing leveled spell list {id}")))?;
        for tag in [T(b"LVLD"), T(b"LVLF"), T(b"LLCT")] {
            if record.find(tag).is_some_and(|bytes| bytes.len() != 1) {
                return Err(Error::Invalid(format!("invalid LVSP {tag}")));
            }
        }
        if record.all(T(b"LVLO")).any(|bytes| bytes.len() != 12)
            || record
                .find(T(b"LLCT"))
                .is_some_and(|bytes| usize::from(bytes[0]) != record.all(T(b"LVLO")).count())
        {
            return Err(Error::Invalid("invalid LVSP entry layout or count".into()));
        }
        if record.find(T(b"LVLG")).is_some() {
            return Err(Error::Invalid(
                "LVSP does not define a chance global".into(),
            ));
        }
        let list = self
            .leveled_list(id)
            .ok_or_else(|| Error::Invalid("missing leveled spell state".into()))?
            .clone();
        if list.flags & !7 != 0
            || list.chance_none > 100
            || list.entries.iter().any(|entry| entry.count == 0)
        {
            return Err(Error::Invalid(
                "invalid LVSP flags chance or quantity".into(),
            ));
        }
        Ok(list)
    }

    fn select_spell_entry(
        &mut self,
        id: GlobalFormId,
        level: u32,
        count: u16,
        path: &mut BTreeSet<GlobalFormId>,
        visits: &mut usize,
        spells: &mut BTreeSet<GlobalFormId>,
    ) -> Result<(), Error> {
        *visits = visits
            .checked_sub(1)
            .ok_or_else(|| Error::Invalid("excessive leveled spell expansion".into()))?;
        let record = self
            .record(id)
            .filter(|record| !record.is_deleted())
            .ok_or_else(|| Error::Invalid(format!("missing spell entry {id}")))?;
        if record.tag == T(b"SPEL") {
            spells.insert(id);
            return Ok(());
        }
        if path.len() >= MAX_DEPTH || !path.insert(id) {
            return Err(Error::Invalid(
                "cyclic or excessive leveled spell nesting".into(),
            ));
        }
        let list = self.spell_list(id)?;
        let highest = list
            .entries
            .iter()
            .filter(|entry| u32::from(entry.level) <= level)
            .map(|entry| entry.level)
            .max();
        let entries: Vec<_> = list
            .entries
            .iter()
            .filter(|entry| {
                u32::from(entry.level) <= level
                    && (list.flags & 1 != 0 || Some(entry.level) == highest)
            })
            .collect();
        let rolls = if list.flags & 2 != 0 { count } else { 1 };
        for _ in 0..rolls {
            *visits = visits
                .checked_sub(1)
                .ok_or_else(|| Error::Invalid("excessive leveled spell rolls".into()))?;
            if entries.is_empty()
                || list.chance_none == 100
                || (list.chance_none != 0 && self.random_int(0, 99) < i32::from(list.chance_none))
            {
                continue;
            }
            if list.flags & 4 != 0 {
                for entry in &entries {
                    self.select_spell_entry(entry.form, level, entry.count, path, visits, spells)?;
                }
            } else {
                let index = if entries.len() == 1 {
                    0
                } else {
                    (self.random_u64() % entries.len() as u64) as usize
                };
                let entry = entries[index];
                self.select_spell_entry(entry.form, level, entry.count, path, visits, spells)?;
            }
        }
        path.remove(&id);
        Ok(())
    }
}
