// SPDX-License-Identifier: GPL-3.0-or-later

//! Resolve animation foot tags and collision materials through Skyrim records.
//!
//! Layouts: xEdit `wbDefinitionsTES5.pas` FSTS, FSTP, IPDS, IPCT, MATT and
//! DOBJ. DOBJ entries merge individually across overrides, as documented in
//! UESP's DOBJ page. Material hashing uses CommonLibSSE-NG's CRC convention
//! and lowercase MNAM, verified against all 79 shipped material names.
//! This selects descriptors. Their conditions, variations and playback belong
//! to the sound execution path, not to collision or filename heuristics.

use std::collections::BTreeSet;

use soe_esm::cursor::{zstring, Fields};
use soe_esm::{Error, GlobalFormId, LoadOrder, Record, Result, Tag};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

mod actor;

/// Serialized XCNT order. FSTS DATA stores the groups in reverse order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gait {
    Walk,
    Run,
    Sprint,
    Sneak,
    Swim,
}

impl Gait {
    fn index(self) -> usize {
        match self {
            Self::Walk => 0,
            Self::Run => 1,
            Self::Sprint => 2,
            Self::Sneak => 3,
            Self::Swim => 4,
        }
    }
}

/// Preserve the two authored impact sound slots, including missing slots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FootstepImpact {
    pub footstep: GlobalFormId,
    pub impact: GlobalFormId,
    pub sounds: [Option<GlobalFormId>; 2],
}

/// A borrowed view of the authoritative load order, with no copied game state.
pub struct Footsteps<'a> {
    order: &'a LoadOrder,
}

impl<'a> Footsteps<'a> {
    #[must_use]
    pub fn new(order: &'a LoadOrder) -> Self {
        Self { order }
    }

    /// Read a sparse default-object entry, preserving the defining file's
    /// FormID namespace. Zero-filled entries do not erase earlier keys.
    pub fn default_object(&self, key: [u8; 4]) -> Result<Option<GlobalFormId>> {
        let mut value = None;
        let manager = GlobalFormId {
            file: 0,
            object: 0x31,
        };
        for owner in self.order.overrides(manager) {
            let record = self.order.record(*owner);
            if record.tag != T(b"DOBJ") {
                return Err(Error::Invalid(format!(
                    "default object manager has type {}",
                    record.tag
                )));
            }
            if record.is_deleted() {
                value = None;
                continue;
            }
            for bytes in record.all(T(b"DNAM")) {
                if bytes.len() % 8 != 0 {
                    return Err(Error::Invalid("DOBJ DNAM has a partial entry".into()));
                }
                for entry in bytes.chunks_exact(8) {
                    if key != [0; 4] && entry[..4] == key {
                        let raw = Fields::new(record.tag, T(b"DNAM"), &entry[4..]).u32()?;
                        value = self.link(owner.file, raw);
                    }
                }
            }
        }
        Ok(value)
    }

    /// Map the collision hash to MATT identity. Ambiguous names are reported
    /// rather than depending on record iteration order.
    pub fn material(&self, hash: u32) -> Result<Option<GlobalFormId>> {
        let mut found = None;
        for (id, record) in self.order.winners_of(T(b"MATT")) {
            if record.is_deleted() {
                continue;
            }
            let Some(name) = record.find(T(b"MNAM")) else {
                continue;
            };
            if material_name_hash(name) == hash {
                if found.is_some() {
                    return Err(Error::Invalid(format!(
                        "multiple MATT records name material hash {hash}"
                    )));
                }
                found = Some(id);
            }
        }
        Ok(found)
    }

    /// Match only the requested animation tag in the requested movement group.
    /// Empty groups and null impact links are silent, not random substitutes.
    pub fn impacts(
        &self,
        set: GlobalFormId,
        gait: Gait,
        event: &str,
        material: GlobalFormId,
    ) -> Result<Vec<FootstepImpact>> {
        let (record, file) = self.record(set, T(b"FSTS"))?;
        let bytes = record.required(T(b"XCNT"))?;
        let mut counts = Fields::new(record.tag, T(b"XCNT"), bytes);
        counts.exact(20)?;
        let mut sizes = [0_usize; 5];
        for size in &mut sizes {
            *size = counts.u32()? as usize;
        }
        let total = sizes
            .iter()
            .try_fold(0_usize, |total, n| total.checked_add(*n))
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(|| Error::Invalid("FSTS count overflow".into()))?;
        let data = record.required(T(b"DATA"))?;
        Fields::new(record.tag, T(b"DATA"), data).exact(total)?;
        let start = sizes[gait.index() + 1..].iter().sum::<usize>() * 4;
        let end = start + sizes[gait.index()] * 4;
        let mut out = Vec::new();
        for bytes in data[start..end].chunks_exact(4) {
            let raw = Fields::new(record.tag, T(b"DATA"), bytes).u32()?;
            let Some(step) = self.link(file, raw) else {
                continue;
            };
            let (footstep, owner) = self.record(step, T(b"FSTP"))?;
            if !zstring(footstep.required(T(b"ANAM"))?).eq_ignore_ascii_case(event) {
                continue;
            }
            let Some(set) = self.record_link(footstep, owner, T(b"DATA"))? else {
                continue;
            };
            let Some(impact) = self.impact(set, material)? else {
                continue;
            };
            let (record, owner) = self.record(impact, T(b"IPCT"))?;
            let mut sounds = [None; 2];
            for (index, tag) in [b"SNAM", b"NAM1"].into_iter().enumerate() {
                if let Some(sound) = self.record_link(record, owner, T(tag))? {
                    self.record(sound, T(b"SNDR"))?;
                    sounds[index] = Some(sound);
                }
            }
            out.push(FootstepImpact {
                footstep: step,
                impact,
                sounds,
            });
        }
        Ok(out)
    }

    fn impact(&self, set: GlobalFormId, material: GlobalFormId) -> Result<Option<GlobalFormId>> {
        let (record, file) = self.record(set, T(b"IPDS"))?;
        let mut material = Some(material);
        let mut visited = BTreeSet::new();
        while let Some(id) = material {
            if !visited.insert(id) {
                return Err(Error::Invalid(format!("MATT parent cycle at {id}")));
            }
            let (mat, owner) = self.record(id, T(b"MATT"))?;
            for bytes in record.all(T(b"PNAM")) {
                let mut pair = Fields::new(record.tag, T(b"PNAM"), bytes);
                pair.exact(8)?;
                if self.link(file, pair.u32()?) == Some(id) {
                    return Ok(self.link(file, pair.u32()?));
                }
            }
            material = self.record_link(mat, owner, T(b"PNAM"))?;
        }
        Ok(None)
    }

    fn record(&self, id: GlobalFormId, tag: Tag) -> Result<(&'a Record, usize)> {
        let owner = self
            .order
            .winner_ref(id)
            .ok_or_else(|| Error::Invalid(format!("missing {tag} {id}")))?;
        let record = self.order.record(owner);
        if record.tag != tag || record.is_deleted() {
            return Err(Error::Invalid(format!(
                "expected live {} at {id}, got {}",
                tag, record.tag
            )));
        }
        Ok((record, owner.file))
    }

    fn record_link(&self, record: &Record, file: usize, tag: Tag) -> Result<Option<GlobalFormId>> {
        let Some(bytes) = record.find(tag) else {
            return Ok(None);
        };
        let mut field = Fields::new(record.tag, tag, bytes);
        field.exact(4)?;
        Ok(self.link(file, field.u32()?))
    }

    fn link(&self, file: usize, raw: u32) -> Option<GlobalFormId> {
        (raw != 0).then(|| self.order.resolve(file, raw))
    }
}

/// Hash raw MNAM bytes through their NUL terminator, using ASCII lowercase.
/// Keeping bytes avoids changing the plugin's CP1252 text into UTF-8.
/// Bethesda CRC32 starts at zero and omits the final inversion. crc32fast's
/// public initial value and result use the conventional inverted convention.
#[must_use]
pub fn material_name_hash(name: &[u8]) -> u32 {
    let mut hash = crc32fast::Hasher::new_with_initial(u32::MAX);
    let end = name
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(name.len());
    hash.update(&name[..end].to_ascii_lowercase());
    !hash.finalize()
}
