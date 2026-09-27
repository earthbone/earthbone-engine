// SPDX-License-Identifier: GPL-3.0-or-later

//! RACE attribute prefix from xEdit wbRecord(RACE)/DATA and UESP RACE.
//! The record remains authoritative. This view supplies initial actor values
//! and unarmed inputs, not an independent race database.

use soe_esm::{Error, GlobalFormId, Record, Tag};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attributes {
    /// Up to 7 `(skill actor value index, boost)` pairs, `DATA` bytes 0 to
    /// 13. The index is a full actor value (xEdit `wbActorValueEnum`), where
    /// skills occupy 6 to 23, 6 lower than [`crate::npc::SKILLS`]'s own
    /// order. An unused slot carries a zero boost and is skipped by callers.
    pub skill_boosts: [(i8, i8); 7],
    pub health: f32,
    pub magicka: f32,
    pub stamina: f32,
    pub carry_weight: f32,
    pub mass: f32,
    pub heal_rate: f32,
    pub magicka_rate: f32,
    pub stamina_rate: f32,
    pub unarmed_damage: f32,
    pub unarmed_reach: f32,
    /// Height multiplier, male and female, `DATA` offsets 16 and 20. Used
    /// with an NPC_'s own `NAM6` the same way `TESNPC::GetHeight()` does,
    /// `race->data.height[sex] * npc.height`, for a runtime-generated actor
    /// controller capsule when the skeleton has no authored one.
    pub height_male: f32,
    pub height_female: f32,
    /// Weight multiplier, male and female, `DATA` offsets 24 and 28.
    pub weight_male: f32,
    pub weight_female: f32,
}

/// Sex-specific animation assets named by a `RACE` record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnimationModels {
    pub male_skeleton: Option<String>,
    pub female_skeleton: Option<String>,
    pub male_behavior: Option<String>,
    pub female_behavior: Option<String>,
}

impl AnimationModels {
    /// Read skeletal `ANAM` strings and behavior graph `MODL` paths in the
    /// order xEdit places them under the male and female markers.
    ///
    /// # Errors
    /// Rejects a record whose type is not `RACE`.
    pub fn from_record(record: &Record) -> Result<Self, Error> {
        if record.tag != Tag(*b"RACE") {
            return Err(Error::Invalid("race animation models require RACE".into()));
        }
        let mut models = Self::default();
        let mut female = false;
        for subrecord in &record.subrecords {
            match subrecord.tag.as_bytes() {
                b"MNAM" => female = false,
                b"FNAM" => female = true,
                b"ANAM" => {
                    let path = nonempty_path(&subrecord.data);
                    if female {
                        models.female_skeleton = path;
                    } else {
                        models.male_skeleton = path;
                    }
                }
                b"MODL" => {
                    let path = nonempty_path(&subrecord.data);
                    if female {
                        models.female_behavior = path;
                    } else {
                        models.male_behavior = path;
                    }
                }
                _ => {}
            }
        }
        Ok(models)
    }

    #[must_use]
    pub fn select(&self, female: bool) -> AnimationRig {
        AnimationRig {
            skeleton: if female {
                self.female_skeleton.clone()
            } else {
                self.male_skeleton.clone()
            },
            behavior: if female {
                self.female_behavior.clone()
            } else {
                self.male_behavior.clone()
            },
        }
    }
}

/// The selected race rig for an actor's current sex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationRig {
    pub skeleton: Option<String>,
    pub behavior: Option<String>,
}

/// Record identity and the rig selected for an actor's current race and sex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActorAnimationRig {
    pub race: GlobalFormId,
    pub female: bool,
    pub skeleton: Option<String>,
    pub behavior: Option<String>,
}

impl crate::World {
    /// Select the animation assets authored by the actor's current race.
    pub fn actor_animation_rig(
        &mut self,
        actor: GlobalFormId,
    ) -> Result<Option<ActorAnimationRig>, Error> {
        let current_race = self
            .actor(actor)
            .ok_or_else(|| Error::Invalid(format!("animation rig requires an actor: {actor}")))?
            .race;
        let base = self.reference(actor).and_then(|reference| reference.base);
        let (traits_race, female) = match base {
            Some(_) => {
                let traits = self.actor_component_source(actor, crate::npc::template::TRAITS)?;
                (
                    self.actor_base_race(traits)?,
                    self.npc_record(traits)?.is_female(),
                )
            }
            None => (None, false),
        };
        let race = current_race.or(traits_race);
        let Some(race) = race else {
            return Ok(None);
        };
        let record = self
            .record(race)
            .ok_or_else(|| Error::Invalid(format!("missing RACE {race}")))?;
        let models = AnimationModels::from_record(record)?;
        let selected = models.select(female);
        Ok(Some(ActorAnimationRig {
            race,
            female,
            skeleton: selected.skeleton,
            behavior: selected.behavior,
        }))
    }
}

fn nonempty_path(bytes: &[u8]) -> Option<String> {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    let value = std::str::from_utf8(&bytes[..end]).ok()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

impl Attributes {
    /// Read the mandatory DATA prefix. Absence remains distinguishable from zero.
    ///
    /// # Errors
    /// Rejects a wrong record type, short DATA or non-finite attribute.
    pub fn from_record(record: &Record) -> Result<Option<Self>, Error> {
        if record.tag != Tag(*b"RACE") {
            return Err(Error::Invalid("race attributes require RACE".into()));
        }
        let Some(data) = record.find(Tag(*b"DATA")) else {
            return Ok(None);
        };
        if data.len() < 124 {
            return Err(Error::Invalid("short RACE DATA".into()));
        }
        let field = |offset| -> Result<f32, Error> {
            let bytes = data
                .get(offset..offset + 4)
                .ok_or_else(|| Error::Invalid("short RACE attribute".into()))?;
            let value = f32::from_le_bytes(
                bytes
                    .try_into()
                    .map_err(|_| Error::Invalid("short RACE attribute".into()))?,
            );
            if !value.is_finite() {
                return Err(Error::Invalid("non-finite RACE attribute".into()));
            }
            Ok(value)
        };
        let mut skill_boosts = [(0i8, 0i8); 7];
        for (i, slot) in skill_boosts.iter_mut().enumerate() {
            let base = i * 2;
            *slot = (data[base] as i8, data[base + 1] as i8);
        }
        Ok(Some(Self {
            skill_boosts,
            health: field(36)?,
            magicka: field(40)?,
            stamina: field(44)?,
            carry_weight: field(48)?,
            mass: field(52)?,
            heal_rate: field(84)?,
            magicka_rate: field(88)?,
            stamina_rate: field(92)?,
            unarmed_damage: field(96)?,
            unarmed_reach: field(100)?,
            height_male: field(16)?,
            height_female: field(20)?,
            weight_male: field(24)?,
            weight_female: field(28)?,
        }))
    }
}
