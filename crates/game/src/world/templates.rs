// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-reference NPC template choices. Layouts follow xEdit LVLN and NPC_.
//! Group traversal follows skymp EvaluateTemplate. Level filtering follows
//! the LVLF contract and OpenMW's highest-eligible versus all-level selection.
//! Placed and dynamically created actors receive the containing encounter level.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{Error, GlobalFormId};
use soe_papyrus::vm::state::Writer;
use soe_papyrus::VmError;

use super::save::FormReader;
use super::{le_u32, World, T};
use crate::npc::Npc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TemplateChoice {
    pub temporary_base: Option<GlobalFormId>,
    /// None for imported choices whose original encounter level is not saved.
    pub level: Option<u32>,
    /// None records a chance-none or an empty eligible list, without rerolling.
    pub chain: Option<Vec<GlobalFormId>>,
}

impl World {
    /// Select leveled templates once for a reference. Repeated calls retain its
    /// choice even when the player level or a script-modified list changes.
    ///
    /// # Errors
    /// Invalid records, cycles and unsupported actor counts preserve the RNG
    /// and do not install a partial selection.
    pub fn prepare_actor_template(
        &mut self,
        reference: GlobalFormId,
        base: GlobalFormId,
        level: u32,
    ) -> Result<bool, Error> {
        if let Some(choice) = self.actor_templates.get(&reference) {
            return Ok(choice.chain.is_some());
        }
        if level == 0 {
            return Err(Error::Invalid(
                "actor selection requires a positive level".into(),
            ));
        }
        let previous_rng = self.rng;
        match self.choose_template_chain(base, level) {
            Ok((chain, leveled)) => {
                let present = chain.is_some();
                if leveled {
                    self.actor_templates.insert(
                        reference,
                        TemplateChoice {
                            temporary_base: None,
                            level: Some(level),
                            chain,
                        },
                    );
                }
                Ok(present)
            }
            Err(error) => {
                self.rng = previous_rng;
                Err(error)
            }
        }
    }

    fn choose_template_chain(
        &mut self,
        base: GlobalFormId,
        level: u32,
    ) -> Result<(Option<Vec<GlobalFormId>>, bool), Error> {
        let mut current = base;
        let mut visited = BTreeSet::new();
        let mut chain = Vec::new();
        let mut groups = 0x1fff;
        let mut leveled = false;
        loop {
            visit(&mut visited, current)?;
            if self.tag_of(current) == Some(T(b"LVLN")) {
                leveled = true;
                let Some(selected) = self.choose_actor_entry(current, level)? else {
                    return Ok((None, true));
                };
                current = selected;
                continue;
            }
            let npc = self.npc_record(current)?;
            chain.push(current);
            groups &= npc.config.template_flags;
            let Some(raw) = npc.template.filter(|raw| *raw != 0 && groups != 0) else {
                return Ok((Some(chain), leveled));
            };
            current = self
                .resolve_from(current, raw)
                .ok_or_else(|| Error::Invalid("unresolved NPC template".into()))?;
        }
    }

    fn choose_actor_entry(
        &mut self,
        list: GlobalFormId,
        level: u32,
    ) -> Result<Option<GlobalFormId>, Error> {
        let record = self
            .record(list)
            .filter(|record| !record.is_deleted() && record.tag == T(b"LVLN"))
            .ok_or_else(|| Error::Invalid(format!("missing leveled actor list {list}")))?;
        for bytes in record.all(T(b"LVLO")) {
            if bytes.len() != 12 {
                return Err(Error::Invalid(
                    "LVLN entry must contain twelve bytes".into(),
                ));
            }
        }
        for tag in [T(b"LVLD"), T(b"LVLF"), T(b"LLCT")] {
            if record.find(tag).is_some_and(|bytes| bytes.len() != 1) {
                return Err(Error::Invalid(format!("invalid LVLN {tag}")));
            }
        }
        if record
            .find(T(b"LLCT"))
            .is_some_and(|bytes| usize::from(bytes[0]) != record.all(T(b"LVLO")).count())
        {
            return Err(Error::Invalid("LVLN entry count mismatch".into()));
        }
        let chance_global = match record.find(T(b"LVLG")) {
            Some(bytes) if bytes.len() == 4 => le_u32(bytes).filter(|raw| *raw != 0),
            Some(_) => return Err(Error::Invalid("invalid LVLN chance global".into())),
            None => None,
        };
        let list_state = self
            .leveled_list(list)
            .ok_or_else(|| Error::Invalid("missing leveled actor state".into()))?
            .clone();
        if list_state.flags & !3 != 0 {
            return Err(Error::Invalid("unsupported leveled actor flags".into()));
        }
        let chance = if let Some(raw) = chance_global {
            let global = self
                .resolve_from(list, raw)
                .filter(|id| self.tag_of(*id) == Some(T(b"GLOB")))
                .ok_or_else(|| Error::Invalid("missing LVLN chance global".into()))?;
            self.global_value(global)
        } else {
            f32::from(list_state.chance_none)
        };
        if !chance.is_finite() || !(0.0..=100.0).contains(&chance) {
            return Err(Error::Invalid("invalid LVLN chance-none percentage".into()));
        }
        let highest = list_state
            .entries
            .iter()
            .filter(|entry| u32::from(entry.level) <= level)
            .map(|entry| entry.level)
            .max();
        let entries: Vec<_> = list_state
            .entries
            .iter()
            .filter(|entry| {
                u32::from(entry.level) <= level
                    && (list_state.flags & 1 != 0 || Some(entry.level) == highest)
            })
            .collect();
        if entries.iter().any(|entry| entry.count != 1) {
            return Err(Error::Invalid(
                "leveled actor templates require count one".into(),
            ));
        }
        if entries.is_empty()
            || chance == 100.0
            || (chance > 0.0 && self.random_int(0, 99) < chance as i32)
        {
            return Ok(None);
        }
        let index = if entries.len() == 1 {
            0
        } else {
            (self.random_u64() % entries.len() as u64) as usize
        };
        Ok(Some(entries[index].form))
    }

    /// Record supplying an actor's inherited group, retaining its spawn choice.
    ///
    /// # Errors
    /// Rejects missing actors, suppressed spawns and invalid template chains.
    pub fn actor_component_source(
        &mut self,
        actor: GlobalFormId,
        group: u16,
    ) -> Result<GlobalFormId, Error> {
        let base = self
            .reference(actor)
            .filter(|state| state.is_actor)
            .and_then(|state| state.base)
            .ok_or_else(|| Error::Invalid(format!("missing actor base for {actor}")))?;
        self.reference_component_source(actor, base, group)
    }

    pub(super) fn reference_component_source(
        &self,
        reference: GlobalFormId,
        base: GlobalFormId,
        group: u16,
    ) -> Result<GlobalFormId, Error> {
        if let Some(choice) = self.actor_templates.get(&reference) {
            if let Some(temporary) = choice.temporary_base {
                return Ok(temporary);
            }
            let chain = choice
                .chain
                .as_ref()
                .ok_or_else(|| Error::Invalid("leveled actor spawn selected none".into()))?;
            for &id in chain {
                let npc = self.npc_record(id)?;
                if !npc.uses_template(group) || npc.template == Some(0) {
                    return Ok(id);
                }
            }
            return Err(Error::Invalid("incomplete saved NPC template chain".into()));
        }
        self.npc_component_source(base, group)
    }

    pub(crate) fn actor_npc_part(
        &mut self,
        actor: GlobalFormId,
        group: u16,
    ) -> Result<(GlobalFormId, Npc), Error> {
        let source = self.actor_component_source(actor, group)?;
        Ok((source, self.npc_record(source)?))
    }

    pub(super) fn write_actor_templates(&self, writer: &mut Writer) {
        use super::save::write_form_id;
        writer.len(self.actor_templates.len());
        for (&reference, choice) in &self.actor_templates {
            write_form_id(writer, reference);
            writer.u32(choice.level.unwrap_or(0));
            writer.bool(choice.chain.is_some());
            if let Some(chain) = &choice.chain {
                writer.len(chain.len());
                for &id in chain {
                    write_form_id(writer, id);
                }
            }
        }
    }

    pub(super) fn read_actor_templates(
        &self,
        reader: &mut FormReader<'_, '_>,
        allow_unknown_level: bool,
    ) -> Result<BTreeMap<GlobalFormId, TemplateChoice>, VmError> {
        use super::save::read_form_id;
        let invalid = || VmError::State("invalid saved actor template selection".into());
        let mut choices = BTreeMap::new();
        for _ in 0..reader.len()? {
            let reference = read_form_id(reader)?;
            let level = reader.u32()?;
            if level == 0 && !allow_unknown_level {
                return Err(invalid());
            }
            let level = (level != 0).then_some(level);
            let chain = if reader.bool()? {
                let count = reader.len()?;
                if count == 0 || count > 128 {
                    return Err(invalid());
                }
                let mut chain = Vec::with_capacity(count);
                let mut seen = BTreeSet::new();
                for _ in 0..count {
                    let id = read_form_id(reader)?;
                    if !seen.insert(id) || self.npc_record(id).is_err() {
                        return Err(invalid());
                    }
                    chain.push(id);
                }
                Some(chain)
            } else {
                None
            };
            if choices
                .insert(
                    reference,
                    TemplateChoice {
                        temporary_base: None,
                        level,
                        chain,
                    },
                )
                .is_some()
            {
                return Err(invalid());
            }
        }
        Ok(choices)
    }
}

fn visit(visited: &mut BTreeSet<GlobalFormId>, id: GlobalFormId) -> Result<(), Error> {
    if !visited.insert(id) || visited.len() > 128 {
        return Err(Error::Invalid(
            "cyclic or excessive leveled NPC template chain".into(),
        ));
    }
    Ok(())
}
