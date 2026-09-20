// SPDX-License-Identifier: GPL-3.0-or-later

//! Form lookup: editor ids, winning records, run-time FormIDs and their
//! inverse, keywords, names and descriptions, and the Papyrus class and
//! `GetType` code a form answers to.

use soe_esm::{GlobalFormId, Record, Slot, Tag};

use super::{class_for_tag, le_u32, text, type_code_for_tag, Handle, World, DYNAMIC_FILE, T};

impl World {
    pub fn form(&self, editor_id: &str) -> Option<GlobalFormId> {
        self.editor_ids
            .get(&editor_id.to_ascii_lowercase())
            .copied()
    }

    pub fn record(&self, id: GlobalFormId) -> Option<&Record> {
        self.record_overrides
            .get(&id)
            .map(|value| &value.record)
            .or_else(|| self.order.winner(id))
    }

    pub fn exists(&self, id: GlobalFormId) -> bool {
        self.refs.contains_key(&id)
            || self.record_overrides.contains_key(&id)
            || self.order.contains(id)
    }

    pub fn tag_of(&self, id: GlobalFormId) -> Option<Tag> {
        self.record(id).map(|record| record.tag).or_else(|| {
            self.refs.get(&id).map(|reference| {
                if reference.is_actor {
                    T(b"ACHR")
                } else {
                    T(b"REFR")
                }
            })
        })
    }

    pub fn editor_id(&self, id: GlobalFormId) -> Option<String> {
        self.record(id).and_then(Record::editor_id)
    }

    /// The `FULL` name of a form, or of a reference's base object.
    pub fn name_of(&self, id: GlobalFormId) -> Option<String> {
        if let Some(r) = self.refs.get(&id) {
            if let Some(n) = &r.display_name {
                return Some(n.clone());
            }
            let base = self
                .actor_templates
                .get(&id)
                .and_then(|c| c.temporary_base)
                .or(r.base);
            return base.and_then(|b| self.name_of(b));
        }
        let rec = self.record(id)?;
        if matches!(rec.tag.as_bytes(), b"REFR" | b"ACHR") {
            let raw = rec.find(T(b"NAME"))?;
            return self.name_of(self.resolve_from(id, le_u32(raw)?)?);
        }
        let text = text(
            rec.find(T(b"FULL"))?,
            self.record_text_localized(id, T(b"FULL")),
        );
        Some(self.resolve_text(id, T(b"FULL"), &text))
    }

    /// The `DESC` of a form as text, `<string N>` when its table entry is absent.
    pub fn description_of(&self, id: GlobalFormId) -> Option<String> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        let localized = self.order.file(rref.file).plugin.header.is_localized();
        let text = text(rec.find(T(b"DESC"))?, localized);
        Some(self.resolve_text(id, T(b"DESC"), &text))
    }

    pub fn runtime_id(&self, id: GlobalFormId) -> u32 {
        if id.file == 0 && id.object == super::PLAYER_OBJECT {
            super::PLAYER_OBJECT
        } else {
            self.order.runtime_id(id)
        }
    }

    /// The inverse of [`World::runtime_id`].
    pub fn from_runtime_id(&self, form_id: u32) -> Option<GlobalFormId> {
        if form_id == super::PLAYER_OBJECT {
            let player = GlobalFormId {
                file: 0,
                object: super::PLAYER_OBJECT,
            };
            return self.exists(player).then_some(player);
        }
        let top = (form_id >> 24) as u8;
        let id = if top == 0xFF {
            GlobalFormId {
                file: DYNAMIC_FILE,
                object: form_id & 0x00FF_FFFF,
            }
        } else if top == 0xFE {
            let slot = ((form_id >> 12) & 0xFFF) as u16;
            let file = self.file_with_slot(Slot::Light(slot))?;
            GlobalFormId {
                file,
                object: form_id & 0xFFF,
            }
        } else {
            let file = self.file_with_slot(Slot::Regular(top))?;
            GlobalFormId {
                file,
                object: form_id & 0x00FF_FFFF,
            }
        };
        self.exists(id).then_some(id)
    }

    fn file_with_slot(&self, slot: Slot) -> Option<u16> {
        self.order
            .files()
            .iter()
            .position(|f| f.slot == slot)
            .map(|p| p as u16)
    }

    /// `Game.GetFormFromFile`: the low 24 bits are an object in that file.
    pub fn form_from_file(&self, object: u32, file_name: &str) -> Option<GlobalFormId> {
        let file = self.order.position_of(file_name)? as u16;
        let id = GlobalFormId {
            file,
            object: object & 0x00FF_FFFF,
        };
        self.exists(id).then_some(id)
    }

    pub fn resolve_from(&self, owner: GlobalFormId, raw: u32) -> Option<GlobalFormId> {
        if raw == 0 {
            return None;
        }
        if let Some(id) = self
            .record_overrides
            .get(&owner)
            .and_then(|value| value.links.get(&raw))
        {
            return Some(*id);
        }
        let origin = self
            .record_overrides
            .get(&owner)
            .map_or(owner, |r| r.origin);
        let rref = self.order.winner_ref(origin)?;
        Some(self.order.resolve(rref.file, raw))
    }

    /// Resolve the owning quest through the record's authored relationship.
    /// INFO ownership follows its topic group, never TPIC or PNAM.
    pub fn owning_quest(&self, id: GlobalFormId) -> Option<GlobalFormId> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        if rec.is_deleted() {
            return None;
        }
        if rec.tag == T(b"INFO") {
            return self
                .dialogue_topic_of(id)
                .and_then(|topic| self.owning_quest(topic));
        }
        let tag = match *rec.tag.as_bytes() {
            [b'S', b'C', b'E', b'N'] => T(b"PNAM"),
            [b'P', b'A', b'C', b'K'] | [b'D', b'I', b'A', b'L'] | [b'D', b'L', b'B', b'R'] => {
                T(b"QNAM")
            }
            _ => return None,
        };
        let raw = rec.find(tag).and_then(le_u32)?;
        (raw != 0).then(|| self.order.resolve(rref.file, raw))
    }

    /// Placed references of a base object, up to `limit`, actors first. An
    /// editor id in the game names the base (`Hulda` is an `NPC_`), while
    /// everything that acts on an actor at run time needs the `ACHR` that
    /// places it, and that record usually carries no editor id of its own.
    /// This walks the winning records rather than the run-time table, so it
    /// finds a reference nothing has touched yet, and it reads `NAME`
    /// directly instead of building state for every candidate.
    pub fn references_of_base(&self, base: GlobalFormId, limit: usize) -> Vec<GlobalFormId> {
        let mut out = Vec::new();
        if limit == 0 {
            return out;
        }
        for tag in [T(b"ACHR"), T(b"REFR")] {
            for (id, rec) in self.order.winners_of(tag) {
                let Some(raw) = rec.find(T(b"NAME")).and_then(le_u32) else {
                    continue;
                };
                if self.resolve_from(id, raw) == Some(base) {
                    out.push(id);
                    if out.len() >= limit {
                        return out;
                    }
                }
            }
        }
        out
    }

    /// Every placed object the cell index files under a cell, the live ones
    /// then the deleted ones. Builds the index on first use, which is why
    /// this needs `&mut self` where the rest of the lookups do not.
    pub fn cell_references(&mut self, cell: GlobalFormId) -> Vec<GlobalFormId> {
        let index = self.order.build_cell_index();
        let mut out = index.references(cell).to_vec();
        out.extend_from_slice(index.deleted_references(cell));
        out
    }

    /// Associated keywords, including Keyword Item Distributor additions.
    /// MGEF cancellation lists are not associations under CK Magic Effect.
    pub fn keywords(&self, id: GlobalFormId) -> Vec<GlobalFormId> {
        if self.tag_of(id) == Some(T(b"MGEF"))
            && self
                .magic_effect(id)
                .is_ok_and(|effect| effect.data.flags & super::magic::DISPEL_WITH_KEYWORDS != 0)
        {
            return Vec::new();
        }
        self.keyword_entries(id)
    }

    /// Authored and distributed entries, before interpreting an effect's flag.
    pub(super) fn keyword_entries(&self, id: GlobalFormId) -> Vec<GlobalFormId> {
        let mut out: Vec<GlobalFormId> = match self.record(id) {
            Some(rec) => rec.find(T(b"KWDA")).map_or_else(Vec::new, |kwda| {
                kwda.as_chunks::<4>()
                    .0
                    .iter()
                    .filter_map(|c| self.resolve_from(id, u32::from_le_bytes(*c)))
                    .collect()
            }),
            None => Vec::new(),
        };
        if let Some(added) = self.mod_data.added_keywords.get(&id) {
            for k in added {
                if !out.contains(k) {
                    out.push(*k);
                }
            }
        }
        out
    }

    pub fn has_keyword(&mut self, id: GlobalFormId, keyword: GlobalFormId) -> bool {
        let target = self
            .actor_templates
            .get(&id)
            .and_then(|c| c.temporary_base)
            .or_else(|| self.base_of(id))
            .unwrap_or(id);
        self.keywords(target).contains(&keyword)
    }

    /// The Papyrus class a form answers to when no script is attached.
    pub fn class_of(&self, handle: Handle) -> &'static str {
        match handle {
            Handle::ActiveEffect(_) => "ActiveMagicEffect",
            Handle::Alias { quest, id } => {
                let location = self
                    .quests
                    .get(&quest)
                    .and_then(|q| q.def.alias(id))
                    .is_some_and(|a| a.location);
                if location {
                    "LocationAlias"
                } else {
                    "ReferenceAlias"
                }
            }
            Handle::Form(id) => match self.tag_of(id).map(|t| *t.as_bytes()) {
                Some(t) => class_for_tag(&t),
                None => "Form",
            },
        }
    }

    /// The `GetType` code, CommonLib's `FormType` numbering.
    pub fn type_code(&self, id: GlobalFormId) -> i32 {
        match self.tag_of(id).map(|t| *t.as_bytes()) {
            Some(t) => type_code_for_tag(&t),
            None => 0,
        }
    }
}
