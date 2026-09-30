// SPDX-License-Identifier: GPL-3.0-or-later

//! Keyword Item Distributor, `*_KID.ini`.
//!
//! Every line uses the same key, `Keyword`, and the value carries five fields
//! in the order `LookupConfigs.cpp` reads them:
//!
//! ```text
//! Keyword = KeywordFormOrEditorID|Type|Filters|Traits|Chance
//! ```
//!
//! The type names the kind of record the keyword is attached to, and it comes
//! from a fixed table in `EnumDefs.h`. Unlike Spell Perk Item Distributor the
//! filter field is grouped rather than bucketed, see
//! [`crate::filter::GroupedFilters`].
//!
//! Traits are per type and their grammar differs for each, so this crate keeps
//! the field as written and resolves nothing. That is honest rather than
//! lossy: an armour trait and a weapon trait share no syntax.

use crate::filter::GroupedFilters;
use crate::form::FormRef;
use crate::ini::{field, is_valid_entry, Ini};
use std::path::Path;

/// The record kinds a keyword can be attached to, from `header_strings`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Armor,
    Weapon,
    Ammo,
    MagicEffect,
    Potion,
    Scroll,
    Location,
    Ingredient,
    Book,
    MiscItem,
    Key,
    SoulGem,
    Spell,
    Activator,
    Flora,
    Furniture,
    Race,
    TalkingActivator,
    Enchantment,
}

/// The spellings `DISTRIBUTION::header_strings` accepts, in enum order.
const TYPE_NAMES: [(&str, ItemType); 19] = [
    ("Armor", ItemType::Armor),
    ("Weapon", ItemType::Weapon),
    ("Ammo", ItemType::Ammo),
    ("Magic Effect", ItemType::MagicEffect),
    ("Potion", ItemType::Potion),
    ("Scroll", ItemType::Scroll),
    ("Location", ItemType::Location),
    ("Ingredient", ItemType::Ingredient),
    ("Book", ItemType::Book),
    ("Misc Item", ItemType::MiscItem),
    ("Key", ItemType::Key),
    ("Soul Gem", ItemType::SoulGem),
    ("Spell", ItemType::Spell),
    ("Activator", ItemType::Activator),
    ("Flora", ItemType::Flora),
    ("Furniture", ItemType::Furniture),
    ("Race", ItemType::Race),
    ("Talking Activator", ItemType::TalkingActivator),
    ("Enchantment", ItemType::Enchantment),
];

/// One `Keyword = ...` line.
#[derive(Debug, Clone, PartialEq)]
pub struct KeywordRule {
    /// The keyword to attach.
    pub keyword: FormRef,
    pub item_type: ItemType,
    pub filters: GroupedFilters,
    /// Kept as written, since the grammar is per item type.
    pub traits: String,
    /// Percent, 0 to 100.
    pub chance: f32,
    pub line: usize,
}

/// A named set of keywords that cannot be added to the same record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExclusiveGroup {
    /// The group name, shared across all KID files.
    pub name: String,
    /// Keywords included by this line.
    pub include: Vec<FormRef>,
    /// Keywords removed from this group by this line.
    pub exclude: Vec<FormRef>,
    /// The source line.
    pub line: usize,
}

/// Every keyword one file attaches, in file order.
#[derive(Debug, Clone, PartialEq)]
pub struct KidFile {
    pub path: std::path::PathBuf,
    pub rules: Vec<KeywordRule>,
    /// Exclusive group declarations, in file order.
    pub exclusive_groups: Vec<ExclusiveGroup>,
    /// Lines whose type field named nothing, which KID skips silently.
    pub skipped: Vec<(String, usize)>,
}

impl KidFile {
    /// Reads one `*_KID.ini`.
    ///
    /// # Errors
    ///
    /// [`crate::Error::Io`] when the file cannot be read.
    pub fn read(path: impl AsRef<Path>) -> crate::Result<Self> {
        let ini = Ini::read(path)?;
        Ok(Self::from_ini(&ini))
    }

    /// Reads an already parsed file.
    pub fn from_ini(ini: &Ini) -> Self {
        let mut rules = Vec::new();
        let mut exclusive_groups = Vec::new();
        let mut skipped = Vec::new();
        for entry in ini.anonymous() {
            if entry.key == "ExclusiveGroup" {
                let mut fields = entry.value.split('|');
                let name = fields.next().unwrap_or_default().trim();
                let filters = fields.next().unwrap_or_default();
                let (include, exclude): (Vec<_>, Vec<_>) = filters
                    .split(',')
                    .map(str::trim)
                    .filter(|filter| !filter.is_empty())
                    .map(|filter| {
                        if let Some(excluded) = filter.strip_prefix('-') {
                            (None, Some(FormRef::parse(excluded.trim())))
                        } else {
                            (Some(FormRef::parse(filter)), None)
                        }
                    })
                    .fold((Vec::new(), Vec::new()), |(mut yes, mut no), (a, b)| {
                        yes.extend(a);
                        no.extend(b);
                        (yes, no)
                    });
                if name.is_empty() || include.is_empty() && exclude.is_empty() {
                    skipped.push((entry.value.clone(), entry.line));
                } else {
                    exclusive_groups.push(ExclusiveGroup {
                        name: name.to_string(),
                        include,
                        exclude,
                        line: entry.line,
                    });
                }
                continue;
            }
            if !entry.key.eq_ignore_ascii_case("Keyword") {
                skipped.push((entry.key.clone(), entry.line));
                continue;
            }
            let fields: Vec<String> = entry
                .value
                .split('|')
                .map(|s| s.trim().to_string())
                .collect();
            let Some(item_type) = parse_type(field(&fields, 1)) else {
                skipped.push((entry.value.clone(), entry.line));
                continue;
            };
            rules.push(KeywordRule {
                keyword: FormRef::parse(field(&fields, 0)),
                item_type,
                filters: GroupedFilters::parse(field(&fields, 2)),
                traits: field(&fields, 3).to_string(),
                chance: parse_chance(field(&fields, 4)),
                line: entry.line,
            });
        }
        Self {
            path: ini.path.clone(),
            rules,
            exclusive_groups,
            skipped,
        }
    }
}

fn parse_type(text: &str) -> Option<ItemType> {
    let t = text.trim();
    TYPE_NAMES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(t))
        .map(|(_, v)| *v)
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
    use super::{parse_type, ItemType, KidFile};
    use crate::ini::Ini;

    fn parse(text: &str) -> KidFile {
        KidFile::from_ini(&Ini::parse("t_KID.ini", text))
    }

    #[test]
    fn a_line_fills_every_field() {
        let f = parse("Keyword = MyKeyword|Armor|Iron+*Helm,-Daedric|traits|50\n");
        let r = &f.rules[0];
        assert_eq!(r.item_type, ItemType::Armor);
        assert_eq!(r.filters.all.len(), 1);
        assert_eq!(r.filters.any.len(), 1);
        assert_eq!(r.traits, "traits");
        assert!((r.chance - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn a_two_word_type_name_is_matched_whole() {
        assert_eq!(parse_type("Magic Effect"), Some(ItemType::MagicEffect));
        assert_eq!(parse_type("Soul Gem"), Some(ItemType::SoulGem));
        assert_eq!(
            parse_type("Talking Activator"),
            Some(ItemType::TalkingActivator)
        );
        assert_eq!(parse_type("Nonsense"), None);
    }

    #[test]
    fn a_line_with_no_type_is_skipped_and_reported() {
        let f = parse("Keyword = A|Armor\nKeyword = B|Nonsense\n");
        assert_eq!(f.rules.len(), 1);
        assert_eq!(f.skipped.len(), 1);
        assert_eq!(f.skipped[0].1, 2);
    }

    #[test]
    fn a_line_with_another_key_is_kept_apart() {
        let f = parse("Keyword = A|Armor\nExclusiveGroup = G|x,y\n");
        assert_eq!(f.rules.len(), 1);
        assert!(f.skipped.is_empty());
        assert_eq!(f.exclusive_groups[0].name, "G");
        assert_eq!(f.exclusive_groups[0].include.len(), 2);
    }

    #[test]
    fn a_group_exclusion_removes_a_keyword_from_its_named_set() {
        let f = parse("ExclusiveGroup = G|A,-B\n");
        assert_eq!(f.exclusive_groups[0].include.len(), 1);
        assert_eq!(f.exclusive_groups[0].exclude.len(), 1);
        assert!(f.skipped.is_empty());
    }

    #[test]
    fn a_missing_chance_defaults_to_certain() {
        let f = parse("Keyword = A|Weapon|Iron\n");
        assert!((f.rules[0].chance - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn every_documented_type_is_recognised() {
        for name in [
            "Armor",
            "Weapon",
            "Ammo",
            "Magic Effect",
            "Potion",
            "Scroll",
            "Location",
            "Ingredient",
            "Book",
            "Misc Item",
            "Key",
            "Soul Gem",
            "Spell",
            "Activator",
            "Flora",
            "Furniture",
            "Race",
            "Talking Activator",
            "Enchantment",
        ] {
            assert!(parse_type(name).is_some(), "{name} should be a type");
        }
    }
}
