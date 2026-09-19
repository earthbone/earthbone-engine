// SPDX-License-Identifier: GPL-3.0-or-later

//! VM properties, extracted from the interpreter described in docs/papyrus.md.
//! The public API remains on Vm and uses the same single scheduler.

use super::{
    Function, InstanceId, LoadedScript, ObjectId, Rc, ThreadState, Value, Vm, VmError,
    SETTER_BUDGET,
};

impl Vm {
    /// Fill a property, the way the loader fills properties from a plugin's
    /// `VMAD` data. An auto property writes its backing variable directly. A
    /// full property runs the compiled `setter` to completion first, the same
    /// frame-and-run machinery [`Vm::call`] uses, so the game's own logic
    /// (the trap family's `TriggerType` doubling into a separate stored
    /// variable, for one) applies exactly as it would from a running script.
    /// A read-only full property (a getter and no setter) is reported as
    /// [`VmError::ReadOnlyProperty`] rather than run at all.
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
        let auto_var = self.instances[id.0]
            .chain
            .iter()
            .find_map(|s| s.object.property(property).and_then(|p| p.auto_var.clone()));
        if let Some(var) = auto_var {
            self.instances[id.0]
                .vars
                .insert(var.to_ascii_lowercase(), value);
            return Ok(());
        }
        // A full property has no backing variable of its own: the game runs
        // its compiled Set function, so the host does the same.
        let key = property.to_ascii_lowercase();
        let Some((owner, setter)) = self.instances[id.0]
            .chain
            .iter()
            .find_map(|s| s.setters.get(&key).map(|f| (s.clone(), f.clone())))
        else {
            // The property may still exist as a getter-only full property,
            // counted apart from one the loaded PEX has never heard of.
            let known = self.instances[id.0]
                .chain
                .iter()
                .any(|s| s.object.property(property).is_some());
            return Err(if known {
                VmError::ReadOnlyProperty {
                    class: script.to_string(),
                    name: property.to_string(),
                }
            } else {
                VmError::UnknownFunction {
                    class: script.to_string(),
                    name: property.to_string(),
                }
            });
        };
        self.run_property_call(
            id,
            object,
            owner,
            setter,
            &[value],
            &format!("{script}.{property} setter"),
        );
        Ok(())
    }

    /// Read a property, running the compiled `getter` to completion for a
    /// full property the same way [`Vm::set_property`] runs the `setter`. A
    /// write-only full property (a setter and no getter) reads as `None`.
    pub fn get_property(
        &mut self,
        object: ObjectId,
        script: &str,
        property: &str,
    ) -> Option<Value> {
        let id = self.instance_on(object, script)?;
        let auto_var = self.instances[id.0]
            .chain
            .iter()
            .find_map(|s| s.object.property(property).and_then(|p| p.auto_var.clone()));
        if let Some(var) = auto_var {
            return self.instances[id.0]
                .vars
                .get(&var.to_ascii_lowercase())
                .cloned();
        }
        let key = property.to_ascii_lowercase();
        let (owner, getter) = self.instances[id.0]
            .chain
            .iter()
            .find_map(|s| s.getters.get(&key).map(|f| (s.clone(), f.clone())))?;
        self.run_property_call(
            id,
            object,
            owner,
            getter,
            &[],
            &format!("{script}.{property} getter"),
        )
    }

    /// Spawn a call to a full property's compiled `getter` or `setter` and
    /// run it to completion outside the scheduler, the way [`Vm::set_property`]
    /// and [`Vm::get_property`] fill or read a property from a plugin's
    /// `VMAD` data rather than from a running thread. A call still runnable
    /// after the budget is logged and abandoned instead of retried, and
    /// reads back as `None`, which `set_property` ignores either way. A
    /// script error inside the call is logged like any other and never
    /// surfaces to the caller.
    pub(super) fn run_property_call(
        &mut self,
        instance: InstanceId,
        object: ObjectId,
        owner: Rc<LoadedScript>,
        function: Rc<Function>,
        args: &[Value],
        label: &str,
    ) -> Option<Value> {
        let frame = Self::make_frame(
            Some(instance),
            Some(object),
            Some(owner),
            function,
            args,
            None,
        );
        let tid = self.spawn(frame);
        for _ in 0..64 {
            self.run(SETTER_BUDGET);
            if self.thread_state(tid) != Some(ThreadState::Runnable) {
                break;
            }
        }
        if self.thread_state(tid) != Some(ThreadState::Done) {
            let time = self.time;
            self.log
                .push(time, format!("{label} did not finish synchronously"));
            return None;
        }
        self.thread_result(tid)
    }

    /// The instance on `obj` whose chain declares the property, with the
    /// declaring script.
    pub(super) fn find_property(
        &self,
        obj: ObjectId,
        prop: &str,
    ) -> Option<(InstanceId, Rc<LoadedScript>)> {
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

    pub(super) fn prop_get(&mut self, t: usize, obj: ObjectId, prop: &str, dest: Option<String>) {
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

    pub(super) fn prop_set(&mut self, t: usize, obj: ObjectId, prop: &str, value: Value) {
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
