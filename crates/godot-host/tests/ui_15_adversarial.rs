// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Adversarial checks for gift menu rows.

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::inventory::{Extra, Field, Stack};
use soe_game::world::GiftMenuRequest;
use soe_game::Game;
use soe_godot_host::inventory_menu::{gift_entries, Entry};
use soe_papyrus::vm::LatentId;

const ACTOR_BASE: u32 = 0x900;
const ACTOR_REF: u32 = 0x901;
const DIRECT_ITEM: u32 = 0x902;
const KEYWORD_ITEM: u32 = 0x903;
const UNMATCHED_ITEM: u32 = 0x904;
const KEYWORD: u32 = 0x905;
const FILTER: u32 = 0x906;

fn id(object: u32) -> GlobalFormId {
    GlobalFormId { file: 0, object }
}

fn game() -> Game {
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR_BASE));
    plugin.push_top(
        Record::new(Tag(*b"ACHR"), ACTOR_REF).sub(Tag(*b"NAME"), ACTOR_BASE.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"MISC"), DIRECT_ITEM)
            .zsub(Tag(*b"FULL"), "Listed item")
            .sub(Tag(*b"DATA"), 5_i32.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"MISC"), KEYWORD_ITEM)
            .zsub(Tag(*b"FULL"), "Keyword item")
            .sub(Tag(*b"DATA"), 7_i32.to_le_bytes())
            .sub(Tag(*b"KWDA"), KEYWORD.to_le_bytes()),
    );
    plugin.push_top(
        Record::new(Tag(*b"MISC"), UNMATCHED_ITEM)
            .zsub(Tag(*b"FULL"), "Unmatched item")
            .sub(Tag(*b"DATA"), 9_i32.to_le_bytes()),
    );
    plugin.push_top(Record::new(Tag(*b"KYWD"), KEYWORD));
    plugin.push_top(
        Record::new(Tag(*b"FLST"), FILTER)
            .sub(Tag(*b"LNAM"), DIRECT_ITEM.to_le_bytes())
            .sub(Tag(*b"LNAM"), KEYWORD.to_le_bytes()),
    );
    let mut order = LoadOrder::new();
    order.add("GiftMenuAdversarial.esm", plugin).unwrap();
    Game::new(order)
}

fn request(filter: Option<GlobalFormId>, show_stolen: bool) -> GiftMenuRequest {
    GiftMenuRequest {
        latent: LatentId(1),
        giver: id(ACTOR_REF),
        source: id(0x14),
        target: id(ACTOR_REF),
        filter,
        show_stolen,
        use_favor_points: true,
        favor_points: 0,
    }
}

#[test]
fn ui_15_adversarial_filter_includes_form_and_keyword_members_only() {
    let mut game = game();
    let player = game.player();
    {
        let mut world = game.world_mut();
        world.add_item(player, id(DIRECT_ITEM), 1);
        world.add_item(player, id(KEYWORD_ITEM), 2);
        world.add_item(player, id(UNMATCHED_ITEM), 3);
    }

    let rows = gift_entries(&game, &request(Some(id(FILTER)), false));

    assert_eq!(
        rows.iter()
            .map(|row| (row.item, row.count))
            .collect::<Vec<_>>(),
        [(id(DIRECT_ITEM), 1), (id(KEYWORD_ITEM), 2)]
    );
}

#[test]
fn ui_15_adversarial_unresolved_filter_shows_no_inventory_rows() {
    let mut game = game();
    let player = game.player();
    game.world_mut().add_item(player, id(DIRECT_ITEM), 1);

    assert!(gift_entries(&game, &request(Some(id(0x9ff)), false)).is_empty());
}

#[test]
fn ui_15_adversarial_hides_stolen_stack_but_lists_unowned_stack_of_same_form() {
    let mut game = game();
    let player = game.player();
    game.world_mut()
        .reference(player)
        .unwrap()
        .inventory
        .restore(
            id(DIRECT_ITEM),
            2,
            vec![
                Stack::new(
                    1,
                    vec![Extra {
                        kind: 33,
                        fields: vec![Field::Form(Some(id(0x999)))],
                    }],
                )
                .unwrap(),
                Stack::new(1, vec![]).unwrap(),
            ],
        )
        .unwrap();

    let rows = gift_entries(&game, &request(None, false));

    assert_eq!(
        rows.iter()
            .map(|Entry { item, count, .. }| (*item, *count))
            .collect::<Vec<_>>(),
        [(id(DIRECT_ITEM), 1)],
        "stolen stacks are hidden without hiding the eligible stack sharing their base form"
    );
}
