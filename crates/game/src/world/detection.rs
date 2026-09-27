// SPDX-License-Identifier: GPL-3.0-or-later

//! Detection steps against the world's actor records, and their SOEW block.
//! The formula and state rules are documented in `crate::detection`.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::{Value, VmError};

use super::save::{read_form_id, write_form_id, FormReader};
use super::World;
use crate::detection::{
    Awareness, DetectionLevel, DetectionSettings, DetectionState, Error, Observation, Senses,
    StealthPool,
};

impl World {
    /// Detection settings from the load order's GMST records.
    ///
    /// # Errors
    /// [`Error::Setting`] for a missing, non-numeric or out-of-range setting.
    pub fn detection_settings(&self) -> Result<DetectionSettings, Error> {
        let mut values = [0.0_f32; 26];
        for (value, name) in values.iter_mut().zip(DetectionSettings::NAMES) {
            *value = match self.game_setting(name) {
                Some(Value::Float(v)) => v,
                #[allow(clippy::cast_precision_loss)]
                Some(Value::Int(v)) => v as f32,
                _ => return Err(Error::Setting(name)),
            };
        }
        DetectionSettings::from_values(values)
    }

    /// A target's shared stealth-point pool, once any observer has seen it.
    pub fn stealth_pool(&self, target: GlobalFormId) -> Option<&StealthPool> {
        self.detection.pool(target)
    }

    /// One observer's awareness of one target.
    pub fn awareness(&self, observer: GlobalFormId, target: GlobalFormId) -> Option<Awareness> {
        self.detection
            .pool(target)?
            .observers
            .get(&observer)
            .copied()
    }

    /// Evaluate one step of observations and advance the shared pools.
    ///
    /// Every observation is checked before anything changes, and the new
    /// detection state replaces the old one only when the whole batch
    /// evaluates. A rejected batch leaves pools, awareness and timers as they
    /// were. Reading actor values may still build an actor from its records,
    /// as every actor-value read does.
    ///
    /// # Errors
    /// Negative or non-finite inputs, observers or targets that are not live
    /// actors, self observation, a repeated pair, invalid settings, a malformed
    /// worn armor record or a non-finite result.
    pub fn evaluate_detection(
        &mut self,
        observations: &[Observation],
        elapsed: f32,
    ) -> Result<(), Error> {
        if !elapsed.is_finite() || elapsed < 0.0 {
            return Err(Error::Observation);
        }
        let settings = self.detection_settings()?;
        let mut pairs = BTreeSet::new();
        for o in observations {
            let numbers = [o.distance, o.light_level, o.action_sound];
            if numbers.iter().any(|n| !n.is_finite() || *n < 0.0) {
                return Err(Error::Observation);
            }
            if o.observer == o.target {
                return Err(Error::SelfObservation(o.observer));
            }
            for id in [o.observer, o.target] {
                self.check_detection_actor(id)?;
            }
            if !pairs.insert((o.observer, o.target)) {
                return Err(Error::Duplicate {
                    observer: o.observer,
                    target: o.target,
                });
            }
        }

        let mut by_target: BTreeMap<GlobalFormId, Vec<(GlobalFormId, f32)>> = BTreeMap::new();
        for o in observations {
            let senses = self.senses(o)?;
            let value = crate::detection::detection_value(&settings, o, &senses)?;
            by_target
                .entry(o.target)
                .or_default()
                .push((o.observer, value));
        }

        let mut next = self.detection.clone();
        for (target, values) in &by_target {
            let sneaking = self.actors.get(target).is_some_and(|a| a.sneaking);
            next.pools
                .entry(*target)
                .or_insert_with(|| StealthPool::full(&settings))
                .step(&settings, values, sneaking, elapsed);
        }
        if next.pools.values().any(|p| {
            !p.points.is_finite()
                || !p.alert_wait.is_finite()
                || p.observers.values().any(|a| !a.lost_timer.is_finite())
        }) {
            return Err(Error::Number);
        }
        self.detection = next;
        Ok(())
    }

    fn check_detection_actor(&self, id: GlobalFormId) -> Result<(), Error> {
        let actor = self.tag_of(id) == Some(super::T(b"ACHR"));
        let dead = self.actors.get(&id).is_some_and(|a| a.dead);
        if actor && !dead {
            Ok(())
        } else {
            Err(Error::Actor(id))
        }
    }

    fn senses(&mut self, o: &Observation) -> Result<Senses, Error> {
        let observer = self.actor(o.observer).ok_or(Error::Actor(o.observer))?;
        let observer_sneak = observer.values.get("sneak").current();
        let blindness = observer.values.get("blindness").current();
        let fighting = observer.combat_target;
        let target = self.actor(o.target).ok_or(Error::Actor(o.target))?;
        let target_sneak = target.values.get("sneak").current();
        let muffle = if target.values.contains("movementnoisemult") {
            target.values.get("movementnoisemult").current()
        } else {
            1.0
        };
        let invisible = target.values.get("invisibility").current() > 0.0;
        let worn = target.equipped.clone();

        let mut armor_weight = 0.0_f32;
        for item in worn {
            if self.tag_of(item) != Some(super::T(b"ARMO")) {
                continue;
            }
            let armor = self
                .combat_armor_record(item)
                .map_err(|_| Error::Armor(item))?;
            armor_weight += armor.weight.unwrap_or(0.0);
        }

        let previous = self.awareness(o.observer, o.target).map(|a| a.level);
        let busy_elsewhere = self.detection.pools.iter().any(|(t, p)| {
            *t != o.target
                && p.observers
                    .get(&o.observer)
                    .is_some_and(|a| a.level == DetectionLevel::Combat)
        });
        Ok(Senses {
            observer_sneak,
            target_sneak,
            armor_weight,
            muffle,
            blindness,
            invisible,
            observer_alert: previous == Some(DetectionLevel::Alert),
            observer_busy: fighting.is_some_and(|t| t != o.target) || busy_elsewhere,
        })
    }

    /// Writes a count even for an empty state in version 35 and later.
    pub(super) fn write_detection(&self, out: &mut Writer) {
        out.len(self.detection.pools.len());
        for (target, pool) in &self.detection.pools {
            write_form_id(out, *target);
            out.f32(pool.points);
            out.f32(pool.alert_wait);
            out.len(pool.observers.len());
            for (observer, a) in &pool.observers {
                write_form_id(out, *observer);
                out.u8(a.level as u8);
                out.f32(a.detection);
                out.f32(a.lost_timer);
            }
        }
    }
}

/// Read the detection block. Versions 30 to 34 omit an empty block. Version
/// 35 writes an explicit zero count before later state. Actor identities are
/// checked by the caller once references are known.
pub(super) fn read_detection(
    input: &mut FormReader<'_, '_>,
    framed: bool,
) -> Result<DetectionState, VmError> {
    let bad = || VmError::State("invalid detection state".into());
    let mut state = DetectionState::default();
    if input.remaining() == 0 && !framed {
        return Ok(state);
    }
    let pools = input.len()?;
    if pools == 0 && framed {
        return Ok(state);
    }
    if pools == 0 {
        return Err(bad());
    }
    for _ in 0..pools {
        let target = read_form_id(input)?;
        let points = input.f32()?;
        let alert_wait = input.f32()?;
        if !points.is_finite() || points < 0.0 || !alert_wait.is_finite() || alert_wait < 0.0 {
            return Err(bad());
        }
        let mut observers = BTreeMap::new();
        let count = input.len()?;
        if count == 0 {
            return Err(bad());
        }
        for _ in 0..count {
            let observer = read_form_id(input)?;
            let level = DetectionLevel::from_u8(input.u8()?).ok_or_else(bad)?;
            let detection = input.f32()?;
            let lost_timer = input.f32()?;
            if observer == target
                || !detection.is_finite()
                || !lost_timer.is_finite()
                || lost_timer < 0.0
            {
                return Err(bad());
            }
            let awareness = Awareness {
                level,
                detection,
                lost_timer,
            };
            if observers.insert(observer, awareness).is_some() {
                return Err(bad());
            }
        }
        let pool = StealthPool {
            points,
            alert_wait,
            observers,
        };
        if state.pools.insert(target, pool).is_some() {
            return Err(bad());
        }
    }
    Ok(state)
}

/// Every actor a detection block names, for the loader's identity check.
pub(super) fn detection_actors(state: &DetectionState) -> impl Iterator<Item = GlobalFormId> + '_ {
    state
        .pools
        .iter()
        .flat_map(|(target, pool)| std::iter::once(*target).chain(pool.observers.keys().copied()))
}
