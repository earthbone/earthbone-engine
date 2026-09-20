// SPDX-License-Identifier: GPL-3.0-or-later

//! Temporary NPC records assembled from the selected component sources.
//! UESP NPC and skymp NPC_ document template groups. xEdit supplies subrecords.
//! CommonLib names the copied-template flag. No source plugin is mutated.

mod links;

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::{Error, GlobalFormId, Tag};

use super::{
    record_overrides::{raw_words, RecordOverride},
    World, DYNAMIC_FILE, T,
};
use crate::npc::template;

fn invalid(reason: impl Into<String>) -> Error {
    Error::Invalid(reason.into())
}

fn group(tag: Tag) -> Option<u16> {
    Some(match tag.as_bytes() {
        b"INAM" | b"VTCK" | b"RNAM" | b"DEST" | b"DSTD" | b"DMDL" | b"DMDT" | b"DMDS" | b"DSTF"
        | b"WNAM" | b"ANAM" | b"PNAM" | b"HCLF" | b"NAM5" | b"NAM6" | b"NAM7" | b"NAM8"
        | b"CSDT" | b"CSDI" | b"CSDC" | b"CSCR" | b"FTST" | b"QNAM" | b"NAM9" | b"NAMA"
        | b"TINI" | b"TINC" | b"TINV" | b"TIAS" => template::TRAITS,
        b"CNAM" => template::STATS,
        b"SNAM" | b"CRIF" => template::FACTIONS,
        b"SPCT" | b"SPLO" | b"PRKZ" | b"PRKR" => template::SPELLS,
        b"AIDT" | b"ZNAM" | b"GNAM" => template::AI_DATA,
        b"PKID" => template::AI_PACKAGES,
        b"FULL" | b"SHRT" => template::BASE_DATA,
        b"COCT" | b"CNTO" | b"COED" | b"DOFT" | b"SOFT" => template::INVENTORY,
        b"VMAD" => template::SCRIPT,
        b"SPOR" | b"OCOR" | b"GWOR" | b"ECOR" | b"DPLT" => template::DEF_PACK_LIST,
        b"ATKR" | b"ATKD" | b"ATKE" => template::ATTACK_DATA,
        b"KSIZ" | b"KWDA" => template::KEYWORDS,
        _ => return None,
    })
}

struct Builder<'a> {
    world: &'a World,
    value: RecordOverride,
    used: BTreeSet<u32>,
}

impl World {
    fn selected_component(
        &self,
        chain: &[GlobalFormId],
        group: u16,
    ) -> Result<GlobalFormId, Error> {
        for &id in chain {
            let npc = self.npc_record(id)?;
            if !npc.uses_template(group) || npc.template == Some(0) {
                return Ok(id);
            }
        }
        Err(invalid("incomplete temporary NPC component chain"))
    }

    pub(super) fn materialize_npc(
        &self,
        actor: GlobalFormId,
        id: GlobalFormId,
        chain: &[GlobalFormId],
    ) -> Result<RecordOverride, Error> {
        let origin = *chain
            .first()
            .ok_or_else(|| invalid("empty temporary NPC chain"))?;
        let source = self
            .record(origin)
            .ok_or_else(|| invalid("missing temporary NPC origin"))?;
        let mut value = RecordOverride::new(origin, source.clone());
        value.temporary_for = Some(actor);
        value.record.form_id = 0xFF00_0000 | id.object;
        value.record.subrecords.retain(|sub| {
            group(sub.tag).is_none() && !matches!(sub.tag.as_bytes(), b"TPLT" | b"ACBS" | b"DNAM")
        });
        let mut sources = BTreeMap::new();
        let mut used = BTreeSet::new();
        for &source in chain {
            used.extend(raw_words(
                self.record(source)
                    .ok_or_else(|| invalid("missing NPC source"))?,
            ));
        }
        for bit in 0..13 {
            let flag = 1 << bit;
            if flag != 0x40 {
                sources.insert(flag, self.selected_component(chain, flag)?);
            }
        }
        let mut builder = Builder {
            world: self,
            value,
            used,
        };
        for (&part, &source) in &sources {
            let record = self
                .record(source)
                .ok_or_else(|| invalid("missing NPC source"))?;
            for sub in record
                .subrecords
                .iter()
                .filter(|sub| group(sub.tag) == Some(part))
            {
                let mut sub = sub.clone();
                builder.relocate(source, &mut sub)?;
                builder.value.record.subrecords.push(sub);
            }
        }
        let stats = sources[&template::STATS];
        let traits = sources[&template::TRAITS];
        let base = sources[&template::BASE_DATA];
        let inventory = sources[&template::INVENTORY];
        let acbs = |source| -> Result<Vec<u8>, Error> {
            self.record(source)
                .and_then(|r| r.find(T(b"ACBS")))
                .filter(|data| data.len() == 24)
                .map(<[u8]>::to_vec)
                .ok_or_else(|| invalid("temporary NPC needs complete ACBS"))
        };
        let mut config = acbs(origin)?;
        let stats_config = acbs(stats)?;
        config[4..16].copy_from_slice(&stats_config[4..16]);
        config[20..24].copy_from_slice(&stats_config[20..24]);
        let mut flags = super::le_u32(&config).ok_or_else(|| invalid("missing NPC flags"))?;
        // Group flag membership follows UESP's template descriptions.
        for (source, mask) in [
            (traits, 0x0009_0005),
            (stats, 0x0004_0090),
            (base, 0x0010_484A),
        ] {
            let source_flags =
                super::le_u32(&acbs(source)?).ok_or_else(|| invalid("missing NPC flags"))?;
            flags = (flags & !mask) | (source_flags & mask);
        }
        config[..4].copy_from_slice(&flags.to_le_bytes());
        let copied = u16::from_le_bytes([config[18], config[19]]) | 0x8000;
        config[18..20].copy_from_slice(&copied.to_le_bytes());
        builder.value.set(T(b"ACBS"), config);
        if let Some(data) = self.record(stats).and_then(|r| r.find(T(b"DNAM"))) {
            if data.len() != 52 {
                return Err(invalid("temporary NPC needs complete DNAM"));
            }
            let mut skills = data.to_vec();
            for (source, range) in [(traits, 44..48), (inventory, 48..49)] {
                if let Some(data) = self.record(source).and_then(|r| r.find(T(b"DNAM"))) {
                    if data.len() != 52 {
                        return Err(invalid("temporary NPC needs complete DNAM"));
                    }
                    skills[range.clone()].copy_from_slice(&data[range]);
                }
            }
            builder.value.set(T(b"DNAM"), skills);
        }
        builder.value.text_origin = self
            .record_overrides
            .get(&base)
            .map_or(base, |r| r.text_origin);
        builder.value.plain_full = !self.record_text_localized(base, T(b"FULL"));
        if let Some(extra) = self.restored_npc_extra(traits) {
            builder.value.npc.original_race = extra.original_race;
            builder.value.npc.body_tint = extra.body_tint;
        }
        Ok(builder.value)
    }

    /// The private base of a leveled actor, or the authored base for other actors.
    ///
    /// # Errors
    /// Malformed inherited data or exhausted identities do not install a partial base.
    pub fn leveled_actor_base(
        &mut self,
        actor: GlobalFormId,
    ) -> Result<Option<GlobalFormId>, Error> {
        let base = self
            .reference(actor)
            .filter(|r| r.is_actor)
            .and_then(|r| r.base);
        let Some(choice) = self.actor_templates.get(&actor) else {
            return Ok(base);
        };
        if let Some(id) = choice.temporary_base {
            return Ok(Some(id));
        }
        let Some(chain) = &choice.chain else {
            return Ok(None);
        };
        let next = self
            .next_dynamic
            .checked_add(1)
            .filter(|n| *n <= 0xFFFFFF)
            .ok_or_else(|| invalid("temporary NPC identity space exhausted"))?;
        let id = GlobalFormId {
            file: DYNAMIC_FILE,
            object: next,
        };
        if self.exists(id) {
            return Err(invalid("temporary NPC identity collision"));
        }
        let value = self.materialize_npc(actor, id, chain)?;
        self.record_overrides.insert(id, value);
        self.next_dynamic = next;
        if let Some(choice) = self.actor_templates.get_mut(&actor) {
            choice.temporary_base = Some(id);
        }
        Ok(Some(id))
    }
}
