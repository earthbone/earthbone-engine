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

use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;
use std::time::Instant;

use crate::pex::{Function, Object, Opcode, Operand, Script};
use crate::value::{ArrayRef, ElementKind, ObjectId, Value};

/// The most elements `ArrayCreate` will allocate.
///
/// The size comes from the script, so without a ceiling a single instruction
/// allocates whatever a mod asks for. `fuzz-vm-run` found this with one
/// instruction that asked for 26.5 GB. Vanilla has the same hazard and handles
/// it with a warning to the author rather than a limit: the Creation Kit wiki
/// says of `Utility.CreateArray` that "it is possible to create arrays longer
/// than 128 elements, and size is treated as an unsigned integer. Negative
/// numbers will produce extremely large values, which can cause major issues."
///
/// A million is far above anything a script legitimately builds, since vanilla
/// arrays are conventionally capped at 128, and far below a size that
/// threatens the process. Scripts that want more should be using a form list.
pub const MAX_ARRAY_LEN: usize = 1 << 20;

/// The longest string `StrCat` will build.
///
/// Same hazard as [`MAX_ARRAY_LEN`] reached a different way. `StrCat dest,
/// dest, dest` doubles its input, so a loop passes a gigabyte in about thirty
/// iterations and the interpreter's step budget never binds. `fuzz-vm-run`
/// found it that way. A megabyte is far past any message, name or log line a
/// script legitimately builds.
pub const MAX_STRING_LEN: usize = 1 << 20;

mod collection;
mod dispatch;
mod execution;
mod properties;
mod scheduler;

pub mod coverage;
pub mod hash;
pub mod restore;
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
    /// A full property with a getter and no setter, so [`Vm::set_property`]
    /// cannot fill it. Kept apart from [`VmError::UnknownFunction`] so a
    /// caller can count a property the game never expected to set separately
    /// from one that is genuinely missing.
    ReadOnlyProperty {
        class: String,
        name: String,
    },
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
            VmError::ReadOnlyProperty { class, name } => {
                write!(f, "{class}.{name} is a read-only property")
            }
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
    /// Real seconds including time spent in menus.
    SleepingMenu(f64),
    /// Accumulated game hours, independent of changes to timescale.
    SleepingGame(f64),
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
    profiling: Rc<RefCell<Profiler>>,
    thread_id: ThreadId,
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
    /// Start timing Papyrus functions defined by a script name.
    pub fn start_script_profiling(&mut self, script: &str) {
        let target = script.to_ascii_lowercase();
        let mut profiler = self.profiling.borrow_mut();
        profiler.start(ProfileScope::Script(target));
    }
    /// Stop script profiling and write the collected Chrome trace profile.
    pub fn stop_script_profiling(&mut self, script: &str) -> std::io::Result<()> {
        self.profiling
            .borrow_mut()
            .stop(&ProfileScope::Script(script.to_ascii_lowercase()))
    }
    /// Start timing functions on the current Papyrus thread's call stack.
    pub fn start_stack_profiling(&mut self) {
        let mut profiler = self.profiling.borrow_mut();
        profiler.start(ProfileScope::Stack(self.thread_id));
    }
    /// Stop profiling the current Papyrus thread's call stack.
    pub fn stop_stack_profiling(&mut self) -> std::io::Result<()> {
        self.profiling
            .borrow_mut()
            .stop(&ProfileScope::Stack(self.thread_id))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProfileScope {
    Script(String),
    Stack(ThreadId),
}

#[derive(Default)]
struct Profiler {
    active: Option<(ProfileScope, std::time::Instant)>,
    functions: std::collections::BTreeMap<String, (u64, u64)>,
    output_dir: Option<std::path::PathBuf>,
}

impl Profiler {
    fn start(&mut self, scope: ProfileScope) {
        self.active = Some((scope, Instant::now()));
        self.functions.clear();
    }
    fn stop(&mut self, scope: &ProfileScope) -> std::io::Result<()> {
        let Some((active, started)) = self.active.take() else {
            return Ok(());
        };
        if &active != scope {
            self.active = Some((active, started));
            return Ok(());
        }
        let dir = self
            .output_dir
            .clone()
            .or_else(|| std::env::var_os("SOE_PROFILE_DIR").map(Into::into))
            .unwrap_or_else(|| std::path::PathBuf::from("logs/Script/Profiling"));
        std::fs::create_dir_all(&dir)?;
        let filename = match scope {
            ProfileScope::Script(name) => name.clone(),
            ProfileScope::Stack(id) => format!("stack-{}", id.0),
        };
        let safe_filename: String = filename
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(format!(
            "soe-{safe_filename}-{}.json",
            started.elapsed().as_nanos()
        ));
        let mut events = Vec::new();
        for (name, (calls, elapsed)) in &self.functions {
            events.push(format!("{{\"name\":\"{}\",\"cat\":\"papyrus.profile\",\"ph\":\"X\",\"ts\":0,\"dur\":{:.3},\"pid\":1,\"tid\":1,\"args\":{{\"calls\":{},\"duration_ns\":{}}}}}", json_escape(name), *elapsed as f64 / 1000.0, calls, elapsed));
        }
        std::fs::write(
            path,
            format!("{{\"traceEvents\":[{}]}}\n", events.join(",")),
        )
    }
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(escaped, "\\u{:04x}", character as u32);
            }
            character => escaped.push(character),
        }
    }
    escaped
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
    /// Calls that reached a registered native, keyed `class.function` in
    /// lowercase, so a host can see which natives real scripts hit.
    pub calls: HashMap<String, u64>,
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
    source_fingerprint: Option<[u8; 32]>,
    parent: Option<String>,
    object: Object,
    conditional_mask: u32,
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

fn variable_key(script: &str, name: &str) -> String {
    let script = script.to_ascii_lowercase();
    format!("{}:{script}{}", script.len(), name.to_ascii_lowercase())
}

impl Instance {
    fn variable_key(&self, script: &str, name: &str) -> String {
        let start = self
            .chain
            .iter()
            .position(|owner| owner.name.eq_ignore_ascii_case(script))
            .unwrap_or(0);
        let owner = self.chain[start..].iter().find(|owner| {
            owner
                .object
                .variables
                .iter()
                .any(|variable| variable.name.eq_ignore_ascii_case(name))
        });
        variable_key(owner.map_or(script, |owner| owner.name.as_str()), name)
    }
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
    /// Transient coverage state, omitted from saves and state hashes.
    coverage_recorded: bool,
    profile_recorded: bool,
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
/// Instructions a property getter or setter may run per scheduler pass when
/// the host fills or reads a full property.
const SETTER_BUDGET: usize = 4096;

#[derive(Default)]
pub struct Vm {
    coverage: Option<coverage::Tracker>,
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
    menu_time: f64,
    game_hours: f64,
    profiling: Rc<RefCell<Profiler>>,
}

impl Vm {
    pub fn new() -> Vm {
        Vm::default()
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    /// Set the directory used by Papyrus script and stack profiles.
    pub fn set_profile_directory(&mut self, path: impl Into<std::path::PathBuf>) {
        self.profiling.borrow_mut().output_dir = Some(path.into());
    }

    // ----- loading -----

    pub fn load(&mut self, script: Script) {
        self.load_with_fingerprint(script, None);
    }

    fn load_with_fingerprint(&mut self, script: Script, source_fingerprint: Option<[u8; 32]>) {
        // A new script set needs a fresh coverage denominator.
        self.coverage = None;
        let conditional_mask = script
            .user_flags
            .iter()
            .filter(|flag| flag.name.eq_ignore_ascii_case("conditional"))
            .filter_map(|flag| 1u32.checked_shl(u32::from(flag.bit)))
            .fold(0, |mask, bit| mask | bit);
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
                source_fingerprint,
                parent,
                object,
                conditional_mask,
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
        self.load_with_fingerprint(script, Some(Sha256::digest(data).into()));
        Ok(())
    }

    /// Loaded script names and SHA-256 PEX source fingerprints, sorted by
    /// case-insensitive name. `None` means the script was loaded as a `Script`
    /// value and its original source bytes are unknown.
    #[must_use]
    pub fn loaded_script_fingerprints(&self) -> Vec<(String, Option<[u8; 32]>)> {
        let mut scripts: Vec<_> = self
            .scripts
            .values()
            .map(|script| (script.name.clone(), script.source_fingerprint))
            .collect();
        scripts.sort_by_key(|entry| entry.0.to_ascii_lowercase());
        scripts
    }

    pub fn has_script(&self, name: &str) -> bool {
        self.scripts.contains_key(&name.to_ascii_lowercase())
    }

    /// Every function a loaded script declares as native, as `(class,
    /// function, global)` in the script's own spelling, sorted. A host
    /// compares this against its registry to see what the scripts expect.
    #[must_use]
    pub fn declared_natives(&self) -> Vec<(String, String, bool)> {
        let mut out: Vec<(String, String, bool)> = self
            .scripts
            .values()
            .flat_map(|s| {
                s.object.states.iter().flat_map(|st| {
                    st.functions
                        .iter()
                        .filter(|f| f.is_native())
                        .map(|f| (s.name.clone(), f.name.clone(), f.is_global()))
                })
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// The classes a method lookup walks: the declaring one, the parents its
    /// loaded script names, then the parents declared through
    /// [`NativeRegistry::declare_parent`]. Dispatch walks the same list, so a
    /// script redeclaring a parent's native still finds the registration.
    #[must_use]
    pub fn native_chain(&self, class: &str) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut next = Some(class.to_ascii_lowercase());
        while let Some(n) = next.take() {
            if out.contains(&n) || out.len() > 64 {
                break;
            }
            next = self
                .script(&n)
                .and_then(|s| s.parent.clone())
                .map(|p| p.to_ascii_lowercase())
                .or_else(|| self.natives.parents.get(&n).cloned());
            out.push(n);
        }
        out
    }

    /// Whether the scheduler answers this one itself, so no registration is
    /// wanted: the three `Utility` waits, and `GotoState` and `GetState` on
    /// every script object.
    fn is_builtin_native(class: &str, name: &str, global: bool) -> bool {
        let name = name.to_ascii_lowercase();
        if global {
            class.eq_ignore_ascii_case("utility")
                && matches!(name.as_str(), "wait" | "waitmenumode" | "waitgametime")
        } else {
            matches!(name.as_str(), "gotostate" | "getstate")
        }
    }

    /// The declared natives nothing covers. A method is looked up along its
    /// class chain the way dispatch does, a global only on the class named
    /// in the call, and the scheduler's own functions never count.
    #[must_use]
    pub fn unregistered_natives(&self) -> Vec<(String, String, bool)> {
        self.declared_natives()
            .into_iter()
            .filter(|(class, name, global)| {
                if Self::is_builtin_native(class, name, *global) {
                    return false;
                }
                if *global {
                    !self.natives.has_global(class, name)
                } else {
                    !self
                        .native_chain(class)
                        .iter()
                        .any(|c| self.natives.has_method(c, name))
                }
            })
            .collect()
    }

    /// Call sites per function name across every loaded script, counting
    /// `CallMethod`, `CallStatic` and `CallParent` instructions by the name
    /// operand. Keys are lowercase. A static count of what the scripts can
    /// reach, not of what a run reached.
    #[must_use]
    pub fn call_sites(&self) -> HashMap<String, u64> {
        let mut out: HashMap<String, u64> = HashMap::new();
        for ((_, name), n) in self.call_sites_by_script() {
            *out.entry(name).or_default() += n;
        }
        out
    }

    /// `call_sites` split by the calling script, keyed `(script, function)`
    /// in lowercase, so a host can ask which scripts reach a function.
    #[must_use]
    pub fn call_sites_by_script(&self) -> HashMap<(String, String), u64> {
        let mut out: HashMap<(String, String), u64> = HashMap::new();
        for s in self.scripts.values() {
            let script = s.name.to_ascii_lowercase();
            let functions = s.object.states.iter().flat_map(|st| st.functions.iter());
            let getters = s.object.properties.iter().filter_map(|p| p.getter.as_ref());
            let setters = s.object.properties.iter().filter_map(|p| p.setter.as_ref());
            for f in functions.chain(getters).chain(setters) {
                for ins in &f.code {
                    let name_index = match ins.op {
                        Opcode::CallMethod | Opcode::CallParent => 0,
                        Opcode::CallStatic => 1,
                        _ => continue,
                    };
                    if let Some(crate::pex::Operand::Identifier(name)) = ins.args.get(name_index) {
                        *out.entry((script.clone(), name.to_ascii_lowercase()))
                            .or_default() += 1;
                    }
                }
            }
        }
        out
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
                let key = variable_key(&s.name, &v.name);
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

    /// The most derived script of each instance attached to an object, in
    /// attachment order. What a console's `ShowVars` lists before it prints
    /// the variables of each.
    pub fn scripts_on(&self, object: ObjectId) -> Vec<String> {
        let Some(obj) = self.objects.get(&object) else {
            return Vec::new();
        };
        obj.instances
            .iter()
            .filter_map(|i| self.instances.get(i.0))
            .filter_map(|inst| inst.chain.first().map(|s| s.name.clone()))
            .collect()
    }

    pub fn vars_on(&self, object: ObjectId, script: &str) -> Vec<(String, Value)> {
        let Some(id) = self.instance_on(object, script) else {
            return Vec::new();
        };
        let instance = &self.instances[id.0];
        let start = instance
            .chain
            .iter()
            .position(|owner| owner.name.eq_ignore_ascii_case(script))
            .unwrap_or(0);
        let mut visible = std::collections::BTreeMap::new();
        for owner in &instance.chain[start..] {
            let prefix = format!("{}:{}", owner.name.len(), owner.name.to_ascii_lowercase());
            for (key, value) in &instance.vars {
                if let Some(name) = key.strip_prefix(&prefix) {
                    visible
                        .entry(name.to_owned())
                        .or_insert_with(|| value.clone());
                }
            }
        }
        visible.into_iter().collect()
    }

    /// The property names one attached script declares, sorted, walking the
    /// whole inheritance chain.
    pub fn properties_on(&self, object: ObjectId, script: &str) -> Vec<String> {
        let Some(id) = self.instance_on(object, script) else {
            return Vec::new();
        };
        let mut out: Vec<String> = self.instances[id.0]
            .chain
            .iter()
            .flat_map(|s| s.object.properties.iter().map(|p| p.name.clone()))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Loaded scripts that define a function or event of this name, in any
    /// state. Answers "who would hear this event" without sending it.
    pub fn scripts_handling(&self, name: &str) -> Vec<String> {
        let key = name.to_ascii_lowercase();
        let mut out: Vec<String> = self
            .scripts
            .values()
            .filter(|s| s.functions.keys().any(|(_, f)| *f == key))
            .map(|s| s.name.clone())
            .collect();
        out.sort();
        out
    }

    /// Read a script variable, `::Name_var` for auto properties.
    pub fn get_var(&self, object: ObjectId, script: &str, var: &str) -> Option<Value> {
        let id = self.instance_on(object, script)?;
        let key = self.instances[id.0].variable_key(script, var);
        self.instances[id.0].vars.get(&key).cloned()
    }

    /// Read matching conditional variables from attached script instances.
    /// CK conditions name a variable, not a script. Multiple matches remain
    /// visible to the caller instead of choosing by attachment order.
    /// Auto properties use their compiled `::Name_var` variable name.
    pub fn conditional_vars(&self, object: ObjectId, name: &str) -> Vec<Value> {
        let Some(object) = self.objects.get(&object) else {
            return Vec::new();
        };
        object
            .instances
            .iter()
            .filter_map(|id| {
                let instance = &self.instances[id.0];
                let (script, variable) = instance.chain.iter().find_map(|script| {
                    script
                        .object
                        .variables
                        .iter()
                        .find(|variable| variable.name.eq_ignore_ascii_case(name))
                        .map(|variable| (script, variable))
                })?;
                let mask = script.conditional_mask;
                if script.object.user_flags & mask == 0 || variable.user_flags & mask == 0 {
                    return None;
                }
                let key = variable_key(&script.name, &variable.name);
                instance.vars.get(&key).cloned()
            })
            .collect()
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
        let key = self.instances[id.0].variable_key(script, var);
        self.instances[id.0].vars.insert(key, value);
        Ok(())
    }

    pub fn state_of(&self, object: ObjectId, script: &str) -> Option<String> {
        let id = self.instance_on(object, script)?;
        Some(self.instances[id.0].state.clone())
    }

    // ----- threads -----

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
            coverage_recorded: false,
            profile_recorded: false,
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
            let key = instances[iid.0].variable_key(
                frame.script.as_ref().map_or("", |script| &script.name),
                name,
            );
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
            let instance_key = instances[iid.0].variable_key(
                frame.script.as_ref().map_or("", |script| &script.name),
                name,
            );
            if let Some(slot) = instances[iid.0].vars.get_mut(&instance_key) {
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
            let key = instances[iid.0].variable_key(
                frame.script.as_ref().map_or("", |script| &script.name),
                name,
            );
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
