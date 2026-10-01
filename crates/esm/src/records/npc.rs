// SPDX-License-Identifier: GPL-3.0-or-later

//! The NPC face fields that can be edited without generating FaceGen assets.
//!
//! The NPC `PNAM` head-part list and 19-float `NAM9` face morph come from
//! `TES5Edit/Core/wbDefinitionsTES5.pas`. TiltedEvolution's `FaceGenSystem`
//! resolves a head-part form before finding its mesh node. The bundled nifly
//! source covers NIF morph data, but the PyNifly TRI source named by the editor
//! milestone is not present in this reference corpus. TRI mesh generation is
//! outside this record editor.

use crate::cursor::Fields;
use crate::error::{Error, Result};
use crate::load_order::LoadOrder;
use crate::plugin::{Record, Subrecord};
use crate::tag::Tag;

/// The NPC_ record signature.
pub const NPC: Tag = Tag(*b"NPC_");
/// Repeated head-part FormIDs on an NPC_ record.
pub const HEAD_PART: Tag = Tag(*b"PNAM");
/// The 19-float face morph block on an NPC_ record.
pub const FACE_MORPH: Tag = Tag(*b"NAM9");
/// Number of floats in the NPC face morph block.
pub const FACE_MORPH_COUNT: usize = 19;

const HEAD_PART_RECORD: Tag = Tag(*b"HDPT");

/// NPC face metadata that can be edited while preserving all other subrecords.
#[derive(Debug, Clone, PartialEq)]
pub struct NpcFace {
    /// The NPC FormID as written in its file.
    pub form_id: u32,
    /// Head-part FormIDs in PNAM order.
    pub head_parts: Vec<u32>,
    /// The NAM9 face morph values in TES5Edit field order.
    pub morph: [f32; FACE_MORPH_COUNT],
}

impl NpcFace {
    /// Read the head-part links and face morph from an NPC_ record.
    ///
    /// Missing PNAM or NAM9 fields are read as an empty list or zero values.
    ///
    /// # Errors
    ///
    /// Returns an error for a non-NPC record, malformed PNAM or NAM9 data, or
    /// non-finite morph values.
    pub fn from_record(record: &Record) -> Result<Self> {
        if record.tag != NPC {
            return Err(Error::Invalid(format!(
                "expected {NPC} record, found {}",
                record.tag
            )));
        }

        let mut head_parts = Vec::new();
        for data in record.all(HEAD_PART) {
            let mut fields = Fields::new(record.tag, HEAD_PART, data);
            fields.exact(4)?;
            head_parts.push(fields.u32()?);
        }

        let mut morph = [0.0; FACE_MORPH_COUNT];
        if let Some(data) = record.find(FACE_MORPH) {
            let mut fields = Fields::new(record.tag, FACE_MORPH, data);
            fields.exact(FACE_MORPH_COUNT * 4)?;
            for value in &mut morph {
                *value = fields.f32()?;
            }
        }
        validate_morph(&morph)?;

        Ok(Self {
            form_id: record.form_id,
            head_parts,
            morph,
        })
    }

    /// Create an NPC face override with validated head-part links and morphs.
    ///
    /// Each head-part link must resolve in `source_file` to a winning HDPT
    /// record. Other NPC subrecords remain unchanged.
    ///
    /// # Errors
    ///
    /// Returns an error if the source file is absent, a link is missing or is
    /// not an HDPT, the record is not NPC_, or a morph value is non-finite.
    pub fn edit_record(
        record: &Record,
        head_parts: &[u32],
        morph: [f32; FACE_MORPH_COUNT],
        order: &LoadOrder,
        source_file: usize,
    ) -> Result<Record> {
        if record.tag != NPC {
            return Err(Error::Invalid(format!(
                "expected {NPC} record, found {}",
                record.tag
            )));
        }
        if order.files().get(source_file).is_none() {
            return Err(Error::Invalid(format!(
                "source file index {source_file} is not in the load order"
            )));
        }
        validate_morph(&morph)?;
        for &raw in head_parts {
            let id = order.resolve(source_file, raw);
            let Some(target) = order.winner(id) else {
                return Err(Error::Invalid(format!(
                    "NPC head part {raw:08X} does not resolve to a record"
                )));
            };
            if target.tag != HEAD_PART_RECORD {
                return Err(Error::Invalid(format!(
                    "NPC head part {raw:08X} resolves to {}, expected {HEAD_PART_RECORD}",
                    target.tag
                )));
            }
        }

        let mut edited = record.clone();
        replace_fields(
            &mut edited,
            HEAD_PART,
            head_parts
                .iter()
                .map(|id| Subrecord::new(HEAD_PART, id.to_le_bytes()))
                .collect(),
        );
        replace_fields(
            &mut edited,
            FACE_MORPH,
            vec![Subrecord::new(FACE_MORPH, morph_bytes(&morph))],
        );
        Ok(edited)
    }
}

fn validate_morph(morph: &[f32; FACE_MORPH_COUNT]) -> Result<()> {
    if morph.iter().any(|value| !value.is_finite()) {
        return Err(Error::Invalid(
            "NPC face morph values must be finite".into(),
        ));
    }
    Ok(())
}

fn morph_bytes(morph: &[f32; FACE_MORPH_COUNT]) -> Vec<u8> {
    morph.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn replace_fields(record: &mut Record, tag: Tag, replacement: Vec<Subrecord>) {
    let mut replaced = false;
    let mut subrecords = Vec::with_capacity(record.subrecords.len() + replacement.len());
    for subrecord in record.subrecords.drain(..) {
        if subrecord.tag == tag {
            if !replaced {
                subrecords.extend(replacement.iter().cloned());
                replaced = true;
            }
        } else {
            subrecords.push(subrecord);
        }
    }
    if !replaced {
        subrecords.extend(replacement);
    }
    record.subrecords = subrecords;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edt_10_npc_face_reads_exactly_nineteen_morph_values() {
        let morph = std::array::from_fn(|index| index as f32 * 0.25);
        let record = Record::new(NPC, 1).sub(FACE_MORPH, morph_bytes(&morph));
        assert_eq!(NpcFace::from_record(&record).unwrap().morph, morph);
    }

    #[test]
    fn edt_10_npc_face_rejects_malformed_morph_values() {
        let record = Record::new(NPC, 1).sub(FACE_MORPH, [0; 75]);
        assert!(NpcFace::from_record(&record).is_err());
    }
}
