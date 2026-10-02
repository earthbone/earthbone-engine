// SPDX-License-Identifier: GPL-3.0-or-later

//! Authored headless playthrough routes over the shared Game and PLY-01 inputs.
//! PLY-02 supplies the route grammar. CK PathToReference, Activate and quest
//! queries supply gameplay contracts. Navigation uses the existing Detour-based
//! NAVM graph rather than implementing another pathfinder. See docs/game.md.

mod navigation;
mod parse;

use std::fmt::Write as _;
use std::path::Path;

use soe_esm::{GlobalFormId, LoadOrder};

use crate::player_input::{Command, Confirmation, STEP_SECONDS};
use crate::Game;

/// A route input, startup or execution failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("route line {line}: {reason}")]
    Parse { line: usize, reason: String },
    #[error("route startup: {0}")]
    Startup(String),
}

/// A point in a named cell, in Skyrim coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub cell: String,
    pub position: [f32; 3],
}

/// A placed reference or an explicit cell marker.
#[derive(Debug, Clone, PartialEq)]
pub enum Destination {
    Reference(String),
    Marker(Marker),
}

/// One authored operation or assertion. Time bounds count 1/60-second ticks.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Navigate {
        destination: Destination,
        speed: f32,
        max_ticks: u32,
    },
    Activate(String),
    Talk {
        speaker: String,
        topic: String,
    },
    WaitStage {
        quest: String,
        stage: u16,
        max_ticks: u32,
    },
    ExpectStage {
        quest: String,
        stage: u16,
    },
    ExpectPosition {
        marker: Marker,
        tolerance: f32,
    },
    ExpectItem {
        holder: String,
        item: String,
        count: i32,
    },
    ExpectNoVmError,
}

/// Source lines paired with typed operations, retained for failure reports.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Route {
    pub steps: Vec<(usize, Step)>,
}

/// Completed route, including the first failure and a deterministic state dump.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub completed: usize,
    pub ticks: u64,
    pub failure: Option<String>,
    pub state: String,
    pub trace: Vec<String>,
}

impl Report {
    /// Whether every authored operation and assertion completed.
    pub fn passed(&self) -> bool {
        self.failure.is_none()
    }

    /// Render the route result, live world state and ordered execution trace.
    pub fn format(&self) -> String {
        let outcome = self.failure.as_deref().unwrap_or("PASS");
        format!(
            "{outcome}\ncompleted={} ticks={}\nworld state:\n{}trace:\n{}\n",
            self.completed,
            self.ticks,
            self.state,
            self.trace.join("\n")
        )
    }
}

/// Read a route and run it from an external ESS save, without a rendering host.
/// Plugins are loaded in the save's regular and light plugin order. Scripts
/// come from the existing archive/loose overlay. Import failures are fatal.
///
/// # Errors
/// Reports route syntax, missing plugins/scripts, malformed ESS or unsupported
/// restoration through the existing transactional world/VM importer.
pub fn run_from_save(data: &Path, save_path: &Path, route_path: &Path) -> Result<Report, Error> {
    let text = std::fs::read_to_string(route_path).map_err(|e| Error::Startup(e.to_string()))?;
    let route = Route::parse(&text)?;
    let mut game = load_save(data, save_path)?;
    Ok(run(&mut game, &route))
}

/// Load the saved plugin names and supported ESS world/VM state before playing.
/// This uses the runtime's existing save restoration boundary.
///
/// # Errors
/// Returns missing inputs, plugin/script parse errors or ESS import failures.
pub fn load_save(data: &Path, save_path: &Path) -> Result<Game, Error> {
    load(data, save_path, SaveMode::Resume)
}

/// Restore supported ESS world fields and bind fresh scripts for checkpoint
/// routes. Saved script variables and execution are not imported in this mode.
///
/// # Errors
/// Returns missing inputs, plugin/script parse errors or world import failures.
pub fn load_world_checkpoint(data: &Path, save_path: &Path) -> Result<Game, Error> {
    load(data, save_path, SaveMode::WorldCheckpoint)
}

#[derive(Clone, Copy)]
enum SaveMode {
    Resume,
    WorldCheckpoint,
}

fn load(data: &Path, save_path: &Path, mode: SaveMode) -> Result<Game, Error> {
    let startup = |e: String| Error::Startup(e);
    let save = soe_save::Save::open(save_path).map_err(|e| startup(e.to_string()))?;
    let mut order = LoadOrder::new();
    for name in save.plugins.iter().chain(&save.light_plugins) {
        let name = name.to_text();
        let path = data.join(&name);
        let plugin = soe_esm::read_plugin(&path)
            .map_err(|e| startup(format!("{}: {e}", path.display())))?
            .map_err(|e| startup(format!("{}: {e}", path.display())))?;
        order
            .add(&name, plugin)
            .map_err(|e| startup(e.to_string()))?;
    }
    order.build_cell_index();
    let mut game = Game::new(order);
    let scripts = game.load_scripts_from_data(data);
    if !scripts.scripts_failed.is_empty() {
        return Err(startup(format!(
            "script load errors: {:?}",
            scripts.scripts_failed
        )));
    }
    match mode {
        SaveMode::Resume => game
            .restore_vanilla_save_file(save_path)
            .map_err(|e| startup(e.to_string()))?,
        SaveMode::WorldCheckpoint => {
            let mut world = game.world_mut();
            let mut restore = || -> Result<(), crate::vanilla_save::RestoreError> {
                world.restore_vanilla_globals(&save)?;
                world.restore_vanilla_npcs(&save)?;
                world.restore_vanilla_record_changes(&save)?;
                world.restore_vanilla_reference_placements(&save)?;
                world.restore_vanilla_actor_templates(&save)?;
                world.restore_vanilla_inventories(&save)?;
                world.restore_vanilla_reference_extras(&save)?;
                world.restore_vanilla_quests(&save)?;
                world.restore_vanilla_lists(&save)?;
                Ok(())
            };
            restore().map_err(|e| startup(e.to_string()))?;
        }
    }
    game.bind();
    let cell = game.world().loaded_cell();
    if let Some(cell) = cell {
        game.bind_cell(cell);
    }
    Ok(game)
}

/// Execute a parsed route against the live world, stopping at its first failure.
/// Route state and reports are transient. Gameplay mutations stay in Game.
pub fn run(game: &mut Game, route: &Route) -> Report {
    let mut report = Report::default();
    append_log(game, &mut report, 0);
    for (index, (line, step)) in route.steps.iter().enumerate() {
        report
            .trace
            .push(format!("step {} line {line}: {step:?}", index + 1));
        if let Err(reason) = execute(game, step, &mut report) {
            report.failure = Some(format!(
                "FAIL step {} line {line}: {step:?}: {reason}",
                index + 1
            ));
            break;
        }
        report.completed += 1;
    }
    report.state = snapshot(game);
    report
}

fn resolve(game: &mut Game, name: &str) -> Result<GlobalFormId, String> {
    let player = game.player();
    let world = game.world();
    let id = if name.eq_ignore_ascii_case("player") {
        Some(player)
    } else if let Some((file, object)) = name.rsplit_once(':') {
        let object = u32::from_str_radix(object.trim_start_matches("0x"), 16)
            .map_err(|e| format!("invalid form {name}: {e}"))?;
        if object > 0x00FF_FFFF {
            return Err(format!("object ID exceeds 24 bits: {name}"));
        }
        if file.eq_ignore_ascii_case("dynamic") {
            Some(GlobalFormId {
                file: crate::world::DYNAMIC_FILE,
                object,
            })
        } else {
            world.form_from_file(object, file)
        }
    } else {
        world.form(name)
    };
    id.filter(|id| world.exists(*id))
        .ok_or_else(|| format!("form unavailable: {name}"))
}

fn action(game: &mut Game, command: &Command, report: &mut Report) -> Result<(), String> {
    let log_start = game.vm.log.entries.len();
    let result = game.apply_player_input(command).map_err(|e| e.to_string());
    append_log(game, report, log_start);
    tick(game, report);
    report
        .trace
        .push(format!("tick {}: {command:?}: {result:?}", report.ticks));
    result.map(|_| ())
}

fn tick(game: &mut Game, report: &mut Report) {
    let log_start = game.vm.log.entries.len();
    game.tick(STEP_SECONDS);
    report.ticks += 1;
    append_log(game, report, log_start);
}

pub(super) fn append_log(game: &Game, report: &mut Report, start: usize) {
    report.trace.extend(
        game.vm.log.entries[start..]
            .iter()
            .map(|entry| format!("papyrus {:.6}: {}", entry.time, entry.message)),
    );
}

fn execute(game: &mut Game, step: &Step, report: &mut Report) -> Result<(), String> {
    match step {
        Step::Navigate {
            destination,
            speed,
            max_ticks,
        } => navigation::navigate(game, destination, *speed, *max_ticks, report),
        Step::Activate(name) => {
            let target = resolve(game, name)?;
            action(game, &Command::Activate { target }, report)
        }
        Step::Talk { speaker, topic } => {
            let target = resolve(game, speaker)?;
            let topic = resolve(game, topic)?;
            action(game, &Command::Activate { target }, report)?;
            action(game, &Command::ChooseTopic { topic }, report)?;
            action(game, &Command::ConfirmMenu(Confirmation::Dialogue), report)
        }
        Step::WaitStage {
            quest,
            stage,
            max_ticks,
        } => {
            if *max_ticks > 36_000 {
                return Err("invalid wait bound".into());
            }
            let quest = resolve(game, quest)?;
            for elapsed in 0..=*max_ticks {
                let done = game
                    .world_mut()
                    .quest_state(quest)
                    .ok_or("not a quest")?
                    .done
                    .contains(stage);
                if done {
                    return Ok(());
                }
                if elapsed < *max_ticks {
                    tick(game, report);
                }
            }
            Err(format!(
                "timeout waiting for {quest} stage {stage} after {max_ticks} ticks"
            ))
        }
        Step::ExpectStage { quest, stage } => {
            let quest = resolve(game, quest)?;
            let actual = game
                .world_mut()
                .quest_state(quest)
                .ok_or("not a quest")?
                .highest;
            if actual == *stage {
                Ok(())
            } else {
                Err(format!("quest {quest} stage is {actual}, expected {stage}"))
            }
        }
        Step::ExpectPosition { marker, tolerance } => {
            let cell = resolve(game, &marker.cell)?;
            let player = game.player();
            let world = game.world();
            let reference = world
                .reference_ref(player)
                .ok_or("player reference unavailable")?;
            let distance = navigation::distance(reference.position, marker.position);
            if reference.cell == Some(cell) && distance <= *tolerance {
                Ok(())
            } else {
                Err(format!("player cell {:?} position {:?}, expected {cell} {:?} within {tolerance}, distance {distance}", reference.cell, reference.position, marker.position))
            }
        }
        Step::ExpectItem {
            holder,
            item,
            count,
        } => {
            let holder = resolve(game, holder)?;
            let item = resolve(game, item)?;
            let mut world = game.world_mut();
            world
                .reference(holder)
                .ok_or("inventory holder unavailable")?;
            let actual =
                crate::conditions::inventory_count(&mut world, holder, item, &mut Vec::new())
                    .map_err(|error| error.to_string())?;
            if actual == *count {
                Ok(())
            } else {
                Err(format!(
                    "{holder} item {item} count is {actual}, expected {count}"
                ))
            }
        }
        Step::ExpectNoVmError => {
            if let Some(entry) = game
                .vm
                .log
                .entries
                .iter()
                .find(|entry| entry.message.starts_with("error:"))
            {
                Err(format!("Papyrus VM {}: {}", entry.time, entry.message))
            } else {
                Ok(())
            }
        }
    }
}

fn snapshot(game: &mut Game) -> String {
    let player = game.player();
    let world = game.world();
    let mut out = format!("player {player}\n");
    if let Some(reference) = world.reference_ref(player) {
        let _ = writeln!(
            out,
            "cell {:?} position {:?} rotation {:?}",
            reference.cell, reference.position, reference.rotation
        );
        let mut items: Vec<_> = reference.inventory.iter().collect();
        items.sort_by_key(|(id, _)| **id);
        for (id, count) in items {
            let _ = writeln!(out, "item {id} count {count}");
        }
    }
    for id in world.loaded_quests() {
        if let Some(quest) = world.quest(id) {
            let _ = writeln!(
                out,
                "quest {id} stage {} done {:?} running {} completed {} failed {}",
                quest.highest, quest.done, quest.running, quest.completed, quest.failed
            );
        }
    }
    let _ = writeln!(out, "vm threads {:?}", game.vm.live_threads());
    out
}
