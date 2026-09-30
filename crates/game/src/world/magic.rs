// SPDX-License-Identifier: GPL-3.0-or-later

//! Active spell applications, following CK Magic Effect, DispelSpell and
//! DispelAllSpells contracts, xEdit SPEL/MGEF fields and CommonLib MagicSystem
//! enums. Invisibility follows xEdit's MGEF archetype table and UESP's MGEF
//! layout. Lock effects update eligible door and container references through
//! the shared lock state. Definitions stay in the winning records. Per-cast
//! magnitudes, elapsed time and reversible actor-value deltas belong to the world.
//! Cure Disease removes active MGEFs marked with MagicDisease. Disarm drops
//! one eligible hand weapon as a loose reference. Unsupported delivery and
//! archetypes return errors before applying effects. Aimed delivery follows
//! the SPIT field in xEdit and applies only through an actor target supplied
//! by a projectile contact. Target-location delivery uses the selected point
//! as the center for authored area effects.

use soe_esm::records::gameplay::{Effect, Ingestible, MagicEffect, Spell};
use soe_esm::{GlobalFormId, Record, Tag};
use soe_papyrus::{ObjectId, Value};

use crate::actor_values::actor_value_name;
use crate::combat::Hand;
use crate::inventory::{Error as InventoryError, Selection};

use crate::conditions::{self, Context};

use super::{Alloc, Handle, Pending, World};

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
const WEAPON_BOUND: u32 = 1 << 13;
const ETHEREALIZE: u32 = 41;
pub(super) const DISPEL_WITH_KEYWORDS: u32 = 1 << 8;
const INGESTIBLE_POISON: u32 = 1 << 17;
const INVISIBILITY: u32 = 11;
const LIGHT: u32 = 12;
const LOCK: u32 = 15;
const OPEN: u32 = 16;
const MAX_RESISTANCE: f32 = 85.0;
const RESIST_MAGIC: i32 = 44;
const FIRST_SPECIFIC_RESISTANCE: i32 = 40;
const LAST_SPECIFIC_RESISTANCE: i32 = 45;
const IGNORE_RESISTANCE: u32 = 1 << 20;

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

/// One concentration spell held by its caster until release or exhaustion.
#[derive(Debug, Clone, PartialEq)]
pub struct ConcentrationCast {
    spell: GlobalFormId,
    source: GlobalFormId,
    caster: GlobalFormId,
    target: Option<GlobalFormId>,
    cost_per_second: f64,
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
}

#[derive(Clone)]
enum DispelRule {
    Timed,
    Disease,
    Keywords(Vec<GlobalFormId>),
}

struct PreparedEffect {
    effect: ActiveEffect,
    summon_base: Option<GlobalFormId>,
    light_base: Option<GlobalFormId>,
    bound_weapon: Option<GlobalFormId>,
    lock_level: Option<i32>,
    open_lock: bool,
    disarm: bool,
    no_recast: bool,
    hit_event: bool,
    dispel_rules: Vec<DispelRule>,
    dispel: Vec<EffectId>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CastDelivery {
    Direct,
    ProjectileContact,
    Location,
    Scroll,
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

    /// Whether an actor currently has a live Etherealize application.
    /// The status belongs to the active effect identity, so ending one
    /// application cannot clear another or an actor's independent ghost flag.
    pub fn is_ethereal(&self, actor: GlobalFormId) -> bool {
        self.active_effects.values().any(|effect| {
            effect.target == actor
                && self
                    .magic_effect(effect.effect)
                    .is_ok_and(|definition| definition.data.archetype == ETHEREALIZE)
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
        let rref = self.order.winner_ref(id).ok_or(MagicError::Missing(id))?;
        Ok(MagicEffect::from_record(
            self.order.record(rref),
            self.order.file(rref.file).plugin.header.is_localized(),
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
            Some(tag) if tag == *b"ENCH" => self.enchantment_effects(item)?,
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
            if !conditions_evaluated
                && (!entry.conditions.is_empty() || !base.conditions.is_empty())
            {
                return Err(MagicError::Unsupported("magic condition evaluation"));
            }
            if data.flags & HOSTILE != 0 {
                if caster.is_some_and(|actor| self.is_ethereal(actor)) {
                    return Err(MagicError::Unsupported("ethereal action"));
                }
                let actor = self.actor(target).ok_or(MagicError::Target)?;
                if actor.dead || actor.ghost {
                    return Err(MagicError::Target);
                }
            }
            if !matches!(data.archetype, 0 | 1 | 2 | 3 | 5) {
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
            let magnitude = self.resisted_magnitude(target, data, magnitude)?;
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
            if data.archetype == 3 {
                dispel_rules.push(DispelRule::Disease);
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
                light_base: None,
                bound_weapon: None,
                lock_level: None,
                open_lock: false,
                disarm: false,
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
        let prepared = self.prepare_spell(
            spell,
            source,
            caster,
            target,
            CastDelivery::Direct,
            None,
            true,
        )?;
        if let Some(caster) = caster {
            self.break_invisibility(caster, alloc);
        }
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
        self.concentration_cast.take().is_some()
    }

    /// Whether the caster currently has a held concentration spell.
    pub fn is_concentration_casting(&self, caster: GlobalFormId) -> bool {
        self.concentration_cast
            .as_ref()
            .is_some_and(|cast| cast.caster == caster)
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
                return errors;
            }
            let resource_cost = (cast.cost_per_second * slice) as f32;
            if resource_cost > 0.0 {
                if let Some(actor) = self.actor(cast.caster) {
                    if actor.values.damage("magicka", resource_cost).is_err() {
                        errors.push(MagicError::Number);
                        self.concentration_cast = None;
                        return errors;
                    }
                } else {
                    errors.push(MagicError::Target);
                    self.concentration_cast = None;
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
        for spell in spells {
            self.apply_actor_ability(actor, spell);
        }
    }

    /// Add a known spell and activate it when it is a constant-effect ability.
    pub fn add_actor_spell(&mut self, actor: GlobalFormId, spell: GlobalFormId) -> bool {
        let Some(state) = self.actor(actor) else {
            return false;
        };
        if !state.spells.insert(spell) {
            return false;
        }
        self.apply_actor_ability(actor, spell);
        true
    }

    fn apply_actor_ability(&mut self, actor: GlobalFormId, spell: GlobalFormId) {
        if self
            .active_effects
            .values()
            .any(|effect| effect.target == actor && effect.source == actor && effect.spell == spell)
        {
            return;
        }
        let Ok(definition) = self.spell(spell) else {
            return;
        };
        if definition.data.spell_type != 4
            || definition.data.cast_type != 0
            || definition.data.delivery != 0
        {
            return;
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
            return;
        };
        let Ok(prepared) = self.prepare_effect_actions(prepared) else {
            return;
        };
        for prepared in prepared {
            let id = prepared.effect.id;
            self.next_effect = id.0 + 1;
            if prepared.effect.recover || prepared.effect.duration == Some(0.0) {
                if let Some(state) = self.actors.get_mut(&prepared.effect.target) {
                    if change_values(&mut state.values, &prepared.effect, 1.0).is_err() {
                        continue;
                    }
                }
            }
            self.active_effects.insert(id, prepared.effect.clone());
        }
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
        let concentration = cast_delivery == CastDelivery::Direct && definition.data.cast_type == 2;
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
                if !distance.is_finite() || distance > definition.data.range {
                    return Err(MagicError::Target);
                }
                if !line_of_sight_clear {
                    return Err(MagicError::Target);
                }
                source
            }
            2 if cast_delivery == CastDelivery::ProjectileContact => {
                target.ok_or(MagicError::Target)?
            }
            2 => {
                return Err(MagicError::Unsupported(
                    "aimed cast requires projectile contact",
                ));
            }
            _ => return Err(MagicError::Unsupported("location delivery")),
        };
        if self.reference(source).is_none() || self.reference(target).is_none() {
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
                if caster.is_some_and(|actor| self.is_ethereal(actor)) {
                    return Err(MagicError::Unsupported("ethereal action"));
                }
                let actor = self.actor(target).ok_or(MagicError::Target)?;
                if actor.dead || actor.ghost {
                    return Err(MagicError::Target);
                }
            }
            if !matches!(
                data.archetype,
                0 | 1 | 2 | 3 | 4 | 5 | 9 | 11 | 12 | LOCK | OPEN | 17 | 18 | 39 | ETHEREALIZE
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
            if data.taper_duration != 0.0 {
                return Err(MagicError::Unsupported("taper"));
            }
            if data.perk != 0 || data.equip_ability != 0 {
                return Err(MagicError::Unsupported("perk or equip ability"));
            }
            let cast_type_matches = if scroll {
                data.cast_type == 1
            } else {
                data.cast_type == definition.data.cast_type
            };
            if !cast_type_matches || data.delivery != definition.data.delivery {
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
            let light_base = if data.archetype == LIGHT {
                let base = self
                    .resolve_from(effect, data.associated_item)
                    .filter(|item| self.tag_of(*item) == Some(super::T(b"LIGH")))
                    .ok_or(MagicError::Unsupported("light record"))?;
                Some(base)
            } else {
                None
            };
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
            if data.archetype == 39 {
                let enchantment = self
                    .resolve_from(effect, data.associated_item)
                    .ok_or(MagicError::Missing(effect))?;
                if self.tag_of(enchantment) != Some(super::T(b"ENCH")) {
                    return Err(MagicError::Unsupported("enhance weapon association"));
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
            if data.archetype == 3 {
                dispel_rules.push(DispelRule::Disease);
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
            let mut recipients = if matches!(data.archetype, LOCK | OPEN) {
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
                        && !self.is_ethereal(id)
                });
            }
            for recipient in recipients {
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
                let mut changes = Vec::new();
                if matches!(data.archetype, 0 | 4 | 5) {
                    let name = actor_value_name(data.primary_actor_value)
                        .ok_or(MagicError::Unsupported("actor value"))?;
                    let amount = if data.archetype == 4 {
                        -magnitude
                    } else {
                        signed
                    };
                    changes.push((name.to_string(), amount));
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
                    light_base,
                    bound_weapon,
                    lock_level,
                    open_lock: data.archetype == OPEN,
                    disarm,
                    hit_event: data.flags & (HOSTILE | NO_HIT_EVENT) == HOSTILE,
                    dispel_rules: dispel_rules.clone(),
                    dispel: Vec::new(),
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

    fn apply_prepared_actions(
        &mut self,
        prepared: Vec<PreparedEffect>,
        alloc: Alloc<'_>,
        disarms: Vec<(GlobalFormId, f32)>,
    ) -> Result<Vec<EffectId>, MagicError> {
        let mut applied = Vec::new();
        for mut prepared in prepared {
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
            if let Some(base) = prepared.summon_base {
                let summon = self.place_at(base, prepared.effect.source, false);
                let owner = prepared.effect.caster.unwrap_or(prepared.effect.target);
                if let Some(reference) = self.reference(summon) {
                    reference.owner = Some(owner);
                }
                prepared.effect.summoned = Some(summon);
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
        if effect.recover && self.apply_effect_values(&effect, -1.0, alloc).is_err() {
            return false;
        }
        if let Some(summon) = effect.summoned {
            if let Some(reference) = self.reference(summon) {
                reference.deleted = true;
                reference.enabled = false;
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
        let mut applicable = effect.clone();
        if self.is_ethereal(effect.target) {
            applicable
                .changes
                .retain(|(name, amount)| name != "health" || f64::from(*amount) * scale >= 0.0);
        }
        change_values(&mut values, &applicable, scale)?;
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
}

fn finite_nonnegative(value: f32) -> Result<f32, MagicError> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(MagicError::Number)
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
