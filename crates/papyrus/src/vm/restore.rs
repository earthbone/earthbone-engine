// SPDX-License-Identifier: GPL-3.0-or-later

//! Import named script variables without initialization events.
//! Saved slot ordering is the caller's format-specific responsibility.
//! Instance construction follows the existing VM attachment model.

use std::collections::{HashMap, HashSet};

use crate::pex::Instruction;

use super::{
    operand_to_value, variable_key, Frame, Instance, InstanceId, ObjectId, Thread, ThreadId,
    ThreadState, Value, Vm, VmError,
};

/// One saved instance after its object identity and variable names are resolved.
#[derive(Clone, Debug)]
pub struct InstanceState {
    pub object: ObjectId,
    pub script: String,
    pub state: String,
    pub variables: Vec<SavedVariable>,
}

/// A saved variable paired with its saved declaration.
#[derive(Clone, Debug)]
pub struct SavedVariable {
    pub declaring_script: String,
    pub name: String,
    pub type_name: String,
    pub value: Value,
}

/// One saved Papyrus frame after save-format names and values are resolved.
#[derive(Clone, Debug)]
pub struct SavedFrame {
    /// The object the frame runs on. `None` for a global function, which has
    /// no instance or self object, as in `call_static`.
    pub object: Option<ObjectId>,
    pub script: String,
    pub function: String,
    pub pc: usize,
    pub locals: Vec<(String, Value)>,
    pub code: Vec<Instruction>,
    pub return_dest: Option<String>,
}

/// A saved thread in caller to callee frame order.
#[derive(Clone, Debug)]
pub struct SavedThread {
    pub frames: Vec<SavedFrame>,
}

impl Vm {
    /// Restore active threads after their objects and instances are restored.
    /// The full batch is checked before any thread is added.
    ///
    /// # Errors
    ///
    /// Returns an error for missing functions, mismatched bytecode, invalid
    /// positions or locals, unresolved owners, or existing live threads.
    pub fn restore_threads(&mut self, saved: &[SavedThread]) -> Result<Vec<ThreadId>, VmError> {
        if self
            .threads
            .iter()
            .any(|thread| thread.state != ThreadState::Done)
        {
            return Err(VmError::State(
                "thread restoration requires no live threads".into(),
            ));
        }
        let mut threads = Vec::with_capacity(saved.len());
        for saved_thread in saved {
            if saved_thread.frames.is_empty() || saved_thread.frames.len() > super::MAX_DEPTH {
                return Err(VmError::State(
                    "saved thread has an invalid frame count".into(),
                ));
            }
            let mut frames = Vec::with_capacity(saved_thread.frames.len());
            for (index, saved_frame) in saved_thread.frames.iter().enumerate() {
                let (instance, script, function) = match saved_frame.object {
                    Some(object) => {
                        let (instance, script, function) = self
                            .find_script_method(object, &saved_frame.script, &saved_frame.function)
                            .ok_or_else(|| {
                                VmError::State(format!(
                                    "saved function {}.{} is unavailable",
                                    saved_frame.script, saved_frame.function
                                ))
                            })?;
                        (Some(instance), script, function)
                    }
                    None => {
                        let script = self.script(&saved_frame.script).ok_or_else(|| {
                            VmError::State(format!(
                                "saved global function {}.{} is unavailable",
                                saved_frame.script, saved_frame.function
                            ))
                        })?;
                        let function = script
                            .functions
                            .get(&(String::new(), saved_frame.function.to_ascii_lowercase()))
                            .cloned()
                            .ok_or_else(|| {
                                VmError::State(format!(
                                    "saved global function {}.{} is unavailable",
                                    saved_frame.script, saved_frame.function
                                ))
                            })?;
                        (None, script, function)
                    }
                };
                // A handler that changes state while it runs is saved after the
                // instance is already in the new state, so the frame belongs
                // to the same-named function of an earlier state. Find the
                // body in any state of the declaring script that matches.
                let function = if same_code(&saved_frame.code, &function.code) {
                    function
                } else {
                    let name = saved_frame.function.to_ascii_lowercase();
                    script
                        .functions
                        .iter()
                        .filter(|((_, candidate), _)| *candidate == name)
                        .map(|(_, candidate)| candidate)
                        .find(|candidate| same_code(&saved_frame.code, &candidate.code))
                        .cloned()
                        .ok_or_else(|| {
                            VmError::State(format!(
                                "saved function {}.{} differs from loaded PEX",
                                saved_frame.script, saved_frame.function
                            ))
                        })?
                };
                if saved_frame.pc > function.code.len()
                    || saved_frame.locals.len() != function.params.len() + function.locals.len()
                {
                    return Err(VmError::State(
                        "saved frame position or local count is invalid".into(),
                    ));
                }
                let mut locals = HashMap::new();
                let mut local_types = HashMap::new();
                for (desc, (name, value)) in function
                    .params
                    .iter()
                    .chain(&function.locals)
                    .zip(&saved_frame.locals)
                {
                    // SOURCE: a frame suspended inside a native function
                    // (Utility.Wait) has no PEX body, so the game names its
                    // arguments `param1`, `param2` by position. They map onto
                    // the declared parameters in order.
                    let positional = is_positional_param(name);
                    if !positional && !desc.name.eq_ignore_ascii_case(name) {
                        return Err(VmError::State(format!(
                            "saved local {name} differs from PEX {} in {}.{}",
                            desc.name, saved_frame.script, saved_frame.function
                        )));
                    }
                    let key = desc.name.to_ascii_lowercase();
                    locals.insert(key.clone(), value.clone());
                    local_types.insert(key, desc.type_name.clone());
                }
                if index > 0 && saved_frame.return_dest.is_none() {
                    return Err(VmError::State(
                        "saved caller has no return destination".into(),
                    ));
                }
                frames.push(Frame {
                    coverage_recorded: false,
                    profile_recorded: false,
                    instance,
                    self_object: saved_frame.object,
                    script: Some(script),
                    function,
                    pc: saved_frame.pc,
                    locals,
                    local_types,
                    ret_dest: saved_frame.return_dest.clone(),
                });
            }
            threads.push(frames);
        }
        let mut ids = Vec::with_capacity(threads.len());
        for frames in threads {
            self.next_thread += 1;
            let id = ThreadId(self.next_thread);
            self.threads.push(Thread {
                id,
                frames,
                state: ThreadState::Runnable,
                result: Value::None,
                pending_dest: None,
            });
            ids.push(id);
        }
        Ok(ids)
    }

    /// Replace attached instances after validating the entire batch.
    ///
    /// Objects and PEX scripts must already exist. No events or functions run.
    /// Shared values retain their identity. The returned IDs follow input order.
    ///
    /// # Errors
    ///
    /// Existing threads, missing objects or scripts, cyclic inheritance,
    /// duplicate target instances or mismatched declarations.
    /// Live VM state is unchanged on error.
    pub fn restore_instances(
        &mut self,
        saved: &[InstanceState],
    ) -> Result<Vec<InstanceId>, VmError> {
        if !self.threads.is_empty() {
            return Err(VmError::State(
                "instance restoration requires a VM without threads".into(),
            ));
        }
        let mut instances = Vec::with_capacity(saved.len());
        let mut targets = HashSet::new();
        for input in saved {
            if !self.objects.contains_key(&input.object) {
                return Err(VmError::UnknownObject(input.object));
            }
            if !targets.insert((input.object, input.script.to_ascii_lowercase())) {
                return Err(VmError::State(format!(
                    "duplicate restored instance {} on {}",
                    input.script, input.object.0
                )));
            }
            let mut chain = Vec::new();
            let mut visited = HashSet::new();
            let mut next = Some(input.script.clone());
            while let Some(name) = next {
                if !visited.insert(name.to_ascii_lowercase()) {
                    return Err(VmError::State(format!(
                        "cyclic script inheritance at {name}"
                    )));
                }
                let script = self.script(&name).ok_or(VmError::UnknownScript(name))?;
                next = script.parent.clone();
                chain.push(script);
            }
            let mut vars = HashMap::new();
            let mut var_types = HashMap::new();
            for script in chain.iter().rev() {
                for variable in &script.object.variables {
                    let key = variable_key(&script.name, &variable.name);
                    vars.insert(
                        key.clone(),
                        operand_to_value(&variable.default, Some(&variable.type_name)),
                    );
                    var_types.insert(key, variable.type_name.clone());
                }
            }
            let mut names = HashSet::new();
            for variable in &input.variables {
                let key = variable_key(&variable.declaring_script, &variable.name);
                if !names.insert(key.clone()) {
                    return Err(VmError::State(format!(
                        "duplicate saved variable {}.{}",
                        input.script, variable.name
                    )));
                }
                let Some(type_name) = var_types.get(&key) else {
                    return Err(VmError::State(format!(
                        "unknown saved variable {}.{}",
                        input.script, variable.name
                    )));
                };
                if !type_name.eq_ignore_ascii_case(&variable.type_name) {
                    return Err(VmError::State(format!(
                        "saved variable {}.{} has type {}, loaded type is {type_name}",
                        input.script, variable.name, variable.type_name
                    )));
                }
                vars.insert(key, variable.value.clone());
            }
            instances.push(Instance {
                object: input.object,
                chain,
                state: input.state.clone(),
                vars,
                var_types,
            });
        }
        for object in self.objects.values_mut() {
            object.instances.clear();
        }
        for (index, instance) in instances.iter().enumerate() {
            if let Some(object) = self.objects.get_mut(&instance.object) {
                object.instances.push(InstanceId(index));
            }
        }
        self.instances = instances;
        Ok((0..saved.len()).map(InstanceId).collect())
    }
}

/// Whether a saved function body is the loaded PEX body.
///
/// SOURCE: the ESS string table folds case, so a saved operand carries the
/// first spelling the game met (`ShouldSpawn` where the PEX says
/// `shouldSpawn`), and the call opcodes save their function names as string
/// operands where the PEX stores identifiers. Papyrus names are case
/// insensitive, so text operands compare by value without case and a string
/// stands for an identifier. Opcodes, operand counts and every number must
/// still match exactly.
fn same_code(saved: &[crate::pex::Instruction], loaded: &[crate::pex::Instruction]) -> bool {
    use crate::pex::Operand;
    fn same_operand(a: &Operand, b: &Operand) -> bool {
        match (a, b) {
            (
                Operand::Identifier(a) | Operand::Str(a),
                Operand::Identifier(b) | Operand::Str(b),
            ) => a.eq_ignore_ascii_case(b),
            _ => a == b,
        }
    }
    saved.len() == loaded.len()
        && saved.iter().zip(loaded).all(|(a, b)| {
            a.op == b.op
                && a.args.len() == b.args.len()
                && a.args.iter().zip(&b.args).all(|(a, b)| same_operand(a, b))
        })
}

/// Whether a saved local name is the positional `paramN` form of a native frame.
fn is_positional_param(name: &str) -> bool {
    name.strip_prefix("param")
        .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}
