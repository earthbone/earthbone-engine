// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient `RFCT` visual effects attached to references. The three `DATA`
//! fields follow `TES5Edit/Core/wbDefinitionsTES5.pas` (`ARTO`, `EFSH`, flags).

use soe_esm::GlobalFormId;

use super::{le_u32, World, T};

/// The authored forms and flags in an `RFCT` record's `DATA` subrecord.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualEffectData {
    pub art: Option<GlobalFormId>,
    pub shader: Option<GlobalFormId>,
    pub flags: u32,
}

/// One active visual effect attached to a target reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualEffectAttachment {
    pub effect: GlobalFormId,
    pub target: GlobalFormId,
    pub art: Option<GlobalFormId>,
    pub shader: Option<GlobalFormId>,
    pub flags: u32,
}

impl World {
    /// Read the `ARTO`, `EFSH` and flags fields from a winning `RFCT` record.
    pub fn visual_effect_data(&self, id: GlobalFormId) -> Option<VisualEffectData> {
        let record = self.record(id)?;
        if record.tag != T(b"RFCT") || record.is_deleted() {
            return None;
        }
        let data = record.find(T(b"DATA"))?;
        if data.len() != 12 {
            return None;
        }
        let resolve = |offset| {
            le_u32(&data[offset..])
                .filter(|raw| *raw != 0)
                .and_then(|raw| self.resolve_from(id, raw))
        };
        Some(VisualEffectData {
            art: resolve(0),
            shader: resolve(4),
            flags: le_u32(&data[8..])?,
        })
    }

    /// Attach a winning `RFCT` effect to a reference. Replaying the same
    /// effect on the same target keeps one attachment, matching the pair of
    /// forms used by `Stop` to release it.
    pub fn play_visual_effect(&mut self, effect: GlobalFormId, target: GlobalFormId) -> bool {
        let Some(data) = self.visual_effect_data(effect) else {
            return false;
        };
        let target_tag = self.tag_of(target).map(|tag| *tag.as_bytes());
        if target_tag != Some(*b"REFR") && target_tag != Some(*b"ACHR") {
            return false;
        }
        self.visual_effects.insert(
            (effect, target),
            VisualEffectAttachment {
                effect,
                target,
                art: data.art,
                shader: data.shader,
                flags: data.flags,
            },
        );
        true
    }

    /// Stop one effect on one target, returning whether it had been attached.
    pub fn stop_visual_effect(&mut self, effect: GlobalFormId, target: GlobalFormId) -> bool {
        self.visual_effects.remove(&(effect, target)).is_some()
    }

    /// Active visual attachments in deterministic form-id order.
    pub fn visual_effects(&self) -> impl Iterator<Item = &VisualEffectAttachment> {
        self.visual_effects.values()
    }
}
