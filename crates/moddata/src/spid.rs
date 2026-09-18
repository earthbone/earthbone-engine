// SPDX-License-Identifier: GPL-3.0-or-later

//! Spell Perk Item Distributor, `*_DISTR.ini`.
//!
//! One line per distribution, all of them in the anonymous section. The field
//! order is written at the top of SPID's own `Defs.h`:
//!
//! ```text
//! Record = FormOrEditorID|StringFilters|RawFormFilters|LevelFilters|Traits|IdxOrCount|Chance
//! ```
//!
//! The key names what is being handed out, and it carries two optional
//! prefixes that `DeathDistribution.cpp` peels in order: `Final` marks an
//! outfit that later distributions may not replace, and `Death` moves the
//! distribution to the moment the actor dies. So `FinalDeathOutfit` is a valid
//! key and means both.

use crate::filter::BucketFilters;
use crate::form::FormRef;
use crate::ini::{field, is_valid_entry, split_list, Ini};
use std::path::Path;

/// What a line hands out. `Form` is the generic case, where the record type is
/// inferred from what the form turns out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordType {
    Form,
    Spell,
    Perk,
    Item,
    Shout,
    LevSpell,
    Package,
    Outfit,
    Keyword,
    Faction,
    SleepOutfit,
    Skin,
}

/// The names in `RECORD::detail::names`, in enum order.
const RECORD_NAMES: [(&str, RecordType); 12] = [
    ("Form", RecordType::Form),
    ("Spell", RecordType::Spell),
    ("Perk", RecordType::Perk),
    ("Item", RecordType::Item),
    ("Shout", RecordType::Shout),
    ("LevSpell", RecordType::LevSpell),
    ("Package", RecordType::Package),
    ("Outfit", RecordType::Outfit),
    ("Keyword", RecordType::Keyword),
    ("Faction", RecordType::Faction),
    ("SleepOutfit", RecordType::SleepOutfit),
    ("Skin", RecordType::Skin),
];

/// An inclusive range, which is how SPID writes levels and counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub min: u32,
    pub max: u32,
}

impl Range {
    fn exact(v: u32) -> Self {
        Self { min: v, max: v }
    }
}

/// A skill or skill-weight bound, written `skill(min/max)` or `wskill(min)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillLevel {
    /// Skill index, which SPID accepts below 18.
    pub skill: u32,
    pub range: Range,
}

/// The level field, field 3.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LevelFilters {
    pub actor_level: Option<Range>,
    pub skill_levels: Vec<SkillLevel>,
    /// Weights taken from the actor's class rather than its current skill.
    pub skill_weights: Vec<SkillLevel>,
}

/// The trait field, field 4. Every one is a tri-state: unset means the filter
/// does not care.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Traits {
    /// `M` or `-F` for male, `F` or `-M` for female.
    pub female: Option<bool>,
    pub unique: Option<bool>,
    pub summonable: Option<bool>,
    pub child: Option<bool>,
    pub leveled: Option<bool>,
    pub teammate: Option<bool>,
    pub starts_dead: Option<bool>,
}

/// Field 5, which means a stack index for a package and a count otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexOrCount {
    Index(i32),
    Count(Range),
}

/// One parsed line.
#[derive(Debug, Clone, PartialEq)]
pub struct Distribution {
    pub record_type: RecordType,
    /// `Final`, only meaningful on an outfit.
    pub final_: bool,
    /// `Death`, so it applies when the actor dies.
    pub on_death: bool,
    /// What is handed out.
    pub form: FormRef,
    pub strings: BucketFilters,
    pub forms: BucketFilters,
    pub levels: LevelFilters,
    pub traits: Traits,
    pub index_or_count: IndexOrCount,
    /// Percent, 0 to 100.
    pub chance: f32,
    /// 1 based line in its file, for reporting a conflict.
    pub line: usize,
}

/// Every distribution one file asks for, in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct DistrFile {
    pub path: std::path::PathBuf,
    pub distributions: Vec<Distribution>,
    /// Lines whose key was not a record type, kept so a report can name them.
    pub unknown_keys: Vec<(String, usize)>,
}

impl DistrFile {
    /// Reads one `*_DISTR.ini`.
    ///
    /// # Errors
    ///
    /// [`crate::Error::Io`] when the file cannot be read. A line that does not
    /// parse is recorded in `unknown_keys` rather than failing the file, which
    /// is what SPID does.
    pub fn read(path: impl AsRef<Path>) -> crate::Result<Self> {
        let ini = Ini::read(path)?;
        Ok(Self::from_ini(&ini))
    }

    /// Reads an already parsed file.
    pub fn from_ini(ini: &Ini) -> Self {
        let mut distributions = Vec::new();
        let mut unknown_keys = Vec::new();
        for entry in ini.anonymous() {
            match parse_key(&entry.key) {
                Some((record_type, final_, on_death)) => {
                    distributions.push(parse_line(
                        record_type,
                        final_,
                        on_death,
                        &entry.value,
                        entry.line,
                    ));
                }
                None => unknown_keys.push((entry.key.clone(), entry.line)),
            }
        }
        Self {
            path: ini.path.clone(),
            distributions,
            unknown_keys,
        }
    }
}

/// Peels `Final` then `Death`, then matches the record name.
fn parse_key(key: &str) -> Option<(RecordType, bool, bool)> {
    let mut rest = key.trim();
    let final_ = match rest.strip_prefix("Final") {
        Some(r) => {
            rest = r;
            true
        }
        None => false,
    };
    let on_death = match rest.strip_prefix("Death") {
        Some(r) => {
            rest = r;
            true
        }
        None => false,
    };
    RECORD_NAMES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(rest))
        .map(|(_, t)| (*t, final_, on_death))
}

fn parse_line(
    record_type: RecordType,
    final_: bool,
    on_death: bool,
    value: &str,
    line: usize,
) -> Distribution {
    let fields: Vec<String> = value.split('|').map(|s| s.trim().to_string()).collect();
    let is_package = record_type == RecordType::Package;
    Distribution {
        record_type,
        final_,
        on_death,
        form: FormRef::parse(field(&fields, 0)),
        strings: BucketFilters::parse_strings(field(&fields, 1)),
        forms: BucketFilters::parse_forms(field(&fields, 2)),
        levels: parse_levels(field(&fields, 3)),
        traits: parse_traits(field(&fields, 4)),
        index_or_count: parse_index_or_count(field(&fields, 5), is_package),
        chance: parse_chance(field(&fields, 6)),
        line,
    }
}

fn parse_levels(text: &str) -> LevelFilters {
    let mut out = LevelFilters::default();
    if !is_valid_entry(text) {
        return out;
    }
    for token in split_list(text, ',') {
        if token.contains('(') {
            // `skill(min/max)`, with a leading `w` for a class weight.
            let is_weight = token.starts_with('w') || token.starts_with('W');
            let cleaned: String = token
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
                .collect();
            let mut parts = cleaned.split_whitespace();
            if is_weight {
                // The leading w merged into the first number's token.
                let first = parts.next().unwrap_or_default();
                let skill = first.trim_start_matches(['w', 'W']).parse::<u32>().ok();
                push_skill(&mut out.skill_weights, skill, &mut parts);
            } else {
                let skill = parts.next().and_then(|s| s.parse::<u32>().ok());
                push_skill(&mut out.skill_levels, skill, &mut parts);
            }
        } else if let Some((lo, hi)) = token.split_once('/') {
            if let (Ok(min), Ok(max)) = (lo.trim().parse(), hi.trim().parse()) {
                out.actor_level = Some(Range { min, max });
            }
        } else if let Ok(level) = token.parse() {
            out.actor_level = Some(Range::exact(level));
        }
    }
    out
}

fn push_skill<'a>(
    into: &mut Vec<SkillLevel>,
    skill: Option<u32>,
    parts: &mut impl Iterator<Item = &'a str>,
) {
    // SPID ignores a skill index of 18 or more.
    let Some(skill) = skill.filter(|s| *s < 18) else {
        return;
    };
    let Some(min) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return;
    };
    let range = match parts.next().and_then(|s| s.parse::<u32>().ok()) {
        Some(max) => Range { min, max },
        None => Range::exact(min),
    };
    into.push(SkillLevel { skill, range });
}

fn parse_traits(text: &str) -> Traits {
    let mut out = Traits::default();
    if !is_valid_entry(text) {
        return out;
    }
    for token in split_list(text, '/') {
        match token.as_str() {
            "M" | "-F" => out.female = Some(false),
            "F" | "-M" => out.female = Some(true),
            "U" => out.unique = Some(true),
            "-U" => out.unique = Some(false),
            "S" => out.summonable = Some(true),
            "-S" => out.summonable = Some(false),
            "C" => out.child = Some(true),
            "-C" => out.child = Some(false),
            "L" => out.leveled = Some(true),
            "-L" => out.leveled = Some(false),
            "T" => out.teammate = Some(true),
            "-T" => out.teammate = Some(false),
            "D" => out.starts_dead = Some(true),
            "-D" => out.starts_dead = Some(false),
            _ => {}
        }
    }
    out
}

fn parse_index_or_count(text: &str, is_package: bool) -> IndexOrCount {
    if is_package {
        return IndexOrCount::Index(if is_valid_entry(text) {
            text.parse().unwrap_or(0)
        } else {
            0
        });
    }
    if !is_valid_entry(text) {
        return IndexOrCount::Count(Range::exact(1));
    }
    if let Some((lo, hi)) = text.split_once('-') {
        if let (Ok(min), Ok(max)) = (lo.trim().parse(), hi.trim().parse()) {
            return IndexOrCount::Count(Range { min, max });
        }
    }
    IndexOrCount::Count(Range::exact(text.parse().unwrap_or(1)))
}

fn parse_chance(text: &str) -> f32 {
    if is_valid_entry(text) {
        text.parse().unwrap_or(100.0)
    } else {
        100.0
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_key, DistrFile, IndexOrCount, Range, RecordType};
    use crate::ini::Ini;

    fn parse(text: &str) -> DistrFile {
        DistrFile::from_ini(&Ini::parse("t_DISTR.ini", text))
    }

    #[test]
    fn a_key_names_a_record_type() {
        assert_eq!(parse_key("Spell"), Some((RecordType::Spell, false, false)));
        assert_eq!(parse_key("Item"), Some((RecordType::Item, false, false)));
        assert_eq!(parse_key("Nonsense"), None);
    }

    #[test]
    fn the_final_and_death_prefixes_peel_in_order() {
        assert_eq!(
            parse_key("DeathItem"),
            Some((RecordType::Item, false, true))
        );
        assert_eq!(
            parse_key("FinalOutfit"),
            Some((RecordType::Outfit, true, false))
        );
        assert_eq!(
            parse_key("FinalDeathOutfit"),
            Some((RecordType::Outfit, true, true))
        );
    }

    #[test]
    fn a_full_line_fills_every_field() {
        let f = parse("Spell = 0x12~My.esp|Bandit,-Guard|Skyrim.esm|5/20|F/U|2-4|75\n");
        let d = &f.distributions[0];
        assert_eq!(d.record_type, RecordType::Spell);
        assert_eq!(d.strings.match_any.len(), 1);
        assert_eq!(d.strings.not.len(), 1);
        assert_eq!(d.forms.match_any.len(), 1);
        assert_eq!(d.levels.actor_level, Some(Range { min: 5, max: 20 }));
        assert_eq!(d.traits.female, Some(true));
        assert_eq!(d.traits.unique, Some(true));
        assert_eq!(
            d.index_or_count,
            IndexOrCount::Count(Range { min: 2, max: 4 })
        );
        assert!((d.chance - 75.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_short_line_takes_the_documented_defaults() {
        let f = parse("Perk = MyPerk\n");
        let d = &f.distributions[0];
        assert!(d.strings.is_empty());
        assert!((d.chance - 100.0).abs() < f32::EPSILON);
        assert_eq!(d.index_or_count, IndexOrCount::Count(Range::exact(1)));
    }

    #[test]
    fn none_skips_a_field_positionally() {
        let f = parse("Item = MyItem|NONE|NONE|10\n");
        let d = &f.distributions[0];
        assert!(d.strings.is_empty());
        assert!(d.forms.is_empty());
        assert_eq!(d.levels.actor_level, Some(Range::exact(10)));
    }

    #[test]
    fn a_package_reads_field_five_as_an_index() {
        let f = parse("Package = MyPackage|||||3\n");
        assert_eq!(f.distributions[0].index_or_count, IndexOrCount::Index(3));
    }

    #[test]
    fn a_skill_bound_separates_levels_from_class_weights() {
        let f = parse("Spell = X|||6(10/50),w7(20)\n");
        let l = &f.distributions[0].levels;
        assert_eq!(l.skill_levels.len(), 1);
        assert_eq!(l.skill_levels[0].skill, 6);
        assert_eq!(l.skill_levels[0].range, Range { min: 10, max: 50 });
        assert_eq!(l.skill_weights.len(), 1);
        assert_eq!(l.skill_weights[0].skill, 7);
        assert_eq!(l.skill_weights[0].range, Range::exact(20));
    }

    #[test]
    fn an_unknown_key_is_reported_and_does_not_stop_the_file() {
        let f = parse("Spell = A\nWibble = B\nPerk = C\n");
        assert_eq!(f.distributions.len(), 2);
        assert_eq!(f.unknown_keys.len(), 1);
        assert_eq!(f.unknown_keys[0].0, "Wibble");
        assert_eq!(f.unknown_keys[0].1, 2);
    }

    #[test]
    fn every_documented_record_type_is_recognised() {
        for name in [
            "Form",
            "Spell",
            "Perk",
            "Item",
            "Shout",
            "LevSpell",
            "Package",
            "Outfit",
            "Keyword",
            "Faction",
            "SleepOutfit",
            "Skin",
        ] {
            assert!(parse_key(name).is_some(), "{name} should be a record type");
        }
    }
}
