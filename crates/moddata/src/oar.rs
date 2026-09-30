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
//! `user.json` beside it replaces the submod data while its name comes from
//! `config.json`, which `Parsing.cpp` reads first.
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
    /// Conditions in each group are ORed. The groups themselves are ANDed.
    pub condition_groups: Vec<Vec<Condition>>,
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
    /// [`Error::Json`] when `root` itself has an unreadable `config.json`.
    /// Invalid nested submods are skipped independently. A directory that
    /// holds neither layout yields nothing.
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
        // Parsing.cpp treats each submod as an independent parse result and
        // keeps successful siblings when one submod fails.
        let _ = walk(&path, depth + 1, out);
    }
    Ok(())
}

fn read_config(dir: &Path, config: &Path) -> Result<SubMod> {
    let author_doc = read_json(config)?;
    let author_obj = author_doc
        .as_object()
        .ok_or_else(|| Error::json(config, "the document is not an object"))?;

    let name = author_obj
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map_or_else(|| directory_name(dir), ToString::to_string);

    let user = dir.join("user.json");
    let (data_path, data_doc) = if user.is_file() {
        if author_obj
            .get("name")
            .and_then(serde_json::Value::as_str)
            .is_none()
        {
            return Err(Error::json(config, "no name"));
        }
        (user.as_path(), read_json(&user)?)
    } else {
        (config, author_doc)
    };
    let data_obj = data_doc
        .as_object()
        .ok_or_else(|| Error::json(data_path, "the document is not an object"))?;

    // Parsing.cpp reads user.json in data-only mode, which still requires an
    // integer priority and does not merge omitted data from config.json.
    let priority = data_obj
        .get("priority")
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| Error::json(data_path, "no valid priority"))?;

    let conditions = data_obj
        .get("conditions")
        .and_then(serde_json::Value::as_array)
        .map_or_else(Vec::new, |a| {
            a.iter()
                .map(|value| read_condition(value).unwrap_or_else(invalid_condition))
                .collect()
        });
    let condition_groups = conditions.iter().cloned().map(|c| vec![c]).collect();

    Ok(SubMod {
        directory: dir.to_path_buf(),
        name,
        priority,
        conditions,
        condition_groups,
        legacy: false,
    })
}

fn read_json(path: &Path) -> Result<serde_json::Value> {
    let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
    serde_json::from_str(&text).map_err(|e| Error::json(path, e.to_string()))
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

fn invalid_condition() -> Condition {
    Condition {
        name: String::new(),
        negated: false,
        required_plugin: None,
        parameters: serde_json::Map::new(),
    }
}

/// The legacy layout: the directory name is the priority and the conditions
/// are one per line in `_conditions.txt`.
fn read_legacy(dir: &Path) -> Result<Option<SubMod>> {
    let file = dir.join("_conditions.txt");
    if !file.is_file() {
        return Ok(None);
    }
    let user = dir.join("user.json");
    if user.is_file() {
        let doc = read_json(&user)?;
        let obj = doc
            .as_object()
            .ok_or_else(|| Error::json(&user, "the document is not an object"))?;
        let priority = obj
            .get("priority")
            .and_then(serde_json::Value::as_i64)
            .and_then(|value| i32::try_from(value).ok())
            .ok_or_else(|| Error::json(&user, "no valid priority"))?;
        let conditions = obj
            .get("conditions")
            .and_then(serde_json::Value::as_array)
            .map_or_else(Vec::new, |a| a.iter().filter_map(read_condition).collect());
        let condition_groups = conditions.iter().cloned().map(|c| vec![c]).collect();
        return Ok(Some(SubMod {
            directory: dir.to_path_buf(),
            name: priority.to_string(),
            priority,
            conditions,
            condition_groups,
            legacy: true,
        }));
    }
    let name = directory_name(dir);
    let Ok(priority) = name.trim().parse::<i32>() else {
        // A conditions file outside a numbered directory is not a submod the
        // plugin would load, so it is reported as absent rather than guessed.
        return Ok(None);
    };
    let text = std::fs::read_to_string(&file).map_err(|e| Error::io(&file, e))?;
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(';'))
        .collect();
    let mut condition_groups = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let first = lines[index];
        if first.ends_with("OR") {
            let mut group = Vec::new();
            let mut line = first;
            loop {
                let continued = line.ends_with("OR");
                let content = if continued {
                    line[..line.len() - 2].trim_end()
                } else {
                    line
                };
                group.push(read_legacy_condition(content));
                index += 1;
                if !continued || index == lines.len() {
                    break;
                }
                line = lines[index];
            }
            condition_groups.push(group);
        } else {
            condition_groups.push(vec![read_legacy_condition(first)]);
            index += 1;
        }
    }
    let conditions = condition_groups.iter().flatten().cloned().collect();
    Ok(Some(SubMod {
        directory: dir.to_path_buf(),
        name,
        priority,
        conditions,
        condition_groups,
        legacy: true,
    }))
}

fn read_legacy_condition(line: &str) -> Condition {
    let (negated, rest) = match line.strip_prefix("NOT ") {
        Some(r) => (true, r.trim()),
        None => (false, line),
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
    fn a_submod_without_a_priority_is_skipped_without_a_default() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(tmp.path(), "Mod/Sub/config.json", r#"{"name":"x"}"#);
        let found = AnimationMod::scan(tmp.path()).expect("scan keeps walking");
        assert!(found.sub_mods.is_empty());
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
    fn mod_04_user_priority_selects_the_higher_priority_submod() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Author/config.json",
            r#"{"name":"Author submod","priority":4}"#,
        );
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Author/user.json",
            r#"{"priority":20}"#,
        );
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Other/config.json",
            r#"{"name":"Other submod","priority":10}"#,
        );

        let animation_mod = AnimationMod::scan(tmp.path()).expect("scan");

        assert_eq!(animation_mod.sub_mods[0].name, "Author submod");
        assert_eq!(animation_mod.sub_mods[0].priority, 20);
    }

    #[test]
    fn mod_04_user_data_replaces_author_conditions() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Walk/config.json",
            r#"{"name":"Walking","priority":4,"conditions":[{"condition":"AuthorOnly"}]}"#,
        );
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Walk/user.json",
            r#"{"priority":8,"conditions":[{"condition":"UserOnly"}]}"#,
        );

        let animation_mod = AnimationMod::scan(tmp.path()).expect("scan");

        assert_eq!(animation_mod.sub_mods[0].name, "Walking");
        assert_eq!(animation_mod.sub_mods[0].priority, 8);
        assert_eq!(animation_mod.sub_mods[0].conditions.len(), 1);
        assert_eq!(animation_mod.sub_mods[0].conditions[0].name, "UserOnly");
    }

    #[test]
    fn mod_04_user_priority_must_fit_i32() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Walk/config.json",
            r#"{"name":"Walking","priority":4}"#,
        );
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Walk/user.json",
            r#"{"priority":2147483648}"#,
        );

        let err = AnimationMod::scan(tmp.path()).expect_err("invalid override");

        assert!(err.to_string().contains("no valid priority"));
    }

    #[test]
    fn mod_04_user_priority_is_required() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Walk/config.json",
            r#"{"name":"Walking","priority":4}"#,
        );
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Walk/user.json",
            r#"{"conditions":[]}"#,
        );

        let err = AnimationMod::scan(tmp.path()).expect_err("missing override priority");

        assert!(err.to_string().contains("no valid priority"));
    }

    #[test]
    fn mod_04_user_override_requires_author_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Walk/config.json",
            r#"{"priority":4}"#,
        );
        write(
            tmp.path(),
            "OpenAnimationReplacer/Pack/Walk/user.json",
            r#"{"priority":8}"#,
        );

        let err = AnimationMod::scan(tmp.path()).expect_err("missing author name");

        assert!(err.to_string().contains("no name"));
    }

    #[test]
    fn mod_04_legacy_user_priority_replaces_directory_priority() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "DynamicAnimationReplacer/_CustomConditions/7/_conditions.txt",
            "IsFemale()\n",
        );
        write(
            tmp.path(),
            "DynamicAnimationReplacer/_CustomConditions/7/user.json",
            r#"{"priority":30,"conditions":[{"condition":"UserOnly"}]}"#,
        );

        let animation_mod = AnimationMod::scan(tmp.path()).expect("scan");

        assert!(animation_mod.sub_mods[0].legacy);
        assert_eq!(animation_mod.sub_mods[0].name, "30");
        assert_eq!(animation_mod.sub_mods[0].priority, 30);
        assert_eq!(animation_mod.sub_mods[0].conditions[0].name, "UserOnly");
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

    #[test]
    fn anm_09_legacy_or_lines_form_an_alternative_condition_group() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "DynamicAnimationReplacer/_CustomConditions/100/_conditions.txt",
            "IsFemale() OR\nIsActorBase(0x800)\nIsRace(0x801)\n",
        );
        let found = AnimationMod::scan(tmp.path()).expect("scan");
        let sub_mod = &found.sub_mods[0];
        assert_eq!(sub_mod.condition_groups.len(), 2);
        assert_eq!(sub_mod.condition_groups[0].len(), 2);
        assert_eq!(sub_mod.condition_groups[1].len(), 1);
    }

    #[test]
    fn anm_09_malformed_json_conditions_are_retained_for_fail_closed_evaluation() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write(
            tmp.path(),
            "OAR/Test/Sub/config.json",
            r#"{"priority":1,"conditions":[null]}"#,
        );
        let found = AnimationMod::scan(tmp.path()).expect("scan");
        assert_eq!(found.sub_mods[0].condition_groups.len(), 1);
        assert_eq!(found.sub_mods[0].condition_groups[0][0].name, "");
    }
}
