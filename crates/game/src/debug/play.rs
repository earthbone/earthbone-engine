// SPDX-License-Identifier: GPL-3.0-or-later

//! Play console syntax and dispatch, from the local UESP Console.wikitext snapshot.

use super::{number, resolve, Console, HostRequest, Outcome};
use crate::Game;
use soe_esm::GlobalFormId;

mod actors;
mod environment;
mod inventory;
mod quests;

pub(super) fn tokenize(line: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            ';' | '#' if !quoted => break,
            c if (c.is_whitespace() || c == ',') && !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if quoted {
        return Err("unterminated quoted argument".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

pub(super) fn integer(word: &str, min: i32, max: i32) -> Result<i32, String> {
    let value: i32 = word
        .parse()
        .map_err(|_| format!("expected an integer, got {word}"))?;
    if value < min || value > max {
        return Err(format!("integer outside {min} to {max}"));
    }
    Ok(value)
}

pub(super) struct Args<'a>(pub &'a [&'a str]);
impl Args<'_> {
    pub fn word(&self, i: usize) -> Result<&str, String> {
        self.0
            .get(i)
            .copied()
            .ok_or_else(|| format!("missing argument {}", i + 1))
    }
    pub fn int(&self, i: usize) -> Result<i32, String> {
        integer(self.word(i)?, i32::MIN + 1, i32::MAX)
    }
    pub fn float(&self, i: usize) -> Result<f32, String> {
        Ok(number(self.word(i)?)? as f32)
    }
    pub fn form(&self, game: &mut Game, i: usize) -> Result<GlobalFormId, String> {
        resolve(game, self.word(i)?)
    }
    pub fn editor_form(&self, game: &mut Game, i: usize) -> Result<GlobalFormId, String> {
        super::resolve_editor(game, self.word(i)?)
    }
    pub fn flag(&self, i: usize, default: bool) -> Result<bool, String> {
        self.0.get(i).map_or(Ok(default), |w| super::flag(Some(w)))
    }
}

pub(super) fn help(game: &mut Game, args: &[&str]) -> Result<Outcome, String> {
    if args.is_empty() {
        return Ok(Outcome::List(Console::help("")));
    }
    let query = args.first().copied().unwrap_or("").to_ascii_lowercase();
    let filter = args.get(1).map_or(Ok(0), |w| integer(w, 0, 4))?;
    let mut lines = if filter == 0 || filter == 1 {
        Console::help(&query)
    } else {
        Vec::new()
    };
    if filter == 0 || filter == 4 {
        let world = game.world();
        for (id, rref) in world.order().winners() {
            let record = world.order().record(rref);
            let edid = record.editor_id().unwrap_or_default();
            let name = world.name_of(id).unwrap_or_default();
            if (query.is_empty()
                || edid.to_ascii_lowercase().contains(&query)
                || name.to_ascii_lowercase().contains(&query))
                && args
                    .get(2)
                    .is_none_or(|tag| record.tag.to_string().eq_ignore_ascii_case(tag))
            {
                lines.push(format!(
                    "{}: ({:08X}) {edid} {name}",
                    record.tag,
                    world.runtime_id(id)
                ));
            }
        }
    }
    if filter == 0 || filter == 2 {
        let globals: Vec<_> = game
            .world()
            .order()
            .winners_of(soe_esm::Tag(*b"GLOB"))
            .map(|(id, _)| id)
            .collect();
        for id in globals {
            let edid = game.world().editor_id(id).unwrap_or_default();
            if edid.to_ascii_lowercase().contains(&query) {
                lines.push(format!("{edid} = {}", game.world_mut().global_value(id)));
            }
        }
    }
    if filter == 0 || filter == 3 {
        let settings: Vec<_> = game
            .world()
            .order()
            .winners_of(soe_esm::Tag(*b"GMST"))
            .filter_map(|(_, r)| r.editor_id())
            .collect();
        for name in settings {
            if name.to_ascii_lowercase().contains(&query) {
                lines.push(format!("{name} = {:?}", game.world().game_setting(&name)));
            }
        }
    }
    lines.sort();
    lines.dedup();
    Ok(Outcome::List(lines))
}

pub(super) fn change_items(
    game: &mut Game,
    holder: GlobalFormId,
    item: GlobalFormId,
    count: i32,
    add: bool,
) -> Result<Outcome, String> {
    if game.world_mut().reference(holder).is_none() {
        return Err("target is not a reference".into());
    }
    let vm = &mut game.vm;
    let mut world = game.world.borrow_mut();
    if add && count > 0 {
        if !world.add_item_notified(holder, item, count, &mut |n, c| vm.create_object_of(n, c)) {
            return Err("cannot add item or resolve leveled list".into());
        }
    } else {
        let count = if add { -count } else { count };
        if count < 0 {
            return Err("removeitem requires a nonnegative count".into());
        }
        world.remove_item_notified(holder, item, count, None, &mut |n, c| {
            vm.create_object_of(n, c)
        });
    }
    Ok(Outcome::Done)
}

impl Console {
    pub(super) fn play_command(
        &mut self,
        game: &mut Game,
        target: Option<&str>,
        name: &str,
        words: &[&str],
    ) -> Result<Outcome, String> {
        let args = Args(words);
        if environment::handles(name) {
            return self.environment_command(game, target, name, &args);
        }
        if quests::handles(name) {
            return quests::run(game, name, &args);
        }
        if inventory::handles(name) {
            let id = self.target_of(game, target)?;
            return self.inventory_command(game, id, name, &args);
        }
        if actors::handles(name) {
            return self.actor_command(game, target, name, &args);
        }
        match super::catalog::entry(name) {
            Some(entry) => Err(format!("{}: {}", entry.name, entry.reason)),
            None => Err(format!("unknown command {name}, try help")),
        }
    }
}
