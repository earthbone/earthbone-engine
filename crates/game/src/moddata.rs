// SPDX-License-Identifier: GPL-3.0-or-later

//! Applying the configuration formats `soe_moddata` reads.
//!
//! `crates/moddata` parses the four formats popular SKSE plugins invented and
//! resolves them against a load order. It stops there on purpose: nothing in
//! that crate touches a running world. This module is the other half, the one
//! that makes a mod carrying no code actually change the game.
//!
//! # Where each format lands
//!
//! - **Keyword Item Distributor** attaches keywords to records, so the added
//!   keywords merge into [`crate::World::keywords`] itself rather than sitting
//!   in a table only this module reads. `HasKeyword` is the same path for a
//!   record's own `KWDA` and for a distributed keyword, which is the point:
//!   a script cannot tell the difference, and neither can a native.
//! - **Spell Perk Item Distributor** hands things to actors, and an actor's
//!   state is built from its `NPC_` base on first use. So distributions are
//!   keyed by base form and applied there, which means an actor that loads
//!   after the distribution still gets what it was given.
//! - **Base Object Swapper** replaces the base object on a placed reference.
//!   A reference may not be loaded at start, so swaps are held and applied
//!   when the reference is built.
//! - **Open Animation Replacer** picks animations by condition, which is not
//!   world state at all. Its trees are counted in the report and left for the
//!   animation system.
//!
//! # Order
//!
//! This implementation distributes once at game start, before scripts run.
//! SPID's per-actor event lifecycle remains unimplemented. A swap is applied
//! when the reference is built, see above.
//!
//! # Chance
//!
//! Ordinary SPID chances use the world's seeded generator per matching NPC
//! base. SPID's trailing `!` requests a deterministic per-actor choice, whose
//! seed and actor lifecycle are not implemented. Fractional deterministic
//! rules are counted as unsupported. Zero and 100 percent remain exact.
//! KID's deterministic target seed is also not reproduced yet. See
//! `docs/moddata.md` for these compatibility limits.

use crate::world::World;
use soe_esm::{GlobalFormId, Tag};
use soe_moddata::{kid, spid, Change, ConfigKind, Patch, Subject};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// What one distribution hands to one actor base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Distributed {
    pub record_type: spid::RecordType,
    pub what: GlobalFormId,
    /// Resolved from the rule's index or count, at least one.
    pub count: i32,
    pub on_death: bool,
}

impl Distributed {
    /// A stable name for the record type, so the state hash does not depend on
    /// the enum's declaration order the way a cast to an integer would.
    #[must_use]
    pub fn record_type_name(&self) -> &'static str {
        use spid::RecordType as R;
        match self.record_type {
            R::Form => "Form",
            R::Spell => "Spell",
            R::Perk => "Perk",
            R::Item => "Item",
            R::Shout => "Shout",
            R::LevSpell => "LevSpell",
            R::Package => "Package",
            R::Outfit => "Outfit",
            R::Keyword => "Keyword",
            R::Faction => "Faction",
            R::SleepOutfit => "SleepOutfit",
            R::Skin => "Skin",
        }
    }
}

/// Everything the mod data files changed, held on the world.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModDataState {
    /// Keywords added to a form, merged by [`crate::World::keywords`].
    pub added_keywords: BTreeMap<GlobalFormId, BTreeSet<GlobalFormId>>,
    /// What each `NPC_` base was handed, applied when its actor is built.
    pub distributed: BTreeMap<GlobalFormId, Vec<Distributed>>,
    /// Base object replacements, applied when a reference is built.
    pub base_swaps: BTreeMap<GlobalFormId, GlobalFormId>,
    pub report: ModDataReport,
}

impl ModDataState {
    /// What a base was handed, empty when nothing was.
    #[must_use]
    pub fn for_base(&self, base: GlobalFormId) -> &[Distributed] {
        self.distributed.get(&base).map_or(&[], Vec::as_slice)
    }

    /// The base a reference should use, following a swap when one applies.
    #[must_use]
    pub fn swap(&self, base: GlobalFormId) -> GlobalFormId {
        self.base_swaps.get(&base).copied().unwrap_or(base)
    }
}

/// Counts for a report, so a load says what it did rather than being silent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModDataReport {
    pub files: usize,
    pub keyword_rules: usize,
    pub keywords_attached: usize,
    pub distribution_rules: usize,
    pub distributions_applied: usize,
    pub swap_rules: usize,
    pub oar_trees: usize,
    /// Rules a chance roll dropped.
    pub skipped_by_chance: usize,
    /// Rules carrying a level or trait filter, which are not evaluated yet, so
    /// the rule is skipped rather than applied to the wrong actors.
    pub skipped_unsupported_filter: usize,
    /// Deterministic fractional chances need per-actor distribution support.
    pub skipped_unsupported_chance: usize,
    /// Lines the parser could not read, reported by the parser itself.
    pub unreadable_lines: usize,
}

/// The record tag a keyword rule's item type selects.
fn tag_of(item: kid::ItemType) -> Tag {
    use kid::ItemType as I;
    let bytes: &[u8; 4] = match item {
        I::Armor => b"ARMO",
        I::Weapon => b"WEAP",
        I::Ammo => b"AMMO",
        I::MagicEffect => b"MGEF",
        I::Potion => b"ALCH",
        I::Scroll => b"SCRL",
        I::Location => b"LCTN",
        I::Ingredient => b"INGR",
        I::Book => b"BOOK",
        I::MiscItem => b"MISC",
        I::Key => b"KEYM",
        I::SoulGem => b"SLGM",
        I::Spell => b"SPEL",
        I::Activator => b"ACTI",
        I::Flora => b"FLOR",
        I::Furniture => b"FURN",
        I::Race => b"RACE",
        I::TalkingActivator => b"TACT",
        I::Enchantment => b"ENCH",
    };
    Tag::new(bytes)
}

/// The facts a filter may ask about a form, read out of the world.
fn subject_of(world: &World, id: GlobalFormId) -> Subject {
    Subject {
        id: Some(id),
        editor_id: world.editor_id(id).unwrap_or_default(),
        name: world.name_of(id).unwrap_or_default(),
        keywords: world
            .keywords(id)
            .into_iter()
            .filter_map(|k| world.editor_id(k))
            .collect(),
    }
}

fn in_range(value: u32, range: spid::Range) -> bool {
    range.min <= value && value <= range.max
}

/// Match SPID level and base traits that can be read from an NPC record.
fn matches_spid_actor_filters(world: &World, base: GlobalFormId, d: &spid::Distribution) -> bool {
    let Some(npc) = world.npc(base) else {
        return false;
    };
    let levels = &d.levels;
    let actor_level = npc.level_for(world.player_level);
    if levels
        .actor_level
        .is_some_and(|range| !in_range(actor_level, range))
    {
        return false;
    }
    let skills = npc.stats.unwrap_or_default();
    for filter in &levels.skill_levels {
        let Some(value) = skills
            .skills
            .get(filter.skill as usize)
            .zip(skills.skill_offsets.get(filter.skill as usize))
            .map(|(value, offset)| u32::from(*value) + u32::from(*offset))
        else {
            return false;
        };
        if !in_range(value, filter.range) {
            return false;
        }
    }
    if !levels.skill_weights.is_empty() {
        let Some(raw_class) = npc.class else {
            return false;
        };
        let class_id = world.order().resolve(usize::from(base.file), raw_class);
        let Some(class_record) = world.record(class_id) else {
            return false;
        };
        let Ok(Some(class)) = crate::class::Class::from_record(class_record) else {
            return false;
        };
        for filter in &levels.skill_weights {
            let Some(value) = class.skill_weights.get(filter.skill as usize) else {
                return false;
            };
            if !in_range(u32::from(*value), filter.range) {
                return false;
            }
        }
    }

    let traits = &d.traits;
    if traits
        .female
        .is_some_and(|expected| npc.is_female() != expected)
        || traits
            .unique
            .is_some_and(|expected| (npc.config.flags & crate::npc::flags::UNIQUE != 0) != expected)
        || traits.summonable.is_some_and(|expected| {
            (npc.config.flags & crate::npc::flags::SUMMONABLE != 0) != expected
        })
        || traits.leveled.is_some_and(|expected| {
            (npc.config.flags & crate::npc::flags::PC_LEVEL_MULT != 0) != expected
        })
    {
        return false;
    }
    if traits.child.is_some_and(|expected| {
        let is_child = npc
            .race
            .map(|raw| world.order().resolve(usize::from(base.file), raw))
            .and_then(|race| world.editor_id(race))
            .is_some_and(|editor_id| editor_id.contains("RaceChild"));
        is_child != expected
    }) {
        return false;
    }
    if traits.teammate.is_some_and(|expected| {
        let potential_follower_faction = GlobalFormId {
            file: 0,
            object: 0x0005_C84D,
        };
        let is_teammate = npc.factions.iter().any(|(raw, _)| {
            world.order().resolve(usize::from(base.file), *raw) == potential_follower_faction
        });
        is_teammate != expected
    }) {
        return false;
    }
    true
}

fn has_actor_only_filter(d: &spid::Distribution) -> bool {
    d.traits.starts_dead.is_some()
}

/// Reads, resolves and applies every configuration file under `roots`, lowest
/// root first so a later one shadows an earlier one, which is how a profile
/// layers its directories.
///
/// Returns what it did. A directory that cannot be listed is skipped rather
/// than failing the load, since a profile may name a root that is not there.
pub fn apply(world: &mut World, roots: &[PathBuf]) -> ModDataReport {
    let mut files: Vec<(ConfigKind, PathBuf)> = Vec::new();
    for root in roots {
        if let Ok(found) = soe_moddata::discover(root) {
            files.extend(found);
        }
    }
    // All three formats resolve a conflict by file name, last alphabetically
    // wins, so the order across roots is the name order and not the root
    // order. `discover` sorts within a root, this sorts across them.
    files.sort_by(|a, b| a.1.file_name().cmp(&b.1.file_name()));

    let mut report = ModDataReport::default();
    for (kind, path) in &files {
        match kind {
            ConfigKind::Kid => apply_kid(world, path, &mut report),
            ConfigKind::Distr => apply_spid(world, path, &mut report),
            ConfigKind::Swap => apply_bos(world, path, &mut report),
        }
    }
    report.files = files.len();
    report.oar_trees = count_oar_trees(roots);
    world.mod_data.report = report;
    report
}

/// Open Animation Replacer's submods are counted and left alone. They select
/// animations rather than changing world state, so the animation system is
/// what will read them.
fn count_oar_trees(roots: &[PathBuf]) -> usize {
    roots
        .iter()
        .filter_map(|root| soe_moddata::oar::AnimationMod::scan(root).ok())
        .map(|found| found.sub_mods.len())
        .sum()
}

fn roll(world: &mut World, chance: f32) -> bool {
    if chance >= 100.0 {
        return true;
    }
    if chance <= 0.0 {
        return false;
    }
    world.random_float(0.0, 100.0) < chance
}

fn apply_kid(world: &mut World, path: &Path, report: &mut ModDataReport) {
    let Ok(file) = kid::KidFile::read(path) else {
        return;
    };
    report.unreadable_lines += file.skipped.len();
    for rule in &file.rules {
        report.keyword_rules += 1;
        let Some(keyword) = rule.keyword.resolve(world.order()) else {
            continue;
        };
        if !roll(world, rule.chance) {
            report.skipped_by_chance += 1;
            continue;
        }
        let tag = tag_of(rule.item_type);
        let candidates: Vec<GlobalFormId> =
            world.order().winners_of(tag).map(|(id, _)| id).collect();
        for id in candidates {
            let subject = subject_of(world, id);
            if !rule.filters.matches(world.order(), &subject) {
                continue;
            }
            if world
                .mod_data
                .added_keywords
                .entry(id)
                .or_default()
                .insert(keyword)
            {
                report.keywords_attached += 1;
            }
        }
    }
}

fn apply_spid(world: &mut World, path: &Path, report: &mut ModDataReport) {
    let Ok(file) = spid::DistrFile::read(path) else {
        return;
    };
    report.unreadable_lines += file.unknown_keys.len();
    let bases: Vec<GlobalFormId> = world
        .order()
        .winners_of(Tag::new(b"NPC_"))
        .map(|(id, _)| id)
        .collect();
    for d in &file.distributions {
        report.distribution_rules += 1;
        let Some(what) = d.form.resolve(world.order()) else {
            continue;
        };
        if has_actor_only_filter(d) {
            report.skipped_unsupported_filter += 1;
            continue;
        }
        if d.deterministic_chance && d.chance > 0.0 && d.chance < 100.0 {
            report.skipped_unsupported_chance += 1;
            continue;
        }
        let count = match d.index_or_count {
            spid::IndexOrCount::Count(r) => i32::try_from(r.min.max(1)).unwrap_or(1),
            spid::IndexOrCount::Index(_) => 1,
        };
        for base in &bases {
            let subject = subject_of(world, *base);
            if !d.strings.matches(world.order(), &subject) {
                continue;
            }
            if !d.forms.matches(world.order(), &subject) {
                continue;
            }
            if !matches_spid_actor_filters(world, *base, d) {
                continue;
            }
            if !roll(world, d.chance) {
                report.skipped_by_chance += 1;
                continue;
            }
            world
                .mod_data
                .distributed
                .entry(*base)
                .or_default()
                .push(Distributed {
                    record_type: d.record_type,
                    what,
                    count,
                    on_death: d.on_death,
                });
            report.distributions_applied += 1;
        }
    }
}

fn apply_bos(world: &mut World, path: &Path, report: &mut ModDataReport) {
    let Ok(file) = soe_moddata::bos::SwapFile::read(path) else {
        return;
    };
    let patches: Vec<Patch> = soe_moddata::resolve_bos(&file, world.order());
    for patch in patches {
        report.swap_rules += 1;
        let Some(target) = patch.target else {
            continue;
        };
        if !roll(world, patch.chance) {
            report.skipped_by_chance += 1;
            continue;
        }
        // A property override keeps its text, see the moddata crate docs, so
        // only a base replacement changes world state here.
        if let Change::SwapBase { to } = patch.change {
            world.mod_data.base_swaps.insert(target, to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{tag_of, Distributed, ModDataState};
    use soe_esm::GlobalFormId;
    use soe_moddata::kid::ItemType;
    use soe_moddata::spid::RecordType;

    fn id(object: u32) -> GlobalFormId {
        GlobalFormId { file: 0, object }
    }

    #[test]
    fn every_item_type_names_a_record_tag() {
        // A missing arm would silently select nothing, so this walks them all.
        for item in [
            ItemType::Armor,
            ItemType::Weapon,
            ItemType::Ammo,
            ItemType::MagicEffect,
            ItemType::Potion,
            ItemType::Scroll,
            ItemType::Location,
            ItemType::Ingredient,
            ItemType::Book,
            ItemType::MiscItem,
            ItemType::Key,
            ItemType::SoulGem,
            ItemType::Spell,
            ItemType::Activator,
            ItemType::Flora,
            ItemType::Furniture,
            ItemType::Race,
            ItemType::TalkingActivator,
            ItemType::Enchantment,
        ] {
            let tag = tag_of(item);
            assert_eq!(tag.0.len(), 4, "{item:?} produced a short tag");
        }
    }

    #[test]
    fn a_swap_falls_through_when_nothing_matches() {
        let mut state = ModDataState::default();
        assert_eq!(state.swap(id(7)), id(7));
        state.base_swaps.insert(id(7), id(9));
        assert_eq!(state.swap(id(7)), id(9));
    }

    #[test]
    fn a_base_with_no_distribution_reads_as_empty_rather_than_absent() {
        let mut state = ModDataState::default();
        assert!(state.for_base(id(1)).is_empty());
        state.distributed.insert(
            id(1),
            vec![Distributed {
                record_type: RecordType::Spell,
                what: id(2),
                count: 1,
                on_death: false,
            }],
        );
        assert_eq!(state.for_base(id(1)).len(), 1);
    }
}
