// SPDX-License-Identifier: GPL-3.0-or-later

//! Godot dictionary transport for the PLY-01 Game input driver.
//! Simulation and event ordering live in soe_game::player_input.

use godot::prelude::*;
use soe_esm::GlobalFormId;
use soe_game::combat::Hand;
use soe_game::player_input::{Command, Confirmation, Driver};

use super::PlayerNode;

fn field<T: FromGodot>(input: &VarDictionary, key: &str) -> Result<T, String> {
    input
        .get(key)
        .and_then(|value| value.try_to::<T>().ok())
        .ok_or_else(|| format!("missing or invalid player input field {key}"))
}

fn form(input: &VarDictionary, prefix: &str) -> Result<GlobalFormId, String> {
    let file = field::<i64>(input, &format!("{prefix}file"))?;
    let object = field::<i64>(input, &format!("{prefix}object"))?;
    Ok(GlobalFormId {
        file: u16::try_from(file).map_err(|_| "invalid player input file index")?,
        object: u32::try_from(object).map_err(|_| "invalid player input object id")?,
    })
}

fn vector(value: Vector3) -> [f32; 3] {
    [value.x, value.y, value.z]
}

fn decode(input: &VarDictionary) -> Result<Command, String> {
    match field::<GString>(input, "action")?.to_string().as_str() {
        "move_to" => Ok(Command::MoveTo {
            target: form(input, "")?,
            offset: vector(field(input, "offset")?),
            match_rotation: field(input, "match_rotation")?,
        }),
        "turn" => Ok(Command::Turn {
            rotation: vector(field(input, "rotation")?),
        }),
        "activate" => Ok(Command::Activate {
            target: form(input, "")?,
        }),
        "attack" => Ok(Command::Attack {
            target: form(input, "")?,
            weapon: form(input, "weapon_")?,
            power: field(input, "power")?,
        }),
        "cast" => {
            let hand = match field::<GString>(input, "hand")?.to_string().as_str() {
                "left" => Hand::Left,
                "right" => Hand::Right,
                _ => return Err("invalid player input casting hand".into()),
            };
            let target = if field::<bool>(input, "has_target")? {
                Some(form(input, "")?)
            } else {
                None
            };
            Ok(Command::Cast {
                hand,
                target,
                held: field(input, "held")?,
            })
        }
        "choose_topic" => Ok(Command::ChooseTopic {
            topic: form(input, "")?,
        }),
        "confirm_menu" => {
            let confirmation = match field::<GString>(input, "menu")?.to_string().as_str() {
                "dialogue" => Confirmation::Dialogue,
                "gift" => Confirmation::Gift,
                "craft" => Confirmation::Craft {
                    bench: form(input, "")?,
                    recipe: form(input, "recipe_")?,
                },
                _ => return Err("invalid player input menu".into()),
            };
            Ok(Command::ConfirmMenu(confirmation))
        }
        "wait" => Ok(Command::Wait),
        _ => Err("unknown player input action".into()),
    }
}

#[godot_api(secondary)]
impl PlayerNode {
    /// Queue a typed gameplay action. Invalid dictionaries never advance Game.
    #[allow(clippy::needless_pass_by_value)]
    #[func]
    pub fn queue_player_input(&mut self, input: VarDictionary) -> bool {
        if self.game.is_none() {
            return false;
        }
        match decode(&input).and_then(|command| {
            self.player_input
                .push(command)
                .map_err(|error| error.to_string())
        }) {
            Ok(()) => true,
            Err(error) => {
                godot_error!("PlayerNode: {error}");
                false
            }
        }
    }

    /// Execute one queued command with the shared fixed-step simulation.
    /// Returns false for an empty queue, gameplay rejection or host sync failure.
    #[func]
    pub fn step_player_input(&mut self) -> bool {
        let Some(game) = self.game.as_mut() else {
            return false;
        };
        let Some(event) = self.player_input.step(game) else {
            return false;
        };
        let synced = self.sync_game_pose();
        if let (Some(game), Some(streamer)) = (&mut self.game, self.game_streamer.as_mut()) {
            streamer
                .bind_mut()
                .sync_reference_changes(&mut game.world_mut());
        }
        if event.result.is_ok() {
            match &event.command {
                Command::Activate { target } => self.open_swing_door(*target),
                Command::ConfirmMenu(Confirmation::Gift) => {
                    if self
                        .menu
                        .title()
                        .is_some_and(|title| title.starts_with("Gift: "))
                    {
                        self.close_menu();
                    }
                }
                Command::ConfirmMenu(Confirmation::Craft { bench, .. }) => {
                    self.refresh_player_input_crafting(*bench);
                }
                _ => {}
            }
        }
        let dialogue = self
            .game
            .as_ref()
            .is_some_and(|game| game.world().dialogue_session().is_some());
        if !dialogue
            || matches!(
                event.command,
                Command::Activate { .. }
                    | Command::ChooseTopic { .. }
                    | Command::ConfirmMenu(Confirmation::Dialogue)
            )
        {
            self.present_player_input_dialogue();
        }
        event.result.is_ok() && synced
    }

    /// Ordered simulation events and Papyrus traces, in the headless log format.
    #[func]
    pub fn player_input_log(&self) -> PackedStringArray {
        self.player_input
            .events()
            .iter()
            .map(|event| GString::from(&format!("{event:?}")))
            .collect()
    }

    /// Number of commands waiting for their next fixed step.
    #[func]
    pub fn player_input_pending(&self) -> i64 {
        self.player_input.pending() as i64
    }

    /// Clear the transient command queue and log without changing World.
    #[func]
    pub fn clear_player_input(&mut self) {
        self.player_input = Driver::default();
    }
}
