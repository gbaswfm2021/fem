//! File and sequence orchestration for declared scalar and vector P1 frequency cases.
//! Numerical work and result projection remain in shared, profile-neutral modules.
use crate::prepared_case::{Output, PreparedCase};
use crate::prepared_frequency_output;
use crate::prepared_frequency_scalar;
use crate::result_contract::{CsvVariable, ResultData, ResultPlan};
use crate::SolverCache;
use serde_json::Value;
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

struct ComputedCase {
    data: ResultData,
    plan: ResultPlan,
    output: PathBuf,
    field_filename: String,
    csv_filename: String,
    variable_names: Vec<String>,
    frequency: f64,
    field_files: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NumericalFamily { Scalar, Vector, Hcurl2d }

fn frequency_family_for_formulation(
    package: &Value,
    formulation: &str,
    has_nonzero_longitudinal_wavenumber: bool,
    fallback: NumericalFamily,
) -> Result<NumericalFamily, String> {
    let declaration = package
        .get("harmonicLongitudinalWavenumber")
        .and_then(|value| value.get("formulations"))
        .and_then(|value| value.get(formulation));

    let Some(declaration) = declaration else {
        return if has_nonzero_longitudinal_wavenumber {
            Err("Nonzero longitudinal wavenumber has no selected formulation contract.".into())
        } else {
            Ok(fallback)
        };
    };

    match declaration.get("numericalFamily").and_then(Value::as_str) {
        Some("deRhamTriangle") => Ok(NumericalFamily::Hcurl2d),
        Some("scalarSimplexP1") => Ok(NumericalFamily::Scalar),
        Some("vectorSimplexP1") => Ok(NumericalFamily::Vector),
        Some(other) => Err(format!(
            "Unsupported contract-declared longitudinal numerical family {other:?}."
        )),
        None => Ok(fallback),
    }
}

fn safe_case_csv(name: &str) -> bool {
    name.len() <= 120
        && name.ends_with(".csv")
        && (!name.starts_with('.') || name.starts_with(".fullmeta-case-"))
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn compute(
    request_path: &Path,
    profile: &str,
    cache: &mut SolverCache,
    preflight: &dyn Fn(&Value) -> Result<(), String>,
    family: NumericalFamily,
) -> Result<ComputedCase, String> {
    let input = crate::solver_request::FrequencyInput::load(request_path, profile)
        .map_err(|e| e.to_string())?;
    preflight(&input.model).map_err(|e| format!("Model validation:\n{e}"))?;
    if input
        .model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        != Some(profile)
    {
        return Err("Model scenario profile does not match the selected solver.".into());
    }
    let package = crate::scenario_package::load_installed(profile)?;
    let preparation = package
        .pointer("/schemaDocument/x-fem-capabilities/study/preparationByProfile")
        .and_then(|v| v.get(profile))
        .ok_or("Scenario preparation declaration is missing.")?;
    let case = PreparedCase::from_request(
        input.request.clone(),
        preparation,
        profile,
        request_path.parent().unwrap_or(Path::new(".")),
    )?;
    let Output::Frequency { sweep_csv } = &case.output else {
        return Err("Frequency scalar execution requires a frequency request.".into());
    };
    if !safe_case_csv(sweep_csv) {
        return Err("request.sweepCsv must be one safe .csv filename.".into());
    }
    let parameters = input
        .model
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let symbols = crate::model_expression::resolve_parameters(parameters, &input.model)?;
    let longitudinal_wavenumber=crate::longitudinal::prescribed_wavenumber(
        &package,&input.model,&case.study_id,&case.settings.formulation,&symbols)?;
    let family = frequency_family_for_formulation(
        &package,
        &case.settings.formulation,
        longitudinal_wavenumber != crate::C::default(),
        family,
    )?;
    let text =
        fs::read_to_string(&input.mesh_path).map_err(|e| format!("Could not read mesh: {e}"))?;
    let config = match family {
        NumericalFamily::Scalar => package.get("preparedFrequencyScalar").and_then(|v| v.get(&case.settings.formulation)),
        NumericalFamily::Vector => package.get("preparedVector"),
        NumericalFamily::Hcurl2d => package.get("preparedHcurl2d"),
    }.ok_or("Frequency numerical formulation policy is missing.")?;
    let format = config
        .get("meshFormat")
        .and_then(Value::as_str)
        .ok_or("Frequency scalar meshFormat is required.")?;
    let data = if matches!(family,NumericalFamily::Hcurl2d) {
        crate::solver_common::stage_started("assembly");
        match format{
            "fullmeta-v2"=>{
                crate::prepared_scalar_execution::check_dense_2d_records(&text)?;
                let mesh=crate::mesh2d::parse_mesh_v2(&text)?;
                crate::prepared_hcurl2d_model::solve(&case,&package,&input.model,&input.map,&mesh,&symbols,cache)?
            }
            other=>return Err(format!("H(curl) direct-sum mesh format {other:?} is unsupported.")),
        }
    } else if matches!(family, NumericalFamily::Vector) {
        crate::solver_common::stage_started("assembly");
        match format {
            "fullmeta-v2" => {
                crate::prepared_scalar_execution::check_dense_2d_records(&text)?;
                let mesh = crate::mesh2d::parse_mesh_v2(&text)?;
                crate::prepared_vector_model::solve(&case,&package,&input.model,&input.map,
                    crate::prepared_vector_model::Mesh::Triangle(&mesh),&symbols,cache)?.data
            },
            "fullmeta-v1-indexed" => {
                let mesh = crate::mesh3d::parse_indexed_v1(&text,crate::mesh3d::DuplicatePolicy::Reject)?;
                crate::prepared_vector_model::solve(&case,&package,&input.model,&input.map,
                    crate::prepared_vector_model::Mesh::Tetrahedron(&mesh),&symbols,cache)?.data
            },
            other => return Err(format!("Unsupported vector mesh format {other:?}.")),
        }
    } else { match format {
        "fullmeta-v2" => {
            crate::prepared_scalar_execution::check_dense_2d_records(&text)?;
            let mesh = crate::mesh2d::parse_mesh_v2(&text)?;
            let periodic = crate::scalar2d::periodic_dof_map(&input.model, &input.map, &mesh)?;
            crate::solver_common::stage_started("assembly");
            let result = prepared_frequency_scalar::solve(
                &case,
                &package,
                &input.model,
                &input.map,
                prepared_frequency_scalar::Mesh::Triangle {
                    nodes: &mesh.nodes,
                    cells: &mesh.triangles,
                    facets: &mesh.boundary_edges,
                },
                &symbols,
                Some(&periodic),
                cache,
            )?;
            prepared_frequency_output::build(
                &case,
                &package,
                &input.model,
                &input.map,
                prepared_frequency_output::Mesh::Triangle(&mesh),
                &result,
            )?
        }
        "fullmeta-v1-indexed" => {
            let mesh =
                crate::mesh3d::parse_indexed_v1(&text, crate::mesh3d::DuplicatePolicy::LastWins)?;
            let facets = mesh
                .boundary_faces
                .iter()
                .map(|(nodes, tag)| (nodes.to_vec(), *tag))
                .collect::<Vec<_>>();
            let periodic = crate::scalar2d::periodic_dof_map_for_entities(
                &input.model,
                &input.map,
                &mesh.nodes,
                &facets,
                3,
            )?;
            crate::solver_common::stage_started("assembly");
            let result = prepared_frequency_scalar::solve(
                &case,
                &package,
                &input.model,
                &input.map,
                prepared_frequency_scalar::Mesh::Tetrahedron {
                    nodes: &mesh.nodes,
                    cells: &mesh.tetrahedra,
                    facets: &mesh.boundary_faces,
                },
                &symbols,
                Some(&periodic),
                cache,
            )?;
            prepared_frequency_output::build(
                &case,
                &package,
                &input.model,
                &input.map,
                prepared_frequency_output::Mesh::Tetrahedron(&mesh),
                &result,
            )?
        }
        other => {
            return Err(format!(
                "Unsupported frequency scalar mesh format {other:?}."
            ))
        }
    }};
    let prefix = package
        .pointer("/schemaDocument/x-fem-scenario/resultDefaults/frequencyFieldPrefix")
        .and_then(Value::as_str)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 80
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        })
        .ok_or("Scenario frequencyFieldPrefix must be a safe filename prefix.")?;
    let field_filename = format!("{prefix}{:.6}Hz.json", input.frequency);
    // The contract-selected operator formulation also owns the result plan at
    // zero kz. A full-polarization option stays on its declared direct-sum
    // field space even when the longitudinal derivative vanishes.
    let result_formulation =
        crate::longitudinal::operator_formulation(&package, &case.settings.formulation);
    let plan =
        crate::scenario_plan::ScenarioPlan::from_package(&package, result_formulation)?
            .result
            .ok_or("Scenario result plan is required.")?;
    let variable_names = if matches!(family,NumericalFamily::Vector) {
        if config.get("globalVariables").and_then(Value::as_str) != Some("ignore") { return Err("Unsupported vector global-variable policy.".into()); }
        Vec::new()
    } else { input
        .model
        .pointer("/model/variables")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            row.get("symbol")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect() };
    let field_files = if matches!(family, NumericalFamily::Vector|NumericalFamily::Hcurl2d) {
        input.model.pointer("/model/study").and_then(Value::as_array).and_then(|studies|
            studies.iter().find(|study| study.get("id").and_then(Value::as_str) == Some(case.study_id.as_str())))
            .and_then(|study| study.pointer("/output/fieldFiles")).and_then(Value::as_bool).unwrap_or(true)
    } else { true };
    Ok(ComputedCase {
        data,
        plan,
        output: input.output_directory,
        field_filename,
        csv_filename: sweep_csv.clone(),
        variable_names,
        frequency: input.frequency,
        field_files,
    })
}

fn variable_columns(cases: &[ComputedCase]) -> Result<Vec<CsvVariable>, String> {
    let first = cases.first().ok_or("No frequency cases were computed.")?;
    if cases
        .iter()
        .any(|case| case.variable_names != first.variable_names)
    {
        return Err("Frequency cases declare different global-variable columns.".into());
    }
    first
        .variable_names
        .iter()
        .map(|name| {
            let mut real_only = true;
            for case in cases {
                let value = case
                    .data
                    .complex
                    .get(name)
                    .ok_or_else(|| format!("Global variable {name:?} was not returned."))?;
                if !value.re.is_finite() || !value.im.is_finite() {
                    return Err(format!("Global variable {name:?} is non-finite."));
                }
                real_only &= value.im.abs() <= 1e-12 * value.re.abs().max(1.);
            }
            Ok(CsvVariable {
                name: name.clone(),
                real_only,
            })
        })
        .collect()
}

fn publish(case: &ComputedCase, variables: &[CsvVariable]) -> Result<(), String> {
    // Construct both artifacts before creating files.
    let encoded = if case.field_files {
        let document = case.plan.document(&case.data)?;
        Some(serde_json::to_vec_pretty(&document).map_err(|e| format!("Could not encode field output: {e}"))?)
    } else { None };
    let csv = case
        .plan
        .csv_with_variables(std::slice::from_ref(&case.data), variables)?;
    let completion = case.plan.completion(&case.data)?;
    fs::create_dir_all(&case.output)
        .map_err(|e| format!("Could not create output directory: {e}"))?;
    crate::solver_common::stage_started("output");
    if let Some(encoded) = encoded { fs::write(case.output.join(&case.field_filename), encoded)
        .map_err(|e| format!("Could not write field output: {e}"))?; }
    fs::write(case.output.join(&case.csv_filename), csv)
        .map_err(|e| format!("Could not write sweep CSV: {e}"))?;
    if let Some(line) = completion {
        println!("{line}");
    }
    Ok(())
}

pub(crate) fn run_request(path: &Path, profile: &str) -> Result<(), String> {
    run_request_for(path,profile,NumericalFamily::Scalar)
}
pub(crate) fn run_vector_request(path: &Path, profile: &str) -> Result<(), String> {
    run_request_for(path,profile,NumericalFamily::Vector)
}
fn run_request_for(path: &Path, profile: &str, family: NumericalFamily) -> Result<(), String> {
    let case = compute(
        path,
        profile,
        &mut SolverCache::default(),
        &crate::preflight_client::validate,
        family,
    )?;
    let columns = variable_columns(std::slice::from_ref(&case))?;
    publish(&case, &columns)
}

pub(crate) fn run_sequence(path: &Path, profile: &str) -> Result<(), String> {
    run_sequence_for(path,profile,NumericalFamily::Scalar)
}
pub(crate) fn run_vector_sequence(path: &Path, profile: &str) -> Result<(), String> {
    run_sequence_for(path,profile,NumericalFamily::Vector)
}
fn run_sequence_for(path: &Path, profile: &str, family: NumericalFamily) -> Result<(), String> {
    let manifest: Value = serde_json::from_slice(
        &fs::read(path).map_err(|e| format!("Could not read sequence manifest: {e}"))?,
    )
    .map_err(|e| format!("Invalid sequence manifest JSON: {e}"))?;
    let requests = manifest
        .get("requestFiles")
        .and_then(Value::as_array)
        .filter(|rows| !rows.is_empty())
        .ok_or("Solver sequence manifest must contain requestFiles.")?;
    let base = path.parent().unwrap_or(Path::new("."));
    let mut cache = SolverCache::default();
    let mut cases = Vec::with_capacity(requests.len());
    for (index, item) in requests.iter().enumerate() {
        let name = item
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("Sequence request path must be nonempty.")?;
        let request = Path::new(name);
        if request
            .components()
            .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(format!(
                "Sequence request {index} must not traverse parent directories."
            ));
        }
        let request = if request.is_absolute() {
            request.to_path_buf()
        } else {
            base.join(request)
        };
        let request_json: Value = serde_json::from_slice(
            &fs::read(&request).map_err(|e| format!("Could not read sequence request: {e}"))?,
        )
        .map_err(|e| format!("Invalid sequence request JSON: {e}"))?;
        let frequency = crate::solver_request::single_case_frequency(&request_json)?;
        println!(
            "{}",
            crate::solver_orchestration::solver_case_progress_line(
                "case-start",
                index,
                requests.len(),
                frequency
            )
        );
        println!(
            "Solving {profile} case {} of {}.",
            index + 1,
            requests.len()
        );
        let mut case = compute(
            &request,
            profile,
            &mut cache,
            &crate::preflight_client::validate,
            family,
        )?;
        let provisional = variable_columns(std::slice::from_ref(&case))?;
        publish(&case, &provisional)?;
        case.plan.retain_csv_data(&mut case.data, &case.variable_names)?;
        println!(
            "{}",
            crate::solver_orchestration::solver_case_progress_line(
                "case-complete",
                index,
                requests.len(),
                frequency
            )
        );
        cases.push(case);
    }
    let columns = variable_columns(&cases)?;
    // The coordinator waits for the sequence child to exit before reading
    // any internal case CSV, so rewrite with batch-wide column shape now.
    for case in &cases {
        let csv = case
            .plan
            .csv_with_variables(std::slice::from_ref(&case.data), &columns)?;
        fs::write(case.output.join(&case.csv_filename), csv)
            .map_err(|e| format!("Could not normalize sweep CSV: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::C;
    use std::collections::HashMap;

    fn case(names: &[&str], values: &[C]) -> ComputedCase {
        let plan = ResultPlan::from_json(&serde_json::json!({"document":{"literal":{}},
            "csv":[{"header":"value","input":"value","format":"fixed","precision":1}]})).unwrap();
        ComputedCase {
            data: ResultData {
                values: HashMap::new(),
                complex: names
                    .iter()
                    .zip(values)
                    .map(|(name, value)| (name.to_string(), *value))
                    .collect(),
                series: HashMap::new(),
                matrices: HashMap::new(),
            },
            plan,
            output: PathBuf::new(),
            field_filename: String::new(),
            csv_filename: String::new(),
            variable_names: names.iter().map(|name| name.to_string()).collect(),
            frequency: 1.,
            field_files: true,
        }
    }

    #[test]
    fn sequence_uses_one_global_variable_column_shape_for_all_cases() {
        let first = case(
            &["response", "power"],
            &[C { re: 1., im: 0. }, C { re: 2., im: 0. }],
        );
        let second = case(
            &["response", "power"],
            &[C { re: 3., im: 2. }, C { re: 4., im: 0. }],
        );
        let columns = variable_columns(&[first, second]).unwrap();
        assert_eq!(columns.len(), 2);
        assert!(!columns[0].real_only);
        assert!(columns[1].real_only);
        let first = case(&["response"], &[C { re: 1., im: 0. }]);
        let second = case(&["other"], &[C { re: 1., im: 0. }]);
        assert!(variable_columns(&[first, second]).is_err());
        let bad = case(
            &["response"],
            &[C {
                re: f64::NAN,
                im: 0.,
            }],
        );
        assert!(variable_columns(&[bad]).is_err());
    }

    #[test]
    fn internal_sequence_csv_names_are_safe_without_allowing_paths() {
        assert!(safe_case_csv("response.csv"));
        assert!(safe_case_csv(".fullmeta-case-123-request-0001.csv"));
        assert!(!safe_case_csv(".hidden.csv"));
        assert!(!safe_case_csv("../outside.csv"));
        assert!(!safe_case_csv(".fullmeta-case-../outside.csv"));
    }

    #[test]
    fn zero_kz_uses_the_formulations_declared_numerical_family() {
        let package = serde_json::json!({
            "harmonicLongitudinalWavenumber": {
                "formulations": {
                    "Full": { "numericalFamily": "deRhamTriangle" }
                }
            }
        });
        let family = frequency_family_for_formulation(
            &package,
            "Full",
            false,
            NumericalFamily::Scalar,
        )
        .unwrap();
        assert!(matches!(family, NumericalFamily::Hcurl2d));
    }

    #[test]
    fn zero_kz_preserves_the_default_family_when_no_formulation_is_declared() {
        let package = serde_json::json!({
            "harmonicLongitudinalWavenumber": {
                "formulations": {
                    "Full": { "numericalFamily": "deRhamTriangle" }
                }
            }
        });
        let family = frequency_family_for_formulation(
            &package,
            "Ez",
            false,
            NumericalFamily::Scalar,
        )
        .unwrap();
        assert!(matches!(family, NumericalFamily::Scalar));
    }

    #[test]
    fn nonzero_kz_still_requires_a_formulation_contract() {
        let error = frequency_family_for_formulation(
            &serde_json::json!({}),
            "Full",
            true,
            NumericalFamily::Scalar,
        )
        .unwrap_err();
        assert!(error.contains("Nonzero longitudinal wavenumber"));
    }

    #[test]
    fn unsupported_contract_numerical_family_is_reported() {
        let package = serde_json::json!({
            "harmonicLongitudinalWavenumber": {
                "formulations": {
                    "Full": { "numericalFamily": "unknown" }
                }
            }
        });
        let error = frequency_family_for_formulation(
            &package,
            "Full",
            false,
            NumericalFamily::Scalar,
        )
        .unwrap_err();
        assert!(error.contains("Unsupported contract-declared"));
    }

    #[test]
    fn csv_only_publication_does_not_build_field_document() {
        use serde_json::json;
        use std::time::{SystemTime, UNIX_EPOCH};
        let output = std::env::temp_dir().join(format!("fullmeta-csv-only-{}-{}",std::process::id(),SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
        let plan = ResultPlan::from_json(&json!({"document":{"input":"missingField"},
            "csv":[{"header":"frequency","input":"frequency","format":"fixed","precision":1}]})).unwrap();
        let case = ComputedCase { data:ResultData { values:HashMap::from([("frequency".into(),json!(2.))]),
            complex:HashMap::new(),series:HashMap::new(),matrices:HashMap::new() },plan,output:output.clone(),
            field_filename:"field.json".into(),csv_filename:"out.csv".into(),variable_names:vec![],frequency:2.,field_files:false };
        publish(&case,&[]).unwrap();
        assert_eq!(fs::read_to_string(output.join("out.csv")).unwrap(),"frequency\n2.0\n");
        assert!(!output.join("field.json").exists());
        fs::remove_dir_all(output).unwrap();
    }

    

    
}
