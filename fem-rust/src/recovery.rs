//! Declarative recovery of a vector field from a gradient or curl.
//!
//! This primitive computes a complex scalar factor times a real Cartesian
//! transform of a complex operator sample. It deliberately leaves cell traversal,
//! material sampling and output reduction to the caller during migration.

use std::collections::HashMap;

use serde_json::{Value, json};

use crate::coefficient_expr::CoefficientExpressions;
use crate::{C, c_add, c_mul, c_scale};

pub(crate) struct RecoveryPlan {
    operator: RecoveryOperator,
    dimension: usize,
    operator_dimension: usize,
    longitudinal_wavenumber: C,
    transform: [[f64; 3]; 3],
    factor: CoefficientExpressions,
    additive_domain_sources: Vec<(String, CoefficientExpressions)>,
    sampling: String,
    output: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecoveryOperator {
    Gradient,
    Curl,
}

impl RecoveryPlan {
    pub(crate) fn has_longitudinal_operator(&self) -> bool {
        self.dimension == 2 && self.operator_dimension == 3
    }

    pub(crate) fn set_longitudinal_wavenumber(&mut self, value: C) -> Result<(), String> {
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err("Recovery longitudinal wavenumber must be finite.".into());
        }
        if !self.has_longitudinal_operator() && value != C::default() {
            return Err("Recovery does not declare a longitudinal operator dimension.".into());
        }
        self.longitudinal_wavenumber = value;
        Ok(())
    }

    pub(crate) fn longitudinal_derivative(&self, value: C) -> C {
        if self.has_longitudinal_operator() {
            crate::longitudinal::derivative_value(self.longitudinal_wavenumber, value, true)
        } else {
            C::default()
        }
    }

    /// Check that the declared operator can act on the primary field.
    pub(crate) fn require_shape(&self, dimension: usize, components: usize) -> Result<(), String> {
        match self.operator {
            RecoveryOperator::Gradient if (2..=3).contains(&dimension) && components == 1 => Ok(()),
            RecoveryOperator::Curl if (2..=3).contains(&dimension) && self.operator_dimension == 3 && components == 3 => Ok(()),
            RecoveryOperator::Gradient => Err(
                "Gradient vector recovery requires a scalar field in two or three dimensions."
                    .into(),
            ),
            RecoveryOperator::Curl => Err(
                "Curl vector recovery requires a three-component field in three dimensions.".into(),
            ),
        }
    }

    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let dimension = value.get("dimension").and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()).unwrap_or(3);
        let operator_dimension = value.get("operatorDimension").and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()).unwrap_or(dimension);
        if !(2..=3).contains(&dimension) || !(dimension..=3).contains(&operator_dimension) {
            return Err("Recovery dimensions must declare a 2D or 3D mesh operator.".into());
        }
        let operator = match value.get("operator").and_then(Value::as_str) {
            Some("gradient") => RecoveryOperator::Gradient,
            Some("curl") => RecoveryOperator::Curl,
            _ => return Err("Recovery operator must be gradient or curl.".into()),
        };
        let rows = value
            .get("transform")
            .and_then(Value::as_array)
            .filter(|rows| rows.len() == 3)
            .ok_or("Recovery transform must be a 3-by-3 real matrix.")?;
        let mut transform = [[0.0; 3]; 3];
        for (i, row) in rows.iter().enumerate() {
            let values = row
                .as_array()
                .filter(|row| row.len() == 3)
                .ok_or("Recovery transform must be a 3-by-3 real matrix.")?;
            for (j, entry) in values.iter().enumerate() {
                transform[i][j] = entry.as_f64().filter(|x| x.is_finite()).ok_or_else(|| {
                    format!("Recovery transform[{i}][{j}] must be finite and real.")
                })?;
            }
        }
        let expression = value.get("factor").ok_or("Recovery factor is required.")?;
        let factor = CoefficientExpressions::from_json(&json!({"factor": expression}))?;
        let mut additive_domain_sources = Vec::new();
        if let Some(entries) = value.get("additiveDomainSources") {
            for (index, entry) in entries.as_array().ok_or("Recovery additiveDomainSources must be an array.")?.iter().enumerate() {
                let feature_type = entry.get("featureType").and_then(Value::as_str)
                    .filter(|v| !v.is_empty()).ok_or_else(|| format!("Recovery additiveDomainSources[{index}] requires featureType."))?.to_owned();
                let factor = CoefficientExpressions::from_json(&json!({"factor": entry.get("factor").ok_or_else(|| format!("Recovery additiveDomainSources[{index}] requires factor."))?}))?;
                if additive_domain_sources.iter().any(|(kind, _)| kind == &feature_type) {
                    return Err(format!("Recovery repeats additive domain source type {feature_type:?}."));
                }
                additive_domain_sources.push((feature_type, factor));
            }
        }
        let sampling = value
            .get("sampling")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("Recovery sampling is required.")?
            .to_owned();
        let output = value
            .get("output")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or("Recovery output is required.")?
            .to_owned();
        Ok(Self {
            operator,
            dimension,
            operator_dimension,
            longitudinal_wavenumber: C::default(),
            transform,
            factor,
            additive_domain_sources,
            sampling,
            output,
        })
    }

    /// Reject unsupported sample/reduction methods rather than silently
    /// interpreting them as the current runner's fixed behavior.
    pub(crate) fn require_policy(&self, sampling: &str, output: &str) -> Result<(), String> {
        if self.sampling != sampling || self.output != output {
            return Err(format!(
                "Recovery policy ({:?}, {:?}) is unsupported; expected ({sampling:?}, {output:?}).",
                self.sampling, self.output
            ));
        }
        Ok(())
    }

    pub(crate) fn apply(
        &self,
        gradient: [C; 3],
        symbols: &HashMap<String, C>,
    ) -> Result<[C; 3], String> {
        if self.operator != RecoveryOperator::Gradient {
            return Err("A curl recovery cannot consume a gradient sample.".into());
        }
        self.apply_sample(gradient, symbols)
    }

    pub(crate) fn apply_curl(
        &self,
        curl: [C; 3],
        symbols: &HashMap<String, C>,
    ) -> Result<[C; 3], String> {
        if self.operator != RecoveryOperator::Curl {
            return Err("A gradient recovery cannot consume a curl sample.".into());
        }
        self.apply_sample(curl, symbols)
    }

    pub(crate) fn additive_domain_source_types(&self) -> impl Iterator<Item = &str> {
        self.additive_domain_sources.iter().map(|(feature_type, _)| feature_type.as_str())
    }

    /// Apply the declared curl recovery and any scenario-declared additive
    /// source contributions. An absent source is the zero vector.
    pub(crate) fn apply_curl_with_domain_sources(
        &self,
        curl: [C; 3],
        symbols: &HashMap<String, C>,
        sources: &HashMap<String, Vec<C>>,
    ) -> Result<[C; 3], String> {
        let mut output = self.apply_curl(curl, symbols)?;
        for (feature_type, factor_plan) in &self.additive_domain_sources {
            let Some(source) = sources.get(feature_type) else { continue; };
            let vector: [C; 3] = source.as_slice().try_into()
                .map_err(|_| format!("Recovery source {feature_type:?} requires three components."))?;
            let factor = *factor_plan.evaluate(symbols)?.get("factor")
                .ok_or("Recovery additive source factor was not evaluated.")?;
            for i in 0..3 {
                let mut projected = C::default();
                for j in 0..3 { projected = c_add(projected, c_scale(vector[j], self.transform[i][j])); }
                output[i] = c_add(output[i], c_mul(factor, projected));
                if !output[i].re.is_finite() || !output[i].im.is_finite() {
                    return Err("Recovery additive source produced a non-finite value.".into());
                }
            }
        }
        Ok(output)
    }

    fn apply_sample(&self, sample: [C; 3], symbols: &HashMap<String, C>) -> Result<[C; 3], String> {
        if sample
            .iter()
            .any(|c| !c.re.is_finite() || !c.im.is_finite())
        {
            return Err("Recovery operator sample must be finite.".into());
        }
        let factors = self.factor.evaluate(symbols)?;
        let factor = *factors
            .get("factor")
            .ok_or("Recovery factor was not evaluated.")?;
        let mut output = [C::default(); 3];
        for i in 0..3 {
            let mut projected = C::default();
            for j in 0..3 {
                projected = c_add(projected, c_scale(sample[j], self.transform[i][j]));
            }
            output[i] = c_mul(factor, projected);
            if !output[i].re.is_finite() || !output[i].im.is_finite() {
                return Err("Recovery produced a non-finite value.".into());
            }
        }
        Ok(output)
    }
}

pub(crate) fn load_plan(profile: &str, formulation: &str) -> Result<RecoveryPlan, String> {
    let package = crate::scenario_package::load_installed(profile)?;
    let value = package
        .get("recoveries")
        .and_then(|v| v.get(formulation))
        .ok_or_else(|| format!("Scenario {profile:?} lacks recoveries[{formulation:?}]."))?;
    RecoveryPlan::from_json(value)
        .map_err(|e| format!("Scenario {profile:?} recoveries[{formulation:?}]: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: C, expected: C) {
        assert!((actual.re - expected.re).abs() < 1e-12);
        assert!((actual.im - expected.im).abs() < 1e-12);
    }

    #[test]
    fn json_identity_and_rotation_recover_complex_gradients() {
        let value = json!({"operator":"gradient", "transform":[[1,0,0],[0,1,0],[0,0,1]],
            "factor":{"divide":[{"complex":[0,1]},{"multiply":[{"symbol":"omega"},{"symbol":"rho"}]}]},
            "sampling":"cellCentroid", "output":"nodalArithmeticMean"});
        let plan = RecoveryPlan::from_json(&value).unwrap();
        plan.require_policy("cellCentroid", "nodalArithmeticMean")
            .unwrap();
        let symbols = HashMap::from([
            ("omega".into(), C { re: 2., im: 0. }),
            ("rho".into(), C { re: 2., im: 0. }),
        ]);
        let gradient = [C { re: 2., im: 3. }, C { re: -1., im: 1. }, C::default()];
        let recovered = plan.apply(gradient, &symbols).unwrap();
        close(recovered[0], C { re: -0.75, im: 0.5 });
        close(
            recovered[1],
            C {
                re: -0.25,
                im: -0.25,
            },
        );
        let mut rotated = value;
        rotated["transform"] = json!([[0, 1, 0], [-1, 0, 0], [0, 0, 0]]);
        let recovered_rotated = RecoveryPlan::from_json(&rotated)
            .unwrap()
            .apply(gradient, &symbols)
            .unwrap();
        close(recovered_rotated[0], recovered[1]);
        close(
            recovered_rotated[1],
            C {
                re: -recovered[0].re,
                im: -recovered[0].im,
            },
        );
    }

    #[test]
    fn invalid_transform_and_unsupported_policy_are_rejected() {
        let mut value = json!({"operator":"gradient", "transform":[[1,0,0],[0,1,0],[0,0,1]],
            "factor":-1, "sampling":"cellConstant", "output":"cell"});
        let plan = RecoveryPlan::from_json(&value).unwrap();
        assert!(
            plan.require_policy("cellCentroid", "nodalArithmeticMean")
                .is_err()
        );
        value["transform"] = json!([[1, 0], [0, 1]]);
        assert!(RecoveryPlan::from_json(&value).is_err());
    }

    #[test]
    fn curl_recovery_uses_declared_model_axes_and_rejects_wrong_sample_kind() {
        let value = json!({"operator":"curl", "transform":[[0,1,0],[0,0,1],[1,0,0]],
            "factor":{"divide":[{"complex":[0,1]},{"multiply":[{"symbol":"omega"},{"symbol":"mu"}]}]},
            "sampling":"cellCentroid", "output":"nodalArithmeticMeanAndPoint"});
        let plan = RecoveryPlan::from_json(&value).unwrap();
        let input = [
            C { re: 2., im: 3. },
            C { re: 4., im: -2. },
            C { re: -1., im: 1. },
        ];
        let symbols = HashMap::from([
            ("omega".into(), C { re: 2., im: 0. }),
            ("mu".into(), C { re: 2., im: 0. }),
        ]);
        assert!(plan.apply(input, &symbols).is_err());
        let result = plan.apply_curl(input, &symbols).unwrap();
        close(result[0], C { re: 0.5, im: 1.0 });
        close(
            result[1],
            C {
                re: -0.25,
                im: -0.25,
            },
        );
        close(result[2], C { re: -0.75, im: 0.5 });
    }

    #[test]
    fn longitudinal_recovery_uses_minus_i_kz_times_the_primary_field() {
        let plan = RecoveryPlan::from_json(&json!({"dimension":2,"operatorDimension":3,
            "operator":"gradient","transform":[[1,0,0],[0,1,0],[0,0,1]],
            "factor":1,"sampling":"cellCentroid","output":"cell"})).unwrap();
        let mut plan=plan;
        plan.set_longitudinal_wavenumber(C { re:2., im:3. }).unwrap();
        let derivative=plan.longitudinal_derivative(C { re:4., im:1. });
        close(derivative,C { re:14., im:-5. });
        plan.set_longitudinal_wavenumber(C::default()).unwrap();
        close(plan.longitudinal_derivative(C { re:4., im:1. }),C::default());
    }
}
