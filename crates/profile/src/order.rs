// SPDX-License-Identifier: GPL-3.0-or-later

//! The load order the engine ends up with, from the files on disk, the
//! lists, and the engine's own rules.
//!
//! The rules, in the order they apply:
//!
//! 1. The hardcoded masters, then the `Skyrim.ccc` creations, always active,
//!    in that fixed order.
//! 2. Everything else in the order `loadorder.txt` gives, then any plugin
//!    `plugins.txt` names that `loadorder.txt` does not, then any file on
//!    disk neither list names, alphabetically and disabled.
//! 3. Within that, masters (by flag or extension) load before plain plugins,
//!    each group keeping its relative order. This is what the engine does
//!    whatever a list says, and it is why an ESM-flagged `.esp` "jumps".
//! 4. Active plugins take slots: a top byte from `00` to `FD` for regular
//!    files, a twelve-bit slot under `FE` for light ones.
//!
//! A plugin whose master is missing, disabled, or later in the order is kept
//! and reported. The engine would crash on it, so a caller decides.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{Error, Result};
use crate::header::PluginHeader;
use crate::plugins_txt::PluginLine;

/// Top bytes `00` to `FD`. `FE` is the light prefix, `FF` belongs to saves.
pub const REGULAR_SLOTS: usize = 254;
/// Twelve bits under `FE`.
pub const LIGHT_SLOTS: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Regular(u8),
    Light(u16),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub enabled: bool,
    /// Loads before plain plugins.
    pub master: bool,
    pub light: bool,
    /// Placed by the engine, not the list.
    pub early: bool,
    pub masters: Vec<String>,
    pub slot: Option<Slot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// A list names a plugin that is not on disk.
    Missing { name: String },
    /// A hardcoded master is not on disk, the engine will not start.
    HardcodedMissing { name: String },
    MissingMaster { plugin: String, master: String },
    MasterDisabled { plugin: String, master: String },
    MasterLater { plugin: String, master: String },
    /// A file on disk whose header could not be read, left out.
    Unreadable { name: String, error: String },
}

#[derive(Debug, Clone, Default)]
pub struct LoadOrder {
    pub entries: Vec<Entry>,
    pub problems: Vec<Problem>,
}

impl LoadOrder {
    pub fn active(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.enabled)
    }

    pub fn active_names(&self) -> Vec<String> {
        self.active().map(|e| e.name.clone()).collect()
    }

    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// The lines to write to a `plugins.txt`: the engine ignores the early
    /// loaders there, MO2 writes them anyway, and so does this.
    pub fn plugins_txt_lines(&self) -> Vec<PluginLine> {
        self.entries
            .iter()
            .map(|e| PluginLine {
                name: e.name.clone(),
                enabled: e.enabled,
            })
            .collect()
    }

    pub fn loadorder_txt_names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.name.clone()).collect()
    }

    /// Masters that are missing, disabled or late for an active plugin.
    pub fn broken(&self) -> bool {
        self.problems.iter().any(|p| {
            matches!(
                p,
                Problem::HardcodedMissing { .. }
                    | Problem::MissingMaster { .. }
                    | Problem::MasterDisabled { .. }
                    | Problem::MasterLater { .. }
            )
        })
    }
}

pub struct Inputs<'a> {
    /// Every plugin file found, headers read.
    pub available: &'a [PluginHeader],
    pub plugins_txt: &'a [PluginLine],
    pub loadorder_txt: &'a [String],
    /// From `ccc::early_loading`, hardcoded names first.
    pub early: &'a [String],
}

fn key(name: &str) -> String {
    name.to_ascii_lowercase()
}

pub fn resolve<'a>(inputs: &Inputs<'a>) -> Result<LoadOrder> {
    let by_name: HashMap<String, &'a PluginHeader> = inputs
        .available
        .iter()
        .map(|h| (key(&h.name), h))
        .collect();
    let mut placed: HashMap<String, ()> = HashMap::new();
    let mut problems = Vec::new();
    let mut early = Vec::new();
    let hardcoded: Vec<String> = crate::ccc::HARDCODED_SE.iter().map(|s| key(s)).collect();

    for name in inputs.early {
        let k = key(name);
        match by_name.get(&k) {
            Some(h) => {
                if placed.insert(k, ()).is_none() {
                    early.push(entry(h, true, true));
                }
            }
            None if hardcoded.contains(&k) => {
                problems.push(Problem::HardcodedMissing { name: name.clone() });
            }
            None => {}
        }
    }

    // The user-ordered part, before the master partition.
    let mut enabled_by: HashMap<String, bool> = HashMap::new();
    for line in inputs.plugins_txt {
        enabled_by.insert(key(&line.name), line.enabled);
    }
    let mut base: Vec<&'a PluginHeader> = Vec::new();
    let mut take = |name: &str, base: &mut Vec<&'a PluginHeader>| {
        let k = key(name);
        if placed.contains_key(&k) {
            return;
        }
        match by_name.get(&k) {
            Some(h) => {
                placed.insert(k, ());
                base.push(h);
            }
            None => problems.push(Problem::Missing {
                name: name.to_string(),
            }),
        }
    };
    for name in inputs.loadorder_txt {
        take(name, &mut base);
    }
    for line in inputs.plugins_txt {
        take(&line.name, &mut base);
    }
    let mut rest: Vec<&'a PluginHeader> = inputs
        .available
        .iter()
        .filter(|h| !placed.contains_key(&key(&h.name)))
        .collect();
    rest.sort_by_key(|h| key(&h.name));
    base.extend(rest);

    let mut entries = early;
    let (masters, plugins): (Vec<_>, Vec<_>) = base.into_iter().partition(|h| h.is_master());
    for h in masters.into_iter().chain(plugins) {
        let enabled = enabled_by.get(&key(&h.name)).copied().unwrap_or(false);
        entries.push(entry(h, enabled, false));
    }

    let mut regular = 0usize;
    let mut light = 0usize;
    for e in entries.iter_mut().filter(|e| e.enabled) {
        if e.light {
            if light >= LIGHT_SLOTS {
                return Err(Error::TooManyLight {
                    count: light + 1,
                    max: LIGHT_SLOTS,
                });
            }
            e.slot = Some(Slot::Light(light as u16));
            light += 1;
        } else {
            if regular >= REGULAR_SLOTS {
                return Err(Error::TooManyPlugins {
                    count: regular + 1,
                    max: REGULAR_SLOTS,
                });
            }
            e.slot = Some(Slot::Regular(regular as u8));
            regular += 1;
        }
    }

    let position: HashMap<String, usize> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| (key(&e.name), i))
        .collect();
    for (i, e) in entries.iter().enumerate().filter(|(_, e)| e.enabled) {
        for m in &e.masters {
            match position.get(&key(m)) {
                None => problems.push(Problem::MissingMaster {
                    plugin: e.name.clone(),
                    master: m.clone(),
                }),
                Some(&j) if !entries[j].enabled => problems.push(Problem::MasterDisabled {
                    plugin: e.name.clone(),
                    master: m.clone(),
                }),
                Some(&j) if j > i => problems.push(Problem::MasterLater {
                    plugin: e.name.clone(),
                    master: m.clone(),
                }),
                Some(_) => {}
            }
        }
    }

    Ok(LoadOrder { entries, problems })
}

fn entry(h: &PluginHeader, enabled: bool, early: bool) -> Entry {
    Entry {
        name: h.name.clone(),
        path: h.path.clone(),
        enabled,
        master: h.is_master(),
        light: h.is_light(),
        early,
        masters: h.masters.clone(),
        slot: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins_txt::parse_plugins_txt;

    fn header(name: &str, masters: &[&str], master_flag: bool, light_flag: bool) -> PluginHeader {
        PluginHeader {
            name: name.into(),
            path: PathBuf::from(format!("/data/{name}")),
            masters: masters.iter().map(|m| m.to_string()).collect(),
            master_flag,
            light_flag,
            localized: false,
        }
    }

    fn vanilla() -> Vec<PluginHeader> {
        vec![
            header("Skyrim.esm", &[], true, false),
            header("Update.esm", &["Skyrim.esm"], true, false),
            header("Dawnguard.esm", &["Skyrim.esm", "Update.esm"], true, false),
            header("HearthFires.esm", &["Skyrim.esm", "Update.esm"], true, false),
            header("Dragonborn.esm", &["Skyrim.esm", "Update.esm"], true, false),
        ]
    }

    #[test]
    fn early_loaders_come_first_then_masters_then_plugins() {
        let mut files = vanilla();
        files.push(header("ccFish.esm", &["Skyrim.esm"], true, false));
        files.push(header("Plain.esp", &["Skyrim.esm"], false, false));
        files.push(header("Flagged.esp", &["Skyrim.esm"], true, false));
        files.push(header("Tiny.esl", &["Skyrim.esm"], false, false));
        files.push(header("Fe.esp", &["Skyrim.esm", "Plain.esp"], false, true));
        files.push(header("Unlisted.esp", &["Skyrim.esm"], false, false));
        let plugins = parse_plugins_txt(
            "*Skyrim.esm\n*Plain.esp\n*Flagged.esp\nDisabledOne.esp\n*Tiny.esl\n*Fe.esp\n",
        );
        let early = crate::ccc::early_loading(&["ccFish.esm".to_string(), "ccGone.esl".to_string()]);
        let order = resolve(&Inputs {
            available: &files,
            plugins_txt: &plugins,
            loadorder_txt: &[],
            early: &early,
        })
        .unwrap();
        let names: Vec<&str> = order.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "Skyrim.esm", "Update.esm", "Dawnguard.esm", "HearthFires.esm", "Dragonborn.esm",
                "ccFish.esm", "Flagged.esp", "Tiny.esl", "Plain.esp", "Fe.esp", "Unlisted.esp",
            ]
        );
        assert!(order.find("ccFish.esm").unwrap().early);
        assert!(!order.find("Unlisted.esp").unwrap().enabled);
        assert_eq!(order.find("Update.esm").unwrap().slot, Some(Slot::Regular(1)));
        assert_eq!(order.find("Flagged.esp").unwrap().slot, Some(Slot::Regular(6)));
        assert_eq!(order.find("Tiny.esl").unwrap().slot, Some(Slot::Light(0)));
        assert_eq!(order.find("Plain.esp").unwrap().slot, Some(Slot::Regular(7)));
        assert_eq!(order.find("Fe.esp").unwrap().slot, Some(Slot::Light(1)));
        assert_eq!(order.find("Unlisted.esp").unwrap().slot, None);
        assert_eq!(
            order.problems,
            vec![Problem::Missing { name: "DisabledOne.esp".into() }]
        );
        assert!(!order.broken());
        assert_eq!(order.plugins_txt_lines().len(), 11);
    }

    #[test]
    fn loadorder_txt_orders_the_disabled_too_and_master_problems_are_reported() {
        let mut files = vanilla();
        files.push(header("A.esp", &["Skyrim.esm", "B.esp"], false, false));
        files.push(header("B.esp", &["Skyrim.esm", "Off.esp"], false, false));
        files.push(header("Off.esp", &["Skyrim.esm"], false, false));
        files.push(header("Orphan.esp", &["Skyrim.esm", "Nowhere.esm"], false, false));
        let plugins = parse_plugins_txt("*A.esp\n*B.esp\nOff.esp\n*Orphan.esp\n");
        let loadorder: Vec<String> = ["Skyrim.esm", "A.esp", "Off.esp", "B.esp", "Orphan.esp"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let early = crate::ccc::early_loading(&[]);
        let order = resolve(&Inputs {
            available: &files,
            plugins_txt: &plugins,
            loadorder_txt: &loadorder,
            early: &early,
        })
        .unwrap();
        let names: Vec<&str> = order.entries.iter().skip(5).map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["A.esp", "Off.esp", "B.esp", "Orphan.esp"]);
        assert!(order.broken());
        assert!(order.problems.contains(&Problem::MasterLater { plugin: "A.esp".into(), master: "B.esp".into() }));
        assert!(order.problems.contains(&Problem::MasterDisabled { plugin: "B.esp".into(), master: "Off.esp".into() }));
        assert!(order.problems.contains(&Problem::MissingMaster { plugin: "Orphan.esp".into(), master: "Nowhere.esm".into() }));
        assert_eq!(order.problems.len(), 3);
    }

    #[test]
    fn a_missing_hardcoded_master_is_a_problem_and_slots_overflow_is_an_error() {
        let files = vec![header("Update.esm", &["Skyrim.esm"], true, false)];
        let early = crate::ccc::early_loading(&[]);
        let order = resolve(&Inputs {
            available: &files,
            plugins_txt: &[],
            loadorder_txt: &[],
            early: &early,
        })
        .unwrap();
        assert!(order.problems.contains(&Problem::HardcodedMissing { name: "Skyrim.esm".into() }));
        assert!(order.broken());

        let mut many = vanilla();
        let mut text = String::new();
        for i in 0..250 {
            let name = format!("P{i:03}.esp");
            many.push(header(&name, &[], false, false));
            text.push_str(&format!("*{name}\n"));
        }
        let plugins = parse_plugins_txt(&text);
        let err = resolve(&Inputs {
            available: &many,
            plugins_txt: &plugins,
            loadorder_txt: &[],
            early: &early,
        })
        .unwrap_err();
        assert!(matches!(err, Error::TooManyPlugins { max: 254, .. }));
    }

    #[test]
    fn names_match_case_insensitively_across_lists_and_disk() {
        let mut files = vanilla();
        files.push(header("MyMod.ESP", &["skyrim.esm"], false, false));
        let plugins = parse_plugins_txt("*mymod.esp\n");
        let early = crate::ccc::early_loading(&[]);
        let order = resolve(&Inputs {
            available: &files,
            plugins_txt: &plugins,
            loadorder_txt: &[],
            early: &early,
        })
        .unwrap();
        let e = order.find("MYMOD.esp").unwrap();
        assert!(e.enabled);
        assert_eq!(e.name, "MyMod.ESP", "the disk spelling is kept");
        assert!(order.problems.is_empty());
    }
}
