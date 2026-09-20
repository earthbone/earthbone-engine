// SPDX-License-Identifier: GPL-3.0-or-later

//! Active spell applications, following CK Magic Effect, DispelSpell and
//! DispelAllSpells contracts, xEdit SPEL/MGEF fields and CommonLib MagicSystem
//! enums. Definitions stay in the winning records. Per-cast magnitudes,
//! elapsed time and reversible actor-value deltas belong to the world.
//! Unsupported delivery and archetypes return errors before applying effects.

use soe_esm::records::gameplay::{MagicEffect, Spell};
use soe_esm::GlobalFormId;
use soe_papyrus::{ObjectId, Value};

use super::{Alloc, Handle, Pending, World};

mod state;
pub(super) use state::read_magic;

const HOSTILE: u32 = 1;
const RECOVER: u32 = 1 << 1;
const DETRIMENTAL: u32 = 1 << 2;
const NO_HIT_EVENT: u32 = 1 << 4;
const NO_DURATION: u32 = 1 << 9;
const NO_MAGNITUDE: u32 = 1 << 10;
const NO_RECAST: u32 = 1 << 17;
const NO_DEATH_DISPEL: u32 = 1 << 28;
pub(super) const DISPEL_WITH_KEYWORDS: u32 = 1 << 8;

/// Identity of one effect application, independent of its MGEF FormID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectId(pub u64);

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
    pub(crate) recover: bool,
    pub(crate) changes: Vec<(String, f32)>,
}

/// A spell was rejected before any of its effects were applied.
#[derive(Debug, thiserror::Error)]
pub enum MagicError {
    #[error("missing magic record {0:?}")]
    Missing(GlobalFormId),
    #[error("invalid magic record: {0}")]
    Record(#[from] soe_esm::Error),
    #[error("unsupported magic behavior: {0}")]
    Unsupported(&'static str),
    #[error("spell requires an actor target")]
    Target,
    #[error("invalid spell magnitude or duration")]
    Number,
    #[error("active effect identity exhausted")]
    Identity,
}

enum DispelRule {
    Timed,
    Keywords(Vec<GlobalFormId>),
}

struct PreparedEffect {
    effect: ActiveEffect,
    no_recast: bool,
    hit_event: bool,
    dispel_rules: Vec<DispelRule>,
    dispel: Vec<EffectId>,
}

impl World {
    /// All currently applied effects, in application order.
    pub fn active_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects.values()
    }

    /// The live application represented by a Papyrus ActiveMagicEffect object.
    pub fn active_effect_of(&self, object: ObjectId) -> Option<&ActiveEffect> {
        let Handle::ActiveEffect(id) = self.handle_of(object)? else {
            return None;
        };
        self.active_effects.get(&id)
    }

    /// Read the current winning SPEL definition.
    pub fn spell(&self, id: GlobalFormId) -> Result<Spell, MagicError> {
        let rref = self.order.winner_ref(id).ok_or(MagicError::Missing(id))?;
        Ok(Spell::from_record(
            self.order.record(rref),
            self.order.file(rref.file).plugin.header.is_localized(),
        )?)
    }

    /// Read the current winning MGEF definition.
    pub fn magic_effect(&self, id: GlobalFormId) -> Result<MagicEffect, MagicError> {
        let rref = self.order.winner_ref(id).ok_or(MagicError::Missing(id))?;
        Ok(MagicEffect::from_record(
            self.order.record(rref),
            self.order.file(rref.file).plugin.header.is_localized(),
        )?)
    }

    /// Execute a direct scripted cast. This bypasses player charging and cost.
    /// The caller supplies the source reference and the actor credited as caster.
    ///
    /// # Errors
    ///
    /// Returns an error before applying any effect when delivery, conditions,
    /// hit resolution or an archetype needs an execution path not yet available.
    pub fn cast_spell(
        &mut self,
        spell: GlobalFormId,
        source: GlobalFormId,
        caster: Option<GlobalFormId>,
        target: Option<GlobalFormId>,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let definition = self.spell(spell)?;
        if !matches!(definition.data.cast_type, 0 | 1) {
            return Err(MagicError::Unsupported("concentration or scroll casting"));
        }
        let target = match definition.data.delivery {
            0 => source,
            3 => target.ok_or(MagicError::Target)?,
            _ => {
                return Err(MagicError::Unsupported(
                    "projectile, contact or location delivery",
                ))
            }
        };
        if self.reference(source).is_none() || self.actor(target).is_none() {
            return Err(MagicError::Target);
        }
        let caster = caster.filter(|id| self.actor(*id).is_some());
        let mut prepared = Vec::new();
        for (index, entry) in definition.effects.iter().enumerate() {
            let effect = self
                .resolve_from(spell, entry.base_effect)
                .ok_or(MagicError::Missing(spell))?;
            let base = self.magic_effect(effect)?;
            let data = &base.data;
            if !entry.conditions.is_empty() || !base.conditions.is_empty() {
                return Err(MagicError::Unsupported("magic condition evaluation"));
            }
            if data.flags & HOSTILE != 0 {
                let actor = self.actor(target).ok_or(MagicError::Target)?;
                if actor.dead || actor.ghost {
                    return Err(MagicError::Target);
                }
            }
            if !matches!(data.archetype, 0 | 1 | 2 | 5) {
                return Err(MagicError::Unsupported("effect archetype"));
            }
            if entry.data.area != 0 || data.area != 0 || data.taper_duration != 0.0 {
                return Err(MagicError::Unsupported("area or taper"));
            }
            if data.perk != 0 || data.equip_ability != 0 {
                return Err(MagicError::Unsupported("perk or equip ability"));
            }
            if data.cast_type != definition.data.cast_type
                || data.delivery != definition.data.delivery
            {
                return Err(MagicError::Unsupported("mixed casting types or delivery"));
            }
            let magnitude = if data.flags & NO_MAGNITUDE != 0 {
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
            if matches!(data.archetype, 0 | 5) {
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
            let duration = if definition.data.cast_type == 0 {
                None
            } else {
                Some(if data.flags & NO_DURATION != 0 {
                    0.0
                } else {
                    f64::from(entry.data.duration)
                })
            };
            let mut dispel_rules = Vec::new();
            if data.archetype == 2 {
                dispel_rules.push(DispelRule::Timed);
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
                    spell,
                    effect,
                    effect_index: index as u32,
                    source,
                    caster,
                    target,
                    magnitude,
                    duration,
                    elapsed: 0.0,
                    recover: data.flags & RECOVER != 0,
                    changes,
                },
                no_recast: data.flags & NO_RECAST != 0,
                hit_event: data.flags & (HOSTILE | NO_HIT_EVENT) == HOSTILE,
                dispel_rules,
                dispel: Vec::new(),
            });
        }
        let prepared = self.prepare_effect_actions(prepared, target)?;
        let mut applied = Vec::new();
        for prepared in prepared {
            if prepared.hit_event {
                let aggressor = self.form_value(caster.or(Some(source)), alloc);
                let spell_value = self.form_value(Some(spell), alloc);
                let args = vec![
                    aggressor,
                    spell_value,
                    Value::None,
                    Value::Bool(false),
                    Value::Bool(false),
                    Value::Bool(false),
                    Value::Bool(false),
                ];
                for recipient in self.actor_event_recipients(target) {
                    let object = self.object_of(recipient, alloc);
                    self.pending.push(Pending::Event {
                        object,
                        name: "OnHit".into(),
                        args: args.clone(),
                    });
                }
            }
            let id = prepared.effect.id;
            self.next_effect += 1;
            let object = self.object_of(Handle::ActiveEffect(id), alloc);
            self.pending.push(Pending::BindEffect {
                object,
                effect: prepared.effect.effect,
            });
            let target_value = self.form_value(Some(target), alloc);
            let caster_value = self.form_value(caster, alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnEffectStart".into(),
                args: vec![target_value, caster_value],
            });
            let source_value = self.form_value(Some(source), alloc);
            let effect_value = self.form_value(Some(prepared.effect.effect), alloc);
            self.form_event(
                target,
                "OnMagicEffectApply",
                vec![source_value, effect_value],
                alloc,
            );
            // An instant lethal effect must receive its target's death events too.
            self.active_effects.insert(id, prepared.effect.clone());
            for victim in prepared.dispel {
                self.dispel_effect(victim, alloc);
            }
            if prepared.effect.recover || prepared.effect.duration == Some(0.0) {
                self.apply_effect_values(&prepared.effect, 1.0)?;
            }
            applied.push(id);
        }
        Ok(applied)
    }

    /// Validate authored action order before events or live actor values change.
    fn prepare_effect_actions(
        &mut self,
        prepared: Vec<PreparedEffect>,
        target: GlobalFormId,
    ) -> Result<Vec<PreparedEffect>, MagicError> {
        let mut live: Vec<_> = self
            .active_effects
            .values()
            .filter(|effect| effect.target == target)
            .cloned()
            .collect();
        let mut values = self.actor(target).ok_or(MagicError::Target)?.values.clone();
        let mut next = self.next_effect;
        let mut plan = Vec::new();
        for mut prepared in prepared {
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
                    DispelRule::Keywords(keywords) => {
                        let spells: std::collections::BTreeSet<_> =
                            live.iter().map(|effect| effect.spell).collect();
                        for spell in spells {
                            if self.spell_matches_dispel_keywords(spell, keywords)? {
                                selected.extend(
                                    live.iter()
                                        .filter(|effect| effect.spell == spell)
                                        .map(|effect| effect.id),
                                );
                            }
                        }
                    }
                }
            }
            for effect in &live {
                if selected.contains(&effect.id) && effect.recover {
                    change_values(&mut values, effect, -1.0)?;
                }
            }
            live.retain(|effect| !selected.contains(&effect.id));
            prepared.dispel = selected.into_iter().collect();
            if prepared.effect.recover || prepared.effect.duration == Some(0.0) {
                change_values(&mut values, &prepared.effect, 1.0)?;
            }
            live.push(prepared.effect.clone());
            plan.push(prepared);
        }
        Ok(plan)
    }

    fn spell_matches_dispel_keywords(
        &self,
        spell: GlobalFormId,
        keywords: &[GlobalFormId],
    ) -> Result<bool, MagicError> {
        if keywords.is_empty() {
            return Ok(false);
        }
        for entry in self.spell(spell)?.effects {
            let effect = self
                .resolve_from(spell, entry.base_effect)
                .ok_or(MagicError::Missing(spell))?;
            self.magic_effect(effect)?;
            if self
                .keywords(effect)
                .iter()
                .any(|keyword| keywords.contains(keyword))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Match active MGEF identity without changing known or equipped spells.
    pub fn has_magic_effect(&self, target: GlobalFormId, effect: GlobalFormId) -> bool {
        self.active_effects
            .values()
            .any(|e| e.target == target && e.effect == effect)
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
        effects.into_iter().any(|id| self.has_keyword(id, keyword))
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

    /// End one application and reverse only its temporary value changes.
    pub fn dispel_effect(&mut self, id: EffectId, alloc: Alloc<'_>) -> bool {
        let Some(effect) = self.active_effects.get(&id).cloned() else {
            return false;
        };
        if effect.recover && self.apply_effect_values(&effect, -1.0).is_err() {
            return false;
        }
        self.active_effects.remove(&id);
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
            let dt = effect.duration.map_or(seconds, |duration| {
                seconds.min((duration - effect.elapsed).max(0.0))
            });
            if !effect.recover && effect.duration != Some(0.0) {
                if let Err(error) = self.apply_effect_values(&effect, dt) {
                    errors.push(error);
                    continue;
                }
            }
            effect.elapsed = (effect.elapsed + dt).min(f64::MAX);
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

    fn apply_effect_values(&mut self, effect: &ActiveEffect, scale: f64) -> Result<(), MagicError> {
        let actor = self.actor(effect.target).ok_or(MagicError::Target)?;
        let mut values = actor.values.clone();
        change_values(&mut values, effect, scale)?;
        let dies = effect.changes.iter().any(|(name, _)| name == "health")
            && values.get("health").current() <= 0.0;
        actor.values = values;
        if dies {
            self.kill_actor(effect.target, effect.caster);
        }
        Ok(())
    }
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

/// Actor-value indices from CommonLib ActorValues.h and the xEdit AV enum.
/// Names use Skyrim's Papyrus spelling, including Marksman and Speechcraft.
fn actor_value_name(index: i32) -> Option<&'static str> {
    const NAMES: [&str; 46] = [
        "aggression",
        "confidence",
        "energy",
        "morality",
        "mood",
        "assistance",
        "onehanded",
        "twohanded",
        "marksman",
        "block",
        "smithing",
        "heavyarmor",
        "lightarmor",
        "pickpocket",
        "lockpicking",
        "sneak",
        "alchemy",
        "speechcraft",
        "alteration",
        "conjuration",
        "destruction",
        "illusion",
        "restoration",
        "enchanting",
        "health",
        "magicka",
        "stamina",
        "healrate",
        "magickarate",
        "staminarate",
        "speedmult",
        "inventoryweight",
        "carryweight",
        "critchance",
        "meleedamage",
        "unarmeddamage",
        "mass",
        "voicepoints",
        "voicerate",
        "damageresist",
        "poisonresist",
        "resistfire",
        "resistshock",
        "resistfrost",
        "resistmagic",
        "resistdisease",
    ];
    usize::try_from(index)
        .ok()
        .and_then(|i| NAMES.get(i))
        .copied()
}
