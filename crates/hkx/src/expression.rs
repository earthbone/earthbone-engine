// SPDX-License-Identifier: GPL-3.0-or-later

//! Scalar Havok condition expressions. Operator kinds and real-valued operands
//! follow serde-hkx's hkbCompiledExpressionSetToken class. The behavior editor
//! preserves source only, so parsing and evaluation are implemented here.

use crate::behavior::{ExpressionScope, VariableType};
use crate::interp::Variables;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExpressionError {
    Syntax(usize),
    UnknownSymbol(String),
    UnsupportedType(String),
    TooComplex,
    InvalidValue,
}

type Result<T> = std::result::Result<T, ExpressionError>;

#[derive(Debug, Clone)]
pub(crate) enum Expression {
    Number(f32),
    Variable { index: usize, property: bool },
    Unary(&'static str, Box<Self>),
    Binary(&'static str, Box<Self>, Box<Self>),
    Function(Function, Vec<Self>),
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Function {
    Rand01,
    Sin,
    Cos,
    Asin,
    Acos,
    Sqrt,
    Abs,
    Ceil,
    Floor,
    SqrtInv,
    Pow,
    Max,
    Min,
    RandRange,
    Atan2Approx,
    Clamp,
    Mod,
    Deg2Rad,
    Rad2Deg,
    CosD,
    SinD,
    AcosD,
    AsinD,
    Atan2ApproxD,
    Sign,
    Lerp,
    Clerp,
    Cond,
}

impl Function {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "rand01" => Self::Rand01,
            "sin" => Self::Sin,
            "cos" => Self::Cos,
            "asin" => Self::Asin,
            "acos" => Self::Acos,
            "sqrt" => Self::Sqrt,
            "fabs" => Self::Abs,
            "ceil" => Self::Ceil,
            "floor" => Self::Floor,
            "sqrtinv" => Self::SqrtInv,
            "pow" => Self::Pow,
            "max" => Self::Max,
            "min" => Self::Min,
            "randrange" => Self::RandRange,
            "atan2approx" => Self::Atan2Approx,
            "clamp" => Self::Clamp,
            "mod" => Self::Mod,
            "deg2rad" => Self::Deg2Rad,
            "rad2deg" => Self::Rad2Deg,
            "cosd" => Self::CosD,
            "sind" => Self::SinD,
            "acosd" => Self::AcosD,
            "asind" => Self::AsinD,
            "atan2approxd" => Self::Atan2ApproxD,
            "sign" => Self::Sign,
            "lerp" => Self::Lerp,
            "clerp" => Self::Clerp,
            "cond" => Self::Cond,
            _ => return None,
        })
    }

    fn arity(self) -> usize {
        match self {
            Self::Rand01 => 0,
            Self::Sin
            | Self::Cos
            | Self::Asin
            | Self::Acos
            | Self::Sqrt
            | Self::Abs
            | Self::Ceil
            | Self::Floor
            | Self::SqrtInv
            | Self::Deg2Rad
            | Self::Rad2Deg
            | Self::CosD
            | Self::SinD
            | Self::AcosD
            | Self::AsinD
            | Self::Sign => 1,
            Self::Pow
            | Self::Max
            | Self::Min
            | Self::RandRange
            | Self::Atan2Approx
            | Self::Mod
            | Self::Atan2ApproxD => 2,
            Self::Clamp | Self::Lerp | Self::Clerp | Self::Cond => 3,
        }
    }
}

impl Expression {
    pub(crate) fn compile(
        source: &str,
        variables: &Variables,
        properties: &Variables,
        scope: Option<&ExpressionScope>,
    ) -> Result<Self> {
        let mut parser = Parser {
            source,
            offset: 0,
            count: 0,
            variables,
            properties,
            scope,
        };
        let expression = parser.expression(0)?;
        parser.whitespace();
        if parser.offset != source.len() {
            return Err(ExpressionError::Syntax(parser.offset));
        }
        Ok(expression)
    }

    pub(crate) fn evaluate(
        &self,
        variables: &Variables,
        properties: &Variables,
        random_state: &mut u64,
    ) -> Result<f32> {
        let value = match self {
            Self::Number(value) => *value,
            Self::Variable { index, property } => {
                let table = if *property { properties } else { variables };
                let word = table.word(*index).ok_or(ExpressionError::InvalidValue)?;
                match table.ty(*index) {
                    Some(VariableType::Bool) => f32::from(u8::from(word != 0)),
                    Some(VariableType::Int8) => f32::from(word as i8),
                    Some(VariableType::Int16) => f32::from(word as i16),
                    Some(VariableType::Int32) => word as f32,
                    Some(VariableType::Real) => f32::from_bits(word as u32),
                    _ => return Err(ExpressionError::InvalidValue),
                }
            }
            Self::Unary(op, expression) => {
                let value = expression.evaluate(variables, properties, random_state)?;
                match *op {
                    "!" => truth(value == 0.0),
                    "-" => -value,
                    _ => value,
                }
            }
            Self::Binary(op, left, right) => {
                let left = left.evaluate(variables, properties, random_state)?;
                if *op == "&&" && left == 0.0 {
                    return Ok(0.0);
                }
                if *op == "||" && left != 0.0 {
                    return Ok(1.0);
                }
                let right = right.evaluate(variables, properties, random_state)?;
                match *op {
                    "+" => left + right,
                    "-" => left - right,
                    "*" => left * right,
                    "/" => left / right,
                    "%" => left % right,
                    "==" => truth(left == right),
                    "!=" => truth(left != right),
                    "<" => truth(left < right),
                    "<=" => truth(left <= right),
                    ">" => truth(left > right),
                    ">=" => truth(left >= right),
                    "&&" | "||" => truth(right != 0.0),
                    _ => return Err(ExpressionError::InvalidValue),
                }
            }
            Self::Function(function, arguments) => {
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    values.push(argument.evaluate(variables, properties, random_state)?);
                }
                evaluate_function(*function, &values, random_state)?
            }
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(ExpressionError::InvalidValue)
        }
    }
}

fn evaluate_function(function: Function, values: &[f32], random_state: &mut u64) -> Result<f32> {
    if values.len() != function.arity() {
        return Err(ExpressionError::InvalidValue);
    }
    let first = values.first().copied().unwrap_or_default();
    let second = values.get(1).copied().unwrap_or_default();
    let third = values.get(2).copied().unwrap_or_default();
    let degrees = std::f32::consts::PI / 180.0;
    let value = match function {
        Function::Rand01 => next_random(random_state),
        Function::Sin => first.sin(),
        Function::Cos => first.cos(),
        Function::Asin => first.asin(),
        Function::Acos => first.acos(),
        Function::Sqrt => first.sqrt(),
        Function::Abs => first.abs(),
        Function::Ceil => first.ceil(),
        Function::Floor => first.floor(),
        Function::SqrtInv => first.sqrt().recip(),
        Function::Pow => first.powf(second),
        Function::Max => first.max(second),
        Function::Min => first.min(second),
        Function::RandRange => first + (second - first) * next_random(random_state),
        Function::Atan2Approx => first.atan2(second),
        Function::Clamp => {
            if second > third {
                return Err(ExpressionError::InvalidValue);
            }
            first.clamp(second, third)
        }
        Function::Mod => first % second,
        Function::Deg2Rad => first * degrees,
        Function::Rad2Deg => first / degrees,
        Function::CosD => (first * degrees).cos(),
        Function::SinD => (first * degrees).sin(),
        Function::AcosD => first.acos() / degrees,
        Function::AsinD => first.asin() / degrees,
        Function::Atan2ApproxD => first.atan2(second) / degrees,
        Function::Sign => {
            if first > 0.0 {
                1.0
            } else if first < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        Function::Lerp => first + (second - first) * third,
        Function::Clerp => {
            let delta = (second - first + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            first + delta * third
        }
        Function::Cond => {
            if first == 0.0 {
                third
            } else {
                second
            }
        }
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ExpressionError::InvalidValue)
    }
}

fn next_random(state: &mut u64) -> f32 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    let bits = state.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40;
    bits as f32 / (1_u32 << 24) as f32
}

fn truth(value: bool) -> f32 {
    f32::from(u8::from(value))
}

struct Parser<'a> {
    source: &'a str,
    offset: usize,
    count: usize,
    variables: &'a Variables,
    properties: &'a Variables,
    scope: Option<&'a ExpressionScope>,
}

impl Parser<'_> {
    fn whitespace(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.offset += 1;
        }
    }

    fn consume(&mut self, text: &str) -> bool {
        self.whitespace();
        if self.source[self.offset..].starts_with(text) {
            self.offset += text.len();
            true
        } else {
            false
        }
    }

    fn expression(&mut self, minimum: u8) -> Result<Expression> {
        self.count += 1;
        if self.count > 256 {
            return Err(ExpressionError::TooComplex);
        }
        let mut left = self.atom()?;
        loop {
            self.whitespace();
            let Some((operator, precedence)) = [
                ("||", 1),
                ("&&", 2),
                ("==", 3),
                ("!=", 3),
                ("<=", 4),
                (">=", 4),
                ("<", 4),
                (">", 4),
                ("+", 5),
                ("-", 5),
                ("*", 6),
                ("/", 6),
                ("%", 6),
            ]
            .into_iter()
            .find(|(op, _)| self.source[self.offset..].starts_with(op)) else {
                break;
            };
            if precedence < minimum {
                break;
            }
            self.offset += operator.len();
            let right = self.expression(precedence + 1)?;
            let comparison = matches!(operator, "==" | "!=" | "<" | "<=" | ">" | ">=");
            let compared_value = left.clone();
            let first = Expression::Binary(operator, Box::new(left), Box::new(right));
            let comma_offset = self.offset;
            if comparison && self.consume(",") {
                self.whitespace();
                let next_operator = [("<=", 4), (">=", 4), ("<", 4), (">", 4)]
                    .into_iter()
                    .find(|(op, _)| self.source[self.offset..].starts_with(op));
                if let Some((next_operator, next_precedence)) = next_operator {
                    self.offset += next_operator.len();
                    let next_right = self.expression(next_precedence + 1)?;
                    let second = Expression::Binary(
                        next_operator,
                        Box::new(compared_value),
                        Box::new(next_right),
                    );
                    left = Expression::Binary("&&", Box::new(first), Box::new(second));
                } else {
                    self.offset = comma_offset;
                    left = first;
                }
            } else {
                left = first;
            }
        }
        Ok(left)
    }

    fn atom(&mut self) -> Result<Expression> {
        for op in ["!", "-", "+"] {
            if self.consume(op) {
                return Ok(Expression::Unary(op, Box::new(self.expression(7)?)));
            }
        }
        if self.consume("(") {
            let value = self.expression(0)?;
            if !self.consume(")") {
                return Err(ExpressionError::Syntax(self.offset));
            }
            return Ok(value);
        }
        self.whitespace();
        let start = self.offset;
        let bytes = self.source.as_bytes();
        let first = bytes
            .get(start)
            .copied()
            .ok_or(ExpressionError::Syntax(start))?;
        if first.is_ascii_digit() || first == b'.' {
            while bytes
                .get(self.offset)
                .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
            {
                self.offset += 1;
            }
            if bytes
                .get(self.offset)
                .is_some_and(|b| matches!(b, b'e' | b'E'))
            {
                self.offset += 1;
                if bytes
                    .get(self.offset)
                    .is_some_and(|b| matches!(b, b'+' | b'-'))
                {
                    self.offset += 1;
                }
                while bytes.get(self.offset).is_some_and(u8::is_ascii_digit) {
                    self.offset += 1;
                }
            }
            let value: f32 = self.source[start..self.offset]
                .parse()
                .map_err(|_| ExpressionError::Syntax(start))?;
            return if value.is_finite() {
                Ok(Expression::Number(value))
            } else {
                Err(ExpressionError::InvalidValue)
            };
        }
        if !first.is_ascii_alphabetic() && first != b'_' {
            return Err(ExpressionError::Syntax(start));
        }
        while bytes
            .get(self.offset)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.offset += 1;
        }
        let name = &self.source[start..self.offset];
        match name {
            "true" => return Ok(Expression::Number(1.0)),
            "false" => return Ok(Expression::Number(0.0)),
            _ => {}
        }
        if self.consume("(") {
            let function = Function::from_name(name)
                .ok_or_else(|| ExpressionError::UnknownSymbol(name.to_string()))?;
            let mut arguments = Vec::new();
            if !self.consume(")") {
                loop {
                    arguments.push(self.expression(0)?);
                    if self.consume(")") {
                        break;
                    }
                    if !self.consume(",") {
                        return Err(ExpressionError::Syntax(self.offset));
                    }
                }
            }
            if arguments.len() != function.arity() {
                return Err(ExpressionError::Syntax(self.offset));
            }
            return Ok(Expression::Function(function, arguments));
        }
        let variable = self.variables.index(name).filter(|_| {
            self.scope
                .is_none_or(|scope| scope.variables.iter().any(|variable| variable == name))
        });
        let property = self.properties.index(name).filter(|_| {
            self.scope
                .is_none_or(|scope| scope.properties.iter().any(|property| property == name))
        });
        let (index, property, table) = if let Some(index) = variable {
            (index, false, self.variables)
        } else if let Some(index) = property {
            (index, true, self.properties)
        } else {
            return Err(ExpressionError::UnknownSymbol(name.to_string()));
        };
        if !matches!(
            table.ty(index),
            Some(
                VariableType::Bool
                    | VariableType::Int8
                    | VariableType::Int16
                    | VariableType::Int32
                    | VariableType::Real
            )
        ) {
            return Err(ExpressionError::UnsupportedType(name.to_string()));
        }
        Ok(Expression::Variable { index, property })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behavior::VariableInfo;

    const TROLL_BEHAVIOR: &str = "meshes/actors/troll/behaviors/trollbehavior.hkx";

    fn evaluate(source: &str) -> Result<f32> {
        let variables = Variables::default();
        let mut random_state = 0x4d59_5df4_d0f3_3173;
        Expression::compile(source, &variables, &variables, None)?.evaluate(
            &variables,
            &variables,
            &mut random_state,
        )
    }

    #[test]
    fn anm_50_troll_shipped_expression_matches_comma_comparison_trace() {
        let Some(data) = std::env::var_os("SKYRIM_DATA") else {
            eprintln!("SKYRIM_DATA not set, skipping");
            return;
        };
        let archive =
            soe_bsa::Archive::open(&std::path::Path::new(&data).join("Skyrim - Animations.bsa"))
                .expect("animation archive opens");
        let bytes = archive
            .read(TROLL_BEHAVIOR)
            .expect("troll behavior is in the animation archive");
        let packfile = crate::Packfile::parse(&bytes).expect("troll behavior parses");
        let graph = crate::behavior::BehaviorGraph::from_packfile(&packfile)
            .expect("troll behavior graph decodes");
        let source = graph
            .nodes
            .iter()
            .filter_map(|node| match &node.kind {
                crate::behavior::NodeKind::StateMachine(machine) => Some(
                    machine
                        .states
                        .iter()
                        .flat_map(|state| state.transitions.iter())
                        .chain(machine.wildcard_transitions.iter()),
                ),
                _ => None,
            })
            .flatten()
            .filter_map(|transition| transition.condition.as_ref())
            .filter_map(|condition| condition.expression().map(|(source, _)| source))
            .find(|source| *source == "(staggerDirection < .25) || (staggerDirection > .75) || (staggerMagnitude > .3 , < .7)")
            .expect("the shipped troll guard is present");
        let variables = Variables::from_graph_data(&graph.data);
        let properties = Variables::from_infos(
            &graph
                .data
                .character_property_names
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            &graph.data.character_property_infos,
        );
        let expression = Expression::compile(source, &variables, &properties, None)
            .expect("the shipped expression compiles");
        let magnitude = variables
            .index("staggerMagnitude")
            .expect("staggerMagnitude is declared");
        let direction = variables
            .index("staggerDirection")
            .expect("staggerDirection is declared");
        let mut values = variables;
        values.set_real(direction, 0.5);
        let mut random_state = 0x4d59_5df4_d0f3_3173;
        for (value, expected) in [
            (0.2, 0.0),
            (0.3, 0.0),
            (0.31, 1.0),
            (0.5, 1.0),
            (0.69, 1.0),
            (0.7, 0.0),
            (0.8, 0.0),
        ] {
            values.set_real(magnitude, value);
            assert_eq!(
                expression
                    .evaluate(&values, &properties, &mut random_state)
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn precedence_and_short_circuit_match_scalar_guards() {
        for source in [
            "1 + 2 * 3 == 7 && !false",
            "(1 + 2) * 3 == 9",
            "true || 1 / 0",
            "0 == (false && 1 / 0)",
            "-2 * -3 == +6",
            "1e-3 < .5",
            "5 % 2 == 1",
            "1 - 2 - 3 == -4",
        ] {
            assert_eq!(evaluate(source).unwrap(), 1.0, "{source}");
        }
    }

    #[test]
    fn malformed_or_unsupported_inputs_return_errors() {
        for source in [
            "",
            "(",
            "1 +",
            "1 2",
            "1 = 1",
            "1 & 1",
            "1e+",
            "2..0",
            "unknown",
            "sin()",
            "sin(1, 2)",
            "rand01(0)",
            "unknown(1)",
            "1 / 0",
            "1e99",
            "\u{00e9}",
        ] {
            assert!(evaluate(source).is_err(), "{source}");
        }
        assert_eq!(
            evaluate(&"!".repeat(300)).unwrap_err(),
            ExpressionError::TooComplex
        );
    }

    #[test]
    fn anm_49_authored_math_functions_match_scalar_results() {
        for (source, expected) in [
            ("sin(0)", 0.0),
            ("cos(0)", 1.0),
            ("asin(1)", std::f32::consts::FRAC_PI_2),
            ("acos(1)", 0.0),
            ("sqrt(9)", 3.0),
            ("fabs(-3)", 3.0),
            ("ceil(1.2)", 2.0),
            ("floor(1.8)", 1.0),
            ("sqrtinv(4)", 0.5),
            ("pow(2, 3)", 8.0),
            ("max(2, 3)", 3.0),
            ("min(2, 3)", 2.0),
            ("clamp(4, 0, 3)", 3.0),
            ("mod(5, 2)", 1.0),
            ("deg2rad(180)", std::f32::consts::PI),
            ("rad2deg(3.1415927)", 180.0),
            ("cosd(60)", 0.5),
            ("sind(30)", 0.5),
            ("acosd(1)", 0.0),
            ("asind(1)", 90.0),
            ("atan2approx(1, 1)", std::f32::consts::FRAC_PI_4),
            ("atan2approxd(1, 1)", 45.0),
            ("sign(-3)", -1.0),
            ("lerp(2, 6, .25)", 3.0),
            ("clerp(3, -3, .5)", std::f32::consts::PI),
            ("cond(1, 7, 9)", 7.0),
        ] {
            let actual = evaluate(source).unwrap();
            assert!((actual - expected).abs() < 0.0001, "{source}: {actual}");
        }
    }

    #[test]
    fn anm_49_random_functions_replay_from_a_fixed_seed() {
        for source in ["rand01()", "randrange(-2, 3)"] {
            let first = evaluate(source).unwrap();
            let replay = evaluate(source).unwrap();
            assert_eq!(first, replay, "{source}");
        }
    }

    #[test]
    fn narrow_integer_and_real_storage_use_declared_types() {
        let infos: Vec<_> = [
            VariableType::Bool,
            VariableType::Int8,
            VariableType::Int16,
            VariableType::Int32,
            VariableType::Real,
        ]
        .into_iter()
        .map(|ty| VariableInfo {
            ty,
            ..Default::default()
        })
        .collect();
        let mut variables =
            Variables::from_infos(&["enabled", "tiny", "small", "state", "Speed"], &infos);
        for (i, word) in [2, 255, 65535, 17, 0.25f32.to_bits() as i32]
            .into_iter()
            .enumerate()
        {
            variables.set_word(i, word);
        }
        let properties = Variables::default();
        let expression = Expression::compile(
            "enabled == 1 && tiny == -1 && small == -1 && state == 17 && Speed == .25",
            &variables,
            &properties,
            None,
        )
        .unwrap();
        let mut random_state = 0x4d59_5df4_d0f3_3173;
        assert_eq!(
            expression
                .evaluate(&variables, &properties, &mut random_state)
                .unwrap(),
            1.0
        );
        variables.set_real(4, f32::NAN);
        let mut random_state = 0x4d59_5df4_d0f3_3173;
        assert_eq!(
            expression
                .evaluate(&variables, &properties, &mut random_state)
                .unwrap_err(),
            ExpressionError::InvalidValue
        );
    }

    #[test]
    fn character_properties_resolve_by_name_without_becoming_graph_variables() {
        let properties = Variables::from_infos(
            &["scale"],
            &[VariableInfo {
                ty: VariableType::Real,
                ..Default::default()
            }],
        );
        let variables = Variables::default();
        let expression = Expression::compile("scale == 0", &variables, &properties, None).unwrap();
        let mut random_state = 0x4d59_5df4_d0f3_3173;
        assert_eq!(
            expression
                .evaluate(&variables, &properties, &mut random_state)
                .unwrap(),
            1.0
        );
    }
}
