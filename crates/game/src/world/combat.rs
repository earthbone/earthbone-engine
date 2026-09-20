// SPDX-License-Identifier: GPL-3.0-or-later

//! Ordinary melee damage uses winning records and existing actor/inventory state.
//! GMST interpolation and armor formulas follow the sources in docs/combat.md.
//! Unsupported modifiers reject the hit before health or events change.

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::Value;

use super::{Alloc, Pending, World};
use crate::combat::{
    records::{Armor, Weapon},
    Error, HitResult, MeleeHit,
};

const SUPPORTED_ARMOR_SLOTS: u32 = (1 << 14) - 1;

impl World {
    /// Unequip an item from an actor through the same authoritative inventory state.
    ///
    /// # Errors
    /// Missing actors or references.
    pub fn unequip_item(&mut self, actor: GlobalFormId, item: GlobalFormId) -> Result<(), Error> {
        self.actor(actor).ok_or(Error::Actor(actor))?;
        let reference = self.reference(actor).ok_or(Error::Actor(actor))?;
        reference.inventory.unequip(item);
        self.actors
            .get_mut(&actor)
            .ok_or(Error::Actor(actor))?
            .equipped
            .remove(&item);
        Ok(())
    }

    /// Equip an item, replacing armor that occupies any supported biped slot.
    ///
    /// # Errors
    /// Missing actors, malformed armor records or unsupported armor types.
    pub fn equip_item(&mut self, actor: GlobalFormId, item: GlobalFormId) -> Result<(), Error> {
        self.actor(actor).ok_or(Error::Actor(actor))?;
        let armor = self.equip_armor(item)?;
        let conflicts = if let Some(armor) = armor {
            let equipped: Vec<_> = self
                .actor_ref(actor)
                .into_iter()
                .flat_map(|state| state.equipped.iter().copied())
                .map(|equipped| self.equip_armor(equipped).map(|armor| (equipped, armor)))
                .collect::<Result<_, _>>()?;
            equipped
                .into_iter()
                .filter_map(|(equipped, equipped_armor)| {
                    (equipped != item
                        && equipped_armor.is_some_and(|equipped_armor| {
                            armor.slots & equipped_armor.slots & SUPPORTED_ARMOR_SLOTS != 0
                        }))
                    .then_some(equipped)
                })
                .collect()
        } else {
            Vec::new()
        };
        let reference = self.reference(actor).ok_or(Error::Actor(actor))?;
        if reference.inventory.count(item) == 0 {
            reference.inventory.add(item, 1)?;
        }
        for conflict in &conflicts {
            reference.inventory.unequip(*conflict);
        }
        let state = self.actors.get_mut(&actor).ok_or(Error::Actor(actor))?;
        state
            .equipped
            .retain(|equipped| !conflicts.contains(equipped));
        state.equipped.insert(item);
        Ok(())
    }

    fn equip_armor(&self, item: GlobalFormId) -> Result<Option<Armor>, Error> {
        let Some(_) = self
            .record(item)
            .filter(|record| record.tag == Tag(*b"ARMO"))
        else {
            return Ok(None);
        };
        let armor = self.armor_for_equipment(item)?;
        if armor.kind.is_some_and(|kind| kind > 2) {
            return Err(Error::Unsupported("unknown armor skill"));
        }
        Ok(Some(armor))
    }

    /// Execute one ordinary melee contact after host collision detection.
    /// This currently evaluates the Adept difficulty path only.
    ///
    /// # Errors
    /// Invalid actors/equipment/settings, unsupported combat modifiers or overflow.
    /// Lazy record views may initialize, but rejected hits do not damage or emit events.
    pub fn melee_hit(&mut self, hit: MeleeHit, alloc: Alloc<'_>) -> Result<HitResult, Error> {
        if hit.attacker == hit.target {
            return Err(Error::Actor(hit.target));
        }
        self.validate_combat_actor(hit.attacker)?;
        self.validate_combat_actor(hit.target)?;
        let target = self.actor_ref(hit.target).ok_or(Error::Actor(hit.target))?;
        if target.protected || target.essential {
            return Err(Error::Unsupported("protected or essential combat/bleedout"));
        }
        if self
            .reference_ref(hit.target)
            .is_some_and(|reference| reference.base.is_some())
        {
            let base = self.actor_component_source(hit.target, crate::npc::template::BASE_DATA)?;
            if self.actor_base(base).is_some_and(|base| base.invulnerable) {
                return Err(Error::Unsupported("invulnerable target"));
            }
        }
        let weapon = Weapon::from_record(self.record(hit.weapon).ok_or(Error::Equipment)?)?;
        if !matches!(weapon.animation, 1..=6) || !matches!(weapon.skill, 6..=8 | -1) {
            return Err(Error::Unsupported("non-melee weapon or governing skill"));
        }
        if weapon.template.is_some() || weapon.enchantment.is_some() {
            return Err(Error::Unsupported("weapon template or enchantment"));
        }
        if weapon.flags & 1 != 0 || weapon.flags2 & 0x1000 != 0 || weapon.resist != -1 {
            return Err(Error::Unsupported("weapon resistance or non-hostile flags"));
        }
        let attacker = self
            .actor_ref(hit.attacker)
            .ok_or(Error::Actor(hit.attacker))?;
        if !attacker.equipped.contains(&hit.weapon) {
            return Err(Error::Equipment);
        }
        if self.item_count(hit.attacker, hit.weapon) <= 0 {
            return Err(Error::Equipment);
        }
        self.check_combat_stack(hit.attacker, hit.weapon)?;
        let skill = match weapon.skill {
            6 => "onehanded",
            7 => "twohanded",
            8 => "marksman",
            _ => "",
        };
        let scale = if skill.is_empty() {
            1.0
        } else {
            let (min, max) = if self.player == Some(hit.attacker) {
                ("fDamagePCSkillMin", "fDamagePCSkillMax")
            } else {
                ("fDamageSkillMin", "fDamageSkillMax")
            };
            self.combat_skill_scale(hit.attacker, skill, min, max)?
        };
        let weapon_damage =
            finite(f64::from(weapon.damage) * scale * self.combat_setting("fDamageWeaponMult")?)?;
        let (armor_rating, categories) = self.combat_armor(hit.target)?;
        let reduction = (f64::from(armor_rating) * self.combat_setting("fArmorScalingFactor")?
            / 100.0
            + f64::from(categories) * self.combat_setting("fArmorBaseFactor")?)
        .min(self.combat_setting("fMaxArmorRating")? / 100.0)
        .clamp(0.0, 1.0);
        let difficulty = if self.player == Some(hit.attacker) {
            self.combat_setting("fDiffMultHPByPCN")?
        } else if self.player == Some(hit.target) {
            self.combat_setting("fDiffMultHPToPCN")?
        } else {
            1.0
        };
        let health_damage = finite(f64::from(weapon_damage) * (1.0 - reduction) * difficulty)?;
        // Prepare damage before allocating events, so overflow cannot leave a partial hit.
        let mut values = self
            .actor_ref(hit.target)
            .ok_or(Error::Actor(hit.target))?
            .values
            .clone();
        values.damage("health", health_damage)?;
        let lethal = values.get("health").current() <= 0.0;
        let aggressor = self.form_value(Some(hit.attacker), alloc);
        let source = self.form_value(Some(hit.weapon), alloc);
        let args = vec![
            aggressor,
            source,
            Value::None,
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
            Value::Bool(false),
        ];
        for recipient in self.actor_event_recipients(hit.target) {
            let object = self.object_of(recipient, alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnHit".into(),
                args: args.clone(),
            });
        }
        if let Some(target) = self.actors.get_mut(&hit.target) {
            target.values = values;
        }
        let killed = lethal && self.kill_actor(hit.target, Some(hit.attacker));
        Ok(HitResult {
            weapon_damage,
            armor_rating,
            armor_reduction: reduction as f32,
            health_damage,
            killed,
        })
    }

    fn validate_combat_actor(&mut self, id: GlobalFormId) -> Result<(), Error> {
        let actor = self.actor(id).ok_or(Error::Actor(id))?;
        if actor.dead || actor.ghost {
            return Err(Error::Actor(id));
        }
        if !actor.perks.is_empty() {
            return Err(Error::Unsupported("perk entry points"));
        }
        for name in [
            "attackdamagemult",
            "onehandedmod",
            "twohandedmod",
            "marksmanmod",
            "onehandedpowermod",
            "twohandedpowermod",
            "marksmanpowermod",
            "lightarmormod",
            "heavyarmormod",
            "lightarmorpowermod",
            "heavyarmorpowermod",
        ] {
            if actor.values.entries.contains_key(name) {
                return Err(Error::Unsupported("combat actor-value multipliers"));
            }
        }
        Ok(())
    }

    fn check_combat_stack(&self, actor: GlobalFormId, item: GlobalFormId) -> Result<(), Error> {
        let reference = self.refs.get(&actor).ok_or(Error::Actor(actor))?;
        if reference.inventory.count(item) <= 0 {
            return Err(Error::Equipment);
        }
        if reference.inventory.stacks(item).iter().any(|stack| {
            stack
                .extras()
                .iter()
                .any(|extra| matches!(extra.kind, 37 | 62 | 155))
        }) {
            return Err(Error::Unsupported(
                "tempered, poisoned or enchanted item instances",
            ));
        }
        Ok(())
    }

    fn combat_setting(&self, name: &'static str) -> Result<f64, Error> {
        match self.game_setting(name) {
            Some(Value::Float(value)) if value.is_finite() && value >= 0.0 => Ok(f64::from(value)),
            _ => Err(Error::Setting(name)),
        }
    }

    fn combat_skill_scale(
        &self,
        actor: GlobalFormId,
        skill: &str,
        min: &'static str,
        max: &'static str,
    ) -> Result<f64, Error> {
        let value = self
            .actor_ref(actor)
            .ok_or(Error::Actor(actor))?
            .values
            .get(skill)
            .current()
            .max(0.0);
        let low = self.combat_setting(min)?;
        let high = self.combat_setting(max)?;
        let scale = low + (high - low) * f64::from(value) / 100.0;
        if !scale.is_finite() || scale < 0.0 {
            return Err(Error::Number);
        }
        Ok(scale)
    }

    fn combat_armor(&self, actor: GlobalFormId) -> Result<(f32, u32), Error> {
        let state = self.actor_ref(actor).ok_or(Error::Actor(actor))?;
        let mut rating = f64::from(state.values.get("damageresist").current());
        let mut occupied = 0_u32;
        let mut categories = 0_u32;
        for &item in &state.equipped {
            let Some(_) = self
                .record(item)
                .filter(|record| record.tag == Tag(*b"ARMO"))
            else {
                continue;
            };
            self.check_combat_stack(actor, item)?;
            let armor = self.armor_for_equipment(item)?;
            if armor.enchantment.is_some() {
                return Err(Error::Unsupported("armor enchantment"));
            }
            if armor.slots & occupied != 0 {
                return Err(Error::Unsupported("overlapping equipped armor"));
            }
            occupied |= armor.slots;
            let skill = match armor.kind {
                Some(0) => "lightarmor",
                Some(1) => "heavyarmor",
                Some(2) => continue,
                _ => return Err(Error::Unsupported("unknown armor skill")),
            };
            let max = if self.player == Some(actor) {
                "fArmorRatingPCMax"
            } else {
                "fArmorRatingMax"
            };
            rating += f64::from(armor.rating)
                * self.combat_skill_scale(actor, skill, "fArmorRatingBase", max)?;
            categories |= armor.slots & ((1 << 0) | (1 << 2) | (1 << 3) | (1 << 7) | (1 << 9));
        }
        Ok((finite(rating.max(0.0))?, categories.count_ones()))
    }

    /// Resolve physical armor fields without caching a second item definition.
    ///
    /// # Errors
    /// Missing, malformed, cyclic or non-ARMO template records.
    pub(crate) fn armor_for_equipment(&self, item: GlobalFormId) -> Result<Armor, Error> {
        let sources = Armor::sources(self.order(), item)?;
        let child = self.record(sources.child).ok_or(Error::Equipment)?;
        let inherited = self.record(sources.inherited).ok_or(Error::Equipment)?;
        Ok(Armor::from_sources(child, inherited)?)
    }
}

fn finite(value: f64) -> Result<f32, Error> {
    let value = value as f32;
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        Err(Error::Number)
    }
}
