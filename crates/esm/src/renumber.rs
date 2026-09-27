// SPDX-License-Identifier: GPL-3.0-or-later

//! FormID remapping for plugin identities and the typed reference fields the
//! reader currently models.

use std::collections::HashMap;

use crate::error::{Error, Result};
use crate::plugin::{Group, GroupKind, Plugin};
use crate::tag::Tag;

pub(crate) fn remap_plugin(plugin: &mut Plugin, mapping: &HashMap<u32, u32>) -> Result<()> {
    if mapping.is_empty() {
        return Ok(());
    }
    let mut updated = plugin.clone();
    for record in &mut updated.records {
        record.form_id = remap(record.form_id, mapping);
        record.placement.world = record.placement.world.map(|id| remap(id, mapping));
        record.placement.cell = record.placement.cell.map(|id| remap(id, mapping));
        for subrecord in &mut record.subrecords {
            if subrecord.tag == Tag::new(b"VMAD") {
                remap_vmad(&mut subrecord.data, mapping)?;
            } else if form_id_field(record.tag, subrecord.tag) {
                map_words(&mut subrecord.data, mapping);
            }
        }
    }
    for subrecord in &mut updated.header_record.subrecords {
        if subrecord.tag == Tag::new(b"ONAM") {
            map_words(&mut subrecord.data, mapping);
        }
    }
    for group in &mut updated.groups {
        remap_group(group, mapping);
    }
    updated.header.overridden_forms = updated
        .header
        .overridden_forms
        .iter()
        .map(|id| remap(*id, mapping))
        .collect();
    *plugin = updated;
    Ok(())
}

fn remap_group(group: &mut Group, mapping: &HashMap<u32, u32>) {
    group.kind = match group.kind {
        GroupKind::WorldChildren(id) => GroupKind::WorldChildren(remap(id, mapping)),
        GroupKind::CellChildren(id) => GroupKind::CellChildren(remap(id, mapping)),
        GroupKind::CellPersistent(id) => GroupKind::CellPersistent(remap(id, mapping)),
        GroupKind::CellTemporary(id) => GroupKind::CellTemporary(remap(id, mapping)),
        GroupKind::CellVisibleDistant(id) => GroupKind::CellVisibleDistant(remap(id, mapping)),
        GroupKind::TopicChildren(id) => GroupKind::TopicChildren(remap(id, mapping)),
        other => other,
    };
    for entry in &mut group.entries {
        if let crate::plugin::Entry::Group(child) = entry {
            remap_group(child, mapping);
        }
    }
}

fn remap(id: u32, mapping: &HashMap<u32, u32>) -> u32 {
    mapping.get(&id).copied().unwrap_or(id)
}

fn map_words(bytes: &mut [u8], mapping: &HashMap<u32, u32>) {
    for chunk in bytes.as_chunks_mut::<4>().0 {
        let old = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if let Some(new) = mapping.get(&old) {
            chunk.copy_from_slice(&new.to_le_bytes());
        }
    }
}

// These pairs are the FormID fields exposed by the typed ESM record views.
fn form_id_field(record: Tag, field: Tag) -> bool {
    let scalar: &[(&[u8; 4], &[u8; 4])] = &[
        (b"CELL", b"LTMP"),
        (b"CELL", b"XCWT"),
        (b"CELL", b"XOWN"),
        (b"CELL", b"XLCN"),
        (b"CELL", b"XCIM"),
        (b"CELL", b"XCMO"),
        (b"CELL", b"XCAS"),
        (b"CELL", b"XEZN"),
        (b"CELL", b"XCCM"),
        (b"WRLD", b"WNAM"),
        (b"WRLD", b"CNAM"),
        (b"WRLD", b"NAM2"),
        (b"WRLD", b"NAM3"),
        (b"WRLD", b"XLCN"),
        (b"WRLD", b"XEZN"),
        (b"WRLD", b"ZNAM"),
        (b"LCTN", b"PNAM"),
        (b"LCTN", b"MNAM"),
        (b"REFR", b"NAME"),
        (b"REFR", b"XOWN"),
        (b"REFR", b"XLCN"),
        (b"REFR", b"XLYR"),
        (b"REFR", b"XEZN"),
        (b"REFR", b"XEMI"),
        (b"LTEX", b"TNAM"),
        (b"NAVM", b"ONAM"),
        (b"SCEN", b"PNAM"),
        (b"DIAL", b"QNAM"),
        (b"INFO", b"TPIC"),
        (b"INFO", b"PNAM"),
        (b"INFO", b"DNAM"),
        (b"INFO", b"ANAM"),
        (b"INFO", b"TWAT"),
        (b"INFO", b"ONAM"),
        (b"INFO", b"SNAM"),
        (b"INFO", b"LNAM"),
        (b"MGEF", b"KWDA"),
        (b"MGEF", b"ESCE"),
        (b"INFO", b"TCLT"),
    ];
    scalar
        .iter()
        .any(|(rec, sub)| record.as_bytes() == *rec && field.as_bytes() == *sub)
        || (record == Tag::new(b"LTEX") && field == Tag::new(b"GNAM"))
        || (record == Tag::new(b"CELL") && field == Tag::new(b"XCLR"))
        || (record == Tag::new(b"MGEF")
            && (field == Tag::new(b"KWDA") || field == Tag::new(b"ESCE")))
        || (record == Tag::new(b"INFO") && field == Tag::new(b"TCLT"))
}

fn remap_vmad(data: &mut [u8], mapping: &HashMap<u32, u32>) -> Result<()> {
    let mut cursor = VmadCursor { data, pos: 0 };
    cursor.skip(2)?; // VMAD version
    let object_format = cursor.u16()?;
    if object_format != 1 && object_format != 2 {
        return Err(Error::Invalid(format!(
            "unsupported VMAD object format {object_format}"
        )));
    }
    let scripts = cursor.u16()?;
    let mut offsets = Vec::new();
    for _ in 0..scripts {
        cursor.string()?;
        cursor.skip(1)?; // script flags
        let properties = cursor.u16()?;
        for _ in 0..properties {
            cursor.string()?;
            let kind = cursor.u8()?;
            cursor.skip(1)?; // property flags
            match kind {
                0 => {}
                1 => object(&mut cursor, object_format, &mut offsets)?,
                2 => {
                    cursor.string()?;
                }
                3 | 4 => cursor.skip(4)?,
                5 => cursor.skip(1)?,
                11 => {
                    let count = cursor.u32()?;
                    for _ in 0..count {
                        object(&mut cursor, object_format, &mut offsets)?;
                    }
                }
                12 => {
                    let count = cursor.u32()?;
                    for _ in 0..count {
                        cursor.string()?;
                    }
                }
                13 | 14 => {
                    let count = cursor.u32()?;
                    cursor.skip(
                        count
                            .checked_mul(4)
                            .ok_or_else(|| Error::Invalid("VMAD array size overflow".into()))?,
                    )?;
                }
                15 => {
                    let count = cursor.u32()?;
                    cursor.skip(count)?;
                }
                other => {
                    return Err(Error::Invalid(format!(
                        "unknown VMAD property type {other}"
                    )))
                }
            }
        }
    }
    let VmadCursor { data, .. } = cursor;
    for at in offsets {
        let old = u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
        if let Some(new) = mapping.get(&old) {
            data[at..at + 4].copy_from_slice(&new.to_le_bytes());
        }
    }
    Ok(())
}

fn object(cursor: &mut VmadCursor<'_>, format: u16, offsets: &mut Vec<usize>) -> Result<()> {
    let link = if format == 2 {
        cursor.skip(4)?;
        let at = cursor.pos;
        cursor.skip(4)?;
        at
    } else {
        let at = cursor.pos;
        cursor.skip(4)?;
        cursor.skip(4)?;
        at
    };
    offsets.push(link);
    Ok(())
}

struct VmadCursor<'a> {
    data: &'a mut [u8],
    pos: usize,
}

impl VmadCursor<'_> {
    fn skip(&mut self, amount: usize) -> Result<()> {
        self.pos = self
            .pos
            .checked_add(amount)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| Error::Invalid("truncated VMAD data".into()))?;
        Ok(())
    }
    fn u8(&mut self) -> Result<u8> {
        let value = *self
            .data
            .get(self.pos)
            .ok_or_else(|| Error::Invalid("truncated VMAD data".into()))?;
        self.skip(1)?;
        Ok(value)
    }
    fn u16(&mut self) -> Result<u16> {
        let end = self
            .pos
            .checked_add(2)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| Error::Invalid("truncated VMAD data".into()))?;
        let value = u16::from_le_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos = end;
        Ok(value)
    }
    fn u32(&mut self) -> Result<usize> {
        let end = self
            .pos
            .checked_add(4)
            .filter(|end| *end <= self.data.len())
            .ok_or_else(|| Error::Invalid("truncated VMAD data".into()))?;
        let value = u32::from_le_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]) as usize;
        self.pos = end;
        Ok(value)
    }
    fn string(&mut self) -> Result<()> {
        let len = self.u16()? as usize;
        self.skip(len)
    }
}
