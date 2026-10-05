// SPDX-License-Identifier: GPL-3.0-or-later

//! PRF-22 session transport over PLY-01 inputs and the existing SOEG snapshot.
//! Seeded replay follows the local Godot random-number manual. The repository's
//! VM Reader/Writer provides the encoding, rather than a second save reader.

use std::path::Path;

use soe_esm::GlobalFormId;
use soe_papyrus::vm::state::{Reader, Writer};
use soe_papyrus::VmError;

use super::{Command, Confirmation, Driver, Event};
use crate::combat::Hand;
use crate::determinism::{self, TickHash};
use crate::Game;

/// Fixed simulation frames between state comparisons.
pub const HASH_INTERVAL: u64 = 60;

/// A portable diagnostic recording. Definitions are supplied by the replaying
/// Game and must match the snapshot's plugin and script provenance.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    seed: u64,
    initial: Vec<u8>,
    residency: Vec<u8>,
    bindings: Vec<u8>,
    commands: Vec<Command>,
    outcomes: Vec<Result<String, String>>,
    hashes: Vec<TickHash>,
}

/// Recording, transport or deterministic replay failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("session requires an empty queue and no active capture")]
    Busy,
    #[error("no session recording is active")]
    Inactive,
    #[error("invalid session: {0}")]
    Format(String),
    #[error("session state: {0}")]
    State(#[from] VmError),
    #[error("session input: {0}")]
    Input(#[from] super::Error),
    #[error("session file: {0}")]
    Io(#[from] std::io::Error),
    #[error("session outcome diverged at frame {0}")]
    Outcome(u64),
    #[error("session {0}")]
    Divergence(determinism::Divergence),
}

#[derive(Debug)]
pub(super) enum Capture {
    Record(Session),
    Replay(Session),
}

fn hash(game: &Game, frame: u64) -> TickHash {
    let mut hash = game.state_hash();
    hash.tick = frame;
    hash
}

fn compare(expected: &TickHash, actual: &TickHash) -> Result<(), Error> {
    determinism::compare(std::slice::from_ref(expected), std::slice::from_ref(actual))
        .map_err(Error::Divergence)
}

impl Session {
    /// The seed applied to every gameplay random stream at recording start.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// Executed commands in fixed-step order, including waits and rejections.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// Initial state, each 60-frame boundary and the final partial interval.
    /// Tick numbers in this log are session frame numbers, beginning at zero.
    pub fn hashes(&self) -> &[TickHash] {
        &self.hashes
    }

    /// Serialize the version-one SOER envelope with a CRC32 of its payload.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut body = Writer::new();
        body.u64(self.seed);
        body.bytes(&self.initial);
        body.bytes(&self.residency);
        body.bytes(&self.bindings);
        body.len(self.commands.len());
        for (command, outcome) in self.commands.iter().zip(&self.outcomes) {
            write_command(&mut body, command);
            body.bool(outcome.is_ok());
            body.str(match outcome {
                Ok(value) | Err(value) => value,
            });
        }
        body.str(&determinism::to_text(&self.hashes));
        let mut out = Writer::new();
        out.out.extend_from_slice(b"SOER");
        out.u16(1);
        out.u32(crc32fast::hash(&body.out));
        out.bytes(&body.out);
        out.out
    }

    /// Decode and validate a recording without changing gameplay state.
    ///
    /// # Errors
    /// Rejects corruption, truncation, unknown commands, non-finite transforms,
    /// trailing data and missing or unordered checkpoints.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let mut envelope = Reader::new(bytes);
        if envelope.take(4)? != b"SOER" || envelope.u16()? != 1 {
            return Err(Error::Format("unsupported magic or version".into()));
        }
        let checksum = envelope.u32()?;
        let payload = envelope.bytes()?;
        envelope.expect_end()?;
        if crc32fast::hash(payload) != checksum {
            return Err(Error::Format("checksum mismatch".into()));
        }
        let mut reader = Reader::new(payload);
        let seed = reader.u64()?;
        let initial = reader.bytes()?.to_vec();
        let residency = reader.bytes()?.to_vec();
        let bindings = reader.bytes()?.to_vec();
        let count = reader.len()?;
        if count > reader.remaining() {
            return Err(Error::Format("input count exceeds payload".into()));
        }
        let mut commands = Vec::new();
        let mut outcomes = Vec::new();
        for _ in 0..count {
            let command = read_command(&mut reader)?;
            Driver::default().push(command.clone())?;
            commands.push(command);
            let accepted = reader.bool()?;
            let message = reader.str()?;
            outcomes.push(if accepted { Ok(message) } else { Err(message) });
        }
        let text = reader.str()?;
        let hashes = determinism::from_text(&text);
        reader.expect_end()?;
        let frames = commands.len() as u64;
        let mut expected: Vec<_> = (0..=frames / HASH_INTERVAL)
            .map(|index| index * HASH_INTERVAL)
            .collect();
        if !frames.is_multiple_of(HASH_INTERVAL) {
            expected.push(frames);
        }
        if determinism::to_text(&hashes) != text
            || hashes.iter().map(|hash| hash.tick).collect::<Vec<_>>() != expected
            || hashes.iter().any(|hash| hash.parts.is_empty())
        {
            return Err(Error::Format("invalid checkpoint log".into()));
        }
        Ok(Self {
            seed,
            initial,
            residency,
            bindings,
            commands,
            outcomes,
            hashes,
        })
    }

    /// Write a complete recording to a caller-selected path.
    ///
    /// # Errors
    /// Returns the filesystem write error.
    pub fn write(&self, path: &Path) -> Result<(), Error> {
        std::fs::write(path, self.to_bytes())?;
        Ok(())
    }

    /// Read and validate a complete recording.
    ///
    /// # Errors
    /// Returns file or recording validation errors.
    pub fn read(path: &Path) -> Result<Self, Error> {
        Self::from_bytes(&std::fs::read(path)?)
    }
}

impl Driver {
    /// Start a recording at the current state and reset its gameplay RNG streams.
    /// Previously completed events are cleared. Pending inputs are rejected.
    ///
    /// # Errors
    /// Returns an active-capture, pending-input or snapshot serialization error.
    pub fn start_recording(&mut self, game: &mut Game, seed: u64) -> Result<(), Error> {
        if self.session.is_some() || !self.queue.is_empty() || self.active_package.is_some() {
            return Err(Error::Busy);
        }
        game.world_mut().reset_random_seed(seed);
        let initial = game.save_state()?;
        self.events.clear();
        self.session_error = None;
        self.session = Some(Capture::Record(Session {
            seed,
            initial,
            residency: game.world().record_session_residency(),
            bindings: record_bindings(game),
            commands: Vec::new(),
            outcomes: Vec::new(),
            hashes: vec![hash(game, 0)],
        }));
        Ok(())
    }

    /// Finish recording, including a hash of a final partial interval.
    ///
    /// # Errors
    /// Rejects a missing recording or unexecuted queued commands.
    pub fn finish_recording(&mut self, game: &Game) -> Result<Session, Error> {
        if !self.queue.is_empty() || self.active_package.is_some() {
            return Err(Error::Busy);
        }
        if !matches!(self.session, Some(Capture::Record(_))) {
            return Err(Error::Inactive);
        }
        let Some(Capture::Record(mut session)) = self.session.take() else {
            return Err(Error::Inactive);
        };
        let frame = session.commands.len() as u64;
        if session
            .hashes
            .last()
            .is_some_and(|value| value.tick == frame)
        {
            session.hashes.pop();
        }
        session.hashes.push(hash(game, frame));
        Ok(session)
    }

    /// Restore the initial snapshot and queue its recorded inputs. The caller
    /// loads the same definitions first. Each subsequent step verifies outcomes
    /// and scheduled hashes. A divergence stops further input execution.
    ///
    /// # Errors
    /// Rejects busy drivers, changed definitions, invalid state or initial hashes.
    pub fn start_replay(&mut self, game: &mut Game, session: &Session) -> Result<(), Error> {
        if self.session.is_some() || !self.queue.is_empty() || self.active_package.is_some() {
            return Err(Error::Busy);
        }
        if Game::saved_provenance(&session.initial)? != Game::saved_provenance(&game.save_state()?)?
        {
            return Err(Error::Format(
                "replay definitions differ from recording".into(),
            ));
        }
        game.validate_saved_state(&session.initial)?;
        game.load_state(&session.initial)?;
        game.world_mut()
            .restore_session_residency(&session.residency)?;
        game.world_mut().reset_random_seed(session.seed);
        restore_bindings(game, &session.bindings)?;
        let expected = session
            .hashes
            .first()
            .ok_or_else(|| Error::Format("no initial hash".into()))?;
        compare(expected, &hash(game, 0))?;
        self.events.clear();
        self.session_error = None;
        self.queue.extend(session.commands.iter().cloned());
        if !session.commands.is_empty() {
            self.session = Some(Capture::Replay(session.clone()));
        }
        Ok(())
    }

    /// A replay failure, retained until a new capture or driver reset.
    pub fn replay_error(&self) -> Option<&Error> {
        self.session_error.as_ref()
    }

    pub(super) fn capture_step(&mut self, game: &Game, event: &Event) {
        let frame = self.events.len() as u64;
        match self.session.as_mut() {
            Some(Capture::Record(session)) => {
                session.commands.push(event.command.clone());
                session.outcomes.push(event.result.clone());
                if frame.is_multiple_of(HASH_INTERVAL) {
                    session.hashes.push(hash(game, frame));
                }
            }
            Some(Capture::Replay(session)) => {
                let result = if session.outcomes.get((frame - 1) as usize) != Some(&event.result) {
                    Err(Error::Outcome(frame))
                } else if let Some(expected) = session.hashes.iter().find(|hash| hash.tick == frame)
                {
                    compare(expected, &hash(game, frame))
                } else {
                    Ok(())
                };
                if let Err(error) = result {
                    self.session_error = Some(error);
                }
                if self.session_error.is_some() || self.queue.is_empty() {
                    self.session = None;
                    self.queue.clear();
                }
            }
            None => {}
        }
    }
}

fn write_form(writer: &mut Writer, id: GlobalFormId) {
    writer.u16(id.file);
    writer.u32(id.object);
}

fn record_bindings(game: &Game) -> Vec<u8> {
    let mut writer = Writer::new();
    for forms in [
        &game.bound_forms,
        &game.bound_cells,
        &game.greeting_range_actors,
    ] {
        writer.len(forms.len());
        for form in forms {
            write_form(&mut writer, *form);
        }
    }
    writer.len(game.active_hand_casts.len());
    for (actor, hand) in &game.active_hand_casts {
        write_form(&mut writer, *actor);
        writer.u8(match hand {
            Hand::Left => 0,
            Hand::Right => 1,
        });
    }
    writer.len(game.mcm_registered.len());
    for form in &game.mcm_registered {
        write_form(&mut writer, *form);
    }
    writer.bool(game.mcm_active.is_some());
    if let Some(form) = game.mcm_active {
        write_form(&mut writer, form);
    }
    writer.bool(game.bound.is_some());
    if let Some(report) = &game.bound {
        writer.len(report.forms_with_scripts);
        writer.len(report.scripts_attached);
        writer.len(report.properties_set);
        writer.len(report.quests_started);
        writer.len(report.scripts_missing.len());
        for (script, form) in &report.scripts_missing {
            writer.str(script);
            write_form(&mut writer, *form);
        }
        for list in [&report.properties_missing, &report.properties_read_only] {
            writer.len(list.len());
            for (script, property) in list {
                writer.str(script);
                writer.str(property);
            }
        }
    }
    let mut calls: Vec<_> = game.vm.natives.calls.iter().collect();
    calls.sort_by(|a, b| a.0.cmp(b.0));
    writer.len(calls.len());
    for (name, count) in calls {
        writer.str(name);
        writer.u64(*count);
    }
    writer.out
}

fn restore_bindings(game: &mut Game, bytes: &[u8]) -> Result<(), VmError> {
    let mut reader = Reader::new(bytes);
    let mut sets = Vec::new();
    for _ in 0..3 {
        let mut ids = std::collections::BTreeSet::new();
        for _ in 0..reader.len()? {
            ids.insert(read_form(&mut reader)?);
        }
        sets.push(ids);
    }
    let mut hands = std::collections::BTreeSet::new();
    for _ in 0..reader.len()? {
        let actor = read_form(&mut reader)?;
        let hand = match reader.u8()? {
            0 => Hand::Left,
            1 => Hand::Right,
            _ => return Err(VmError::State("invalid session casting hand".into())),
        };
        hands.insert((actor, hand));
    }
    let mut mcm_registered = Vec::new();
    for _ in 0..reader.len()? {
        mcm_registered.push(read_form(&mut reader)?);
    }
    let mcm_active = if reader.bool()? {
        Some(read_form(&mut reader)?)
    } else {
        None
    };
    let bound = if reader.bool()? {
        let mut report = crate::BindReport {
            forms_with_scripts: reader.len()?,
            scripts_attached: reader.len()?,
            properties_set: reader.len()?,
            quests_started: reader.len()?,
            ..crate::BindReport::default()
        };
        for _ in 0..reader.len()? {
            report
                .scripts_missing
                .push((reader.str()?, read_form(&mut reader)?));
        }
        for list in [
            &mut report.properties_missing,
            &mut report.properties_read_only,
        ] {
            for _ in 0..reader.len()? {
                list.push((reader.str()?, reader.str()?));
            }
        }
        Some(report)
    } else {
        None
    };
    let mut calls = std::collections::HashMap::new();
    for _ in 0..reader.len()? {
        let name = reader.str()?;
        let count = reader.u64()?;
        if calls.insert(name, count).is_some() {
            return Err(VmError::State("duplicate session native counter".into()));
        }
    }
    reader.expect_end()?;
    let mut sets = sets.into_iter();
    game.bound_forms = sets.next().unwrap_or_default();
    game.bound_cells = sets.next().unwrap_or_default();
    game.greeting_range_actors = sets.next().unwrap_or_default();
    game.active_hand_casts = hands;
    game.mcm_registered = mcm_registered;
    game.mcm_active = mcm_active;
    game.bound = bound;
    game.vm.natives.calls = calls;
    Ok(())
}

fn read_form(reader: &mut Reader<'_>) -> Result<GlobalFormId, VmError> {
    Ok(GlobalFormId {
        file: reader.u16()?,
        object: reader.u32()?,
    })
}

fn write_vector(writer: &mut Writer, vector: &[f32; 3]) {
    for value in vector {
        writer.f32(*value);
    }
}

fn read_vector(reader: &mut Reader<'_>) -> Result<[f32; 3], VmError> {
    Ok([reader.f32()?, reader.f32()?, reader.f32()?])
}

fn write_command(writer: &mut Writer, command: &Command) {
    match command {
        Command::Wait => writer.u8(0),
        Command::MoveTo {
            target,
            offset,
            match_rotation,
        } => {
            writer.u8(1);
            write_form(writer, *target);
            write_vector(writer, offset);
            writer.bool(*match_rotation);
        }
        Command::Turn { rotation } => {
            writer.u8(2);
            write_vector(writer, rotation);
        }
        Command::Activate { target } => {
            writer.u8(3);
            write_form(writer, *target);
        }
        Command::Attack {
            target,
            weapon,
            power,
        } => {
            writer.u8(4);
            write_form(writer, *target);
            write_form(writer, *weapon);
            writer.bool(*power);
        }
        Command::Cast { hand, target, held } => {
            writer.u8(5);
            writer.u8(match hand {
                Hand::Left => 0,
                Hand::Right => 1,
            });
            writer.bool(target.is_some());
            if let Some(target) = target {
                write_form(writer, *target);
            }
            writer.bool(*held);
        }
        Command::ChooseTopic { topic } => {
            writer.u8(6);
            write_form(writer, *topic);
        }
        Command::ConfirmMenu(confirmation) => {
            writer.u8(7);
            match confirmation {
                Confirmation::Dialogue => writer.u8(0),
                Confirmation::Gift => writer.u8(1),
                Confirmation::Craft { bench, recipe } => {
                    writer.u8(2);
                    write_form(writer, *bench);
                    write_form(writer, *recipe);
                }
                Confirmation::Alchemy { bench, ingredients } => {
                    writer.u8(3);
                    write_form(writer, *bench);
                    for ingredient in ingredients {
                        write_form(writer, *ingredient);
                    }
                }
            }
        }
        Command::RunPackage { package, duration } => {
            writer.u8(8);
            write_form(writer, *package);
            writer.bool(duration.is_some());
            if let Some(duration) = duration {
                writer.f64(*duration);
            }
        }
    }
}

fn read_command(reader: &mut Reader<'_>) -> Result<Command, Error> {
    Ok(match reader.u8()? {
        0 => Command::Wait,
        1 => Command::MoveTo {
            target: read_form(reader)?,
            offset: read_vector(reader)?,
            match_rotation: reader.bool()?,
        },
        2 => Command::Turn {
            rotation: read_vector(reader)?,
        },
        3 => Command::Activate {
            target: read_form(reader)?,
        },
        4 => Command::Attack {
            target: read_form(reader)?,
            weapon: read_form(reader)?,
            power: reader.bool()?,
        },
        5 => Command::Cast {
            hand: match reader.u8()? {
                0 => Hand::Left,
                1 => Hand::Right,
                value => return Err(Error::Format(format!("unknown hand {value}"))),
            },
            target: if reader.bool()? {
                Some(read_form(reader)?)
            } else {
                None
            },
            held: reader.bool()?,
        },
        6 => Command::ChooseTopic {
            topic: read_form(reader)?,
        },
        7 => Command::ConfirmMenu(match reader.u8()? {
            0 => Confirmation::Dialogue,
            1 => Confirmation::Gift,
            2 => Confirmation::Craft {
                bench: read_form(reader)?,
                recipe: read_form(reader)?,
            },
            3 => Confirmation::Alchemy {
                bench: read_form(reader)?,
                ingredients: [read_form(reader)?, read_form(reader)?],
            },
            value => return Err(Error::Format(format!("unknown menu {value}"))),
        }),
        8 => Command::RunPackage {
            package: read_form(reader)?,
            duration: if reader.bool()? {
                Some(reader.f64()?)
            } else {
                None
            },
        },
        value => return Err(Error::Format(format!("unknown command {value}"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prf_22_transport_rejects_unknown_commands_hands_and_menus() {
        for bytes in [&[255][..], &[5, 2][..], &[7, 3][..]] {
            assert!(matches!(
                read_command(&mut Reader::new(bytes)),
                Err(Error::Format(_))
            ));
        }
    }

    #[test]
    fn prf_22_transport_rejects_nonfinite_inputs_and_missing_checkpoint_logs() {
        let mut game = Game::new(soe_esm::LoadOrder::new());
        let mut driver = Driver::default();
        driver.start_recording(&mut game, 0).unwrap();
        let session = driver.finish_recording(&game).unwrap();
        let mut invalid = session.clone();
        invalid.hashes.clear();
        assert!(matches!(
            Session::from_bytes(&invalid.to_bytes()),
            Err(Error::Format(_))
        ));
        invalid = session.clone();
        invalid.hashes.push(session.hashes[0].clone());
        assert!(matches!(
            Session::from_bytes(&invalid.to_bytes()),
            Err(Error::Format(_))
        ));
        invalid = session;
        invalid.commands.push(Command::Turn {
            rotation: [f32::NAN, 0.0, 0.0],
        });
        invalid.outcomes.push(Err("rejected".into()));
        assert!(matches!(
            Session::from_bytes(&invalid.to_bytes()),
            Err(Error::Input(super::super::Error::Transform))
        ));
    }
}
