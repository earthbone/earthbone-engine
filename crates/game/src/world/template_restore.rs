// SPDX-License-Identifier: GPL-3.0-or-later

//! Restore recorded NPC choices without repeating leveled-list selection.
//! ExtraLeveledCreature links follow CommonLib, xEdit and the external corpus.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use soe_save::Save;

use super::{le_u32, templates::TemplateChoice, World, DYNAMIC_FILE, T};
use crate::vanilla_save::{RestoreError, SavedFormMap};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct TemplateRestoreReport {
    pub actors: usize,
    pub restored_npc_changes: usize,
    pub pending_form_flags: BTreeMap<GlobalFormId, (u32, u16)>,
}

fn bad(actor: GlobalFormId, reason: impl std::fmt::Display) -> RestoreError {
    RestoreError::WorldReference {
        id: actor,
        reason: reason.to_string(),
    }
}

impl World {
    fn saved_list_contains(
        &self,
        root: GlobalFormId,
        selected: GlobalFormId,
    ) -> Result<bool, String> {
        let mut pending = vec![root];
        let mut visited = BTreeSet::new();
        while let Some(list) = pending.pop() {
            if !visited.insert(list) {
                continue;
            }
            if visited.len() > 4096 {
                return Err("excessive saved actor selection graph".into());
            }
            let entries = if let Some(state) = self.leveled_lists.get(&list) {
                state
                    .entries
                    .iter()
                    .map(|entry| entry.form)
                    .collect::<Vec<_>>()
            } else {
                let record = self
                    .record(list)
                    .filter(|r| r.tag == T(b"LVLN") && !r.is_deleted())
                    .ok_or_else(|| format!("missing actor list {list}"))?;
                let mut entries = vec![];
                for bytes in record.all(T(b"LVLO")) {
                    if bytes.len() != 12 {
                        return Err(format!("invalid actor list {list}"));
                    }
                    let raw = le_u32(&bytes[4..]).ok_or("invalid actor list entry")?;
                    entries.push(
                        self.resolve_from(list, raw)
                            .ok_or("unresolved actor list entry")?,
                    );
                }
                entries
            };
            for entry in entries {
                if entry == selected {
                    return Ok(true);
                }
                if self.tag_of(entry) == Some(T(b"LVLN")) {
                    pending.push(entry);
                }
            }
        }
        Ok(false)
    }

    fn saved_template_chain(
        &self,
        original: GlobalFormId,
        selected: GlobalFormId,
    ) -> Result<Vec<GlobalFormId>, String> {
        let mut chain = vec![];
        let mut visited = BTreeSet::new();
        let mut current = original;
        let mut remaining_groups = 0x1FFF;
        let mut used_selection = false;
        loop {
            if !visited.insert(current) || visited.len() > 128 {
                return Err("cyclic or excessive saved template chain".into());
            }
            if self.tag_of(current) == Some(T(b"LVLN")) {
                if used_selection {
                    return Err("saved actor needs another leveled template selection".into());
                }
                if !self.saved_list_contains(current, selected)? {
                    return Err(format!(
                        "saved NPC {selected} is absent from actor list {current}"
                    ));
                }
                current = selected;
                used_selection = true;
                continue;
            }
            let npc = self.npc_record(current).map_err(|e| e.to_string())?;
            chain.push(current);
            remaining_groups &= npc.config.template_flags;
            let Some(raw) = npc
                .template
                .filter(|raw| *raw != 0 && remaining_groups != 0)
            else {
                if !used_selection {
                    return Err("saved leveled choice has no authored leveled link".into());
                }
                return Ok(chain);
            };
            current = self
                .resolve_from(current, raw)
                .ok_or("unresolved saved NPC template")?;
        }
    }

    /// Restore selected NPC chains after placements and before actor initialization.
    /// No RNG, chance-none check, inventory event or actor initialization runs.
    ///
    /// # Errors
    /// Existing actor state/choices, missing identities, invalid links or malformed data.
    pub fn restore_vanilla_actor_templates(
        &mut self,
        save: &Save,
    ) -> Result<TemplateRestoreReport, RestoreError> {
        if !self.actors.is_empty()
            || !self.actor_bases.is_empty()
            || !self.actor_templates.is_empty()
        {
            return Err(RestoreError::ActorTemplates(
                "restore actor choices before actor initialization".into(),
            ));
        }
        let mapping = SavedFormMap::new(save, &self.order)?;
        let mut decoded = BTreeMap::new();
        let mut report = TemplateRestoreReport::default();
        for change in &save.change_forms {
            let Some(saved) = change.leveled_creature(save.form_version)? else {
                continue;
            };
            let actor = mapping
                .reference(change.form)?
                .ok_or(RestoreError::Reference(change.form.raw()))?;
            let original = mapping
                .reference(saved.original_base)?
                .ok_or_else(|| bad(actor, "null original actor base"))?;
            let selected = mapping
                .reference(saved.template_base)?
                .ok_or_else(|| bad(actor, "null selected actor template"))?;
            let state = self
                .refs
                .get(&actor)
                .ok_or_else(|| bad(actor, "restore reference placements first"))?;
            if !state.is_actor || state.base != Some(original) {
                return Err(bad(
                    actor,
                    "saved original base disagrees with actor placement",
                ));
            }
            let chain = self
                .saved_template_chain(original, selected)
                .map_err(|error| bad(actor, error))?;
            if decoded.insert(actor, (chain, saved.npc)).is_some() {
                return Err(bad(actor, "duplicate saved actor choice"));
            }
        }
        let mut next = self.next_dynamic;
        let mut prepared = BTreeMap::new();
        let mut records = BTreeMap::new();
        for (actor, (chain, npc)) in decoded {
            next = next
                .checked_add(1)
                .filter(|n| *n <= 0xFFFFFF)
                .ok_or_else(|| bad(actor, "temporary NPC identity space exhausted"))?;
            let id = GlobalFormId {
                file: DYNAMIC_FILE,
                object: next,
            };
            if self.exists(id) {
                return Err(bad(actor, "temporary NPC identity collision"));
            }
            let value = self
                .materialize_npc(actor, id, &chain)
                .map_err(|e| bad(actor, e))?;
            let value = self.apply_saved_npc(id, value, &npc, &mapping)?;
            if let Some(flags) = value.npc.pending_form_flags {
                report.pending_form_flags.insert(actor, flags);
            }
            report.restored_npc_changes += usize::from(npc.change_flags != 0);
            records.insert(id, value);
            prepared.insert(
                actor,
                TemplateChoice {
                    temporary_base: Some(id),
                    level: None,
                    chain: Some(chain),
                },
            );
        }
        report.actors = prepared.len();
        self.record_overrides.extend(records);
        self.actor_templates = prepared;
        self.next_dynamic = next;
        Ok(report)
    }
}
