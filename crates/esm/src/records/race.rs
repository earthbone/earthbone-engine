// SPDX-License-Identifier: GPL-3.0-or-later

//! RACE, read for the part that drives a talking face: the FaceFX phoneme
//! mapping.
//!
//! A race names its own morph targets in `PHTN` and then, in `PHWT`, gives a
//! weight for each of those targets for each of the 43 FaceFX phonemes. The
//! names differ per race, which is the whole reason the mapping lives on the
//! record rather than in the engine: a dragon carries eight targets with its
//! own names where a humanoid carries sixteen. Layout from
//! `TES5Edit/Core/wbDefinitionsTES5.pas` (`wbPHWT`, `wbPhonemeTargets`) and
//! `uesp-modfileformat/RACE.wikitext`.
//!
//! Everything else a RACE holds is left in the `Record`, since nothing needs
//! it yet.

use crate::cursor::{zstring, Fields};
use crate::error::Result;
use crate::plugin::Record;
use crate::tag::{self, Tag};

/// The FaceFX phonemes, in the order their `PHWT` subrecords appear. The
/// order is the file format, not a preference, so an index into this table is
/// an index into [`Race::phoneme_weights`].
pub const PHONEMES: [&str; 43] = [
    "IY", "IH", "EH", "EY", "AE", "AA", "AW", "AY", "AH", "AO", "OY", "OW", "UH", "UW", "ER", "AX",
    "S", "SH", "Z", "ZH", "F", "TH", "V", "DH", "M", "N", "NG", "L", "R", "W", "Y", "HH", "B", "D",
    "JH", "G", "P", "T", "K", "CH", "SIL", "SHOTSIL", "FLAP",
];

const PHTN: Tag = Tag::new(b"PHTN");
const PHWT: Tag = Tag::new(b"PHWT");

#[derive(Debug, Clone, PartialEq)]
pub struct Race {
    pub form_id: u32,
    pub editor_id: Option<String>,
    /// `PHTN`, the morph targets this race's face carries, in file order.
    /// Sixteen for a humanoid, eight for a dragon, whatever a mod ships.
    pub phoneme_targets: Vec<String>,
    /// `PHWT`, one row per phoneme in [`PHONEMES`] order, each row one weight
    /// per entry of `phoneme_targets`. A file that stops early leaves the
    /// remaining phonemes absent rather than zeroed, so a caller can tell
    /// "not written" from "written as zero".
    pub phoneme_weights: Vec<Vec<f32>>,
}

impl Race {
    /// # Errors
    ///
    /// When a `PHWT` subrecord is not a whole number of floats.
    pub fn from_record(rec: &Record) -> Result<Race> {
        debug_assert_eq!(rec.tag, tag::RACE);
        let phoneme_targets = rec.all(PHTN).map(zstring).collect();
        let mut phoneme_weights = Vec::new();
        for d in rec.all(PHWT) {
            let mut f = Fields::new(rec.tag, PHWT, d);
            if d.len() % 4 != 0 {
                f.exact(d.len() / 4 * 4)?;
            }
            let mut row = Vec::with_capacity(d.len() / 4);
            while f.remaining() >= 4 {
                row.push(f.f32()?);
            }
            phoneme_weights.push(row);
        }
        Ok(Race {
            form_id: rec.form_id,
            editor_id: rec.editor_id(),
            phoneme_targets,
            phoneme_weights,
        })
    }

    /// The weight row for one phoneme, by its FaceFX name. `None` when the
    /// name is not a phoneme or the file stopped before reaching it.
    #[must_use]
    pub fn weights_for(&self, phoneme: &str) -> Option<&[f32]> {
        for (index, name) in PHONEMES.iter().enumerate() {
            if *name == phoneme {
                return self.phoneme_weights.get(index).map(Vec::as_slice);
            }
        }
        None
    }

    /// True when every phoneme has a row and every row covers every target,
    /// which is what a face can actually be driven from.
    #[must_use]
    pub fn phoneme_mapping_is_complete(&self) -> bool {
        !self.phoneme_targets.is_empty()
            && self.phoneme_weights.len() == PHONEMES.len()
            && self
                .phoneme_weights
                .iter()
                .all(|r| r.len() == self.phoneme_targets.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weights(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    fn built() -> Record {
        let mut rec = Record::new(tag::RACE, 0x13746).zsub(tag::EDID, "NordRace");
        for name in ["Aah", "BigAah", "BMP"] {
            rec = rec.zsub(b"PHTN", name);
        }
        for (i, _) in PHONEMES.iter().enumerate() {
            let base = i as f32;
            rec = rec.sub(b"PHWT", weights(&[base, base + 0.5, base + 0.25]));
        }
        rec
    }

    #[test]
    fn a_race_reads_its_targets_and_one_row_per_phoneme() {
        let race = Race::from_record(&built()).unwrap();
        assert_eq!(race.phoneme_targets, ["Aah", "BigAah", "BMP"]);
        assert_eq!(race.phoneme_weights.len(), PHONEMES.len());
        assert!(race.phoneme_mapping_is_complete());
        // AA is index 5, so its row was written from 5.0.
        assert_eq!(race.weights_for("AA"), Some([5.0f32, 5.5, 5.25].as_slice()));
        assert_eq!(race.weights_for("SIL").unwrap()[0], 40.0);
        assert_eq!(race.weights_for("NOTAPHONEME"), None);
    }

    #[test]
    fn a_dragon_sized_row_is_kept_as_written() {
        let rec = Record::new(tag::RACE, 1)
            .zsub(b"PHTN", "LipBigAah")
            .sub(b"PHWT", weights(&[1.0]));
        let race = Race::from_record(&rec).unwrap();
        assert_eq!(race.phoneme_targets.len(), 1);
        assert_eq!(race.weights_for("IY"), Some([1.0f32].as_slice()));
        // One row of 43 is not a face anybody can drive.
        assert!(!race.phoneme_mapping_is_complete());
        assert_eq!(race.weights_for("IH"), None);
    }

    #[test]
    fn a_ragged_weight_block_is_an_error() {
        let rec = Record::new(tag::RACE, 1).sub(b"PHWT", [0u8; 7]);
        assert!(Race::from_record(&rec).is_err());
    }
}
