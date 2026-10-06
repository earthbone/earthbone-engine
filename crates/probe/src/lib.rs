// SPDX-License-Identifier: GPL-3.0-or-later

//! Vanilla Papyrus log transport, following tes4skyrim's request/result pairing.
//! PEX and plugin output reuse the engine writers and the local Caprica/xEdit schemas.

mod animation;
mod compare;
mod runner;
mod schedule;
mod sequence;

pub use compare::{compare, parse_log, run, run_installed, Comparison, Difference, Observation};
pub use runner::{build, plugin, script};
pub use schedule::{parse_schedule_log, ScheduleDifference, ScheduleRow, ScheduleTable};
pub use sequence::{AnimationGraph, Form, FormKind, Menu, Probe, Step, Tolerances};

/// The bundled quest-stage and real-wait sequence.
pub const EXAMPLE: &str = include_str!("../probes/quest_wait.toml");

/// Failures preserve the transport, authoring or execution boundary.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An authored sequence violates the command contract.
    #[error("invalid probe: {0}")]
    Invalid(String),
    /// The authored TOML is malformed.
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    /// An observation is malformed.
    #[error("observation line {line}: {reason}")]
    Observation { line: usize, reason: String },
    /// A filesystem operation failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The runtime or assembler rejected a request.
    #[error("probe execution: {0}")]
    Execution(String),
    /// JSON output failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
