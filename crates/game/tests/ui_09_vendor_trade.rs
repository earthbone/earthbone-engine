// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! UI-09 vendor purchases follow xEdit FACT vendor fields and SkyUI's
//! BarterMenu quantity confirmation and adjusted per-item price contract.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FACTION: u32 = 0x800;
const MERCHANT: u32 = 0x500;
const CHEST: u32 = 0x501;
const CHEST_BASE: u32 = 0x400;
const MERCHANT_BASE: u32 = 0x502;
const LIST: u32 = 0x900;
const ITEM: u32 = 0x901;
const GOLD: u32 = 0xF;
const PLAYER_GOLD: i32 = 25;
const MERCHANT_GOLD: i32 = 50;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(
        Record::new(T(b"FACT"), FACTION)
            .zsub(T(b"EDID"), "VendorFaction")
            .sub(T(b"DATA"), (1_u32 << 14).to_le_bytes())
            .sub(T(b"VEND"), LIST.to_le_bytes())
            .sub(T(b"VENC"), CHEST.to_le_bytes())
            .sub(T(b"VENV"), {
                let mut data = Vec::new();
                data.extend_from_slice(&0_u16.to_le_bytes());
                data.extend_from_slice(&23_u16.to_le_bytes());
                data.extend_from_slice(&120_u32.to_le_bytes());
                data.extend_from_slice(&[0, 0, 0, 0]);
                data
            }),
    );
    plugin.push_top(
        Record::new(T(b"FLST"), LIST)
            .sub(T(b"LNAM"), ITEM.to_le_bytes())
            .sub(T(b"LNAM"), 0x902_u32.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"MISC"), 0x902).sub(
        T(b"DATA"),
        [1_i32.to_le_bytes(), 0.0_f32.to_le_bytes()].concat(),
    ));
    plugin.push_top(
        Record::new(T(b"MISC"), ITEM)
            .zsub(T(b"EDID"), "VendorItem")
            .sub(
                T(b"DATA"),
                [25_i32.to_le_bytes(), 1.0_f32.to_le_bytes()].concat(),
            ),
    );
    plugin.push_top(Record::new(T(b"MISC"), GOLD).zsub(T(b"EDID"), "Gold001"));
    plugin.push_top(
        Record::new(T(b"CONT"), CHEST_BASE)
            .zsub(T(b"EDID"), "MerchantChest")
            .sub(T(b"CNTO"), {
                let mut entry = ITEM.to_le_bytes().to_vec();
                entry.extend_from_slice(&3_i32.to_le_bytes());
                entry
            })
            .sub(T(b"CNTO"), {
                let mut entry = GOLD.to_le_bytes().to_vec();
                entry.extend_from_slice(&MERCHANT_GOLD.to_le_bytes());
                entry
            }),
    );
    plugin.push_top(
        Record::new(T(b"REFR"), CHEST)
            .zsub(T(b"EDID"), "MerchantChestRef")
            .sub(T(b"NAME"), CHEST_BASE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"NPC_"), MERCHANT_BASE)
            .zsub(T(b"EDID"), "Merchant")
            .sub(
                T(b"SNAM"),
                [FACTION.to_le_bytes().as_slice(), &[0, 0, 0, 0]].concat(),
            )
            .sub(T(b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), MERCHANT)
            .zsub(T(b"EDID"), "MerchantRef")
            .sub(T(b"NAME"), MERCHANT_BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut perk = Record::new(T(b"PERK"), 0xA00).sub(T(b"DATA"), [0, 0, 1, 1, 0]);
    for (entry, multiplier) in [(8, 1.0_f32 / 1.1), (60, 1.1_f32)] {
        perk = perk
            .sub(T(b"PRKE"), [2, 0, 0])
            .sub(T(b"DATA"), [entry, 3, 2])
            .sub(T(b"EPFT"), [1])
            .sub(T(b"EPFD"), multiplier.to_le_bytes())
            .sub(T(b"PRKF"), []);
    }
    plugin.push_top(perk);
    let mut order = LoadOrder::new();
    order.add("Ui09.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(MERCHANT)));
    let player = game.player();
    game.world_mut().add_item(player, id(GOLD), PLAYER_GOLD);
    game
}

#[test]
fn ui_09_vendor_purchase_transfers_item_and_gold_together() {
    let mut game = fixture();
    let player = game.player();

    game.buy_vendor_item(id(FACTION), id(MERCHANT), id(ITEM), 2, 10)
        .unwrap();

    assert_eq!(game.world_mut().item_count(player, id(ITEM)), 2);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(ITEM)), 1);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 5);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 70);
}

#[test]
fn ui_09_rejected_vendor_purchase_preserves_items_and_gold() {
    let mut game = fixture();
    let player = game.player();
    game.world_mut().add_item(player, id(GOLD), -20);

    let result = game.buy_vendor_item(id(FACTION), id(MERCHANT), id(ITEM), 2, 10);

    assert!(result.is_err());
    assert_eq!(game.world_mut().item_count(player, id(ITEM)), 0);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(ITEM)), 3);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 5);
    assert_eq!(
        game.world_mut().item_count(id(CHEST), id(GOLD)),
        MERCHANT_GOLD
    );
}

#[test]
fn ui_09_overflowing_vendor_purchase_preserves_items_and_gold() {
    let mut game = fixture();
    let player = game.player();
    game.world_mut().add_item(player, id(ITEM), i32::MAX);

    let result = game.buy_vendor_item(id(FACTION), id(MERCHANT), id(ITEM), 1, 10);

    assert!(result.is_err());
    assert_eq!(game.world_mut().item_count(player, id(ITEM)), i32::MAX);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(ITEM)), 3);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), PLAYER_GOLD);
    assert_eq!(
        game.world_mut().item_count(id(CHEST), id(GOLD)),
        MERCHANT_GOLD
    );
}

#[test]
fn ui_31_prices_use_speech_perks_fortify_and_round_each_item() {
    use soe_game::barter::Side::{Buy, Sell};
    let mut game = fixture();
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 15.0)
        .unwrap();
    let prices = game.barter_prices(id(MERCHANT)).unwrap();
    assert_eq!(prices.unit_price(25, Buy).unwrap(), 78);
    assert_eq!(prices.unit_price(500, Buy).unwrap(), 1553);
    assert_eq!(prices.unit_price(25, Sell).unwrap(), 8);
    game.world_mut()
        .actor(player)
        .unwrap()
        .perks
        .insert(id(0xA00));
    let prices = game.barter_prices(id(MERCHANT)).unwrap();
    assert_eq!(prices.unit_price(25, Buy).unwrap(), 71);
    assert_eq!(prices.unit_price(25, Sell).unwrap(), 9);
    game.world_mut()
        .set_actor_value(player, "speechcraftmodifier", 20.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(player, "speechcraftpowermodifier", 10.0)
        .unwrap();
    let prices = game.barter_prices(id(MERCHANT)).unwrap();
    assert_eq!(prices.unit_price(25, Buy).unwrap(), 51);
    assert_eq!(prices.unit_price(25, Sell).unwrap(), 12);
    game.world_mut()
        .set_actor_value(player, "speechcraft", 200.0)
        .unwrap();
    game.world_mut()
        .set_actor_value(player, "speechcraftmodifier", 100.0)
        .unwrap();
    let prices = game.barter_prices(id(MERCHANT)).unwrap();
    assert_eq!(prices.unit_price(100, Buy).unwrap(), 105);
    assert_eq!(prices.unit_price(100, Sell).unwrap(), 95);
    assert!(prices.unit_price(-1, Buy).is_err());
    assert!(prices.unit_price(i32::MAX, Buy).is_err());
}

#[test]
fn ui_31_buy_sell_and_rejection_use_shared_inventory_and_gold() {
    use soe_game::barter::Side::{Buy, Sell};
    let mut game = fixture();
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 15.0)
        .unwrap();
    assert!(game
        .trade_vendor_item(id(FACTION), id(MERCHANT), id(ITEM), 1, Buy)
        .is_err());
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 25);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(ITEM)), 3);
    game.world_mut().add_item(player, id(GOLD), 250);
    game.trade_vendor_item(id(FACTION), id(MERCHANT), id(ITEM), 1, Buy)
        .unwrap();
    assert_eq!(game.world_mut().item_count(player, id(ITEM)), 1);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(ITEM)), 2);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 197);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 128);
    game.trade_vendor_item(id(FACTION), id(MERCHANT), id(ITEM), 1, Sell)
        .unwrap();
    assert_eq!(game.world_mut().item_count(player, id(ITEM)), 0);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(ITEM)), 3);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), 205);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(GOLD)), 120);
    let before = game.world_mut().item_count(player, id(GOLD));
    assert!(game
        .trade_vendor_item(id(FACTION), id(MERCHANT), id(GOLD), 1, Sell)
        .is_err());
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), before);
}

#[test]
fn ui_31_winning_settings_change_quotes_and_bad_settings_reject() {
    use soe_game::barter::Side::{Buy, Sell};
    let mut game = fixture();
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 50.0)
        .unwrap();
    game.world_mut()
        .settings
        .insert("fbartermax".into(), soe_papyrus::Value::Float(4.0));
    game.world_mut()
        .settings
        .insert("fbartermin".into(), soe_papyrus::Value::Float(2.0));
    let prices = game.barter_prices(id(MERCHANT)).unwrap();
    assert_eq!(prices.unit_price(25, Buy).unwrap(), 75);
    assert_eq!(prices.unit_price(25, Sell).unwrap(), 8);
    game.world_mut()
        .settings
        .insert("fbartermin".into(), soe_papyrus::Value::Float(f32::NAN));
    assert!(game.barter_prices(id(MERCHANT)).is_err());
}

#[test]
fn ui_31_trades_clear_ownership_and_equipment_but_keep_other_extras() {
    use soe_game::inventory::{Extra, Field, Inventory, Stack};
    let mut game = fixture();
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.set_actor_value(player, "speechcraft", 15.0).unwrap();
        world.add_item(player, id(GOLD), 250);
        let chest = world.reference(id(CHEST)).unwrap();
        chest.inventory = Inventory::default();
        chest.inventory.add(id(GOLD), 50).unwrap();
        chest
            .inventory
            .restore(
                id(ITEM),
                1,
                vec![Stack::new(
                    1,
                    vec![
                        Extra {
                            kind: 33,
                            fields: vec![Field::Form(Some(id(FACTION)))],
                        },
                        Extra {
                            kind: 22,
                            fields: vec![],
                        },
                        Extra {
                            kind: 37,
                            fields: vec![Field::FloatBits(1.5_f32.to_bits())],
                        },
                    ],
                )
                .unwrap()],
            )
            .unwrap();
    }
    game.trade_vendor_item(
        id(FACTION),
        id(MERCHANT),
        id(ITEM),
        1,
        soe_game::barter::Side::Buy,
    )
    .unwrap();
    {
        let mut world = game.world_mut();
        let stack = &world.reference(player).unwrap().inventory.stacks(id(ITEM))[0];
        assert!(!stack.worn());
        assert!(stack.extra(33).is_none());
        assert_eq!(
            stack.extra(37).unwrap().fields,
            vec![Field::FloatBits(1.5_f32.to_bits())]
        );
        world.actor(player).unwrap().equipped.insert(id(ITEM));
    }
    game.trade_vendor_item(
        id(FACTION),
        id(MERCHANT),
        id(ITEM),
        1,
        soe_game::barter::Side::Sell,
    )
    .unwrap();
    assert!(!game
        .world_mut()
        .actor(player)
        .unwrap()
        .equipped
        .contains(&id(ITEM)));
}

#[test]
fn ui_31_zero_gold_sale_moves_item_without_changing_wallets() {
    let mut game = fixture();
    let player = game.player();
    game.world_mut()
        .set_actor_value(player, "speechcraft", 15.0)
        .unwrap();
    game.world_mut().add_item(player, id(0x902), 1);
    game.trade_vendor_item(
        id(FACTION),
        id(MERCHANT),
        id(0x902),
        1,
        soe_game::barter::Side::Sell,
    )
    .unwrap();
    assert_eq!(game.world_mut().item_count(player, id(0x902)), 0);
    assert_eq!(game.world_mut().item_count(id(CHEST), id(0x902)), 1);
    assert_eq!(game.world_mut().item_count(player, id(GOLD)), PLAYER_GOLD);
    assert_eq!(
        game.world_mut().item_count(id(CHEST), id(GOLD)),
        MERCHANT_GOLD
    );
    game.world_mut()
        .actor(id(MERCHANT))
        .unwrap()
        .factions
        .remove(&id(FACTION));
    assert!(matches!(
        game.trade_vendor_item(
            id(FACTION),
            id(MERCHANT),
            id(ITEM),
            1,
            soe_game::barter::Side::Buy
        ),
        Err(soe_game::services::TradeError::Unavailable)
    ));
}

#[test]
fn ui_31_service_condition_checks_hours_without_materializing_stock() {
    use soe_esm::records::gameplay::{Comparison, Condition};
    use soe_game::conditions::{self, Context};
    let game = fixture();
    let condition = Condition {
        flags: 2,
        comparison: Comparison::Number(1.0),
        function: 255,
        parameters: [0, 0],
        run_on: 0,
        reference: 0,
        parameter_3: -1,
        strings: [None, None],
    };
    let context = Context {
        owner: id(FACTION),
        subject: id(MERCHANT),
        target: None,
        quest: None,
        tempering_enchanted: None,
        tempering_item: None,
        alchemy_making_poison: None,
        alchemy_effect: None,
        magic_item: None,
        dual_cast: None,
        skill_use: None,
        light_level: None,
    };
    assert!(game.world().reference_ref(id(CHEST)).is_none());
    game.world_mut()
        .actor(id(MERCHANT))
        .unwrap()
        .factions
        .insert(id(FACTION), -1);
    let mut membership = condition.clone();
    membership.function = 71;
    membership.parameters[0] = FACTION;
    assert_eq!(
        conditions::evaluate_one(&mut game.world_mut(), &game.vm, context, &membership),
        Ok(true)
    );
    assert_eq!(
        conditions::evaluate_one(&mut game.world_mut(), &game.vm, context, &condition),
        Ok(true)
    );
    assert!(game.world().reference_ref(id(CHEST)).is_none());
    game.world_mut().clock.game_days = 23.5 / 24.0;
    assert_eq!(
        conditions::evaluate_one(&mut game.world_mut(), &game.vm, context, &condition),
        Ok(false)
    );
    game.world_mut().clock.game_days = 0.5;
    game.world_mut()
        .actor(id(MERCHANT))
        .unwrap()
        .factions
        .remove(&id(FACTION));
    assert_eq!(
        conditions::evaluate_one(&mut game.world_mut(), &game.vm, context, &condition),
        Ok(false)
    );
}
