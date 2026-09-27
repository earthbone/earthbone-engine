// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Installed COBJ execution. Hearthfires nail quantities and the forge reference
//! are also covered by skymp/unit/CraftTest.cpp. No game data is copied here.

use soe_esm::{records::gameplay::Recipe, GlobalFormId, LoadOrder, Plugin, Tag};
use soe_game::Game;
use std::collections::BTreeMap;

#[test]
fn installed_forge_executes_vanilla_and_hearthfires_recipes() {
    let Some(data) = std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from) else {
        eprintln!("SKYRIM_DATA not set, skipping installed crafting execution");
        return;
    };
    let mut order = LoadOrder::new();
    for name in [
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ] {
        order
            .add(
                name,
                Plugin::parse(&std::fs::read(data.join(name)).unwrap()).unwrap(),
            )
            .unwrap();
    }
    let recipes: Vec<_> = order
        .winners_of(Tag(*b"COBJ"))
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, record)| (id, Recipe::from_record(record).unwrap()))
        .collect();
    assert!(
        recipes.len() >= 1300,
        "five-master survey unexpectedly small"
    );
    let mut functions = BTreeMap::<u16, usize>::new();
    for (_, recipe) in &recipes {
        for condition in &recipe.conditions {
            *functions.entry(condition.function).or_default() += 1;
            assert!(
                soe_game::conditions::supports_function(condition.function),
                "COBJ predicate {} has no implementation",
                condition.function
            );
        }
    }
    let forge = GlobalFormId {
        file: 0,
        object: 0x1ad6e,
    };
    let mut game = Game::new(order);
    let player = game.world_mut().player();
    for (output, expected_count) in [
        (
            GlobalFormId {
                file: 0,
                object: 0x1397e,
            },
            1,
        ),
        (
            GlobalFormId {
                file: 3,
                object: 0x300f,
            },
            10,
        ),
    ] {
        let candidates: Vec<_> = recipes
            .iter()
            .filter(|(id, recipe)| {
                recipe
                    .created_object
                    .and_then(|raw| game.world().resolve_from(*id, raw))
                    == Some(output)
                    && recipe
                        .workbench_keyword
                        .and_then(|raw| game.world().resolve_from(*id, raw))
                        == game.form("CraftingSmithingForge")
            })
            .collect();
        assert_eq!(
            candidates.len(),
            1,
            "expected a unique forge recipe for {output}"
        );
        let (recipe_id, recipe) = candidates[0];
        let mut expected_inputs = BTreeMap::new();
        for input in &recipe.ingredients {
            let resolved = game.world().resolve_from(*recipe_id, input.item).unwrap();
            *expected_inputs.entry(resolved).or_insert(0) += input.count;
        }
        game.world_mut()
            .reference(player)
            .unwrap()
            .inventory
            .clear();
        for (&item, &count) in &expected_inputs {
            game.world_mut().add_item(player, item, count);
        }
        let result = game
            .craft(player, forge, *recipe_id)
            .unwrap_or_else(|e| panic!("{:?}: {e}", recipe.editor_id));
        assert_eq!(result.created, output);
        assert_eq!(result.count, expected_count);
        assert_eq!(result.consumed, expected_inputs);
        let condition = soe_esm::records::gameplay::Condition {
            flags: 0,
            comparison: soe_esm::records::gameplay::Comparison::Number(expected_count as f32),
            function: 47,
            parameters: [recipe.created_object.unwrap(), 0],
            run_on: 2,
            reference: 0x14,
            parameter_3: -1,
            strings: [None, None],
        };
        let context = soe_game::conditions::Context {
            owner: *recipe_id,
            subject: forge,
            target: None,
            quest: None,
            tempering_enchanted: None,
            tempering_item: None,
        };
        assert!(soe_game::conditions::evaluate_one(
            &mut game.world_mut(),
            &game.vm,
            context,
            &condition
        )
        .unwrap());
        assert_eq!(
            game.world_mut()
                .reference(player)
                .unwrap()
                .inventory
                .iter()
                .map(|(id, n)| (*id, n))
                .collect::<BTreeMap<_, _>>(),
            BTreeMap::from([(output, expected_count)])
        );
        eprintln!(
            "crafted {:?}: {expected_inputs:?} -> {expected_count} x {output}",
            recipe.editor_id
        );
    }
    eprintln!(
        "surveyed {} recipes, condition counts {functions:?}",
        recipes.len()
    );
}
