// SPDX-License-Identifier: GPL-3.0-or-later

//! Debug commands: a text interface that forces game state directly, so a
//! test can reach a state the gameplay path does not produce on its own.
//!
//! The problem this solves is narrow. A cold [`crate::Game::start`] runs the
//! quests flagged Start Game Enabled and nothing else, so anything behind
//! dialogue, a menu or a player choice is unreachable and its test has to
//! build a synthetic plugin instead of using the installed game. Skyrim
//! answers the same problem with its console: `setstage`, `prid`,
//! `player.additem`. This is that console's command layer without a UI.
//!
//! Three callers share it: the scenario harness through its `debug` step,
//! Rust tests through [`Console::run`], and later an in-game console, which
//! is a separate plan item and builds only the UI on top.
//!
//! # Not a back door
//!
//! Every command is an ordinary operation on [`World`](crate::World) and the
//! VM, the same call a native or a fragment would make. `setvampire` runs
//! `SendVampirismStateChanged` through the VM's own method dispatch, it does
//! not assign the flag. So a state a command reaches is a state the engine
//! can reach, and a test built on one is not testing a fiction. The cost is
//! that a command fails where the engine would fail, which is the point.
//!
//! # Grammar
//!
//! ```text
//! [<target>.]<command> [argument ...]
//! ```
//!
//! `<target>` is an editor id, a hex FormID (`0x0001A332`, or bare hex the
//! way the console takes it), or `player`. Without one a command that needs
//! a target uses the reference `prid` selected last. Names and command words
//! are case insensitive, `#` starts a comment.
//!
//! Naming follows the vanilla console where a command exists there
//! (`setstage`, `prid`, `placeatme`, `moveto`, `additem`, `setav`, `evp`,
//! `sv`), and the Papyrus function otherwise, so what a modder already knows
//! carries over. [`Console::help`] lists every command with its arguments.
//!
//! # What was read first
//!
//! The vanilla command names and their argument types come from
//! `CommonLibSSE-NG/include/RE/C/CommandTable.h` (the game's own command
//! table, with `SCRIPT_PARAM_TYPE` naming what each argument accepts) and
//! from `skymp/skyrim-platform/docs/ConsoleComandsDump.txt`, a dump of the
//! shipped command list. skymp's `ConsoleCommands.cpp` solves the same
//! problem for a multiplayer server and is the structural precedent: a typed
//! argument, one function per command, a dispatch table. None of the three
//! could be reused as code (two are C++ against the real engine's object
//! model, one is a plain list), so this is the same shape over `World`.

use std::fmt;

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

use crate::world::Handle;
use crate::Game;

/// What a command answers. Every variant renders through [`fmt::Display`],
/// which is what an in-game console would print, while a test reads the
/// typed value instead of parsing text.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The command did its work and has nothing to report.
    Done,
    Bool(bool),
    Int(i64),
    Float(f64),
    Form(GlobalFormId),
    Text(String),
    List(Vec<String>),
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outcome::Done => write!(f, "ok"),
            Outcome::Bool(b) => write!(f, "{b}"),
            Outcome::Int(n) => write!(f, "{n}"),
            Outcome::Float(v) => write!(f, "{v}"),
            Outcome::Form(id) => write!(f, "{id}"),
            Outcome::Text(s) => write!(f, "{s}"),
            Outcome::List(items) => write!(f, "{}", items.join("\n")),
        }
    }
}

/// One command's name, its arguments and what it does, for [`Console::help`].
struct Entry {
    name: &'static str,
    args: &'static str,
    what: &'static str,
}

/// Every command, in the order `help` prints them. The dispatch in
/// [`Console::run`] matches the same names, and a test checks the two agree
/// so a command can never be added to one and forgotten in the other.
const COMMANDS: &[Entry] = &[
    Entry {
        name: "help",
        args: "[prefix]",
        what: "list commands, filtered by prefix",
    },
    Entry {
        name: "prid",
        args: "<ref>",
        what: "select a reference for later commands",
    },
    Entry {
        name: "getselected",
        args: "",
        what: "the selected reference",
    },
    Entry {
        name: "lookup",
        args: "<name or id>",
        what: "resolve a name or hex id to a form",
    },
    Entry {
        name: "listrefs",
        args: "<cell>",
        what: "placed objects in a cell",
    },
    Entry {
        name: "findref",
        args: "<base> [count]",
        what: "placed references of a base object",
    },
    Entry {
        name: "pridbase",
        args: "<base>",
        what: "select the first placed reference of a base",
    },
    Entry {
        name: "setstage",
        args: "<quest> <n>",
        what: "set a quest stage and run its fragment",
    },
    Entry {
        name: "getstage",
        args: "<quest>",
        what: "the highest stage set",
    },
    Entry {
        name: "getstagedone",
        args: "<quest> <n>",
        what: "whether a stage has been set",
    },
    Entry {
        name: "startquest",
        args: "<quest>",
        what: "start a quest",
    },
    Entry {
        name: "stopquest",
        args: "<quest>",
        what: "stop a quest",
    },
    Entry {
        name: "objective",
        args: "<quest> <n> displayed|completed|failed [true|false]",
        what: "set an objective flag",
    },
    Entry {
        name: "forcealias",
        args: "<quest> <alias> <ref>",
        what: "fill a quest alias",
    },
    Entry {
        name: "showquestaliases",
        args: "<quest>",
        what: "aliases of a quest and what fills them",
    },
    Entry {
        name: "placeatme",
        args: "<base> [count]",
        what: "place new references of a base object",
    },
    Entry {
        name: "moveto",
        args: "<ref>",
        what: "move the target to another reference",
    },
    Entry {
        name: "movetocell",
        args: "<cell>",
        what: "move the target into a cell",
    },
    Entry {
        name: "enable",
        args: "",
        what: "enable the target",
    },
    Entry {
        name: "disable",
        args: "",
        what: "disable the target",
    },
    Entry {
        name: "getcell",
        args: "",
        what: "the cell the target is in",
    },
    Entry {
        name: "additem",
        args: "<item> [count]",
        what: "add items to the target",
    },
    Entry {
        name: "removeitem",
        args: "<item> [count]",
        what: "remove items from the target",
    },
    Entry {
        name: "getitemcount",
        args: "<item>",
        what: "how many the target holds",
    },
    Entry {
        name: "setav",
        args: "<name> <value>",
        what: "set an actor value",
    },
    Entry {
        name: "getav",
        args: "<name>",
        what: "read an actor value",
    },
    Entry {
        name: "modav",
        args: "<name> <delta>",
        what: "change an actor value",
    },
    Entry {
        name: "addspell",
        args: "<spell>",
        what: "add a spell to the target",
    },
    Entry {
        name: "removespell",
        args: "<spell>",
        what: "remove a spell from the target",
    },
    Entry {
        name: "addperk",
        args: "<perk>",
        what: "add a perk to the target",
    },
    Entry {
        name: "removeperk",
        args: "<perk>",
        what: "remove a perk from the target",
    },
    Entry {
        name: "addshout",
        args: "<shout>",
        what: "add a shout to the target",
    },
    Entry {
        name: "setrace",
        args: "<race>",
        what: "set the target's race",
    },
    Entry {
        name: "setfactionrank",
        args: "<faction> <rank>",
        what: "set the target's rank in a faction",
    },
    Entry {
        name: "setvampire",
        args: "[true|false]",
        what: "run SendVampirismStateChanged on the target",
    },
    Entry {
        name: "setwerewolf",
        args: "[true|false]",
        what: "run SendLycanthropyStateChanged on the target",
    },
    Entry {
        name: "kill",
        args: "[killer]",
        what: "kill the target",
    },
    Entry {
        name: "resurrect",
        args: "",
        what: "bring the target back",
    },
    Entry {
        name: "evp",
        args: "",
        what: "evaluate the target's package",
    },
    Entry {
        name: "getcurrentpackage",
        args: "",
        what: "the package the target is running",
    },
    Entry {
        name: "callfunc",
        args: "<Function> [argument ...]",
        what: "call a function on the target, natives included",
    },
    Entry {
        name: "sendevent",
        args: "<Event> [argument ...]",
        what: "deliver an event to the target's scripts",
    },
    Entry {
        name: "scriptshandling",
        args: "<Event>",
        what: "loaded scripts that define this event",
    },
    Entry {
        name: "sv",
        args: "",
        what: "scripts, variables and properties on the target",
    },
    Entry {
        name: "set",
        args: "<global> [to] <value>",
        what: "set a global variable",
    },
    Entry {
        name: "getglobalvalue",
        args: "<global>",
        what: "read a global variable",
    },
    Entry {
        name: "tick",
        args: "<seconds>",
        what: "advance real time and settle the VM",
    },
    Entry {
        name: "advancegametime",
        args: "<hours>",
        what: "advance game time by hours",
    },
    Entry {
        name: "getgametime",
        args: "",
        what: "game time in hours",
    },
];

/// A running command session. The only state it keeps is the selected
/// reference, the way the console keeps what `prid` picked.
#[derive(Debug, Clone, Default)]
pub struct Console {
    selected: Option<GlobalFormId>,
    /// Probe scripts assembled so far, so each gets its own script name.
    probes: usize,
}

impl Console {
    pub fn new() -> Console {
        Console::default()
    }

    /// The reference `prid` selected, if any.
    pub fn selected(&self) -> Option<GlobalFormId> {
        self.selected
    }

    /// Select a reference without going through a command line.
    pub fn select(&mut self, id: GlobalFormId) {
        self.selected = Some(id);
    }

    /// Every command with its arguments, one line each, filtered by an
    /// optional name prefix.
    pub fn help(prefix: &str) -> Vec<String> {
        COMMANDS
            .iter()
            .filter(|c| c.name.starts_with(prefix))
            .map(|c| {
                if c.args.is_empty() {
                    format!("{:<20} {}", c.name, c.what)
                } else {
                    format!("{:<20} {:<50} {}", c.name, c.args, c.what)
                }
            })
            .collect()
    }

    /// Run one command line. An empty line or a comment answers
    /// [`Outcome::Done`] without doing anything.
    ///
    /// # Errors
    ///
    /// The command word is unknown, an argument is missing or will not
    /// parse, a name resolves to no form, or the operation itself fails the
    /// way it would fail for a script.
    pub fn run(&mut self, game: &mut Game, line: &str) -> Result<Outcome, String> {
        let body = line.split('#').next().unwrap_or("").trim();
        if body.is_empty() {
            return Ok(Outcome::Done);
        }
        let mut words = body.split_whitespace();
        let head = words.next().ok_or("empty command")?;
        let args: Vec<&str> = words.collect();
        let (target, name) = split_target(head);
        let name = name.to_ascii_lowercase();
        self.dispatch(game, target, &name, &args)
    }

    /// Run a block of command lines, stopping at the first failure and
    /// naming the line that failed.
    ///
    /// # Errors
    ///
    /// Whatever [`Console::run`] returned, prefixed with the line number.
    pub fn run_block(&mut self, game: &mut Game, text: &str) -> Result<Outcome, String> {
        let mut last = Outcome::Done;
        for (n, line) in text.lines().enumerate() {
            last = self
                .run(game, line)
                .map_err(|e| format!("line {}: {e}", n + 1))?;
        }
        Ok(last)
    }

    /// The reference a command works on: an explicit `target.` prefix, else
    /// what `prid` selected.
    fn target_of(&self, game: &mut Game, target: Option<&str>) -> Result<GlobalFormId, String> {
        match target {
            Some(t) => resolve(game, t),
            None => self
                .selected
                .ok_or_else(|| "no target: use `prid <ref>` or a `target.` prefix".to_string()),
        }
    }

    #[allow(clippy::too_many_lines)]
    fn dispatch(
        &mut self,
        game: &mut Game,
        target: Option<&str>,
        name: &str,
        args: &[&str],
    ) -> Result<Outcome, String> {
        let arg = |i: usize| -> Result<&str, String> {
            args.get(i)
                .copied()
                .ok_or_else(|| format!("{name}: missing argument {}", i + 1))
        };
        match name {
            "help" => Ok(Outcome::List(Console::help(args.first().unwrap_or(&"")))),

            // ----- selection and lookup -----
            "prid" => {
                let id = resolve(game, arg(0)?)?;
                self.selected = Some(id);
                Ok(Outcome::Form(id))
            }
            "getselected" => self.selected.map_or_else(
                || Err("nothing selected".to_string()),
                |id| Ok(Outcome::Form(id)),
            ),
            "lookup" => {
                let id = resolve(game, arg(0)?)?;
                let world = game.world();
                let edid = world.editor_id(id).unwrap_or_default();
                let runtime = world.runtime_id(id);
                Ok(Outcome::Text(format!("{id} {edid} {runtime:08X}")))
            }
            "findref" | "pridbase" => {
                let named = arg(0)?.to_string();
                let base = resolve(game, &named)?;
                let limit = if name == "pridbase" {
                    1
                } else {
                    args.get(1).map_or(Ok(8.0), |c| number(c))? as usize
                };
                let found = game.world().references_of_base(base, limit.max(1));
                let first = found
                    .first()
                    .copied()
                    .ok_or_else(|| format!("nothing places {named}"))?;
                if name == "pridbase" {
                    self.selected = Some(first);
                    return Ok(Outcome::Form(first));
                }
                Ok(Outcome::List(
                    found.iter().map(ToString::to_string).collect(),
                ))
            }
            "listrefs" => {
                let cell = resolve(game, arg(0)?)?;
                let refs = game.world_mut().cell_references(cell);
                let world = game.world();
                Ok(Outcome::List(
                    refs.iter()
                        .map(|r| {
                            let edid = world.editor_id(*r).unwrap_or_default();
                            format!("{r} {edid}")
                        })
                        .collect(),
                ))
            }

            // ----- quests -----
            "setstage" => {
                let quest = resolve(game, arg(0)?)?;
                let stage = number(arg(1)?)? as u16;
                if game.set_stage(quest, stage) {
                    game.run_until_idle(64);
                    Ok(Outcome::Done)
                } else {
                    Err(format!("{} has no stage {stage}", arg(0)?))
                }
            }
            "getstage" => {
                let quest = resolve(game, arg(0)?)?;
                let stage = game.world_mut().quest_state(quest).map_or(0, |q| q.highest);
                Ok(Outcome::Int(i64::from(stage)))
            }
            "getstagedone" => {
                let quest = resolve(game, arg(0)?)?;
                let stage = number(arg(1)?)? as u16;
                let done = game
                    .world_mut()
                    .quest_state(quest)
                    .is_some_and(|q| q.done.contains(&stage));
                Ok(Outcome::Bool(done))
            }
            "startquest" => {
                let quest = resolve(game, arg(0)?)?;
                let started = game.start_quest(quest);
                game.run_until_idle(64);
                Ok(Outcome::Bool(started))
            }
            "stopquest" => {
                let quest = resolve(game, arg(0)?)?;
                let stopped = game.world_mut().stop_quest(quest);
                Ok(Outcome::Bool(stopped))
            }
            "objective" => {
                let quest = resolve(game, arg(0)?)?;
                let index = number(arg(1)?)? as u16;
                let field = arg(2)?.to_ascii_lowercase();
                let on = flag(args.get(3).copied())?;
                let mut world = game.world_mut();
                let objective = world
                    .objective(quest, index)
                    .ok_or_else(|| format!("no objective {index}"))?;
                match field.as_str() {
                    "displayed" => objective.displayed = on,
                    "completed" => objective.completed = on,
                    "failed" => objective.failed = on,
                    other => return Err(format!("unknown objective field {other}")),
                }
                Ok(Outcome::Done)
            }
            "forcealias" => {
                let quest = resolve(game, arg(0)?)?;
                let alias = number(arg(1)?)? as u32;
                let target = resolve(game, arg(2)?)?;
                if game.world_mut().force_alias(quest, alias, Some(target)) {
                    Ok(Outcome::Done)
                } else {
                    Err(format!("no alias {alias} on {}", arg(0)?))
                }
            }
            "showquestaliases" => {
                let named = arg(0)?.to_string();
                let quest = resolve(game, &named)?;
                let mut world = game.world_mut();
                let state = world
                    .quest_state(quest)
                    .ok_or_else(|| format!("{named} is not a quest"))?;
                let mut lines: Vec<String> = state
                    .aliases
                    .iter()
                    .map(|(id, filled)| match filled {
                        Some(f) => format!("{id} {f}"),
                        None => format!("{id} empty"),
                    })
                    .collect();
                lines.sort();
                Ok(Outcome::List(lines))
            }

            // ----- references -----
            "placeatme" => {
                let base = resolve(game, arg(0)?)?;
                let at = self.target_of(game, target)?;
                let count = args.get(1).map_or(Ok(1.0), |c| number(c))? as i64;
                let mut placed = Vec::new();
                for _ in 0..count.max(1) {
                    placed.push(game.world_mut().place_at(base, at, false));
                }
                let last = placed.last().copied();
                Ok(last.map_or(Outcome::Done, Outcome::Form))
            }
            "moveto" => {
                let id = self.target_of(game, target)?;
                let to = resolve(game, arg(0)?)?;
                if game.world_mut().move_to(id, to, [0.0; 3]) {
                    Ok(Outcome::Done)
                } else {
                    Err("neither side is a reference".to_string())
                }
            }
            "movetocell" => {
                let id = self.target_of(game, target)?;
                let cell = resolve(game, arg(0)?)?;
                let mut world = game.world_mut();
                let reference = world
                    .reference(id)
                    .ok_or_else(|| format!("{id} is not a reference"))?;
                reference.cell = Some(cell);
                Ok(Outcome::Done)
            }
            "enable" | "disable" => {
                let id = self.target_of(game, target)?;
                let on = name == "enable";
                let mut world = game.world_mut();
                let reference = world
                    .reference(id)
                    .ok_or_else(|| format!("{id} is not a reference"))?;
                reference.enabled = on;
                Ok(Outcome::Done)
            }
            "getcell" => {
                let id = self.target_of(game, target)?;
                let cell = game.world_mut().reference(id).and_then(|r| r.cell);
                cell.map_or_else(
                    || Ok(Outcome::Text("none".into())),
                    |c| Ok(Outcome::Form(c)),
                )
            }

            // ----- inventory -----
            "additem" | "removeitem" => {
                let holder = self.target_of(game, target)?;
                let item = resolve(game, arg(0)?)?;
                let count = args.get(1).map_or(Ok(1.0), |c| number(c))? as i32;
                let mut world = game.world_mut();
                if name == "additem" {
                    world.add_item(holder, item, count);
                    Ok(Outcome::Done)
                } else {
                    Ok(Outcome::Int(i64::from(
                        world.remove_item(holder, item, count, None),
                    )))
                }
            }
            "getitemcount" => {
                let holder = self.target_of(game, target)?;
                let item = resolve(game, arg(0)?)?;
                Ok(Outcome::Int(i64::from(
                    game.world_mut().item_count(holder, item),
                )))
            }

            // ----- actors -----
            "setav" | "modav" => {
                let actor = self.target_of(game, target)?;
                let key = arg(0)?.to_string();
                let value = number(arg(1)?)? as f32;
                let mut world = game.world_mut();
                if world.actor(actor).is_none() {
                    return Err(format!("{actor} is not an actor"));
                }
                if name == "setav" {
                    world.set_actor_value(actor, &key, value);
                } else {
                    world.mod_actor_value(actor, &key, value);
                }
                Ok(Outcome::Float(f64::from(world.actor_value(actor, &key))))
            }
            "getav" => {
                let actor = self.target_of(game, target)?;
                let key = arg(0)?;
                Ok(Outcome::Float(f64::from(
                    game.world_mut().actor_value(actor, key),
                )))
            }
            "addspell" | "removespell" | "addperk" | "removeperk" | "addshout" => {
                let actor = self.target_of(game, target)?;
                let form = resolve(game, arg(0)?)?;
                let mut world = game.world_mut();
                let state = world
                    .actor(actor)
                    .ok_or_else(|| format!("{actor} is not an actor"))?;
                let changed = match name {
                    "addspell" => state.spells.insert(form),
                    "removespell" => state.spells.remove(&form),
                    "addperk" => state.perks.insert(form),
                    "removeperk" => state.perks.remove(&form),
                    _ => state.shouts.insert(form),
                };
                Ok(Outcome::Bool(changed))
            }
            "setrace" => {
                let actor = self.target_of(game, target)?;
                let race = resolve(game, arg(0)?)?;
                let mut world = game.world_mut();
                let state = world
                    .actor(actor)
                    .ok_or_else(|| format!("{actor} is not an actor"))?;
                state.race = Some(race);
                Ok(Outcome::Done)
            }
            "setfactionrank" => {
                let actor = self.target_of(game, target)?;
                let faction = resolve(game, arg(0)?)?;
                let rank = number(arg(1)?)? as i32;
                let mut world = game.world_mut();
                let state = world
                    .actor(actor)
                    .ok_or_else(|| format!("{actor} is not an actor"))?;
                state.factions.insert(faction, rank);
                Ok(Outcome::Done)
            }
            "kill" => {
                let actor = self.target_of(game, target)?;
                let killer = match args.first() {
                    Some(k) => Some(resolve(game, k)?),
                    None => None,
                };
                Ok(Outcome::Bool(game.world_mut().kill_actor(actor, killer)))
            }
            "resurrect" => {
                let actor = self.target_of(game, target)?;
                let mut world = game.world_mut();
                let state = world
                    .actor(actor)
                    .ok_or_else(|| format!("{actor} is not an actor"))?;
                state.dead = false;
                state.killer = None;
                let health = state.base_values.get("health").copied().unwrap_or(100.0);
                state.values.insert("health".into(), health);
                Ok(Outcome::Done)
            }
            "evp" => {
                let actor = self.target_of(game, target)?;
                let vm = &mut game.vm;
                let package = game
                    .world
                    .borrow_mut()
                    .evaluate_package(actor, &mut |n, c| vm.create_object_of(n, c));
                game.run_until_idle(64);
                Ok(package.map_or(Outcome::Text("none".into()), Outcome::Form))
            }
            "getcurrentpackage" => {
                let actor = self.target_of(game, target)?;
                let package = game.world().current_package(actor);
                package.map_or_else(
                    || Ok(Outcome::Text("none".into())),
                    |p| Ok(Outcome::Form(p)),
                )
            }

            // ----- the vampirism and lycanthropy senders -----
            //
            // These run the native rather than assigning the flag, so the
            // event reaches the actor's scripts exactly as it does when a
            // Dawnguard fragment makes the same call.
            "setvampire" | "setwerewolf" => {
                let actor = self.target_of(game, target)?;
                let on = flag(args.first().copied())?;
                let function = if name == "setvampire" {
                    "SendVampirismStateChanged"
                } else {
                    "SendLycanthropyStateChanged"
                };
                self.probes += 1;
                call_on(game, self.probes, actor, function, &[Value::Bool(on)])?;
                Ok(Outcome::Done)
            }

            // ----- the VM -----
            "callfunc" => {
                let id = self.target_of(game, target)?;
                let function = arg(0)?.to_string();
                let values = literal_args(game, &args[1..]);
                self.probes += 1;
                let result = call_on(game, self.probes, id, &function, &values)?;
                Ok(Outcome::Text(format!("{result:?}")))
            }
            "sendevent" => {
                let id = self.target_of(game, target)?;
                let event = arg(0)?.to_string();
                let values = literal_args(game, &args[1..]);
                let threads = game.send_event(id, &event, &values);
                Ok(Outcome::Int(threads.len() as i64))
            }
            "scriptshandling" => Ok(Outcome::List(game.vm.scripts_handling(arg(0)?))),
            "sv" => {
                let id = self.target_of(game, target)?;
                let object = game.object_of_form(id);
                let mut lines = Vec::new();
                for script in game.vm.scripts_on(object) {
                    lines.push(format!("script {script}"));
                    for (var, value) in game.vm.vars_on(object, &script) {
                        lines.push(format!("  {var} = {value:?}"));
                    }
                    for property in game.vm.properties_on(object, &script) {
                        lines.push(format!("  property {property}"));
                    }
                }
                Ok(Outcome::List(lines))
            }

            // ----- globals and the clock -----
            "set" => {
                let global = resolve(game, arg(0)?)?;
                // `set <global> to <value>` is the console's own spelling.
                let rest = if args.get(1).is_some_and(|w| w.eq_ignore_ascii_case("to")) {
                    arg(2)?
                } else {
                    arg(1)?
                };
                let value = number(rest)? as f32;
                game.world_mut().set_global(global, value);
                Ok(Outcome::Done)
            }
            "getglobalvalue" => {
                let global = resolve(game, arg(0)?)?;
                Ok(Outcome::Float(f64::from(
                    game.world_mut().global_value(global),
                )))
            }
            "tick" => {
                game.tick(number(arg(0)?)?);
                Ok(Outcome::Done)
            }
            "advancegametime" => {
                let hours = number(arg(0)?)?;
                let timescale = game.world().clock.timescale.max(f64::EPSILON);
                game.tick(hours * 3600.0 / timescale);
                Ok(Outcome::Done)
            }
            "getgametime" => Ok(Outcome::Float(game.world().clock.game_hours())),

            other => Err(format!("unknown command {other}, try `help`")),
        }
    }
}

/// `player.additem` splits into the target and the command. A bare command
/// word has no target. Only the last dot separates, so a name with a dot in
/// it still works as a command word.
fn split_target(head: &str) -> (Option<&str>, &str) {
    match head.rsplit_once('.') {
        Some((target, name)) if !target.is_empty() && !name.is_empty() => (Some(target), name),
        _ => (None, head),
    }
}

/// A name, a hex FormID or `player`. Hex is accepted with an `0x` prefix and
/// bare, the way the console takes `prid 1a332`, but a bare form that also
/// names a form is read as the name first, since editor ids win in the
/// console too.
fn resolve(game: &mut Game, name: &str) -> Result<GlobalFormId, String> {
    if name.eq_ignore_ascii_case("player") {
        return Ok(game.player());
    }
    if let Some(id) = game.form(name) {
        return Ok(id);
    }
    let hex = name
        .strip_prefix("0x")
        .or_else(|| name.strip_prefix("0X"))
        .unwrap_or(name);
    if let Ok(raw) = u32::from_str_radix(hex, 16) {
        if let Some(id) = game.world().from_runtime_id(raw) {
            return Ok(id);
        }
    }
    Err(format!("no form named {name}"))
}

fn number(s: &str) -> Result<f64, String> {
    s.parse().map_err(|_| format!("expected a number, got {s}"))
}

fn flag(word: Option<&str>) -> Result<bool, String> {
    match word {
        None | Some("true" | "1") => Ok(true),
        Some("false" | "0") => Ok(false),
        Some(other) => Err(format!("expected true or false, got {other}")),
    }
}

/// Command arguments as Papyrus values: `true` and `false` are booleans, a
/// number is an int when it has no fractional part and a float otherwise, a
/// name that resolves is the form's object, anything else is a string.
fn literal_args(game: &mut Game, words: &[&str]) -> Vec<Value> {
    let mut out = Vec::new();
    for w in words {
        let value = if w.eq_ignore_ascii_case("true") {
            Value::Bool(true)
        } else if w.eq_ignore_ascii_case("false") {
            Value::Bool(false)
        } else if let Ok(n) = w.parse::<i32>() {
            Value::Int(n)
        } else if let Ok(f) = w.parse::<f32>() {
            Value::Float(f)
        } else if let Ok(id) = resolve(game, w) {
            Value::Object(game.object_of_form(id))
        } else {
            Value::string(w)
        };
        out.push(value);
    }
    out
}

/// The Papyrus type name a value is declared as in a probe script.
fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::Str(_) => "String",
        _ => "Form",
    }
}

/// Call a function on a form's object the way a script calls it.
///
/// `Vm::call` reaches only functions a loaded script defines: natives are
/// dispatched by the interpreter's own `CallMethod` handler, which needs a
/// running thread, so a host that called `Vm::call` would miss every native
/// and `setvampire` would fail on an actor with no script of its own. So the
/// command assembles a one-function probe script that performs a real
/// `CallMethod` and runs that. The instruction under test is then the same
/// one a vanilla script compiles to, which is the point of the layer: no
/// path exists here that a script could not take. `crates/game/tests`
/// drives its native scenarios the same way.
fn call_on(
    game: &mut Game,
    probe: usize,
    id: GlobalFormId,
    function: &str,
    args: &[Value],
) -> Result<Value, String> {
    use soe_papyrus::asm::{ident, s, FunctionBuilder, ScriptBuilder};
    use soe_papyrus::Opcode::{CallMethod, Return};

    let object = game.object_of(Handle::Form(id));
    let name = format!("SoeDebugProbe{probe}");
    let mut fun = FunctionBuilder::new("Run", "Var")
        .global()
        .param("t", "Form")
        .local("r", "Var");
    let mut operands = vec![s(function), ident("t"), ident("r")];
    for (k, a) in args.iter().enumerate() {
        let param = format!("a{k}");
        fun = fun.param(&param, type_of(a));
        operands.push(ident(&param));
    }
    let fun = fun
        .ins(CallMethod, operands)
        .ins(Return, vec![ident("r")])
        .build();
    let bytes = ScriptBuilder::new(&name, "Form").function(fun).bytes();
    game.load_script(&bytes)
        .map_err(|e| format!("{function}: building the probe: {e}"))?;
    let mut all = vec![Value::Object(object)];
    all.extend_from_slice(args);
    let thread = game
        .vm
        .call_global(&name, "Run", &all)
        .map_err(|e| format!("{function}: {e}"))?;
    game.run_until_idle(64);
    Ok(game.vm.thread_result(thread).unwrap_or(Value::None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_target_prefix_splits_off_the_command() {
        assert_eq!(split_target("player.additem"), (Some("player"), "additem"));
        assert_eq!(split_target("additem"), (None, "additem"));
        assert_eq!(split_target("0x14.setav"), (Some("0x14"), "setav"));
        assert_eq!(split_target(".additem"), (None, ".additem"));
        assert_eq!(split_target("additem."), (None, "additem."));
    }

    #[test]
    fn flags_take_the_console_spellings() {
        assert_eq!(flag(None), Ok(true));
        assert_eq!(flag(Some("1")), Ok(true));
        assert_eq!(flag(Some("false")), Ok(false));
        assert_eq!(flag(Some("0")), Ok(false));
        assert!(flag(Some("maybe")).is_err());
    }

    #[test]
    fn help_lists_and_filters() {
        assert!(Console::help("").len() > 40);
        let quest = Console::help("get");
        assert!(quest.iter().all(|l| l.starts_with("get")));
        assert!(Console::help("nosuch").is_empty());
    }

    /// Every command `help` advertises must be reachable, and every command
    /// the dispatch answers must be advertised. Without this the two drift
    /// and a command exists that nobody can find.
    #[test]
    fn the_help_table_and_the_dispatch_agree() {
        let mut names: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        names.sort_unstable();
        let mut deduped = names.clone();
        deduped.dedup();
        assert_eq!(names, deduped, "a command is listed twice");
        let source = include_str!("debug.rs");
        for name in &names {
            let quoted = format!("\"{name}\"");
            assert!(
                source.matches(&quoted).count() >= 2,
                "{name} is in the help table but has no dispatch arm"
            );
        }
    }

    #[test]
    fn outcomes_render_as_a_console_would_print_them() {
        assert_eq!(Outcome::Done.to_string(), "ok");
        assert_eq!(Outcome::Int(3).to_string(), "3");
        assert_eq!(Outcome::Bool(false).to_string(), "false");
        assert_eq!(
            Outcome::List(vec!["a".into(), "b".into()]).to_string(),
            "a\nb"
        );
    }
}
