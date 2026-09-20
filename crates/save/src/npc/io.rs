// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded NPC field IO. xEdit keeps head parts outside the optional head block.

use super::{invalid, Face, Head, List, Morph};
use crate::cursor::Cursor;
use crate::{RefId, Result};

pub(super) fn optional<T>(
    r: &mut Cursor<'_>,
    flags: u32,
    flag: u32,
    read: impl FnOnce(&mut Cursor<'_>) -> Result<T>,
) -> Result<Option<T>> {
    if flags & flag != 0 {
        read(r).map(Some)
    } else {
        Ok(None)
    }
}

pub(super) fn bytes<const N: usize>(r: &mut Cursor<'_>) -> Result<[u8; N]> {
    let mut bytes = [0; N];
    bytes.copy_from_slice(r.take(N)?);
    Ok(bytes)
}

pub(super) fn list<T>(
    r: &mut Cursor<'_>,
    width: usize,
    read: impl Fn(&mut Cursor<'_>) -> Result<T>,
) -> Result<List<T>> {
    let count = r.vsval()?;
    if count.value() as usize > r.remaining() / width {
        return Err(invalid("NPC list count exceeds remaining bytes"));
    }
    let mut entries = Vec::with_capacity(count.value() as usize);
    for _ in 0..count.value() {
        entries.push(read(r)?);
    }
    Ok(List { count, entries })
}

pub(super) fn references(r: &mut Cursor<'_>) -> Result<List<RefId>> {
    list(r, 3, |r| r.ref_id())
}

fn words(r: &mut Cursor<'_>) -> Result<Vec<u32>> {
    let count = r.u32()? as usize;
    if count > r.remaining() / 4 {
        return Err(invalid("NPC morph count exceeds remaining bytes"));
    }
    (0..count).map(|_| r.u32()).collect()
}

fn write_words(out: &mut Vec<u8>, values: &[u32]) -> Result<()> {
    let count = u32::try_from(values.len()).map_err(|_| invalid("NPC morph list too long"))?;
    out.extend(count.to_le_bytes());
    for value in values {
        out.extend(value.to_le_bytes());
    }
    Ok(())
}

impl<T> List<T> {
    pub(super) fn write(&self, out: &mut Vec<u8>, write: impl Fn(&T, &mut Vec<u8>)) -> Result<()> {
        if self.count.value() as usize != self.entries.len() {
            return Err(invalid("NPC list count mismatch"));
        }
        self.count.write(out);
        for entry in &self.entries {
            write(entry, out);
        }
        Ok(())
    }
}

impl Face {
    pub(super) fn read(r: &mut Cursor<'_>) -> Result<Self> {
        let head_tag = r.u8()?;
        let head = if head_tag != 0 {
            Some(Head {
                hair_color: r.ref_id()?,
                skin_color: bytes(r)?,
                texture_set: r.ref_id()?,
            })
        } else {
            None
        };
        let head_parts = references(r)?;
        let morph_tag = r.u8()?;
        let morph = if morph_tag != 0 {
            Some(Morph {
                options: words(r)?,
                presets: words(r)?,
            })
        } else {
            None
        };
        Ok(Self {
            head_tag,
            head,
            head_parts,
            morph_tag,
            morph,
        })
    }

    pub(super) fn write(&self, out: &mut Vec<u8>) -> Result<()> {
        if (self.head_tag != 0) != self.head.is_some()
            || (self.morph_tag != 0) != self.morph.is_some()
        {
            return Err(invalid("NPC face tags disagree with field presence"));
        }
        out.push(self.head_tag);
        if let Some(head) = &self.head {
            head.hair_color.write(out);
            out.extend(head.skin_color);
            head.texture_set.write(out);
        }
        self.head_parts.write(out, |id, out| id.write(out))?;
        out.push(self.morph_tag);
        if let Some(morph) = &self.morph {
            write_words(out, &morph.options)?;
            write_words(out, &morph.presets)?;
        }
        Ok(())
    }
}
