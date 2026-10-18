// SPDX-License-Identifier: GPL-3.0-or-later

//! Authored actor foot events select FSTS impacts on the contacted material.

use godot::prelude::*;
use soe_game::footsteps::{Footsteps, Gait};
use soe_physics::{PhysicsWorld, Vec3};

use super::PlayerNode;

/// Skyrim units beyond which an NPC footstep is not played. Without a cut the
/// sound pool fills with steps nobody can hear and the player's own step is
/// refused.
const NPC_FOOTSTEP_RANGE: f32 = 1024.0;
use crate::input::InputState;

impl PlayerNode {
    pub(super) fn sync_actor_sounds(&mut self) {
        let Some(source) = self.data_source.as_ref().map(std::sync::Arc::clone) else {
            return;
        };
        if self.sound_bank.is_none() {
            return;
        }
        let requests: Vec<_> = if let Some(game) = &self.game {
            let world = game.world();
            let start = self
                .actor_sound_cursor
                .min(world.actor_sound_requests.len());
            self.actor_sound_cursor = world.actor_sound_requests.len();
            world.actor_sound_requests[start..]
                .iter()
                .filter_map(|request| {
                    Some((*request, world.reference_ref(request.actor)?.position))
                })
                .collect()
        } else if let Some(pair) = &self.simulation_pair {
            let requests = &pair.current.actor_sound_requests;
            let start = self.actor_sound_cursor.min(requests.len());
            self.actor_sound_cursor = requests.len();
            requests[start..]
                .iter()
                .filter_map(|request| {
                    Some((*request, pair.current.actors.get(&request.actor)?.position))
                })
                .collect()
        } else {
            return;
        };
        let order = source.shared_order();
        for (request, position) in requests {
            let actor = request.actor;
            let descriptor = request.descriptor;
            let plan = if let Some(game) = &self.game {
                let mut world = game.world_mut();
                soe_game::sound::prepare_for(
                    &order,
                    &mut world,
                    &game.vm,
                    descriptor,
                    actor,
                    None,
                    request.draws,
                    &soe_game::sound::SoundSettings::default(),
                )
            } else {
                soe_game::sound::prepare(
                    &order,
                    descriptor,
                    request.draws,
                    &soe_game::sound::SoundSettings::default(),
                )
            };
            match plan {
                Ok(plan) => {
                    let position =
                        crate::units::SkyrimPos::new(position[0], position[1], position[2])
                            .to_godot();
                    if let Some(bank) = self.sound_bank.as_mut() {
                        let slot = bank
                            .bind_mut()
                            .play_prepared(&plan, Vector3::new(position.x, position.y, position.z));
                        if slot >= 0 {
                            *self.actor_sound_starts.entry(actor).or_default() += 1;
                        }
                        soe_trace::info!(target: "host::audio", system = "audio", "ACTOR_SOUND actor={actor} descriptor={descriptor} path={} slot={slot}", plan.data_path);
                    }
                }
                Err(error) => {
                    soe_trace::error!(target: "host::audio", system = "audio", "actor sound {actor} {descriptor}: {error}");
                }
            }
        }
    }

    fn audio_cell(&self, cell: soe_esm::GlobalFormId) -> soe_esm::GlobalFormId {
        let Some(source) = self.data_source.as_ref() else {
            return cell;
        };
        let index = source.cell_index();
        let Some(world) = index.persistent_cell_world(cell) else {
            return cell;
        };
        let feet = self.feet();
        let coord = crate::streaming::CellCoord::containing(crate::units::SkyrimPos::new(
            feet.x, feet.y, feet.z,
        ));
        index.exterior_cell(world, coord.x, coord.y).unwrap_or(cell)
    }
    pub(super) fn start_footstep_audio(&mut self) {
        self.actor_sound_cursor = 0;
        self.actor_sound_starts.clear();
        let Some(source) = self.data_source.clone() else {
            return;
        };
        if self.sound_bank.is_none() {
            let mut bank = crate::godot_audio::SoundBank::new_alloc();
            bank.set_name("SoundBank");
            self.base_mut().add_child(&bank);
            self.sound_bank = Some(bank);
        }
        if let Some(bank) = &mut self.sound_bank {
            bank.bind_mut().use_source(source);
        }
        self.cell_music_type = None;
        self.footstep_cadence.reset();
        if let Some(cell) = self.loaded_cell {
            self.sync_cell_ambience(cell);
            self.sync_cell_music(cell);
        }
    }

    pub(super) fn sync_cell_ambience(&mut self, cell: soe_esm::GlobalFormId) {
        let cell = self.audio_cell(cell);
        let Some(bank) = &mut self.sound_bank else {
            return;
        };
        let Some(game) = &mut self.game else { return };
        let Some(source) = &self.data_source else {
            return;
        };
        let order = source.shared_order();
        let (sounds, player, space) = {
            let mut world = game.world_mut();
            let draw = world.random_float(0.0, 1.0).min(1.0 - f32::EPSILON);
            let weather = world
                .sky_frame(cell)
                .ok()
                .flatten()
                .and_then(|sky| world.record(sky.weather))
                .and_then(|record| record.find(soe_esm::Tag(*b"DATA")))
                .and_then(|data| data.get(11))
                .copied()
                .unwrap_or(1)
                & 15;
            let sounds = match soe_game::ambience::descriptors_for_cell(&world, cell, weather, draw)
            {
                Ok(sounds) => sounds,
                Err(error) => {
                    soe_trace::error!(target: "host::audio", system = "host::audio", "cell ambience: {error}");
                    return;
                }
            };
            let space = world
                .record(cell)
                .and_then(|record| soe_esm::records::cell::Cell::from_record(record, false).ok())
                .and_then(|record| record.acoustic_space)
                .and_then(|raw| world.resolve_from(cell, raw));
            (sounds, world.player(), space)
        };
        let count = sounds.len();
        let mut plans = Vec::new();
        let mut rejected = Vec::new();
        for sound in sounds {
            let mut world = game.world_mut();
            let draws = [
                world.random_float(0.0, 1.0),
                world.random_float(0.0, 1.0),
                world.random_float(0.0, 1.0),
            ];
            let plan = soe_game::sound::prepare_for(
                &order,
                &mut world,
                &game.vm,
                sound,
                player,
                None,
                draws,
                &soe_game::sound::SoundSettings::default(),
            );
            match plan {
                Ok(plan) => plans.push(plan),
                Err(error) => {
                    rejected.push(format!("{sound}: {error}"));
                }
            }
        }
        soe_trace::info!(target: "host::audio", system = "host::audio",
            "AMBIENCE cell={cell} descriptors={count} prepared={} acoustic_space={space:?} rejected={rejected:?}",
            plans.len()
        );
        bank.bind_mut().transition_ambience(space, &plans);
    }

    pub(super) fn sync_cell_music(&mut self, cell: soe_esm::GlobalFormId) {
        let cell = self.audio_cell(cell);
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let descriptor = match crate::audio::music_type_for_cell(&game.world(), cell) {
            Ok(descriptor) => descriptor,
            Err(error) => {
                soe_trace::error!(target: "host::audio", system = "host::audio", "cell music: {error}");
                return;
            }
        };
        let needs_selection = self
            .sound_bank
            .as_ref()
            .is_some_and(|bank| bank.bind().music_needs_selection());
        if descriptor == self.cell_music_type && !needs_selection {
            return;
        }
        self.cell_music_type = descriptor;
        let Some(descriptor) = descriptor else {
            if let Some(bank) = self.sound_bank.as_mut() {
                bank.bind_mut().stop_music();
            }
            return;
        };
        let (subject, draw) = {
            let mut world = game.world_mut();
            (world.player(), world.random_float(0.0, 1.0))
        };
        match soe_game::music::select(game, descriptor, subject, draw) {
            Ok(track) => {
                if let Some(bank) = self.sound_bank.as_mut() {
                    if !bank.bind_mut().play_selected_music(&track) {
                        soe_trace::error!(target: "host::audio", system = "host::audio",
                            "cell music playback failed: {}", bank.bind().last_error());
                    }
                }
            }
            Err(error) => soe_trace::error!(target: "host::audio", system = "host::audio",
                "cell music selection: {error}"),
        }
    }

    pub(super) fn advance_footsteps(
        &mut self,
        before: Vec3,
        was_grounded: bool,
        input: &InputState,
    ) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Audio);
        if self.game.is_none() || self.sound_bank.is_none() {
            return;
        }
        if self.player_appearance.is_some() {
            return;
        }
        let feet = self.feet();
        let grounded = was_grounded && self.character().is_some_and(|c| c.on_ground);
        let distance = (feet.x - before.x).hypot(feet.y - before.y);
        let (gait, stride) = if self.driver.sneaking {
            (Gait::Sneak, 55.0)
        } else if input.sprint {
            (Gait::Sprint, 130.0)
        } else if self.driver.always_run != input.walk {
            (Gait::Run, 90.0)
        } else {
            (Gait::Walk, 70.0)
        };
        let Some(right) =
            self.footstep_cadence
                .advance(distance, grounded && input.is_moving(), stride)
        else {
            return;
        };
        let hit = self
            .world
            .as_ref()
            .and_then(|world| world.raycast(feet + Vec3::Z * 8.0, feet - Vec3::Z * 40.0));
        let Some(hit) = hit.filter(|hit| hit.normal.z > 0.5) else {
            return;
        };
        let event = match (gait, right) {
            (Gait::Sprint, false) => "FootSprintLeft",
            (Gait::Sprint, true) => "FootSprintRight",
            (_, false) => "FootLeft",
            (_, true) => "FootRight",
        };
        let Some(player) = self.game.as_mut().map(soe_game::Game::player) else {
            return;
        };
        self.play_footstep_event(player, gait, event, hit.point, hit.material);
    }

    pub(super) fn play_animation_footsteps(&mut self) {
        let Some(player) = self.game.as_mut().map(soe_game::Game::player) else {
            return;
        };
        let mut events = Vec::new();
        if let Some(visual) = &mut self.player_appearance {
            events.extend(
                visual
                    .bind_mut()
                    .take_footstep_events()
                    .into_iter()
                    .map(|(tag, point)| (player, tag, point)),
            );
        }
        events.extend(self.npc_actors.take_footsteps());
        for (actor, event, point) in events {
            let Some(game) = &self.game else { return };
            let gait = game.world().actor_ref(actor).map_or(Gait::Walk, |state| {
                if state.swimming {
                    Gait::Swim
                } else if state.sneaking {
                    Gait::Sneak
                } else if state.sprinting {
                    Gait::Sprint
                } else if state.running {
                    Gait::Run
                } else {
                    Gait::Walk
                }
            });
            let point = crate::units::GodotPos::new(point.x, point.y, point.z).to_skyrim();
            let point = Vec3::new(point.x, point.y, point.z);
            if actor != player && (self.feet() - point).length() > NPC_FOOTSTEP_RANGE {
                continue;
            }
            let hit = self
                .world
                .as_ref()
                .and_then(|world| world.raycast(point + Vec3::Z * 32.0, point - Vec3::Z * 64.0));
            if let Some(hit) = hit.filter(|hit| hit.normal.z > 0.5) {
                self.play_footstep_event(actor, gait, &event, hit.point, hit.material);
            }
        }
    }

    fn play_footstep_event(
        &mut self,
        actor: soe_esm::GlobalFormId,
        gait: Gait,
        event: &str,
        point: Vec3,
        material: Option<u32>,
    ) {
        let _system = soe_trace::SystemScope::new(soe_trace::System::Audio);
        let Some(source) = &self.data_source else {
            return;
        };
        let order = source.shared_order();
        let steps = Footsteps::new(&order);
        let material = self
            .game
            .as_ref()
            .and_then(|game| {
                game.world()
                    .reference_ref(actor)
                    .and_then(|reference| reference.cell)
            })
            .and_then(|cell| source.water_material_at(cell, [point.x, point.y, point.z]))
            .or(material);
        let material_hash = i64::from(material.unwrap_or_default());
        let right = event.to_ascii_lowercase().contains("right");
        let selected = (|| -> soe_esm::Result<_> {
            let Some(game) = &mut self.game else {
                return Ok(Vec::new());
            };
            let sets = game.world_mut().footstep_sets(actor)?;
            let material = match material {
                Some(hash) => steps.material(hash)?,
                None => steps.default_object(*b"DLMT")?,
            };
            let Some(material) = material else {
                return Ok(Vec::new());
            };
            let mut impacts = Vec::new();
            for set in sets {
                impacts.extend(steps.impacts(set, gait, event, material)?);
            }
            Ok(impacts)
        })();
        let impacts = match selected {
            Ok(impacts) => impacts,
            Err(error) => {
                self.footstep_errors += 1;
                soe_trace::error!(target: "host::footsteps", system = "host::footsteps", "footstep records: {error}");
                return;
            }
        };
        let is_player = self
            .game
            .as_mut()
            .is_some_and(|game| game.player() == actor);
        if is_player && !impacts.is_empty() {
            self.signals()
                .footstep_requested()
                .emit(material_hash, right);
        }
        for impact in impacts {
            let Some(sound) = (if is_player {
                impact.sounds[1].or(impact.sounds[0])
            } else {
                impact.sounds[0]
            }) else {
                continue;
            };
            let Some(game) = &mut self.game else { return };
            let draws = {
                let mut world = game.world_mut();
                [
                    world.random_float(0.0, 1.0),
                    world.random_float(0.0, 1.0),
                    world.random_float(0.0, 1.0),
                ]
            };
            let mut world = game.world_mut();
            let plan = match soe_game::sound::prepare_for(
                &order,
                &mut world,
                &game.vm,
                sound,
                actor,
                None,
                draws,
                &soe_game::sound::SoundSettings::default(),
            ) {
                Ok(plan) => plan,
                Err(error) => {
                    self.footstep_errors += 1;
                    soe_trace::error!(target: "host::footsteps", system = "host::footsteps", "footstep sound: {error}");
                    continue;
                }
            };
            drop(world);
            let position = crate::units::SkyrimPos::new(point.x, point.y, point.z).to_godot();
            if let Some(bank) = &mut self.sound_bank {
                let slot = bank
                    .bind_mut()
                    .play_prepared(&plan, Vector3::new(position.x, position.y, position.z));
                if slot >= 0 {
                    if is_player {
                        self.footstep_count += 1;
                    }
                    game.world_mut().create_detection_event(actor, actor, 20);
                    if self.footstep_history.len() == 256 {
                        self.footstep_history.pop_front();
                    }
                    self.footstep_history.push_back(vdict! {
                        "actor" => actor.to_string().as_str(), "event" => event,
                        "gait" => format!("{gait:?}").as_str(),
                        "material_hash" => material_hash, "descriptor" => sound.to_string().as_str(),
                        "impact" => impact.impact.to_string().as_str(), "slot" => slot,
                        "position" => Vector3::new(position.x, position.y, position.z),
                    });
                    soe_trace::info!(target: "host::footsteps", system = "host::footsteps",
                        "FOOTSTEP actor={actor} event={event} gait={gait:?} material={material_hash} descriptor={sound} slot={slot}");
                    if is_player {
                        self.base_mut().emit_signal(
                            "footstep_played",
                            &[
                                material_hash.to_variant(),
                                slot.to_variant(),
                                right.to_variant(),
                            ],
                        );
                    }
                } else {
                    self.footstep_errors += 1;
                    let reason = self
                        .sound_bank
                        .as_ref()
                        .map(|bank| bank.bind().last_error().to_string())
                        .unwrap_or_default();
                    soe_trace::error!(target: "host::footsteps", system = "host::footsteps", "footstep playback failed for {sound}: {reason}");
                }
            }
        }
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Most recent accepted actor footsteps, with their authored descriptor and material.
    #[func]
    fn footstep_selections(&self) -> Array<VarDictionary> {
        self.footstep_history.iter().cloned().collect()
    }
}
