// SPDX-License-Identifier: GPL-3.0-or-later

//! The runtime form table. Every game object is a `GlobalFormId` from the
//! load order, and the state a running game adds to a form (a reference's
//! position and enable state, a quest's stage, a global's value) lives here
//! keyed by that id. Base data is read from the winning record on first use,
//! so the table stays a view over the load order rather than a copy of it.
//!
//! Natives run inside the VM and cannot touch the VM, so anything a native
//! wants the VM to do (deliver an event, call a fragment) is queued in
//! `pending` and the [`crate::Game`] loop drains it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use soe_esm::plugin::flags;
use soe_esm::records::refr::Refr;
use soe_esm::records::{Text, Transform};
use soe_esm::{GlobalFormId, LoadOrder, Record, RecordRef, Slot, Tag};
use soe_papyrus::{ObjectId, Value};

use crate::clock::GameClock;
use crate::quest::{zstring, Quest};

/// Forms created at run time (`PlaceAtMe`, the fallback player) live in a
/// file slot no plugin can own, the same `FF` prefix the game uses.
pub const DYNAMIC_FILE: u16 = 0xFF;
pub const PLAYER_OBJECT: u32 = 0x14;

const T: fn(&[u8; 4]) -> Tag = Tag::new;

/// What a VM object stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Handle {
    Form(GlobalFormId),
    Alias { quest: GlobalFormId, id: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RefState {
    pub base: Option<GlobalFormId>,
    pub cell: Option<GlobalFormId>,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: f32,
    pub enabled: bool,
    pub deleted: bool,
    pub activation_blocked: bool,
    pub opened: bool,
    pub locked: bool,
    pub lock_level: i32,
    pub display_name: Option<String>,
    pub inventory: BTreeMap<GlobalFormId, i32>,
    /// `(keyword, reference)` per XLKR.
    pub linked: Vec<(Option<GlobalFormId>, GlobalFormId)>,
    /// A door's far side: the other door and where to land.
    pub teleport: Option<(GlobalFormId, Transform)>,
    pub is_actor: bool,
}

impl RefState {
    fn new(base: Option<GlobalFormId>) -> Self {
        RefState {
            base,
            cell: None,
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: 1.0,
            enabled: true,
            deleted: false,
            activation_blocked: false,
            opened: false,
            locked: false,
            lock_level: 0,
            display_name: None,
            inventory: BTreeMap::new(),
            linked: Vec::new(),
            teleport: None,
            is_actor: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActorState {
    /// Current values keyed by lowercase actor value name.
    pub values: HashMap<String, f32>,
    pub base_values: HashMap<String, f32>,
    pub dead: bool,
    pub in_combat: bool,
    pub teammate: bool,
    pub ghost: bool,
    pub essential: bool,
    pub unconscious: bool,
    pub doing_favor: bool,
    pub factions: HashMap<GlobalFormId, i32>,
    pub spells: BTreeSet<GlobalFormId>,
    pub equipped: BTreeSet<GlobalFormId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObjectiveState {
    pub displayed: bool,
    pub completed: bool,
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuestState {
    pub def: Quest,
    pub running: bool,
    pub completed: bool,
    pub failed: bool,
    pub active: bool,
    /// The last stage set, what `GetCurrentStageID` returns.
    pub current: u16,
    /// The highest stage set, what `GetStage` returns.
    pub highest: u16,
    pub done: BTreeSet<u16>,
    pub objectives: BTreeMap<u16, ObjectiveState>,
    pub aliases: HashMap<u32, Option<GlobalFormId>>,
}

/// Work a native asked the VM to do.
#[derive(Debug, Clone)]
pub enum Pending {
    Event {
        object: ObjectId,
        name: String,
        args: Vec<Value>,
    },
    Call {
        object: ObjectId,
        function: String,
        args: Vec<Value>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdateTimer {
    pub object: ObjectId,
    pub due: f64,
    pub interval: Option<f64>,
    pub game_time: bool,
}

/// Allocates VM objects on the world's behalf. Inside a native this is
/// `NativeCall::create_object`, outside it is `Vm::create_object_of`.
pub type Alloc<'a> = &'a mut dyn FnMut(&str, &str) -> ObjectId;

pub struct World {
    order: LoadOrder,
    editor_ids: HashMap<String, GlobalFormId>,
    objects: HashMap<Handle, ObjectId>,
    handles: HashMap<ObjectId, Handle>,
    refs: HashMap<GlobalFormId, RefState>,
    actors: HashMap<GlobalFormId, ActorState>,
    quests: HashMap<GlobalFormId, QuestState>,
    globals: HashMap<GlobalFormId, f32>,
    form_lists: HashMap<GlobalFormId, Vec<GlobalFormId>>,
    next_dynamic: u32,
    player: Option<GlobalFormId>,
    rng: u64,
    pub clock: GameClock,
    pub pending: Vec<Pending>,
    pub timers: Vec<UpdateTimer>,
    pub notifications: Vec<String>,
    pub message_boxes: Vec<String>,
    pub animation_events: Vec<(GlobalFormId, String)>,
    pub player_controls: bool,
    pub ini: HashMap<String, String>,
}

impl World {
    pub fn new(order: LoadOrder) -> World {
        let mut editor_ids = HashMap::new();
        for (file_index, file) in order.files().iter().enumerate() {
            for (i, rec) in file.plugin.records.iter().enumerate() {
                let id = order.resolve(file_index, rec.form_id);
                let is_winner = order.winner_ref(id)
                    == Some(RecordRef {
                        file: file_index,
                        record: i,
                    });
                if !is_winner {
                    continue;
                }
                if let Some(edid) = rec.editor_id() {
                    editor_ids.insert(edid.to_ascii_lowercase(), id);
                }
            }
        }
        World {
            order,
            editor_ids,
            objects: HashMap::new(),
            handles: HashMap::new(),
            refs: HashMap::new(),
            actors: HashMap::new(),
            quests: HashMap::new(),
            globals: HashMap::new(),
            form_lists: HashMap::new(),
            next_dynamic: 0x1000,
            player: None,
            rng: 0x9E37_79B9_7F4A_7C15,
            clock: GameClock::default(),
            pending: Vec::new(),
            timers: Vec::new(),
            notifications: Vec::new(),
            message_boxes: Vec::new(),
            animation_events: Vec::new(),
            player_controls: true,
            ini: HashMap::new(),
        }
    }

    pub fn order(&self) -> &LoadOrder {
        &self.order
    }

    // ----- form lookup -----

    pub fn form(&self, editor_id: &str) -> Option<GlobalFormId> {
        self.editor_ids
            .get(&editor_id.to_ascii_lowercase())
            .copied()
    }

    pub fn record(&self, id: GlobalFormId) -> Option<&Record> {
        self.order.winner(id)
    }

    pub fn exists(&self, id: GlobalFormId) -> bool {
        id.file == DYNAMIC_FILE && self.refs.contains_key(&id) || self.order.contains(id)
    }

    pub fn tag_of(&self, id: GlobalFormId) -> Option<Tag> {
        if id.file == DYNAMIC_FILE {
            return self
                .refs
                .get(&id)
                .map(|r| if r.is_actor { T(b"ACHR") } else { T(b"REFR") });
        }
        self.record(id).map(|r| r.tag)
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
            return r.base.and_then(|b| self.name_of(b));
        }
        let rref = self.order.winner_ref(id)?;
        let rec = self.order.record(rref);
        if matches!(rec.tag.as_bytes(), b"REFR" | b"ACHR") {
            let raw = rec.find(T(b"NAME"))?;
            let base = self.order.resolve(rref.file, le_u32(raw)?);
            return self.name_of(base);
        }
        let localized = self.order.file(rref.file).plugin.header.is_localized();
        match text(rec.find(T(b"FULL"))?, localized) {
            Text::Plain(s) => Some(s),
            Text::Localized(n) => Some(format!("<string {n}>")),
        }
    }

    pub fn runtime_id(&self, id: GlobalFormId) -> u32 {
        if id.file == DYNAMIC_FILE {
            0xFF00_0000 | (id.object & 0x00FF_FFFF)
        } else {
            self.order.runtime_id(id)
        }
    }

    /// The inverse of [`World::runtime_id`].
    pub fn from_runtime_id(&self, form_id: u32) -> Option<GlobalFormId> {
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

    /// Resolve a raw FormID found in the winning record of `owner`.
    pub fn resolve_from(&self, owner: GlobalFormId, raw: u32) -> Option<GlobalFormId> {
        let rref = self.order.winner_ref(owner)?;
        Some(self.order.resolve(rref.file, raw))
    }

    pub fn keywords(&self, id: GlobalFormId) -> Vec<GlobalFormId> {
        let Some(rref) = self.order.winner_ref(id) else {
            return Vec::new();
        };
        let rec = self.order.record(rref);
        let Some(kwda) = rec.find(T(b"KWDA")) else {
            return Vec::new();
        };
        kwda.as_chunks::<4>()
            .0
            .iter()
            .map(|c| self.order.resolve(rref.file, u32::from_le_bytes(*c)))
            .collect()
    }

    pub fn has_keyword(&mut self, id: GlobalFormId, keyword: GlobalFormId) -> bool {
        let target = self.base_of(id).unwrap_or(id);
        self.keywords(target).contains(&keyword)
    }

    /// The Papyrus class a form answers to when no script is attached.
    pub fn class_of(&self, handle: Handle) -> &'static str {
        match handle {
            Handle::Alias { .. } => "ReferenceAlias",
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

    // ----- VM objects -----

    pub fn handle_of(&self, object: ObjectId) -> Option<Handle> {
        self.handles.get(&object).copied()
    }

    pub fn form_of(&self, object: ObjectId) -> Option<GlobalFormId> {
        match self.handle_of(object)? {
            Handle::Form(id) => Some(id),
            Handle::Alias { .. } => None,
        }
    }

    pub fn object_id(&self, handle: Handle) -> Option<ObjectId> {
        self.objects.get(&handle).copied()
    }

    /// The VM object for a handle, created on first use.
    pub fn object_of(&mut self, handle: Handle, alloc: Alloc<'_>) -> ObjectId {
        if let Some(id) = self.objects.get(&handle) {
            return *id;
        }
        let name = match handle {
            Handle::Form(id) => self
                .editor_id(id)
                .unwrap_or_else(|| format!("{:08X}", self.runtime_id(id))),
            Handle::Alias { quest, id } => {
                let alias = self
                    .quest_state(quest)
                    .and_then(|q| q.def.alias(id).map(|a| a.name.clone()));
                alias.unwrap_or_else(|| format!("alias {id}"))
            }
        };
        let class = self.class_of(handle);
        let object = alloc(&name, class);
        self.objects.insert(handle, object);
        self.handles.insert(object, handle);
        object
    }

    pub fn form_value(&mut self, id: Option<GlobalFormId>, alloc: Alloc<'_>) -> Value {
        match id {
            Some(id) if self.exists(id) => Value::Object(self.object_of(Handle::Form(id), alloc)),
            _ => Value::None,
        }
    }

    pub fn form_arg(&self, v: &Value) -> Option<GlobalFormId> {
        v.as_object().and_then(|o| self.form_of(o))
    }

    // ----- references -----

    fn load_ref(&mut self, id: GlobalFormId) -> Option<RefState> {
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
                st.is_actor = true;
                st
            }
            _ => return None,
        };
        st.enabled = rec.flags & flags::INITIALLY_DISABLED == 0;
        st.cell = rec.placement.cell.map(|c| self.order.resolve(file, c));
        if let Some(base) = st.base {
            st.inventory = self.base_inventory(base);
        }
        Some(st)
    }

    fn base_inventory(&self, base: GlobalFormId) -> BTreeMap<GlobalFormId, i32> {
        let mut out = BTreeMap::new();
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
        if let Some(p) = self.player {
            return p;
        }
        let shipped = GlobalFormId {
            file: 0,
            object: PLAYER_OBJECT,
        };
        let id = if self.reference(shipped).is_some() {
            shipped
        } else {
            self.next_dynamic += 1;
            let id = GlobalFormId {
                file: DYNAMIC_FILE,
                object: self.next_dynamic,
            };
            let mut st = RefState::new(None);
            st.is_actor = true;
            st.display_name = Some("Player".into());
            self.refs.insert(id, st);
            id
        };
        self.player = Some(id);
        id
    }

    pub fn actor(&mut self, id: GlobalFormId) -> Option<&mut ActorState> {
        if !self.reference(id).is_some_and(|r| r.is_actor) {
            return None;
        }
        Some(self.actors.entry(id).or_insert_with(|| {
            let mut a = ActorState::default();
            for (name, v) in [("health", 100.0), ("magicka", 100.0), ("stamina", 100.0)] {
                a.values.insert(name.into(), v);
                a.base_values.insert(name.into(), v);
            }
            a
        }))
    }

    pub fn actor_value(&mut self, id: GlobalFormId, name: &str) -> f32 {
        let key = name.to_ascii_lowercase();
        self.actor(id)
            .and_then(|a| a.values.get(&key).copied())
            .unwrap_or(0.0)
    }

    pub fn base_actor_value(&mut self, id: GlobalFormId, name: &str) -> f32 {
        let key = name.to_ascii_lowercase();
        self.actor(id)
            .and_then(|a| a.base_values.get(&key).copied())
            .unwrap_or(0.0)
    }

    pub fn set_actor_value(&mut self, id: GlobalFormId, name: &str, value: f32) {
        let key = name.to_ascii_lowercase();
        if let Some(a) = self.actor(id) {
            a.values.insert(key.clone(), value);
            a.base_values.insert(key, value);
        }
    }

    pub fn mod_actor_value(&mut self, id: GlobalFormId, name: &str, delta: f32) {
        let key = name.to_ascii_lowercase();
        if let Some(a) = self.actor(id) {
            *a.values.entry(key).or_insert(0.0) += delta;
            if key_is_health(name) && a.values["health"] <= 0.0 && !a.essential {
                a.dead = true;
            }
        }
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

    pub fn distance(&mut self, a: GlobalFormId, b: GlobalFormId) -> f32 {
        let (Some(pa), Some(pb)) = (
            self.reference(a).map(|r| r.position),
            self.reference(b).map(|r| r.position),
        ) else {
            return 0.0;
        };
        ((pa[0] - pb[0]).powi(2) + (pa[1] - pb[1]).powi(2) + (pa[2] - pb[2]).powi(2)).sqrt()
    }

    /// Activate `target` by `activator`: default processing for doors and
    /// containers unless the reference blocks activation, then `OnActivate`
    /// to the target's scripts when `send_event` is set. Returns false when
    /// the target is not a reference.
    pub fn activate(
        &mut self,
        target: GlobalFormId,
        activator: GlobalFormId,
        send_event: bool,
        alloc: Alloc<'_>,
    ) -> bool {
        let Some((blocked, base, teleport)) = self
            .reference(target)
            .map(|r| (r.activation_blocked, r.base, r.teleport))
        else {
            return false;
        };
        if !blocked {
            match base.and_then(|b| self.tag_of(b)).map(|t| *t.as_bytes()) {
                Some([b'D', b'O', b'O', b'R']) => {
                    if let Some((far_door, landing)) = teleport {
                        let far_cell = self.reference(far_door).and_then(|d| d.cell);
                        if let Some(r) = self.reference(activator) {
                            r.position = landing.position;
                            r.rotation = landing.rotation;
                            r.cell = far_cell;
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
        true
    }

    // ----- quests -----

    pub fn quest_state(&mut self, id: GlobalFormId) -> Option<&mut QuestState> {
        if !self.quests.contains_key(&id) {
            let rref = self.order.winner_ref(id)?;
            let rec = self.order.record(rref);
            let localized = self.order.file(rref.file).plugin.header.is_localized();
            let def = Quest::from_record(rec, localized).ok()?;
            let aliases = def
                .aliases
                .iter()
                .map(|a| (a.id, a.forced_ref.map(|r| self.order.resolve(rref.file, r))))
                .collect();
            self.quests.insert(
                id,
                QuestState {
                    def,
                    running: false,
                    completed: false,
                    failed: false,
                    active: false,
                    current: 0,
                    highest: 0,
                    done: BTreeSet::new(),
                    objectives: BTreeMap::new(),
                    aliases,
                },
            );
        }
        self.quests.get_mut(&id)
    }

    pub fn quest(&self, id: GlobalFormId) -> Option<&QuestState> {
        self.quests.get(&id)
    }

    /// `Quest.Start`: true when the quest was not running. Delivers `OnInit`
    /// to the quest's scripts, then sets the start-up stage.
    pub fn start_quest(&mut self, id: GlobalFormId, alloc: Alloc<'_>) -> bool {
        let Some(q) = self.quest_state(id) else {
            return false;
        };
        if q.running {
            return false;
        }
        q.running = true;
        q.active = true;
        let startup = q.def.startup_stage();
        let object = self.object_of(Handle::Form(id), alloc);
        self.pending.push(Pending::Event {
            object,
            name: "OnInit".into(),
            args: Vec::new(),
        });
        if let Some(stage) = startup {
            self.set_stage(id, stage, alloc);
        }
        true
    }

    pub fn stop_quest(&mut self, id: GlobalFormId) -> bool {
        match self.quest_state(id) {
            Some(q) if q.running => {
                q.running = false;
                q.active = false;
                true
            }
            _ => false,
        }
    }

    /// `Quest.SetStage`: starts the quest when needed, records the stage,
    /// applies the log entry flags, and queues the stage fragment. Returns
    /// false for a stage the quest does not define.
    pub fn set_stage(&mut self, id: GlobalFormId, stage: u16, alloc: Alloc<'_>) -> bool {
        let Some(q) = self.quest_state(id) else {
            return false;
        };
        if q.def.stage(stage).is_none() {
            return false;
        }
        if !q.running && !q.completed {
            q.running = true;
            q.active = true;
            let object = self.object_of(Handle::Form(id), alloc);
            self.pending.push(Pending::Event {
                object,
                name: "OnInit".into(),
                args: Vec::new(),
            });
        }
        let Some(q) = self.quests.get_mut(&id) else {
            return false;
        };
        q.current = stage;
        q.highest = q.highest.max(stage);
        q.done.insert(stage);
        let Some(def_stage) = q.def.stage(stage) else {
            return false;
        };
        if def_stage.completes_quest() {
            q.completed = true;
            q.running = false;
        }
        if def_stage.fails_quest() {
            q.failed = true;
            q.running = false;
        }
        let fragments: Vec<String> = q
            .def
            .vmad
            .as_ref()
            .map(|v| {
                v.fragments
                    .iter()
                    .filter(|f| f.stage == stage)
                    .map(|f| f.function.clone())
                    .collect()
            })
            .unwrap_or_default();
        if !fragments.is_empty() {
            let object = self.object_of(Handle::Form(id), alloc);
            for function in fragments {
                self.pending.push(Pending::Call {
                    object,
                    function,
                    args: Vec::new(),
                });
            }
        }
        true
    }

    pub fn objective(&mut self, id: GlobalFormId, index: u16) -> Option<&mut ObjectiveState> {
        let q = self.quest_state(id)?;
        q.def.objective(index)?;
        Some(q.objectives.entry(index).or_default())
    }

    pub fn alias_ref(&mut self, quest: GlobalFormId, alias: u32) -> Option<GlobalFormId> {
        self.quest_state(quest)?
            .aliases
            .get(&alias)
            .copied()
            .flatten()
    }

    pub fn force_alias(
        &mut self,
        quest: GlobalFormId,
        alias: u32,
        target: Option<GlobalFormId>,
    ) -> bool {
        match self.quest_state(quest) {
            Some(q) if q.def.alias(alias).is_some() => {
                q.aliases.insert(alias, target);
                true
            }
            _ => false,
        }
    }

    // ----- globals, form lists, game settings -----

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

    /// A `GMST` by editor id: the first letter of the id gives the type.
    pub fn game_setting(&self, name: &str) -> Option<Value> {
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

    // ----- timers and randomness -----

    pub fn register_update(
        &mut self,
        object: ObjectId,
        seconds: f64,
        repeat: bool,
        game_time: bool,
    ) {
        self.unregister_update(object, game_time);
        let now = if game_time {
            self.clock.game_hours()
        } else {
            self.clock.real_seconds
        };
        self.timers.push(UpdateTimer {
            object,
            due: now + seconds,
            interval: repeat.then_some(seconds),
            game_time,
        });
    }

    pub fn unregister_update(&mut self, object: ObjectId, game_time: bool) {
        self.timers
            .retain(|t| !(t.object == object && t.game_time == game_time));
    }

    /// Move every due timer into `pending` as `OnUpdate` or
    /// `OnUpdateGameTime` events.
    pub fn fire_timers(&mut self) {
        let real = self.clock.real_seconds;
        let hours = self.clock.game_hours();
        let mut i = 0;
        while i < self.timers.len() {
            let t = &mut self.timers[i];
            let now = if t.game_time { hours } else { real };
            if now < t.due {
                i += 1;
                continue;
            }
            let name = if t.game_time {
                "OnUpdateGameTime"
            } else {
                "OnUpdate"
            };
            let object = t.object;
            match t.interval {
                Some(iv) => {
                    t.due += iv;
                    i += 1;
                }
                None => {
                    self.timers.remove(i);
                }
            }
            self.pending.push(Pending::Event {
                object,
                name: name.into(),
                args: Vec::new(),
            });
        }
    }

    pub fn random_u64(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    pub fn random_int(&mut self, min: i32, max: i32) -> i32 {
        let (lo, hi) = (min.min(max), min.max(max));
        let span = (hi as i64 - lo as i64 + 1) as u64;
        lo + (self.random_u64() % span) as i32
    }

    pub fn random_float(&mut self, min: f32, max: f32) -> f32 {
        let unit = (self.random_u64() >> 11) as f64 / (1u64 << 53) as f64;
        min + (max - min) * unit as f32
    }
}

fn key_is_health(name: &str) -> bool {
    name.eq_ignore_ascii_case("health")
}

fn le_u32(d: &[u8]) -> Option<u32> {
    (d.len() >= 4).then(|| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
}

fn le_f32(d: &[u8]) -> f32 {
    f32::from_le_bytes([d[0], d[1], d[2], d[3]])
}

fn text(d: &[u8], localized: bool) -> Text {
    match (localized, le_u32(d)) {
        (true, Some(n)) if d.len() == 4 => Text::Localized(n),
        _ => Text::Plain(zstring(d)),
    }
}

pub fn class_for_tag(tag: &[u8; 4]) -> &'static str {
    match tag {
        b"REFR" => "ObjectReference",
        b"ACHR" => "Actor",
        b"QUST" => "Quest",
        b"CELL" => "Cell",
        b"WRLD" => "WorldSpace",
        b"GLOB" => "GlobalVariable",
        b"KYWD" => "Keyword",
        b"NPC_" => "ActorBase",
        b"ACTI" => "Activator",
        b"DOOR" => "Door",
        b"CONT" => "Container",
        b"STAT" => "Static",
        b"MISC" => "MiscObject",
        b"WEAP" => "Weapon",
        b"ARMO" => "Armor",
        b"BOOK" => "Book",
        b"ALCH" => "Potion",
        b"INGR" => "Ingredient",
        b"KEYM" => "Key",
        b"FLST" => "FormList",
        b"FACT" => "Faction",
        b"SPEL" => "Spell",
        b"LCTN" => "Location",
        b"MGEF" => "MagicEffect",
        b"FURN" => "Furniture",
        b"LIGH" => "Light",
        b"MESG" => "Message",
        b"RACE" => "Race",
        b"PACK" => "Package",
        b"SCEN" => "Scene",
        b"SNDR" => "Sound",
        b"IDLE" => "Idle",
        b"PERK" => "Perk",
        b"SHOU" => "Shout",
        b"WOOP" => "WordOfPower",
        b"ENCH" => "Enchantment",
        b"AMMO" => "Ammo",
        b"LVLI" => "LeveledItem",
        b"LVLN" => "LeveledActor",
        b"IMAD" => "ImageSpaceModifier",
        b"WTHR" => "Weather",
        b"TXST" => "TextureSet",
        b"OTFT" => "Outfit",
        _ => "Form",
    }
}

pub fn type_code_for_tag(tag: &[u8; 4]) -> i32 {
    match tag {
        b"KYWD" => 4,
        b"FACT" => 8,
        b"GLOB" => 9,
        b"CLAS" => 10,
        b"RACE" => 14,
        b"SPEL" => 22,
        b"ACTI" => 24,
        b"ARMO" => 26,
        b"BOOK" => 27,
        b"CONT" => 28,
        b"DOOR" => 29,
        b"INGR" => 30,
        b"LIGH" => 31,
        b"MISC" => 32,
        b"STAT" => 34,
        b"FURN" => 40,
        b"WEAP" => 41,
        b"AMMO" => 42,
        b"NPC_" => 43,
        b"KEYM" => 45,
        b"ALCH" => 46,
        b"WRLD" => 59,
        b"CELL" => 60,
        b"REFR" => 61,
        b"ACHR" => 62,
        b"QUST" => 77,
        b"FLST" => 91,
        b"LCTN" => 104,
        _ => 0,
    }
}
