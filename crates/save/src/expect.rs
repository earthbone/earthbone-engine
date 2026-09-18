// SPDX-License-Identifier: GPL-3.0-or-later

//! Turning a save into scenario expectations the harness in `crates/game` can
//! run, in the grammar `docs/validation.md` documents.
//!
//! The save stores FormIDs and the harness resolves editor ids, so a caller
//! supplies the mapping. It comes from a loaded load order, which this crate
//! deliberately does not depend on: a save reader should not need the game's
//! plugins to parse, only to name what it found. A form the mapping does not
//! cover becomes a comment carrying its FormID rather than being dropped, so
//! the output always accounts for everything in the save.

use crate::Save;

/// One rendered expectation, or a comment for something that could not be
/// named.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Line {
    Expect(String),
    Comment(String),
}

impl std::fmt::Display for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Line::Expect(text) => write!(f, "{text}"),
            Line::Comment(text) => write!(f, "# {text}"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Expectations {
    pub lines: Vec<Line>,
    /// Stages that produced an expectation.
    pub stages: usize,
    /// Globals that produced an expectation.
    pub globals: usize,
    /// Forms the name mapping did not cover.
    pub unnamed: usize,
}

impl Expectations {
    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&line.to_string());
            out.push('\n');
        }
        out
    }
}

impl Save {
    /// Renders the save's quest stages and globals as scenario expectations.
    ///
    /// `name_of` maps a FormID to the editor id the harness resolves. Return
    /// `None` for a form you cannot name and it becomes a comment.
    pub fn to_expectations<F>(&self, name_of: F) -> crate::Result<Expectations>
    where
        F: Fn(u32) -> Option<String>,
    {
        let mut out = Expectations::default();

        for (form, stages) in self.quest_stages()? {
            let done: Vec<_> = stages.iter().filter(|stage| stage.is_done()).collect();
            if done.is_empty() {
                continue;
            }
            match name_of(form) {
                Some(name) => {
                    for stage in done {
                        out.lines.push(Line::Expect(format!(
                            "expect stage-done {name} {} true",
                            stage.stage
                        )));
                        out.stages += 1;
                    }
                }
                None => {
                    out.unnamed += 1;
                    out.lines.push(Line::Comment(format!(
                        "quest {form:#010x} has {} done stages, no editor id for it",
                        done.len()
                    )));
                }
            }
        }

        for variable in self.globals()?.variables {
            let Some(form) = self.resolve(variable.form) else {
                out.unnamed += 1;
                continue;
            };
            match name_of(form) {
                Some(name) => {
                    out.lines.push(Line::Expect(format!(
                        "expect global {name} == {}",
                        variable.value
                    )));
                    out.globals += 1;
                }
                None => {
                    out.unnamed += 1;
                    out.lines.push(Line::Comment(format!(
                        "global {form:#010x} is {}, no editor id for it",
                        variable.value
                    )));
                }
            }
        }

        Ok(out)
    }
}
