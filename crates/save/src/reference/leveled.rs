// SPDX-License-Identifier: GPL-3.0-or-later

//! ExtraLeveledCreature links and inline NPC changes. CommonLib names the
//! original/template bases, and xEdit/UESP define their saved representation.

use super::{extra::Extra, fields::invalid};
use crate::{cursor::Cursor, npc::NpcChange, ChangeForm, RefId, Result};

#[derive(Debug, Clone)]
pub struct LeveledCreature {
    pub original_base: RefId,
    pub template_base: RefId,
    pub npc: NpcChange,
    /// End of this extra entry. Later extra data and actor state remain unread.
    pub bytes_consumed: usize,
}

impl ChangeForm {
    /// Read the saved leveled actor choice, stopping after ExtraLeveledCreature.
    /// This does not decode or validate the remaining extra data or inventory.
    ///
    /// # Errors
    /// Invalid prefix, unknown earlier extra layout or a missing declared entry.
    pub fn leveled_creature(&self, form_version: u8) -> Result<Option<LeveledCreature>> {
        if self.type_index() != 1 || self.change_flags & (1 << 18) == 0 {
            return Ok(None);
        }
        let header = self
            .reference_header(form_version)?
            .ok_or_else(|| invalid("missing actor header"))?;
        let payload = self.payload()?;
        let mut r = Cursor::new(&payload, "leveled actor extra data");
        r.seek(header.bytes_consumed, "leveled actor extra data")?;
        let count = r.vsval()?.value();
        if count as usize > r.remaining() {
            return Err(invalid("extra-data count exceeds remaining bytes"));
        }
        for _ in 0..count {
            let start = r.pos();
            if r.u8()? != 45 {
                r.seek(start, "leveled actor extra data")?;
                Extra::read(&mut r)?;
                continue;
            }
            let original_base = r.ref_id()?;
            let template_base = r.ref_id()?;
            let flags = r.u32()?;
            let (npc, used) = NpcChange::read_prefix(&payload[r.pos()..], flags)?;
            return Ok(Some(LeveledCreature {
                original_base,
                template_base,
                npc,
                bytes_consumed: r.pos() + used,
            }));
        }
        Err(invalid(
            "actor declares leveled data but has no leveled creature entry",
        ))
    }
}
