//! FEM-owned implementation of the schema-registered result resolution API.
//! The Kernel server only launches this JSON-over-stdio handler; result and
//! Study semantics remain in the FEM application package.

#[path = "study_fingerprint.rs"]
mod study_fingerprint;

use crate::model_expression;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Component, Path, PathBuf};

type Result<T> = std::result::Result<T, String>;

pub(super) fn study_input_fingerprint(model: &Value, study_id: &str) -> Option<Value> {
    study_fingerprint::fingerprint(model, study_id)
}

#[derive(Clone)]
pub(super) struct Run {
    pub(super) id: String,
    pub(super) directory: PathBuf,
    pub(super) record: Value,
}

struct ResultScope<'a> {
    root: &'a Path,
    document: &'a Path,
    project: &'a Path,
    runs_directory: &'a Path,
    model: &'a Value,
}

pub fn resolve(input: &Value) -> Result<Value> {
    let root = env::var("FULLMETA_ROOT")
        .map(PathBuf::from)
        .map_err(|_| "The Kernel server did not provide the FullMeta root.".to_string())?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the FullMeta root: {error}"))?;
    let document = env::var("FULLMETA_DOCUMENT")
        .map(PathBuf::from)
        .map_err(|_| "The result request must identify its opened document.".to_string())?
        .canonicalize()
        .map_err(|error| format!("Could not resolve the opened document: {error}"))?;
    if !document.starts_with(&root) || !document.is_file() {
        return Err("The opened document must be a regular file inside FullMeta.".into());
    }
    let model = input
        .get("model")
        .filter(|value| value.is_object())
        .ok_or("The result request must contain the current model JSON.")?;
    let validation_errors = crate::model_validation::validate(model);
    if !validation_errors.is_empty() {
        return Err(validation_errors.join("\n"));
    }
    if let Some(message) = crate::sibling_model_id_conflict(model, &document)? {
        return Err(message);
    }
    let project = document
        .parent()
        .ok_or("The opened model has no containing folder.")?;
    let model_folder = input
        .get("folder")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "projects/{}",
                model
                    .pointer("/metadata/id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
        });
    let expected_project = checked_existing_path(&root, &model_folder, true)?;
    if expected_project != project {
        return Err("The result request folder does not match the opened model document.".into());
    }
    if let Some(file) = input.get("file").and_then(Value::as_str) {
        if document.file_name().and_then(|name| name.to_str()) != Some(file) {
            return Err("The result request file does not match the opened model document.".into());
        }
    }
    let data_folder = input
        .get("dataFolder")
        .and_then(Value::as_str)
        .unwrap_or(&model_folder);
    if checked_existing_path(&root, data_folder, true)? != project {
        return Err("Result data must stay beside the selected model JSON.".into());
    }
    let action_id = input
        .get("actionId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("The result request must select an action ID.")?;
    let action = model
        .pointer("/model/results")
        .and_then(Value::as_array)
        .and_then(|actions| {
            actions
                .iter()
                .find(|action| action.get("id").and_then(Value::as_str) == Some(action_id))
        })
        .ok_or_else(|| format!("Unknown result action {action_id}."))?;
    let runs_name = env::var("FULLMETA_STORAGE_RUNS_DIRECTORY")
        .map_err(|_| "The active schema does not declare its run storage directory.".to_string())?;
    if !valid_single_component(&runs_name) {
        return Err("The active schema declares an invalid run storage directory.".into());
    }
    let runs_directory = project.join(runs_name);
    let scope = ResultScope {
        root: &root,
        document: &document,
        project,
        runs_directory: &runs_directory,
        model,
    };
    let action_type = action.get("type").and_then(Value::as_str).unwrap_or("");
    match action_type {
        "plot.sweep" => resolve_sweep(action, &scope),
        "sweep" => resolve_quantity_sweep(input, action, &scope),
        "field" if model.get("modelFormat").and_then(Value::as_str) == Some("fem-model/2") => {
            resolve_field(input, action, &scope)
        }
        _ => Err(format!("Unsupported result action type {action_type}.")),
    }
}

fn resolve_quantity_sweep(input: &Value, action: &Value, scope: &ResultScope<'_>) -> Result<Value> {
    let model = scope.model;
    let quantity = action
        .get("quantity")
        .and_then(Value::as_str)
        .ok_or("A sweep result requires its quantity.")?;
    let profile = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .ok_or("The model has no profile ID.")?;
    if !crate::model_schema::sweep_result_quantities_for_profile(profile)?
        .iter()
        .any(|item| item == quantity)
    {
        return Err(format!(
            "Profile {profile} does not declare sweep quantity {quantity}."
        ));
    }
    let studies = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .ok_or("A sweep result requires a Study.")?;
    let selected = input.get("studyId").and_then(Value::as_str);
    let runs = scope.eligible_runs(selected)?;
    let study = runs
        .iter()
        .find_map(|run| {
            let study_id = run.record.pointer("/study/id")?.as_str()?;
            studies
                .iter()
                .find(|study| study["id"].as_str() == Some(study_id))
        })
        .or_else(|| {
            studies
                .iter()
                .find(|study| selected.is_none_or(|id| study["id"].as_str() == Some(id)))
        })
        .ok_or("A sweep result requires an existing Study.")?;
    let study_id = study["id"]
        .as_str()
        .ok_or("A sweep result Study must have an ID.")?;
    let axes = study
        .pointer("/parameterSweep/parameters")
        .and_then(Value::as_array);
    let x_column = match axes.map(Vec::as_slice) {
        Some([axis]) => axis["parameter"]
            .as_str()
            .ok_or("The Study sweep axis has no parameter symbol.")?,
        _ => {
            if profile == "em-3d" {
                "frequency_GHz"
            } else {
                "frequency_Hz"
            }
        }
    };
    let name = action["name"].as_str().unwrap_or(quantity);
    let mut recipe = action.clone();
    recipe["datasets"] = json!([{"id":action["id"],"name":name,"source":{"type":"study","studyId":study_id},"xColumn":x_column,"style":"solid"}]);
    recipe["series"] = json!([{"id":action["id"],"name":name,"column":quantity}]);
    let mut resolved = resolve_sweep(&recipe, scope)?;
    resolved["action"] = action.clone();
    resolved["series"] = recipe["series"].clone();
    Ok(resolved)
}

fn resolve_sweep(action: &Value, scope: &ResultScope<'_>) -> Result<Value> {
    let ResultScope {
        root,
        project,
        model,
        ..
    } = *scope;
    let mut datasets = Vec::new();
    for dataset in action
        .get("datasets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut result = dataset.clone();
        let source = dataset
            .get("source")
            .ok_or("A result dataset has no source.")?;
        match source.get("type").and_then(Value::as_str) {
            Some("file") => {
                let relative = source
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or("A file result source has no path.")?;
                let Some(artifact) = checked_file_maybe(project, relative)? else {
                    result["missing"] = json!(true);
                    result["message"] = json!(format!("Result file is missing: {relative}"));
                    datasets.push(result);
                    continue;
                };
                result["path"] = json!(relative.replace('\\', "/"));
                result["url"] = json!(artifact_url(root, &artifact)?);
            }
            Some("study") => {
                let study_id = source
                    .get("studyId")
                    .and_then(Value::as_str)
                    .ok_or("A Study result source has no studyId.")?;
                let mut checked_run = None;
                let mut missing_artifact = None;
                for run in scope.eligible_runs(Some(study_id))? {
                    match checked_study_csv(&run, action, dataset) {
                        Ok((csv, cases, metadata)) => {
                            checked_run = Some((run, csv, cases, metadata));
                            break;
                        }
                        Err(error) => {
                            missing_artifact.get_or_insert(error);
                        }
                    }
                }
                let Some((run, csv_path, parameter_cases, metadata)) = checked_run else {
                    result["missing"] = json!(true);
                    let name = study_display_name(model, study_id);
                    result["message"] = json!(missing_run_message(name, missing_artifact));
                    datasets.push(result);
                    continue;
                };
                let relative =
                    slash_path(csv_path.strip_prefix(project).map_err(|_| {
                        "The result artifact escaped its project folder.".to_string()
                    })?);
                result["path"] = json!(relative);
                result["runId"] = json!(run.id);
                result["modelFreshness"] = json!("current");
                result["parameterCasesByFrequency"] = parameter_cases;
                result["tabularMetadata"] = metadata;
                if let Some(variables) = run
                    .record
                    .pointer("/model/snapshot/model/variables")
                    .and_then(Value::as_array)
                {
                    let expressions: Map<String, Value> = variables
                        .iter()
                        .filter_map(|variable| {
                            Some((
                                variable.get("symbol")?.as_str()?.to_string(),
                                variable.get("expression")?.as_str()?.into(),
                            ))
                        })
                        .collect();
                    if !expressions.is_empty() {
                        result["storedVariableExpressions"] = Value::Object(expressions);
                    }
                    let units: Map<String, Value> = variables
                        .iter()
                        .filter_map(|variable| {
                            Some((
                                variable.get("symbol")?.as_str()?.to_string(),
                                variable.get("unit")?.as_str()?.into(),
                            ))
                        })
                        .collect();
                    if !units.is_empty() {
                        result["storedVariableUnits"] = Value::Object(units);
                    }
                }
                result["url"] = json!(artifact_url(root, &csv_path)?);
            }
            _ => return Err("A result dataset source type is unsupported.".into()),
        }
        datasets.push(result);
    }
    Ok(json!({ "action": action, "datasets": datasets }))
}

/// Use the same candidate and checked artifacts for figure export and display.
pub(super) fn resolve_plot_datasets(
    action: &Value,
    model: &Value,
    root: &Path,
    document: &Path,
    runs_directory: &Path,
) -> Result<Value> {
    let project = document
        .parent()
        .ok_or("The opened model has no containing folder.")?;
    resolve_sweep(
        action,
        &ResultScope {
            root,
            document,
            project,
            runs_directory,
            model,
        },
    )
}

fn checked_run_json(run: &Run, filename: &str) -> Result<Value> {
    let path = checked_file_maybe(&run.directory, filename)?
        .ok_or_else(|| format!("Study run {} is missing {filename}.", run.id))?;
    read_json(&path)
}

fn checked_study_csv(
    run: &Run,
    action: &Value,
    dataset: &Value,
) -> Result<(PathBuf, Value, Value)> {
    let relative = run
        .record
        .pointer("/artifacts/csv")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Study run {} did not declare a CSV artifact.", run.id))?;
    let path = checked_file_maybe(&run.directory, relative)?
        .ok_or_else(|| format!("Study run {} is missing {relative}.", run.id))?;
    let request = checked_run_json(run, "request.json")?;
    let (cases, metadata) = run_result_data(run, &request, dataset)?;
    let source = format!("Study run {} CSV {relative}", run.id);
    let file = fs::File::open(&path).map_err(|error| format!("{source}: {error}"))?;
    let actual_bytes = file
        .metadata()
        .map_err(|error| format!("{source}: {error}"))?
        .len();
    if run
        .record
        .pointer("/artifacts/csvBytes")
        .and_then(Value::as_u64)
        .is_some_and(|expected| expected != actual_bytes)
    {
        return Err(format!(
            "{source}: size differs from the verified run record."
        ));
    }
    let mut lines = BufReader::new(file).lines();
    let header = lines
        .next()
        .transpose()
        .map_err(|error| format!("{source}: {error}"))?
        .ok_or_else(|| format!("{source}: missing header."))?;
    let headers = header
        .trim_start_matches('\u{feff}')
        .split(',')
        .map(str::trim)
        .collect::<Vec<_>>();
    let header_set = headers.iter().copied().collect::<HashSet<_>>();
    if header_set.contains("") || header_set.len() != headers.len() {
        return Err(format!("{source}: invalid header."));
    }
    let parameter_columns = metadata["columns"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|column| column["key"].as_str())
        .collect::<HashSet<_>>();
    let global_symbols = run
        .record
        .pointer("/model/snapshot/model/variables")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|variable| variable["symbol"].as_str())
        .collect::<HashSet<_>>();
    let value_column = |key: &str| {
        header_set.contains(key)
            || (global_symbols.contains(key) && header_set.contains(format!("{key}_re").as_str()))
    };
    let required_values = action
        .get("series")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|series| series["column"].as_str());
    for key in required_values {
        if !value_column(key) {
            return Err(format!("{source}: missing declared series column {key}."));
        }
    }
    let required_columns = dataset
        .get("xColumn")
        .and_then(Value::as_str)
        .into_iter()
        .chain(
            dataset
                .get("parameterColumns")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        );
    for key in required_columns {
        if !header_set.contains(key) && !parameter_columns.contains(key) {
            return Err(format!("{source}: missing declared column {key}."));
        }
    }
    for key in action
        .get("tableColumns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !parameter_columns.contains(key) && !value_column(key) {
            return Err(format!("{source}: missing declared table column {key}."));
        }
    }
    let coordinates = metadata["coordinates"]
        .as_array()
        .ok_or_else(|| format!("{source}: missing saved Study coordinates."))?;
    let mut rows = 0;
    for line in lines {
        let line = line.map_err(|error| format!("{source}: {error}"))?;
        if line.trim().is_empty() {
            continue;
        }
        rows += 1;
        let cells = line.split(',').map(str::trim).collect::<Vec<_>>();
        if cells.len() != headers.len()
            || cells
                .iter()
                .any(|value| !value.parse::<f64>().is_ok_and(f64::is_finite))
        {
            return Err(format!("{source}: malformed numeric row {rows}."));
        }
    }
    if rows == 0 || rows != coordinates.len() {
        return Err(format!(
            "{source}: sample count differs from saved Study coordinates."
        ));
    }
    Ok((path, cases, metadata))
}

fn resolve_field(input: &Value, action: &Value, scope: &ResultScope<'_>) -> Result<Value> {
    let model = scope.model;
    let studies = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .filter(|studies| !studies.is_empty())
        .ok_or("The Model 2 field result has no Study to read from.")?;
    let mut missing_artifact = None;
    let selected_study = input.get("studyId").and_then(Value::as_str);
    for run in scope.eligible_runs(selected_study)? {
        match resolve_field_run(input, action, scope, &run) {
            Ok(result) => return Ok(result),
            Err(error) => {
                missing_artifact.get_or_insert(error);
            }
        }
    }
    let name = selected_study
        .or_else(|| studies.first().and_then(|study| study["id"].as_str()))
        .map(|id| study_display_name(model, id))
        .unwrap_or("the selected Study");
    Ok(
        json!({"action":action,"missing":true,"message":missing_run_message(name, missing_artifact)}),
    )
}

fn missing_run_message(study_name: &str, reason: Option<String>) -> String {
    let mut message = format!("No compatible completed run is available for Study {study_name}.");
    if let Some(reason) = reason {
        message.push(' ');
        message.push_str(&reason);
    }
    message
}

fn resolve_field_run(
    input: &Value,
    action: &Value,
    scope: &ResultScope<'_>,
    run: &Run,
) -> Result<Value> {
    let ResultScope {
        root,
        project,
        runs_directory,
        model,
        ..
    } = *scope;
    let study_id = run
        .record
        .pointer("/study/id")
        .and_then(Value::as_str)
        .ok_or("The run has no Study ID.")?;
    let study_name = study_display_name(model, study_id);
    let profile = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .ok_or("The model has no profile ID.")?;
    let component = action
        .pointer("/quantity/component")
        .and_then(Value::as_str)
        .ok_or("The selected field result requires a component.")?;
    let scenario = crate::model_schema::scenario_metadata_for_profile(profile)?;
    let plot_sources = scenario
        .pointer("/fields/plotSources")
        .and_then(Value::as_array);
    let supports_pointwise =
        plot_sources.is_some_and(|sources| sources.iter().any(|source| source == "pointwise"));
    let is_pointwise = supports_pointwise
        && definitions_containing(model.pointer("/model/pointwiseVariables"), component).is_some();
    if run
        .record
        .pointer("/solver/fieldFiles")
        .and_then(Value::as_bool)
        == Some(false)
    {
        return Err(format!(
            "Study {study_name} run {} did not save field files. Run the Study again with field output enabled.",
            run.id
        ));
    }
    let request = checked_run_json(run, "request.json")?;
    let run_relative = format!("{}/{}", storage_name(runs_directory)?, run.id);
    let files = list_field_files(run, &run_relative, model, &request)?;
    let (parameter_cases, saved_metadata) = run_result_data(run, &request, &Value::Null)?;
    let expected_count = run
        .record
        .pointer("/artifacts/fieldFileCount")
        .and_then(Value::as_u64);
    let coordinates = saved_metadata["coordinates"]
        .as_array()
        .ok_or("The run has no saved Study coordinates.")?;
    if files.is_empty()
        || expected_count.is_some_and(|count| count != files.len() as u64)
        || coordinates.len() != files.len()
    {
        return Err(format!(
            "Study {study_name} run {} has no complete field output.",
            run.id
        ));
    }
    let mut field_rows = Vec::new();
    for file in &files {
        let by_case = file
            .get("caseIndex")
            .and_then(Value::as_u64)
            .map(|index| index as usize)
            .and_then(|index| {
                coordinates.iter().find(|coordinate| {
                    coordinate.get("caseIndex").and_then(Value::as_u64) == Some(index as u64)
                })
            });
        let candidates: Vec<&Value> = if let Some(coordinate) = by_case {
            vec![coordinate]
        } else if let Some(frequency) = file.get("frequencyHz").and_then(Value::as_f64) {
            coordinates
                .iter()
                .filter(|coordinate| {
                    coordinate.get("frequencyHz").and_then(Value::as_f64) == Some(frequency)
                })
                .collect()
        } else if coordinates.len() == 1 {
            vec![&coordinates[0]]
        } else {
            Vec::new()
        };
        if candidates.len() != 1 {
            return Err(format!(
                "Field artifact {} has no unambiguous saved parameter case. Run the Study again to refresh its field metadata.",
                file["name"].as_str().unwrap_or("(unknown)")
            ));
        }
        field_rows.push((file.clone(), candidates[0].clone()));
    }
    let field_coordinates = field_rows
        .iter()
        .map(|(_, coordinate)| coordinate.clone())
        .collect::<Vec<_>>();
    let mut tabular_metadata = saved_metadata.clone();
    tabular_metadata["coordinates"] = json!(field_coordinates);
    let columns = tabular_metadata
        .get("columns")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let default_filters = if let Some(filters) =
        action.get("filters").filter(|value| !value.is_null())
    {
        filters.clone()
    } else {
        let requested_case = input
            .get("caseIndex")
            .and_then(Value::as_u64)
            .map(|index| index as usize);
        let parameters = model
            .pointer("/model/parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let configured_hz = model_expression::resolve_parameters(&parameters, model)
            .ok()
            .and_then(|parameters| {
                action.get("frequency").and_then(|frequency| {
                    let action_id = action.get("id").and_then(Value::as_str).unwrap_or("result");
                    model_expression::evaluate_quantity(
                        frequency,
                        &parameters,
                        &format!("{action_id}.frequency"),
                    )
                    .ok()
                })
            })
            .filter(|frequency| frequency.is_finite() && *frequency > 0.0);
        let requested_frequency = input
            .get("frequencyGHz")
            .and_then(Value::as_f64)
            .filter(|frequency| frequency.is_finite())
            .map(|value| value * 1e9)
            .filter(|frequency| frequency.is_finite() && *frequency > 0.0)
            .or(configured_hz);
        let configured = requested_case
            .and_then(|index| {
                field_rows.iter().find(|(file, _)| {
                    file.get("caseIndex").and_then(Value::as_u64) == Some(index as u64)
                })
            })
            .or_else(|| {
                requested_frequency.and_then(|frequency| nearest_field(&field_rows, frequency))
            })
            .or_else(|| field_rows.first());
        configured
            .and_then(|(_, coordinate)| coordinate.get("values").cloned())
            .unwrap_or_else(|| json!({}))
    };
    let (selected_filters, adjustments) =
        resolve_fixed_filters(&columns, &field_coordinates, &default_filters)?;
    let matching = field_rows
        .iter()
        .filter(|(_, coordinate)| row_matches(coordinate.get("values"), &selected_filters))
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return Err("No rows match the selected parameter values.".into());
    }
    if matching.len() != 1 {
        return Ok(
            json!({"action":action,"runId":run.id,"studyId":study_id,"tabularMetadata":tabular_metadata,
            "selectionError":"The selected parameter values must identify exactly one available field artifact."}),
        );
    }
    let (selected, coordinate) = matching[0];
    let name = selected
        .get("name")
        .and_then(Value::as_str)
        .ok_or("The selected field artifact has no filename.")?;
    let relative = format!("{run_relative}/output/{name}");
    let artifact = checked_file_maybe(project, &relative)?
        .ok_or_else(|| format!("Field artifact is missing: {relative}"))?;
    let raw = read_json(&artifact)?;
    if raw.get("studyId").is_some_and(|id| id != study_id) {
        return Err(format!(
            "Field artifact {relative} belongs to a different Study."
        ));
    }
    let is_material = plot_sources
        .is_some_and(|sources| sources.iter().any(|source| source == "material"))
        && crate::model_schema::material_properties_for_profile(profile)?
            .iter()
            .any(|property| property["symbol"] == component);
    let quantity = if is_material {
        raw.get("materialFieldsByCell")
    } else {
        raw.get("fields")
    };
    if !is_pointwise
        && quantity
            .and_then(|fields| fields.get(component))
            .is_none_or(Value::is_null)
    {
        return Err(format!(
            "Field artifact {relative} is missing the requested component {component}."
        ));
    }
    let selected_case = coordinate
        .get("caseIndex")
        .cloned()
        .or_else(|| selected.get("caseIndex").cloned())
        .unwrap_or(Value::Null);
    let frequency_hz = selected
        .get("frequencyHz")
        .and_then(Value::as_f64)
        .or_else(|| coordinate.get("frequencyHz").and_then(Value::as_f64));
    let frequency_ghz = frequency_hz.map(|value| value / 1e9);
    let mut available_cases = files.iter().filter(|file| file.get("caseIndex").and_then(Value::as_u64).is_some())
        .map(|file| json!({"caseIndex":file["caseIndex"],"frequencyGHz":file.get("frequencyHz").and_then(Value::as_f64).map(|value| value/1e9),"label":file["label"]})).collect::<Vec<_>>();
    if available_cases.iter().any(|entry| {
        entry
            .get("frequencyGHz")
            .is_some_and(|value| value.is_null())
    }) {
        for entry in &mut available_cases {
            if entry.get("frequencyGHz").is_some_and(Value::is_null) {
                entry.as_object_mut().unwrap().remove("frequencyGHz");
            }
        }
    }
    let mut available_frequencies = Vec::<f64>::new();
    for file in &files {
        if let Some(value) = file
            .get("frequencyHz")
            .and_then(Value::as_f64)
            .map(|value| value / 1e9)
        {
            if !available_frequencies.contains(&value) {
                available_frequencies.push(value);
            }
        }
    }
    let stationary = profile == "thermal-2d";
    let mut response = json!({
        "action":action,"runId":run.id,"studyId":study_id,"studyName":study_name,
        "modelFreshness":"current",
        "tabularMetadata":tabular_metadata,"selectedFilters":selected_filters,
        "selectionAdjustments":adjustments,"caseIndex":selected_case,
        "path":relative,"url":artifact_url(root,&artifact)?,
        "availableCases":if stationary {json!([])} else {json!(available_cases)},
        "availableFrequenciesGHz":if stationary {json!([])} else {json!(available_frequencies)},
        "stationary":stationary,
    });
    if stationary {
        response["caseIndex"] = selected.get("caseIndex").cloned().unwrap_or(json!(0));
    } else if let Some(frequency) = frequency_ghz {
        response["frequencyGHz"] = json!(frequency);
        if let Some(parameter) = checked_frequency_parameter(run, study_id) {
            response["frequencyParameter"] = parameter;
        }
    }
    if supports_pointwise {
        let evaluation = checked_field_evaluation(
            run,
            coordinates,
            &parameter_cases,
            response["caseIndex"].as_u64(),
            is_pointwise.then_some(component),
        );
        match evaluation {
            Ok(Some(evaluation)) => response["fieldEvaluation"] = evaluation,
            Err(error) if is_pointwise => return Err(error),
            _ => {}
        }
    }
    if let Some(case_index) = response["caseIndex"].as_u64() {
        if let Some(mesh) = checked_field_mesh(scope, run, case_index, coordinates.len() == 1) {
            response["meshArtifact"] = mesh;
        }
    }
    Ok(response)
}

fn checked_frequency_parameter(run: &Run, study_id: &str) -> Option<Value> {
    let model = run.record.pointer("/model/snapshot/model")?;
    let study = model
        .get("study")?
        .as_array()?
        .iter()
        .find(|study| study["id"] == study_id)?;
    let physics = model.get("physics")?;
    let physics_id = physics.get("id")?.as_str()?;
    if study.get("physicsId")?.as_str()? != physics_id {
        return None;
    }
    let symbol = physics.pointer("/frequency/expression")?.as_str()?.trim();
    let definitions = model.get("parameters")?.as_array()?;
    let mut matches = definitions
        .iter()
        .filter(|parameter| parameter["symbol"] == symbol);
    let unit = matches.next()?.get("unit")?.as_str()?;
    if matches.next().is_some() {
        return None;
    }
    let (_, dimension) = model_expression::unit(unit).ok()?;
    let (_, frequency_dimension) = model_expression::unit("Hz").ok()?;
    (dimension == frequency_dimension).then(|| json!({"symbol":symbol,"unit":unit}))
}

fn checked_field_evaluation(
    run: &Run,
    coordinates: &[Value],
    parameter_cases: &Value,
    case_index: Option<u64>,
    requested: Option<&str>,
) -> Result<Option<Value>> {
    let snapshot = run
        .record
        .pointer("/model/snapshot")
        .ok_or_else(|| format!("Run {} omits its model snapshot.", run.id))?;
    let definitions = snapshot
        .pointer("/model/pointwiseVariables")
        .filter(|value| value.as_array().is_some_and(|rows| !rows.is_empty()));
    if let Some(component) = requested {
        if definitions_containing(definitions, component).is_none() {
            return Err(format!(
                "Run {} has no saved pointwise definition for {component}.",
                run.id
            ));
        }
    }
    let Some(definitions) = definitions else {
        return Ok(None);
    };
    let case_index = case_index.ok_or_else(|| {
        format!(
            "Run {} does not identify the selected case for pointwise evaluation.",
            run.id
        )
    })?;
    let case_row = coordinates
        .iter()
        .position(|row| row["caseIndex"].as_u64() == Some(case_index))
        .ok_or_else(|| {
            format!(
                "Run {} request.json omits prepared case {case_index}.",
                run.id
            )
        })?;
    let parameters = parameter_cases
        .get(case_row)
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!(
                "Run {} request.json omits parameters for case {case_index}.",
                run.id
            )
        })?;
    for parameter in snapshot
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let symbol = parameter
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Run {} has a saved parameter without a symbol.", run.id))?;
        if parameters
            .get(symbol)
            .and_then(Value::as_f64)
            .is_none_or(|value| !value.is_finite())
        {
            return Err(format!(
                "Run {} request.json has no finite {symbol} value for case {case_index}.",
                run.id
            ));
        }
    }
    Ok(Some(
        json!({"definitions": definitions, "parameters": parameters}),
    ))
}

fn checked_field_mesh(
    scope: &ResultScope<'_>,
    run: &Run,
    case_index: u64,
    single_case: bool,
) -> Option<Value> {
    let geometry = run.record.get("geometry")?;
    let case = if let Some(cases) = geometry.get("cases") {
        let mut matches = cases
            .as_array()?
            .iter()
            .filter(|case| case["caseIndex"].as_u64() == Some(case_index));
        let selected = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        selected
    } else if single_case {
        geometry
    } else {
        return None;
    };
    let mesh = case.get("mesh")?.as_str()?;
    let geometry_hash = case.get("geometryHash")?.as_str()?;
    let cache = || {
        let relative = format!("{}-mesh-cache.json", mesh.strip_suffix("-mesh.mesh")?);
        let path = checked_file_maybe(&run.directory, &relative).ok()??;
        let cache = read_json(&path).ok()?;
        (cache["schema"] == "fullmeta-mesh-cache/3"
            && cache["geometryHash"].as_str() == Some(geometry_hash))
        .then_some(cache)
    };
    let needs_cache = ["mesh", "map"].iter().any(|kind| {
        case.get(kind)
            .and_then(Value::as_str)
            .is_some_and(|relative| {
                geometry
                    .get("files")
                    .and_then(|files| files.get(kind))
                    .and_then(|file| file.get("path"))
                    .and_then(Value::as_str)
                    != Some(relative)
            })
    });
    let cache = needs_cache.then(cache).flatten();
    let artifact = |kind: &str, relative: &str| {
        let path = checked_file_maybe(&run.directory, relative).ok()??;
        let filename = path.file_name()?.to_str()?;
        let expected = geometry
            .get("files")
            .and_then(|files| files.get(kind))
            .filter(|file| file["path"].as_str() == Some(relative))
            .or_else(|| {
                cache
                    .as_ref()?
                    .get("files")?
                    .get(kind)
                    .filter(|file| file["path"].as_str() == Some(filename))
            })?;
        let bytes = fs::read(&path).ok()?;
        if bytes.is_empty()
            || expected["bytes"].as_u64() != Some(bytes.len() as u64)
            || expected["sha256"].as_str() != Some(crate::sha256::hex_digest(&bytes).as_str())
        {
            return None;
        }
        Some(
            json!({"path":slash_path(path.strip_prefix(scope.project).ok()?),
            "url":artifact_url(scope.root, &path).ok()?,"runId":run.id,"caseIndex":case_index}),
        )
    };
    let mut result = artifact("mesh", mesh)?;
    if let Some(map) = case
        .get("map")
        .and_then(Value::as_str)
        .and_then(|path| artifact("map", path))
    {
        result["modelMap"] = map;
    }
    Some(result)
}

fn definitions_containing<'a>(value: Option<&'a Value>, symbol: &str) -> Option<&'a Value> {
    value.filter(|definitions| {
        definitions.as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row.get("symbol").and_then(Value::as_str) == Some(symbol))
        })
    })
}

fn nearest_field<'a>(rows: &'a [(Value, Value)], requested_hz: f64) -> Option<&'a (Value, Value)> {
    rows.iter().min_by(|left, right| {
        let left_hz = left
            .0
            .get("frequencyHz")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        let right_hz = right
            .0
            .get("frequencyHz")
            .and_then(Value::as_f64)
            .unwrap_or(f64::INFINITY);
        (left_hz - requested_hz)
            .abs()
            .total_cmp(&(right_hz - requested_hz).abs())
    })
}

fn resolve_fixed_filters(
    columns: &[Value],
    coordinates: &[Value],
    requested: &Value,
) -> Result<(Value, Value)> {
    let mut filters = Map::new();
    let mut adjustments = Vec::new();
    let supplied = requested
        .as_object()
        .ok_or("Parameter filters must be an object.")?;
    let allowed = columns
        .iter()
        .filter_map(|column| column.get("key").and_then(Value::as_str))
        .collect::<HashSet<_>>();
    for (key, value) in supplied {
        if !allowed.contains(key.as_str()) {
            adjustments.push(json!({"type":"removed-filter","column":key,"previousValue":value}));
        }
    }
    let rows = coordinates
        .iter()
        .filter_map(|coordinate| coordinate.get("values").and_then(Value::as_object))
        .collect::<Vec<_>>();
    let mut candidates = rows
        .iter()
        .copied()
        .filter(|row| {
            supplied
                .iter()
                .filter(|(key, value)| {
                    allowed.contains(key.as_str())
                        && value.as_f64().is_some_and(|candidate| {
                            rows.iter()
                                .any(|row| row.get(*key).and_then(Value::as_f64) == Some(candidate))
                        })
                })
                .all(|(key, value)| row.get(key) == Some(value))
        })
        .collect::<Vec<_>>();
    for column in columns {
        let Some(key) = column.get("key").and_then(Value::as_str) else {
            continue;
        };
        let values = rows
            .iter()
            .filter_map(|row| row.get(key).and_then(Value::as_f64))
            .collect::<Vec<_>>();
        let mut unique = values.clone();
        unique.sort_by(f64::total_cmp);
        unique.dedup_by(|a, b| a == b);
        let Some(first) = unique.first().copied() else {
            return Err(format!(
                "Parameter {} has no available values.",
                column.get("label").and_then(Value::as_str).unwrap_or(key)
            ));
        };
        let candidate = supplied.get(key).and_then(Value::as_f64);
        let observed_default = candidates
            .iter()
            .filter_map(|row| row.get(key).and_then(Value::as_f64))
            .min_by(f64::total_cmp)
            .unwrap_or(first);
        let value = candidate
            .filter(|candidate| unique.contains(candidate))
            .unwrap_or(observed_default);
        if let Some(previous) = supplied.get(key) {
            if candidate.is_none() {
                return Err(format!(
                    "The selected value for {} must be a finite number.",
                    column.get("label").and_then(Value::as_str).unwrap_or(key)
                ));
            }
            if candidate != Some(value) {
                adjustments.push(json!({"type":"filter-value","column":key,"previousValue":previous,"value":value}));
            }
        } else {
            adjustments.push(json!({"type":"added-filter","column":key,"value":value}));
        }
        filters.insert(key.to_string(), json!(value));
        candidates.retain(|row| row.get(key).and_then(Value::as_f64) == Some(value));
    }
    Ok((Value::Object(filters), json!(adjustments)))
}

fn row_matches(row: Option<&Value>, filters: &Value) -> bool {
    let Some(row) = row.and_then(Value::as_object) else {
        return false;
    };
    filters.as_object().is_some_and(|filters| {
        filters
            .iter()
            .all(|(key, value)| row.get(key) == Some(value))
    })
}

pub(super) fn run_result_data(
    run: &Run,
    request: &Value,
    dataset: &Value,
) -> Result<(Value, Value)> {
    let metadata = result_table_metadata(run, request, dataset)?;
    let Some(cases) = request.get("frequencyCases") else {
        return Ok((json!([{}]), metadata));
    };
    let cases = cases
        .as_array()
        .ok_or_else(|| format!("Run {} has inconsistent saved parameter cases.", run.id))?;
    let mut values = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        if case
            .get("frequencyHz")
            .and_then(Value::as_f64)
            .is_none_or(|value| !value.is_finite())
        {
            return Err(format!(
                "Run {} has inconsistent saved parameter cases.",
                run.id
            ));
        }
        let parameters = case
            .get("globalParameters")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let Some(object) = parameters.as_object() else {
            return Err(format!(
                "Run {} has invalid saved parameter values at row {}.",
                run.id,
                index + 1
            ));
        };
        if object
            .values()
            .any(|value| value.as_f64().is_none_or(|number| !number.is_finite()))
        {
            return Err(format!(
                "Run {} has invalid saved parameter values at row {}.",
                run.id,
                index + 1
            ));
        }
        values.push(parameters);
    }
    Ok((json!(values), metadata))
}

fn result_table_metadata(run: &Run, request: &Value, dataset: &Value) -> Result<Value> {
    let study = run.record.pointer("/study/snapshot").ok_or_else(|| format!("Run {} is missing its saved Study snapshot. Run the Study again to refresh its saved result metadata.", run.id))?;
    let sweep = study.get("parameterSweep");
    let empty_axes = Vec::new();
    let axes = match sweep {
        Some(value) => value.get("parameters").and_then(Value::as_array).ok_or_else(|| format!("Run {} has invalid saved sweep dimensions. Run the Study again to refresh its saved result metadata.", run.id))?,
        None => &empty_axes,
    };
    if request.get("frequencyCases").is_none()
        && axes.is_empty()
        && study.get("type").and_then(Value::as_str) == Some("stationary")
    {
        return Ok(
            json!({"columns":[],"coordinates":[{"caseIndex":0,"values":{}}],"defaultXColumn":null}),
        );
    }
    let definitions = run
        .record
        .pointer("/model/snapshot/model/parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    let mut seen = HashSet::new();
    let mut columns = Vec::new();
    for axis in axes {
        let key = axis
            .get("parameter")
            .and_then(Value::as_str)
            .filter(|key| !key.is_empty())
            .ok_or_else(|| {
                format!(
                    "Run {} has invalid saved sweep dimensions. Run the Study again.",
                    run.id
                )
            })?;
        if !seen.insert(key) {
            return Err(format!(
                "Run {} has invalid or duplicated saved sweep dimensions. Run the Study again.",
                run.id
            ));
        }
        columns.push(
            saved_parameter_column(run, definitions, key)?.ok_or_else(|| {
                format!(
                    "Run {} is missing a saved definition for sweep parameter {key}.",
                    run.id
                )
            })?,
        );
    }
    let x_column = dataset.get("xColumn").and_then(Value::as_str);
    let frequency_column = if columns.is_empty() {
        let key = if x_column == Some("frequency_Hz") {
            "frequency_Hz"
        } else {
            "frequency_GHz"
        };
        columns.push(json!({"key":key,"label":"Frequency","unit":if key == "frequency_Hz" {"Hz"} else {"GHz"},"displayScale":if key == "frequency_Hz" {1.0} else {1e9},"role":"parameter"}));
        seen.insert(key);
        Some(key)
    } else {
        None
    };
    let default_x = columns
        .last()
        .and_then(|column| column.get("key"))
        .cloned()
        .unwrap_or(Value::Null);
    // Only publish explicitly requested additional parameters. The slice selector
    // fixes every non-X parameter and would otherwise drop valid sweep rows.
    let requested_keys = x_column
        .into_iter()
        .chain(
            dataset
                .get("parameterColumns")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .chain(
            dataset
                .get("filters")
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|filters| filters.keys().map(String::as_str)),
        );
    for key in requested_keys {
        if seen.insert(key) {
            if let Some(column) = saved_parameter_column(run, definitions, key)? {
                columns.push(column);
            }
        }
    }
    let cases = request.get("frequencyCases").and_then(Value::as_array)
        .ok_or_else(|| format!("Run {} is missing consistent saved result cases. Run the Study again to refresh its saved result metadata.", run.id))?;
    let mut case_indices = HashSet::new();
    let mut coordinates = Vec::new();
    for (row_index, entry) in cases.iter().enumerate() {
        let frequency = entry
            .get("frequencyHz")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("Run {} has inconsistent saved result cases.", run.id))?;
        let mut values = Map::new();
        for column in &columns {
            let key = column.get("key").and_then(Value::as_str).unwrap_or("");
            let value = if frequency_column == Some(key) {
                Some(frequency)
            } else {
                entry
                    .get("globalParameters")
                    .and_then(|values| values.get(key))
                    .and_then(Value::as_f64)
            };
            let value = value.filter(|value| value.is_finite()).ok_or_else(|| {
                format!(
                    "Run {} is missing the saved {key} coordinate at result row {}.",
                    run.id,
                    row_index + 1
                )
            })?;
            values.insert(key.to_string(), json!(value));
        }
        let case_index = entry
            .get("caseIndex")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(row_index);
        if !case_indices.insert(case_index) {
            return Err(format!(
                "Run {} has duplicated saved result case indices.",
                run.id
            ));
        }
        coordinates.push(
            json!({"caseIndex":case_index,"frequencyHz":frequency,"values":Value::Object(values)}),
        );
    }
    Ok(json!({"columns":columns,"coordinates":coordinates,"defaultXColumn":default_x}))
}

fn saved_parameter_column(run: &Run, definitions: &[Value], key: &str) -> Result<Option<Value>> {
    let mut matches = definitions
        .iter()
        .filter(|parameter| parameter.get("symbol").and_then(Value::as_str) == Some(key));
    let Some(parameter) = matches.next() else {
        return Ok(None);
    };
    if matches.next().is_some() {
        return Err(format!(
            "Run {} has ambiguous saved definitions for parameter {key}.",
            run.id
        ));
    }
    let unit = parameter
        .get("unit")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "Run {} has an unsupported saved unit for parameter {key}.",
                run.id
            )
        })?;
    let (scale, _) = model_expression::unit(unit).map_err(|_| {
        format!(
            "Run {} has an unsupported saved unit for parameter {key}.",
            run.id
        )
    })?;
    if !scale.is_finite() || scale <= 0.0 {
        return Err(format!(
            "Run {} has an invalid saved unit for parameter {key}.",
            run.id
        ));
    }
    let label = parameter
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .unwrap_or(key);
    Ok(Some(
        json!({"key":key,"label":label,"unit":unit,"displayScale":scale,"role":"parameter"}),
    ))
}

fn completed_runs(directory: &Path, study_id: Option<&str>) -> Result<Vec<Run>> {
    let info = match fs::symlink_metadata(directory) {
        Ok(info) => info,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("Could not inspect run storage: {error}")),
    };
    if is_link(&info) || !info.is_dir() {
        return Err("The run storage path must be a regular directory.".into());
    }
    let mut runs = Vec::new();
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("Could not list completed runs: {error}"))?
    {
        let entry = entry.map_err(|error| format!("Could not inspect a run directory: {error}"))?;
        let info = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("Could not inspect a run directory: {error}"))?;
        if !info.is_dir() || is_link(&info) {
            continue;
        }
        let run_id = entry.file_name().to_string_lossy().into_owned();
        if !valid_single_component(&run_id) {
            continue;
        }
        let run_directory = entry
            .path()
            .canonicalize()
            .map_err(|error| format!("Could not resolve run directory: {error}"))?;
        if !run_directory.starts_with(directory) {
            continue;
        }
        let Ok(Some(manifest)) = checked_file_maybe(&run_directory, "run.json") else {
            continue;
        };
        let Ok(record) = read_json(&manifest) else {
            continue;
        };
        if record["schema"] != "fullmeta-run/1"
            || record["kind"] != "study"
            || record["state"] != "completed"
            || record["runId"] != run_id
        {
            continue;
        }
        if study_id
            .is_some_and(|id| record.pointer("/study/id").and_then(Value::as_str) != Some(id))
        {
            continue;
        }
        runs.push(Run {
            id: run_id,
            directory: run_directory,
            record,
        });
    }
    runs.sort_by(|left, right| {
        run_timestamp(&right.record, &right.id)
            .cmp(run_timestamp(&left.record, &left.id))
            .then_with(|| right.id.cmp(&left.id))
    });
    Ok(runs)
}

impl ResultScope<'_> {
    fn eligible_runs(&self, study_id: Option<&str>) -> Result<Vec<Run>> {
        let mut runs = completed_runs(self.runs_directory, study_id)?;
        let profile = self
            .model
            .pointer("/scenario/profileId")
            .and_then(Value::as_str);
        let mut current = HashMap::new();
        runs.retain(|run| {
            let record = &run.record;
            let Some(record_file) = record.pointer("/model/file").and_then(Value::as_str) else {
                return false;
            };
            if checked_existing_path(self.root, record_file, false).as_deref() != Ok(self.document)
            {
                return false;
            }
            let Some(study_id) = record.pointer("/study/id").and_then(Value::as_str) else {
                return false;
            };
            let Some(snapshot) = record.pointer("/model/snapshot") else {
                return false;
            };
            if snapshot
                .pointer("/scenario/profileId")
                .and_then(Value::as_str)
                != profile
                || snapshot.pointer("/metadata/id") != record.pointer("/model/id")
            {
                return false;
            }
            let expected = current
                .entry(study_id.to_string())
                .or_insert_with(|| study_input_fingerprint(self.model, study_id));
            expected.as_ref().is_some_and(|expected| {
                saved_fingerprint(record, snapshot, study_id).as_ref() == Some(expected)
            })
        });
        Ok(runs)
    }
}

fn saved_fingerprint(record: &Value, snapshot: &Value, study_id: &str) -> Option<Value> {
    let digest = study_fingerprint::snapshot_digest(snapshot)?;
    let expected_digest = record.pointer("/model/sha256")?.as_str()?;
    if !digest.eq_ignore_ascii_case(expected_digest) {
        return None;
    }
    let projected = study_input_fingerprint(snapshot, study_id)?;
    if record
        .pointer("/study/inputFingerprint")
        .is_some_and(|saved| saved != &projected)
    {
        return None;
    }
    Some(projected)
}

#[cfg(test)]
mod freshness_tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        created: Vec<(PathBuf, bool)>,
    }

    impl Fixture {
        fn new() -> Self {
            let base = env::var_os("FEM3_FRESHNESS_TEST_DIRECTORY")
                .map(PathBuf::from)
                .unwrap_or_else(env::temp_dir);
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = base.join(format!("fem3-result-scope-{}-{nonce}", std::process::id()));
            fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            Self {
                root: root.clone(),
                created: vec![(root, true)],
            }
        }

        fn directory(&mut self, relative: &str) {
            let path = self.root.join(relative);
            fs::create_dir(&path).unwrap();
            self.created.push((path, true));
        }

        fn file(&mut self, relative: &str, value: &str) {
            let path = self.root.join(relative);
            fs::write(&path, value).unwrap();
            self.created.push((path, false));
        }

        fn run(&mut self, id: &str, date: &str, model_file: &str, model: &Value, csv: &str) {
            let folder = format!("runs/{id}");
            self.directory(&folder);
            self.directory(&format!("{folder}/output"));
            let record = json!({
                "schema":"fullmeta-run/1", "kind":"study", "state":"completed", "runId":id, "completedAt":date,
                "model":{"file":model_file,"id":model["metadata"]["id"],"snapshot":model,"sha256":study_fingerprint::snapshot_digest(model)},
                "study":{"id":"study","snapshot":model["model"]["study"][0]},
                "artifacts":{"csv":"output/sweep.csv","csvBytes":csv.len()}
            });
            self.file(&format!("{folder}/run.json"), &record.to_string());
            self.file(&format!("{folder}/request.json"), &json!({
                "studyId":"study","frequencyCases":[{"caseIndex":0,"frequencyHz":1e9,"globalParameters":{}}]
            }).to_string());
            self.file(&format!("{folder}/output/sweep.csv"), csv);
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            for (path, directory) in self.created.iter().rev() {
                let _ = if *directory {
                    fs::remove_dir(path)
                } else {
                    fs::remove_file(path)
                };
            }
        }
    }

    #[test]
    fn local_field_quantities_use_checked_snapshot_and_selected_case_parameters() {
        let definitions = json!([{"symbol":"gain","expression":"p * multiplier","unit":"Pa"}]);
        let run = Run {
            id: "checked".into(),
            directory: PathBuf::new(),
            record: json!({
                "model":{"snapshot":{"model":{"parameters":[{"symbol":"multiplier"}],"pointwiseVariables":definitions}}}
            }),
        };
        let coordinates = [json!({"caseIndex":3}), json!({"caseIndex":9})];
        let parameters = json!([{"multiplier":2.0},{"multiplier":5.0}]);
        let context = checked_field_evaluation(&run, &coordinates, &parameters, Some(9), None)
            .unwrap()
            .unwrap();
        assert_eq!(
            context,
            json!({"definitions":definitions,"parameters":{"multiplier":5.0}})
        );
        assert!(
            checked_field_evaluation(&run, &coordinates, &parameters, Some(1), Some("gain"))
                .is_err()
        );
        assert!(
            checked_field_evaluation(&run, &coordinates, &parameters, Some(9), Some("absent"))
                .is_err()
        );
    }

    #[test]
    fn field_mesh_descriptor_matches_one_saved_case_and_validates_each_artifact() {
        let mut fixture = Fixture::new();
        fixture.directory("runs");
        fixture.directory("runs/checked");
        fixture.directory("runs/checked/mesh");
        let mesh = "mesh/b-mesh.mesh";
        let map = "mesh/b-model-map.json";
        fixture.file(&format!("runs/checked/{mesh}"), "native mesh");
        fixture.file(&format!("runs/checked/{map}"), "map data");
        let entry = |name: &str, value: &str| json!({"path":name,"bytes":value.len(),"sha256":crate::sha256::hex_digest(value.as_bytes())});
        fixture.file("runs/checked/mesh/b-mesh-cache.json", &json!({
            "schema":"fullmeta-mesh-cache/3","geometryHash":"case-b",
            "files":{"mesh":entry("b-mesh.mesh", "native mesh"),"map":entry("b-model-map.json", "map data")},
        }).to_string());
        let runs_directory = fixture.root.join("runs");
        let scope = ResultScope {
            root: &fixture.root,
            project: &fixture.root,
            document: &fixture.root,
            runs_directory: &runs_directory,
            model: &Value::Null,
        };
        let mut run = Run {
            id: "checked".into(),
            directory: runs_directory.join("checked"),
            record: json!({
                "geometry":{"geometryHash":"case-a","mesh":"mesh/a-mesh.mesh","cases":[
                    {"caseIndex":0,"geometryHash":"case-a","mesh":"mesh/a-mesh.mesh"},
                    {"caseIndex":1,"geometryHash":"case-b","mesh":mesh,"map":map},
                ]},
            }),
        };
        assert!(checked_field_mesh(&scope, &run, 0, false).is_none());
        let selected = checked_field_mesh(&scope, &run, 1, false).unwrap();
        assert_eq!(selected["path"], format!("runs/checked/{mesh}"));
        assert_eq!(selected["runId"], "checked");
        assert_eq!(selected["caseIndex"], 1);
        assert_eq!(selected["modelMap"]["path"], format!("runs/checked/{map}"));
        fs::write(run.directory.join(map), "bad data").unwrap();
        assert!(
            checked_field_mesh(&scope, &run, 1, false)
                .unwrap()
                .get("modelMap")
                .is_none()
        );
        fs::write(run.directory.join(mesh), "broken mesh").unwrap();
        assert!(checked_field_mesh(&scope, &run, 1, false).is_none());
        run.record["geometry"] = json!({"geometryHash":"case-b","mesh":mesh,
            "files":{"mesh":entry(mesh, "broken mesh")}});
        assert!(checked_field_mesh(&scope, &run, 1, false).is_none());
        assert!(checked_field_mesh(&scope, &run, 0, true).is_some());
    }

    #[test]
    fn missing_indexed_field_reports_its_recorded_path() {
        let mut fixture = Fixture::new();
        fixture.directory("output");
        let run = Run {
            id: "checked".into(),
            directory: fixture.root.clone(),
            record: json!({
                "artifacts":{"fieldFiles":[{"name":"field.json","caseIndex":0,"frequencyHz":1e9}]}
            }),
        };
        let message =
            list_field_files(&run, "fem_runs/checked", &Value::Null, &Value::Null).unwrap_err();
        assert!(message.starts_with(
            "fem_runs/checked/output/field.json: could not read indexed field artifact:"
        ));
        fixture.file("output/field.json", "");
        let message =
            list_field_files(&run, "fem_runs/checked", &Value::Null, &Value::Null).unwrap_err();
        assert_eq!(
            message,
            "fem_runs/checked/output/field.json: expected a nonempty regular field artifact."
        );
    }

    #[test]
    fn study_fingerprint_reads_verified_legacy_snapshots_and_rejects_tampering() {
        let original = study_fingerprint::tests::specimen();
        let expected = study_input_fingerprint(&original, "study")
            .expect("valid spec-derived input projection");
        let mut record = json!({
            "model":{"sha256":study_fingerprint::snapshot_digest(&original)},
            "study":{"id":"study"},
        });
        assert_eq!(
            saved_fingerprint(&record, &original, "study"),
            Some(expected.clone())
        );
        record["study"]["inputFingerprint"] = expected.clone();
        assert_eq!(
            saved_fingerprint(&record, &original, "study"),
            Some(expected)
        );
        record["study"]["inputFingerprint"]["version"] = json!(999);
        assert!(saved_fingerprint(&record, &original, "study").is_none());
        let mut modified = original;
        modified["model"]["parameters"][0]["expression"] = json!("2000000000");
        record["study"]
            .as_object_mut()
            .unwrap()
            .remove("inputFingerprint");
        assert!(saved_fingerprint(&record, &modified, "study").is_none());
    }

    #[test]
    fn study_fingerprint_prefers_older_recipe_compatible_output_and_preserves_document_scope() {
        let model = study_fingerprint::tests::specimen();
        let mut fixture = Fixture::new();
        fixture.file("first.json", &model.to_string());
        fixture.file("sibling.json", &model.to_string());
        fixture.directory("runs");
        fixture.run(
            "first",
            "2026-01-01T00:00:00Z",
            "first.json",
            &model,
            "frequency_GHz,signal\n1,2\n",
        );
        fixture.run(
            "newerMissingSeries",
            "2026-01-02T00:00:00Z",
            "first.json",
            &model,
            "frequency_GHz,other\n1,2\n",
        );
        fixture.run(
            "sibling",
            "2026-01-03T00:00:00Z",
            "sibling.json",
            &model,
            "frequency_GHz,signal\n1,3\n",
        );
        let mut changed = model.clone();
        changed["model"]["parameters"][0]["expression"] = json!("2000000000");
        fixture.run(
            "stale",
            "2026-01-04T00:00:00Z",
            "first.json",
            &changed,
            "frequency_GHz,signal\n2,4\n",
        );
        let document = fixture.root.join("first.json").canonicalize().unwrap();
        let runs_directory = fixture.root.join("runs");
        let scope = ResultScope {
            root: &fixture.root,
            document: &document,
            project: &fixture.root,
            runs_directory: &runs_directory,
            model: &model,
        };
        let action = json!({
            "type":"plot.sweep",
            "datasets":[{"id":"plot","source":{"type":"study","studyId":"study"},"xColumn":"frequency_GHz"}],
            "series":[{"column":"signal"}]
        });
        let resolved = resolve_sweep(&action, &scope).unwrap();
        assert_eq!(resolved["datasets"][0]["runId"], "first");
        assert_eq!(resolved["datasets"][0]["modelFreshness"], "current");
    }

    #[test]
    fn sweep_accepts_saved_global_symbol_and_explicit_imaginary_column() {
        let model = study_fingerprint::tests::specimen();
        let mut fixture = Fixture::new();
        fixture.file("model.json", &model.to_string());
        fixture.directory("runs");
        fixture.run(
            "checked",
            "2026-01-01T00:00:00Z",
            "model.json",
            &model,
            "frequency_GHz,summary_re,summary_im\n1,2,3\n",
        );
        let document = fixture.root.join("model.json").canonicalize().unwrap();
        let runs_directory = fixture.root.join("runs");
        let scope = ResultScope {
            root: &fixture.root,
            document: &document,
            project: &fixture.root,
            runs_directory: &runs_directory,
            model: &model,
        };
        let action = json!({
            "type":"plot.sweep",
            "datasets":[{"id":"plot","source":{"type":"study","studyId":"study"},"xColumn":"frequency_GHz"}],
            "series":[{"column":"summary"},{"column":"summary_im"}], "tableColumns":["summary","summary_im"]
        });
        let resolved = resolve_sweep(&action, &scope).unwrap();
        assert_eq!(resolved["datasets"][0]["runId"], "checked");
        assert_eq!(
            resolved["datasets"][0]["storedVariableUnits"]["summary"],
            "m^3"
        );
        assert_eq!(resolved["action"]["series"], action["series"]);
    }
}

fn study_display_name<'a>(model: &'a Value, study_id: &'a str) -> &'a str {
    model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|study| study["id"].as_str() == Some(study_id))
        .and_then(|study| study["name"].as_str())
        .unwrap_or(study_id)
}

fn run_timestamp<'a>(record: &'a Value, id: &'a str) -> &'a str {
    record
        .get("completedAt")
        .and_then(Value::as_str)
        .or_else(|| record.get("startedAt").and_then(Value::as_str))
        .unwrap_or(id)
}

fn list_field_files(
    run: &Run,
    run_relative: &str,
    model: &Value,
    request: &Value,
) -> Result<Vec<Value>> {
    let output = run.directory.join("output");
    let output_relative = format!("{run_relative}/output");
    let output_info = fs::symlink_metadata(&output).map_err(|error| {
        format!("{output_relative}: could not inspect field output directory: {error}")
    })?;
    if !output_info.is_dir() || is_link(&output_info) {
        return Err(format!(
            "{output_relative}: expected a regular field output directory."
        ));
    }
    let check_indexed_file = |name: &str| -> Result<()> {
        let relative = format!("{output_relative}/{name}");
        if !valid_single_component(name) || !name.ends_with(".json") {
            return Err(format!("{relative}: invalid saved field filename."));
        }
        let info = fs::symlink_metadata(output.join(name)).map_err(|error| {
            format!("{relative}: could not read indexed field artifact: {error}")
        })?;
        if is_link(&info) || !info.is_file() || info.len() == 0 {
            return Err(format!(
                "{relative}: expected a nonempty regular field artifact."
            ));
        }
        Ok(())
    };
    let cases = request
        .get("frequencyCases")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let axes = run
        .record
        .pointer("/study/snapshot/parameterSweep/parameters")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let label_for = |case_index: Option<usize>, frequency: f64, fallback: usize| {
        let parameter_rows = case_index
            .and_then(|index| cases.get(index))
            .and_then(|case| case.get("parameterOverrides"))
            .and_then(Value::as_array);
        let details = axes
            .iter()
            .filter_map(|axis| {
                let parameter = axis.get("parameter").and_then(Value::as_str)?;
                let row = parameter_rows?
                    .iter()
                    .find(|row| row.get("symbol").and_then(Value::as_str) == Some(parameter))?;
                let expression = row.get("expression").and_then(Value::as_str)?;
                let unit = row
                    .get("unit")
                    .and_then(Value::as_str)
                    .filter(|unit| *unit != "1")
                    .map(|unit| format!(" {unit}"))
                    .unwrap_or_default();
                Some(format!("{parameter}={expression}{unit}"))
            })
            .collect::<Vec<_>>();
        let detail = if details.is_empty() {
            format!("{:.7} GHz", frequency / 1e9)
        } else {
            details.join(", ")
        };
        format!(
            "Case {} · {}",
            case_index.map(|index| index + 1).unwrap_or(fallback + 1),
            detail
        )
    };
    let profile = model.pointer("/scenario/profileId").and_then(Value::as_str);
    // Stationary thermal artifacts are indexed without a frequencyHz value;
    // handle them before the generic frequency-indexed harmonic branch.
    if profile == Some("thermal-2d") {
        let indexed = run
            .record
            .pointer("/artifacts/fieldFiles")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|item| item.get("name").and_then(Value::as_str))
            .collect::<Vec<_>>();
        let fallback = run
            .record
            .pointer("/artifacts/fields")
            .and_then(Value::as_str)
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str());
        let names = if indexed.is_empty() {
            fallback.into_iter().collect()
        } else {
            indexed
        };
        let mut files = Vec::new();
        for name in names {
            check_indexed_file(name)?;
            files.push(json!({"name":name,"caseIndex":0,"label":"Stationary temperature"}));
        }
        return Ok(files);
    }
    let indexed = run
        .record
        .pointer("/artifacts/fieldFiles")
        .and_then(Value::as_array);
    if let Some(indexed) = indexed {
        let mut files = Vec::new();
        for item in indexed {
            let Some(name) = item.get("name").and_then(Value::as_str) else {
                continue;
            };
            let case_index = item
                .get("caseIndex")
                .and_then(Value::as_u64)
                .map(|value| value as usize);
            let frequency = item
                .get("frequencyHz")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value > 0.0)
                .ok_or_else(|| {
                    format!("{output_relative}/{name}: saved field index has an invalid frequency.")
                })?;
            check_indexed_file(name)?;
            files.push(json!({"name":name,"frequencyHz":frequency,"caseIndex":case_index,"label":label_for(case_index,frequency,files.len())}));
        }
        files.sort_by(|left, right| {
            let lc = left
                .get("caseIndex")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX);
            let rc = right
                .get("caseIndex")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX);
            lc.cmp(&rc).then_with(|| {
                left.get("frequencyHz")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
                    .total_cmp(
                        &right
                            .get("frequencyHz")
                            .and_then(Value::as_f64)
                            .unwrap_or(0.0),
                    )
            })
        });
        return Ok(files);
    }
    // Older harmonic runs used a filename template instead of an index.
    let template = run
        .record
        .pointer("/artifacts/fields")
        .and_then(Value::as_str)
        .unwrap_or("");
    let filename = Path::new(template)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let Some(marker_start) = filename.find("<frequency-") else {
        return Ok(Vec::new());
    };
    let Some(marker_end_rel) = filename[marker_start..].find('>') else {
        return Ok(Vec::new());
    };
    let marker_end = marker_start + marker_end_rel + 1;
    let marker = &filename[marker_start..marker_end];
    let scale = if marker.contains("-GHz-") { 1e9 } else { 1.0 };
    let prefix = &filename[..marker_start];
    let suffix = &filename[marker_end..];
    let mut files = Vec::new();
    fn walk(
        directory: &Path,
        base: &Path,
        prefix: &str,
        suffix: &str,
        scale: f64,
        files: &mut Vec<Value>,
    ) -> Result<()> {
        let Ok(entries) = fs::read_dir(directory) else {
            return Ok(());
        };
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let info = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
            if is_link(&info) {
                continue;
            }
            if info.is_dir() {
                walk(&entry.path(), base, prefix, suffix, scale, files)?;
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            if !info.is_file()
                || info.len() == 0
                || !name.starts_with(prefix)
                || !name.ends_with(suffix)
            {
                continue;
            }
            let number = &name[prefix.len()..name.len() - suffix.len()];
            let Ok(value) = number.parse::<f64>() else {
                continue;
            };
            if !value.is_finite() || value <= 0.0 {
                continue;
            }
            let entry_path = entry.path();
            let relative = entry_path
                .strip_prefix(base)
                .map_err(|error| error.to_string())?;
            let relative_name = slash_path(relative);
            let case_index = relative_name
                .split('/')
                .find_map(|part| part.strip_prefix("case-")?.parse::<usize>().ok())
                .map(|index| index.saturating_sub(1));
            files.push(json!({"name":relative_name,"frequencyHz":value*scale,"caseIndex":case_index,"label":format!("Case {} · {:.7} GHz",case_index.map(|index|index+1).unwrap_or(files.len()+1),value*scale/1e9)}));
        }
        Ok(())
    }
    walk(&output, &output, prefix, suffix, scale, &mut files)?;
    files.sort_by(|left, right| {
        left.get("frequencyHz")
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
            .total_cmp(
                &right
                    .get("frequencyHz")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
            )
    });
    Ok(files)
}

pub(super) fn checked_file_maybe(base: &Path, relative: &str) -> Result<Option<PathBuf>> {
    let relative_path = validated_relative(relative)?;
    let components = relative_path.components().collect::<Vec<_>>();
    let mut current = base.to_path_buf();
    for (index, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            continue;
        };
        current.push(name);
        let info = match fs::symlink_metadata(&current) {
            Ok(info) => info,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("Could not inspect result path: {error}")),
        };
        if is_link(&info) || (index + 1 < components.len() && !info.is_dir()) {
            return Err(
                "Result paths must use regular files and directories without symbolic links."
                    .into(),
            );
        }
    }
    let actual = current
        .canonicalize()
        .map_err(|error| format!("Could not resolve result path: {error}"))?;
    let base = base
        .canonicalize()
        .map_err(|error| format!("Could not resolve result folder: {error}"))?;
    if !actual.starts_with(base) || !actual.is_file() {
        return Err("A result artifact must be a regular file inside its project folder.".into());
    }
    Ok(Some(actual))
}

pub(super) fn checked_existing_path(
    root: &Path,
    relative: &str,
    expect_directory: bool,
) -> Result<PathBuf> {
    let relative = validated_relative(relative)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            continue;
        };
        current.push(name);
        let info = fs::symlink_metadata(&current)
            .map_err(|error| format!("Could not resolve model path: {error}"))?;
        if is_link(&info) {
            return Err("Model paths must not contain symbolic links.".into());
        }
    }
    let actual = current
        .canonicalize()
        .map_err(|error| format!("Could not resolve model path: {error}"))?;
    if !actual.starts_with(root) || (expect_directory && !actual.is_dir()) {
        return Err("The model folder must be inside FullMeta.".into());
    }
    Ok(actual)
}

fn validated_relative(value: &str) -> Result<PathBuf> {
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if normalized.is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return Err("The requested path must be relative and stay inside FullMeta.".into());
    }
    Ok(path.to_path_buf())
}

fn artifact_url(root: &Path, path: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "Result artifacts must stay inside FullMeta.".to_string())?;
    let path = slash_path(relative);
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            )
        {
            encoded.push(byte as char)
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    Ok(format!("/api/kernel/artifact?path={encoded}"))
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
pub(super) fn valid_single_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value.contains(['/', '\\', ':', '\0'])
        && value != "."
        && value != ".."
}
fn storage_name(directory: &Path) -> Result<String> {
    directory
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .ok_or("The run storage path has no directory name.".into())
}
pub(super) fn read_json(path: &Path) -> Result<Value> {
    let bytes =
        fs::read(path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid JSON in {}: {error}", path.display()))
}
fn is_link(info: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const REPARSE: u32 = 0x400;
        info.file_type().is_symlink() || info.file_attributes() & REPARSE != 0
    }
    #[cfg(not(windows))]
    {
        info.file_type().is_symlink()
    }
}
