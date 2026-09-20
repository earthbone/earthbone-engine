// SPDX-License-Identifier: GPL-3.0-or-later

//! Import named script variables without initialization events.
//! Saved slot ordering is the caller's format-specific responsibility.
//! Instance construction follows the existing VM attachment model.

use std::collections::{HashMap, HashSet};

use super::{operand_to_value, variable_key, Instance, InstanceId, ObjectId, Value, Vm, VmError};

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

impl Vm {
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
