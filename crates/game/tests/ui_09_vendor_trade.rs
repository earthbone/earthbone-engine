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
    plugin.push_top(Record::new(T(b"FLST"), LIST).sub(T(b"LNAM"), ITEM.to_le_bytes()));
    plugin.push_top(Record::new(T(b"MISC"), ITEM).zsub(T(b"EDID"), "VendorItem"));
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
            .sub(T(b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), MERCHANT)
            .zsub(T(b"EDID"), "MerchantRef")
            .sub(T(b"NAME"), MERCHANT_BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
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
