// SPDX-License-Identifier: GPL-3.0-or-later

//! Actor console mutations follow UESP Skyrim Console over shared runtime state.

use super::{Args, Console, Game, GlobalFormId, Outcome};
mod references;
mod values;

pub(super) fn handles(name: &str) -> bool {
    values::handles(name)
        || references::handles(name)
        || matches!(
            name,
            "cast"
                | "pushactoraway"
                | "dispelallspells"
                | "resetai"
                | "stopcombat"
                | "stopcombatalarmonactor"
                | "setghost"
                | "setunconscious"
                | "setrestrained"
                | "setrelationshiprank"
                | "getrelationshiprank"
                | "removefromfaction"
                | "removefromallfactions"
                | "addtofaction"
                | "getfactionrank"
                | "getlevel"
                | "setlevel"
                | "advlevel"
                | "sexchange"
                | "setessential"
                | "setprotected"
                | "setactorsai"
                | "playidle"
                | "sendanimevent"
                | "equipshout"
                | "equipspell"
                | "removeshout"
                | "hasperk"
                | "hasspell"
                | "hasshout"
                | "getdead"
                | "isessential"
                | "isprotected"
                | "isincombat"
                | "getisghost"
                | "setplayerteammate"
                | "setalert"
                | "setignorefriendlyhits"
        )
}

impl Console {
    pub(super) fn actor_command(
        &mut self,
        game: &mut Game,
        target: Option<&str>,
        name: &str,
        args: &Args<'_>,
    ) -> Result<Outcome, String> {
        if name == "setessential" || name == "setprotected" {
            let base = args.form(game, 0)?;
            let on = args.flag(1, true)?;
            if game.world_mut().actor_base(base).is_none() {
                return Err("requires an NPC base ID".into());
            }
            if name == "setessential" {
                game.world_mut().set_actor_base_essential(base, on);
            } else {
                game.world_mut().set_actor_base_protected(base, on);
            }
            return Ok(Outcome::Done);
        }
        let id = if name == "advlevel"
            || name == "sexchange" && target.is_none() && self.selected.is_none()
        {
            game.player()
        } else {
            self.target_of(game, target)?
        };
        if references::handles(name) {
            return references::run(game, id, name, args);
        }
        if game.world_mut().actor(id).is_none() {
            return Err("target is not an actor".into());
        }
        if values::handles(name) {
            return values::run(game, id, name, args);
        }
        if matches!(
            name,
            "setrelationshiprank"
                | "getrelationshiprank"
                | "removefromfaction"
                | "removefromallfactions"
                | "addtofaction"
                | "getfactionrank"
        ) {
            return relations(game, id, name, args);
        }
        if matches!(
            name,
            "cast"
                | "pushactoraway"
                | "playidle"
                | "sendanimevent"
                | "equipspell"
                | "equipshout"
                | "removeshout"
                | "hasperk"
                | "hasspell"
                | "hasshout"
        ) {
            return magic(game, id, name, args);
        }
        if matches!(name, "advlevel" | "setlevel" | "getlevel" | "sexchange") {
            return level(game, id, name, args);
        }
        if name == "stopcombatalarmonactor" {
            return Ok(Outcome::Bool(game.world_mut().stop_combat_alarm(id)));
        }
        if name == "dispelallspells" {
            let vm = &mut game.vm;
            game.world
                .borrow_mut()
                .dispel_all_spells(id, &mut |n, c| vm.create_object_of(n, c));
            return Ok(Outcome::Done);
        }
        actor_flags(game, id, name, args)
    }
}

fn actor_flags(
    game: &mut Game,
    id: GlobalFormId,
    name: &str,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    let on = args.flag(0, true)?;
    let mut world = game.world_mut();
    let state = world.actor(id).ok_or("not an actor")?;
    match name {
        "stopcombat" | "resetai" => {
            state.in_combat = false;
            state.combat_target = None;
            state.weapon_drawn = false;
            state.ai.reevaluate = true;
        }
        "setghost" => state.ghost = on,
        "setunconscious" => {
            state.unconscious = on;
            if on {
                state.restrained = false;
            }
        }
        "setrestrained" => {
            state.restrained = on;
            if on {
                state.unconscious = false;
            }
        }
        "setactorsai" => state.ai_disabled = !on,
        "setplayerteammate" => state.teammate = on,
        "setalert" => state.alerted = on,
        "getdead" => return Ok(Outcome::Bool(state.dead)),
        "isessential" => return Ok(Outcome::Bool(state.essential)),
        "isprotected" => return Ok(Outcome::Bool(state.protected)),
        "isincombat" => return Ok(Outcome::Bool(state.in_combat)),
        "getisghost" => return Ok(Outcome::Bool(state.ghost)),
        "setignorefriendlyhits" => {
            world
                .reference(id)
                .ok_or("not a reference")?
                .ignore_friendly_hits = on;
        }
        _ => return Err(format!("unhandled actor command {name}")),
    }
    Ok(Outcome::Done)
}

fn relations(
    game: &mut Game,
    id: GlobalFormId,
    name: &str,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    if name == "removefromallfactions" {
        game.world_mut()
            .actor(id)
            .ok_or("not an actor")?
            .factions
            .clear();
        return Ok(Outcome::Done);
    }
    let other = args.form(game, 0)?;
    let rank = if matches!(name, "setrelationshiprank" | "addtofaction") {
        Some(args.int(1)?)
    } else {
        None
    };
    let mut world = game.world_mut();
    if name.contains("faction") && world.faction(other).is_none() {
        return Err("requires a faction".into());
    }
    if name.contains("relationship") && world.actor(other).is_none() {
        return Err("requires another actor".into());
    }
    let state = world.actor(id).ok_or("not an actor")?;
    match name {
        "setrelationshiprank" => {
            state.relationships.insert(other, rank.unwrap_or(0));
        }
        "getrelationshiprank" => {
            return Ok(Outcome::Int(i64::from(
                state.relationships.get(&other).copied().unwrap_or(0),
            )))
        }
        "addtofaction" if rank.unwrap_or(0) >= 0 => {
            state.factions.insert(other, rank.unwrap_or(0));
        }
        "removefromfaction" | "addtofaction" => {
            state.factions.remove(&other);
        }
        _ => {
            return Ok(Outcome::Int(i64::from(
                state.factions.get(&other).copied().unwrap_or(-1),
            )))
        }
    }
    Ok(Outcome::Done)
}

fn level(
    game: &mut Game,
    id: GlobalFormId,
    name: &str,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    if name == "sexchange" {
        let mut world = game.world_mut();
        let base = world.base_of(id).ok_or("actor has no base")?;
        let state = world.actor_base(base).ok_or("not an NPC base")?;
        state.female = !state.female;
        return Ok(Outcome::Bool(state.female));
    }
    let mut world = game.world_mut();
    let current = world.actor(id).ok_or("not an actor")?.level;
    if name == "getlevel" {
        return Ok(Outcome::Int(i64::from(current)));
    }
    let level = if name == "advlevel" {
        current.saturating_add(1)
    } else if args.0.len() == 1 {
        u32::try_from(args.int(0)?)
            .map_err(|_| "level must be positive")?
            .max(1)
    } else {
        let min = args.int(2)?.max(1);
        let max = args.int(3)?;
        if min > max {
            return Err("minimum exceeds maximum".into());
        }
        (world.player_level as f32 * args.float(0)? / 1000.0 + args.float(1)?)
            .clamp(min as f32, max as f32) as u32
    };
    world.actor(id).ok_or("not an actor")?.level = level;
    if world.is_player(id) {
        world.player_level = level;
    }
    Ok(Outcome::Done)
}

fn magic(
    game: &mut Game,
    id: GlobalFormId,
    name: &str,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    if name == "pushactoraway" {
        let target = args.form(game, 0)?;
        game.world_mut()
            .push_actor_away(id, target, args.float(1)?)
            .map_err(str::to_owned)?;
        return Ok(Outcome::Done);
    }
    if name == "cast" {
        return cast(game, id, args);
    }
    if name == "sendanimevent" {
        let event = args.word(0)?;
        game.world_mut().request_animation(id, event);
        return Ok(Outcome::Done);
    }
    let form = args.form(game, 0)?;
    if name == "playidle" {
        if game.world().tag_of(form) != Some(soe_esm::Tag(*b"IDLE")) {
            return Err("requires an IDLE".into());
        }
        let event = game
            .world()
            .editor_id(form)
            .ok_or("idle has no editor ID")?;
        game.world_mut().request_animation(id, &event);
        return Ok(Outcome::Done);
    }
    if name == "equipspell" {
        let hand = match args.word(1)?.to_ascii_lowercase().as_str() {
            "left" => crate::combat::Hand::Left,
            "right" => crate::combat::Hand::Right,
            _ => return Err("voice and instant equipspell are not bridged".into()),
        };
        game.world_mut()
            .equip_hand_spell(id, hand, form)
            .map_err(|e| e.to_string())?;
        return Ok(Outcome::Done);
    }
    let mut world = game.world_mut();
    let state = world.actor(id).ok_or("not an actor")?;
    match name {
        "equipshout" => {
            state.equipped_shout = Some(form);
        }
        "removeshout" => {
            state.shouts.remove(&form);
        }
        "hasperk" => return Ok(Outcome::Bool(state.perks.contains(&form))),
        "hasspell" => return Ok(Outcome::Bool(state.spells.contains(&form))),
        _ => return Ok(Outcome::Bool(state.shouts.contains(&form))),
    }
    Ok(Outcome::Done)
}

fn cast(game: &mut Game, actor: GlobalFormId, args: &Args<'_>) -> Result<Outcome, String> {
    let spell = args.form(game, 0)?;
    let target = args.form(game, 1)?;
    let source = args.word(2)?.to_ascii_lowercase();
    if !matches!(
        source.as_str(),
        "left" | "right" | "voice" | "power" | "instant"
    ) {
        return Err("cast source must be left, right, voice or power".into());
    }
    let vm = &mut game.vm;
    let mut world = game.world.borrow_mut();
    world
        .cast_spell(spell, actor, Some(actor), Some(target), &mut |n, c| {
            vm.create_object_of(n, c)
        })
        .map_err(|e| e.to_string())?;
    let state = world.actor(actor).ok_or("not an actor")?;
    match source.as_str() {
        "left" => state.left_spell = Some(spell),
        "right" => state.right_spell = Some(spell),
        "instant" => state.instant_spell = Some(spell),
        _ => state.other_spell = Some(spell),
    }
    state.equipped.insert(spell);
    drop(world);
    game.run_until_idle(64);
    Ok(Outcome::Done)
}
