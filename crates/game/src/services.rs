// SPDX-License-Identifier: GPL-3.0-or-later

//! Vendor services from winning FACT, FLST, LVLI and merchant-container records.
//! FACT field identities follow xEdit wbDefinitionsTES5.pas. Eligibility uses
//! the shared CTDA evaluator, as required by the CK ShowBarterMenu contract.

use soe_esm::records::gameplay::Condition;
use soe_esm::{Error as EsmError, GlobalFormId, Record, Tag};
use soe_papyrus::Vm;
use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::conditions::{self, Context};
use crate::inventory::Inventory;
use crate::world::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const VENDOR: u32 = 1 << 14;

/// Service data decoded from a vendor faction and its referenced forms.
#[derive(Debug, Clone, PartialEq)]
pub struct VendorService {
    pub faction: GlobalFormId,
    pub merchant_container: GlobalFormId,
    pub buy_sell_list: GlobalFormId,
    pub start_hour: u16,
    pub end_hour: u16,
    pub radius: u32,
    pub only_buys_stolen: bool,
    pub not_sell_buy: bool,
    pub conditions_met: bool,
    pub within_hours: bool,
    pub available: bool,
    /// Current eligible merchandise, with leveled-list quantities and metadata resolved.
    pub merchandise: Inventory,
    /// Gold in the merchant container after resolving any leveled gold entries.
    pub gold: i32,
}

/// Errors while decoding or evaluating a vendor faction.
#[derive(Debug, thiserror::Error)]
pub enum VendorError {
    #[error("invalid vendor faction {0}")]
    Faction(GlobalFormId),
    #[error("vendor record or merchandise: {0}")]
    Record(#[from] EsmError),
    #[error("vendor condition: {0}")]
    Condition(#[from] conditions::Error),
    #[error("invalid vendor data: {0}")]
    Invalid(&'static str),
}

/// Resolve the service exposed by `faction` to `merchant` at the current time.
/// A non-vendor faction returns `Ok(None)`. Conditions run with the merchant
/// as subject and the player as target, matching the barter invocation.
///
/// # Errors
/// Returns an error for malformed faction data, unsupported conditions or
/// invalid merchandise. Lazy form reads may populate world caches and list
/// selection advances the world's saved random stream when the service is
/// available.
pub fn resolve_vendor_service(
    world: &mut World,
    vm: &Vm,
    faction: GlobalFormId,
    merchant: GlobalFormId,
) -> Result<Option<VendorService>, VendorError> {
    let record = world
        .record(faction)
        .filter(|record| !record.is_deleted() && record.tag == T(b"FACT"))
        .cloned()
        .ok_or(VendorError::Faction(faction))?;
    let data = record
        .find(T(b"DATA"))
        .ok_or(VendorError::Invalid("missing FACT.DATA"))?;
    let flags = (data.len() == 4)
        .then(|| read_u32(data))
        .flatten()
        .ok_or(VendorError::Invalid("invalid FACT.DATA"))?;
    if flags & VENDOR == 0 {
        return Ok(None);
    }

    let vendor = record
        .find(T(b"VENV"))
        .filter(|data| data.len() == 12)
        .ok_or(VendorError::Invalid("missing or invalid FACT.VENV"))?;
    let start_hour = read_u16(&vendor[0..2]).ok_or(VendorError::Invalid("invalid VENV start"))?;
    let end_hour = read_u16(&vendor[2..4]).ok_or(VendorError::Invalid("invalid VENV end"))?;
    let radius = read_u32(&vendor[4..8]).ok_or(VendorError::Invalid("invalid VENV radius"))?;
    if vendor[8] > 1 || vendor[9] > 1 {
        return Err(VendorError::Invalid("invalid VENV trade flags"));
    }
    let only_buys_stolen = vendor[8] != 0;
    let not_sell_buy = vendor[9] != 0;
    if start_hour > 24 || end_hour > 24 {
        return Err(VendorError::Invalid("VENV hours must not exceed 24"));
    }

    let list = resolve_record_link(world, faction, &record, T(b"VEND"), T(b"FLST"))?;
    let container = resolve_record_link(world, faction, &record, T(b"VENC"), T(b"REFR"))?;
    let conditions = faction_conditions(&record)?;
    let player = world.player();
    let merchant_loaded = world.is_reference_loaded(merchant);
    let conditions_met = merchant_loaded
        && vendor_location_met(world, merchant, faction, &record, radius)?
        && conditions::evaluate(
            world,
            vm,
            Context {
                owner: faction,
                subject: merchant,
                target: Some(player),
                quest: None,
                tempering_enchanted: None,
                skill_use: None,
                light_level: None,
            },
            &conditions,
        )?;
    let hour = world.clock.game_hours().rem_euclid(24.0);
    let within_hours = match start_hour.cmp(&end_hour) {
        Ordering::Equal => true,
        Ordering::Less => hour >= f64::from(start_hour) && hour < f64::from(end_hour),
        Ordering::Greater => hour >= f64::from(start_hour) || hour < f64::from(end_hour),
    };
    let available = merchant_loaded && conditions_met && within_hours;

    let mut merchandise = Inventory::default();
    let mut gold = 0;
    if available {
        let items = world
            .form_list(list)
            .cloned()
            .ok_or(VendorError::Invalid("VEND is not a form list"))?;
        let stock = world
            .reference(container)
            .ok_or(VendorError::Invalid(
                "merchant container is not a reference",
            ))?
            .inventory
            .clone();
        let mut resolved_stock = Inventory::default();
        let mut listed_from_leveled = BTreeSet::new();
        for (item, count) in stock.iter() {
            if world.tag_of(*item) == Some(T(b"LVLI")) {
                let rolled =
                    world.roll_leveled_inventory(*item, world.player_level.max(1), count)?;
                if items.contains(item) {
                    listed_from_leveled.extend(rolled.iter().map(|(form, _)| *form));
                }
                merge_inventory(&mut resolved_stock, &rolled)?;
            } else if world.is_inventory_base(*item) {
                copy_item(&mut resolved_stock, &stock, *item)?;
            }
        }
        let currency = world.form("Gold001").unwrap_or(GlobalFormId {
            file: 0,
            object: 0xF,
        });
        gold = resolved_stock.count(currency);
        for (item, _) in resolved_stock.iter() {
            if *item == currency {
                continue;
            }
            let listed = listed_from_leveled.contains(item)
                || items.contains(item)
                || world
                    .keywords(*item)
                    .iter()
                    .any(|keyword| items.contains(keyword));
            if listed != not_sell_buy {
                copy_item(&mut merchandise, &resolved_stock, *item)?;
            }
        }
    }

    Ok(Some(VendorService {
        faction,
        merchant_container: container,
        buy_sell_list: list,
        start_hour,
        end_hour,
        radius,
        only_buys_stolen,
        not_sell_buy,
        conditions_met,
        within_hours,
        available,
        merchandise,
        gold,
    }))
}

fn vendor_location_met(
    world: &mut World,
    merchant: GlobalFormId,
    faction: GlobalFormId,
    record: &Record,
    radius: u32,
) -> Result<bool, VendorError> {
    let Some(data) = record.find(T(b"PLVD")) else {
        return Ok(true);
    };
    if data.len() != 12 {
        return Err(VendorError::Invalid("invalid FACT.PLVD"));
    }
    let mode = read_u32(&data[0..4]).ok_or(VendorError::Invalid("invalid PLVD mode"))?;
    let raw = read_u32(&data[4..8]).ok_or(VendorError::Invalid("invalid PLVD target"))?;
    let merchant_ref = world
        .reference(merchant)
        .ok_or(VendorError::Invalid("merchant is not a reference"))?;
    let position = merchant_ref.position;
    let cell = merchant_ref.cell;
    match mode {
        0 => {
            let target = world
                .resolve_from(faction, raw)
                .ok_or(VendorError::Invalid("missing PLVD reference"))?;
            let target_ref = world
                .reference(target)
                .ok_or(VendorError::Invalid("invalid PLVD reference"))?;
            let target_position = target_ref.position;
            let target_cell = target_ref.cell;
            Ok(same_space(world, cell, target_cell)
                && within_radius(position, target_position, radius))
        }
        1 => {
            let target = world
                .resolve_from(faction, raw)
                .filter(|id| world.tag_of(*id) == Some(T(b"CELL")))
                .ok_or(VendorError::Invalid("invalid PLVD cell"))?;
            Ok(cell == Some(target))
        }
        3 => Ok(world
            .editor_position(merchant)
            .is_some_and(|editor| within_radius(position, editor, radius))),
        12 => Ok(true),
        _ => Err(VendorError::Invalid("unsupported PLVD location type")),
    }
}

fn same_space(world: &World, a: Option<GlobalFormId>, b: Option<GlobalFormId>) -> bool {
    if a == b {
        return true;
    }
    let worldspace = |cell: Option<GlobalFormId>| {
        let cell = cell?;
        if world.cell_is_interior(cell) {
            return None;
        }
        let raw = world.record(cell)?.placement.world?;
        world.resolve_from(cell, raw)
    };
    worldspace(a).is_some() && worldspace(a) == worldspace(b)
}

fn within_radius(a: [f32; 3], b: [f32; 3], radius: u32) -> bool {
    if a.iter().chain(&b).any(|component| !component.is_finite()) {
        return false;
    }
    a.iter()
        .zip(b)
        .map(|(x, y)| (f64::from(*x) - f64::from(y)).powi(2))
        .sum::<f64>()
        <= f64::from(radius).powi(2)
}

fn copy_item(
    target: &mut Inventory,
    source: &Inventory,
    item: GlobalFormId,
) -> Result<(), VendorError> {
    for stack in source.stacks(item) {
        target
            .add_selected_stack(item, stack.clone())
            .map_err(|_| VendorError::Invalid("invalid vendor merchandise"))?;
    }
    let described: i32 = source.stacks(item).iter().map(|stack| stack.count()).sum();
    target
        .add(item, source.count(item) - described)
        .map_err(|_| VendorError::Invalid("invalid vendor merchandise"))?;
    Ok(())
}

fn merge_inventory(target: &mut Inventory, source: &Inventory) -> Result<(), VendorError> {
    for (item, total) in source.iter() {
        for stack in source.stacks(*item) {
            target
                .add_selected_stack(*item, stack.clone())
                .map_err(|_| VendorError::Invalid("invalid leveled merchandise"))?;
        }
        let described: i32 = source.stacks(*item).iter().map(|stack| stack.count()).sum();
        target
            .add(*item, total - described)
            .map_err(|_| VendorError::Invalid("invalid leveled merchandise"))?;
    }
    Ok(())
}

fn faction_conditions(record: &Record) -> Result<Vec<Condition>, VendorError> {
    let mut conditions = Vec::new();
    for subrecord in &record.subrecords {
        Condition::push(record, subrecord, &mut conditions)?;
    }
    Ok(conditions)
}

fn resolve_record_link(
    world: &World,
    faction: GlobalFormId,
    record: &Record,
    field: Tag,
    expected: Tag,
) -> Result<GlobalFormId, VendorError> {
    let raw = record
        .find(field)
        .filter(|bytes| bytes.len() == 4)
        .and_then(read_u32)
        .filter(|raw| *raw != 0)
        .ok_or(VendorError::Invalid("missing or invalid vendor link"))?;
    let form = world
        .resolve_from(faction, raw)
        .filter(|form| world.tag_of(*form) == Some(expected))
        .ok_or(VendorError::Invalid("vendor link has the wrong form type"))?;
    Ok(form)
}

fn read_u16(data: &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(..2)?.try_into().ok()?))
}

fn read_u32(data: &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(..4)?.try_into().ok()?))
}
