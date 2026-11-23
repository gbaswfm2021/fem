//! Normalize a single solver invocation without selecting a physics profile.
//!
//! The coordinator remains responsible for splitting frequency sweeps,
//! staging files and maintaining run records. This module does not run
//! preflight, parse meshes, create output directories or publish artifacts.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CaseKind {
    Stationary,
    Frequency { frequency_hz: f64, case: Value },
}



#[derive(Clone, Debug)]
pub(crate) enum Output {
    Stationary { csv: String, field_files: bool, field_filename: Option<String> },
    Frequency { sweep_csv: String },
}

#[derive(Clone, Debug)]
pub(crate) struct SolverSettings {
    pub(crate) formulation: String,
    pub(crate) element_order: u64,
    pub(crate) quadrature: String,
    pub(crate) backend: String,
    pub(crate) ordering: String,
    pub(crate) threads: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedCase {
    pub(crate) request: Value,
    pub(crate) profile: String,
    pub(crate) study_id: String,
    pub(crate) kind: CaseKind,
    pub(crate) settings: SolverSettings,
    pub(crate) output: Output,
    pub(crate) model_path: PathBuf,
    pub(crate) mesh_path: PathBuf,
    pub(crate) map_path: PathBuf,
    pub(crate) output_directory: PathBuf,
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value.get(key).and_then(Value::as_str).filter(|s| !s.is_empty())
        .ok_or_else(|| format!("request.{key} is required."))
}

fn safe_filename(name: &str, extension: &str) -> bool {
    name.len() <= 120 && name.ends_with(extension) && !name.starts_with('.')
        && name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

impl PreparedCase {
    /// `preparation` is the selected package's preparation declaration, e.g.
    /// `{ "kind": "frequencyPlan" }`; profile is an identity check, not a
    /// numerical dispatch key. Frequency envelopes must already contain one case.
    pub(crate) fn from_request(
        request: Value, preparation: &Value, profile: &str, base: &Path,
    ) -> Result<Self, String> {
        let kind = preparation.get("kind").and_then(Value::as_str)
            .ok_or("Scenario preparation must declare kind.")?;
        if kind != "stationaryPlan" && kind != "frequencyPlan" {
            return Err(format!("Preparation kind {kind:?} is not supported by PreparedCase; configured frequency requests require their existing adapter."));
        }
        // A configured-frequency request must not be mistaken for a common
        // frequencyPlan merely because both happen to contain a frequency.
        if request.get("modelConfiguration").is_some() || request.get("meshMap").is_some() {
            return Err("Configured frequency request shape is not supported by PreparedCase.".into());
        }
        let case_kind = if kind == "frequencyPlan" {
            crate::solver_request::validate_single_case_request(&request, profile)?;
            let frequency_hz = crate::solver_request::single_case_frequency(&request)?;
            CaseKind::Frequency { frequency_hz, case: request["frequencyCases"][0].clone() }
        } else {
            let allowed = ["schema", "solver", "studyId", "fieldFormulation", "elementOrder",
                "constitutiveQuadrature", "solverBackend", "solverOrdering", "solverThreads",
                "csvFilename", "fieldFiles", "fieldFilename", "modelFile", "meshFile",
                "modelMapFile", "outputDirectory"];
            let object = request.as_object().ok_or("Stationary solver request must be an object.")?;
            for key in object.keys() {
                if !allowed.contains(&key.as_str()) {
                    return Err(format!("Unsupported stationary solver request property {key:?}."));
                }
            }
            if string(&request, "schema")? != "fem-solver-request/1" || string(&request, "solver")? != profile {
                return Err("Stationary request schema or solver identity is invalid.".into());
            }
            CaseKind::Stationary
        };
        let formulation = string(&request, "fieldFormulation")?.to_owned();
        let element_order = request.get("elementOrder").and_then(Value::as_u64)
            .ok_or("request.elementOrder must be an unsigned integer.")?;
        let quadrature = string(&request, "constitutiveQuadrature")?.to_owned();
        let backend = string(&request, "solverBackend")?.to_owned();
        let ordering = string(&request, "solverOrdering")?.to_owned();
        let threads = request.get("solverThreads").and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or("request.solverThreads must be an unsigned integer.")?;
        let settings = SolverSettings { formulation, element_order, quadrature, backend, ordering, threads };

        if kind == "stationaryPlan" {
            let package = crate::scenario_package::load_installed(profile)?;
            let weak = package.get("weakForms").and_then(|forms| forms.get(&settings.formulation))
                .ok_or("The stationary formulation is not declared by the scenario.")?;
            if !weak.pointer("/element/orders").and_then(Value::as_array)
                .is_some_and(|orders| orders.iter().any(|value| value.as_u64() == Some(settings.element_order))) {
                return Err("The stationary element order is not declared by the scenario.".into());
            }
            let capabilities = crate::model_schema::engine_capabilities()?;
            let study = capabilities.get("study").ok_or("Study capabilities are missing.")?;
            let profile_settings = study.get("solverSettingsByProfile").and_then(|v| v.get(profile))
                .ok_or("Scenario solver settings are missing.")?;
            let options = json!({"backend":settings.backend,"ordering":settings.ordering,
                "threads":settings.threads,"constitutiveQuadrature":settings.quadrature});
            crate::solver_options::resolve(study, profile_settings, &options, &settings.formulation, settings.element_order)?;
        } else {
            let package = crate::scenario_package::load_installed(profile)?;
            let capabilities = crate::model_schema::engine_capabilities()?;
            let study = capabilities.get("study").ok_or("Study capabilities are missing.")?;
            let profile_settings = study.get("solverSettingsByProfile").and_then(|v|v.get(profile))
                .ok_or("Scenario solver settings are missing.")?;
            // Check the requested rule against common options using valid
            // declared defaults for unrelated solver fields. Their legacy
            // direct-request acceptance is not changed by this check.
            crate::solver_options::resolve(study,profile_settings,
                &json!({"constitutiveQuadrature":settings.quadrature}),&settings.formulation,settings.element_order)?;
            if let Some(vector)=package.get("preparedVector") {
                let allowed=vector.get("supportedQuadrature").and_then(Value::as_array).filter(|rows|!rows.is_empty())
                    .ok_or("Prepared vector supportedQuadrature must be a nonempty array.")?;
                let mut names=std::collections::HashSet::new();
                for row in allowed {
                    let name=row.as_str().filter(|value|!value.is_empty()).ok_or("Prepared vector quadrature names must be nonempty strings.")?;
                    if !names.insert(name) {return Err("Prepared vector quadrature names must be unique.".into());}
                }
                if !names.contains(settings.quadrature.as_str()) {
                    return Err(format!("Constitutive quadrature {:?} is not supported by this vector formulation.",settings.quadrature));
                }
            }
        }
        let output = if kind == "stationaryPlan" {
            let csv = string(&request, "csvFilename")?;
            if !safe_filename(csv, ".csv") { return Err("request.csvFilename must be one safe .csv filename.".into()); }
            let field_files = request.get("fieldFiles").and_then(Value::as_bool)
                .ok_or("request.fieldFiles must be a boolean.")?;
            let filename = request.get("fieldFilename").and_then(Value::as_str)
                .filter(|name| safe_filename(name, ".json")).map(str::to_string);
            if field_files && filename.is_none() { return Err("request.fieldFilename is required when fieldFiles is true.".into()); }
            Output::Stationary { csv: csv.into(), field_files, field_filename: filename }
        } else {
            Output::Frequency { sweep_csv: string(&request, "sweepCsv")?.into() }
        };
        let path = |key: &str| -> Result<PathBuf, String> {
            let value = string(&request, key)?;
            if kind == "stationaryPlan" && (value.chars().count() > 4096 || value.contains('\0')) {
                return Err(format!("request.{key} is not a valid file reference."));
            }
            let path = PathBuf::from(value);
            Ok(if path.is_absolute() { path } else { base.join(path) })
        };
        let model_path = path("modelFile")?;
        let mesh_path = path("meshFile")?;
        let map_path = path("modelMapFile")?;
        let output_directory = path("outputDirectory")?;
        let study_id = string(&request, "studyId")?.to_string();
        Ok(Self { request, profile: profile.into(), study_id, kind: case_kind, settings, output,
            model_path, mesh_path, map_path, output_directory })
    }
}
