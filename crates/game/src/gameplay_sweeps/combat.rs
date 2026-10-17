// SPDX-License-Identifier: GPL-3.0-or-later

//! Combat sweep: attack a target NPC with every weapon, and the reverse.
//!
//! Oracle: WEAP records (DATA damage, DNAM animation type and stagger),
//! the `fCombatSneak*Mult` game settings, and CK wiki combat rules. A swing
//! must lower the target's health by exactly the reported damage. A power
//! attack hits harder than a normal one. A sneak attack multiplies the
//! normal damage by the setting of its weapon type. A shield block cuts the
//! damage and a bash reports as a bash. A lethal hit kills. Bows and
//! crossbows spend one arrow or bolt. Hit reactions and recovered arrows
//! belong to the installed Godot pass.

use std::sync::Arc;

use serde_json::{json, Value as Json};
use soe_esm::{GlobalFormId, LoadOrder, Tag};

use super::magic::{bases, scene, Bases, Scene};
use super::{cause_key, near, report, row, selected, visual_check, Options, Outcome};
use crate::combat::{HitResult, MeleeHit};
use crate::with_world;

const UNARMED_ATTACK: GlobalFormId = GlobalFormId {
    file: 0,
    object: 0x1F4,
};
const MAX_ARMOR_REDUCTION: f32 = 0.8;

/// One kind of swing the sweep makes with a weapon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swing {
    Normal,
    Power,
    Sneak,
    ShieldBlock,
    Kill,
    NpcAttacksPlayer,
}

impl Swing {
    pub const ALL: [Swing; 6] = [
        Swing::Normal,
        Swing::Power,
        Swing::Sneak,
        Swing::ShieldBlock,
        Swing::Kill,
        Swing::NpcAttacksPlayer,
    ];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Swing::Normal => "normal",
            Swing::Power => "power",
            Swing::Sneak => "sneak",
            Swing::ShieldBlock => "shield_block",
            Swing::Kill => "kill",
            Swing::NpcAttacksPlayer => "npc_attacks_player",
        }
    }
}

/// Weapon animation types of DNAM, UESP Weapon: hand to hand 0 through crossbow 9.
#[must_use]
pub fn weapon_class(animation: u8) -> &'static str {
    match animation {
        0 => "hand_to_hand",
        1 => "one_handed_sword",
        2 => "dagger",
        3 => "one_handed_axe",
        4 => "mace",
        5 => "two_handed_sword",
        6 => "two_handed_axe",
        7 => "bow",
        8 => "staff",
        9 => "crossbow",
        _ => "unknown",
    }
}

/// The sneak attack multiplier setting of a melee weapon type.
#[must_use]
pub fn sneak_setting(animation: u8) -> Option<&'static str> {
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

/// Reasons a hit result contradicts the combat rules, empty when it is consistent.
#[must_use]
pub fn result_problems(
    hit: &HitResult,
    health_lost: f32,
    killed_expected: bool,
) -> Vec<(&'static str, String)> {
    let mut problems = Vec::new();
    if !(0.0..=MAX_ARMOR_REDUCTION + 1e-4).contains(&hit.armor_reduction) {
        problems.push((
            "armor_reduction_range",
            format!(
                "armor reduction {} is outside 0 to 0.8",
                hit.armor_reduction
            ),
        ));
    }
    if hit.health_damage < 0.0 || !hit.health_damage.is_finite() {
        problems.push((
            "health_damage_invalid",
            format!("health damage {} is not a finite amount", hit.health_damage),
        ));
    }
    if !hit.killed && !near(health_lost, hit.health_damage, 0.01) {
        problems.push((
            "health_loss_mismatch",
            format!(
                "target lost {health_lost} health, the result says {}",
                hit.health_damage
            ),
        ));
    }
    if killed_expected && !hit.killed {
        problems.push(("lethal_hit_survived", "a lethal hit did not kill".into()));
    }
    problems
}

fn equip(scene: &mut Scene, holder: GlobalFormId, item: GlobalFormId) -> Result<(), Outcome> {
    let mut world = scene.game.world_mut();
    if item != UNARMED_ATTACK {
        world.add_item(holder, item, 1);
        world.equip_item(holder, item).map_err(|e| {
            Outcome::error(
                &cause_key("equip", &e.to_string()),
                e.to_string(),
                json!({}),
            )
        })?;
    }
    Ok(())
}

fn find_shield(order: &LoadOrder) -> Option<GlobalFormId> {
    order
        .winners_of(Tag(*b"ARMO"))
        .find(|(_, r)| {
            r.editor_id().is_some_and(|n| {
                n.to_ascii_lowercase().starts_with("armoriron")
                    && n.to_ascii_lowercase().contains("shield")
            })
        })
        .map(|(id, _)| id)
}

fn health(scene: &mut Scene, id: GlobalFormId) -> f32 {
    scene.game.world_mut().actor_value(id, "health")
}

fn melee(
    scene: &mut Scene,
    attacker: GlobalFormId,
    target: GlobalFormId,
    weapon: GlobalFormId,
    power: bool,
    bash: bool,
) -> Result<(HitResult, f32), Outcome> {
    let before = health(scene, target);
    let hit = MeleeHit {
        attacker,
        target,
        weapon,
        is_bash: bash,
        is_power_attack: power,
    };
    let result = with_world!(scene.game, |world, alloc| world.melee_hit(
        hit,
        Vec::new(),
        Vec::new(),
        alloc
    ));
    match result {
        Ok(result) => {
            let lost = before - health(scene, target);
            Ok((result, lost))
        }
        Err(e) => Err(Outcome::error(
            &cause_key("hit", &e.to_string()),
            e.to_string(),
            json!({}),
        )),
    }
}

fn animation(scene: &Scene, weapon: GlobalFormId) -> Option<u8> {
    let world = scene.game.world();
    crate::combat::records::Weapon::from_record(world.record(weapon)?)
        .ok()
        .map(|w| w.animation)
}

/// Swing one melee weapon in one way and check the result.
#[must_use]
pub fn check_swing(
    order: &Arc<LoadOrder>,
    bases: &Bases,
    weapon: GlobalFormId,
    swing: Swing,
) -> Outcome {
    let mut base = scene(order, bases);
    let Some(animation) = animation(&base, weapon) else {
        return Outcome::error(
            "weapon_record_unreadable",
            "the WEAP record does not decode",
            json!({}),
        );
    };
    let observed = json!({"class":weapon_class(animation)});
    if animation >= 7 {
        return Outcome::none(
            if animation == 8 {
                "staff_cast_path_missing"
            } else {
                "ranged_weapon_has_own_case"
            },
            "bows and crossbows are checked by the ammunition case, staves by the magic sweep",
            observed,
        );
    }
    let (player, npc) = (base.player, base.npc);
    let (attacker, target) = if swing == Swing::NpcAttacksPlayer {
        (npc, player)
    } else {
        (player, npc)
    };
    if let Err(outcome) = equip(&mut base, attacker, weapon) {
        return outcome;
    }
    if target == player {
        let _ = base
            .game
            .world_mut()
            .set_actor_value(player, "health", 10_000.0);
    }
    let normal = match melee(&mut base, attacker, target, weapon, false, false) {
        Ok(value) => value,
        Err(outcome) => return outcome,
    };
    let reference = normal.0.health_damage;
    let mut problems = result_problems(&normal.0, normal.1, false);
    let observed = json!({"class":weapon_class(animation),"normal_damage":reference,
        "armor_reduction":normal.0.armor_reduction,"staggered":normal.0.staggered});
    match swing {
        Swing::Normal | Swing::NpcAttacksPlayer => {}
        Swing::Power => {
            let mut fresh = scene(order, bases);
            let (p, n) = (fresh.player, fresh.npc);
            if let Err(outcome) = equip(&mut fresh, p, weapon) {
                return outcome;
            }
            match melee(&mut fresh, p, n, weapon, true, false) {
                Ok((power, lost)) => {
                    problems.extend(result_problems(&power, lost, false));
                    if reference > 0.0 && power.health_damage <= reference {
                        problems.push((
                            "not_stronger",
                            format!(
                                "power attack did {}, the normal attack {reference}",
                                power.health_damage
                            ),
                        ));
                    }
                }
                Err(outcome) => return outcome,
            }
        }
        Swing::Sneak => {
            let Some(setting) = sneak_setting(animation) else {
                return Outcome::none(
                    "no_sneak_setting",
                    "no sneak multiplier for this weapon type",
                    observed,
                );
            };
            let mut fresh = scene(order, bases);
            let (p, n) = (fresh.player, fresh.npc);
            if let Err(outcome) = equip(&mut fresh, p, weapon) {
                return outcome;
            }
            if let Some(state) = fresh.game.world_mut().actor(p) {
                state.sneaking = true;
            }
            let multiplier = match fresh.game.world().game_setting(setting) {
                Some(soe_papyrus::Value::Float(v)) => v,
                Some(soe_papyrus::Value::Int(v)) => v as f32,
                _ => return Outcome::error("sneak_setting_missing", setting, observed),
            };
            match melee(&mut fresh, p, n, weapon, false, false) {
                Ok((sneak, lost)) => {
                    problems.extend(result_problems(&sneak, lost, false));
                    if !sneak.sneak_attacked {
                        problems.push((
                            "flag_missing",
                            "a hit from sneaking on an unaware target is not a sneak attack".into(),
                        ));
                    } else if reference > 0.0
                        && !near(sneak.health_damage, reference * multiplier, 0.05)
                    {
                        problems.push((
                            "multiplier_wrong",
                            format!(
                                "sneak damage {} is not {reference} times {setting} {multiplier}",
                                sneak.health_damage
                            ),
                        ));
                    }
                }
                Err(outcome) => return outcome,
            }
        }
        Swing::ShieldBlock => {
            let Some(shield) = find_shield(order) else {
                return Outcome::none("shield_not_found", "no iron shield ARMO matched", observed);
            };
            let mut fresh = scene(order, bases);
            let (p, n) = (fresh.player, fresh.npc);
            if let Err(outcome) = equip(&mut fresh, n, shield) {
                return outcome;
            }
            if let Err(e) = fresh.game.world_mut().set_blocking(n, true) {
                return Outcome::error(
                    &cause_key("block", &e.to_string()),
                    e.to_string(),
                    observed,
                );
            }
            if let Err(outcome) = equip(&mut fresh, p, weapon) {
                return outcome;
            }
            match melee(&mut fresh, p, n, weapon, false, false) {
                Ok((blocked, lost)) => {
                    problems.extend(result_problems(&blocked, lost, false));
                    if !blocked.blocked {
                        problems.push((
                            "not_blocked",
                            "the target held a block stance with a shield and was not blocked"
                                .into(),
                        ));
                    } else if reference > 0.0 && blocked.health_damage >= reference {
                        problems.push((
                            "no_reduction",
                            format!("block left {} damage of {reference}", blocked.health_damage),
                        ));
                    }
                }
                Err(outcome) => return outcome,
            }
        }
        Swing::Kill => {
            let mut fresh = scene(order, bases);
            let (p, n) = (fresh.player, fresh.npc);
            if let Err(outcome) = equip(&mut fresh, p, weapon) {
                return outcome;
            }
            let _ = fresh.game.world_mut().set_actor_value(n, "health", 1.0);
            match melee(&mut fresh, p, n, weapon, false, false) {
                Ok((kill, lost)) => {
                    problems.extend(result_problems(&kill, lost, true));
                    if kill.killed && !fresh.game.world().actor_ref(n).is_some_and(|a| a.dead) {
                        problems.push((
                            "dead_flag_mismatch",
                            "the result says killed, the actor is not dead".into(),
                        ));
                    }
                }
                Err(outcome) => return outcome,
            }
        }
    }
    match problems.first() {
        None => Outcome::pass(observed),
        Some((key, _)) => Outcome::wrong(
            &format!("{}_{key}", swing.label()),
            problems
                .iter()
                .map(|(_, reason)| reason.as_str())
                .collect::<Vec<_>>()
                .join(". "),
            observed,
        ),
    }
}

/// A shield bash on an NPC reports as a bash.
#[must_use]
pub fn check_bash(order: &Arc<LoadOrder>, bases: &Bases, shield: GlobalFormId) -> Outcome {
    let mut base = scene(order, bases);
    let (player, npc) = (base.player, base.npc);
    if let Err(outcome) = equip(&mut base, player, shield) {
        return outcome;
    }
    match melee(&mut base, player, npc, shield, false, true) {
        Ok((hit, lost)) => {
            let mut problems = result_problems(&hit, lost, false);
            if !hit.bashed {
                problems.push(("not_reported", "a bash did not report as a bash".into()));
            }
            if problems.is_empty() {
                Outcome::pass(json!({"damage":hit.health_damage,"staggered":hit.staggered}))
            } else {
                Outcome::wrong(
                    &format!("bash_{}", problems[0].0),
                    problems
                        .iter()
                        .map(|(_, r)| r.as_str())
                        .collect::<Vec<_>>()
                        .join(". "),
                    json!({}),
                )
            }
        }
        Err(outcome) => outcome,
    }
}

/// Fire a bow or crossbow with its ammunition. One arrow or bolt is spent.
#[must_use]
pub fn check_ranged(order: &Arc<LoadOrder>, bases: &Bases, weapon: GlobalFormId) -> Outcome {
    let mut base = scene(order, bases);
    let player = base.player;
    let Some(animation) = animation(&base, weapon).filter(|a| *a == 7 || *a == 9) else {
        return Outcome::none("not_ranged", "not a bow or crossbow", json!({}));
    };
    let names: &[&str] = if animation == 7 {
        &["ironarrow"]
    } else {
        &["dlc1bolt", "bolt"]
    };
    let ammo = names.iter().find_map(|wanted| {
        order
            .winners_of(Tag(*b"AMMO"))
            .find(|(_, r)| {
                r.editor_id()
                    .is_some_and(|n| n.to_ascii_lowercase().starts_with(wanted))
            })
            .map(|(id, _)| id)
    });
    let Some(ammo) = ammo else {
        return Outcome::none(
            "ammunition_not_found",
            "no AMMO record matched the weapon class",
            json!({}),
        );
    };
    base.game.world_mut().add_item(player, ammo, 10);
    if let Err(outcome) =
        equip(&mut base, player, weapon).and_then(|()| equip(&mut base, player, ammo))
    {
        return outcome;
    }
    let before = base.game.world_mut().item_count(player, ammo);
    let released = with_world!(base.game, |world, alloc| world.release_projectile(
        player,
        weapon,
        [0.0, 0.0, 120.0],
        [0.0, 1.0, 0.0],
        alloc
    ));
    if let Err(e) = released {
        return Outcome::error(
            &cause_key("release", &e.to_string()),
            e.to_string(),
            json!({}),
        );
    }
    let after = base.game.world_mut().item_count(player, ammo);
    if before - after != 1 {
        return Outcome::wrong(
            "ammunition_not_spent",
            format!("ammunition went from {before} to {after}"),
            json!({}),
        );
    }
    Outcome::pass(json!({"class":weapon_class(animation),"ammo_left":after}))
}

/// What only the installed Godot pass can see.
#[must_use]
pub fn visual_checks() -> Vec<Json> {
    [
        "hit reaction animation on the target",
        "death animation and ragdoll",
        "power attack and bash animations",
        "arrow flight, impact and recovered arrow",
        "blocking with a weapon",
    ]
    .iter()
    .map(|name| visual_check(name, &[]))
    .collect()
}

/// Run the combat sweep.
#[must_use]
pub fn run(order: &Arc<LoadOrder>, options: &Options) -> Json {
    let Some(bases) = bases(order) else {
        return report(
            "combat",
            "no target base forms found",
            Vec::new(),
            visual_checks(),
        );
    };
    let mut rows = Vec::new();
    for (weapon, name) in selected(order, Tag(*b"WEAP"), options) {
        eprintln!("combat sweep: weapon {weapon} {name}");
        let class = {
            let scene = scene(order, &bases);
            animation(&scene, weapon)
        };
        if class.is_some_and(|a| a == 7 || a == 9) {
            rows.push(row(
                order,
                weapon,
                "WEAP",
                "ranged",
                &check_ranged(order, &bases, weapon),
            ));
            continue;
        }
        for swing in Swing::ALL {
            rows.push(row(
                order,
                weapon,
                "WEAP",
                swing.label(),
                &check_swing(order, &bases, weapon, swing),
            ));
        }
        rows.push(row(
            order,
            weapon,
            "WEAP",
            "weapon_block",
            &Outcome::none(
                "weapon_block_not_evaluated",
                "blocking with a weapon and no shield is not evaluated by melee_hit",
                json!({}),
            ),
        ));
    }
    if let Some(shield) = find_shield(order) {
        rows.push(row(
            order,
            shield,
            "ARMO",
            "bash",
            &check_bash(order, &bases, shield),
        ));
    }
    report(
        "combat",
        "WEAP records, fCombatSneak settings and CK wiki combat rules, no captured vanilla runtime oracle",
        rows,
        visual_checks(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(armor: f32, damage: f32, killed: bool) -> HitResult {
        HitResult {
            weapon_damage: 10.0,
            armor_rating: 0.0,
            armor_reduction: armor,
            blocked: false,
            block_reduction: 0.0,
            resistance_reduction: 0.0,
            health_damage: damage,
            killed,
            staggered: false,
            bashed: false,
            poisoned: false,
            sneak_attacked: false,
        }
    }

    #[test]
    fn consistent_result_has_no_problems() {
        let result = hit(0.2, 8.0, false);
        assert!(result_problems(&result, 8.0, false).is_empty());
    }

    #[test]
    fn health_loss_must_match_the_result() {
        let result = hit(0.0, 8.0, false);
        assert_eq!(result_problems(&result, 5.0, false).len(), 1);
    }

    #[test]
    fn armor_reduction_is_capped_at_eighty_percent() {
        let result = hit(0.9, 1.0, false);
        assert!(!result_problems(&result, 1.0, false).is_empty());
    }

    #[test]
    fn lethal_hit_must_kill() {
        let result = hit(0.0, 1.0, false);
        assert!(!result_problems(&result, 1.0, true).is_empty());
    }

    #[test]
    fn weapon_classes_and_sneak_settings_line_up() {
        assert_eq!(weapon_class(7), "bow");
        assert_eq!(weapon_class(9), "crossbow");
        assert_eq!(sneak_setting(2), Some("fCombatSneak1HDaggerMult"));
        assert_eq!(sneak_setting(7), None);
    }
}
