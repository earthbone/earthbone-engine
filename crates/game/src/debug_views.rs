// SPDX-License-Identifier: GPL-3.0-or-later

//! Console diagnostics over World and VM, using the local UESP Console command
//! names. NAVM flags follow xEdit wbNavmeshTriangleFlags and wbNavmeshCoverFlags.
//! Host requests are transient and consumed by godot-host's debug_draw bridge.

use std::collections::VecDeque;
use std::path::PathBuf;

use soe_esm::GlobalFormId;

use crate::debug::{Console, Entry, Outcome};
use crate::{Game, World};

mod dialogue;
pub use dialogue::npc_dialogue;
#[cfg(test)]
mod tests;

/// Diagnostics that need live host resources rather than record state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostRequest {
    Animation(GlobalFormId),
    Scenegraph(GlobalFormId),
    MapCamera,
    Hdr,
    LocalMap,
}

/// Off by default, session-only presentation switches and pending host work.
#[derive(Debug, Default)]
pub struct Views {
    pub navmesh: bool,
    pub paths: bool,
    pub ai_text: bool,
    pub pivot: Option<GlobalFormId>,
    pub papyrus: bool,
    pub papyrus_directory: Option<PathBuf>,
    pub host_requests: VecDeque<HostRequest>,
    papyrus_cursor: usize,
    previous_papyrus_logs: Option<crate::papyrus_logs::FileLogs>,
}

impl Views {
    /// Whether any world-space geometry or labels need updating.
    pub fn drawing(&self) -> bool {
        self.navmesh || self.paths || self.pivot.is_some() || self.ai_text
    }
}

pub(crate) const COMMANDS: &[Entry] = &[
    Entry {
        name: "togglenavmesh",
        args: "",
        what: "draw loaded NAVM flags, links and player triangle",
    },
    Entry {
        name: "tnm",
        args: "",
        what: "ToggleNavMesh",
    },
    Entry {
        name: "togglepathline",
        args: "",
        what: "draw actor paths, waypoints and destinations",
    },
    Entry {
        name: "tpl",
        args: "",
        what: "TogglePathLine",
    },
    Entry {
        name: "printailist",
        args: "",
        what: "loaded actors, packages, procedures, targets and routes",
    },
    Entry {
        name: "pai",
        args: "",
        what: "PrintAiList",
    },
    Entry {
        name: "showailist",
        args: "",
        what: "selected actor AI, or all loaded actors without a selection",
    },
    Entry {
        name: "showanim",
        args: "",
        what: "selected actor's live graph, clips and clocks",
    },
    Entry {
        name: "sa",
        args: "",
        what: "ShowAnim",
    },
    Entry {
        name: "showscenegraph",
        args: "",
        what: "selected reference's NIF tree, controllers and collision",
    },
    Entry {
        name: "ssg",
        args: "",
        what: "ShowScenegraph",
    },
    Entry {
        name: "sfsg",
        args: "",
        what: "ShowScenegraph",
    },
    Entry {
        name: "shownodes",
        args: "All|Off",
        what: "selected reference's scene tree or close the report",
    },
    Entry {
        name: "togglepapyruslog",
        args: "",
        what: "toggle VM log delivery to console and Papyrus.0.log",
    },
    Entry {
        name: "tplog",
        args: "",
        what: "TogglePapyrusLog",
    },
    Entry {
        name: "printnpcdialog",
        args: "",
        what: "candidate topics, eligibility and every CTDA result",
    },
    Entry {
        name: "pdialog",
        args: "",
        what: "PrintNPCDialog",
    },
    Entry {
        name: "calcpathtopoint",
        args: "<x> <y> <z>",
        what: "query NAVM without moving the selected actor or player",
    },
    Entry {
        name: "p2p",
        args: "<x> <y> <z>",
        what: "CalcPathToPoint",
    },
    Entry {
        name: "showpivot",
        args: "",
        what: "toggle a yellow cross at the selected reference",
    },
    Entry {
        name: "sp",
        args: "",
        what: "ShowPivot",
    },
    Entry {
        name: "visualrefposition",
        args: "",
        what: "toggle the selected reference's position axes",
    },
    Entry {
        name: "vrp",
        args: "",
        what: "VisualRefPosition",
    },
    Entry {
        name: "showwhodetectsplayer",
        args: "",
        what: "actors currently detecting the player",
    },
    Entry {
        name: "swdp",
        args: "",
        what: "ShowWhoDetectsPlayer",
    },
    Entry {
        name: "testseendata",
        args: "",
        what: "current observer-specific detection and loaded-reference state",
    },
    Entry {
        name: "tsd",
        args: "",
        what: "TestSeenData",
    },
    Entry {
        name: "getplayername",
        args: "",
        what: "read the player's record-backed name",
    },
    Entry {
        name: "checkmemory",
        args: "",
        what: "Rust live bytes and allocation calls by system",
    },
    Entry {
        name: "chkmem",
        args: "",
        what: "CheckMemory",
    },
    Entry {
        name: "toggledebugtext",
        args: "",
        what: "toggle actor AI labels in the 3D view",
    },
    Entry {
        name: "tdt",
        args: "",
        what: "ToggleDebugText",
    },
    Entry {
        name: "togglemapcam",
        args: "",
        what: "toggle a top-down orthographic debug camera",
    },
    Entry {
        name: "tmc",
        args: "",
        what: "ToggleMapCam",
    },
    Entry {
        name: "togglehdrdebug",
        args: "",
        what: "toggle the host HDR luminance buffer",
    },
    Entry {
        name: "thd",
        args: "",
        what: "ToggleHDRDebug",
    },
    Entry {
        name: "testlocalmap",
        args: "[0]",
        what: "inspect loaded NAVM from the map camera, without fog of war",
    },
    Entry {
        name: "tlm",
        args: "[0]",
        what: "TestLocalMap",
    },
    Entry {
        name: "toggleverletdebug",
        args: "",
        what: "unavailable: no Verlet cape solver, host uses Jolt",
    },
    Entry {
        name: "tvd",
        args: "",
        what: "ToggleVerletDebug",
    },
    Entry {
        name: "togglemultiboundcheck",
        args: "",
        what: "unavailable: no multibound culling checks, host uses Godot",
    },
    Entry {
        name: "mbc",
        args: "",
        what: "ToggleMultiboundCheck",
    },
];

pub(crate) fn run(
    console: &Console,
    game: &mut Game,
    target: Option<&str>,
    name: &str,
    args: &[&str],
) -> Option<Result<Outcome, String>> {
    if !COMMANDS.iter().any(|command| command.name == name) {
        return None;
    }
    Some(dispatch(console, game, target, name, args))
}

fn dispatch(
    console: &Console,
    game: &mut Game,
    target: Option<&str>,
    name: &str,
    args: &[&str],
) -> Result<Outcome, String> {
    if !matches!(
        name,
        "calcpathtopoint" | "p2p" | "testlocalmap" | "tlm" | "shownodes"
    ) && !args.is_empty()
    {
        return Err(format!("{name}: takes no arguments"));
    }
    match name {
        "togglenavmesh" | "tnm" => {
            game.debug_views.navmesh = !game.debug_views.navmesh;
            Ok(Outcome::Bool(game.debug_views.navmesh))
        }
        "togglepathline" | "tpl" => {
            game.debug_views.paths = !game.debug_views.paths;
            Ok(Outcome::Bool(game.debug_views.paths))
        }
        "printailist" | "pai" => Ok(Outcome::List(ai_list(&mut game.world_mut(), None))),
        "showailist" => {
            let selected = if target.is_some() || console.selected().is_some() {
                Some(console.target_of(game, target)?)
            } else {
                None
            };
            Ok(Outcome::List(ai_list(&mut game.world_mut(), selected)))
        }
        "showanim" | "sa" => {
            let actor = console.target_of(game, target)?;
            if game.world().actor_ref(actor).is_none() {
                return Err(format!("{actor}: no loaded actor"));
            }
            Ok(request(game, HostRequest::Animation(actor)))
        }
        "showscenegraph" | "ssg" | "sfsg" => {
            let reference = console.target_of(game, target)?;
            Ok(request(game, HostRequest::Scenegraph(reference)))
        }
        "shownodes" => match args {
            [off] if off.eq_ignore_ascii_case("off") => Ok(Outcome::Done),
            [all] if all.eq_ignore_ascii_case("all") => {
                let reference = console.target_of(game, target)?;
                Ok(request(game, HostRequest::Scenegraph(reference)))
            }
            _ => Err("ShowNodes: expected All or Off".into()),
        },
        "togglepapyruslog" | "tplog" => toggle_papyrus(game),
        "printnpcdialog" | "pdialog" => {
            let actor = console.target_of(game, target)?;
            npc_dialogue(game, actor).map(Outcome::List)
        }
        "calcpathtopoint" | "p2p" => query_path(console, game, target, args),
        _ => dispatch_other(console, game, target, name, args),
    }
}

fn dispatch_other(
    console: &Console,
    game: &mut Game,
    target: Option<&str>,
    name: &str,
    args: &[&str],
) -> Result<Outcome, String> {
    match name {
        "showpivot" | "sp" | "visualrefposition" | "vrp" => {
            let reference = console.target_of(game, target)?;
            if game.world().ref_placement(reference).is_none() {
                return Err(format!("{reference}: no placed reference"));
            }
            game.debug_views.pivot =
                (game.debug_views.pivot != Some(reference)).then_some(reference);
            Ok(Outcome::Bool(game.debug_views.pivot.is_some()))
        }
        "showwhodetectsplayer" | "swdp" => Ok(seen_data(game, true)),
        "testseendata" | "tsd" => Ok(seen_data(game, false)),
        "getplayername" => {
            let player = game.player();
            game.world()
                .name_of(player)
                .map(Outcome::Text)
                .ok_or_else(|| "player has no name".into())
        }
        "checkmemory" | "chkmem" => {
            let snapshot = soe_trace::MemorySnapshot::now();
            Ok(Outcome::List(
                soe_trace::System::ALL
                    .iter()
                    .map(|system| {
                        let index = *system as usize;
                        format!(
                            "{} Rust live_bytes={} allocation_calls={} allocated_bytes={}",
                            system.name(),
                            snapshot.live_bytes[index],
                            snapshot.allocation_calls[index],
                            snapshot.allocated_bytes[index]
                        )
                    })
                    .collect(),
            ))
        }
        "toggledebugtext" | "tdt" => {
            game.debug_views.ai_text = !game.debug_views.ai_text;
            Ok(Outcome::Bool(game.debug_views.ai_text))
        }
        "togglemapcam" | "tmc" => Ok(request(game, HostRequest::MapCamera)),
        "togglehdrdebug" | "thd" => Ok(request(game, HostRequest::Hdr)),
        "testlocalmap" | "tlm" => {
            if !matches!(args, [] | ["0"]) {
                return Err(
                    "TestLocalMap: only 0 is supported, local-map fog of war has no runtime state"
                        .into(),
                );
            }
            Ok(request(game, HostRequest::LocalMap))
        }
        "toggleverletdebug" | "tvd" => Err("no Verlet cape solver, host uses Jolt".into()),
        "togglemultiboundcheck" | "mbc" => {
            Err("no multibound culling checks, host uses Godot".into())
        }
        _ => Err(format!("{name}: missing debug view handler")),
    }
}

fn request(game: &mut Game, request: HostRequest) -> Outcome {
    game.debug_views.host_requests.push_back(request);
    Outcome::Text(format!("host diagnostic requested: {request:?}"))
}

fn toggle_papyrus(game: &mut Game) -> Result<Outcome, String> {
    let enabled = !game.debug_views.papyrus;
    if enabled {
        let directory = game
            .debug_views
            .papyrus_directory
            .clone()
            .unwrap_or_else(|| PathBuf::from("logs/debug-views"));
        let logs = crate::papyrus_logs::FileLogs::open(&directory, true, true)
            .map_err(|error| format!("Papyrus log {}: {error}", directory.display()))?;
        game.debug_views.previous_papyrus_logs = game.papyrus_logs.replace(logs);
        game.papyrus_log_cursor = game.vm.log.entries.len();
        game.debug_views.papyrus_directory = Some(directory);
        game.debug_views.papyrus_cursor = game.vm.log.entries.len();
    } else {
        game.papyrus_logs = game.debug_views.previous_papyrus_logs.take();
        game.papyrus_log_cursor = game.vm.log.entries.len();
    }
    game.debug_views.papyrus = enabled;
    Ok(Outcome::Bool(enabled))
}

/// New VM entries for the console overlay. Turning the overlay off stops copying.
pub fn papyrus_lines(game: &mut Game) -> Vec<String> {
    if !game.debug_views.papyrus {
        return Vec::new();
    }
    let cursor = game
        .debug_views
        .papyrus_cursor
        .min(game.vm.log.entries.len());
    let lines = game.vm.log.entries[cursor..]
        .iter()
        .map(|entry| format!("[Papyrus {:.3}] {}", entry.time, entry.message))
        .collect();
    game.debug_views.papyrus_cursor = game.vm.log.entries.len();
    lines
}

/// Actual route shape and recent query outcome. One waypoint is called out even
/// when it is valid because the actor and destination share a triangle.
pub fn path_state(world: &World, actor: GlobalFormId) -> &'static str {
    let Some(state) = world.actor_ref(actor) else {
        return "unloaded";
    };
    let ai = &state.ai;
    if let Some(query) = world.ai.debug_query(actor) {
        return query.state;
    }
    if ai.path.len() == 1 {
        return "one point";
    }
    if let Some(last) = ai.path.last() {
        return if ai
            .destination
            .is_some_and(|goal| crate::ai::distance(*last, goal) > 4.0)
        {
            "partial"
        } else {
            "found"
        };
    }
    if let Some(metrics) = world.ai.path_metrics().get(&actor) {
        if metrics.requests > metrics.completed + metrics.cancellations {
            return "pending";
        }
        if metrics
            .samples
            .back()
            .is_some_and(|sample| sample.failed && !sample.cancelled)
        {
            return "failed";
        }
    }
    if ai.arrived {
        "arrived"
    } else {
        "idle"
    }
}

/// Loaded actor driver rows in resolved FormID order, without constructing actors.
pub fn ai_list(world: &mut World, selected: Option<GlobalFormId>) -> Vec<String> {
    world.loaded_actors().into_iter().filter(|actor| selected.is_none_or(|selected| selected == *actor)).filter_map(|actor| {
        let target = crate::ai::debug_target(world, actor);
        let package_destination = crate::ai::debug_destination(world, actor).map(|place| place.position);
        let state = world.actor_ref(actor)?;
        let reference = world.ref_placement(actor)?;
        Some(format!("{actor} {} package={:?} procedure={:?} target={target:?} destination={:?} package_destination={:?} path={} waypoints={} position={:?} speed={} arrived={}",
            world.editor_id(actor).unwrap_or_default(), state.ai.package, state.ai.procedure, state.ai.destination, package_destination.or(state.ai.center).or(state.ai.destination), path_state(world, actor), state.ai.path.len(), reference.position, state.ai.speed, state.ai.arrived))
    }).collect()
}

fn seen_data(game: &mut Game, detecting_only: bool) -> Outcome {
    let player = game.player();
    let world = game.world();
    let lines = world
        .loaded_actors()
        .into_iter()
        .filter(|actor| *actor != player)
        .filter_map(|actor| {
            let detected = world.is_actor_detected_by(player, actor).unwrap_or(false);
            (!detecting_only || detected).then(|| {
                format!(
                    "{actor} detected_player={detected} awareness={:?} 3d_loaded={}",
                    world.awareness(actor, player),
                    world.is_reference_3d_loaded(actor)
                )
            })
        })
        .collect();
    Outcome::List(lines)
}

fn query_path(
    console: &Console,
    game: &mut Game,
    target: Option<&str>,
    args: &[&str],
) -> Result<Outcome, String> {
    if args.len() != 3 {
        return Err("CalcPathToPoint: expected x y z in Skyrim units".into());
    }
    let mut goal = [0.0; 3];
    for (coordinate, word) in goal.iter_mut().zip(args) {
        *coordinate = word
            .parse::<f32>()
            .map_err(|_| format!("invalid coordinate {word}"))?;
        if !coordinate.is_finite() {
            return Err("path coordinates must be finite".into());
        }
    }
    let actor = if target.is_some() || console.selected().is_some() {
        console.target_of(game, target)?
    } else {
        game.player()
    };
    let placement = game.world().ref_placement(actor).ok_or("no placed actor")?;
    let cell = game
        .world()
        .reference_ref(actor)
        .and_then(|r| r.cell)
        .ok_or("actor has no cell")?;
    match crate::play::navigation_path(game, cell, placement.position, goal) {
        Ok(path) => Ok(Outcome::List(vec![
            format!(
                "{} points={} cost={} from={:?} target={goal:?}",
                if path.points.len() == 1 {
                    "one point"
                } else {
                    "found"
                },
                path.points.len(),
                path.cost,
                placement.position
            ),
            format!("waypoints={:?}", path.points),
        ])),
        Err(reason) => Ok(Outcome::Text(format!("failed: {reason}"))),
    }
}
