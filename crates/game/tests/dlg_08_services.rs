// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Planned check DLG-08: resolve vendor merchandise, gold and faction conditions.
//! FACT vendor fields follow `TES5Edit/Core/wbDefinitionsTES5.pas`. The
//! condition behavior follows the CK `ShowBarterMenu_-_Actor` service note.

use soe_esm::plugin::flags;
use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FACTION: u32 = 0x800;
const MERCHANT: u32 = 0x500;
const CONTAINER_REF: u32 = 0x501;
const CONTAINER_BASE: u32 = 0x400;
const LIST: u32 = 0x900;
const ITEM: u32 = 0x901;
const LEVELED: u32 = 0x902;
const LEVELED_ITEM: u32 = 0x903;
const GOLD: u32 = 0xF;
const GLOBAL: u32 = 0xA00;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn venv(start: u16, end: u16, stolen: bool, disabled: bool) -> Vec<u8> {
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&start.to_le_bytes());
    data.extend_from_slice(&end.to_le_bytes());
    data.extend_from_slice(&120_u16.to_le_bytes());
    data.extend_from_slice(&[0, 0, u8::from(stolen), u8::from(disabled), 0, 0]);
    data
}

fn faction(list: u32, container: u32, hours: (u16, u16)) -> Record {
    Record::new(T(b"FACT"), FACTION)
        .zsub(T(b"EDID"), "MerchantFaction")
        .sub(T(b"DATA"), (1_u32 << 14).to_le_bytes())
        .sub(T(b"VEND"), list.to_le_bytes())
        .sub(T(b"VENC"), container.to_le_bytes())
        .sub(T(b"VENV"), venv(hours.0, hours.1, true, false))
}

fn form_list(form: u32, items: &[u32]) -> Record {
    let mut record = Record::new(T(b"FLST"), form).zsub(T(b"EDID"), "VendorList");
    for item in items {
        record = record.sub(T(b"LNAM"), item.to_le_bytes());
    }
    record
}

fn plugin(vendor: Record, list_items: &[u32]) -> Plugin {
    let mut plugin = Plugin::new(FileHeader {
        flags: flags::MASTER,
        ..Default::default()
    });
    plugin.push_top(vendor);
    plugin.push_top(form_list(LIST, list_items));
    plugin.push_top(Record::new(T(b"MISC"), ITEM).zsub(T(b"EDID"), "VendorItem"));
    plugin.push_top(Record::new(T(b"MISC"), LEVELED_ITEM).zsub(T(b"EDID"), "ListItem"));
    plugin.push_top(Record::new(T(b"MISC"), GOLD).zsub(T(b"EDID"), "Gold001"));
    plugin.push_top(
        Record::new(T(b"LVLI"), LEVELED)
            .sub(T(b"LVLD"), [0])
            .sub(T(b"LVLF"), [0])
            .sub(T(b"LLCT"), [1])
            .sub(T(b"LVLO"), {
                let mut entry = Vec::from(1_u16.to_le_bytes());
                entry.extend_from_slice(&[0, 0]);
                entry.extend_from_slice(&LEVELED_ITEM.to_le_bytes());
                entry.extend_from_slice(&2_u16.to_le_bytes());
                entry.extend_from_slice(&[0, 0]);
                entry
            }),
    );
    plugin.push_top(
        Record::new(T(b"CONT"), CONTAINER_BASE)
            .zsub(T(b"EDID"), "MerchantChest")
            .sub(T(b"CNTO"), {
                let mut entry = GOLD.to_le_bytes().to_vec();
                entry.extend_from_slice(&37_i32.to_le_bytes());
                entry
            }),
    );
    plugin.push_top(
        Record::new(T(b"REFR"), CONTAINER_REF)
            .zsub(T(b"EDID"), "MerchantChestRef")
            .sub(T(b"NAME"), CONTAINER_BASE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"REFR"), MERCHANT)
            .zsub(T(b"EDID"), "MerchantRef")
            .sub(T(b"NAME"), CONTAINER_BASE.to_le_bytes()),
    );
    plugin.push_top(Record::new(T(b"GLOB"), GLOBAL).sub(T(b"FLTV"), 0.0_f32.to_le_bytes()));
    plugin
}

fn game(vendor: Record, items: &[u32]) -> Game {
    let mut order = LoadOrder::new();
    order.add("Vendor.esm", plugin(vendor, items)).unwrap();
    Game::new(order)
}

fn global_condition(expected: f32) -> Vec<u8> {
    let mut condition = vec![0; 32];
    condition[4..8].copy_from_slice(&expected.to_le_bytes());
    condition[8..10].copy_from_slice(&74_u16.to_le_bytes());
    condition[12..16].copy_from_slice(&GLOBAL.to_le_bytes());
    condition[20..24].copy_from_slice(&0_u32.to_le_bytes());
    condition
}

/// Winning FACT vendor links resolve the merchandise list and the merchant chest.
#[test]
fn dlg_08_winning_faction_resolves_merchandise_and_gold() {
    let mut game = game(faction(LIST, CONTAINER_REF, (0, 23)), &[ITEM, LEVELED]);
    let service = game
        .resolve_vendor_service(id(FACTION), id(MERCHANT))
        .unwrap()
        .unwrap();

    assert!(
        service.available,
        "an in-hours unconditional vendor is available"
    );
    assert_eq!(service.merchandise.count(id(ITEM)), 1);
    assert_eq!(service.merchandise.count(id(LEVELED_ITEM)), 2);
    assert_eq!(service.gold, 37, "gold comes from the merchant container");
    assert!(
        service.only_buys_stolen,
        "VENV stolen-item policy is retained"
    );
    assert_eq!(service.radius, 120, "VENV radius is retained");
}

/// A winning faction override supplies service links and conditional state.
#[test]
fn dlg_08_winning_override_controls_service_fields() {
    let mut base = plugin(faction(LIST, CONTAINER_REF, (0, 23)), &[ITEM]);
    base.push_top(Record::new(T(b"FLST"), 0x910).sub(T(b"LNAM"), ITEM.to_le_bytes()));
    let mut patch = Plugin::new(FileHeader {
        masters: vec!["Vendor.esm".into()],
        ..Default::default()
    });
    let mut override_faction = faction(0x910, 0x921, (0, 23));
    override_faction
        .subrecords
        .retain(|subrecord| subrecord.tag != T(b"VENV"));
    patch.push_top(
        override_faction
            .zsub(T(b"EDID"), "MerchantFaction")
            .sub(T(b"VENV"), venv(0, 23, false, false)),
    );
    patch.push_top(Record::new(T(b"MISC"), 0x911).zsub(T(b"EDID"), "OverrideItem"));
    patch.push_top(Record::new(T(b"FLST"), 0x910).sub(T(b"LNAM"), 0x911_u32.to_le_bytes()));
    patch.push_top(
        Record::new(T(b"CONT"), 0x920)
            .zsub(T(b"EDID"), "OverriddenChest")
            .sub(T(b"CNTO"), {
                let mut entry = GOLD.to_le_bytes().to_vec();
                entry.extend_from_slice(&91_i32.to_le_bytes());
                entry
            }),
    );
    patch.push_top(
        Record::new(T(b"REFR"), 0x921)
            .zsub(T(b"EDID"), "OverriddenChestRef")
            .sub(T(b"NAME"), 0x920_u32.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("Vendor.esm", base).unwrap();
    order.add("VendorPatch.esp", patch).unwrap();
    let mut game = Game::new(order);

    let service = game
        .resolve_vendor_service(id(FACTION), id(MERCHANT))
        .unwrap()
        .unwrap();
    let override_item = game.world_mut().form("OverrideItem").unwrap();
    let override_chest_ref = game.world_mut().form("OverriddenChestRef").unwrap();
    assert_eq!(
        service.merchandise.count(override_item),
        1,
        "the winning form list supplies its plugin-local item: {service:?}, {override_item:?}"
    );
    assert_eq!(service.merchandise.count(id(ITEM)), 0);
    assert_eq!(service.merchant_container, override_chest_ref);
    assert_eq!(
        service.gold, 91,
        "VENC resolves through the winning faction"
    );
    assert!(
        !service.only_buys_stolen,
        "the winning VENV field controls the policy"
    );
}

/// Factions without the FACT vendor flag do not create a service.
#[test]
fn dlg_08_non_vendor_faction_has_no_service() {
    let mut vendor = faction(LIST, CONTAINER_REF, (0, 23));
    if let Some(data) = vendor
        .subrecords
        .iter_mut()
        .find(|subrecord| subrecord.tag == T(b"DATA"))
    {
        data.data = 0_u32.to_le_bytes().to_vec();
    }
    let mut game = game(vendor, &[ITEM]);

    assert!(game
        .resolve_vendor_service(id(FACTION), id(MERCHANT))
        .unwrap()
        .is_none());
}

/// False faction CTDA conditions hide merchandise and prevent gold exposure.
#[test]
fn dlg_08_faction_conditions_gate_the_service() {
    let vendor = faction(LIST, CONTAINER_REF, (0, 23)).sub(T(b"CTDA"), global_condition(1.0));
    let mut game = game(vendor, &[ITEM]);
    let service = game
        .resolve_vendor_service(id(FACTION), id(MERCHANT))
        .unwrap()
        .unwrap();

    assert!(
        !service.conditions_met,
        "FACT CTDA compares against its resolved GLOB"
    );
    assert!(
        !service.available,
        "false faction conditions disable barter"
    );
    assert!(
        service.merchandise.is_empty(),
        "ineligible merchandise stays hidden"
    );
    assert_eq!(
        service.gold, 0,
        "an ineligible service exposes no merchant gold"
    );
}

/// VENV hours and its no-trade flag prevent an otherwise valid service.
#[test]
fn dlg_08_hours_and_trade_flag_gate_the_service() {
    let vendor = faction(LIST, CONTAINER_REF, (1, 2)).sub(T(b"VENV"), venv(1, 2, true, true));
    let mut game = game(vendor, &[ITEM]);
    let service = game
        .resolve_vendor_service(id(FACTION), id(MERCHANT))
        .unwrap()
        .unwrap();

    assert!(service.conditions_met, "empty FACT conditions pass");
    assert!(
        !service.within_hours,
        "the initial game hour is outside the service window"
    );
    assert!(
        !service.available,
        "the VENV no-trade flag disables service"
    );
    assert!(
        service.merchandise.is_empty(),
        "closed services expose no merchandise"
    );
}

/// A FACT with a malformed required vendor struct returns a typed error.
#[test]
fn dlg_08_malformed_vendor_record_is_rejected() {
    let mut vendor = faction(LIST, CONTAINER_REF, (0, 23));
    vendor
        .subrecords
        .retain(|subrecord| subrecord.tag != T(b"VENV"));
    let vendor = vendor.sub(T(b"VENV"), [0; 11]);
    let mut game = game(vendor, &[ITEM]);
    let error = game
        .resolve_vendor_service(id(FACTION), id(MERCHANT))
        .unwrap_err();
    assert!(
        error.to_string().contains("VENV"),
        "malformed vendor data has context"
    );
}
