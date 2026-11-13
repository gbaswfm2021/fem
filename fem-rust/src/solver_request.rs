use serde_json::Value;

pub(crate) fn validate_request(request: &Value, expected_solver: &str) -> Result<(), String> {
    let root_schema = crate::model_schema::root()?;
    let request_schema = root_schema
        .pointer("/$defs/solverRequest")
        .ok_or_else(|| "The FEM model schema has no solver request definition.".to_string())?;
    let errors = crate::json_schema::validate_with_root(request, request_schema, root_schema);
    if !errors.is_empty() {
        return Err(format!("Invalid solver request: {}", errors.join(" ")));
    }
    let solver = request
        .get("solver")
        .and_then(Value::as_str)
        .ok_or_else(|| "request.solver is required.".to_string())?;
    if solver != expected_solver {
        return Err(format!(
            "request.solver must be {expected_solver:?}; received {solver:?}."
        ));
    }
    Ok(())
}

pub(crate) fn validate_single_case_request(
    request: &Value,
    expected_solver: &str,
) -> Result<(), String> {
    validate_request(request, expected_solver)?;
    let cases = request
        .get("frequencyCases")
        .and_then(Value::as_array)
        .ok_or_else(|| "request.frequencyCases must be an array.".to_string())?;
    if cases.len() != 1 {
        return Err(format!(
            "The {expected_solver} scenario solver accepts exactly one frequency case; received {}.",
            cases.len()
        ));
    }
    Ok(())
}

pub(crate) fn single_case_frequency(request: &Value) -> Result<f64, String> {
    let frequency_cases = request
        .get("frequencyCases")
        .and_then(Value::as_array)
        .ok_or_else(|| "request.frequencyCases must be an array.".to_string())?;
    if frequency_cases.len() != 1 {
        return Err(format!(
            "request.frequencyCases must contain exactly one frequency; received {}.",
            frequency_cases.len()
        ));
    }
    frequency_cases[0]
        .get("frequencyHz")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite() && *value > 0.0)
        .ok_or_else(|| {
            "request.frequencyCases[0].frequencyHz must be positive and finite.".to_string()
        })
}

pub(crate) fn validate_study_reference(model: &Value, request: &Value) -> Result<(), String> {
    let study_id = request
        .get("studyId")
        .and_then(Value::as_str)
        .ok_or_else(|| "request.studyId is required.".to_string())?;
    let studies = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .ok_or_else(|| "model.study must be an array.".to_string())?;
    if !studies
        .iter()
        .any(|study| study.get("id").and_then(Value::as_str) == Some(study_id))
    {
        return Err(format!(
            "request.studyId {study_id:?} is not present in model.study."
        ));
    }
    Ok(())
}
