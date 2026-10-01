// SPDX-License-Identifier: GPL-3.0-or-later

//! Active spell applications, following CK Magic Effect, DispelSpell and
//! DispelAllSpells contracts, xEdit SPEL/MGEF fields and CommonLib MagicSystem
//! enums. Definitions stay in the winning records. Per-cast magnitudes,
//! elapsed time and reversible actor-value deltas belong to the world.
//! Unsupported delivery and archetypes return errors before applying effects.
//! Vampire Lord transformations use their associated race and own its ability changes.
//! Invisibility follows xEdit's MGEF archetype table and UESP's MGEF
//! layout. Lock effects update eligible door and container references through
//! the shared lock state. Definitions stay in the winning records. Per-cast
//! magnitudes, elapsed time and reversible actor-value deltas belong to the world.
//! Cure Disease removes active MGEFs marked with MagicDisease. Disarm drops
//! one eligible hand weapon as a loose reference. Unsupported delivery and
//! archetypes return errors before applying effects. Aimed delivery follows
//! the SPIT field in xEdit and applies only through an actor target supplied
//! by a projectile contact. Target-location delivery uses the selected point
//! as the center for authored area effects.

use std::collections::{BTreeMap, BTreeSet};

use soe_esm::records::gameplay::{Effect, Ingestible, MagicEffect, Spell};
use soe_esm::{GlobalFormId, Record, Tag};
use soe_papyrus::{ObjectId, Value};

use crate::actor_values::actor_value_name;
use crate::combat::Hand;
use crate::inventory::{Error as InventoryError, Selection};

use crate::conditions::{self, Context};

use super::save::{read_form_id, read_opt_form_id, write_form_id, write_opt_form_id, FormReader};
use super::{Alloc, Handle, Pending, World, T};

mod state;
pub(crate) use state::SECTION as SAVE_SECTION;
pub(super) use state::{read_magic, read_werewolf_transform, write_werewolf_transform};

const HOSTILE: u32 = 1;
const RECOVER: u32 = 1 << 1;
const DETRIMENTAL: u32 = 1 << 2;
const NO_HIT_EVENT: u32 = 1 << 4;
const NO_DURATION: u32 = 1 << 9;
const NO_MAGNITUDE: u32 = 1 << 10;
const NO_AREA: u32 = 1 << 11;
const NO_RECAST: u32 = 1 << 17;
const NO_DEATH_DISPEL: u32 = 1 << 28;
const WEAPON_BOUND: u32 = 1 << 13;
pub(super) const DISPEL_WITH_KEYWORDS: u32 = 1 << 8;
const INGESTIBLE_POISON: u32 = 1 << 17;
const PARALYSIS_ARCHETYPE: u32 = 21;
const CURE_PARALYSIS_ARCHETYPE: u32 = 27;
const DEMORALIZE: u32 = 7;
const FRENZY_ARCHETYPE: u32 = 8;
const TURN_UNDEAD: u32 = 24;
const INVISIBILITY: u32 = 11;
const LIGHT: u32 = 12;
const LOCK: u32 = 15;
const OPEN: u32 = 16;
const MAX_RESISTANCE: f32 = 85.0;
const RESIST_MAGIC: i32 = 44;
const FIRST_SPECIFIC_RESISTANCE: i32 = 40;
const LAST_SPECIFIC_RESISTANCE: i32 = 45;
const IGNORE_RESISTANCE: u32 = 1 << 20;
const PEAK_VALUE_MODIFIER: u32 = 34;
const CLOAK_CADENCE: f64 = 1.0;
const SLOW_TIME: u32 = 37;

/// Identity of one effect application, independent of its MGEF FormID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectId(pub u64);

/// A spell waiting for its release input or held for concentration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellCast {
    pub spell: GlobalFormId,
    pub target: Option<GlobalFormId>,
    pub kind: SpellCastKind,
}

/// The cast types that have an interruptible live phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpellCastKind {
    FireAndForget,
    Concentration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PriorOwner {
    Unowned,
    Owned(GlobalFormId),
}

/// Mutable state of one application. Actor values include the recoverable
/// deltas until this instance ends, so loading must never apply them again.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveEffect {
    pub id: EffectId,
    pub spell: GlobalFormId,
    pub effect: GlobalFormId,
    pub effect_index: u32,
    pub source: GlobalFormId,
    pub caster: Option<GlobalFormId>,
    pub target: GlobalFormId,
    pub magnitude: f32,
    pub duration: Option<f64>,
    pub elapsed: f64,
    /// Runtime actor created by a Summon Creature effect, dismissed with it.
    pub summoned: Option<GlobalFormId>,
    /// Previous runtime owner while this effect commands a summoned actor.
    pub(crate) restore_owner: Option<PriorOwner>,
    pub(crate) recover: bool,
    pub(crate) changes: Vec<(String, f32)>,
}

/// State owned by one live Vampire Lord transformation effect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VampireLordForm {
    pub actor: GlobalFormId,
    pub previous_race: Option<GlobalFormId>,
    pub vampire_race: GlobalFormId,
    pub added_spells: std::collections::BTreeSet<GlobalFormId>,
    pub ability_effects: Vec<EffectId>,
}

/// One concentration spell held by its caster until release or exhaustion.
#[derive(Debug, Clone, PartialEq)]
pub struct ConcentrationCast {
    spell: GlobalFormId,
    source: GlobalFormId,
    caster: GlobalFormId,
    target: Option<GlobalFormId>,
    cost_per_second: f64,
}

/// Pulse phase and applications last created by one Cloak effect.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct CloakState {
    pub(crate) elapsed: f64,
    pub(crate) owned: Vec<EffectId>,
}

/// A spell was rejected before any of its effects were applied.
#[derive(Debug, thiserror::Error)]
pub enum MagicError {
    #[error("missing magic record {0:?}")]
    Missing(GlobalFormId),
    #[error("invalid magic record: {0}")]
    Record(#[from] soe_esm::Error),
    #[error("magic effect condition: {0}")]
    Condition(#[from] conditions::Error),
    #[error("unsupported magic behavior: {0}")]
    Unsupported(&'static str),
    #[error("spell requires an actor target")]
    Target,
    #[error("invalid spell magnitude or duration")]
    Number,
    #[error("caster does not have enough magicka")]
    InsufficientMagicka,
    #[error("active effect identity exhausted")]
    Identity,
    #[error("inventory selection: {0}")]
    Inventory(#[from] InventoryError),
    #[error("missing or invalid stagger game setting {0}")]
    StaggerSetting(&'static str),
}

#[derive(Clone)]
enum DispelRule {
    Timed,
    Disease,
    Keywords(Vec<GlobalFormId>),
    Paralysis,
    Poisons,
}

struct PreparedEffect {
    effect: ActiveEffect,
    summon_base: Option<GlobalFormId>,
    werewolf_race: Option<(GlobalFormId, BTreeSet<GlobalFormId>, BTreeSet<GlobalFormId>)>,
    command_owner: Option<GlobalFormId>,
    light_base: Option<GlobalFormId>,
    bound_weapon: Option<GlobalFormId>,
    lock_level: Option<i32>,
    open_lock: bool,
    disarm: bool,
    stagger_duration: Option<f64>,
    no_recast: bool,
    hit_event: bool,
    dispel_rules: Vec<DispelRule>,
    dispel: Vec<EffectId>,
    vampire_form: Option<(GlobalFormId, std::collections::BTreeSet<GlobalFormId>)>,
}

pub(super) struct PreparedWeaponEnchantment(Vec<PreparedEffect>);
#[derive(Clone, Copy, PartialEq, Eq)]
enum CastDelivery {
    Direct,
    ProjectileContact,
    CloakPulse,
    Location,
    Scroll,
}

impl World {
    /// Begin a fire-and-forget charge or a held concentration cast.
    /// Starting a cast does not apply effects or spend magicka.
    ///
    /// # Errors
    /// Returns an error for a missing spell or a cast type without an
    /// interruptible live phase.
    pub fn begin_spell_cast(
        &mut self,
        caster: GlobalFormId,
        spell: GlobalFormId,
        target: Option<GlobalFormId>,
    ) -> Result<(), MagicError> {
        let definition = self.spell(spell)?;
        let kind = match definition.data.cast_type {
            1 => SpellCastKind::FireAndForget,
            2 => SpellCastKind::Concentration,
            _ => return Err(MagicError::Unsupported("non-interruptible cast type")),
        };
        self.spell_casts_in_progress.insert(
            caster,
            SpellCast {
                spell,
                target,
                kind,
            },
        );
        Ok(())
    }

    /// Release a charged fire-and-forget cast through normal spell effects.
    /// The cast remains pending if effect preparation fails.
    ///
    /// # Errors
    /// Returns an error when there is no pending cast, when the cast is not
    /// fire-and-forget, or when the existing spell execution path rejects it.
    pub fn release_spell_cast(
        &mut self,
        caster: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let cast = self
            .spell_casts_in_progress
            .get(&caster)
            .copied()
            .ok_or(MagicError::Unsupported("no pending spell cast"))?;
        if cast.kind != SpellCastKind::FireAndForget {
            return Err(MagicError::Unsupported("concentration release"));
        }
        let effects = self.cast_spell(cast.spell, caster, Some(caster), cast.target, alloc)?;
        self.spell_casts_in_progress.remove(&caster);
        Ok(effects)
    }

    /// Stop a reference's pending charge or concentration cast.
    /// Released casts have no pending session and their effects remain live.
    pub fn interrupt_spell_cast(&mut self, caster: GlobalFormId) -> bool {
        self.spell_casts_in_progress.remove(&caster).is_some()
    }

    /// Read the pending cast phase for a reference.
    pub fn pending_spell_cast(&self, caster: GlobalFormId) -> Option<SpellCast> {
        self.spell_casts_in_progress.get(&caster).copied()
    }

    /// Whether a live Paralysis archetype currently suppresses actor motion.
    pub fn actor_is_paralyzed(&self, actor: GlobalFormId) -> bool {
        self.active_effects.values().any(|active| {
            active.target == actor
                && self
                    .magic_effect(active.effect)
                    .is_ok_and(|effect| effect.data.archetype == PARALYSIS_ARCHETYPE)
        })
    }

    pub(crate) fn create_brewed_potion(
        &mut self,
        effects: Vec<(GlobalFormId, f32, u32, u32)>,
    ) -> GlobalFormId {
        self.next_dynamic = self.next_dynamic.saturating_add(1);
        let id = GlobalFormId {
            file: super::DYNAMIC_FILE,
            object: self.next_dynamic,
        };
        self.brewed_potions.insert(id, effects);
        id
    }

    pub(crate) fn slow_time_scale(&self) -> f64 {
        let slow_percent = self
            .active_effects
            .values()
            .filter_map(|active| {
                let definition = self.magic_effect(active.effect).ok()?;
                (definition.data.archetype == SLOW_TIME).then_some(active.magnitude)
            })
            .fold(0.0_f32, f32::max)
            .min(100.0);
        f64::from(100.0 - slow_percent) / 100.0
    }

    /// Seconds remaining before an actor may use a shout again.
    #[must_use]
    pub fn voice_recovery_time(&mut self, actor: GlobalFormId) -> f32 {
        let now = self.clock.real_seconds;
        self.actor(actor)
            .and_then(|state| state.voice_recovery_until)
            .map_or(0.0, |until| (until - now).max(0.0) as f32)
    }

    /// Set an actor's shout recovery in real seconds.
    ///
    /// # Errors
    ///
    /// Returns `false` for an absent actor, non-finite duration, or negative duration.
    pub fn set_voice_recovery_time(&mut self, actor: GlobalFormId, seconds: f32) -> bool {
        if !seconds.is_finite() || seconds < 0.0 || self.actor(actor).is_none() {
            return false;
        }
        let deadline = (self.clock.real_seconds + f64::from(seconds)).min(f64::MAX);
        if let Some(state) = self.actor(actor) {
            state.voice_recovery_until = (seconds > 0.0).then_some(deadline);
            true
        } else {
            false
        }
    }

    /// Cast the highest authored word spell of the actor's equipped shout and
    /// start the recovery authored by that word.
    ///
    /// # Errors
    ///
    /// Returns a magic error before setting recovery if the shout, spell,
    /// target, or authored recovery data is unusable.
    pub fn cast_shout(
        &mut self,
        actor: GlobalFormId,
        target: Option<GlobalFormId>,
        alloc: crate::world::Alloc<'_>,
    ) -> Result<(), MagicError> {
        let shout = self
            .actor(actor)
            .and_then(|state| state.equipped_shout)
            .ok_or(MagicError::Target)?;
        if self.voice_recovery_time(actor) > 0.0 {
            return Err(MagicError::Unsupported("voice recovery"));
        }
        let record = self.record(shout).ok_or(MagicError::Missing(shout))?;
        if record.tag != Tag::new(b"SHOU") {
            return Err(MagicError::Missing(shout));
        }
        let words: Vec<_> = record.all(Tag::new(b"SNAM")).collect();
        if words.len() != 3 || words.iter().any(|word| word.len() != 12) {
            return Err(MagicError::Unsupported("malformed shout words"));
        }
        let selected = words
            .iter()
            .rev()
            .find_map(|word| {
                let raw = u32::from_le_bytes(word[4..8].try_into().ok()?);
                (raw != 0).then_some((raw, f32::from_le_bytes(word[8..12].try_into().ok()?)))
            })
            .ok_or(MagicError::Unsupported("shout has no spell"))?;
        let (raw_spell, recovery) = selected;
        if !recovery.is_finite() || recovery < 0.0 {
            return Err(MagicError::Number);
        }
        let spell = self
            .resolve_from(shout, raw_spell)
            .ok_or(MagicError::Missing(shout))?;
        self.cast_spell(spell, actor, Some(actor), target, alloc)?;
        let deadline = (self.clock.real_seconds + f64::from(recovery)).min(f64::MAX);
        if let Some(state) = self.actor(actor) {
            state.voice_recovery_until = (recovery > 0.0).then_some(deadline);
        }
        Ok(())
    }

    /// The source of an active Turn Undead effect that currently makes this
    /// actor flee. Teammates do not flee until their combat state begins.
    pub(crate) fn turn_undead_threat(&mut self, target: GlobalFormId) -> Option<GlobalFormId> {
        let actor = self.actor(target)?;
        if actor.dead || actor.ghost || actor.teammate && !actor.in_combat {
            return None;
        }
        let undead = self.form("ActorTypeUndead")?;
        if !self.has_keyword(target, undead) {
            return None;
        }
        let effects: Vec<_> = self
            .active_effects
            .values()
            .filter(|effect| {
                effect.target == target
                    && effect
                        .duration
                        .is_some_and(|duration| duration > effect.elapsed)
            })
            .map(|effect| (effect.effect, effect.caster, effect.source))
            .collect();
        effects.into_iter().find_map(|(effect, caster, source)| {
            self.magic_effect(effect)
                .is_ok_and(|definition| definition.data.archetype == TURN_UNDEAD)
                .then(|| {
                    caster
                        .filter(|id| self.reference(*id).is_some())
                        .unwrap_or(source)
                })
                .filter(|id| self.reference(*id).is_some())
        })
    }

    /// All currently applied effects, in application order.
    pub fn active_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects.values()
    }

    /// Whether an actor currently has a live Calm application.
    pub(crate) fn is_calmed(&self, target: GlobalFormId) -> bool {
        self.active_effects.values().any(|effect| {
            effect.target == target
                && self
                    .magic_effect(effect.effect)
                    .is_ok_and(|base| base.data.archetype == 6)
        })
    }

    /// The active Grab Actor application controlling an actor, if any.
    ///
    /// The returned application identifies both the selected actor and its
    /// caster. Host movement and throw behavior are separate from this world
    /// relationship.
    #[must_use]
    pub fn grab_actor_effect_on(&self, target: GlobalFormId) -> Option<&ActiveEffect> {
        self.active_effects.values().find(|effect| {
            effect.target == target
                && self
                    .magic_effect(effect.effect)
                    .is_ok_and(|definition| definition.data.archetype == 45)
        })
    }

    /// Whether a live Demoralize application is making this actor flee.
    pub fn is_fleeing(&self, target: GlobalFormId) -> bool {
        self.active_effects.values().any(|effect| {
            effect.target == target
                && self
                    .magic_effect(effect.effect)
                    .is_ok_and(|base| base.data.archetype == DEMORALIZE)
        })
    }

    /// The source reference that a fleeing actor should move away from.
    pub(crate) fn flee_source(&self, target: GlobalFormId) -> Option<GlobalFormId> {
        self.active_effects.values().find_map(|effect| {
            (effect.target == target
                && self
                    .magic_effect(effect.effect)
                    .is_ok_and(|base| base.data.archetype == DEMORALIZE))
            .then_some(effect.caster.unwrap_or(effect.source))
        })
    }

    /// The live application represented by a Papyrus ActiveMagicEffect object.
    pub fn active_effect_of(&self, object: ObjectId) -> Option<&ActiveEffect> {
        let Handle::ActiveEffect(id) = self.handle_of(object)? else {
            return None;
        };
        self.active_effects.get(&id)
    }

    /// Whether an actor currently has a live Frenzy application.
    #[must_use]
    pub fn is_frenzied(&self, target: GlobalFormId) -> bool {
        self.active_effects.values().any(|effect| {
            effect.target == target
                && self
                    .magic_effect(effect.effect)
                    .is_ok_and(|base| base.data.archetype == FRENZY_ARCHETYPE)
        })
    }

    /// Whether an actor currently has an active Invisibility effect.
    #[must_use]
    pub fn is_invisible(&self, actor: GlobalFormId) -> bool {
        self.active_effects.values().any(|effect| {
            effect.target == actor
                && self
                    .magic_effect(effect.effect)
                    .is_ok_and(|definition| definition.data.archetype == INVISIBILITY)
        })
    }

    /// End all active Invisibility effects on an actor after a break action.
    pub fn break_invisibility(&mut self, actor: GlobalFormId, alloc: Alloc<'_>) {
        let ids: Vec<_> = self
            .active_effects
            .values()
            .filter(|effect| {
                effect.target == actor
                    && self
                        .magic_effect(effect.effect)
                        .is_ok_and(|definition| definition.data.archetype == INVISIBILITY)
            })
            .map(|effect| effect.id)
            .collect();
        for id in ids {
            self.dispel_effect(id, alloc);
        }
    }

    /// Equip an item and apply its worn armor enchantment, if it has one.
    ///
    /// # Errors
    /// Invalid equipment, malformed enchantment records or unsupported effects.
    pub fn equip_item_with_effects(
        &mut self,
        actor: GlobalFormId,
        item: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Result<(), crate::combat::Error> {
        let previously_equipped = self
            .actor(actor)
            .ok_or(crate::combat::Error::Actor(actor))?
            .equipped
            .clone();
        let enchantment = self.item_enchantment(item)?;
        let already_active = enchantment.is_some_and(|enchantment| {
            self.active_effects.values().any(|effect| {
                effect.target == actor && effect.spell == enchantment && effect.source == item
            })
        });
        let prepared = if already_active {
            None
        } else if let Some(enchantment) = enchantment {
            Some(self.enchantment_prepared_effects(item, enchantment, actor)?)
        } else {
            None
        };
        self.equip_item(actor, item)?;
        let equipped = self
            .actor(actor)
            .ok_or(crate::combat::Error::Actor(actor))?
            .equipped
            .clone();
        let replaced: Vec<_> = previously_equipped.difference(&equipped).copied().collect();
        let removed: Vec<_> = self
            .active_effects
            .values()
            .filter(|effect| {
                effect.target == actor
                    && self
                        .item_enchantment(effect.source)
                        .is_ok_and(|enchantment| {
                            enchantment.is_some_and(|enchantment| effect.spell == enchantment)
                        })
                    && replaced.contains(&effect.source)
            })
            .map(|effect| effect.id)
            .collect();
        for effect in removed {
            self.dispel_effect(effect, alloc);
        }
        if let Some(prepared) = prepared {
            self.apply_prepared_effects(prepared, alloc)?;
        }
        Ok(())
    }

    /// Unequip an item and remove effects supplied by its worn enchantment.
    ///
    /// # Errors
    /// Missing actor or reference.
    pub fn unequip_item_with_effects(
        &mut self,
        actor: GlobalFormId,
        item: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Result<(), crate::combat::Error> {
        self.unequip_item(actor, item)?;
        let enchantment = self.item_enchantment(item)?;
        let effects: Vec<_> = self
            .active_effects
            .values()
            .filter(|effect| {
                effect.target == actor
                    && effect.source == item
                    && enchantment.is_some_and(|enchantment| effect.spell == enchantment)
            })
            .map(|effect| effect.id)
            .collect();
        for effect in effects {
            self.dispel_effect(effect, alloc);
        }
        Ok(())
    }

    fn item_enchantment(&self, item: GlobalFormId) -> Result<Option<GlobalFormId>, MagicError> {
        if self.tag_of(item) != Some(super::T(b"ARMO")) {
            return Ok(None);
        }
        let Some(record) = self.record(item) else {
            return Ok(None);
        };
        let Some(raw) = record.find(super::T(b"EITM")) else {
            return Ok(None);
        };
        if raw.len() != 4 {
            return Err(MagicError::Unsupported("malformed armor enchantment link"));
        }
        let raw: [u8; 4] = raw
            .try_into()
            .map_err(|_| MagicError::Unsupported("malformed armor enchantment link"))?;
        let raw = u32::from_le_bytes(raw);
        if raw == 0 {
            return Ok(None);
        }
        let enchantment = self
            .resolve_from(item, raw)
            .ok_or(MagicError::Missing(item))?;
        if self.tag_of(enchantment) != Some(super::T(b"ENCH")) {
            return Err(MagicError::Missing(enchantment));
        }
        Ok(Some(enchantment))
    }

    fn enchantment_effects(&self, enchantment: GlobalFormId) -> Result<Vec<Effect>, MagicError> {
        let source = self
            .record(enchantment)
            .ok_or(MagicError::Missing(enchantment))?;
        let enit = source
            .find(super::T(b"ENIT"))
            .ok_or(MagicError::Missing(enchantment))?;
        if !matches!(enit.len(), 32 | 36) {
            return Err(MagicError::Unsupported("malformed enchantment data"));
        }
        let cast_type = u32::from_le_bytes(
            enit[8..12]
                .try_into()
                .map_err(|_| MagicError::Unsupported("malformed enchantment data"))?,
        );
        let delivery = u32::from_le_bytes(
            enit[16..20]
                .try_into()
                .map_err(|_| MagicError::Unsupported("malformed enchantment data"))?,
        );
        let enchantment_type = u32::from_le_bytes(
            enit[20..24]
                .try_into()
                .map_err(|_| MagicError::Unsupported("malformed enchantment data"))?,
        );
        if cast_type != 0 || delivery != 0 || enchantment_type != 6 {
            return Err(MagicError::Unsupported("nonconstant armor enchantment"));
        }
        let mut record =
            Record::new(Tag(*b"SPEL"), source.form_id).sub(super::T(b"SPIT"), vec![0; 36]);
        record.subrecords.extend(
            source
                .subrecords
                .iter()
                .filter(|sub| {
                    matches!(
                        sub.tag.as_bytes(),
                        b"EFID" | b"EFIT" | b"CTDA" | b"CIS1" | b"CIS2"
                    )
                })
                .cloned(),
        );
        Ok(Spell::from_record(&record, false)?.effects)
    }

    fn enchantment_prepared_effects(
        &mut self,
        item: GlobalFormId,
        enchantment: GlobalFormId,
        actor: GlobalFormId,
    ) -> Result<Vec<PreparedEffect>, MagicError> {
        let entries = self.enchantment_effects(enchantment)?;
        let mut prepared = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let context = Context {
                owner: enchantment,
                subject: actor,
                target: Some(actor),
                quest: None,
                tempering_enchanted: None,
                tempering_item: None,
                alchemy_making_poison: None,
                alchemy_effect: None,
                magic_item: None,
                dual_cast: None,
                skill_use: None,
                light_level: None,
            };
            if !conditions::evaluate_without_vm(self, context, &entry.conditions)? {
                continue;
            }
            let effect = self
                .resolve_from(enchantment, entry.base_effect)
                .ok_or(MagicError::Missing(enchantment))?;
            let base = self.magic_effect(effect)?;
            let context = Context {
                owner: effect,
                ..context
            };
            if !conditions::evaluate_without_vm(self, context, &base.conditions)? {
                continue;
            }
            let mut entry = entry.clone();
            entry.conditions.clear();
            let mut one =
                self.item_prepared_effects(enchantment, &[entry], Some(actor), actor, true)?;
            for prepared in &mut one {
                prepared.effect.effect_index = index as u32;
                prepared.effect.source = item;
                prepared.effect.duration = None;
                prepared.effect.recover = true;
            }
            prepared.extend(one);
        }
        Ok(prepared)
    }

    /// Read the current winning SPEL definition.
    pub fn spell(&self, id: GlobalFormId) -> Result<Spell, MagicError> {
        let rref = self.order.winner_ref(id).ok_or(MagicError::Missing(id))?;
        let record = self.order.record(rref);
        if record.is_deleted() {
            return Err(MagicError::Missing(id));
        }
        Ok(Spell::from_record(
            record,
            self.order.file(rref.file).plugin.header.is_localized(),
        )?)
    }

    /// Read a winning SCRL record using the shared SPIT and effect layout.
    fn scroll(&self, id: GlobalFormId) -> Result<Spell, MagicError> {
        let record = self.record(id).ok_or(MagicError::Missing(id))?;
        if record.is_deleted() || record.tag.as_bytes() != b"SCRL" {
            return Err(MagicError::Missing(id));
        }
        let localized = self.record_text_localized(id, Tag::new(b"FULL"));
        let mut spell_record = record.clone();
        spell_record.tag = Tag::new(b"SPEL");
        Ok(Spell::from_record(&spell_record, localized)?)
    }

    /// Read the current winning MGEF definition.
    pub fn magic_effect(&self, id: GlobalFormId) -> Result<MagicEffect, MagicError> {
        let record = self.record(id).ok_or(MagicError::Missing(id))?;
        if record.is_deleted() {
            return Err(MagicError::Missing(id));
        }
        Ok(MagicEffect::from_record(
            record,
            self.record_text_localized(id, T(b"FULL")),
        )?)
    }

    /// Read the current winning ALCH definition.
    pub fn ingestible(&self, id: GlobalFormId) -> Result<Ingestible, MagicError> {
        if let Some(effects) = self.brewed_potions.get(&id) {
            return Ok(Ingestible {
                form_id: id.object,
                editor_id: None,
                name: Some(soe_esm::records::Text::Plain("Potion".into())),
                description: None,
                data: soe_esm::records::gameplay::IngestibleData {
                    value: 0,
                    flags: 0,
                    addiction: 0,
                    addiction_chance: 0.0,
                    consume_sound: 0,
                },
                effects: effects
                    .iter()
                    .map(
                        |(_, magnitude, area, duration)| soe_esm::records::gameplay::Effect {
                            base_effect: 0,
                            data: soe_esm::records::gameplay::EffectData {
                                magnitude: *magnitude,
                                area: *area,
                                duration: *duration,
                            },
                            conditions: Vec::new(),
                        },
                    )
                    .collect(),
            });
        }
        let rref = self.order.winner_ref(id).ok_or(MagicError::Missing(id))?;
        Ok(Ingestible::from_record(
            self.order.record(rref),
            self.order.file(rref.file).plugin.header.is_localized(),
        )?)
    }

    pub(crate) fn magic_item_effect(
        &self,
        item: GlobalFormId,
        index: u32,
    ) -> Result<Effect, MagicError> {
        let effects = match self.tag_of(item).map(|tag| *tag.as_bytes()) {
            Some(tag) if tag == *b"SPEL" => self.spell(item)?.effects,
            Some(tag) if tag == *b"ALCH" => self.ingestible(item)?.effects,
            Some(tag) if tag == *b"ENCH" => {
                let record = self
                    .record(item)
                    .filter(|record| !record.is_deleted() && record.tag == super::T(b"ENCH"))
                    .ok_or(MagicError::Missing(item))?;
                weapon_enchantment_effects(record).or_else(|_| self.enchantment_effects(item))?
            }
            _ => return Err(MagicError::Missing(item)),
        };
        effects
            .get(index as usize)
            .cloned()
            .ok_or(MagicError::Unsupported("active effect entry"))
    }

    /// Consume one ALCH inventory item and apply its effects to its drinker.
    ///
    /// # Errors
    ///
    /// Returns an error without consuming the item when it is absent, malformed
    /// or needs a magic behavior that has no execution path yet.
    pub fn drink_ingestible(
        &mut self,
        ingestible: GlobalFormId,
        drinker: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        if self.item_count(drinker, ingestible) == 0 {
            return Err(MagicError::Missing(ingestible));
        }
        let definition = self.ingestible(ingestible)?;
        if definition.data.flags & INGESTIBLE_POISON != 0 {
            return Err(MagicError::Unsupported("poison application to a weapon"));
        }
        let prepared = self.ingestible_prepared_effects(
            ingestible,
            &definition.effects,
            Some(drinker),
            drinker,
        )?;
        let applied = self.apply_prepared_effects(prepared, alloc)?;
        self.remove_item_notified(drinker, ingestible, 1, None, alloc);
        Ok(applied)
    }

    /// Apply a weapon poison coating's effects to a melee target
    /// (`crates/game/src/world/combat.rs`, CMB-25). The poisoning actor
    /// stands in for `drink_ingestible`'s drinker as caster and source, the
    /// hit's target is the one actor the coating ever reaches. No corpus
    /// source describes the closed-source runtime's exact dose or
    /// reapplication behavior (`ExtraPoison.h`/`.cpp` only carry the item
    /// and a count, no consumption logic), so this always applies the full
    /// effect list once, matching the done check's own wording rather than
    /// a decrementing dose count. The caller clears the consumed extra.
    ///
    /// # Errors
    /// Missing or malformed poison record, a dead or ghost target when an
    /// effect is hostile, or an effect this world cannot apply.
    pub(super) fn apply_poison(
        &mut self,
        attacker: GlobalFormId,
        poison: GlobalFormId,
        target: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let definition = self.ingestible(poison)?;
        let prepared =
            self.ingestible_prepared_effects(poison, &definition.effects, Some(attacker), target)?;
        self.apply_prepared_effects(prepared, alloc)
    }

    pub(super) fn prepare_weapon_enchantment(
        &mut self,
        enchantment: GlobalFormId,
        attacker: GlobalFormId,
        target: GlobalFormId,
    ) -> Result<PreparedWeaponEnchantment, MagicError> {
        let record = self
            .record(enchantment)
            .filter(|record| !record.is_deleted() && record.tag == super::T(b"ENCH"))
            .ok_or(MagicError::Missing(enchantment))?;
        let data = record
            .find(super::T(b"ENIT"))
            .filter(|data| matches!(data.len(), 32 | 36))
            .ok_or_else(|| {
                MagicError::Record(soe_esm::Error::Invalid("malformed ENCH ENIT".into()))
            })?;
        let cast_type = u32::from_le_bytes(data[8..12].try_into().map_err(|_| {
            MagicError::Record(soe_esm::Error::Invalid("malformed ENCH cast type".into()))
        })?);
        let delivery = u32::from_le_bytes(data[16..20].try_into().map_err(|_| {
            MagicError::Record(soe_esm::Error::Invalid("malformed ENCH delivery".into()))
        })?);
        let enchant_type = u32::from_le_bytes(data[20..24].try_into().map_err(|_| {
            MagicError::Record(soe_esm::Error::Invalid("malformed ENCH type".into()))
        })?);
        let base_enchantment = u32::from_le_bytes(data[24..28].try_into().map_err(|_| {
            MagicError::Record(soe_esm::Error::Invalid("malformed ENCH base link".into()))
        })?);
        if cast_type != 1 || delivery != 1 || enchant_type != 6 {
            return Err(MagicError::Unsupported("weapon enchantment casting"));
        }
        if base_enchantment != 0 {
            return Err(MagicError::Unsupported("base weapon enchantment"));
        }
        let effects = weapon_enchantment_effects(record)?;
        let prepared =
            self.ingestible_prepared_effects(enchantment, &effects, Some(attacker), target)?;
        Ok(PreparedWeaponEnchantment(
            self.prepare_effect_actions(prepared)?,
        ))
    }

    pub(super) fn apply_weapon_enchantment(
        &mut self,
        prepared: PreparedWeaponEnchantment,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        self.apply_prepared_plan(prepared.0, alloc)
    }

    /// Check an enchantment against the target's health after the physical hit.
    /// The first preparation sees the pre-hit values, so it cannot detect a
    /// combined hit and effect that overflows an actor-value component.
    pub(super) fn validate_weapon_enchantment_after_hit(
        &mut self,
        prepared: &PreparedWeaponEnchantment,
        target_values: &crate::actor_values::Values,
    ) -> Result<(), MagicError> {
        let mut values = target_values.clone();
        for action in &prepared.0 {
            for id in &action.dispel {
                if let Some(effect) = self.active_effects.get(id).filter(|effect| effect.recover) {
                    change_values(&mut values, effect, -1.0)?;
                }
            }
            if action.effect.recover || action.effect.duration == Some(0.0) {
                let before = values.get("health").current();
                change_values(&mut values, &action.effect, 1.0)?;
                let touches_health = action
                    .effect
                    .changes
                    .iter()
                    .any(|(name, _)| name == "health");
                let actor = self
                    .actor_ref(action.effect.target)
                    .ok_or(MagicError::Target)?;
                let player_exception = actor.protected
                    && !actor.essential
                    && self.player.is_some()
                    && action.effect.caster == self.player;
                let eligible = touches_health
                    && (actor.protected || actor.essential)
                    && !player_exception
                    && values.get("health").current() < before;
                let (clamped, entering) = self.clamp_health_at_bleedout(
                    action.effect.target,
                    eligible,
                    values.get("health").current(),
                );
                if entering {
                    super::bleedout::commit_bleedout_health(&mut values, clamped)
                        .map_err(|_| MagicError::Number)?;
                }
            }
        }
        Ok(())
    }

    /// Shared ALCH effect-entry validation and `PreparedEffect` construction
    /// for `drink_ingestible` and `apply_poison`. `item` is the ALCH record
    /// both the caster and source fields report, `effects` its already-read
    /// effect list, `caster` the actor credited with the application and
    /// `target` the actor whose values change.
    fn ingestible_prepared_effects(
        &mut self,
        item: GlobalFormId,
        effects: &[soe_esm::records::gameplay::Effect],
        caster: Option<GlobalFormId>,
        target: GlobalFormId,
    ) -> Result<Vec<PreparedEffect>, MagicError> {
        self.item_prepared_effects(item, effects, caster, target, false)
    }

    fn item_prepared_effects(
        &mut self,
        resolution: GlobalFormId,
        effects: &[soe_esm::records::gameplay::Effect],
        caster: Option<GlobalFormId>,
        target: GlobalFormId,
        conditions_evaluated: bool,
    ) -> Result<Vec<PreparedEffect>, MagicError> {
        if self.actor(target).is_none() {
            return Err(MagicError::Target);
        }
        let mut prepared = Vec::new();
        for (index, entry) in effects.iter().enumerate() {
            let effect = self
                .brewed_potions
                .get(&resolution)
                .and_then(|entries| entries.get(index))
                .map(|entry| entry.0)
                .or_else(|| self.resolve_from(resolution, entry.base_effect))
                .ok_or(MagicError::Missing(resolution))?;
            let base = self.magic_effect(effect)?;
            let data = &base.data;
            if data.archetype == PEAK_VALUE_MODIFIER {
                self.peak_value_keyword(effect, &base)?;
            }
            if !conditions_evaluated
                && (!entry.conditions.is_empty() || !base.conditions.is_empty())
            {
                return Err(MagicError::Unsupported("magic condition evaluation"));
            }
            if data.flags & HOSTILE != 0 {
                let actor = self.actor(target).ok_or(MagicError::Target)?;
                if actor.dead || actor.ghost {
                    return Err(MagicError::Target);
                }
            }
            if data.archetype == 40 {
                return Err(MagicError::Unsupported(
                    "spawn hazard requires hazard ownership and expiry",
                ));
            }
            if !matches!(
                data.archetype,
                0 | 1
                    | 2
                    | 5
                    | PARALYSIS_ARCHETYPE
                    | CURE_PARALYSIS_ARCHETYPE
                    | 3
                    | 29
                    | 31
                    | 32
                    | 33
                    | PEAK_VALUE_MODIFIER
                    | 4
                    | 6
                    | 9
                    | 10
                    | INVISIBILITY
                    | LIGHT
                    | LOCK
                    | OPEN
                    | 17
                    | 18
                    | 20
                    | 38
                    | DEMORALIZE
                    | FRENZY_ARCHETYPE
                    | TURN_UNDEAD
            ) {
                return Err(MagicError::Unsupported("effect archetype"));
            }
            if entry.data.area != 0 || data.area != 0 || data.taper_duration != 0.0 {
                return Err(MagicError::Unsupported("area or taper"));
            }
            if data.perk != 0 {
                return Err(MagicError::Unsupported("perk application"));
            }
            self.validate_equip_ability(effect, data.equip_ability)?;
            let magnitude = if data.flags & NO_MAGNITUDE != 0 {
                0.0
            } else {
                entry.data.magnitude
            };
            if !magnitude.is_finite() || magnitude < 0.0 {
                return Err(MagicError::Number);
            }
            let magnitude = self.resisted_magnitude(target, data, magnitude)?;
            let signed = if data.flags & DETRIMENTAL != 0 {
                -magnitude
            } else {
                magnitude
            };
            let mut changes = Vec::new();
            if data.archetype == PARALYSIS_ARCHETYPE {
                changes.push(("paralysis".to_string(), signed));
            } else if matches!(data.archetype, 0 | 5 | 31 | PEAK_VALUE_MODIFIER) {
                let name = actor_value_name(data.primary_actor_value)
                    .ok_or(MagicError::Unsupported("actor value"))?;
                changes.push((name.to_string(), signed));
                if data.archetype == 5 {
                    let name = actor_value_name(data.second_actor_value)
                        .ok_or(MagicError::Unsupported("secondary actor value"))?;
                    let amount = signed * data.second_actor_value_weight;
                    if !amount.is_finite() {
                        return Err(MagicError::Number);
                    }
                    changes.push((name.to_string(), amount));
                }
            } else if data.archetype == 6 {
                changes.push(("aggression".to_string(), -magnitude));
            } else if data.archetype == DEMORALIZE {
                changes.push(("confidence".to_string(), -magnitude));
            }
            if data.archetype == 32 {
                let name = actor_value_name(data.primary_actor_value)
                    .ok_or(MagicError::Unsupported("actor value"))?;
                changes.push((name.to_string(), signed));
            }
            // An ingestible's effects carry the same dispel archetype and
            // keyword flags a cast spell's do.
            let mut dispel_rules = Vec::new();
            if data.archetype == 2 {
                dispel_rules.push(DispelRule::Timed);
            }
            if data.archetype == CURE_PARALYSIS_ARCHETYPE {
                dispel_rules.push(DispelRule::Paralysis);
            }
            if data.archetype == 3 {
                dispel_rules.push(DispelRule::Disease);
            }
            if data.archetype == 29 {
                dispel_rules.push(DispelRule::Poisons);
            }
            if data.flags & DISPEL_WITH_KEYWORDS != 0 {
                let keywords = self.keyword_entries(effect);
                for keyword in &keywords {
                    if self.tag_of(*keyword) != Some(super::T(b"KYWD")) {
                        return Err(MagicError::Missing(*keyword));
                    }
                }
                dispel_rules.push(DispelRule::Keywords(keywords));
            }
            prepared.push(PreparedEffect {
                effect: ActiveEffect {
                    id: EffectId(0),
                    spell: resolution,
                    effect,
                    effect_index: index as u32,
                    source: resolution,
                    caster,
                    target,
                    magnitude,
                    duration: if data.archetype == 32 && data.flags & NO_DURATION != 0 {
                        None
                    } else {
                        Some(if data.flags & NO_DURATION != 0 {
                            0.0
                        } else {
                            f64::from(entry.data.duration)
                        })
                    },
                    elapsed: 0.0,
                    summoned: None,
                    restore_owner: None,
                    recover: data.flags & RECOVER != 0
                        || matches!(data.archetype, PARALYSIS_ARCHETYPE | DEMORALIZE | 38),
                    changes,
                },
                no_recast: data.flags & NO_RECAST != 0,
                summon_base: None,
                werewolf_race: None,
                command_owner: None,
                light_base: None,
                bound_weapon: None,
                lock_level: None,
                open_lock: false,
                disarm: false,
                stagger_duration: (data.archetype == 33)
                    .then(|| {
                        self.combat_setting("fStaggerDuration")
                            .map_err(|_| MagicError::StaggerSetting("fStaggerDuration"))
                    })
                    .transpose()?,
                hit_event: false,
                dispel_rules,
                dispel: Vec::new(),
                vampire_form: None,
            });
        }
        Ok(prepared)
    }

    /// The actors an area effect selects around `anchor`: every enabled,
    /// undeleted actor reference with a finite position within `radius` game
    /// units of the anchor's own position, anchor included, each named once.
    /// A non-positive or non-finite radius, or an anchor with no finite
    /// position, selects only the anchor, so a point effect behaves exactly
    /// as before area was supported.
    ///
    /// `radius` is the authored EFIT `Area` (TES5Edit's per-effect area
    /// field, in game units), already zeroed by the caller when the effect's
    /// `No Area` flag is set. Distance ignores cell boundaries, the same
    /// convention `World::distance` uses for `GetDistance`: a hit near an
    /// exterior cell seam still reaches an actor across it, and `self.refs`
    /// holds only references the world has already loaded nearby, so no
    /// further residency check is needed here.
    fn area_targets(&mut self, anchor: GlobalFormId, radius: f32) -> Vec<GlobalFormId> {
        let anchor_position = self.reference_ref(anchor).map(|r| r.position);
        let Some(center) = anchor_position.filter(|p| p.iter().all(|v| v.is_finite())) else {
            return vec![anchor];
        };
        if !(radius.is_finite() && radius > 0.0) {
            return vec![anchor];
        }
        let mut selected = self.area_targets_at(center, radius);
        if !selected.contains(&anchor) {
            selected.push(anchor);
            selected.sort_unstable();
        }
        selected
    }

    fn area_targets_at_position(&self, center: [f32; 3], radius: f32) -> Vec<GlobalFormId> {
        self.area_targets_at(center, radius)
    }

    fn area_targets_at(&self, center: [f32; 3], radius: f32) -> Vec<GlobalFormId> {
        if !center.iter().all(|value| value.is_finite()) {
            return Vec::new();
        }
        if !(radius.is_finite() && radius >= 0.0) {
            return Vec::new();
        }
        if !(radius.is_finite() && radius > 0.0) {
            return self
                .refs
                .iter()
                .filter_map(|(&candidate, reference)| {
                    (reference.is_actor
                        && reference.enabled
                        && !reference.deleted
                        && reference.position.iter().all(|v| v.is_finite())
                        && reference.position == center)
                        .then_some(candidate)
                })
                .collect();
        }
        let mut selected = std::collections::BTreeSet::new();
        for (&candidate, reference) in &self.refs {
            if !reference.is_actor
                || !reference.enabled
                || reference.deleted
                || !reference.position.iter().all(|v| v.is_finite())
            {
                continue;
            }
            let offset = [
                reference.position[0] - center[0],
                reference.position[1] - center[1],
                reference.position[2] - center[2],
            ];
            let distance = offset.iter().map(|v| v * v).sum::<f32>().sqrt();
            if distance <= radius {
                selected.insert(candidate);
            }
        }
        selected.into_iter().collect()
    }

    /// A Lock effect accepts only references whose base form can carry an
    /// attached lock in the record format.
    fn lockable_reference(&self, reference: GlobalFormId) -> bool {
        self.reference_ref(reference)
            .and_then(|state| state.base)
            .and_then(|base| self.tag_of(base))
            .is_some_and(|tag| tag == super::T(b"DOOR") || tag == super::T(b"CONT"))
    }

    /// Execute a direct scripted cast. This bypasses player charging and cost.
    /// The caller supplies the source reference and the actor credited as caster.
    ///
    /// # Errors
    ///
    /// Returns an error before applying any effect when delivery,
    /// hit resolution or an archetype needs an execution path not yet available.
    pub fn cast_spell(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: Option<GlobalFormId>,
        target: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        if self.spell(spell)?.data.cast_type == 2 {
            return Err(MagicError::Unsupported(
                "concentration requires a held cast",
            ));
        }
        let prepared = self.prepare_spell_cast(spell, source, caster, target)?;
        if let Some(caster) = caster {
            self.break_invisibility(caster, alloc);
        }
        self.apply_prepared_effects(prepared, alloc)
    }

    fn cast_spell_inner(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: Option<GlobalFormId>,
        target: Option<GlobalFormId>,
        _cloak_pulse: bool,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let prepared = self.prepare_spell(
            spell,
            source,
            caster,
            target,
            CastDelivery::CloakPulse,
            None,
            true,
        )?;
        self.apply_prepared_effects(prepared, alloc)
    }

    /// Charge time for a spell, using its authored value or the longest
    /// casting time among its effects when the record auto-calculates it.
    pub fn spell_charge_time(&self, spell: GlobalFormId) -> Result<f32, MagicError> {
        let definition = self.spell(spell)?;
        if definition.data.flags & 1 != 0 {
            return finite_nonnegative(definition.data.charge_time);
        }
        let mut charge_time: f32 = 0.0;
        for entry in &definition.effects {
            let effect = self
                .resolve_from(spell, entry.base_effect)
                .ok_or(MagicError::Missing(spell))?;
            charge_time = charge_time.max(finite_nonnegative(
                self.magic_effect(effect)?.data.casting_time,
            )?);
        }
        Ok(charge_time)
    }

    /// Magicka expenditure for a spell and caster, including the authored
    /// manual or auto-calculated cost and its half-cost perk.
    pub fn spell_cost(
        &mut self,
        spell: GlobalFormId,
        caster: GlobalFormId,
    ) -> Result<f32, MagicError> {
        let definition = self.spell(spell)?;
        let cost = if definition.data.flags & 1 != 0 {
            definition.data.base_cost as f32
        } else {
            let mut total = 0.0_f64;
            for entry in &definition.effects {
                let effect = self
                    .resolve_from(spell, entry.base_effect)
                    .ok_or(MagicError::Missing(spell))?;
                let data = self.magic_effect(effect)?.data;
                let magnitude = f64::from(entry.data.magnitude.max(1.0));
                let duration = if definition.data.cast_type == 2 || entry.data.duration == 0 {
                    10.0
                } else {
                    f64::from(entry.data.duration)
                };
                total += f64::from(finite_nonnegative(data.base_cost)?)
                    * (magnitude * duration / 10.0).powf(1.1);
            }
            total as f32
        };
        let cost = finite_nonnegative(cost)?;
        let half_cost_perk = if definition.data.half_cost_perk == 0 {
            None
        } else {
            Some(
                self.resolve_from(spell, definition.data.half_cost_perk)
                    .ok_or(MagicError::Missing(spell))?,
            )
        };
        let caster_state = self.actor(caster).ok_or(MagicError::Target)?;
        Ok(
            if half_cost_perk.is_some_and(|perk| caster_state.perks.contains(&perk)) {
                cost * 0.5
            } else {
                cost
            },
        )
    }

    /// Execute a one-shot cast paid for by an actor's current magicka.
    ///
    /// Effect validation and magicka availability are checked before either
    /// the resource value or active effects change.
    pub fn cast_charged_spell(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: GlobalFormId,
        target: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        if self.spell(spell)?.data.cast_type == 2 {
            return Err(MagicError::Unsupported(
                "concentration requires a held cast",
            ));
        }
        let prepared = self.prepare_spell_cast(spell, source, Some(caster), target)?;
        let prepared = self.prepare_effect_actions(prepared)?;
        let disarms = prepared
            .iter()
            .filter(|prepared| prepared.disarm)
            .map(|prepared| (prepared.effect.target, prepared.effect.magnitude))
            .collect();
        let cost = self.spell_cost(spell, caster)?;
        let actor = self.actor(caster).ok_or(MagicError::Target)?;
        if actor.values.get("magicka").current() < cost {
            return Err(MagicError::InsufficientMagicka);
        }
        actor
            .values
            .damage("magicka", cost)
            .map_err(|_| MagicError::Number)?;
        let spell_value = self.form_value(Some(spell), alloc);
        self.form_event(source, "OnSpellCast", vec![spell_value], alloc);
        self.apply_prepared_actions(prepared, alloc, disarms)
    }

    /// Begin holding a concentration spell. Its effects and magicka cost
    /// advance through `tick_concentration` while world time runs.
    ///
    /// # Errors
    /// Returns an error if the spell is not a supported concentration cast,
    /// its effects cannot execute, or the caster has no magicka to spend.
    pub fn begin_concentration_cast(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: GlobalFormId,
        target: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) -> Result<(), MagicError> {
        if self.spell(spell)?.data.cast_type != 2 {
            return Err(MagicError::Unsupported("spell is not concentration"));
        }
        self.prepare_spell_cast(spell, source, Some(caster), target)?;
        let cost_per_second = f64::from(self.spell_cost(spell, caster)?) / 10.0;
        let available = self
            .actor(caster)
            .ok_or(MagicError::Target)?
            .values
            .get("magicka")
            .current();
        if cost_per_second > 0.0 && available <= 0.0 {
            return Err(MagicError::InsufficientMagicka);
        }
        // Starting another concentration cast interrupts the previous one.
        // Drop its transient Telekinesis hold only after the replacement has
        // passed validation, so a rejected cast leaves the current hold intact.
        self.release_concentration_cast();
        self.concentration_cast = Some(ConcentrationCast {
            spell,
            source,
            caster,
            target,
            cost_per_second,
        });
        let spell_value = self.form_value(Some(spell), alloc);
        self.form_event(source, "OnSpellCast", vec![spell_value], alloc);
        Ok(())
    }

    /// Release the currently held concentration spell.
    pub fn release_concentration_cast(&mut self) -> bool {
        let released = self.concentration_cast.take().is_some();
        if released {
            self.telekinesis_grab = None;
        }
        released
    }

    /// Begin holding one enabled, non-actor reference during a Telekinesis
    /// concentration cast. The host separately confirms that it has a
    /// movable physics body for the reference.
    pub fn grab_telekinesis_reference(
        &mut self,
        caster: GlobalFormId,
        reference: GlobalFormId,
    ) -> bool {
        let is_telekinesis = self.concentration_cast.as_ref().is_some_and(|cast| {
            cast.caster == caster
                && self.spell(cast.spell).is_ok_and(|spell| {
                    spell.effects.iter().any(|entry| {
                        self.resolve_from(cast.spell, entry.base_effect)
                            .and_then(|id| self.magic_effect(id).ok())
                            .is_some_and(|effect| effect.data.archetype == 20)
                    })
                })
        });
        let eligible = self
            .reference(reference)
            .is_some_and(|state| state.enabled && !state.deleted && !state.is_actor);
        if !is_telekinesis || !eligible || self.telekinesis_grab.is_some() {
            return false;
        }
        self.telekinesis_grab = Some((caster, reference));
        true
    }

    /// The reference currently held by the caster's Telekinesis effect.
    #[must_use]
    pub fn telekinesis_reference(&self, caster: GlobalFormId) -> Option<GlobalFormId> {
        self.telekinesis_grab
            .filter(|(owner, _)| *owner == caster)
            .map(|(_, reference)| reference)
    }

    /// Release the held reference without changing its World placement.
    pub fn release_telekinesis_reference(&mut self, caster: GlobalFormId) -> bool {
        if self
            .telekinesis_grab
            .is_some_and(|(owner, _)| owner == caster)
        {
            self.telekinesis_grab = None;
            true
        } else {
            false
        }
    }

    /// Whether the caster currently has a held concentration spell.
    pub fn is_concentration_casting(&self, caster: GlobalFormId) -> bool {
        self.concentration_cast
            .as_ref()
            .is_some_and(|cast| cast.caster == caster)
    }

    pub(crate) fn concentration_spell(&self, caster: GlobalFormId) -> Option<GlobalFormId> {
        self.concentration_cast
            .as_ref()
            .filter(|cast| cast.caster == caster)
            .map(|cast| cast.spell)
    }

    /// Advance the held cast in unpaused simulation seconds, applying its
    /// effects in bounded slices and stopping when magicka is exhausted.
    pub fn tick_concentration(&mut self, seconds: f64, alloc: Alloc<'_>) -> Vec<MagicError> {
        let mut errors = Vec::new();
        if !seconds.is_finite() || seconds <= 0.0 {
            return errors;
        }
        let Some(cast) = self.concentration_cast.clone() else {
            return errors;
        };
        let available = self
            .actor(cast.caster)
            .map_or(0.0, |actor| actor.values.get("magicka").current());
        if cast.cost_per_second > 0.0 && available <= 0.0 {
            self.concentration_cast = None;
            self.telekinesis_grab = None;
            return errors;
        }
        let duration = if cast.cost_per_second > 0.0 {
            seconds.min(f64::from(available) / cast.cost_per_second)
        } else {
            seconds
        };
        let mut elapsed = 0.0;
        while elapsed < duration {
            let slice = (duration - elapsed).min(0.1);
            let mut prepared = match self.prepare_spell_cast(
                cast.spell,
                cast.source,
                Some(cast.caster),
                cast.target,
            ) {
                Ok(prepared) => prepared,
                Err(error) => {
                    errors.push(error);
                    self.concentration_cast = None;
                    self.telekinesis_grab = None;
                    return errors;
                }
            };
            for effect in &mut prepared {
                effect.effect.duration = Some(0.0);
                effect.no_recast = false;
                for (_, amount) in &mut effect.effect.changes {
                    *amount *= slice as f32;
                }
            }
            if let Err(error) = self.apply_prepared_effects(prepared, alloc) {
                errors.push(error);
                self.concentration_cast = None;
                self.telekinesis_grab = None;
                return errors;
            }
            let resource_cost = (cast.cost_per_second * slice) as f32;
            if resource_cost > 0.0 {
                if let Some(actor) = self.actor(cast.caster) {
                    if actor.values.damage("magicka", resource_cost).is_err() {
                        errors.push(MagicError::Number);
                        self.concentration_cast = None;
                        self.telekinesis_grab = None;
                        return errors;
                    }
                } else {
                    errors.push(MagicError::Target);
                    self.concentration_cast = None;
                    self.telekinesis_grab = None;
                    return errors;
                }
            }
            elapsed += slice;
        }
        let exhausted = cast.cost_per_second > 0.0
            && self
                .actor(cast.caster)
                .is_none_or(|actor| actor.values.get("magicka").current() <= 0.0);
        if exhausted {
            self.concentration_cast = None;
            self.telekinesis_grab = None;
        }
        errors
    }

    fn prepare_spell_cast(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: Option<GlobalFormId>,
        target: Option<GlobalFormId>,
    ) -> Result<Vec<PreparedEffect>, MagicError> {
        self.prepare_spell(
            spell,
            source,
            caster,
            target,
            CastDelivery::Direct,
            None,
            true,
        )
    }

    /// Apply an aimed spell to the actor resolved from one projectile contact.
    /// A host contact query supplies `target`; direct script casts cannot use
    /// this delivery path.
    ///
    /// # Errors
    /// Returns an error before applying effects when the spell is not aimed,
    /// the contact target is not an eligible actor, or an effect is unsupported.
    pub fn cast_spell_on_projectile_contact(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: Option<GlobalFormId>,
        target: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let prepared = self.prepare_spell(
            spell,
            source,
            caster,
            Some(target),
            CastDelivery::ProjectileContact,
            None,
            true,
        )?;
        if let Some(caster) = caster {
            self.break_invisibility(caster, alloc);
        }
        self.apply_prepared_effects(prepared, alloc)
    }

    /// Apply a target-location spell at a selected world position.
    /// `line_of_sight_clear` comes from the host collision query between the
    /// casting reference and the selected point.
    ///
    /// # Errors
    /// Returns an error before applying effects for a non-location spell, an
    /// invalid or out-of-range position, an obstructed path, or an unsupported
    /// effect.
    pub fn cast_spell_at_location(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: Option<GlobalFormId>,
        position: [f32; 3],
        line_of_sight_clear: bool,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let prepared = self.prepare_spell(
            spell,
            source,
            caster,
            None,
            CastDelivery::Location,
            Some(position),
            line_of_sight_clear,
        )?;
        if let Some(caster) = caster {
            self.break_invisibility(caster, alloc);
        }
        self.apply_prepared_effects(prepared, alloc)
    }

    /// Cast the selected scroll and consume one item after its effects apply.
    ///
    /// # Errors
    /// Invalid or stale selections, non-scroll records, unsupported effects,
    /// invalid targets or a failed effect preparation leave the inventory intact.
    pub fn cast_scroll(
        &mut self,
        holder: GlobalFormId,
        selected: &Selection,
        source: GlobalFormId,
        caster: Option<GlobalFormId>,
        target: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        if selected.count() != 1 || self.tag_of(selected.item()) != Some(Tag::new(b"SCRL")) {
            return Err(MagicError::Unsupported("scroll selection"));
        }
        self.reference(holder)
            .ok_or(MagicError::Target)?
            .inventory
            .selected_stack(selected)?;
        let prepared = self.prepare_spell(
            selected.item(),
            source,
            caster,
            target,
            CastDelivery::Scroll,
            None,
            true,
        )?;
        if let Some(caster) = caster {
            self.break_invisibility(caster, &mut *alloc);
        }
        let applied = self.apply_prepared_effects(prepared, &mut *alloc)?;
        self.remove_selected_notified(holder, selected, None, &mut *alloc)?;
        let scroll_value = self.form_value(Some(selected.item()), &mut *alloc);
        self.form_event(source, "OnSpellCast", vec![scroll_value], &mut *alloc);
        Ok(applied)
    }

    /// Apply constant-effect abilities from an actor's resolved spell list.
    ///
    pub(crate) fn apply_actor_abilities(&mut self, actor: GlobalFormId) {
        let Some(state) = self.actor(actor) else {
            return;
        };
        let spells = state.spells.clone();
        self.apply_abilities(actor, spells);
    }

    /// Add a known spell and activate it when it is a constant-effect ability.
    pub fn add_actor_spell(&mut self, actor: GlobalFormId, spell: GlobalFormId) -> bool {
        let Some(state) = self.actor(actor) else {
            return false;
        };
        if !state.spells.insert(spell) {
            return false;
        }
        self.apply_abilities(actor, [spell].into_iter().collect());
        true
    }

    fn apply_abilities(
        &mut self,
        actor: GlobalFormId,
        spells: BTreeSet<GlobalFormId>,
    ) -> BTreeSet<EffectId> {
        self.apply_abilities_with_alloc(actor, spells, None)
    }

    fn apply_abilities_with_alloc(
        &mut self,
        actor: GlobalFormId,
        spells: BTreeSet<GlobalFormId>,
        mut alloc: Option<Alloc<'_>>,
    ) -> BTreeSet<EffectId> {
        let mut owned = BTreeSet::new();
        for spell in spells {
            let Ok(definition) = self.spell(spell) else {
                continue;
            };
            if definition.data.spell_type != 4
                || definition.data.cast_type != 0
                || definition.data.delivery != 0
            {
                continue;
            }
            let Ok(prepared) = self.prepare_spell(
                spell,
                actor,
                Some(actor),
                Some(actor),
                CastDelivery::Direct,
                None,
                true,
            ) else {
                continue;
            };
            let Ok(prepared) = self.prepare_effect_actions(prepared) else {
                continue;
            };
            for prepared in prepared {
                let id = prepared.effect.id;
                owned.insert(id);
                self.next_effect = id.0 + 1;
                if prepared.effect.recover || prepared.effect.duration == Some(0.0) {
                    if let Some(state) = self.actors.get_mut(&prepared.effect.target) {
                        if change_values(&mut state.values, &prepared.effect, 1.0).is_err() {
                            continue;
                        }
                    }
                }
                self.active_effects.insert(id, prepared.effect.clone());
                if let Some(alloc) = alloc.as_deref_mut() {
                    let object = self.object_of(Handle::ActiveEffect(id), alloc);
                    self.pending.push(Pending::BindEffect {
                        object,
                        effect: prepared.effect.effect,
                    });
                    let target = self.form_value(Some(actor), alloc);
                    let caster = self.form_value(Some(actor), alloc);
                    self.pending.push(Pending::Event {
                        object,
                        name: "OnEffectStart".into(),
                        args: vec![target, caster.clone()],
                    });
                    let effect = self.form_value(Some(prepared.effect.effect), alloc);
                    self.form_event(actor, "OnMagicEffectApply", vec![caster, effect], alloc);
                }
            }
        }
        owned
    }

    fn race_effects(
        &self,
        race: GlobalFormId,
    ) -> Result<(BTreeSet<GlobalFormId>, BTreeSet<GlobalFormId>), MagicError> {
        let record = self.record(race).ok_or(MagicError::Missing(race))?;
        let mut spells = BTreeSet::new();
        let mut shouts = BTreeSet::new();
        for raw in record.all(super::T(b"SPLO")) {
            if raw.len() != 4 {
                return Err(MagicError::Unsupported("werewolf race effect layout"));
            }
            let raw = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            let spell = self
                .resolve_from(race, raw)
                .ok_or(MagicError::Missing(race))?;
            match self.tag_of(spell) {
                Some(tag) if tag == super::T(b"SPEL") => {
                    self.spell(spell)?;
                    spells.insert(spell);
                }
                Some(tag) if tag == super::T(b"SHOU") => {
                    shouts.insert(spell);
                }
                _ => return Err(MagicError::Unsupported("werewolf race effect")),
            }
        }
        Ok((spells, shouts))
    }

    fn start_werewolf_transform(
        &mut self,
        actor: GlobalFormId,
        effect: EffectId,
        race: GlobalFormId,
        race_spells: BTreeSet<GlobalFormId>,
        race_shouts: BTreeSet<GlobalFormId>,
        alloc: Alloc<'_>,
    ) -> Result<(), MagicError> {
        let state = self.actor(actor).ok_or(MagicError::Target)?;
        if state.werewolf_transform.is_some() {
            return Err(MagicError::Unsupported("actor already transformed"));
        }
        let prior_race = state.race;
        let added_spells = race_spells
            .difference(&state.spells)
            .copied()
            .collect::<BTreeSet<_>>();
        let added_shouts = race_shouts
            .difference(&state.shouts)
            .copied()
            .collect::<BTreeSet<_>>();
        state.race = Some(race);
        state.spells.extend(race_spells);
        state.shouts.extend(race_shouts);
        let added_effects =
            self.apply_abilities_with_alloc(actor, added_spells.clone(), Some(alloc));
        let state = self.actors.get_mut(&actor).ok_or(MagicError::Target)?;
        state.werewolf_transform = Some(super::WerewolfTransformState {
            effect,
            prior_race,
            transformed_race: race,
            added_spells: added_spells.clone(),
            added_shouts,
            added_effects,
        });
        Ok(())
    }

    fn finish_werewolf_transform(
        &mut self,
        actor: GlobalFormId,
        effect: EffectId,
        alloc: Alloc<'_>,
    ) {
        let Some(state) = self.actors.get(&actor) else {
            return;
        };
        let Some(transform) = state.werewolf_transform.clone() else {
            return;
        };
        if transform.effect != effect {
            return;
        }
        if let Some(state) = self.actors.get_mut(&actor) {
            if state.race == Some(transform.transformed_race) {
                state.race = transform.prior_race;
            }
            for spell in &transform.added_spells {
                state.spells.remove(spell);
            }
            for shout in &transform.added_shouts {
                state.shouts.remove(shout);
            }
            state.werewolf_transform = None;
        }
        for id in transform.added_effects {
            self.dispel_effect(id, alloc);
        }
    }
    fn vampire_lord_race(
        &self,
        effect: GlobalFormId,
        raw_race: u32,
    ) -> Result<(GlobalFormId, std::collections::BTreeSet<GlobalFormId>), MagicError> {
        let race = self
            .resolve_from(effect, raw_race)
            .filter(|race| self.tag_of(*race) == Some(super::T(b"RACE")))
            .ok_or(MagicError::Missing(effect))?;
        let record_ref = self
            .order
            .winner_ref(race)
            .ok_or(MagicError::Missing(race))?;
        let record = self.order.record(record_ref);
        if record.is_deleted() {
            return Err(MagicError::Missing(race));
        }
        let mut spells = std::collections::BTreeSet::new();
        for raw in record.all(super::T(b"SPLO")) {
            if raw.len() != 4 {
                return Err(MagicError::Unsupported("race ability layout"));
            }
            let form = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
            let Some(spell) = self.resolve_from(race, form) else {
                return Err(MagicError::Missing(race));
            };
            if self.tag_of(spell) == Some(super::T(b"SPEL"))
                && self.spell(spell)?.data.spell_type == 4
            {
                spells.insert(spell);
            }
        }
        Ok((race, spells))
    }

    fn prepare_spell(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: Option<GlobalFormId>,
        target: Option<GlobalFormId>,
        cast_delivery: CastDelivery,
        location: Option<[f32; 3]>,
        line_of_sight_clear: bool,
    ) -> Result<Vec<PreparedEffect>, MagicError> {
        let scroll = cast_delivery == CastDelivery::Scroll;
        let definition = if scroll {
            self.scroll(spell)?
        } else {
            self.spell(spell)?
        };
        if cast_delivery == CastDelivery::Location && definition.data.delivery != 4 {
            return Err(MagicError::Unsupported("location delivery"));
        }
        let concentration = matches!(
            cast_delivery,
            CastDelivery::Direct | CastDelivery::CloakPulse
        ) && definition.data.cast_type == 2;
        if (scroll && definition.data.cast_type != 3)
            || (!scroll && !concentration && !matches!(definition.data.cast_type, 0 | 1))
        {
            return Err(MagicError::Unsupported("concentration or scroll casting"));
        }
        let target = match definition.data.delivery {
            0 => source,
            1 => {
                let target = target.ok_or(MagicError::Target)?;
                if !definition.data.range.is_finite() || definition.data.range < 0.0 {
                    return Err(MagicError::Number);
                }
                let distance = self.distance(source, target);
                if self.reference(source).is_none()
                    || self.actor(target).is_none()
                    || !distance.is_finite()
                    || distance > definition.data.range
                {
                    return Err(MagicError::Target);
                }
                target
            }
            3 => target.ok_or(MagicError::Target)?,
            4 if cast_delivery == CastDelivery::Location => {
                let position = location.ok_or(MagicError::Target)?;
                if !position.iter().all(|value| value.is_finite())
                    || !definition.data.range.is_finite()
                    || definition.data.range < 0.0
                {
                    return Err(MagicError::Number);
                }
                let Some(source_position) = self.reference(source).map(|r| r.position) else {
                    return Err(MagicError::Target);
                };
                if !source_position.iter().all(|value| value.is_finite()) {
                    return Err(MagicError::Target);
                }
                let distance = source_position
                    .iter()
                    .zip(position)
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<f32>()
                    .sqrt();
                if !distance.is_finite() || distance > definition.data.range || !line_of_sight_clear
                {
                    return Err(MagicError::Target);
                }
                source
            }
            2 if matches!(
                cast_delivery,
                CastDelivery::ProjectileContact | CastDelivery::CloakPulse
            ) =>
            {
                target.ok_or(MagicError::Target)?
            }
            2 => {
                return Err(MagicError::Unsupported(
                    "aimed cast requires projectile contact",
                ))
            }
            _ => return Err(MagicError::Unsupported("location delivery")),
        };
        if self.reference(source).is_none() || self.reference(target).is_none() {
            return Err(MagicError::Target);
        }
        let caster = caster.filter(|id| self.actor(*id).is_some());
        let mut prepared = Vec::new();
        let mut commanded_targets = std::collections::BTreeSet::new();
        for (index, entry) in definition.effects.iter().enumerate() {
            let effect = self
                .resolve_from(spell, entry.base_effect)
                .ok_or(MagicError::Missing(spell))?;
            let base = self.magic_effect(effect)?;
            let data = &base.data;
            if data.archetype == PEAK_VALUE_MODIFIER {
                self.peak_value_keyword(effect, &base)?;
            }
            let context = Context {
                owner: spell,
                subject: target,
                target: caster,
                quest: None,
                tempering_enchanted: None,
                tempering_item: None,
                alchemy_making_poison: None,
                alchemy_effect: None,
                magic_item: Some(spell),
                dual_cast: Some(false),
                skill_use: None,
                light_level: None,
            };
            if !conditions::evaluate_without_vm(self, context, &entry.conditions)? {
                continue;
            }
            let context = Context {
                owner: effect,
                ..context
            };
            if !conditions::evaluate_without_vm(self, context, &base.conditions)? {
                continue;
            }
            if data.flags & HOSTILE != 0 {
                let actor = self.actor(target).ok_or(MagicError::Target)?;
                if actor.dead || actor.ghost {
                    return Err(MagicError::Target);
                }
            }
            if data.archetype == 40 {
                return Err(MagicError::Unsupported(
                    "spawn hazard requires hazard ownership and expiry",
                ));
            }
            if !matches!(
                data.archetype,
                0 | 1
                    | 2
                    | 5
                    | 18
                    | PARALYSIS_ARCHETYPE
                    | CURE_PARALYSIS_ARCHETYPE
                    | 36
                    | 46
                    | 3
                    | 4
                    | 6
                    | 9
                    | 11
                    | 12
                    | LOCK
                    | OPEN
                    | 17
                    | 35
                    | 29
                    | 31
                    | 32
                    | 33
                    | PEAK_VALUE_MODIFIER
                    | SLOW_TIME
                    | 45
                    | 10
                    | 20
                    | 38
                    | DEMORALIZE
                    | FRENZY_ARCHETYPE
                    | TURN_UNDEAD
            ) {
                return Err(MagicError::Unsupported("effect archetype"));
            }
            if matches!(data.archetype, LOCK | OPEN) {
                if !self.lockable_reference(target) {
                    return Err(MagicError::Target);
                }
                if entry.data.area != 0 || data.area != 0 {
                    return Err(MagicError::Unsupported("lock area"));
                }
            } else if self.actor(target).is_none() {
                return Err(MagicError::Target);
            }
            if data.archetype == TURN_UNDEAD
                && self
                    .form("ActorTypeUndead")
                    .is_none_or(|keyword| !self.has_keyword(target, keyword))
            {
                continue;
            }
            if data.taper_duration != 0.0 {
                return Err(MagicError::Unsupported("taper"));
            }
            if data.perk != 0 {
                return Err(MagicError::Unsupported("perk application"));
            }
            self.validate_equip_ability(effect, data.equip_ability)?;
            let cast_type_matches = if scroll {
                data.cast_type == 1
            } else {
                data.cast_type == definition.data.cast_type
            };
            if !cast_type_matches || data.delivery != definition.data.delivery {
                return Err(MagicError::Unsupported("mixed casting types or delivery"));
            }
            let magnitude = if data.flags & NO_MAGNITUDE != 0 && data.archetype != 35 {
                0.0
            } else {
                entry.data.magnitude
            };
            if !magnitude.is_finite() || magnitude < 0.0 {
                return Err(MagicError::Number);
            }
            let signed = if data.flags & DETRIMENTAL != 0 {
                -magnitude
            } else {
                magnitude
            };
            let mut changes = Vec::new();
            if matches!(data.archetype, 0 | 5 | PEAK_VALUE_MODIFIER) {
                let name = actor_value_name(data.primary_actor_value)
                    .ok_or(MagicError::Unsupported("actor value"))?;
                changes.push((name.to_string(), signed));
                if data.archetype == 5 {
                    let name = actor_value_name(data.second_actor_value)
                        .ok_or(MagicError::Unsupported("secondary actor value"))?;
                    let amount = signed * data.second_actor_value_weight;
                    if !amount.is_finite() {
                        return Err(MagicError::Number);
                    }
                    changes.push((name.to_string(), amount));
                }
            }
            if data.archetype == 32 {
                let name = actor_value_name(data.primary_actor_value)
                    .ok_or(MagicError::Unsupported("actor value"))?;
                changes.push((name.to_string(), signed));
            }
            let summon_base = if data.archetype == 18 {
                let base = self
                    .resolve_from(effect, data.associated_item)
                    .ok_or(MagicError::Missing(effect))?;
                if self.tag_of(base) != Some(super::T(b"NPC_")) {
                    return Err(MagicError::Unsupported("summon actor base"));
                }
                if self
                    .npc(base)
                    .is_none_or(|npc| npc.config.flags & crate::npc::flags::SUMMONABLE == 0)
                {
                    return Err(MagicError::Unsupported("summon actor not summonable"));
                }
                Some(base)
            } else {
                None
            };
            let werewolf_race = if data.archetype == 36 {
                let race = self
                    .resolve_from(effect, data.associated_item)
                    .ok_or(MagicError::Missing(effect))?;
                if self.tag_of(race) != Some(super::T(b"RACE"))
                    || self.record(race).is_none_or(|record| record.is_deleted())
                {
                    return Err(MagicError::Unsupported("werewolf race"));
                }
                let (spells, shouts) = self.race_effects(race)?;
                Some((race, spells, shouts))
            } else {
                None
            };
            let vampire_form = if data.archetype == 46 {
                Some(self.vampire_lord_race(effect, data.associated_item)?)
            } else {
                None
            };
            let command_owner = if data.archetype == 10 {
                caster.or_else(|| self.actor(source).map(|_| source))
            } else {
                None
            };
            let light_base = if data.archetype == LIGHT {
                Some(
                    self.resolve_from(effect, data.associated_item)
                        .filter(|item| self.tag_of(*item) == Some(super::T(b"LIGH")))
                        .ok_or(MagicError::Unsupported("light record"))?,
                )
            } else {
                None
            };
            if data.archetype == 35 {
                let associated = self
                    .resolve_from(effect, data.associated_item)
                    .ok_or(MagicError::Missing(effect))?;
                if self.tag_of(associated) != Some(super::T(b"SPEL")) {
                    return Err(MagicError::Unsupported("cloak associated spell"));
                }
                let cloak_spell = self.spell(associated)?;
                if cloak_spell.data.cast_type != 2 || !matches!(cloak_spell.data.delivery, 2 | 3) {
                    return Err(MagicError::Unsupported(
                        "cloak spell must be concentration aimed or target",
                    ));
                }
            }
            let bound_weapon = if data.archetype == 17 {
                let weapon = self
                    .resolve_from(effect, data.associated_item)
                    .filter(|item| self.tag_of(*item) == Some(super::T(b"WEAP")))
                    .ok_or(MagicError::Unsupported("bound weapon item"))?;
                let (_, weapon_data) = self
                    .combat_weapon_source(weapon)
                    .map_err(|_| MagicError::Unsupported("bound weapon record"))?;
                if weapon_data.flags2 & WEAPON_BOUND == 0 {
                    return Err(MagicError::Unsupported("weapon is not marked bound"));
                }
                self.validate_hand_weapon_grant(target, Hand::Right, weapon)
                    .map_err(|_| MagicError::Unsupported("bound weapon equipment"))?;
                Some(weapon)
            } else {
                None
            };
            let lock_level = (data.archetype == LOCK).then(|| magnitude.round() as i32);
            let disarm = data.archetype == 9;
            let duration = if (definition.data.cast_type == 0 && data.archetype != TURN_UNDEAD)
                || (data.archetype == 32 && data.flags & NO_DURATION != 0)
            {
                None
            } else {
                Some(if data.flags & NO_DURATION != 0 {
                    0.0
                } else {
                    f64::from(entry.data.duration)
                })
            };
            let stagger_duration = (data.archetype == 33)
                .then(|| {
                    self.combat_setting("fStaggerDuration")
                        .map_err(|_| MagicError::StaggerSetting("fStaggerDuration"))
                })
                .transpose()?;
            let mut dispel_rules = Vec::new();
            if data.archetype == 2 {
                dispel_rules.push(DispelRule::Timed);
            }
            if data.archetype == CURE_PARALYSIS_ARCHETYPE {
                dispel_rules.push(DispelRule::Paralysis);
            }
            if data.archetype == 3 {
                dispel_rules.push(DispelRule::Disease);
            }
            if data.archetype == 29 {
                dispel_rules.push(DispelRule::Poisons);
            }
            if data.flags & DISPEL_WITH_KEYWORDS != 0 {
                let keywords = self.keyword_entries(effect);
                for keyword in &keywords {
                    if self.tag_of(*keyword) != Some(super::T(b"KYWD")) {
                        return Err(MagicError::Missing(*keyword));
                    }
                }
                dispel_rules.push(DispelRule::Keywords(keywords));
            }
            let radius = if data.flags & NO_AREA != 0 {
                0.0
            } else {
                entry.data.area as f32
            };
            let mut recipients = if werewolf_race.is_some()
                || vampire_form.is_some()
                || matches!(data.archetype, LOCK | OPEN)
            {
                vec![target]
            } else if definition.data.delivery == 4 {
                self.area_targets_at_position(location.ok_or(MagicError::Target)?, radius)
            } else {
                self.area_targets(target, radius)
            };
            if data.flags & HOSTILE != 0 {
                recipients.retain(|&id| {
                    self.actor(id)
                        .is_some_and(|actor| !actor.dead && !actor.ghost)
                });
            }
            for recipient in recipients {
                let restore_owner = if data.archetype == 10 {
                    let is_summoned = self
                        .active_effects
                        .values()
                        .any(|active| active.summoned == Some(recipient));
                    let is_already_commanded = self
                        .active_effects
                        .values()
                        .any(|active| active.target == recipient && active.restore_owner.is_some());
                    if command_owner.is_none()
                        || !is_summoned
                        || is_already_commanded
                        || !commanded_targets.insert(recipient)
                    {
                        continue;
                    }
                    Some(
                        match self
                            .reference_ref(recipient)
                            .and_then(|reference| reference.owner)
                        {
                            Some(owner) => PriorOwner::Owned(owner),
                            None => PriorOwner::Unowned,
                        },
                    )
                } else {
                    None
                };
                let magnitude = if !scroll && definition.data.flags & IGNORE_RESISTANCE != 0 {
                    magnitude
                } else {
                    self.resisted_magnitude(recipient, data, magnitude)?
                };
                let signed = if data.flags & DETRIMENTAL != 0 {
                    -magnitude
                } else {
                    magnitude
                };
                let mut recipient_changes = Vec::new();
                if matches!(data.archetype, 0 | 4 | 5 | 31 | 32 | PEAK_VALUE_MODIFIER) {
                    let name = actor_value_name(data.primary_actor_value)
                        .ok_or(MagicError::Unsupported("actor value"))?;
                    recipient_changes.push((
                        name.to_string(),
                        if data.archetype == 4 {
                            -magnitude
                        } else {
                            signed
                        },
                    ));
                    if data.archetype == 5 {
                        let name = actor_value_name(data.second_actor_value)
                            .ok_or(MagicError::Unsupported("secondary actor value"))?;
                        let amount = signed * data.second_actor_value_weight;
                        if !amount.is_finite() {
                            return Err(MagicError::Number);
                        }
                        recipient_changes.push((name.to_string(), amount));
                    }
                } else if data.archetype == PARALYSIS_ARCHETYPE {
                    recipient_changes.push(("paralysis".into(), signed));
                } else if data.archetype == 6 {
                    recipient_changes.push(("aggression".into(), -magnitude));
                } else if data.archetype == DEMORALIZE {
                    recipient_changes.push(("confidence".into(), -magnitude));
                }
                if data.archetype == FRENZY_ARCHETYPE {
                    recipient_changes.push(("aggression".into(), signed));
                }
                prepared.push(PreparedEffect {
                    effect: ActiveEffect {
                        id: EffectId(0),
                        spell,
                        effect,
                        effect_index: index as u32,
                        source,
                        caster,
                        target: recipient,
                        magnitude,
                        duration,
                        elapsed: 0.0,
                        summoned: None,
                        restore_owner,
                        recover: data.flags & RECOVER != 0
                            || matches!(data.archetype, PARALYSIS_ARCHETYPE | DEMORALIZE | 38),
                        changes: recipient_changes,
                    },
                    no_recast: data.flags & NO_RECAST != 0 || data.archetype == 46,
                    summon_base,
                    werewolf_race: werewolf_race.clone(),
                    command_owner,
                    light_base,
                    bound_weapon,
                    lock_level,
                    open_lock: data.archetype == OPEN,
                    disarm,
                    stagger_duration,
                    hit_event: data.flags & (HOSTILE | NO_HIT_EVENT) == HOSTILE,
                    dispel_rules: dispel_rules.clone(),
                    dispel: Vec::new(),
                    vampire_form: vampire_form.clone(),
                });
            }
        }
        Ok(prepared)
    }

    fn apply_prepared_effects(
        &mut self,
        prepared: Vec<PreparedEffect>,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let (locks, remaining): (Vec<_>, Vec<_>) = prepared
            .into_iter()
            .partition(|effect| effect.lock_level.is_some());
        let (opens, ordinary): (Vec<_>, Vec<_>) =
            remaining.into_iter().partition(|effect| effect.open_lock);
        let disarms: Vec<_> = ordinary
            .iter()
            .filter(|prepared| prepared.disarm)
            .map(|prepared| (prepared.effect.target, prepared.effect.magnitude))
            .collect();
        for effect in locks {
            if let Some(level) = effect.lock_level {
                self.set_lock_level(effect.effect.target, level, alloc);
            }
        }
        for effect in opens {
            let target = effect.effect.target;
            if self.reference_ref(target).is_some_and(|state| {
                state.locked && state.lock_level as f32 <= effect.effect.magnitude
            }) {
                self.set_locked(target, false, alloc);
            }
        }
        let prepared = self.prepare_effect_actions(ordinary)?;
        self.apply_prepared_actions(prepared, alloc, disarms)
    }

    fn apply_prepared_plan(
        &mut self,
        prepared: Vec<PreparedEffect>,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        self.apply_prepared_actions(prepared, alloc, Vec::new())
    }

    fn apply_prepared_actions(
        &mut self,
        prepared: Vec<PreparedEffect>,
        alloc: Alloc<'_>,
        disarms: Vec<(GlobalFormId, f32)>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let mut applied = Vec::new();
        for mut prepared in prepared {
            let id = prepared.effect.id;
            self.next_effect = id.0 + 1;
            if let Some(weapon) = prepared.bound_weapon {
                let inventory = &mut self
                    .reference(prepared.effect.target)
                    .ok_or(MagicError::Target)?
                    .inventory;
                inventory
                    .add(weapon, 1)
                    .map_err(|_| MagicError::Unsupported("bound weapon inventory"))?;
                if self
                    .equip_hand_weapon(prepared.effect.target, Hand::Right, weapon)
                    .is_err()
                {
                    self.reference(prepared.effect.target)
                        .ok_or(MagicError::Target)?
                        .inventory
                        .take(weapon, 1);
                    return Err(MagicError::Unsupported("bound weapon equipment"));
                }
                self.inventory_event(prepared.effect.target, weapon, 1, None, alloc);
            }
            if let Some(duration) = prepared.stagger_duration {
                self.begin_stagger(
                    prepared.effect.target,
                    f64::from(prepared.effect.magnitude),
                    prepared.effect.caster.or(Some(prepared.effect.source)),
                    self.clock.real_seconds + duration,
                );
            }
            if let Some(base) = prepared.summon_base {
                let summon = self.place_at(base, prepared.effect.source, false);
                let owner = prepared.effect.caster.unwrap_or(prepared.effect.target);
                if let Some(reference) = self.reference(summon) {
                    reference.owner = Some(owner);
                }
                prepared.effect.summoned = Some(summon);
            }
            if let Some((race, spells, shouts)) = prepared.werewolf_race {
                self.start_werewolf_transform(
                    prepared.effect.target,
                    prepared.effect.id,
                    race,
                    spells,
                    shouts,
                    alloc,
                )?;
            }
            if let Some(previous_owner) = prepared.effect.restore_owner {
                if let Some(reference) = self.reference(prepared.effect.target) {
                    reference.owner = prepared.command_owner;
                }
                prepared.effect.restore_owner = Some(previous_owner);
            }
            if let Some(base) = prepared.light_base {
                prepared.effect.summoned = Some(self.place_at(base, prepared.effect.target, false));
            }
            if prepared.hit_event {
                let aggressor = self.form_value(
                    prepared.effect.caster.or(Some(prepared.effect.source)),
                    alloc,
                );
                let spell_value = self.form_value(Some(prepared.effect.spell), alloc);
                let args = vec![
                    aggressor,
                    spell_value,
                    Value::None,
                    Value::Bool(false),
                    Value::Bool(false),
                    Value::Bool(false),
                    Value::Bool(false),
                ];
                for recipient in self.actor_event_recipients(prepared.effect.target) {
                    let object = self.object_of(recipient, alloc);
                    self.pending.push(Pending::Event {
                        object,
                        name: "OnHit".into(),
                        args: args.clone(),
                    });
                }
            }
            let object = self.object_of(Handle::ActiveEffect(id), alloc);
            self.pending.push(Pending::BindEffect {
                object,
                effect: prepared.effect.effect,
            });
            let target_value = self.form_value(Some(prepared.effect.target), alloc);
            let caster_value = self.form_value(prepared.effect.caster, alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnEffectStart".into(),
                args: vec![target_value, caster_value],
            });
            let caster_value = self.form_value(prepared.effect.caster, alloc);
            let effect_value = self.form_value(Some(prepared.effect.effect), alloc);
            self.form_event(
                prepared.effect.target,
                "OnMagicEffectApply",
                vec![caster_value, effect_value],
                alloc,
            );
            // An instant lethal effect must receive its target's death events too.
            self.active_effects.insert(id, prepared.effect.clone());
            if let Some((vampire_race, race_spells)) = prepared.vampire_form.clone() {
                let actor_id = prepared.effect.target;
                let previous_race = self.actor(actor_id).and_then(|actor| actor.race);
                let added_spells: std::collections::BTreeSet<_> = {
                    let actor = self.actor(actor_id).ok_or(MagicError::Target)?;
                    actor.race = Some(vampire_race);
                    race_spells
                        .into_iter()
                        .filter(|spell| actor.spells.insert(*spell))
                        .collect()
                };
                let ability_effects = self
                    .apply_abilities(actor_id, added_spells.clone())
                    .into_iter()
                    .collect::<Vec<_>>();
                for ability_effect in &ability_effects {
                    let Some(effect_form) = self
                        .active_effects
                        .get(ability_effect)
                        .map(|active| active.effect)
                    else {
                        continue;
                    };
                    let object = self.object_of(Handle::ActiveEffect(*ability_effect), alloc);
                    self.pending.push(Pending::BindEffect {
                        object,
                        effect: effect_form,
                    });
                    let target_value = self.form_value(Some(actor_id), alloc);
                    self.pending.push(Pending::Event {
                        object,
                        name: "OnEffectStart".into(),
                        args: vec![target_value.clone(), target_value],
                    });
                    let caster_value = self.form_value(Some(actor_id), alloc);
                    let effect_value = self.form_value(Some(effect_form), alloc);
                    self.form_event(
                        actor_id,
                        "OnMagicEffectApply",
                        vec![caster_value, effect_value],
                        alloc,
                    );
                }
                self.vampire_lord_forms.insert(
                    id,
                    VampireLordForm {
                        actor: actor_id,
                        previous_race,
                        vampire_race,
                        added_spells,
                        ability_effects,
                    },
                );
            }
            if self
                .magic_effect(prepared.effect.effect)
                .is_ok_and(|base| base.data.archetype == 35)
            {
                self.cloak_effects.insert(id, CloakState::default());
            }
            for victim in prepared.dispel {
                self.dispel_effect(victim, alloc);
            }
            if prepared.effect.recover
                || prepared.effect.duration == Some(0.0)
                || self.is_peak_value_effect(prepared.effect.effect)?
            {
                self.apply_effect_values(&prepared.effect, 1.0, alloc)?;
            }
            applied.push(id);
        }
        for (target, magnitude) in disarms {
            self.disarm_equipped_weapon(target, magnitude, alloc);
        }
        Ok(applied)
    }

    /// Drop one eligible hand weapon into a loose reference at the target.
    fn disarm_equipped_weapon(&mut self, target: GlobalFormId, magnitude: f32, alloc: Alloc<'_>) {
        let Some(level) = self.actor(target).map(|actor| actor.level) else {
            return;
        };
        if level as f32 > magnitude {
            return;
        }
        let Some(weapon) = self
            .hand_weapon(target, Hand::Right)
            .or_else(|| self.hand_weapon(target, Hand::Left))
        else {
            return;
        };
        let bundle = self.remove_item_bundle(target, weapon, 1, None);
        if bundle.count() != 1 {
            return;
        }
        let item = bundle
            .stacks()
            .first()
            .cloned()
            .unwrap_or_else(crate::inventory::Stack::plain_one);
        let dropped = self.place_at(weapon, target, false);
        if let Some(reference) = self.reference(dropped) {
            reference.item = Some(item);
        }
        if let Some(actor) = self.actor(target) {
            actor.equipped.remove(&weapon);
            if actor.left_weapon == Some(weapon) {
                actor.left_weapon = None;
            }
            if actor.right_weapon == Some(weapon) {
                actor.right_weapon = None;
            }
        }
        if let Some(reference) = self.reference(target) {
            reference.inventory.unequip(weapon);
        }
        self.inventory_event(target, weapon, -1, None, alloc);
    }

    fn resisted_magnitude(
        &mut self,
        target: GlobalFormId,
        data: &soe_esm::records::gameplay::MagicEffectData,
        magnitude: f32,
    ) -> Result<f32, MagicError> {
        if data.flags & HOSTILE == 0 {
            return Ok(magnitude);
        }
        if data.resistance != -1
            && !matches!(
                data.resistance,
                FIRST_SPECIFIC_RESISTANCE..=LAST_SPECIFIC_RESISTANCE
            )
        {
            return Err(MagicError::Unsupported("magic resistance value"));
        }
        let values = &self.actor(target).ok_or(MagicError::Target)?.values;
        let mut resisted = magnitude;
        let magic_resistance = values
            .get("resistmagic")
            .current()
            .clamp(0.0, MAX_RESISTANCE);
        resisted *= 1.0 - magic_resistance / 100.0;
        if data.resistance != -1 && data.resistance != RESIST_MAGIC {
            let name = actor_value_name(data.resistance)
                .ok_or(MagicError::Unsupported("magic resistance value"))?;
            let specific = values.get(name).current().clamp(0.0, MAX_RESISTANCE);
            resisted *= 1.0 - specific / 100.0;
        }
        if !resisted.is_finite() {
            return Err(MagicError::Number);
        }
        Ok(resisted)
    }

    /// Validate authored action order before events or live actor values change.
    ///
    /// An area hit prepares one `PreparedEffect` per selected actor, each
    /// carrying its own `target`. Live effects and actor values are tracked
    /// per target here, so one recipient's dispels and value changes never
    /// leak into another's.
    fn prepare_effect_actions(
        &mut self,
        prepared: Vec<PreparedEffect>,
    ) -> Result<Vec<PreparedEffect>, MagicError> {
        let mut live_by_target: std::collections::HashMap<GlobalFormId, Vec<ActiveEffect>> =
            std::collections::HashMap::new();
        let mut values_by_target: std::collections::HashMap<
            GlobalFormId,
            crate::actor_values::Values,
        > = std::collections::HashMap::new();
        let mut next = self.next_effect;
        let mut plan = Vec::new();
        for mut prepared in prepared {
            let target = prepared.effect.target;
            let live = live_by_target.entry(target).or_insert_with(|| {
                self.active_effects
                    .values()
                    .filter(|effect| effect.target == target)
                    .cloned()
                    .collect()
            });
            let values = match values_by_target.entry(target) {
                std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    let base = self.actor(target).ok_or(MagicError::Target)?.values.clone();
                    entry.insert(base)
                }
            };
            if self.magic_effect(prepared.effect.effect)?.data.archetype == 38 {
                let confidence = values.get("confidence").current();
                prepared.effect.changes = vec![(
                    "confidence".to_string(),
                    (prepared.effect.magnitude - confidence).max(0.0),
                )];
            }
            if prepared.no_recast
                && live
                    .iter()
                    .any(|effect| effect.effect == prepared.effect.effect)
            {
                continue;
            }
            prepared.effect.id = EffectId(next);
            next = next.checked_add(1).ok_or(MagicError::Identity)?;
            let mut selected = std::collections::BTreeSet::new();
            for rule in &prepared.dispel_rules {
                match rule {
                    DispelRule::Timed => selected.extend(
                        live.iter()
                            .filter(|effect| effect.duration.is_some_and(|duration| duration > 0.0))
                            .map(|effect| effect.id),
                    ),
                    DispelRule::Disease => {
                        if let Some(keyword) = self.form("MagicDisease") {
                            for effect in live.iter() {
                                if self.keyword_entries(effect.effect).contains(&keyword) {
                                    selected.insert(effect.id);
                                }
                            }
                        }
                    }
                    DispelRule::Keywords(keywords) => {
                        for effect in live.iter() {
                            self.magic_effect(effect.effect)?;
                            if self
                                .keywords(effect.effect)
                                .iter()
                                .any(|keyword| keywords.contains(keyword))
                            {
                                selected.insert(effect.id);
                            }
                        }
                    }
                    DispelRule::Paralysis => {
                        for effect in live.iter() {
                            if self.magic_effect(effect.effect)?.data.archetype
                                == PARALYSIS_ARCHETYPE
                            {
                                selected.insert(effect.id);
                            }
                        }
                    }
                    DispelRule::Poisons => {
                        for effect in live.iter() {
                            if self.tag_of(effect.source) == Some(super::T(b"ALCH"))
                                && self.ingestible(effect.source)?.data.flags & INGESTIBLE_POISON
                                    != 0
                            {
                                selected.insert(effect.id);
                            }
                        }
                    }
                }
            }
            for effect in live.iter() {
                if selected.contains(&effect.id) && effect.recover {
                    change_values(values, effect, -1.0)?;
                }
            }
            live.retain(|effect| !selected.contains(&effect.id));
            prepared.dispel = selected.into_iter().collect();
            if prepared.effect.recover || prepared.effect.duration == Some(0.0) {
                change_values(values, &prepared.effect, 1.0)?;
            }
            live.push(prepared.effect.clone());
            plan.push(prepared);
        }
        Ok(plan)
    }

    /// Match active MGEF identity without changing known or equipped spells.
    pub fn has_magic_effect(&self, target: GlobalFormId, effect: GlobalFormId) -> bool {
        self.active_effects
            .values()
            .any(|e| e.target == target && e.effect == effect)
    }

    /// Check an actor's known spells and active MGEF equip-ability grants.
    pub fn has_spell(&mut self, actor: GlobalFormId, spell: GlobalFormId) -> bool {
        if self
            .actor(actor)
            .is_some_and(|state| state.spells.contains(&spell))
        {
            return true;
        }
        let effects: Vec<_> = self
            .active_effects
            .values()
            .filter(|effect| effect.target == actor)
            .map(|effect| effect.effect)
            .collect();
        effects.into_iter().any(|effect| {
            self.magic_effect(effect).is_ok_and(|definition| {
                self.resolve_from(effect, definition.data.equip_ability) == Some(spell)
            })
        })
    }

    fn validate_equip_ability(&self, effect: GlobalFormId, raw: u32) -> Result<(), MagicError> {
        if raw == 0 {
            return Ok(());
        }
        let ability = self
            .resolve_from(effect, raw)
            .ok_or(MagicError::Missing(effect))?;
        let rref = self
            .order
            .winner_ref(ability)
            .ok_or(MagicError::Missing(ability))?;
        if self.order.record(rref).is_deleted() || self.tag_of(ability) != Some(super::T(b"SPEL")) {
            return Err(MagicError::Missing(ability));
        }
        self.spell(ability)?;
        Ok(())
    }

    /// Match keywords on each active effect's winning definition.
    pub fn has_magic_effect_with_keyword(
        &mut self,
        target: GlobalFormId,
        keyword: GlobalFormId,
    ) -> bool {
        let effects: Vec<_> = self
            .active_effects
            .values()
            .filter(|e| e.target == target)
            .map(|e| e.effect)
            .collect();
        effects
            .into_iter()
            .any(|id| self.keyword_entries(id).contains(&keyword))
    }

    /// End every application from this spell on this actor.
    pub fn dispel_spell(
        &mut self,
        target: GlobalFormId,
        spell: GlobalFormId,
        alloc: Alloc<'_>,
    ) -> bool {
        let ids: Vec<_> = self
            .active_effects
            .values()
            .filter(|e| e.target == target && e.spell == spell)
            .map(|e| e.id)
            .collect();
        let removed = !ids.is_empty();
        for id in ids {
            self.dispel_effect(id, alloc);
        }
        removed
    }

    /// End ordinary spells, preserving diseases, abilities, addictions and
    /// constant enchantments as specified by Actor.DispelAllSpells.
    pub fn dispel_all_spells(&mut self, target: GlobalFormId, alloc: Alloc<'_>) {
        let ids: Vec<_> = self
            .active_effects
            .values()
            .filter(|e| {
                e.target == target
                    && !self.is_worn_enchantment(e)
                    && self.spell(e.spell).is_ok_and(|s| {
                        !(matches!(s.data.spell_type, 1 | 4 | 10)
                            || s.data.spell_type == 6 && s.data.cast_type == 0)
                    })
            })
            .map(|e| e.id)
            .collect();
        for id in ids {
            self.dispel_effect(id, alloc);
        }
    }

    fn is_worn_enchantment(&self, effect: &ActiveEffect) -> bool {
        self.item_enchantment(effect.source)
            .is_ok_and(|enchantment| enchantment == Some(effect.spell))
            && self
                .actor_ref(effect.target)
                .is_some_and(|actor| actor.equipped.contains(&effect.source))
    }

    /// End one application and reverse only its temporary value changes.
    pub fn dispel_effect(&mut self, id: EffectId, alloc: Alloc<'_>) -> bool {
        let Some(effect) = self.active_effects.get(&id).cloned() else {
            return false;
        };
        if (effect.recover || self.is_peak_value_effect(effect.effect).unwrap_or(false))
            && self.apply_effect_values(&effect, -1.0, alloc).is_err()
        {
            return false;
        }
        if let Some(cloak) = self.cloak_effects.remove(&id) {
            for owned in cloak.owned {
                self.dispel_effect(owned, alloc);
            }
        }
        for cloak in self.cloak_effects.values_mut() {
            cloak.owned.retain(|owned| *owned != id);
        }
        if let Some(summon) = effect.summoned {
            if let Some(reference) = self.reference(summon) {
                reference.deleted = true;
                reference.enabled = false;
            }
        }
        self.finish_werewolf_transform(effect.target, effect.id, alloc);
        if let Some(form) = self.vampire_lord_forms.remove(&id) {
            if let Some(actor) = self.actor(form.actor) {
                if actor.race == Some(form.vampire_race) {
                    actor.race = form.previous_race;
                }
                for spell in &form.added_spells {
                    actor.spells.remove(spell);
                }
            }
            for ability_effect in form.ability_effects {
                self.dispel_effect(ability_effect, alloc);
            }
        }
        if let Some(previous_owner) = effect.restore_owner {
            if let Some(reference) = self.reference(effect.target) {
                reference.owner = match previous_owner {
                    PriorOwner::Unowned => None,
                    PriorOwner::Owned(owner) => Some(owner),
                };
            }
        }
        if let Ok(definition) = self.magic_effect(effect.effect) {
            if definition.data.archetype == 17 {
                if let Some(weapon) = self
                    .resolve_from(effect.effect, definition.data.associated_item)
                    .filter(|item| self.tag_of(*item) == Some(super::T(b"WEAP")))
                {
                    if self.item_count(effect.target, weapon) <= 1 {
                        if let Some(actor) = self.actor(effect.target) {
                            if actor.left_weapon == Some(weapon) {
                                actor.left_weapon = None;
                            }
                            if actor.right_weapon == Some(weapon) {
                                actor.right_weapon = None;
                            }
                            actor.equipped.remove(&weapon);
                        }
                        if let Some(reference) = self.reference(effect.target) {
                            reference.inventory.unequip(weapon);
                        }
                    }
                    self.remove_item_notified(effect.target, weapon, 1, None, alloc);
                }
            }
        }
        self.active_effects.remove(&id);
        if self
            .magic_effect(effect.effect)
            .is_ok_and(|base| base.data.archetype == FRENZY_ARCHETYPE)
            && !self.is_frenzied(effect.target)
        {
            if let Some(actor) = self.actors.get_mut(&effect.target) {
                actor.in_combat = false;
                actor.combat_target = None;
                actor.ai.reevaluate = true;
                actor.ai.path.clear();
                actor.ai.path_cells.clear();
                actor.ai.destination = None;
                actor.ai.arrived = false;
            }
        }
        if let Some(object) = self.object_id(Handle::ActiveEffect(id)) {
            self.timers.retain(|t| t.object != object);
            self.sleep_listeners.retain(|listener| *listener != object);
            self.stats_listeners.retain(|listener| *listener != object);
            self.animation_listeners
                .retain(|listener| listener.listener != object);
            self.los_listeners
                .retain(|listener| listener.listener != object);
            self.inventory_filters.remove(&object);
            let target = self.form_value(Some(effect.target), alloc);
            let caster = self.form_value(effect.caster, alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnEffectFinish".into(),
                args: vec![target, caster],
            });
        }
        true
    }

    /// Advance magic in unpaused simulation seconds, including the final
    /// partial interval when one host tick crosses an effect's expiry.
    pub fn tick_magic(&mut self, seconds: f64, alloc: Alloc<'_>) -> Vec<MagicError> {
        let mut errors = Vec::new();
        if !seconds.is_finite() || seconds < 0.0 {
            return errors;
        }
        let ids: Vec<_> = self.active_effects.keys().copied().collect();
        for id in ids {
            let Some(mut effect) = self.active_effects.get(&id).cloned() else {
                continue;
            };
            let dead = self.actor(effect.target).is_some_and(|a| a.dead);
            let persist_death = self
                .magic_effect(effect.effect)
                .is_ok_and(|base| base.data.flags & NO_DEATH_DISPEL != 0);
            if dead && !persist_death {
                self.dispel_effect(id, alloc);
                continue;
            }
            if self.cloak_effects.contains_key(&id) {
                if let Err(error) = self.tick_cloak(id, seconds, alloc) {
                    errors.push(error);
                    continue;
                }
                let expired = self.active_effects.get(&id).is_some_and(|current| {
                    current
                        .duration
                        .is_some_and(|duration| current.elapsed >= duration)
                });
                if expired {
                    self.dispel_effect(id, alloc);
                }
                continue;
            }
            let spell_conditions = if self.tag_of(effect.spell) == Some(super::T(b"SPEL")) {
                match self
                    .magic_item_effect(effect.spell, effect.effect_index)
                    .and_then(|entry| {
                        let base = self.magic_effect(effect.effect)?;
                        Ok(!entry.conditions.is_empty() || !base.conditions.is_empty())
                    }) {
                    Ok(has_conditions) => has_conditions,
                    Err(error) => {
                        errors.push(error);
                        continue;
                    }
                }
            } else {
                false
            };
            let mut remaining = seconds;
            let mut failed = false;
            while remaining > 0.0 {
                let until_expiry = effect
                    .duration
                    .map_or(remaining, |duration| (duration - effect.elapsed).max(0.0));
                let to_check = if spell_conditions {
                    let fraction = effect.elapsed.rem_euclid(1.0);
                    if fraction == 0.0 {
                        1.0
                    } else {
                        1.0 - fraction
                    }
                } else {
                    remaining
                };
                let dt = remaining.min(until_expiry).min(to_check);
                if dt <= 0.0 {
                    break;
                }
                let reached_check = spell_conditions && dt + 1e-9 >= to_check;
                if effect.duration != Some(0.0) {
                    let result = if self
                        .magic_effect(effect.effect)
                        .is_ok_and(|base| base.data.archetype == 32)
                    {
                        self.accumulate_effect_value(&effect, dt)
                    } else if !effect.recover
                        && !self.is_peak_value_effect(effect.effect).unwrap_or(false)
                    {
                        self.apply_effect_values(&effect, dt, alloc)
                    } else {
                        Ok(())
                    };
                    if let Err(error) = result {
                        errors.push(error);
                        failed = true;
                        break;
                    }
                }
                effect.elapsed = (effect.elapsed + dt).min(f64::MAX);
                if reached_check {
                    effect.elapsed = effect.elapsed.round();
                }
                remaining -= dt;
                let expired = effect
                    .duration
                    .is_some_and(|duration| effect.elapsed >= duration);
                if expired {
                    break;
                }
                if reached_check {
                    match self.spell_effect_conditions_hold(&effect) {
                        Ok(true) => {}
                        Ok(false) => {
                            self.active_effects.insert(id, effect.clone());
                            self.dispel_effect(id, alloc);
                            failed = true;
                            break;
                        }
                        Err(error) => {
                            errors.push(error);
                            self.active_effects.insert(id, effect.clone());
                            failed = true;
                            break;
                        }
                    }
                }
            }
            if failed {
                continue;
            }
            let expired = effect
                .duration
                .is_some_and(|duration| effect.elapsed >= duration);
            self.active_effects.insert(id, effect);
            if expired {
                self.dispel_effect(id, alloc);
            }
        }
        errors
    }

    fn accumulate_effect_value(
        &mut self,
        effect: &ActiveEffect,
        seconds: f64,
    ) -> Result<(), MagicError> {
        let (name, _) = effect.changes.first().ok_or(MagicError::Number)?;
        let rate = match self.game_setting("fMagicWardPowerMaxBase") {
            Some(Value::Float(rate)) if rate.is_finite() && rate >= 0.0 => f64::from(rate),
            Some(_) => return Err(MagicError::Number),
            None => return Err(MagicError::Unsupported("ward power setting")),
        };
        let actor = self.actor(effect.target).ok_or(MagicError::Target)?;
        let current = f64::from(actor.values.get(name).current());
        let next = (current + rate * seconds).clamp(0.0, f64::from(effect.magnitude));
        let actor = self.actor(effect.target).ok_or(MagicError::Target)?;
        actor
            .values
            .force(name, next as f32)
            .map_err(|_| MagicError::Number)
    }

    fn tick_cloak(
        &mut self,
        id: EffectId,
        seconds: f64,
        alloc: Alloc<'_>,
    ) -> Result<(), MagicError> {
        let Some(effect) = self.active_effects.get(&id).cloned() else {
            return Ok(());
        };
        let base = self.magic_effect(effect.effect)?;
        let associated = self
            .resolve_from(effect.effect, base.data.associated_item)
            .ok_or(MagicError::Missing(effect.effect))?;
        let radius = effect.magnitude;
        let dt = effect.duration.map_or(seconds, |duration| {
            seconds.min((duration - effect.elapsed).max(0.0))
        });
        let state = self.cloak_effects.get_mut(&id).ok_or(MagicError::Target)?;
        state.elapsed = (state.elapsed + dt).min(f64::MAX);
        while self
            .cloak_effects
            .get(&id)
            .is_some_and(|state| state.elapsed >= CLOAK_CADENCE)
        {
            let old_owned = {
                let state = self.cloak_effects.get_mut(&id).ok_or(MagicError::Target)?;
                state.elapsed -= CLOAK_CADENCE;
                std::mem::take(&mut state.owned)
            };
            for child in old_owned {
                self.dispel_effect(child, alloc);
            }
            let targets = self.area_targets(effect.target, radius);
            let mut owned = Vec::new();
            for target in targets {
                owned.extend(self.cast_spell_inner(
                    associated,
                    target,
                    effect.caster,
                    Some(target),
                    true,
                    alloc,
                )?);
            }
            if let Some(state) = self.cloak_effects.get_mut(&id) {
                state.owned = owned;
            }
        }
        if let Some(active) = self.active_effects.get_mut(&id) {
            active.elapsed = (active.elapsed + dt).min(f64::MAX);
        }
        Ok(())
    }

    fn spell_effect_conditions_hold(&mut self, effect: &ActiveEffect) -> Result<bool, MagicError> {
        let definition = self.spell(effect.spell)?;
        let entry = definition
            .effects
            .get(effect.effect_index as usize)
            .ok_or(MagicError::Unsupported("active effect entry"))?;
        let base = self.magic_effect(effect.effect)?;
        let context = |owner| Context {
            owner,
            subject: effect.target,
            target: effect.caster,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
            alchemy_making_poison: None,
            alchemy_effect: None,
            magic_item: None,
            dual_cast: None,
            skill_use: None,
            light_level: None,
        };
        Ok(
            conditions::evaluate_without_vm(self, context(effect.spell), &entry.conditions)?
                && conditions::evaluate_without_vm(self, context(effect.effect), &base.conditions)?,
        )
    }

    fn apply_effect_values(
        &mut self,
        effect: &ActiveEffect,
        scale: f64,
        alloc: Alloc<'_>,
    ) -> Result<(), MagicError> {
        let actor = self.actor(effect.target).ok_or(MagicError::Target)?;
        let (protected, essential) = (actor.protected, actor.essential);
        let before = actor.values.get("health").current();
        let mut values = actor.values.clone();
        if self.is_peak_value_effect(effect.effect)? {
            self.change_peak_value(&mut values, effect, scale)?;
        } else {
            change_values(&mut values, effect, scale)?;
        }
        let touches_health = effect.changes.iter().any(|(name, _)| name == "health");
        // The caster stands in for melee_hit's attacker (combat.rs,
        // CMB-13/CMB-34): a merely protected, non-essential actor still
        // dies to the player's own magic, but a caster-less or NPC-cast
        // effect clamps like any other source. World::change_actor_value
        // (actors.rs, CMB-35) has no caster to check at all, so it never
        // grants this exception.
        let player_exception =
            protected && !essential && self.player.is_some() && effect.caster == self.player;
        let bleedout_target = touches_health
            && (protected || essential)
            && !player_exception
            && values.get("health").current() < before;
        let (clamped, entering_bleedout) = self.clamp_health_at_bleedout(
            effect.target,
            bleedout_target,
            values.get("health").current(),
        );
        if entering_bleedout {
            // `Values::restore` alone only raises the `damage` component, so
            // it silently no-ops for a RECOVER-flagged effect, which
            // `change_values` above writes through `modify_temporary`
            // instead. The shared commit helper uses `damage` when the
            // maximum allows it and `permanent` only when the maximum fell
            // below the threshold, matching `World::change_actor_value`
            // (bleedout.rs, CMB-35 round 2).
            super::bleedout::commit_bleedout_health(&mut values, clamped)
                .map_err(|_| MagicError::Number)?;
        }
        // Gated on `entering_bleedout`, not read fresh from `values`: before
        // the fix above, a failed clamp commit could leave `current()` deeply
        // negative even though the actor was already flagged as entering
        // bleedout, and this reading alone would then call `kill_actor`
        // right below, killing a protected or essential actor the clamp was
        // supposed to spare. A successful bleedout entry must never be a
        // death, no matter what `current()` reads afterward.
        let dies = touches_health && !entering_bleedout && values.get("health").current() <= 0.0;
        if let Some(actor) = self.actors.get_mut(&effect.target) {
            actor.values = values;
            if entering_bleedout {
                actor.unconscious = true;
                actor.in_combat = false;
                actor.combat_target = None;
            }
        }
        if self
            .magic_effect(effect.effect)
            .is_ok_and(|definition| definition.data.archetype == 4)
        {
            if let Some(caster) = effect.caster {
                if let Some(actor) = self.actor(caster) {
                    let mut values = actor.values.clone();
                    let mut transfer = effect.clone();
                    for (_, amount) in &mut transfer.changes {
                        *amount = -*amount;
                    }
                    change_values(&mut values, &transfer, scale)?;
                    if let Some(actor) = self.actors.get_mut(&caster) {
                        actor.values = values;
                    }
                }
            }
        }
        if entering_bleedout {
            for recipient in self.actor_event_recipients(effect.target) {
                let object = self.object_of(recipient, alloc);
                self.pending.push(Pending::Event {
                    object,
                    name: "OnEnterBleedout".into(),
                    args: Vec::new(),
                });
            }
        }
        if dies {
            self.kill_actor(effect.target, effect.caster);
        }
        Ok(())
    }

    fn is_peak_value_effect(&self, effect: GlobalFormId) -> Result<bool, MagicError> {
        Ok(self.magic_effect(effect)?.data.archetype == PEAK_VALUE_MODIFIER)
    }

    fn peak_value_keyword(
        &self,
        effect: GlobalFormId,
        definition: &MagicEffect,
    ) -> Result<GlobalFormId, MagicError> {
        let keyword = self
            .resolve_from(effect, definition.data.associated_item)
            .ok_or(MagicError::Missing(effect))?;
        if self.tag_of(keyword) != Some(super::T(b"KYWD")) {
            return Err(MagicError::Unsupported("peak value keyword"));
        }
        Ok(keyword)
    }

    /// Apply only the strongest active modifier sharing this MGEF's keyword
    /// and primary actor value. The group is rebuilt from winning records and
    /// live applications, so expiration needs no additional saved state.
    fn change_peak_value(
        &self,
        values: &mut crate::actor_values::Values,
        effect: &ActiveEffect,
        scale: f64,
    ) -> Result<(), MagicError> {
        let definition = self.magic_effect(effect.effect)?;
        let keyword = self.peak_value_keyword(effect.effect, &definition)?;
        let Some((actor_value, _)) = effect.changes.first() else {
            return Err(MagicError::Unsupported("peak value actor value"));
        };
        let candidate = |active: &ActiveEffect| -> Result<Option<f32>, MagicError> {
            if active.target != effect.target {
                return Ok(None);
            }
            let active_definition = self.magic_effect(active.effect)?;
            if active_definition.data.archetype != PEAK_VALUE_MODIFIER {
                return Ok(None);
            }
            let active_keyword = self.peak_value_keyword(active.effect, &active_definition)?;
            if active_keyword != keyword {
                return Ok(None);
            }
            Ok(active
                .changes
                .iter()
                .find(|(name, _)| name == actor_value)
                .map(|(_, amount)| *amount))
        };
        let without_current = self
            .active_effects
            .values()
            .filter(|active| active.id != effect.id)
            .map(candidate)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .fold(None, |peak: Option<f32>, amount| {
                Some(peak.map_or(amount, |current| current.max(amount)))
            });
        let with_current = self
            .active_effects
            .values()
            .map(candidate)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .fold(None, |peak: Option<f32>, amount| {
                Some(peak.map_or(amount, |current| current.max(amount)))
            });
        let (before, after) = if scale < 0.0 {
            (with_current.unwrap_or(0.0), without_current.unwrap_or(0.0))
        } else {
            (without_current.unwrap_or(0.0), with_current.unwrap_or(0.0))
        };
        let delta = after - before;
        values
            .modify_temporary(actor_value, delta)
            .map_err(|_| MagicError::Number)
    }
}

fn finite_nonnegative(value: f32) -> Result<f32, MagicError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(MagicError::Number)
    }
}

pub(super) fn write_vampire_lord_state(world: &World, writer: &mut soe_papyrus::vm::state::Writer) {
    writer.len(world.vampire_lord_forms.len());
    for (effect, form) in &world.vampire_lord_forms {
        writer.u64(effect.0);
        write_form_id(writer, form.actor);
        write_opt_form_id(writer, form.previous_race);
        write_form_id(writer, form.vampire_race);
        writer.len(form.added_spells.len());
        for spell in &form.added_spells {
            write_form_id(writer, *spell);
        }
        writer.len(form.ability_effects.len());
        for ability in &form.ability_effects {
            writer.u64(ability.0);
        }
    }
}

pub(super) fn read_vampire_lord_state(
    world: &mut World,
    reader: &mut FormReader<'_, '_>,
) -> Result<(), soe_papyrus::VmError> {
    let count = reader.len()?;
    let mut forms = BTreeMap::new();
    for _ in 0..count {
        let id = EffectId(reader.u64()?);
        let actor = read_form_id(reader)?;
        let previous_race = read_opt_form_id(reader)?;
        let vampire_race = read_form_id(reader)?;
        let spell_count = reader.len()?;
        let mut added_spells = std::collections::BTreeSet::new();
        for _ in 0..spell_count {
            if !added_spells.insert(read_form_id(reader)?) {
                return Err(soe_papyrus::VmError::State(
                    "duplicate Vampire Lord ability".into(),
                ));
            }
        }
        let ability_count = reader.len()?;
        let mut ability_effects = Vec::with_capacity(ability_count);
        for _ in 0..ability_count {
            ability_effects.push(EffectId(reader.u64()?));
        }
        if id.0 == 0 {
            return Err(soe_papyrus::VmError::State(
                "invalid Vampire Lord transformation state".into(),
            ));
        }
        if forms
            .insert(
                id,
                VampireLordForm {
                    actor,
                    previous_race,
                    vampire_race,
                    added_spells,
                    ability_effects,
                },
            )
            .is_some()
        {
            return Err(soe_papyrus::VmError::State(
                "duplicate Vampire Lord transformation".into(),
            ));
        }
    }
    world.vampire_lord_forms = forms;
    Ok(())
}

pub(super) fn validate_vampire_lord_state(
    world: &World,
    actors: &std::collections::HashMap<GlobalFormId, super::ActorState>,
    effects: &BTreeMap<EffectId, ActiveEffect>,
) -> Result<(), soe_papyrus::VmError> {
    for (id, form) in &world.vampire_lord_forms {
        if effects.get(id).is_none_or(|effect| {
            effect.target != form.actor
                || !world
                    .magic_effect(effect.effect)
                    .is_ok_and(|definition| definition.data.archetype == 46)
        }) || !actors.contains_key(&form.actor)
            || world.tag_of(form.vampire_race) != Some(super::T(b"RACE"))
        {
            return Err(soe_papyrus::VmError::State(
                "invalid Vampire Lord transformation state".into(),
            ));
        }
    }
    Ok(())
}

fn change_values(
    values: &mut crate::actor_values::Values,
    effect: &ActiveEffect,
    scale: f64,
) -> Result<(), MagicError> {
    for (name, amount) in &effect.changes {
        let delta =
            (f64::from(*amount) * scale).clamp(-f64::from(f32::MAX), f64::from(f32::MAX)) as f32;
        let result = if effect.recover {
            values.modify_temporary(name, delta)
        } else if delta < 0.0 {
            values.damage(name, delta)
        } else {
            values.restore(name, delta)
        };
        result.map_err(|_| MagicError::Number)?;
    }
    Ok(())
}

fn weapon_enchantment_effects(record: &Record) -> Result<Vec<Effect>, MagicError> {
    let mut effects = Vec::new();
    let mut pending: Option<u32> = None;
    for subrecord in &record.subrecords {
        match subrecord.tag.as_bytes() {
            b"EFID" => {
                if pending.is_some() || subrecord.data.len() != 4 {
                    return Err(MagicError::Unsupported(
                        "malformed weapon enchantment effects",
                    ));
                }
                pending = Some(u32::from_le_bytes(
                    subrecord.data.as_slice().try_into().map_err(|_| {
                        MagicError::Unsupported("malformed weapon enchantment effects")
                    })?,
                ));
            }
            b"EFIT" => {
                let base_effect = pending.take().ok_or(MagicError::Unsupported(
                    "malformed weapon enchantment effects",
                ))?;
                if subrecord.data.len() != 12 {
                    return Err(MagicError::Unsupported(
                        "malformed weapon enchantment effects",
                    ));
                }
                let bytes = subrecord.data.as_slice();
                let magnitude = f32::from_le_bytes(bytes[..4].try_into().map_err(|_| {
                    MagicError::Unsupported("malformed weapon enchantment effects")
                })?);
                let area = u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| {
                    MagicError::Unsupported("malformed weapon enchantment effects")
                })?);
                let duration = u32::from_le_bytes(bytes[8..12].try_into().map_err(|_| {
                    MagicError::Unsupported("malformed weapon enchantment effects")
                })?);
                if !magnitude.is_finite() || magnitude < 0.0 {
                    return Err(MagicError::Number);
                }
                effects.push(Effect {
                    base_effect,
                    data: soe_esm::records::gameplay::EffectData {
                        magnitude,
                        area,
                        duration,
                    },
                    conditions: Vec::new(),
                });
            }
            b"CTDA" | b"CIS1" | b"CIS2" => {
                return Err(MagicError::Unsupported("weapon enchantment conditions"));
            }
            _ => {}
        }
    }
    if pending.is_some() || effects.is_empty() {
        return Err(MagicError::Unsupported(
            "malformed weapon enchantment effects",
        ));
    }
    Ok(effects)
}
