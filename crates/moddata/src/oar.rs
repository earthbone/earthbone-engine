// SPDX-License-Identifier: GPL-3.0-or-later

//! Open Animation Replacer, and the Dynamic Animation Replacer layout it
//! replaced.
//!
//! Neither is an ini. A mod ships a directory tree and the plugin walks it,
//! so what this module reads is the layout itself:
//!
//! ```text
//! meshes/actors/<actor>/animations/OpenAnimationReplacer/<mod>/<submod>/config.json
//! meshes/actors/<actor>/animations/DynamicAnimationReplacer/_CustomConditions/<priority>/_conditions.txt
//! ```
//!
//! In the modern layout a submod carries a `config.json` whose `priority` is
//! required and whose `conditions` array selects when its animations apply. A
//! `user.json` beside it overrides fields, which `Parsing.cpp` reads second.
//! In the legacy layout the directory name is the priority, an integer, and
//! the conditions are one per line in a text file.
//!
//! Conditions are the part this module deliberately leaves structural. Open
//! Animation Replacer lets a plugin register new condition types at runtime,
//! so the set is open ended and no fixed list would stay right. Each condition
//! keeps its name, its negation, the plugin it needs and its raw parameters,
//! which is enough to report what a mod asks for and to implement the common
//! ones later.

use crate::error::{Error, Result};
use std::path::{Path, PathBuf};

/// One condition as written, with its arguments left as JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// The condition's registered name, such as `IsActorBase`.
    pub name: String,
    /// Written `negated`, so the condition must not hold.
    pub negated: bool,
    /// Present when the condition comes from another plugin.
    pub required_plugin: Option<String>,
    /// Everything else the object carried, so nothing is lost.
    pub parameters: serde_json::Map<String, serde_json::Value>,
}

/// One submod: a directory of animations with the conditions that select it.
#[derive(Debug, Clone, PartialEq)]
pub struct SubMod {
    pub directory: PathBuf,
    /// From `name`, or the directory name when absent.
    pub name: String,
    /// Higher wins. Required in the modern layout, and the directory name in
    /// the legacy one.
    pub priority: i32,
    pub conditions: Vec<Condition>,
    /// True when this came from a `_conditions.txt` tree.
    pub legacy: bool,
}

/// Everything one mod directory asks for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AnimationMod {
    pub sub_mods: Vec<SubMod>,
}

impl AnimationMod {
    /// Walks a directory tree and collects every submod under it, in either
    /// layout. `root` is usually a mod's `meshes` directory or a whole Data
    /// directory.
    ///
    /// # Errors
    ///
    /// [`Error::Json`] when a `config.json` is present but unreadable as a
    /// submod. A directory that holds neither layout yields nothing.
    pub fn scan(root: impl AsRef<Path>) -> Result<Self> {
        let mut sub_mods = Vec::new();
        walk(root.as_ref(), 0, &mut sub_mods)?;
        sub_mods.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.name.cmp(&b.name)));
        Ok(Self { sub_mods })
    }
}

/// Bounded so a mistaken root cannot walk a whole disk. The deepest real
/// layout is meshes/actors/<actor>/animations/<plugin>/<mod>/<submod>.
const MAX_DEPTH: usize = 12;

fn walk(dir: &Path, depth: usize, out: &mut Vec<SubMod>) -> Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    let config = dir.join("config.json");
    if config.is_file() {
        out.push(read_config(dir, &config)?);
        return Ok(());
    }
    if let Some(sub) = read_legacy(dir)? {
        out.push(sub);
        return Ok(());
    }
    // A directory that cannot be listed drops out of the scan. A mod tree
    // often sits beside directories the user cannot read, and one of those
    // should not fail the walk.
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    let mut paths: Vec<PathBuf> = entries
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    paths.sort();
    for path in paths {
        walk(&path, depth + 1, out)?;
    }
    Ok(())
}

fn read_config(dir: &Path, config: &Path) -> Result<SubMod> {
    let text = std::fs::read_to_string(config).map_err(|e| Error::io(config, e))?;
    let doc: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| Error::json(config, e.to_string()))?;
    let obj = doc
        .as_object()
        .ok_or_else(|| Error::json(config, "the document is not an object"))?;

    // Priority is required in the modern layout, and Parsing.cpp refuses a
    // submod without it rather than defaulting.
    let priority = obj
        .get("priority")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| Error::json(config, "no priority"))?;

    let name = obj
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| directory_name(dir), ToString::to_string);

    let conditions = obj
        .get("conditions")
        .and_then(serde_json::Value::as_array)
        .map_or_else(Vec::new, |a| a.iter().filter_map(read_condition).collect());

    Ok(SubMod {
        directory: dir.to_path_buf(),
        name,
        priority: i32::try_from(priority).unwrap_or(i32::MAX),
        conditions,
        legacy: false,
    })
}

fn read_condition(value: &serde_json::Value) -> Option<Condition> {
    let obj = value.as_object()?;
    let name = obj.get("condition")?.as_str()?.to_string();
    let mut parameters = obj.clone();
    parameters.remove("condition");
    parameters.remove("negated");
    parameters.remove("requiredPlugin");
    Some(Condition {
        name,
        negated: obj
            .get("negated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        required_plugin: obj
            .get("requiredPlugin")
            .and_then(serde_json::Value::as_str)
            .map(ToString::to_string),
        parameters,
    })
}

/// The legacy layout: the directory name is the priority and the conditions
/// are one per line in `_conditions.txt`.
fn read_legacy(dir: &Path) -> Result<Option<SubMod>> {
    let file = dir.join("_conditions.txt");
    if !file.is_file() {
        return Ok(None);
    }
    let name = directory_name(dir);
    let Ok(priority) = name.trim().parse::<i32>() else {
        // A conditions file outside a numbered directory is not a submod the
        // plugin would load, so it is reported as absent rather than guessed.
        return Ok(None);
    };
    let text = std::fs::read_to_string(&file).map_err(|e| Error::io(&file, e))?;
    let conditions = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(';'))
        .map(|l| {
            let (negated, rest) = match l.strip_prefix("NOT ") {
                Some(r) => (true, r.trim()),
                None => (false, l),
            };
            let head = rest.split('(').next().unwrap_or(rest).trim();
            Condition {
                name: head.to_string(),
                negated,
                required_plugin: None,
                parameters: {
                    let mut m = serde_json::Map::new();
                    m.insert(
                        "raw".to_string(),
                        serde_json::Value::String(rest.to_string()),
                    );
                    m
                },
            }
        })
        .collect();
    Ok(Some(SubMod {
        directory: dir.to_path_buf(),
        name,
        priority,
        conditions,
        legacy: true,
    }))
}

fn directory_name(dir: &Path) -> String {
    dir.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::AnimationMod;
    use std::fs;

    fn write(dir: &std::path::Path, rel: &str, text: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().expect("a parent")).expect("dirs");
        fs::write(path, text).expect("write");
    }

    #[test]
    fn a_modern_submod_reads_its_priority_and_conditions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "OpenAnimationReplacer/MyMod/Walk/config.json",
            r#"{"name":"Walking","priority":42,"conditions":[
                 {"condition":"IsActorBase","negated":true,"form":{"pluginName":"Skyrim.esm","formID":"7"}},
                 {"condition":"IsEquippedType","requiredPlugin":"Other.dll","typeValue":3}
               ]}"#,
        );
        let m = AnimationMod::scan(tmp.path()).expect("scan");
        assert_eq!(m.sub_mods.len(), 1);
        let s = &m.sub_mods[0];
        assert_eq!(s.name, "Walking");
        assert_eq!(s.priority, 42);
        assert!(!s.legacy);
        assert_eq!(s.conditions.len(), 2);
        assert!(s.conditions[0].negated);
        assert_eq!(s.conditions[0].name, "IsActorBase");
        // The rest of the object is kept rather than dropped.
        assert!(s.conditions[0].parameters.contains_key("form"));
        assert_eq!(
            s.conditions[1].required_plugin.as_deref(),
            Some("Other.dll")
        );
    }

    #[test]
    fn a_submod_without_a_priority_is_an_error_not_a_default() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(tmp.path(), "Mod/Sub/config.json", r#"{"name":"x"}"#);
        assert!(AnimationMod::scan(tmp.path()).is_err());
    }

    #[test]
    fn a_missing_name_falls_back_to_the_directory() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(tmp.path(), "Mod/Sprint/config.json", r#"{"priority":1}"#);
        let m = AnimationMod::scan(tmp.path()).expect("scan");
        assert_eq!(m.sub_mods[0].name, "Sprint");
    }

    #[test]
    fn the_legacy_layout_takes_its_priority_from_the_directory_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "DynamicAnimationReplacer/_CustomConditions/100/_conditions.txt",
            "IsFemale()\nNOT IsEquippedRight(\"Skyrim.esm\" | 0x12)\n; a comment\n",
        );
        let m = AnimationMod::scan(tmp.path()).expect("scan");
        assert_eq!(m.sub_mods.len(), 1);
        let s = &m.sub_mods[0];
        assert!(s.legacy);
        assert_eq!(s.priority, 100);
        assert_eq!(s.conditions.len(), 2);
        assert_eq!(s.conditions[0].name, "IsFemale");
        assert!(s.conditions[1].negated);
        assert_eq!(s.conditions[1].name, "IsEquippedRight");
    }

    #[test]
    fn a_conditions_file_outside_a_numbered_directory_is_not_a_submod() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(tmp.path(), "NotANumber/_conditions.txt", "IsFemale()\n");
        let m = AnimationMod::scan(tmp.path()).expect("scan");
        assert!(m.sub_mods.is_empty());
    }

    #[test]
    fn submods_come_back_highest_priority_first() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(tmp.path(), "M/A/config.json", r#"{"priority":1}"#);
        write(tmp.path(), "M/B/config.json", r#"{"priority":9}"#);
        let m = AnimationMod::scan(tmp.path()).expect("scan");
        assert_eq!(m.sub_mods[0].priority, 9);
        assert_eq!(m.sub_mods[1].priority, 1);
    }

    #[test]
    fn both_layouts_can_live_in_one_tree() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "OpenAnimationReplacer/M/S/config.json",
            r#"{"priority":5}"#,
        );
        write(
            tmp.path(),
            "DynamicAnimationReplacer/_CustomConditions/7/_conditions.txt",
            "IsFemale()\n",
        );
        let m = AnimationMod::scan(tmp.path()).expect("scan");
        assert_eq!(m.sub_mods.len(), 2);
        assert!(m.sub_mods.iter().any(|s| s.legacy));
        assert!(m.sub_mods.iter().any(|s| !s.legacy));
    }
}
