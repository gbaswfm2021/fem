//! Declarative essential traces and generic weighted-DOF constraints.
//!
//! A point trace can currently be mapped to one nodal DOF per component. The
//! same insertion primitive accepts already-computed oriented moments and
//! periodic weights, but this module does not yet evaluate edge/face moment
//! integrals or solve general coupled multipoint constraints.
use crate::C;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Functional {
    Point,
    OrientedMoment,
}

#[derive(Clone, Copy, Debug)]
enum LegacyInput {
    ZeroOnly,
    Scalar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Trace {
    Value,
    Tangential,
}

pub(crate) struct EssentialFormPlan {
    dimension: usize,
    components: usize,
    functional: Functional,
    trace: Trace,
    legacy: LegacyInput,
    zero_tolerance: f64,
    conflict_relative: f64,
    conflict_scale_floor: f64,
    replace_compatible: bool,
    real_values: bool,
    conflict_message: Option<String>,
    legacy_message: Option<String>,
    legacy_field_message: Option<String>,
    field_message: Option<String>,
    invalid_weight_message: Option<String>,
    minimum_weight_magnitude: f64,
}

impl EssentialFormPlan {
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let number = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(|| {
                    format!("Essential form {name} must be a finite nonnegative number.")
                })
        };
        let message = |name: &str| -> Result<Option<String>, String> {
            match value.get(name) {
                None => Ok(None),
                Some(v) => v.as_str().filter(|text| !text.is_empty() && text.len() <= 1024)
                    .map(|text| Some(text.to_string()))
                    .ok_or_else(|| format!("Essential form {name} must be a nonempty diagnostic template.")),
            }
        };
        let dimension = value
            .get("dimension")
            .and_then(Value::as_u64)
            .ok_or("Essential form dimension must be an integer.")?
            as usize;
        let components = value
            .get("fieldComponents")
            .and_then(Value::as_u64)
            .ok_or("Essential form fieldComponents must be an integer.")?
            as usize;
        if !(2..=3).contains(&dimension) || !(1..=3).contains(&components) {
            return Err(
                "Essential form requires dimension 2 or 3 and 1–3 field components.".into(),
            );
        }
        let functional = match value.get("functional").and_then(Value::as_str) {
            Some("point") => Functional::Point,
            Some("orientedMoment") => Functional::OrientedMoment,
            _ => return Err("Essential form functional must be point or orientedMoment.".into()),
        };
        let trace = match value.get("trace").and_then(Value::as_str) {
            Some("value") => Trace::Value,
            Some("tangential") if functional == Functional::OrientedMoment => Trace::Tangential,
            _ => return Err("Essential form trace is unsupported for its functional.".into()),
        };
        let legacy = match value.get("legacyInput").and_then(Value::as_str) {
            Some("zeroOnly") => LegacyInput::ZeroOnly,
            Some("scalar") if components == 1 => LegacyInput::Scalar,
            _ => {
                return Err(
                    "Essential form legacyInput must be zeroOnly or a single-component scalar."
                        .into(),
                );
            }
        };
        let replace_compatible = match value.get("duplicatePolicy").and_then(Value::as_str) {
            None | Some("first") => false,
            Some("last") => true,
            _ => return Err("Essential form duplicatePolicy must be first or last.".into()),
        };
        let real_values = match value.get("valueType").and_then(Value::as_str) {
            None | Some("complex") => false,
            Some("real") => true,
            _ => return Err("Essential form valueType must be real or complex.".into()),
        };
        Ok(Self {
            dimension,
            components,
            functional,
            trace,
            legacy,
            zero_tolerance: number("legacyZeroTolerance")?,
            conflict_relative: number("conflictRelativeTolerance")?,
            conflict_scale_floor: number("conflictScaleFloor")?,
            replace_compatible,
            real_values,
            conflict_message: message("conflictMessage")?,
            legacy_message: message("legacyMessage")?,
            legacy_field_message: message("legacyFieldMessage")?,
            field_message: message("fieldMessage")?,
            invalid_weight_message: message("invalidWeightMessage")?,
            minimum_weight_magnitude: match value.get("minimumWeightMagnitude") {
                Some(_) => number("minimumWeightMagnitude")?,
                None => 0.0,
            },
        })
    }

    fn diagnostic(template: &Option<String>, context: &str, default: String) -> String {
        template.as_ref().map_or(default, |value| value.replace("{context}", context))
    }

    pub(crate) fn require_shape(&self, dimension: usize, components: usize) -> Result<(), String> {
        if self.dimension != dimension || self.components != components {
            return Err(format!(
                "Essential form declares dimension {} and {} components; expected {dimension} and {components}.",
                self.dimension, self.components
            ));
        }
        Ok(())
    }

    /// Ensure the element adapter is applying the declared oriented trace;
    /// the element library is responsible for calculating its actual moments.
    pub(crate) fn require_tangential_moments(&self) -> Result<(), String> {
        if self.functional != Functional::OrientedMoment || self.trace != Trace::Tangential {
            return Err("Essential form must declare tangential oriented moments.".into());
        }
        Ok(())
    }

    /// Resolve a complete prescribed field at a nodal point. A scalar legacy
    /// value can either directly prescribe a scalar or be accepted only when
    /// numerically zero, as declared by the scenario.
    pub(crate) fn point_values(
        &self,
        legacy: C,
        field: Option<&[C]>,
        context: &str,
    ) -> Result<Vec<C>, String> {
        if self.functional != Functional::Point {
            return Err(
                "A moment essential form cannot be evaluated as a nodal point value.".into(),
            );
        }
        self.resolve_values(legacy, field, context)
    }

    /// Validate Cartesian data for a moment trace. The returned field is still
    /// pointwise; an oriented finite-element functional must integrate it.
    pub(crate) fn moment_values(
        &self,
        legacy: C,
        field: Option<&[C]>,
        context: &str,
    ) -> Result<Vec<C>, String> {
        if self.functional != Functional::OrientedMoment {
            return Err("A point essential form cannot be evaluated as an oriented moment field.".into());
        }
        self.resolve_values(legacy, field, context)
    }

    fn resolve_values(
        &self,
        legacy: C,
        field: Option<&[C]>,
        context: &str,
    ) -> Result<Vec<C>, String> {
        if !legacy.re.is_finite() || !legacy.im.is_finite() {
            let template = if field.is_some() && self.legacy_field_message.is_some() {
                &self.legacy_field_message
            } else { &self.legacy_message };
            return Err(Self::diagnostic(template, context,
                format!("{context}: essential scalar value must be finite.")));
        }
        if self.real_values && legacy.im != 0.0 {
            return Err(format!("{context}: essential scalar value must be real."));
        }
        if matches!(self.legacy, LegacyInput::ZeroOnly)
            && legacy.re.hypot(legacy.im) > self.zero_tolerance
        {
            let template = if field.is_some() && self.legacy_field_message.is_some() {
                &self.legacy_field_message
            } else { &self.legacy_message };
            return Err(Self::diagnostic(template, context,
                format!("{context}: essential scalar value must be zero; use fieldValues.")));
        }
        let values = match field {
            Some(field) if field.len() == self.components => field.to_vec(),
            Some(_) => {
                return Err(format!(
                    "{context}: fieldValues requires {} components.",
                    self.components
                ));
            }
            None => vec![legacy; self.components],
        };
        if values
            .iter()
            .any(|v| !v.re.is_finite() || !v.im.is_finite())
        {
            return Err(Self::diagnostic(&self.field_message, context,
                format!("{context}: prescribed essential field must be finite.")));
        }
        if self.real_values && values.iter().any(|v| v.im != 0.0) {
            return Err(format!(
                "{context}: prescribed essential field must be real."
            ));
        }
        Ok(values)
    }

    /// Insert a single prescribed DOF after removing its nonzero orientation or
    /// periodic weight (`local_value = weight * representative_value`). A
    /// caller imposing a moment supplies its already-integrated moment value.
    /// Other coupled linear relations need a separate constraint mechanism.
    pub(crate) fn insert_dof(
        &self,
        constraints: &mut HashMap<usize, C>,
        dof: usize,
        local_value: C,
        weight: C,
        context: &str,
    ) -> Result<(), String> {
        if [local_value.re, local_value.im, weight.re, weight.im]
            .iter()
            .any(|x| !x.is_finite())
            || (weight.re == 0.0 && weight.im == 0.0)
            || weight.re.hypot(weight.im) < self.minimum_weight_magnitude
        {
            return Err(Self::diagnostic(&self.invalid_weight_message, context,
                format!("{context}: invalid essential value or DOF weight.")));
        }
        let prescribed = if weight.re == 1.0 && weight.im == 0.0 {
            local_value
        } else {
            // Scale before squaring to reduce overflow for non-unit weights.
            let scale = weight.re.abs().max(weight.im.abs());
            let re = weight.re / scale;
            let im = weight.im / scale;
            let norm = re * re + im * im;
            let nr = local_value.re / scale;
            let ni = local_value.im / scale;
            C {
                re: (nr * re + ni * im) / norm,
                im: (ni * re - nr * im) / norm,
            }
        };
        if !prescribed.re.is_finite() || !prescribed.im.is_finite() {
            return Err(format!(
                "{context}: normalized essential value is not finite."
            ));
        }
        if let Some(previous) = constraints.get(&dof) {
            let scale = previous
                .re
                .hypot(previous.im)
                .max(prescribed.re.hypot(prescribed.im))
                .max(self.conflict_scale_floor);
            if (previous.re - prescribed.re).hypot(previous.im - prescribed.im)
                > self.conflict_relative * scale
            {
                return Err(Self::diagnostic(&self.conflict_message, context,
                    format!("Conflicting essential constraints at {context}.")));
            }
            if self.replace_compatible {
                constraints.insert(dof, prescribed);
            }
        } else {
            constraints.insert(dof, prescribed);
        }
        Ok(())
    }

    /// Insert a real DOF without converting the stationary real constraint
    /// set to complex values. Compatible duplicates obey the declared policy.
    pub(crate) fn insert_real(
        &self,
        constraints: &mut HashMap<usize, f64>,
        dof: usize,
        value: f64,
        context: &str,
    ) -> Result<(), String> {
        if !value.is_finite() {
            return Err(format!("{context}: essential value must be finite."));
        }
        if let Some(&previous) = constraints.get(&dof) {
            let scale = previous
                .abs()
                .max(value.abs())
                .max(self.conflict_scale_floor);
            if (previous - value).abs() > self.conflict_relative * scale {
                return Err(Self::diagnostic(&self.conflict_message, context,
                    format!("Conflicting essential constraints at {context}.")));
            }
            if self.replace_compatible {
                constraints.insert(dof, value);
            }
        } else {
            constraints.insert(dof, value);
        }
        Ok(())
    }
}

pub(crate) fn load_plan(
    profile: &str,
    formulation: &str,
    condition: &str,
) -> Result<EssentialFormPlan, String> {
    let package = crate::scenario_package::load_installed(profile)?;
    let form = package
        .get("essentialForms")
        .and_then(|v| v.get(formulation))
        .and_then(|v| v.get(condition))
        .ok_or_else(|| {
            format!("Scenario {profile:?} has no {condition:?} essential form for {formulation:?}.")
        })?;
    EssentialFormPlan::from_json(form).map_err(|e| {
        format!("Scenario {profile:?} essential form {formulation:?}/{condition:?}: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn plan(components: usize, legacy: &str, functional: &str) -> EssentialFormPlan {
        EssentialFormPlan::from_json(
            &json!({"dimension":2,"fieldComponents":components,"trace":"value",
            "functional":functional,"legacyInput":legacy,"legacyZeroTolerance":1e-14,
            "conflictRelativeTolerance":1e-9,"conflictScaleFloor":1e-12}),
        )
        .unwrap()
    }

    #[test]
    fn scalar_and_complete_vector_inputs_preserve_zero_compatibility() {
        let scalar = plan(1, "scalar", "point");
        assert_eq!(
            scalar
                .point_values(C { re: 3.0, im: 2.0 }, None, "p")
                .unwrap()[0]
                .re,
            3.0
        );
        let vector = plan(2, "zeroOnly", "point");
        let fields = [C { re: 1.0, im: -2.0 }, C { re: 3.0, im: 4.0 }];
        assert_eq!(
            vector
                .point_values(C::default(), Some(&fields), "u")
                .unwrap()[1]
                .im,
            4.0
        );
        assert!(
            vector
                .point_values(C { re: 1.0, im: 0.0 }, Some(&fields), "u")
                .is_err()
        );
        assert!(
            vector
                .point_values(C::default(), Some(&fields[..1]), "u")
                .is_err()
        );
        assert_eq!(
            vector.point_values(C::default(), None, "u").unwrap().len(),
            2
        );
        // Existing vector solvers accept a tiny legacy scalar as zero for
        // validation, then use that value for each absent field component.
        let tiny = C {
            re: 1e-15,
            im: -2e-15,
        };
        for component in vector.point_values(tiny, None, "u").unwrap() {
            assert_eq!(component.re, tiny.re);
            assert_eq!(component.im, tiny.im);
        }
    }

    #[test]
    fn oriented_and_periodic_constraints_normalize_and_detect_conflicts() {
        let plan = plan(2, "zeroOnly", "orientedMoment");
        let mut values = HashMap::new();
        plan.insert_dof(
            &mut values,
            4,
            C { re: 2.0, im: 3.0 },
            C { re: -1.0, im: 0.0 },
            "edge",
        )
        .unwrap();
        let normalized = values[&4];
        assert!((normalized.re + 2.0).abs() < 1e-12 && (normalized.im + 3.0).abs() < 1e-12);
        plan.insert_dof(
            &mut values,
            4,
            C { re: 3.0, im: -2.0 },
            C { re: 0.0, im: 1.0 },
            "edge",
        )
        .unwrap();
        assert!(
            plan.insert_dof(
                &mut values,
                4,
                C { re: 1.0, im: 0.0 },
                C { re: 1.0, im: 0.0 },
                "edge"
            )
            .is_err()
        );
        assert!(
            plan.insert_dof(&mut values, 5, C::default(), C::default(), "edge")
                .is_err()
        );
        assert!(plan.point_values(C::default(), None, "edge").is_err());
    }

    #[test]
    fn real_scalar_rejects_complex_input_and_replaces_compatible_duplicates() {
        let plan = EssentialFormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,
            "trace":"value","functional":"point","legacyInput":"scalar","valueType":"real",
            "legacyZeroTolerance":0,"conflictRelativeTolerance":1e-10,
            "conflictScaleFloor":1,"duplicatePolicy":"last"}))
        .unwrap();
        assert!(
            plan.point_values(C { re: 300.0, im: 1.0 }, None, "value")
                .is_err()
        );
        let mut constraints = HashMap::new();
        plan.insert_real(&mut constraints, 2, 300.0, "node")
            .unwrap();
        plan.insert_real(&mut constraints, 2, 300.0 + 1e-9, "node")
            .unwrap();
        assert_eq!(constraints[&2], 300.0 + 1e-9);
        assert!(
            plan.insert_real(&mut constraints, 2, 301.0, "node")
                .is_err()
        );
        assert_eq!(constraints[&2], 300.0 + 1e-9);
    }
}
