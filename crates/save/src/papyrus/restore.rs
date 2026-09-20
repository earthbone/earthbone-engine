// SPDX-License-Identifier: GPL-3.0-or-later

//! Papyrus saved slots to runtime instances. Parent members precede child
//! members in FallrimTools Script.getExtendedMembers. IDs are resolved by the
//! host, never inferred from the saved form field or a transient VM pointer.

use std::collections::{BTreeMap, BTreeSet};

use soe_papyrus::value::ElementKind;
use soe_papyrus::vm::restore::{InstanceState, SavedVariable};
use soe_papyrus::vm::InstanceId;
use soe_papyrus::{ArrayRef, ObjectId, Value as RuntimeValue, Vm};

use super::{bad, MemberDesc, Papyrus, Result, ScriptDef, StringRef, Value, ValueType};

/// Saved identities needed when importing stack frames after instance state.
#[derive(Debug)]
pub struct RestoredInstances {
    pub instances: BTreeMap<u64, InstanceId>,
    pub arrays: BTreeMap<u64, ArrayRef>,
}

impl Papyrus {
    /// Restore instance and reference variables after the host maps saved IDs.
    ///
    /// A mapping to None explicitly marks an unresolved reference as null.
    /// Every saved instance and reference header needs a live object mapping.
    /// No form bindings, threads, events or native calls are created here.
    ///
    /// # Errors
    ///
    /// Missing identities, invalid layouts, unsupported value types or an
    /// incompatible VM cause an error before any live VM state is replaced.
    pub fn restore_instances(
        &self,
        vm: &mut Vm,
        objects: &BTreeMap<u64, Option<ObjectId>>,
    ) -> Result<RestoredInstances> {
        for object in objects.values().flatten() {
            if vm.object(*object).is_none() {
                return Err(bad(format!("mapped object {} does not exist", object.0)));
            }
        }
        let layouts = Layouts::new(self)?;
        let values = Values::new(self, objects)?;
        let data: BTreeMap<_, _> = self
            .instance_data
            .iter()
            .chain(&self.reference_data)
            .map(|data| (data.id, data))
            .collect();
        if data.len() != self.instance_data.len() + self.reference_data.len() {
            return Err(bad("instance and reference IDs overlap"));
        }
        let mut saved_ids = Vec::new();
        let mut inputs = Vec::new();
        for (id, script) in self
            .instances
            .iter()
            .map(|instance| (instance.id, instance.script))
            .chain(
                self.references
                    .iter()
                    .map(|reference| (reference.id, reference.script)),
            )
        {
            let body = data
                .get(&id)
                .ok_or_else(|| bad(format!("missing variable data {id:#x}")))?;
            let object = values
                .object(id)?
                .ok_or_else(|| bad(format!("instance {id:#x} has no live object")))?;
            if vm.object(object).is_none() {
                return Err(bad(format!("mapped object {} does not exist", object.0)));
            }
            let script_name = text(self, script)?;
            let members = layouts.members(script_name)?;
            if members.len() != body.variables.len() {
                return Err(bad(format!(
                    "instance {id:#x} ({script_name}) has {} slots for {} members",
                    body.variables.len(),
                    members.len()
                )));
            }
            let mut variables = Vec::with_capacity(members.len());
            for ((owner, member), value) in members.iter().zip(&body.variables) {
                variables.push(SavedVariable {
                    declaring_script: text(self, owner.name)?.to_owned(),
                    name: text(self, member.name)?.to_owned(),
                    type_name: text(self, member.type_name)?.to_owned(),
                    value: values.value(value)?,
                });
            }
            saved_ids.push(id);
            inputs.push(InstanceState {
                object,
                script: script_name.to_owned(),
                state: text(self, body.state_or_type)?.to_owned(),
                variables,
            });
        }
        let ids = vm
            .restore_instances(&inputs)
            .map_err(|error| bad(error.to_string()))?;
        Ok(RestoredInstances {
            instances: saved_ids.into_iter().zip(ids).collect(),
            arrays: values.arrays,
        })
    }
}

fn text(papyrus: &Papyrus, reference: StringRef) -> Result<&str> {
    papyrus
        .string(reference)
        .ok_or_else(|| bad(format!("invalid string reference {}", reference.0)))
}

struct Layouts<'a> {
    papyrus: &'a Papyrus,
    scripts: BTreeMap<String, &'a ScriptDef>,
}

impl<'a> Layouts<'a> {
    fn new(papyrus: &'a Papyrus) -> Result<Self> {
        let mut scripts = BTreeMap::new();
        for script in &papyrus.scripts {
            let name = text(papyrus, script.name)?.to_ascii_lowercase();
            if scripts.insert(name.clone(), script).is_some() {
                return Err(bad(format!("duplicate saved script definition {name}")));
            }
        }
        Ok(Self { papyrus, scripts })
    }

    fn members(&self, name: &str) -> Result<Vec<(&'a ScriptDef, &'a MemberDesc)>> {
        let mut chain = Vec::new();
        let mut next = name.to_ascii_lowercase();
        let mut visited = BTreeSet::new();
        while !next.is_empty() {
            if !visited.insert(next.clone()) {
                return Err(bad(format!("cyclic saved inheritance at {next}")));
            }
            let script = self
                .scripts
                .get(&next)
                .ok_or_else(|| bad(format!("missing saved script definition {next}")))?;
            chain.push(*script);
            next = text(self.papyrus, script.parent)?.to_ascii_lowercase();
        }
        Ok(chain
            .into_iter()
            .rev()
            .flat_map(|script| script.members.iter().map(move |member| (script, member)))
            .collect())
    }
}

struct Values<'a> {
    papyrus: &'a Papyrus,
    objects: &'a BTreeMap<u64, Option<ObjectId>>,
    arrays: BTreeMap<u64, ArrayRef>,
}

impl<'a> Values<'a> {
    fn new(papyrus: &'a Papyrus, objects: &'a BTreeMap<u64, Option<ObjectId>>) -> Result<Self> {
        let mut arrays = BTreeMap::new();
        for header in &papyrus.arrays {
            if header.length as usize > soe_papyrus::MAX_ARRAY_LEN {
                return Err(bad("saved array exceeds the VM allocation limit"));
            }
            let kind = element_kind(header.element_type)?;
            if arrays
                .insert(header.id, ArrayRef::new(kind, header.length as usize))
                .is_some()
            {
                return Err(bad(format!("duplicate saved array {:#x}", header.id)));
            }
        }
        let values = Self {
            papyrus,
            objects,
            arrays,
        };
        for data in &papyrus.array_data {
            let array = values
                .arrays
                .get(&data.id)
                .ok_or_else(|| bad(format!("unknown array data {:#x}", data.id)))?;
            if array.len() != data.variables.len() {
                return Err(bad("array length differs from header"));
            }
            let mut items = Vec::with_capacity(array.len());
            for value in &data.variables {
                let value = values.value(value)?;
                if !matches!(
                    (array.kind, &value),
                    (
                        ElementKind::Object,
                        RuntimeValue::Object(_) | RuntimeValue::None
                    ) | (ElementKind::Str, RuntimeValue::Str(_))
                        | (ElementKind::Int, RuntimeValue::Int(_))
                        | (ElementKind::Float, RuntimeValue::Float(_))
                        | (ElementKind::Bool, RuntimeValue::Bool(_))
                ) {
                    return Err(bad(format!(
                        "array {:#x} contains a value of the wrong type",
                        data.id
                    )));
                }
                items.push(value);
            }
            *array.items.borrow_mut() = items;
        }
        Ok(values)
    }

    fn object(&self, id: u64) -> Result<Option<ObjectId>> {
        if id == 0 {
            return Ok(None);
        }
        self.objects
            .get(&id)
            .copied()
            .ok_or_else(|| bad(format!("saved object {id:#x} has no mapping")))
    }

    fn value(&self, value: &Value) -> Result<RuntimeValue> {
        Ok(match value {
            Value::Null(_) => RuntimeValue::None,
            Value::Reference { id, .. } => self
                .object(*id)?
                .map_or(RuntimeValue::None, RuntimeValue::Object),
            Value::String(reference) => RuntimeValue::string(text(self.papyrus, *reference)?),
            Value::Integer(value) => RuntimeValue::Int(*value),
            Value::Float(value) => RuntimeValue::Float(*value),
            Value::Boolean(value) => RuntimeValue::Bool(*value != 0),
            Value::Array {
                element_type, id, ..
            } => {
                if *id == 0 {
                    return Ok(RuntimeValue::None);
                }
                let array = self
                    .arrays
                    .get(id)
                    .ok_or_else(|| bad(format!("saved array {id:#x} has no header")))?;
                if array.kind != element_kind(*element_type)? {
                    return Err(bad("array value type differs from header"));
                }
                RuntimeValue::Array(array.clone())
            }
            Value::Variant(_) | Value::Struct { .. } => {
                return Err(bad("variant and struct runtime values are unsupported"))
            }
        })
    }
}

fn element_kind(kind: ValueType) -> Result<ElementKind> {
    match kind {
        ValueType::Reference => Ok(ElementKind::Object),
        ValueType::String => Ok(ElementKind::Str),
        ValueType::Integer => Ok(ElementKind::Int),
        ValueType::Float => Ok(ElementKind::Float),
        ValueType::Boolean => Ok(ElementKind::Bool),
        _ => Err(bad("unsupported runtime array element type")),
    }
}
