// SPDX-License-Identifier: GPL-3.0-or-later

//! Main-thread delivery of tracing events to Godot 4.4 GlobalScope logging.
//! The local godot-docs GlobalScope push_error and push_warning contracts
//! require debugger routing, while ordinary output uses print_rich.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use godot::classes::{INode, Node, ProjectSettings};
use godot::prelude::*;
use soe_trace::logging::{self, LogLine};
use soe_trace::tracing::Level;

static OUTPUT: Mutex<VecDeque<LogLine>> = Mutex::new(VecDeque::new());

pub(crate) fn install() {
    let path = ProjectSettings::singleton()
        .globalize_path("user://logs")
        .to_string();
    if let Err(error) = logging::install(Path::new(&path), |line| {
        if let Ok(mut output) = OUTPUT.lock() {
            if output.len() == 8192 {
                output.pop_front();
            }
            output.push_back(line.clone());
        }
    }) {
        godot_error!("engine logging: {error}");
    }
}

pub(crate) fn flush(all: bool) {
    if let Some(log) = logging::global() {
        let result = if all { log.flush_all() } else { log.flush() };
        if let Err(error) = result {
            godot_error!("engine log file: {error}");
        }
    }
    let lines = OUTPUT
        .lock()
        .map(|mut output| output.drain(..).collect::<Vec<_>>())
        .unwrap_or_default();
    for line in lines {
        route(&line);
    }
}

fn route(line: &LogLine) {
    let text = line.to_string();
    match line.level {
        Level::ERROR => {
            godot_error!("{text}");
        }
        Level::WARN => {
            godot_warn!("{text}");
        }
        _ => {
            // CONTEXT: Log data is literal text, never user-supplied BBCode.
            let text = text.replace('[', "[lb]");
            godot::global::print_rich(&[text.to_variant()]);
        }
    }
}

/// Host logging controls used by the console and F3 overlay.
#[derive(GodotClass)]
#[class(tool, base=Node)]
pub struct SoeLogBridge {
    base: Base<Node>,
}

#[godot_api]
impl INode for SoeLogBridge {
    fn init(base: Base<Node>) -> Self {
        Self { base }
    }

    fn process(&mut self, _delta: f64) {
        flush(false);
    }

    fn exit_tree(&mut self) {
        flush(true);
    }
}

// CONTEXT: Godot callable arguments cross the extension boundary as owned values.
#[allow(clippy::needless_pass_by_value)]
#[godot_api]
impl SoeLogBridge {
    /// Current verbosity directives.
    #[func]
    fn log_filter(&self) -> GString {
        logging::global()
            .map(|log| GString::from(&log.filter()))
            .unwrap_or_default()
    }

    /// Reload SOE_LOG syntax, retaining the previous filter after invalid input.
    #[func]
    fn set_log_filter(&self, filter: GString) -> bool {
        logging::global().is_some_and(|log| log.set_filter(&filter.to_string()).is_ok())
    }

    /// Apply a logging console command, returning a printable response.
    #[func]
    fn console(&self, command: GString) -> GString {
        let command = command.to_string();
        let command = command.trim();
        if command == "log" {
            return self.log_filter();
        }
        if let Some(filter) = command.strip_prefix("log ") {
            return if self.set_log_filter(filter.into()) {
                self.log_filter()
            } else {
                "invalid log filter".into()
            };
        }
        "usage: log [SOE_LOG directives]".into()
    }

    /// Logging clock for a hitch recorder's start and end boundaries.
    #[func]
    fn log_time_ms(&self) -> i64 {
        logging::global().map_or(0, |log| log.now_ms() as i64)
    }

    /// Recent-window output, also including the current one-second bucket.
    #[func]
    fn log_window(&self, start_ms: i64, end_ms: i64) -> PackedStringArray {
        logging::global()
            .map(|log| {
                log.window(start_ms.max(0) as u64, end_ms.max(0) as u64)
                    .iter()
                    .map(|line| GString::from(&line.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Persist the PRF-18 logging window in a user path.
    #[func]
    fn dump_log_window(&self, path: GString, start_ms: i64, end_ms: i64) -> bool {
        let path = ProjectSettings::singleton()
            .globalize_path(&path)
            .to_string();
        logging::global().is_some_and(|log| {
            log.export_window(
                Path::new(&path),
                start_ms.max(0) as u64,
                end_ms.max(0) as u64,
            )
            .is_ok()
        })
    }

    /// Drain the current bucket before a diagnostic capture or shutdown.
    #[func]
    fn flush_logs(&self) {
        flush(true);
    }

    /// Emit representative structured events for the Godot routing regression.
    #[func]
    fn tool_06_emit(&self) {
        soe_trace::error!(target: "game::melee", system = "game::melee", actor = "0:000014", form_id = "0:000014", plugin = "fixture.esp", "tool_06 weapon not equipped");
        soe_trace::warn!(target: "papyrus", system = "papyrus", script = "Tool06", "tool_06 warning");
        soe_trace::info!(target: "host", system = "host", "tool_06 info");
        soe_trace::debug!(target: "game::ai", system = "game::ai", "tool_06 debug");
        soe_trace::trace!(target: "papyrus", system = "papyrus", "tool_06 trace");
    }
}
