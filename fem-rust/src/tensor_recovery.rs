//! Declarative complex tensor recovery from a symmetric gradient.
//!
//! A scenario provides the output order and a matrix of coefficient names or
//! constants. The generic engine symmetrizes the sampled gradient, evaluates
//! the declared scalar coefficients, and performs the matrix contraction.
//! Traversal, sampling and output reduction remain caller responsibilities.
use std::collections::HashMap;

use serde_json::Value;

use crate::coefficient_expr::CoefficientExpressions;
use crate::{C, c_add, c_mul, c_scale};

enum Entry {
    Constant(f64),
    Coefficient(String),
}

pub(crate) struct TensorRecoveryPlan {
    dimension: usize,
    components: Vec<String>,
    matrix: Vec<Vec<Entry>>,
    coefficients: CoefficientExpressions,
    sampling: String,
    output: String,
}

impl TensorRecoveryPlan {
    /// Output component order as declared by the package.
    pub(crate) fn components(&self) -> &[String] { &self.components }

    pub(crate) fn require_policy(&self, sampling: &str, output: &str) -> Result<(), String> {
        if self.sampling != sampling || self.output != output {
            return Err(format!("Tensor recovery policy ({:?}, {:?}) is unsupported; expected ({sampling:?}, {output:?}).", self.sampling, self.output));
        }
        Ok(())
    }
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        if value.get("operator").and_then(Value::as_str) != Some("symmetricGradient") {
            return Err("Tensor recovery operator must be symmetricGradient.".into());
        }
        let dimension = value
            .get("dimension")
            .and_then(Value::as_u64)
            .ok_or("Tensor recovery dimension must be an integer.")?
            as usize;
        let field_components = value
            .get("fieldComponents")
            .and_then(Value::as_u64)
            .ok_or("Tensor recovery fieldComponents must be an integer.")?
            as usize;
        if !(2..=3).contains(&dimension) || field_components != dimension {
            return Err(
                "Tensor recovery requires a 2D or 3D vector field matching its dimension.".into(),
            );
        }
        let labels = value
            .get("components")
            .and_then(Value::as_array)
            .filter(|v| !v.is_empty() && v.len() <= 16)
            .ok_or("Tensor recovery components must be a nonempty array of at most 16 names.")?;
        let mut components = Vec::with_capacity(labels.len());
        for label in labels {
            let name = label
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .ok_or("Tensor recovery component names must be nonempty strings.")?;
            if components.iter().any(|item| item == name) {
                return Err(format!("Duplicate tensor recovery component {name:?}."));
            }
            components.push(name.to_string());
        }
        let rows = value
            .get("matrix")
            .and_then(Value::as_array)
            .filter(|rows| rows.len() == components.len())
            .ok_or("Tensor recovery matrix must have one row per output component.")?;
        let mut matrix = Vec::with_capacity(rows.len());
        for (r, row) in rows.iter().enumerate() {
            let entries = row
                .as_array()
                .filter(|v| v.len() == dimension * dimension)
                .ok_or_else(|| {
                    format!(
                        "Tensor recovery matrix row {r} must contain {} entries.",
                        dimension * dimension
                    )
                })?;
            let mut parsed = Vec::with_capacity(entries.len());
            for (c, entry) in entries.iter().enumerate() {
                if let Some(number) = entry.as_f64().filter(|x| x.is_finite()) {
                    parsed.push(Entry::Constant(number));
                } else if let Some(name) =
                    entry.as_str().filter(|s| !s.is_empty() && s.len() <= 128)
                {
                    parsed.push(Entry::Coefficient(name.to_string()));
                } else {
                    return Err(format!(
                        "Tensor recovery matrix[{r}][{c}] must be a finite number or coefficient name."
                    ));
                }
            }
            matrix.push(parsed);
        }
        let policy = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("Tensor recovery {name} is required."))
        };
        Ok(Self {
            dimension,
            components,
            matrix,
            coefficients: CoefficientExpressions::from_json(
                value
                    .get("coefficients")
                    .ok_or("Tensor recovery coefficients are required.")?,
            )?,
            sampling: policy("sampling")?,
            output: policy("output")?,
        })
    }

    pub(crate) fn require_contract(
        &self,
        dimension: usize,
        components: &[&str],
        sampling: &str,
        output: &str,
    ) -> Result<(), String> {
        if self.dimension != dimension
            || self
                .components
                .iter()
                .map(String::as_str)
                .ne(components.iter().copied())
            || self.sampling != sampling
            || self.output != output
        {
            return Err(format!(
                "Tensor recovery dimension, component order, or sampling/output policy is unsupported: dimension {}, components {:?}, policy ({:?}, {:?}).",
                self.dimension, self.components, self.sampling, self.output
            ));
        }
        Ok(())
    }

    pub(crate) fn require_shape(
        &self,
        dimension: usize,
        field_components: usize,
    ) -> Result<(), String> {
        if self.dimension != dimension || self.dimension != field_components {
            return Err(format!(
                "Tensor recovery declares a {}-component field in {}D; expected {field_components} components in {dimension}D.",
                self.dimension, self.dimension
            ));
        }
        Ok(())
    }

    /// `gradient` uses row-major field-component-by-coordinate ordering.
    pub(crate) fn apply(
        &self,
        gradient: &[C],
        symbols: &HashMap<String, C>,
    ) -> Result<Vec<C>, String> {
        let dimension = self.dimension;
        if gradient.len() != dimension * dimension
            || gradient
                .iter()
                .any(|v| !v.re.is_finite() || !v.im.is_finite())
        {
            return Err("Tensor recovery needs a finite, correctly shaped gradient.".into());
        }
        let mut symmetric = [C::default(); 9];
        for i in 0..dimension {
            for j in 0..dimension {
                symmetric[i * dimension + j] = c_scale(
                    c_add(gradient[i * dimension + j], gradient[j * dimension + i]),
                    0.5,
                );
            }
        }
        let coefficients = self.coefficients.evaluate(symbols)?;
        let mut result = Vec::with_capacity(self.matrix.len());
        for row in &self.matrix {
            let mut value = C::default();
            for (entry, sample) in row.iter().zip(&symmetric) {
                let coefficient = match entry {
                    Entry::Constant(number) => C {
                        re: *number,
                        im: 0.0,
                    },
                    Entry::Coefficient(name) => *coefficients.get(name).ok_or_else(|| {
                        format!("Tensor recovery coefficient {name:?} is missing.")
                    })?,
                };
                value = c_add(value, c_mul(coefficient, *sample));
            }
            if !value.re.is_finite() || !value.im.is_finite() {
                return Err("Tensor recovery produced a non-finite component.".into());
            }
            result.push(value);
        }
        Ok(result)
    }
}

pub(crate) fn load_plan(profile: &str, formulation: &str) -> Result<TensorRecoveryPlan, String> {
    let package = crate::scenario_package::load_installed(profile)?;
    let value = package
        .get("tensorRecoveries")
        .and_then(|v| v.get(formulation))
        .ok_or_else(|| format!("Scenario {profile:?} lacks tensorRecoveries[{formulation:?}]."))?;
    TensorRecoveryPlan::from_json(value)
        .map_err(|e| format!("Scenario {profile:?} tensorRecoveries[{formulation:?}]: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn matrix_recovers_complex_symmetric_gradient_and_respects_declared_order() {
        let plan = TensorRecoveryPlan::from_json(
            &json!({"operator":"symmetricGradient","dimension":2,"fieldComponents":2,
            "components":["normal","shear"],"sampling":"cellCentroid","output":"cell",
            "coefficients":{"a":{"symbol":"a"},"b":{"symbol":"b"}},
            "matrix":[["a",0,0,0],[0,"b","b",0]]}),
        )
        .unwrap();
        plan.require_contract(2, &["normal", "shear"], "cellCentroid", "cell")
            .unwrap();
        assert!(
            plan.require_contract(2, &["shear", "normal"], "cellCentroid", "cell")
                .is_err()
        );
        let gradient = [
            C { re: 2., im: 1. },
            C { re: 3., im: -1. },
            C { re: -1., im: 3. },
            C::default(),
        ];
        let symbols = HashMap::from([
            ("a".into(), C { re: 2., im: 0. }),
            ("b".into(), C { re: 1., im: 1. }),
        ]);
        let result = plan.apply(&gradient, &symbols).unwrap();
        assert!((result[0].re - 4.0).abs() < 1e-12 && (result[0].im - 2.0).abs() < 1e-12);
        assert!((result[1].re - 0.0).abs() < 1e-12 && (result[1].im - 4.0).abs() < 1e-12);
        assert!(plan.apply(&gradient[..3], &symbols).is_err());
    }
}
