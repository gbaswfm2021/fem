//! Validate time-grid quantities for the shared transient preparation contract.
//! This internal parser does not itself enable a user-facing Study.
use crate::model_expression::{self, Scalar};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug)]
pub(crate) struct TimeGrid {
    pub(crate) start: f64,
    pub(crate) end: f64,
    pub(crate) max_step: f64,
    pub(crate) max_steps: usize,
    pub(crate) output_times: Vec<f64>,
}

pub(crate) struct PreparedTransientCase {
    pub(crate) grid: TimeGrid,
    pub(crate) parameters: HashMap<String, Scalar>,
    pub(crate) overrides: Vec<Value>,
    pub(crate) longitudinal_wavenumber: crate::solver_common::C,
}

impl PreparedTransientCase {
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let grid = TimeGrid::from_prepared(
            value
                .get("time")
                .ok_or("Prepared transient case requires time.")?,
        )?;
        let values = value
            .get("globalParameters")
            .and_then(Value::as_object)
            .ok_or("Prepared transient globalParameters must be an object.")?;
        let dimensions = value
            .get("globalParameterDimensions")
            .and_then(Value::as_object)
            .ok_or("Prepared transient globalParameterDimensions must be an object.")?;
        if values.len() != dimensions.len() || values.contains_key("t") {
            return Err(
                "Prepared transient parameters and dimensions differ or conflict with time.".into(),
            );
        }
        let mut parameters = HashMap::with_capacity(values.len());
        for (name, value) in values {
            let number = value
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("Prepared transient parameter {name:?} must be finite."))?;
            let dims = dimensions
                .get(name)
                .and_then(Value::as_array)
                .filter(|v| v.len() == 5)
                .ok_or_else(|| {
                    format!("Prepared transient parameter {name:?} requires five dimensions.")
                })?;
            let mut dimension = [0.; 5];
            for (axis, value) in dims.iter().enumerate() {
                dimension[axis] = value.as_f64().filter(|v| v.is_finite()).ok_or_else(|| {
                    format!("Prepared transient parameter {name:?} dimensions must be finite.")
                })?;
            }
            parameters.insert(
                name.clone(),
                Scalar {
                    value: number,
                    dimension,
                },
            );
        }
        let overrides = value
            .get("parameterOverrides")
            .and_then(Value::as_array)
            .ok_or("Prepared transient parameterOverrides must be an array.")?
            .clone();
        let longitudinal_wavenumber = if let Some(wavenumber) = value.get("longitudinalWavenumber") {
            let real = wavenumber.get("real").and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .ok_or("Prepared transient longitudinal wavenumber real part must be finite.")?;
            let imaginary = wavenumber.get("imaginary").and_then(Value::as_f64)
                .filter(|value| value.is_finite())
                .ok_or("Prepared transient longitudinal wavenumber imaginary part must be finite.")?;
            crate::longitudinal::validate_prepared_case_value(
                &serde_json::json!({"longitudinalWavenumber":wavenumber}),
                crate::solver_common::C { re: real, im: imaginary },
            )?;
            if imaginary != 0.0 {
                return Err("Prepared transient longitudinal wavenumber must be real.".into());
            }
            crate::solver_common::C { re: real, im: 0.0 }
        } else {
            crate::solver_common::C::default()
        };
        Ok(Self {
            grid,
            parameters,
            overrides,
            longitudinal_wavenumber,
        })
    }
}
fn seconds(value: &Value, symbols: &HashMap<String, Scalar>, path: &str) -> Result<f64, String> {
    let unit = value
        .get("unit")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{path}.unit is required."))?;
    let dimension = model_expression::unit(unit)?.1;
    let seconds_dimension = model_expression::unit("s")?.1;
    if dimension
        .iter()
        .zip(seconds_dimension)
        .any(|(a, b)| (a - b).abs() > 1e-10)
    {
        return Err(format!("{path} must have time units."));
    }
    let result = model_expression::evaluate_quantity(value, symbols, path)?;
    if !result.is_finite() {
        return Err(format!("{path} must be finite."));
    }
    Ok(result)
}
impl TimeGrid {
    fn validated(
        start: f64,
        end: f64,
        max_step: f64,
        max_steps: usize,
        output_times: Vec<f64>,
    ) -> Result<Self, String> {
        if !start.is_finite()
            || !end.is_finite()
            || !max_step.is_finite()
            || end <= start
            || max_step <= 0.
            || max_steps == 0
        {
            return Err("Transient interval and maximum step must be finite and positive.".into());
        }
        if output_times.len() > max_steps.saturating_add(1) {
            return Err("Transient output count exceeds the step limit plus initial state.".into());
        }
        if output_times
            .iter()
            .any(|t| !t.is_finite() || *t < start || *t > end)
            || output_times.windows(2).any(|v| v[0] >= v[1])
        {
            return Err(
                "Transient output times must be strictly increasing within the study interval."
                    .into(),
            );
        }
        // Output samples are exact step endpoints. Account for the extra
        // subdivisions they can introduce, rather than failing late in a run.
        let mut previous = start;
        let mut minimum_steps = 0.0_f64;
        for end_point in output_times.iter().copied().chain(std::iter::once(end)) {
            let ratio = (end_point - previous) / max_step;
            if !ratio.is_finite() {
                return Err("Transient time interval requires too many steps.".into());
            }
            let tolerance = 16.0 * f64::EPSILON * ratio.abs().max(1.0);
            if end_point > previous {
                minimum_steps += (ratio - tolerance).max(0.).ceil().max(1.);
            }
            previous = end_point;
        }
        if minimum_steps > max_steps as f64 {
            return Err(
                "Transient maxSteps is too small for the interval and requested output times."
                    .into(),
            );
        }
        Ok(Self {
            start,
            end,
            max_step,
            max_steps,
            output_times,
        })
    }

    /// Validate the SI-valued time section of one prepared sweep case.
    pub(crate) fn from_prepared(value: &Value) -> Result<Self, String> {
        let number = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_f64)
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("Prepared transient time {key} must be finite."))
        };
        let start = number("startSeconds")?;
        let end = number("endSeconds")?;
        let max_step = number("maxStepSeconds")?;
        let max_steps = value
            .get("maxSteps")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or("Prepared transient maxSteps must be a positive integer.")?;
        let output_times = value
            .get("outputTimesSeconds")
            .and_then(Value::as_array)
            .ok_or("Prepared transient outputTimesSeconds must be an array.")?
            .iter()
            .map(|v| {
                v.as_f64()
                    .filter(|v| v.is_finite())
                    .ok_or("Prepared transient output time must be finite.".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::validated(start, end, max_step, max_steps, output_times)
    }
    pub(crate) fn from_json(
        value: &Value,
        parameters: &HashMap<String, Scalar>,
    ) -> Result<Self, String> {
        if parameters.contains_key("t") {
            return Err("Parameter symbol t conflicts with transient time.".into());
        }
        let get = |key: &str| {
            value
                .get(key)
                .ok_or_else(|| format!("Transient time grid requires {key}."))
        };
        let start = seconds(get("startTime")?, parameters, "transient.startTime")?;
        let end = seconds(get("endTime")?, parameters, "transient.endTime")?;
        let max_step = seconds(get("maxStep")?, parameters, "transient.maxStep")?;
        let max_steps = get("maxSteps")?
            .as_u64()
            .and_then(|v| usize::try_from(v).ok())
            .filter(|v| *v > 0)
            .ok_or("Transient maxSteps must be a positive integer.")?;
        let times = get("outputTimes")?
            .as_array()
            .ok_or("Transient outputTimes must be an array.")?;
        let output_times = times
            .iter()
            .enumerate()
            .map(|(i, v)| seconds(v, parameters, &format!("transient.outputTimes[{i}]")))
            .collect::<Result<Vec<_>, _>>()?;
        Self::validated(start, end, max_step, max_steps, output_times)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn q(expression: &str, unit: &str) -> Value {
        json!({"expression":expression,"unit":unit})
    }
    #[test]
    fn validates_dimensioned_time_schedule_and_parameter_conflicts() {
        let mut value = json!({"startTime":q("0","s"),"endTime":q("2","s"),"maxStep":q("0.5","s"),
            "maxSteps":10,"outputTimes":[q("0","s"),q("1","s"),q("2","s")]});
        let grid = TimeGrid::from_json(&value, &HashMap::new()).unwrap();
        assert_eq!(
            (grid.start, grid.end, grid.max_step, grid.max_steps),
            (0., 2., 0.5, 10)
        );
        assert_eq!(grid.output_times, vec![0., 1., 2.]);
        value["maxStep"] = q("1", "m");
        assert!(
            TimeGrid::from_json(&value, &HashMap::new())
                .unwrap_err()
                .contains("time units")
        );
    }
    #[test]
    fn time_grid_is_resolved_for_each_parameter_case() {
        let value = json!({"startTime":q("0","s"),"endTime":q("duration","s"),"maxStep":q("duration/4","s"),
            "maxSteps":8,"outputTimes":[q("0","s"),q("duration","s")]});
        for end in [2.0, 4.0] {
            let symbols = HashMap::from([(
                "duration".into(),
                Scalar {
                    value: end,
                    dimension: model_expression::unit("s").unwrap().1,
                },
            )]);
            let grid = TimeGrid::from_json(&value, &symbols).unwrap();
            assert_eq!((grid.end, grid.max_step), (end, end / 4.));
        }
    }
    #[test]
    fn rejects_step_budget_that_cannot_reach_all_output_times() {
        let value = json!({"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":0.6,"maxSteps":2,"outputTimesSeconds":[0.4,0.8]});
        assert!(
            TimeGrid::from_prepared(&value)
                .unwrap_err()
                .contains("maxSteps")
        );
        let valid = json!({"startSeconds":0.,"endSeconds":0.3,"maxStepSeconds":0.1,"maxSteps":3,"outputTimesSeconds":[0.3]});
        assert!(TimeGrid::from_prepared(&valid).is_ok());
    }
}
