// SPDX-License-Identifier: GPL-3.0-or-later

//! Ordinary melee damage uses winning records and existing actor/inventory state.
//! GMST interpolation and armor formulas follow the sources in docs/combat.md.
//! Unsupported modifiers reject the hit before health or events change.

use std::collections::BTreeSet;

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::Value;

use super::{Alloc, Pending, World};
use crate::combat::{
    records::{Armor, Weapon},
    Difficulty, Error, HitResult, MeleeHit,
};

const UNARMED_ATTACK: u32 = 0x1F4;

const SUPPORTED_ARMOR_SLOTS: u32 = (1 << 14) - 1;

impl World {
    /// Return the selected difficulty tier.
    #[must_use]
    pub const fn difficulty(&self) -> Difficulty {
        self.difficulty
    }

    /// Select the difficulty tier used for player melee contacts.
    pub fn set_difficulty(&mut self, difficulty: Difficulty) {
        self.difficulty = difficulty;
    }

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

    /// Set whether an actor currently holds a block stance, matching the
    /// host's animation graph. Ordinary melee contacts read this flag to
    /// decide whether shield block reduction applies. It is not part of
    /// saved state, see `ActorState::blocking`.
    ///
    /// # Errors
    /// Missing actor.
    pub fn set_blocking(&mut self, actor: GlobalFormId, blocking: bool) -> Result<(), Error> {
        self.actors
            .get_mut(&actor)
            .ok_or(Error::Actor(actor))?
            .blocking = blocking;
        Ok(())
    }

    fn equip_armor(&self, item: GlobalFormId) -> Result<Option<Armor>, Error> {
        let Some(_) = self
            .record(item)
            .filter(|record| record.tag == Tag(*b"ARMO"))
        else {
            return Ok(None);
        };
        let armor = self.combat_armor_record(item)?;
        if armor.kind.is_some_and(|kind| kind > 2) {
            return Err(Error::Unsupported("unknown armor skill"));
        }
        Ok(Some(armor))
    }

    /// Execute one ordinary melee contact after host collision detection.
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
        let (record_protected, record_essential) = (target.protected, target.essential);
        // `ActorBase.SetProtected` and `ActorBase.SetEssential`
        // (crates/game/src/natives.rs) write the base's own cache, and
        // `World::actor_base` lazily derives it from the winning record the
        // same way `World::actor` derives `ActorState`. A target already
        // built before either native ran still carries the value its own
        // build cached, so read the base fresh here instead of trusting
        // that cache, the same pattern the invulnerable check below already
        // uses for the same target.
        let (protected, essential) = if self
            .reference_ref(hit.target)
            .is_some_and(|reference| reference.base.is_some())
        {
            let base = self.actor_component_source(hit.target, crate::npc::template::BASE_DATA)?;
            self.actor_base(base)
                .map_or((record_protected, record_essential), |base| {
                    (base.protected, base.essential)
                })
        } else {
            (record_protected, record_essential)
        };
        // Vanilla lets the player kill a merely protected, non-essential
        // actor outright instead of only ever dropping them into bleedout
        // (docs/combat.md, CMB-34). No local CK wiki page pins the exact
        // "combat has ended" qualifier the engine's own notes flagged as
        // unverified, so the exception is keyed on the attacker alone,
        // matching every other player/NPC branch already in this function
        // (see the difficulty and skill-scale checks above). An essential
        // actor stays gated for every attacker, including the player: the
        // wiki's SetProtected/SetEssential pages state the two flags are
        // mutually exclusive, and nothing grounds a player exception for
        // essential actors the way it does for protected ones.
        let player_exception = self.player == Some(hit.attacker) && protected && !essential;
        let bleedout_target = (protected || essential) && !player_exception;
        if self
            .reference_ref(hit.target)
            .is_some_and(|reference| reference.base.is_some())
        {
            let base = self.actor_component_source(hit.target, crate::npc::template::BASE_DATA)?;
            if self.actor_base(base).is_some_and(|base| base.invulnerable) {
                return Err(Error::Unsupported("invulnerable target"));
            }
        }
        let weapon_damage = if hit.weapon.file == 0 && hit.weapon.object == UNARMED_ATTACK {
            self.unarmed_damage(hit.attacker, hit.weapon)?
        } else {
            let weapon = self.combat_weapon(hit.weapon)?;
            if !matches!(weapon.animation, 1..=6) || !matches!(weapon.skill, 6..=8 | -1) {
                return Err(Error::Unsupported("non-melee weapon or governing skill"));
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
            finite(f64::from(weapon.damage) * scale * self.combat_setting("fDamageWeaponMult")?)?
        };
        let attack_multiplier = finite(f64::from(
            self.actor_ref(hit.attacker)
                .ok_or(Error::Actor(hit.attacker))?
                .values
                .get("attackdamagemult")
                .current(),
        ))?;
        let weapon_damage = finite(f64::from(weapon_damage) * f64::from(attack_multiplier))?;
        let (armor_rating, categories) = self.combat_armor(hit.target)?;
        let reduction = (f64::from(armor_rating) * self.combat_setting("fArmorScalingFactor")?
            / 100.0
            + f64::from(categories) * self.combat_setting("fArmorBaseFactor")?)
        .min(self.combat_setting("fMaxArmorRating")? / 100.0)
        .clamp(0.0, 1.0);
        let block = self.combat_block(hit.target)?;
        let blocked = block.is_some();
        let block_reduction = block.unwrap_or(0.0);
        let (from_player, to_player) = self.difficulty.gmst_names();
        let difficulty = if self.player == Some(hit.attacker) {
            self.combat_setting(from_player)?
        } else if self.player == Some(hit.target) {
            self.combat_setting(to_player)?
        } else {
            1.0
        };
        // Armor and shield block are independent reductions on the same base
        // damage, so they stack multiplicatively rather than adding. Sources
        // do not state vanilla's exact ordering, see docs/combat.md.
        let health_damage = finite(
            f64::from(weapon_damage) * (1.0 - reduction) * (1.0 - block_reduction) * difficulty,
        )?;
        // Prepare damage before allocating events, so overflow cannot leave a partial hit.
        let mut values = self
            .actor_ref(hit.target)
            .ok_or(Error::Actor(hit.target))?
            .values
            .clone();
        values.damage("health", health_damage)?;
        // A protected or essential target's health is clamped at its bleedout
        // threshold instead of falling through it, matching the installed
        // runtime's comparison at 0x1406cf330 (docs/combat.md, CMB-13).
        // validate_combat_actor already rejects a target that is already
        // unconscious, so entry is the only transition handled here.
        let threshold = bleedout_target
            .then(|| self.actor_bleedout_threshold(hit.target))
            .transpose()?;
        let entering_bleedout = match threshold {
            Some(threshold) if values.get("health").current() <= threshold => {
                let clamp = threshold - values.get("health").current();
                values.restore("health", clamp)?;
                true
            }
            _ => false,
        };
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
            Value::Bool(blocked),
        ];
        for recipient in self.actor_event_recipients(hit.target) {
            let object = self.object_of(recipient, alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnHit".into(),
                args: args.clone(),
            });
        }
        if entering_bleedout {
            for recipient in self.actor_event_recipients(hit.target) {
                let object = self.object_of(recipient, alloc);
                self.pending.push(Pending::Event {
                    object,
                    name: "OnEnterBleedout".into(),
                    args: Vec::new(),
                });
            }
        }
        if let Some(target) = self.actors.get_mut(&hit.target) {
            target.values = values;
            if entering_bleedout {
                target.unconscious = true;
                target.in_combat = false;
                target.combat_target = None;
            }
        }
        let killed = !bleedout_target && lethal && self.kill_actor(hit.target, Some(hit.attacker));
        Ok(HitResult {
            weapon_damage,
            armor_rating,
            armor_reduction: reduction as f32,
            blocked,
            block_reduction: block_reduction as f32,
            health_damage,
            killed,
        })
    }

    fn validate_combat_actor(&mut self, id: GlobalFormId) -> Result<(), Error> {
        let actor = self.actor(id).ok_or(Error::Actor(id))?;
        if actor.dead || actor.ghost || actor.unconscious {
            return Err(Error::Actor(id));
        }
        if !actor.perks.is_empty() {
            return Err(Error::Unsupported("perk entry points"));
        }
        for name in [
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

    fn unarmed_damage(&self, attacker: GlobalFormId, source: GlobalFormId) -> Result<f32, Error> {
        // The base-game attack form chooses race damage, but remains a live WEAP
        // record. This keeps deleted, malformed, and modifier-bearing overrides
        // from becoming a bypass around ordinary combat record validation.
        let weapon = Weapon::from_record(self.record(source).ok_or(Error::Equipment)?)?;
        if weapon.template.is_some() || weapon.enchantment.is_some() {
            return Err(Error::Unsupported("weapon template or enchantment"));
        }
        if weapon.flags & 1 != 0 || weapon.flags2 & 0x1000 != 0 || weapon.resist != -1 {
            return Err(Error::Unsupported("weapon resistance or non-hostile flags"));
        }
        let attacker = self.actor_ref(attacker).ok_or(Error::Actor(attacker))?;
        let value = attacker
            .values
            .entries
            .get("unarmeddamage")
            .ok_or(Error::Unsupported("missing race unarmed damage"))?
            .current();
        finite(f64::from(value))
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
            if self
                .record(item)
                .is_none_or(|record| record.tag != Tag(*b"ARMO"))
            {
                continue;
            }
            self.check_combat_stack(actor, item)?;
            let armor = self.combat_armor_record(item)?;
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

    /// Shield block reduction for a target holding a block stance, or `None`
    /// when not blocking or no equipped armor occupies the shield slot
    /// (BOD2 bit 9). Weapon-only blocking without a shield is not evaluated.
    /// UESP's Block page and its talk-page defaults describe the shield
    /// term, sources and open gaps are in docs/combat.md.
    fn combat_block(&self, actor: GlobalFormId) -> Result<Option<f64>, Error> {
        let state = self.actor_ref(actor).ok_or(Error::Actor(actor))?;
        if !state.blocking {
            return Ok(None);
        }
        let mut shield_rating = None;
        for &item in &state.equipped {
            if self
                .record(item)
                .is_none_or(|record| record.tag != Tag(*b"ARMO"))
            {
                continue;
            }
            let armor = self.combat_armor_record(item)?;
            if armor.slots & (1 << 9) != 0 {
                shield_rating = Some(armor.rating);
                break;
            }
        }
        let Some(shield_rating) = shield_rating else {
            return Ok(None);
        };
        let skill = f64::from(state.values.get("block").current()).max(0.0);
        let skill_term = self.combat_setting("fBlockSkillMult")? - (100.0 - skill) / 100.0;
        let reduction = (self.combat_setting("fShieldBaseFactor")?
            + self.combat_setting("fShieldScalingFactor")? / 100.0
                * f64::from(shield_rating)
                * skill_term)
            .min(self.combat_setting("fBlockMax")?)
            .clamp(0.0, 1.0);
        Ok(Some(reduction))
    }

    fn combat_weapon(&self, id: GlobalFormId) -> Result<Weapon, Error> {
        Ok(self.combat_weapon_source(id)?.1)
    }

    pub(super) fn combat_weapon_source(
        &self,
        id: GlobalFormId,
    ) -> Result<(GlobalFormId, Weapon), Error> {
        let mut current = id;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(current) || seen.len() > 128 {
                return Err(Error::Unsupported("cyclic weapon template"));
            }
            let record = self.record(current).ok_or(Error::Equipment)?;
            let (template, enchantment) = Weapon::template_links(record)?;
            if enchantment.is_some() {
                return Err(Error::Unsupported("weapon enchantment"));
            }
            let Some(template) = template else {
                return Ok((current, Weapon::from_record(record)?));
            };
            current = self
                .resolve_from(current, template)
                .ok_or(Error::Unsupported("unresolved weapon template"))?;
        }
    }

    pub(super) fn combat_armor_record(&self, id: GlobalFormId) -> Result<Armor, Error> {
        let mut current = id;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(current) || seen.len() > 128 {
                return Err(Error::Unsupported("cyclic armor template"));
            }
            let record = self
                .record(current)
                .filter(|record| record.tag == Tag(*b"ARMO"))
                .ok_or(Error::Equipment)?;
            let (template, enchantment) = Armor::template_links(record)?;
            if enchantment.is_some() {
                return Err(Error::Unsupported("armor enchantment"));
            }
            let Some(template) = template else {
                return Ok(Armor::from_record(record)?);
            };
            current = self
                .resolve_from(current, template)
                .ok_or(Error::Unsupported("unresolved armor template"))?;
        }
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
