// SPDX-License-Identifier: GPL-3.0-or-later

//! Actor value console operations follow UESP over the shared component model.

use super::{Args, Game, GlobalFormId, Outcome};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "forceav"
            | "damageactorvalue"
            | "restoreactorvalue"
            | "resethealth"
            | "getbaseav"
            | "getavinfo"
            | "getactorvaluepercent"
            | "getpermanentactorvalue"
    )
}

pub(super) fn run(
    game: &mut Game,
    id: GlobalFormId,
    name: &str,
    args: &Args<'_>,
) -> Result<Outcome, String> {
    let key = if name == "resethealth" {
        "health"
    } else {
        args.word(0)?
    };
    if crate::actor_values::actor_value_index(key).is_none() {
        return Err(format!("unknown actor value {key}"));
    }
    let mut world = game.world_mut();
    match name {
        "getbaseav" => return Ok(Outcome::Float(f64::from(world.base_actor_value(id, key)))),
        "getactorvaluepercent" => {
            return Ok(Outcome::Float(f64::from(
                world.actor_value_percentage(id, key),
            )))
        }
        "getavinfo" | "getpermanentactorvalue" => {
            let c = world.actor(id).ok_or("not an actor")?.values.get(key);
            if name == "getpermanentactorvalue" {
                return Ok(Outcome::Float(f64::from(c.base() + c.permanent())));
            }
            return Ok(Outcome::Text(format!(
                "base={} permanent={} temporary={} damage={} current={}",
                c.base(),
                c.permanent(),
                c.temporary(),
                c.damage(),
                c.current()
            )));
        }
        "resethealth" => world.restore_actor_value(id, key, f32::MAX),
        "forceav" => world.force_actor_value(id, key, args.float(1)?),
        "damageactorvalue" => world.damage_actor_value(id, key, args.float(1)?),
        _ => world.restore_actor_value(id, key, args.float(1)?),
    }
    .map_err(|e| e.to_string())?;
    Ok(Outcome::Done)
}
