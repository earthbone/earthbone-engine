// SPDX-License-Identifier: GPL-3.0-or-later

//! Vendor and follower services. Vendor FACT fields follow xEdit
//! wbDefinitionsTES5.pas. Follower recruitment retains its prior service
//! values and places the follow package above the actor's ordinary AI stack.

use soe_esm::records::gameplay::Condition;
use soe_esm::{Error as EsmError, GlobalFormId, Record, Tag};
use soe_papyrus::{ObjectId, Value, Vm};
use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::conditions::{self, Context};
use crate::inventory::Inventory;
use crate::world::World;

const T: fn(&[u8; 4]) -> Tag = Tag::new;
const VENDOR: u32 = 1 << 14;

/// Saved follower service state for one recruited actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FollowerServiceState {
    pub package: GlobalFormId,
    pub faction: GlobalFormId,
    pub previous_faction_rank: Option<i32>,
    pub previous_teammate: bool,
    pub previous_can_do_favors: bool,
}

/// Invalid follower service requests.
#[derive(Debug, thiserror::Error)]
pub enum FollowerError {
    #[error("follower actor is not an actor reference: {0}")]
    Actor(GlobalFormId),
    #[error("follower service faction is not a faction: {0}")]
    Faction(GlobalFormId),
    #[error("follower package is not a package: {0}")]
    Package(GlobalFormId),
    #[error("actor is already owned by a follower service: {0}")]
    AlreadyRecruited(GlobalFormId),
    #[error("actor has no follower service: {0}")]
    NotRecruited(GlobalFormId),
}

impl World {
    /// Recruit an actor into a follower service while retaining its ordinary
    /// package stack underneath the service follow package.
    ///
    /// # Errors
    /// Returns an error if the actor, faction or package is invalid, or if
    /// the actor already belongs to a follower service.
    pub fn recruit_follower(
        &mut self,
        actor: GlobalFormId,
        faction: GlobalFormId,
        package: GlobalFormId,
        can_do_favors: bool,
    ) -> Result<(), FollowerError> {
        if !self
            .reference(actor)
            .is_some_and(|reference| reference.is_actor)
        {
            return Err(FollowerError::Actor(actor));
        }
        if !self
            .record(faction)
            .is_some_and(|record| record.tag == T(b"FACT") && !record.is_deleted())
        {
            return Err(FollowerError::Faction(faction));
        }
        if !self
            .record(package)
            .is_some_and(|record| record.tag == T(b"PACK") && !record.is_deleted())
        {
            return Err(FollowerError::Package(package));
        }
        if self.follower_services.contains_key(&actor) {
            return Err(FollowerError::AlreadyRecruited(actor));
        }
        let actor_state = self.actor(actor).ok_or(FollowerError::Actor(actor))?;
        let state = FollowerServiceState {
            package,
            faction,
            previous_faction_rank: actor_state.factions.get(&faction).copied(),
            previous_teammate: actor_state.teammate,
            previous_can_do_favors: actor_state.can_do_favors,
        };
        actor_state.teammate = true;
        actor_state.can_do_favors = can_do_favors;
        actor_state.factions.insert(faction, 0);
        actor_state.ai.reevaluate = true;
        self.follower_services.insert(actor, state);
        Ok(())
    }

    /// Dismiss an actor and restore the teammate, favor and service faction
    /// values that were present before recruitment.
    ///
    /// # Errors
    /// Returns an error if the actor has no active follower service.
    pub fn dismiss_follower(&mut self, actor: GlobalFormId) -> Result<(), FollowerError> {
        let state = self
            .follower_services
            .remove(&actor)
            .ok_or(FollowerError::NotRecruited(actor))?;
        if let Some(actor_state) = self.actor(actor) {
            actor_state.teammate = state.previous_teammate;
            actor_state.can_do_favors = state.previous_can_do_favors;
            match state.previous_faction_rank {
                Some(rank) => {
                    actor_state.factions.insert(state.faction, rank);
                }
                None => {
                    actor_state.factions.remove(&state.faction);
                }
            }
            actor_state.ai.reevaluate = true;
        }
        Ok(())
    }

    /// Return the package temporarily owned by an active follower service.
    pub fn follower_package(&self, actor: GlobalFormId) -> Option<GlobalFormId> {
        self.follower_services
            .get(&actor)
            .map(|state| state.package)
    }

    /// Return actors currently registered with follower services in stable order.
    pub fn followers(&self) -> Vec<GlobalFormId> {
        self.follower_services.keys().copied().collect()
    }
}

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

/// Errors while committing a vendor purchase.
#[derive(Debug, thiserror::Error)]
pub enum TradeError {
    #[error("vendor service is unavailable")]
    Unavailable,
    #[error("vendor does not have enough of this item")]
    Stock,
    #[error("invalid trade quantity or price")]
    Price,
    #[error("vendor service: {0}")]
    Vendor(#[from] VendorError),
    #[error("inventory trade: {0}")]
    Inventory(#[from] crate::inventory::Error),
    #[error("barter quote: {0}")]
    Quote(#[from] crate::barter::Error),
}

impl crate::Game {
    /// Select an available vendor faction from the actor's memberships.
    /// If all services are unavailable, retain the first for an empty menu.
    ///
    /// # Errors
    /// Reports invalid vendor data rather than silently discarding a service.
    pub fn merchant_vendor_service(
        &mut self,
        merchant: GlobalFormId,
    ) -> Result<Option<VendorService>, VendorError> {
        let mut factions: Vec<_> = self
            .world_mut()
            .actor(merchant)
            .map(|actor| actor.factions.keys().copied().collect())
            .unwrap_or_default();
        factions.sort();
        let mut unavailable = None;
        for faction in factions {
            if let Some(service) = self.resolve_vendor_service(faction, merchant)? {
                if service.available {
                    return Ok(Some(service));
                }
                if unavailable.is_none() {
                    unavailable = Some(service);
                }
            }
        }
        Ok(unavailable)
    }

    /// Check the vendor list, keyword policy, value and stolen-item setting.
    pub fn vendor_accepts_sale(&self, service: &VendorService, item: GlobalFormId) -> bool {
        let mut world = self.world_mut();
        let player = world.player();
        let currency = world.form("Gold001").unwrap_or(GlobalFormId {
            file: 0,
            object: 15,
        });
        if item == currency || world.item_value(item) <= 0 {
            return false;
        }
        let Some(list) = world.form_list(service.buy_sell_list).cloned() else {
            return false;
        };
        let listed = list.contains(&item)
            || world
                .keywords(item)
                .iter()
                .any(|keyword| list.contains(keyword));
        let stolen = world.reference(player).is_some_and(|reference| reference.inventory.stacks(item).iter().any(|stack| {
            matches!(stack.extra(33).map(|extra| extra.fields.as_slice()), Some([crate::inventory::Field::Form(Some(owner))]) if *owner != player)
        }));
        listed != service.not_sell_buy && (!stolen || service.only_buys_stolen)
    }

    /// Exchange a selected quantity at the current vanilla barter quote.
    ///
    /// # Errors
    /// Rejects unavailable service, disallowed items, invalid quantities,
    /// malformed quotes, insufficient stock or gold and inventory overflow.
    pub fn trade_vendor_item(
        &mut self,
        faction: GlobalFormId,
        merchant: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        side: crate::barter::Side,
    ) -> Result<(), TradeError> {
        if count <= 0 {
            return Err(TradeError::Price);
        }
        if self
            .world_mut()
            .actor(merchant)
            .is_none_or(|actor| !actor.factions.contains_key(&faction))
        {
            return Err(TradeError::Unavailable);
        }
        let price = self
            .barter_prices(merchant)?
            .unit_price(self.world().item_value(item), side)?;
        if side == crate::barter::Side::Buy {
            return self.buy_vendor_item(faction, merchant, item, count, price);
        }
        let service = self
            .resolve_vendor_service(faction, merchant)?
            .filter(|service| service.available)
            .ok_or(TradeError::Unavailable)?;
        let player = self.player();
        if !self.vendor_accepts_sale(&service, item)
            || self.world_mut().item_count(player, item) < count
        {
            return Err(TradeError::Stock);
        }
        let total = count.checked_mul(price).ok_or(TradeError::Price)?;
        let currency = self.form("Gold001").unwrap_or(GlobalFormId {
            file: 0,
            object: 15,
        });
        self.world.borrow_mut().trade_vendor_items_notified(
            player,
            item,
            count,
            service.merchant_container,
            currency,
            total,
            &mut |name, class| self.vm.create_object_of(name, class),
        )?;
        self.run_until_idle(64);
        Ok(())
    }
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
    let lookup = |object, name: String| vm.conditional_vars(object, &name);
    vendor_service_view(world, Some(&lookup), faction, merchant, VendorView::Stock)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VendorView {
    Eligibility,
    Stock,
}

pub(crate) fn offers_services_now(
    world: &mut World,
    lookup: Option<&dyn Fn(ObjectId, String) -> Vec<Value>>,
    merchant: GlobalFormId,
) -> Result<bool, VendorError> {
    let mut factions: Vec<_> = world
        .actor(merchant)
        .ok_or(VendorError::Invalid("service subject is not an actor"))?
        .factions
        .keys()
        .copied()
        .collect();
    factions.sort();
    for faction in factions {
        if vendor_service_view(world, lookup, faction, merchant, VendorView::Eligibility)?
            .is_some_and(|service| service.available)
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn vendor_service_view(
    world: &mut World,
    lookup: Option<&dyn Fn(ObjectId, String) -> Vec<Value>>,
    faction: GlobalFormId,
    merchant: GlobalFormId,
    view: VendorView,
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
    let radius =
        u32::from(read_u16(&vendor[4..6]).ok_or(VendorError::Invalid("invalid VENV radius"))?);
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
    if conditions.iter().any(|condition| condition.function == 255) {
        return Err(VendorError::Invalid("recursive vendor service condition"));
    }
    let player = world.player();
    let merchant_loaded = world.is_reference_loaded(merchant);
    let context = Context {
        owner: faction,
        subject: merchant,
        target: Some(player),
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
    let conditions_met = merchant_loaded
        && vendor_location_met(world, merchant, faction, &record, radius)?
        && match lookup {
            Some(lookup) => conditions::evaluate_with_lookup(world, context, &conditions, lookup)?,
            None => conditions::evaluate_without_vm(world, context, &conditions)?,
        };
    let hour = world.clock.game_hours().rem_euclid(24.0);
    let within_hours = match start_hour.cmp(&end_hour) {
        Ordering::Equal => true,
        Ordering::Less => hour >= f64::from(start_hour) && hour < f64::from(end_hour),
        Ordering::Greater => hour >= f64::from(start_hour) || hour < f64::from(end_hour),
    };
    let available = merchant_loaded && conditions_met && within_hours;

    let mut merchandise = Inventory::default();
    let mut gold = 0;
    if available && view == VendorView::Stock {
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
        let listed_from_leveled = listed_leveled_items(world, &items)?;
        for (item, count) in stock.iter() {
            if world.tag_of(*item) == Some(T(b"LVLI")) {
                let rolled =
                    world.roll_leveled_inventory(*item, world.player_level.max(1), count)?;
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

fn listed_leveled_items(
    world: &mut World,
    forms: &[GlobalFormId],
) -> Result<BTreeSet<GlobalFormId>, VendorError> {
    let mut pending: Vec<_> = forms
        .iter()
        .copied()
        .filter(|form| world.tag_of(*form) == Some(T(b"LVLI")))
        .collect();
    let mut visited = BTreeSet::new();
    let mut leaves = BTreeSet::new();
    while let Some(form) = pending.pop() {
        if !visited.insert(form) {
            continue;
        }
        if visited.len() > 4096 {
            return Err(VendorError::Invalid("excessive vendor list graph"));
        }
        if world.tag_of(form) == Some(T(b"LVLI")) {
            let list = world
                .leveled_list(form)
                .ok_or(VendorError::Invalid("missing vendor leveled list"))?;
            pending.extend(list.entries.iter().map(|entry| entry.form));
        } else {
            leaves.insert(form);
        }
    }
    Ok(leaves)
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
