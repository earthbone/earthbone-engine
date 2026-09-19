// SPDX-License-Identifier: GPL-3.0-or-later

//! VM dispatch, extracted from the interpreter described in docs/papyrus.md.
//! The public API remains on Vm and uses the same single scheduler.

use super::{
    Function, InstanceId, LoadedScript, NativeCall, NativeRegistry, NativeReturn, ObjectId, Rc,
    ThreadId, ThreadState, Value, Vm, VmError, MAX_DEPTH,
};

impl Vm {
    /// Find a script function on an object. Returns the defining script, the
    /// instance, and the function.
    pub(super) fn find_method(
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
    pub(super) fn class_names(&self, object: ObjectId) -> Vec<String> {
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

    /// Finish the top frame and hand its value to the caller.
    pub(super) fn return_from(&mut self, t: usize, value: Value) {
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
    pub(super) fn push_call(
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
    pub(super) fn call_native(
        &mut self,
        t: usize,
        global: bool,
        class: &str,
        name: &str,
        self_object: Option<ObjectId>,
        args: Vec<Value>,
        dest: Option<String>,
    ) -> bool {
        let _span = soe_trace::Span::dynamic(|| format!("{class}.{name}"), "native");
        let key = NativeRegistry::key(class, name);
        let table = if global {
            &mut self.natives.globals
        } else {
            &mut self.natives.methods
        };
        let Some(f) = table.get_mut(&key) else {
            return false;
        };
        *self
            .natives
            .calls
            .entry(format!("{}.{}", key.0, key.1))
            .or_insert(0) += 1;
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
    pub(super) fn builtin_global(
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
    pub(super) fn builtin_method(
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

    pub(super) fn call_method(
        &mut self,
        t: usize,
        obj: ObjectId,
        name: &str,
        args: &[Value],
        dest: Option<String>,
        start_after: Option<String>,
    ) {
        if start_after.is_none() && self.builtin_method(t, obj, name, args, dest.clone()) {
            return;
        }
        if let Some((iid, script, f)) = self.find_method(obj, name, start_after.as_deref()) {
            if f.is_native() {
                let class = script.name.clone();
                if self.call_native(
                    t,
                    false,
                    &class,
                    name,
                    Some(obj),
                    args.to_vec(),
                    dest.clone(),
                ) {
                    return;
                }
                // A script may redeclare a parent's native (`Alias.pex`
                // declares `RegisterForUpdate`, the registry holds it on
                // `Form`), so walk the classes above the declaring one.
                let lclass = class.to_ascii_lowercase();
                let mut classes = self.class_names(obj);
                if let Some(pos) = classes.iter().position(|c| *c == lclass) {
                    classes.drain(..=pos);
                }
                for parent in classes {
                    if self.call_native(
                        t,
                        false,
                        &parent,
                        name,
                        Some(obj),
                        args.to_vec(),
                        dest.clone(),
                    ) {
                        return;
                    }
                }
                self.err(t, &format!("native {class}.{name}() has no implementation"));
                if let Some(d) = dest {
                    let frame = self.threads[t].frames.last_mut().unwrap();
                    Self::store(&mut self.instances, frame, &d, Value::None);
                }
                return;
            }
            self.push_call(t, Some(iid), Some(obj), Some(script), f, args, dest);
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
                args.to_vec(),
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

    pub(super) fn call_static(
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
}
