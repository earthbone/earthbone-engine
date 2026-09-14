// SPDX-License-Identifier: GPL-3.0-or-later

//! The virtual machine: scripts, objects with attached script instances,
//! threads with their own frame stacks, a scheduler with sleeping and latent
//! threads, and the native function registry.
//!
//! Script errors follow the game's rule: they are logged and the thread
//! carries on with `None`, they never abort the VM. Only misuse of the host
//! API returns [`VmError`].
//!
//! Function lookup on an object walks each attached instance in attach order,
//! and inside an instance walks the script chain from the most derived script
//! to the root, checking the current state and then the empty state of each.
//! Natives are found by the name of the script that declares them, so a
//! native on `ObjectReference` is reached from an `Actor` through the chain,
//! and the registry can declare parents for classes whose PEX is not loaded.

// A running thread always has a frame and a spawned thread always has an
// index. The unwraps below state those invariants, a broken one is a VM bug.
#![allow(clippy::unwrap_used)]

use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

use crate::pex::{Function, Object, Opcode, Operand, Script};
use crate::value::{ArrayRef, ElementKind, ObjectId, Value};

pub mod state;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmError {
    UnknownScript(String),
    UnknownObject(ObjectId),
    UnknownFunction {
        class: String,
        name: String,
    },
    Pex(String),
    /// A saved state that cannot be read or does not match the loaded scripts.
    State(String),
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::UnknownScript(s) => write!(f, "script {s} is not loaded"),
            VmError::UnknownObject(o) => write!(f, "object {} does not exist", o.0),
            VmError::UnknownFunction { class, name } => {
                write!(f, "{class} has no function {name}")
            }
            VmError::Pex(e) => write!(f, "{e}"),
            VmError::State(e) => write!(f, "saved state: {e}"),
        }
    }
}

impl std::error::Error for VmError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LatentId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceId(pub usize);

#[derive(Debug, Clone, PartialEq)]
pub enum ThreadState {
    Runnable,
    /// Sleeping until the VM clock reaches the time.
    Sleeping(f64),
    /// Waiting for a native to finish.
    Latent(LatentId),
    Done,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub time: f64,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Log {
    pub entries: Vec<LogEntry>,
}

impl Log {
    pub fn push(&mut self, time: f64, message: impl Into<String>) {
        self.entries.push(LogEntry {
            time,
            message: message.into(),
        });
    }
    pub fn messages(&self) -> Vec<&str> {
        self.entries.iter().map(|e| e.message.as_str()).collect()
    }
    pub fn contains(&self, needle: &str) -> bool {
        self.entries.iter().any(|e| e.message.contains(needle))
    }
}

/// What a native hands back.
pub enum NativeReturn {
    Value(Value),
    /// The thread sleeps this many VM seconds, then continues with `None`.
    Wait(f32),
    /// The thread waits until [`Vm::resolve_latent`] is called with the id.
    Latent(LatentId),
}

/// The call a native receives.
pub struct NativeCall<'a> {
    pub self_object: Option<ObjectId>,
    pub args: Vec<Value>,
    pub time: f64,
    pub log: &'a mut Log,
    latent_counter: &'a mut u64,
    objects: &'a mut HashMap<ObjectId, GameObject>,
    next_object: &'a mut u64,
}

impl NativeCall<'_> {
    pub fn arg(&self, i: usize) -> Value {
        self.args.get(i).cloned().unwrap_or(Value::None)
    }
    /// Allocate a latent id for a call that finishes later.
    pub fn defer(&mut self) -> LatentId {
        *self.latent_counter += 1;
        LatentId(*self.latent_counter)
    }
    /// Create an object of a base class from inside a native, so a host can
    /// hand back forms the VM has not seen yet (`Game.GetForm`, a linked
    /// reference, a quest alias). Same as [`Vm::create_object_of`].
    pub fn create_object(&mut self, name: &str, class: &str) -> ObjectId {
        *self.next_object += 1;
        let id = ObjectId(*self.next_object);
        self.objects.insert(
            id,
            GameObject {
                name: name.to_string(),
                class: Some(class.to_string()),
                instances: Vec::new(),
            },
        );
        id
    }
    pub fn object_name(&self, id: ObjectId) -> Option<&str> {
        self.objects.get(&id).map(|o| o.name.as_str())
    }
}

pub type NativeFn = Box<dyn FnMut(&mut NativeCall<'_>) -> NativeReturn>;

#[derive(Default)]
pub struct NativeRegistry {
    methods: HashMap<(String, String), NativeFn>,
    globals: HashMap<(String, String), NativeFn>,
    parents: HashMap<String, String>,
    /// `(class, function, global)` for every registration that lost to an
    /// earlier one. First registered wins.
    pub duplicates: Vec<(String, String, bool)>,
}

impl NativeRegistry {
    fn key(class: &str, name: &str) -> (String, String) {
        (class.to_ascii_lowercase(), name.to_ascii_lowercase())
    }

    /// A native called on an object of `class` or a class deriving from it.
    pub fn register_method(&mut self, class: &str, name: &str, f: NativeFn) -> bool {
        let key = Self::key(class, name);
        if self.methods.contains_key(&key) {
            self.duplicates
                .push((class.to_string(), name.to_string(), false));
            return false;
        }
        self.methods.insert(key, f);
        true
    }

    /// A global native, `Class.Function(...)`.
    pub fn register_global(&mut self, class: &str, name: &str, f: NativeFn) -> bool {
        let key = Self::key(class, name);
        if self.globals.contains_key(&key) {
            self.duplicates
                .push((class.to_string(), name.to_string(), true));
            return false;
        }
        self.globals.insert(key, f);
        true
    }

    /// Declare the parent of a class whose PEX is not loaded, so method
    /// lookup can keep walking.
    pub fn declare_parent(&mut self, class: &str, parent: &str) {
        self.parents
            .insert(class.to_ascii_lowercase(), parent.to_ascii_lowercase());
    }

    pub fn has_method(&self, class: &str, name: &str) -> bool {
        self.methods.contains_key(&Self::key(class, name))
    }

    pub fn has_global(&self, class: &str, name: &str) -> bool {
        self.globals.contains_key(&Self::key(class, name))
    }
}

/// A loaded script with its functions shared behind `Rc` so frames can hold
/// them.
struct LoadedScript {
    name: String,
    parent: Option<String>,
    object: Object,
    /// `(state, function)` both lowercased.
    functions: HashMap<(String, String), Rc<Function>>,
    getters: HashMap<String, Rc<Function>>,
    setters: HashMap<String, Rc<Function>>,
}

struct Instance {
    object: ObjectId,
    /// Most derived first.
    chain: Vec<Rc<LoadedScript>>,
    state: String,
    vars: HashMap<String, Value>,
    var_types: HashMap<String, String>,
}

#[derive(Debug)]
pub struct GameObject {
    pub name: String,
    /// The base class the object answers to when no attached script
    /// defines a function, `Quest` for a quest form, `Actor` for an actor
    /// reference. `None` for objects created with [`Vm::create_object`].
    pub class: Option<String>,
    pub instances: Vec<InstanceId>,
}

struct Frame {
    instance: Option<InstanceId>,
    self_object: Option<ObjectId>,
    /// The script that defines the running function, for `CallParent`.
    script: Option<Rc<LoadedScript>>,
    function: Rc<Function>,
    pc: usize,
    locals: HashMap<String, Value>,
    local_types: HashMap<String, String>,
    /// Where the caller wants the return value.
    ret_dest: Option<String>,
}

struct Thread {
    id: ThreadId,
    frames: Vec<Frame>,
    state: ThreadState,
    result: Value,
    /// Variable in the top frame waiting for a latent native.
    pending_dest: Option<String>,
}

const MAX_DEPTH: usize = 128;

#[derive(Default)]
pub struct Vm {
    scripts: HashMap<String, Rc<LoadedScript>>,
    objects: HashMap<ObjectId, GameObject>,
    next_object: u64,
    instances: Vec<Instance>,
    threads: Vec<Thread>,
    next_thread: u64,
    latent_counter: u64,
    latent_waiters: HashMap<LatentId, ThreadId>,
    pub natives: NativeRegistry,
    pub log: Log,
    time: f64,
}

impl Vm {
    pub fn new() -> Vm {
        Vm::default()
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    // ----- loading -----

    pub fn load(&mut self, script: Script) {
        for object in script.objects {
            let mut functions = HashMap::new();
            for st in &object.states {
                for f in &st.functions {
                    functions.insert(
                        (st.name.to_ascii_lowercase(), f.name.to_ascii_lowercase()),
                        Rc::new(f.clone()),
                    );
                }
            }
            let mut getters = HashMap::new();
            let mut setters = HashMap::new();
            for p in &object.properties {
                if let Some(g) = &p.getter {
                    getters.insert(p.name.to_ascii_lowercase(), Rc::new(g.clone()));
                }
                if let Some(s) = &p.setter {
                    setters.insert(p.name.to_ascii_lowercase(), Rc::new(s.clone()));
                }
            }
            let name = object.name.clone();
            let parent = if object.parent.is_empty() {
                None
            } else {
                Some(object.parent.clone())
            };
            let loaded = LoadedScript {
                name: name.clone(),
                parent,
                object,
                functions,
                getters,
                setters,
            };
            self.scripts
                .insert(name.to_ascii_lowercase(), Rc::new(loaded));
        }
    }

    pub fn load_bytes(&mut self, data: &[u8]) -> Result<(), VmError> {
        let script = crate::pex::parse(data).map_err(|e| VmError::Pex(e.to_string()))?;
        self.load(script);
        Ok(())
    }

    pub fn has_script(&self, name: &str) -> bool {
        self.scripts.contains_key(&name.to_ascii_lowercase())
    }

    fn script(&self, name: &str) -> Option<Rc<LoadedScript>> {
        self.scripts.get(&name.to_ascii_lowercase()).cloned()
    }

    /// The loaded scripts from `name` up to the root, most derived first,
    /// plus the first parent name that is not loaded.
    fn chain_of(&self, name: &str) -> (Vec<Rc<LoadedScript>>, Option<String>) {
        let mut chain = Vec::new();
        let mut next = Some(name.to_string());
        while let Some(n) = next.take() {
            match self.script(&n) {
                Some(s) => {
                    next = s.parent.clone();
                    chain.push(s);
                }
                None => return (chain, Some(n)),
            }
            if chain.len() > 64 {
                break;
            }
        }
        (chain, None)
    }

    // ----- objects -----

    pub fn create_object(&mut self, name: &str) -> ObjectId {
        self.new_object(name, None)
    }

    /// An object with a base class. Natives registered for that class and
    /// for the parents declared through [`NativeRegistry::declare_parent`]
    /// are reachable on it without any script attached.
    pub fn create_object_of(&mut self, name: &str, class: &str) -> ObjectId {
        self.new_object(name, Some(class.to_string()))
    }

    fn new_object(&mut self, name: &str, class: Option<String>) -> ObjectId {
        self.next_object += 1;
        let id = ObjectId(self.next_object);
        self.objects.insert(
            id,
            GameObject {
                name: name.to_string(),
                class,
                instances: Vec::new(),
            },
        );
        id
    }

    pub fn object_class(&self, id: ObjectId) -> Option<&str> {
        self.objects.get(&id).and_then(|o| o.class.as_deref())
    }

    pub fn object_name(&self, id: ObjectId) -> Option<&str> {
        self.objects.get(&id).map(|o| o.name.as_str())
    }

    pub fn object(&self, id: ObjectId) -> Option<&GameObject> {
        self.objects.get(&id)
    }

    /// Attach a script to an object. Variables take their compiled defaults
    /// and the instance starts in the script's auto state.
    pub fn attach(&mut self, object: ObjectId, script: &str) -> Result<InstanceId, VmError> {
        if !self.objects.contains_key(&object) {
            return Err(VmError::UnknownObject(object));
        }
        let (chain, _) = self.chain_of(script);
        if chain.is_empty() {
            return Err(VmError::UnknownScript(script.to_string()));
        }
        let mut vars = HashMap::new();
        let mut var_types = HashMap::new();
        for s in chain.iter().rev() {
            for v in &s.object.variables {
                let key = v.name.to_ascii_lowercase();
                var_types.insert(key.clone(), v.type_name.clone());
                vars.insert(key, operand_to_value(&v.default, Some(&v.type_name)));
            }
        }
        let state = chain[0].object.auto_state.clone();
        let id = InstanceId(self.instances.len());
        self.instances.push(Instance {
            object,
            chain,
            state,
            vars,
            var_types,
        });
        self.objects.get_mut(&object).unwrap().instances.push(id);
        Ok(id)
    }

    fn instance_on(&self, object: ObjectId, script: &str) -> Option<InstanceId> {
        let obj = self.objects.get(&object)?;
        obj.instances.iter().copied().find(|i| {
            self.instances[i.0]
                .chain
                .iter()
                .any(|s| s.name.eq_ignore_ascii_case(script))
        })
    }

    /// Read a script variable, `::Name_var` for auto properties.
    pub fn get_var(&self, object: ObjectId, script: &str, var: &str) -> Option<Value> {
        let id = self.instance_on(object, script)?;
        self.instances[id.0]
            .vars
            .get(&var.to_ascii_lowercase())
            .cloned()
    }

    pub fn set_var(
        &mut self,
        object: ObjectId,
        script: &str,
        var: &str,
        value: Value,
    ) -> Result<(), VmError> {
        let id = self
            .instance_on(object, script)
            .ok_or_else(|| VmError::UnknownScript(script.to_string()))?;
        self.instances[id.0]
            .vars
            .insert(var.to_ascii_lowercase(), value);
        Ok(())
    }

    /// Fill an auto property, the way the loader fills properties from a
    /// plugin's VMAD data.
    pub fn set_property(
        &mut self,
        object: ObjectId,
        script: &str,
        property: &str,
        value: Value,
    ) -> Result<(), VmError> {
        let id = self
            .instance_on(object, script)
            .ok_or_else(|| VmError::UnknownScript(script.to_string()))?;
        let var = self.instances[id.0]
            .chain
            .iter()
            .find_map(|s| s.object.property(property).and_then(|p| p.auto_var.clone()))
            .ok_or_else(|| VmError::UnknownFunction {
                class: script.to_string(),
                name: property.to_string(),
            })?;
        self.instances[id.0]
            .vars
            .insert(var.to_ascii_lowercase(), value);
        Ok(())
    }

    pub fn get_property(&self, object: ObjectId, script: &str, property: &str) -> Option<Value> {
        let id = self.instance_on(object, script)?;
        let inst = &self.instances[id.0];
        let var = inst
            .chain
            .iter()
            .find_map(|s| s.object.property(property).and_then(|p| p.auto_var.clone()))?;
        inst.vars.get(&var.to_ascii_lowercase()).cloned()
    }

    pub fn state_of(&self, object: ObjectId, script: &str) -> Option<String> {
        let id = self.instance_on(object, script)?;
        Some(self.instances[id.0].state.clone())
    }

    // ----- threads -----

    fn spawn(&mut self, frame: Frame) -> ThreadId {
        self.next_thread += 1;
        let id = ThreadId(self.next_thread);
        self.threads.push(Thread {
            id,
            frames: vec![frame],
            state: ThreadState::Runnable,
            result: Value::None,
            pending_dest: None,
        });
        id
    }

    fn make_frame(
        instance: Option<InstanceId>,
        self_object: Option<ObjectId>,
        script: Option<Rc<LoadedScript>>,
        function: Rc<Function>,
        args: &[Value],
        ret_dest: Option<String>,
    ) -> Frame {
        let mut locals = HashMap::new();
        let mut local_types = HashMap::new();
        for (i, p) in function.params.iter().enumerate() {
            let key = p.name.to_ascii_lowercase();
            local_types.insert(key.clone(), p.type_name.clone());
            let v = args
                .get(i)
                .cloned()
                .unwrap_or_else(|| ElementKind::from_type(&p.type_name).default_value());
            locals.insert(key, v);
        }
        for l in &function.locals {
            let key = l.name.to_ascii_lowercase();
            local_types.insert(key.clone(), l.type_name.clone());
            locals.insert(key, default_for_type(&l.type_name));
        }
        Frame {
            instance,
            self_object,
            script,
            function,
            pc: 0,
            locals,
            local_types,
            ret_dest,
        }
    }

    /// Find a script function on an object. Returns the defining script, the
    /// instance, and the function.
    fn find_method(
        &self,
        object: ObjectId,
        name: &str,
        start_after: Option<&str>,
    ) -> Option<(InstanceId, Rc<LoadedScript>, Rc<Function>)> {
        let obj = self.objects.get(&object)?;
        let lname = name.to_ascii_lowercase();
        for &iid in &obj.instances {
            let inst = &self.instances[iid.0];
            let mut skipping = start_after.is_some();
            for s in &inst.chain {
                if skipping {
                    if s.name.eq_ignore_ascii_case(start_after.unwrap()) {
                        skipping = false;
                    }
                    continue;
                }
                let state = inst.state.to_ascii_lowercase();
                if let Some(f) = s
                    .functions
                    .get(&(state.clone(), lname.clone()))
                    .or_else(|| s.functions.get(&(String::new(), lname.clone())))
                {
                    return Some((iid, s.clone(), f.clone()));
                }
            }
        }
        None
    }

    /// Class names an object answers to, most derived first, including
    /// parents the registry declared for unloaded classes.
    fn class_names(&self, object: ObjectId) -> Vec<String> {
        let mut out = Vec::new();
        let Some(obj) = self.objects.get(&object) else {
            return out;
        };
        for &iid in &obj.instances {
            let inst = &self.instances[iid.0];
            for s in &inst.chain {
                out.push(s.name.to_ascii_lowercase());
            }
            let mut next = inst.chain.last().and_then(|s| s.parent.clone());
            while let Some(n) = next {
                let n = n.to_ascii_lowercase();
                next = self.natives.parents.get(&n).cloned();
                out.push(n);
            }
        }
        let mut next = obj.class.clone();
        while let Some(n) = next {
            let n = n.to_ascii_lowercase();
            next = self.natives.parents.get(&n).cloned();
            if !out.contains(&n) {
                out.push(n);
            }
        }
        out
    }

    /// Start a thread that calls a method on an object.
    pub fn call(
        &mut self,
        object: ObjectId,
        name: &str,
        args: &[Value],
    ) -> Result<ThreadId, VmError> {
        if !self.objects.contains_key(&object) {
            return Err(VmError::UnknownObject(object));
        }
        let (iid, script, function) =
            self.find_method(object, name, None)
                .ok_or_else(|| VmError::UnknownFunction {
                    class: self.objects[&object].name.clone(),
                    name: name.to_string(),
                })?;
        let frame = Self::make_frame(Some(iid), Some(object), Some(script), function, args, None);
        Ok(self.spawn(frame))
    }

    /// Start a thread that calls a global function of a script.
    pub fn call_global(
        &mut self,
        class: &str,
        name: &str,
        args: &[Value],
    ) -> Result<ThreadId, VmError> {
        let script = self
            .script(class)
            .ok_or_else(|| VmError::UnknownScript(class.to_string()))?;
        let function = script
            .functions
            .get(&(String::new(), name.to_ascii_lowercase()))
            .cloned()
            .ok_or_else(|| VmError::UnknownFunction {
                class: class.to_string(),
                name: name.to_string(),
            })?;
        let frame = Self::make_frame(None, None, Some(script), function, args, None);
        Ok(self.spawn(frame))
    }

    /// Deliver an event: one thread per attached script that handles it.
    pub fn send_event(&mut self, object: ObjectId, name: &str, args: &[Value]) -> Vec<ThreadId> {
        let mut out = Vec::new();
        let Some(obj) = self.objects.get(&object) else {
            return out;
        };
        let lname = name.to_ascii_lowercase();
        let mut targets = Vec::new();
        for &iid in &obj.instances {
            let inst = &self.instances[iid.0];
            let state = inst.state.to_ascii_lowercase();
            for s in &inst.chain {
                if let Some(f) = s
                    .functions
                    .get(&(state.clone(), lname.clone()))
                    .or_else(|| s.functions.get(&(String::new(), lname.clone())))
                {
                    if !f.is_native() {
                        targets.push((iid, s.clone(), f.clone()));
                    }
                    break;
                }
            }
        }
        for (iid, s, f) in targets {
            let frame = Self::make_frame(Some(iid), Some(object), Some(s), f, args, None);
            out.push(self.spawn(frame));
        }
        out
    }

    pub fn thread_state(&self, id: ThreadId) -> Option<ThreadState> {
        self.threads
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.state.clone())
    }

    /// The return value of a finished thread.
    pub fn thread_result(&self, id: ThreadId) -> Option<Value> {
        self.threads
            .iter()
            .find(|t| t.id == id && t.state == ThreadState::Done)
            .map(|t| t.result.clone())
    }

    pub fn idle(&self) -> bool {
        self.threads
            .iter()
            .all(|t| t.state != ThreadState::Runnable)
    }

    pub fn live_threads(&self) -> usize {
        self.threads
            .iter()
            .filter(|t| t.state != ThreadState::Done)
            .count()
    }

    /// Finish a latent native. The waiting thread receives the value and runs
    /// again on the next `run`.
    pub fn resolve_latent(&mut self, id: LatentId, value: Value) -> bool {
        let Some(tid) = self.latent_waiters.remove(&id) else {
            return false;
        };
        let Some(t) = self.threads.iter_mut().find(|t| t.id == tid) else {
            return false;
        };
        if let Some(dest) = t.pending_dest.take() {
            let idx = t.frames.len() - 1;
            let frame = &mut t.frames[idx];
            Self::store(&mut self.instances, frame, &dest, value);
        }
        t.state = ThreadState::Runnable;
        true
    }

    /// Move the clock and wake sleepers whose time has come.
    pub fn advance(&mut self, seconds: f64) {
        self.time += seconds;
        for t in &mut self.threads {
            if let ThreadState::Sleeping(until) = t.state {
                if until <= self.time {
                    t.state = ThreadState::Runnable;
                }
            }
        }
    }

    /// Run runnable threads round-robin until the budget of instructions is
    /// spent or nothing is runnable. Returns instructions executed.
    pub fn run(&mut self, budget: usize) -> usize {
        let slice = 64;
        let mut used = 0;
        while used < budget {
            let mut progressed = false;
            for i in 0..self.threads.len() {
                if self.threads[i].state != ThreadState::Runnable {
                    continue;
                }
                let n = self.step_thread(i, slice.min(budget - used));
                used += n;
                progressed |= n > 0;
                if used >= budget {
                    break;
                }
            }
            if !progressed {
                break;
            }
        }
        used
    }

    /// Run until no thread is runnable, at most `max` instructions. Sleeping
    /// threads are not woken, call [`Vm::advance`] for that. Returns whether
    /// the VM went idle.
    pub fn run_until_idle(&mut self, max: usize) -> bool {
        self.run(max);
        self.idle()
    }

    // ----- variable access inside a frame -----

    fn read_var(instances: &[Instance], frame: &Frame, name: &str) -> Value {
        let key = name.to_ascii_lowercase();
        if key == "self" {
            return frame.self_object.map(Value::Object).unwrap_or(Value::None);
        }
        if let Some(v) = frame.locals.get(&key) {
            return v.clone();
        }
        if let Some(iid) = frame.instance {
            if let Some(v) = instances[iid.0].vars.get(&key) {
                return v.clone();
            }
        }
        Value::None
    }

    fn store(instances: &mut [Instance], frame: &mut Frame, name: &str, value: Value) {
        let key = name.to_ascii_lowercase();
        if let Some(slot) = frame.locals.get_mut(&key) {
            *slot = value;
            return;
        }
        if let Some(iid) = frame.instance {
            if let Some(slot) = instances[iid.0].vars.get_mut(&key) {
                *slot = value;
                return;
            }
        }
        frame.locals.insert(key, value);
    }

    fn type_of(instances: &[Instance], frame: &Frame, name: &str) -> Option<String> {
        let key = name.to_ascii_lowercase();
        if let Some(t) = frame.local_types.get(&key) {
            return Some(t.clone());
        }
        if let Some(iid) = frame.instance {
            if let Some(t) = instances[iid.0].var_types.get(&key) {
                return Some(t.clone());
            }
        }
        None
    }

    fn operand(instances: &[Instance], frame: &Frame, op: &Operand) -> Value {
        match op {
            Operand::Identifier(name) => Self::read_var(instances, frame, name),
            other => operand_to_value(other, None),
        }
    }

    // ----- execution -----

    fn where_am_i(&self, t: usize) -> String {
        let f = self.threads[t].frames.last().unwrap();
        let script = f
            .script
            .as_ref()
            .map(|s| s.name.clone())
            .unwrap_or_default();
        match f.function.line_of(f.pc.saturating_sub(1)) {
            Some(line) => format!("{script}.{} line {line}", f.function.name),
            None => format!(
                "{script}.{} instruction {}",
                f.function.name,
                f.pc.saturating_sub(1)
            ),
        }
    }

    fn err(&mut self, t: usize, msg: &str) {
        let at = self.where_am_i(t);
        self.log.push(self.time, format!("error: {msg} ({at})"));
    }

    fn describe(&self, v: &Value) -> String {
        match v {
            Value::Object(o) => match self.objects.get(o) {
                Some(g) => {
                    let script = g
                        .instances
                        .first()
                        .and_then(|i| self.instances[i.0].chain.first())
                        .map(|s| s.name.clone())
                        .or_else(|| g.class.clone())
                        .unwrap_or_else(|| "Form".into());
                    format!("[{script} <{}>]", g.name)
                }
                None => v.to_string(),
            },
            Value::Array(a) => {
                let items = a.items.borrow();
                let parts: Vec<String> = items.iter().map(|v| self.describe(v)).collect();
                format!("[{}]", parts.join(", "))
            }
            other => other.to_string(),
        }
    }

    /// Finish the top frame and hand its value to the caller.
    fn return_from(&mut self, t: usize, value: Value) {
        let thread = &mut self.threads[t];
        let done = thread.frames.pop().unwrap();
        match thread.frames.last_mut() {
            Some(caller) => {
                if let Some(dest) = done.ret_dest {
                    Self::store(&mut self.instances, caller, &dest, value);
                }
            }
            None => {
                thread.result = value;
                thread.state = ThreadState::Done;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_call(
        &mut self,
        t: usize,
        instance: Option<InstanceId>,
        self_object: Option<ObjectId>,
        script: Option<Rc<LoadedScript>>,
        function: Rc<Function>,
        args: &[Value],
        ret_dest: Option<String>,
    ) {
        if self.threads[t].frames.len() >= MAX_DEPTH {
            self.err(t, "stack overflow, aborting the thread");
            let thread = &mut self.threads[t];
            thread.frames.clear();
            thread.state = ThreadState::Done;
            return;
        }
        let frame = Self::make_frame(instance, self_object, script, function, args, ret_dest);
        self.threads[t].frames.push(frame);
    }

    #[allow(clippy::too_many_arguments)]
    fn call_native(
        &mut self,
        t: usize,
        global: bool,
        class: &str,
        name: &str,
        self_object: Option<ObjectId>,
        args: Vec<Value>,
        dest: Option<String>,
    ) -> bool {
        let key = NativeRegistry::key(class, name);
        let table = if global {
            &mut self.natives.globals
        } else {
            &mut self.natives.methods
        };
        let Some(f) = table.get_mut(&key) else {
            return false;
        };
        let mut call = NativeCall {
            self_object,
            args,
            time: self.time,
            log: &mut self.log,
            latent_counter: &mut self.latent_counter,
            objects: &mut self.objects,
            next_object: &mut self.next_object,
        };
        let ret = f(&mut call);
        let thread = &mut self.threads[t];
        let frame = thread.frames.last_mut().unwrap();
        match ret {
            NativeReturn::Value(v) => {
                if let Some(d) = dest {
                    Self::store(&mut self.instances, frame, &d, v);
                }
            }
            NativeReturn::Wait(secs) => {
                if let Some(d) = dest {
                    Self::store(&mut self.instances, frame, &d, Value::None);
                }
                thread.state = ThreadState::Sleeping(self.time + secs.max(0.0) as f64);
            }
            NativeReturn::Latent(id) => {
                thread.pending_dest = dest;
                thread.state = ThreadState::Latent(id);
                self.latent_waiters.insert(id, thread.id);
            }
        }
        true
    }

    /// `Utility.Wait` and friends are the scheduler's own.
    fn builtin_global(
        &mut self,
        t: usize,
        class: &str,
        name: &str,
        args: &[Value],
        dest: Option<String>,
    ) -> bool {
        if !class.eq_ignore_ascii_case("utility") {
            return false;
        }
        let lname = name.to_ascii_lowercase();
        if lname == "wait" || lname == "waitmenumode" || lname == "waitgametime" {
            let secs = args.first().map(|v| v.as_float()).unwrap_or(0.0);
            let thread = &mut self.threads[t];
            let frame = thread.frames.last_mut().unwrap();
            if let Some(d) = dest {
                Self::store(&mut self.instances, frame, &d, Value::None);
            }
            thread.state = ThreadState::Sleeping(self.time + secs.max(0.0) as f64);
            return true;
        }
        false
    }

    /// `GotoState` and `GetState` live on every script object.
    fn builtin_method(
        &mut self,
        t: usize,
        object: ObjectId,
        name: &str,
        args: &[Value],
        dest: Option<String>,
    ) -> bool {
        let lname = name.to_ascii_lowercase();
        let frame_instance = self.threads[t].frames.last().unwrap().instance;
        let iid = match frame_instance.filter(|i| self.instances[i.0].object == object) {
            Some(i) => i,
            None => match self
                .objects
                .get(&object)
                .and_then(|o| o.instances.first().copied())
            {
                Some(i) => i,
                None => return false,
            },
        };
        match lname.as_str() {
            "gotostate" => {
                let new_state = args.first().map(|v| v.to_string()).unwrap_or_default();
                let old = std::mem::replace(&mut self.instances[iid.0].state, new_state.clone());
                self.send_event(object, "OnEndState", &[]);
                let _ = old;
                self.send_event(object, "OnBeginState", &[]);
                if let Some(d) = dest {
                    let frame = self.threads[t].frames.last_mut().unwrap();
                    Self::store(&mut self.instances, frame, &d, Value::None);
                }
                true
            }
            "getstate" => {
                let s = Value::string(&self.instances[iid.0].state);
                if let Some(d) = dest {
                    let frame = self.threads[t].frames.last_mut().unwrap();
                    Self::store(&mut self.instances, frame, &d, s);
                }
                true
            }
            _ => false,
        }
    }

    fn step_thread(&mut self, t: usize, slice: usize) -> usize {
        let mut n = 0;
        while n < slice && self.threads[t].state == ThreadState::Runnable {
            let Some(frame) = self.threads[t].frames.last() else {
                self.threads[t].state = ThreadState::Done;
                break;
            };
            if frame.pc >= frame.function.code.len() {
                // Falling off the end returns None.
                self.return_from(t, Value::None);
                n += 1;
                continue;
            }
            let function = frame.function.clone();
            let ins = &function.code[frame.pc];
            self.threads[t].frames.last_mut().unwrap().pc += 1;
            n += 1;
            self.exec(t, ins.op, &ins.args);
        }
        n
    }

    fn exec(&mut self, t: usize, op: Opcode, args: &[Operand]) {
        use Opcode::*;
        // Operands read up front, so the borrow of the frame ends before any
        // call pushes a new one.
        let vals: Vec<Value> = {
            let frame = self.threads[t].frames.last().unwrap();
            args.iter()
                .map(|a| Self::operand(&self.instances, frame, a))
                .collect()
        };
        let dest_name = |i: usize| -> Option<String> {
            args.get(i)
                .and_then(|a| a.as_ident())
                .map(|s| s.to_string())
        };
        let set = |vm: &mut Vm, i: usize, v: Value| {
            if let Some(name) = args.get(i).and_then(|a| a.as_ident()) {
                let frame = vm.threads[t].frames.last_mut().unwrap();
                Self::store(&mut vm.instances, frame, name, v);
            }
        };

        match op {
            Nop => {}
            IAdd => set(
                self,
                0,
                Value::Int(vals[1].as_int().wrapping_add(vals[2].as_int())),
            ),
            FAdd => set(
                self,
                0,
                Value::Float(vals[1].as_float() + vals[2].as_float()),
            ),
            ISub => set(
                self,
                0,
                Value::Int(vals[1].as_int().wrapping_sub(vals[2].as_int())),
            ),
            FSub => set(
                self,
                0,
                Value::Float(vals[1].as_float() - vals[2].as_float()),
            ),
            IMul => set(
                self,
                0,
                Value::Int(vals[1].as_int().wrapping_mul(vals[2].as_int())),
            ),
            FMul => set(
                self,
                0,
                Value::Float(vals[1].as_float() * vals[2].as_float()),
            ),
            IDiv => {
                let d = vals[2].as_int();
                if d == 0 {
                    self.err(t, "integer division by zero");
                    set(self, 0, Value::Int(0));
                } else {
                    set(self, 0, Value::Int(vals[1].as_int().wrapping_div(d)));
                }
            }
            FDiv => {
                let d = vals[2].as_float();
                if d == 0.0 {
                    self.err(t, "float division by zero");
                    set(self, 0, Value::Float(0.0));
                } else {
                    set(self, 0, Value::Float(vals[1].as_float() / d));
                }
            }
            IMod => {
                let d = vals[2].as_int();
                if d == 0 {
                    self.err(t, "modulo by zero");
                    set(self, 0, Value::Int(0));
                } else {
                    set(self, 0, Value::Int(vals[1].as_int().wrapping_rem(d)));
                }
            }
            Not => set(self, 0, Value::Bool(!vals[1].as_bool())),
            INeg => set(self, 0, Value::Int(vals[1].as_int().wrapping_neg())),
            FNeg => set(self, 0, Value::Float(-vals[1].as_float())),
            Assign => set(self, 0, vals[1].clone()),
            Cast => {
                let target = {
                    let frame = self.threads[t].frames.last().unwrap();
                    dest_name(0).and_then(|d| Self::type_of(&self.instances, frame, &d))
                };
                let v = self.cast(&vals[1], target.as_deref());
                set(self, 0, v);
            }
            CmpEq => set(self, 0, Value::Bool(vals[1].equals(&vals[2]))),
            CmpLt => set(self, 0, Value::Bool(vals[1].less_than(&vals[2]))),
            CmpLte => set(self, 0, Value::Bool(!vals[2].less_than(&vals[1]))),
            CmpGt => set(self, 0, Value::Bool(vals[2].less_than(&vals[1]))),
            CmpGte => set(self, 0, Value::Bool(!vals[1].less_than(&vals[2]))),
            Jmp => self.jump(t, vals[0].as_int()),
            JmpT => {
                if vals[0].as_bool() {
                    self.jump(t, vals[1].as_int());
                }
            }
            JmpF => {
                if !vals[0].as_bool() {
                    self.jump(t, vals[1].as_int());
                }
            }
            CallMethod => {
                let name = vals[0].to_string();
                let dest = dest_name(2);
                let call_args = vals[3..].to_vec();
                match vals[1].as_object() {
                    None => {
                        self.err(
                            t,
                            &format!("cannot call {name}() on a None object, aborting the call"),
                        );
                        set(self, 2, Value::None);
                    }
                    Some(obj) => self.call_method(t, obj, &name, call_args, dest, None),
                }
            }
            CallParent => {
                let name = vals[0].to_string();
                let dest = dest_name(1);
                let call_args = vals[2..].to_vec();
                let (obj, script) = {
                    let frame = self.threads[t].frames.last().unwrap();
                    (
                        frame.self_object,
                        frame.script.as_ref().map(|s| s.name.clone()),
                    )
                };
                match (obj, script) {
                    (Some(obj), Some(script)) => {
                        self.call_method(t, obj, &name, call_args, dest, Some(script));
                    }
                    _ => {
                        self.err(t, &format!("parent call to {name}() outside an object"));
                        set(self, 1, Value::None);
                    }
                }
            }
            CallStatic => {
                let class = vals[0].to_string();
                let name = vals[1].to_string();
                let dest = dest_name(2);
                let call_args = vals[3..].to_vec();
                self.call_static(t, &class, &name, call_args, dest);
            }
            Return => self.return_from(t, vals[0].clone()),
            StrCat => {
                let s = format!("{}{}", self.describe(&vals[1]), self.describe(&vals[2]));
                set(self, 0, Value::string(&s));
            }
            PropGet => {
                let prop = vals[0].to_string();
                let dest = dest_name(2);
                match vals[1].as_object() {
                    None => {
                        self.err(t, &format!("cannot read property {prop} of a None object"));
                        set(self, 2, Value::None);
                    }
                    Some(obj) => self.prop_get(t, obj, &prop, dest),
                }
            }
            PropSet => {
                let prop = vals[0].to_string();
                match vals[1].as_object() {
                    None => self.err(t, &format!("cannot set property {prop} of a None object")),
                    Some(obj) => self.prop_set(t, obj, &prop, vals[2].clone()),
                }
            }
            ArrayCreate => {
                let kind = {
                    let frame = self.threads[t].frames.last().unwrap();
                    dest_name(0)
                        .and_then(|d| Self::type_of(&self.instances, frame, &d))
                        .and_then(|ty| ElementKind::from_array_type(&ty))
                        .unwrap_or(ElementKind::Object)
                };
                let len = vals[1].as_int();
                if len < 0 {
                    self.err(t, "array size below zero");
                    set(self, 0, Value::None);
                } else {
                    set(self, 0, Value::Array(ArrayRef::new(kind, len as usize)));
                }
            }
            ArrayLength => {
                let len = vals[1].as_array().map(|a| a.len() as i32).unwrap_or(0);
                set(self, 0, Value::Int(len));
            }
            ArrayGetElement => {
                let idx = vals[2].as_int();
                match vals[1].as_array() {
                    None => {
                        self.err(t, "cannot index a None array");
                        set(self, 0, Value::None);
                    }
                    Some(a) => match usize::try_from(idx).ok().and_then(|i| a.get(i)) {
                        Some(v) => set(self, 0, v),
                        None => {
                            let len = a.len();
                            self.err(t, &format!("array index {idx} out of range, length {len}"));
                            set(self, 0, Value::None);
                        }
                    },
                }
            }
            ArraySetElement => {
                let idx = vals[1].as_int();
                match vals[0].as_array() {
                    None => self.err(t, "cannot index a None array"),
                    Some(a) => {
                        let mut items = a.items.borrow_mut();
                        match usize::try_from(idx).ok().filter(|i| *i < items.len()) {
                            Some(i) => items[i] = self.cast_to_kind(&vals[2], a.kind),
                            None => {
                                let len = items.len();
                                drop(items);
                                self.err(
                                    t,
                                    &format!("array index {idx} out of range, length {len}"),
                                );
                            }
                        }
                    }
                }
            }
            ArrayFindElement | ArrayRFindElement => {
                let reverse = op == ArrayRFindElement;
                let found = match vals[0].as_array() {
                    None => -1,
                    Some(a) => {
                        let items = a.items.borrow();
                        let start = vals[3].as_int();
                        if reverse {
                            let start = if start < 0 {
                                items.len() as i32 - 1
                            } else {
                                start.min(items.len() as i32 - 1)
                            };
                            (0..=start.max(-1))
                                .rev()
                                .find(|&i| items[i as usize].equals(&vals[2]))
                                .unwrap_or(-1)
                        } else {
                            let start = start.max(0);
                            (start..items.len() as i32)
                                .find(|&i| items[i as usize].equals(&vals[2]))
                                .unwrap_or(-1)
                        }
                    }
                };
                set(self, 1, Value::Int(found));
            }
            Is
            | StructCreate
            | StructGet
            | StructSet
            | ArrayFindStruct
            | ArrayRFindStruct
            | ArrayAdd
            | ArrayInsert
            | ArrayRemoveLast
            | ArrayRemove
            | ArrayClear
            | ArrayGetAllMatchingStructs => {
                self.err(
                    t,
                    &format!("opcode {op:?} is a Fallout 4 opcode, not supported"),
                );
                let thread = &mut self.threads[t];
                thread.frames.clear();
                thread.state = ThreadState::Done;
            }
        }
    }

    fn jump(&mut self, t: usize, offset: i32) {
        let frame = self.threads[t].frames.last_mut().unwrap();
        // The offset is relative to the jump instruction itself, and pc has
        // already moved past it.
        let base = frame.pc as i64 - 1;
        let target = base + offset as i64;
        if target < 0 || target as usize > frame.function.code.len() {
            self.err(t, &format!("jump to {target} outside the function"));
            let thread = &mut self.threads[t];
            thread.frames.clear();
            thread.state = ThreadState::Done;
            return;
        }
        frame.pc = target as usize;
    }

    fn cast(&mut self, v: &Value, target: Option<&str>) -> Value {
        let Some(target) = target else {
            return v.clone();
        };
        if target.ends_with("[]") {
            return v.clone();
        }
        match target.to_ascii_lowercase().as_str() {
            "int" => Value::Int(v.as_int()),
            "float" => Value::Float(v.as_float()),
            "bool" => Value::Bool(v.as_bool()),
            "string" => Value::string(&self.describe(v)),
            "var" => v.clone(),
            // An object type: objects pass through, anything else is None.
            _ => match v {
                Value::Object(_) | Value::None => v.clone(),
                _ => Value::None,
            },
        }
    }

    fn cast_to_kind(&self, v: &Value, kind: ElementKind) -> Value {
        match kind {
            ElementKind::Int => Value::Int(v.as_int()),
            ElementKind::Float => Value::Float(v.as_float()),
            ElementKind::Bool => Value::Bool(v.as_bool()),
            ElementKind::Str => match v {
                Value::Str(_) => v.clone(),
                other => Value::string(&self.describe(other)),
            },
            ElementKind::Object => v.clone(),
        }
    }

    fn call_method(
        &mut self,
        t: usize,
        obj: ObjectId,
        name: &str,
        args: Vec<Value>,
        dest: Option<String>,
        start_after: Option<String>,
    ) {
        if start_after.is_none() && self.builtin_method(t, obj, name, &args, dest.clone()) {
            return;
        }
        if let Some((iid, script, f)) = self.find_method(obj, name, start_after.as_deref()) {
            if f.is_native() {
                let class = script.name.clone();
                if self.call_native(t, false, &class, name, Some(obj), args, dest.clone()) {
                    return;
                }
                self.err(t, &format!("native {class}.{name}() has no implementation"));
                if let Some(d) = dest {
                    let frame = self.threads[t].frames.last_mut().unwrap();
                    Self::store(&mut self.instances, frame, &d, Value::None);
                }
                return;
            }
            self.push_call(t, Some(iid), Some(obj), Some(script), f, &args, dest);
            return;
        }
        // No script defines it: a native on a class the object derives from.
        let mut classes = self.class_names(obj);
        if let Some(after) = start_after {
            let after = after.to_ascii_lowercase();
            if let Some(pos) = classes.iter().position(|c| *c == after) {
                classes.drain(..=pos);
            }
        }
        for class in classes {
            if self.call_native(
                t,
                false,
                &class,
                name,
                Some(obj),
                args.clone(),
                dest.clone(),
            ) {
                return;
            }
        }
        let who = self.describe(&Value::Object(obj));
        self.err(t, &format!("{who} has no function {name}()"));
        if let Some(d) = dest {
            let frame = self.threads[t].frames.last_mut().unwrap();
            Self::store(&mut self.instances, frame, &d, Value::None);
        }
    }

    fn call_static(
        &mut self,
        t: usize,
        class: &str,
        name: &str,
        args: Vec<Value>,
        dest: Option<String>,
    ) {
        if self.builtin_global(t, class, name, &args, dest.clone()) {
            return;
        }
        if let Some(script) = self.script(class) {
            if let Some(f) = script
                .functions
                .get(&(String::new(), name.to_ascii_lowercase()))
                .cloned()
            {
                if !f.is_native() {
                    self.push_call(t, None, None, Some(script), f, &args, dest);
                    return;
                }
            }
        }
        if self.call_native(t, true, class, name, None, args, dest.clone()) {
            return;
        }
        self.err(t, &format!("no global function {class}.{name}()"));
        if let Some(d) = dest {
            let frame = self.threads[t].frames.last_mut().unwrap();
            Self::store(&mut self.instances, frame, &d, Value::None);
        }
    }

    /// The instance on `obj` whose chain declares the property, with the
    /// declaring script.
    fn find_property(&self, obj: ObjectId, prop: &str) -> Option<(InstanceId, Rc<LoadedScript>)> {
        let o = self.objects.get(&obj)?;
        for &iid in &o.instances {
            for s in &self.instances[iid.0].chain {
                if s.object.property(prop).is_some() {
                    return Some((iid, s.clone()));
                }
            }
        }
        None
    }

    fn prop_get(&mut self, t: usize, obj: ObjectId, prop: &str, dest: Option<String>) {
        let Some((iid, script)) = self.find_property(obj, prop) else {
            let who = self.describe(&Value::Object(obj));
            self.err(t, &format!("{who} has no property {prop}"));
            if let Some(d) = dest {
                let frame = self.threads[t].frames.last_mut().unwrap();
                Self::store(&mut self.instances, frame, &d, Value::None);
            }
            return;
        };
        let p = script.object.property(prop).unwrap();
        if let Some(var) = &p.auto_var {
            let v = self.instances[iid.0]
                .vars
                .get(&var.to_ascii_lowercase())
                .cloned()
                .unwrap_or(Value::None);
            if let Some(d) = dest {
                let frame = self.threads[t].frames.last_mut().unwrap();
                Self::store(&mut self.instances, frame, &d, v);
            }
            return;
        }
        match script.getters.get(&prop.to_ascii_lowercase()).cloned() {
            Some(getter) => {
                self.push_call(t, Some(iid), Some(obj), Some(script), getter, &[], dest);
            }
            None => {
                self.err(t, &format!("property {prop} is write-only"));
                if let Some(d) = dest {
                    let frame = self.threads[t].frames.last_mut().unwrap();
                    Self::store(&mut self.instances, frame, &d, Value::None);
                }
            }
        }
    }

    fn prop_set(&mut self, t: usize, obj: ObjectId, prop: &str, value: Value) {
        let Some((iid, script)) = self.find_property(obj, prop) else {
            let who = self.describe(&Value::Object(obj));
            self.err(t, &format!("{who} has no property {prop}"));
            return;
        };
        let p = script.object.property(prop).unwrap();
        if let Some(var) = &p.auto_var {
            self.instances[iid.0]
                .vars
                .insert(var.to_ascii_lowercase(), value);
            return;
        }
        match script.setters.get(&prop.to_ascii_lowercase()).cloned() {
            Some(setter) => self.push_call(
                t,
                Some(iid),
                Some(obj),
                Some(script),
                setter,
                &[value],
                None,
            ),
            None => self.err(t, &format!("property {prop} is read-only")),
        }
    }
}

fn default_for_type(type_name: &str) -> Value {
    if type_name.ends_with("[]") {
        return Value::None;
    }
    ElementKind::from_type(type_name).default_value()
}

/// A literal operand as a value. A declared type turns a `None` default into
/// the type's zero, which is how the compiler leaves primitives.
pub fn operand_to_value(op: &Operand, declared: Option<&str>) -> Value {
    match op {
        Operand::None => declared.map(default_for_type).unwrap_or(Value::None),
        Operand::Identifier(s) | Operand::Str(s) => Value::string(s),
        Operand::Int(i) => Value::Int(*i),
        Operand::Float(f) => Value::Float(*f),
        Operand::Bool(b) => Value::Bool(*b),
    }
}
