// SPDX-License-Identifier: GPL-3.0-or-later

//! Ordinary melee damage uses winning records and existing actor/inventory state.
//! GMST interpolation and armor formulas follow the sources in docs/combat.md.
//! Unsupported modifiers reject the hit before health or events change.

use std::collections::BTreeSet;

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::Value;

use super::{Alloc, Pending, World};
use crate::combat::{
    records::{Armor, PowerAttack, Weapon},
    Difficulty, Error, HitResult, MeleeHit,
};

const UNARMED_ATTACK: u32 = 0x1F4;

const SUPPORTED_ARMOR_SLOTS: u32 = (1 << 14) - 1;

/// UESP's Combat and Damage pages give power attack stamina cost as an
/// unnamed formula, no GMST is named on either page: "power attack stamina
/// cost = (40 + weapon weight * 2) * attack cost multiplier * (1 - perk
/// effect)". `validate_combat_actor` already rejects an attacker carrying
/// any perk entry, so the perk term is always 1 here. "Attack cost
/// multiplier" is the race's Attacks entry Stamina Mult field, read through
/// `PowerAttack`. See docs/combat.md CMB-21.
const POWER_ATTACK_STAMINA_BASE: f32 = 40.0;
const POWER_ATTACK_STAMINA_PER_WEIGHT: f32 = 2.0;

/// A poisoned stack's index in `Inventory::stacks` alongside its poison
/// item, threaded from `combat_stack_poison` through `check_combat_weapon`
/// to `melee_hit`'s deferred consumption step (CMB-25).
type PoisonStack = Option<(usize, GlobalFormId)>;

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
        let mut bash_stamina_cost = None;
        let mut poison_stack: PoisonStack = None;
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
            (damage, 0.0_f32, 1.0_f32, None, resist, ignores_resistance)
        } else {
            let weapon = self.combat_weapon(hit.weapon)?;
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
        let weapon_damage = finite(f64::from(weapon_damage) * f64::from(quality_multiplier))?;
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
        let health_damage = finite(
            f64::from(weapon_damage)
                * (1.0 - reduction)
                * (1.0 - block_reduction)
                * (1.0 - resistance_reduction)
                * difficulty,
        )?;
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
            Value::Bool(hit.is_power_attack),
            Value::Bool(false),
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
            if staggered {
                target.stagger_end_time = stagger_deadline;
            }
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
        // Poison (CMB-25) applies and clears last, after every other part of
        // the hit has already committed, the same deferred placement
        // bash_stamina_cost and attacker_stamina above use. A poison error
        // here can leave the physical hit committed with the coating still
        // on the weapon, the same accepted partial-commit risk those two
        // already carry for this function.
        let mut poisoned = false;
        if let Some((index, poison_item)) = poison_stack {
            self.apply_poison(hit.attacker, poison_item, hit.target, alloc)?;
            if let Some(reference) = self.reference(hit.attacker) {
                reference
                    .inventory
                    .clear_stack_extra(hit.weapon, index, 62)?;
            }
            poisoned = true;
        }
        let killed = !bleedout_target && lethal && self.kill_actor(hit.target, Some(hit.attacker));
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
        })
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

    fn validate_combat_actor(&mut self, id: GlobalFormId) -> Result<(), Error> {
        let effect_has_perk = self
            .active_effects
            .values()
            .any(|effect| effect.target == id && effect.granted_perk.is_some());
        let actor = self.actor(id).ok_or(Error::Actor(id))?;
        if actor.dead || actor.ghost || actor.unconscious {
            return Err(Error::Actor(id));
        }
        if !actor.perks.is_empty() || effect_has_perk {
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

    /// Rejects an equipped weapon stack that carries an enchantment (kind
    /// 155), but allows poison (kind 62): CMB-25 lets an ordinary weapon
    /// swing read and consume a poisoned stack instead of rejecting it
    /// outright. Bash and armor still reject poison unconditionally through
    /// `check_no_poison_or_enchant`, only the ordinary weapon branch of
    /// `melee_hit` calls this. A missing reference or an item count of zero
    /// or less rejects too.
    fn check_no_enchant(&self, actor: GlobalFormId, item: GlobalFormId) -> Result<(), Error> {
        let reference = self.refs.get(&actor).ok_or(Error::Actor(actor))?;
        if reference.inventory.count(item) <= 0 {
            return Err(Error::Equipment);
        }
        if reference
            .inventory
            .stacks(item)
            .iter()
            .any(|stack| stack.extras().iter().any(|extra| extra.kind == 155))
        {
            return Err(Error::Unsupported("enchanted item instances"));
        }
        Ok(())
    }

    /// Read the equipped weapon's quality and poisoned stack together for an
    /// ordinary melee swing, after rejecting an enchanted stack. Poison is
    /// read instead of rejected, unlike `check_combat_stack`. See
    /// `combat_stack_quality` and `combat_stack_poison`.
    fn check_combat_weapon(
        &self,
        actor: GlobalFormId,
        item: GlobalFormId,
    ) -> Result<(Option<f32>, PoisonStack), Error> {
        self.check_no_enchant(actor, item)?;
        let quality = self.combat_stack_quality(actor, item)?;
        let poison = self.combat_stack_poison(actor, item)?;
        Ok((quality, poison))
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

    fn combat_setting(&self, name: &'static str) -> Result<f64, Error> {
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
