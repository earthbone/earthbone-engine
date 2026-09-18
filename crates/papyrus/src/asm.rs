// SPDX-License-Identifier: GPL-3.0-or-later

//! A PEX writer and a small builder on top of it.
//!
//! Tests and tools build a [`Script`] model with the builder, serialize it with
//! [`write`], and load it back through [`crate::pex::parse`], so the reader is
//! exercised on real bytes rather than on a model handed to it directly. The
//! writer emits the Skyrim layout in either byte order.

use std::collections::HashMap;

use crate::pex::{
    ByteOrder, Function, Header, Instruction, Object, Opcode, Operand, Property, Script, State,
    TypedName, UserFlag, Variable, FUNCTION_GLOBAL, FUNCTION_NATIVE, MAGIC, PROPERTY_AUTOVAR,
    PROPERTY_READ, PROPERTY_WRITE,
};

struct Writer {
    out: Vec<u8>,
    order: ByteOrder,
    strings: Vec<String>,
    index: HashMap<String, u16>,
}

impl Writer {
    fn u8(&mut self, v: u8) {
        self.out.push(v);
    }
    fn u16(&mut self, v: u16) {
        match self.order {
            ByteOrder::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            ByteOrder::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    fn u32(&mut self, v: u32) {
        match self.order {
            ByteOrder::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            ByteOrder::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    fn u64(&mut self, v: u64) {
        match self.order {
            ByteOrder::Big => self.out.extend_from_slice(&v.to_be_bytes()),
            ByteOrder::Little => self.out.extend_from_slice(&v.to_le_bytes()),
        }
    }
    fn raw_string(&mut self, s: &str) {
        self.u16(s.len() as u16);
        self.out.extend_from_slice(s.as_bytes());
    }
    fn string(&mut self, s: &str) {
        let i = self.index[s];
        self.u16(i);
    }

    fn operand(&mut self, v: &Operand) {
        match v {
            Operand::None => self.u8(0),
            Operand::Identifier(s) => {
                self.u8(1);
                self.string(s);
            }
            Operand::Str(s) => {
                self.u8(2);
                self.string(s);
            }
            Operand::Int(i) => {
                self.u8(3);
                self.u32(*i as u32);
            }
            Operand::Float(f) => {
                self.u8(4);
                self.u32(f.to_bits());
            }
            Operand::Bool(b) => {
                self.u8(5);
                self.u8(*b as u8);
            }
        }
    }

    fn typed_names(&mut self, names: &[TypedName]) {
        self.u16(names.len() as u16);
        for n in names {
            self.string(&n.name);
            self.string(&n.type_name);
        }
    }

    fn function_body(&mut self, f: &Function) {
        self.string(&f.return_type);
        self.string(&f.docstring);
        self.u32(f.user_flags);
        self.u8(f.flags);
        self.typed_names(&f.params);
        self.typed_names(&f.locals);
        self.u16(f.code.len() as u16);
        for ins in &f.code {
            self.u8(ins.op as u8);
            let (fixed, var) = ins.op.arity();
            // An instruction can arrive with fewer operands than its opcode
            // takes, from a builder call that left one out or from a tool
            // assembling code. This has no error channel to report that
            // through, so the short operands are written as `None`, which is
            // an operand this format already has. Slicing to `fixed` without
            // the check panicked instead, which `fuzz-vm-run` found with a
            // single `CallStatic` and no arguments at all.
            let supplied = ins.args.len().min(fixed);
            for a in &ins.args[..supplied] {
                self.operand(a);
            }
            for _ in supplied..fixed {
                self.operand(&Operand::None);
            }
            if var {
                let rest = ins.args.get(fixed..).unwrap_or(&[]);
                self.operand(&Operand::Int(rest.len() as i32));
                for a in rest {
                    self.operand(a);
                }
            }
        }
    }

    fn object(&mut self, o: &Object) {
        self.string(&o.name);
        let size_at = self.out.len();
        self.u32(0);
        self.string(&o.parent);
        self.string(&o.docstring);
        self.u32(o.user_flags);
        self.string(&o.auto_state);
        self.u16(o.variables.len() as u16);
        for v in &o.variables {
            self.string(&v.name);
            self.string(&v.type_name);
            self.u32(v.user_flags);
            self.operand(&v.default);
        }
        self.u16(o.properties.len() as u16);
        for p in &o.properties {
            self.string(&p.name);
            self.string(&p.type_name);
            self.string(&p.docstring);
            self.u32(p.user_flags);
            self.u8(p.flags);
            if p.flags & PROPERTY_AUTOVAR != 0 {
                self.string(p.auto_var.as_deref().unwrap_or(""));
            } else {
                if let Some(g) = &p.getter {
                    self.function_body(g);
                }
                if let Some(s) = &p.setter {
                    self.function_body(s);
                }
            }
        }
        self.u16(o.states.len() as u16);
        for s in &o.states {
            self.string(&s.name);
            self.u16(s.functions.len() as u16);
            for f in &s.functions {
                self.string(&f.name);
                self.function_body(f);
            }
        }
        // The size field counts itself, per skymp's reader comment.
        let size = (self.out.len() - size_at) as u32;
        let bytes = match self.order {
            ByteOrder::Big => size.to_be_bytes(),
            ByteOrder::Little => size.to_le_bytes(),
        };
        self.out[size_at..size_at + 4].copy_from_slice(&bytes);
    }
}

fn collect_strings(script: &Script) -> Vec<String> {
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut out = Vec::new();
    let mut add = |s: &str| {
        if !seen.contains_key(s) {
            seen.insert(s.to_string(), ());
            out.push(s.to_string());
        }
    };
    let mut add_operand = |o: &Operand, add: &mut dyn FnMut(&str)| match o {
        Operand::Identifier(s) | Operand::Str(s) => add(s),
        _ => {}
    };
    type AddOperand<'a> = &'a mut dyn FnMut(&Operand, &mut dyn FnMut(&str));
    fn add_function(f: &Function, add: &mut dyn FnMut(&str), add_operand: AddOperand<'_>) {
        add(&f.name);
        add(&f.return_type);
        add(&f.docstring);
        for p in f.params.iter().chain(f.locals.iter()) {
            add(&p.name);
            add(&p.type_name);
        }
        for ins in &f.code {
            for a in &ins.args {
                add_operand(a, add);
            }
        }
    }
    for flag in &script.user_flags {
        add(&flag.name);
    }
    for o in &script.objects {
        add(&o.name);
        add(&o.parent);
        add(&o.docstring);
        add(&o.auto_state);
        for v in &o.variables {
            add(&v.name);
            add(&v.type_name);
            add_operand(&v.default, &mut add);
        }
        for p in &o.properties {
            add(&p.name);
            add(&p.type_name);
            add(&p.docstring);
            if let Some(a) = &p.auto_var {
                add(a);
            }
            add("get");
            add("set");
            for f in p.getter.iter().chain(p.setter.iter()) {
                add_function(f, &mut add, &mut add_operand);
            }
        }
        for s in &o.states {
            add(&s.name);
            for f in &s.functions {
                add_function(f, &mut add, &mut add_operand);
            }
        }
    }
    add("");
    out
}

/// Serialize a script. Line numbers are emitted as a debug block when any
/// function carries them.
pub fn write(script: &Script) -> Vec<u8> {
    let strings = collect_strings(script);
    let index = strings
        .iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i as u16))
        .collect();
    let mut w = Writer {
        out: Vec::new(),
        order: script.header.byte_order,
        strings,
        index,
    };
    w.u32(MAGIC);
    w.u8(script.header.major);
    w.u8(script.header.minor);
    w.u16(script.header.game_id);
    w.u64(script.header.compile_time);
    w.raw_string(&script.header.source);
    w.raw_string(&script.header.user);
    w.raw_string(&script.header.machine);

    w.u16(w.strings.len() as u16);
    let table = w.strings.clone();
    for s in &table {
        w.raw_string(s);
    }

    let mut debug: Vec<(String, String, String, u8, Vec<u16>)> = Vec::new();
    for o in &script.objects {
        for s in &o.states {
            for f in &s.functions {
                if !f.lines.is_empty() {
                    debug.push((
                        o.name.clone(),
                        s.name.clone(),
                        f.name.clone(),
                        0,
                        f.lines.clone(),
                    ));
                }
            }
        }
        for p in &o.properties {
            if let Some(g) = &p.getter {
                if !g.lines.is_empty() {
                    debug.push((
                        o.name.clone(),
                        String::new(),
                        p.name.clone(),
                        1,
                        g.lines.clone(),
                    ));
                }
            }
            if let Some(s) = &p.setter {
                if !s.lines.is_empty() {
                    debug.push((
                        o.name.clone(),
                        String::new(),
                        p.name.clone(),
                        2,
                        s.lines.clone(),
                    ));
                }
            }
        }
    }
    if debug.is_empty() {
        w.u8(0);
    } else {
        w.u8(1);
        w.u64(script.header.compile_time);
        w.u16(debug.len() as u16);
        for (obj, state, func, kind, lines) in &debug {
            w.string(obj);
            w.string(state);
            w.string(func);
            w.u8(*kind);
            w.u16(lines.len() as u16);
            for l in lines {
                w.u16(*l);
            }
        }
    }

    w.u16(script.user_flags.len() as u16);
    for f in &script.user_flags {
        w.string(&f.name);
        w.u8(f.bit);
    }

    w.u16(script.objects.len() as u16);
    for o in &script.objects {
        w.object(o);
    }
    w.out
}

/// Builds one script object with the defaults a Skyrim compile would use.
pub struct ScriptBuilder {
    object: Object,
    header: Header,
}

/// Builds one function. Instructions are added with [`FunctionBuilder::ins`].
pub struct FunctionBuilder {
    function: Function,
}

pub fn ident(name: &str) -> Operand {
    Operand::Identifier(name.to_string())
}
pub fn s(v: &str) -> Operand {
    Operand::Str(v.to_string())
}
pub fn i(v: i32) -> Operand {
    Operand::Int(v)
}
pub fn f(v: f32) -> Operand {
    Operand::Float(v)
}
pub fn b(v: bool) -> Operand {
    Operand::Bool(v)
}
pub fn none() -> Operand {
    Operand::None
}

impl FunctionBuilder {
    pub fn new(name: &str, return_type: &str) -> Self {
        FunctionBuilder {
            function: Function {
                name: name.to_string(),
                return_type: return_type.to_string(),
                docstring: String::new(),
                user_flags: 0,
                flags: 0,
                params: Vec::new(),
                locals: Vec::new(),
                code: Vec::new(),
                lines: Vec::new(),
            },
        }
    }
    pub fn global(mut self) -> Self {
        self.function.flags |= FUNCTION_GLOBAL;
        self
    }
    pub fn native(mut self) -> Self {
        self.function.flags |= FUNCTION_NATIVE;
        self
    }
    pub fn param(mut self, name: &str, type_name: &str) -> Self {
        self.function.params.push(TypedName {
            name: name.to_string(),
            type_name: type_name.to_string(),
        });
        self
    }
    pub fn local(mut self, name: &str, type_name: &str) -> Self {
        self.function.locals.push(TypedName {
            name: name.to_string(),
            type_name: type_name.to_string(),
        });
        self
    }
    /// Adds an instruction.
    ///
    /// An opcode that takes no variadic tail is given exactly its fixed
    /// operands: anything past them is dropped here rather than carried in the
    /// model, because the writer would not emit it either. Keeping it made the
    /// two disagree, since the string table is collected from the whole model
    /// while the code section only holds what was written, so a discarded
    /// `Str` operand left an entry behind that a round trip then removed.
    /// `fuzz-pex-grammar` found that as a reader and writer disagreement.
    pub fn ins(mut self, op: Opcode, args: Vec<Operand>) -> Self {
        let (fixed, var) = op.arity();
        let mut args = args;
        if !var {
            args.truncate(fixed);
        }
        self.function.code.push(Instruction { op, args });
        self
    }
    /// Attach a source line to every instruction added so far, in order.
    pub fn lines(mut self, lines: &[u16]) -> Self {
        self.function.lines = lines.to_vec();
        self
    }
    pub fn build(self) -> Function {
        self.function
    }
}

impl ScriptBuilder {
    pub fn new(name: &str, parent: &str) -> Self {
        ScriptBuilder {
            object: Object {
                name: name.to_string(),
                parent: parent.to_string(),
                docstring: String::new(),
                user_flags: 0,
                auto_state: String::new(),
                variables: Vec::new(),
                properties: Vec::new(),
                states: vec![State {
                    name: String::new(),
                    functions: Vec::new(),
                }],
            },
            header: Header {
                major: 3,
                minor: 2,
                game_id: 1,
                compile_time: 0,
                source: format!("{name}.psc"),
                user: "soe".into(),
                machine: "soe".into(),
                byte_order: ByteOrder::Big,
            },
        }
    }

    pub fn byte_order(mut self, order: ByteOrder) -> Self {
        self.header.byte_order = order;
        self
    }

    pub fn auto_state(mut self, name: &str) -> Self {
        self.object.auto_state = name.to_string();
        self
    }

    pub fn variable(mut self, name: &str, type_name: &str, default: Operand) -> Self {
        self.object.variables.push(Variable {
            name: name.to_string(),
            type_name: type_name.to_string(),
            user_flags: 0,
            default,
        });
        self
    }

    /// An auto property backed by `::Name_var`, with its variable declared.
    pub fn auto_property(mut self, name: &str, type_name: &str, default: Operand) -> Self {
        let var = format!("::{name}_var");
        self.object.variables.push(Variable {
            name: var.clone(),
            type_name: type_name.to_string(),
            user_flags: 0,
            default,
        });
        self.object.properties.push(Property {
            name: name.to_string(),
            type_name: type_name.to_string(),
            docstring: String::new(),
            user_flags: 0,
            flags: PROPERTY_READ | PROPERTY_WRITE | PROPERTY_AUTOVAR,
            auto_var: Some(var),
            getter: None,
            setter: None,
        });
        self
    }

    /// A property with explicit getter and setter functions.
    pub fn property(
        mut self,
        name: &str,
        type_name: &str,
        getter: Option<Function>,
        setter: Option<Function>,
    ) -> Self {
        let mut flags = 0;
        if getter.is_some() {
            flags |= PROPERTY_READ;
        }
        if setter.is_some() {
            flags |= PROPERTY_WRITE;
        }
        self.object.properties.push(Property {
            name: name.to_string(),
            type_name: type_name.to_string(),
            docstring: String::new(),
            user_flags: 0,
            flags,
            auto_var: None,
            getter,
            setter,
        });
        self
    }

    /// Add a function to the empty state.
    pub fn function(self, f: Function) -> Self {
        self.function_in("", f)
    }

    pub fn function_in(mut self, state: &str, f: Function) -> Self {
        if let Some(s) = self
            .object
            .states
            .iter_mut()
            .find(|s| s.name.eq_ignore_ascii_case(state))
        {
            s.functions.push(f);
        } else {
            self.object.states.push(State {
                name: state.to_string(),
                functions: vec![f],
            });
        }
        self
    }

    pub fn build(self) -> Script {
        Script {
            header: self.header,
            user_flags: vec![
                UserFlag {
                    name: "hidden".into(),
                    bit: 0,
                },
                UserFlag {
                    name: "conditional".into(),
                    bit: 1,
                },
            ],
            objects: vec![self.object],
        }
    }

    /// Build and serialize in one step.
    pub fn bytes(self) -> Vec<u8> {
        write(&self.build())
    }
}
