// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounded Skyrim PSC source compilation through the PEX assembler.
//!
//! Header and identifier rules follow the Open Papyrus language reference.
//! Declaration shapes and user flag bits follow Caprica's Skyrim parser and
//! the TESV Papyrus flag definition. Auto properties support scalar types.
//! Unsupported source returns an error.

use std::path::Path;

use thiserror::Error;

use crate::asm;
use crate::pex::{
    ByteOrder, Function, Header, Instruction, Object, Opcode, Operand, Property, Script, State,
    TypedName, UserFlag, Variable, FUNCTION_GLOBAL, FUNCTION_NATIVE, PROPERTY_AUTOVAR,
    PROPERTY_READ, PROPERTY_WRITE,
};

/// A PSC source error with its one-based source line.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompileError {
    /// The source name does not match the declared script.
    #[error("source filename must match ScriptName {script}")]
    Filename { script: String },
    /// The source has no valid script header.
    #[error("line {line}: expected ScriptName header")]
    Header { line: usize },
    /// A declaration or statement is outside the supported grammar.
    #[error("line {line}: unsupported or invalid Papyrus source: {text}")]
    Syntax { line: usize, text: String },
}

fn identifier(text: &str) -> bool {
    let mut chars = text.chars();
    matches!(chars.next(), Some('a'..='z' | 'A'..='Z' | '_'))
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && ![
            "as",
            "auto",
            "autoreadonly",
            "bool",
            "else",
            "elseif",
            "endevent",
            "endfunction",
            "endif",
            "endproperty",
            "endstate",
            "endwhile",
            "event",
            "extends",
            "false",
            "float",
            "function",
            "global",
            "if",
            "import",
            "int",
            "length",
            "native",
            "new",
            "none",
            "parent",
            "property",
            "return",
            "scriptname",
            "self",
            "state",
            "string",
            "true",
            "while",
        ]
        .iter()
        .any(|keyword| text.eq_ignore_ascii_case(keyword))
}

fn syntax(line: usize, text: &str) -> CompileError {
    CompileError::Syntax {
        line,
        text: text.to_string(),
    }
}

fn strip_line_comment(source: &str) -> &str {
    let mut in_string = false;
    let mut escaped = false;
    for (offset, character) in source.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                in_string = false;
            }
        } else if character == '"' {
            in_string = true;
        } else if character == ';' {
            return &source[..offset];
        }
    }
    source
}

fn literal(text: &str) -> Option<Operand> {
    if text.eq_ignore_ascii_case("none") {
        Some(Operand::None)
    } else if text.eq_ignore_ascii_case("true") {
        Some(Operand::Bool(true))
    } else if text.eq_ignore_ascii_case("false") {
        Some(Operand::Bool(false))
    } else if let Ok(value) = text.parse::<i32>() {
        Some(Operand::Int(value))
    } else if let Ok(value) = text.parse::<f32>() {
        Some(Operand::Float(value))
    } else if text.len() >= 2 && text.starts_with('"') && text.ends_with('"') {
        let mut decoded = String::new();
        let mut chars = text[1..text.len() - 1].chars();
        while let Some(character) = chars.next() {
            if character == '\\' {
                decoded.push(match chars.next()? {
                    'n' => '\n',
                    't' => '\t',
                    '\\' => '\\',
                    '"' => '"',
                    _ => return None,
                });
            } else if character == '"' {
                return None;
            } else {
                decoded.push(character);
            }
        }
        Some(Operand::Str(decoded))
    } else {
        None
    }
}

fn canonical_type(name: &str) -> &str {
    if name.eq_ignore_ascii_case("int") {
        "Int"
    } else if name.eq_ignore_ascii_case("float") {
        "Float"
    } else if name.eq_ignore_ascii_case("bool") {
        "Bool"
    } else if name.eq_ignore_ascii_case("string") {
        "String"
    } else {
        name
    }
}

fn type_name(name: &str) -> bool {
    identifier(name)
        || ["int", "float", "bool", "string"]
            .iter()
            .any(|primitive| name.eq_ignore_ascii_case(primitive))
}

fn function_header(line: usize, source: &str) -> Result<Function, CompileError> {
    let open = source.find('(').ok_or_else(|| syntax(line, source))?;
    let close = source.find(')').ok_or_else(|| syntax(line, source))?;
    if close < open || !source[open + 1..close].trim().is_empty() {
        return Err(syntax(line, source));
    }
    let prefix: Vec<&str> = source[..open].split_whitespace().collect();
    let (return_type, name) = match prefix.as_slice() {
        [keyword, name] if keyword.eq_ignore_ascii_case("function") => ("None", *name),
        [ty, keyword, name] if keyword.eq_ignore_ascii_case("function") && type_name(ty) => {
            (*ty, *name)
        }
        _ => return Err(syntax(line, source)),
    };
    if !identifier(name) {
        return Err(syntax(line, source));
    }
    let tail = source[close + 1..].trim();
    let flags = if tail.is_empty() {
        0
    } else if tail.eq_ignore_ascii_case("native") {
        FUNCTION_NATIVE
    } else if tail.eq_ignore_ascii_case("global") {
        FUNCTION_GLOBAL
    } else {
        return Err(syntax(line, source));
    };
    Ok(Function {
        name: name.to_string(),
        return_type: canonical_type(return_type).to_string(),
        docstring: String::new(),
        user_flags: 0,
        flags,
        params: Vec::new(),
        locals: Vec::new(),
        code: Vec::new(),
        lines: Vec::new(),
    })
}

fn generated_state_functions() -> Vec<Function> {
    let mut get = crate::asm::FunctionBuilder::new("GetState", "String")
        .ins(Opcode::Return, vec![Operand::Identifier("::State".into())])
        .build();
    get.docstring = "Function that returns the current state".into();
    let mut goto = Function {
        name: "GotoState".into(),
        return_type: "None".into(),
        docstring: "Function that switches this object to the specified state".into(),
        user_flags: 0,
        flags: 0,
        params: vec![TypedName {
            name: "newState".into(),
            type_name: "String".into(),
        }],
        locals: vec![TypedName {
            name: "::NoneVar".into(),
            type_name: "None".into(),
        }],
        code: Vec::new(),
        lines: Vec::new(),
    };
    let ident = |name: &str| Operand::Identifier(name.into());
    goto.code = vec![
        Instruction {
            op: Opcode::CallMethod,
            args: vec![ident("onEndState"), ident("self"), ident("::NoneVar")],
        },
        Instruction {
            op: Opcode::Assign,
            args: vec![ident("::State"), ident("newState")],
        },
        Instruction {
            op: Opcode::CallMethod,
            args: vec![ident("onBeginState"), ident("self"), ident("::NoneVar")],
        },
    ];
    vec![get, goto]
}

/// Compile a bounded Skyrim PSC source file into a Skyrim PEX file.
///
/// The supported body statements are literal `Return` and implicit return.
/// Other declarations and expressions produce [`CompileError::Syntax`].
///
/// # Errors
/// Returns an error for a mismatched filename, invalid header, or unsupported
/// declaration or statement.
pub fn compile(source_name: &str, source: &str) -> Result<Vec<u8>, CompileError> {
    let mut lines = source.lines().enumerate().filter_map(|(index, source)| {
        let source = strip_line_comment(source).trim();
        (!source.is_empty()).then_some((index + 1, source))
    });
    let (header_line, header) = lines.next().ok_or(CompileError::Header { line: 1 })?;
    let words: Vec<&str> = header.split_whitespace().collect();
    if words.len() < 2 || !words[0].eq_ignore_ascii_case("scriptname") || !identifier(words[1]) {
        return Err(CompileError::Header { line: header_line });
    }
    let name = words[1];
    let filename = Path::new(source_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let extension = Path::new(source_name).extension().and_then(|s| s.to_str());
    if !filename.eq_ignore_ascii_case(name)
        || !extension.is_some_and(|ext| ext.eq_ignore_ascii_case("psc"))
    {
        return Err(CompileError::Filename {
            script: name.to_string(),
        });
    }
    let mut parent = String::new();
    let mut flags = 0u32;
    let mut pos = 2;
    if words
        .get(pos)
        .is_some_and(|w| w.eq_ignore_ascii_case("extends"))
    {
        parent = words
            .get(pos + 1)
            .filter(|w| identifier(w))
            .ok_or_else(|| syntax(header_line, header))?
            .to_string();
        pos += 2;
    }
    for word in &words[pos..] {
        let bit = if word.eq_ignore_ascii_case("hidden") {
            0
        } else if word.eq_ignore_ascii_case("conditional") {
            1
        } else {
            return Err(syntax(header_line, header));
        };
        if flags & (1 << bit) != 0 {
            return Err(syntax(header_line, header));
        }
        flags |= 1 << bit;
    }

    let mut functions = generated_state_functions();
    let mut properties = Vec::new();
    let mut variables = Vec::new();
    let mut active: Option<Function> = None;
    for (line, source) in lines {
        if let Some(function) = active.as_mut() {
            if source.eq_ignore_ascii_case("endfunction") {
                if function
                    .code
                    .last()
                    .is_none_or(|ins| ins.op != Opcode::Return)
                {
                    function.code.push(Instruction {
                        op: Opcode::Return,
                        args: vec![Operand::None],
                    });
                }
                functions.push(active.take().ok_or_else(|| syntax(line, source))?);
            } else if source
                .get(..6)
                .is_some_and(|head| head.eq_ignore_ascii_case("return"))
                && source
                    .get(6..)
                    .is_some_and(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace))
            {
                let rest = source[6..].trim();
                let value = if rest.is_empty() {
                    Operand::None
                } else {
                    literal(rest).ok_or_else(|| syntax(line, source))?
                };
                function.code.push(Instruction {
                    op: Opcode::Return,
                    args: vec![value],
                });
            } else {
                return Err(syntax(line, source));
            }
        } else {
            let words: Vec<&str> = source.split_whitespace().collect();
            let auto_property = match words.as_slice() {
                [ty, keyword, name, auto]
                    if keyword.eq_ignore_ascii_case("property")
                        && auto.eq_ignore_ascii_case("auto")
                        && type_name(ty)
                        && identifier(name) =>
                {
                    Some((*ty, *name))
                }
                _ => None,
            };
            if let Some((ty, name)) = auto_property {
                if properties
                    .iter()
                    .any(|property: &Property| property.name.eq_ignore_ascii_case(name))
                {
                    return Err(syntax(line, source));
                }
                let type_name = canonical_type(ty).to_string();
                let auto_var = format!("::{name}_var");
                variables.push(Variable {
                    name: auto_var.clone(),
                    type_name: type_name.clone(),
                    user_flags: 0,
                    default: match type_name.as_str() {
                        "Int" => Operand::Int(0),
                        "Float" => Operand::Float(0.0),
                        "Bool" => Operand::Bool(false),
                        "String" => Operand::Str(String::new()),
                        _ => Operand::None,
                    },
                });
                properties.push(Property {
                    name: name.to_string(),
                    type_name,
                    docstring: String::new(),
                    user_flags: 0,
                    flags: PROPERTY_READ | PROPERTY_WRITE | PROPERTY_AUTOVAR,
                    auto_var: Some(auto_var),
                    getter: None,
                    setter: None,
                });
                continue;
            }
            let function = function_header(line, source)?;
            if function.is_native() {
                functions.push(function);
            } else {
                active = Some(function);
            }
        }
    }
    if active.is_some() {
        return Err(syntax(source.lines().count(), "missing EndFunction"));
    }
    let user_flags = [("hidden", 0u8), ("conditional", 1u8)]
        .into_iter()
        .map(|(name, bit)| UserFlag {
            name: name.to_string(),
            bit,
        })
        .collect();
    let script = Script {
        header: Header {
            major: 3,
            minor: 2,
            game_id: 1,
            compile_time: 0,
            source: source_name.to_string(),
            user: String::new(),
            machine: String::new(),
            byte_order: ByteOrder::Big,
        },
        user_flags,
        objects: vec![Object {
            name: name.to_string(),
            parent,
            docstring: String::new(),
            user_flags: flags,
            auto_state: String::new(),
            variables,
            properties,
            states: vec![State {
                name: String::new(),
                functions,
            }],
        }],
    };
    Ok(asm::write(&script))
}
