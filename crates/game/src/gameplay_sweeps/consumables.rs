// SPDX-License-Identifier: GPL-3.0-or-later

//! Consumable sweep: consume every ALCH on the player and check each effect.
//!
//! Oracle: UESP and CK wiki magic effect archetypes (Value Modifier,
//! Peak Value Modifier), the MGEF Detrimental and Recover flags from
//! `wbDefinitionsTES5.pas`, and the ALCH record data. Health, magicka and
//! stamina with a duration of zero restore or damage once. With a duration
//! they change per second. Every other actor value changes as a constant
//! for the duration and returns afterwards.

use std::sync::Arc;

use serde_json::{json, Value as Json};
use soe_esm::{GlobalFormId, LoadOrder, Tag};

use super::{cause_key, fresh_game, near, report, row, selected, visual_check, Options, Outcome};

const MGEF_DETRIMENTAL: u32 = 1 << 2;
const ALCH_POISON: u32 = 1 << 17;
const VALUE_MODIFIER: u32 = 0;
const DUAL_VALUE_MODIFIER: u32 = 5;
const PEAK_VALUE_MODIFIER: u32 = 34;

/// What one effect must do to the actor value it names.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Expected {
    /// Change of the current value once the effect is applied.
    Current(f32),
    /// Change of the maximum value while the effect lasts.
    Max(f32),
    /// Signed change of the current value per second.
    PerSecond(f32),
}

/// One ALCH effect resolved against its MGEF.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectPlan {
    pub effect: GlobalFormId,
    pub name: String,
    pub archetype: u32,
    pub flags: u32,
    pub value: Option<String>,
    pub magnitude: f32,
    pub duration: u32,
}

/// UESP name of an MGEF archetype number.
#[must_use]
pub fn archetype_name(archetype: u32) -> &'static str {
    const NAMES: [&str; 47] = [
        "value_modifier",
        "script",
        "dispel",
        "cure_disease",
        "absorb",
        "dual_value_modifier",
        "calm",
        "demoralize",
        "frenzy",
        "disarm",
        "command_summoned",
        "invisibility",
        "light",
        "unused_13",
        "unused_14",
        "lock",
        "open",
        "bound_weapon",
        "summon_creature",
        "detect_life",
        "telekinesis",
        "paralysis",
        "reanimate",
        "soul_trap",
        "turn_undead",
        "guide",
        "werewolf_feed",
        "cure_paralysis",
        "cure_addiction",
        "cure_poison",
        "concussion",
        "value_and_parts",
        "accumulate_magnitude",
        "stagger",
        "peak_value_modifier",
        "cloak",
        "werewolf",
        "slow_time",
        "rally",
        "enhance_weapon",
        "spawn_hazard",
        "etherealize",
        "banish",
        "spawn_scripted_ref",
        "disguise",
        "grab_actor",
        "vampire_lord",
    ];
    NAMES.get(archetype as usize).copied().unwrap_or("unknown")
}

/// Whether an actor value is one of the three vitals.
#[must_use]
pub fn is_vital(value: &str) -> bool {
    matches!(value, "health" | "magicka" | "stamina")
}

/// The modelled change of one effect, or `None` when the archetype has no model.
#[must_use]
pub fn expected_change(plan: &EffectPlan, current: f32, maximum: f32) -> Option<Expected> {
    let value = plan.value.as_deref()?;
    if plan.archetype != VALUE_MODIFIER && plan.archetype != PEAK_VALUE_MODIFIER {
        return None;
    }
    let sign = if plan.flags & MGEF_DETRIMENTAL != 0 {
        -1.0
    } else {
        1.0
    };
    let magnitude = plan.magnitude;
    if !is_vital(value) {
        return Some(Expected::Current(sign * magnitude));
    }
    if plan.archetype == PEAK_VALUE_MODIFIER {
        return Some(Expected::Max(sign * magnitude));
    }
    if plan.duration > 0 {
        return Some(Expected::PerSecond(sign * magnitude));
    }
    Some(Expected::Current(if sign > 0.0 {
        magnitude.min((maximum - current).max(0.0))
    } else {
        -magnitude.min(current.max(0.0))
    }))
}

/// Whether the modelled change returns to the starting value when the effect ends.
#[must_use]
pub fn reverts_at_end(plan: &EffectPlan) -> bool {
    let Some(value) = plan.value.as_deref() else {
        return false;
    };
    plan.duration > 0
        && (plan.archetype == PEAK_VALUE_MODIFIER
            || (plan.archetype == VALUE_MODIFIER && !is_vital(value)))
}

pub(super) fn plan_of(
    world: &crate::world::World,
    id: GlobalFormId,
    effects: &[soe_esm::records::gameplay::Effect],
) -> Result<Vec<EffectPlan>, String> {
    let mut plans = Vec::new();
    for entry in effects {
        let effect = world
            .resolve_from(id, entry.base_effect)
            .ok_or_else(|| format!("effect {:#x} does not resolve", entry.base_effect))?;
        let mgef = world.magic_effect(effect).map_err(|e| e.to_string())?;
        let value = (mgef.data.archetype == VALUE_MODIFIER
            || mgef.data.archetype == PEAK_VALUE_MODIFIER
            || mgef.data.archetype == DUAL_VALUE_MODIFIER)
            .then(|| crate::actor_values::actor_value_name(mgef.data.primary_actor_value))
            .flatten()
            .map(str::to_ascii_lowercase);
        plans.push(EffectPlan {
            effect,
            name: mgef.editor_id.unwrap_or_default(),
            archetype: mgef.data.archetype,
            flags: mgef.data.flags,
            value,
            magnitude: entry.data.magnitude,
            duration: entry.data.duration,
        });
    }
    Ok(plans)
}

struct Snapshot {
    current: f32,
    maximum: f32,
}

fn snapshot(game: &mut crate::Game, player: GlobalFormId, value: &str) -> Snapshot {
    let mut world = game.world_mut();
    Snapshot {
        current: world.actor_value(player, value),
        maximum: world.actor_value_maximum(player, value),
    }
}

fn active_count(game: &crate::Game, player: GlobalFormId, effect: GlobalFormId) -> usize {
    game.world()
        .active_effects()
        .filter(|a| a.target == player && a.effect == effect)
        .count()
}

/// Consume one ALCH on the player and check every effect.
#[must_use]
pub fn check_ingestible(order: &Arc<LoadOrder>, id: GlobalFormId) -> Outcome {
    let mut game = fresh_game(order);
    let player = game.player();
    let (plans, flags) = {
        let world = game.world();
        let ingestible = match world.ingestible(id) {
            Ok(value) => value,
            Err(error) => {
                return Outcome::error(
                    &cause_key("record", &error.to_string()),
                    error.to_string(),
                    json!({}),
                )
            }
        };
        match plan_of(&world, id, &ingestible.effects) {
            Ok(plans) => (plans, ingestible.data.flags),
            Err(error) => {
                return Outcome::error(&cause_key("record", &error), error, json!({}));
            }
        }
    };
    if flags & ALCH_POISON != 0 {
        return Outcome::none(
            "poison_needs_weapon_pass",
            "a poison is applied to a weapon and delivered by a hit, drinking it is refused",
            json!({"effects":plans.len()}),
        );
    }
    if plans.is_empty() {
        return Outcome::none("no_effects", "the ALCH record lists no effects", json!({}));
    }
    game.world_mut().add_item(player, id, 2);
    for value in plans
        .iter()
        .filter_map(|p| p.value.as_deref().filter(|v| is_vital(v)))
    {
        let maximum = game.world_mut().actor_value_maximum(player, value);
        let _ = game
            .world_mut()
            .damage_actor_value(player, value, maximum * 0.5);
    }
    let before: Vec<Option<Snapshot>> = plans
        .iter()
        .map(|p| p.value.as_deref().map(|v| snapshot(&mut game, player, v)))
        .collect();
    let applied = with_world!(game, |world, alloc| world
        .drink_ingestible(id, player, alloc));
    let ids = match applied {
        Ok(ids) => ids,
        Err(error) => {
            return Outcome::error(
                &cause_key("drink", &error.to_string()),
                error.to_string(),
                json!({}),
            )
        }
    };
    let mut observed = json!({"applied":ids.len(),"effects":plans.iter().map(|p|
        json!({"effect":p.name,"archetype":archetype_name(p.archetype),"value":p.value,
        "magnitude":p.magnitude,"duration":p.duration})).collect::<Vec<_>>()});
    let left = game.world_mut().item_count(player, id);
    if left != 1 {
        return Outcome::wrong(
            "item_not_consumed",
            format!("expected one potion left, found {left}"),
            observed,
        );
    }
    let mut unmodeled = None;
    for (plan, start) in plans.iter().zip(&before) {
        if plan.duration > 0 {
            let found = game
                .world()
                .active_effects()
                .find(|a| a.target == player && a.effect == plan.effect)
                .map(|a| a.magnitude);
            match found {
                None => {
                    return Outcome::wrong(
                        "effect_missing_from_active_list",
                        format!("{} is not in the active effects list", plan.name),
                        observed,
                    )
                }
                Some(magnitude) if !near(magnitude, plan.magnitude, 0.01) => {
                    return Outcome::wrong(
                        "active_effect_magnitude",
                        format!(
                            "{} has magnitude {magnitude}, record says {}",
                            plan.name, plan.magnitude
                        ),
                        observed,
                    )
                }
                Some(_) => {}
            }
        }
        let (Some(value), Some(start)) = (plan.value.as_deref(), start) else {
            unmodeled.get_or_insert(plan.archetype);
            continue;
        };
        let Some(expected) = expected_change(plan, start.current, start.maximum) else {
            unmodeled.get_or_insert(plan.archetype);
            continue;
        };
        if matches!(expected, Expected::PerSecond(_)) {
            game.tick(1.0);
        }
        let now = snapshot(&mut game, player, value);
        let (delta, want) = match expected {
            Expected::Current(want) | Expected::PerSecond(want) => {
                (now.current - start.current, want)
            }
            Expected::Max(want) => (now.maximum - start.maximum, want),
        };
        let slack = if matches!(expected, Expected::PerSecond(_)) {
            want.abs() * 0.5 + 1.0
        } else {
            0.01
        };
        if !near(delta, want, slack) {
            return Outcome::wrong(
                &format!("{}_{}_change", archetype_name(plan.archetype), value),
                format!(
                    "{} on {value}: expected {want}, observed {delta}",
                    plan.name
                ),
                observed,
            );
        }
    }
    let longest = plans.iter().map(|p| p.duration).max().unwrap_or(0);
    let mut elapsed = 0.0;
    let step = (f64::from(longest) / 8.0).max(1.0);
    while elapsed < f64::from(longest) + 2.0 {
        game.tick(step);
        elapsed += step;
    }
    for (plan, start) in plans.iter().zip(&before) {
        if plan.duration > 0 && active_count(&game, player, plan.effect) > 0 {
            return Outcome::wrong(
                "effect_outlives_duration",
                format!("{} is still active after {elapsed} seconds", plan.name),
                observed,
            );
        }
        if let (true, Some(value), Some(start)) =
            (reverts_at_end(plan), plan.value.as_deref(), start)
        {
            let now = snapshot(&mut game, player, value);
            let (a, b) = if plan.archetype == PEAK_VALUE_MODIFIER && is_vital(value) {
                (now.maximum, start.maximum)
            } else {
                (now.current, start.current)
            };
            if !near(a, b, 0.01) {
                return Outcome::wrong(
                    "value_not_restored_at_end",
                    format!("{value} is {a} after the effect ended, started at {b}"),
                    observed,
                );
            }
        }
    }
    if let Some(stack) = stacking_failure(order, id, &plans) {
        return stack;
    }
    if let Some(archetype) = unmodeled {
        observed["unmodeled_archetype"] = json!(archetype_name(archetype));
        return Outcome::none(
            &cause_key("archetype", archetype_name(archetype)),
            "universal checks passed, the archetype outcome has no model yet",
            observed,
        );
    }
    Outcome::pass(observed)
}

/// Drink the same potion twice. The vanilla effect refreshes, it does not stack.
fn stacking_failure(
    order: &Arc<LoadOrder>,
    id: GlobalFormId,
    plans: &[EffectPlan],
) -> Option<Outcome> {
    let timed: Vec<_> = plans.iter().filter(|p| p.duration > 0).collect();
    if timed.is_empty() {
        return None;
    }
    let mut game = fresh_game(order);
    let player = game.player();
    game.world_mut().add_item(player, id, 2);
    for _ in 0..2 {
        if let Err(error) = with_world!(game, |world, alloc| world
            .drink_ingestible(id, player, alloc))
        {
            return Some(Outcome::error(
                &cause_key("second_drink", &error.to_string()),
                error.to_string(),
                json!({}),
            ));
        }
    }
    for plan in &timed {
        let expected = plans.iter().filter(|p| p.effect == plan.effect).count();
        let found = active_count(&game, player, plan.effect);
        if found > expected {
            return Some(Outcome::wrong(
                "same_potion_stacks",
                format!(
                    "{} has {found} active copies after two potions, expected {expected}",
                    plan.name
                ),
                json!({"effect":plan.name}),
            ));
        }
    }
    None
}

/// Ingredients: the engine has no eat path yet, so each is reported as such.
#[must_use]
pub fn check_ingredient(order: &LoadOrder, id: GlobalFormId) -> Outcome {
    let effects = order.winner(id).map_or(0, |r| r.all(Tag(*b"EFID")).count());
    Outcome::none(
        "ingredient_eat_path_missing",
        "no public world call eats an INGR, first effect learned and effects applied cannot be observed",
        json!({"effects":effects}),
    )
}

/// Records whose use animation needs the installed Godot pass.
#[must_use]
pub fn visual_checks(order: &LoadOrder) -> Vec<Json> {
    let find = |tag: &[u8; 4], needle: &str| -> Vec<String> {
        order
            .winners_of(Tag(*tag))
            .filter_map(|(id, record)| {
                record
                    .editor_id()
                    .filter(|name| name.to_ascii_lowercase().contains(needle))
                    .map(|name| format!("{id} {name}"))
            })
            .collect()
    };
    let mut checks = vec![
        visual_check("drink animation and sound for every potion", &[]),
        visual_check("eat animation and sound for every food", &[]),
        visual_check("book menu opens and turns the page", &[]),
    ];
    for (name, tag, needle) in [
        (
            "Elder Scroll reading idle, scroll opening, sky and blur",
            b"BOOK",
            "elderscroll",
        ),
        (
            "Elder Scroll reading idle, scroll opening, sky and blur",
            b"BOOK",
            "dragonrend",
        ),
        ("torch held", b"LIGH", "torch01"),
        ("Dwemer lexicon use", b"MISC", "lexicon"),
        ("attunement sphere use", b"MISC", "attunement"),
        ("Azura's star use", b"SLGM", "azura"),
    ] {
        checks.push(visual_check(name, &find(tag, needle)));
    }
    checks
}

/// Run the consumable sweep over every ALCH and INGR.
#[must_use]
pub fn run(order: &Arc<LoadOrder>, options: &Options) -> Json {
    let mut rows = Vec::new();
    for (id, name) in selected(order, Tag(*b"ALCH"), options) {
        eprintln!("consumable sweep: {id} {name}");
        rows.push(row(
            order,
            id,
            "ALCH",
            "drink",
            &check_ingestible(order, id),
        ));
    }
    for (id, name) in selected(order, Tag(*b"INGR"), options) {
        eprintln!("consumable sweep: {id} {name}");
        rows.push(row(order, id, "INGR", "eat", &check_ingredient(order, id)));
    }
    report(
        "consumables",
        "CK wiki and UESP archetypes, MGEF flags and ALCH records, no captured vanilla runtime oracle",
        rows,
        visual_checks(order),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(archetype: u32, flags: u32, value: &str, magnitude: f32, duration: u32) -> EffectPlan {
        EffectPlan {
            effect: GlobalFormId { file: 0, object: 1 },
            name: "T".into(),
            archetype,
            flags,
            value: Some(value.into()),
            magnitude,
            duration,
        }
    }

    #[test]
    fn instant_restore_is_capped_by_the_missing_amount() {
        let restore = plan(0, 0, "health", 50.0, 0);
        assert_eq!(
            expected_change(&restore, 80.0, 100.0),
            Some(Expected::Current(20.0))
        );
        assert_eq!(
            expected_change(&restore, 10.0, 100.0),
            Some(Expected::Current(50.0))
        );
    }

    #[test]
    fn instant_damage_is_capped_by_the_current_value() {
        let damage = plan(0, MGEF_DETRIMENTAL, "health", 50.0, 0);
        assert_eq!(
            expected_change(&damage, 30.0, 100.0),
            Some(Expected::Current(-30.0))
        );
    }

    #[test]
    fn timed_vital_changes_per_second() {
        let regen = plan(0, 0, "health", 3.0, 10);
        assert_eq!(
            expected_change(&regen, 10.0, 100.0),
            Some(Expected::PerSecond(3.0))
        );
        assert!(!reverts_at_end(&regen));
    }

    #[test]
    fn fortify_skill_is_constant_and_reverts() {
        let fortify = plan(0, 0, "onehanded", 20.0, 60);
        assert_eq!(
            expected_change(&fortify, 15.0, 15.0),
            Some(Expected::Current(20.0))
        );
        assert!(reverts_at_end(&fortify));
    }

    #[test]
    fn peak_vital_raises_the_maximum() {
        let fortify = plan(34, 0, "health", 40.0, 60);
        assert_eq!(
            expected_change(&fortify, 100.0, 100.0),
            Some(Expected::Max(40.0))
        );
        assert!(reverts_at_end(&fortify));
    }

    #[test]
    fn other_archetypes_have_no_model() {
        let invisibility = EffectPlan {
            value: None,
            ..plan(11, 0, "x", 0.0, 30)
        };
        assert_eq!(expected_change(&invisibility, 0.0, 0.0), None);
        assert_eq!(archetype_name(11), "invisibility");
        assert_eq!(archetype_name(99), "unknown");
    }
}
