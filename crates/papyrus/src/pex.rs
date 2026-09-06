// SPDX-License-Identifier: GPL-3.0-or-later

//! The compiled script file format and its reader.
//!
//! Every name in the file is an index into one string table, so the model
//! stores resolved strings and keeps the table only for the writer. Identifier
//! operands (locals, parameters, `self`, temporaries) stay as names and are
//! resolved by the VM at run time, the way the original engine does it.

use std::fmt;

pub const MAGIC: u32 = 0xFA57_C0DE;

/// The game the file was compiled for, from the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    /// Skyrim, Skyrim SE: format 3.1 and 3.2.
    Big,
    /// Fallout 4 and later. Read for the header only.
    Little,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PexError {
    Truncated { at: usize, wanted: usize },
    BadMagic(u32),
    UnsupportedVersion { major: u8, minor: u8 },
    BadStringIndex { index: u16, len: usize },
    BadValueType(u8),
    BadOpcode { code: u8, function: String },
    BadVarArgCount { function: String },
}

impl fmt::Display for PexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PexError::Truncated { at, wanted } => {
                write!(f, "file ends at byte {at}, wanted {wanted} more")
            }
            PexError::BadMagic(m) => write!(f, "not a PEX file (magic {m:#010x})"),
            PexError::UnsupportedVersion { major, minor } => {
                write!(f, "PEX format {major}.{minor} is not the Skyrim layout")
            }
            PexError::BadStringIndex { index, len } => {
                write!(f, "string index {index} outside a table of {len}")
            }
            PexError::BadValueType(t) => write!(f, "unknown value type {t}"),
            PexError::BadOpcode { code, function } => {
                write!(f, "unknown opcode {code:#04x} in {function}")
            }
            PexError::BadVarArgCount { function } => {
                write!(f, "variable argument count is not an integer in {function}")
            }
        }
    }
}

impl std::error::Error for PexError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub major: u8,
    pub minor: u8,
    pub game_id: u16,
    pub compile_time: u64,
    pub source: String,
    pub user: String,
    pub machine: String,
    pub byte_order: ByteOrder,
}

/// A literal operand or default value. `Identifier` only appears in
/// instructions.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    None,
    Identifier(String),
    Str(String),
    Int(i32),
    Float(f32),
    Bool(bool),
}

impl Operand {
    pub fn as_ident(&self) -> Option<&str> {
        match self {
            Operand::Identifier(s) => Some(s),
            _ => None,
        }
    }
}

/// Every opcode the format defines. The VM executes the Skyrim set and reports
/// the Fallout 4 ones as unsupported with the instruction index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Opcode {
    Nop = 0,
    IAdd,
    FAdd,
    ISub,
    FSub,
    IMul,
    FMul,
    IDiv,
    FDiv,
    IMod,
    Not,
    INeg,
    FNeg,
    Assign,
    Cast,
    CmpEq,
    CmpLt,
    CmpLte,
    CmpGt,
    CmpGte,
    Jmp,
    JmpT,
    JmpF,
    CallMethod,
    CallParent,
    CallStatic,
    Return,
    StrCat,
    PropGet,
    PropSet,
    ArrayCreate,
    ArrayLength,
    ArrayGetElement,
    ArraySetElement,
    ArrayFindElement,
    ArrayRFindElement,
    Is,
    StructCreate,
    StructGet,
    StructSet,
    ArrayFindStruct,
    ArrayRFindStruct,
    ArrayAdd,
    ArrayInsert,
    ArrayRemoveLast,
    ArrayRemove,
    ArrayClear,
    ArrayGetAllMatchingStructs,
}

impl Opcode {
    pub const COUNT: u8 = Opcode::ArrayGetAllMatchingStructs as u8 + 1;

    pub fn from_u8(code: u8) -> Option<Opcode> {
        if code >= Opcode::COUNT {
            return None;
        }
        // Safety of the transmute-free route: the enum is dense from 0.
        Some(TABLE[code as usize].0)
    }

    /// Fixed operand count, and whether a variable argument list follows.
    pub fn arity(self) -> (usize, bool) {
        let (_, n, var) = TABLE[self as u8 as usize];
        (n, var)
    }

    pub fn is_fallout4(self) -> bool {
        self as u8 >= Opcode::Is as u8
    }
}

const TABLE: [(Opcode, usize, bool); Opcode::COUNT as usize] = [
    (Opcode::Nop, 0, false),
    (Opcode::IAdd, 3, false),
    (Opcode::FAdd, 3, false),
    (Opcode::ISub, 3, false),
    (Opcode::FSub, 3, false),
    (Opcode::IMul, 3, false),
    (Opcode::FMul, 3, false),
    (Opcode::IDiv, 3, false),
    (Opcode::FDiv, 3, false),
    (Opcode::IMod, 3, false),
    (Opcode::Not, 2, false),
    (Opcode::INeg, 2, false),
    (Opcode::FNeg, 2, false),
    (Opcode::Assign, 2, false),
    (Opcode::Cast, 2, false),
    (Opcode::CmpEq, 3, false),
    (Opcode::CmpLt, 3, false),
    (Opcode::CmpLte, 3, false),
    (Opcode::CmpGt, 3, false),
    (Opcode::CmpGte, 3, false),
    (Opcode::Jmp, 1, false),
    (Opcode::JmpT, 2, false),
    (Opcode::JmpF, 2, false),
    (Opcode::CallMethod, 3, true),
    (Opcode::CallParent, 2, true),
    (Opcode::CallStatic, 3, true),
    (Opcode::Return, 1, false),
    (Opcode::StrCat, 3, false),
    (Opcode::PropGet, 3, false),
    (Opcode::PropSet, 3, false),
    (Opcode::ArrayCreate, 2, false),
    (Opcode::ArrayLength, 2, false),
    (Opcode::ArrayGetElement, 3, false),
    (Opcode::ArraySetElement, 3, false),
    (Opcode::ArrayFindElement, 4, false),
    (Opcode::ArrayRFindElement, 4, false),
    (Opcode::Is, 3, false),
    (Opcode::StructCreate, 1, false),
    (Opcode::StructGet, 3, false),
    (Opcode::StructSet, 3, false),
    (Opcode::ArrayFindStruct, 5, false),
    (Opcode::ArrayRFindStruct, 5, false),
    (Opcode::ArrayAdd, 3, false),
    (Opcode::ArrayInsert, 3, false),
    (Opcode::ArrayRemoveLast, 1, false),
    (Opcode::ArrayRemove, 3, false),
    (Opcode::ArrayClear, 1, false),
    (Opcode::ArrayGetAllMatchingStructs, 6, false),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Instruction {
    pub op: Opcode,
    /// Fixed operands followed by the variable arguments, if any.
    pub args: Vec<Operand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedName {
    pub name: String,
    pub type_name: String,
}

pub const FUNCTION_GLOBAL: u8 = 1;
pub const FUNCTION_NATIVE: u8 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub return_type: String,
    pub docstring: String,
    pub user_flags: u32,
    pub flags: u8,
    pub params: Vec<TypedName>,
    pub locals: Vec<TypedName>,
    pub code: Vec<Instruction>,
    /// Source line per instruction, from the debug block when present.
    pub lines: Vec<u16>,
}

impl Function {
    pub fn is_global(&self) -> bool {
        self.flags & FUNCTION_GLOBAL != 0
    }
    pub fn is_native(&self) -> bool {
        self.flags & FUNCTION_NATIVE != 0
    }
    pub fn line_of(&self, pc: usize) -> Option<u16> {
        self.lines.get(pc).copied()
    }
}

pub const PROPERTY_READ: u8 = 1;
pub const PROPERTY_WRITE: u8 = 2;
pub const PROPERTY_AUTOVAR: u8 = 4;

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub type_name: String,
    pub docstring: String,
    pub user_flags: u32,
    pub flags: u8,
    /// The backing variable of an auto property, `::Name_var` by convention.
    pub auto_var: Option<String>,
    pub getter: Option<Function>,
    pub setter: Option<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variable {
    pub name: String,
    pub type_name: String,
    pub user_flags: u32,
    pub default: Operand,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// Empty for the state-less function set.
    pub name: String,
    pub functions: Vec<Function>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub name: String,
    pub parent: String,
    pub docstring: String,
    pub user_flags: u32,
    pub auto_state: String,
    pub variables: Vec<Variable>,
    pub properties: Vec<Property>,
    pub states: Vec<State>,
}

impl Object {
    pub fn state(&self, name: &str) -> Option<&State> {
        self.states.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// A function by name in one state, or in the empty state.
    pub fn function_in(&self, state: &str, name: &str) -> Option<&Function> {
        self.state(state)?
            .functions
            .iter()
            .find(|f| f.name.eq_ignore_ascii_case(name))
    }

    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFlag {
    pub name: String,
    pub bit: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Script {
    pub header: Header,
    pub user_flags: Vec<UserFlag>,
    /// Skyrim compiles one object per file. The reader keeps them all.
    pub objects: Vec<Object>,
}

impl Script {
    /// The script's own object, the first one in the file.
    pub fn object(&self) -> &Object {
        &self.objects[0]
    }
    pub fn name(&self) -> &str {
        &self.objects[0].name
    }
}

/// One debug entry keyed to a function, kept only long enough to attach line
/// numbers to the parsed functions.
struct DebugFunction {
    object: String,
    state: String,
    function: String,
    kind: u8,
    lines: Vec<u16>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    order: ByteOrder,
    strings: Vec<String>,
}

impl<'a> Reader<'a> {
    fn need(&self, n: usize) -> Result<(), PexError> {
        if self.pos + n > self.data.len() {
            return Err(PexError::Truncated {
                at: self.pos,
                wanted: n,
            });
        }
        Ok(())
    }

    fn u8(&mut self) -> Result<u8, PexError> {
        self.need(1)?;
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn u16(&mut self) -> Result<u16, PexError> {
        self.need(2)?;
        let b = [self.data[self.pos], self.data[self.pos + 1]];
        self.pos += 2;
        Ok(match self.order {
            ByteOrder::Big => u16::from_be_bytes(b),
            ByteOrder::Little => u16::from_le_bytes(b),
        })
    }

    fn u32(&mut self) -> Result<u32, PexError> {
        self.need(4)?;
        let mut b = [0u8; 4];
        b.copy_from_slice(&self.data[self.pos..self.pos + 4]);
        self.pos += 4;
        Ok(match self.order {
            ByteOrder::Big => u32::from_be_bytes(b),
            ByteOrder::Little => u32::from_le_bytes(b),
        })
    }

    fn u64(&mut self) -> Result<u64, PexError> {
        self.need(8)?;
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.data[self.pos..self.pos + 8]);
        self.pos += 8;
        Ok(match self.order {
            ByteOrder::Big => u64::from_be_bytes(b),
            ByteOrder::Little => u64::from_le_bytes(b),
        })
    }

    fn f32(&mut self) -> Result<f32, PexError> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn raw_string(&mut self) -> Result<String, PexError> {
        let len = self.u16()? as usize;
        self.need(len)?;
        let s = String::from_utf8_lossy(&self.data[self.pos..self.pos + len]).into_owned();
        self.pos += len;
        Ok(s)
    }

    fn string(&mut self) -> Result<String, PexError> {
        let index = self.u16()?;
        self.strings
            .get(index as usize)
            .cloned()
            .ok_or(PexError::BadStringIndex {
                index,
                len: self.strings.len(),
            })
    }

    fn operand(&mut self) -> Result<Operand, PexError> {
        Ok(match self.u8()? {
            0 => Operand::None,
            1 => Operand::Identifier(self.string()?),
            2 => Operand::Str(self.string()?),
            3 => Operand::Int(self.u32()? as i32),
            4 => Operand::Float(self.f32()?),
            5 => Operand::Bool(self.u8()? != 0),
            t => return Err(PexError::BadValueType(t)),
        })
    }

    fn typed_names(&mut self) -> Result<Vec<TypedName>, PexError> {
        let n = self.u16()?;
        let mut out = Vec::with_capacity(n as usize);
        for _ in 0..n {
            out.push(TypedName {
                name: self.string()?,
                type_name: self.string()?,
            });
        }
        Ok(out)
    }

    fn function_body(&mut self, name: String) -> Result<Function, PexError> {
        let return_type = self.string()?;
        let docstring = self.string()?;
        let user_flags = self.u32()?;
        let flags = self.u8()?;
        let params = self.typed_names()?;
        let locals = self.typed_names()?;
        let count = self.u16()?;
        let mut code = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let byte = self.u8()?;
            let op = Opcode::from_u8(byte).ok_or_else(|| PexError::BadOpcode {
                code: byte,
                function: name.clone(),
            })?;
            let (fixed, var) = op.arity();
            let mut args = Vec::with_capacity(fixed);
            for _ in 0..fixed {
                args.push(self.operand()?);
            }
            if var {
                let n = match self.operand()? {
                    Operand::Int(n) if n >= 0 => n as usize,
                    _ => {
                        return Err(PexError::BadVarArgCount {
                            function: name.clone(),
                        })
                    }
                };
                for _ in 0..n {
                    args.push(self.operand()?);
                }
            }
            code.push(Instruction { op, args });
        }
        Ok(Function {
            name,
            return_type,
            docstring,
            user_flags,
            flags,
            params,
            locals,
            code,
            lines: Vec::new(),
        })
    }

    fn functions(&mut self) -> Result<Vec<Function>, PexError> {
        let n = self.u16()?;
        let mut out = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let name = self.string()?;
            out.push(self.function_body(name)?);
        }
        Ok(out)
    }

    fn object(&mut self) -> Result<Object, PexError> {
        let name = self.string()?;
        let _size = self.u32()?;
        let parent = self.string()?;
        let docstring = self.string()?;
        let user_flags = self.u32()?;
        let auto_state = self.string()?;

        let n = self.u16()?;
        let mut variables = Vec::with_capacity(n as usize);
        for _ in 0..n {
            variables.push(Variable {
                name: self.string()?,
                type_name: self.string()?,
                user_flags: self.u32()?,
                default: self.operand()?,
            });
        }

        let n = self.u16()?;
        let mut properties = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let name = self.string()?;
            let type_name = self.string()?;
            let docstring = self.string()?;
            let user_flags = self.u32()?;
            let flags = self.u8()?;
            let mut p = Property {
                name,
                type_name,
                docstring,
                user_flags,
                flags,
                auto_var: None,
                getter: None,
                setter: None,
            };
            if flags & PROPERTY_AUTOVAR != 0 {
                p.auto_var = Some(self.string()?);
            } else {
                if flags & PROPERTY_READ != 0 {
                    p.getter = Some(self.function_body("get".into())?);
                }
                if flags & PROPERTY_WRITE != 0 {
                    p.setter = Some(self.function_body("set".into())?);
                }
            }
            properties.push(p);
        }

        let n = self.u16()?;
        let mut states = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let name = self.string()?;
            let functions = self.functions()?;
            states.push(State { name, functions });
        }

        Ok(Object {
            name,
            parent,
            docstring,
            user_flags,
            auto_state,
            variables,
            properties,
            states,
        })
    }
}

/// Parse a PEX file from memory.
pub fn parse(data: &[u8]) -> Result<Script, PexError> {
    let mut r = Reader {
        data,
        pos: 0,
        order: ByteOrder::Little,
        strings: Vec::new(),
    };
    let magic = r.u32()?;
    if magic == MAGIC {
        r.order = ByteOrder::Little;
    } else if magic.swap_bytes() == MAGIC {
        r.order = ByteOrder::Big;
    } else {
        return Err(PexError::BadMagic(magic));
    }
    let major = r.u8()?;
    let minor = r.u8()?;
    if major != 3 || !(1..=2).contains(&minor) {
        return Err(PexError::UnsupportedVersion { major, minor });
    }
    let header = Header {
        major,
        minor,
        game_id: r.u16()?,
        compile_time: r.u64()?,
        source: r.raw_string()?,
        user: r.raw_string()?,
        machine: r.raw_string()?,
        byte_order: r.order,
    };

    let n = r.u16()?;
    let mut strings = Vec::with_capacity(n as usize);
    for _ in 0..n {
        strings.push(r.raw_string()?);
    }
    r.strings = strings;

    let mut debug = Vec::new();
    if r.u8()? != 0 {
        let _modified = r.u64()?;
        let n = r.u16()?;
        for _ in 0..n {
            let object = r.string()?;
            let state = r.string()?;
            let function = r.string()?;
            let kind = r.u8()?;
            let count = r.u16()?;
            let mut lines = Vec::with_capacity(count as usize);
            for _ in 0..count {
                lines.push(r.u16()?);
            }
            debug.push(DebugFunction {
                object,
                state,
                function,
                kind,
                lines,
            });
        }
    }

    let n = r.u16()?;
    let mut user_flags = Vec::with_capacity(n as usize);
    for _ in 0..n {
        user_flags.push(UserFlag {
            name: r.string()?,
            bit: r.u8()?,
        });
    }

    let n = r.u16()?;
    let mut objects = Vec::with_capacity(n as usize);
    for _ in 0..n {
        objects.push(r.object()?);
    }

    attach_lines(&mut objects, &debug);

    Ok(Script {
        header,
        user_flags,
        objects,
    })
}

fn attach_lines(objects: &mut [Object], debug: &[DebugFunction]) {
    for d in debug {
        let Some(obj) = objects
            .iter_mut()
            .find(|o| o.name.eq_ignore_ascii_case(&d.object))
        else {
            continue;
        };
        match d.kind {
            0 => {
                if let Some(st) = obj
                    .states
                    .iter_mut()
                    .find(|s| s.name.eq_ignore_ascii_case(&d.state))
                {
                    if let Some(f) = st
                        .functions
                        .iter_mut()
                        .find(|f| f.name.eq_ignore_ascii_case(&d.function))
                    {
                        f.lines = d.lines.clone();
                    }
                }
            }
            1 | 2 => {
                if let Some(p) = obj
                    .properties
                    .iter_mut()
                    .find(|p| p.name.eq_ignore_ascii_case(&d.function))
                {
                    let target = if d.kind == 1 {
                        &mut p.getter
                    } else {
                        &mut p.setter
                    };
                    if let Some(f) = target {
                        f.lines = d.lines.clone();
                    }
                }
            }
            _ => {}
        }
    }
}
