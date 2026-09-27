// SPDX-License-Identifier: GPL-3.0-or-later

//! COBJ creation and ENCH disenchant transactions from xEdit and UESP layouts.
//! Bench WBDT selects creation versus improvement. Inventory events follow CK.
//! Item improvement and other crafting menus have separate execution paths.

use crate::{conditions, Game, World};
use soe_esm::{records::gameplay::Recipe, GlobalFormId, Record, Tag};
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
}

/// The committed transaction, with gross consumption in resolved FormID order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub recipe: GlobalFormId,
    pub consumed: BTreeMap<GlobalFormId, i32>,
    pub created: GlobalFormId,
    pub count: i32,
}

/// The effects learned and exact item consumed by disenchanting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisenchantOutcome {
    pub item: GlobalFormId,
    pub enchantment: GlobalFormId,
    pub learned_effects: Vec<GlobalFormId>,
}

/// A rejected disenchant leaves learned effects, inventory and events unchanged.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum DisenchantError {
    #[error("actor {0} is missing, disabled, dead or unconscious")]
    Actor(GlobalFormId),
    #[error("item {0} is not a disenchantable weapon or armor")]
    Item(GlobalFormId),
    #[error("selected item has no eligible enchantment")]
    Ineligible,
    #[error("enchantment {0} is malformed or has unsupported effects")]
    Enchantment(GlobalFormId),
    #[error("inventory selection: {0}")]
    Inventory(#[from] crate::inventory::Error),
}

struct Prepared {
    outcome: Outcome,
    inventory: crate::inventory::Inventory,
}

impl Game {
    /// Learn unknown magic effects from one selected item and consume that item.
    ///
    /// # Errors
    /// Invalid actor, item instance, enchantment record or inventory selection.
    pub fn disenchant(
        &mut self,
        actor: GlobalFormId,
        item: GlobalFormId,
        source: crate::inventory::StackSource,
    ) -> Result<DisenchantOutcome, DisenchantError> {
        let mut world = self.world.borrow_mut();
        if world
            .reference(actor)
            .is_none_or(|reference| !reference.enabled || reference.deleted)
            || world
                .actor(actor)
                .is_none_or(|state| state.dead || state.unconscious)
        {
            return Err(DisenchantError::Actor(actor));
        }
        let Some(item_record) = world
            .record(item)
            .filter(|record| {
                !record.is_deleted() && matches!(record.tag.as_bytes(), b"ARMO" | b"WEAP")
            })
            .cloned()
        else {
            return Err(DisenchantError::Item(item));
        };
        if let Some(keyword) = world.form("MagicDisallowEnchanting") {
            if world.has_keyword(item, keyword) {
                return Err(DisenchantError::Ineligible);
            }
        }

        let selected = world.select_item(actor, item, source, 1)?;
        let selected_stack = world
            .reference(actor)
            .ok_or(DisenchantError::Actor(actor))?
            .inventory
            .selected_stack(&selected)?;
        let instance_enchantment = match selected_stack.and_then(|stack| stack.extra(155)) {
            None => None,
            Some(extra) => match extra.fields.as_slice() {
                [crate::inventory::Field::Form(Some(enchantment)), crate::inventory::Field::Short(_)] => {
                    Some(*enchantment)
                }
                _ => return Err(DisenchantError::Ineligible),
            },
        };
        let enchantment = instance_enchantment.or_else(|| {
            item_record
                .find(Tag(*b"EITM"))
                .filter(|bytes| bytes.len() == 4)
                .and_then(|bytes| {
                    world.resolve_from(item, u32::from_le_bytes(bytes.try_into().ok()?))
                })
        });
        let enchantment = enchantment.ok_or(DisenchantError::Ineligible)?;
        let record = world
            .record(enchantment)
            .filter(|record| !record.is_deleted() && record.tag == Tag(*b"ENCH"))
            .ok_or(DisenchantError::Enchantment(enchantment))?;
        let (kind, raw_effects) =
            disenchant_effects(record).ok_or(DisenchantError::Enchantment(enchantment))?;
        if kind != 6 {
            return Err(DisenchantError::Ineligible);
        }
        let mut effects = Vec::with_capacity(raw_effects.len());
        for raw in raw_effects {
            let effect = world
                .resolve_from(enchantment, raw)
                .filter(|id| world.tag_of(*id) == Some(Tag(*b"MGEF")))
                .ok_or(DisenchantError::Enchantment(enchantment))?;
            if !effects.contains(&effect) && !world.learned_enchantment_effects.contains(&effect) {
                effects.push(effect);
            }
        }
        if effects.is_empty() {
            return Err(DisenchantError::Ineligible);
        }

        let mut inventory = world
            .reference(actor)
            .ok_or(DisenchantError::Actor(actor))?
            .inventory
            .clone();
        inventory.take_selected(&selected)?;
        world
            .reference(actor)
            .ok_or(DisenchantError::Actor(actor))?
            .inventory = inventory;
        world
            .learned_enchantment_effects
            .extend(effects.iter().copied());
        let vm = &mut self.vm;
        let alloc = &mut |name: &str, class: &str| vm.create_object_of(name, class);
        world.inventory_event(actor, item, -1, None, alloc);
        Ok(DisenchantOutcome {
            item,
            enchantment,
            learned_effects: effects,
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

fn inventory_form(world: &World, id: GlobalFormId) -> bool {
    world.is_inventory_base(id)
}

fn disenchant_effects(record: &Record) -> Option<(u32, Vec<u32>)> {
    let data = record.find(Tag(*b"ENIT"))?;
    if !matches!(data.len(), 32 | 36) {
        return None;
    }
    let kind = u32::from_le_bytes(data.get(20..24)?.try_into().ok()?);
    let mut effects = Vec::new();
    let mut pending = false;
    for subrecord in &record.subrecords {
        match subrecord.tag.as_bytes() {
            b"EFID" => {
                if pending || subrecord.data.len() != 4 {
                    return None;
                }
                effects.push(u32::from_le_bytes(
                    subrecord.data.as_slice().try_into().ok()?,
                ));
                pending = true;
            }
            b"EFIT" => {
                if !pending || subrecord.data.len() != 12 {
                    return None;
                }
                pending = false;
            }
            _ => {}
        }
    }
    (!pending && !effects.is_empty()).then_some((kind, effects))
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
