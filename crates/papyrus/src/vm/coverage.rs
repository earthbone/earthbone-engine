// SPDX-License-Identifier: GPL-3.0-or-later

//! Function coverage uses the same identities as saved VM frames.
//! Counters are optional diagnostics, not serialized simulation state.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;

use super::{Frame, Function, LoadedScript, ObjectId, Vm};

/// Where a function body lives in its defining script. Names are lowercase.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum FunctionKind {
    Method { state: String, name: String },
    Getter(String),
    Setter(String),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct FunctionId {
    pub script: String,
    pub kind: FunctionKind,
}

/// One loaded, non-native body and the objects on which it executed.
/// `None` denotes execution without a self object, such as a global call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionCoverage {
    pub id: FunctionId,
    pub objects: BTreeSet<Option<ObjectId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub executed: usize,
    pub total: usize,
}

impl Summary {
    /// No executable bodies is undefined coverage, not 100 percent.
    #[must_use]
    pub fn percent(self) -> Option<f64> {
        (self.total != 0).then(|| 100.0 * self.executed as f64 / self.total as f64)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
    pub functions: Vec<FunctionCoverage>,
    pub scripts: BTreeMap<String, Summary>,
}

#[derive(Default)]
pub(super) struct Tracker {
    hits: BTreeMap<FunctionId, BTreeSet<Option<ObjectId>>>,
}

pub(super) fn key_of(script: &LoadedScript, f: &Rc<Function>) -> Option<FunctionKind> {
    if let Some(((state, name), _)) = script.functions.iter().find(|(_, g)| Rc::ptr_eq(g, f)) {
        return Some(FunctionKind::Method {
            state: state.clone(),
            name: name.clone(),
        });
    }
    if let Some((p, _)) = script.getters.iter().find(|(_, g)| Rc::ptr_eq(g, f)) {
        return Some(FunctionKind::Getter(p.clone()));
    }
    if let Some((p, _)) = script.setters.iter().find(|(_, g)| Rc::ptr_eq(g, f)) {
        return Some(FunctionKind::Setter(p.clone()));
    }
    None
}

impl Tracker {
    pub(super) fn record(
        &mut self,
        frame: &mut Frame,
        scripts: &HashMap<String, Rc<LoadedScript>>,
    ) {
        if frame.coverage_recorded {
            return;
        }
        frame.coverage_recorded = true;
        if frame.function.is_native() {
            return;
        }
        let Some(script) = &frame.script else { return };
        let name = script.name.to_ascii_lowercase();
        if !scripts
            .get(&name)
            .is_some_and(|current| Rc::ptr_eq(current, script))
        {
            return;
        }
        let Some(kind) = key_of(script, &frame.function) else {
            return;
        };
        let id = FunctionId { script: name, kind };
        self.hits.entry(id).or_default().insert(frame.self_object);
    }
}

impl Vm {
    /// Start a fresh coverage interval, including frames already in progress.
    /// Loading more scripts ends the interval. Start after the load is complete.
    pub fn start_coverage(&mut self) {
        self.coverage = Some(Tracker::default());
        for thread in &mut self.threads {
            for frame in &mut thread.frames {
                frame.coverage_recorded = false;
            }
        }
    }

    /// Stop recording and return the final report for this interval.
    pub fn stop_coverage(&mut self) -> Option<Report> {
        let report = self.coverage_report();
        self.coverage = None;
        report
    }

    /// Snapshot all loaded non-native functions, including unexecuted bodies.
    /// A scheduled call counts only after the scheduler visits its frame.
    #[must_use]
    pub fn coverage_report(&self) -> Option<Report> {
        let tracker = self.coverage.as_ref()?;
        let mut report = Report::default();
        for script in self.scripts.values() {
            let name = script.name.to_ascii_lowercase();
            let summary = report.scripts.entry(name.clone()).or_default();
            for f in script
                .functions
                .values()
                .chain(script.getters.values())
                .chain(script.setters.values())
            {
                if f.is_native() {
                    continue;
                }
                let Some(kind) = key_of(script, f) else {
                    continue;
                };
                let id = FunctionId {
                    script: name.clone(),
                    kind,
                };
                let objects = tracker.hits.get(&id).cloned().unwrap_or_default();
                summary.total += 1;
                summary.executed += usize::from(!objects.is_empty());
                report.functions.push(FunctionCoverage { id, objects });
            }
        }
        report.functions.sort_by(|a, b| a.id.cmp(&b.id));
        Some(report)
    }
}
