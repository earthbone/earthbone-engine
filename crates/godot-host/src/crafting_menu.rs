// SPDX-License-Identifier: GPL-3.0-or-later

//! Craftable COBJ rows and confirmation through the shared Game transaction.

use soe_esm::{records::gameplay::Recipe, GlobalFormId, Tag};
use soe_game::{crafting::Outcome, Game};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub recipe: GlobalFormId,
    pub name: String,
    pub output: GlobalFormId,
    pub count: i32,
    pub available: bool,
}

/// Return recipes the shared Game currently accepts at this bench.
pub fn entries(game: &mut Game, actor: GlobalFormId, bench: GlobalFormId) -> Vec<Entry> {
    let mut recipe_ids: Vec<_> = game
        .world()
        .order()
        .winners_of(Tag(*b"COBJ"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, _)| id)
        .collect();
    recipe_ids.sort_unstable();
    recipe_ids
        .into_iter()
        .filter_map(|recipe| {
            let outcome = match game.check_craft(actor, bench, recipe) {
                Ok(outcome) => Some((outcome.created, outcome.count, true)),
                Err(soe_game::crafting::Error::MissingIngredient { .. }) => {
                    let world = game.world();
                    let record = world.record(recipe)?;
                    let parsed = Recipe::from_record(record).ok()?;
                    let output = world.resolve_from(recipe, parsed.created_object?)?;
                    let count = i32::from(parsed.created_count?);
                    if count <= 0 {
                        return None;
                    }
                    Some((output, count, false))
                }
                Err(_) => None,
            }?;
            Some(Entry {
                recipe,
                name: crate::item_card::display_name(&game.world(), outcome.0),
                output: outcome.0,
                count: outcome.1,
                available: outcome.2,
            })
        })
        .collect()
}

/// Confirm one listed recipe through the same transaction used by gameplay.
pub fn confirm(
    game: &mut Game,
    actor: GlobalFormId,
    bench: GlobalFormId,
    recipe: GlobalFormId,
) -> Result<Outcome, soe_game::crafting::Error> {
    game.craft(actor, bench, recipe)
}

/// Whether a reference uses the object creation bench mode.
pub fn is_creation_bench(game: &Game, reference: GlobalFormId) -> bool {
    let world = game.world();
    let base = world
        .reference_ref(reference)
        .and_then(|value| value.base)
        .or_else(|| {
            let raw = world.record(reference)?.find(Tag(*b"NAME"))?;
            let raw = u32::from_le_bytes(raw.try_into().ok()?);
            world.resolve_from(reference, raw)
        });
    let Some(base) = base else {
        return false;
    };
    world
        .record(base)
        .filter(|record| !record.is_deleted() && record.tag == Tag(*b"FURN"))
        .and_then(|record| record.find(Tag(*b"WBDT")))
        .is_some_and(|data| data.len() == 2 && data[0] == 1)
}
