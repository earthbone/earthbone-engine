// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Tempering raises one item instance's quality and consumes its material atomically.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{crafting::Error, inventory::*, Game, Pending};

const PLAYER: u32 = 0x14;
const BASE: u32 = 0x800;
const BENCH: u32 = 0x801;
const FURN: u32 = 0x802;
const BENCH_KEYWORD: u32 = 0x803;
const WEAPON: u32 = 0x804;
const MATERIAL: u32 = 0x805;
const RECIPE: u32 = 0x806;
const ENCHANTMENT: u32 = 0x807;

fn id(raw: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: raw,
    }
}

fn recipe(target: u32) -> Record {
    let mut weapon = WEAPON.to_le_bytes().to_vec();
    weapon.extend(1_i32.to_le_bytes());
    let mut material = MATERIAL.to_le_bytes().to_vec();
    material.extend(2_i32.to_le_bytes());
    Record::new(Tag(*b"COBJ"), RECIPE)
        .sub(Tag(*b"CNAM"), target.to_le_bytes())
        .sub(Tag(*b"BNAM"), BENCH_KEYWORD.to_le_bytes())
        .sub(Tag(*b"NAM1"), 1_u16.to_le_bytes())
        .sub(Tag(*b"CNTO"), weapon)
        .sub(Tag(*b"CNTO"), material)
}

fn game(weapon_tag: [u8; 4], bench_type: u8) -> Game {
    let keyword_name = if bench_type == 2 {
        "CraftingSmithingSharpeningWheel"
    } else {
        "CraftingSmithingArmorTable"
    };
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), BASE));
    plugin.push_top(
        Record::new(Tag(*b"KYWD"), BENCH_KEYWORD)
            .sub(Tag(*b"EDID"), keyword_name.as_bytes().to_vec()),
    );
    plugin.push_top(Record::new(Tag(weapon_tag), WEAPON));
    plugin.push_top(Record::new(Tag(*b"MISC"), MATERIAL));
    plugin.push_top(
        Record::new(Tag(*b"FURN"), FURN)
            .sub(Tag(*b"WBDT"), [bench_type, 0xff])
            .sub(Tag(*b"KWDA"), BENCH_KEYWORD.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), BASE.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"REFR"), BENCH).sub(Tag(*b"NAME"), FURN.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ENCH"), ENCHANTMENT));
    plugin.push_top(recipe(WEAPON));
    let mut order = LoadOrder::new();
    order.add("Tempering.esm", plugin).unwrap();
    Game::new(order)
}

fn quality(value: f32) -> Extra {
    Extra {
        kind: 37,
        fields: vec![Field::FloatBits(value.to_bits())],
    }
}

fn enriched_weapon(count: i32) -> Stack {
    Stack::new(
        count,
        vec![
            quality(1.1),
            Extra {
                kind: 155,
                fields: vec![Field::Form(Some(id(ENCHANTMENT))), Field::Short(500)],
            },
            Extra {
                kind: 40,
                fields: vec![Field::FloatBits(0.75_f32.to_bits())],
            },
        ],
    )
    .unwrap()
}

fn pending_events(game: &Game) -> Vec<(String, Vec<soe_papyrus::Value>)> {
    game.world()
        .pending
        .iter()
        .map(|event| match event {
            Pending::Event { name, args, .. } => (name.clone(), args.clone()),
            other => panic!("unexpected pending item: {other:?}"),
        })
        .collect()
}

#[test]
fn crf_01_tempering_updates_one_instance_and_consumes_material_atomically() {
    let mut game = game(*b"WEAP", 2);
    game.world_mut()
        .reference(id(PLAYER))
        .unwrap()
        .inventory
        .restore(id(WEAPON), 2, vec![enriched_weapon(2)])
        .unwrap();
    game.world_mut().add_item(id(PLAYER), id(MATERIAL), 2);
    game.world_mut()
        .set_actor_value(id(PLAYER), "smithing", 50.0)
        .unwrap();

    let result = game.temper(id(PLAYER), id(BENCH), id(RECIPE)).unwrap();
    assert_eq!(result.item, id(WEAPON));
    assert_eq!(result.consumed.get(&id(MATERIAL)), Some(&2));
    assert_eq!(result.previous_quality, 1.1);
    assert_eq!(result.quality, 1.5);
    let world = game.world();
    let inventory = &world.reference_ref(id(PLAYER)).unwrap().inventory;
    assert_eq!(inventory.count(id(MATERIAL)), 0);
    assert_eq!(inventory.count(id(WEAPON)), 2);
    let stacks = inventory.stacks(id(WEAPON));
    assert_eq!(stacks.len(), 2);
    assert_eq!(stacks[0].count(), 1);
    assert_eq!(stacks[1].count(), 1);
    assert_eq!(stacks[0].extra(37), Some(&quality(1.1)));
    assert_eq!(stacks[1].extra(37), Some(&quality(1.5)));
    for kind in [40, 155] {
        assert_eq!(stacks[0].extra(kind), stacks[1].extra(kind));
    }
    let events = pending_events(&game);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].0, "OnItemRemoved");
    assert_eq!(events[0].1[1], soe_papyrus::Value::Int(2));
}

#[test]
fn crf_01_tempering_failure_preserves_quality_ingredients_and_events() {
    let mut game = game(*b"WEAP", 2);
    game.world_mut()
        .reference(id(PLAYER))
        .unwrap()
        .inventory
        .restore(id(WEAPON), 2, vec![enriched_weapon(2)])
        .unwrap();
    game.world_mut().add_item(id(PLAYER), id(MATERIAL), 1);
    let before = game
        .world()
        .reference_ref(id(PLAYER))
        .unwrap()
        .inventory
        .clone();

    assert_eq!(
        game.temper(id(PLAYER), id(BENCH), id(RECIPE)),
        Err(Error::MissingIngredient {
            item: id(MATERIAL),
            needed: 2,
            available: 1,
        })
    );
    assert_eq!(
        game.world().reference_ref(id(PLAYER)).unwrap().inventory,
        before
    );
    assert!(pending_events(&game).is_empty());
}

#[test]
fn crf_01_tempering_accepts_weapons_and_armor_at_their_smithing_station() {
    for (tag, station) in [(*b"WEAP", 2), (*b"ARMO", 7)] {
        let mut game = game(tag, station);
        game.world_mut()
            .reference(id(PLAYER))
            .unwrap()
            .inventory
            .restore(id(WEAPON), 1, vec![enriched_weapon(1)])
            .unwrap();
        game.world_mut().add_item(id(PLAYER), id(MATERIAL), 2);
        game.world_mut()
            .set_actor_value(id(PLAYER), "smithing", 100.0)
            .unwrap();

        let result = game.temper(id(PLAYER), id(BENCH), id(RECIPE)).unwrap();
        assert_eq!(result.quality, 1.6);
        assert_eq!(
            game.world()
                .reference_ref(id(PLAYER))
                .unwrap()
                .inventory
                .stacks(id(WEAPON))[0]
                .extra(37),
            Some(&quality(1.6))
        );
    }
}
