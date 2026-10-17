// SPDX-License-Identifier: GPL-3.0-or-later

//! MOVT SPED and RACE movement links follow local xEdit and UESP layouts.
//! These views are derived from winning records and live actor values.

use soe_esm::{Error, GlobalFormId, Record, Tag};

use crate::world::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// Directional rates in Creation units per real second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Directions {
    pub left: f32,
    pub right: f32,
    pub forward: f32,
    pub back: f32,
}

impl Directions {
    const fn uniform(speed: f32) -> Self {
        Self {
            left: speed,
            right: speed,
            forward: speed,
            back: speed,
        }
    }

    /// Blend the directional components after normalizing the input axes.
    /// A diagonal never exceeds the faster of its two authored rates.
    #[must_use]
    pub fn speed(self, right: f32, forward: f32) -> f32 {
        let [right, forward] = self.velocity(right, forward);
        right.hypot(forward)
    }

    fn velocity(self, right: f32, forward: f32) -> [f32; 2] {
        let length = right.hypot(forward);
        if !length.is_finite() || length <= 1e-6 {
            return [0.0; 2];
        }
        let side = if right < 0.0 { self.left } else { self.right };
        let ahead = if forward < 0.0 {
            self.back
        } else {
            self.forward
        };
        [right / length * side, forward / length * ahead]
    }
}

/// The translational portion of a MOVT SPED block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementType {
    pub walk: Directions,
    pub run: Directions,
}

impl MovementType {
    /// Read either historical SPED size, retaining zero directional rates.
    ///
    /// # Errors
    /// Rejects wrong record types, missing fields, invalid sizes and rates.
    pub fn from_record(record: &Record) -> Result<Self, Error> {
        if record.tag != T(b"MOVT") || record.is_deleted() {
            return Err(Error::Invalid("movement type requires live MOVT".into()));
        }
        Self::from_sped(record.required(T(b"SPED"))?)
    }

    fn from_sped(data: &[u8]) -> Result<Self, Error> {
        if !matches!(data.len(), 40 | 44) {
            return Err(Error::Invalid("MOVT SPED must be 40 or 44 bytes".into()));
        }
        let number = |offset| {
            let value = f32::from_le_bytes(
                data[offset..offset + 4]
                    .try_into()
                    .map_err(|_| Error::Invalid("short MOVT speed".into()))?,
            );
            if value.is_finite() && value >= 0.0 {
                Ok(value)
            } else {
                Err(Error::Invalid("invalid MOVT speed".into()))
            }
        };
        Ok(Self {
            walk: Directions {
                left: number(0)?,
                right: number(8)?,
                forward: number(16)?,
                back: number(24)?,
            },
            run: Directions {
                left: number(4)?,
                right: number(12)?,
                forward: number(20)?,
                back: number(28)?,
            },
        })
    }
}

/// Requested movement independent of input devices or AI packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gait {
    Walk,
    FastWalk,
    Jog,
    Run,
    Sprint,
    SneakWalk,
    SneakRun,
}

/// A transient actor movement view for the host and package interpreter.
#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub walk: MovementType,
    pub run: MovementType,
    pub sprint: MovementType,
    pub sneak: MovementType,
    pub multiplier: f32,
    pub encumbered: bool,
}

impl Default for Profile {
    fn default() -> Self {
        use soe_physics::{FAST_WALK_SPEED, JOG_SPEED, RUN_SPEED, WALK_SPEED};
        Self {
            walk: MovementType {
                walk: Directions::uniform(WALK_SPEED),
                run: Directions::uniform(FAST_WALK_SPEED),
            },
            run: MovementType {
                walk: Directions::uniform(JOG_SPEED),
                run: Directions::uniform(RUN_SPEED),
            },
            sprint: MovementType {
                walk: Directions::uniform(RUN_SPEED),
                run: Directions::uniform(RUN_SPEED),
            },
            sneak: MovementType {
                walk: Directions::uniform(WALK_SPEED),
                run: Directions::uniform(WALK_SPEED),
            },
            multiplier: 1.0,
            encumbered: false,
        }
    }
}

impl Profile {
    /// Select a gait, then apply the live SpeedMult percentage.
    #[must_use]
    pub fn speed(&self, gait: Gait, right: f32, forward: f32) -> f32 {
        let [right, forward] = self.velocity(gait, right, forward);
        right.hypot(forward)
    }

    /// Local right and forward velocity components after directional scaling.
    /// Input magnitude is normalized, leaving analog strength to the caller.
    #[must_use]
    pub fn velocity(&self, gait: Gait, right: f32, forward: f32) -> [f32; 2] {
        let rates = if self.encumbered {
            match gait {
                Gait::SneakWalk | Gait::SneakRun => self.sneak.walk,
                _ => self.walk.walk,
            }
        } else {
            match gait {
                Gait::Walk => self.walk.walk,
                Gait::FastWalk => self.walk.run,
                Gait::Jog => self.run.walk,
                Gait::Run => self.run.run,
                Gait::Sprint => self.sprint.run,
                Gait::SneakWalk => self.sneak.walk,
                Gait::SneakRun => self.sneak.run,
            }
        };
        rates
            .velocity(right, forward)
            .map(|component| component * self.multiplier)
    }
}

impl World {
    /// Resolve the actor's current race movement defaults and matching overrides.
    /// Missing MOVT data uses the physics constants as a fallback.
    pub fn actor_movement(&mut self, actor: GlobalFormId) -> Profile {
        let Some((race, multiplier, encumbered)) = self.movement_modifiers(actor) else {
            return Profile::default();
        };
        let mut profile = Profile {
            multiplier,
            encumbered,
            ..Profile::default()
        };
        for (tag, target) in [
            (T(b"WKMV"), &mut profile.walk),
            (T(b"RNMV"), &mut profile.run),
            (T(b"SPMV"), &mut profile.sprint),
            (T(b"SNMV"), &mut profile.sneak),
        ] {
            if let Some(view) = race_movement(self, race, tag) {
                *target = view;
            }
        }
        profile
    }

    pub(crate) fn movement_modifiers(
        &mut self,
        actor: GlobalFormId,
    ) -> Option<(Option<GlobalFormId>, f32, bool)> {
        let state = self.actor(actor)?;
        let race = state.race;
        let multiplier = if state.values.contains("speedmult") {
            (state.values.get("speedmult").current() / 100.0).max(0.0)
        } else {
            1.0
        };
        let capacity = state
            .values
            .contains("carryweight")
            .then(|| state.values.get("carryweight").current());
        let weight = state
            .values
            .contains("inventoryweight")
            .then(|| state.values.get("inventoryweight").current());
        let encumbered = capacity.is_some_and(|capacity| {
            weight.unwrap_or_else(|| self.carried_weight(actor)) > capacity
        });
        Some((race, multiplier, encumbered))
    }
}

fn race_movement(world: &World, race: Option<GlobalFormId>, tag: Tag) -> Option<MovementType> {
    let movement = movement_type(world, race, tag)?;
    let mut view = MovementType::from_record(world.record(movement)?).ok()?;
    let Some((race, record)) = race.and_then(|race| Some((race, world.record(race)?))) else {
        return Some(view);
    };
    for (target, offset) in [
        (&mut view.walk.left, 0),
        (&mut view.run.left, 4),
        (&mut view.walk.right, 8),
        (&mut view.run.right, 12),
        (&mut view.walk.forward, 16),
        (&mut view.run.forward, 20),
        (&mut view.walk.back, 24),
        (&mut view.run.back, 28),
    ] {
        if let Some(override_speed) = race_override(world, race, record, movement, offset) {
            *target = override_speed;
        }
    }
    Some(view)
}

pub(crate) fn movement_type(
    world: &World,
    race: Option<GlobalFormId>,
    tag: Tag,
) -> Option<GlobalFormId> {
    if let Some(race) = race {
        let record = world
            .record(race)
            .filter(|record| record.tag == T(b"RACE") && !record.is_deleted())?;
        if let Some(raw) = record.find(tag).and_then(raw_form).filter(|raw| *raw != 0) {
            return world.resolve_from(race, raw);
        }
    }
    let key = match tag.as_bytes() {
        b"WKMV" => *b"DMWL",
        b"RNMV" => *b"DMRN",
        b"SPMV" => *b"DMSP",
        b"SNMV" => *b"DMSN",
        _ => return None,
    };
    crate::footsteps::Footsteps::new(world.order())
        .default_object(key)
        .ok()
        .flatten()
}

fn raw_form(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}

// CONTEXT: AI callers also accept a forward-only prefix, preserving their
// existing fixtures. The host validates the complete MOVT before using it.
pub(crate) fn race_override(
    world: &World,
    race: GlobalFormId,
    record: &Record,
    movement_type: GlobalFormId,
    offset: usize,
) -> Option<f32> {
    let mut current = None;
    for subrecord in &record.subrecords {
        if subrecord.tag == T(b"MTYP") {
            current = raw_form(&subrecord.data).and_then(|raw| world.resolve_from(race, raw));
        } else if subrecord.tag == T(b"SPED") && current == Some(movement_type) {
            return speed_at(&subrecord.data, offset);
        }
    }
    None
}

pub(crate) fn speed_at(data: &[u8], offset: usize) -> Option<f32> {
    let speed = f32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?);
    (speed.is_finite() && speed > 0.0).then_some(speed)
}
