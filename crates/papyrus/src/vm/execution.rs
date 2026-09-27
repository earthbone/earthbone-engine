// SPDX-License-Identifier: GPL-3.0-or-later

//! VM execution, extracted from the interpreter described in docs/papyrus.md.
//! The public API remains on Vm and uses the same single scheduler.

use super::{
    ArrayRef, ElementKind, Opcode, Operand, ThreadState, Value, Vm, MAX_ARRAY_LEN, MAX_STRING_LEN,
};

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
            let ins = &function.code[frame.pc];
            self.threads[t].frames.last_mut().unwrap().pc += 1;
            n += 1;
            self.exec(t, ins.op, &ins.args);
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
            Is
            | StructCreate
            | StructGet
            | StructSet
            | ArrayFindStruct
            | ArrayRFindStruct
            | ArrayGetAllMatchingStructs => {
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
