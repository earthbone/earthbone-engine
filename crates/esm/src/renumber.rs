// SPDX-License-Identifier: GPL-3.0-or-later

//! FormID remapping for plugin identities and the typed reference fields the
//! reader currently models. Field layouts follow local TES5Edit definitions
//! and Mutagen's `Remapping-FormLinks.md` describes the link traversal contract.

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
                remap_vmad(
                    &mut subrecord.data,
                    mapping,
                    record.tag == Tag::new(b"QUST"),
                )?;
            } else if record.tag == Tag::new(b"HAZD") && subrecord.tag == Tag::new(b"DATA") {
                remap_hazd_data(&mut subrecord.data, mapping)?;
            } else if record.tag == Tag::new(b"MGEF") && subrecord.tag == Tag::new(b"DATA") {
                remap_mgef_data(&mut subrecord.data, mapping)?;
            } else if record.tag == Tag::new(b"WEAP") && subrecord.tag == Tag::new(b"CRDT") {
                remap_weap_critical_data(&mut subrecord.data, mapping)?;
            } else if subrecord.tag == Tag::new(b"DSTD") {
                remap_destruction_stage(record.tag, &mut subrecord.data, mapping)?;
            } else if record.tag == Tag::new(b"INFO") && subrecord.tag == Tag::new(b"TRDT") {
                remap_info_response(&mut subrecord.data, mapping)?;
            } else if record.tag == Tag::new(b"COBJ") {
                remap_cobj_field(subrecord.tag, &mut subrecord.data, mapping)?;
            } else if subrecord.tag == Tag::new(b"CTDA") {
                remap_ctda(&mut subrecord.data, mapping)?;
            } else if record.tag == Tag::new(b"REFR") {
                remap_refr_field(subrecord.tag, &mut subrecord.data, mapping)?;
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

fn remap_info_response(data: &mut [u8], mapping: &HashMap<u32, u32>) -> Result<()> {
    if data.len() != 24 {
        return Err(Error::SubrecordSize {
            record: Tag::new(b"INFO"),
            tag: Tag::new(b"TRDT"),
            len: data.len(),
            expected: "24".into(),
        });
    }
    let old = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
    if let Some(new) = mapping.get(&old) {
        data[16..20].copy_from_slice(&new.to_le_bytes());
    }
    Ok(())
}

fn remap_cobj_field(tag: Tag, data: &mut [u8], mapping: &HashMap<u32, u32>) -> Result<()> {
    let offset = match tag.as_bytes() {
        b"CNAM" | b"BNAM" => {
            if data.len() != 4 {
                return Err(Error::SubrecordSize {
                    record: Tag::new(b"COBJ"),
                    tag,
                    len: data.len(),
                    expected: "4".into(),
                });
            }
            0
        }
        b"CNTO" => {
            if data.len() != 8 {
                return Err(Error::SubrecordSize {
                    record: Tag::new(b"COBJ"),
                    tag,
                    len: data.len(),
                    expected: "8".into(),
                });
            }
            0
        }
        b"COED" => {
            if data.len() != 12 {
                return Err(Error::SubrecordSize {
                    record: Tag::new(b"COBJ"),
                    tag,
                    len: data.len(),
                    expected: "12".into(),
                });
            }
            0
        }
        _ => return Ok(()),
    };
    let old = u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .map_err(|_| Error::Invalid("COBJ FormID offset exceeds subrecord".into()))?,
    );
    if let Some(new) = mapping.get(&old) {
        data[offset..offset + 4].copy_from_slice(&new.to_le_bytes());
    }
    Ok(())
}

fn remap_ctda(data: &mut [u8], mapping: &HashMap<u32, u32>) -> Result<()> {
    if data.len() != 32 {
        return Err(Error::SubrecordSize {
            record: Tag::new(b"CTDA"),
            tag: Tag::new(b"CTDA"),
            len: data.len(),
            expected: "32".into(),
        });
    }
    // wbConditions uses the comparison union as a GLOB only when flag 0x04 is set.
    let mut offsets = Vec::new();
    if data[0] & 4 != 0 {
        offsets.push(4);
    }
    // Run On value 2 stores a reference FormID in the following field.
    if u32::from_le_bytes(
        data[20..24]
            .try_into()
            .map_err(|_| Error::Invalid("CTDA Run On offset exceeds subrecord".into()))?,
    ) == 2
    {
        offsets.push(24);
    }
    for offset in offsets {
        let old = u32::from_le_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|_| Error::Invalid("CTDA FormID offset exceeds subrecord".into()))?,
        );
        if let Some(new) = mapping.get(&old) {
            data[offset..offset + 4].copy_from_slice(&new.to_le_bytes());
        }
    }
    Ok(())
}

fn remap_refr_field(tag: Tag, data: &mut [u8], mapping: &HashMap<u32, u32>) -> Result<()> {
    let offsets: &[usize] = match tag.as_bytes() {
        b"XESP" | b"XTEL" | b"XNDP" => &[0],
        b"XLOC" => &[4],
        b"XLKR" if data.len() == 4 => &[0],
        b"XLKR" => &[0, 4],
        _ => {
            if form_id_field(Tag::new(b"REFR"), tag) {
                map_words(data, mapping);
            }
            return Ok(());
        }
    };
    let valid = match tag.as_bytes() {
        b"XESP" | b"XNDP" => data.len() == 8,
        b"XTEL" => matches!(data.len(), 28 | 32),
        b"XLOC" => matches!(data.len(), 9 | 12 | 20),
        b"XLKR" => matches!(data.len(), 4 | 8),
        _ => true,
    };
    if !valid {
        return Err(Error::SubrecordSize {
            record: Tag::new(b"REFR"),
            tag,
            len: data.len(),
            expected: "valid REFR reference field size".into(),
        });
    }
    for &offset in offsets {
        let old = u32::from_le_bytes(
            data[offset..offset + 4]
                .try_into()
                .map_err(|_| Error::Invalid("REFR reference offset exceeds subrecord".into()))?,
        );
        if let Some(new) = mapping.get(&old) {
            data[offset..offset + 4].copy_from_slice(&new.to_le_bytes());
        }
    }
    Ok(())
}

fn remap_hazd_data(data: &mut [u8], mapping: &HashMap<u32, u32>) -> Result<()> {
    if data.len() != 40 {
        return Err(Error::SubrecordSize {
            record: Tag::new(b"HAZD"),
            tag: Tag::new(b"DATA"),
            len: data.len(),
            expected: "40".into(),
        });
    }
    for chunk in data[24..].as_chunks_mut::<4>().0 {
        let old = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        if let Some(new) = mapping.get(&old) {
            chunk.copy_from_slice(&new.to_le_bytes());
        }
    }
    Ok(())
}

fn remap_mgef_data(data: &mut [u8], mapping: &HashMap<u32, u32>) -> Result<()> {
    if data.len() != 152 {
        return Err(Error::SubrecordSize {
            record: Tag::new(b"MGEF"),
            tag: Tag::new(b"DATA"),
            len: data.len(),
            expected: "152".into(),
        });
    }
    for offset in [
        8, 24, 32, 36, 72, 76, 92, 96, 100, 108, 116, 120, 124, 128, 132, 136,
    ] {
        let value = u32::from_le_bytes(data[offset..offset + 4].try_into().map_err(|_| {
            Error::SubrecordSize {
                record: Tag::new(b"MGEF"),
                tag: Tag::new(b"DATA"),
                len: data.len(),
                expected: "152".into(),
            }
        })?);
        if let Some(mapped) = mapping.get(&value) {
            data[offset..offset + 4].copy_from_slice(&mapped.to_le_bytes());
        }
    }
    Ok(())
}

fn remap_weap_critical_data(data: &mut [u8], mapping: &HashMap<u32, u32>) -> Result<()> {
    let offset = match data.len() {
        20 => 12,
        24 => 16,
        _ => {
            return Err(Error::SubrecordSize {
                record: Tag::new(b"WEAP"),
                tag: Tag::new(b"CRDT"),
                len: data.len(),
                expected: "20 or 24".into(),
            });
        }
    };
    let old = u32::from_le_bytes(data[offset..offset + 4].try_into().map_err(|_| {
        Error::SubrecordSize {
            record: Tag::new(b"WEAP"),
            tag: Tag::new(b"CRDT"),
            len: data.len(),
            expected: "20 or 24".into(),
        }
    })?);
    if let Some(new) = mapping.get(&old) {
        data[offset..offset + 4].copy_from_slice(&new.to_le_bytes());
    }
    Ok(())
}

fn remap_destruction_stage(
    record: Tag,
    data: &mut [u8],
    mapping: &HashMap<u32, u32>,
) -> Result<()> {
    if data.len() != 20 {
        return Err(Error::SubrecordSize {
            record,
            tag: Tag::new(b"DSTD"),
            len: data.len(),
            expected: "20".into(),
        });
    }
    for offset in [8, 12] {
        let old = u32::from_le_bytes(data[offset..offset + 4].try_into().map_err(|_| {
            Error::SubrecordSize {
                record,
                tag: Tag::new(b"DSTD"),
                len: data.len(),
                expected: "20".into(),
            }
        })?);
        if let Some(new) = mapping.get(&old) {
            data[offset..offset + 4].copy_from_slice(&new.to_le_bytes());
        }
    }
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
        (b"REFR", b"XTNM"),
        (b"REFR", b"XMBR"),
        (b"REFR", b"XLRT"),
        (b"REFR", b"XLRL"),
        (b"REFR", b"INAM"),
        (b"LTEX", b"TNAM"),
        (b"WEAP", b"EITM"),
        (b"WEAP", b"BIDS"),
        (b"WEAP", b"BAMT"),
        (b"WEAP", b"EFSD"),
        (b"WEAP", b"INAM"),
        (b"WEAP", b"WNAM"),
        (b"WEAP", b"SNAM"),
        (b"WEAP", b"XNAM"),
        (b"WEAP", b"NAM7"),
        (b"WEAP", b"TNAM"),
        (b"WEAP", b"UNAM"),
        (b"WEAP", b"NAM9"),
        (b"WEAP", b"NAM8"),
        (b"WEAP", b"CNAM"),
        (b"WEAP", b"ETYP"),
        (b"WEAP", b"YNAM"),
        (b"WEAP", b"ZNAM"),
        (b"ARMO", b"EITM"),
        (b"ARMO", b"RNAM"),
        (b"ARMO", b"TNAM"),
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
        (b"HAZD", b"MNAM"),
        (b"INFO", b"TCLT"),
        (b"COBJ", b"CNAM"),
        (b"COBJ", b"BNAM"),
    ];
    scalar
        .iter()
        .any(|(rec, sub)| record.as_bytes() == *rec && field.as_bytes() == *sub)
        || (record == Tag::new(b"LTEX") && field == Tag::new(b"GNAM"))
        || (record == Tag::new(b"CELL") && field == Tag::new(b"XCLR"))
        || (field == Tag::new(b"KWDA"))
        || (record == Tag::new(b"MGEF") && field == Tag::new(b"ESCE"))
        || (record == Tag::new(b"INFO") && field == Tag::new(b"TCLT"))
}

fn remap_vmad(data: &mut [u8], mapping: &HashMap<u32, u32>, quest_fragments: bool) -> Result<()> {
    let mut cursor = VmadCursor { data, pos: 0 };
    cursor.skip(2)?; // VMAD version
    let object_format = cursor.u16()?;
    if object_format != 1 && object_format != 2 {
        return Err(Error::Invalid(format!(
            "unsupported VMAD object format {object_format}"
        )));
    }
    let mut offsets = Vec::new();
    remap_scripts(&mut cursor, object_format, &mut offsets)?;
    if quest_fragments && cursor.pos < cursor.data.len() {
        remap_quest_fragments(&mut cursor, object_format, &mut offsets)?;
        if cursor.pos != cursor.data.len() {
            return Err(Error::Invalid("trailing bytes in QUST VMAD data".into()));
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

fn remap_scripts(
    cursor: &mut VmadCursor<'_>,
    object_format: u16,
    offsets: &mut Vec<usize>,
) -> Result<()> {
    let scripts = cursor.u16()?;
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
                1 => object(cursor, object_format, offsets)?,
                2 => cursor.string()?,
                3 | 4 => cursor.skip(4)?,
                5 => cursor.skip(1)?,
                11 => {
                    let count = cursor.u32()?;
                    for _ in 0..count {
                        object(cursor, object_format, offsets)?;
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
    Ok(())
}

fn remap_quest_fragments(
    cursor: &mut VmadCursor<'_>,
    object_format: u16,
    offsets: &mut Vec<usize>,
) -> Result<()> {
    cursor.skip(1)?; // extra bind data version
    let fragments = cursor.u16()?;
    cursor.string()?; // fragment file name
    for _ in 0..fragments {
        cursor.skip(9)?; // stage, log entry and fragment flags
        cursor.string()?; // script name
        cursor.string()?; // fragment name
    }
    let aliases = cursor.u16()?;
    for _ in 0..aliases {
        object(cursor, object_format, offsets)?;
        cursor.skip(2)?; // alias VMAD version
        let alias_object_format = cursor.u16()?;
        if alias_object_format != 1 && alias_object_format != 2 {
            return Err(Error::Invalid(format!(
                "unsupported VMAD alias object format {alias_object_format}"
            )));
        }
        remap_scripts(cursor, alias_object_format, offsets)?;
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
