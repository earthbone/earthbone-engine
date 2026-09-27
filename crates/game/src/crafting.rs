// SPDX-License-Identifier: GPL-3.0-or-later

//! COBJ creation transactions from xEdit/UESP layouts and skymp CraftService.
//! Bench WBDT selects creation versus improvement. Inventory events follow CK.
//! Item improvement and the other crafting menus have separate execution paths.

use crate::inventory::{Error as InventoryError, Extra, Field, Selection};
use crate::{conditions, Game, World};
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
    #[error("enchanting actor {0} is missing, disabled, dead or unconscious")]
    EnchantActor(GlobalFormId),
    #[error("enchanting item must be one unenchanted weapon or armor instance")]
    EnchantItem,
    #[error("enchantment {0} is invalid or not in the supplied learned effects")]
    Enchantment(GlobalFormId),
    #[error("soul gem selection is not one valid filled soul gem")]
    SoulGem,
    #[error("inventory selection: {0}")]
    Inventory(#[from] InventoryError),
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
    /// Enchant one selected weapon or armor instance and consume one selected filled soul gem.
    /// `learned_enchantments` is the caller's learned-effect set until the
    /// runtime has an owning learned-enchantment store.
    ///
    /// # Errors
    /// Rejects an invalid actor, selection, soul gem, target item or unknown enchantment.
    pub fn enchant_item(
        &mut self,
        actor: GlobalFormId,
        item: &Selection,
        soul_gem: &Selection,
        enchantment: GlobalFormId,
        learned_enchantments: &[GlobalFormId],
    ) -> Result<(), Error> {
        if item.count() != 1 {
            return Err(Error::EnchantItem);
        }
        if soul_gem.count() != 1 {
            return Err(Error::SoulGem);
        }
        if !learned_enchantments.contains(&enchantment) {
            return Err(Error::Enchantment(enchantment));
        }

        let mut world = self.world.borrow_mut();
        if world
            .reference(actor)
            .is_none_or(|r| !r.enabled || r.deleted)
            || world.actor(actor).is_none_or(|a| a.dead || a.unconscious)
        {
            return Err(Error::EnchantActor(actor));
        }
        let enchantment_record = world
            .record(enchantment)
            .filter(|record| !record.is_deleted() && record.tag == Tag(*b"ENCH"))
            .ok_or(Error::Enchantment(enchantment))?;
        let effect_ids: Vec<_> = enchantment_record
            .all(Tag(*b"EFID"))
            .filter(|effect| effect.len() == 4)
            .map(|effect| u32::from_le_bytes([effect[0], effect[1], effect[2], effect[3]]))
            .collect();
        let effect_data_count = enchantment_record
            .all(Tag(*b"EFIT"))
            .filter(|data| data.len() == 12)
            .count();
        if enchantment_record
            .find(Tag(*b"ENIT"))
            .is_none_or(|data| data.len() < 32)
            || effect_ids.is_empty()
            || effect_data_count != effect_ids.len()
        {
            return Err(Error::Enchantment(enchantment));
        }
        for raw_effect in effect_ids {
            let effect = world
                .resolve_from(enchantment, raw_effect)
                .ok_or(Error::Enchantment(enchantment))?;
            if world
                .record(effect)
                .is_none_or(|record| record.is_deleted() || record.tag != Tag(*b"MGEF"))
            {
                return Err(Error::Enchantment(enchantment));
            }
        }

        let inventory = world
            .reference(actor)
            .ok_or(Error::EnchantActor(actor))?
            .inventory
            .clone();
        world
            .record(item.item())
            .filter(|record| {
                !record.is_deleted() && matches!(record.tag.as_bytes(), b"WEAP" | b"ARMO")
            })
            .ok_or(Error::EnchantItem)?;
        let item_stack = inventory.selected_stack(item)?;
        if item_stack.is_some_and(|stack| stack.extra(155).is_some()) {
            return Err(Error::EnchantItem);
        }

        let gem_record = world
            .record(soul_gem.item())
            .filter(|record| !record.is_deleted() && record.tag == Tag(*b"SLGM"))
            .ok_or(Error::SoulGem)?;
        let capacity = gem_record
            .find(Tag(*b"SLCP"))
            .and_then(|data| data.first())
            .copied()
            .ok_or(Error::SoulGem)?;
        let gem_stack = inventory.selected_stack(soul_gem)?;
        gem_stack
            .and_then(|stack| stack.extra(156))
            .and_then(|extra| match extra.fields.as_slice() {
                [Field::Byte(level)] => Some(*level),
                _ => None,
            })
            .filter(|level| *level > 0 && *level <= capacity)
            .ok_or(Error::SoulGem)?;

        let mut prepared = inventory.clone();
        prepared.update_selected(
            item,
            vec![Extra {
                kind: 155,
                fields: vec![Field::Form(Some(enchantment)), Field::Short(0)],
            }],
        )?;
        let gem_selection =
            prepared.select(soul_gem.item(), soul_gem.source(), soul_gem.count())?;
        prepared.take_selected(&gem_selection)?;

        let vm = &mut self.vm;
        world
            .reference(actor)
            .ok_or(Error::EnchantActor(actor))?
            .inventory = prepared;
        let alloc = &mut |name: &str, class: &str| vm.create_object_of(name, class);
        world.inventory_event(actor, soul_gem.item(), -1, None, alloc);
        Ok(())
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
