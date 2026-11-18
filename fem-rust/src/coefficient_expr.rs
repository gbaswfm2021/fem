//! Small typed complex arithmetic expressions for declarative form coefficients.
//!
//! Inputs are already evaluated numerical symbols. Model quantity parsing,
//! dimensions and dependency validation remain with the existing model adapters;
//! this module does not replace the FullMeta expression runtime.

use crate::{C, c_add, c_mul};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

#[derive(Debug)]
enum Expr {
    Constant(f64),
    Symbol(String),
    Add(Vec<Expr>),
    Multiply(Vec<Expr>),
    Divide(Box<Expr>, Box<Expr>),
    Complex(Box<Expr>, Box<Expr>),
}

impl Expr {
    fn dependencies(&self, output: &mut HashSet<String>) {
        match self {
            Self::Symbol(name) => {
                output.insert(name.clone());
            }
            Self::Add(args) | Self::Multiply(args) => {
                for arg in args {
                    arg.dependencies(output);
                }
            }
            Self::Divide(a, b) | Self::Complex(a, b) => {
                a.dependencies(output);
                b.dependencies(output);
            }
            Self::Constant(_) => {}
        }
    }

    /// Lower once to a compact stack program with integer-indexed symbols.
    fn emit(&self, slots: &HashMap<String, usize>, code: &mut Vec<Instruction>) {
        match self {
            Self::Constant(value) => code.push(Instruction::Constant(*value)),
            Self::Symbol(name) => code.push(Instruction::Load(slots[name])),
            Self::Add(args) => {
                code.push(Instruction::Constant(0.0));
                for arg in args {
                    arg.emit(slots, code);
                    code.push(Instruction::Add);
                }
            }
            Self::Multiply(args) => {
                code.push(Instruction::Constant(1.0));
                for arg in args {
                    arg.emit(slots, code);
                    code.push(Instruction::Multiply);
                }
            }
            Self::Divide(a, b) | Self::Complex(a, b) => {
                a.emit(slots, code);
                b.emit(slots, code);
                code.push(if matches!(self, Self::Divide(_, _)) {
                    Instruction::Divide
                } else {
                    Instruction::Complex
                });
            }
        }
    }

    fn parse(value: &Value, depth: usize, budget: &mut usize, path: &str) -> Result<Self, String> {
        if depth > 32 || *budget == 0 {
            return Err(format!(
                "{path}: coefficient expression exceeds its complexity limit."
            ));
        }
        *budget -= 1;
        if let Some(number) = value.as_f64() {
            return if number.is_finite() {
                Ok(Self::Constant(number))
            } else {
                Err(format!("{path}: constant must be finite."))
            };
        }
        let object = value
            .as_object()
            .filter(|object| object.len() == 1)
            .ok_or_else(|| format!("{path}: expected a number or a single-operator object."))?;
        let (operator, argument) = object.iter().next().unwrap();
        if operator == "symbol" {
            let name = argument
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .ok_or_else(|| format!("{path}.symbol: expected a nonempty symbol name."))?;
            return Ok(Self::Symbol(name.to_owned()));
        }
        let arguments = argument
            .as_array()
            .ok_or_else(|| format!("{path}.{operator}: expected an array."))?;
        if arguments.is_empty() || arguments.len() > 64 {
            return Err(format!(
                "{path}.{operator}: expected between 1 and 64 arguments."
            ));
        }
        let mut parsed = Vec::with_capacity(arguments.len());
        for (index, child) in arguments.iter().enumerate() {
            parsed.push(Self::parse(
                child,
                depth + 1,
                budget,
                &format!("{path}.{operator}[{index}]"),
            )?);
        }
        match operator.as_str() {
            "add" => Ok(Self::Add(parsed)),
            "multiply" => Ok(Self::Multiply(parsed)),
            "divide" | "complex" if parsed.len() == 2 => {
                let mut iter = parsed.into_iter();
                let first = Box::new(iter.next().unwrap());
                let second = Box::new(iter.next().unwrap());
                if operator == "divide" {
                    Ok(Self::Divide(first, second))
                } else {
                    Ok(Self::Complex(first, second))
                }
            }
            "divide" | "complex" => Err(format!(
                "{path}.{operator}: expected exactly two arguments."
            )),
            _ => Err(format!(
                "{path}: unsupported coefficient operator {operator:?}."
            )),
        }
    }

    #[cfg(test)]
    fn evaluate(&self, symbols: &HashMap<String, C>, path: &str) -> Result<C, String> {
        let result = match self {
            Self::Constant(value) => C {
                re: *value,
                im: 0.0,
            },
            Self::Symbol(name) => *symbols
                .get(name)
                .ok_or_else(|| format!("{path}: missing symbol {name:?}."))?,
            Self::Add(args) => {
                let mut value = C::default();
                for arg in args {
                    value = c_add(value, arg.evaluate(symbols, path)?);
                }
                value
            }
            Self::Multiply(args) => {
                let mut value = C { re: 1.0, im: 0.0 };
                for arg in args {
                    value = c_mul(value, arg.evaluate(symbols, path)?);
                }
                value
            }
            Self::Divide(a, b) => {
                let numerator = a.evaluate(symbols, path)?;
                let denominator = b.evaluate(symbols, path)?;
                if denominator.re == 0.0 && denominator.im == 0.0 {
                    return Err(format!("{path}: division by zero."));
                }
                // Normalize before squaring to avoid overflow/underflow for
                // representable results such as 1/(1e-300) or 1/(1e300).
                let scale = denominator.re.abs().max(denominator.im.abs());
                let re = denominator.re / scale;
                let im = denominator.im / scale;
                let norm = re * re + im * im;
                let nr = numerator.re / scale;
                let ni = numerator.im / scale;
                C {
                    re: (nr * re + ni * im) / norm,
                    im: (ni * re - nr * im) / norm,
                }
            }
            Self::Complex(re, im) => {
                let re = re.evaluate(symbols, path)?;
                let im = im.evaluate(symbols, path)?;
                if re.im != 0.0 || im.im != 0.0 {
                    return Err(format!(
                        "{path}: complex constructor components must be real."
                    ));
                }
                C {
                    re: re.re,
                    im: im.re,
                }
            }
        };
        if !result.re.is_finite() || !result.im.is_finite() {
            return Err(format!(
                "{path}: coefficient expression produced a non-finite value."
            ));
        }
        Ok(result)
    }
}

#[derive(Clone, Copy)]
enum Instruction {
    Constant(f64),
    Load(usize),
    Add,
    Multiply,
    Divide,
    Complex,
}

struct Program {
    name: String,
    target: usize,
    code: Vec<Instruction>,
}

pub(crate) struct CoefficientExpressions {
    slot_names: Vec<String>,
    slot_by_name: HashMap<String, usize>,
    programs: Vec<Program>,
    stack_capacity: usize,
}

/// Reusable buffers bound to a particular expression plan and ordered list of
/// inputs. Calling `evaluate` overwrites the previous result in-place.
pub(crate) struct BoundCoefficients<'a> {
    expressions: &'a CoefficientExpressions,
    input_slots: Vec<Option<usize>>,
    values: Vec<Option<C>>,
    stack: Vec<C>,
    output: HashMap<String, C>,
}

impl CoefficientExpressions {
    pub(crate) fn declares(&self, name: &str) -> bool {
        self.programs.iter().any(|program| program.name == name)
    }

    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let object = value
            .as_object()
            .filter(|o| !o.is_empty())
            .ok_or("Coefficient expressions must be a nonempty object.")?;
        let mut expressions = HashMap::new();
        let mut budget = 4096;
        for (name, expression) in object {
            if name.is_empty() || name.len() > 128 {
                return Err(
                    "Coefficient names must be nonempty and at most 128 characters.".into(),
                );
            }
            expressions.insert(
                name.clone(),
                Expr::parse(expression, 0, &mut budget, &format!("coefficients.{name}"))?,
            );
        }
        // Compile a deterministic dependency order once, rather than
        // rediscovering the graph at every quadrature point. A self-named
        // symbol is an external input, such as `mu = symbol(mu)`.
        let mut pending: Vec<String> = expressions.keys().cloned().collect();
        pending.sort();
        let mut order = Vec::with_capacity(pending.len());
        while !pending.is_empty() {
            let ready = pending.iter().position(|name| {
                let mut dependencies = HashSet::new();
                expressions[name].dependencies(&mut dependencies);
                !dependencies
                    .iter()
                    .any(|dependency| dependency != name && pending.contains(dependency))
            });
            let Some(index) = ready else {
                return Err(format!(
                    "Cyclic coefficient dependencies: {}.",
                    pending.join(", ")
                ));
            };
            order.push(pending.remove(index));
        }
        let mut names: HashSet<String> = expressions.keys().cloned().collect();
        for expression in expressions.values() {
            expression.dependencies(&mut names);
        }
        let mut slot_names: Vec<_> = names.into_iter().collect();
        slot_names.sort();
        let slot_by_name: HashMap<_, _> = slot_names
            .iter()
            .enumerate()
            .map(|(index, name)| (name.clone(), index))
            .collect();
        let mut stack_capacity = 0;
        let programs = order
            .into_iter()
            .map(|name| {
                let mut code = Vec::new();
                expressions[&name].emit(&slot_by_name, &mut code);
                stack_capacity = stack_capacity.max(code.len());
                Program {
                    target: slot_by_name[&name],
                    name,
                    code,
                }
            })
            .collect();
        Ok(Self {
            slot_names,
            slot_by_name,
            programs,
            stack_capacity,
        })
    }

    pub(crate) fn evaluate(
        &self,
        symbols: &HashMap<String, C>,
    ) -> Result<HashMap<String, C>, String> {
        let pairs: Vec<_> = symbols.iter().collect();
        let names: Vec<_> = pairs.iter().map(|(name, _)| name.as_str()).collect();
        let mut bound = self.bind(&names)?;
        let values: Vec<_> = pairs.iter().map(|(_, value)| **value).collect();
        bound.evaluate(&values)?;
        Ok(bound.output)
    }

    /// Bind input names to integer slots once; unrelated inputs are accepted
    /// and ignored, matching the existing symbol-map evaluation contract.
    pub(crate) fn bind<'a>(&'a self, names: &[&str]) -> Result<BoundCoefficients<'a>, String> {
        let mut seen = HashSet::new();
        let mut input_slots = Vec::with_capacity(names.len());
        for &name in names {
            if !seen.insert(name) {
                return Err(format!("Duplicate coefficient input symbol {name:?}."));
            }
            input_slots.push(self.slot_by_name.get(name).copied());
        }
        let output = self
            .programs
            .iter()
            .map(|program| (program.name.clone(), C::default()))
            .collect();
        Ok(BoundCoefficients {
            expressions: self,
            input_slots,
            values: vec![None; self.slot_names.len()],
            stack: Vec::with_capacity(self.stack_capacity),
            output,
        })
    }
}

impl BoundCoefficients<'_> {
    pub(crate) fn evaluate(&mut self, inputs: &[C]) -> Result<&HashMap<String, C>, String> {
        if inputs.len() != self.input_slots.len() {
            return Err(format!(
                "Expected {} coefficient inputs, got {}.",
                self.input_slots.len(),
                inputs.len()
            ));
        }
        self.values.fill(None);
        for (slot, value) in self.input_slots.iter().zip(inputs) {
            if let Some(slot) = slot {
                self.values[*slot] = Some(*value);
            }
        }
        for program in &self.expressions.programs {
            self.stack.clear();
            for instruction in &program.code {
                let path = || format!("coefficients.{}", program.name);
                let value = match *instruction {
                    Instruction::Constant(value) => C { re: value, im: 0.0 },
                    Instruction::Load(slot) => self.values[slot].ok_or_else(|| {
                        format!(
                            "{}: missing symbol {:?}.",
                            path(),
                            self.expressions.slot_names[slot]
                        )
                    })?,
                    Instruction::Add
                    | Instruction::Multiply
                    | Instruction::Divide
                    | Instruction::Complex => {
                        let b = self
                            .stack
                            .pop()
                            .ok_or("Invalid coefficient expression stack.")?;
                        let a = self
                            .stack
                            .pop()
                            .ok_or("Invalid coefficient expression stack.")?;
                        match *instruction {
                            Instruction::Add => c_add(a, b),
                            Instruction::Multiply => c_mul(a, b),
                            Instruction::Divide => {
                                if b.re == 0.0 && b.im == 0.0 {
                                    return Err(format!("{}: division by zero.", path()));
                                }
                                let scale = b.re.abs().max(b.im.abs());
                                let re = b.re / scale;
                                let im = b.im / scale;
                                let norm = re * re + im * im;
                                let nr = a.re / scale;
                                let ni = a.im / scale;
                                C {
                                    re: (nr * re + ni * im) / norm,
                                    im: (ni * re - nr * im) / norm,
                                }
                            }
                            Instruction::Complex => {
                                if a.im != 0.0 || b.im != 0.0 {
                                    return Err(format!(
                                        "{}: complex constructor components must be real.",
                                        path()
                                    ));
                                }
                                C { re: a.re, im: b.re }
                            }
                            _ => unreachable!(),
                        }
                    }
                };
                if !value.re.is_finite() || !value.im.is_finite() {
                    return Err(format!(
                        "{}: coefficient expression produced a non-finite value.",
                        path()
                    ));
                }
                self.stack.push(value);
            }
            if self.stack.len() != 1 {
                return Err("Invalid coefficient expression stack result.".into());
            }
            let value = self.stack[0];
            self.values[program.target] = Some(value);
            *self
                .output
                .get_mut(&program.name)
                .ok_or("Missing coefficient output slot.")? = value;
        }
        Ok(&self.output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn arithmetic_and_complex_values_come_from_declaration() {
        let expressions = CoefficientExpressions::from_json(&json!({
            "a":{"divide":[1,{"symbol":"x"}]},
            "b":{"complex":[{"multiply":[-1,{"symbol":"x"},{"symbol":"x"}]},{"symbol":"y"}]}
        }))
        .unwrap();
        let symbols = HashMap::from([
            ("x".into(), C { re: 2., im: 0. }),
            ("y".into(), C { re: 3., im: 0. }),
        ]);
        let result = expressions.evaluate(&symbols).unwrap();
        assert_eq!(result["a"].re, 0.5);
        assert_eq!(result["b"].re, -4.);
        assert_eq!(result["b"].im, 3.);
    }

    #[test]
    fn invalid_or_nonfinite_expressions_fail() {
        assert!(CoefficientExpressions::from_json(&json!({"x":{"divide":[1]}})).is_err());
        let expressions =
            CoefficientExpressions::from_json(&json!({"x":{"divide":[1,{"symbol":"denom"}]}}))
                .unwrap();
        assert!(expressions.evaluate(&HashMap::new()).is_err());
        assert!(
            expressions
                .evaluate(&HashMap::from([("denom".into(), C::default())]))
                .is_err()
        );
    }

    #[test]
    fn division_handles_extreme_finite_denominators() {
        let expressions =
            CoefficientExpressions::from_json(&json!({"x":{"divide":[1,{"symbol":"d"}]}})).unwrap();
        for d in [1e-300, 1e300] {
            let symbols = HashMap::from([("d".into(), C { re: d, im: 0.0 })]);
            let result = expressions.evaluate(&symbols).unwrap();
            assert!((result["x"].re * d - 1.0).abs() < 1e-14);
            assert_eq!(result["x"].im, 0.0);
        }
    }

    #[test]
    fn resolves_dependent_coefficients_and_rejects_cycles() {
        let expressions = CoefficientExpressions::from_json(&json!({
            "derived": {"multiply":[{"symbol":"base"}, 3]},
            "base": {"add":[{"symbol":"input"}, 2]}
        }))
        .unwrap();
        let result = expressions
            .evaluate(&HashMap::from([("input".into(), C { re: 2.0, im: 0.0 })]))
            .unwrap();
        assert_eq!(result["derived"].re, 12.0);
        assert!(
            CoefficientExpressions::from_json(&json!({"a":{"symbol":"b"}, "b":{"symbol":"a"}}))
                .is_err()
        );
    }

    #[test]
    fn bound_slots_reuse_workspace_and_preserve_dependencies() {
        let expressions = CoefficientExpressions::from_json(&json!({
            "base": {"add":[{"symbol":"base"},{"symbol":"x"}]},
            "derived": {"multiply":[{"symbol":"base"},3]},
            "phasor": {"complex":[{"symbol":"derived"},{"symbol":"y"}]}
        }))
        .unwrap();
        let mut bound = expressions.bind(&["x", "base", "y", "unused"]).unwrap();
        let inputs = [
            C { re: 2., im: 0. },
            C { re: 4., im: 0. },
            C { re: -1., im: 0. },
            C::default(),
        ];
        let values = bound.evaluate(&inputs).unwrap();
        assert_eq!(values["base"].re, 6.);
        assert_eq!(values["derived"].re, 18.);
        assert_eq!(values["phasor"].im, -1.);
        let inputs = [
            C { re: -2., im: 0. },
            C { re: 1., im: 0. },
            C { re: 2., im: 0. },
            C::default(),
        ];
        let values = bound.evaluate(&inputs).unwrap();
        assert_eq!(values["derived"].re, -3.);
        assert_eq!(values["phasor"].im, 2.);
        assert!(bound.evaluate(&inputs[..3]).is_err());
        assert!(expressions.bind(&["x", "x"]).is_err());
        let mut missing = expressions.bind(&["base"]).unwrap();
        assert!(
            missing
                .evaluate(&[C { re: 1., im: 0. }])
                .unwrap_err()
                .contains("missing symbol")
        );
    }

    #[test]
    fn compiled_program_matches_reference_tree_for_complex_arithmetic() {
        let source = json!({"divide":[
            {"multiply":[{"add":[{"symbol":"a"},{"symbol":"b"}]},{"complex":[2,3]}]},
            {"add":[{"symbol":"b"},4]}
        ]});
        let expressions =
            CoefficientExpressions::from_json(&json!({"out":source.clone()})).unwrap();
        let reference = Expr::parse(&source, 0, &mut 4096, "reference").unwrap();
        let mut bound = expressions.bind(&["a", "b"]).unwrap();
        for inputs in [
            [C { re: 1.0, im: 2.0 }, C { re: -2.0, im: 0.5 }],
            [C { re: -3.0, im: 1.0 }, C { re: 2.0, im: -1.0 }],
        ] {
            let symbols =
                HashMap::from([("a".to_string(), inputs[0]), ("b".to_string(), inputs[1])]);
            let expected = reference.evaluate(&symbols, "reference").unwrap();
            let actual = bound.evaluate(&inputs).unwrap()["out"];
            assert!((expected.re - actual.re).abs() < 1e-12);
            assert!((expected.im - actual.im).abs() < 1e-12);
        }
    }
}
