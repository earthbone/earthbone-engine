// SPDX-License-Identifier: GPL-3.0-or-later
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

//! The game runtime for the Skyrim open engine: a load order becomes a
//! [`World`] (the form table plus run-time state), scripts named by `VMAD`
//! subrecords are attached to VM objects with their property values, the
//! base game's natives are registered, and a clock drives the VM.
//!
//! Round 2 of `ENGINE-PLAN.md`. The design rule from section 3 of the
//! research map holds: every system reads forms by resolved FormID from the
//! load order, so plugins, xEdit output and patchers keep their meaning.
//!
//! ```no_run
//! use soe_esm::{LoadOrder, Plugin};
//! use soe_game::Game;
//!
//! let mut order = LoadOrder::new();
//! order.add("Skyrim.esm", Plugin::parse(&std::fs::read("Data/Skyrim.esm").unwrap()).unwrap()).unwrap();
//! let mut game = Game::new(order);
//! game.load_script(&std::fs::read("Data/Scripts/MQ101.pex").unwrap()).unwrap();
//! game.start();
//! game.tick(1.0 / 60.0);
//! ```

pub mod clock;
pub mod harness;
pub mod natives;
pub mod quest;
pub mod vmad;
pub mod world;

use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use soe_esm::{GlobalFormId, LoadOrder, RecordRef, Tag};
use soe_papyrus::value::ElementKind;
use soe_papyrus::{ArrayRef, ObjectId, ThreadId, Value, Vm, VmError};

pub use clock::GameClock;
pub use natives::Registered;
pub use world::{Handle, Pending, World};

use vmad::{ObjectRef, PropertyValue, ScriptEntry, Vmad};

/// What [`Game::bind`] found in the load order's `VMAD` subrecords.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BindReport {
    pub forms_with_scripts: usize,
    pub scripts_attached: usize,
    /// `(script, form)` pairs whose PEX is not loaded.
    pub scripts_missing: Vec<(String, GlobalFormId)>,
    pub properties_set: usize,
    /// `(script, property)` the loaded PEX does not declare.
    pub properties_missing: Vec<(String, String)>,
    pub quests_started: usize,
}

pub struct Game {
    pub vm: Vm,
    world: Rc<RefCell<World>>,
    pub registered: Registered,
    bound: Option<BindReport>,
    /// Instructions per pump round before the loop yields.
    pub budget: usize,
}

impl Game {
    pub fn new(order: LoadOrder) -> Game {
        let world = Rc::new(RefCell::new(World::new(order)));
        let mut vm = Vm::new();
        let registered = natives::register(&mut vm, &world);
        Game {
            vm,
            world,
            registered,
            bound: None,
            budget: 100_000,
        }
    }

    /// Register natives from another provider, an extender in the PAPER
    /// mould. First registration wins, and the returned list names every
    /// function this provider lost.
    pub fn add_natives(
        &mut self,
        provider: impl FnOnce(&mut Vm, &Rc<RefCell<World>>),
    ) -> Vec<(String, String, bool)> {
        let before = self.vm.natives.duplicates.len();
        provider(&mut self.vm, &self.world);
        self.vm.natives.duplicates[before..].to_vec()
    }

    pub fn load_script(&mut self, pex: &[u8]) -> Result<(), VmError> {
        self.vm.load_bytes(pex)
    }

    pub fn world(&self) -> Ref<'_, World> {
        self.world.borrow()
    }

    pub fn world_mut(&self) -> RefMut<'_, World> {
        self.world.borrow_mut()
    }

    pub fn shared_world(&self) -> Rc<RefCell<World>> {
        self.world.clone()
    }

    pub fn form(&self, editor_id: &str) -> Option<GlobalFormId> {
        self.world.borrow().form(editor_id)
    }

    pub fn object_of(&mut self, handle: Handle) -> ObjectId {
        let vm = &mut self.vm;
        self.world
            .borrow_mut()
            .object_of(handle, &mut |name, class| vm.create_object_of(name, class))
    }

    pub fn object_of_form(&mut self, id: GlobalFormId) -> ObjectId {
        self.object_of(Handle::Form(id))
    }

    /// Attach every script the load order names and fill its properties.
    /// Runs once, later calls return the first report.
    pub fn bind(&mut self) -> BindReport {
        if let Some(r) = &self.bound {
            return r.clone();
        }
        let mut report = BindReport::default();
        let carriers: Vec<(GlobalFormId, RecordRef, Tag, Vmad)> = {
            let world = self.world.borrow();
            let order = world.order();
            let mut out = Vec::new();
            for (file_index, file) in order.files().iter().enumerate() {
                for (i, rec) in file.plugin.records.iter().enumerate() {
                    let Some(raw) = rec.find(Tag::new(b"VMAD")) else {
                        continue;
                    };
                    let id = order.resolve(file_index, rec.form_id);
                    let rref = RecordRef {
                        file: file_index,
                        record: i,
                    };
                    if order.winner_ref(id) != Some(rref) {
                        continue;
                    }
                    if let Ok(vmad) = Vmad::parse(raw, rec.tag) {
                        out.push((id, rref, rec.tag, vmad));
                    }
                }
            }
            out
        };
        for (id, rref, _tag, vmad) in carriers {
            report.forms_with_scripts += 1;
            let object = self.object_of(Handle::Form(id));
            self.attach_all(object, id, rref.file, &vmad.scripts, &mut report);
            for alias in &vmad.aliases {
                let alias_id = u32::try_from(alias.object.alias).unwrap_or(u32::MAX);
                let handle = Handle::Alias {
                    quest: id,
                    id: alias_id,
                };
                let object = self.object_of(handle);
                self.attach_all(object, id, rref.file, &alias.scripts, &mut report);
            }
        }
        self.bound = Some(report.clone());
        report
    }

    fn attach_all(
        &mut self,
        object: ObjectId,
        owner: GlobalFormId,
        file: usize,
        scripts: &[ScriptEntry],
        report: &mut BindReport,
    ) {
        for entry in scripts {
            if entry.status == 3 {
                continue;
            }
            if self.vm.attach(object, &entry.name).is_err() {
                report.scripts_missing.push((entry.name.clone(), owner));
                continue;
            }
            report.scripts_attached += 1;
            for p in &entry.properties {
                if p.status == 3 {
                    continue;
                }
                let value = self.property_value(owner, file, &p.value);
                match self.vm.set_property(object, &entry.name, &p.name, value) {
                    Ok(()) => report.properties_set += 1,
                    Err(_) => report
                        .properties_missing
                        .push((entry.name.clone(), p.name.clone())),
                }
            }
        }
    }

    fn object_ref_value(&mut self, owner: GlobalFormId, file: usize, o: ObjectRef) -> Value {
        if o.form_id == 0 {
            return Value::None;
        }
        let target = self.world.borrow().order().resolve(file, o.form_id);
        let handle = match u32::try_from(o.alias) {
            Ok(alias) => Handle::Alias {
                quest: target,
                id: alias,
            },
            Err(_) => Handle::Form(target),
        };
        let exists = self.world.borrow().exists(target);
        if !exists {
            return Value::None;
        }
        let _ = owner;
        Value::Object(self.object_of(handle))
    }

    fn property_value(&mut self, owner: GlobalFormId, file: usize, v: &PropertyValue) -> Value {
        fn array(kind: ElementKind, items: Vec<Value>) -> Value {
            let arr = ArrayRef::new(kind, 0);
            *arr.items.borrow_mut() = items;
            Value::Array(arr)
        }
        match v {
            PropertyValue::Object(o) => self.object_ref_value(owner, file, *o),
            PropertyValue::Str(s) => Value::string(s),
            PropertyValue::Int(i) => Value::Int(*i),
            PropertyValue::Float(f) => Value::Float(*f),
            PropertyValue::Bool(b) => Value::Bool(*b),
            PropertyValue::Objects(v) => {
                let items = v
                    .iter()
                    .map(|o| self.object_ref_value(owner, file, *o))
                    .collect();
                array(ElementKind::Object, items)
            }
            PropertyValue::Strs(v) => array(
                ElementKind::Str,
                v.iter().map(|s| Value::string(s)).collect(),
            ),
            PropertyValue::Ints(v) => {
                array(ElementKind::Int, v.iter().map(|i| Value::Int(*i)).collect())
            }
            PropertyValue::Floats(v) => array(
                ElementKind::Float,
                v.iter().map(|f| Value::Float(*f)).collect(),
            ),
            PropertyValue::Bools(v) => array(
                ElementKind::Bool,
                v.iter().map(|b| Value::Bool(*b)).collect(),
            ),
        }
    }

    /// Bind scripts, start every quest flagged Start Game Enabled, and run
    /// the VM until it settles.
    pub fn start(&mut self) -> BindReport {
        let mut report = self.bind();
        let quests: Vec<GlobalFormId> = self
            .world
            .borrow()
            .order()
            .winners_of(Tag::new(b"QUST"))
            .map(|(id, _)| id)
            .collect();
        for id in quests {
            let starts = self
                .world
                .borrow_mut()
                .quest_state(id)
                .is_some_and(|q| q.def.starts_enabled());
            if starts && self.start_quest(id) {
                report.quests_started += 1;
            }
        }
        self.run_until_idle(64);
        self.bound = Some(report.clone());
        report
    }

    pub fn start_quest(&mut self, id: GlobalFormId) -> bool {
        let vm = &mut self.vm;
        self.world
            .borrow_mut()
            .start_quest(id, &mut |n, c| vm.create_object_of(n, c))
    }

    pub fn set_stage(&mut self, id: GlobalFormId, stage: u16) -> bool {
        let vm = &mut self.vm;
        self.world
            .borrow_mut()
            .set_stage(id, stage, &mut |n, c| vm.create_object_of(n, c))
    }

    /// Activate a reference the way a player click would, then settle.
    pub fn activate(&mut self, target: GlobalFormId, activator: GlobalFormId) -> bool {
        let ok = {
            let vm = &mut self.vm;
            self.world
                .borrow_mut()
                .activate(target, activator, true, &mut |n, c| {
                    vm.create_object_of(n, c)
                })
        };
        self.run_until_idle(64);
        ok
    }

    /// Deliver an event to a form's scripts and settle.
    pub fn send_event(&mut self, id: GlobalFormId, name: &str, args: &[Value]) -> Vec<ThreadId> {
        let object = self.object_of_form(id);
        let threads = self.vm.send_event(object, name, args);
        self.run_until_idle(64);
        threads
    }

    pub fn player(&mut self) -> GlobalFormId {
        self.world.borrow_mut().player()
    }

    /// Advance real time: the clock, the VM scheduler, due update timers,
    /// then run until the VM settles.
    pub fn tick(&mut self, real_seconds: f64) {
        {
            let mut w = self.world.borrow_mut();
            w.clock.advance(real_seconds);
            w.fire_timers();
        }
        self.vm.advance(real_seconds);
        self.run_until_idle(64);
    }

    /// Drain queued events and calls into the VM. Returns how many.
    pub fn pump(&mut self) -> usize {
        let pending: Vec<Pending> = std::mem::take(&mut self.world.borrow_mut().pending);
        let n = pending.len();
        for p in pending {
            match p {
                Pending::Event { object, name, args } => {
                    self.vm.send_event(object, &name, &args);
                }
                Pending::Call {
                    object,
                    function,
                    args,
                } => {
                    if let Err(e) = self.vm.call(object, &function, &args) {
                        self.vm
                            .log
                            .push(self.vm.time(), format!("fragment {function}: {e}"));
                    }
                }
            }
        }
        n
    }

    /// Alternate VM runs and pumps until nothing is runnable and nothing is
    /// queued, or `max_rounds` passes. Returns true when settled.
    pub fn run_until_idle(&mut self, max_rounds: usize) -> bool {
        for _ in 0..max_rounds {
            self.pump();
            self.vm.run(self.budget);
            let queued = !self.world.borrow().pending.is_empty();
            if self.vm.idle() && !queued {
                return true;
            }
        }
        false
    }

    pub fn traces(&self) -> Vec<String> {
        self.vm
            .log
            .entries
            .iter()
            .map(|e| e.message.clone())
            .collect()
    }
}
