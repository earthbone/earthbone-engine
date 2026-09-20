// SPDX-License-Identifier: GPL-3.0-or-later

//! References: the run-time state of a placed object built from its `REFR`
//! or `ACHR` record on first use, dynamic placement, the player, inventory,
//! movement, distance and activation.

use std::collections::BTreeMap;

use soe_esm::plugin::flags;
use soe_esm::records::refr::Refr;
use soe_esm::GlobalFormId;

use super::{
    le_f32, le_u32, Alloc, Handle, Pending, RefState, World, DYNAMIC_FILE, PLAYER_OBJECT, T,
};

impl World {
    /// Build a reference's state from its record, without inserting it.
    /// `crate::world::transition` uses this to prove that unloading a
    /// reference loses nothing, by rebuilding it and comparing.
    pub(super) fn load_ref(&mut self, id: GlobalFormId) -> Option<RefState> {
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        let file = rref.file;
        let mut st = match rec.tag.as_bytes() {
            b"REFR" => {
                let r = Refr::from_record(rec).ok()?;
                let mut st = RefState::new(Some(self.order.resolve(file, r.base)));
                if let Some(t) = r.transform {
                    st.position = t.position;
                    st.rotation = t.rotation;
                }
                st.scale = r.scale;
                if let Some(lock) = r.lock {
                    st.locked = true;
                    st.lock_level = i32::from(lock.level);
                }
                st.linked = r
                    .linked_refs
                    .iter()
                    .map(|l| {
                        let kw = (l.keyword != 0).then(|| self.order.resolve(file, l.keyword));
                        (kw, self.order.resolve(file, l.reference))
                    })
                    .collect();
                st.teleport = r
                    .teleport
                    .map(|t| (self.order.resolve(file, t.door), t.destination));
                st.owner = r
                    .owner
                    .filter(|o| *o != 0)
                    .map(|o| self.order.resolve(file, o));
                st.location = r
                    .location
                    .filter(|l| *l != 0)
                    .map(|l| self.order.resolve(file, l));
                st
            }
            b"ACHR" => {
                let base = self.order.resolve(file, le_u32(rec.find(T(b"NAME"))?)?);
                let mut st = RefState::new(Some(base));
                if let Some(d) = rec.find(T(b"DATA")).filter(|d| d.len() >= 24) {
                    for (i, v) in st.position.iter_mut().enumerate() {
                        *v = le_f32(&d[i * 4..]);
                    }
                    for (i, v) in st.rotation.iter_mut().enumerate() {
                        *v = le_f32(&d[12 + i * 4..]);
                    }
                }
                if let Some(s) = rec.find(T(b"XSCL")).filter(|d| d.len() >= 4) {
                    st.scale = le_f32(s);
                }
                st.owner = rec
                    .find(T(b"XOWN"))
                    .and_then(le_u32)
                    .filter(|o| *o != 0)
                    .map(|o| self.order.resolve(file, o));
                st.location = rec
                    .find(T(b"XLCN"))
                    .and_then(le_u32)
                    .filter(|l| *l != 0)
                    .map(|l| self.order.resolve(file, l));
                // An actor reference carries XLKR the same way a REFR does,
                // which is how the DLC scripts find a coffin or a cart.
                st.linked = rec
                    .all(T(b"XLKR"))
                    .filter_map(|d| {
                        let reference = match d.len() {
                            4 => le_u32(d)?,
                            8.. => le_u32(&d[4..])?,
                            _ => return None,
                        };
                        let keyword = (d.len() >= 8)
                            .then(|| le_u32(d))
                            .flatten()
                            .filter(|k| *k != 0)
                            .map(|k| self.order.resolve(file, k));
                        Some((keyword, self.order.resolve(file, reference)))
                    })
                    .collect();
                st.is_actor = true;
                st
            }
            _ => return None,
        };
        // A deleted winner starts disabled, the way xEdit's undelete and
        // disable cleaning would have left it, so whatever still names it
        // resolves instead of crashing the way the vanilla engine does.
        st.enabled = rec.flags & flags::INITIALLY_DISABLED == 0 && !rec.is_deleted();
        st.cell = rec.placement.cell.map(|c| self.order.resolve(file, c));
        // Base Object Swapper replaces the base of a placed reference, and a
        // reference is built here rather than at start, which is why the swap
        // is held until now. See `crate::moddata`.
        st.base = st.base.map(|b| self.mod_data.swap(b));
        if let Some(base) = st.base {
            st.inventory = self.base_inventory(base);
        }
        Some(st)
    }

    /// A base's own `CNTO` list plus anything a distribution handed it as an
    /// item, so a distributed item counts the same as a shipped one through
    /// `GetItemCount`. See `crate::moddata`.
    fn base_inventory(&self, base: GlobalFormId) -> BTreeMap<GlobalFormId, i32> {
        let mut out = BTreeMap::new();
        for d in self.mod_data.for_base(base) {
            if d.record_type == soe_moddata::spid::RecordType::Item {
                *out.entry(d.what).or_insert(0) += d.count;
            }
        }
        let Some(rref) = self.order.winner_ref(base) else {
            return out;
        };
        for cnto in self.order.record(rref).all(T(b"CNTO")) {
            if cnto.len() >= 8 {
                let item = self.order.resolve(rref.file, le_u32(cnto).unwrap_or(0));
                let count = i32::from_le_bytes([cnto[4], cnto[5], cnto[6], cnto[7]]);
                *out.entry(item).or_insert(0) += count;
            }
        }
        out
    }

    pub fn reference(&mut self, id: GlobalFormId) -> Option<&mut RefState> {
        if !self.refs.contains_key(&id) {
            let st = self.load_ref(id)?;
            self.refs.insert(id, st);
        }
        self.refs.get_mut(&id)
    }

    pub fn reference_ref(&self, id: GlobalFormId) -> Option<&RefState> {
        self.refs.get(&id)
    }

    pub fn base_of(&mut self, id: GlobalFormId) -> Option<GlobalFormId> {
        self.reference(id).and_then(|r| r.base)
    }

    pub fn is_reference(&mut self, id: GlobalFormId) -> bool {
        self.reference(id).is_some()
    }

    /// A new reference of `base` at the position of `at`, the `PlaceAtMe`
    /// path. Returns the dynamic id.
    pub fn place_at(
        &mut self,
        base: GlobalFormId,
        at: GlobalFormId,
        disabled: bool,
    ) -> GlobalFormId {
        let (position, rotation, cell) = match self.reference(at) {
            Some(r) => (r.position, r.rotation, r.cell),
            None => ([0.0; 3], [0.0; 3], None),
        };
        self.next_dynamic += 1;
        let id = GlobalFormId {
            file: DYNAMIC_FILE,
            object: self.next_dynamic,
        };
        let mut st = RefState::new(Some(base));
        st.position = position;
        st.rotation = rotation;
        st.cell = cell;
        st.enabled = !disabled;
        st.is_actor = self.tag_of(base) == Some(T(b"NPC_"));
        st.inventory = self.base_inventory(base);
        self.refs.insert(id, st);
        id
    }

    pub fn player(&mut self) -> GlobalFormId {
        if let Some(player) = self.player {
            return player;
        }
        let id = GlobalFormId {
            file: 0,
            object: PLAYER_OBJECT,
        };
        if self.reference(id).is_none() {
            let base = self
                .form("Player")
                .filter(|base| self.tag_of(*base) == Some(T(b"NPC_")));
            let mut state = RefState::new(base);
            state.is_actor = true;
            state.display_name = Some("Player".into());
            if let Some(base) = base {
                state.inventory = self.base_inventory(base);
            }
            self.refs.insert(id, state);
        }
        self.player = Some(id);
        id
    }

    pub fn item_count(&mut self, holder: GlobalFormId, item: GlobalFormId) -> i32 {
        self.reference(holder)
            .and_then(|r| r.inventory.get(&item).copied())
            .unwrap_or(0)
    }

    pub fn add_item(&mut self, holder: GlobalFormId, item: GlobalFormId, count: i32) {
        if let Some(r) = self.reference(holder) {
            let n = r.inventory.entry(item).or_insert(0);
            *n += count;
            if *n <= 0 {
                r.inventory.remove(&item);
            }
        }
    }

    /// Remove up to `count` and hand them to `target` when given. Returns how
    /// many moved.
    pub fn remove_item(
        &mut self,
        holder: GlobalFormId,
        item: GlobalFormId,
        count: i32,
        target: Option<GlobalFormId>,
    ) -> i32 {
        let have = self.item_count(holder, item);
        let moved = count.min(have).max(0);
        if moved == 0 {
            return 0;
        }
        self.add_item(holder, item, -moved);
        if let Some(t) = target {
            self.add_item(t, item, moved);
        }
        moved
    }

    pub fn move_to(&mut self, id: GlobalFormId, target: GlobalFormId, offset: [f32; 3]) -> bool {
        let Some((pos, cell)) = self.reference(target).map(|t| (t.position, t.cell)) else {
            return false;
        };
        match self.reference(id) {
            Some(r) => {
                r.position = [pos[0] + offset[0], pos[1] + offset[1], pos[2] + offset[2]];
                r.cell = cell;
                true
            }
            None => false,
        }
    }

    /// `MoveToMyEditorLocation`: back to the record's cell and transform.
    /// A dynamic reference has no record and stays put.
    pub fn move_to_editor_location(&mut self, id: GlobalFormId) -> bool {
        let Some(fresh) = self.load_ref(id) else {
            return false;
        };
        match self.reference(id) {
            Some(r) => {
                r.position = fresh.position;
                r.rotation = fresh.rotation;
                r.cell = fresh.cell;
                true
            }
            None => false,
        }
    }

    pub fn distance(&mut self, a: GlobalFormId, b: GlobalFormId) -> f32 {
        let (Some(pa), Some(pb)) = (
            self.reference(a).map(|r| r.position),
            self.reference(b).map(|r| r.position),
        ) else {
            return 0.0;
        };
        ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt()
    }

    /// Activate a reference and unload the source cell immediately.
    /// A default-only call suppresses the event and ignores BlockActivation.
    pub fn activate(
        &mut self,
        target: GlobalFormId,
        activator: GlobalFormId,
        send_event: bool,
        alloc: Alloc<'_>,
    ) -> bool {
        self.activate_with_unload(target, activator, send_event, false, alloc)
    }

    /// Activate with optional deferred source-cell unload. Default-only calls
    /// bypass BlockActivation, as specified by the CK Activate reference.
    /// Default processing for doors and
    /// containers unless the reference blocks activation, then `OnActivate`
    /// to the target's scripts when `send_event` is set. Returns false when
    /// the target is not a reference.
    pub fn activate_with_unload(
        &mut self,
        target: GlobalFormId,
        activator: GlobalFormId,
        send_event: bool,
        defer_unload: bool,
        alloc: Alloc<'_>,
    ) -> bool {
        let Some((blocked, base, teleport)) = self
            .reference(target)
            .map(|r| (r.activation_blocked, r.base, r.teleport))
        else {
            return false;
        };
        let kind = base.and_then(|b| self.tag_of(b));
        let lockable = kind == Some(T(b"DOOR")) || kind == Some(T(b"CONT"));
        let process = (!blocked || !send_event)
            && (!lockable || self.unlock_with_key(target, activator, alloc));
        if process {
            match kind.map(|t| *t.as_bytes()) {
                // A door with teleport data is a load door, the kind that
                // fires a load screen in vanilla. A door without it simply
                // swings, and both exist in the shipped files.
                Some([b'D', b'O', b'O', b'R']) => {
                    if teleport.is_some() && activator == self.player() && defer_unload {
                        if let Some(way) = self.arm_doorway(target) {
                            self.commit_doorway(&way);
                        }
                    } else if let Some((far_door, landing)) = teleport {
                        let far_cell = self.reference(far_door).and_then(|d| d.cell);
                        if let Some(r) = self.reference(activator) {
                            r.position = landing.position;
                            r.rotation = landing.rotation;
                            r.cell = far_cell;
                        }
                        // Only the player's walk swaps what is resident. An
                        // actor going through a door moves without dragging
                        // the world after it.
                        if activator == self.player() {
                            self.change_cell(far_cell);
                        }
                    } else if let Some(r) = self.reference(target) {
                        r.opened = !r.opened;
                    }
                }
                Some([b'C', b'O', b'N', b'T']) => {
                    if let Some(r) = self.reference(target) {
                        r.opened = true;
                    }
                }
                _ => {}
            }
        }
        if send_event {
            let who = self.form_value(Some(activator), alloc);
            let object = self.object_of(Handle::Form(target), alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnActivate".into(),
                args: vec![who],
            });
        }
        process
    }
}
