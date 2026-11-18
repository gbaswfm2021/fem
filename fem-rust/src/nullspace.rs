//! Declared nullspace treatment for compatible, unanchored components.
use serde_json::Value;

pub(crate) struct MeanConstraintPlan {
    pub(crate) balance_relative_tolerance: f64,
    pub(crate) phase_tolerance: f64,
    pub(crate) reference_property: String,
    pub(crate) reference_unit: String,
    pub(crate) reference_default: f64,
}

impl MeanConstraintPlan {
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        if value.get("kind").and_then(Value::as_str) != Some("componentMean") {
            return Err("Nullspace kind must be componentMean.".into());
        }
        let tolerance = |name: &str| {
            value
                .get(name)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(|| format!("Nullspace {name} must be finite and nonnegative."))
        };
        let reference = value
            .get("reference")
            .and_then(Value::as_object)
            .ok_or("Nullspace reference must be an object.")?;
        let string = |name: &str| {
            reference
                .get(name)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("Nullspace reference.{name} must be a nonempty string."))
        };
        let default = reference
            .get("default")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .ok_or("Nullspace reference.default must be finite.")?;
        Ok(Self {
            balance_relative_tolerance: tolerance("balanceRelativeTolerance")?,
            phase_tolerance: tolerance("phaseTolerance")?,
            reference_property: string("studyProperty")?,
            reference_unit: string("unit")?,
            reference_default: default,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn validates_mean_reference_and_tolerances() {
        let mut declaration = json!({"kind":"componentMean", "balanceRelativeTolerance":1e-10,
            "phaseTolerance":1e-12, "reference":{"studyProperty":"reference", "unit":"K", "default":0}});
        let plan = MeanConstraintPlan::from_json(&declaration).unwrap();
        assert_eq!(plan.reference_property, "reference");
        assert_eq!(plan.reference_unit, "K");
        assert_eq!(plan.reference_default, 0.0);
        assert_eq!(plan.phase_tolerance, 1e-12);
        assert_eq!(plan.balance_relative_tolerance, 1e-10);
        declaration["balanceRelativeTolerance"] = json!(-1);
        assert!(MeanConstraintPlan::from_json(&declaration).is_err());
        declaration["kind"] = json!("unknown");
        assert!(MeanConstraintPlan::from_json(&declaration).is_err());
    }
}
