// SPDX-License-Identifier: GPL-3.0-or-later

//! COBJ creation transactions from xEdit/UESP layouts and skymp CraftService.
//! Bench WBDT selects creation versus improvement. Inventory events follow CK.
//! Tempering applies COBJ improvement recipes to exact item instances.

use crate::{
    conditions,
    inventory::{Error as InventoryError, Extra, Field, Inventory, StackSource},
    Game, World,
};
use soe_esm::{records::gameplay::Recipe, GlobalFormId, Tag};
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
    #[error("item instance: {0}")]
    ItemInstance(#[from] InventoryError),
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
    #[error("item {0} cannot be tempered")]
    TemperItem(GlobalFormId),
    #[error("recipe {0} does not describe one temperable item")]
    TemperRecipe(GlobalFormId),
    #[error("item {0} has no higher supported tempering quality")]
    TemperQuality(GlobalFormId),
}

/// The committed transaction, with gross consumption in resolved FormID order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub recipe: GlobalFormId,
    pub consumed: BTreeMap<GlobalFormId, i32>,
    pub created: GlobalFormId,
    pub count: i32,
}

/// The item and ingredients changed by one committed tempering operation.
#[derive(Debug, Clone, PartialEq)]
pub struct TemperOutcome {
    pub recipe: GlobalFormId,
    pub item: GlobalFormId,
    pub consumed: BTreeMap<GlobalFormId, i32>,
    pub previous_quality: f32,
    pub quality: f32,
}

struct Prepared {
    outcome: Outcome,
    inventory: crate::inventory::Inventory,
}

impl Game {
    /// Improve one weapon or armor instance at the matching smithing bench.
    ///
    /// The target item's COBJ ingredient is selected as one exact instance.
    /// Other ingredients are consumed in the same inventory transaction.
    ///
    /// # Errors
    /// Rejects unsupported records, benches, conditions, missing ingredients,
    /// ambiguous item stacks and items that cannot be improved further.
    pub fn temper(
        &mut self,
        actor: GlobalFormId,
        bench: GlobalFormId,
        recipe_id: GlobalFormId,
    ) -> Result<TemperOutcome, Error> {
        let (outcome, inventory) = self.prepare_temper(actor, bench, recipe_id)?;
        let vm = &mut self.vm;
        let mut world = self.world.borrow_mut();
        let holder = world.reference(actor).ok_or(Error::Actor(actor))?;
        holder.inventory = inventory;
        let alloc = &mut |name: &str, class: &str| vm.create_object_of(name, class);
        for (item, count) in &outcome.consumed {
            world.inventory_event(actor, *item, -*count, None, alloc);
        }
        Ok(outcome)
    }

    fn prepare_temper(
        &mut self,
        actor: GlobalFormId,
        bench: GlobalFormId,
        recipe_id: GlobalFormId,
    ) -> Result<(TemperOutcome, crate::inventory::Inventory), Error> {
        let mut world = self.world.borrow_mut();
        if world
            .reference(actor)
            .is_none_or(|r| !r.enabled || r.deleted)
            || world.actor(actor).is_none_or(|a| a.dead || a.unconscious)
        {
            return Err(Error::Actor(actor));
        }
        let record = world
            .record(recipe_id)
            .filter(|record| !record.is_deleted() && record.tag == Tag(*b"COBJ"))
            .ok_or(Error::Recipe(recipe_id))?;
        let recipe = Recipe::from_record(record)?;
        let target_raw = recipe
            .created_object
            .filter(|raw| *raw != 0)
            .ok_or(Error::Recipe(recipe_id))?;
        if recipe.created_count.is_some_and(|count| count != 1) {
            return Err(Error::TemperRecipe(recipe_id));
        }
        let target = conditions::resolve(&world, recipe_id, target_raw, None)?;
        let target_tag = world.record(target).map(|record| record.tag);
        if !target_tag.is_some_and(|tag| tag == Tag(*b"WEAP") || tag == Tag(*b"ARMO"))
            || !world.is_inventory_base(target)
        {
            return Err(Error::TemperItem(target));
        }
        let keyword = recipe
            .workbench_keyword
            .filter(|raw| *raw != 0)
            .ok_or(Error::Recipe(recipe_id))?;
        let keyword = conditions::resolve(&world, recipe_id, keyword, Some(*b"KYWD"))?;
        validate_temper_bench(&mut world, bench, keyword)?;

        let mut target_ingredient = false;
        let mut consumed = BTreeMap::new();
        for ingredient in recipe.ingredients {
            if ingredient.ownership.is_some() {
                return Err(Error::IngredientInstance);
            }
            if ingredient.count <= 0 {
                return Err(Error::Quantity);
            }
            let item = conditions::resolve(&world, recipe_id, ingredient.item, None)?;
            if !world.is_inventory_base(item) {
                return Err(Error::Recipe(recipe_id));
            }
            if item == target {
                if target_ingredient || ingredient.count != 1 {
                    return Err(Error::TemperRecipe(recipe_id));
                }
                target_ingredient = true;
            } else {
                let total = consumed.entry(item).or_insert(0_i32);
                *total = total.checked_add(ingredient.count).ok_or(Error::Quantity)?;
            }
        }
        if !target_ingredient {
            return Err(Error::TemperRecipe(recipe_id));
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
        let source = tempering_source(&inventory, target)?;
        let selected = inventory.select(target, source, 1)?;
        let enchanted = inventory
            .selected_stack(&selected)?
            .is_some_and(|stack| stack.extra(155).is_some());
        let context = conditions::Context {
            owner: recipe_id,
            subject: actor,
            target: Some(target),
            quest: None,
            tempering_enchanted: Some(enchanted),
            tempering_item: Some(target),
        };
        if !conditions::evaluate(&mut world, &self.vm, context, &recipe.conditions)? {
            return Err(Error::Ineligible);
        }
        let previous_quality = inventory
            .selected_stack(&selected)?
            .and_then(|stack| stack.extra(37))
            .and_then(|extra| match extra.fields.as_slice() {
                [crate::inventory::Field::FloatBits(bits)] => Some(f32::from_bits(*bits)),
                _ => None,
            })
            .unwrap_or(1.0);
        if !previous_quality.is_finite() {
            return Err(Error::TemperQuality(target));
        }
        let smithing = world.actor_value(actor, "smithing");
        let quality = (1.0 + smithing.max(10.0) / 100.0)
            .min(1.6)
            .max(previous_quality);
        if quality <= previous_quality {
            return Err(Error::TemperQuality(target));
        }
        inventory.update_selected(
            &selected,
            vec![Extra {
                kind: 37,
                fields: vec![Field::FloatBits(quality.to_bits())],
            }],
        )?;
        Ok((
            TemperOutcome {
                recipe: recipe_id,
                item: target,
                consumed,
                previous_quality,
                quality,
            },
            inventory,
        ))
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
            tempering_item: None,
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

fn tempering_source(inventory: &Inventory, target: GlobalFormId) -> Result<StackSource, Error> {
    if inventory.count(target) <= 0 {
        return Err(Error::MissingIngredient {
            item: target,
            needed: 1,
            available: inventory.count(target),
        });
    }
    let stacks = inventory.stacks(target);
    let represented: i64 = stacks
        .iter()
        .map(|stack| i64::from(stack.count().max(0)))
        .sum();
    let plain = i64::from(inventory.count(target)) - represented;
    let mut selected = (1.0_f32, StackSource::Plain);
    if plain <= 0 {
        selected.0 = f32::INFINITY;
    }
    for (index, stack) in stacks.iter().enumerate() {
        let quality = stack
            .extra(37)
            .map(|extra| match extra.fields.as_slice() {
                [Field::FloatBits(bits)] => Ok(f32::from_bits(*bits)),
                _ => Err(Error::TemperQuality(target)),
            })
            .transpose()?
            .unwrap_or(1.0);
        if !quality.is_finite() {
            return Err(Error::TemperQuality(target));
        }
        if quality < selected.0 {
            selected = (quality, StackSource::Extra(index));
        }
    }
    Ok(selected.1)
}

fn inventory_form(world: &World, id: GlobalFormId) -> bool {
    world.is_inventory_base(id)
}

fn validate_temper_bench(
    world: &mut World,
    bench: GlobalFormId,
    keyword: GlobalFormId,
) -> Result<(), Error> {
    let reference = world
        .reference(bench)
        .filter(|reference| reference.enabled && !reference.deleted)
        .ok_or(Error::Bench(bench))?;
    let base = reference.base.ok_or(Error::Bench(bench))?;
    let record = world
        .record(base)
        .filter(|record| !record.is_deleted())
        .ok_or(Error::Bench(bench))?;
    match record.tag.as_bytes() {
        b"FURN" => {
            let data = record
                .find(Tag(*b"WBDT"))
                .filter(|data| data.len() == 2)
                .ok_or(Error::Bench(bench))?;
            if data[0] != 2 {
                return Err(Error::BenchType(data[0]));
            }
        }
        b"ACTI" => {}
        _ => return Err(Error::Bench(bench)),
    }
    let valid_keyword = [
        "CraftingSmithingArmorTable",
        "CraftingSmithingSharpeningWheel",
    ]
    .into_iter()
    .filter_map(|name| world.form(name))
    .any(|form| form == keyword);
    if !valid_keyword || !world.has_keyword(base, keyword) {
        return Err(Error::Bench(bench));
    }
    Ok(())
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
