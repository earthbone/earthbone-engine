// SPDX-License-Identifier: GPL-3.0-or-later

//! Console placement and lock operations follow UESP Skyrim Console.

use super::{Args, Game, GlobalFormId, Outcome};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "getpos"
            | "setpos"
            | "getangle"
            | "setangle"
            | "getscale"
            | "setscale"
            | "modscale"
            | "lock"
            | "unlock"
            | "getlocked"
            | "getlocklevel"
            | "setownership"
            | "clearownership"
            | "getdisabled"
            | "getdistance"
            | "getself"
            | "movetomyeditorlocation"
            | "resetreference"
            | "setactoralpha"
            | "markfordelete"
    )
}

pub(super) fn run(
    game: &mut Game,
    id: GlobalFormId,
    name: &str,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    if matches!(name, "getpos" | "setpos" | "getangle" | "setangle") {
        return axis(game, id, name, args);
    }
    let owner = if name == "setownership" {
        Some(if args.0.is_empty() {
            let player = game.player();
            game.world_mut()
                .base_of(player)
                .ok_or("player has no base")?
        } else {
            args.form(game, 0)?
        })
    } else {
        None
    };
    if name == "getdistance" {
        let to = args.form(game, 0)?;
        return Ok(Outcome::Float(f64::from(game.world_mut().distance(id, to))));
    }
    if name == "getself" {
        return Ok(Outcome::Form(id));
    }
    if matches!(name, "lock" | "unlock") {
        let level = if name == "lock" {
            args.0.first().map_or(Ok(100), |_| args.int(0))?
        } else {
            0
        };
        let vm = &mut game.vm;
        let mut world = game.world.borrow_mut();
        if world.reference(id).is_none() {
            return Err("not a reference".into());
        }
        world.set_lock_level(id, level, &mut |n, c| vm.create_object_of(n, c));
        world.set_locked(id, name == "lock", &mut |n, c| vm.create_object_of(n, c));
        return Ok(Outcome::Done);
    }
    let mut world = game.world_mut();
    if name == "movetomyeditorlocation" {
        return Ok(Outcome::Bool(world.move_to_editor_location(id)));
    }
    if name == "resetreference" {
        return Ok(Outcome::Bool(world.reset_reference(id, None)));
    }
    if let Some(owner) = owner {
        if !matches!(
            world.tag_of(owner).map(|t| *t.as_bytes()),
            Some([b'N', b'P', b'C', b'_'] | [b'F', b'A', b'C', b'T'])
        ) {
            return Err("ownership requires NPC_ or FACT".into());
        }
    }
    let reference = world.reference(id).ok_or("not a reference")?;
    match name {
        "markfordelete" => reference.console_delete_pending = true,
        "getscale" => return Ok(Outcome::Float(f64::from(reference.scale))),
        "getlocked" => return Ok(Outcome::Bool(reference.locked)),
        "getlocklevel" => return Ok(Outcome::Int(i64::from(reference.lock_level))),
        "getdisabled" => return Ok(Outcome::Bool(!reference.enabled)),
        "setownership" | "clearownership" => {
            reference.owner = owner;
            reference.owner_global = None;
            reference.owner_rank = None;
        }
        "setactoralpha" => reference.alpha = (args.float(0)? / 100.0).clamp(0.0, 1.0),
        _ => {
            let value = args.float(0)?;
            let scale = if name == "modscale" {
                reference.scale + value
            } else {
                value
            };
            if !(0.1..=10.0).contains(&scale) {
                return Err("scale must be between 0.1 and 10".into());
            }
            reference.scale = scale;
        }
    }
    Ok(Outcome::Done)
}

fn axis(game: &mut Game, id: GlobalFormId, name: &str, args: &Args<'_>) -> Result<Outcome, String> {
    let index = match args.word(0)?.to_ascii_lowercase().as_str() {
        "x" => 0,
        "y" => 1,
        "z" => 2,
        _ => return Err("axis must be x, y or z".into()),
    };
    let mut world = game.world_mut();
    let reference = world.reference(id).ok_or("not a reference")?;
    let angle = name.ends_with("angle");
    let values = if angle {
        &mut reference.rotation
    } else {
        &mut reference.position
    };
    if name.starts_with("set") {
        let value = args.float(1)?;
        values[index] = if angle { value.to_radians() } else { value };
        Ok(Outcome::Done)
    } else {
        Ok(Outcome::Float(f64::from(if angle {
            values[index].to_degrees()
        } else {
            values[index]
        })))
    }
}
