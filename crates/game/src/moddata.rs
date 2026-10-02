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
//!   world state at all. Its submods are retained by `Game` for runtime path
//!   selection and counted in the report.
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
//! base. SPID's trailing `!` requests a deterministic per-actor choice. Those
//! rules are evaluated when an actor is built from its reference ID and are
//! keyed from the entry value, save character ID and actor runtime ID. The
//! host supplies the current save character ID through `World`; fractional
//! deterministic chances use SPID's seeded CLibUtil Xoshiro256** draw.
//! Zero and 100 percent remain exact.
//! KID chance uses its keyword editor ID and target FormID as a deterministic
//! seed. Exclusive groups prevent adding a keyword when another group member
//! is already present on the record.

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
    /// Zero-based NPC package-list insertion position for package rules.
    pub package_index: Option<usize>,
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
    /// Fractional deterministic SPID rules, evaluated for each actor instance.
    pub actor_rules: Vec<ActorDistribution>,
    /// Base object replacements, applied when a reference is built.
    pub base_swaps: BTreeMap<GlobalFormId, GlobalFormId>,
    /// Per-reference chance for a base swap when BOS supplies a chance call.
    pub base_swap_chances: BTreeMap<GlobalFormId, soe_moddata::bos::PropertyChance>,
    /// Replacements for explicitly named references, before base swaps.
    pub reference_swaps: BTreeMap<GlobalFormId, GlobalFormId>,
    /// Per-reference chance for an explicit reference swap.
    pub reference_swap_chances: BTreeMap<GlobalFormId, soe_moddata::bos::PropertyChance>,
    /// BOS property overrides, applied when a reference is built.
    pub properties: BTreeMap<GlobalFormId, soe_moddata::bos::Properties>,
    pub report: ModDataReport,
}

/// A resolved SPID distribution whose chance depends on an actor reference.
#[derive(Debug, Clone, PartialEq)]
pub struct ActorDistribution {
    /// NPC base selected by the rule filters.
    pub base: GlobalFormId,
    /// Resolved form handed to the actor.
    pub what: GlobalFormId,
    /// Consumer selected from the explicit type or resolved record signature.
    pub record_type: spid::RecordType,
    /// Parsed rule, including its per-entry chance seed.
    pub distribution: spid::Distribution,
    /// Resolved item count, at least one.
    pub count: i32,
    /// Zero-based NPC package-list insertion position for package rules.
    pub package_index: Option<usize>,
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
    /// Chance checks that rejected a candidate.
    pub skipped_by_chance: usize,
    /// Rules carrying a level or trait filter, which are not evaluated yet, so
    /// the rule is skipped rather than applied to the wrong actors.
    pub skipped_unsupported_filter: usize,
    /// Rules skipped because their chance mode is unsupported.
    pub skipped_unsupported_chance: usize,
    /// Rules whose record type has no runtime consumer, or whose generic
    /// `Form` signature cannot be inferred to a supported type.
    pub skipped_unsupported_consumer: usize,
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

/// Distributions applicable to one actor reference, including stable SPID
/// chances whose result depends on its runtime FormID.
pub(crate) fn for_actor(
    world: &World,
    actor: GlobalFormId,
    base: GlobalFormId,
) -> Vec<Distributed> {
    let mut result = world.mod_data.for_base(base).to_vec();
    let subject = subject_of(world, base);
    let player = world.player_character_id();
    let actor_id = u64::from(world.order().runtime_id(actor));
    for rule in &world.mod_data.actor_rules {
        if rule.base != base
            || !rule.distribution.strings.matches(world.order(), &subject)
            || !rule.distribution.forms.matches(world.order(), &subject)
            || !deterministic_roll(
                rule.distribution.line_seed,
                player,
                actor_id,
                rule.distribution.chance,
            )
        {
            continue;
        }
        result.push(Distributed {
            record_type: rule.record_type,
            what: rule.what,
            count: rule.count,
            package_index: rule.package_index,
            on_death: rule.distribution.on_death,
        });
    }
    result
}

fn deterministic_roll(line_seed: u64, player: u64, actor: u64, chance: f32) -> bool {
    if chance >= 100.0 {
        return true;
    }
    if chance <= 0.0 {
        return false;
    }
    let mut seed = line_seed;
    for value in [player, actor] {
        seed ^= value
            .wrapping_add(0x9e37_79b9)
            .wrapping_add(seed << 6)
            .wrapping_add(seed >> 2);
    }
    let mut random = SpidRng::new(seed);
    let unit = (random.next() >> 11) as f64 / (1_u64 << 53) as f64;
    unit * 100.0 <= f64::from(chance)
}

/// CLibUtil's `RNG(seed).generate()` uses Xoshiro256** seeded by SplitMix64.
/// The generator is small enough to keep the source-defined deterministic path
/// independent of platform RNG implementations.
struct SpidRng([u64; 4]);

impl SpidRng {
    fn new(seed: u64) -> Self {
        let mut state = seed;
        let mut words = [0; 4];
        for word in &mut words {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut value = state;
            value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            *word = value ^ (value >> 31);
        }
        Self(words)
    }

    fn next(&mut self) -> u64 {
        let result = self.0[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let shifted = self.0[1] << 17;
        self.0[2] ^= self.0[0];
        self.0[3] ^= self.0[1];
        self.0[1] ^= self.0[2];
        self.0[0] ^= self.0[3];
        self.0[2] ^= shifted;
        self.0[3] = self.0[3].rotate_left(45);
        result
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

/// Resolve SPID's generic `Form` type from the winning record signature.
/// `Skin` and `SleepOutfit` cannot be inferred from signatures, as SPID's
/// reference documents, because their signatures overlap `Item` and `Outfit`.
fn inferred_record_type(world: &World, form: GlobalFormId) -> Option<spid::RecordType> {
    let tag = world.tag_of(form)?;
    Some(match &tag.0 {
        b"SPEL" => spid::RecordType::Spell,
        b"LVSP" => spid::RecordType::LevSpell,
        b"PERK" => spid::RecordType::Perk,
        b"SHOU" => spid::RecordType::Shout,
        b"PACK" | b"FLST" => spid::RecordType::Package,
        b"OTFT" => spid::RecordType::Outfit,
        b"KYWD" => spid::RecordType::Keyword,
        b"FACT" => spid::RecordType::Faction,
        b"ALCH" | b"AMMO" | b"ARMO" | b"BOOK" | b"INGR" | b"KEYM" | b"LVLI" | b"MISC" | b"SCRL"
        | b"SLGM" | b"WEAP" => spid::RecordType::Item,
        _ => return None,
    })
}

/// Check that an explicit SPID type names a record signature in its schema.
fn record_type_matches(world: &World, record_type: spid::RecordType, form: GlobalFormId) -> bool {
    let Some(tag) = world.tag_of(form) else {
        return false;
    };
    match record_type {
        spid::RecordType::Form => inferred_record_type(world, form).is_some(),
        spid::RecordType::Spell => matches!(&tag.0, b"SPEL" | b"LVSP"),
        spid::RecordType::Perk => tag.0 == *b"PERK",
        spid::RecordType::Item => matches!(
            &tag.0,
            b"ALCH"
                | b"AMMO"
                | b"ARMO"
                | b"BOOK"
                | b"INGR"
                | b"KEYM"
                | b"LVLI"
                | b"MISC"
                | b"SCRL"
                | b"SLGM"
                | b"WEAP"
        ),
        spid::RecordType::Shout => tag.0 == *b"SHOU",
        spid::RecordType::LevSpell => tag.0 == *b"LVSP",
        spid::RecordType::Package => matches!(&tag.0, b"PACK" | b"FLST"),
        spid::RecordType::Outfit | spid::RecordType::SleepOutfit => tag.0 == *b"OTFT",
        spid::RecordType::Keyword => tag.0 == *b"KYWD",
        spid::RecordType::Faction => tag.0 == *b"FACT",
        spid::RecordType::Skin => tag.0 == *b"ARMO",
    }
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
    let kid_exclusions = kid_exclusions(world, &files);
    for (kind, path) in &files {
        match kind {
            ConfigKind::Kid => apply_kid(world, path, &kid_exclusions, &mut report),
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
    world.random_float_for(crate::world::RngStream::ModData, 0.0, 100.0) < chance
}

fn kid_exclusions(
    world: &World,
    files: &[(ConfigKind, PathBuf)],
) -> BTreeMap<GlobalFormId, BTreeSet<GlobalFormId>> {
    let mut groups: BTreeMap<String, BTreeSet<GlobalFormId>> = BTreeMap::new();
    for (_, path) in files.iter().filter(|(kind, _)| *kind == ConfigKind::Kid) {
        let Ok(file) = kid::KidFile::read(path) else {
            continue;
        };
        for group in file.exclusive_groups {
            let keywords = groups.entry(group.name).or_default();
            for form in group.include {
                if let Some(id) = form.resolve(world.order()) {
                    if world.tag_of(id) == Some(Tag::new(b"KYWD")) {
                        keywords.insert(id);
                    }
                }
            }
            for form in group.exclude {
                if let Some(id) = form.resolve(world.order()) {
                    if world.tag_of(id) == Some(Tag::new(b"KYWD")) {
                        keywords.remove(&id);
                    }
                }
            }
        }
    }
    let mut exclusions = BTreeMap::<GlobalFormId, BTreeSet<GlobalFormId>>::new();
    for members in groups.values() {
        for keyword in members {
            exclusions
                .entry(*keyword)
                .or_default()
                .extend(members.iter().copied().filter(|member| member != keyword));
        }
    }
    exclusions
}

fn apply_kid(
    world: &mut World,
    path: &Path,
    exclusions: &BTreeMap<GlobalFormId, BTreeSet<GlobalFormId>>,
    report: &mut ModDataReport,
) {
    let Ok(file) = kid::KidFile::read(path) else {
        return;
    };
    report.unreadable_lines += file.skipped.len();
    for rule in &file.rules {
        report.keyword_rules += 1;
        let Some(keyword) = rule.keyword.resolve(world.order()) else {
            continue;
        };
        let tag = tag_of(rule.item_type);
        let candidates: Vec<GlobalFormId> =
            world.order().winners_of(tag).map(|(id, _)| id).collect();
        for id in candidates {
            if !kid_chance_passes(world, keyword, id, rule.chance) {
                report.skipped_by_chance += 1;
                continue;
            }
            let subject = subject_of(world, id);
            if !rule.filters.matches(world.order(), &subject) {
                continue;
            }
            if exclusions.get(&keyword).is_some_and(|excluded| {
                world
                    .keywords(id)
                    .iter()
                    .any(|item| excluded.contains(item))
            }) {
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

fn kid_chance_passes(
    world: &World,
    keyword: GlobalFormId,
    target: GlobalFormId,
    chance: f32,
) -> bool {
    if chance >= 100.0 {
        return true;
    }
    if chance <= 0.0 {
        return false;
    }
    let item_form_id = world.runtime_id(target);
    let editor_id = world.editor_id(keyword).unwrap_or_default();
    let seed = kid_seed(&editor_id, item_form_id);
    let mut random = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    random = (random ^ (random >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    random = (random ^ (random >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    random ^= random >> 31;
    let unit = (random >> 11) as f64 / (1_u64 << 53) as f64;
    unit <= f64::from(chance / 100.0)
}

fn kid_seed(keyword_editor_id: &str, item_form_id: u32) -> u64 {
    let keyword_hash = keyword_editor_id
        .bytes()
        .fold(0x811C_9DC5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
    szudzik_pair(u64::from(keyword_hash), u64::from(item_form_id))
}

fn szudzik_pair(left: u64, right: u64) -> u64 {
    if left >= right {
        left.wrapping_mul(left)
            .wrapping_add(left)
            .wrapping_add(right)
    } else {
        left.wrapping_add(right.wrapping_mul(right))
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
        let record_type = match d.record_type {
            spid::RecordType::Form => inferred_record_type(world, what),
            record_type => Some(record_type),
        };
        let Some(record_type) = record_type else {
            report.skipped_unsupported_consumer += 1;
            continue;
        };
        if !record_type_matches(world, record_type, what) {
            report.skipped_unsupported_consumer += 1;
            continue;
        }
        if has_actor_only_filter(d) {
            report.skipped_unsupported_filter += 1;
            continue;
        }
        let (count, package_index) = match d.index_or_count {
            spid::IndexOrCount::Count(r) => (i32::try_from(r.min.max(1)).unwrap_or(1), None),
            spid::IndexOrCount::Index(index) => (1, Some(usize::try_from(index).unwrap_or(0))),
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
            if d.deterministic_chance && d.chance > 0.0 && d.chance < 100.0 {
                world.mod_data.actor_rules.push(ActorDistribution {
                    base: *base,
                    what,
                    record_type,
                    distribution: d.clone(),
                    count,
                    package_index,
                });
                continue;
            }
            if !roll(world, d.chance) {
                report.skipped_by_chance += 1;
                continue;
            }
            if record_type == spid::RecordType::Keyword {
                if world
                    .mod_data
                    .added_keywords
                    .entry(*base)
                    .or_default()
                    .insert(what)
                {
                    report.keywords_attached += 1;
                }
                report.distributions_applied += 1;
                continue;
            }
            world
                .mod_data
                .distributed
                .entry(*base)
                .or_default()
                .push(Distributed {
                    record_type,
                    what,
                    count,
                    package_index,
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
        match patch.change {
            Change::SwapBase { to, properties } => {
                world.mod_data.base_swaps.insert(target, to);
                let parsed = soe_moddata::bos::Properties::parse(&properties);
                if let Some(chance) = parsed.chance {
                    world.mod_data.base_swap_chances.insert(target, chance);
                } else {
                    world.mod_data.base_swap_chances.remove(&target);
                }
                if parsed != soe_moddata::bos::Properties::default() {
                    world.mod_data.properties.insert(target, parsed);
                }
            }
            Change::SwapReference { to, properties } => {
                world.mod_data.reference_swaps.insert(target, to);
                let parsed = soe_moddata::bos::Properties::parse(&properties);
                if let Some(chance) = parsed.chance {
                    world.mod_data.reference_swap_chances.insert(target, chance);
                } else {
                    world.mod_data.reference_swap_chances.remove(&target);
                }
                if parsed != soe_moddata::bos::Properties::default() {
                    world.mod_data.properties.insert(target, parsed);
                }
            }
            Change::SetProperties { properties } => {
                let parsed = soe_moddata::bos::Properties::parse(&properties);
                if parsed != soe_moddata::bos::Properties::default() {
                    world.mod_data.properties.insert(target, parsed);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{kid_seed, tag_of, Distributed, ModDataState};
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
                package_index: None,
                on_death: false,
            }],
        );
        assert_eq!(state.for_base(id(1)).len(), 1);
    }

    #[test]
    fn mod_02_kid_seed_uses_upstream_fnv_and_szudzik_inputs() {
        assert_eq!(kid_seed("MyKeyword", 0x803), 136_734_229_902_520_693);
    }
}
