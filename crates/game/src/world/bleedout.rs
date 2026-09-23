// SPDX-License-Identifier: GPL-3.0-or-later

//! Bleedout thresholds follow xEdit ACBS/CLAS layouts and the installed runtime
//! calculation identified in docs/combat.md. Entry and the health-driven
//! recovery path reuse the existing `unconscious` and `no_bleedout_recovery`
//! actor state, so both already round trip through save and hash.

use soe_esm::{cursor::Fields, GlobalFormId, Tag};
use soe_papyrus::Value;

use super::World;
use crate::{
    combat::Error,
    npc::{flags, template},
};

impl World {
    /// Read the health threshold from the actor's selected stats source.
    /// An ACBS override is absolute health. A class value scales permanent health.
    /// This query does not enter bleedout or apply recovery.
    ///
    /// # Errors
    /// Rejects invalid actors, records, settings and negative or non-finite results.
    pub fn actor_bleedout_threshold(&mut self, actor: GlobalFormId) -> Result<f32, Error> {
        let source = self.actor_component_source(actor, template::STATS)?;
        let npc = self.npc_record(source)?;
        if npc.config.flags & flags::BLEEDOUT_OVERRIDE != 0 {
            // ACBS stores the bits as U16. The runtime sign-extends this field.
            return nonnegative(f32::from(npc.config.bleedout as i16));
        }
        let fraction = match npc.class.filter(|raw| *raw != 0) {
            Some(raw) => {
                let class = self
                    .resolve_from(source, raw)
                    .ok_or(Error::Unsupported("unresolved bleedout class"))?;
                let record = self
                    .record(class)
                    .filter(|record| record.tag == Tag(*b"CLAS") && !record.is_deleted())
                    .ok_or(Error::Unsupported("missing or wrong bleedout class"))?;
                let tag = Tag(*b"DATA");
                let mut fields = Fields::new(record.tag, tag, record.required(tag)?);
                fields.exact(36)?;
                fields.skip(24)?;
                nonnegative(fields.f32()?)?
            }
            None => match self.game_setting("fBleedoutDefault") {
                Some(Value::Float(value)) if value.is_finite() && value >= 0.0 => value,
                _ => return Err(Error::Setting("fBleedoutDefault")),
            },
        };
        let health = self
            .actor(actor)
            .ok_or(Error::Actor(actor))?
            .values
            .get("health");
        let permanent = nonnegative(health.base() + health.permanent())?;
        nonnegative(permanent * fraction)
    }

    /// Recover a bleeding-out actor once a health change (a restore, a heal
    /// spell, a potion) carries current health back above the cached
    /// threshold, unless `SetNoBleedoutRecovery` holds them down.
    ///
    /// This is the health-driven recovery path the installed runtime takes
    /// at 0x1406cf330 (docs/combat.md, CMB-13): comparing changed health
    /// against the threshold and checking `NoBleedoutRecovery`. Per the
    /// ck-wiki GetNoBleedoutRecovery/SetNoBleedoutRecovery pages, that flag
    /// alone gates recovery. `protected` and `essential` only gate entry
    /// into bleedout (`World::melee_hit`, combat.rs) and must not be
    /// re-read here: a script can lift essential status on an actor while
    /// they are still unconscious (a common "scene is over" pattern), and
    /// that must not strand them down for good. The separate
    /// `essentialDownTimer` path that gets an unhealed actor back up on its
    /// own after a duration remains unverified and is not modeled here.
    pub(super) fn resolve_bleedout_recovery(&mut self, id: GlobalFormId) {
        let Some(state) = self.actor_ref(id) else {
            return;
        };
        if !state.unconscious || state.no_bleedout_recovery {
            return;
        }
        let current = state.values.get("health").current();
        let Ok(threshold) = self.actor_bleedout_threshold(id) else {
            return;
        };
        if current <= threshold {
            return;
        }
        if let Some(state) = self.actor(id) {
            state.unconscious = false;
        }
    }
}

fn nonnegative(value: f32) -> Result<f32, Error> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(Error::Number)
    }
}
