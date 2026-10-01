// SPDX-License-Identifier: GPL-3.0-or-later

//! Trainer purchases read NPC_ CNAM and CLAS DATA from the winning load order.
//! TES5Edit, UESP CLAS, CommonLibSSE-NG PlayerCharacter and the CK Game script
//! establish trainer fields, the per-level session counter and menu entry.

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::Writer;
use soe_papyrus::{Value, VmError};

use super::save::FormReader;
use super::{le_u32, Alloc, World, T};
use crate::actor_values::ACTOR_VALUE_NAMES;
use crate::class::Class;
use crate::inventory::StackSource;

const GOLD_ID: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0xF,
};
const TRAINING_SESSIONS_PER_LEVEL: u32 = 5;
const TRAINER_SKILL_CAPS: [f32; 5] = [25.0, 50.0, 75.0, 90.0, 100.0];

/// Values displayed by the trainer menu for one purchase.
#[derive(Debug, Clone, PartialEq)]
pub struct Offer {
    pub skill: &'static str,
    pub current: f32,
    pub cap: f32,
    pub cost: i32,
    pub sessions_remaining: u32,
}

/// Training purchase failures.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    /// The player or trainer reference could not be resolved.
    #[error("missing training actor")]
    Actor,
    /// The trainer did not have a valid class that teaches this skill.
    #[error("actor is not a valid trainer")]
    Trainer,
    /// Training has reached its current skill or per-level limit.
    #[error("training limit reached")]
    Limit,
    /// The training price setting or calculated price was invalid.
    #[error("invalid training cost")]
    Cost,
    /// The player did not carry enough gold.
    #[error("not enough gold for training")]
    Gold,
    /// The skill value could not be changed.
    #[error("invalid skill value")]
    Skill,
    /// The inventory transfer failed.
    #[error("training payment failed: {0}")]
    Inventory(String),
}

fn cost_multiplier(world: &World) -> Result<f32, Error> {
    match world.game_setting("fTrainingMultCost") {
        Some(Value::Float(value)) if value.is_finite() && value > 0.0 => Ok(value),
        Some(Value::Int(value)) if value > 0 => Ok(value as f32),
        _ => Err(Error::Cost),
    }
}

fn trainer_class(world: &World, trainer: GlobalFormId) -> Result<Class, Error> {
    let reference = world.reference_ref(trainer).ok_or(Error::Actor)?;
    if !reference.is_actor {
        return Err(Error::Trainer);
    }
    let base = reference.base.ok_or(Error::Trainer)?;
    let npc = world
        .record(base)
        .filter(|record| record.tag == T(b"NPC_") && !record.is_deleted())
        .ok_or(Error::Trainer)?;
    let class_link = npc
        .find(T(b"CNAM"))
        .and_then(le_u32)
        .ok_or(Error::Trainer)?;
    let class_id = world.resolve_from(base, class_link).ok_or(Error::Trainer)?;
    let record = world
        .record(class_id)
        .filter(|record| !record.is_deleted())
        .ok_or(Error::Trainer)?;
    Class::from_record(record)
        .ok()
        .flatten()
        .filter(|class| class.teaches < 18 && (1..=5).contains(&class.maximum_training_level))
        .ok_or(Error::Trainer)
}

impl World {
    /// Read the trainer's skill, current price and remaining level sessions.
    pub fn training_offer(&mut self, trainer: GlobalFormId) -> Result<Offer, Error> {
        let class = trainer_class(self, trainer)?;
        let player = self.player();
        let level = self.actor_ref(player).ok_or(Error::Actor)?.level;
        let used = if self.training_session_level == level {
            self.training_sessions_this_level
        } else {
            0
        };
        let skill = ACTOR_VALUE_NAMES
            .get(6 + usize::from(class.teaches))
            .copied()
            .ok_or(Error::Trainer)?;
        let current = self.base_actor_value(player, skill);
        let cap = TRAINER_SKILL_CAPS[usize::from(class.maximum_training_level - 1)];
        if !current.is_finite() || current < 0.0 {
            return Err(Error::Limit);
        }
        let raw_cost = current * cost_multiplier(self)?;
        if !raw_cost.is_finite() || raw_cost <= 0.0 || raw_cost.ceil() > i32::MAX as f32 {
            return Err(Error::Cost);
        }
        Ok(Offer {
            skill,
            current,
            cap,
            cost: raw_cost.ceil() as i32,
            sessions_remaining: TRAINING_SESSIONS_PER_LEVEL - used,
        })
    }

    /// Queue a Papyrus Game.ShowTrainingMenu request for the host presenter.
    pub fn request_training_menu(&mut self, trainer: GlobalFormId) -> bool {
        if trainer_class(self, trainer).is_err() {
            return false;
        }
        self.pending_training_menu = Some(trainer);
        true
    }

    /// Take the pending trainer menu request, if one was queued by Papyrus.
    pub fn take_training_menu_request(&mut self) -> Option<GlobalFormId> {
        self.pending_training_menu.take()
    }

    /// Buy one training session from an actor whose class teaches a skill.
    ///
    /// The player can buy five sessions per character level. The class's
    /// training tier caps the skill at 25, 50, 75, 90 or 100. The price is
    /// the current skill level times `fTrainingMultCost`, rounded up.
    ///
    /// # Errors
    /// Rejects invalid trainers, exhausted limits, invalid settings, or
    /// insufficient gold before changing the skill, inventory or session count.
    pub fn purchase_training(
        &mut self,
        trainer: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Result<i32, Error> {
        let class = trainer_class(self, trainer)?;
        let player = self.player();
        let actor = self.actor_ref(player).ok_or(Error::Actor)?;
        let level = actor.level;
        let used = if self.training_session_level == level {
            self.training_sessions_this_level
        } else {
            0
        };
        if used >= TRAINING_SESSIONS_PER_LEVEL {
            return Err(Error::Limit);
        }

        let skill = ACTOR_VALUE_NAMES
            .get(6 + usize::from(class.teaches))
            .copied()
            .ok_or(Error::Trainer)?;
        let current = self.base_actor_value(player, skill);
        let cap = TRAINER_SKILL_CAPS[usize::from(class.maximum_training_level - 1)];
        if !current.is_finite() || current < 0.0 || current + 1.0 > cap {
            return Err(Error::Limit);
        }
        let raw_cost = current * cost_multiplier(self)?;
        if !raw_cost.is_finite() || raw_cost <= 0.0 || raw_cost.ceil() > i32::MAX as f32 {
            return Err(Error::Cost);
        }
        let price = raw_cost.ceil() as i32;
        let gold = self.form("Gold001").unwrap_or(GOLD_ID);
        if self.item_count(player, gold) < price {
            return Err(Error::Gold);
        }

        let mut values = self.actor_ref(player).ok_or(Error::Actor)?.values.clone();
        values
            .set_base(skill, current + 1.0)
            .map_err(|_| Error::Skill)?;
        let selected = self
            .select_item(player, gold, StackSource::Plain, price)
            .map_err(|error| Error::Inventory(error.to_string()))?;

        self.remove_selected_notified(player, &selected, Some(trainer), alloc)
            .map_err(|error| Error::Inventory(error.to_string()))?;
        self.actor(player).ok_or(Error::Actor)?.values = values;
        self.training_session_level = level;
        self.training_sessions_this_level = used + 1;
        Ok(price)
    }
}

/// Write the player's session count and the level it belongs to.
pub(crate) fn write_training_sessions(world: &World, writer: &mut Writer) {
    writer.u32(world.training_session_level);
    writer.u32(world.training_sessions_this_level);
}

/// Restore the player's session count for its saved character level.
pub(crate) fn read_training_sessions(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
) -> Result<(), VmError> {
    let level = reader.u32()?;
    let sessions = reader.u32()?;
    if sessions > TRAINING_SESSIONS_PER_LEVEL {
        return Err(VmError::State("invalid DLG-16 training sessions".into()));
    }
    world.training_session_level = level;
    world.training_sessions_this_level = sessions;
    Ok(())
}
