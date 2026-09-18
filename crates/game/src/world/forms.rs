// SPDX-License-Identifier: GPL-3.0-or-later

//! Form state built from records on first use: globals, form lists,
//! leveled lists, locations and their parent chain, factions, actor bases,
//! cells and game settings.

use soe_esm::GlobalFormId;
use soe_papyrus::Value;

use super::{
    le_f32, le_u32, ActorBaseState, CellState, FactionState, LeveledEntry, LeveledList,
    LocationState, World, T,
};
use crate::quest::zstring;

impl World {
    pub fn global_value(&mut self, id: GlobalFormId) -> f32 {
        if let Some(v) = self.globals.get(&id) {
            return *v;
        }
        let v = self
            .record(id)
            .filter(|r| r.tag == T(b"GLOB"))
            .and_then(|r| r.find(T(b"FLTV")))
            .filter(|d| d.len() >= 4)
            .map(le_f32)
            .unwrap_or(0.0);
        self.globals.insert(id, v);
        v
    }

    pub fn set_global(&mut self, id: GlobalFormId, value: f32) {
        self.globals.insert(id, value);
    }

    pub fn form_list(&mut self, id: GlobalFormId) -> Option<&mut Vec<GlobalFormId>> {
        if !self.form_lists.contains_key(&id) {
            let rref = self.order.winner_ref(id)?;
            let rec = self.order.record(rref);
            if rec.tag != T(b"FLST") {
                return None;
            }
            let items = rec
                .all(T(b"LNAM"))
                .filter_map(le_u32)
                .map(|raw| self.order.resolve(rref.file, raw))
                .collect();
            self.form_lists.insert(id, items);
        }
        self.form_lists.get_mut(&id)
    }

    /// An `LVLI`, `LVLN` or `LVSP` record: `LVLD` chance none, `LVLF`
    /// flags and the `LVLO` entries (level `u16`, two unused bytes, the
    /// form, count `u16`, per `wbLeveledListEntry`).
    pub fn leveled_list(&mut self, id: GlobalFormId) -> Option<&mut LeveledList> {
        if !self.leveled_lists.contains_key(&id) {
            let rref = self.order.winner_ref(id)?;
            let rec = self.order.record(rref);
            if !matches!(rec.tag.as_bytes(), b"LVLI" | b"LVLN" | b"LVSP") {
                return None;
            }
            let chance_none = rec.find(T(b"LVLD")).and_then(|d| d.first().copied());
            let flags = rec.find(T(b"LVLF")).and_then(|d| d.first().copied());
            let entries: Vec<LeveledEntry> = rec
                .all(T(b"LVLO"))
                .filter(|d| d.len() >= 8)
                .map(|d| LeveledEntry {
                    level: u16::from_le_bytes([d[0], d[1]]),
                    form: self.order.resolve(rref.file, le_u32(&d[4..]).unwrap_or(0)),
                    count: if d.len() >= 10 {
                        u16::from_le_bytes([d[8], d[9]])
                    } else {
                        1
                    },
                })
                .collect();
            let chance_none = chance_none.unwrap_or(0);
            self.leveled_lists.insert(
                id,
                LeveledList {
                    chance_none,
                    flags: flags.unwrap_or(0),
                    base_entries: entries.len(),
                    entries,
                    base_chance_none: chance_none,
                },
            );
        }
        self.leveled_lists.get_mut(&id)
    }

    pub fn location(&mut self, id: GlobalFormId) -> Option<&mut LocationState> {
        if !self.locations.contains_key(&id) {
            if self.tag_of(id) != Some(T(b"LCTN")) {
                return None;
            }
            self.locations.insert(id, LocationState::default());
        }
        self.locations.get_mut(&id)
    }

    /// `PNAM` of an `LCTN`.
    pub fn location_parent(&self, id: GlobalFormId) -> Option<GlobalFormId> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        if rec.tag != T(b"LCTN") {
            return None;
        }
        rec.find(T(b"PNAM"))
            .and_then(le_u32)
            .filter(|p| *p != 0)
            .map(|p| self.order.resolve(rref.file, p))
    }

    /// The parent chain of a location, nearest first, the location itself
    /// excluded. Stops on a cycle.
    pub fn location_ancestors(&self, id: GlobalFormId) -> Vec<GlobalFormId> {
        let mut out = Vec::new();
        let mut next = self.location_parent(id);
        while let Some(p) = next {
            if p == id || out.contains(&p) {
                break;
            }
            out.push(p);
            next = self.location_parent(p);
        }
        out
    }

    /// `Location.IsChild`: whether `child` sits anywhere below `parent`.
    pub fn location_is_child(&self, parent: GlobalFormId, child: GlobalFormId) -> bool {
        self.location_ancestors(child).contains(&parent)
    }

    /// `Location.HasCommonParent`: a shared ancestor, restricted to those
    /// carrying `filter` when given.
    pub fn locations_share_parent(
        &self,
        a: GlobalFormId,
        b: GlobalFormId,
        filter: Option<GlobalFormId>,
    ) -> bool {
        let mine = self.location_ancestors(a);
        self.location_ancestors(b)
            .iter()
            .any(|p| mine.contains(p) && filter.is_none_or(|k| self.keywords(*p).contains(&k)))
    }

    /// Where a reference is: its own `XLCN`, else its cell's.
    pub fn current_location(&mut self, id: GlobalFormId) -> Option<GlobalFormId> {
        let (own, cell) = self.reference(id).map(|r| (r.location, r.cell))?;
        if own.is_some() {
            return own;
        }
        let cell = cell?;
        let rref = self.order.winner_ref(cell)?;
        self.order
            .record(rref)
            .find(T(b"XLCN"))
            .and_then(le_u32)
            .filter(|l| *l != 0)
            .map(|l| self.order.resolve(rref.file, l))
    }

    /// Where the record placed a reference: its `XLCN`, else the `XLCN` of
    /// the cell it was placed in. Unaffected by `MoveTo`.
    pub fn editor_location(&self, id: GlobalFormId) -> Option<GlobalFormId> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        if let Some(l) = rec.find(T(b"XLCN")).and_then(le_u32).filter(|l| *l != 0) {
            return Some(self.order.resolve(rref.file, l));
        }
        let cell = self.order.resolve(rref.file, rec.placement.cell?);
        let cref = self.order.winner_ref(cell)?;
        self.order
            .record(cref)
            .find(T(b"XLCN"))
            .and_then(le_u32)
            .filter(|l| *l != 0)
            .map(|l| self.order.resolve(cref.file, l))
    }

    /// A `FACT` record: `XNAM` relations and the crime tracking flag.
    pub fn faction(&mut self, id: GlobalFormId) -> Option<&mut FactionState> {
        if !self.factions.contains_key(&id) {
            let rref = self.order.winner_ref(id)?;
            let rec = self.order.record(rref);
            if rec.tag != T(b"FACT") {
                return None;
            }
            let mut st = FactionState::default();
            for x in rec.all(T(b"XNAM")).filter(|d| d.len() >= 12) {
                let other = self.order.resolve(rref.file, le_u32(x).unwrap_or(0));
                let combat = i32::from_le_bytes([x[8], x[9], x[10], x[11]]);
                st.reactions.insert(other, combat);
            }
            st.tracks_crime = rec
                .find(T(b"DATA"))
                .and_then(le_u32)
                .is_some_and(|f| f & 0x40 != 0);
            self.factions.insert(id, st);
        }
        self.factions.get_mut(&id)
    }

    pub fn actor_base(&mut self, id: GlobalFormId) -> Option<&mut ActorBaseState> {
        if !self.actor_bases.contains_key(&id) {
            let npc = self.npc(id)?;
            let file = self.order.winner_ref(id)?.file;
            let f = npc.config.flags;
            let st = ActorBaseState {
                essential: npc.is_essential(),
                protected: npc.is_protected(),
                invulnerable: f & crate::npc::flags::INVULNERABLE != 0,
                unique: f & crate::npc::flags::UNIQUE != 0,
                female: npc.is_female(),
                race: npc.race.map(|r| self.order.resolve(file, r)),
                class: npc.class.map(|c| self.order.resolve(file, c)),
                outfit: npc.outfit.map(|o| self.order.resolve(file, o)),
                sleep_outfit: npc.sleep_outfit.map(|o| self.order.resolve(file, o)),
                dead_count: 0,
            };
            self.actor_bases.insert(id, st);
        }
        self.actor_bases.get_mut(&id)
    }

    pub fn cell_state(&mut self, id: GlobalFormId) -> Option<&mut CellState> {
        if !self.cells.contains_key(&id) {
            let rref = self.order.winner_ref(id)?;
            let rec = self.order.record(rref);
            if rec.tag != T(b"CELL") {
                return None;
            }
            let owner = rec
                .find(T(b"XOWN"))
                .and_then(le_u32)
                .filter(|o| *o != 0)
                .map(|o| self.order.resolve(rref.file, o));
            self.cells.insert(
                id,
                CellState {
                    owner,
                    public: false,
                },
            );
        }
        self.cells.get_mut(&id)
    }

    /// A `GMST` by editor id, a `SetGameSetting*` override first. The
    /// first letter of the id gives the type.
    pub fn game_setting(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.settings.get(&name.to_ascii_lowercase()) {
            return Some(v.clone());
        }
        let id = self.form(name)?;
        let rec = self.record(id).filter(|r| r.tag == T(b"GMST"))?;
        let data = rec.find(T(b"DATA"))?;
        Some(match name.as_bytes().first() {
            Some(b'f') if data.len() >= 4 => Value::Float(le_f32(data)),
            Some(b'i' | b'u') if data.len() >= 4 => Value::Int(le_u32(data)? as i32),
            Some(b'b') if data.len() >= 4 => Value::Bool(le_u32(data)? != 0),
            Some(b's') => Value::string(&zstring(data)),
            _ => return None,
        })
    }
}
