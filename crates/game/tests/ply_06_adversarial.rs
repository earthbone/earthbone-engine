// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! PLY-06 adversarial checks against the installed Skyrim records.

use crate::common::installed;
use soe_esm::{GlobalFormId, Tag};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

#[test]
fn ply_06_remove_all_override_clears_each_actor_once_and_preserves_other_packages() {
    let Some(mut game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };

    let mut actors = vec![game.player()];
    let candidates = game
        .world()
        .order()
        .winners_of(T(b"ACHR"))
        .map(|(form, _)| form)
        .collect::<Vec<_>>();
    for id in candidates {
        if actors.len() == 2 {
            break;
        }
        if game.world_mut().actor(id).is_some() && !actors.contains(&id) {
            actors.push(id);
        }
    }
    assert_eq!(actors.len(), 2, "fixture needs two actor references");

    let package = game.form("WhiterunAdrianneEstablishingScene").unwrap();
    let other = game.form("C06EorlundReforgeWuuthrad").unwrap();
    for actor in &actors {
        assert!(game.world_mut().add_package_override(*actor, package, 40, 0));
        assert!(game.world_mut().add_package_override(*actor, other, 30, 0));
    }

    assert_eq!(game.world_mut().remove_all_package_overrides(package), 2);
    for actor in actors {
        assert_eq!(game.world().count_package_overrides(actor), 1);
        assert_eq!(
            game.world()
                .actor_ref(actor)
                .unwrap()
                .runtime_package_overrides[0]
                .package,
            other
        );
    }
}

#[test]
fn ply_06_failed_vanilla_brew_leaves_all_components_and_experience_unchanged() {
    let Some(mut game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let player = game.player();
    let bench = GlobalFormId { file: 0, object: 0x000e4c31 };
    let flower = GlobalFormId { file: 0, object: 0x00077e1c };
    let wheat = GlobalFormId { file: 0, object: 0x0004b0ba };
    {
        let mut world = game.world_mut();
        world.actor(player).unwrap();
        let lab = world.reference(bench).unwrap();
        lab.enabled = true;
        lab.deleted = false;
        world.add_item(player, flower, 2);
    }
    let before = game.world().reference_ref(player).unwrap().inventory.clone();
    let experience = game.world().skill_experience(player, "alchemy");
    let notifications = game.world().pending.len();

    assert!(game
        .brew_potion(player, bench, &[flower, wheat])
        .is_err());
    assert_eq!(game.world().reference_ref(player).unwrap().inventory, before);
    assert_eq!(game.world().skill_experience(player, "alchemy"), experience);
    assert_eq!(game.world().pending.len(), notifications);
}

#[test]
fn ply_06_repeated_vanilla_brews_consume_each_pair_and_persist_progression() {
    let Some(mut game) = installed::game(&[
        "Skyrim.esm",
        "Update.esm",
        "Dawnguard.esm",
        "HearthFires.esm",
        "Dragonborn.esm",
    ]) else {
        return;
    };
    let player = game.player();
    let bench = GlobalFormId { file: 0, object: 0x000e4c31 };
    let flower = GlobalFormId { file: 0, object: 0x00077e1c };
    let wheat = GlobalFormId { file: 0, object: 0x0004b0ba };
    {
        let mut world = game.world_mut();
        let lab = world.reference(bench).unwrap();
        lab.enabled = true;
        lab.deleted = false;
        world.add_item(player, flower, 2);
        world.add_item(player, wheat, 2);
    }

    game.brew_potion(player, bench, &[flower, wheat]).unwrap();
    let first_xp = game.world().skill_experience(player, "alchemy").unwrap();
    game.brew_potion(player, bench, &[flower, wheat]).unwrap();
    let second_xp = game.world().skill_experience(player, "alchemy").unwrap();

    assert_eq!(game.world_mut().item_count(player, flower), 0);
    assert_eq!(game.world_mut().item_count(player, wheat), 0);
    assert!(first_xp > 0.0);
    assert!(second_xp > first_xp);
    let saved = game.save_state().unwrap();
    game.load_state(&saved).unwrap();
    assert_eq!(game.world().skill_experience(player, "alchemy"), Some(second_xp));
}
