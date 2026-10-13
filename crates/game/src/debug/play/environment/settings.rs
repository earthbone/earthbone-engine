// SPDX-License-Identifier: GPL-3.0-or-later

//! Settings console operations follow UESP and the existing GMST override store.

use super::{Args, Game, Outcome, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "getgs"
            | "setgs"
            | "getinisetting"
            | "setinisetting"
            | "fw"
            | "sw"
            | "releaseweatheroverride"
    )
}

pub(super) fn run(game: &mut Game, name: &str, args: &Args<'_>) -> Result<Outcome, String> {
    if matches!(name, "fw" | "sw") {
        let weather = args.form(game, 0)?;
        game.world_mut()
            .set_active_weather(weather, args.flag(1, false)?, name == "fw")
            .map_err(|e| e.to_string())?;
        return Ok(Outcome::Done);
    }
    if name == "releaseweatheroverride" {
        game.world_mut().release_weather_override();
        return Ok(Outcome::Done);
    }
    let key = args.word(0)?.to_ascii_lowercase();
    if name.ends_with("inisetting") {
        if name.starts_with("set") {
            game.world_mut()
                .ini
                .insert(key.clone(), args.word(1)?.to_owned());
        }
        return game
            .world()
            .ini
            .get(&key)
            .cloned()
            .map(Outcome::Text)
            .ok_or_else(|| "INI setting not found".into());
    }
    let current = game
        .world()
        .game_setting(&key)
        .ok_or("game setting not found")?;
    if name == "getgs" {
        return Ok(Outcome::Text(format!("{current:?}")));
    }
    let value = match current {
        Value::Float(_) => Value::Float(args.float(1)?),
        Value::Int(_) => Value::Int(args.int(1)?),
        Value::Bool(_) => Value::Bool(args.flag(1, false)?),
        Value::Str(_) => Value::string(args.word(1)?),
        _ => return Err("unsupported setting type".into()),
    };
    game.world_mut().settings.insert(key, value);
    Ok(Outcome::Done)
}
