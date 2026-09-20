// SPDX-License-Identifier: GPL-3.0-or-later

//! Extra-data field view of the shared NPC decoder. RefIDs remain typed so
//! inventory import can map them, and saved count widths survive re-encoding.

use super::fields::{Field, FieldArray};
use crate::{cursor::Cursor, npc::List, npc::NpcChange, RefId, Result};

fn list<T>(values: List<T>, entry: impl FnMut(T) -> Vec<Field>) -> Field {
    Field::Array(FieldArray::variable(
        values.count,
        values.entries.into_iter().map(entry).collect(),
    ))
}

fn references(values: List<RefId>) -> Field {
    list(values, |id| vec![Field::Reference(id)])
}

pub(super) fn read(r: &mut Cursor<'_>, flags: u32) -> Result<Vec<Field>> {
    let start = r.pos();
    let (npc, used) = NpcChange::read_prefix(r.rest(), flags)?;
    r.seek(start + used, "inline NPC fields")?;
    let mut out = vec![];
    if let Some((flags, unknown)) = npc.form_flags {
        out.extend([Field::Word(flags), Field::Short(unknown)]);
    }
    if let Some(bytes) = npc.base {
        out.push(Field::Bytes(bytes.to_vec()));
    }
    if let Some(values) = npc.factions {
        out.push(list(values, |entry| {
            vec![Field::Reference(entry.form), Field::Byte(entry.rank as u8)]
        }));
    }
    if let Some(spells) = npc.spells {
        out.extend([
            references(spells.spells),
            references(spells.leveled_spells),
            references(spells.shouts),
        ]);
    }
    if let Some(bytes) = npc.ai {
        out.push(Field::Bytes(bytes.to_vec()));
    }
    if let Some(name) = npc.name {
        out.push(Field::String(name));
    }
    if let Some(bytes) = npc.skills {
        out.push(Field::Bytes(bytes.to_vec()));
    }
    if let Some(class) = npc.class {
        out.push(Field::Reference(class));
    }
    if let Some((new, old)) = npc.race {
        out.extend([Field::Reference(new), Field::Reference(old)]);
    }
    if let Some(face) = npc.face {
        out.push(Field::Byte(face.head_tag));
        if let Some(head) = face.head {
            out.extend([
                Field::Reference(head.hair_color),
                Field::Word(u32::from_le_bytes(head.skin_color)),
                Field::Reference(head.texture_set),
            ]);
        }
        out.push(references(face.head_parts));
        out.push(Field::Byte(face.morph_tag));
        if let Some(morph) = face.morph {
            out.push(Field::Array(FieldArray::fixed(
                morph
                    .options
                    .into_iter()
                    .map(|bits| vec![Field::Float(f32::from_bits(bits))])
                    .collect(),
            )?));
            out.push(Field::Array(FieldArray::fixed(
                morph
                    .presets
                    .into_iter()
                    .map(|v| vec![Field::Word(v)])
                    .collect(),
            )?));
        }
    }
    if let Some(gender) = npc.gender {
        out.push(Field::Byte(gender));
    }
    for outfit in [npc.default_outfit, npc.sleep_outfit].into_iter().flatten() {
        out.push(Field::Reference(outfit));
    }
    Ok(out)
}
