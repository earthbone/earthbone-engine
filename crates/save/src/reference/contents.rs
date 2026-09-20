// SPDX-License-Identifier: GPL-3.0-or-later

//! Saved reference extra data and inventory deltas, using xEdit's three-level
//! entry/stack/extra nesting. Actor execution and animation remain separate.

use super::{fields::invalid, ExtraData};
use crate::cursor::{Cursor, VsVal};
use crate::{ChangeForm, RefId, Result};

/// An entry changes the base container count and may describe distinct stacks.
#[derive(Clone, Debug)]
pub struct InventoryEntry {
    pub item: RefId,
    pub delta: i32,
    count: VsVal,
    pub stacks: Vec<ExtraData>,
}

#[derive(Clone, Debug)]
pub struct Inventory {
    count: VsVal,
    pub entries: Vec<InventoryEntry>,
}

#[derive(Clone, Debug)]
pub struct ReferenceContents {
    pub extra: Option<ExtraData>,
    pub inventory: Option<Inventory>,
    /// Start of this section, after the placement header.
    pub start: usize,
    /// End of inventory. Following animation/actor fields remain unapplied.
    pub end: usize,
    pub remaining_bytes: usize,
}

impl Inventory {
    fn read(r: &mut Cursor<'_>) -> Result<Self> {
        let count = r.vsval()?;
        if count.value() as usize > r.remaining() / 8 {
            return Err(invalid("inventory count exceeds remaining bytes"));
        }
        let mut entries = Vec::with_capacity(count.value() as usize);
        for _ in 0..count.value() {
            let item = r.ref_id()?;
            let delta = r.u32()? as i32;
            let stack_count = r.vsval()?;
            if stack_count.value() as usize > r.remaining() {
                return Err(invalid("stack count exceeds remaining bytes"));
            }
            let mut stacks = Vec::with_capacity(stack_count.value() as usize);
            for _ in 0..stack_count.value() {
                stacks.push(ExtraData::read(r)?);
            }
            entries.push(InventoryEntry {
                item,
                delta,
                count: stack_count,
                stacks,
            });
        }
        Ok(Self { count, entries })
    }

    fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        if self.count.value() as usize != self.entries.len() {
            return Err(invalid("inventory count mismatch"));
        }
        self.count.write(out);
        for entry in &self.entries {
            entry.item.write(out);
            out.extend(entry.delta.to_le_bytes());
            if entry.count.value() as usize != entry.stacks.len() {
                return Err(invalid("stack count mismatch"));
            }
            entry.count.write(out);
            for stack in &entry.stacks {
                stack.write(out)?;
            }
        }
        Ok(())
    }
}

fn read_extra(change: &ChangeForm, r: &mut Cursor<'_>) -> Result<Option<ExtraData>> {
    let common = (1 << 6)
        | (1 << 11)
        | (1 << 12)
        | (1 << 17)
        | (1 << 25)
        | (1 << 26)
        | (1 << 29)
        | (1 << 31);
    let extra_mask = if change.type_index() == 1 {
        common | (1 << 18) | (1 << 30)
    } else {
        common | (1 << 10)
    };
    if change.change_flags & extra_mask != 0 {
        Ok(Some(ExtraData::read(r)?))
    } else {
        Ok(None)
    }
}

impl ChangeForm {
    /// Decode only a REFR/ACHR top-level extra list. Inventory stack extras and
    /// later actor fields are separate sections and are not traversed here.
    ///
    /// # Errors
    /// Invalid placement headers, unknown extra layouts or truncated data.
    pub fn reference_extra_data(&self, form_version: u8) -> Result<Option<ExtraData>> {
        let Some(header) = self.reference_header(form_version)? else {
            return Ok(None);
        };
        let payload = self.payload()?;
        let mut r = Cursor::new(&payload, "reference extra data");
        r.seek(header.bytes_consumed, "reference extra data")?;
        read_extra(self, &mut r)
    }

    /// Decode extra data and inventory following a REFR/ACHR placement header.
    ///
    /// # Errors
    /// A bad header, unknown extra layout, truncated fields or impossible counts.
    /// Animation and the remaining actor payload are not decoded by this pass.
    pub fn reference_contents(&self, form_version: u8) -> Result<Option<ReferenceContents>> {
        let Some(header) = self.reference_header(form_version)? else {
            return Ok(None);
        };
        let payload = self.payload()?;
        let mut r = Cursor::new(&payload, "reference contents");
        r.seek(header.bytes_consumed, "reference contents")?;
        let extra = read_extra(self, &mut r)?;
        let inventory = if self.change_flags & ((1 << 5) | (1 << 27)) != 0 {
            Some(Inventory::read(&mut r)?)
        } else {
            None
        };
        Ok(Some(ReferenceContents {
            extra,
            inventory,
            start: header.bytes_consumed,
            end: r.pos(),
            remaining_bytes: r.remaining(),
        }))
    }
}

impl ReferenceContents {
    /// Rebuild the decoded section from typed fields, retaining count widths.
    ///
    /// # Errors
    /// An edited collection no longer matches its saved count.
    pub fn data_bytes(&self) -> Result<Vec<u8>> {
        let mut out = vec![];
        if let Some(extra) = &self.extra {
            extra.write(&mut out)?;
        }
        if let Some(inventory) = &self.inventory {
            inventory.write(&mut out)?;
        }
        Ok(out)
    }
}
