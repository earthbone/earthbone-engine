// SPDX-License-Identifier: GPL-3.0-or-later

//! Transient native menu views from SkyUI-Community's ItemMenus data setters.
//! Record field offsets follow local xEdit and UESP, never copied AS2 code.

use soe_esm::{GlobalFormId, Record, Tag};
use soe_game::Game;

use crate::inventory_menu::{self, Category};

/// A presentation snapshot. Actions always resolve its form in Game again.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub form: GlobalFormId,
    pub name: String,
    pub category: String,
    pub icon: String,
    pub count: i32,
    pub equipped: bool,
    pub favorite: bool,
    pub value: i32,
    pub weight: Option<f32>,
    pub damage: Option<u16>,
    pub armor: Option<f32>,
    pub cost: Option<u32>,
    pub description: String,
}

/// Read all positive stacks without storing an authoritative menu inventory.
pub fn inventory(game: &Game, holder: GlobalFormId) -> Vec<Row> {
    inventory_menu::entries(game, holder, Category::All)
        .into_iter()
        .map(|entry| {
            let mut row = item(game, entry.item, entry.count);
            row.category = entry.category.label().into();
            row.equipped = entry.equipped;
            row
        })
        .collect()
}

/// Read the known hand-castable spells supported by the existing magic menu.
pub fn magic(game: &Game) -> Vec<Row> {
    let player = game.world_mut().player();
    crate::magic_menu::entries(game, player)
        .into_iter()
        .map(|entry| {
            let mut row = item(game, entry.spell, 1);
            row.category = "Spells".into();
            row.icon = "default_spell".into();
            row.cost = Some(entry.cost);
            row.equipped = entry.left_equipped || entry.right_equipped;
            row
        })
        .collect()
}

/// Resolve one merchandise or inventory form's current winning fields.
pub fn item(game: &Game, form: GlobalFormId, count: i32) -> Row {
    let player = game.world_mut().player();
    let world = game.world();
    let record = world.record(form);
    let category = if record.is_some_and(|record| record.tag == Tag(*b"SPEL")) {
        "Spells"
    } else {
        inventory_menu::category_for(record).label()
    };
    let icon = match category {
        "Weapons" => "default_weapon",
        "Apparel" => "default_armor",
        "Potions" => "default_potion",
        "Food" => "default_food",
        "Ingredients" => "default_ingredient",
        "Books" => "default_book",
        "Keys" => "default_key",
        "Scrolls" => "default_scroll",
        "Spells" => "default_spell",
        _ => "default_misc",
    };
    Row {
        form,
        name: crate::item_card::display_name(&world, form),
        category: category.into(),
        icon: icon.into(),
        count,
        equipped: false,
        favorite: world
            .actor_ref(player)
            .is_some_and(|actor| actor.favorites.contains(&form)),
        value: world.item_value(form),
        weight: record.and_then(weight),
        damage: record.and_then(|record| {
            soe_game::combat::records::Weapon::from_record(record)
                .ok()
                .map(|weapon| weapon.damage)
        }),
        armor: record
            .filter(|record| record.tag == Tag(*b"ARMO"))
            .and_then(|record| {
                let bytes: [u8; 4] = record.find(Tag(*b"DNAM"))?.try_into().ok()?;
                Some(u32::from_le_bytes(bytes) as f32 / 100.0)
            }),
        cost: None,
        description: world
            .description_of(form)
            .filter(|text| !text.starts_with("<string "))
            .unwrap_or_default(),
    }
}

fn weight(record: &Record) -> Option<f32> {
    let offset = match record.tag.as_bytes() {
        b"WEAP" | b"ARMO" | b"MISC" | b"INGR" | b"KEYM" | b"SCRL" | b"SLGM" => 4,
        b"BOOK" => 12,
        b"ALCH" => 0,
        b"AMMO" => 16,
        _ => return None,
    };
    let bytes = record.find(Tag(*b"DATA"))?.get(offset..offset + 4)?;
    let value = f32::from_le_bytes(bytes.try_into().ok()?);
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// Toggle the existing actor favorite set for a carried item or known spell.
pub fn toggle_favorite(game: &Game, form: GlobalFormId) -> bool {
    let player = game.world_mut().player();
    let mut world = game.world_mut();
    let carried = world.item_count(player, form) > 0;
    let Some(actor) = world.actor(player) else {
        return false;
    };
    if !carried && !actor.spells.contains(&form) {
        return false;
    }
    if !actor.favorites.remove(&form) {
        actor.favorites.insert(form);
    }
    true
}

/// Snapshot current merchant merchandise and eligible player stacks.
pub fn barter(game: &mut Game, faction: GlobalFormId, merchant: GlobalFormId) -> Vec<(Row, i64)> {
    let Ok(Some(service)) = game.resolve_vendor_service(faction, merchant) else {
        return Vec::new();
    };
    if !service.available {
        return Vec::new();
    }
    let Ok(prices) = game.barter_prices(merchant) else {
        return Vec::new();
    };
    let mut rows: Vec<_> = service
        .merchandise
        .iter()
        .filter_map(|(form, count)| {
            let mut row = item(game, *form, count);
            row.value = prices
                .unit_price(row.value, soe_game::barter::Side::Buy)
                .ok()?;
            Some((row, 0))
        })
        .collect();
    let player = game.player();
    rows.extend(
        inventory(game, player)
            .into_iter()
            .filter(|row| game.vendor_accepts_sale(&service, row.form))
            .filter_map(|mut row| {
                row.value = prices
                    .unit_price(row.value, soe_game::barter::Side::Sell)
                    .ok()?;
                Some((row, 1))
            }),
    );
    rows
}

/// Exchange one selected stack at the current shared barter quote.
/// Invalid service, stock, ownership, quantity or gold leaves both inventories intact.
pub fn trade(
    game: &mut Game,
    faction: GlobalFormId,
    merchant: GlobalFormId,
    item: GlobalFormId,
    count: i32,
    side: i64,
) -> bool {
    let side = match side {
        0 => soe_game::barter::Side::Buy,
        1 => soe_game::barter::Side::Sell,
        _ => return false,
    };
    game.trade_vendor_item(faction, merchant, item, count, side)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_20_missing_localization_never_displays_table_debug_markers() {
        let mut plugin = soe_esm::Plugin::new(soe_esm::FileHeader {
            flags: soe_esm::plugin::flags::LOCALIZED,
            ..Default::default()
        });
        plugin.push_top(
            Record::new(Tag(*b"MISC"), 0x800)
                .sub(Tag(*b"FULL"), 1_u32.to_le_bytes())
                .sub(Tag(*b"DESC"), 0_u32.to_le_bytes()),
        );
        let mut order = soe_esm::LoadOrder::new();
        order.add("MissingStrings.esm", plugin).unwrap();
        let game = Game::new(order);
        let row = item(
            &game,
            GlobalFormId {
                file: 0,
                object: 0x800,
            },
            1,
        );
        assert_eq!(row.name, "Unnamed item");
        assert!(row.description.is_empty());
    }
}
