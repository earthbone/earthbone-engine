// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::Game;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const FACTION: u32 = 0x800;
const MERCHANT: u32 = 0x500;
const CONTAINER: u32 = 0x501;
const CONTAINER_BASE: u32 = 0x400;
const MERCHANT_BASE: u32 = 0x502;
const LIST: u32 = 0x900;
const ITEM: u32 = 0x901;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn fixture() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    let mut vendor = Vec::from(0_u16.to_le_bytes());
    vendor.extend_from_slice(&24_u16.to_le_bytes());
    vendor.extend_from_slice(&120_u16.to_le_bytes());
    vendor.extend_from_slice(&[0xff, 0xff, 0, 0, 0, 0]);
    let mut faction = Record::new(T(b"FACT"), FACTION)
        .sub(T(b"DATA"), (1_u32 << 14).to_le_bytes())
        .sub(T(b"VEND"), LIST.to_le_bytes())
        .sub(T(b"VENC"), CONTAINER.to_le_bytes())
        .sub(T(b"VENV"), vendor);
    let mut location = Vec::from(0_u32.to_le_bytes());
    location.extend_from_slice(&CONTAINER.to_le_bytes());
    location.extend_from_slice(&[0; 4]);
    faction = faction.sub(T(b"PLVD"), location);
    plugin.push_top(faction);
    plugin.push_top(Record::new(T(b"FLST"), LIST).sub(T(b"LNAM"), ITEM.to_le_bytes()));
    plugin.push_top(Record::new(T(b"MISC"), ITEM));
    plugin.push_top(
        Record::new(T(b"CONT"), CONTAINER_BASE)
            .sub(T(b"CNTO"), [ITEM.to_le_bytes().as_slice(), &2_i32.to_le_bytes()].concat()),
    );
    plugin.push_top(
        Record::new(T(b"REFR"), CONTAINER)
            .sub(T(b"NAME"), CONTAINER_BASE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(T(b"NPC_"), MERCHANT_BASE).sub(T(b"ACBS"), vec![0; 24]),
    );
    plugin.push_top(
        Record::new(T(b"ACHR"), MERCHANT)
            .sub(T(b"NAME"), MERCHANT_BASE.to_le_bytes())
            .sub(T(b"DATA"), vec![0; 24]),
    );
    let mut order = LoadOrder::new();
    order.add("Vendor.esm", plugin).unwrap();
    let mut game = Game::new(order);
    assert!(game.world_mut().drive_actor(id(MERCHANT)));
    game.world_mut().reference(id(CONTAINER)).unwrap().position = [200.0, 0.0, 0.0];
    game
}

#[test]
fn ui_09_vendor_radius_ignores_unknown_venv_bytes() {
    let mut game = fixture();
    let service = game.resolve_vendor_service(id(FACTION), id(MERCHANT)).unwrap().unwrap();

    assert!(!service.available, "resolved radius was {}", service.radius);
    assert_eq!(service.radius, 120);
}
