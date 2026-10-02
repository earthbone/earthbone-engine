// SPDX-License-Identifier: GPL-3.0-or-later
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use soe_esm::{FileHeader, GlobalFormId, LoadOrder, Plugin, Record, Tag};
use soe_game::{
    Game,
    crafting::Error,
    inventory::{Extra, Field, StackSource},
};

const PLAYER: u32 = 0x14;
const ACTOR: u32 = 0x800;
const ARMOR: u32 = 0x801;
const GEM: u32 = 0x802;
const ENCHANTMENT: u32 = 0x803;
const EFFECT: u32 = 0x804;

fn id(raw: u32) -> GlobalFormId {
    GlobalFormId {
        file: 0,
        object: raw,
    }
}

#[test]
fn staff_enchantment_cannot_be_applied_to_armor() {
    let mut staff_enit = vec![0; 36];
    staff_enit[20..24].copy_from_slice(&12_u32.to_le_bytes());
    let mut plugin = Plugin::new(FileHeader::default());
    plugin.push_top(Record::new(Tag(*b"NPC_"), ACTOR));
    plugin.push_top(Record::new(Tag(*b"ACHR"), PLAYER).sub(Tag(*b"NAME"), ACTOR.to_le_bytes()));
    plugin.push_top(Record::new(Tag(*b"ARMO"), ARMOR));
    plugin.push_top(Record::new(Tag(*b"SLGM"), GEM).sub(Tag(*b"SLCP"), [5]));
    plugin.push_top(
        Record::new(Tag(*b"ENCH"), ENCHANTMENT)
            .sub(Tag(*b"ENIT"), staff_enit)
            .sub(Tag(*b"EFID"), EFFECT.to_le_bytes())
            .sub(Tag(*b"EFIT"), [0; 12]),
    );
    plugin.push_top(Record::new(Tag(*b"MGEF"), EFFECT));
    let mut order = LoadOrder::new();
    order.add("StaffEnchant.esm", plugin).unwrap();
    let mut game = Game::new(order);
    game.world_mut().add_item(id(PLAYER), id(ARMOR), 1);
    game.world_mut().add_item(id(PLAYER), id(GEM), 1);
    let gem = {
        let mut world = game.world_mut();
        let gem = world
            .select_item(id(PLAYER), id(GEM), StackSource::Plain, 1)
            .unwrap();
        let gem = world
            .reference(id(PLAYER))
            .unwrap()
            .inventory
            .update_selected(
                &gem,
                vec![Extra {
                    kind: 156,
                    fields: vec![Field::Byte(5)],
                }],
            )
            .unwrap();
        gem
    };
    let item = game
        .world_mut()
        .select_item(id(PLAYER), id(ARMOR), StackSource::Plain, 1)
        .unwrap();

    assert_eq!(
        game.enchant_item(id(PLAYER), &item, &gem, id(ENCHANTMENT), &[id(ENCHANTMENT)]),
        Err(Error::EnchantItem)
    );
    assert_eq!(game.world_mut().item_count(id(PLAYER), id(GEM)), 1);
}
