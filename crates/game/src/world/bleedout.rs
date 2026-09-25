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

    /// Clamp a post-damage health value at the actor's bleedout threshold,
    /// mirroring the inline clamp `World::melee_hit` applies before
    /// committing a hit (combat.rs, CMB-13). `World::change_actor_value`
    /// and `World::apply_effect_values` (magic.rs) share this so the
    /// scripted DamageActorValue and magic damage paths enter bleedout the
    /// same way a melee hit does instead of killing a protected or
    /// essential actor outright (CMB-35).
    ///
    /// `eligible` carries the caller's own protected/essential and player
    /// exception decision, since melee, script and magic sources key that
    /// exception on different identities. An unreadable threshold is
    /// skipped rather than propagated, the same way `resolve_bleedout_recovery`
    /// above already skips: many callers reach actors with no full NPC
    /// record (tests, synthetic actors) and a missing threshold there must
    /// not block a health change that would otherwise succeed.
    ///
    /// Returns the health to commit and whether the actor is entering
    /// bleedout. Not eligible, an unreadable threshold, or health already
    /// above the threshold returns `current` unchanged and `false`.
    pub(super) fn clamp_health_at_bleedout(
        &mut self,
        actor: GlobalFormId,
        eligible: bool,
        current: f32,
    ) -> (f32, bool) {
        if !eligible {
            return (current, false);
        }
        let Ok(threshold) = self.actor_bleedout_threshold(actor) else {
            return (current, false);
        };
        if current > threshold {
            return (current, false);
        }
        (threshold, true)
    }
}

/// Commit a bleedout clamp so health reads exactly `clamped`, choosing the
/// component that keeps the actor's maximum honest (CMB-35).
///
/// When `clamped` fits under the maximum, only the `damage` component moves,
/// the same `Values::restore` correction `World::melee_hit` applies
/// (combat.rs, CMB-13). That covers `DamageActorValue` and ordinary magic
/// damage, and leaves `maximum()` untouched so a later full heal lands on the
/// real maximum rather than one inflated by the clamp. A decrease that went
/// through `base`, `permanent` or `temporary` (`SetActorValue`,
/// `ModActorValue`, `ForceActorValue`, `ModTemporaryActorValue`, a RECOVER
/// magic effect) can leave the maximum itself below the threshold. Damage
/// cannot be positive, so there the damage is cleared and `Values::force`
/// raises `permanent` until the current value equals `clamped`.
///
/// # Errors
/// Rejects non-finite or overflowing totals. Callers commit a clone, so a
/// failure leaves the live actor untouched.
pub(super) fn commit_bleedout_health(
    values: &mut crate::actor_values::Values,
    clamped: f32,
) -> Result<(), crate::actor_values::Error> {
    let health = values.get("health");
    if clamped <= health.maximum() {
        return values.restore("health", clamped - health.current());
    }
    values.restore("health", f32::MAX)?;
    values.force("health", clamped)
}

fn nonnegative(value: f32) -> Result<f32, Error> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(Error::Number)
    }
}

#[cfg(test)]
mod tests {
    use super::commit_bleedout_health;
    use crate::actor_values::{Error, Values};

    /// A damage-path clamp moves only `damage`, so a full heal afterward
    /// returns to the original maximum instead of one the clamp inflated.
    #[test]
    fn damage_path_clamp_keeps_maximum() -> Result<(), Error> {
        let mut values = Values::default();
        values.set_base("health", 100.0)?;
        values.damage("health", 500.0)?;
        commit_bleedout_health(&mut values, 20.0)?;
        assert_eq!(values.get("health").current(), 20.0);
        assert_eq!(values.get("health").maximum(), 100.0);
        values.restore("health", f32::MAX)?;
        assert_eq!(values.get("health").current(), 100.0);
        Ok(())
    }

    /// A decrease that pulled the maximum under the threshold clears damage
    /// and raises `permanent`, the only way to reach the threshold there.
    #[test]
    fn maximum_path_clamp_lands_on_threshold() -> Result<(), Error> {
        let mut values = Values::default();
        values.set_base("health", 100.0)?;
        values.damage("health", 30.0)?;
        values.modify("health", -500.0)?;
        commit_bleedout_health(&mut values, 20.0)?;
        let health = values.get("health");
        assert_eq!(health.current(), 20.0);
        assert_eq!(health.damage(), 0.0);
        assert_eq!(health.maximum(), 20.0);
        Ok(())
    }
}
