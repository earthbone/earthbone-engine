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

    pub(crate) fn evaluate(&self, variables: &Variables, properties: &Variables) -> Result<f32> {
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
                let value = expression.evaluate(variables, properties)?;
                match *op {
                    "!" => truth(value == 0.0),
                    "-" => -value,
                    _ => value,
                }
            }
            Self::Binary(op, left, right) => {
                let left = left.evaluate(variables, properties)?;
                if *op == "&&" && left == 0.0 {
                    return Ok(0.0);
                }
                if *op == "||" && left != 0.0 {
                    return Ok(1.0);
                }
                let right = right.evaluate(variables, properties)?;
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
        };
        if value.is_finite() {
            Ok(value)
        } else {
            Err(ExpressionError::InvalidValue)
        }
    }
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
            left = Expression::Binary(operator, Box::new(left), Box::new(right));
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

    fn evaluate(source: &str) -> Result<f32> {
        let variables = Variables::default();
        Expression::compile(source, &variables, &variables, None)?.evaluate(&variables, &variables)
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
            "", "(", "1 +", "1 2", "1 = 1", "1 & 1", "1e+", "2..0", "unknown", "sin(1)", "1 / 0",
            "1e99", "\u{00e9}",
        ] {
            assert!(evaluate(source).is_err(), "{source}");
        }
        assert_eq!(
            evaluate(&"!".repeat(300)).unwrap_err(),
            ExpressionError::TooComplex
        );
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
        assert_eq!(expression.evaluate(&variables, &properties).unwrap(), 1.0);
        variables.set_real(4, f32::NAN);
        assert_eq!(
            expression.evaluate(&variables, &properties).unwrap_err(),
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
        assert_eq!(expression.evaluate(&variables, &properties).unwrap(), 1.0);
    }
}
