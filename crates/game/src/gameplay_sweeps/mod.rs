// SPDX-License-Identifier: GPL-3.0-or-later

//! Headless gameplay sweeps for consumables, magic and targeting, and combat.
//!
//! Each sweep builds a fresh [`Game`] per record, drives the public world
//! calls the engine itself uses, and records `pass`, `wrong outcome`,
//! `no outcome` or `error` with a cause and a reason, the same row shape as
//! the activation sweep. Expectations come from the CK wiki, UESP and the
//! record data, never from the binary. What needs animation, visuals or
//! sound is listed under `visual` and stays `not run` until the installed
//! Godot pass reports it, so a missing host pass fails the sweep.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{json, Value as Json};
use soe_esm::{GlobalFormId, LoadOrder, Tag};

use crate::Game;

pub const PASS: &str = "pass";
pub const WRONG: &str = "wrong outcome";
pub const NO_OUTCOME: &str = "no outcome";
pub const ERROR: &str = "error";

/// Bind a world borrow and an allocator closure for one block.
///
/// `with_world!(game, |world, alloc| body)` gives `world` as a mutable
/// borrow of the shared world and `alloc` as the VM object allocator.
#[macro_export]
macro_rules! with_world {
    ($game:expr, |$world:ident, $alloc:ident| $body:expr) => {{
        let shared = $game.shared_world();
        let mut $world = shared.borrow_mut();
        let vm = &mut $game.vm;
        let mut make = |name: &str, class: &str| vm.create_object_of(name, class);
        let $alloc: $crate::world::Alloc<'_> = &mut make;
        $body
    }};
}

pub mod combat;
pub mod consumables;
pub mod magic;

/// One checked record, before it is labelled for the report.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub status: &'static str,
    pub cause: String,
    pub reason: String,
    pub observed: Json,
}

impl Outcome {
    #[must_use]
    pub fn pass(observed: Json) -> Self {
        Self {
            status: PASS,
            cause: String::new(),
            reason: String::new(),
            observed,
        }
    }

    #[must_use]
    pub fn wrong(cause: &str, reason: impl Into<String>, observed: Json) -> Self {
        Self::failed(WRONG, cause, reason, observed)
    }

    #[must_use]
    pub fn none(cause: &str, reason: impl Into<String>, observed: Json) -> Self {
        Self::failed(NO_OUTCOME, cause, reason, observed)
    }

    #[must_use]
    pub fn error(cause: &str, reason: impl Into<String>, observed: Json) -> Self {
        Self::failed(ERROR, cause, reason, observed)
    }

    fn failed(
        status: &'static str,
        cause: &str,
        reason: impl Into<String>,
        observed: Json,
    ) -> Self {
        Self {
            status,
            cause: cause.to_string(),
            reason: reason.into(),
            observed,
        }
    }
}

/// Stable cause key from an error message: lowercase words joined by `_`.
#[must_use]
pub fn cause_key(prefix: &str, message: &str) -> String {
    let mut key = String::new();
    for c in message.chars() {
        if c.is_ascii_alphanumeric() {
            key.push(c.to_ascii_lowercase());
        } else if !key.ends_with('_') {
            key.push('_');
        }
    }
    let key = key.trim_matches('_');
    let key: String = key.chars().take(48).collect();
    format!("{prefix}_{}", key.trim_end_matches('_'))
}

/// Run selection: an editor id prefix filter and a record limit.
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub only: Option<String>,
    pub limit: Option<usize>,
}

impl Options {
    /// Read `--only <prefix>` and `--limit <n>` from command line arguments.
    #[must_use]
    pub fn from_args(args: &[String]) -> Self {
        let value = |flag: &str| {
            args.iter()
                .position(|a| a == flag)
                .and_then(|i| args.get(i + 1))
                .cloned()
        };
        Self {
            only: value("--only"),
            limit: value("--limit").and_then(|v| v.parse().ok()),
        }
    }
}

/// Winning records of one tag, filtered and limited, with their editor ids.
#[must_use]
pub fn selected(order: &LoadOrder, tag: Tag, options: &Options) -> Vec<(GlobalFormId, String)> {
    let mut rows: Vec<_> = order
        .winners_of(tag)
        .filter(|(_, record)| !record.is_deleted())
        .map(|(id, record)| (id, record.editor_id().unwrap_or_default()))
        .filter(|(_, name)| {
            options.only.as_ref().is_none_or(|prefix| {
                name.to_ascii_lowercase()
                    .starts_with(&prefix.to_ascii_lowercase())
            })
        })
        .collect();
    rows.sort_by_key(|(id, _)| *id);
    if let Some(limit) = options.limit {
        rows.truncate(limit);
    }
    rows
}

/// A game with the shared order, started, ready to take a sweep case.
#[must_use]
pub fn fresh_game(order: &Arc<LoadOrder>) -> Game {
    let mut game = Game::from_shared_order(Arc::clone(order));
    game.start();
    game
}

/// Label one outcome as a report row.
#[must_use]
pub fn row(
    order: &LoadOrder,
    id: GlobalFormId,
    base_type: &str,
    case: &str,
    outcome: &Outcome,
) -> Json {
    json!({"reference":{"id":id.to_string(),"file":id.file,"object":id.object,
        "editor_id":order.winner(id).and_then(soe_esm::Record::editor_id)},
        "base_type":base_type,"case":case,"status":outcome.status,
        "cause":outcome.cause,"reason":outcome.reason,"observed":outcome.observed})
}

/// Counts by base type and status, and by cause for every row that failed.
#[must_use]
pub fn counts(rows: &[Json]) -> Json {
    let mut types: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut causes: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        let status = row["status"].as_str().unwrap_or_default();
        *types
            .entry(row["base_type"].as_str().unwrap_or_default().into())
            .or_default()
            .entry(status.into())
            .or_default() += 1;
        if status != PASS {
            *causes
                .entry(row["cause"].as_str().unwrap_or_default().into())
                .or_default() += 1;
        }
    }
    json!({"by_base_type":types,"by_failure_cause":causes})
}

/// One check the installed Godot pass must report. Until it does, it fails.
#[must_use]
pub fn visual_check(name: &str, records: &[String]) -> Json {
    json!({"check":name,"records":records,"status":"not run","cause":"godot_pass_not_run"})
}

/// The sweep report: rows, counts and the pending visual checks.
#[must_use]
pub fn report(kind: &str, oracle: &str, rows: &[Json], visual: &[Json]) -> Json {
    let counts = counts(rows);
    json!({"schema":1,"sweep":kind,"oracle":oracle,"counts":counts,
        "visual":visual,"rows":rows})
}

/// True when every row passed and every visual check was reported as a pass.
#[must_use]
pub fn all_passed(report: &Json) -> bool {
    report["rows"]
        .as_array()
        .is_some_and(|rows| rows.iter().all(|r| r["status"] == PASS))
        && report["visual"]
            .as_array()
            .is_some_and(|rows| rows.iter().all(|r| r["status"] == PASS))
}

/// Markdown summary with counts per base type and per failure cause.
#[must_use]
pub fn markdown(report: &Json) -> String {
    let mut out = format!(
        "# {} sweep\n\n{}\n\nA pass covers the stated observation. Archetypes without a model \
         are reported as no outcome, never as passes. Visual checks stay not run until the \
         installed Godot pass reports them.\n\n| Base | pass | wrong outcome | no outcome | error |\n\
         | --- | ---: | ---: | ---: | ---: |\n",
        report["sweep"].as_str().unwrap_or_default(),
        report["oracle"].as_str().unwrap_or_default()
    );
    if let Some(types) = report["counts"]["by_base_type"].as_object() {
        for (kind, by_status) in types {
            let cell = |s: &str| by_status[s].as_u64().unwrap_or(0);
            out += &format!(
                "| {kind} | {} | {} | {} | {} |\n",
                cell(PASS),
                cell(WRONG),
                cell(NO_OUTCOME),
                cell(ERROR)
            );
        }
    }
    out += "\n| Failure cause | Rows |\n| --- | ---: |\n";
    if let Some(causes) = report["counts"]["by_failure_cause"].as_object() {
        for (cause, count) in causes {
            out += &format!("| {cause} | {count} |\n");
        }
    }
    let pending = report["visual"]
        .as_array()
        .map_or(0, |v| v.iter().filter(|c| c["status"] != PASS).count());
    out += &format!("\nVisual checks not passed: {pending}.\n");
    out
}

/// Approximate float equality with an absolute and a relative tolerance.
#[must_use]
pub fn near(a: f32, b: f32, tolerance: f32) -> bool {
    (a - b).abs() <= tolerance.max(b.abs() * 0.02)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cause_keys_are_stable_words() {
        assert_eq!(
            cause_key("drink", "unsupported magic behavior: poison application!"),
            "drink_unsupported_magic_behavior_poison_application"
        );
    }

    #[test]
    fn counts_keep_passes_out_of_causes() {
        let order = LoadOrder::new();
        let id = GlobalFormId { file: 0, object: 1 };
        let rows = vec![
            row(&order, id, "ALCH", "a", &Outcome::pass(json!({}))),
            row(
                &order,
                id,
                "ALCH",
                "b",
                &Outcome::wrong("c1", "x", json!({})),
            ),
            row(
                &order,
                id,
                "ALCH",
                "c",
                &Outcome::none("c1", "y", json!({})),
            ),
        ];
        let counts = counts(&rows);
        assert_eq!(counts["by_failure_cause"]["c1"], 2);
        assert_eq!(counts["by_base_type"]["ALCH"]["pass"], 1);
        let report = report("t", "o", &rows, &[visual_check("v", &[])]);
        assert!(!all_passed(&report));
        assert!(markdown(&report).contains("| c1 | 2 |"));
    }

    #[test]
    fn options_read_flags() {
        let args: Vec<String> = ["--only", "Heal", "--limit", "5"]
            .iter()
            .map(ToString::to_string)
            .collect();
        let options = Options::from_args(&args);
        assert_eq!(options.only.as_deref(), Some("Heal"));
        assert_eq!(options.limit, Some(5));
    }

    #[test]
    fn near_uses_relative_tolerance() {
        assert!(near(100.0, 101.0, 0.1));
        assert!(!near(10.0, 12.0, 0.1));
    }
}
