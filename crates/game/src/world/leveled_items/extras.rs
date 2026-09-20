// SPDX-License-Identifier: GPL-3.0-or-later

//! xEdit COED layout and CommonLib extra-data types. Runtime application follows
//! the installed SkyrimSE ContainerItemExtra and multi-item selection routines.

use soe_esm::{Error, GlobalFormId};

use super::{le_u32, World, T};
use crate::inventory::{Extra, Field};

impl World {
    pub(super) fn leveled_item_extras(&self, list: GlobalFormId) -> Result<Vec<Vec<Extra>>, Error> {
        let record = self.record(list).ok_or_else(|| invalid("missing list"))?;
        let mut entries = Vec::new();
        let mut previous = None;
        for sub in &record.subrecords {
            if sub.tag == T(b"LVLO") {
                let item = le_u32(&sub.data[4..]).and_then(|raw| self.resolve_from(list, raw));
                entries.push(Vec::new());
                previous = item;
            } else if sub.tag == T(b"COED") {
                let item = previous
                    .take()
                    .ok_or_else(|| invalid("orphan or duplicate COED"))?;
                let extra = self.item_extra_fields(list, item, &sub.data)?;
                let entry = entries.last_mut().ok_or_else(|| invalid("orphan COED"))?;
                *entry = extra;
            } else {
                previous = None;
            }
        }
        Ok(entries)
    }

    fn item_extra_fields(
        &self,
        list: GlobalFormId,
        item: GlobalFormId,
        bytes: &[u8],
    ) -> Result<Vec<Extra>, Error> {
        if bytes.len() != 12 {
            return Err(invalid("invalid COED length"));
        }
        let owner = le_u32(bytes).ok_or_else(|| invalid("missing owner"))?;
        let conditional = le_u32(&bytes[4..]).ok_or_else(|| invalid("missing conditional"))?;
        let health = le_u32(&bytes[8..]).ok_or_else(|| invalid("missing health"))?;
        let condition = f32::from_bits(health);
        if !condition.is_finite() {
            return Err(invalid("invalid condition multiplier"));
        }
        let mut extras = Vec::new();
        if owner != 0 {
            let owner = self
                .resolve_from(list, owner)
                .ok_or_else(|| invalid("unresolved owner"))?;
            let tag = self
                .record(owner)
                .filter(|r| !r.is_deleted())
                .map(|r| r.tag);
            if tag != Some(T(b"NPC_")) && tag != Some(T(b"FACT")) {
                return Err(invalid("owner must be NPC_ or FACT"));
            }
            extras.push(Extra {
                kind: 33,
                fields: vec![Field::Form(Some(owner))],
            });
            if tag == Some(T(b"NPC_")) && conditional != 0 {
                let global = self
                    .resolve_from(list, conditional)
                    .filter(|id| {
                        self.record(*id)
                            .is_some_and(|r| !r.is_deleted() && r.tag == T(b"GLOB"))
                    })
                    .ok_or_else(|| invalid("invalid ownership global"))?;
                extras.push(Extra {
                    kind: 34,
                    fields: vec![Field::Form(Some(global))],
                });
            } else if tag == Some(T(b"FACT")) && conditional != u32::MAX {
                extras.push(Extra {
                    kind: 35,
                    fields: vec![Field::Word(conditional)],
                });
            }
        }
        if (self.tag_of(item) == Some(T(b"WEAP")) || self.tag_of(item) == Some(T(b"ARMO")))
            && condition != -1.0
            && !(condition > 0.9999 && condition < 1.0001)
        {
            extras.push(Extra {
                kind: 37,
                fields: vec![Field::FloatBits(health)],
            });
        }
        Ok(extras)
    }
}

fn invalid(message: &str) -> Error {
    Error::Invalid(format!("LVLI metadata: {message}"))
}
