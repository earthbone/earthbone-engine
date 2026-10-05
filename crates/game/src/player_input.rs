// SPDX-License-Identifier: GPL-3.0-or-later

//! Fixed-step player commands over Game, shared by headless and Godot runs.
//! PLY-01 defines the schedule. CK Activate, MoveTo and OnContainerChanged
//! define the interaction contracts, documented in docs/game.md.

use std::collections::VecDeque;

use soe_esm::GlobalFormId;

use crate::combat::{Hand, MeleeHit};
use crate::Game;

pub mod session;

/// Simulation seconds advanced after each command, including a rejected command.
pub const STEP_SECONDS: f64 = 1.0 / 60.0;

/// A gameplay operation selected in a modal menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirmation {
    /// Finish the current dialogue response and run its end fragment.
    Dialogue,
    /// Close a latent gift menu and resume its caller.
    Gift,
    /// Commit a COBJ transaction at the supplied workstation.
    Craft {
        bench: GlobalFormId,
        recipe: GlobalFormId,
    },
}

/// One player action at the next fixed simulation step.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Place the player at a reference plus a world-space offset in Skyrim units.
    /// This is an explicit placement command, rather than a navigation request.
    MoveTo {
        target: GlobalFormId,
        offset: [f32; 3],
        match_rotation: bool,
    },
    /// Set world-relative XYZ angles in radians, with Z as clockwise yaw.
    Turn { rotation: [f32; 3] },
    /// Default activation with OnActivate, including pickups and actor dialogue.
    Activate { target: GlobalFormId },
    /// Execute an authored melee contact against a supplied target.
    /// The weapon must pass the same equipment checks as a host contact.
    Attack {
        target: GlobalFormId,
        weapon: GlobalFormId,
        power: bool,
    },
    /// Press or release the player's equipped casting hand.
    Cast {
        hand: Hand,
        target: Option<GlobalFormId>,
        held: bool,
    },
    /// Choose an offered DIAL topic in the current conversation.
    ChooseTopic { topic: GlobalFormId },
    /// Perform the selected gameplay menu operation.
    ConfirmMenu(Confirmation),
    /// Advance one fixed step with no new player action.
    Wait,
}

/// Invalid input or a failure reported by the shared gameplay executor.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("player input replay already owns the command queue")]
    Replay,
    #[error("player input contains a non-finite transform")]
    Transform,
    #[error("player input reference unavailable: {0:?}")]
    Reference(GlobalFormId),
    #[error("player input activation rejected: {0:?}")]
    Activation(GlobalFormId),
    #[error("player input dialogue: {0}")]
    Dialogue(#[from] crate::dialogue::Error),
    #[error("player input attack: {0}")]
    Combat(#[from] crate::combat::Error),
    #[error("player input cast: {0}")]
    Magic(#[from] crate::world::magic::MagicError),
    #[error("player input crafting: {0}")]
    Craft(#[from] crate::crafting::Error),
    #[error("player input gift menu is not open")]
    Menu,
}

/// A completed step, including gameplay results and the VM's new event traces.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub step: u64,
    pub command: Command,
    pub result: Result<String, String>,
    pub cell: Option<GlobalFormId>,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub traces: Vec<String>,
}

/// Transient command queue and ordered event log. World remains authoritative.
#[derive(Debug, Default)]
pub struct Driver {
    queue: VecDeque<Command>,
    events: Vec<Event>,
    session: Option<session::Capture>,
    session_error: Option<session::Error>,
}

impl Driver {
    /// Queue an action without changing gameplay state.
    ///
    /// # Errors
    /// Rejects non-finite positions or rotations before enqueueing.
    pub fn push(&mut self, command: Command) -> Result<(), Error> {
        if matches!(self.session, Some(session::Capture::Replay(_))) {
            return Err(Error::Replay);
        }
        let values = match &command {
            Command::MoveTo { offset, .. } => Some(offset),
            Command::Turn { rotation } => Some(rotation),
            _ => None,
        };
        if values.is_some_and(|values| values.iter().any(|value| !value.is_finite())) {
            return Err(Error::Transform);
        }
        self.queue.push_back(command);
        Ok(())
    }

    /// Commands still waiting for a simulation step.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// Completed steps in execution order.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Execute one queued action and advance Game by exactly STEP_SECONDS.
    /// An empty queue does not advance time. Gameplay failures are logged and
    /// consume their step, so a script can inspect and reproduce a rejection.
    pub fn step(&mut self, game: &mut Game) -> Option<Event> {
        if self.session_error.is_some() {
            return None;
        }
        let command = self.queue.pop_front()?;
        let trace_start = game.vm.log.entries.len();
        let result = game
            .apply_player_input(&command)
            .map_err(|error| error.to_string());
        game.tick(STEP_SECONDS);
        let player = game.player();
        let (cell, position, rotation) = game
            .world()
            .reference_ref(player)
            .map_or((None, [0.0; 3], [0.0; 3]), |reference| {
                (reference.cell, reference.position, reference.rotation)
            });
        let event = Event {
            step: self.events.len() as u64,
            command,
            result,
            cell,
            position,
            rotation,
            traces: game.vm.log.entries[trace_start..]
                .iter()
                .map(|entry| entry.message.clone())
                .collect(),
        };
        self.events.push(event.clone());
        self.capture_step(game, &event);
        Some(event)
    }
}

impl Game {
    /// Apply one player action through the existing form-backed gameplay APIs.
    /// Scheduling belongs to Driver. This call alone does not advance the clock.
    ///
    /// # Errors
    /// Reports unavailable references, invalid transforms or gameplay rejection.
    pub fn apply_player_input(&mut self, command: &Command) -> Result<String, Error> {
        let player = self.player();
        match command {
            Command::MoveTo {
                target,
                offset,
                match_rotation,
            } => {
                let (prior_cell, from) = self
                    .world_mut()
                    .reference(player)
                    .map(|reference| (reference.cell, reference.position))
                    .ok_or(Error::Reference(player))?;
                let (cell, to) = {
                    let mut world = self.world_mut();
                    let reference = world.reference(*target).ok_or(Error::Reference(*target))?;
                    if !reference.enabled || reference.deleted {
                        return Err(Error::Reference(*target));
                    }
                    let to = std::array::from_fn(|axis| reference.position[axis] + offset[axis]);
                    if !to.iter().all(|value: &f32| value.is_finite())
                        || (*match_rotation
                            && !reference.rotation.iter().all(|value| value.is_finite()))
                    {
                        return Err(Error::Transform);
                    }
                    world.move_to_with_rotation(player, *target, *offset, *match_rotation);
                    let reference = world.reference(player).ok_or(Error::Reference(player))?;
                    (reference.cell, to)
                };
                if prior_cell != cell {
                    self.change_cell(cell);
                }
                self.update_trigger_actor_position(player, cell, from, to);
                Ok(format!("moved to {target}"))
            }
            Command::Turn { rotation } => {
                let mut world = self.world_mut();
                let reference = world.reference(player).ok_or(Error::Reference(player))?;
                if !rotation.iter().all(|value| value.is_finite()) {
                    return Err(Error::Transform);
                }
                reference.rotation = *rotation;
                Ok("turned".into())
            }
            Command::Activate { target } => {
                let actor = {
                    let mut world = self.world_mut();
                    let reference = world.reference(*target).ok_or(Error::Reference(*target))?;
                    if !reference.enabled || reference.deleted {
                        return Err(Error::Reference(*target));
                    }
                    reference.is_actor
                };
                if !self.activate(*target, player) {
                    return Err(Error::Activation(*target));
                }
                if actor {
                    self.begin_dialogue(*target, player)?;
                    Ok(format!("conversation {target}"))
                } else {
                    Ok(format!("activated {target}"))
                }
            }
            Command::Attack {
                target,
                weapon,
                power,
            } => {
                let result = self.melee_hit(MeleeHit {
                    attacker: player,
                    target: *target,
                    weapon: *weapon,
                    is_bash: false,
                    is_power_attack: *power,
                })?;
                Ok(format!("hit {target}: {result:?}"))
            }
            Command::Cast { hand, target, held } => {
                let accepted = if *held {
                    self.begin_hand_cast(player, *hand, *target)?
                } else {
                    self.end_hand_cast(player, *hand)
                };
                Ok(format!("cast {hand:?} held={held} changed={accepted}"))
            }
            Command::ChooseTopic { topic } => {
                let line = self.choose_dialogue(*topic)?;
                Ok(format!("topic {topic}: {line:?}"))
            }
            Command::ConfirmMenu(confirmation) => match confirmation {
                Confirmation::Dialogue => {
                    let line = self.advance_dialogue()?;
                    Ok(format!("dialogue confirmed: {line:?}"))
                }
                Confirmation::Gift => {
                    if !self.close_gift_menu() {
                        return Err(Error::Menu);
                    }
                    Ok("gift confirmed".into())
                }
                Confirmation::Craft { bench, recipe } => {
                    let outcome = self.craft(player, *bench, *recipe)?;
                    Ok(format!("crafted: {outcome:?}"))
                }
            },
            Command::Wait => Ok("waited".into()),
        }
    }
}
