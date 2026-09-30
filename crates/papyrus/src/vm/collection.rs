// SPDX-License-Identifier: GPL-3.0-or-later

//! Targeted reclamation of host objects whose native lifetime has ended.

use std::collections::HashSet;
use std::rc::Rc;

use super::{InstanceId, Vm};
use crate::{ObjectId, Value};

impl Vm {
    /// Remove candidates unreachable from other objects, thread frames/results
    /// and `host_roots`. Returns removed object IDs in ascending order.
    ///
    /// The host must include queued callbacks and any values it retains outside
    /// the VM. Bare object IDs are not implicit roots. Instances are compacted,
    /// so callers must reacquire instance indices after this call. Object IDs
    /// remain stable and are never reused. A borrowed array defers collection.
    pub fn collect_objects(
        &mut self,
        candidates: &[ObjectId],
        host_roots: &[Value],
    ) -> Vec<ObjectId> {
        let candidates: HashSet<_> = candidates
            .iter()
            .copied()
            .filter(|id| self.objects.contains_key(id))
            .collect();
        if candidates.is_empty() {
            return Vec::new();
        }
        let mut work = host_roots.to_vec();
        work.extend(
            self.objects
                .keys()
                .filter(|id| !candidates.contains(id))
                .copied()
                .map(Value::Object),
        );
        for thread in &self.threads {
            work.push(thread.result.clone());
            for frame in &thread.frames {
                work.extend(frame.self_object.map(Value::Object));
                if let Some(id) = frame.instance {
                    work.push(Value::Object(self.instances[id.0].object));
                }
                work.extend(frame.locals.values().cloned());
            }
        }
        let mut reachable = HashSet::new();
        let mut arrays = HashSet::new();
        while let Some(value) = work.pop() {
            match value {
                Value::Object(id) if reachable.insert(id) => {
                    if let Some(object) = self.objects.get(&id) {
                        for instance in &object.instances {
                            work.extend(self.instances[instance.0].vars.values().cloned());
                        }
                    }
                }
                Value::Array(array) if arrays.insert(Rc::as_ptr(&array.items)) => {
                    let Ok(items) = array.items.try_borrow() else {
                        return Vec::new();
                    };
                    work.extend(items.iter().cloned());
                }
                Value::Struct(value) => {
                    work.extend(value.fields.borrow().values().cloned());
                }
                _ => {}
            }
        }
        let removed: HashSet<_> = candidates.difference(&reachable).copied().collect();
        if removed.is_empty() {
            return Vec::new();
        }
        self.objects.retain(|id, _| !removed.contains(id));
        let mut remap = vec![None; self.instances.len()];
        let mut next = 0;
        for (index, instance) in self.instances.iter().enumerate() {
            if !removed.contains(&instance.object) {
                remap[index] = Some(InstanceId(next));
                next += 1;
            }
        }
        self.instances
            .retain(|instance| !removed.contains(&instance.object));
        for object in self.objects.values_mut() {
            for instance in &mut object.instances {
                // Surviving objects retain every attached instance.
                *instance = remap[instance.0].unwrap();
            }
        }
        for thread in &mut self.threads {
            for frame in &mut thread.frames {
                frame.instance = frame.instance.map(|id| {
                    // The frame roots its instance owner.
                    remap[id.0].unwrap()
                });
            }
        }
        let mut removed: Vec<_> = removed.into_iter().collect();
        removed.sort_unstable();
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::{f, i, ident, none, s, FunctionBuilder, ScriptBuilder};
    use crate::pex::Opcode::*;
    use crate::value::{ArrayRef, ElementKind};
    use crate::{NativeReturn, ThreadState};

    fn vm() -> Vm {
        let script = ScriptBuilder::new("Probe", "")
            .variable("peer", "Probe", none())
            .variable("peers", "Probe[]", none())
            .variable("count", "Int", i(0))
            .function(
                FunctionBuilder::new("Finish", "None")
                    .ins(
                        CallStatic,
                        vec![s("Utility"), s("Wait"), ident("unused"), f(1.0)],
                    )
                    .ins(IAdd, vec![ident("count"), ident("count"), i(1)])
                    .build(),
            )
            .function(
                FunctionBuilder::new("Hold", "None")
                    .param("held", "Probe")
                    .ins(CallStatic, vec![s("Host"), s("Wait"), ident("unused")])
                    .build(),
            )
            .function(
                FunctionBuilder::new("Echo", "Probe")
                    .param("held", "Probe")
                    .ins(Return, vec![ident("held")])
                    .build(),
            )
            .bytes();
        let mut vm = Vm::new();
        vm.load_bytes(&script).unwrap();
        vm
    }

    fn object(vm: &mut Vm) -> ObjectId {
        let id = vm.create_object("probe");
        vm.attach(id, "Probe").unwrap();
        id
    }

    #[test]
    fn collection_traces_live_arrays_and_reclaims_unreachable_object_cycles() {
        let mut vm = vm();
        let first = object(&mut vm);
        let second = object(&mut vm);
        let live = object(&mut vm);
        vm.set_var(first, "Probe", "peer", Value::Object(second))
            .unwrap();
        vm.set_var(second, "Probe", "peer", Value::Object(first))
            .unwrap();
        let array = ArrayRef::new(ElementKind::Object, 1);
        array.items.borrow_mut()[0] = Value::Object(first);
        vm.set_var(live, "Probe", "peers", Value::Array(array.clone()))
            .unwrap();
        assert!(vm.collect_objects(&[first, second], &[]).is_empty());
        array.items.borrow_mut()[0] = Value::None;
        assert_eq!(
            vm.collect_objects(&[first, second], &[]),
            vec![first, second]
        );
        assert_eq!(vm.objects.len(), 1);
        assert_eq!(vm.instances.len(), 1);
        assert_eq!(vm.object(live).unwrap().instances, vec![InstanceId(0)]);
        assert_eq!(vm.get_var(live, "Probe", "count"), Some(Value::Int(0)));
    }

    #[test]
    fn collection_preserves_sleeping_frames_during_compaction_and_save_load() {
        let mut vm = vm();
        let expired = object(&mut vm);
        let sleeping = object(&mut vm);
        let thread = vm.call(sleeping, "Finish", &[]).unwrap();
        assert!(vm.run_until_idle(1000));
        assert_eq!(vm.thread_state(thread), Some(ThreadState::Sleeping(1.0)));
        assert_eq!(vm.collect_objects(&[expired, sleeping], &[]), vec![expired]);
        assert_eq!(vm.object(sleeping).unwrap().instances, vec![InstanceId(0)]);
        let bytes = vm.save_state().unwrap();
        vm.load_state(&bytes).unwrap();
        vm.advance(1.0);
        assert!(vm.run_until_idle(1000));
        assert_eq!(vm.get_var(sleeping, "Probe", "count"), Some(Value::Int(1)));
        assert_eq!(vm.collect_objects(&[sleeping], &[]), vec![sleeping]);
        assert!(vm.instances.is_empty());
    }

    #[test]
    fn collection_preserves_latent_frame_arguments_and_completed_results() {
        let mut vm = vm();
        vm.natives.register_global(
            "Host",
            "Wait",
            Box::new(|call| NativeReturn::Latent(call.defer())),
        );
        let held = object(&mut vm);
        let thread = vm
            .call_global("Probe", "Hold", &[Value::Object(held)])
            .unwrap();
        assert!(vm.run_until_idle(1000));
        assert!(vm.collect_objects(&[held], &[]).is_empty());
        let Some(ThreadState::Latent(latent)) = vm.thread_state(thread) else {
            panic!("expected latent wait");
        };
        assert!(vm.resolve_latent(latent, Value::None));
        assert!(vm.run_until_idle(1000));
        let result = vm
            .call_global("Probe", "Echo", &[Value::Object(held)])
            .unwrap();
        assert!(vm.run_until_idle(1000));
        assert!(vm.collect_objects(&[held], &[]).is_empty());
        assert_eq!(vm.thread_result(result), Some(Value::Object(held)));
    }

    #[test]
    fn collection_defers_for_borrowed_arrays_and_handles_array_cycles() {
        let mut vm = vm();
        let expired = object(&mut vm);
        let array = ArrayRef::new(ElementKind::Object, 1);
        array.items.borrow_mut()[0] = Value::Array(array.clone());
        let roots = [Value::Array(array.clone())];
        let borrow = array.items.borrow_mut();
        assert!(vm.collect_objects(&[expired], &roots).is_empty());
        drop(borrow);
        assert_eq!(vm.collect_objects(&[expired], &roots), vec![expired]);
        array.items.borrow_mut().clear();
    }

    #[test]
    fn collection_bounds_object_and_instance_storage_over_repeated_lifetimes() {
        let mut vm = vm();
        let live = object(&mut vm);
        for _ in 0..256 {
            let expired = object(&mut vm);
            assert!(vm
                .collect_objects(&[expired], &[Value::Object(expired)])
                .is_empty());
            assert_eq!(vm.collect_objects(&[expired], &[]), vec![expired]);
            assert_eq!(vm.objects.len(), 1);
            assert_eq!(vm.instances.len(), 1);
        }
        assert_eq!(vm.get_var(live, "Probe", "count"), Some(Value::Int(0)));
    }
}
