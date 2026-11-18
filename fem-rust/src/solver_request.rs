use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Shared input for single-frequency scenario runners. The scenario continues
/// to own mesh format, field/formulation and boundary-specific validation.
pub(crate) struct FrequencyInput {
    pub(crate) request: Value,
    pub(crate) model: Value,
    pub(crate) map: Value,
    pub(crate) mesh_path: PathBuf,
    pub(crate) output_directory: PathBuf,
    pub(crate) frequency: f64,
}

struct FrequencyPaths {
    model: PathBuf,
    mesh: PathBuf,
    map: PathBuf,
    output: PathBuf,
}

impl FrequencyPaths {
    fn from_request(request: &Value, base: &Path) -> Result<Self, String> {
        let resolve = |key: &str| -> Result<PathBuf, String> {
            let value = request
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("request.{key} is required."))?;
            let path = PathBuf::from(value);
            Ok(if path.is_absolute() {
                path
            } else {
                base.join(path)
            })
        };
        Ok(Self {
            model: resolve("modelFile")?,
            mesh: resolve("meshFile")?,
            map: resolve("modelMapFile")?,
            output: resolve("outputDirectory")?,
        })
    }
}

impl FrequencyInput {
    pub(crate) fn load(
        request_path: &Path,
        solver: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let request: Value = serde_json::from_slice(&fs::read(request_path)?)?;
        validate_single_case_request(&request, solver)
            .map_err(|error| format!("Solver request: {error}"))?;
        let paths = FrequencyPaths::from_request(
            &request,
            request_path.parent().unwrap_or(Path::new(".")),
        )?;
        let model: Value = serde_json::from_slice(&fs::read(&paths.model)?)?;
        validate_study_reference(&model, &request)
            .map_err(|error| format!("Solver request: {error}"))?;
        let frequency =
            single_case_frequency(&request).map_err(|error| format!("Solver request: {error}"))?;
        let map: Value = serde_json::from_slice(&fs::read(&paths.map)?)?;
        Ok(Self {
            request,
            model,
            map,
            mesh_path: paths.mesh,
            output_directory: paths.output,
            frequency,
        })
    }
}

pub(crate) fn validate_request(request: &Value, expected_solver: &str) -> Result<(), String> {
    let root_schema = crate::model_schema::root()?;
    let _request_schema = root_schema
        .pointer("/$defs/solverRequest")
        .ok_or_else(|| "The FEM model schema has no solver request definition.".to_string())?;
    let errors = crate::model_schema::context()?.validate_at(request, "/$defs/solverRequest")?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn frequency_paths_resolve_relative_to_request_and_preserve_absolute_paths() {
        let base = std::env::current_dir().unwrap().join("fixture-run");
        let absolute_mesh = base.join("other").join("mesh.msh");
        let request = json!({
            "modelFile": "model.json", "meshFile": absolute_mesh,
            "modelMapFile": "meshes/map.json", "outputDirectory": "outputs"
        });
        let paths = FrequencyPaths::from_request(&request, &base).unwrap();
        assert_eq!(paths.model, base.join("model.json"));
        assert_eq!(paths.mesh, absolute_mesh);
        assert_eq!(paths.map, base.join("meshes").join("map.json"));
        assert_eq!(paths.output, base.join("outputs"));
        let error = FrequencyPaths::from_request(&json!({"modelFile":"ok"}), &base)
            .err()
            .unwrap();
        assert_eq!(error, "request.meshFile is required.");
    }

    #[test]
    fn frequency_and_study_checks_accept_only_the_selected_case() {
        let model = json!({"model":{"study":[{"id":"study-a"}]}});
        let request = json!({"studyId":"study-a", "frequencyCases":[{"frequencyHz":12.5}]});
        assert_eq!(single_case_frequency(&request).unwrap(), 12.5);
        validate_study_reference(&model, &request).unwrap();
        assert!(single_case_frequency(&json!({"frequencyCases":[]})).is_err());
        assert!(single_case_frequency(&json!({"frequencyCases":[{"frequencyHz":0}]})).is_err());
        assert!(validate_study_reference(&model, &json!({"studyId":"missing"})).is_err());
    }
}
