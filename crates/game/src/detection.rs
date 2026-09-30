// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared actor detection and stealth points.
//!
//! The detection value, its sound, visual and skill terms and the four
//! detection states follow the Creation Kit wiki pages `Category:Detection`
//! and `Stealth Points`. UESP's `Skyrim:Sneak` page substitutes the vanilla
//! constants into the same formula and applies distance attenuation once per
//! term, which is the reading used here. Every multiplier is a GMST read from
//! the load order, with runtime defaults in `game_settings` for the ones
//! Skyrim.esm omits. CommonLib's `CombatGroup` keeps stealth points per
//! target, and the CK page drives them from the highest detection of every
//! hostile observer, so one pool per target is shared by all observers.

use std::collections::BTreeMap;

use soe_esm::GlobalFormId;

/// Detection state of one observer toward one target, CK `Category:Detection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(u8)]
pub enum DetectionLevel {
    #[default]
    Normal = 0,
    Alert = 1,
    Combat = 2,
    Lost = 3,
}

impl DetectionLevel {
    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Self::Normal,
            1 => Self::Alert,
            2 => Self::Combat,
            3 => Self::Lost,
            _ => return None,
        })
    }
}

/// How the target moves during the observed step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Motion {
    #[default]
    Still,
    Walking,
    Running,
}

/// One observer and target pair as the host sensed it for a step.
///
/// Geometry, light and sound come from the host. Skills, worn armor weight,
/// muffle, blindness, invisibility, sneaking and combat targets are read from
/// the world's actor records when the step is evaluated.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Observation {
    pub observer: GlobalFormId,
    pub target: GlobalFormId,
    /// Distance between the two actors in game units.
    pub distance: f32,
    /// Whether the observer has line of sight to the target.
    pub line_of_sight: bool,
    /// Light level at the target as the host measures it, never negative.
    pub light_level: f32,
    pub motion: Motion,
    /// Loudness of the target's current action, such as a weapon swing.
    pub action_sound: f32,
    /// A separate emitted sound's distance from the observer. `None` means
    /// the target made the sound at its own position.
    pub action_sound_distance: Option<f32>,
    /// Sight to an emitted sound's source. `None` uses target sight.
    pub action_sound_line_of_sight: Option<bool>,
}

/// A one-step sound at a reference's location, credited to its actor owner.
/// The source need not be the owner, as a trap can make noise for an actor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionEvent {
    pub owner: GlobalFormId,
    pub cell: GlobalFormId,
    pub position: [f32; 3],
    pub sound_level: f32,
}

/// The actor record values one observation reads.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Senses {
    /// The observer's current Sneak actor value.
    pub observer_sneak: f32,
    /// The target's effective Sneak value, including active Disguise effects.
    pub target_sneak: f32,
    /// Summed weight of the target's worn armor records.
    pub armor_weight: f32,
    /// The target's `MovementNoiseMult`, 1 when the actor has none.
    pub muffle: f32,
    /// The observer's `Blindness` actor value.
    pub blindness: f32,
    /// Whether the target's `Invisibility` actor value is above zero.
    pub invisible: bool,
    /// Whether the observer is already Alert toward this target.
    pub observer_alert: bool,
    /// Whether the observer is in combat with some other actor.
    pub observer_busy: bool,
}

/// Detection and stealth-point game settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectionSettings {
    pub base_value: f32,
    pub max_distance: f32,
    pub attenuation_exponent: f32,
    pub sounds_mult: f32,
    pub sound_los_mult: f32,
    pub equipped_weight_base: f32,
    pub equipped_weight_mult: f32,
    pub running_mult: f32,
    pub action_mult: f32,
    pub light_move_mult: f32,
    pub light_run_mult: f32,
    pub light_mod: f32,
    pub light_mult: f32,
    pub stealth_boy_mult: f32,
    pub perception_skill_min: f32,
    pub perception_skill_max: f32,
    pub alert_mod: f32,
    pub combat_mod: f32,
    pub stealth_point_max: f32,
    pub drain_mult: f32,
    pub regen_mult: f32,
    pub regen_min: f32,
    pub alert_wait: f32,
    pub lost_wait: f32,
    pub detection_threshold: f32,
    pub sneak_detection_threshold: f32,
}

impl DetectionSettings {
    /// GMST name for each field, in field order.
    pub(crate) const NAMES: [&'static str; 26] = [
        "fSneakBaseValue",
        "fSneakMaxDistance",
        "fSneakDistanceAttenuationExponent",
        "fSneakSoundsMult",
        "fSneakSoundLosMult",
        "fSneakEquippedWeightBase",
        "fSneakEquippedWeightMult",
        "fSneakRunningMult",
        "fSneakActionMult",
        "fSneakLightMoveMult",
        "fSneakLightRunMult",
        "fDetectionSneakLightMod",
        "fSneakLightMult",
        "fSneakStealthBoyMult",
        "fSneakPerceptionSkillMin",
        "fSneakPerceptionSkillMax",
        "fSneakAlertMod",
        "fSneakCombatMod",
        "fCombatStealthPointMax",
        "fCombatStealthPointDrainMult",
        "fCombatStealthPointRegenMult",
        "fCombatStealthPointRegenMin",
        "fCombatStealthPointRegenAlertWaitTime",
        "fCombatStealthPointRegenLostWaitTime",
        "iCombatStealthPointDetectionThreshold",
        "iCombatStealthPointSneakDetectionThreshold",
    ];

    /// Build settings from values in [`Self::NAMES`] order.
    ///
    /// # Errors
    /// A non-finite value, a distance or pool maximum that is not positive, or
    /// a negative wait, rate or exponent, named by its GMST.
    pub fn from_values(v: [f32; 26]) -> Result<Self, Error> {
        if let Some(i) = v.iter().position(|x| !x.is_finite()) {
            return Err(Error::Setting(Self::NAMES[i]));
        }
        let s = Self {
            base_value: v[0],
            max_distance: v[1],
            attenuation_exponent: v[2],
            sounds_mult: v[3],
            sound_los_mult: v[4],
            equipped_weight_base: v[5],
            equipped_weight_mult: v[6],
            running_mult: v[7],
            action_mult: v[8],
            light_move_mult: v[9],
            light_run_mult: v[10],
            light_mod: v[11],
            light_mult: v[12],
            stealth_boy_mult: v[13],
            perception_skill_min: v[14],
            perception_skill_max: v[15],
            alert_mod: v[16],
            combat_mod: v[17],
            stealth_point_max: v[18],
            drain_mult: v[19],
            regen_mult: v[20],
            regen_min: v[21],
            alert_wait: v[22],
            lost_wait: v[23],
            detection_threshold: v[24],
            sneak_detection_threshold: v[25],
        };
        let checks = [
            (s.max_distance > 0.0, 1),
            (s.attenuation_exponent >= 0.0, 2),
            (s.stealth_point_max > 0.0, 18),
            (s.drain_mult >= 0.0, 19),
            (s.regen_mult >= 0.0, 20),
            (s.regen_min >= 0.0, 21),
            (s.alert_wait >= 0.0, 22),
            (s.lost_wait >= 0.0, 23),
        ];
        if let Some((_, i)) = checks.iter().find(|(ok, _)| !ok) {
            return Err(Error::Setting(Self::NAMES[*i]));
        }
        Ok(s)
    }
}

/// Detection value of one observation, CK `Category:Detection`.
///
/// `fSneakBaseValue + (sound + visual + observer skill) * attenuation +
/// (observer skill - target skill)`. The target skill factor has no
/// definition on the CK page. It is read here as the same perception scale
/// applied to the target's Sneak without the observer's state modifiers.
///
/// # Errors
/// [`Error::Number`] when the result is not finite.
pub fn detection_value(
    s: &DetectionSettings,
    o: &Observation,
    senses: &Senses,
) -> Result<f32, Error> {
    let attenuation = if o.distance >= s.max_distance {
        0.0
    } else {
        ((s.max_distance - o.distance) / s.max_distance).powf(s.attenuation_exponent)
    };
    let moving = o.motion != Motion::Still;
    let running = o.motion == Motion::Running;

    let movement = if moving {
        let weight = s.equipped_weight_base + s.equipped_weight_mult * senses.armor_weight;
        let run = if running { s.running_mult } else { 1.0 };
        weight * run * senses.muffle.max(0.0)
    } else {
        0.0
    };
    let action = o.action_sound * s.action_mult;
    let los = if o.line_of_sight {
        1.0
    } else {
        s.sound_los_mult
    };
    let sound = s.sounds_mult * movement * los;
    let event_distance = o.action_sound_distance.unwrap_or(o.distance);
    let event_attenuation = if event_distance >= s.max_distance {
        0.0
    } else {
        ((s.max_distance - event_distance) / s.max_distance).powf(s.attenuation_exponent)
    };
    let event_los = if o.action_sound_line_of_sight.unwrap_or(o.line_of_sight) {
        1.0
    } else {
        s.sound_los_mult
    };
    let action_sound = s.sounds_mult * action * event_los * event_attenuation;

    let visual = if o.line_of_sight {
        let motion_mult = 1.0
            + f32::from(u8::from(moving)) * s.light_move_mult
            + f32::from(u8::from(running)) * s.light_run_mult;
        let light = (s.light_mod + o.light_level) * s.light_mult;
        let blind = (100.0 - senses.blindness.clamp(0.0, 100.0)) / 100.0;
        let invisible = if senses.invisible {
            s.stealth_boy_mult
        } else {
            1.0
        };
        light * motion_mult * blind * invisible
    } else {
        0.0
    };

    let scale = |sneak: f32| {
        s.perception_skill_min + (s.perception_skill_max - s.perception_skill_min) * sneak / 100.0
    };
    let modifier = 1.0
        + f32::from(u8::from(senses.observer_alert)) * s.alert_mod
        + f32::from(u8::from(senses.observer_busy)) * s.combat_mod;
    let observer_skill = scale(senses.observer_sneak) * modifier;
    let target_skill = scale(senses.target_sneak);

    let value = s.base_value
        + (sound + visual + observer_skill) * attenuation
        + action_sound
        + (observer_skill - target_skill);
    if value.is_finite() {
        Ok(value)
    } else {
        Err(Error::Number)
    }
}

/// One observer's standing toward a target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Awareness {
    pub level: DetectionLevel,
    /// Detection value from the observer's last evaluated observation.
    pub detection: f32,
    /// Seconds the target has stayed undetected while this observer is in
    /// Combat, counted toward `fCombatStealthPointRegenLostWaitTime`.
    pub lost_timer: f32,
}

/// A target's stealth points and the observers that share them.
#[derive(Debug, Clone, PartialEq)]
pub struct StealthPool {
    /// Remaining stealth points, 0 to `fCombatStealthPointMax`.
    pub points: f32,
    /// Seconds left before the pool may refill after an observer became Alert.
    pub alert_wait: f32,
    /// Awareness by observer.
    pub observers: BTreeMap<GlobalFormId, Awareness>,
}

impl StealthPool {
    pub(crate) const fn full(s: &DetectionSettings) -> Self {
        Self {
            points: s.stealth_point_max,
            alert_wait: 0.0,
            observers: BTreeMap::new(),
        }
    }

    /// Advance the shared pool and then every evaluated observer.
    ///
    /// `values` holds the observer and detection value of each observation of
    /// this target in the step.
    pub(crate) fn step(
        &mut self,
        s: &DetectionSettings,
        values: &[(GlobalFormId, f32)],
        target_sneaking: bool,
        elapsed: f32,
    ) {
        let highest = values
            .iter()
            .map(|(_, d)| *d)
            .fold(f32::NEG_INFINITY, f32::max);
        // Stealth Points always = 0 when enemies are in combat with you.
        let held = self
            .observers
            .values()
            .any(|a| a.level == DetectionLevel::Combat);
        if held {
            self.points = 0.0;
        } else if highest > 0.0 {
            self.points = (self.points - highest * s.drain_mult * elapsed).max(0.0);
            self.alert_wait = s.alert_wait;
        } else if self.alert_wait > 0.0 {
            self.alert_wait = (self.alert_wait - elapsed).max(0.0);
        } else if highest < 0.0 {
            let rate = (-highest * s.regen_mult).max(s.regen_min);
            self.points = (self.points + rate * elapsed).min(s.stealth_point_max);
        }

        let threshold = if target_sneaking {
            s.sneak_detection_threshold
        } else {
            s.detection_threshold
        };
        let depleted = self.points <= 0.0;
        let refilled = self.points >= s.stealth_point_max;
        let mut lost = false;
        for (observer, d) in values {
            let a = self.observers.entry(*observer).or_insert(Awareness {
                level: DetectionLevel::Normal,
                detection: *d,
                lost_timer: 0.0,
            });
            a.detection = *d;
            let found = *d > threshold || (depleted && *d > 0.0);
            a.level = match a.level {
                DetectionLevel::Normal | DetectionLevel::Alert | DetectionLevel::Lost if found => {
                    DetectionLevel::Combat
                }
                DetectionLevel::Normal if *d > 0.0 => DetectionLevel::Alert,
                DetectionLevel::Alert | DetectionLevel::Lost if *d < 0.0 && refilled => {
                    DetectionLevel::Normal
                }
                DetectionLevel::Combat if *d > 0.0 => {
                    a.lost_timer = 0.0;
                    DetectionLevel::Combat
                }
                DetectionLevel::Combat => {
                    a.lost_timer += elapsed;
                    if a.lost_timer >= s.lost_wait {
                        a.lost_timer = 0.0;
                        lost = true;
                        DetectionLevel::Lost
                    } else {
                        DetectionLevel::Combat
                    }
                }
                level => level,
            };
        }
        // Stealth Points: once the combat loss timer expires with detection
        // still below zero, the points begin to increase.
        if lost && highest <= 0.0 {
            self.alert_wait = 0.0;
        }
        // The combat hold also covers an observer that entered Combat in this
        // step, so the pool reads zero on the step combat starts.
        if self
            .observers
            .values()
            .any(|a| a.level == DetectionLevel::Combat)
        {
            self.points = 0.0;
        }
    }
}

/// Every stealth-point pool, keyed by target.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DetectionState {
    pub(crate) pools: BTreeMap<GlobalFormId, StealthPool>,
}

impl DetectionState {
    pub fn pool(&self, target: GlobalFormId) -> Option<&StealthPool> {
        self.pools.get(&target)
    }

    pub fn is_empty(&self) -> bool {
        self.pools.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid detection actor {0}")]
    Actor(GlobalFormId),
    #[error("actor {0} cannot observe itself")]
    SelfObservation(GlobalFormId),
    #[error("observer {observer} observes target {target} twice in one step")]
    Duplicate {
        observer: GlobalFormId,
        target: GlobalFormId,
    },
    #[error("negative or non-finite observation or elapsed time")]
    Observation,
    #[error("missing or invalid detection game setting {0}")]
    Setting(&'static str),
    #[error("invalid worn armor record {0}")]
    Armor(GlobalFormId),
    #[error("non-finite detection calculation")]
    Number,
}
