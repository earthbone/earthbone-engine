// SPDX-License-Identifier: GPL-3.0-or-later

//! Coverage groups reuse the winning QUST VMAD and VM object handle tables.
//! They are a report derived from forms, not a second store of quest state.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use soe_esm::{GlobalFormId, Tag};
use soe_papyrus::vm::coverage::{FunctionCoverage, FunctionId, Report, Summary};
use soe_papyrus::ObjectId;

use crate::{vmad::Vmad, Game, Handle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestCoverage {
    pub id: GlobalFormId,
    pub editor_id: String,
    pub summary: Summary,
    pub functions: Vec<FunctionId>,
    pub executed: Vec<FunctionId>,
    pub missing_scripts: BTreeSet<String>,
    pub vmad_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    pub scripts: Report,
    pub quests: Vec<QuestCoverage>,
}

fn percent(summary: Summary) -> String {
    summary
        .percent()
        .map_or_else(|| "n/a".into(), |p| format!("{p:.2}%"))
}

impl Coverage {
    /// Stable text for a harness artifact. Full uncovered identities remain
    /// available in `scripts.functions` and each quest's `functions` field.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out =
            String::from("Function coverage: loaded non-native bodies, not gameplay completion\n");
        for (name, summary) in &self.scripts.scripts {
            let _ = writeln!(
                out,
                "script {name}: {}/{} ({})",
                summary.executed,
                summary.total,
                percent(*summary)
            );
        }
        for quest in &self.quests {
            let s = quest.summary;
            let _ = writeln!(
                out,
                "quest {} {:?}: {}/{} ({})",
                quest.editor_id,
                quest.id,
                s.executed,
                s.total,
                percent(s)
            );
            for missing in &quest.missing_scripts {
                let _ = writeln!(out, "  missing script {missing}");
            }
            if let Some(error) = &quest.vmad_error {
                let _ = writeln!(out, "  invalid VMAD: {error}");
            }
            for id in &quest.executed {
                let _ = writeln!(out, "  hit {} {:?}", id.script, id.kind);
            }
        }
        for f in &self.scripts.functions {
            if !f.objects.is_empty() {
                let _ = writeln!(out, "hit {} {:?}", f.id.script, f.id.kind);
            }
        }
        out
    }
}

impl Game {
    /// Snapshot the current coverage interval and group quest/alias executions.
    /// Shared scripts count only when they ran on that quest or one of its
    /// aliases. Unowned global calls remain in the per-script report.
    #[must_use]
    pub fn coverage_report(&self) -> Option<Coverage> {
        let scripts = self.vm.coverage_report()?;
        let mut by_script: BTreeMap<&str, Vec<&FunctionCoverage>> = BTreeMap::new();
        for f in &scripts.functions {
            by_script.entry(&f.id.script).or_default().push(f);
        }
        let world = self.world();
        let mut quests = Vec::new();
        for (id, record) in world.order().winners_of(Tag::new(b"QUST")) {
            let mut quest = QuestCoverage {
                id,
                editor_id: record.editor_id().unwrap_or_default(),
                summary: Summary::default(),
                functions: Vec::new(),
                executed: Vec::new(),
                missing_scripts: BTreeSet::new(),
                vmad_error: None,
            };
            let mut scope: BTreeMap<FunctionId, bool> = BTreeMap::new();
            let mut include = |name: &str, object: Option<ObjectId>| {
                if !self.vm.has_script(name) {
                    quest.missing_scripts.insert(name.to_ascii_lowercase());
                }
                for class in self.vm.native_chain(name) {
                    for f in by_script.get(class.as_str()).into_iter().flatten() {
                        let hit = object.is_some_and(|o| f.objects.contains(&Some(o)));
                        *scope.entry(f.id.clone()).or_default() |= hit;
                    }
                }
            };
            if let Some(bytes) = record.find(Tag::new(b"VMAD")) {
                match Vmad::parse(bytes, record.tag) {
                    Ok(vmad) => {
                        let object = world.object_id(Handle::Form(id));
                        for script in vmad.scripts.iter().filter(|s| s.status != 3) {
                            include(&script.name, object);
                        }
                        for alias in &vmad.aliases {
                            let alias_id = u32::try_from(alias.object.alias).unwrap_or(u32::MAX);
                            let object = world.object_id(Handle::Alias {
                                quest: id,
                                id: alias_id,
                            });
                            for script in alias.scripts.iter().filter(|s| s.status != 3) {
                                include(&script.name, object);
                            }
                        }
                    }
                    Err(error) => quest.vmad_error = Some(error.to_string()),
                }
            }
            for (function, executed) in scope {
                if executed {
                    quest.executed.push(function.clone());
                }
                quest.functions.push(function);
            }
            quest.summary = Summary {
                executed: quest.executed.len(),
                total: quest.functions.len(),
            };
            quests.push(quest);
        }
        quests.sort_by_key(|q| q.id);
        Some(Coverage { scripts, quests })
    }
}
