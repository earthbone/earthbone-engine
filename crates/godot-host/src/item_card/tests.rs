// SPDX-License-Identifier: GPL-3.0-or-later

//! UI-25 card fixtures and installed inventory/name/model survey.

use super::*;
use crate::data_source::DataSource;
use crate::inventory_menu::{entries, Category};
use soe_esm::{FileHeader, LoadOrder, Plugin};
use soe_game::Game;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game(records: impl IntoIterator<Item = Record>) -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    for record in records {
        plugin.push_top(record);
    }
    let mut order = LoadOrder::new();
    order.add("Items.esm", plugin).unwrap();
    Game::new(order)
}

fn item(tag: [u8; 4], object: u32, bytes: Vec<u8>) -> Record {
    Record::new(Tag(tag), object)
        .zsub(T(b"FULL"), "Named item")
        .zsub(T(b"MODL"), "Items/ground.nif")
        .sub(T(b"DATA"), bytes)
}

fn value_weight() -> Vec<u8> {
    [17_u32.to_le_bytes(), 2.5_f32.to_le_bytes()].concat()
}

#[test]
fn ui_25_card_reads_weight_value_and_type_specific_statistics() {
    let weapon = item(
        *b"WEAP",
        1,
        [value_weight(), 9_u16.to_le_bytes().to_vec()].concat(),
    );
    let mut ammo = vec![0; 8];
    ammo.extend_from_slice(&12_f32.to_le_bytes());
    ammo.extend_from_slice(&17_u32.to_le_bytes());
    ammo.extend_from_slice(&0.1_f32.to_le_bytes());
    let mut book = vec![0; 8];
    book.extend_from_slice(&value_weight());
    let mut records = vec![weapon, item(*b"AMMO", 2, ammo), item(*b"BOOK", 3, book)];
    for (index, tag) in [b"MISC", b"INGR", b"KEYM", b"SLGM", b"SCRL"]
        .into_iter()
        .enumerate()
    {
        records.push(item(*tag, index as u32 + 4, value_weight()));
    }
    records.push(
        item(*b"ALCH", 9, 2.5_f32.to_le_bytes().to_vec()).sub(T(b"ENIT"), 17_u32.to_le_bytes()),
    );
    let game = game(records);
    let mut world = game.world_mut();
    for object in 1..=9 {
        let card = ItemCard::read(&mut world, id(object)).unwrap();
        assert_eq!(card.name, "Named item");
        assert_eq!(card.value, 17, "object {object}");
        assert_eq!(card.weight, if object == 2 { 0.1 } else { 2.5 });
        assert_eq!(card.model.as_deref(), Some("Items/ground.nif"));
        assert_eq!(
            card.damage,
            match object {
                1 => Some(9.0),
                2 => Some(12.0),
                _ => None,
            }
        );
        assert!(card.statistics().contains("Weight") && card.statistics().contains("Value"));
    }
}

#[test]
fn ui_25_armor_card_uses_ground_model_and_template_properties() {
    let parent = item(*b"ARMO", 1, value_weight())
        .sub(T(b"DNAM"), 0x10007D0_u32.to_le_bytes())
        .sub(
            T(b"BOD2"),
            [4_u32.to_le_bytes(), 1_u32.to_le_bytes()].concat(),
        )
        .zsub(T(b"MOD2"), "Armor/ground.nif");
    let child = Record::new(T(b"ARMO"), 2)
        .zsub(T(b"FULL"), "Enchanted armor")
        .sub(T(b"TNAM"), 1_u32.to_le_bytes())
        .sub(
            T(b"DATA"),
            [90_u32.to_le_bytes(), 99_f32.to_le_bytes()].concat(),
        )
        .sub(T(b"MODL"), 0x1234_u32.to_le_bytes());
    let game = game([parent, child]);
    let card = ItemCard::read(&mut game.world_mut(), id(2)).unwrap();
    assert_eq!(card.name, "Enchanted armor");
    assert_eq!(card.value, 90);
    assert_eq!(card.weight, 2.5);
    assert_eq!(card.armor, Some(20.0));
    assert_eq!(card.model.as_deref(), Some("Armor/ground.nif"));
    assert!(card.statistics().contains("Armor  20"));
}

#[test]
fn ui_25_weapon_card_follows_template_without_replacing_child_name() {
    let parent = item(
        *b"WEAP",
        1,
        [value_weight(), 9_u16.to_le_bytes().to_vec()].concat(),
    );
    let child = Record::new(T(b"WEAP"), 2)
        .zsub(T(b"FULL"), "Enchanted sword")
        .sub(T(b"CNAM"), 1_u32.to_le_bytes());
    let game = game([parent, child]);
    let card = ItemCard::read(&mut game.world_mut(), id(2)).unwrap();
    assert_eq!(card.name, "Enchanted sword");
    assert_eq!(card.damage, Some(9.0));
    assert!(card.statistics().contains("Damage  9"));
}

#[test]
fn ui_25_armor_card_uses_selected_sex_and_falls_back_to_male_ground_model() {
    use soe_game::npc::{self, Config, Stats};
    let mut race_data = vec![0; 128];
    race_data[32..36].copy_from_slice(&1_u32.to_le_bytes());
    let armor = item(*b"ARMO", 1, value_weight())
        .sub(T(b"DNAM"), 2000_u32.to_le_bytes())
        .sub(
            T(b"BOD2"),
            [4_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
        )
        .zsub(T(b"MOD2"), "Armor/male.nif");
    let mut female_model = armor.clone();
    female_model.form_id = 2;
    female_model = female_model.zsub(T(b"MOD4"), "Armor/female.nif");
    let game = game([
        Record::new(T(b"RACE"), 0x700).sub(T(b"DATA"), race_data),
        npc::build_record(
            0x14,
            "Player",
            "Player",
            Config {
                flags: npc::flags::FEMALE,
                ..Config::default()
            },
            Stats::default(),
        )
        .sub(T(b"RNAM"), 0x700_u32.to_le_bytes()),
        armor,
        female_model,
    ]);
    let mut world = game.world_mut();
    let male_only = ItemCard::read(&mut world, id(1)).unwrap();
    assert_eq!(male_only.model.as_deref(), Some("Armor/male.nif"));
    assert_eq!(male_only.armor, Some(0.0), "clothing ignores DNAM");
    assert_eq!(
        ItemCard::read(&mut world, id(2)).unwrap().model.as_deref(),
        Some("Armor/female.nif")
    );
    world.set_player_character_traits(id(0x700), false).unwrap();
    assert_eq!(
        ItemCard::read(&mut world, id(2)).unwrap().model.as_deref(),
        Some("Armor/male.nif")
    );
}

#[test]
fn ui_25_card_rejects_malformed_numbers_and_template_cycles() {
    for bytes in [
        vec![0; 7],
        [17_u32.to_le_bytes(), f32::NAN.to_le_bytes()].concat(),
        [17_u32.to_le_bytes(), (-1_f32).to_le_bytes()].concat(),
    ] {
        let game = game([item(*b"MISC", 1, bytes)]);
        assert!(ItemCard::read(&mut game.world_mut(), id(1)).is_err());
    }
    let game = game([Record::new(T(b"WEAP"), 1).sub(T(b"CNAM"), 1_u32.to_le_bytes())]);
    assert!(ItemCard::read(&mut game.world_mut(), id(1))
        .unwrap_err()
        .to_string()
        .contains("cycle"));
    assert!(ItemCard::read(&mut game.world_mut(), id(99)).is_err());
}

#[test]
fn ui_25_localized_item_names_use_winning_plugin_tables() {
    use soe_esm::plugin::flags;
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir(directory.path().join("Strings")).unwrap();
    let table = |text: &str| {
        let mut bytes = [
            1_u32.to_le_bytes(),
            (text.len() as u32 + 1).to_le_bytes(),
            7_u32.to_le_bytes(),
            0_u32.to_le_bytes(),
        ]
        .concat();
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
        bytes
    };
    std::fs::write(
        directory.path().join("Strings/Items_english.strings"),
        table("Original name"),
    )
    .unwrap();
    std::fs::write(
        directory.path().join("Strings/Patch_english.strings"),
        table("Winning name"),
    )
    .unwrap();
    let mut base = Plugin::new(FileHeader {
        flags: flags::LOCALIZED,
        ..FileHeader::default()
    });
    base.push_top(
        Record::new(T(b"MISC"), 1)
            .sub(T(b"FULL"), 7_u32.to_le_bytes())
            .sub(T(b"DATA"), value_weight()),
    );
    let mut patch = Plugin::new(FileHeader {
        flags: flags::LOCALIZED,
        masters: vec!["Items.esm".into()],
        ..FileHeader::default()
    });
    patch.push_top(
        Record::new(T(b"MISC"), 1)
            .sub(T(b"FULL"), 7_u32.to_le_bytes())
            .sub(T(b"DATA"), value_weight()),
    );
    let mut order = LoadOrder::new();
    order.add("Items.esm", base).unwrap();
    order.add("Patch.esp", patch).unwrap();
    let mut game = Game::new(order);
    let mut vfs = soe_bsa::Vfs::new();
    vfs.add_loose_root(directory.path()).unwrap();
    game.world_mut()
        .load_localized_strings(&vfs, "english")
        .unwrap();
    let player = game.player();
    game.world_mut().add_item(player, id(1), 1);
    assert_eq!(
        entries(&game, player, Category::All)[0].name,
        "Winning name"
    );
    assert_eq!(
        ItemCard::read(&mut game.world_mut(), id(1)).unwrap().name,
        "Winning name"
    );
    assert_eq!(display_name(&game.world(), id(99)), "Unnamed item");
}

#[test]
fn ui_25_vanilla_container_player_and_merchant_list_full_names_and_models() {
    let Ok(data) = std::env::var("SKYRIM_DATA") else {
        return;
    };
    let source = DataSource::open(std::path::Path::new(&data)).unwrap();
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
    let mut merchant = None;
    for holder in merchant_chests {
        game.world_mut().reference(holder);
        if entries(&game, holder, Category::All).len() >= 5 {
            merchant = Some(holder);
            break;
        }
    }
    let merchant = merchant.expect("vanilla merchant chest with merchandise");
    let containers: Vec<_> = {
        let world = game.world();
        world
            .order()
            .winners_of(T(b"REFR"))
            .filter_map(|(id, record)| {
                let bytes: [u8; 4] = record.find(T(b"NAME"))?.try_into().ok()?;
                let base = world.resolve_from(id, u32::from_le_bytes(bytes))?;
                let record = world.record(base)?;
                (record.tag == T(b"CONT") && record.find(T(b"CNTO")).is_some()).then_some(id)
            })
            .take(100)
            .collect()
    };
    let mut container = None;
    for holder in containers {
        game.world_mut().reference(holder);
        if !entries(&game, holder, Category::All).is_empty() {
            container = Some(holder);
            break;
        }
    }
    let container = container.expect("vanilla container with inventory");
    let player = game.player();
    for holder in [container, merchant] {
        for entry in entries(&game, holder, Category::All) {
            game.world_mut().add_item(player, entry.item, 1);
        }
    }
    let mut listed = 0;
    let mut models = BTreeSet::new();
    for holder in [container, player, merchant] {
        let rows = entries(&game, holder, Category::All);
        assert!(!rows.is_empty(), "holder {holder}");
        for row in rows {
            let mut world = game.world_mut();
            let expected = world.name_of(row.item).unwrap_or_else(|| {
                panic!(
                    "holder {holder}, item {} tag {:?} editor {:?}: no FULL",
                    row.item,
                    world.tag_of(row.item),
                    world.editor_id(row.item)
                )
            });
            assert!(
                !expected.is_empty() && !expected.starts_with("<string "),
                "{} {expected}",
                row.item
            );
            assert_eq!(row.name, expected);
            assert_ne!(row.name, row.item.to_string());
            let card = ItemCard::read(&mut world, row.item)
                .unwrap_or_else(|error| panic!("{} {}: {error}", row.item, row.name));
            assert_eq!(card.name, row.name);
            assert!(card.weight.is_finite() && card.weight >= 0.0);
            if let Some(model) = card.model {
                models.insert(model);
            }
            listed += 1;
        }
    }
    assert!(
        models.len() >= 3,
        "model survey needs at least three distinct ground models"
    );
    for model in &models {
        let surfaces = source.item_preview_surfaces(model).unwrap();
        assert!(!surfaces.is_empty(), "empty item model {model}");
        assert!(surfaces
            .iter()
            .all(|(arrays, _)| arrays.triangle_count() > 0));
    }
    println!("UI-25 vanilla rows={listed} ground_models={}", models.len());
}
