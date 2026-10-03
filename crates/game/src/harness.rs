// SPDX-License-Identifier: GPL-3.0-or-later

//! Conformance scenarios: a line-based script drives a [`Game`] and checks
//! what the vanilla engine would show. The oracle for a scenario is a
//! capture made once in the real game (a `Papyrus.0.log` with tracing on,
//! or values read from a save), so the engine is checked against Skyrim
//! without launching Skyrim. `docs/validation.md` has the format and the
//! capture procedure.
//!
//! Grammar, one step per line, `#` starts a comment, names are editor ids
//! and `player` is the player reference:
//!
//! ```text
//! activate <ref> [by <actor>]
//! tick <seconds>
//! event <form> <EventName>
//! stage <quest> <n>
//! debug <command line>            see `crate::debug`
//! expect stage <quest> <op> <n>
//! expect stage-done <quest> <n> [true|false]
//! expect objective <quest> <n> displayed|completed|failed [true|false]
//! expect running|completed|failed <quest> [true|false]
//! expect item <holder> <item> <op> <n>
//! expect enabled <ref> [true|false]
//! expect cell <ref> <cell>
//! expect actor-value <actor> <name> <op> <value>
//! expect global <name> <op> <value>
//! expect notification "<text>"
//! expect trace "<text>"
//! expect traces-match <path to a Papyrus log or a plain list of lines>
//! ```
//!
//! `<op>` is one of `== != < <= > >=`.

use std::fmt::Write as _;

use soe_esm::GlobalFormId;

use crate::Game;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl Op {
    fn parse(s: &str) -> Result<Op, String> {
        Ok(match s {
            "==" => Op::Eq,
            "!=" => Op::Ne,
            "<" => Op::Lt,
            "<=" => Op::Le,
            ">" => Op::Gt,
            ">=" => Op::Ge,
            other => return Err(format!("unknown operator {other}")),
        })
    }
    fn holds(&self, actual: f64, expected: f64) -> bool {
        match self {
            Op::Eq => (actual - expected).abs() < 1e-4,
            Op::Ne => (actual - expected).abs() >= 1e-4,
            Op::Lt => actual < expected,
            Op::Le => actual <= expected,
            Op::Gt => actual > expected,
            Op::Ge => actual >= expected,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Check {
    Stage {
        quest: String,
        op: Op,
        value: f64,
    },
    StageDone {
        quest: String,
        stage: u16,
        expected: bool,
    },
    Objective {
        quest: String,
        index: u16,
        field: String,
        expected: bool,
    },
    QuestFlag {
        quest: String,
        flag: String,
        expected: bool,
    },
    Item {
        holder: String,
        item: String,
        op: Op,
        value: f64,
    },
    Enabled {
        reference: String,
        expected: bool,
    },
    Cell {
        reference: String,
        cell: String,
    },
    ActorValue {
        actor: String,
        name: String,
        op: Op,
        value: f64,
    },
    Global {
        name: String,
        op: Op,
        value: f64,
    },
    Notification(String),
    Trace(String),
    TracesMatch(String),
    TraceSequence(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Activate {
        target: String,
        activator: String,
    },
    Tick(f64),
    Event {
        form: String,
        name: String,
    },
    Stage {
        quest: String,
        stage: u16,
    },
    /// A [`crate::debug`] command line, which is how a scenario reaches a
    /// state the gameplay path does not produce on its own.
    Debug(String),
    Expect(Check),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scenario {
    pub steps: Vec<(usize, Step)>,
}

impl Scenario {
    /// Add the nonempty trace lines from a vanilla Papyrus log as one ordered
    /// expectation at the end of this scenario.
    pub fn append_papyrus_log_expectations(&mut self, text: &str) {
        let line = self.steps.last().map_or(1, |(line, _)| line + 1);
        self.steps.push((
            line,
            Step::Expect(Check::TraceSequence(papyrus_log_traces(text))),
        ));
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Report {
    pub steps: usize,
    pub checks: usize,
    /// `line: what failed` per failed check.
    pub failures: Vec<String>,
    /// Present when the caller enabled VM coverage before this run.
    pub coverage: Option<crate::coverage::Coverage>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

fn quoted(rest: &str) -> Result<String, String> {
    let t = rest.trim();
    if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
        Ok(t[1..t.len() - 1].to_string())
    } else {
        Err(format!("expected a quoted string, got {t}"))
    }
}

fn flag(words: &[&str], at: usize) -> Result<bool, String> {
    match words.get(at) {
        None | Some(&"true") => Ok(true),
        Some(&"false") => Ok(false),
        Some(other) => Err(format!("expected true or false, got {other}")),
    }
}

fn number(s: &str) -> Result<f64, String> {
    s.parse().map_err(|_| format!("expected a number, got {s}"))
}

fn stage_index(s: &str) -> Result<u16, String> {
    s.parse()
        .map_err(|_| format!("expected a stage index, got {s}"))
}

impl Scenario {
    pub fn parse(text: &str) -> Result<Scenario, String> {
        let mut steps = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = n + 1;
            let body = raw.split('#').next().unwrap_or("").trim();
            if body.is_empty() {
                continue;
            }
            let step = parse_step(body).map_err(|e| format!("line {line}: {e}"))?;
            steps.push((line, step));
        }
        Ok(Scenario { steps })
    }
}

fn parse_step(body: &str) -> Result<Step, String> {
    let words: Vec<&str> = body.split_whitespace().collect();
    let arg = |i: usize| {
        words
            .get(i)
            .copied()
            .ok_or_else(|| format!("missing argument {i}"))
    };
    Ok(match words[0] {
        "activate" => {
            let target = arg(1)?.to_string();
            let activator = match (words.get(2), words.get(3)) {
                (Some(&"by"), Some(a)) => (*a).to_string(),
                (None, _) => "player".to_string(),
                _ => return Err("expected `activate <ref> [by <actor>]`".into()),
            };
            Step::Activate { target, activator }
        }
        "tick" => Step::Tick(number(arg(1)?)?),
        "event" => Step::Event {
            form: arg(1)?.to_string(),
            name: arg(2)?.to_string(),
        },
        "stage" => Step::Stage {
            quest: arg(1)?.to_string(),
            stage: stage_index(arg(2)?)?,
        },
        "debug" => {
            let rest = body["debug".len()..].trim();
            if rest.is_empty() {
                return Err("expected `debug <command line>`".into());
            }
            Step::Debug(rest.to_string())
        }
        "expect" => Step::Expect(match arg(1)? {
            "stage" => Check::Stage {
                quest: arg(2)?.to_string(),
                op: Op::parse(arg(3)?)?,
                value: number(arg(4)?)?,
            },
            "stage-done" => Check::StageDone {
                quest: arg(2)?.to_string(),
                stage: stage_index(arg(3)?)?,
                expected: flag(&words, 4)?,
            },
            "objective" => Check::Objective {
                quest: arg(2)?.to_string(),
                index: stage_index(arg(3)?)?,
                field: arg(4)?.to_string(),
                expected: flag(&words, 5)?,
            },
            f @ ("running" | "completed" | "failed") => Check::QuestFlag {
                quest: arg(2)?.to_string(),
                flag: f.to_string(),
                expected: flag(&words, 3)?,
            },
            "item" => Check::Item {
                holder: arg(2)?.to_string(),
                item: arg(3)?.to_string(),
                op: Op::parse(arg(4)?)?,
                value: number(arg(5)?)?,
            },
            "enabled" => Check::Enabled {
                reference: arg(2)?.to_string(),
                expected: flag(&words, 3)?,
            },
            "cell" => Check::Cell {
                reference: arg(2)?.to_string(),
                cell: arg(3)?.to_string(),
            },
            "actor-value" => Check::ActorValue {
                actor: arg(2)?.to_string(),
                name: arg(3)?.to_string(),
                op: Op::parse(arg(4)?)?,
                value: number(arg(5)?)?,
            },
            "global" => Check::Global {
                name: arg(2)?.to_string(),
                op: Op::parse(arg(3)?)?,
                value: number(arg(4)?)?,
            },
            "notification" => Check::Notification(quoted(&body["expect notification".len()..])?),
            "trace" => Check::Trace(quoted(&body["expect trace".len()..])?),
            "traces-match" => Check::TracesMatch(arg(2)?.to_string()),
            other => return Err(format!("unknown check {other}")),
        }),
        other => return Err(format!("unknown step {other}")),
    })
}

/// The trace lines of a Papyrus log: the `[date - time]` prefix the game
/// writes is dropped, blank lines are skipped.
pub fn papyrus_log_traces(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let body = vanilla_trace_payload(l).unwrap_or(l);
            if body.trim().is_empty() {
                return None;
            }
            Some(body.to_string())
        })
        .collect()
}

fn vanilla_trace_payload(line: &str) -> Option<&str> {
    let close = line.find("] ")?;
    let timestamp = line.strip_prefix('[')?.get(..close.checked_sub(1)?)?;
    let (date, time) = timestamp.split_once(" - ")?;
    let mut date_parts = date.split('/');
    let (month, day, year) = (date_parts.next()?, date_parts.next()?, date_parts.next()?);
    if date_parts.next().is_some()
        || month.len() != 2
        || day.len() != 2
        || year.len() != 4
        || !month.bytes().all(|byte| byte.is_ascii_digit())
        || !day.bytes().all(|byte| byte.is_ascii_digit())
        || !year.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let time = time
        .strip_suffix("AM")
        .or_else(|| time.strip_suffix("PM"))?;
    let (clock, seconds) = time.split_once(':')?;
    let (hour, minute) = (clock, seconds.get(..2)?);
    let second = seconds.get(3..5)?;
    if seconds.as_bytes().get(2) != Some(&b':')
        || seconds.len() != 5
        || hour.len() != 2
        || !hour.bytes().all(|byte| byte.is_ascii_digit())
        || !minute.bytes().all(|byte| byte.is_ascii_digit())
        || !second.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(&line[close + 2..])
}

fn resolve(game: &mut Game, name: &str) -> Result<GlobalFormId, String> {
    if name.eq_ignore_ascii_case("player") {
        return Ok(game.player());
    }
    game.form(name)
        .ok_or_else(|| format!("no form named {name}"))
}

/// Run every step against the game. Expectations never stop the run, they
/// land in the report.
pub fn run(game: &mut Game, scenario: &Scenario) -> Report {
    let mut report = Report::default();
    // One console for the whole scenario, so a `prid` in one step selects
    // the target the next step works on, the way it does in the game.
    let mut console = crate::debug::Console::new();
    for (line, step) in &scenario.steps {
        report.steps += 1;
        let outcome = match step {
            Step::Activate { target, activator } => resolve(game, target).and_then(|t| {
                let a = resolve(game, activator)?;
                if game.activate(t, a) {
                    Ok(())
                } else {
                    Err(format!("{target} is not a reference"))
                }
            }),
            Step::Tick(seconds) => {
                game.tick(*seconds);
                Ok(())
            }
            Step::Event { form, name } => resolve(game, form).map(|id| {
                game.send_event(id, name, &[]);
            }),
            Step::Stage { quest, stage } => resolve(game, quest).and_then(|q| {
                if game.set_stage(q, *stage) {
                    game.run_until_idle(64);
                    Ok(())
                } else {
                    Err(format!("{quest} has no stage {stage}"))
                }
            }),
            Step::Debug(command) => console.run(game, command).map(|_| ()),
            Step::Expect(check) => {
                report.checks += 1;
                evaluate(game, check)
            }
        };
        if let Err(e) = outcome {
            report.failures.push(format!("line {line}: {e}"));
        }
    }
    report.coverage = game.coverage_report();
    report
}

fn compare(what: &str, actual: f64, op: &Op, expected: f64) -> Result<(), String> {
    if op.holds(actual, expected) {
        Ok(())
    } else {
        Err(format!("{what} is {actual}, expected {op:?} {expected}"))
    }
}

fn expect_bool(what: &str, actual: bool, expected: bool) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{what} is {actual}, expected {expected}"))
    }
}

fn evaluate(game: &mut Game, check: &Check) -> Result<(), String> {
    match check {
        Check::Stage { quest, op, value } => {
            let q = resolve(game, quest)?;
            let stage = game.world_mut().quest_state(q).map_or(0, |s| s.highest);
            compare(&format!("stage of {quest}"), f64::from(stage), op, *value)
        }
        Check::StageDone {
            quest,
            stage,
            expected,
        } => {
            let q = resolve(game, quest)?;
            let done = game
                .world_mut()
                .quest_state(q)
                .is_some_and(|s| s.done.contains(stage));
            expect_bool(&format!("stage {stage} of {quest} done"), done, *expected)
        }
        Check::Objective {
            quest,
            index,
            field,
            expected,
        } => {
            let q = resolve(game, quest)?;
            let mut w = game.world_mut();
            let o = w
                .objective(q, *index)
                .ok_or_else(|| format!("{quest} has no objective {index}"))?;
            let actual = match field.as_str() {
                "displayed" => o.displayed,
                "completed" => o.completed,
                "failed" => o.failed,
                other => return Err(format!("unknown objective field {other}")),
            };
            expect_bool(
                &format!("objective {index} of {quest} {field}"),
                actual,
                *expected,
            )
        }
        Check::QuestFlag {
            quest,
            flag,
            expected,
        } => {
            let q = resolve(game, quest)?;
            let mut w = game.world_mut();
            let s = w
                .quest_state(q)
                .ok_or_else(|| format!("{quest} is not a quest"))?;
            let actual = match flag.as_str() {
                "running" => s.running,
                "completed" => s.completed,
                _ => s.failed,
            };
            expect_bool(&format!("{quest} {flag}"), actual, *expected)
        }
        Check::Item {
            holder,
            item,
            op,
            value,
        } => {
            let h = resolve(game, holder)?;
            let it = resolve(game, item)?;
            let n = game.world_mut().item_count(h, it);
            compare(
                &format!("{item} held by {holder}"),
                f64::from(n),
                op,
                *value,
            )
        }
        Check::Enabled {
            reference,
            expected,
        } => {
            let r = resolve(game, reference)?;
            let enabled = game.world_mut().reference(r).is_some_and(|s| s.enabled);
            expect_bool(&format!("{reference} enabled"), enabled, *expected)
        }
        Check::Cell { reference, cell } => {
            let r = resolve(game, reference)?;
            let c = resolve(game, cell)?;
            let actual = game.world_mut().reference(r).and_then(|s| s.cell);
            if actual == Some(c) {
                Ok(())
            } else {
                Err(format!("{reference} is in {actual:?}, expected {cell}"))
            }
        }
        Check::ActorValue {
            actor,
            name,
            op,
            value,
        } => {
            let a = resolve(game, actor)?;
            let v = game.world_mut().actor_value(a, name);
            compare(&format!("{name} of {actor}"), f64::from(v), op, *value)
        }
        Check::Global { name, op, value } => {
            let g = resolve(game, name)?;
            let v = game.world_mut().global_value(g);
            compare(name, f64::from(v), op, *value)
        }
        Check::Notification(text) => {
            if game.world().notifications.iter().any(|n| n == text) {
                Ok(())
            } else {
                Err(format!(
                    "no notification {text:?}, saw {:?}",
                    game.world().notifications
                ))
            }
        }
        Check::Trace(text) => {
            if game.traces().iter().any(|t| t == text) {
                Ok(())
            } else {
                Err(format!("no trace {text:?}, saw {:?}", game.traces()))
            }
        }
        Check::TracesMatch(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            let expected = papyrus_log_traces(&text);
            let actual = game.traces();
            diff_traces(&expected, &actual)
        }
        Check::TraceSequence(expected) => diff_traces(expected, &game.traces()),
    }
}

/// Expected traces must appear in the actual log in the same order. Extra
/// actual lines are allowed, since the engine logs its own errors too.
fn diff_traces(expected: &[String], actual: &[String]) -> Result<(), String> {
    let mut pos = 0;
    let mut divergences = Vec::new();
    for (i, e) in expected.iter().enumerate() {
        match actual[pos..].iter().position(|a| a == e) {
            Some(k) => pos += k + 1,
            None => {
                let mut msg = String::new();
                let _ = write!(
                    msg,
                    "expected trace {i} {e:?} not found after position {pos}"
                );
                if let Some(next) = actual.get(pos) {
                    let _ = write!(msg, ", next actual line is {next:?}");
                }
                divergences.push(msg);
            }
        }
    }
    if divergences.is_empty() {
        Ok(())
    } else {
        Err(divergences.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scenario_parses_every_step_kind() {
        let text = r#"
            # comment
            activate LeverRef by player
            activate DoorRef
            tick 2.5
            event MQ01 OnInit
            stage MQ01 10
            expect stage MQ01 >= 10
            expect stage-done MQ01 10
            expect objective MQ01 10 completed false
            expect running MQ01
            expect item player Gold001 == 5
            expect enabled LeverRef false
            expect cell player TestCell
            expect actor-value player Health > 0
            expect global GameHour < 24
            expect notification "Lever pulled"
            expect trace "MQ01 init"
            expect traces-match captures/lever.log
            debug player.setav health 50
        "#;
        let s = Scenario::parse(text).unwrap();
        assert_eq!(s.steps.len(), 18);
        assert_eq!(
            s.steps[17].1,
            Step::Debug("player.setav health 50".to_string())
        );
        assert!(Scenario::parse("debug").is_err());
        assert_eq!(
            s.steps[1].1,
            Step::Activate {
                target: "DoorRef".into(),
                activator: "player".into()
            }
        );
        assert_eq!(
            s.steps[14].1,
            Step::Expect(Check::Notification("Lever pulled".into()))
        );
        assert!(Scenario::parse("expect stage MQ01 ~ 3")
            .unwrap_err()
            .contains("line 1"));
        assert!(Scenario::parse("jump").is_err());
        assert!(Scenario::parse("expect notification unquoted").is_err());
    }

    #[test]
    fn papyrus_log_lines_lose_their_timestamp() {
        let log = "[09/14/2026 - 10:00:01AM] MQ01 init\n\n[09/14/2026 - 10:00:02AM] Lever pulled\nplain line\n";
        assert_eq!(
            papyrus_log_traces(log),
            ["MQ01 init", "Lever pulled", "plain line"]
        );
    }

    #[test]
    fn expected_traces_must_appear_in_order() {
        let a = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert!(diff_traces(&a(&["x", "y"]), &a(&["x", "noise", "y"])).is_ok());
        assert!(diff_traces(&a(&["y", "x"]), &a(&["x", "y"])).is_err());
        assert!(diff_traces(&a(&[]), &a(&["x"])).is_ok());
    }
}
