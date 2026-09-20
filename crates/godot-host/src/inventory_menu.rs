// SPDX-License-Identifier: GPL-3.0-or-later

//! Form-backed inventory rows for the native host menu.

use soe_esm::{GlobalFormId, Record, Tag};
use soe_game::Game;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    All,
    Weapons,
    Apparel,
    Potions,
    Scrolls,
    Food,
    Ingredients,
    Books,
    Keys,
    Misc,
}

impl Category {
    pub const ALL: [Self; 10] = [
        Self::All,
        Self::Weapons,
        Self::Apparel,
        Self::Potions,
        Self::Scrolls,
        Self::Food,
        Self::Ingredients,
        Self::Books,
        Self::Keys,
        Self::Misc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Weapons => "Weapons",
            Self::Apparel => "Apparel",
            Self::Potions => "Potions",
            Self::Scrolls => "Scrolls",
            Self::Food => "Food",
            Self::Ingredients => "Ingredients",
            Self::Books => "Books",
            Self::Keys => "Keys",
            Self::Misc => "Misc",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub item: GlobalFormId,
    pub name: String,
    pub count: i32,
    pub equipped: bool,
    pub category: Category,
}

/// List a holder's positive inventory totals in form-id order.
pub fn entries(game: &Game, holder: GlobalFormId, category: Category) -> Vec<Entry> {
    let world = game.world();
    let equipped = world.actor_ref(holder).map(|actor| &actor.equipped);
    let Some(reference) = world.reference_ref(holder) else {
        return Vec::new();
    };
    reference
        .inventory
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(item, count)| {
            let category = category_for(world.record(*item));
            Entry {
                item: *item,
                name: world.name_of(*item).unwrap_or_else(|| format!("{item}")),
                count,
                equipped: equipped.is_some_and(|items| items.contains(item)),
                category,
            }
        })
        .filter(|entry| category == Category::All || entry.category == category)
        .collect()
}

/// Toggle an equipped weapon or armor through the shared game world.
pub fn toggle_equip(game: &mut Game, item: GlobalFormId) -> bool {
    let player = game.player();
    let mut world = game.world_mut();
    let Some(tag) = world.tag_of(item) else {
        return false;
    };
    if !matches!(tag.as_bytes(), b"WEAP" | b"ARMO") || world.item_count(player, item) <= 0 {
        return false;
    }
    if world
        .actor_ref(player)
        .is_some_and(|actor| actor.equipped.contains(&item))
    {
        world.unequip_item(player, item).is_ok()
    } else {
        world.equip_item(player, item).is_ok()
    }
}

fn category_for(record: Option<&Record>) -> Category {
    let Some(record) = record else {
        return Category::Misc;
    };
    match *record.tag.as_bytes() {
        [b'W', b'E', b'A', b'P'] | [b'A', b'M', b'M', b'O'] => Category::Weapons,
        [b'A', b'R', b'M', b'O'] => Category::Apparel,
        [b'A', b'L', b'C', b'H'] if alch_food(record) => Category::Food,
        [b'A', b'L', b'C', b'H'] => Category::Potions,
        [b'S', b'C', b'R', b'L'] => Category::Scrolls,
        [b'I', b'N', b'G', b'R'] => Category::Ingredients,
        [b'B', b'O', b'O', b'K'] => Category::Books,
        [b'K', b'E', b'Y', b'M'] => Category::Keys,
        _ => Category::Misc,
    }
}

fn alch_food(record: &Record) -> bool {
    record.find(Tag(*b"ENIT")).is_some_and(|data| {
        data.len() >= 8 && u32::from_le_bytes([data[4], data[5], data[6], data[7]]) & 2 != 0
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use soe_esm::{FileHeader, LoadOrder, Plugin};

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    fn game() -> Game {
        let mut plugin = Plugin::new(FileHeader::default());
        plugin.push_top(Record::new(Tag(*b"WEAP"), 0x100).zsub(Tag(*b"FULL"), "Sword"));
        for (object, name) in [(0x101, "Boots"), (0x103, "Other boots")] {
            plugin.push_top(
                Record::new(Tag(*b"ARMO"), object)
                    .zsub(Tag(*b"FULL"), name)
                    .sub(Tag(*b"DNAM"), 0_u32.to_le_bytes())
                    .sub(
                        Tag(*b"BOD2"),
                        [128_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
                    ),
            );
        }
        plugin.push_top(
            Record::new(Tag(*b"ALCH"), 0x102)
                .zsub(Tag(*b"FULL"), "Bread")
                .sub(
                    Tag(*b"ENIT"),
                    [0_i32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
                ),
        );
        let mut order = LoadOrder::new();
        order.add("Inventory.esm", plugin).unwrap();
        Game::new(order)
    }

    #[test]
    fn entries_filter_skyui_categories_and_equipment_uses_the_game_operation() {
        let mut game = game();
        let player = game.player();
        {
            let mut world = game.world_mut();
            world.add_item(player, id(0x100), 2);
            world.add_item(player, id(0x101), 1);
            world.add_item(player, id(0x102), 3);
        }
        assert_eq!(entries(&game, player, Category::Food)[0].name, "Bread");
        assert_eq!(entries(&game, player, Category::Weapons)[0].count, 2);
        assert!(toggle_equip(&mut game, id(0x100)));
        assert!(entries(&game, player, Category::Weapons)[0].equipped);
        assert!(toggle_equip(&mut game, id(0x100)));
        assert!(!entries(&game, player, Category::Weapons)[0].equipped);
        assert!(!toggle_equip(&mut game, id(0x102)));
        assert!(!toggle_equip(&mut game, id(0x999)));
    }

    #[test]
    fn equipment_replaces_shared_slots_and_unequip_clears_worn_instances() {
        use soe_game::inventory::{Extra, Stack};
        let mut game = game();
        let player = game.player();
        assert!(!toggle_equip(&mut game, id(0x103)));
        {
            let mut world = game.world_mut();
            world.add_item(player, id(0x101), 1);
            world.add_item(player, id(0x103), 1);
        }
        assert!(toggle_equip(&mut game, id(0x101)));
        assert!(toggle_equip(&mut game, id(0x103)));
        let rows = entries(&game, player, Category::Apparel);
        assert!(!rows[0].equipped);
        assert!(rows[1].equipped);
        {
            let mut world = game.world_mut();
            world
                .reference(player)
                .unwrap()
                .inventory
                .restore(
                    id(0x103),
                    1,
                    vec![Stack::new(
                        1,
                        vec![Extra {
                            kind: 22,
                            fields: vec![],
                        }],
                    )
                    .unwrap()],
                )
                .unwrap();
        }
        assert!(toggle_equip(&mut game, id(0x103)));
        assert!(!entries(&game, player, Category::Apparel)[1].equipped);
        assert!(game
            .world()
            .reference_ref(player)
            .unwrap()
            .inventory
            .stacks(id(0x103))
            .iter()
            .all(|stack| !stack.worn()));
    }
}
