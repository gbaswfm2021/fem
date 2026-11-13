pub use fullmeta_core::quantity::QuantityValue as Scalar;
use fullmeta_model::runtime_expressions as runtime;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
enum Node {
    Number(f64),
    Symbol(String),
    Unary(char, Box<Node>),
    Binary(char, Box<Node>, Box<Node>),
    Call(String, Vec<Node>),
}

#[derive(Clone, Debug)]
enum Token {
    Number(f64),
    Name(String),
    Symbol(char),
    End,
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
}

fn tokenize(source: &str) -> Result<Vec<Token>> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() || c == '.' {
            let start = i;
            while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'.' {
                i += 1;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
            }
            if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
                let exponent = i;
                i += 1;
                if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                    i += 1;
                }
                let digits = i;
                while i < bytes.len() && (bytes[i] as char).is_ascii_digit() {
                    i += 1;
                }
                if digits == i {
                    i = exponent;
                }
            }
            let raw = &source[start..i];
            let value = raw
                .parse::<f64>()
                .map_err(|_| format!("Invalid number “{raw}” in expression."))?;
            if !value.is_finite() {
                return Err("Expression contains a non-finite number.".to_string());
            }
            tokens.push(Token::Number(value));
            continue;
        }
        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            i += 1;
            while i < bytes.len()
                && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            tokens.push(Token::Name(source[start..i].to_string()));
            continue;
        }
        if "+-*/^(),".contains(c) {
            tokens.push(Token::Symbol(c));
            i += 1;
            continue;
        }
        return Err(format!("Unsupported character “{c}” in expression."));
    }
    tokens.push(Token::End);
    Ok(tokens)
}

impl Parser {
    fn peek(&self) -> &Token {
        &self.tokens[self.at]
    }
    fn take_symbol(&mut self, expected: char) -> Result<()> {
        if matches!(self.peek(), Token::Symbol(c) if *c == expected) {
            self.at += 1;
            Ok(())
        } else {
            Err(format!("Expected “{expected}” in expression."))
        }
    }
    fn expression(&mut self, minimum: u8) -> Result<Node> {
        let mut left = match self.peek().clone() {
            Token::Symbol(op @ ('+' | '-')) => {
                self.at += 1;
                Node::Unary(op, Box::new(self.expression(3)?))
            }
            Token::Number(value) => {
                self.at += 1;
                Node::Number(value)
            }
            Token::Symbol('(') => {
                self.at += 1;
                let node = self.expression(0)?;
                self.take_symbol(')')?;
                node
            }
            Token::Name(name) => {
                self.at += 1;
                if matches!(self.peek(), Token::Symbol('(')) {
                    self.at += 1;
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Token::Symbol(')')) {
                        args.push(self.expression(0)?);
                        while matches!(self.peek(), Token::Symbol(',')) {
                            self.at += 1;
                            args.push(self.expression(0)?);
                        }
                    }
                    self.take_symbol(')')?;
                    Node::Call(name, args)
                } else {
                    Node::Symbol(name)
                }
            }
            _ => return Err("Enter a number, symbol, or parenthesized expression.".to_string()),
        };
        loop {
            let op = match self.peek() {
                Token::Symbol(op @ ('+' | '-' | '*' | '/' | '^')) => *op,
                _ => break,
            };
            let precedence = match op {
                '+' | '-' => 1,
                '*' | '/' => 2,
                '^' => 3,
                _ => unreachable!(),
            };
            if precedence < minimum {
                break;
            }
            self.at += 1;
            let right = self.expression(precedence + u8::from(op != '^'))?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
}

fn parse(source: &str) -> Result<Node> {
    let source = if source.trim().is_empty() {
        "0"
    } else {
        source.trim()
    };
    let mut parser = Parser {
        tokens: tokenize(source)?,
        at: 0,
    };
    let node = parser.expression(0)?;
    if !matches!(parser.peek(), Token::End) {
        return Err("There is extra text after the expression.".to_string());
    }
    Ok(node)
}

fn geo_number(value: f64) -> String {
    format!("{value:.15e}")
}

fn geo_expression(
    node: &Node,
    symbols: &HashMap<String, Scalar>,
    coordinates: [&str; 3],
    path: &str,
) -> Result<String> {
    Ok(match node {
        Node::Number(value) => geo_number(*value),
        Node::Symbol(name) => match name.as_str() {
            "x" => coordinates[0].to_string(),
            "y" => coordinates[1].to_string(),
            "z" => coordinates[2].to_string(),
            "pi" => geo_number(std::f64::consts::PI),
            "e" => geo_number(std::f64::consts::E),
            _ => geo_number(
                symbols
                    .get(name)
                    .ok_or_else(|| format!("{path}: unknown symbol “{name}”."))?
                    .value,
            ),
        },
        Node::Unary(op, child) => format!(
            "({op}{})",
            geo_expression(child, symbols, coordinates, path)?
        ),
        Node::Binary(op, left, right) => format!(
            "({} {op} {})",
            geo_expression(left, symbols, coordinates, path)?,
            geo_expression(right, symbols, coordinates, path)?
        ),
        Node::Call(name, args) => {
            let rendered = args
                .iter()
                .map(|arg| geo_expression(arg, symbols, coordinates, path))
                .collect::<Result<Vec<_>>>()?;
            let gmsh_name = match name.as_str() {
                "sin" => "Sin",
                "cos" => "Cos",
                "tan" => "Tan",
                "asin" => "Asin",
                "acos" => "Acos",
                "atan" => "Atan",
                "sinh" => "Sinh",
                "cosh" => "Cosh",
                "tanh" => "Tanh",
                "exp" => "Exp",
                "log" | "ln" => "Log",
                "abs" => "Fabs",
                "floor" => "Floor",
                "ceil" => "Ceil",
                "sign" => "Sign",
                "sqrt" => "Sqrt",
                "min" | "max" => {
                    if rendered.is_empty() {
                        return Err(format!("{path}: {name} needs at least one argument."));
                    }
                    let operation = if name == "min" { "Min" } else { "Max" };
                    let mut value = rendered[0].clone();
                    for argument in rendered.iter().skip(1) {
                        value = format!("{operation}({value}, {argument})");
                    }
                    return Ok(value);
                }
                "pow" => {
                    if rendered.len() != 2 {
                        return Err(format!("{path}: pow expects two arguments."));
                    }
                    return Ok(format!("({})^({})", rendered[0], rendered[1]));
                }
                "log10" => {
                    if rendered.len() != 1 {
                        return Err(format!("{path}: log10 expects one argument."));
                    }
                    return Ok(format!("(Log({})/Log(10))", rendered[0]));
                }
                _ => {
                    return Err(format!(
                        "{path}: function “{name}” cannot be compiled to a Gmsh expression."
                    ));
                }
            };
            if rendered.len() != 1 {
                return Err(format!("{path}: {name} expects one argument."));
            }
            format!("{gmsh_name}({})", rendered[0])
        }
    })
}

/// Compile a model expression to Gmsh's scalar expression language. Model
/// parameters are replaced by their resolved SI values and x/y/z are replaced
/// by the supplied Gmsh coordinate expressions.
pub fn compile_geo_expression(
    source: &str,
    symbols: &HashMap<String, Scalar>,
    coordinates: [&str; 3],
    path: &str,
) -> Result<String> {
    let node = parse(source)?;
    geo_expression(&node, symbols, coordinates, path)
}

fn real_policy() -> Result<&'static Value> {
    static POLICY: OnceLock<Result<Value>> = OnceLock::new();
    match POLICY.get_or_init(|| {
        let capabilities = crate::schema_metadata::engine_capabilities()?;
        crate::schema_metadata::expression_runtime_policy(
            "real",
            &Value::Null,
            &json!({"capabilities": capabilities}),
        )
    }) {
        Ok(policy) => Ok(policy),
        Err(error) => Err(error.clone()),
    }
}

pub fn validate_expression(source: &str, path: &str) -> Result<()> {
    runtime::CompiledExpression::compile(source, real_policy()?, path).map(|_| ())
}

pub fn evaluate_expression(
    source: &str,
    symbols: &HashMap<String, Scalar>,
    path: &str,
) -> Result<Scalar> {
    runtime::CompiledExpression::compile(source, real_policy()?, path)?
        .evaluate_with(|name| symbols.get(name).copied(), path)
}

pub fn resolve_parameters(parameters: &[Value], model: &Value) -> Result<HashMap<String, Scalar>> {
    let mut reserved = ["pi", "e", "x", "y", "z", "freq", "omega", "t"]
        .into_iter()
        .map(str::to_string)
        .collect::<HashSet<_>>();
    reserved.extend(crate::schema_metadata::reserved_symbols_for_model(model)?);
    let reserved: Vec<_> = reserved.into_iter().collect();
    let policy = crate::schema_metadata::expression_runtime_policy(
        "parameters",
        model,
        &json!({"reservedSymbols": reserved, "realPolicy": real_policy()?}),
    )?;
    runtime::resolve_parameters(parameters, &policy)?
        .into_iter()
        .map(|(name, value)| {
            let scalar = serde_json::from_value(value).map_err(|error| error.to_string())?;
            Ok((name, scalar))
        })
        .collect()
}

pub fn evaluate_quantity(
    quantity: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
) -> Result<f64> {
    let unit_name = quantity
        .get("unit")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}: expected a unit."))?;
    if fullmeta_core::quantity::get_unit(unit_name)?.dimension != [1.0, 0.0, 0.0, 0.0, 0.0] {
        return Err(format!(
            "{path}: a geometry or mesh quantity must use a length unit, received {unit_name}."
        ));
    }
    runtime::CompiledQuantity::compile(quantity, real_policy()?, path)?
        .evaluate_with(|name| symbols.get(name).copied(), path)
}
