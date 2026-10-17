// SPDX-License-Identifier: GPL-3.0-or-later

//! FLOR and TREE activation from xEdit PFIG/PFPC and CK Flora semantics.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(kind: [u8; 4]) -> Game {
    game_with_chances(kind, [100; 4])
}

fn game_with_chances(kind: [u8; 4], chances: [u8; 4]) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"ACBS"), [0; 24]));
    plugin.push_top(
        Record::new(Tag(*b"GLOB"), 0x905)
            .zsub(Tag(*b"EDID"), "GameMonth")
            .sub(Tag(*b"FNAM"), b"f")
            .sub(Tag(*b"FLTV"), 0f32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"INGR"), 0x900)
            .zsub(Tag(*b"FULL"), "Mountain Flower")
            .sub(Tag(*b"DATA"), [0; 8]),
    );
    plugin.push_top(Record::new(Tag(*b"SNDR"), 0x904));
    plugin.push_top(Record::new(Tag(*b"NPC_"), 0x903).sub(Tag(*b"ACBS"), [0; 24]));
    plugin.push_top(
        Record::new(Tag(kind), 0x901)
            .zsub(Tag(*b"FULL"), "Mountain Flower Plant")
            .sub(Tag(*b"PFIG"), 0x900u32.to_le_bytes())
            .sub(Tag(*b"SNAM"), 0x904u32.to_le_bytes())
            .sub(Tag(*b"PFPC"), chances),
    );
    plugin.push_top(
        Record::new(Tag(*b"REFR"), 0x902)
            .sub(Tag(*b"NAME"), 0x901u32.to_le_bytes())
            .sub(Tag(*b"DATA"), [0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Harvest.esm", plugin).unwrap();
    let mut game = Game::new(order);
    let player = game.player();
    game.world_mut().actor(player).unwrap();
    game
}

#[test]
fn round4_flora_and_trees_give_the_ingredient_in_the_inventory_menu() {
    for kind in [*b"FLOR", *b"TREE"] {
        let mut game = game(kind);
        let player = game.player();
        assert!(game.activate(id(0x902), player));
        assert_eq!(
            game.world_mut().item_count(player, id(0x900)),
            1,
            "{kind:?} activation gives produce"
        );
        let rows = crate::inventory_menu::entries(
            &game,
            player,
            crate::inventory_menu::Category::Ingredients,
        );
        assert!(rows
            .iter()
            .any(|row| row.item == id(0x900) && row.count == 1 && row.name == "Mountain Flower"));
        assert!(game.activate(id(0x902), player));
        assert_eq!(
            game.world_mut().item_count(player, id(0x900)),
            1,
            "a harvested plant cannot give twice"
        );
        game.world_mut().clock.game_days += 10.0;
        assert!(game.activate(id(0x902), player));
        assert_eq!(
            game.world_mut().item_count(player, id(0x900)),
            2,
            "produce returns after the cell respawn time"
        );
    }
}

#[test]
fn round4_harvestable_flora_and_trees_have_a_harvest_prompt() {
    for kind in [*b"FLOR", *b"TREE"] {
        let game = game(kind);
        game.world_mut().set_crosshair_reference(Some(id(0x902)));
        assert_eq!(
            super::prompt(&game),
            Some(("Mountain Flower Plant".into(), "Harvest".into(), false))
        );
    }
}

#[test]
fn round4_market_food_and_ingredients_have_selection_collision() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let order = source.shared_order();
    let (city, _) = source.resolve_world("WhiterunWorld").unwrap();
    let mut game = Game::from_shared_order(order.clone());
    let player = game.player();
    game.world_mut().actor(player).unwrap();
    let mut checked = 0;
    let mut missing = Vec::new();
    for (cell, _) in source.cell_index().cells() {
        if source
            .cell_index()
            .grid_of(cell)
            .is_none_or(|grid| grid.world != city)
        {
            continue;
        }
        for &id in source.physical_references(cell) {
            let Some(winner) = order.winner_ref(id) else {
                continue;
            };
            let record = order.record(winner);
            if record.tag != Tag(*b"REFR") {
                continue;
            }
            let Ok(reference) = soe_esm::records::refr::Refr::from_record(record) else {
                continue;
            };
            let base = order.resolve(winner.file, reference.base);
            let Some(record) = order.winner(base) else {
                continue;
            };
            if !matches!(record.tag.as_bytes(), b"ALCH" | b"INGR")
                || !soe_game::World::authored_reference_enabled(&order, id)
            {
                continue;
            }
            let name = record.editor_id().unwrap_or_default();
            let before = game.world_mut().item_count(player, base);
            assert!(
                game.activate(id, player),
                "market item {id} {name} activates"
            );
            assert!(
                game.world_mut().item_count(player, base) > before,
                "market item {id} {name} enters inventory"
            );
            let rows =
                crate::inventory_menu::entries(&game, player, crate::inventory_menu::Category::All);
            assert!(rows.iter().any(|row| row.item == base));
            let colliders = source.placed_reference_colliders(id);
            eprintln!(
                "market {id} {name} {:?} collision {:?}",
                reference.transform,
                colliders.as_ref().map(|v| v.len())
            );
            if colliders.as_ref().is_ok_and(|v| !v.is_empty()) {
                checked += 1;
            } else {
                missing.push((id, name));
            }
        }
    }
    assert!(checked > 10, "installed city food and ingredients sampled");
    assert!(
        missing.is_empty(),
        "takeable city items missing activation collision: {missing:?}"
    );
}

#[test]
fn round4_harvest_state_survives_save_and_respawn() {
    let mut game = game(*b"FLOR");
    let player = game.player();
    game.activate(id(0x902), player);
    let bytes = game.world().save_state();
    let mut restored = super::harvest_tests::game(*b"FLOR");
    restored.world_mut().load_state(&bytes).unwrap();
    assert!(restored.world().harvested(id(0x902)));
    restored.activate(id(0x902), player);
    assert_eq!(restored.world_mut().item_count(player, id(0x900)), 1);
    restored.world_mut().advance_clock(10.0 * 86400.0);
    assert!(!restored.world().harvested(id(0x902)));
}

#[test]
fn round4_zero_season_chance_still_harvests_without_produce() {
    let mut game = game_with_chances(*b"FLOR", [0; 4]);
    let player = game.player();
    assert!(game.activate(id(0x902), player));
    assert!(game.world().harvested(id(0x902)));
    assert_eq!(game.world_mut().item_count(player, id(0x900)), 0);
}

#[test]
fn round4_installed_flora_models_expose_harvest_states() {
    let Some(data) = std::env::var_os("SKYRIM_DATA") else {
        return;
    };
    let source = crate::data_source::DataSource::open(std::path::Path::new(&data)).unwrap();
    let order = source.shared_order();
    // CONTEXT: Update.esm includes Creation Club base forms even in a base-only loadout.
    let mut supplemental = soe_bsa::Vfs::new();
    let mut archives: Vec<_> = std::fs::read_dir(&data)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            let name = path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_ascii_lowercase();
            (name.starts_with("cc") || name == "_resourcepack.bsa")
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("bsa"))
        })
        .collect();
    archives.sort();
    for archive in archives {
        supplemental.add_archive_path(&archive).unwrap();
    }

    let mut checked = 0;
    let mut missing = Vec::new();
    for tag in [Tag(*b"TREE"), Tag(*b"FLOR")] {
        for (id, record) in order.winners_of(tag) {
            if record.find(Tag(*b"PFIG")).is_none() {
                continue;
            }
            let Some(path) = record.find(Tag(*b"MODL")).map(soe_esm::cursor::zstring) else {
                continue;
            };
            let bytes = match source
                .vfs()
                .read(&format!("meshes/{path}"))
                .or_else(|_| supplemental.read(&format!("meshes/{path}")))
            {
                Ok(bytes) => bytes,
                Err(error) => {
                    missing.push((id, path, error.to_string()));
                    continue;
                }
            };
            let mut file = soe_nif::parse(&bytes).unwrap();
            let switches: Vec<_> = file
                .blocks
                .iter()
                .filter_map(|block| {
                    if let soe_nif::blocks::Block::Node(node) = block {
                        node.switch.map(|state| {
                            (node.av.name().to_owned(), state.active, node.children.len())
                        })
                    } else {
                        None
                    }
                })
                .collect();
            let types: Vec<_> = file
                .header
                .block_types
                .iter()
                .filter(|kind| {
                    kind.contains("Value")
                        || kind.contains("Switch")
                        || kind.contains("Vis")
                        || kind.contains("Controller")
                })
                .collect();
            eprintln!(
                "flora {id} {} {path}: {switches:?} {types:?}",
                record.editor_id().unwrap_or_default()
            );
            if switches.is_empty() {
                let names: Vec<_> = file
                    .blocks
                    .iter()
                    .filter_map(|block| match block {
                        soe_nif::blocks::Block::Node(node) => Some((node.av.name(), node.av.flags)),
                        soe_nif::blocks::Block::BsTriShape(shape) => {
                            Some((shape.av.name(), shape.av.flags))
                        }
                        _ => None,
                    })
                    .collect();
                eprintln!("flora graph {id}: {names:?}");
            }
            if !switches.is_empty() {
                let before = file.meshes().unwrap();
                for block in &mut file.blocks {
                    if let soe_nif::blocks::Block::Node(node) = block {
                        if let Some(switch) = node.switch.as_mut() {
                            switch.active = 1;
                        }
                    }
                }
                assert_ne!(
                    file.meshes().unwrap(),
                    before,
                    "{id} {path} has distinct harvest geometry"
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 20);
    assert!(
        missing.is_empty(),
        "harvest models missing from the installed VFS: {missing:?}"
    );
}

#[test]
fn round4_owned_harvest_preserves_theft_and_queues_sound() {
    let mut game = game(*b"FLOR");
    let player = game.player();
    game.world_mut().reference(id(0x902)).unwrap().owner = Some(id(0x903));
    assert!(game.world_mut().loose_item_is_stealing(id(0x902), player));
    assert!(game.activate(id(0x902), player));
    assert!(game
        .world()
        .sound_instances
        .values()
        .any(|(sound, reference, _)| *sound == id(0x904) && *reference == id(0x902)));
    let mut world = game.world_mut();
    let stack = &world.reference(player).unwrap().inventory.stacks(id(0x900))[0];
    assert_eq!(
        stack.extra(33).unwrap().fields,
        vec![soe_game::inventory::Field::Form(Some(id(0x903)))]
    );
}

#[test]
fn round4_only_carryable_lights_enter_the_take_path() {
    for carry in [false, true] {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"ACBS"), [0; 24]));
        let mut data = [0u8; 48];
        data[12..16].copy_from_slice(&(if carry { 2u32 } else { 0 }).to_le_bytes());
        plugin.push_top(
            Record::new(Tag(*b"LIGH"), 0x900)
                .zsub(Tag(*b"FULL"), "Light")
                .sub(Tag(*b"DATA"), data),
        );
        plugin.push_top(
            Record::new(Tag(*b"REFR"), 0x902)
                .sub(Tag(*b"NAME"), 0x900u32.to_le_bytes())
                .sub(Tag(*b"DATA"), [0; 24]),
        );
        let mut order = LoadOrder::new();
        order.add("Light.esm", plugin).unwrap();
        let mut game = Game::new(order);
        let player = game.player();
        game.world_mut().actor(player).unwrap();
        game.activate(id(0x902), player);
        assert_eq!(
            game.world_mut().item_count(player, id(0x900)),
            i32::from(carry),
            "LIGH carry flag {carry}"
        );
        assert_eq!(game.world().reference_enabled(id(0x902)), !carry);
    }
}

#[test]
fn round4_takeable_record_types_share_owned_inventory_receipt() {
    for kind in [
        *b"ALCH", *b"INGR", *b"MISC", *b"BOOK", *b"WEAP", *b"ARMO", *b"AMMO", *b"KEYM", *b"SLGM",
        *b"SCRL", *b"LIGH",
    ] {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"NPC_"), 7).sub(Tag(*b"ACBS"), [0; 24]));
        plugin.push_top(Record::new(Tag(*b"NPC_"), 0x903).sub(Tag(*b"ACBS"), [0; 24]));
        let mut data = [0u8; 48];
        if kind == *b"LIGH" {
            data[12..16].copy_from_slice(&2u32.to_le_bytes());
        }
        plugin.push_top(
            Record::new(Tag(kind), 0x900)
                .zsub(Tag(*b"FULL"), "Item")
                .sub(Tag(*b"DATA"), data),
        );
        plugin.push_top(
            Record::new(Tag(*b"REFR"), 0x902)
                .sub(Tag(*b"NAME"), 0x900u32.to_le_bytes())
                .sub(Tag(*b"XOWN"), 0x903u32.to_le_bytes())
                .sub(Tag(*b"DATA"), [0; 24]),
        );
        let mut order = LoadOrder::new();
        order.add("Take.esm", plugin).unwrap();
        let mut game = Game::new(order);
        let player = game.player();
        game.world_mut().actor(player).unwrap();
        assert!(game.activate(id(0x902), player), "{kind:?}");
        assert_eq!(
            game.world_mut().item_count(player, id(0x900)),
            1,
            "{kind:?}"
        );
        assert!(!game.world().reference_enabled(id(0x902)));
        let mut world = game.world_mut();
        let stack = &world.reference(player).unwrap().inventory.stacks(id(0x900))[0];
        assert_eq!(
            stack.extra(33).unwrap().fields,
            vec![soe_game::inventory::Field::Form(Some(id(0x903)))]
        );
        assert_eq!(stack.reference(), Some(id(0x902)));
    }
}

#[test]
fn round4_rejected_save_preserves_harvest_state() {
    let mut game = game(*b"FLOR");
    let player = game.player();
    game.activate(id(0x902), player);
    let bytes = game.world().save_state();
    assert!(game
        .world_mut()
        .load_state(&bytes[..bytes.len() - 1])
        .is_err());
    assert!(game.world().harvested(id(0x902)));
    assert_eq!(game.world_mut().item_count(player, id(0x900)), 1);
}

#[test]
fn round4_pfpc_bytes_select_spring_summer_fall_and_winter() {
    for (season, month) in [2.0, 5.0, 8.0, 11.0].into_iter().enumerate() {
        for successful in 0..4 {
            let mut chances = [0; 4];
            chances[successful] = 100;
            let mut game = game_with_chances(*b"FLOR", chances);
            let player = game.player();
            game.world_mut().set_global(id(0x905), month);
            game.activate(id(0x902), player);
            assert_eq!(
                game.world_mut().item_count(player, id(0x900)),
                i32::from(season == successful),
                "PFPC season {season}, chance in byte {successful}"
            );
        }
    }
}

#[test]
fn round4_harvest_notifies_a_resident_geometry_observer() {
    let mut game = game(*b"FLOR");
    let player = game.player();
    let mut changes = soe_game::world::ReferenceChanges::default();
    changes.watch(&game.world(), id(0x902));
    assert_eq!(changes.changed(&game.world()), vec![id(0x902)]);
    game.activate(id(0x902), player);
    assert_eq!(
        changes.changed(&game.world()),
        vec![id(0x902)],
        "harvest changes presentation even with the same placement"
    );
}
