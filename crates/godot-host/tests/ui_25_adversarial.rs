// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial item-name and item-card checks for UI-25.

use std::path::Path;

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;
use soe_godot_host::data_source::DataSource;
use soe_godot_host::inventory_menu::{self, Category};
use soe_godot_host::item_card::{display_name, ItemCard};

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn synthetic_game(record: Record) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(record);
    let mut order = LoadOrder::new();
    order.add("Items.esm", plugin).unwrap();
    Game::new(order)
}

#[test]
fn ui_25_adversarial_installed_menu_items_keep_full_names() {
    let data = std::env::var("SKYRIM_DATA").expect("SKYRIM_DATA is required for UI-25");
    let source = DataSource::open(Path::new(&data)).unwrap();
    let mut game = source.localized_game().unwrap();
    let merchant_chests: Vec<_> = {
        let world = game.world();
        world
            .order()
            .winners_of(T(b"FACT"))
            .filter_map(|(id, record)| {
                let bytes: [u8; 4] = record.find(T(b"VENC"))?.try_into().ok()?;
                world.resolve_from(id, u32::from_le_bytes(bytes))
            })
            .collect()
    };
    let merchant = merchant_chests.into_iter().find(|holder| {
        game.world_mut().reference(*holder);
        inventory_menu::entries(&game, *holder, Category::All).len() >= 5
    }).expect("vanilla merchant inventory");
    let containers: Vec<_> = {
        let world = game.world();
        world
            .order()
            .winners_of(T(b"REFR"))
            .filter_map(|(id, record)| {
                let bytes: [u8; 4] = record.find(T(b"NAME"))?.try_into().ok()?;
                let base = world.resolve_from(id, u32::from_le_bytes(bytes))?;
                let base = world.record(base)?;
                (base.tag == T(b"CONT") && base.find(T(b"CNTO")).is_some()).then_some(id)
            })
            .take(100)
            .collect()
    };
    let container = containers.into_iter().find(|holder| {
        game.world_mut().reference(*holder);
        !inventory_menu::entries(&game, *holder, Category::All).is_empty()
    }).expect("vanilla container inventory");
    let player = game.player();
    for holder in [container, merchant] {
        for entry in inventory_menu::entries(&game, holder, Category::All) {
            game.world_mut().add_item(player, entry.item, 1);
        }
    }

    let mut listed = 0;
    for holder in [container, player, merchant] {
        let rows = inventory_menu::entries(&game, holder, Category::All);
        assert!(!rows.is_empty(), "empty holder {holder}");
        for row in rows {
            assert!(!row.name.trim().is_empty());
            assert!(!row.name.starts_with("<string "), "{}: {}", row.item, row.name);
            assert_ne!(row.name, row.item.to_string(), "{}", row.item);
            let card = ItemCard::read(&mut game.world_mut(), row.item)
                .unwrap_or_else(|error| panic!("{} {}: {error}", row.item, row.name));
            assert_eq!(card.name, row.name);
            assert!(card.weight.is_finite() && card.weight >= 0.0);
            listed += 1;
        }
    }
    assert!(listed >= 3, "expected rows across container, player and merchant");
}

#[test]
fn ui_25_adversarial_missing_name_and_form_never_leak_ids() {
    let game = synthetic_game(Record::new(T(b"MISC"), 1));
    assert_eq!(display_name(&game.world(), id(1)), "Unnamed item");
    assert_eq!(display_name(&game.world(), id(999)), "Unnamed item");
    assert!(!display_name(&game.world(), id(1)).contains("<string "));
    assert!(ItemCard::read(&mut game.world_mut(), id(999)).is_err());
}

#[test]
fn ui_25_adversarial_rejects_nonfinite_weapon_stats_and_template_cycles() {
    let bad_weight = [
        7_u32.to_le_bytes().to_vec(),
        f32::NAN.to_le_bytes().to_vec(),
        9_u16.to_le_bytes().to_vec(),
    ]
    .concat();
    let bad_stats = Record::new(T(b"WEAP"), 1)
        .zsub(T(b"FULL"), "Broken sword")
        .sub(T(b"DATA"), bad_weight);
    assert!(ItemCard::read(&mut synthetic_game(bad_stats).world_mut(), id(1)).is_err());

    let first = Record::new(T(b"WEAP"), 1)
        .zsub(T(b"FULL"), "First")
        .sub(T(b"CNAM"), 2_u32.to_le_bytes());
    let second = Record::new(T(b"WEAP"), 2).sub(T(b"CNAM"), 1_u32.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(first);
    plugin.push_top(second);
    let mut order = LoadOrder::new();
    order.add("Items.esm", plugin).unwrap();
    let game = Game::new(order);
    assert!(ItemCard::read(&mut game.world_mut(), id(1)).is_err());
}
