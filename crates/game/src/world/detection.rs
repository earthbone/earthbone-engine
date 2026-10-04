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
    Awareness, DetectionEvent, DetectionLevel, DetectionSettings, DetectionState, Error,
    Observation, Senses, StealthPool,
};

impl World {
    /// Queue a sound at `source` for the next host detection step.
    /// The CK permits levels above 100, and a separate actor may own the sound.
    /// Invalid actors, references and negative levels leave the queue unchanged.
    pub fn create_detection_event(
        &mut self,
        source: GlobalFormId,
        owner: GlobalFormId,
        sound_level: i32,
    ) -> bool {
        if sound_level < 0 || self.tag_of(owner) != Some(super::T(b"ACHR")) {
            return false;
        }
        let Some(reference) = self.reference(source) else {
            return false;
        };
        let (Some(cell), true) = (reference.cell, reference.enabled && !reference.deleted) else {
            return false;
        };
        let position = reference.position;
        self.detection_events.push(DetectionEvent {
            owner,
            cell,
            position,
            sound_level: sound_level as f32,
        });
        true
    }

    /// Take the sounds emitted since the previous observation step.
    pub fn take_detection_events(&mut self) -> Vec<DetectionEvent> {
        std::mem::take(&mut self.detection_events)
    }

    /// Whether `target` is currently Alert or in Combat toward `observer`.
    /// This reads the observer-specific result from the shared detection
    /// state. It does not create an observation or advance detection.
    ///
    /// # Errors
    /// Returns [`Error::Actor`] when either form is not a live actor.
    pub fn is_actor_detected_by(
        &self,
        target: GlobalFormId,
        observer: GlobalFormId,
    ) -> Result<bool, Error> {
        self.check_detection_actor(target)?;
        self.check_detection_actor(observer)?;
        Ok(self
            .awareness(observer, target)
            .is_some_and(|a| matches!(a.level, DetectionLevel::Alert | DetectionLevel::Combat)))
    }

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
            let numbers = [
                o.distance,
                o.light_level,
                o.action_sound,
                o.action_sound_distance.unwrap_or(0.0),
            ];
            if numbers.iter().any(|n| !n.is_finite() || *n < 0.0) {
                return Err(Error::Observation);
            }
            if o.observer == o.target {
                return Err(Error::SelfObservation(o.observer));
            }
            self.check_detection_actor(o.observer)?;
            if self.tag_of(o.target) == Some(super::T(b"ACHR")) {
                self.check_detection_actor(o.target)?;
            } else if !self.is_player(o.observer)
                || self
                    .reference(o.target)
                    .is_none_or(|reference| reference.deleted)
            {
                return Err(Error::Actor(o.target));
            }
            if !pairs.insert((o.observer, o.target)) {
                return Err(Error::Duplicate {
                    observer: o.observer,
                    target: o.target,
                });
            }
        }

        let mut combat_targets: BTreeMap<GlobalFormId, BTreeSet<GlobalFormId>> = BTreeMap::new();
        for (target, pool) in &self.detection.pools {
            for (observer, awareness) in &pool.observers {
                if awareness.level == DetectionLevel::Combat {
                    combat_targets.entry(*observer).or_default().insert(*target);
                }
            }
        }
        let mut target_senses = BTreeMap::new();
        let mut observer_senses = BTreeMap::new();
        let mut by_target: BTreeMap<GlobalFormId, Vec<(GlobalFormId, f32)>> = BTreeMap::new();
        for o in observations {
            if self.tag_of(o.target) != Some(super::T(b"ACHR")) {
                continue;
            }
            let target = match target_senses.entry(o.target) {
                std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    *entry.insert(self.target_senses(o.target)?)
                }
            };
            let (observer_sneak, blindness, fighting) = match observer_senses.entry(o.observer) {
                std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let observer = self.actor(o.observer).ok_or(Error::Actor(o.observer))?;
                    *entry.insert((
                        observer.values.get("sneak").current(),
                        observer.values.get("blindness").current(),
                        observer.combat_target,
                    ))
                }
            };
            let busy_elsewhere = combat_targets
                .get(&o.observer)
                .is_some_and(|targets| targets.len() > 1 || !targets.contains(&o.target));
            let senses = Senses {
                observer_sneak,
                blindness,
                observer_alert: self
                    .awareness(o.observer, o.target)
                    .is_some_and(|awareness| awareness.level == DetectionLevel::Alert),
                observer_busy: fighting.is_some_and(|target| target != o.target) || busy_elsewhere,
                ..target
            };
            let value = crate::detection::detection_value(&settings, o, &senses)?;
            by_target
                .entry(o.target)
                .or_default()
                .push((o.observer, value));
        }
        for (target, pool) in &self.detection.pools {
            let Some(target_ref) = self
                .reference_ref(*target)
                .filter(|reference| reference.enabled && !reference.deleted)
            else {
                continue;
            };
            let Some(cell) = target_ref.cell.filter(|cell| self.cell_is_resident(*cell)) else {
                continue;
            };
            for observer in pool.observers.keys() {
                if pairs.contains(&(*observer, *target)) {
                    continue;
                }
                let Some(observer_ref) = self.reference_ref(*observer).filter(|reference| {
                    reference.cell == Some(cell) && reference.enabled && !reference.deleted
                }) else {
                    continue;
                };
                let distance = target_ref
                    .position
                    .iter()
                    .zip(observer_ref.position)
                    .map(|(a, b)| (*a - b).powi(2))
                    .sum::<f32>()
                    .sqrt();
                if distance > settings.max_distance {
                    by_target
                        .entry(*target)
                        .or_default()
                        .push((*observer, settings.base_value.min(-1.0)));
                }
            }
        }
        let line_of_sight = observations
            .iter()
            .filter(|observation| observation.line_of_sight)
            .map(|observation| (observation.observer, observation.target))
            .collect();

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
        self.line_of_sight = line_of_sight;
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

    fn target_senses(&mut self, id: GlobalFormId) -> Result<Senses, Error> {
        let magic_invisible = self.is_invisible(id);
        let target = self.actor(id).ok_or(Error::Actor(id))?;
        let target_sneak = target.values.get("sneak").current();
        let muffle = if target.values.contains("movementnoisemult") {
            target.values.get("movementnoisemult").current()
        } else {
            1.0
        };
        let invisible = target.values.get("invisibility").current() > 0.0 || magic_invisible;
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

        Ok(Senses {
            observer_sneak: 0.0,
            target_sneak,
            armor_weight,
            muffle,
            blindness: 0.0,
            invisible,
            observer_alert: false,
            observer_busy: false,
        })
    }

    /// Writes the detection block, always as an explicit pool count. Versions
    /// 30 to 36 instead omit the whole block while no pool exists, relying on
    /// detection being the last thing in the stream. That stops holding once
    /// a save section table can follow it, so this write is unconditional
    /// and `read_detection` picks the right reading by version.
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

/// Read the version 30 detection block.
///
/// Versions 30 to 36 write nothing at all when there is no pool, so an
/// absent block reads back as the default state, and a present one must
/// hold at least one pool so each state has a single encoding. That relies
/// on detection being the last thing in the stream, which stopped holding
/// once a save section table could follow it, so version 37 onward instead
/// writes an explicit pool count, zero included, and reads it unconditionally.
/// Actor identities are checked by the caller once references are known.
pub(super) fn read_detection(
    input: &mut FormReader<'_, '_>,
    version: u16,
) -> Result<DetectionState, VmError> {
    let bad = || VmError::State("invalid detection state".into());
    let mut state = DetectionState::default();
    if version < 37 && input.remaining() == 0 {
        return Ok(state);
    }
    let pools = input.len()?;
    if pools == 0 {
        if version >= 37 {
            return Ok(state);
        }
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

#[cfg(test)]
mod prf_31_tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin, Record};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn world() -> World {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(super::super::T(b"CELL"), 1));
        plugin.push_top(Record::new(super::super::T(b"NPC_"), 2));
        for object in [3, 4] {
            let mut reference =
                Record::new(super::super::T(b"ACHR"), object).sub(b"NAME", 2_u32.to_le_bytes());
            reference.placement.cell = Some(1);
            plugin.push_top(reference);
        }
        let mut order = LoadOrder::new();
        order.add("Detection.esm", plugin).unwrap();
        let mut world = World::new(order);
        let mut values = [0.0; 26];
        values[0] = -60.0;
        values[1] = 100.0;
        values[2] = 1.0;
        values[12] = 1.0;
        values[18] = 100.0;
        values[20] = 1.0;
        values[21] = 1.0;
        values[23] = 1.0;
        for (name, value) in DetectionSettings::NAMES.iter().zip(values) {
            world
                .settings
                .insert(name.to_ascii_lowercase(), Value::Float(value));
        }
        world.load_cell(id(1));
        world
    }

    #[test]
    fn prf_31_pruned_pairs_advance_loss_and_recovery_without_new_observers() {
        let mut world = world();
        let observation = Observation {
            observer: id(3),
            target: id(4),
            distance: 0.0,
            line_of_sight: true,
            light_level: 100.0,
            motion: crate::detection::Motion::Still,
            action_sound: 0.0,
            action_sound_distance: None,
            action_sound_line_of_sight: None,
        };
        world.evaluate_detection(&[observation], 0.0).unwrap();
        assert_eq!(
            world.awareness(id(3), id(4)).unwrap().level,
            DetectionLevel::Combat
        );
        world.reference(id(4)).unwrap().position = [1000.0, 0.0, 0.0];
        world.evaluate_detection(&[], 1.0).unwrap();
        assert_eq!(
            world.awareness(id(3), id(4)).unwrap().level,
            DetectionLevel::Lost
        );
        world.evaluate_detection(&[], 2.0).unwrap();
        assert_eq!(
            world.awareness(id(3), id(4)).unwrap().level,
            DetectionLevel::Normal
        );
        assert!(world.awareness(id(4), id(3)).is_none());
    }

    #[test]
    fn prf_31_rejected_batch_does_not_age_pruned_pairs() {
        let mut world = world();
        let observation = Observation {
            observer: id(3),
            target: id(4),
            distance: 0.0,
            line_of_sight: true,
            light_level: 100.0,
            motion: crate::detection::Motion::Still,
            action_sound: 0.0,
            action_sound_distance: None,
            action_sound_line_of_sight: None,
        };
        world.evaluate_detection(&[observation], 0.0).unwrap();
        world.reference(id(4)).unwrap().position = [1000.0, 0.0, 0.0];
        let before = world.detection.clone();
        assert!(world
            .evaluate_detection(
                &[Observation {
                    distance: -1.0,
                    ..observation
                }],
                2.0
            )
            .is_err());
        assert_eq!(world.detection, before);
    }
}
