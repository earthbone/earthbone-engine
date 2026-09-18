// SPDX-License-Identifier: GPL-3.0-or-later

//! A fixed-seed hash of the VM's run-time state for the determinism
//! detector (`docs/validation.md`, "Determinism"). Every map is visited in
//! key order so the hash never depends on `HashMap` iteration, and every
//! float goes in as its bit pattern. Two runs of one seed that hash
//! differently differ in behaviour, and the per-subsystem parts say where.

use std::collections::HashMap;

use super::{ThreadState, Vm};
use crate::value::Value;

/// FNV-1a over 64 bits. No per-process seed, so a hash means the same
/// thing in every process and on every machine of the same width.
#[derive(Debug, Clone, Copy)]
pub struct StateHasher(u64);

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const PRIME: u64 = 0x0100_0000_01b3;

/// How deep nested arrays are followed. Papyrus arrays hold values, not
/// arrays, so anything deeper than one level is a host construct.
const ARRAY_DEPTH: usize = 4;

impl StateHasher {
    #[must_use]
    pub const fn new() -> Self {
        Self(OFFSET)
    }

    pub fn u8(&mut self, v: u8) {
        self.0 ^= u64::from(v);
        self.0 = self.0.wrapping_mul(PRIME);
    }

    pub fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.u8(x);
        }
    }

    pub fn bool(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }

    pub fn f64(&mut self, v: f64) {
        self.u64(v.to_bits());
    }

    pub fn len(&mut self, n: usize) {
        self.u64(u64::try_from(n).unwrap_or(u64::MAX));
    }

    pub fn str(&mut self, s: &str) {
        self.len(s.len());
        self.bytes(s.as_bytes());
    }

    pub fn opt_str(&mut self, s: Option<&str>) {
        match s {
            None => self.u8(0),
            Some(s) => {
                self.u8(1);
                self.str(s);
            }
        }
    }

    pub fn opt_u64(&mut self, v: Option<u64>) {
        match v {
            None => self.u8(0),
            Some(v) => {
                self.u8(1);
                self.u64(v);
            }
        }
    }

    /// Fold a finished sub-hash in.
    pub fn sub(&mut self, h: u64) {
        self.u64(h);
    }

    /// Visit a map in key order, so the result does not depend on the
    /// map's iteration order.
    pub fn map<K: Ord, V>(&mut self, map: &HashMap<K, V>, mut each: impl FnMut(&mut Self, &K, &V)) {
        let mut entries: Vec<(&K, &V)> = map.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        self.len(entries.len());
        for (k, v) in entries {
            each(self, k, v);
        }
    }

    pub fn value(&mut self, v: &Value) {
        self.value_at(v, 0);
    }

    fn value_at(&mut self, v: &Value, depth: usize) {
        match v {
            Value::None => self.u8(0),
            Value::Object(id) => {
                self.u8(1);
                self.u64(id.0);
            }
            Value::Str(s) => {
                self.u8(2);
                self.str(s);
            }
            Value::Int(i) => {
                self.u8(3);
                self.i32(*i);
            }
            Value::Float(f) => {
                self.u8(4);
                self.f32(*f);
            }
            Value::Bool(b) => {
                self.u8(5);
                self.bool(*b);
            }
            Value::Array(a) => {
                self.u8(6);
                if depth >= ARRAY_DEPTH {
                    self.len(a.len());
                    return;
                }
                let items = a.items.borrow();
                self.len(items.len());
                for item in items.iter() {
                    self.value_at(item, depth + 1);
                }
            }
        }
    }

    #[must_use]
    pub fn finish(self) -> u64 {
        self.0
    }
}

/// The VM's state hash, one part per subsystem so a divergence names one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmHash {
    /// Game objects: names, classes, attached instance ids, the allocator.
    pub objects: u64,
    /// Script instances: state name and every variable.
    pub instances: u64,
    /// Threads: frames, program counters, locals, results, the clock.
    pub threads: u64,
    /// Latent call ids and which thread waits on each.
    pub latents: u64,
    /// The trace log, message by message.
    pub log: u64,
    /// Native call counts by name.
    pub natives: u64,
}

impl VmHash {
    #[must_use]
    pub fn parts(&self) -> [(&'static str, u64); 6] {
        [
            ("vm.objects", self.objects),
            ("vm.instances", self.instances),
            ("vm.threads", self.threads),
            ("vm.latents", self.latents),
            ("vm.log", self.log),
            ("vm.natives", self.natives),
        ]
    }
}

impl Vm {
    /// Hash every piece of run-time state the VM owns. Loaded scripts and
    /// the native tables are inputs, not state, and stay out.
    #[must_use]
    pub fn state_hash(&self) -> VmHash {
        let mut h = StateHasher::new();
        h.map(&self.objects, |h, id, o| {
            h.u64(id.0);
            h.str(&o.name);
            h.opt_str(o.class.as_deref());
            h.len(o.instances.len());
            for i in &o.instances {
                h.len(i.0);
            }
        });
        h.u64(self.next_object);
        let objects = h.finish();

        let mut h = StateHasher::new();
        h.len(self.instances.len());
        for inst in &self.instances {
            h.u64(inst.object.0);
            h.len(inst.chain.len());
            for script in &inst.chain {
                h.str(&script.name);
            }
            h.str(&inst.state);
            h.map(&inst.vars, |h, k, v| {
                h.str(k);
                h.value(v);
            });
        }
        let instances = h.finish();

        let mut h = StateHasher::new();
        h.len(self.threads.len());
        for t in &self.threads {
            h.u64(t.id.0);
            match &t.state {
                ThreadState::Runnable => h.u8(0),
                ThreadState::Sleeping(until) => {
                    h.u8(1);
                    h.f64(*until);
                }
                ThreadState::Latent(id) => {
                    h.u8(2);
                    h.u64(id.0);
                }
                ThreadState::Done => h.u8(3),
            }
            h.value(&t.result);
            h.opt_str(t.pending_dest.as_deref());
            h.len(t.frames.len());
            for f in &t.frames {
                h.opt_u64(f.instance.map(|i| u64::try_from(i.0).unwrap_or(u64::MAX)));
                h.opt_u64(f.self_object.map(|o| o.0));
                h.opt_str(f.script.as_ref().map(|s| s.name.as_str()));
                h.str(&f.function.name);
                h.len(f.pc);
                h.map(&f.locals, |h, k, v| {
                    h.str(k);
                    h.value(v);
                });
                h.opt_str(f.ret_dest.as_deref());
            }
        }
        h.u64(self.next_thread);
        h.f64(self.time);
        let threads = h.finish();

        let mut h = StateHasher::new();
        h.u64(self.latent_counter);
        let mut waiters: Vec<(u64, u64)> = self
            .latent_waiters
            .iter()
            .map(|(k, v)| (k.0, v.0))
            .collect();
        waiters.sort_unstable();
        h.len(waiters.len());
        for (latent, thread) in waiters {
            h.u64(latent);
            h.u64(thread);
        }
        let latents = h.finish();

        let mut h = StateHasher::new();
        h.len(self.log.entries.len());
        for e in &self.log.entries {
            h.f64(e.time);
            h.str(&e.message);
        }
        let log = h.finish();

        let mut h = StateHasher::new();
        h.map(&self.natives.calls, |h, k, v| {
            h.str(k);
            h.u64(*v);
        });
        let natives = h.finish();

        VmHash {
            objects,
            instances,
            threads,
            latents,
            log,
            natives,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{ArrayRef, ElementKind, ObjectId};

    #[test]
    fn map_order_does_not_change_the_hash() {
        let mut a = HashMap::new();
        let mut b = HashMap::new();
        for i in 0..64u32 {
            a.insert(i, i * 2);
        }
        for i in (0..64u32).rev() {
            b.insert(i, i * 2);
        }
        let mut ha = StateHasher::new();
        ha.map(&a, |h, k, v| {
            h.u32(*k);
            h.u32(*v);
        });
        let mut hb = StateHasher::new();
        hb.map(&b, |h, k, v| {
            h.u32(*k);
            h.u32(*v);
        });
        assert_eq!(ha.finish(), hb.finish());
    }

    #[test]
    fn values_hash_by_content_and_kind() {
        let mut h1 = StateHasher::new();
        h1.value(&Value::Int(1));
        let mut h2 = StateHasher::new();
        h2.value(&Value::Float(1.0));
        assert_ne!(h1.finish(), h2.finish());

        let arr = ArrayRef::new(ElementKind::Int, 2);
        arr.items.borrow_mut()[1] = Value::Int(7);
        let same = ArrayRef::new(ElementKind::Int, 2);
        same.items.borrow_mut()[1] = Value::Int(7);
        let mut h1 = StateHasher::new();
        h1.value(&Value::Array(arr));
        let mut h2 = StateHasher::new();
        h2.value(&Value::Array(same));
        assert_eq!(
            h1.finish(),
            h2.finish(),
            "two arrays with equal items hash alike"
        );

        let mut h1 = StateHasher::new();
        h1.value(&Value::Object(ObjectId(3)));
        let mut h2 = StateHasher::new();
        h2.value(&Value::Object(ObjectId(4)));
        assert_ne!(h1.finish(), h2.finish());
    }

    #[test]
    fn a_fresh_vm_hashes_the_same_twice() {
        let a = Vm::new().state_hash();
        let b = Vm::new().state_hash();
        assert_eq!(a, b);
    }
}
