// SPDX-License-Identifier: GPL-3.0-or-later

//! Controller-distance fallback for player footsteps, pending animation events.

use godot::prelude::*;
use soe_game::footsteps::{Footsteps, Gait};
use soe_physics::{PhysicsWorld, Vec3};

use super::PlayerNode;
use crate::input::InputState;

impl PlayerNode {
    pub(super) fn start_footstep_audio(&mut self) {
        let Some(source) = self.data_source.clone() else {
            return;
        };
        if self.sound_bank.is_none() {
            let bank = crate::godot_audio::SoundBank::new_alloc();
            self.base_mut().add_child(&bank);
            self.sound_bank = Some(bank);
        }
        if let Some(bank) = &mut self.sound_bank {
            bank.bind_mut().use_source(source);
        }
        self.footstep_cadence.reset();
        if let Some(cell) = self.loaded_cell {
            self.sync_cell_ambience(cell);
        }
    }

    pub(super) fn sync_cell_ambience(&mut self, cell: soe_esm::GlobalFormId) {
        let Some(bank) = &mut self.sound_bank else {
            return;
        };
        let tracks = self
            .game
            .as_ref()
            .and_then(|game| soe_game::ambience::tracks_for_cell(&mut game.world_mut(), cell).ok());
        let track = tracks.and_then(|tracks| tracks.into_iter().next());
        if !self.ambience_owner.set(track.as_ref()) {
            return;
        }
        let mut bank = bank.bind_mut();
        bank.stop_ambience();
        if let Some(track) = track {
            if !bank.play_ambience(track.path.as_str().into()) {
                soe_trace::error!(target: "host::footsteps", system = "host::footsteps", "cell ambience {}: {}", track.path, bank.last_error());
            }
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
        let Some(source) = &self.data_source else {
            return;
        };
        let order = source.shared_order();
        let steps = Footsteps::new(&order);
        let event = match (gait, right) {
            (Gait::Sprint, false) => "FootSprintLeft",
            (Gait::Sprint, true) => "FootSprintRight",
            (_, false) => "FootLeft",
            (_, true) => "FootRight",
        };
        let selected = (|| -> soe_esm::Result<_> {
            let Some(game) = &mut self.game else {
                return Ok(Vec::new());
            };
            let player = game.player();
            let sets = game.world_mut().footstep_sets(player)?;
            let material = match hit.material {
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
        for impact in impacts {
            // Shipped footstep IPCTs pair NPC sound 1 with player sound 2.
            let Some(sound) = impact.sounds[1].or(impact.sounds[0]) else {
                continue;
            };
            let Some(game) = &mut self.game else { return };
            let player = game.player();
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
                player,
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
            let position =
                crate::units::SkyrimPos::new(hit.point.x, hit.point.y, hit.point.z).to_godot();
            if let Some(bank) = &mut self.sound_bank {
                if bank
                    .bind_mut()
                    .play_prepared(&plan, Vector3::new(position.x, position.y, position.z))
                    >= 0
                {
                    self.footstep_count += 1;
                    game.world_mut().create_detection_event(player, player, 20);
                } else {
                    self.footstep_errors += 1;
                    soe_trace::error!(target: "host::footsteps", system = "host::footsteps", "footstep playback failed");
                }
            }
        }
    }
}
