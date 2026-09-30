// SPDX-License-Identifier: GPL-3.0-or-later

//! Active spell applications, following CK Magic Effect, DispelSpell and
//! DispelAllSpells contracts, xEdit SPEL/MGEF fields and CommonLib MagicSystem
//! enums. Definitions stay in the winning records. Per-cast magnitudes,
//! elapsed time and reversible actor-value deltas belong to the world.
//! Unsupported delivery and archetypes return errors before applying effects.

use soe_esm::records::gameplay::{Effect, Ingestible, MagicEffect, Spell};
use soe_esm::GlobalFormId;
use soe_papyrus::{ObjectId, Value};

use crate::actor_values::actor_value_name;

use crate::conditions::{self, Context};

use super::{Alloc, Handle, Pending, World, T};

mod state;
pub(super) use state::read_magic;

const HOSTILE: u32 = 1;
const RECOVER: u32 = 1 << 1;
const DETRIMENTAL: u32 = 1 << 2;
const NO_HIT_EVENT: u32 = 1 << 4;
const NO_DURATION: u32 = 1 << 9;
const NO_MAGNITUDE: u32 = 1 << 10;
const NO_AREA: u32 = 1 << 11;
const NO_RECAST: u32 = 1 << 17;
const NO_DEATH_DISPEL: u32 = 1 << 28;
pub(super) const DISPEL_WITH_KEYWORDS: u32 = 1 << 8;
const INGESTIBLE_POISON: u32 = 1 << 17;

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
    /// Runtime actor created by a Summon Creature effect, dismissed with it.
    pub summoned: Option<GlobalFormId>,
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
    #[error("magic effect condition: {0}")]
    Condition(#[from] conditions::Error),
    #[error("unsupported magic behavior: {0}")]
    Unsupported(&'static str),
    #[error("spell requires an actor target")]
    Target,
    #[error("invalid spell magnitude or duration")]
    Number,
    #[error("active effect identity exhausted")]
    Identity,
}

#[derive(Clone)]
enum DispelRule {
    Timed,
    Keywords(Vec<GlobalFormId>),
}

struct PreparedEffect {
    effect: ActiveEffect,
    summon_base: Option<GlobalFormId>,
    no_recast: bool,
    hit_event: bool,
    dispel_rules: Vec<DispelRule>,
    dispel: Vec<EffectId>,
}

impl World {
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
        let record = self.order.record(rref);
        if record.is_deleted() {
            return Err(MagicError::Missing(id));
        }
        Ok(Spell::from_record(
            record,
            self.order.file(rref.file).plugin.header.is_localized(),
        )?)
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
        if self.actor(target).is_none() {
            return Err(MagicError::Target);
        }
        let mut prepared = Vec::new();
        for (index, entry) in effects.iter().enumerate() {
            let effect = self
                .brewed_potions
                .get(&item)
                .and_then(|entries| entries.get(index))
                .map(|entry| entry.0)
                .or_else(|| self.resolve_from(item, entry.base_effect))
                .ok_or(MagicError::Missing(item))?;
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
            // An ingestible's effects carry the same dispel archetype and
            // keyword flags a cast spell's do.
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
                    spell: item,
                    effect,
                    effect_index: index as u32,
                    source: item,
                    caster,
                    target,
                    magnitude,
                    duration: Some(if data.flags & NO_DURATION != 0 {
                        0.0
                    } else {
                        f64::from(entry.data.duration)
                    }),
                    elapsed: 0.0,
                    summoned: None,
                    recover: data.flags & RECOVER != 0,
                    changes,
                },
                no_recast: data.flags & NO_RECAST != 0,
                summon_base: None,
                hit_event: false,
                dispel_rules,
                dispel: Vec::new(),
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
        let mut selected = std::collections::BTreeSet::new();
        selected.insert(anchor);
        for (&candidate, reference) in &self.refs {
            if candidate == anchor
                || !reference.is_actor
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
            let context = Context {
                owner: spell,
                subject: target,
                target: caster,
                quest: None,
                tempering_enchanted: None,
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
            if !matches!(data.archetype, 0 | 1 | 2 | 5 | 18) {
                return Err(MagicError::Unsupported("effect archetype"));
            }
            if data.taper_duration != 0.0 {
                return Err(MagicError::Unsupported("taper"));
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
            let radius = if data.flags & NO_AREA != 0 {
                0.0
            } else {
                entry.data.area as f32
            };
            let mut recipients = self.area_targets(target, radius);
            if data.flags & HOSTILE != 0 {
                recipients.retain(|&id| {
                    self.actor(id)
                        .is_some_and(|actor| !actor.dead && !actor.ghost)
                });
            }
            for recipient in recipients {
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
                        recover: data.flags & RECOVER != 0,
                        changes: changes.clone(),
                    },
                    no_recast: data.flags & NO_RECAST != 0,
                    summon_base,
                    hit_event: data.flags & (HOSTILE | NO_HIT_EVENT) == HOSTILE,
                    dispel_rules: dispel_rules.clone(),
                    dispel: Vec::new(),
                });
            }
        }
        self.apply_prepared_effects(prepared, alloc)
    }

    fn apply_prepared_effects(
        &mut self,
        prepared: Vec<PreparedEffect>,
        alloc: Alloc<'_>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let prepared = self.prepare_effect_actions(prepared)?;
        let mut applied = Vec::new();
        for mut prepared in prepared {
            if let Some(base) = prepared.summon_base {
                let summon = self.place_at(base, prepared.effect.source, false);
                let owner = prepared.effect.caster.unwrap_or(prepared.effect.target);
                if let Some(reference) = self.reference(summon) {
                    reference.owner = Some(owner);
                }
                prepared.effect.summoned = Some(summon);
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
            let id = prepared.effect.id;
            self.next_effect += 1;
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
            for victim in prepared.dispel {
                self.dispel_effect(victim, alloc);
            }
            if prepared.effect.recover || prepared.effect.duration == Some(0.0) {
                self.apply_effect_values(&prepared.effect, 1.0, alloc)?;
            }
            applied.push(id);
        }
        Ok(applied)
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
        if effect.recover && self.apply_effect_values(&effect, -1.0, alloc).is_err() {
            return false;
        }
        if let Some(summon) = effect.summoned {
            if let Some(reference) = self.reference(summon) {
                reference.deleted = true;
                reference.enabled = false;
            }
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
                if let Err(error) = self.apply_effect_values(&effect, dt, alloc) {
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
        change_values(&mut values, effect, scale)?;
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
