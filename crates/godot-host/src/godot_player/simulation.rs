// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-37 opt-in host adapter. Godot's local thread_safe_apis.rst keeps
//! scene objects on this thread. Only immutable data and plain commands cross
//! the boundary, and rendering never borrows the worker's Game.

use super::{Arc, PlayerNode};
use godot::prelude::*;
use soe_game::simulation::{Command, Config, PlayerPose, Simulation};
use soe_physics::PhysicsWorld;

impl PlayerNode {
    pub(super) fn advance_simulation_controller(&mut self, delta: f64) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Physics);
        if !delta.is_finite() || delta <= 0.0 {
            return;
        }
        let (dx, dy) = std::mem::take(&mut self.pending_look);
        self.rig.look(dx, dy);
        let input = self.gather();
        let intent = self.driver.update(&input, &self.rig);
        if let (Some(world), Some(character)) = (self.world.as_mut(), self.character) {
            world.set_crouch(character, intent.crouch);
            let (direction, speed, jump) = super::player_locomotion(intent, false);
            world.walk(character, direction, speed);
            if jump {
                world.try_jump(character);
            }
            world.step_character(character, delta as f32);
        }
        self.sync_npc_grounding(delta as f32);
        let feet = self.feet();
        if let Some(simulation) = &self.simulation {
            simulation.set_player_pose(PlayerPose {
                cell: self.loaded_cell,
                position: [feet.x, feet.y, feet.z],
                rotation: [-self.rig.pitch(), 0.0, self.rig.yaw()],
                sprinting: intent.sprinting,
            });
        }
        self.motion.step(feet, self.capsule_height());
        if !self.auto_step {
            self.update_camera();
        }
    }

    pub(super) fn present_simulation(&mut self) {
        let Some(simulation) = &self.simulation else {
            return;
        };
        if let Some(error) = simulation.failure() {
            godot_error!("Simulation: {error}");
            self.simulation_error = error;
            self.auto_step = false;
        }
        let previously_presented = self.simulation_pair.clone();
        if let Some(pair) = simulation.poll() {
            self.simulation_pair = Some(pair);
        }
        let Some(pair) = &self.simulation_pair else {
            return;
        };
        let now = std::time::Instant::now();
        let mut placements: std::collections::BTreeMap<_, _> = pair
            .current
            .actors
            .keys()
            .filter_map(|id| Some((*id, pair.placement(*id, now)?)))
            .collect();
        if let Some(previously_presented) = previously_presented {
            for (id, prior) in &previously_presented.current.actors {
                if !placements.contains_key(id) {
                    placements.insert(
                        *id,
                        soe_game::world::RefPlacement {
                            visible: false,
                            ..*prior
                        },
                    );
                }
            }
        }
        if let Some(streamer) = self.game_streamer.as_mut() {
            streamer.bind_mut().sync_snapshot_placements(&placements);
        }
    }

    pub(super) fn sync_simulation_bodies(&mut self) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Physics);
        let Some(pair) = &self.simulation_pair else {
            return;
        };
        let Some(world) = self.base().get_world_3d() else {
            return;
        };
        let now = std::time::Instant::now();
        let contacts: Vec<_> = pair
            .current
            .contacts
            .iter()
            .filter_map(|actor| {
                let mut placement = pair.placement(actor.actor, now)?;
                if let Some(character) = self
                    .world
                    .as_ref()
                    .and_then(|world| self.npc_grounding.character(world, actor.actor))
                {
                    placement.position = [
                        character.position.x,
                        character.position.y,
                        character.position.z,
                    ];
                }
                placement.visible.then_some(soe_game::world::ContactActor {
                    position: placement.position,
                    rotation: placement.rotation,
                    ..*actor
                })
            })
            .collect();
        self.actor_bodies.sync(world.get_space(), &contacts);
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// End the draft simulation before replacing host cell resources.
    #[func]
    pub fn stop_threaded_gameplay(&mut self) {
        self.simulation = None;
        self.simulation_pair = None;
    }

    /// Start the PRF-37 draft path. Initialization, binding and nav graph building
    /// happen on the simulation thread. The legacy Game must not already exist.
    #[func]
    pub fn start_threaded_gameplay(&mut self) -> bool {
        if self.simulation.is_some() {
            return true;
        }
        if self.game.is_some() {
            return false;
        }
        let (Some(source), Some(cell)) = (self.data_source.as_ref(), self.loaded_cell) else {
            return false;
        };
        let source = Arc::clone(source);
        let mut resident: Vec<_> = self
            .game_streamer
            .as_ref()
            .map(|streamer| streamer.bind().simulation_resident_cells())
            .unwrap_or_default()
            .into_iter()
            .filter(|id| *id != cell)
            .collect();
        if let Some(persistent) = source
            .exterior_world(cell)
            .and_then(|world| source.cell_index().world_persistent_cell(world))
        {
            if persistent != cell {
                resident.push(persistent);
                resident.sort_unstable();
                resident.dedup();
            }
        }
        let feet = self.feet();
        let position = [feet.x, feet.y, feet.z];
        let rotation = [-self.rig.pitch(), 0.0, self.rig.yaw()];
        let worker = Simulation::spawn(Config::default(), move || {
            let mut game = source.localized_game().map_err(|error| error.to_string())?;
            game.apply_mod_data(source.vfs().loose_roots());
            let scripts = game.load_scripts_from_vfs(source.vfs());
            if !scripts.scripts_failed.is_empty() {
                return Err(format!(
                    "{} scripts failed to load",
                    scripts.scripts_failed.len()
                ));
            }
            game.start();
            let player = game.player();
            if let Some(reference) = game.world_mut().reference(player) {
                reference.cell = Some(cell);
                reference.position = position;
                reference.rotation = rotation;
            }
            game.change_cell(Some(cell));
            game.attach_streaming_cells(&resident)
                .map_err(|error| error.to_string())?;
            game.run_until_idle(64);
            game.world_mut()
                .ai
                .set_planning_workers(
                    std::num::NonZeroUsize::new(4).unwrap_or(std::num::NonZeroUsize::MIN),
                )
                .map_err(|error| error.to_string())?;
            soe_game::ai::nav_graph(&mut game.world_mut());
            Ok(game)
        });
        match worker {
            Ok(worker) => {
                self.simulation = Some(worker);
                self.main_menu.leave();
                true
            }
            Err(error) => {
                self.simulation_error = error.to_string();
                false
            }
        }
    }

    /// Snapshot progress and independent simulation/worker durations. A missing
    /// first snapshot reports ready=false, rather than a healthy empty tick.
    #[func]
    pub fn simulation_status(&self) -> VarDictionary {
        let Some(pair) = &self.simulation_pair else {
            return vdict! { "ready" => false, "error" => self.simulation_error.as_str() };
        };
        vdict! {
            "ready" => true,
            "error" => self.simulation_error.as_str(),
            "ticks" => pair.current.sequence as i64,
            "seconds" => pair.current.seconds,
            "actors" => pair.current.contacts.len() as i64,
            "evaluations" => pair.current.evaluations as i64,
            "tick_ms" => pair.current.systems.frame_ns as f64 / 1e6,
            "worker_ms" => pair.current.worker_time.as_secs_f64() * 1000.0,
            "behind_seconds" => pair.lag(std::time::Instant::now()).as_secs_f64(),
        }
    }

    /// Queue an activation against the worker's authoritative world. Host menu
    /// and load-door result presentation are outside this draft adapter.
    #[func]
    pub fn simulation_activate(&self, file: i32, object: i64) -> bool {
        let (Ok(file), Ok(object)) = (u16::try_from(file), u32::try_from(object)) else {
            return false;
        };
        self.simulation.as_ref().is_some_and(|worker| {
            worker.command(Command::Activate(soe_esm::GlobalFormId { file, object }))
        })
    }
}
