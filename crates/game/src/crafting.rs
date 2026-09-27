// SPDX-License-Identifier: GPL-3.0-or-later

//! COBJ creation transactions from xEdit/UESP layouts and skymp CraftService.
//! Bench WBDT selects creation versus improvement. Inventory events follow CK.
//! Item improvement and the other crafting menus have separate execution paths.

use crate::{conditions, Game, World};
use soe_esm::{
    plugin::Subrecord,
    records::gameplay::{Ingestible, Recipe},
    GlobalFormId, Record, Tag,
};
use std::collections::BTreeMap;

/// A rejected craft leaves inventory quantities and the event queue unchanged.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("actor {0} is missing, disabled, dead or unconscious")]
    Actor(GlobalFormId),
    #[error("bench {0} is missing, disabled or does not support this recipe")]
    Bench(GlobalFormId),
    #[error("bench type {0} does not create objects")]
    BenchType(u8),
    #[error("recipe {0} is missing, deleted or incomplete")]
    Recipe(GlobalFormId),
    #[error("recipe fields: {0}")]
    Format(#[from] soe_esm::Error),
    #[error("recipe condition: {0}")]
    Condition(#[from] conditions::Error),
    #[error("recipe conditions are false")]
    Ineligible,
    #[error("recipe requires unsupported ingredient instance data")]
    IngredientInstance,
    #[error("invalid or overflowing recipe quantity")]
    Quantity,
    #[error("need {needed} of {item}, have {available}")]
    MissingIngredient {
        item: GlobalFormId,
        needed: i32,
        available: i32,
    },
    #[error("alchemy requires exactly two ingredients")]
    AlchemyInputs,
    #[error("ingredient {0} is missing, deleted or malformed")]
    Ingredient(GlobalFormId),
    #[error("the ingredients have no shared eligible effects")]
    NoSharedEffects,
}

/// A potion created from two ingredients and their common eligible effects.
#[derive(Debug, Clone, PartialEq)]
pub struct AlchemyOutcome {
    pub consumed: BTreeMap<GlobalFormId, i32>,
    pub created: GlobalFormId,
    pub effects: Vec<GlobalFormId>,
}

/// The committed transaction, with gross consumption in resolved FormID order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub recipe: GlobalFormId,
    pub consumed: BTreeMap<GlobalFormId, i32>,
    pub created: GlobalFormId,
    pub count: i32,
}

struct Prepared {
    outcome: Outcome,
    inventory: crate::inventory::Inventory,
}

impl Game {
    /// Brew one potion from two ingredients at an alchemy station.
    ///
    /// # Errors
    ///
    /// Rejects invalid ingredients, station or effect conditions, missing
    /// inventory, and combinations with no shared eligible effects.
    pub fn brew_potion(
        &mut self,
        actor: GlobalFormId,
        bench: GlobalFormId,
        ingredients: &[GlobalFormId],
    ) -> Result<AlchemyOutcome, Error> {
        if ingredients.len() != 2 {
            return Err(Error::AlchemyInputs);
        }
        let mut world = self.world.borrow_mut();
        if world
            .reference(actor)
            .is_none_or(|r| !r.enabled || r.deleted)
            || world.actor(actor).is_none_or(|a| a.dead || a.unconscious)
        {
            return Err(Error::Actor(actor));
        }
        validate_alchemy_bench(&mut world, bench)?;
        let mut definitions = Vec::with_capacity(2);
        for &id in ingredients {
            let record = world
                .record(id)
                .filter(|record| !record.is_deleted() && record.tag == Tag(*b"INGR"))
                .ok_or(Error::Ingredient(id))?
                .clone();
            definitions.push((
                id,
                ingredient_definition(record).map_err(|_| Error::Ingredient(id))?,
            ));
        }
        let mut effects = Vec::new();
        for first in &definitions[0].1.effects {
            let Some(first_effect) = world.resolve_from(definitions[0].0, first.base_effect) else {
                return Err(Error::Ingredient(definitions[0].0));
            };
            let Some(second) = definitions[1].1.effects.iter().find(|candidate| {
                world.resolve_from(definitions[1].0, candidate.base_effect) == Some(first_effect)
            }) else {
                continue;
            };
            let effect = first_effect;
            let mut eligible = true;
            for (owner, entry) in [(definitions[0].0, first), (definitions[1].0, second)] {
                let context = conditions::Context {
                    owner,
                    subject: actor,
                    target: None,
                    quest: None,
                    tempering_enchanted: None,
                };
                if !conditions::evaluate(&mut world, &self.vm, context, &entry.conditions)? {
                    eligible = false;
                    break;
                }
            }
            if eligible {
                effects.push((
                    effect,
                    first.data.magnitude,
                    first.data.area,
                    first.data.duration,
                ));
            }
        }
        if effects.is_empty() {
            return Err(Error::NoSharedEffects);
        }
        let effect_ids = effects.iter().map(|entry| entry.0).collect();
        let mut consumed = BTreeMap::new();
        for &ingredient in ingredients {
            *consumed.entry(ingredient).or_insert(0) += 1;
        }
        let mut inventory = world
            .reference(actor)
            .ok_or(Error::Actor(actor))?
            .inventory
            .clone();
        for (&item, &needed) in &consumed {
            let available = inventory.count(item);
            if available < needed {
                return Err(Error::MissingIngredient {
                    item,
                    needed,
                    available,
                });
            }
            inventory.take(item, needed);
        }
        let created = world.create_brewed_potion(effects);
        inventory.add(created, 1).map_err(|_| Error::Quantity)?;
        world.reference(actor).ok_or(Error::Actor(actor))?.inventory = inventory;
        let vm = &mut self.vm;
        let alloc = &mut |name: &str, class: &str| vm.create_object_of(name, class);
        for (item, count) in &consumed {
            world.inventory_event(actor, *item, -*count, None, alloc);
        }
        world.inventory_event(actor, created, 1, None, alloc);
        Ok(AlchemyOutcome {
            consumed,
            created,
            effects: effect_ids,
        })
    }

    /// Test this recipe against current world and VM state without spending ingredients.
    ///
    /// # Errors
    ///
    /// Returns the same eligibility, context and quantity errors as craft.
    pub fn check_craft(
        &mut self,
        actor: GlobalFormId,
        bench: GlobalFormId,
        recipe: GlobalFormId,
    ) -> Result<Outcome, Error> {
        self.prepare_craft(actor, bench, recipe)
            .map(|prepared| prepared.outcome)
    }

    /// Create one recipe batch and queue inventory events. The next pump runs handlers.
    /// Rechecks eligibility each time, including after a previous UI preview.
    ///
    /// # Errors
    ///
    /// Rejects incomplete recipes, unsupported bench modes, missing ingredients,
    /// unsupported conditions and quantities that do not fit the inventory model.
    pub fn craft(
        &mut self,
        actor: GlobalFormId,
        bench: GlobalFormId,
        recipe: GlobalFormId,
    ) -> Result<Outcome, Error> {
        let prepared = self.prepare_craft(actor, bench, recipe)?;
        let vm = &mut self.vm;
        let mut world = self.world.borrow_mut();
        let holder = world.reference(actor).ok_or(Error::Actor(actor))?;
        holder.inventory = prepared.inventory;
        let alloc = &mut |name: &str, class: &str| vm.create_object_of(name, class);
        for (item, count) in &prepared.outcome.consumed {
            world.inventory_event(actor, *item, -*count, None, alloc);
        }
        world.inventory_event(
            actor,
            prepared.outcome.created,
            prepared.outcome.count,
            None,
            alloc,
        );
        Ok(prepared.outcome)
    }

    fn prepare_craft(
        &mut self,
        actor: GlobalFormId,
        bench: GlobalFormId,
        recipe_id: GlobalFormId,
    ) -> Result<Prepared, Error> {
        let mut world = self.world.borrow_mut();
        if world
            .reference(actor)
            .is_none_or(|r| !r.enabled || r.deleted)
            || world.actor(actor).is_none_or(|a| a.dead || a.unconscious)
        {
            return Err(Error::Actor(actor));
        }
        let rec = world
            .record(recipe_id)
            .filter(|rec| !rec.is_deleted() && rec.tag == Tag(*b"COBJ"))
            .ok_or(Error::Recipe(recipe_id))?;
        let recipe = Recipe::from_record(rec)?;
        let output = recipe
            .created_object
            .filter(|raw| *raw != 0)
            .ok_or(Error::Recipe(recipe_id))?;
        let created = conditions::resolve(&world, recipe_id, output, None)?;
        if !inventory_form(&world, created) {
            return Err(Error::Recipe(recipe_id));
        }
        let count = i32::from(
            recipe
                .created_count
                .filter(|n| *n > 0)
                .ok_or(Error::Quantity)?,
        );
        let keyword = recipe
            .workbench_keyword
            .filter(|raw| *raw != 0)
            .ok_or(Error::Recipe(recipe_id))?;
        let keyword = conditions::resolve(&world, recipe_id, keyword, Some(*b"KYWD"))?;
        validate_bench(&mut world, bench, keyword)?;
        let context = conditions::Context {
            owner: recipe_id,
            subject: actor,
            target: None,
            quest: None,
            tempering_enchanted: None,
        };
        if !conditions::evaluate(&mut world, &self.vm, context, &recipe.conditions)? {
            return Err(Error::Ineligible);
        }
        let mut consumed: BTreeMap<GlobalFormId, i32> = BTreeMap::new();
        for ingredient in recipe.ingredients {
            if ingredient.ownership.is_some() {
                return Err(Error::IngredientInstance);
            }
            if ingredient.count <= 0 {
                return Err(Error::Quantity);
            }
            let item = conditions::resolve(&world, recipe_id, ingredient.item, None)?;
            if !inventory_form(&world, item) {
                return Err(Error::Recipe(recipe_id));
            }
            let total = consumed.entry(item).or_default();
            *total = total.checked_add(ingredient.count).ok_or(Error::Quantity)?;
        }
        let mut inventory = world
            .reference(actor)
            .ok_or(Error::Actor(actor))?
            .inventory
            .clone();
        for (&item, &needed) in &consumed {
            let available = inventory.count(item);
            if available < needed {
                return Err(Error::MissingIngredient {
                    item,
                    needed,
                    available,
                });
            }
            inventory.take(item, needed);
        }
        inventory.add(created, count).map_err(|_| Error::Quantity)?;
        Ok(Prepared {
            outcome: Outcome {
                recipe: recipe_id,
                consumed,
                created,
                count,
            },
            inventory,
        })
    }
}

fn ingredient_definition(mut record: Record) -> Result<Ingestible, soe_esm::Error> {
    record.tag = Tag(*b"ALCH");
    record.subrecords.retain(|sub| sub.tag != Tag(*b"ENIT"));
    record.subrecords.push(Subrecord {
        tag: Tag(*b"ENIT"),
        data: vec![0; 20],
    });
    Ingestible::from_record(&record, false)
}

fn validate_alchemy_bench(world: &mut World, bench: GlobalFormId) -> Result<(), Error> {
    let reference = world
        .reference(bench)
        .filter(|r| r.enabled && !r.deleted)
        .ok_or(Error::Bench(bench))?;
    let base = reference.base.ok_or(Error::Bench(bench))?;
    let record = world
        .record(base)
        .filter(|r| !r.is_deleted())
        .ok_or(Error::Bench(bench))?;
    match record.tag.as_bytes() {
        b"FURN" => {
            let data = record
                .find(Tag(*b"WBDT"))
                .filter(|data| data.len() == 2)
                .ok_or(Error::Bench(bench))?;
            if data[0] != 4 {
                return Err(Error::BenchType(data[0]));
            }
        }
        b"ACTI" => {}
        _ => return Err(Error::Bench(bench)),
    }
    let keyword = world.form("CraftingAlchemy").ok_or(Error::Bench(bench))?;
    if !world.has_keyword(base, keyword) {
        return Err(Error::Bench(bench));
    }
    Ok(())
}

fn inventory_form(world: &World, id: GlobalFormId) -> bool {
    world.is_inventory_base(id)
}

fn validate_bench(
    world: &mut World,
    bench: GlobalFormId,
    keyword: GlobalFormId,
) -> Result<(), Error> {
    let reference = world
        .reference(bench)
        .filter(|r| r.enabled && !r.deleted)
        .ok_or(Error::Bench(bench))?;
    let base = reference.base.ok_or(Error::Bench(bench))?;
    let record = world
        .record(base)
        .filter(|r| !r.is_deleted())
        .ok_or(Error::Bench(bench))?;
    match record.tag.as_bytes() {
        b"FURN" => {
            let data = record
                .find(Tag(*b"WBDT"))
                .filter(|data| data.len() == 2)
                .ok_or(Error::Bench(bench))?;
            if data[0] != 1 {
                return Err(Error::BenchType(data[0]));
            }
        }
        b"ACTI" => {}
        _ => return Err(Error::Bench(bench)),
    }
    for name in [
        "CraftingSmithingArmorTable",
        "CraftingSmithingSharpeningWheel",
    ] {
        if world.form(name) == Some(keyword) {
            return Err(Error::BenchType(2));
        }
    }
    if !world.has_keyword(base, keyword) {
        return Err(Error::Bench(bench));
    }
    Ok(())
}
