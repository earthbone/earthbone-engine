// SPDX-License-Identifier: GPL-3.0-or-later

//! Player skill use and character level requirements from AVIF AVSK and GMST records.
//! The AVSK field order follows TES5Edit and UESP's AVIF layout. The improve
//! curve follows SKSE's `GetExperienceForLevel` implementation and the CK
//! `Game` and `ActorValueInfo` script contracts. Character thresholds use the
//! two XP-level GMSTs documented by the CK `Game` script and local SKSE source.

use std::collections::BTreeMap;

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::vm::state::{ArrayPool, ArrayTable, Writer};
use soe_papyrus::{Value, VmError};

use crate::actor_values::{actor_value_index, ACTOR_VALUE_NAMES};
use crate::world::save::FormReader;
use crate::world::{ActorState, World};

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FIRST_SKILL: usize = 6;
const SKILL_COUNT: usize = 18;
const MAX_SKILL: f32 = 100.0;

/// Failures while resolving or applying recorded skill use.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// The actor was not present in the runtime actor table.
    #[error("missing actor {0}")]
    Actor(GlobalFormId),
    /// The name did not identify one of the 18 skills.
    #[error("unknown skill {0}")]
    Skill(String),
    /// No matching winning AVIF record with valid skill data was found.
    #[error("missing or invalid AVIF for skill {0}")]
    ActorValueInfo(String),
    /// The required skill curve setting was absent or invalid.
    #[error("missing or invalid fSkillUseCurve")]
    Curve,
    /// The actor or level-up game settings could not produce a valid threshold.
    #[error("missing or invalid character level threshold")]
    LevelThreshold,
    /// No level-up choice was available or the selected attribute was invalid.
    #[error("invalid level-up choice")]
    LevelChoice,
    /// Usage or the resulting saved state was non-finite or out of range.
    #[error("invalid skill experience")]
    Experience,
}

/// Per-level skill usage multipliers from an AVIF `AVSK` subrecord.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SkillUse {
    use_mult: f32,
    use_offset: f32,
    improve_mult: f32,
    improve_offset: f32,
}

fn canonical_skill(name: &str) -> Option<&'static str> {
    let index = actor_value_index(name)?;
    let index = usize::try_from(index).ok()?;
    if !(FIRST_SKILL..FIRST_SKILL + SKILL_COUNT).contains(&index) {
        return None;
    }
    ACTOR_VALUE_NAMES.get(index).copied()
}

fn skill_use(world: &World, skill: &str) -> Result<SkillUse, Error> {
    let expected = format!("av{}", skill.to_ascii_lowercase());
    let record = world
        .order()
        .winners_of(T(b"AVIF"))
        .filter(|(_, record)| !record.is_deleted())
        .find(|(_, record)| {
            record
                .editor_id()
                .is_some_and(|id| id.eq_ignore_ascii_case(&expected))
        })
        .map(|(_, record)| record)
        .ok_or_else(|| Error::ActorValueInfo(skill.to_owned()))?;
    let data = record
        .find(T(b"AVSK"))
        .filter(|data| data.len() == 16)
        .ok_or_else(|| Error::ActorValueInfo(skill.to_owned()))?;
    let float = |at: usize| {
        let bytes: [u8; 4] = data.get(at..at + 4)?.try_into().ok()?;
        Some(f32::from_le_bytes(bytes))
    };
    let values = SkillUse {
        use_mult: float(0).ok_or_else(|| Error::ActorValueInfo(skill.to_owned()))?,
        use_offset: float(4).ok_or_else(|| Error::ActorValueInfo(skill.to_owned()))?,
        improve_mult: float(8).ok_or_else(|| Error::ActorValueInfo(skill.to_owned()))?,
        improve_offset: float(12).ok_or_else(|| Error::ActorValueInfo(skill.to_owned()))?,
    };
    if [
        values.use_mult,
        values.use_offset,
        values.improve_mult,
        values.improve_offset,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        return Err(Error::ActorValueInfo(skill.to_owned()));
    }
    Ok(values)
}

fn curve_setting(world: &World) -> Result<f32, Error> {
    let value = world.game_setting("fSkillUseCurve");
    match value {
        Some(Value::Float(value)) if value.is_finite() => Ok(value),
        Some(Value::Int(value)) => Ok(value as f32),
        _ => Err(Error::Curve),
    }
}

impl World {
    /// Experience required for this actor's next character level.
    ///
    /// The threshold uses `fXPLevelUpBase + level * fXPLevelUpMult`.
    ///
    /// # Errors
    /// Returns an error when the actor or either setting is absent or invalid.
    pub fn character_experience_to_next_level(&self, actor_id: GlobalFormId) -> Result<f32, Error> {
        let actor = self.actor_ref(actor_id).ok_or(Error::Actor(actor_id))?;
        let base = level_setting(self, "fXPLevelUpBase")?;
        let multiplier = level_setting(self, "fXPLevelUpMult")?;
        let threshold = base + actor.level as f32 * multiplier;
        if threshold.is_finite() && threshold > 0.0 {
            Ok(threshold)
        } else {
            Err(Error::LevelThreshold)
        }
    }

    /// Add character experience and award a choice for every crossed threshold.
    ///
    /// # Errors
    /// Rejects missing actors, invalid experience or invalid level settings
    /// without changing actor progression state.
    pub fn advance_character_experience(
        &mut self,
        actor_id: GlobalFormId,
        amount: f32,
    ) -> Result<(), Error> {
        if !amount.is_finite() || amount < 0.0 {
            return Err(Error::Experience);
        }
        self.actor(actor_id).ok_or(Error::Actor(actor_id))?;
        let base = level_setting(self, "fXPLevelUpBase")?;
        let multiplier = level_setting(self, "fXPLevelUpMult")?;
        let actor = self.actor_ref(actor_id).ok_or(Error::Actor(actor_id))?;
        if !actor.character_experience.is_finite() || actor.character_experience < 0.0 {
            return Err(Error::Experience);
        }
        let mut level = actor.level;
        let previous_level = level;
        let mut experience = actor.character_experience + amount;
        let mut choices = actor.level_up_choices;
        if !experience.is_finite() {
            return Err(Error::Experience);
        }
        while level < u32::MAX {
            let threshold = base + level as f32 * multiplier;
            if !threshold.is_finite() || threshold <= 0.0 {
                return Err(Error::LevelThreshold);
            }
            if experience < threshold {
                break;
            }
            if choices == u32::MAX {
                return Err(Error::LevelChoice);
            }
            experience -= threshold;
            level += 1;
            choices += 1;
        }
        let actor = self.actor(actor_id).ok_or(Error::Actor(actor_id))?;
        actor.level = level;
        actor.character_experience = experience;
        actor.level_up_choices = choices;
        if self.is_player_reference(actor_id) {
            self.player_level = level;
            if level > previous_level {
                for reached_level in previous_level + 1..=level {
                    let _ = self.commonlib_level_increase_at(reached_level);
                }
            }
        }
        Ok(())
    }

    /// Number of unspent attribute choices earned by this actor.
    #[must_use]
    pub fn level_up_choices(&self, actor_id: GlobalFormId) -> Option<u32> {
        self.actor_ref(actor_id).map(|actor| actor.level_up_choices)
    }

    /// Spend one available choice to increase Health, Magicka or Stamina by 10.
    ///
    /// # Errors
    /// Returns an error when no choice is available or the attribute is not one
    /// of the three level-up attributes. State is unchanged on failure.
    pub fn choose_level_up_attribute(
        &mut self,
        actor_id: GlobalFormId,
        attribute: &str,
    ) -> Result<(), Error> {
        let canonical = match attribute.to_ascii_lowercase().as_str() {
            "health" => "health",
            "magicka" => "magicka",
            "stamina" => "stamina",
            _ => return Err(Error::LevelChoice),
        };
        let actor = self.actor(actor_id).ok_or(Error::Actor(actor_id))?;
        if actor.level_up_choices == 0 {
            return Err(Error::LevelChoice);
        }
        let mut values = actor.values.clone();
        let next = values.get(canonical).base() + 10.0;
        values
            .set_base(canonical, next)
            .map_err(|_| Error::LevelChoice)?;
        actor.values = values;
        actor.level_up_choices -= 1;
        Ok(())
    }

    /// Add recorded use to one actor skill, applying every earned skill level.
    ///
    /// AVIF `AVSK` use multipliers produce usage experience. The skill's
    /// improve multiplier and offset, with `fSkillUseCurve`, set the amount
    /// needed at the current level. Skill levels stop at 100.
    ///
    /// # Errors
    /// Rejects invalid skills, missing actor/AVIF/curve data and non-finite
    /// inputs before changing actor state.
    pub fn advance_skill(
        &mut self,
        actor_id: GlobalFormId,
        name: &str,
        magnitude: f32,
    ) -> Result<(), Error> {
        let skill = canonical_skill(name).ok_or_else(|| Error::Skill(name.to_owned()))?;
        if !magnitude.is_finite() || magnitude < 0.0 {
            return Err(Error::Experience);
        }
        if magnitude == 0.0 {
            return Ok(());
        }
        let data = skill_use(self, skill)?;
        let curve = curve_setting(self)?;
        let gained = data.use_mult * magnitude + data.use_offset;
        if !gained.is_finite() || gained < 0.0 {
            return Err(Error::Experience);
        }
        self.actor(actor_id).ok_or(Error::Actor(actor_id))?;
        let actor = self.actor_ref(actor_id).ok_or(Error::Actor(actor_id))?;
        let level = actor.values.get(skill).base();
        let partial = actor.skill_experience.get(skill).copied().unwrap_or(0.0);
        if !level.is_finite() || !partial.is_finite() || partial < 0.0 {
            return Err(Error::Experience);
        }
        let mut next_level = level.min(MAX_SKILL);
        let mut next_experience = partial + gained;
        if !next_experience.is_finite() {
            return Err(Error::Experience);
        }
        while next_level < MAX_SKILL {
            let needed = data.improve_mult * next_level.powf(curve) + data.improve_offset;
            if !needed.is_finite() || needed <= 0.0 {
                return Err(Error::ActorValueInfo(skill.to_owned()));
            }
            if next_experience < needed {
                break;
            }
            next_experience -= needed;
            next_level = (next_level + 1.0).min(MAX_SKILL);
        }
        if next_level >= MAX_SKILL {
            next_experience = 0.0;
        }
        let actor = self.actor(actor_id).ok_or(Error::Actor(actor_id))?;
        actor
            .values
            .set_base(skill, next_level)
            .map_err(|_| Error::Experience)?;
        actor
            .skill_experience
            .insert(skill.to_owned(), next_experience);
        let skill_ranks_gained = next_level - level;
        if skill_ranks_gained > 0.0 && actor_id == self.player() {
            self.commonlib_skill_increase(skill);
        }
        if skill_ranks_gained > 0.0
            && level_setting(self, "fXPLevelUpBase").is_ok()
            && level_setting(self, "fXPLevelUpMult").is_ok()
        {
            self.advance_character_experience(actor_id, skill_ranks_gained)?;
        }
        Ok(())
    }

    /// Partial usage experience for one actor skill, or zero before first use.
    #[must_use]
    pub fn skill_experience(&self, actor_id: GlobalFormId, name: &str) -> Option<f32> {
        let skill = canonical_skill(name)?;
        self.actor_ref(actor_id)
            .map(|actor| actor.skill_experience.get(skill).copied().unwrap_or(0.0))
    }
}

fn level_setting(world: &World, name: &str) -> Result<f32, Error> {
    let value = world.game_setting(name);
    match value {
        Some(Value::Float(value)) if value.is_finite() => Ok(value),
        Some(Value::Int(value)) => Ok(value as f32),
        _ => Err(Error::LevelThreshold),
    }
}

/// Write the actor's partial skill use experience into its named save section.
pub(crate) fn write_skill_experience(
    actor: &ActorState,
    writer: &mut Writer,
    _pool: &mut ArrayPool,
) {
    writer.len(actor.skill_experience.len());
    for (name, experience) in &actor.skill_experience {
        writer.str(name);
        writer.f32(*experience);
    }
}

/// Restore partial skill experience and reject duplicate, invalid or non-skill entries.
pub(crate) fn read_skill_experience(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
    _table: &ArrayTable,
) -> Result<(), VmError> {
    let count = reader.len()?;
    let mut values = BTreeMap::new();
    for _ in 0..count {
        let name = reader.str()?.to_ascii_lowercase();
        let experience = reader.f32()?;
        if canonical_skill(&name).is_none()
            || !experience.is_finite()
            || experience < 0.0
            || values.insert(name.clone(), experience).is_some()
        {
            return Err(VmError::State(format!(
                "invalid PRG-01 skill experience {name}"
            )));
        }
    }
    actor.skill_experience = values;
    Ok(())
}

/// Write the actor's current-level experience and unspent choices.
pub(crate) fn write_character_progression(actor: &ActorState, writer: &mut Writer) {
    writer.f32(actor.character_experience);
    writer.u32(actor.level_up_choices);
}

/// Restore current-level experience and unspent choices.
pub(crate) fn read_character_progression(
    actor: &mut ActorState,
    reader: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    let experience = reader.f32()?;
    let choices = reader.u32()?;
    if !experience.is_finite() || experience < 0.0 {
        return Err(VmError::State("invalid PRG-07 character experience".into()));
    }
    actor.character_experience = experience;
    actor.level_up_choices = choices;
    Ok(())
}
