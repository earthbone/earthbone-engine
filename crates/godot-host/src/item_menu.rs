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
    let category = record.map_or("Misc", |record| match record.tag.as_bytes() {
        b"WEAP" | b"AMMO" => "Weapons",
        b"ARMO" => "Apparel",
        b"ALCH" => "Potions",
        b"INGR" => "Ingredients",
        b"BOOK" => "Books",
        b"KEYM" => "Keys",
        b"SCRL" => "Scrolls",
        b"SPEL" => "Spells",
        _ => "Misc",
    });
    let icon = match category {
        "Weapons" => "default_weapon",
        "Apparel" => "default_armor",
        "Potions" => "default_potion",
        "Ingredients" => "default_ingredient",
        "Books" => "default_book",
        "Keys" => "default_key",
        "Scrolls" => "default_scroll",
        "Spells" => "default_spell",
        _ => "default_misc",
    };
    Row {
        form,
        name: world.name_of(form).unwrap_or_else(|| form.to_string()),
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
        description: world.description_of(form).unwrap_or_default(),
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

fn accepted_sale(
    game: &Game,
    service: &soe_game::services::VendorService,
    item: GlobalFormId,
) -> bool {
    let player = game.world_mut().player();
    let mut world = game.world_mut();
    let currency = world.form("Gold001").unwrap_or(GlobalFormId {
        file: 0,
        object: 15,
    });
    if item == currency || world.item_value(item) <= 0 || service.only_buys_stolen {
        return false;
    }
    let list = world
        .form_list(service.buy_sell_list)
        .cloned()
        .unwrap_or_default();
    let listed = list.contains(&item)
        || world
            .keywords(item)
            .iter()
            .any(|keyword| list.contains(keyword));
    let owned = world.reference(player).is_some_and(|reference| reference.inventory.stacks(item).iter().any(|stack| {
        matches!(stack.extra(33).map(|extra| extra.fields.as_slice()), Some([soe_game::inventory::Field::Form(Some(owner))]) if *owner != player)
    }));
    listed != service.not_sell_buy && !owned
}

/// Snapshot current merchant merchandise and eligible player stacks.
pub fn barter(game: &mut Game, faction: GlobalFormId, merchant: GlobalFormId) -> Vec<(Row, i64)> {
    let Ok(Some(service)) = game.resolve_vendor_service(faction, merchant) else {
        return Vec::new();
    };
    if !service.available {
        return Vec::new();
    }
    let mut rows: Vec<_> = service
        .merchandise
        .iter()
        .map(|(form, count)| (item(game, *form, count), 0))
        .collect();
    let player = game.player();
    rows.extend(
        inventory(game, player)
            .into_iter()
            .filter(|row| accepted_sale(game, &service, row.form))
            .map(|row| (row, 1)),
    );
    rows
}

/// Exchange one selected stack at the record value, the default UI multiplier.
/// Invalid service, stock, ownership, quantity or gold leaves both inventories intact.
pub fn trade(
    game: &mut Game,
    faction: GlobalFormId,
    merchant: GlobalFormId,
    item: GlobalFormId,
    count: i32,
    side: i64,
) -> bool {
    let price = game.world().item_value(item);
    if count <= 0 || price <= 0 {
        return false;
    }
    if side == 0 {
        return game
            .buy_vendor_item(faction, merchant, item, count, price)
            .is_ok();
    }
    if side != 1 {
        return false;
    }
    let Ok(Some(service)) = game.resolve_vendor_service(faction, merchant) else {
        return false;
    };
    if !service.available || !accepted_sale(game, &service, item) {
        return false;
    }
    let Some(total) = count.checked_mul(price) else {
        return false;
    };
    let player = game.player();
    let currency = game.form("Gold001").unwrap_or(GlobalFormId {
        file: 0,
        object: 15,
    });
    let world = game.shared_world();
    let success = world
        .borrow_mut()
        .trade_items_notified(
            player,
            item,
            count,
            service.merchant_container,
            currency,
            total,
            &mut |name, class| game.vm.create_object_of(name, class),
        )
        .is_ok();
    if success {
        game.run_until_idle(64);
    }
    success
}
