// SPDX-License-Identifier: GPL-3.0-or-later

//! Form state built from records on first use: globals, form lists,
//! leveled lists, locations and their parent chain, factions, actor bases,
//! cells and game settings.

use soe_esm::records::lctn::Location;
use soe_esm::GlobalFormId;
use soe_papyrus::Value;
use std::collections::HashSet;

use super::{
    le_f32, le_u32, ActorBaseState, CellState, FactionState, LeveledEntry, LeveledList,
    LocationState, World, T,
};
use crate::crime::CrimeValues;
use crate::quest::zstring;

pub(super) fn add_saved_form_list_entries(
    items: &mut Vec<GlobalFormId>,
    additions: impl IntoIterator<Item = GlobalFormId>,
) {
    let mut seen: HashSet<_> = items.iter().copied().collect();
    let mut created = Vec::new();
    let mut regular = Vec::new();
    for item in additions {
        if item.object == 0 || !seen.insert(item) {
            continue;
        }
        if item.file == super::DYNAMIC_FILE {
            created.push(item);
        } else {
            regular.push(item);
        }
    }
    let at = items
        .iter()
        .take_while(|entry| entry.file == super::DYNAMIC_FILE)
        .count();
    items.splice(at..at, created);
    items.extend(regular);
}

impl World {
    /// Whether an actor base's race carries the RACE DATA Playable flag.
    pub fn race_is_playable(&mut self, actor: GlobalFormId) -> Option<bool> {
        let race = self.actor_base(actor)?.race?;
        let rec = self
            .record(race)
            .filter(|record| record.tag == T(b"RACE"))?;
        let data = rec.find(T(b"DATA"))?;
        // The seven skill boosts occupy two bytes each, followed by two
        // unknown bytes, four height and weight floats, then the flags.
        let flags = data.get(32..36)?;
        Some(u32::from_le_bytes(flags.try_into().ok()?) & 1 != 0)
    }

    /// Whether an actor base's race carries the RACE DATA Child flag.
    pub fn race_is_child(&mut self, actor: GlobalFormId) -> Option<bool> {
        let race = self.actor_base(actor)?.race?;
        let rec = self
            .record(race)
            .filter(|record| record.tag == T(b"RACE"))?;
        let data = rec.find(T(b"DATA"))?;
        let flags = data.get(32..36)?;
        Some(u32::from_le_bytes(flags.try_into().ok()?) & 4 != 0)
    }

    /// Whether a winning RELA record connects two actor bases with an ASTP.
    /// Relationship direction is ignored because association membership is
    /// mutual, while parent checks use the authored direction separately.
    pub fn actors_have_association(
        &self,
        first: GlobalFormId,
        second: GlobalFormId,
        association: GlobalFormId,
    ) -> bool {
        self.order.winners_of(T(b"RELA")).any(|(id, record)| {
            if record.is_deleted() {
                return false;
            }
            let Some(data) = record.find(T(b"DATA")) else {
                return false;
            };
            if data.len() < 16 {
                return false;
            }
            let parent = self.resolve_from(id, le_u32(&data[..4]).unwrap_or(0));
            let child = self.resolve_from(id, le_u32(&data[4..8]).unwrap_or(0));
            let assoc = self.resolve_from(id, le_u32(&data[12..16]).unwrap_or(0));
            assoc == Some(association)
                && ((parent == Some(first) && child == Some(second))
                    || (parent == Some(second) && child == Some(first)))
        })
    }

    /// Whether two actor bases have a relationship whose association is marked
    /// Family Association in ASTP DATA. With no second actor, any family link
    /// for the first actor is sufficient.
    pub fn actors_have_family_relationship(
        &self,
        actor: GlobalFormId,
        other: Option<GlobalFormId>,
    ) -> bool {
        self.order.winners_of(T(b"RELA")).any(|(id, record)| {
            if record.is_deleted() {
                return false;
            }
            let Some(data) = record.find(T(b"DATA")) else {
                return false;
            };
            if data.len() < 16 {
                return false;
            }
            let parent = self.resolve_from(id, le_u32(&data[..4]).unwrap_or(0));
            let child = self.resolve_from(id, le_u32(&data[4..8]).unwrap_or(0));
            let linked = parent == Some(actor) || child == Some(actor);
            let paired = other.is_none_or(|other| {
                (parent == Some(actor) && child == Some(other))
                    || (parent == Some(other) && child == Some(actor))
            });
            let association = self.resolve_from(id, le_u32(&data[12..16]).unwrap_or(0));
            linked
                && paired
                && association
                    .and_then(|association| self.record(association))
                    .filter(|record| record.tag == T(b"ASTP") && !record.is_deleted())
                    .and_then(|record| record.find(T(b"DATA")))
                    .and_then(le_u32)
                    .is_some_and(|flags| flags & 1 != 0)
        })
    }

    /// Whether the authored RELA direction names `parent` and `child`.
    pub fn actors_have_parent_relationship(
        &self,
        parent: GlobalFormId,
        child: GlobalFormId,
    ) -> bool {
        self.order.winners_of(T(b"RELA")).any(|(id, record)| {
            if record.is_deleted() {
                return false;
            }
            let Some(data) = record.find(T(b"DATA")) else {
                return false;
            };
            data.len() >= 8
                && self.resolve_from(id, le_u32(&data[..4]).unwrap_or(0)) == Some(parent)
                && self.resolve_from(id, le_u32(&data[4..8]).unwrap_or(0)) == Some(child)
        })
    }

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
        if self.form("GameDaysPassed") == Some(id) {
            self.clock.game_days = f64::from(value);
        } else if self.form("TimeScale") == Some(id) {
            self.clock.timescale = f64::from(value);
        }
    }

    pub fn form_list(&mut self, id: GlobalFormId) -> Option<&mut Vec<GlobalFormId>> {
        if !self.form_lists.contains_key(&id) {
            let rec = self.record(id)?;
            if rec.tag != T(b"FLST") {
                return None;
            }
            let mut items: Vec<_> = rec
                .all(T(b"LNAM"))
                .filter_map(le_u32)
                .map(|raw| {
                    self.resolve_from(id, raw)
                        .unwrap_or(GlobalFormId { file: 0, object: 0 })
                })
                .collect();
            if self
                .record_overrides
                .get(&id)
                .is_some_and(|value| value.list.is_some())
            {
                let authored = self
                    .order
                    .winner(id)
                    .map_or(0, |record| record.all(T(b"LNAM")).count());
                let additions = items.split_off(authored.min(items.len()));
                add_saved_form_list_entries(&mut items, additions);
            }
            self.form_lists.insert(id, items);
        }
        self.form_lists.get_mut(&id)
    }

    /// An `LVLI`, `LVLN` or `LVSP` record: `LVLD` chance none, `LVLF`
    /// flags and the `LVLO` entries (level `u16`, two unused bytes, the
    /// form, count `u16`, per `wbLeveledListEntry`).
    pub fn leveled_list(&mut self, id: GlobalFormId) -> Option<&mut LeveledList> {
        if !self.leveled_lists.contains_key(&id) {
            let rec = self.record(id)?;
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
                    form: self
                        .resolve_from(id, le_u32(&d[4..]).unwrap_or(0))
                        .unwrap_or(GlobalFormId { file: 0, object: 0 }),
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

    /// The `MNAM` world marker that supplies an `LCTN` package destination.
    pub fn location_marker(&self, id: GlobalFormId) -> Option<GlobalFormId> {
        let rref = self.order.winner_ref(id)?;
        let record = self.order.record(rref);
        if record.is_deleted() {
            return None;
        }
        let location = Location::from_record(record).ok()?;
        let marker = location
            .world_marker
            .filter(|marker| *marker != 0)
            .map(|marker| self.order.resolve(rref.file, marker))?;
        self.order
            .winner(marker)
            .filter(|record| {
                !record.is_deleted() && matches!(record.tag.as_bytes(), b"REFR" | b"ACHR")
            })
            .map(|_| marker)
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
            let rec = self.record(id)?;
            if rec.tag != T(b"FACT") {
                return None;
            }
            let mut st = FactionState::default();
            for x in rec.all(T(b"XNAM")).filter(|d| d.len() >= 12) {
                let Some(other) = self.resolve_from(id, le_u32(x).unwrap_or(0)) else {
                    continue;
                };
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

    /// `FACT.CRVA`, the bounty table `record_witnessed_crime` prices a
    /// crime against. Read straight from the winning record on every call:
    /// unlike `crime_gold_violent`, it never changes at runtime, so caching
    /// it in `FactionState` would only add a save version to keep it in
    /// step with the record. Defaults to all zero when `id` is not a `FACT`
    /// record, `CRVA` is absent (one vanilla record, `MS08AlikrFaction`,
    /// has none, per UESP) or malformed, the same silent-default treatment
    /// `tracks_crime` gives a malformed `DATA`.
    #[must_use]
    pub fn crime_values(&self, id: GlobalFormId) -> CrimeValues {
        self.record(id)
            .filter(|r| r.tag == T(b"FACT"))
            .and_then(|r| r.find(T(b"CRVA")))
            .and_then(|d| CrimeValues::from_bytes(d).ok())
            .unwrap_or_default()
    }

    pub fn actor_base(&mut self, id: GlobalFormId) -> Option<&mut ActorBaseState> {
        if !self.actor_bases.contains_key(&id) {
            use crate::npc::template;
            let (_, npc) = self.npc_part(id, template::BASE_DATA).ok()?;
            let (_, traits) = self.npc_part(id, template::TRAITS).ok()?;
            let (stats_source, stats) = self.npc_part(id, template::STATS).ok()?;
            let (inventory_source, inventory) = self.npc_part(id, template::INVENTORY).ok()?;
            let f = npc.config.flags;
            let st = ActorBaseState {
                essential: npc.is_essential(),
                protected: npc.is_protected(),
                invulnerable: f & crate::npc::flags::INVULNERABLE != 0,
                unique: self.npc_record(id).ok()?.config.flags & crate::npc::flags::UNIQUE != 0,
                female: traits.is_female(),
                race: {
                    let (source, _) = self.npc_part(id, template::TRAITS).ok()?;
                    self.npc_race(source).ok()?
                },
                class: stats
                    .class
                    .and_then(|raw| self.resolve_from(stats_source, raw)),
                outfit: inventory
                    .outfit
                    .and_then(|raw| self.resolve_from(inventory_source, raw)),
                sleep_outfit: inventory
                    .sleep_outfit
                    .and_then(|raw| self.resolve_from(inventory_source, raw)),
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
        let Some(id) = self.form(name) else {
            return crate::game_settings::engine_default(name).map(Value::Float);
        };
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
