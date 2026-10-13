// SPDX-License-Identifier: GPL-3.0-or-later

//! Untargeted console actions from UESP Skyrim Console.

use super::{Args, Console, Game, HostRequest, Outcome};
use soe_esm::{CellIndex, Tag};
use soe_papyrus::Value;
mod crime;
mod settings;

pub(super) fn handles(name: &str) -> bool {
    matches!(name, "tfc" | "sucsm" | "tm" | "tll" | "csb" | "pcb")
        || crime::handles(name)
        || settings::handles(name)
        || matches!(
            name,
            "tgm"
                | "tim"
                | "tai"
                | "tcai"
                | "tcl"
                | "tdetect"
                | "tscr"
                | "tmove"
                | "coc"
                | "cow"
                | "coe"
                | "killallactors"
                | "kah"
                | "psb"
                | "bat"
                | "quitgame"
                | "showracemenu"
                | "setplayerrace"
                | "save"
                | "load"
                | "fov"
                | "sgtm"
                | "showglobalvars"
                | "setlocationcleared"
                | "getlocationcleared"
                | "enableplayercontrols"
                | "disableplayercontrols"
                | "getpcmiscstat"
                | "modpcmiscstat"
                | "advskill"
                | "incpcs"
                | "teachword"
                | "unlockword"
                | "resetinterior"
                | "tm"
                | "tll"
                | "csb"
                | "pcb"
        )
}

impl Console {
    pub(super) fn environment_command(
        &mut self,
        game: &mut Game,
        target: Option<&str>,
        name: &str,
        args: &Args<'_>,
    ) -> Result<Outcome, String> {
        if crime::handles(name) {
            return self.crime_command(game, target, name, args);
        }
        if settings::handles(name) {
            return settings::run(game, name, args);
        }
        if matches!(
            name,
            "tgm" | "tim" | "tai" | "tcai" | "tcl" | "tdetect" | "tscr" | "tmove"
        ) {
            return self.toggle(game, target, name);
        }
        if matches!(name, "coc" | "cow" | "coe") {
            return teleport(game, name, args);
        }
        if name == "bat" {
            return self.batch(game, args.word(0)?);
        }
        if matches!(name, "killallactors" | "kah" | "psb") {
            return mass(game, name);
        }
        if matches!(name, "enableplayercontrols" | "disableplayercontrols") {
            return controls(game, name == "enableplayercontrols", args);
        }
        match name {
            "tfc" => self
                .host_requests
                .push(HostRequest::FreeCamera(args.flag(0, false)?)),
            "sucsm" => {
                let speed = args.float(0)?;
                if speed < 0.0 {
                    return Err("camera speed must be nonnegative".into());
                }
                self.host_requests.push(HostRequest::FreeCameraSpeed(speed));
            }
            "tm" | "tll" | "csb" => self
                .host_requests
                .push(HostRequest::Toggle(name.to_owned())),
            "pcb" => self.host_requests.push(HostRequest::Purge),
            "quitgame" => game
                .world_mut()
                .debug_host_requests
                .push(crate::world::DebugHostRequest::Quit),
            "fov" => {
                let value = args.0.first().map_or(Ok(75.0), |_| args.float(0))?;
                if value <= 0.0 {
                    return Err("fov must be positive".into());
                }
                self.host_requests.push(HostRequest::Fov(value.min(179.0)));
            }
            "sgtm" => {
                let value = args.float(0)?;
                if value < 0.0 {
                    return Err("time multiplier must be nonnegative".into());
                }
                self.host_requests
                    .push(HostRequest::TimeMultiplier(f64::from(value)));
            }
            "showracemenu" | "setplayerrace" => {
                if let Some(race) = args.0.first() {
                    let race = super::super::resolve(game, race)?;
                    let player = game.player();
                    game.world_mut().actor(player).ok_or("missing player")?.race = Some(race);
                } else {
                    self.host_requests.push(HostRequest::RaceMenu);
                }
            }
            "save" | "load" => {
                if args.0.len() > 1 {
                    return Err("save text dumps are Creation Engine diagnostics and have no engine equivalent".into());
                }
                let name_arg = args.word(0)?;
                if name_arg.is_empty()
                    || name_arg.contains(['/', '\\'])
                    || matches!(name_arg, "." | "..")
                {
                    return Err("save name must be a filename".into());
                }
                self.host_requests.push(HostRequest::Save {
                    name: name_arg.to_owned(),
                    load: name == "load",
                });
            }
            "showglobalvars" => return super::help(game, &["", "2"]),
            "resetinterior" => {
                let cell = args.form(game, 0)?;
                game.world_mut()
                    .reset_cell(cell)
                    .map_err(|e| e.to_string())?;
            }
            _ => return misc(game, name, args),
        }
        Ok(Outcome::Done)
    }

    fn toggle(
        &mut self,
        game: &mut Game,
        target: Option<&str>,
        name: &str,
    ) -> Result<Outcome, String> {
        if name == "tai" && (target.is_some() || self.selected.is_some()) {
            let id = self.target_of(game, target)?;
            let mut world = game.world_mut();
            if let Some(actor) = world.actor(id) {
                actor.ai_disabled = !actor.ai_disabled;
                return Ok(Outcome::Bool(!actor.ai_disabled));
            }
        }
        if name == "tcl" && (target.is_some() || self.selected.is_some()) {
            let id = self.target_of(game, target)?;
            if id != game.player() {
                return Err("targeted tcl is not bridged to reference collision filters".into());
            }
        }
        let mut world = game.world_mut();
        let value = match name {
            "tgm" => &mut world.debug_god_mode,
            "tim" => &mut world.debug_immortal_mode,
            "tai" => &mut world.debug_ai_enabled,
            "tcai" => &mut world.debug_combat_ai_enabled,
            "tcl" => &mut world.debug_collisions_enabled,
            "tdetect" => &mut world.debug_detection_enabled,
            "tscr" => &mut world.debug_scripts_enabled,
            _ => &mut world.player_control_flags[0],
        };
        *value = !*value;
        let on = *value;
        world.player_controls = world.player_control_flags.iter().all(|v| *v);
        Ok(Outcome::Bool(on))
    }

    fn batch(&mut self, game: &mut Game, name: &str) -> Result<Outcome, String> {
        if self.batch_depth >= 16 {
            return Err("batch recursion exceeds 16".into());
        }
        let root = self
            .batch_directory
            .clone()
            .or_else(|| std::env::var_os("SKYRIM_DATA").map(std::path::PathBuf::from))
            .ok_or("SKYRIM_DATA is required for bat")?;
        let path = [
            root.parent().unwrap_or(&root).join(name),
            root.join(format!("{name}.txt")),
            root.join(name),
        ]
        .into_iter()
        .find(|p| p.is_file())
        .ok_or("batch file not found")?;
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.batch_depth += 1;
        let lines = text
            .lines()
            .enumerate()
            .map(|(index, line)| match self.run(game, line) {
                Ok(value) => format!("{}: {value}", index + 1),
                Err(error) => format!("{}: error: {error}", index + 1),
            })
            .collect();
        self.batch_depth -= 1;
        Ok(Outcome::List(lines))
    }
}

fn teleport(game: &mut Game, name: &str, args: &Args<'_>) -> Result<Outcome, String> {
    let index = CellIndex::build(game.world().order());
    let (cell, position) = if name == "coc" {
        let cell = args.form(game, 0)?;
        if game.world().tag_of(cell) != Some(Tag(*b"CELL")) {
            return Err("coc requires a cell".into());
        }
        (cell, [0.0; 3])
    } else {
        let world = if name == "cow" {
            args.form(game, 0)?
        } else {
            super::super::resolve(game, "Tamriel")?
        };
        let start = usize::from(name == "cow");
        let x = args.int(start)?;
        let y = args.int(start + 1)?;
        let cell = index
            .exterior_cell(world, x, y)
            .ok_or("exterior cell not found")?;
        (
            cell,
            [(x as f32 + 0.5) * 4096.0, (y as f32 + 0.5) * 4096.0, 0.0],
        )
    };
    let player = game.player();
    {
        let mut world = game.world_mut();
        let reference = world.reference(player).ok_or("missing player")?;
        reference.position = position;
        reference.cell = Some(cell);
    }
    game.change_cell(Some(cell));
    game.run_until_idle(64);
    Ok(Outcome::Form(cell))
}

fn controls(game: &mut Game, enable: bool, args: &Args<'_>) -> Result<Outcome, String> {
    if args.0.len() > 9 {
        return Err("controls take at most nine arguments".into());
    }
    let defaults = if enable {
        [true; 8]
    } else {
        [true, true, false, false, false, true, true, false]
    };
    let mut requested = [false; 8];
    for (index, value) in requested.iter_mut().enumerate() {
        *value = args.flag(index, defaults[index])?;
    }
    let pov = args.0.get(8).map_or(Ok(0), |_| args.int(8))?;
    let mut world = game.world_mut();
    for (flag, requested) in world.player_control_flags.iter_mut().zip(requested) {
        if requested {
            *flag = enable;
        }
    }
    world.player_control_pov_type = pov;
    world.player_controls = world.player_control_flags.iter().all(|flag| *flag);
    Ok(Outcome::Done)
}

fn mass(game: &mut Game, name: &str) -> Result<Outcome, String> {
    let player = game.player();
    if name == "psb" {
        let spells: Vec<_> = game
            .world()
            .order()
            .winners_of(Tag(*b"SPEL"))
            .map(|(id, _)| id)
            .collect();
        let shouts: Vec<_> = game
            .world()
            .order()
            .winners_of(Tag(*b"SHOU"))
            .map(|(id, _)| id)
            .collect();
        let words: Vec<_> = game
            .world()
            .order()
            .winners_of(Tag(*b"WOOP"))
            .map(|(id, _)| id)
            .collect();
        let mut world = game.world_mut();
        world.words_taught.extend(words);
        let actor = world.actor(player).ok_or("missing player")?;
        actor.spells.extend(spells);
        actor.shouts.extend(shouts);
    } else {
        let actors: Vec<_> = game
            .world_mut()
            .contact_actors()
            .into_iter()
            .map(|a| a.actor)
            .collect();
        for actor in actors {
            if actor == player {
                continue;
            }
            let hostile = game
                .world_mut()
                .actor(actor)
                .is_some_and(|a| a.combat_target == Some(player));
            if name == "killallactors" || hostile {
                game.world_mut().kill_actor(actor, Some(player));
            }
        }
    }
    Ok(Outcome::Done)
}

fn misc(game: &mut Game, name: &str, args: &Args<'_>) -> Result<Outcome, String> {
    match name {
        "getlocationcleared" | "setlocationcleared" => {
            let id = args.form(game, 0)?;
            let on = args.flag(1, true)?;
            let mut world = game.world_mut();
            let location = world.location(id).ok_or("requires a location")?;
            if name == "setlocationcleared" {
                location.cleared = on;
            }
            Ok(Outcome::Bool(location.cleared))
        }
        "teachword" | "unlockword" => {
            let word_id = args.form(game, 0)?;
            if game.world().tag_of(word_id) != Some(Tag(*b"WOOP")) {
                return Err("requires a word of power".into());
            }
            let mut world = game.world_mut();
            if name == "teachword" {
                world.words_taught.insert(word_id);
            } else {
                world.words_unlocked.insert(word_id);
            }
            Ok(Outcome::Done)
        }
        "getpcmiscstat" | "modpcmiscstat" => {
            let key = args.word(0)?.to_ascii_lowercase();
            let delta = if name == "modpcmiscstat" {
                args.int(1)?
            } else {
                0
            };
            let mut world = game.world_mut();
            let value = world.stats.entry(key).or_default();
            *value = value.saturating_add(delta);
            Ok(Outcome::Int(i64::from(*value)))
        }
        _ => {
            let player = game.player();
            let skill = args.word(0)?;
            if name == "incpcs" {
                game.world_mut()
                    .increment_skill(player, skill)
                    .map_err(|e| e.to_string())?;
                return Ok(Outcome::Done);
            }
            let amount = args.float(1)?;
            game.world_mut()
                .advance_skill(player, skill, amount)
                .map_err(|e| e.to_string())?;
            Ok(Outcome::Done)
        }
    }
}
