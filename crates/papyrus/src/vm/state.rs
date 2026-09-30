// SPDX-License-Identifier: GPL-3.0-or-later

//! Save and load of live VM state: objects, script instances with their
//! variables, threads with their frame stacks, the clock and the log.
//!
//! The format is the engine's own, versioned by `VERSION`, little-endian,
//! with no dependency outside the standard library. Scripts are not stored.
//! Saved state with missing or changed definitions stays suspended until
//! compatible PEX files load. Every frame names its function by script, state
//! and name so it can be found again.
//! Arrays keep their identity: every distinct array reachable from a
//! variable, local, result or pending argument goes into a pool once and
//! values refer to it by index, so two variables that shared an array before
//! the save share it after the load.
//!
//! [`Writer`], [`Reader`] and [`ArrayPool`] are public so the game crate can
//! write its own state in the same encoding, including `Value`s.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use crate::pex::Function;
use crate::value::{ArrayRef, ElementKind, ObjectId, StructRef, Value};

use super::coverage::{key_of, FunctionKind as FnKey};

use super::{
    Frame, GameObject, InstanceId, LatentId, LoadedScript, LogEntry, Thread, ThreadId, ThreadState,
    Vm, VmError,
};

pub const MAGIC: &[u8; 4] = b"SOEV";
pub const VERSION: u16 = 4;

/// Little-endian byte writer.
#[derive(Default)]
pub struct Writer {
    pub out: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    pub fn bool(&mut self, v: bool) {
        self.out.push(u8::from(v));
    }
    pub fn u16(&mut self, v: u16) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn f64(&mut self, v: f64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }
    pub fn len(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }
    pub fn str(&mut self, s: &str) {
        self.len(s.len());
        self.out.extend_from_slice(s.as_bytes());
    }
    pub fn opt_str(&mut self, s: Option<&str>) {
        match s {
            Some(s) => {
                self.bool(true);
                self.str(s);
            }
            None => self.bool(false),
        }
    }
    pub fn opt_u64(&mut self, v: Option<u64>) {
        match v {
            Some(v) => {
                self.bool(true);
                self.u64(v);
            }
            None => self.bool(false),
        }
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.len(b.len());
        self.out.extend_from_slice(b);
    }
}

/// Little-endian byte reader that refuses to run past the end.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
    pub fn take(&mut self, n: usize) -> Result<&'a [u8], VmError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|e| *e <= self.data.len())
            .ok_or_else(|| bad(&format!("needs {n} bytes at {}", self.pos)))?;
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    pub fn u8(&mut self) -> Result<u8, VmError> {
        Ok(self.take(1)?[0])
    }
    pub fn bool(&mut self) -> Result<bool, VmError> {
        Ok(self.u8()? != 0)
    }
    pub fn u16(&mut self) -> Result<u16, VmError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    pub fn u32(&mut self) -> Result<u32, VmError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn i32(&mut self) -> Result<i32, VmError> {
        Ok(self.u32()? as i32)
    }
    pub fn u64(&mut self) -> Result<u64, VmError> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }
    pub fn f32(&mut self) -> Result<f32, VmError> {
        Ok(f32::from_bits(self.u32()?))
    }
    pub fn f64(&mut self) -> Result<f64, VmError> {
        Ok(f64::from_bits(self.u64()?))
    }
    /// A count that must fit in the bytes that remain. Not a collection
    /// length, `Reader` has nothing to be empty, so `is_empty` would not
    /// mean anything here.
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&mut self) -> Result<usize, VmError> {
        let n = self.u32()? as usize;
        if n > self.remaining() {
            return Err(bad(&format!("count {n} exceeds the data")));
        }
        Ok(n)
    }
    pub fn str(&mut self) -> Result<String, VmError> {
        let n = self.len()?;
        let b = self.take(n)?;
        String::from_utf8(b.to_vec()).map_err(|_| bad("string is not UTF-8"))
    }
    pub fn opt_str(&mut self) -> Result<Option<String>, VmError> {
        Ok(if self.bool()? {
            Some(self.str()?)
        } else {
            None
        })
    }
    pub fn opt_u64(&mut self) -> Result<Option<u64>, VmError> {
        Ok(if self.bool()? {
            Some(self.u64()?)
        } else {
            None
        })
    }
    pub fn bytes(&mut self) -> Result<&'a [u8], VmError> {
        let n = self.len()?;
        self.take(n)
    }
    pub fn expect_end(&self) -> Result<(), VmError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(bad(&format!("{} trailing bytes", self.remaining())))
        }
    }
}

fn bad(msg: &str) -> VmError {
    VmError::State(msg.to_string())
}

fn kind_byte(k: ElementKind) -> u8 {
    match k {
        ElementKind::Object => 0,
        ElementKind::Str => 1,
        ElementKind::Int => 2,
        ElementKind::Float => 3,
        ElementKind::Bool => 4,
    }
}

fn kind_from(b: u8) -> Result<ElementKind, VmError> {
    Ok(match b {
        0 => ElementKind::Object,
        1 => ElementKind::Str,
        2 => ElementKind::Int,
        3 => ElementKind::Float,
        4 => ElementKind::Bool,
        other => return Err(bad(&format!("unknown array kind {other}"))),
    })
}

/// Every distinct array reachable from the values written through it, in
/// first-seen order, so identity survives the round trip.
#[derive(Default)]
pub struct ArrayPool {
    arrays: Vec<ArrayRef>,
    index: HashMap<*const RefCell<Vec<Value>>, u32>,
}

impl ArrayPool {
    pub fn new() -> Self {
        Self::default()
    }

    fn intern(&mut self, a: &ArrayRef) -> u32 {
        let key = Rc::as_ptr(&a.items);
        if let Some(i) = self.index.get(&key) {
            return *i;
        }
        let i = u32::try_from(self.arrays.len()).unwrap_or(u32::MAX);
        self.arrays.push(a.clone());
        self.index.insert(key, i);
        // Elements may be arrays too: intern them so the pool is closed.
        let items: Vec<Value> = a.items.borrow().clone();
        for v in &items {
            if let Value::Array(inner) = v {
                self.intern(inner);
            } else if let Value::Struct(s) = v {
                let fields: Vec<Value> = s.fields.borrow().values().cloned().collect();
                for field in &fields {
                    if let Value::Array(inner) = field {
                        self.intern(inner);
                    }
                }
            }
        }
        i
    }

    /// Write one value. Arrays go into the pool and are written by index.
    pub fn write_value(&mut self, w: &mut Writer, v: &Value) {
        match v {
            Value::None => w.u8(0),
            Value::Object(o) => {
                w.u8(1);
                w.u64(o.0);
            }
            Value::Str(s) => {
                w.u8(2);
                w.str(s);
            }
            Value::Int(i) => {
                w.u8(3);
                w.i32(*i);
            }
            Value::Float(f) => {
                w.u8(4);
                w.f32(*f);
            }
            Value::Bool(b) => {
                w.u8(5);
                w.bool(*b);
            }
            Value::Array(a) => {
                let i = self.intern(a);
                w.u8(6);
                w.u32(i);
            }
            Value::Struct(s) => {
                w.u8(7);
                w.str(&s.name);
                let fields = s.fields.borrow();
                w.len(fields.len());
                for (name, value) in fields.iter() {
                    w.str(name);
                    self.write_value(w, value);
                }
            }
        }
    }

    /// Write the pool itself. Call after every value has been written, the
    /// reader needs the pool before the values, so write it into its own
    /// writer and put it first.
    pub fn write_pool(&mut self, w: &mut Writer) {
        // Interning while writing elements can grow the pool, so loop by
        // index rather than iterating a borrowed vector.
        let mut i = 0;
        let mut bodies = Vec::new();
        while i < self.arrays.len() {
            let a = self.arrays[i].clone();
            let mut body = Writer::new();
            body.u8(kind_byte(a.kind));
            let items: Vec<Value> = a.items.borrow().clone();
            body.len(items.len());
            for v in &items {
                self.write_value(&mut body, v);
            }
            bodies.push(body.out);
            i += 1;
        }
        w.len(bodies.len());
        for b in bodies {
            w.out.extend_from_slice(&b);
        }
    }
}

/// The reading side of [`ArrayPool`].
pub struct ArrayTable {
    arrays: Vec<ArrayRef>,
}

impl ArrayTable {
    /// Read a pool written by [`ArrayPool::write_pool`].
    pub fn read_pool(r: &mut Reader<'_>) -> Result<ArrayTable, VmError> {
        let n = r.len()?;
        let mut arrays = Vec::with_capacity(n);
        let mut kinds = Vec::with_capacity(n);
        // Two passes: allocate every array first so element references to
        // later arrays resolve, then fill them.
        let start = r.pos;
        for _ in 0..n {
            let kind = kind_from(r.u8()?)?;
            kinds.push(kind);
            let len = r.len()?;
            for _ in 0..len {
                skip_value(r)?;
            }
            arrays.push(ArrayRef::new(kind, 0));
        }
        let table = ArrayTable { arrays };
        r.pos = start;
        for i in 0..n {
            r.u8()?;
            let len = r.len()?;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(table.read_value(r)?);
            }
            *table.arrays[i].items.borrow_mut() = items;
        }
        Ok(table)
    }

    pub fn empty() -> ArrayTable {
        ArrayTable { arrays: Vec::new() }
    }

    pub fn read_value(&self, r: &mut Reader<'_>) -> Result<Value, VmError> {
        Ok(match r.u8()? {
            0 => Value::None,
            1 => Value::Object(ObjectId(r.u64()?)),
            2 => Value::string(&r.str()?),
            3 => Value::Int(r.i32()?),
            4 => Value::Float(r.f32()?),
            5 => Value::Bool(r.bool()?),
            6 => {
                let i = r.u32()? as usize;
                let a = self
                    .arrays
                    .get(i)
                    .ok_or_else(|| bad(&format!("array {i} is not in the pool")))?;
                Value::Array(a.clone())
            }
            7 => {
                let name = r.str()?;
                let count = r.len()?;
                let mut fields = std::collections::BTreeMap::new();
                for _ in 0..count {
                    let field = r.str()?;
                    if fields.insert(field, self.read_value(r)?).is_some() {
                        return Err(bad("duplicate struct member"));
                    }
                }
                Value::Struct(StructRef::new(name, fields))
            }
            other => return Err(bad(&format!("unknown value tag {other}"))),
        })
    }
}

fn skip_value(r: &mut Reader<'_>) -> Result<(), VmError> {
    match r.u8()? {
        0 => {}
        1 => {
            r.u64()?;
        }
        2 => {
            r.str()?;
        }
        3 | 4 | 6 => {
            r.u32()?;
        }
        7 => {
            r.str()?;
            let count = r.len()?;
            for _ in 0..count {
                r.str()?;
                skip_value(r)?;
            }
        }
        5 => {
            r.u8()?;
        }
        other => return Err(bad(&format!("unknown value tag {other}"))),
    }
    Ok(())
}

fn write_vars(
    pool: &mut ArrayPool,
    w: &mut Writer,
    vars: &HashMap<String, Value>,
    types: &HashMap<String, String>,
) {
    w.len(vars.len());
    let mut entries: Vec<_> = vars.iter().collect();
    entries.sort_by_key(|(name, _)| *name);
    for (name, value) in entries {
        w.str(name);
        w.opt_str(types.get(name).map(String::as_str));
        pool.write_value(w, value);
    }
}

/// A script instance's or a frame's variables, and the declared type name
/// of each variable that has one (every script variable does, a local only
/// when a cast needs it).
type Vars = (HashMap<String, Value>, HashMap<String, String>);

fn read_vars(table: &ArrayTable, r: &mut Reader<'_>) -> Result<Vars, VmError> {
    let n = r.len()?;
    let mut vars = HashMap::with_capacity(n);
    let mut types = HashMap::with_capacity(n);
    for _ in 0..n {
        let name = r.str()?;
        if let Some(t) = r.opt_str()? {
            types.insert(name.clone(), t);
        }
        vars.insert(name, table.read_value(r)?);
    }
    Ok((vars, types))
}

fn write_state(w: &mut Writer, s: &ThreadState) {
    match s {
        ThreadState::Runnable => w.u8(0),
        ThreadState::Sleeping(until) => {
            w.u8(1);
            w.f64(*until);
        }
        ThreadState::Latent(id) => {
            w.u8(2);
            w.u64(id.0);
        }
        ThreadState::Done => w.u8(3),
        ThreadState::SleepingMenu(until) => {
            w.u8(4);
            w.f64(*until);
        }
        ThreadState::SleepingGame(until) => {
            w.u8(5);
            w.f64(*until);
        }
    }
}

fn read_state(r: &mut Reader<'_>) -> Result<ThreadState, VmError> {
    Ok(match r.u8()? {
        0 => ThreadState::Runnable,
        1 => ThreadState::Sleeping(r.f64()?),
        2 => ThreadState::Latent(LatentId(r.u64()?)),
        3 => ThreadState::Done,
        4 => ThreadState::SleepingMenu(r.f64()?),
        5 => ThreadState::SleepingGame(r.f64()?),
        other => return Err(bad(&format!("unknown thread state {other}"))),
    })
}

fn find_fn(script: &LoadedScript, key: &FnKey) -> Option<Rc<Function>> {
    match key {
        FnKey::Method { state, name } => script
            .functions
            .get(&(state.clone(), name.clone()))
            .cloned(),
        FnKey::Getter(p) => script.getters.get(p).cloned(),
        FnKey::Setter(p) => script.setters.get(p).cloned(),
    }
}

fn saved_script_fingerprints(vm: &Vm) -> Vec<(String, Option<[u8; 32]>)> {
    let mut scripts = BTreeMap::new();
    for script in vm
        .instances
        .iter()
        .flat_map(|instance| instance.chain.iter())
        .chain(
            vm.threads
                .iter()
                .flat_map(|thread| thread.frames.iter())
                .filter_map(|frame| frame.script.as_ref()),
        )
    {
        scripts
            .entry(script.name.to_ascii_lowercase())
            .or_insert_with(|| (script.name.clone(), script.source_fingerprint));
    }
    scripts.into_values().collect()
}

fn read_script_fingerprints(
    reader: &mut Reader<'_>,
) -> Result<HashMap<String, Option<[u8; 32]>>, VmError> {
    let count = reader.len()?;
    let mut scripts = HashMap::with_capacity(count);
    for _ in 0..count {
        let name = reader.str()?;
        let fingerprint = if reader.bool()? {
            let bytes = reader.take(32)?;
            let mut fingerprint = [0; 32];
            fingerprint.copy_from_slice(bytes);
            Some(fingerprint)
        } else {
            None
        };
        if scripts
            .insert(name.to_ascii_lowercase(), fingerprint)
            .is_some()
        {
            return Err(bad("duplicate script fingerprint entry"));
        }
    }
    Ok(scripts)
}

fn check_script_fingerprint(
    saved: &HashMap<String, Option<[u8; 32]>>,
    loaded: &LoadedScript,
) -> Result<(), VmError> {
    if let Some(Some(fingerprint)) = saved.get(&loaded.name.to_ascii_lowercase()) {
        if loaded.source_fingerprint.as_ref() != Some(fingerprint) {
            return Err(VmError::IncompatibleScript(loaded.name.clone()));
        }
    }
    Ok(())
}

impl Vm {
    /// Serialize everything but the loaded scripts and the native registry.
    ///
    /// # Errors
    ///
    /// A frame whose function cannot be named (no script attached to it)
    /// cannot be restored and is refused.
    pub fn save_state(&self) -> Result<Vec<u8>, VmError> {
        if let Some(bytes) = &self.suspended_state {
            return Ok(bytes.clone());
        }
        let mut pool = ArrayPool::new();
        let mut body = Writer::new();

        body.u64(self.next_object);
        body.len(self.objects.len());
        let mut objects: Vec<_> = self.objects.iter().collect();
        objects.sort_by_key(|(id, _)| id.0);
        for (id, o) in objects {
            body.u64(id.0);
            body.str(&o.name);
            body.opt_str(o.class.as_deref());
            body.len(o.instances.len());
            for i in &o.instances {
                body.u64(i.0 as u64);
            }
        }

        body.len(self.instances.len());
        for inst in &self.instances {
            body.u64(inst.object.0);
            body.len(inst.chain.len());
            for s in &inst.chain {
                body.str(&s.name);
            }
            body.str(&inst.state);
            write_vars(&mut pool, &mut body, &inst.vars, &inst.var_types);
        }

        body.u64(self.next_thread);
        body.len(self.threads.len());
        for t in &self.threads {
            body.u64(t.id.0);
            write_state(&mut body, &t.state);
            pool.write_value(&mut body, &t.result);
            body.opt_str(t.pending_dest.as_deref());
            body.len(t.frames.len());
            for f in &t.frames {
                let script = f
                    .script
                    .as_ref()
                    .ok_or_else(|| bad("a frame without a script cannot be saved"))?;
                let key = key_of(script, &f.function).ok_or_else(|| {
                    bad(&format!(
                        "frame function {} not found in {}",
                        f.function.name, script.name
                    ))
                })?;
                body.str(&script.name);
                match key {
                    FnKey::Method { state, name } => {
                        body.u8(0);
                        body.str(&state);
                        body.str(&name);
                    }
                    FnKey::Getter(p) => {
                        body.u8(1);
                        body.str(&p);
                    }
                    FnKey::Setter(p) => {
                        body.u8(2);
                        body.str(&p);
                    }
                }
                body.opt_u64(f.instance.map(|i| i.0 as u64));
                body.opt_u64(f.self_object.map(|o| o.0));
                body.u64(f.pc as u64);
                write_vars(&mut pool, &mut body, &f.locals, &f.local_types);
                body.opt_str(f.ret_dest.as_deref());
            }
        }

        body.u64(self.latent_counter);
        body.len(self.latent_waiters.len());
        let mut waiters: Vec<_> = self.latent_waiters.iter().collect();
        waiters.sort_by_key(|(id, _)| id.0);
        for (l, t) in waiters {
            body.u64(l.0);
            body.u64(t.0);
        }
        body.f64(self.time);
        body.f64(self.menu_time);
        body.f64(self.game_hours);
        body.len(self.log.entries.len());
        for e in &self.log.entries {
            body.f64(e.time);
            body.str(&e.message);
        }

        let mut out = Writer::new();
        out.out.extend_from_slice(MAGIC);
        out.u16(VERSION);
        pool.write_pool(&mut out);
        let fingerprints = saved_script_fingerprints(self);
        out.len(fingerprints.len());
        for (name, fingerprint) in fingerprints {
            out.str(&name);
            match fingerprint {
                Some(fingerprint) => {
                    out.bool(true);
                    out.out.extend_from_slice(&fingerprint);
                }
                None => out.bool(false),
            }
        }
        out.out.extend_from_slice(&body.out);
        Ok(out.out)
    }

    /// Replace the live state with a saved one. The scripts the save names
    /// must be loaded already, the native registry is kept.
    ///
    /// # Errors
    ///
    /// A wrong magic or version, a truncated or inconsistent stream.
    pub fn load_state(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        match self.load_state_inner(bytes) {
            Ok(()) => {
                self.suspended_state = None;
                Ok(())
            }
            Err(
                VmError::UnknownScript(_)
                | VmError::UnknownFunction { .. }
                | VmError::IncompatibleScript(_),
            ) => {
                self.clear_runtime_state();
                self.suspended_state = Some(bytes.to_vec());
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn clear_runtime_state(&mut self) {
        self.objects.clear();
        self.next_object = 0;
        self.instances.clear();
        self.threads.clear();
        self.next_thread = 0;
        self.latent_counter = 0;
        self.latent_waiters.clear();
        self.time = 0.0;
        self.menu_time = 0.0;
        self.game_hours = 0.0;
        self.log.entries.clear();
    }

    pub(super) fn load_state_inner(&mut self, bytes: &[u8]) -> Result<(), VmError> {
        let mut r = Reader::new(bytes);
        if r.take(4)? != MAGIC {
            return Err(bad("not a VM state file"));
        }
        let version = r.u16()?;
        if !(1..=VERSION).contains(&version) {
            return Err(bad(&format!(
                "state version {version}, this VM reads {VERSION}"
            )));
        }
        let table = ArrayTable::read_pool(&mut r)?;
        let fingerprints = if version >= 4 {
            read_script_fingerprints(&mut r)?
        } else {
            HashMap::new()
        };

        let next_object = r.u64()?;
        let n = r.len()?;
        let mut objects = HashMap::with_capacity(n);
        for _ in 0..n {
            let id = ObjectId(r.u64()?);
            let name = r.str()?;
            let class = r.opt_str()?;
            let k = r.len()?;
            let mut instances = Vec::with_capacity(k);
            for _ in 0..k {
                instances.push(InstanceId(r.u64()? as usize));
            }
            objects.insert(
                id,
                GameObject {
                    name,
                    class,
                    instances,
                },
            );
        }

        let n = r.len()?;
        let mut instances = Vec::with_capacity(n);
        for _ in 0..n {
            let object = ObjectId(r.u64()?);
            let k = r.len()?;
            let mut chain = Vec::with_capacity(k);
            for _ in 0..k {
                let name = r.str()?;
                let script = self.script(&name).ok_or(VmError::UnknownScript(name))?;
                check_script_fingerprint(&fingerprints, &script)?;
                chain.push(script);
            }
            let state = r.str()?;
            let (mut vars, mut var_types) = read_vars(&table, &mut r)?;
            if version < 3 {
                let old_vars = std::mem::take(&mut vars);
                let old_types = std::mem::take(&mut var_types);
                for script in &chain {
                    for variable in &script.object.variables {
                        let old_key = variable.name.to_ascii_lowercase();
                        let key = super::variable_key(&script.name, &variable.name);
                        if let Some(value) = old_vars.get(&old_key) {
                            vars.insert(key.clone(), value.clone());
                            var_types.insert(
                                key,
                                old_types
                                    .get(&old_key)
                                    .cloned()
                                    .unwrap_or_else(|| variable.type_name.clone()),
                            );
                        }
                    }
                }
                if let Some(script) = chain.first() {
                    for (name, value) in old_vars {
                        if chain.iter().any(|owner| {
                            owner
                                .object
                                .variables
                                .iter()
                                .any(|variable| variable.name.eq_ignore_ascii_case(&name))
                        }) {
                            continue;
                        }
                        let key = super::variable_key(&script.name, &name);
                        vars.entry(key.clone()).or_insert(value);
                        if let Some(type_name) = old_types.get(&name) {
                            var_types.entry(key).or_insert_with(|| type_name.clone());
                        }
                    }
                }
            }
            instances.push(super::Instance {
                object,
                chain,
                state,
                vars,
                var_types,
            });
        }
        for o in objects.values() {
            if o.instances.iter().any(|i| i.0 >= instances.len()) {
                return Err(bad("object refers to an instance past the table"));
            }
        }

        let next_thread = r.u64()?;
        let n = r.len()?;
        let mut threads = Vec::with_capacity(n);
        for _ in 0..n {
            let id = ThreadId(r.u64()?);
            let state = read_state(&mut r)?;
            let result = table.read_value(&mut r)?;
            let pending_dest = r.opt_str()?;
            let k = r.len()?;
            let mut frames = Vec::with_capacity(k);
            for _ in 0..k {
                let script_name = r.str()?;
                let script = self
                    .script(&script_name)
                    .ok_or_else(|| VmError::UnknownScript(script_name.clone()))?;
                check_script_fingerprint(&fingerprints, &script)?;
                let key = match r.u8()? {
                    0 => FnKey::Method {
                        state: r.str()?,
                        name: r.str()?,
                    },
                    1 => FnKey::Getter(r.str()?),
                    2 => FnKey::Setter(r.str()?),
                    other => return Err(bad(&format!("unknown function kind {other}"))),
                };
                let function = find_fn(&script, &key).ok_or_else(|| VmError::UnknownFunction {
                    class: script_name.clone(),
                    name: match &key {
                        FnKey::Method { name, .. } => name.clone(),
                        FnKey::Getter(p) | FnKey::Setter(p) => p.clone(),
                    },
                })?;
                let instance = r
                    .opt_u64()?
                    .map(|i| {
                        usize::try_from(i)
                            .ok()
                            .filter(|index| *index < instances.len())
                            .map(InstanceId)
                            .ok_or_else(|| bad("frame refers to an instance past the table"))
                    })
                    .transpose()?;
                let self_object = r.opt_u64()?.map(ObjectId);
                let pc = r.u64()? as usize;
                let (locals, local_types) = read_vars(&table, &mut r)?;
                let ret_dest = r.opt_str()?;
                frames.push(Frame {
                    coverage_recorded: false,
                    instance,
                    self_object,
                    script: Some(script),
                    function,
                    pc,
                    locals,
                    local_types,
                    ret_dest,
                });
            }
            threads.push(Thread {
                id,
                frames,
                state,
                result,
                pending_dest,
            });
        }

        let latent_counter = r.u64()?;
        let n = r.len()?;
        let mut latent_waiters = HashMap::with_capacity(n);
        for _ in 0..n {
            latent_waiters.insert(LatentId(r.u64()?), ThreadId(r.u64()?));
        }
        let time = r.f64()?;
        let (menu_time, game_hours) = if version >= 2 {
            (r.f64()?, r.f64()?)
        } else {
            (time, time * 20.0 / 3600.0)
        };
        let n = r.len()?;
        let mut entries = Vec::with_capacity(n);
        for _ in 0..n {
            entries.push(LogEntry {
                time: r.f64()?,
                message: r.str()?,
            });
        }
        r.expect_end()?;

        self.objects = objects;
        self.next_object = next_object;
        self.instances = instances;
        self.threads = threads;
        self.next_thread = next_thread;
        self.latent_counter = latent_counter;
        self.latent_waiters = latent_waiters;
        self.time = time;
        self.menu_time = menu_time;
        self.game_hours = game_hours;
        self.log.entries = entries;
        if self.coverage.is_some() {
            self.start_coverage();
        }
        Ok(())
    }
}

impl Vm {
    /// Validate a saved VM against these loaded scripts without changing live state.
    ///
    /// # Errors
    /// The same malformed-state and script-definition errors as `load_state`.
    pub fn validate_state(&self, bytes: &[u8]) -> Result<(), VmError> {
        let mut scratch = Self {
            scripts: self.scripts.clone(),
            ..Self::new()
        };
        scratch.load_state_inner(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::variable_key;

    fn as_legacy_state(mut bytes: Vec<u8>, version: u16) -> Vec<u8> {
        let (fingerprints_start, fingerprints_end) = {
            let mut reader = Reader::new(&bytes);
            reader.take(6).unwrap();
            ArrayTable::read_pool(&mut reader).unwrap();
            let fingerprints_start = reader.pos;
            read_script_fingerprints(&mut reader).unwrap();
            (fingerprints_start, reader.pos)
        };
        bytes.drain(fingerprints_start..fingerprints_end);
        bytes[4..6].copy_from_slice(&version.to_le_bytes());
        bytes
    }

    #[test]
    fn sav_59_unavailable_state_survives_two_save_cycles_and_resumes() {
        use crate::asm::{ident, none, s, FunctionBuilder, ScriptBuilder};

        let script = ScriptBuilder::new("LostScript", "")
            .variable("left", "Int[]", none())
            .variable("right", "Int[]", none())
            .function(
                FunctionBuilder::new("Run", "None")
                    .local("ignored", "None")
                    .ins(
                        crate::pex::Opcode::CallStatic,
                        vec![s("LostScript"), s("Inner"), ident("ignored")],
                    )
                    .ins(crate::pex::Opcode::Return, vec![none()])
                    .build(),
            )
            .function(
                FunctionBuilder::new("Inner", "None")
                    .ins(crate::pex::Opcode::Return, vec![none()])
                    .build(),
            )
            .bytes();
        let incompatible = ScriptBuilder::new("LostScript", "")
            .variable("left", "Int[]", none())
            .variable("right", "Int[]", none())
            .variable("changedSchema", "Int", crate::asm::i(1))
            .function(
                FunctionBuilder::new("Run", "None")
                    .local("ignored", "None")
                    .ins(
                        crate::pex::Opcode::CallStatic,
                        vec![s("LostScript"), s("Inner"), ident("ignored")],
                    )
                    .ins(crate::pex::Opcode::Return, vec![none()])
                    .build(),
            )
            .function(
                FunctionBuilder::new("Inner", "None")
                    .ins(crate::pex::Opcode::Return, vec![none()])
                    .build(),
            )
            .bytes();

        let mut source = Vm::new();
        source.load_bytes(&script).unwrap();
        let object = source.create_object("saved owner");
        let instance = source.attach(object, "LostScript").unwrap();
        let shared = ArrayRef::new(ElementKind::Int, 1);
        source.instances[instance.0].vars.insert(
            variable_key("LostScript", "left"),
            Value::Array(shared.clone()),
        );
        source.instances[instance.0].vars.insert(
            variable_key("LostScript", "right"),
            Value::Array(shared.clone()),
        );
        let thread = source.call_global("LostScript", "Run", &[]).unwrap();
        assert_eq!(source.run(1), 1);
        let active = source.threads.iter_mut().find(|t| t.id == thread).unwrap();
        active.state = ThreadState::Latent(LatentId(41));
        active.pending_dest = Some("pendingResult".into());
        active.result = Value::Array(shared.clone());
        active.frames[1]
            .locals
            .insert("pendingArgument".into(), Value::Array(shared));
        source.latent_waiters.insert(LatentId(41), thread);
        let original = source.save_state().unwrap();

        let mut recovering = Vm::new();
        recovering.load_state(&original).unwrap();
        assert!(recovering.has_suspended_state());
        assert_eq!(recovering.run(100), 0);
        let first_cycle = recovering.save_state().unwrap();
        assert_eq!(first_cycle, original);

        let mut recovering = Vm::new();
        recovering.load_state(&first_cycle).unwrap();
        assert!(recovering.has_suspended_state());
        let second_cycle = recovering.save_state().unwrap();
        assert_eq!(second_cycle, original);

        recovering.load_bytes(&incompatible).unwrap();
        assert!(recovering.has_suspended_state());
        assert_eq!(recovering.run(100), 0);
        recovering.load_bytes(&script).unwrap();
        assert!(!recovering.has_suspended_state());
        assert_eq!(recovering.live_threads(), 1);
        assert_eq!(recovering.threads[0].frames.len(), 2);
        assert_eq!(
            recovering.thread_state(thread),
            Some(ThreadState::Latent(LatentId(41)))
        );
        assert_eq!(recovering.latent_waiters.get(&LatentId(41)), Some(&thread));
        assert_eq!(
            recovering.threads[0].pending_dest.as_deref(),
            Some("pendingResult")
        );

        let vars = &recovering.instances[instance.0].vars;
        let Value::Array(left) = &vars[&variable_key("LostScript", "left")] else {
            panic!("left variable should remain an array")
        };
        let Value::Array(right) = &vars[&variable_key("LostScript", "right")] else {
            panic!("right variable should remain an array")
        };
        let Value::Array(result) = &recovering.threads[0].result else {
            panic!("thread result should remain an array")
        };
        let Value::Array(local) = &recovering.threads[0].frames[1].locals["pendingArgument"] else {
            panic!("frame local should remain an array")
        };
        assert!(Rc::ptr_eq(&left.items, &right.items));
        assert!(Rc::ptr_eq(&left.items, &result.items));
        assert!(Rc::ptr_eq(&left.items, &local.items));
    }

    #[test]
    fn serialized_waiters_do_not_depend_on_hashmap_insertion_order() {
        let mut first = Vm::new();
        let mut second = Vm::new();
        for id in 1..=16 {
            first.latent_waiters.insert(LatentId(id), ThreadId(id));
        }
        for id in (1..=16).rev() {
            second.latent_waiters.insert(LatentId(id), ThreadId(id));
        }
        assert_eq!(first.save_state().unwrap(), second.save_state().unwrap());
    }

    #[test]
    fn invalid_frame_instance_is_rejected_without_replacing_live_state() {
        use crate::asm::{i, FunctionBuilder, ScriptBuilder};
        use crate::pex::Opcode;
        let bytes = ScriptBuilder::new("Probe", "")
            .function(
                FunctionBuilder::new("Run", "Int")
                    .ins(Opcode::Return, vec![i(1)])
                    .build(),
            )
            .bytes();
        let mut vm = Vm::new();
        vm.load_bytes(&bytes).unwrap();
        let object = vm.create_object("probe");
        vm.attach(object, "Probe").unwrap();
        vm.call(object, "Run", &[]).unwrap();
        let valid = vm.save_state().unwrap();
        vm.threads[0].frames[0].instance = Some(InstanceId(usize::MAX));
        let invalid = vm.save_state().unwrap();
        vm.load_state(&valid).unwrap();
        assert!(matches!(vm.load_state(&invalid), Err(VmError::State(_))));
        assert_eq!(vm.save_state().unwrap(), valid);
        vm.run(100);
        assert!(vm.idle());
    }

    #[test]
    fn values_round_trip_and_shared_arrays_stay_shared() {
        let shared = ArrayRef::new(ElementKind::Int, 2);
        shared.items.borrow_mut()[0] = Value::Int(7);
        let nested = ArrayRef::new(ElementKind::Object, 1);
        nested.items.borrow_mut()[0] = Value::Array(shared.clone());
        let values = vec![
            Value::None,
            Value::Object(ObjectId(9)),
            Value::string("Hallo"),
            Value::Int(-3),
            Value::Float(2.5),
            Value::Bool(true),
            Value::Array(shared.clone()),
            Value::Array(shared.clone()),
            Value::Array(nested),
        ];
        let mut pool = ArrayPool::new();
        let mut body = Writer::new();
        body.len(values.len());
        for v in &values {
            pool.write_value(&mut body, v);
        }
        let mut out = Writer::new();
        pool.write_pool(&mut out);
        out.out.extend_from_slice(&body.out);

        let mut r = Reader::new(&out.out);
        let table = ArrayTable::read_pool(&mut r).unwrap();
        let n = r.len().unwrap();
        let back: Vec<Value> = (0..n).map(|_| table.read_value(&mut r).unwrap()).collect();
        r.expect_end().unwrap();
        for (a, b) in values.iter().zip(&back) {
            assert!(
                a.equals(b) || matches!((a, b), (Value::Array(_), Value::Array(_))),
                "{a:?} vs {b:?}"
            );
        }
        let (Value::Array(x), Value::Array(y)) = (&back[6], &back[7]) else {
            panic!("arrays expected");
        };
        assert!(x.same(y), "identity kept");
        assert_eq!(x.get(0), Some(Value::Int(7)));
        let Value::Array(outer) = &back[8] else {
            panic!()
        };
        let Some(Value::Array(inner)) = outer.get(0) else {
            panic!()
        };
        assert!(inner.same(x), "nested array is the same shared one");
    }

    #[test]
    fn a_thread_asleep_in_wait_resumes_after_load() {
        use crate::asm::{f, i, ident, s, FunctionBuilder, ScriptBuilder};
        use crate::pex::Opcode::{CallStatic, IAdd, Return};
        let script = ScriptBuilder::new("Sleepy", "")
            .variable("::n", "Int", i(1))
            .function(
                FunctionBuilder::new("Nap", "Int")
                    .ins(IAdd, vec![ident("::n"), ident("::n"), i(10)])
                    .ins(
                        CallStatic,
                        vec![s("Utility"), s("Wait"), ident("::nonevar"), f(2.0)],
                    )
                    .ins(IAdd, vec![ident("::n"), ident("::n"), i(100)])
                    .ins(Return, vec![ident("::n")])
                    .build(),
            )
            .bytes();
        let mut vm = Vm::new();
        vm.load_bytes(&script).unwrap();
        let obj = vm.create_object("o");
        vm.attach(obj, "Sleepy").unwrap();
        let tid = vm.call(obj, "Nap", &[]).unwrap();
        vm.run(1000);
        assert!(matches!(
            vm.thread_state(tid),
            Some(ThreadState::Sleeping(_))
        ));
        assert_eq!(vm.get_var(obj, "Sleepy", "::n"), Some(Value::Int(11)));
        let bytes = vm.save_state().unwrap();

        let mut fresh = Vm::new();
        fresh.load_bytes(&script).unwrap();
        fresh.load_state(&bytes).unwrap();
        assert!(matches!(
            fresh.thread_state(tid),
            Some(ThreadState::Sleeping(_))
        ));
        fresh.run(1000);
        assert_eq!(fresh.thread_result(tid), None, "still asleep");
        fresh.advance(2.5);
        fresh.run(1000);
        assert_eq!(fresh.thread_result(tid), Some(Value::Int(111)));
        assert_eq!(fresh.get_var(obj, "Sleepy", "::n"), Some(Value::Int(111)));
        assert!(matches!(fresh.thread_state(tid), Some(ThreadState::Done)));
    }

    #[test]
    fn version_one_clock_loads_with_legacy_defaults() {
        let mut vm = Vm::new();
        vm.advance(180.0);
        let mut bytes = as_legacy_state(vm.save_state().unwrap(), 1);
        // Empty log: its four-byte count follows the three version 2 clocks.
        let end = bytes.len() - 4;
        bytes.drain(end - 16..end);
        let mut loaded = Vm::new();
        loaded.load_state(&bytes).unwrap();
        assert_eq!(loaded.time, 180.0);
        assert_eq!(loaded.menu_time, 180.0);
        assert_eq!(loaded.game_hours, 1.0);
    }

    #[test]
    fn version_two_unqualified_variables_expand_into_declared_slots() {
        use crate::asm::{i, ScriptBuilder};
        let parent = ScriptBuilder::new("Parent", "")
            .variable("count", "Int", i(0))
            .variable("inherited", "Int", i(0))
            .build();
        let child = ScriptBuilder::new("Child", "Parent")
            .variable("count", "Int", i(1))
            .build();
        let mut old = Vm::new();
        old.load(parent.clone());
        old.load(child.clone());
        let object = old.create_object("legacy");
        old.attach(object, "Child").unwrap();
        old.instances[0].vars = HashMap::from([
            ("count".into(), Value::Int(77)),
            ("inherited".into(), Value::Int(42)),
        ]);
        old.instances[0].var_types = HashMap::from([
            ("count".into(), "Int".into()),
            ("inherited".into(), "Int".into()),
        ]);
        let bytes = as_legacy_state(old.save_state().unwrap(), 2);
        let mut fresh = Vm::new();
        fresh.load(parent);
        fresh.load(child);
        fresh.load_state(&bytes).unwrap();
        assert_eq!(
            fresh.get_var(object, "Parent", "count"),
            Some(Value::Int(77))
        );
        assert_eq!(
            fresh.get_var(object, "Child", "count"),
            Some(Value::Int(77))
        );
        fresh
            .set_var(object, "Parent", "inherited", Value::Int(9))
            .unwrap();
        assert_eq!(
            fresh
                .vars_on(object, "Child")
                .into_iter()
                .find(|(name, _)| name == "inherited")
                .map(|(_, value)| value),
            Some(Value::Int(9))
        );
        fresh
            .set_var(object, "Parent", "count", Value::Int(12))
            .unwrap();
        assert_eq!(
            fresh.get_var(object, "Child", "count"),
            Some(Value::Int(77))
        );
    }

    #[test]
    fn truncated_state_is_refused() {
        let mut vm = Vm::new();
        let obj = vm.create_object_of("x", "Form");
        let _ = obj;
        let bytes = vm.save_state().unwrap();
        for cut in [0, 3, 5, 6, 10, bytes.len() - 1] {
            assert!(Vm::new().load_state(&bytes[..cut]).is_err(), "cut {cut}");
        }
        let mut wrong = bytes.clone();
        wrong[4] = 9;
        assert!(Vm::new().load_state(&wrong).is_err());
        let mut fresh = Vm::new();
        fresh.load_state(&bytes).unwrap();
        assert_eq!(fresh.object_class(obj), Some("Form"));
    }
}
