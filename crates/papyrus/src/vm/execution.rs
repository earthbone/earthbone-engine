// SPDX-License-Identifier: GPL-3.0-or-later

//! VM execution, extracted from the interpreter described in docs/papyrus.md.
//! The public API remains on Vm and uses the same single scheduler.

use super::{
    operand_to_value, ArrayRef, ElementKind, Opcode, Operand, StructRef, ThreadState, Value, Vm,
    MAX_ARRAY_LEN, MAX_STRING_LEN,
};
use std::collections::BTreeMap;
use std::time::Instant;

impl Vm {
    pub(super) fn step_thread(&mut self, t: usize, slice: usize) -> usize {
        let mut n = 0;
        while n < slice && self.threads[t].state == ThreadState::Runnable {
            let Some(frame) = self.threads[t].frames.last_mut() else {
                self.threads[t].state = ThreadState::Done;
                break;
            };
            if let Some(coverage) = &mut self.coverage {
                coverage.record(frame, &self.scripts);
            }
            if frame.pc >= frame.function.code.len() {
                // Falling off the end returns None.
                self.return_from(t, Value::None);
                n += 1;
                continue;
            }
            let function = frame.function.clone();
            let profile_script = frame.script.as_ref().map(|script| script.name.clone());
            let health_object = frame.self_object;
            let profile_name = function.name.clone();
            let profile_first_call = !frame.profile_recorded;
            let pc = frame.pc;
            frame.profile_recorded = true;
            let ins = &function.code[pc];
            self.threads[t].frames.last_mut().unwrap().pc += 1;
            n += 1;
            soe_trace::count(soe_trace::Operation::PapyrusInstructions, 1);
            let started = Instant::now();
            self.exec(t, ins.op, &ins.args);
            let elapsed_ns = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
            if let Some(script) = profile_script {
                self.record_running_health(health_object, &script, elapsed_ns);
                let thread_id = self.threads[t].id;
                let mut profiler = self.profiling.borrow_mut();
                if let Some((scope, _)) = &profiler.active {
                    let matches = match scope {
                        super::ProfileScope::Script(target) => target.eq_ignore_ascii_case(&script),
                        super::ProfileScope::Stack(target) => *target == thread_id,
                    };
                    if matches {
                        let key = format!("{script}.{profile_name}");
                        let entry = profiler.functions.entry(key).or_default();
                        entry.0 += u64::from(profile_first_call);
                        entry.1 += elapsed_ns;
                    }
                }
            }
        }
        n
    }

    pub(super) fn exec(&mut self, t: usize, op: Opcode, args: &[Operand]) {
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
        // A function, class or property name is the operand's own text. The
        // compiler writes it as an identifier operand, and reading it as a
        // variable would resolve to None.
        let name_at = |i: usize| -> String {
            match args.get(i) {
                Some(Operand::Identifier(s) | Operand::Str(s)) => s.clone(),
                _ => vals.get(i).map(Value::to_string).unwrap_or_default(),
            }
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
                let name = name_at(0);
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
                    Some(obj) => self.call_method(t, obj, &name, &call_args, dest, None),
                }
            }
            CallParent => {
                let name = name_at(0);
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
                        self.call_method(t, obj, &name, &call_args, dest, Some(script));
                    }
                    _ => {
                        self.err(t, &format!("parent call to {name}() outside an object"));
                        set(self, 1, Value::None);
                    }
                }
            }
            CallStatic => {
                let class = name_at(0);
                let name = name_at(1);
                let dest = dest_name(2);
                let call_args = vals[3..].to_vec();
                self.call_static(t, &class, &name, call_args, dest);
            }
            Return => self.return_from(t, vals[0].clone()),
            StrCat => {
                let (left, right) = (self.describe(&vals[1]), self.describe(&vals[2]));
                // Checked before building, because building it is the failure.
                let len = left.len().saturating_add(right.len());
                if len > MAX_STRING_LEN {
                    self.err(
                        t,
                        &format!("string length {len} is over the limit of {MAX_STRING_LEN}"),
                    );
                } else {
                    let joined = format!("{left}{right}");
                    set(self, 0, Value::string(&joined));
                }
            }
            PropGet => {
                let prop = name_at(0);
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
                let prop = name_at(0);
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
                } else if len as usize > MAX_ARRAY_LEN {
                    self.err(
                        t,
                        &format!("array size {len} is over the limit of {MAX_ARRAY_LEN}"),
                    );
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
            ArrayAdd => match vals[0].as_array() {
                None => self.err(t, "cannot add to a None array"),
                Some(array) => {
                    let count = vals[2].as_int();
                    let mut items = array.items.borrow_mut();
                    if count > 0 {
                        let count = count as usize;
                        if count > MAX_ARRAY_LEN.saturating_sub(items.len()) {
                            self.err(t, "array length limit exceeded by Add");
                        } else {
                            let value = self.cast_to_kind(&vals[1], array.kind);
                            let new_len = items.len() + count;
                            items.resize(new_len, value);
                        }
                    }
                }
            },
            ArrayInsert => match vals[0].as_array() {
                None => self.err(t, "cannot insert into a None array"),
                Some(array) => {
                    let index = vals[2].as_int();
                    let mut items = array.items.borrow_mut();
                    match usize::try_from(index).ok().filter(|i| *i <= items.len()) {
                        Some(index) if items.len() < MAX_ARRAY_LEN => {
                            let value = self.cast_to_kind(&vals[1], array.kind);
                            items.insert(index, value);
                        }
                        Some(_) => self.err(t, "array length limit exceeded by Insert"),
                        None => self.err(
                            t,
                            &format!(
                                "array insert index {index} out of range, length {}",
                                items.len()
                            ),
                        ),
                    }
                }
            },
            ArrayRemoveLast => match vals[0].as_array() {
                None => self.err(t, "cannot remove from a None array"),
                Some(array) => {
                    array.items.borrow_mut().pop();
                }
            },
            ArrayRemove => match vals[0].as_array() {
                None => self.err(t, "cannot remove from a None array"),
                Some(array) => {
                    let index = vals[1].as_int();
                    let count = vals[2].as_int();
                    let mut items = array.items.borrow_mut();
                    if count > 0 {
                        match usize::try_from(index).ok().filter(|i| *i < items.len()) {
                            Some(index) => {
                                let end = index.saturating_add(count as usize).min(items.len());
                                items.drain(index..end);
                            }
                            None => self.err(
                                t,
                                &format!(
                                    "array remove index {index} out of range, length {}",
                                    items.len()
                                ),
                            ),
                        }
                    }
                }
            },
            ArrayClear => match vals[0].as_array() {
                None => self.err(t, "cannot clear a None array"),
                Some(array) => array.items.borrow_mut().clear(),
            },
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
            Is => {
                let supported = self.threads[t]
                    .frames
                    .last()
                    .and_then(|f| f.script.as_ref())
                    .is_some_and(|s| s.game_id == 2 && s.major == 3 && s.minor >= 9);
                if supported {
                    let target = match args.get(2) {
                        Some(Operand::Identifier(name)) if !name.is_empty() => Some(name.as_str()),
                        _ => None,
                    };
                    let destination_type = {
                        let frame = self.threads[t].frames.last().unwrap();
                        dest_name(0).and_then(|dest| Self::type_of(&self.instances, frame, &dest))
                    };
                    let Some(target) = target.filter(|_| {
                        destination_type
                            .as_deref()
                            .is_some_and(|name| name.eq_ignore_ascii_case("Bool"))
                    }) else {
                        self.err(
                            t,
                            "Is requires a Bool destination and a nonempty identifier type operand",
                        );
                        set(self, 0, Value::Bool(false));
                        return;
                    };
                    let matches = match vals[1].as_object() {
                        None if vals[1].is_none() => false,
                        None => {
                            self.err(t, "Is operand must be an object or None");
                            false
                        }
                        Some(object) => self.object_is(object, target),
                    };
                    set(self, 0, Value::Bool(matches));
                } else {
                    self.err(t, "Is requires a Fallout 4 3.9 or newer script");
                    set(self, 0, Value::Bool(false));
                }
            }
            StructCreate => {
                let (supported, script, dest) = {
                    let frame = self.threads[t].frames.last().unwrap();
                    (
                        frame
                            .script
                            .as_ref()
                            .is_some_and(|s| s.game_id == 2 && s.major == 3 && s.minor >= 9),
                        frame.script.clone(),
                        dest_name(0),
                    )
                };
                if !supported {
                    self.err(t, "struct_create requires a Fallout 4 3.9 or newer script");
                    set(self, 0, Value::None);
                } else if let (Some(script), Some(dest)) = (script, dest) {
                    let struct_name = {
                        let frame = self.threads[t].frames.last().unwrap();
                        Self::type_of(&self.instances, frame, &dest)
                    };
                    let definition = struct_name.as_deref().and_then(|name| {
                        script.object.structs.iter().find(|s| {
                            s.name.eq_ignore_ascii_case(name)
                                || format!("{}.{}", script.name, s.name).eq_ignore_ascii_case(name)
                        })
                    });
                    if let Some(definition) = definition {
                        let fields = definition
                            .members
                            .iter()
                            .map(|member| {
                                (
                                    member.name.to_ascii_lowercase(),
                                    Self::cast_to_type(
                                        &operand_to_value(&member.default, Some(&member.type_name)),
                                        &member.type_name,
                                    ),
                                )
                            })
                            .collect::<BTreeMap<_, _>>();
                        set(
                            self,
                            0,
                            Value::Struct(StructRef::new(definition.name.clone(), fields)),
                        );
                    } else {
                        self.err(t, "struct_create destination has no declared struct type");
                        set(self, 0, Value::None);
                    }
                } else {
                    self.err(t, "struct_create destination must be an identifier");
                }
            }
            StructGet => {
                let supported = self.threads[t]
                    .frames
                    .last()
                    .and_then(|f| f.script.as_ref())
                    .is_some_and(|s| s.game_id == 2 && s.major == 3 && s.minor >= 9);
                let field = match args.get(2) {
                    Some(Operand::Str(field)) if !field.is_empty() => field.to_ascii_lowercase(),
                    _ => {
                        self.err(t, "struct_get member operand must be a nonempty string");
                        set(self, 0, Value::None);
                        return;
                    }
                };
                if !supported {
                    self.err(t, "struct_get requires a Fallout 4 3.9 or newer script");
                    set(self, 0, Value::None);
                } else if let Value::Struct(s) = &vals[1] {
                    let member =
                        self.threads[t]
                            .frames
                            .last()
                            .and_then(|f| f.script.as_ref())
                            .and_then(|script| {
                                script.object.structs.iter().find(|definition| {
                                    definition.name.eq_ignore_ascii_case(&s.name)
                                })
                            })
                            .and_then(|definition| {
                                definition
                                    .members
                                    .iter()
                                    .find(|member| member.name.eq_ignore_ascii_case(&field))
                            });
                    if let Some(member) = member {
                        let destination_type = {
                            let frame = self.threads[t].frames.last().unwrap();
                            dest_name(0)
                                .and_then(|dest| Self::type_of(&self.instances, frame, &dest))
                        };
                        if !destination_type
                            .as_deref()
                            .is_some_and(|target| target.eq_ignore_ascii_case(&member.type_name))
                        {
                            self.err(
                                t,
                                &format!("struct member {field} type does not match destination"),
                            );
                            set(self, 0, Value::None);
                        } else if let Some(value) = s.fields.borrow().get(&field).cloned() {
                            set(self, 0, value);
                        } else {
                            self.err(t, &format!("struct {} has no member {field}", s.name));
                            set(self, 0, Value::None);
                        }
                    } else {
                        self.err(t, &format!("struct {} has no member {field}", s.name));
                        set(self, 0, Value::None);
                    }
                } else {
                    self.err(t, "struct_get operand must be a struct");
                    set(self, 0, Value::None);
                }
            }
            StructSet => {
                let supported = self.threads[t]
                    .frames
                    .last()
                    .and_then(|f| f.script.as_ref())
                    .is_some_and(|s| s.game_id == 2 && s.major == 3 && s.minor >= 9);
                let field = match args.get(1) {
                    Some(Operand::Str(field)) if !field.is_empty() => field.to_ascii_lowercase(),
                    _ => {
                        self.err(t, "struct_set member operand must be a nonempty string");
                        return;
                    }
                };
                if !supported {
                    self.err(t, "struct_set requires a Fallout 4 3.9 or newer script");
                } else if let Value::Struct(s) = &vals[0] {
                    let member =
                        self.threads[t]
                            .frames
                            .last()
                            .and_then(|f| f.script.as_ref())
                            .and_then(|script| {
                                script.object.structs.iter().find(|definition| {
                                    definition.name.eq_ignore_ascii_case(&s.name)
                                })
                            })
                            .and_then(|definition| {
                                definition
                                    .members
                                    .iter()
                                    .find(|member| member.name.eq_ignore_ascii_case(&field))
                            });
                    let Some(member) = member else {
                        self.err(t, &format!("struct {} has no member {field}", s.name));
                        return;
                    };
                    if member.is_const {
                        self.err(t, &format!("struct member {field} is const"));
                        return;
                    }
                    if !Self::matches_declared_type(&vals[2], &member.type_name) {
                        self.err(
                            t,
                            &format!(
                                "value of type {} does not match struct member {} type {}",
                                vals[2].type_name(),
                                field,
                                member.type_name
                            ),
                        );
                        return;
                    }
                    let mut fields = s.fields.borrow_mut();
                    if let Some(slot) = fields.get_mut(&field) {
                        *slot = vals[2].clone();
                    } else {
                        self.err(t, &format!("struct {} has no member {field}", s.name));
                    }
                } else {
                    self.err(t, "struct_set operand must be a struct");
                }
            }
            ArrayFindStruct | ArrayRFindStruct | ArrayGetAllMatchingStructs => {
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

    fn object_is(&self, object: super::ObjectId, target: &str) -> bool {
        self.class_names(object)
            .iter()
            .any(|class| class.eq_ignore_ascii_case(target))
    }

    fn cast_to_type(value: &Value, target: &str) -> Value {
        match target.to_ascii_lowercase().as_str() {
            "int" => Value::Int(value.as_int()),
            "float" => Value::Float(value.as_float()),
            "bool" => Value::Bool(value.as_bool()),
            "string" => Value::string(&value.to_string()),
            _ if target.ends_with("[]") => value.clone(),
            _ => value.clone(),
        }
    }

    fn matches_declared_type(value: &Value, target: &str) -> bool {
        match target.to_ascii_lowercase().as_str() {
            "int" => matches!(value, Value::Int(_)),
            "float" => matches!(value, Value::Float(_)),
            "bool" => matches!(value, Value::Bool(_)),
            "string" => matches!(value, Value::Str(_)),
            name if name.ends_with("[]") => matches!(value, Value::None | Value::Array(_)),
            _ if matches!(value, Value::None) => true,
            name if name
                .rsplit(['.', '#'])
                .next()
                .is_some_and(|expected| {
                    matches!(value, Value::Struct(value) if value.name.eq_ignore_ascii_case(expected))
                }) => true,
            _ => matches!(value, Value::None | Value::Object(_)),
        }
    }

    pub(super) fn jump(&mut self, t: usize, offset: i32) {
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

    pub(super) fn cast(&mut self, v: &Value, target: Option<&str>) -> Value {
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

    pub(super) fn cast_to_kind(&self, v: &Value, kind: ElementKind) -> Value {
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
}
