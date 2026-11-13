//! Converts a submitted FullMeta model and Study into the solver's run request.
//!
//! The common FEM dispatcher expands frequency cases and prepares solver data
//! using contracts declared by the scenario schemas. Material expressions remain
//! authoritative and are sampled by the solver on its actual mesh. The editor
//! supplies the model snapshot, selected Study, and filesystem paths.
//!
//! Entry precondition: the dispatcher has validated both the model and the
//! preparation envelope against their deployed schema rule programs. This
//! module extracts those validated declarations and performs numerical work;
//! model admissibility and request-format criteria are not repeated here.

use crate::model_expression::{self, Scalar};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};

type Result<T> = std::result::Result<T, String>;

fn field<'a>(value: &'a Value, key: &str, path: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| format!("{path}.{key} is required."))
}
fn text<'a>(value: &'a Value, path: &str) -> Result<&'a str> {
    value
        .as_str()
        .ok_or_else(|| format!("{path} must be a string."))
}
fn array<'a>(value: &'a Value, path: &str) -> Result<&'a [Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{path} must be an array."))
}
fn str_field<'a>(value: &'a Value, key: &str, path: &str) -> Result<&'a str> {
    text(field(value, key, path)?, &format!("{path}.{key}"))
}
fn opt_string<'a>(value: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or(fallback)
}
fn find_id<'a>(items: &'a [Value], id: &str) -> Option<&'a Value> {
    items
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
}

fn values_by_symbol(model: &Value, parameters: &[Value]) -> Result<HashMap<String, Scalar>> {
    model_expression::resolve_parameters(parameters, model)
}
fn quantity(value: &Value, symbols: &HashMap<String, Scalar>, path: &str) -> Result<f64> {
    model_expression::evaluate_quantity_with_symbols(value, symbols, path)
}
fn sweep_values(sweep: &Value, symbols: &HashMap<String, Scalar>, path: &str) -> Result<Vec<f64>> {
    match sweep.get("type").and_then(Value::as_str) {
        Some("linear") => {
            let count = field(sweep, "count", path)?
                .as_u64()
                .and_then(|count| usize::try_from(count).ok())
                .ok_or_else(|| {
                    format!("{path}.count cannot be represented as an allocation size.")
                })?;
            let intervals = count
                .checked_sub(1)
                .filter(|count| *count > 0)
                .ok_or_else(|| {
                    format!("{path} has no interpolation intervals after validation.")
                })?;
            let start = quantity(
                field(sweep, "start", path)?,
                symbols,
                &format!("{path}.start"),
            )?;
            let end = quantity(field(sweep, "end", path)?, symbols, &format!("{path}.end"))?;
            let mut points = Vec::new();
            points.try_reserve_exact(count).map_err(|error| {
                format!("Could not allocate the evaluated sweep points for {path}: {error}")
            })?;
            points.extend(
                (0..count).map(|index| start + (end - start) * index as f64 / intervals as f64),
            );
            Ok(points)
        }
        Some("values") => {
            let items = array(field(sweep, "values", path)?, &format!("{path}.values"))?;
            let mut points = Vec::new();
            points.try_reserve_exact(items.len()).map_err(|error| {
                format!("Could not allocate the evaluated sweep points for {path}: {error}")
            })?;
            for (index, item) in items.iter().enumerate() {
                points.push(quantity(item, symbols, &format!("{path}.values[{index}]"))?);
            }
            Ok(points)
        }
        _ => Err(format!(
            "Cannot extract the validated sweep representation at {path}."
        )),
    }
}

struct PreparedFrequencyCase {
    frequency: f64,
    symbols: HashMap<String, Scalar>,
    parameters: Vec<Value>,
}

fn frequency_cases(
    study: &Value,
    physics: &Value,
    model: &Value,
) -> Result<Vec<PreparedFrequencyCase>> {
    let model_root = field(model, "model", "model")?;
    let parameters = array(
        field(model_root, "parameters", "model")?,
        "model.parameters",
    )?;
    let sweep_parameters = match study.get("parameterSweep") {
        Some(sweep) => array(
            field(sweep, "parameters", "Parameter sweep")?,
            "Parameter sweep.parameters",
        )?,
        None => &[],
    };
    let physics_id = str_field(physics, "id", "physics")?;
    let frequency_expression = field(physics, "frequency", "physics")?;
    let baseline_symbols = values_by_symbol(model, parameters)?;
    let mut axes = Vec::with_capacity(sweep_parameters.len());
    for (index, sweep_parameter) in sweep_parameters.iter().enumerate() {
        let parameter_path = format!("Parameter sweep parameter {}", index + 1);
        let symbol = str_field(sweep_parameter, "parameter", &parameter_path)?;
        let parameter = parameters
            .iter()
            .find(|entry| entry.get("symbol").and_then(Value::as_str) == Some(symbol))
            .ok_or_else(|| format!("Cannot extract validated sweep parameter {symbol}."))?;
        let values = field(sweep_parameter, "values", &parameter_path)?;
        let points = sweep_values(
            values,
            &baseline_symbols,
            &format!("{parameter_path}.values"),
        )?;
        let (unit_scale, _) = model_expression::unit(str_field(
            parameter,
            "unit",
            &format!("parameter {symbol}"),
        )?)?;
        axes.push((symbol.to_string(), points, unit_scale));
    }
    axes.sort_by(|left, right| left.0.cmp(&right.0));
    let total_points = axes
        .iter()
        .try_fold(1usize, |total, (_, values, _)| {
            total.checked_mul(values.len())
        })
        .ok_or("The evaluated sweep cardinality cannot be represented as an allocation size.")?;
    let mut expanded_parameters = Vec::new();
    expanded_parameters
        .try_reserve_exact(total_points)
        .map_err(|error| format!("Could not allocate the evaluated sweep cases: {error}"))?;
    expand_parameter_axes(&axes, 0, parameters.to_vec(), &mut expanded_parameters)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(expanded_parameters.len())
        .map_err(|error| format!("Could not allocate the prepared frequency cases: {error}"))?;
    for (index, case_parameters) in expanded_parameters.iter().enumerate() {
        let resolved = values_by_symbol(model, case_parameters)?;
        let frequency = quantity(
            frequency_expression,
            &resolved,
            &format!("{physics_id}.frequency[{index}]"),
        )?;
        output.push(PreparedFrequencyCase {
            frequency,
            symbols: resolved,
            parameters: case_parameters.clone(),
        });
    }
    Ok(output)
}

fn expand_parameter_axes(
    axes: &[(String, Vec<f64>, f64)],
    axis_index: usize,
    parameters: Vec<Value>,
    output: &mut Vec<Vec<Value>>,
) -> Result<()> {
    if axis_index == axes.len() {
        output.push(parameters);
        return Ok(());
    }
    let (symbol, points, unit_scale) = &axes[axis_index];
    for point in points {
        let mut case_parameters = parameters.clone();
        let target = case_parameters
            .iter_mut()
            .find(|entry| entry.get("symbol").and_then(Value::as_str) == Some(symbol.as_str()))
            .ok_or_else(|| {
                format!("Parameter sweep references unknown model parameter {symbol}.")
            })?;
        target["expression"] = json!((point / unit_scale).to_string());
        expand_parameter_axes(axes, axis_index + 1, case_parameters, output)?;
    }
    Ok(())
}

/// Integration operators can be referenced by their stable ID or their
/// user-facing symbol. Expressions normally use the symbol, while prepared
/// solver data uses the stable ID.
fn integration_operator_aliases(operators: &[Value]) -> Result<HashMap<String, String>> {
    let mut aliases = HashMap::<String, String>::new();
    for (index, operator) in operators.iter().enumerate() {
        let path = format!("model.integrationOperators[{index}]");
        let id = str_field(operator, "id", &path)?.to_string();
        let name = opt_string(operator, "name", &id).to_string();
        for alias in [id.as_str(), name.as_str()] {
            aliases.insert(alias.to_string(), id.clone());
        }
    }
    Ok(aliases)
}

fn normalize_integration_operator_calls(node: &mut Value, aliases: &HashMap<String, String>) {
    if node.get("kind").and_then(Value::as_str) == Some("call") {
        if let Some(name) = node.get("name").and_then(Value::as_str).map(str::to_string) {
            if let Some(id) = aliases.get(&name) {
                node["name"] = json!(id);
            }
        }
    }
    match node {
        Value::Array(items) => {
            for item in items {
                normalize_integration_operator_calls(item, aliases);
            }
        }
        Value::Object(object) => {
            for item in object.values_mut() {
                normalize_integration_operator_calls(item, aliases);
            }
        }
        _ => {}
    }
}

fn variables_for_study(
    model: &Value,
    component: &Value,
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let model_root = field(model, "model", "model")?;
    let variable_values = array(field(model_root, "variables", "model")?, "model.variables")?;
    let operators = model
        .pointer("/model/integrationOperators")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let operator_aliases = integration_operator_aliases(&operators)?;
    let pointwise_values = model
        .pointer("/model/pointwiseVariables")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut pointwise_output = Vec::with_capacity(pointwise_values.len());
    for (index, variable) in pointwise_values.iter().enumerate() {
        let path = format!("model.pointwiseVariables[{index}]");
        let symbol = str_field(variable, "symbol", &path)?;
        let expression = str_field(variable, "expression", &path)?;
        let (ast, _, _) = model_expression::inspect(expression)
            .map_err(|error| format!("{path}.expression: {error}"))?;
        pointwise_output.push(json!({"symbol":symbol,"ast":ast}));
    }
    let mut definitions = HashMap::<String, Value>::new();
    let mut variable_order = Vec::<String>::new();
    let mut symbols_by_var = HashMap::<String, BTreeSet<String>>::new();
    let mut calls_by_var = HashMap::<String, BTreeSet<String>>::new();
    for (index, variable) in variable_values.iter().enumerate() {
        let path = format!("model.variables[{index}]");
        let symbol = str_field(variable, "symbol", &path)?.to_string();
        let expression = str_field(variable, "expression", &path)?;
        let (mut ast, symbols, calls) = model_expression::inspect(expression)
            .map_err(|error| format!("{path}.expression: {error}"))?;
        normalize_integration_operator_calls(&mut ast, &operator_aliases);
        let mut definition = variable.clone();
        definition["ast"] = ast;
        definitions.insert(symbol.clone(), definition);
        variable_order.push(symbol.clone());
        symbols_by_var.insert(symbol.clone(), symbols);
        calls_by_var.insert(symbol, calls);
    }
    let mut ordered_symbols = Vec::new();
    let mut complete = HashSet::new();
    fn visit_variable(
        symbol: &str,
        definitions: &HashMap<String, Value>,
        dependencies: &HashMap<String, BTreeSet<String>>,
        complete: &mut HashSet<String>,
        ordered: &mut Vec<String>,
    ) {
        // The schema's expression validator already proved acyclicity. Mark
        // on entry to keep this output-order traversal bounded independently.
        if !complete.insert(symbol.to_string()) {
            return;
        }
        if let Some(references) = dependencies.get(symbol) {
            for dependency in references {
                if definitions.contains_key(dependency) {
                    visit_variable(dependency, definitions, dependencies, complete, ordered);
                }
            }
        }
        ordered.push(symbol.to_string());
    }
    for symbol in &variable_order {
        visit_variable(
            symbol,
            &definitions,
            &symbols_by_var,
            &mut complete,
            &mut ordered_symbols,
        );
    }
    let ordered = ordered_symbols
        .iter()
        .filter_map(|symbol| definitions.get(symbol).cloned())
        .collect::<Vec<_>>();
    let referenced_calls = calls_by_var
        .values()
        .flat_map(|calls| calls.iter())
        .filter_map(|call| operator_aliases.get(call).cloned())
        .collect::<HashSet<_>>();
    let mut integration_output = Vec::new();
    for operator in operators.iter().filter(|operator| {
        operator
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| referenced_calls.contains(id))
    }) {
        let id = str_field(operator, "id", "integrationOperator")?;
        let name = opt_string(operator, "name", id);
        let selection_id = str_field(
            operator,
            "selectionId",
            &format!("integration operator {name}"),
        )?;
        let selection = find_id(
            array(field(component, "selections", "model")?, "model.selections")?,
            selection_id,
        )
        .ok_or_else(|| {
            format!("Integration operator {name} references unknown selection {selection_id}.")
        })?;
        let operator_type = crate::model_schema::integration_kind_for_selection(selection)
            .ok_or_else(|| format!("Integration operator {name} selection {selection_id} has no schema-declared integration kind."))?;
        // The mesh map resolves this selection to tagged mesh entities.
        // Selection expressions are interpreted by the mesher.
        integration_output
            .push(json!({"id":id,"integrationKind":operator_type,"selectionId":selection_id}));
    }
    Ok((ordered, integration_output, pointwise_output))
}

fn periodic_pairs_for_symbols(
    component: &Value,
    physics: &Value,
    symbols: &HashMap<String, Scalar>,
) -> Result<Vec<Value>> {
    let mesh = field(component, "mesh", "model")?;
    let pairs = array(
        field(mesh, "periodicPairs", "model.mesh")?,
        "model.mesh.periodicPairs",
    )?;
    let mut periodic_request = Vec::new();
    for feature in crate::model_schema::physics_periodic_conditions(physics)
        .filter(|feature| feature.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        let pair_id = str_field(feature, "periodicPairId", "periodic condition")?;
        let pair = find_id(pairs, pair_id)
            .ok_or_else(|| format!("Unknown mesh periodic pair {pair_id}."))?;
        let translation = array(
            field(pair, "translation", pair_id)?,
            &format!("{pair_id}.translation"),
        )?;
        let phase = feature
            .get("phaseVector")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| vec![json!({"expression":"0","unit":"rad/m"}); 3]);
        let translation = translation
            .iter()
            .enumerate()
            .map(|(index, value)| {
                quantity(value, symbols, &format!("{pair_id}.translation[{index}]"))
            })
            .collect::<Result<Vec<_>>>()?;
        let phase = phase
            .iter()
            .enumerate()
            .map(|(index, value)| {
                quantity(value, symbols, &format!("{pair_id}.phaseVector[{index}]"))
            })
            .collect::<Result<Vec<_>>>()?;
        periodic_request.push(json!({
            "id":pair_id,
            "lowerSelectionId":field(pair, "lowerSelectionId", pair_id)?,
            "upperSelectionId":field(pair, "upperSelectionId", pair_id)?,
            "translationM":translation,
            "phaseVector":phase
        }));
    }
    Ok(periodic_request)
}

/// Installed schemas select a common preparation contract or a scenario adapter.
pub fn uses_common_preparation(model: &Value, capabilities: &Value) -> Result<bool> {
    let profile = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .ok_or("The model requires scenario.profileId.")?;
    Ok(matches!(
        capabilities
            .pointer("/study/preparationByProfile")
            .and_then(|profiles| profiles.get(profile))
            .and_then(|entry| entry.get("kind"))
            .and_then(Value::as_str),
        Some("frequencyPlan" | "configuredFrequencyRequest")
    ))
}

pub(crate) fn prepare_validated_study(envelope: &Value, capabilities: &Value) -> Result<Value> {
    let model = field(envelope, "model", "prepare request")?;
    let profile_id = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .ok_or("The model requires scenario.profileId.")?;
    let preparation = capabilities
        .pointer("/study/preparationByProfile")
        .and_then(|profiles| profiles.get(profile_id))
        .and_then(|entry| entry.get("kind"))
        .and_then(Value::as_str)
        .ok_or("The scenario schema must declare a Study preparation contract.")?;
    if !uses_common_preparation(model, capabilities)? {
        return Err(format!(
            "Preparation contract {preparation:?} is owned by its scenario adapter."
        ));
    }
    let model_root = field(model, "model", "model")?;
    let study_id = str_field(envelope, "studyId", "prepare request")?;
    let study = find_id(
        array(field(model_root, "study", "model")?, "model.study")?,
        study_id,
    )
    .ok_or_else(|| format!("Unknown study {study_id}."))?;
    let capability_study = field(capabilities, "study", "engineCapabilities")?;
    let settings_catalog = field(
        capability_study,
        "solverSettingsByProfile",
        "engineCapabilities.study",
    )?;
    let profile_settings = settings_catalog.get(profile_id).ok_or_else(|| {
        format!("No solver settings are declared for scenario profile {profile_id:?}.")
    })?;
    let defaults = field(profile_settings, "defaults", "scenario solver settings")?;
    let component = model_root;
    let physics_id = str_field(study, "physicsId", "study")?;
    let physics = crate::model_schema::physics(component)
        .filter(|physics| physics.get("id").and_then(Value::as_str) == Some(physics_id))
        .ok_or_else(|| {
            format!("Study {study_id} references unknown Physics interface {physics_id}.")
        })?;
    let discretization = study.get("discretization").unwrap_or(&Value::Null);
    let solver = study.get("solver").unwrap_or(&Value::Null);
    let physics_formulation = physics.get("formulation").and_then(Value::as_str);
    let default_formulation = if profile_settings
        .get("fieldFormulationsFromPhysics")
        .and_then(Value::as_bool)
        == Some(true)
    {
        physics_formulation
            .ok_or("The linked Physics interface does not declare a field formulation.")?
    } else {
        str_field(
            defaults,
            "fieldFormulation",
            "scenario solver settings.defaults",
        )?
    };
    let formulation = discretization
        .get("fieldFormulation")
        .and_then(Value::as_str)
        .unwrap_or(default_formulation);
    let order_default = field(
        defaults,
        "elementOrder",
        "scenario solver settings.defaults",
    )?
    .as_u64()
    .ok_or("Scenario default elementOrder must be an unsigned integer.")?;
    let order = discretization
        .get("elementOrder")
        .and_then(Value::as_u64)
        .unwrap_or(order_default);
    let solver_settings = Value::Object(crate::solver_options::resolve(
        capability_study,
        profile_settings,
        solver,
        formulation,
        order,
    )?);
    let backend = str_field(&solver_settings, "backend", "resolved solver settings")?;
    let constitutive_quadrature = str_field(
        &solver_settings,
        "constitutiveQuadrature",
        "resolved solver settings",
    )?;
    let ordering = str_field(&solver_settings, "ordering", "resolved solver settings")?;
    let threads = solver_settings
        .get("threads")
        .and_then(Value::as_u64)
        .ok_or("Resolved solver settings omit an unsigned thread count.")?;
    let cases = frequency_cases(study, physics, model)?;
    let frequencies_hz = cases.iter().map(|case| case.frequency).collect::<Vec<_>>();
    for (index, frequency) in frequencies_hz.iter().enumerate() {
        crate::model_validation::validate_prepared_frequency(
            *frequency,
            &format!("{study_id}.frequency[{index}]"),
        )?;
    }
    let frequency_cases = cases.iter().map(|case| {
        let global_parameters = case.symbols.iter().map(|(symbol, scalar)| (symbol.clone(), json!(scalar.value))).collect::<Map<_,_>>();
        json!({"frequencyHz":case.frequency,"globalParameters":global_parameters,"globalParameterDimensions":case.symbols.iter().map(|(name,value)|(name.clone(),json!(value.dimension))).collect::<Map<_,_>>(),"parameterOverrides":case.parameters})
    }).collect::<Vec<_>>();
    if preparation == "frequencyPlan" {
        let output = study.get("output").unwrap_or(&Value::Null);
        let csv_name = envelope
            .get("csvFilename")
            .and_then(Value::as_str)
            .or_else(|| output.get("csvFilename").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| format!("{study_id}.csv"));
        let field_files = output
            .get("fieldFiles")
            .and_then(Value::as_bool)
            .or_else(|| {
                capability_study
                    .pointer("/outputDefaults/fieldFiles")
                    .and_then(Value::as_bool)
            })
            .ok_or("Study output defaults must declare fieldFiles.")?;
        return Ok(
            json!({"schema":"fullmeta-study-plan/1","studyId":study_id,"profileId":profile_id,
            "solverSettings":{"fieldFormulation":formulation,"elementOrder":order,"constitutiveQuadrature":constitutive_quadrature,
                "solverBackend":backend,"solverOrdering":ordering,"solverThreads":threads},
            "frequenciesHz":frequencies_hz,"frequencyCases":frequency_cases,
            "csvFilename":csv_name,"fieldFiles":field_files}),
        );
    }
    let mesh_file = str_field(envelope, "meshFile", "prepare request")?;
    let parameters = array(
        field(model_root, "parameters", "model")?,
        "model.parameters",
    )?;
    let symbols = values_by_symbol(model, parameters)?;
    let (variables, integration_operators, pointwise_variables) =
        variables_for_study(model, component)?;
    let mut impedance_boundaries = Vec::new();
    for boundary in crate::model_schema::physics_boundary_conditions(physics)
        .filter(|boundary| boundary.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        let id = str_field(boundary, "id", "impedance boundary")?;
        if boundary.get("type").and_then(Value::as_str) != Some("impedance") {
            return Err(format!(
                "Profile {profile_id}, Study {study_id}, preparation: enabled boundary {id} must use impedance."
            ));
        }
        let selection_id = str_field(boundary, "selectionId", id)?;
        let mut prepared =
            json!({"id":id,"selectionId":selection_id,"impedance":field(boundary,"impedance",id)?});
        if let Some(incident) = boundary.get("incidentField") {
            prepared["incidentField"] = incident.clone();
            prepared["polarization"] = boundary
                .get("polarization")
                .cloned()
                .ok_or_else(|| format!("Boundary {id} requires incident polarization."))?;
        }
        impedance_boundaries.push(prepared);
    }
    let periodic_request = periodic_pairs_for_symbols(component, physics, &symbols)?;
    let mut assignments = Vec::new();
    for (index, material) in array(field(component, "materials", "model")?, "model.materials")?
        .iter()
        .enumerate()
    {
        assignments.push(json!({"selectionId":field(material,"selectionId","material")?,"materialId":material.get("id"),"physicalTag":index+1,"material":material}));
    }
    let domain_selection_ids = physics
        .get("domainSelectionIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let output = study.get("output").unwrap_or(&Value::Null);
    let csv_name = envelope
        .get("csvFilename")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            output
                .get("csvFilename")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("{study_id}.csv"));
    let debug_csv = format!("{}-debug.csv", &csv_name[..csv_name.len() - 4]);
    let field_files = output
        .get("fieldFiles")
        .and_then(Value::as_bool)
        .or_else(|| {
            capability_study
                .pointer("/outputDefaults/fieldFiles")
                .and_then(Value::as_bool)
        })
        .ok_or("Study output defaults must declare fieldFiles.")?;
    let metadata_id = model
        .pointer("/metadata/id")
        .and_then(Value::as_str)
        .unwrap_or("model");
    let request = json!({"schema":"fem-solver-request/1","solver":profile_id,"studyId":study_id,"meshFile":mesh_file,
        "elementOrder":order,"fieldFormulation":formulation,"constitutiveQuadrature":constitutive_quadrature,
        "solverBackend":backend,"solverOrdering":ordering,"solverThreads":threads,"frequencyCases":frequency_cases,
        "outputDirectory":"output","sweepCsv":csv_name,"debugCsv":debug_csv,"fieldFilePrefix":format!("{metadata_id}_{study_id}_field"),
        "modelConfiguration":{"schema":capability_study.get("requestConfigurationSchema"),"scenarioProfileId":profile_id,
            "fieldComponentCoordinateSystem":"model","physicsId":physics_id,"domainSelectionIds":domain_selection_ids,
            "periodicPairIds":periodic_request.iter().map(|entry|entry.get("id")).collect::<Vec<_>>(),"periodicPairs":periodic_request,
            "impedanceBoundaries":impedance_boundaries,"materialAssignments":assignments,"variables":variables,"pointwiseVariables":pointwise_variables,
            "integrationOperators":integration_operators,"fieldFiles":field_files}});
    // Configured requests need a materialized configuration for every sweep
    // case. Keep this in the shared preparer so the coordinator can prepare a
    // whole sweep in one dispatcher call. Only periodic quantities currently
    // depend on resolved sweep parameters; the rest of the configuration is
    // identical across cases and is cloned from the shared base request.
    let case_model_configurations = if preparation == "configuredFrequencyRequest" {
        let base_configuration = request
            .get("modelConfiguration")
            .cloned()
            .ok_or("Configured Study preparation omitted modelConfiguration.")?;
        let mut configurations = Vec::with_capacity(cases.len());
        for prepared_case in &cases {
            let periodic_pairs =
                periodic_pairs_for_symbols(component, physics, &prepared_case.symbols)?;
            let mut configuration = base_configuration.clone();
            configuration["periodicPairs"] = json!(periodic_pairs);
            configurations.push(configuration);
        }
        Some(configurations)
    } else {
        None
    };
    let mut prepared = json!({"request":request,"study":study,"component":component,"assignments":assignments,
        "frequenciesGHz":frequencies_hz.iter().map(|frequency|frequency/1e9).collect::<Vec<_>>()});
    if let Some(configurations) = case_model_configurations {
        prepared["caseModelConfigurations"] = json!(configurations);
    }
    Ok(prepared)
}
