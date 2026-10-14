// SPDX-License-Identifier: GPL-3.0-or-later

//! Inventory console operations follow UESP Skyrim Console and shared inventory APIs.

use super::{Args, Console, Game, GlobalFormId, HostRequest, Outcome};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "showinventory"
            | "removeallitems"
            | "duplicateallitems"
            | "drop"
            | "equipitem"
            | "unequipitem"
            | "resetinventory"
            | "openactorcontainer"
    )
}

impl Console {
    pub(super) fn inventory_command(
        &mut self,
        game: &mut Game,
        id: GlobalFormId,
        name: &str,
        args: &Args<'_>,
    ) -> Result<Outcome, String> {
        if game.world_mut().reference(id).is_none() {
            return Err("target is not a reference".into());
        }
        match name {
            "showinventory" => {
                let items: Vec<_> = game
                    .world_mut()
                    .reference(id)
                    .ok_or("missing reference")?
                    .inventory
                    .iter()
                    .map(|(id, count)| (*id, count))
                    .collect();
                let world = game.world();
                Ok(Outcome::List(
                    items
                        .iter()
                        .map(|(item, count)| {
                            format!(
                                "{count} - {} ({:08X})",
                                world.name_of(*item).unwrap_or_default(),
                                world.runtime_id(*item)
                            )
                        })
                        .collect(),
                ))
            }
            "removeallitems" | "duplicateallitems" => {
                transfer(game, id, name == "duplicateallitems", args)
            }
            "drop" => {
                let item = args.form(game, 0)?;
                let count = args.0.get(1).map_or(Ok(1), |_| args.int(1))?;
                let vm = &mut game.vm;
                let dropped = game
                    .world
                    .borrow_mut()
                    .console_drop(id, item, count, &mut |n, c| vm.create_object_of(n, c));
                if dropped.is_empty() {
                    return Err("no item to drop".into());
                }
                Ok(Outcome::List(
                    dropped.iter().map(ToString::to_string).collect(),
                ))
            }
            "resetinventory" => {
                if !game.world_mut().reset_inventory(id) {
                    return Err("target has no authored inventory".into());
                }
                Ok(Outcome::Done)
            }
            "openactorcontainer" => {
                if args.int(0)? != 1 {
                    return Err("openactorcontainer requires 1".into());
                }
                if game.world_mut().actor(id).is_none() {
                    return Err("not an actor".into());
                }
                self.host_requests.push(HostRequest::Container(id));
                Ok(Outcome::Done)
            }
            _ => equip(game, id, name == "equipitem", args),
        }
    }
}

fn transfer(
    game: &mut Game,
    source: GlobalFormId,
    copy: bool,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    let destination = args.0.first().map(|_| args.form(game, 0)).transpose()?;
    if copy && destination.is_none() {
        return Err("duplicateallitems needs a destination".into());
    }
    if destination == Some(source) {
        return Err("source and destination are the same".into());
    }
    let items = game
        .world_mut()
        .reference(source)
        .ok_or("not a reference")?
        .inventory
        .clone();
    if let Some(to) = destination {
        let mut updated = game
            .world_mut()
            .reference(to)
            .ok_or("not a reference")?
            .inventory
            .clone();
        let totals: Vec<_> = items.iter().map(|(item, count)| (*item, count)).collect();
        let mut duplicate = items.clone();
        for (item, count) in totals {
            let mut bundle = duplicate.take(item, count);
            bundle.unequip();
            if copy {
                bundle.clear_reference_identity();
            }
            updated.receive(item, bundle).map_err(|e| e.to_string())?;
        }
        if copy {
            game.world_mut()
                .reference(to)
                .ok_or("missing destination")?
                .inventory = updated;
            return Ok(Outcome::Done);
        }
    }
    let vm = &mut game.vm;
    let mut world = game.world.borrow_mut();
    for (item, count) in items.iter() {
        world.remove_item_notified(source, *item, count, destination, &mut |n, c| {
            vm.create_object_of(n, c)
        });
    }
    Ok(Outcome::Done)
}

fn equip(
    game: &mut Game,
    actor: GlobalFormId,
    on: bool,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    let item = args.form(game, 0)?;
    let prevent = args.flag(1, false)?;
    let hand = match args.0.get(2).copied() {
        None | Some("right") => crate::combat::Hand::Right,
        Some("left") => crate::combat::Hand::Left,
        Some(_) => return Err("hand must be left or right".into()),
    };
    let vm = &mut game.vm;
    let mut world = game.world.borrow_mut();
    if on {
        if world.tag_of(item) == Some(soe_esm::Tag(*b"WEAP")) && args.0.get(2).is_some() {
            world.equip_item(actor, item).map_err(|e| e.to_string())?;
            world
                .equip_hand_weapon(actor, hand, item)
                .map_err(|e| e.to_string())?;
        }
        world
            .equip_item_with_effects(actor, item, &mut |n, c| vm.create_object_of(n, c))
            .map_err(|e| e.to_string())?;
        let state = world.actor(actor).ok_or("not an actor")?;
        if prevent {
            state.prevent_equipment_removal.insert(item);
        } else {
            state.prevent_equipment_removal.remove(&item);
        }
    } else {
        world
            .force_unequip_item(actor, item)
            .map_err(|e| e.to_string())?;
        world
            .unequip_item_with_effects(actor, item, &mut |n, c| vm.create_object_of(n, c))
            .map_err(|e| e.to_string())?;
    }
    Ok(Outcome::Done)
}
