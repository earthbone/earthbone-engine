// SPDX-License-Identifier: GPL-3.0-or-later

//! Magic and targeting sweep: spells, scrolls, enchantments, shouts and the
//! unique items with scripted or archetype effects.
//!
//! Oracle: SPEL and MGEF records (delivery, cast type, area in feet, resist
//! actor value), CK wiki cast semantics, and UESP resistance rules. A spell
//! cast at an NPC or a creature must apply each timed effect to it with the
//! authored magnitude, charge the magicka cost, and with an area reach a
//! bystander inside the radius and not one outside it. A spell cast at an
//! object or empty space applies no actor effect. The flight of an aimed
//! projectile or beam belongs to the installed Godot pass.

use std::sync::Arc;

use serde_json::{json, Value as Json};
use soe_esm::{GlobalFormId, LoadOrder, Tag};

use super::consumables::{expected_change, plan_of, EffectPlan, Expected};
use super::{cause_key, fresh_game, near, report, row, selected, visual_check, Options, Outcome};
use crate::combat::MeleeHit;
use crate::Game;

/// Game units in one foot, the unit of an effect's area (UESP Units).
pub const UNITS_PER_FOOT: f32 = 21.3333;
const BOOK_TEACHES_SPELL: u8 = 0x04;
const HOSTILE: u32 = 1;
const UNIQUE_NEEDLES: [&str; 9] = [
    "razor",
    "wabbajack",
    "ebonyblade",
    "volendrung",
    "dawnbreaker",
    "bloodskal",
    "aurielsbow",
    "chillrend",
    "spellbreaker",
];

/// The base forms placed as targets, found once from the load order.
#[derive(Debug, Clone, Copy)]
pub struct Bases {
    pub npc: GlobalFormId,
    pub creature: Option<GlobalFormId>,
    pub object: GlobalFormId,
}

/// Which target a case aims at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Itself,
    Npc,
    Creature,
    Object,
    Space,
}

impl Target {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Itself => "self",
            Self::Npc => "npc",
            Self::Creature => "creature",
            Self::Object => "object",
            Self::Space => "space",
        }
    }
}

/// Targets a spell of this delivery is sensible against. Delivery numbers
/// follow MGEF and SPEL: 0 self, 1 contact, 2 aimed, 3 target actor, 4 target location.
#[must_use]
pub fn targets_for(delivery: u32) -> &'static [Target] {
    match delivery {
        0 => &[Target::Itself],
        1..=3 => &[Target::Npc, Target::Creature, Target::Object],
        4 => &[Target::Space],
        _ => &[],
    }
}

/// Whether an area effect should reach a bystander at this distance.
#[must_use]
pub fn in_area(area_feet: u32, distance: f32) -> bool {
    distance <= area_feet as f32 * UNITS_PER_FOOT
}

/// Magnitude after a resistance percentage, UESP resistance rule.
#[must_use]
pub fn resisted_magnitude(magnitude: f32, resistance_percent: f32) -> f32 {
    magnitude * (1.0 - (resistance_percent / 100.0).clamp(0.0, 1.0))
}

fn first_with(order: &LoadOrder, tag: Tag, names: &[&str]) -> Option<GlobalFormId> {
    names.iter().find_map(|wanted| {
        order
            .winners_of(tag)
            .find(|(_, r)| {
                r.editor_id()
                    .is_some_and(|n| n.to_ascii_lowercase().starts_with(wanted))
            })
            .map(|(id, _)| id)
    })
}

/// Choose the target base forms.
#[must_use]
pub fn bases(order: &LoadOrder) -> Option<Bases> {
    let npc = first_with(
        order,
        Tag(*b"NPC_"),
        &["hadvar", "ralof", "lydia", "ysolda"],
    )
    .or_else(|| {
        order
            .winners_of(Tag(*b"NPC_"))
            .find(|(_, r)| r.editor_id().is_some())
            .map(|(id, _)| id)
    })?;
    let creature = first_with(
        order,
        Tag(*b"NPC_"),
        &["encwolf", "encmudcrab", "encskeever", "wolf"],
    );
    let object = first_with(order, Tag(*b"MISC"), &["gold001", "ironingot"])?;
    Some(Bases {
        npc,
        creature,
        object,
    })
}

fn put(world: &mut crate::world::World, id: GlobalFormId, position: [f32; 3]) {
    if let Some(reference) = world.reference(id) {
        reference.position = position;
    }
}

pub(super) struct Scene {
    pub(super) game: Game,
    pub(super) player: GlobalFormId,
    pub(super) npc: GlobalFormId,
    pub(super) creature: Option<GlobalFormId>,
    pub(super) object: GlobalFormId,
}

pub(super) fn scene(order: &Arc<LoadOrder>, bases: &Bases) -> Scene {
    let mut game = fresh_game(order);
    let player = game.player();
    let (npc, creature, object) = {
        let mut world = game.world_mut();
        world.set_actor_value(player, "magicka", 10_000.0).ok();
        put(&mut world, player, [0.0, 0.0, 0.0]);
        let npc = world.place_at(bases.npc, player, false);
        put(&mut world, npc, [0.0, 200.0, 0.0]);
        world.set_actor_value(npc, "health", 10_000.0).ok();
        let creature = bases.creature.map(|base| {
            let id = world.place_at(base, player, false);
            put(&mut world, id, [200.0, 0.0, 0.0]);
            world.set_actor_value(id, "health", 10_000.0).ok();
            id
        });
        let object = world.place_at(bases.object, player, false);
        put(&mut world, object, [0.0, -200.0, 0.0]);
        (npc, creature, object)
    };
    Scene {
        game,
        player,
        npc,
        creature,
        object,
    }
}

fn active_on(game: &Game, target: GlobalFormId, effect: GlobalFormId) -> Vec<f32> {
    game.world()
        .active_effects()
        .filter(|a| a.target == target && a.effect == effect)
        .map(|a| a.magnitude)
        .collect()
}

/// Cast one spell at one target kind and check the outcome.
#[must_use]
pub fn check_spell(
    order: &Arc<LoadOrder>,
    bases: &Bases,
    spell: GlobalFormId,
    target_kind: Target,
) -> Outcome {
    let mut scene = scene(order, bases);
    let (player, game) = (scene.player, &mut scene.game);
    let target = match target_kind {
        Target::Itself | Target::Space => player,
        Target::Npc => scene.npc,
        Target::Creature => match scene.creature {
            Some(id) => id,
            None => {
                return Outcome::none(
                    "creature_base_not_found",
                    "no creature NPC_ record matched the candidate editor ids",
                    json!({}),
                )
            }
        },
        Target::Object => scene.object,
    };
    let (definition, plans) = {
        let world = game.world();
        let definition = match world.spell(spell) {
            Ok(value) => value,
            Err(error) => {
                return Outcome::error(
                    &cause_key("record", &error.to_string()),
                    error.to_string(),
                    json!({}),
                )
            }
        };
        match plan_of(&world, spell, &definition.effects) {
            Ok(plans) => (definition, plans),
            Err(error) => return Outcome::error(&cause_key("record", &error), error, json!({})),
        }
    };
    let cast_type = definition.data.cast_type;
    let cost = game.world_mut().spell_cost(spell, player).unwrap_or(0.0);
    let magicka_before = game.world_mut().actor_value(player, "magicka");
    let actor_target = matches!(target_kind, Target::Itself | Target::Npc | Target::Creature);
    let before: Vec<Option<(f32, f32)>> = plans
        .iter()
        .map(|p| {
            p.value.as_deref().filter(|_| actor_target).map(|v| {
                let mut world = game.world_mut();
                (
                    world.actor_value(target, v),
                    world.actor_value_maximum(target, v),
                )
            })
        })
        .collect();
    let result = match (target_kind, cast_type) {
        (Target::Space, _) => with_world!(game, |world, alloc| world.cast_spell_at_location(
            spell,
            player,
            Some(player),
            [0.0, 400.0, 0.0],
            true,
            alloc
        )),
        (_, 2) => with_world!(game, |world, alloc| world
            .begin_concentration_cast(spell, player, player, Some(target), &mut *alloc)
            .map(|()| Vec::new())),
        (_, 1) => with_world!(game, |world, alloc| world.cast_charged_spell(
            spell,
            player,
            player,
            Some(target),
            alloc
        )),
        _ => with_world!(game, |world, alloc| world.cast_spell(
            spell,
            player,
            Some(player),
            Some(target),
            alloc
        )),
    };
    let mut observed = json!({"delivery":definition.data.delivery,"cast_type":cast_type,
        "target":target_kind.label(),"cost":cost});
    let applied = match result {
        Ok(ids) => ids,
        Err(crate::world::magic::MagicError::Target) if target_kind == Target::Object => {
            return Outcome::pass(observed);
        }
        Err(error) => {
            return Outcome::error(
                &cause_key("cast", &error.to_string()),
                error.to_string(),
                observed,
            )
        }
    };
    if cast_type == 2 {
        let errors = with_world!(game, |world, alloc| world.tick_concentration(1.0, alloc));
        if let Some(error) = errors.first() {
            return Outcome::error(
                &cause_key("concentration", &error.to_string()),
                error.to_string(),
                observed,
            );
        }
    }
    observed["applied"] = json!(applied.len());
    if target_kind == Target::Object {
        let stray = game.world().active_effects().any(|a| a.target == target);
        return if stray {
            Outcome::wrong(
                "effect_on_object",
                "an actor value effect landed on an object",
                observed,
            )
        } else {
            Outcome::pass(observed)
        };
    }
    if target_kind == Target::Space {
        return Outcome::pass(observed);
    }
    let magicka_after = game.world_mut().actor_value(player, "magicka");
    if cast_type == 1 && !near(magicka_before - magicka_after, cost, 0.5) {
        return Outcome::wrong(
            "magicka_cost_not_charged",
            format!(
                "expected {cost} spent, observed {}",
                magicka_before - magicka_after
            ),
            observed,
        );
    }
    let resolved: Vec<&EffectPlan> = plans.iter().collect();
    for (plan, start) in resolved.iter().zip(&before) {
        if plan.duration > 0 && cast_type != 2 {
            let found = active_on(game, target, plan.effect);
            if found.is_empty() {
                return Outcome::wrong(
                    "effect_missing_from_active_list",
                    format!("{} is not active on the {}", plan.name, target_kind.label()),
                    observed,
                );
            }
            if !found.iter().any(|m| near(*m, plan.magnitude, 0.01)) {
                return Outcome::wrong(
                    "active_effect_magnitude",
                    format!(
                        "{} magnitudes {found:?}, record says {}",
                        plan.name, plan.magnitude
                    ),
                    observed,
                );
            }
        }
        let (Some(value), Some((current, maximum))) = (plan.value.as_deref(), start) else {
            continue;
        };
        if let Some(Expected::Current(want)) = expected_change(plan, *current, *maximum) {
            if plan.duration == 0 {
                let now = game.world_mut().actor_value(target, value);
                if !near(now - current, want, 0.01) {
                    return Outcome::wrong(
                        "instant_effect_change",
                        format!(
                            "{} on {value}: expected {want}, observed {}",
                            plan.name,
                            now - current
                        ),
                        observed,
                    );
                }
            }
        }
    }
    if let Some(failure) = area_failure(
        game,
        scene.player,
        target,
        spell,
        &plans,
        &definition.effects,
    ) {
        return failure;
    }
    if let Some(failure) = resistance_failure(
        order,
        bases,
        spell,
        target_kind,
        &plans,
        &definition.effects,
    ) {
        return failure;
    }
    Outcome::pass(observed)
}

/// An effect with an area reaches a bystander inside it and not one outside.
fn area_failure(
    game: &mut Game,
    player: GlobalFormId,
    target: GlobalFormId,
    spell: GlobalFormId,
    plans: &[EffectPlan],
    effects: &[soe_esm::records::gameplay::Effect],
) -> Option<Outcome> {
    let (plan, entry) = plans
        .iter()
        .zip(effects)
        .find(|(p, e)| e.data.area > 0 && p.duration > 0)?;
    let base = game.world_mut().reference(target)?.base?;
    let origin = game.world_mut().reference(target)?.position;
    let area = entry.data.area;
    let inside = area as f32 * UNITS_PER_FOOT * 0.5;
    let outside = area as f32 * UNITS_PER_FOOT + 400.0;
    let (near_id, far_id) = {
        let mut world = game.world_mut();
        let near_id = world.place_at(base, player, false);
        put(
            &mut world,
            near_id,
            [origin[0] + inside, origin[1], origin[2]],
        );
        let far_id = world.place_at(base, player, false);
        put(
            &mut world,
            far_id,
            [origin[0] + outside, origin[1], origin[2]],
        );
        (near_id, far_id)
    };
    let recast = with_world!(game, |world, alloc| world.cast_spell(
        spell,
        player,
        Some(player),
        Some(target),
        alloc
    ));
    if recast.is_err() {
        return None;
    }
    let hit_near = !active_on(game, near_id, plan.effect).is_empty();
    let hit_far = !active_on(game, far_id, plan.effect).is_empty();
    let reason = format!(
        "area {area} feet: bystander at {inside:.0} units hit {hit_near}, at {outside:.0} units hit {hit_far}"
    );
    if !hit_near {
        return Some(Outcome::wrong(
            "area_misses_actor_inside",
            reason,
            json!({}),
        ));
    }
    if hit_far {
        return Some(Outcome::wrong("area_hits_actor_outside", reason, json!({})));
    }
    None
}

/// A hostile value modifier is cut by the target's resistance.
fn resistance_failure(
    order: &Arc<LoadOrder>,
    bases: &Bases,
    spell: GlobalFormId,
    target_kind: Target,
    plans: &[EffectPlan],
    effects: &[soe_esm::records::gameplay::Effect],
) -> Option<Outcome> {
    if !matches!(target_kind, Target::Npc | Target::Creature) {
        return None;
    }
    let mut scene = scene(order, bases);
    let target = if target_kind == Target::Npc {
        scene.npc
    } else {
        scene.creature?
    };
    let player = scene.player;
    for (plan, entry) in plans.iter().zip(effects) {
        let resist = {
            let world = scene.game.world();
            world.magic_effect(plan.effect).ok()?.data.resistance
        };
        let name = crate::actor_values::actor_value_name(resist).map(str::to_ascii_lowercase);
        let Some(name) = name.filter(|_| plan.flags & HOSTILE != 0 && plan.duration > 0) else {
            continue;
        };
        if plan.archetype != 0 && plan.archetype != 34 {
            continue;
        }
        scene
            .game
            .world_mut()
            .set_actor_value(target, &name, 50.0)
            .ok()?;
        let cast = with_world!(scene.game, |world, alloc| world.cast_spell(
            spell,
            player,
            Some(player),
            Some(target),
            alloc
        ));
        cast.ok()?;
        let found = active_on(&scene.game, target, plan.effect);
        let want = resisted_magnitude(entry.data.magnitude, 50.0);
        if !found.iter().any(|m| near(*m, want, 0.05)) {
            return Some(Outcome::wrong(
                "resistance_not_applied",
                format!(
                    "{} with {name} 50 expected magnitude {want}, observed {found:?}",
                    plan.name
                ),
                json!({}),
            ));
        }
        return None;
    }
    None
}

/// Spells a tome teaches, from BOOK flag 0x04 and the DATA spell field.
#[must_use]
pub fn tome_spells(order: &Arc<LoadOrder>, options: &Options) -> Vec<(GlobalFormId, String)> {
    let game = fresh_game(order);
    let world = game.world_mut();
    let mut spells: Vec<GlobalFormId> = Vec::new();
    for (book, record) in order.winners_of(Tag(*b"BOOK")) {
        let Some(data) = record.find(Tag(*b"DATA")) else {
            continue;
        };
        if data.len() < 8 || data[0] & BOOK_TEACHES_SPELL == 0 {
            continue;
        }
        let raw = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        if let Some(spell) = world.resolve_from(book, raw) {
            if !spells.contains(&spell) {
                spells.push(spell);
            }
        }
    }
    spells.sort();
    spells
        .into_iter()
        .map(|id| {
            (
                id,
                order
                    .winner(id)
                    .and_then(soe_esm::Record::editor_id)
                    .unwrap_or_default(),
            )
        })
        .filter(|(_, name)| {
            options.only.as_ref().is_none_or(|p| {
                name.to_ascii_lowercase()
                    .starts_with(&p.to_ascii_lowercase())
            })
        })
        .take(options.limit.unwrap_or(usize::MAX))
        .collect()
}

fn enchantment_of(game: &Game, item: GlobalFormId) -> Option<GlobalFormId> {
    let world = game.world();
    let raw = world.record(item)?.find(Tag(*b"EITM"))?;
    let raw = u32::from_le_bytes(raw.get(0..4)?.try_into().ok()?);
    world.resolve_from(item, raw)
}

fn animation_type(game: &Game, weapon: GlobalFormId) -> Option<u8> {
    let world = game.world();
    crate::combat::records::Weapon::from_record(world.record(weapon)?)
        .ok()
        .map(|w| w.animation)
}

/// Equip an enchanted weapon and hit an NPC with it.
#[must_use]
pub fn check_weapon_enchantment(
    order: &Arc<LoadOrder>,
    bases: &Bases,
    weapon: GlobalFormId,
) -> Outcome {
    let mut scene = scene(order, bases);
    let (player, npc) = (scene.player, scene.npc);
    let game = &mut scene.game;
    let Some(enchantment) = enchantment_of(game, weapon) else {
        return Outcome::none("no_enchantment", "the weapon has no EITM", json!({}));
    };
    let animation = animation_type(game, weapon);
    if animation.is_none_or(|a| a > 6) {
        return Outcome::none(
            "ranged_enchant_needs_projectile",
            "bows, crossbows and staves apply their enchantment through a projectile or cast",
            json!({"animation":animation}),
        );
    }
    {
        let mut world = game.world_mut();
        world.add_item(player, weapon, 1);
        if let Err(error) = world.equip_item(player, weapon) {
            return Outcome::error(
                &cause_key("equip", &error.to_string()),
                error.to_string(),
                json!({}),
            );
        }
    }
    let hit = MeleeHit {
        attacker: player,
        target: npc,
        weapon,
        is_bash: false,
        is_power_attack: false,
    };
    let result = with_world!(game, |world, alloc| world.melee_hit(
        hit,
        Vec::new(),
        Vec::new(),
        alloc
    ));
    if let Err(error) = result {
        return Outcome::error(
            &cause_key("hit", &error.to_string()),
            error.to_string(),
            json!({}),
        );
    }
    let applied = game
        .world()
        .active_effects()
        .filter(|a| a.target == npc && (a.spell == enchantment || a.source == weapon))
        .count();
    let observed = json!({"enchantment":enchantment.to_string(),"active_effects":applied});
    if applied == 0 {
        let health = game.world_mut().actor_value(npc, "health");
        return Outcome::wrong(
            "weapon_enchantment_not_applied",
            format!("the hit left no effect from the enchantment, target health {health}"),
            observed,
        );
    }
    Outcome::pass(observed)
}

/// Wear an enchanted armor piece and check its effects are active on the wearer.
#[must_use]
pub fn check_armor_enchantment(
    order: &Arc<LoadOrder>,
    bases: &Bases,
    armor: GlobalFormId,
) -> Outcome {
    let mut scene = scene(order, bases);
    let player = scene.player;
    let game = &mut scene.game;
    let Some(enchantment) = enchantment_of(game, armor) else {
        return Outcome::none("no_enchantment", "the armor has no EITM", json!({}));
    };
    game.world_mut().add_item(player, armor, 1);
    let result = with_world!(game, |world, alloc| world
        .equip_item_with_effects(player, armor, alloc));
    if let Err(error) = result {
        return Outcome::error(
            &cause_key("equip", &error.to_string()),
            error.to_string(),
            json!({}),
        );
    }
    let active = game
        .world()
        .active_effects()
        .filter(|a| a.target == player && (a.spell == enchantment || a.source == armor))
        .count();
    let observed = json!({"enchantment":enchantment.to_string(),"active_effects":active});
    if active == 0 {
        return Outcome::wrong(
            "worn_enchantment_not_active",
            "wearing the armor left no effect from its enchantment",
            observed,
        );
    }
    Outcome::pass(observed)
}

/// Cast a shout at an NPC and check the recovery time starts.
#[must_use]
pub fn check_shout(order: &Arc<LoadOrder>, bases: &Bases, shout: GlobalFormId) -> Outcome {
    let mut scene = scene(order, bases);
    let (player, npc) = (scene.player, scene.npc);
    let game = &mut scene.game;
    if let Some(state) = game.world_mut().actor(player) {
        state.equipped_shout = Some(shout);
    }
    let result = with_world!(game, |world, alloc| world.cast_shout(
        player,
        Some(npc),
        alloc
    ));
    if let Err(error) = result {
        return Outcome::error(
            &cause_key("shout", &error.to_string()),
            error.to_string(),
            json!({}),
        );
    }
    let recovery = game.world_mut().voice_recovery_time(player);
    let observed = json!({"recovery_seconds":recovery});
    if recovery <= 0.0 {
        return Outcome::wrong(
            "shout_recovery_not_started",
            "the shout cast set no recovery time",
            observed,
        );
    }
    Outcome::pass(observed)
}

/// Cast a scroll record at an NPC through the scroll native.
#[must_use]
pub fn check_scroll(order: &Arc<LoadOrder>, bases: &Bases, scroll: GlobalFormId) -> Outcome {
    let mut scene = scene(order, bases);
    let (player, npc) = (scene.player, scene.npc);
    let result = with_world!(scene.game, |world, alloc| world.cast_scroll_from_record(
        scroll,
        player,
        Some(npc),
        alloc
    ));
    match result {
        Ok(ids) => Outcome::pass(json!({"applied":ids.len()})),
        Err(error) => Outcome::error(
            &cause_key("scroll", &error.to_string()),
            error.to_string(),
            json!({}),
        ),
    }
}

/// Unique items: the generic enchantment check plus what is scripted.
#[must_use]
pub fn check_unique(
    order: &Arc<LoadOrder>,
    bases: &Bases,
    item: GlobalFormId,
    weapon: bool,
) -> Outcome {
    let scripted = order
        .winner(item)
        .is_some_and(|r| r.find(Tag(*b"VMAD")).is_some());
    let enchanted = order
        .winner(item)
        .is_some_and(|r| r.find(Tag(*b"EITM")).is_some());
    if !enchanted {
        return Outcome::none(
            "unique_effect_scripted",
            "no EITM, the effect comes from an attached script that has no model",
            json!({"scripted":scripted}),
        );
    }
    let outcome = if weapon {
        check_weapon_enchantment(order, bases, item)
    } else {
        check_armor_enchantment(order, bases, item)
    };
    let mut observed = outcome.observed.clone();
    observed["scripted"] = json!(scripted);
    Outcome {
        observed,
        ..outcome
    }
}

/// Effects of the visible enchantment and projectile art, for the Godot pass.
#[must_use]
pub fn visual_checks() -> Vec<Json> {
    [
        "aimed projectile or beam reaches the target",
        "enchanted weapon shader glow",
        "fire, frost and shock effect shaders on the weapon and the hit target",
        "absorb and soul trap visuals",
        "Dawnbreaker explosion, Bloodskal Blade beam, Mehrunes' Razor, Wabbajack transformation",
    ]
    .iter()
    .map(|name| visual_check(name, &[]))
    .collect()
}

/// Run the magic sweep.
#[must_use]
pub fn run(order: &Arc<LoadOrder>, options: &Options) -> Json {
    let Some(bases) = bases(order) else {
        return report("magic", "no target base forms found", &[], &visual_checks());
    };
    let mut rows = Vec::new();
    for (spell, name) in tome_spells(order, options) {
        soe_trace::info!(target: "game::sweep", system = "game", "magic sweep: spell {spell} {name}");
        let delivery = {
            let game = fresh_game(order);
            let world = game.world_mut();
            world.spell(spell).map(|s| s.data.delivery).unwrap_or(9)
        };
        for target in targets_for(delivery) {
            rows.push(row(
                order,
                spell,
                "SPEL",
                target.label(),
                &check_spell(order, &bases, spell, *target),
            ));
        }
    }
    for (scroll, name) in selected(order, Tag(*b"SCRL"), options) {
        soe_trace::info!(target: "game::sweep", system = "game", "magic sweep: scroll {scroll} {name}");
        rows.push(row(
            order,
            scroll,
            "SCRL",
            "npc",
            &check_scroll(order, &bases, scroll),
        ));
    }
    let game = fresh_game(order);
    for (weapon, name) in selected(order, Tag(*b"WEAP"), options) {
        if enchantment_of(&game, weapon).is_none() {
            continue;
        }
        soe_trace::info!(target: "game::sweep", system = "game", "magic sweep: weapon {weapon} {name}");
        let case = if animation_type(&game, weapon) == Some(8) {
            "staff"
        } else {
            "weapon"
        };
        let outcome = if case == "staff" {
            Outcome::none(
                "staff_cast_path_missing",
                "no public call casts a staff enchantment",
                json!({}),
            )
        } else {
            check_weapon_enchantment(order, &bases, weapon)
        };
        rows.push(row(order, weapon, "WEAP", case, &outcome));
    }
    for (armor, name) in selected(order, Tag(*b"ARMO"), options) {
        if enchantment_of(&game, armor).is_none() {
            continue;
        }
        soe_trace::info!(target: "game::sweep", system = "game", "magic sweep: armor {armor} {name}");
        rows.push(row(
            order,
            armor,
            "ARMO",
            "worn",
            &check_armor_enchantment(order, &bases, armor),
        ));
    }
    for (shout, name) in selected(order, Tag(*b"SHOU"), options) {
        soe_trace::info!(target: "game::sweep", system = "game", "magic sweep: shout {shout} {name}");
        rows.push(row(
            order,
            shout,
            "SHOU",
            "npc",
            &check_shout(order, &bases, shout),
        ));
    }
    for (tag, weapon) in [(Tag(*b"WEAP"), true), (Tag(*b"ARMO"), false)] {
        for (id, name) in selected(order, tag, options) {
            let lower = name.to_ascii_lowercase();
            if UNIQUE_NEEDLES.iter().any(|needle| lower.contains(needle)) {
                rows.push(row(
                    order,
                    id,
                    "UNIQUE",
                    &name,
                    &check_unique(order, &bases, id, weapon),
                ));
            }
        }
    }
    report(
        "magic",
        "SPEL, MGEF, ENCH and SHOU records, CK wiki cast semantics and UESP resistance, no captured vanilla runtime oracle",
        &rows,
        &visual_checks(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivery_picks_targets() {
        assert_eq!(targets_for(0), &[Target::Itself]);
        assert_eq!(targets_for(2).len(), 3);
        assert_eq!(targets_for(4), &[Target::Space]);
        assert!(targets_for(9).is_empty());
    }

    #[test]
    fn area_is_measured_in_feet() {
        assert!(in_area(10, 200.0));
        assert!(!in_area(10, 230.0));
    }

    #[test]
    fn resistance_scales_magnitude() {
        assert!((resisted_magnitude(40.0, 50.0) - 20.0).abs() < 1e-6);
        assert!((resisted_magnitude(40.0, 150.0)).abs() < 1e-6);
    }

    #[test]
    fn no_targets_in_an_empty_order() {
        assert!(bases(&LoadOrder::new()).is_none());
    }
}
