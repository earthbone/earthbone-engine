// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Stricter installed-data checks for UI-25 inventory names and cards.

use std::path::Path;

use soe_esm::{GlobalFormId, Tag};
use soe_godot_host::data_source::DataSource;
use soe_godot_host::inventory_menu::{self, Category};
use soe_godot_host::item_card::ItemCard;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

fn installed_source() -> Option<DataSource> {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        eprintln!("SKYRIM_DATA not set, skipping installed UI-25 inventory checks");
        return None;
    };
    Some(DataSource::open(Path::new(&data)).unwrap())
}

fn installed_holders(source: &DataSource) -> (soe_game::Game, [GlobalFormId; 3]) {
    let mut game = source.localized_game().unwrap();
    let merchants: Vec<_> = {
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
    let merchant = merchants
        .into_iter()
        .find(|holder| {
            game.world_mut().reference(*holder);
            inventory_menu::entries(&game, *holder, Category::All).len() >= 5
        })
        .expect("vanilla merchant inventory");
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
    let container = containers
        .into_iter()
        .find(|holder| {
            game.world_mut().reference(*holder);
            !inventory_menu::entries(&game, *holder, Category::All).is_empty()
        })
        .expect("vanilla container inventory");
    let player = game.player();
    for holder in [container, merchant] {
        for entry in inventory_menu::entries(&game, holder, Category::All) {
            game.world_mut().add_item(player, entry.item, 1);
        }
    }
    (game, [container, player, merchant])
}

#[test]
fn ui_25_breaker_installed_inventory_rows_never_use_missing_name_fallback() {
    let Some(source) = installed_source() else {
        return;
    };
    let (game, holders) = installed_holders(&source);
    let mut rows_seen = 0;
    for holder in holders {
        let rows = inventory_menu::entries(&game, holder, Category::All);
        assert!(!rows.is_empty(), "empty holder {holder}");
        for row in rows {
            assert_ne!(
                row.name, "Unnamed item",
                "holder {holder}, item {}",
                row.item
            );
            assert!(
                !row.name.starts_with("<string "),
                "{}: {}",
                row.item,
                row.name
            );
            assert_ne!(row.name, row.item.to_string(), "{}", row.item);
            assert_eq!(
                row.name,
                game.world().name_of(row.item).expect("vanilla item FULL"),
                "holder {holder}, item {}",
                row.item
            );
            rows_seen += 1;
        }
    }
    assert!(rows_seen >= 3, "expected rows in all three inventories");
}

#[test]
fn ui_25_breaker_installed_weapon_and_armor_cards_have_model_paths() {
    let Some(source) = installed_source() else {
        return;
    };
    let (game, holders) = installed_holders(&source);
    let mut weapon = false;
    let mut armor = false;
    for holder in holders {
        for row in inventory_menu::entries(&game, holder, Category::All) {
            let tag = game
                .world()
                .record(row.item)
                .expect("inventory base record")
                .tag;
            let card = ItemCard::read(&mut game.world_mut(), row.item).unwrap();
            match tag.as_bytes() {
                b"WEAP" => {
                    assert!(card.damage.is_some(), "{} lacks weapon damage", row.item);
                    weapon = true;
                }
                b"ARMO" => {
                    assert!(card.armor.is_some(), "{} lacks armor rating", row.item);
                    armor = true;
                }
                _ => continue,
            }
            assert!(
                card.model.as_deref().is_some_and(|model| !model.is_empty()),
                "{} has no authored ground model path",
                row.item
            );
        }
    }
    assert!(weapon, "sampled inventories contained no weapon");
    assert!(armor, "sampled inventories contained no armor");
}
