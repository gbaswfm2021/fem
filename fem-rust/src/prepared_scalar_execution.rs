//! File-based entry point for the prepared scalar stationary runner.
//!
//! This is not registered in production dispatch until end-to-end parity and
//! lifecycle behavior are independently verified.
use std::{fs, path::Path};
use serde_json::Value;
use crate::prepared_case::{Output, PreparedCase};
use crate::prepared_scalar::{self, Mesh};
use crate::SolverCache;

pub(crate) fn run_request(request_path: &Path, expected_profile: &str) -> Result<(), String> {
    run_request_with_preflight(request_path, expected_profile, &crate::preflight_client::validate)
}

fn run_request_with_preflight(request_path: &Path, expected_profile: &str, preflight: &dyn Fn(&Value) -> Result<(), String>) -> Result<(), String> {
    let request: Value = serde_json::from_slice(&fs::read(request_path).map_err(|e| format!("Could not read solver request: {e}"))?)
        .map_err(|e| format!("Could not parse solver request: {e}"))?;
    let profile = request.get("solver").and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or("request.solver is required.")?;
    if profile != expected_profile { return Err(format!("request.solver {profile:?} does not match the selected scenario {expected_profile:?}.")); }
    let package = crate::scenario_package::load_installed(profile)?;
    let preparation = package.pointer("/schemaDocument/x-fem-capabilities/study/preparationByProfile")
        .and_then(|v| v.get(profile)).ok_or("Scenario preparation declaration is missing.")?;
    let case = PreparedCase::from_request(request.clone(), preparation, profile, request_path.parent().unwrap_or(Path::new(".")))?;
    let Output::Stationary { csv, field_files, field_filename } = &case.output else { return Err("Prepared scalar execution requires a stationary request.".into()); };
    let model: Value = serde_json::from_slice(&fs::read(&case.model_path).map_err(|e| format!("Could not read model: {e}"))?)
        .map_err(|e| format!("Could not parse model: {e}"))?;
    preflight(&model).map_err(|e| format!("Model validation:\n{e}"))?;
    if model.pointer("/scenario/profileId").and_then(Value::as_str) != Some(profile) {
        return Err("Model scenario profile does not match the selected solver.".into());
    }
    crate::solver_request::validate_study_reference(&model, &case.request)?;
    let symbols = resolve_model_parameters(&package, &model)?;
    let map: Value = serde_json::from_slice(&fs::read(&case.map_path).map_err(|e| format!("Could not read mesh map: {e}"))?)
        .map_err(|e| format!("Could not parse mesh map: {e}"))?;
    let mesh_text = fs::read_to_string(&case.mesh_path).map_err(|e| format!("Could not read mesh: {e}"))?;
    let format = package.pointer("/preparedScalar/meshFormat").and_then(Value::as_str).ok_or("preparedScalar.meshFormat is required.")?;
    let mut cache = SolverCache::default();
    let result = match format {
        "fullmeta-v2" => {
            check_dense_2d_records(&mesh_text)?;
            let mesh = crate::mesh2d::parse_mesh_v2(&mesh_text)?;
            crate::solver_common::stage_started("assembly");
            prepared_scalar::solve_with_result(&case, &package, &model, &map,
                Mesh::Triangle { nodes:&mesh.nodes, cells:&mesh.triangles, facets:&mesh.boundary_edges }, &symbols, &mut cache)?
        }
        "fullmeta-v1-contiguous" => {
            let mesh = crate::mesh3d::parse_contiguous_v1(&mesh_text)?;
            crate::solver_common::stage_started("assembly");
            prepared_scalar::solve_with_result(&case, &package, &model, &map,
                Mesh::Tetrahedron { nodes:&mesh.nodes, cells:&mesh.tetrahedra, facets:&mesh.boundary_faces }, &symbols, &mut cache)?
        }
        other => return Err(format!("Unsupported prepared scalar mesh format {other:?}.")),
    };
    let plan = crate::scenario_plan::ScenarioPlan::from_package(&package, &case.settings.formulation)?;
    let result_plan = plan.result.as_ref().ok_or("Scenario result plan is required.")?;
    // Evaluate both result products before creating or changing any files.
    let document = result_plan.document(&result.data)?;
    let csv_data = result_plan.csv(std::slice::from_ref(&result.data))?;
    let completion = result_plan.completion(&result.data)?;
    fs::create_dir_all(&case.output_directory).map_err(|e| format!("Could not create output directory: {e}"))?;
    crate::solver_common::stage_started("output");
    if *field_files {
        let filename = field_filename.as_deref().ok_or("Field filename is required.")?;
        let data = serde_json::to_vec_pretty(&document).map_err(|e| format!("Could not encode result document: {e}"))?;
        fs::write(case.output_directory.join(filename), data).map_err(|e| format!("Could not write field output: {e}"))?;
    }
    fs::write(case.output_directory.join(csv), csv_data).map_err(|e| format!("Could not write CSV output: {e}"))?;
    if let Some(line) = completion { println!("{line}"); }
    Ok(())
}

fn resolve_model_parameters(package: &Value, model: &Value) -> Result<std::collections::HashMap<String, crate::model_expression::Scalar>, String> {
    if package.pointer("/preparedScalar/parameterResolver").and_then(Value::as_str) != Some("modelValues") {
        return Err("Unsupported prepared scalar parameter resolver.".into());
    }
    let capabilities = crate::model_schema::engine_capabilities()?;
    let allowed = capabilities.pointer("/expressions/modelValueFunctions").and_then(Value::as_array)
        .ok_or("Model expression functions are not declared.")?.iter().map(|value| value.as_str().map(str::to_string)
            .ok_or_else(|| "Model expression function names must be strings.".to_string())).collect::<Result<Vec<_>,_>>()?;
    let mut reserved = crate::model_schema::reserved_symbols_for_model(model)?;
    reserved.extend(["pi","e","i","x","y","z","freq","omega","t"].into_iter().map(str::to_string));
    let parameters = model.pointer("/model/parameters").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
    let resolved = fullmeta_core::quantity::resolve_parameters(parameters, &reserved, &allowed, "model.parameters")?;
    resolved.into_iter().map(|(key,value)| serde_json::from_value(value).map(|value| (key,value)).map_err(|e| e.to_string())).collect()
}

/// The underlying indexed parser allocates by record ID and historically
/// assumes dense edge records. Reject gaps and extreme IDs before allocation.
pub(crate) fn check_dense_2d_records(text: &str) -> Result<(), String> {
    let mut ids = std::collections::HashMap::<&str, std::collections::HashSet<usize>>::new();
    for line in text.lines() {
        let row = line.trim().split(',').collect::<Vec<_>>();
        let Some(kind) = row.first().copied().filter(|v| matches!(*v,"V" | "E" | "C" | "Q")) else { continue; };
        let minimum = match kind { "V" | "E" | "C" => 5, _ => 6 };
        if row.len() < minimum { continue; }
        let id = row[1].parse::<usize>().map_err(|_| "Invalid indexed mesh record ID.")?;
        ids.entry(kind).or_default().insert(id);
    }
    for (kind, ids) in ids {
        if ids.iter().any(|id| *id >= ids.len()) { return Err(format!("Indexed mesh {kind} records are incomplete.")); }
    }
    Ok(())
}


