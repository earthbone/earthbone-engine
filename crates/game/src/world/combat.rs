// SPDX-License-Identifier: GPL-3.0-or-later

//! Ordinary melee damage uses winning records and existing actor/inventory state.
//! GMST interpolation and armor formulas follow the sources in docs/combat.md.
//! Unsupported modifiers reject the hit before health or events change.

use std::collections::BTreeSet;

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::Value;

use super::{le_u32, Alloc, Pending, World, T};
use crate::combat::{
    records::{Armor, PowerAttack, Weapon},
    Difficulty, Error, FireError, Hand, HitResult, MeleeHit,
};
use crate::crime::CrimeType;
use crate::inventory::{Extra, Field, StackSource};

const UNARMED_ATTACK: u32 = 0x1F4;

const SUPPORTED_ARMOR_SLOTS: u32 = (1 << 14) - 1;

/// UESP's Combat and Damage pages give power attack stamina cost as an
/// unnamed formula, no GMST is named on either page: "power attack stamina
/// cost = (40 + weapon weight * 2) * attack cost multiplier * (1 - perk
/// effect)". Stamina-cost perk effects are not applied here. "Attack cost
/// multiplier" is the race's Attacks entry Stamina Mult field, read through
/// `PowerAttack`. See docs/combat.md CMB-21.
const POWER_ATTACK_STAMINA_BASE: f32 = 40.0;
const POWER_ATTACK_STAMINA_PER_WEIGHT: f32 = 2.0;

fn sneak_attack_setting(animation: u8) -> Option<&'static str> {
    Some(match animation {
        0 => "fCombatSneakHandMult",
        1 => "fCombatSneak1HSwordMult",
        2 => "fCombatSneak1HDaggerMult",
        3 => "fCombatSneak1HAxeMult",
        4 => "fCombatSneak1HMaceMult",
        5 => "fCombatSneak2HSwordMult",
        6 => "fCombatSneak2HAxeMult",
        _ => return None,
    })
}

/// A poisoned stack's index in `Inventory::stacks` alongside its poison
/// item, threaded from `combat_stack_poison` through `check_combat_weapon`
/// to `melee_hit`'s deferred consumption step (CMB-25).
type PoisonStack = Option<(usize, GlobalFormId)>;

struct EnchantmentCharge {
    source: StackSource,
    enchantment: GlobalFormId,
    current: f32,
}

impl World {
    /// Validate a Papyrus `Weapon.Fire` request against the shared form table.
    ///
    /// # Errors
    /// Missing or mistyped forms, malformed weapon records, or unavailable
    /// ranged projectile execution.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn fire_weapon(
        &mut self,
        weapon: GlobalFormId,
        source: GlobalFormId,
        ammo: Option<GlobalFormId>,
    ) -> Result<(), FireError> {
        if !matches!(self.tag_of(source), Some(tag) if tag == T(b"REFR") || tag == T(b"ACHR")) {
            return Err(FireError::Source);
        }
        let record = self
            .record(weapon)
            .filter(|record| record.tag == T(b"WEAP"))
            .ok_or(FireError::Weapon(weapon))?;
        Weapon::from_record(record)?;
        if let Some(ammo) = ammo {
            if self
                .record(ammo)
                .is_none_or(|record| record.tag != T(b"AMMO"))
            {
                return Err(FireError::Ammo(ammo));
            }
        }
        Err(FireError::Unsupported("ranged projectile execution"))
    }

    /// Notify listeners that the caller's combat path confirmed a critical hit.
    /// This reports an outcome, it does not calculate critical chance or
    /// damage modifiers.
    ///
    /// # Errors
    /// Missing aggressor or a weapon form that is not a winning WEAP record.
    pub fn report_critical_hit(
        &mut self,
        aggressor: GlobalFormId,
        weapon: GlobalFormId,
        sneak_hit: bool,
    ) -> Result<(), Error> {
        self.actor(aggressor).ok_or(Error::Actor(aggressor))?;
        if self
            .record(weapon)
            .is_none_or(|record| record.tag != Tag(*b"WEAP"))
        {
            return Err(Error::Unsupported("critical hit requires a WEAP form"));
        }
        let event = soe_commonlib_shim::events::CriticalHit {
            aggressor: World::event_form_id(aggressor),
            weapon: World::event_form_id(weapon),
            sneak_hit,
        };
        self.critical_hit_events.send(&event);
        Ok(())
    }

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
        if self
            .actor_ref(actor)
            .is_some_and(|state| state.prevent_equipment_removal.contains(&item))
        {
            return Ok(());
        }
        self.force_unequip_item(actor, item)
    }

    /// Unequip an item even when EquipItem marked it as protected.
    pub fn force_unequip_item(
        &mut self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Result<(), Error> {
        self.actor(actor).ok_or(Error::Actor(actor))?;
        let reference = self.reference(actor).ok_or(Error::Actor(actor))?;
        reference.inventory.unequip(item);
        let state = self.actors.get_mut(&actor).ok_or(Error::Actor(actor))?;
        state.equipped.remove(&item);
        state.prevent_equipment_removal.remove(&item);
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
        if conflicts.iter().any(|equipped| {
            self.actor_ref(actor)
                .is_some_and(|state| state.prevent_equipment_removal.contains(equipped))
        }) {
            return Ok(());
        }
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

    /// Return the actor's equipped shield, if its worn armor occupies the
    /// shield biped slot (BOD2 bit 9).
    ///
    /// # Errors
    /// Missing actor or malformed equipped armor record.
    ///
    /// # Panics
    /// This method does not panic.
    pub fn equipped_shield(&mut self, actor: GlobalFormId) -> Result<Option<GlobalFormId>, Error> {
        let equipped = self
            .actor(actor)
            .ok_or(Error::Actor(actor))?
            .equipped
            .iter()
            .copied()
            .collect::<Vec<_>>();
        for item in equipped {
            if self.item_count(actor, item) <= 0
                || self
                    .record(item)
                    .is_none_or(|record| record.tag != Tag(*b"ARMO"))
            {
                continue;
            }
            if self.combat_armor_record(item)?.slots & (1 << 9) != 0 {
                return Ok(Some(item));
            }
        }
        Ok(None)
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

    fn sneak_attack_qualifies(&self, attacker: GlobalFormId, target: GlobalFormId) -> bool {
        self.actors
            .get(&attacker)
            .is_some_and(|actor| actor.sneaking)
            && self
                .awareness(target, attacker)
                .is_none_or(|awareness| awareness.level != crate::detection::DetectionLevel::Combat)
    }

    fn sneak_attack_multiplier(&self, animation: u8) -> Result<f32, Error> {
        let name = sneak_attack_setting(animation)
            .ok_or(Error::Unsupported("sneak attack weapon type"))?;
        let multiplier = match self.game_setting(name) {
            Some(Value::Float(value)) => value,
            Some(Value::Int(value)) => value as f32,
            _ => return Err(Error::Setting(name)),
        };
        if multiplier.is_finite() && multiplier >= 0.0 {
            Ok(multiplier)
        } else {
            Err(Error::Setting(name))
        }
    }

    /// Execute one ordinary melee contact after host collision detection.
    ///
    /// # Errors
    /// Invalid actors/equipment/settings, unsupported combat modifiers or overflow.
    /// Lazy record views may initialize, but rejected hits do not damage or emit events.
    pub fn melee_hit(
        &mut self,
        hit: MeleeHit,
        outgoing_perks: Vec<crate::perks::EntryPointInvocation>,
        incoming_perks: Vec<crate::perks::EntryPointInvocation>,
        alloc: Alloc<'_>,
    ) -> Result<HitResult, Error> {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Combat);
        if hit.attacker == hit.target {
            return Err(Error::Actor(hit.target));
        }
        self.validate_combat_actor(hit.attacker)?;
        self.validate_combat_actor(hit.target)?;
        let sneak_attacked = !hit.is_bash && self.sneak_attack_qualifies(hit.attacker, hit.target);
        if self.is_ethereal(hit.attacker) {
            return Err(Error::Unsupported("ethereal action"));
        }
        if self.is_ethereal(hit.target) {
            return Err(Error::Unsupported("ethereal target"));
        }
        // Reading the target's actor base below may initialize it lazily, and
        // save_state persists that entry (CMB-36). Validate the equipped
        // weapon's quality extras first (CMB-10), a read-only pass, so a
        // malformed, nonfinite or ambiguous quality rejects with the saved
        // state untouched. Unequipped weapons, attackers without a reference
        // and unarmed contacts skip it and keep their existing errors.
        let unarmed = hit.weapon.file == 0 && hit.weapon.object == UNARMED_ATTACK;
        if !unarmed
            && self.refs.contains_key(&hit.attacker)
            && self
                .actor_ref(hit.attacker)
                .is_some_and(|attacker| attacker.equipped.contains(&hit.weapon))
        {
            self.combat_stack_quality(hit.attacker, hit.weapon)?;
            // Poison (CMB-25) only ever applies to an ordinary weapon swing,
            // never a bash: the bash branch still rejects it unconditionally
            // through `check_no_poison_or_enchant`. Reading it here too,
            // ahead of the target's lazy initialization, gives an ambiguous
            // or malformed poisoned stack the same fail-fast treatment
            // quality already gets.
            if !hit.is_bash {
                self.combat_stack_poison(hit.attacker, hit.weapon)?;
            }
        }
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
        // A bash reads the attacker's equipped shield instead of a weapon
        // record, the row summary's "shield becomes the weapon" reading.
        // Unarmed contacts have no WEAP record to carry a Stagger fraction,
        // so they never stagger. No corpus source states an unarmed default.
        let mut enhance_stamina_damage = 0.0_f32;
        let mut bash_stamina_cost = None;
        let mut poison_stack: PoisonStack = None;
        let mut weapon_enchantment = None;
        let mut enchantment_charge = None;
        let (
            weapon_damage,
            stagger_fraction,
            quality_multiplier,
            power_stamina_cost,
            resist,
            ignores_resistance,
        ) = if hit.is_bash {
            let armor = self.combat_armor_record(hit.weapon)?;
            // UESP's ARMO layout ties BIDS/BAMT to the shield slot (BOD2 bit
            // 9), so a bash source is rejected here rather than trusting the
            // caller to have picked a shield. See docs/combat.md CMB-20.
            if armor.slots & (1 << 9) == 0 {
                return Err(Error::Unsupported("bash source is not a shield"));
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
            let cost = finite(self.combat_setting("fBashStaminaCost")?)?;
            let stamina = self
                .actor_ref(hit.attacker)
                .ok_or(Error::Actor(hit.attacker))?
                .values
                .get("stamina")
                .current();
            if stamina < cost {
                return Err(Error::Unsupported("insufficient stamina for bash"));
            }
            // Deferred, not deducted here: the commit below only applies once
            // every later fallible step (armor, block, difficulty, health
            // and stagger math) has already succeeded, so a bash that fails
            // partway through never spends stamina. See docs/combat.md CMB-20.
            bash_stamina_cost = Some(cost);
            // A shield carries no WEAP DNAM Stagger fraction of its own, and
            // bashing exists to interrupt, so a bash always contributes its
            // full post-reduction damage to the stagger check. Damage reuses
            // the shield's DNAM rating through the same post-armor,
            // post-block, post-difficulty pipeline every other hit uses,
            // rather than a separate GMST or the attacker's other-hand
            // weapon. No corpus source pins a vanilla bash damage formula,
            // this is an engineering estimate, see docs/combat.md CMB-20.
            (
                finite(f64::from(armor.rating))?,
                1.0_f32,
                1.0_f32,
                None,
                -1,
                false,
            )
        } else if unarmed {
            // They also carry no inventory stack, so no quality extra can
            // apply. Unarmed power attacks are a documented gap, see
            // docs/combat.md CMB-21: the vanilla damage and stamina
            // multipliers for them conflict between UESP's own Combat and
            // Damage pages, so nothing is guessed here.
            if hit.is_power_attack {
                return Err(Error::Unsupported("unarmed power attack"));
            }
            let (damage, resist, ignores_resistance) =
                self.unarmed_damage(hit.attacker, hit.weapon)?;
            let damage = if sneak_attacked {
                finite(f64::from(damage) * f64::from(self.sneak_attack_multiplier(0)?))?
            } else {
                damage
            };
            (damage, 0.0_f32, 1.0_f32, None, resist, ignores_resistance)
        } else {
            let (weapon, base_enchantment, maximum_charge) = self.combat_weapon(hit.weapon)?;
            if !matches!(weapon.animation, 1..=6) || !matches!(weapon.skill, 6..=8 | -1) {
                return Err(Error::Unsupported("non-melee weapon or governing skill"));
            }
            if weapon.flags2 & 0x1000 != 0 {
                return Err(Error::Unsupported("weapon non-hostile flags"));
            }
            if !matches!(weapon.resist, -1 | 39..=44) {
                return Err(Error::Unsupported("unsupported weapon resistance value"));
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
            let (quality, poison) = self.check_combat_weapon(hit.attacker, hit.weapon)?;
            poison_stack = poison;
            if let Some(charge) = self.combat_stack_enchantment(
                hit.attacker,
                hit.weapon,
                base_enchantment,
                maximum_charge,
            )? {
                let cost = self.weapon_enchantment_cost(charge.enchantment)?;
                if charge.current >= cost {
                    weapon_enchantment = Some(self.prepare_weapon_enchantment(
                        charge.enchantment,
                        hit.attacker,
                        hit.target,
                    )?);
                    enchantment_charge = Some((charge.source, charge.current - cost));
                }
            }
            let quality_multiplier = quality.unwrap_or(1.0);
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
            let mut weapon_damage = finite(
                f64::from(weapon.damage) * scale * self.combat_setting("fDamageWeaponMult")?,
            )?;
            let mut power_stamina_cost = None;
            if hit.is_power_attack {
                let power_attack = self.combat_power_attack(hit.attacker)?;
                weapon_damage =
                    finite(f64::from(weapon_damage) * f64::from(power_attack.damage_mult))?;
                power_stamina_cost = Some(finite(
                    (f64::from(POWER_ATTACK_STAMINA_BASE)
                        + f64::from(weapon.weight) * f64::from(POWER_ATTACK_STAMINA_PER_WEIGHT))
                        * f64::from(power_attack.stamina_mult),
                )?);
            }
            if sneak_attacked {
                weapon_damage = finite(
                    f64::from(weapon_damage)
                        * f64::from(self.sneak_attack_multiplier(weapon.animation)?),
                )?;
            }
            let (enhance_damage, stamina_damage) =
                self.enhance_weapon_contribution(hit.attacker)?;
            enhance_stamina_damage = stamina_damage;
            weapon_damage = finite(f64::from(weapon_damage) + f64::from(enhance_damage))?;
            (
                weapon_damage,
                weapon.stagger,
                quality_multiplier,
                power_stamina_cost,
                weapon.resist,
                weapon.flags & 1 != 0,
            )
        };
        let attack_multiplier = finite(f64::from(
            self.actor_ref(hit.attacker)
                .ok_or(Error::Actor(hit.attacker))?
                .values
                .get("attackdamagemult")
                .current(),
        ))?;
        let weapon_damage = finite(f64::from(weapon_damage) * f64::from(attack_multiplier))?;
        // Tempered item quality (CMB-10) scales the same way SetItemHealthPercent's
        // "health" percentage is documented, applied after AttackDamageMult and
        // before armor/block reduction. Unarmed contacts hold quality_multiplier
        // at 1.0, see above.
        let mut weapon_damage = finite(f64::from(weapon_damage) * f64::from(quality_multiplier))?;
        if !hit.is_bash {
            let outgoing = self.combat_perk_damage(outgoing_perks)?;
            let incoming = self.combat_perk_damage(incoming_perks)?;
            weapon_damage = finite(incoming(outgoing(f64::from(weapon_damage))?)?)?;
        }
        let (armor_rating, categories) = self.combat_armor(hit.target)?;
        let reduction = (f64::from(armor_rating) * self.combat_setting("fArmorScalingFactor")?
            / 100.0
            + f64::from(categories) * self.combat_setting("fArmorBaseFactor")?)
        .min(self.combat_setting("fMaxArmorRating")? / 100.0)
        .clamp(0.0, 1.0);
        let block = self.combat_block(hit.target)?;
        let blocked = block.is_some();
        let block_reduction = block.unwrap_or(0.0);
        // The weapon's DNAM Resist field (CMB-26) names a further target
        // actor value that reduces this weapon's damage, independent of
        // armor and shield block. `melee_hit` already rejected any resist
        // index outside -1 or 39 through 44 before this point, and unarmed
        // contacts carry the unarmed attack form's own resist field the
        // same way.
        let resistance_reduction =
            self.combat_resistance(hit.target, resist, ignores_resistance)?;
        let (from_player, to_player) = self.difficulty.gmst_names();
        let difficulty = if self.player == Some(hit.attacker) {
            self.combat_setting(from_player)?
        } else if self.player == Some(hit.target) {
            self.combat_setting(to_player)?
        } else {
            1.0
        };
        // Armor, shield block and weapon resistance are independent
        // reductions on the same base damage, so they stack multiplicatively
        // rather than adding. Sources do not state vanilla's exact ordering,
        // see docs/combat.md.
        let mut health_damage = finite(
            f64::from(weapon_damage)
                * (1.0 - reduction)
                * (1.0 - block_reduction)
                * (1.0 - resistance_reduction)
                * difficulty,
        )?;
        if self.debug_god_mode && self.player == Some(hit.target) {
            health_damage = 0.0;
        }
        // Stagger magnitude scales the damage that actually lands (post
        // armor and block, matching the row's "hit magnitude") by the
        // weapon's own Stagger fraction, so a fully absorbed hit and a
        // zero-Stagger weapon both leave the target unstaggered. No local
        // or UESP source states a vanilla stagger threshold or duration,
        // see docs/combat.md CMB-14 for the estimated defaults this uses.
        let stagger_magnitude = f64::from(health_damage) * f64::from(stagger_fraction);
        // A zero Stagger fraction (unarmed, or a weapon whose DNAM carries
        // 0.0) makes stagger_magnitude 0.0 regardless of health_damage, so
        // this hit can never cross any finite positive threshold. Skip the
        // fStaggerThreshold lookup entirely in that case: a malformed value
        // for it anywhere in the load order must not block a hit that could
        // never have qualified.
        let staggered = stagger_fraction != 0.0
            && stagger_magnitude > self.combat_setting("fStaggerThreshold")?;
        // fStaggerDuration only feeds the deadline stored below once a hit
        // is already known to cross the threshold, so fetch it only then. A
        // malformed duration must not block a hit that never needed one.
        let stagger_deadline = staggered
            .then(|| self.combat_setting("fStaggerDuration"))
            .transpose()?
            .map(|duration| self.clock.real_seconds + duration);
        // Prepare damage before allocating events, so overflow cannot leave a partial hit.
        // A power attack's stamina cost is prepared here too, so a
        // malformed attacker Stamina entry cannot leave a partial hit
        // either. No floor applies, matching health_damage below, so an
        // attacker who started the swing with too little stamina still
        // pays the cost. Nothing here re-checks stamina before the swing:
        // this function only runs once the host has already confirmed the
        // power attack, see docs/combat.md CMB-21.
        let attacker_stamina = power_stamina_cost
            .map(|cost| -> Result<_, Error> {
                let mut values = self
                    .actor_ref(hit.attacker)
                    .ok_or(Error::Actor(hit.attacker))?
                    .values
                    .clone();
                values.damage("stamina", cost)?;
                Ok(values)
            })
            .transpose()?;
        let staged_inventory = if enchantment_charge.is_some() || poison_stack.is_some() {
            let mut inventory = self
                .reference(hit.attacker)
                .ok_or(Error::Actor(hit.attacker))?
                .inventory
                .clone();
            if let Some((source, charge)) = enchantment_charge {
                let selection = inventory.select(hit.weapon, source, 1)?;
                inventory.update_selected(
                    &selection,
                    vec![Extra {
                        kind: 40,
                        fields: vec![Field::FloatBits(charge.to_bits())],
                    }],
                )?;
            }
            if let Some((index, _)) = poison_stack {
                inventory.clear_stack_extra(hit.weapon, index, 62)?;
            }
            Some(inventory)
        } else {
            None
        };
        let mut values = self
            .actor_ref(hit.target)
            .ok_or(Error::Actor(hit.target))?
            .values
            .clone();
        values.damage("health", health_damage)?;
        values.damage("stamina", enhance_stamina_damage)?;
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
        if let Some(prepared) = &weapon_enchantment {
            self.validate_weapon_enchantment_after_hit(prepared, &values)?;
        }
        let lethal = values.get("health").current() <= 0.0;
        let attacked_factions = if self.player == Some(hit.attacker) {
            self.actor(hit.target)
                .map(|actor| actor.factions.keys().copied().collect::<Vec<_>>())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        if self.is_player(hit.attacker) {
            self.ai
                .metrics
                .stimulus(hit.target, crate::ai::metrics::Stimulus::PlayerAttack);
        }
        let aggressor = self.form_value(Some(hit.attacker), alloc);
        let source = self.form_value(Some(hit.weapon), alloc);
        let args = vec![
            aggressor,
            source,
            Value::None,
            Value::Bool(hit.is_power_attack),
            Value::Bool(sneak_attacked),
            Value::Bool(hit.is_bash),
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
        if let Some(deadline) = stagger_deadline.filter(|_| staggered) {
            self.begin_stagger(hit.target, stagger_magnitude, Some(hit.attacker), deadline);
        }
        // Committed last, alongside the target's own state, so a bash that
        // errors anywhere above never reaches this line.
        if let Some(cost) = bash_stamina_cost {
            if let Some(attacker) = self.actors.get_mut(&hit.attacker) {
                attacker.values.damage("stamina", -cost)?;
            }
        }
        if let Some(values) = attacker_stamina {
            if let Some(attacker) = self.actors.get_mut(&hit.attacker) {
                attacker.values = values;
            }
        }
        for faction in attacked_factions {
            if let Some(state) = self.faction(faction) {
                state.player_attacked = true;
            }
        }
        // Poison (CMB-25) applies and clears last, after every other part of
        // the hit has already committed, the same deferred placement
        // bash_stamina_cost and attacker_stamina above use. A poison error
        // here can leave the physical hit committed with the coating still
        // on the weapon, the same accepted partial-commit risk those two
        // already carry for this function.
        let mut poisoned = false;
        if let Some((_, poison_item)) = poison_stack {
            self.apply_poison(hit.attacker, poison_item, hit.target, alloc)?;
            poisoned = true;
        }
        if let Some(prepared) = weapon_enchantment {
            self.apply_weapon_enchantment(prepared, alloc)?;
        }
        if let Some(inventory) = staged_inventory {
            if let Some(reference) = self.reference(hit.attacker) {
                reference.inventory = inventory;
            }
        }
        let killed = !bleedout_target && lethal && self.kill_actor(hit.target, Some(hit.attacker));
        if !killed && self.player == Some(hit.attacker) {
            self.record_player_assault(hit.attacker, hit.target);
        }
        self.break_invisibility(hit.attacker, alloc);
        let mut event_flags = soe_commonlib_shim::events::HitFlags::default();
        if hit.is_power_attack {
            event_flags.0 |= soe_commonlib_shim::events::HitFlags::POWER_ATTACK.0;
        }
        if sneak_attacked {
            event_flags.0 |= soe_commonlib_shim::events::HitFlags::SNEAK_ATTACK.0;
        }
        if hit.is_bash {
            event_flags.0 |= soe_commonlib_shim::events::HitFlags::BASH_ATTACK.0;
        }
        if blocked {
            event_flags.0 |= soe_commonlib_shim::events::HitFlags::HIT_BLOCKED.0;
        }
        self.commonlib_hit(soe_commonlib_shim::events::TesHitEvent {
            target: soe_commonlib_shim::refr::ObjectRefHandle(self.runtime_id(hit.target)),
            cause: soe_commonlib_shim::refr::ObjectRefHandle(self.runtime_id(hit.attacker)),
            source: soe_commonlib_shim::form::FormId(self.runtime_id(hit.weapon)),
            projectile: soe_commonlib_shim::form::FormId(0),
            flags: event_flags,
        });
        Ok(HitResult {
            weapon_damage,
            armor_rating,
            armor_reduction: reduction as f32,
            blocked,
            block_reduction: block_reduction as f32,
            resistance_reduction: resistance_reduction as f32,
            health_damage,
            killed,
            staggered,
            bashed: hit.is_bash,
            poisoned,
            sneak_attacked,
        })
    }

    /// Posts a witnessed player assault to the victim's crime faction. The
    /// victim reports the contact they observed to that faction.
    fn record_player_assault(&mut self, perpetrator: GlobalFormId, witness: GlobalFormId) {
        let Some(faction) = self
            .actor_ref(witness)
            .and_then(|actor| actor.crime_faction)
        else {
            return;
        };
        // UESP FACT flags define Ignore Assault independently from Track
        // Crime. Read the winning record so runtime crime state stays in the
        // shared form database rather than another faction cache.
        let ignore_assault = self
            .record(faction)
            .filter(|record| record.tag == Tag(*b"FACT"))
            .and_then(|record| record.find(Tag(*b"DATA")))
            .and_then(le_u32)
            .is_some_and(|flags| flags & 0x100 != 0);
        if !ignore_assault {
            let _ = self.record_witnessed_crime_in_faction(
                perpetrator,
                witness,
                CrimeType::Assault,
                faction,
            );
        }
    }

    /// Damage from supported enchantments attached by active MGEF archetype 39
    /// effects. The owning actor and equipped melee weapon are both required.
    fn enhance_weapon_contribution(&self, attacker: GlobalFormId) -> Result<(f32, f32), Error> {
        let mut health_damage = 0.0_f32;
        let mut stamina_damage = 0.0_f32;
        for active in self
            .active_effects
            .values()
            .filter(|active| active.target == attacker)
        {
            let effect = self.magic_effect(active.effect)?;
            if effect.data.archetype != 39 {
                continue;
            }
            let enchantment = self
                .resolve_from(active.effect, effect.data.associated_item)
                .ok_or(Error::Equipment)?;
            let record = self.record(enchantment).ok_or(Error::Equipment)?;
            if record.tag != Tag(*b"ENCH") {
                return Err(Error::Unsupported("enhance weapon association"));
            }
            let mut magnitude = None;
            let mut actor_value = None;
            let mut pending_effect = false;
            for subrecord in &record.subrecords {
                match subrecord.tag.as_bytes() {
                    b"EFID" => {
                        if pending_effect || magnitude.is_some() {
                            return Err(Error::Unsupported("multiple enhance weapon effects"));
                        }
                        if subrecord.data.len() != 4 {
                            return Err(
                                soe_esm::Error::Invalid("invalid ENCH EFID length".into()).into()
                            );
                        }
                        let raw =
                            u32::from_le_bytes(subrecord.data[..4].try_into().map_err(|_| {
                                soe_esm::Error::Invalid("invalid ENCH EFID".into())
                            })?);
                        let base = self
                            .resolve_from(enchantment, raw)
                            .ok_or(Error::Equipment)?;
                        let base = self.magic_effect(base)?;
                        if base.data.archetype != 0
                            || !matches!(base.data.primary_actor_value, 24 | 25)
                            || base.data.flags & (1 << 2) == 0
                            || !base.conditions.is_empty()
                        {
                            return Err(Error::Unsupported("enhance weapon enchantment effect"));
                        }
                        actor_value = Some(base.data.primary_actor_value);
                        pending_effect = true;
                    }
                    b"EFIT" => {
                        if subrecord.data.len() != 12 || !pending_effect || magnitude.is_some() {
                            return Err(soe_esm::Error::Invalid("invalid ENCH EFIT".into()).into());
                        }
                        let raw =
                            f32::from_le_bytes(subrecord.data[..4].try_into().map_err(|_| {
                                soe_esm::Error::Invalid("invalid ENCH EFIT magnitude".into())
                            })?);
                        if !raw.is_finite() || raw < 0.0 {
                            return Err(Error::Number);
                        }
                        magnitude = Some(raw);
                        pending_effect = false;
                    }
                    b"CTDA" | b"CIS1" | b"CIS2" => {
                        return Err(Error::Unsupported("enhance weapon enchantment conditions"));
                    }
                    _ => {}
                }
            }
            if pending_effect {
                return Err(Error::Unsupported("incomplete weapon enchantment"));
            }
            let entry_magnitude =
                magnitude.ok_or(Error::Unsupported("empty weapon enchantment"))?;
            let contribution = entry_magnitude * active.magnitude;
            match actor_value.ok_or(Error::Unsupported("empty weapon enchantment"))? {
                24 => {
                    health_damage = finite(f64::from(health_damage) + f64::from(contribution))?;
                }
                25 => {
                    stamina_damage = finite(f64::from(stamina_damage) + f64::from(contribution))?;
                }
                _ => return Err(Error::Unsupported("enhance weapon actor value")),
            }
        }
        Ok((health_damage, stamina_damage))
    }

    /// Whether an actor is inside its stagger window right now. Lazily
    /// derived from `ActorState::stagger_end_time` against the real-time
    /// clock, the same expiry pattern `ai::wait_until` uses, so nothing
    /// needs to actively clear the field once the deadline passes.
    #[must_use]
    pub fn is_staggered(&self, id: GlobalFormId) -> bool {
        self.actor_ref(id).is_some_and(|state| {
            state
                .stagger_end_time
                .is_some_and(|until| self.clock.real_seconds < until)
        })
    }

    /// Commit one stagger application with its authored magnitude and source
    /// direction. Callers validate the duration before reaching this point.
    pub(super) fn begin_stagger(
        &mut self,
        target: GlobalFormId,
        magnitude: f64,
        direction: Option<GlobalFormId>,
        deadline: f64,
    ) {
        if let Some(actor) = self.actors.get_mut(&target) {
            actor.stagger_magnitude = magnitude;
            actor.stagger_direction = direction;
            actor.stagger_end_time = Some(deadline);
        }
    }

    fn validate_combat_actor(&mut self, id: GlobalFormId) -> Result<(), Error> {
        let actor = self.actor(id).ok_or(Error::Actor(id))?;
        if actor.dead || actor.ghost || actor.unconscious {
            return Err(Error::Actor(id));
        }
        Ok(())
    }

    fn combat_perk_damage(
        &mut self,
        invocations: Vec<crate::perks::EntryPointInvocation>,
    ) -> Result<impl Fn(f64) -> Result<f64, Error>, Error> {
        let mut operations = Vec::with_capacity(invocations.len());
        for invocation in invocations {
            let operation = match invocation.function {
                1..=3 => {
                    if invocation.data_type != Some(1) || invocation.data.len() != 4 {
                        return Err(Error::Unsupported("malformed perk damage function data"));
                    }
                    let value = f32::from_le_bytes(
                        invocation
                            .data
                            .as_slice()
                            .try_into()
                            .map_err(|_| Error::Number)?,
                    );
                    if !value.is_finite() {
                        return Err(Error::Number);
                    }
                    (invocation.function, f64::from(value))
                }
                6 | 7 => (invocation.function, 0.0),
                5 | 12..=14 => {
                    if invocation.data_type != Some(2) || invocation.data.len() != 8 {
                        return Err(Error::Unsupported("malformed perk damage function data"));
                    }
                    let raw_index = f32::from_le_bytes(
                        invocation.data[0..4]
                            .try_into()
                            .map_err(|_| Error::Number)?,
                    );
                    let coefficient = f32::from_le_bytes(
                        invocation.data[4..8]
                            .try_into()
                            .map_err(|_| Error::Number)?,
                    );
                    if !raw_index.is_finite()
                        || raw_index.fract() != 0.0
                        || !coefficient.is_finite()
                    {
                        return Err(Error::Number);
                    }
                    let index = i32::try_from(raw_index as i64).map_err(|_| Error::Number)?;
                    let name = crate::actor_values::actor_value_name(index)
                        .ok_or(Error::Unsupported("unknown perk actor value"))?;
                    let value = self.actor_value(invocation.actor, name);
                    if !value.is_finite() {
                        return Err(Error::Number);
                    }
                    (
                        invocation.function,
                        f64::from(value) * f64::from(coefficient),
                    )
                }
                _ => return Err(Error::Unsupported("perk damage function")),
            };
            operations.push(operation);
        }
        Ok(move |mut value: f64| {
            for &(function, amount) in &operations {
                value = match function {
                    1 | 12 => amount,
                    2 | 5 => value + amount,
                    3 | 13 => value * amount,
                    6 => value.abs(),
                    7 => -value.abs(),
                    14 => value * (1.0 + amount),
                    _ => value,
                };
                if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
                    return Err(Error::Number);
                }
            }
            Ok(value)
        })
    }

    /// Returns the unarmed base damage plus the unarmed attack form's own
    /// DNAM Resist index and "Ignores Normal Weapon Resistance" flag, so
    /// `melee_hit` applies weapon resistance (CMB-26) the same way for an
    /// unarmed contact as for an equipped one.
    fn unarmed_damage(
        &self,
        attacker: GlobalFormId,
        source: GlobalFormId,
    ) -> Result<(f32, i32, bool), Error> {
        // The base-game attack form chooses race damage, but remains a live WEAP
        // record. This keeps deleted, malformed, and modifier-bearing overrides
        // from becoming a bypass around ordinary combat record validation.
        let weapon = Weapon::from_record(self.record(source).ok_or(Error::Equipment)?)?;
        if weapon.template.is_some() || weapon.enchantment.is_some() {
            return Err(Error::Unsupported("weapon template or enchantment"));
        }
        if weapon.flags2 & 0x1000 != 0 {
            return Err(Error::Unsupported("weapon non-hostile flags"));
        }
        if !matches!(weapon.resist, -1 | 39..=44) {
            return Err(Error::Unsupported("unsupported weapon resistance value"));
        }
        let attacker = self.actor_ref(attacker).ok_or(Error::Actor(attacker))?;
        let value = attacker
            .values
            .entries
            .get("unarmeddamage")
            .ok_or(Error::Unsupported("missing race unarmed damage"))?
            .current();
        Ok((
            finite(f64::from(value))?,
            weapon.resist,
            weapon.flags & 1 != 0,
        ))
    }

    /// Attacker's default single-weapon power attack entry, from the
    /// winning RACE record their `ActorState::race` already resolved.
    /// `ActorState::race` only caches which record wins, the record itself
    /// is still read fresh through the shared `World` here.
    ///
    /// # Errors
    /// Missing actor, race or matching Attacks entry, or a malformed RACE record.
    fn combat_power_attack(&self, actor: GlobalFormId) -> Result<PowerAttack, Error> {
        let race = self
            .actor_ref(actor)
            .ok_or(Error::Actor(actor))?
            .race
            .ok_or(Error::Unsupported(
                "attacker has no race for power attack data",
            ))?;
        let record = self
            .record(race)
            .ok_or(Error::Unsupported("missing attacker race record"))?;
        PowerAttack::from_race(record)?
            .ok_or(Error::Unsupported("race has no default power attack entry"))
    }

    /// Rejects an equipped item that carries a poisoned or enchanted stack,
    /// regardless of item quality (kind 37, CMB-10). Shared by the weapon
    /// and armor call sites: both still reject poison and enchantment
    /// unconditionally (docs/combat.md, CMB-03), but only the weapon reads
    /// quality, so armor stacks never need `check_combat_stack` itself.
    /// A missing reference or an item count of zero or less rejects too.
    fn check_no_poison_or_enchant(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Result<(), Error> {
        let reference = self.refs.get(&actor).ok_or(Error::Actor(actor))?;
        if reference.inventory.count(item) <= 0 {
            return Err(Error::Equipment);
        }
        if reference.inventory.stacks(item).iter().any(|stack| {
            stack
                .extras()
                .iter()
                .any(|extra| matches!(extra.kind, 62 | 155))
        }) {
            return Err(Error::Unsupported("poisoned or enchanted item instances"));
        }
        Ok(())
    }

    /// Read the equipped item's tempered quality (CMB-10) after the poison and
    /// enchantment check. Quality (37, CommonLib `ExtraHealth`) is read instead
    /// of rejected: `None` when no stack of the item carries it,
    /// `Some(multiplier)` when every stack that does carry it agrees on the
    /// same finite, non-negative value.
    ///
    /// The equipped set is form-only (docs/combat.md, CMB-03: "does not infer
    /// item-instance selection from a form-only equipment set"), so a hit cannot
    /// name which physical stack the attacker swung. When the item's stacks
    /// disagree on quality, the selected stack cannot be determined, so the
    /// contact rejects instead of guessing.
    fn check_combat_stack(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Result<Option<f32>, Error> {
        self.check_no_poison_or_enchant(actor, item)?;
        self.combat_stack_quality(actor, item)
    }

    /// Read the equipped weapon's quality and poisoned stack together for an
    /// ordinary melee swing. Poison is read instead of rejected, unlike
    /// `check_combat_stack`. Enchantment extras are resolved by
    /// `combat_stack_enchantment`. See
    /// `combat_stack_quality` and `combat_stack_poison`.
    fn check_combat_weapon(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Result<(Option<f32>, PoisonStack), Error> {
        let quality = self.combat_stack_quality(actor, item)?;
        let poison = self.combat_stack_poison(actor, item)?;
        Ok((quality, poison))
    }

    fn combat_stack_enchantment(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
        base_enchantment: Option<GlobalFormId>,
        base_maximum: f32,
    ) -> Result<Option<EnchantmentCharge>, Error> {
        let inventory = &self.refs.get(&actor).ok_or(Error::Actor(actor))?.inventory;
        let mut instance = None;
        for (index, stack) in inventory.stacks(item).iter().enumerate() {
            let Some(extra) = stack.extra(155) else {
                continue;
            };
            let [Field::Form(Some(enchantment)), Field::Short(maximum)] = extra.fields.as_slice()
            else {
                return Err(Error::Unsupported("malformed weapon enchantment extra"));
            };
            if instance.is_some() || stack.count() != 1 || *maximum == 0 {
                return Err(Error::Unsupported("ambiguous enchanted weapon instances"));
            }
            instance = Some((index, *enchantment, f32::from(*maximum)));
        }
        let (source, enchantment, maximum) = if let Some((index, enchantment, maximum)) = instance {
            (StackSource::Extra(index), enchantment, maximum)
        } else if let Some(enchantment) = base_enchantment {
            let mut stacks = inventory.stacks(item).iter().enumerate();
            let source = match (inventory.count(item), stacks.next(), stacks.next()) {
                (1, Some((index, stack)), None) if stack.count() == 1 => StackSource::Extra(index),
                (1, None, None) => StackSource::Plain,
                _ => return Err(Error::Unsupported("ambiguous enchanted weapon instances")),
            };
            if base_maximum <= 0.0 || !base_maximum.is_finite() {
                return Err(Error::Unsupported(
                    "invalid weapon enchantment charge amount",
                ));
            }
            (source, enchantment, base_maximum)
        } else {
            return Ok(None);
        };
        let current = match source {
            StackSource::Plain => maximum,
            StackSource::Extra(index) => {
                let stack = inventory.stacks(item).get(index).ok_or(Error::Equipment)?;
                match stack.extra(40) {
                    None => maximum,
                    Some(extra) => match extra.fields.as_slice() {
                        [Field::FloatBits(bits)] => {
                            let charge = f32::from_bits(*bits);
                            if !charge.is_finite() || charge < 0.0 || charge > maximum {
                                return Err(Error::Unsupported("invalid weapon instance charge"));
                            }
                            charge
                        }
                        _ => return Err(Error::Unsupported("malformed weapon instance charge")),
                    },
                }
            }
        };
        Ok(Some(EnchantmentCharge {
            source,
            enchantment,
            current,
        }))
    }

    fn weapon_enchantment_cost(&self, enchantment: GlobalFormId) -> Result<f32, Error> {
        let data = self
            .record(enchantment)
            .filter(|record| !record.is_deleted() && record.tag == Tag(*b"ENCH"))
            .and_then(|record| record.find(Tag(*b"ENIT")))
            .filter(|data| matches!(data.len(), 32 | 36))
            .ok_or(Error::Unsupported("malformed weapon enchantment data"))?;
        let bytes: [u8; 4] = data[..4]
            .try_into()
            .map_err(|_| Error::Unsupported("malformed weapon enchantment cost"))?;
        let cost = i32::from_le_bytes(bytes);
        if cost <= 0 {
            return Err(Error::Unsupported("weapon enchantment has no charge cost"));
        }
        Ok(cost as f32)
    }

    /// Read-only poison pass shared by `check_combat_weapon` and the early
    /// validation in `melee_hit` (CMB-25, CommonLib `ExtraPoison`, kind 62).
    /// Returns the stack's index in `Inventory::stacks` alongside the poison
    /// item, so the caller can clear that exact stack's coating without
    /// touching any other stack of the same item. `None` when no stack
    /// carries poison.
    ///
    /// The equipped set is form-only (docs/combat.md, CMB-03: "does not
    /// infer item-instance selection from a form-only equipment set"), so a
    /// hit cannot name which physical stack the attacker swung. When more
    /// than one stack of the item carries poison, the selected stack cannot
    /// be determined, so the contact rejects instead of guessing, the same
    /// rule `combat_stack_quality` applies to tempered value. Unlike
    /// quality, agreement across stacks does not help here: consumption
    /// needs one physical index, not just an agreeing value, so any second
    /// poisoned stack rejects even if it names the same poison and count.
    fn combat_stack_poison(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Result<PoisonStack, Error> {
        let reference = self.refs.get(&actor).ok_or(Error::Actor(actor))?;
        let mut poison: PoisonStack = None;
        for (index, stack) in reference.inventory.stacks(item).iter().enumerate() {
            let Some(extra) = stack.extra(62) else {
                continue;
            };
            let [crate::inventory::Field::Form(Some(form)), crate::inventory::Field::Word(count)] =
                extra.fields.as_slice()
            else {
                return Err(Error::Unsupported("malformed weapon poison extra"));
            };
            if *count == 0 {
                return Err(Error::Unsupported("malformed weapon poison extra"));
            }
            if poison.is_some() {
                return Err(Error::Unsupported("ambiguous poisoned item instances"));
            }
            poison = Some((index, *form));
        }
        Ok(poison)
    }

    /// Read-only quality pass shared by `check_combat_stack` and the early
    /// validation in `melee_hit`. It never touches poison, enchantment or the
    /// item count, so it can run ahead of the checks that own those errors.
    fn combat_stack_quality(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Result<Option<f32>, Error> {
        let reference = self.refs.get(&actor).ok_or(Error::Actor(actor))?;
        let mut quality: Option<f32> = None;
        for stack in reference.inventory.stacks(item) {
            let Some(extra) = stack.extra(37) else {
                continue;
            };
            let [crate::inventory::Field::FloatBits(bits)] = extra.fields.as_slice() else {
                return Err(Error::Unsupported("malformed item quality extra"));
            };
            let value = finite(f64::from(f32::from_bits(*bits)))?;
            match quality {
                Some(existing) if existing != value => {
                    return Err(Error::Unsupported(
                        "ambiguous item quality across equipped stacks",
                    ));
                }
                _ => quality = Some(value),
            }
        }
        Ok(quality)
    }

    pub(super) fn combat_setting(&self, name: &'static str) -> Result<f64, Error> {
        match self.game_setting(name) {
            Some(Value::Float(value)) if value.is_finite() && value >= 0.0 => Ok(f64::from(value)),
            _ => Err(Error::Setting(name)),
        }
    }

    /// Additional multiplicative reduction from the weapon's DNAM Resist
    /// field (offset 0x58/88, `uesp-modfileformat/WEAP.wikitext` line 123),
    /// the target actor value this weapon's damage is measured against. CK
    /// allows 39 DamageResist through 44 MagicResist or -1 for none, always
    /// -1 in original files. DNAM flags bit 0x01, "Ignores Normal Weapon
    /// Resistance" (`WEAP.wikitext` line 58), suppresses the term instead of
    /// rejecting the weapon, matching its documented meaning. `melee_hit`
    /// and `unarmed_damage` already reject any other resist index before
    /// calling this, so a value outside that range here is defensive, not
    /// the primary check.
    ///
    /// No local or UESP source pins the exact reduction formula, so this
    /// treats the actor value as a percentage the same way `combat_armor`
    /// treats DamageResist, applied as its own multiplicative term alongside
    /// armor and shield block. See docs/combat.md CMB-26.
    fn combat_resistance(
        &self,
        target: GlobalFormId,
        resist: i32,
        ignores_resistance: bool,
    ) -> Result<f64, Error> {
        if resist == -1 || ignores_resistance {
            return Ok(0.0);
        }
        let name = crate::actor_values::actor_value_name(resist)
            .ok_or(Error::Unsupported("unsupported weapon resistance value"))?;
        let value = f64::from(
            self.actor_ref(target)
                .ok_or(Error::Actor(target))?
                .values
                .get(name)
                .current(),
        );
        if !value.is_finite() {
            return Err(Error::Number);
        }
        Ok((value / 100.0).clamp(0.0, 1.0))
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
            self.check_no_poison_or_enchant(actor, item)?;
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

    fn combat_weapon(
        &self,
        id: GlobalFormId,
    ) -> Result<(Weapon, Option<GlobalFormId>, f32), Error> {
        let (source, weapon) = self.combat_weapon_source(id)?;
        let mut current = id;
        let mut seen = BTreeSet::new();
        let mut enchantment = None;
        loop {
            if !seen.insert(current) || seen.len() > 128 {
                return Err(Error::Unsupported("cyclic weapon template"));
            }
            let record = self.record(current).ok_or(Error::Equipment)?;
            let (template, raw_enchantment) = Weapon::template_links(record)?;
            if let Some(raw) = raw_enchantment {
                enchantment = Some(
                    self.resolve_from(current, raw)
                        .ok_or(Error::Unsupported("unresolved weapon enchantment"))?,
                );
                break;
            }
            let Some(template) = template else {
                break;
            };
            current = self
                .resolve_from(current, template)
                .ok_or(Error::Unsupported("unresolved weapon template"))?;
        }
        let maximum = self
            .record(source)
            .and_then(|record| record.find(Tag(*b"EAMT")))
            .map(|bytes| {
                if bytes.len() != 2 {
                    return Err(Error::Unsupported(
                        "malformed weapon enchantment charge amount",
                    ));
                }
                Ok(f32::from(u16::from_le_bytes([bytes[0], bytes[1]])))
            })
            .transpose()?
            .unwrap_or(0.0);
        Ok((weapon, enchantment, maximum))
    }

    pub(crate) fn has_bound_weapon_equipped(
        &mut self,
        actor: GlobalFormId,
        hand: Hand,
    ) -> Result<bool, Error> {
        if self.actor(actor).is_none() {
            return Err(Error::Actor(actor));
        }
        let Some(item) = self.hand_weapon(actor, hand) else {
            return Ok(false);
        };
        Ok(self.combat_weapon(item)?.0.flags2 & (1 << 13) != 0)
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
            let (template, _) = Weapon::template_links(record)?;
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
            let (template, _enchantment) = Armor::template_links(record)?;
            let Some(template) = template else {
                return Ok(Armor::from_record(record)?);
            };
            current = self
                .resolve_from(current, template)
                .ok_or(Error::Unsupported("unresolved armor template"))?;
        }
    }

    /// Sum the warmth classification of an actor's equipped armor.
    ///
    /// Survival Mode assigns 54 to warm armor, 17 to cold armor and 27 to
    /// armor without either keyword. Non-armor equipment contributes zero.
    ///
    /// # Errors
    /// Missing actor state or malformed armor keyword data.
    pub(crate) fn actor_warmth_rating(&mut self, actor: GlobalFormId) -> Result<f32, Error> {
        let equipped: Vec<_> = self
            .actor(actor)
            .ok_or(Error::Actor(actor))?
            .equipped
            .iter()
            .copied()
            .collect();
        let warm = self.form("Survival_ArmorWarm");
        let cold = self.form("Survival_ArmorCold");
        let mut total = 0.0;
        for item in equipped {
            let Some(record) = self
                .record(item)
                .filter(|record| record.tag == Tag(*b"ARMO") && !record.is_deleted())
            else {
                continue;
            };
            let mut has_warm = false;
            let mut has_cold = false;
            if let Some(data) = record.find(Tag(*b"KWDA")) {
                if data.len() % 4 != 0 {
                    return Err(Error::Equipment);
                }
                for chunk in data.as_chunks::<4>().0 {
                    let raw = u32::from_le_bytes(*chunk);
                    let Some(keyword) = self.resolve_from(item, raw) else {
                        continue;
                    };
                    has_warm |= Some(keyword) == warm;
                    has_cold |= Some(keyword) == cold;
                }
            }
            total += if has_warm {
                54.0
            } else if has_cold {
                17.0
            } else {
                27.0
            };
        }
        Ok(total)
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
