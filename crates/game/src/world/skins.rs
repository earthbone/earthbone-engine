// SPDX-License-Identifier: GPL-3.0-or-later

//! SKSE ActorBase/Race skin setters update WNAM, never inventory or equipment.

use soe_esm::cursor::Fields;
use soe_esm::{Error, GlobalFormId, Result, Tag};

use super::record_overrides::{raw_words, RecordOverride};
use super::{World, T};

impl World {
    /// Read the direct NPC or race skin. A null NPC skin falls back to the
    /// race only when assembling the actor, as in SKSE's direct GetSkin.
    pub fn skin(&self, owner: GlobalFormId) -> Result<Option<GlobalFormId>> {
        let record = self
            .record(owner)
            .filter(|record| matches!(&record.tag.0, b"NPC_" | b"RACE") && !record.is_deleted())
            .ok_or_else(|| {
                Error::Invalid(format!("skin owner is not a live NPC_ or RACE: {owner}"))
            })?;
        let Some(bytes) = record.find(T(b"WNAM")) else {
            return Ok(None);
        };
        let mut field = Fields::new(record.tag, T(b"WNAM"), bytes);
        field.exact(4)?;
        Ok(self.resolve_from(owner, field.u32()?))
    }

    pub fn set_actor_base_skin(
        &mut self,
        base: GlobalFormId,
        skin: Option<GlobalFormId>,
    ) -> Result<()> {
        self.set_skin(base, T(b"NPC_"), skin)
    }

    pub fn set_race_skin(&mut self, race: GlobalFormId, skin: Option<GlobalFormId>) -> Result<()> {
        self.set_skin(race, T(b"RACE"), skin)
    }

    fn set_skin(
        &mut self,
        owner: GlobalFormId,
        tag: Tag,
        skin: Option<GlobalFormId>,
    ) -> Result<()> {
        let record = self
            .record(owner)
            .filter(|record| record.tag == tag && !record.is_deleted())
            .ok_or_else(|| Error::Invalid(format!("skin owner is not a live {tag}: {owner}")))?;
        if let Some(skin) = skin {
            if self
                .record(skin)
                .is_none_or(|record| record.tag != T(b"ARMO") || record.is_deleted())
            {
                return Err(Error::Invalid(format!("skin is not a live ARMO: {skin}")));
            }
        }
        let mut value = self
            .record_overrides
            .get(&owner)
            .cloned()
            .unwrap_or_else(|| RecordOverride::new(owner, record.clone()));
        let mut used = raw_words(&value.record);
        if let Some(original) = self.order.winner(value.origin) {
            used.extend(raw_words(original));
        }
        used.extend(value.links.keys());
        let raw = value.link(&mut used, skin).map_err(Error::Invalid)?;
        value.set(T(b"WNAM"), raw.to_le_bytes());
        let present = raw_words(&value.record);
        value.links.retain(|raw, _| present.contains(raw));
        self.record_overrides.insert(owner, value);
        Ok(())
    }
}
