// SPDX-License-Identifier: GPL-3.0-or-later

//! A load order and the form table it produces.
//!
//! A FormID written in a file is `index:object`, where the top byte indexes
//! that file's master list and the low 24 bits name the object. The same
//! object is written with different top bytes in different files, so the
//! table is keyed by a `GlobalFormId` that names the owning file by its load
//! order position instead. Every file that carries a record for a global id
//! is an override, and the last one in load order wins, which is the rule
//! Mutagen's cheat sheet and xEdit's conflict view both describe.
//!
//! A light plugin (the `ESL` header flag) does not get a top-byte slot of its
//! own at runtime. The game packs it as `FE:sss:ooo`, a twelve-bit light slot
//! and a twelve-bit object id. `runtime_id` produces that form, the table
//! itself never needs it. Created forms use a separate internal namespace and
//! pack with the `FF` prefix documented by UESP Save File Format. The masks
//! follow Mutagen's Plugins/FormID.cs and docs/plugins/Compaction.md.

use std::collections::HashMap;
use std::sync::OnceLock;

use sha2::{Digest, Sha256};

use crate::cell_index::CellIndex;
use crate::error::{Error, Result};
use crate::plugin::{Plugin, Record};
use crate::tag::Tag;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlobalFormId {
    /// Position of the owning file in the load order, or [`CREATED_FILE`].
    pub file: u16,
    /// The 24-bit object id as written in the file.
    pub object: u32,
}

impl std::fmt::Display for GlobalFormId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{:06X}", self.file, self.object)
    }
}

/// The slot a file occupies in the game's 32-bit FormID space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Regular(u8),
    Light(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordRef {
    pub file: usize,
    pub record: usize,
}

pub struct LoadedFile {
    pub name: String,
    pub plugin: Plugin,
    /// SHA-256 of the serialized definition before load-order additions.
    pub fingerprint: [u8; 32],
    pub slot: Slot,
    /// For each master in the header, its position in the load order.
    pub master_positions: Vec<usize>,
}

#[derive(Default)]
pub struct LoadOrder {
    files: Vec<LoadedFile>,
    table: HashMap<GlobalFormId, Vec<RecordRef>>,
    regular_slots: u8,
    light_slots: u16,
    /// Built on request, dropped whenever a file is added since every
    /// placement may change.
    cell_index: OnceLock<CellIndex>,
    /// Every winner in id order, built on the first walk and dropped when a
    /// file is added. The table is a `HashMap`, so walking it directly gave
    /// a different order per process and `Game::start` inherited it, see
    /// `docs/validation.md`, "Determinism".
    sorted: OnceLock<Vec<(GlobalFormId, RecordRef)>>,
}

pub const MAX_REGULAR_SLOTS: u8 = 0xFE;
pub const MAX_LIGHT_SLOTS: u16 = 0x1000;

/// Internal namespace for created forms. All 254 regular and 4096 light
/// plugins fit below it. This is not the runtime `FF` prefix.
pub const CREATED_FILE: u16 = u16::MAX;

impl LoadOrder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a file at the end of the order. Its masters must already be
    /// loaded, matched by name without regard to case.
    pub fn add(&mut self, name: &str, mut plugin: Plugin) -> Result<usize> {
        let _span =
            soe_trace::Span::with_payload("esm.load_order_add", "esm", plugin.records.len() as u64);
        let fingerprint = plugin.fingerprint();
        // CONTEXT: xEdit Core/Hardcoded/Skyrim.esp defines this engine-owned
        // global, absent from the shipped master but present in ESS globals.
        if self.files.is_empty()
            && name.eq_ignore_ascii_case("Skyrim.esm")
            && !plugin.header.is_light()
            && plugin.header.masters.is_empty()
            && !plugin
                .records
                .iter()
                .any(|record| record.form_id & 0x00FF_FFFF == 0x63)
        {
            plugin.push_top(
                Record::new(Tag(*b"GLOB"), 0x63)
                    .zsub(Tag(*b"EDID"), "PlayCredits")
                    .sub(Tag(*b"FNAM"), *b"s")
                    .sub(Tag(*b"FLTV"), 0_f32.to_le_bytes()),
            );
        }
        let mut master_positions = Vec::with_capacity(plugin.header.masters.len());
        for m in &plugin.header.masters {
            let pos = self
                .files
                .iter()
                .position(|f| f.name.eq_ignore_ascii_case(m))
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "{name} needs master {m}, which is not loaded before it"
                    ))
                })?;
            master_positions.push(pos);
        }
        let slot = if plugin.header.is_light() {
            if self.light_slots >= MAX_LIGHT_SLOTS {
                return Err(Error::Invalid(format!("{name}: no light slots left")));
            }
            let s = Slot::Light(self.light_slots);
            self.light_slots += 1;
            s
        } else {
            if self.regular_slots >= MAX_REGULAR_SLOTS {
                return Err(Error::Invalid(format!("{name}: no regular slots left")));
            }
            let s = Slot::Regular(self.regular_slots);
            self.regular_slots += 1;
            s
        };
        let file_index = self.files.len();
        let file = LoadedFile {
            name: name.to_string(),
            plugin,
            fingerprint,
            slot,
            master_positions,
        };
        for (i, rec) in file.plugin.records.iter().enumerate() {
            let id = resolve_in(&file, file_index, rec.form_id);
            self.table.entry(id).or_default().push(RecordRef {
                file: file_index,
                record: i,
            });
        }
        self.files.push(file);
        self.cell_index.take();
        self.sorted = OnceLock::new();
        Ok(file_index)
    }

    /// Build the cell index once over the current order. Adding a plugin invalidates it.
    /// Shared host and game readers retain the same immutable index.
    pub fn build_cell_index(&self) -> &CellIndex {
        self.cell_index.get_or_init(|| CellIndex::build(self))
    }

    /// The index built by the last `build_cell_index`, if no file was added
    /// since.
    pub fn cell_index(&self) -> Option<&CellIndex> {
        self.cell_index.get()
    }

    pub fn files(&self) -> &[LoadedFile] {
        &self.files
    }

    /// Fingerprint the ordered plugin names and their serialized contents.
    /// The result changes when a plugin is added, removed, reordered, renamed,
    /// or its bytes change.
    #[must_use]
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        for file in &self.files {
            let name = file.name.to_ascii_lowercase();
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update(file.fingerprint);
        }
        hash.finalize().into()
    }

    pub fn file(&self, index: usize) -> &LoadedFile {
        &self.files[index]
    }

    pub fn position_of(&self, name: &str) -> Option<usize> {
        self.files
            .iter()
            .position(|f| f.name.eq_ignore_ascii_case(name))
    }

    /// Resolve a FormID as written in `file` to its global id.
    pub fn resolve(&self, file: usize, raw: u32) -> GlobalFormId {
        resolve_in(&self.files[file], file, raw)
    }

    /// The 32-bit FormID the game would use for this id under this order.
    pub fn runtime_id(&self, id: GlobalFormId) -> u32 {
        if id.file == CREATED_FILE {
            return 0xFF00_0000 | (id.object & 0x00FF_FFFF);
        }
        match self.files[id.file as usize].slot {
            Slot::Regular(s) => (u32::from(s) << 24) | (id.object & 0x00FF_FFFF),
            Slot::Light(s) => 0xFE00_0000 | (u32::from(s) << 12) | (id.object & 0xFFF),
        }
    }

    /// Every record for the id, in load order. Empty when no file has it.
    pub fn overrides(&self, id: GlobalFormId) -> &[RecordRef] {
        self.table.get(&id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The winning record, the last one in load order.
    pub fn winner(&self, id: GlobalFormId) -> Option<&Record> {
        soe_trace::count(soe_trace::Operation::RecordLookups, 1);
        self.overrides(id)
            .last()
            .map(|r| &self.files[r.file].plugin.records[r.record])
    }

    pub fn winner_ref(&self, id: GlobalFormId) -> Option<RecordRef> {
        self.overrides(id).last().copied()
    }

    pub fn record(&self, r: RecordRef) -> &Record {
        soe_trace::count(soe_trace::Operation::RecordLookups, 1);
        &self.files[r.file].plugin.records[r.record]
    }

    /// Whether any file defines the id at all.
    pub fn contains(&self, id: GlobalFormId) -> bool {
        self.table.contains_key(&id)
    }

    pub fn len(&self) -> usize {
        self.table.len()
    }

    pub fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    /// The winners in id order, owning file position first, then object.
    /// One sort per load order, paid by the first walk.
    fn sorted(&self) -> &[(GlobalFormId, RecordRef)] {
        self.sorted.get_or_init(|| {
            let _span =
                soe_trace::Span::with_payload("esm.sort_winners", "esm", self.table.len() as u64);
            let mut all: Vec<(GlobalFormId, RecordRef)> = self
                .table
                .iter()
                .filter_map(|(id, refs)| Some((*id, *refs.last()?)))
                .collect();
            all.sort_unstable_by_key(|(id, _)| *id);
            all
        })
    }

    /// Every id with its winning record reference, in id order: the owning
    /// file's load order position first, then the object id.
    pub fn winners(&self) -> impl Iterator<Item = (GlobalFormId, RecordRef)> + '_ {
        self.sorted().iter().copied()
    }

    /// Winning records of one signature, in the same id order as
    /// [`LoadOrder::winners`].
    pub fn winners_of(&self, tag: Tag) -> impl Iterator<Item = (GlobalFormId, &Record)> {
        self.sorted().iter().filter_map(move |(id, rref)| {
            let r = self.record(*rref);
            (r.tag == tag).then_some((*id, r))
        })
    }

    /// Ids whose winning record does not come from the file that owns the
    /// id, which is what "overridden" means in the conflict sense. Id order.
    pub fn overridden(&self) -> impl Iterator<Item = GlobalFormId> + '_ {
        self.sorted()
            .iter()
            .filter_map(|(id, last)| (last.file != id.file as usize).then_some(*id))
    }
}

fn resolve_in(file: &LoadedFile, file_index: usize, raw: u32) -> GlobalFormId {
    let index = (raw >> 24) as usize;
    let object = raw & 0x00FF_FFFF;
    // An index past the master list names the file itself, however large it
    // is. The Creation Kit writes the file's own records with exactly
    // masters.len(), other tools sometimes write 0xFF.
    let owner = match file.master_positions.get(index) {
        Some(pos) => *pos,
        None => file_index,
    };
    GlobalFormId {
        file: owner as u16,
        object,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::{flags, FileHeader, Record};
    use crate::tag;

    fn master(name: &str, light: bool) -> Plugin {
        let mut p = Plugin::new(FileHeader {
            flags: flags::MASTER | if light { flags::LIGHT_MASTER } else { 0 },
            author: name.into(),
            ..Default::default()
        });
        p.push_top(Record::new(tag::STAT, 0x800).zsub(tag::EDID, "RockA"));
        p.push_top(Record::new(tag::STAT, 0x801).zsub(tag::EDID, "RockB"));
        p
    }

    fn patch(masters: &[&str]) -> Plugin {
        Plugin::new(FileHeader {
            masters: masters.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        })
    }

    #[test]
    fn the_last_override_wins_and_the_owner_is_the_master() {
        let mut lo = LoadOrder::new();
        lo.add("Skyrim.esm", master("Skyrim.esm", false)).unwrap();
        let mut a = patch(&["Skyrim.esm"]);
        a.push_top(Record::new(tag::STAT, 0x00000800).zsub(tag::EDID, "RockA_fromA"));
        a.push_top(Record::new(tag::STAT, 0x01000800).zsub(tag::EDID, "NewInA"));
        lo.add("A.esp", a).unwrap();
        let mut b = patch(&["Skyrim.esm", "A.esp"]);
        b.push_top(Record::new(tag::STAT, 0x00000800).zsub(tag::EDID, "RockA_fromB"));
        // B refers to A's new record through its own master list, index 1.
        b.push_top(Record::new(tag::STAT, 0x01000800).zsub(tag::EDID, "NewInA_fromB"));
        lo.add("B.esp", b).unwrap();

        let rock = GlobalFormId {
            file: 0,
            object: 0x800,
        };
        assert_eq!(lo.overrides(rock).len(), 3);
        assert_eq!(
            lo.winner(rock).unwrap().editor_id().as_deref(),
            Some("RockA_fromB")
        );
        let new_in_a = GlobalFormId {
            file: 1,
            object: 0x800,
        };
        assert_eq!(
            lo.winner(new_in_a).unwrap().editor_id().as_deref(),
            Some("NewInA_fromB")
        );
        // The same raw id means different things in A and in B.
        assert_eq!(lo.resolve(1, 0x01000800), new_in_a);
        assert_eq!(lo.resolve(2, 0x01000800), new_in_a);
        assert_eq!(
            lo.resolve(2, 0x02000123),
            GlobalFormId {
                file: 2,
                object: 0x123
            }
        );
        assert_eq!(lo.runtime_id(new_in_a), 0x01000800);
        assert_eq!(
            lo.runtime_id(GlobalFormId {
                file: 2,
                object: 0x123
            }),
            0x02000123
        );
        let overridden: Vec<_> = lo.overridden().collect();
        assert_eq!(overridden.len(), 2);
        assert_eq!(lo.winners_of(tag::STAT).count(), 3);
        let ids: Vec<GlobalFormId> = lo.winners().map(|(id, _)| id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        let stats: Vec<GlobalFormId> = lo.winners_of(tag::STAT).map(|(id, _)| id).collect();
        assert!(stats.windows(2).all(|w| w[0] < w[1]));
        assert!(overridden.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_missing_master_is_refused() {
        let mut lo = LoadOrder::new();
        let err = lo.add("A.esp", patch(&["Skyrim.esm"])).unwrap_err();
        assert!(err.to_string().contains("Skyrim.esm"));
    }

    #[test]
    fn master_names_match_without_case() {
        let mut lo = LoadOrder::new();
        lo.add("Skyrim.esm", master("Skyrim.esm", false)).unwrap();
        lo.add("A.esp", patch(&["skyrim.ESM"])).unwrap();
        assert_eq!(lo.file(1).master_positions, vec![0]);
    }

    #[test]
    fn rnd_16_load_order_fingerprint_changes_when_plugin_order_changes() {
        let master = master("Skyrim.esm", false);
        let a = patch(&["Skyrim.esm"]);
        let b = patch(&["Skyrim.esm"]);
        let mut first = LoadOrder::new();
        first.add("Skyrim.esm", master.clone()).unwrap();
        first.add("A.esp", a.clone()).unwrap();
        first.add("B.esp", b.clone()).unwrap();
        let mut second = LoadOrder::new();
        second.add("Skyrim.esm", master).unwrap();
        second.add("B.esp", b).unwrap();
        second.add("A.esp", a).unwrap();

        assert_ne!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn rnd_16_load_order_fingerprint_ignores_plugin_name_case() {
        let mut first = LoadOrder::new();
        first
            .add("Skyrim.esm", master("Skyrim.esm", false))
            .unwrap();
        let mut second = LoadOrder::new();
        second
            .add("skyrim.ESM", master("Skyrim.esm", false))
            .unwrap();

        assert_eq!(first.fingerprint(), second.fingerprint());
    }

    #[test]
    fn light_files_share_the_fe_slot() {
        let mut lo = LoadOrder::new();
        lo.add("Skyrim.esm", master("Skyrim.esm", false)).unwrap();
        lo.add("L1.esl", master("L1", true)).unwrap();
        lo.add("Mid.esp", patch(&["Skyrim.esm"])).unwrap();
        lo.add("L2.esl", master("L2", true)).unwrap();
        assert_eq!(lo.file(1).slot, Slot::Light(0));
        assert_eq!(lo.file(2).slot, Slot::Regular(1));
        assert_eq!(lo.file(3).slot, Slot::Light(1));
        assert_eq!(
            lo.runtime_id(GlobalFormId {
                file: 1,
                object: 0x801
            }),
            0xFE000801
        );
        assert_eq!(
            lo.runtime_id(GlobalFormId {
                file: 3,
                object: 0x801
            }),
            0xFE001801
        );
        assert_eq!(
            lo.runtime_id(GlobalFormId {
                file: 2,
                object: 0x123
            }),
            0x01000123
        );
    }

    #[test]
    fn an_index_past_the_master_list_means_the_file_itself() {
        let mut lo = LoadOrder::new();
        lo.add("Skyrim.esm", master("Skyrim.esm", false)).unwrap();
        let mut a = patch(&["Skyrim.esm"]);
        a.push_top(Record::new(tag::STAT, 0xFF000800));
        lo.add("A.esp", a).unwrap();
        assert_eq!(
            lo.resolve(1, 0xFF000800),
            GlobalFormId {
                file: 1,
                object: 0x800
            }
        );
        assert!(lo.contains(GlobalFormId {
            file: 1,
            object: 0x800
        }));
    }

    #[test]
    fn engine_global_defaults_do_not_replace_authored_records_or_overrides() {
        let global = GlobalFormId {
            file: 0,
            object: 0x63,
        };
        for raw in [0x63, 0xFF00_0063] {
            let mut order = LoadOrder::new();
            let mut plugin = master("Skyrim.esm", false);
            plugin.push_top(Record::new(Tag(*b"GLOB"), raw).zsub(tag::EDID, "Authored"));
            order.add("Skyrim.esm", plugin).unwrap();
            assert_eq!(
                order.winner(global).unwrap().editor_id().as_deref(),
                Some("Authored")
            );
            assert_eq!(order.overrides(global).len(), 1);
        }
        let mut order = LoadOrder::new();
        order
            .add("Skyrim.esm", master("Skyrim.esm", false))
            .unwrap();
        assert_eq!(
            order.winner(global).unwrap().editor_id().as_deref(),
            Some("PlayCredits")
        );
        let mut patch = patch(&["Skyrim.esm"]);
        let mut deleted = Record::new(Tag(*b"GLOB"), 0x63);
        deleted.flags = flags::DELETED;
        patch.push_top(deleted);
        order.add("Patch.esp", patch).unwrap();
        assert!(order.winner(global).unwrap().is_deleted());
        assert_eq!(order.overrides(global).len(), 2);
    }

    #[test]
    fn engine_globals_are_not_injected_into_other_plugins_or_a_late_skyrim() {
        let mut order = LoadOrder::new();
        order.add("Other.esm", master("Other.esm", false)).unwrap();
        order
            .add("Skyrim.esm", master("Skyrim.esm", false))
            .unwrap();
        assert!(!order.winners_of(Tag(*b"GLOB")).any(|_| true));
    }
}
