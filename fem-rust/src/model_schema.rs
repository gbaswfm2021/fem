//! FEM caller adapter for application-selected schemas and FEM metadata.
//!
//! The model library receives an explicit schema context. Installation discovery,
//! scenario interpretation, and typed solver declarations remain with this caller.

use fullmeta_model::SchemaContext;
use serde_json::{Value, json};
use std::sync::OnceLock;
use std::{collections::BTreeSet, env, fs, path::PathBuf};

static SELECTED_MODEL_SCHEMA_PATH: OnceLock<PathBuf> = OnceLock::new();

/// The caller selects the installed root; the generic loader follows its refs.
pub(crate) fn context() -> Result<&'static SchemaContext, String> {
    static CONTEXT: OnceLock<Result<SchemaContext, String>> = OnceLock::new();
    match CONTEXT.get_or_init(|| {
        let mut documents =
            fullmeta_model::schema_documents::load_schema_documents(&runtime_model_schema_path()?)?;
        if documents.is_empty() {
            return Err("The model schema document is not loaded.".into());
        }
        let root = documents.remove(0);
        documents.extend(embedded_scenario_documents(&root)?);
        let mut entrypoints = vec!["/$defs/solverRequest".to_string()];
        for pointer in root
            .pointer("/x-fullmeta-rules/entrypoints")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            entrypoints.push(
                pointer
                    .as_str()
                    .ok_or("Validation entrypoints must be JSON Pointer strings.")?
                    .to_string(),
            );
        }
        let context = SchemaContext::new_with_entrypoints(root, documents, entrypoints)?;
        validate_metadata_context(&context)?;
        Ok(context)
    }) {
        Ok(context) => Ok(context),
        Err(error) => Err(error.clone()),
    }
}

fn embedded_scenario_documents(root: &Value) -> Result<Vec<Value>, String> {
    let Some(profiles) = root
        .pointer("/$defs/femProfiles")
        .and_then(Value::as_object)
    else {
        return Ok(Vec::new());
    };
    let root_id = root
        .get("$id")
        .and_then(Value::as_str)
        .ok_or("The FEM model schema must declare a $id before embedding profile schemas.")?;
    let base = root_id
        .rsplit_once('/')
        .map(|(base, _)| format!("{base}/"))
        .ok_or("The FEM model schema $id must include a document path.")?;
    profiles
        .iter()
        .map(|(key, profile_schema)| {
            let profile_id = profile_schema
                .pointer("/x-fem-scenario/profile/id")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("Embedded FEM profile {key:?} must declare x-fem-scenario.profile.id.")
                })?;
            if key != profile_id {
                return Err(format!(
                    "Embedded FEM profile key {key:?} must match its profile ID {profile_id:?}."
                ));
            }
            let mut document = profile_schema.clone();
            let object = document
                .as_object_mut()
                .ok_or_else(|| format!("Embedded FEM profile {key:?} must be a schema object."))?;
            object.insert(
                "$id".into(),
                Value::String(format!("{base}{profile_id}.schema.json")),
            );
            if let Some(dialect) = root.get("$schema") {
                object.insert("$schema".into(), dialect.clone());
            }
            Ok(document)
        })
        .collect()
}

pub(crate) fn root() -> Result<&'static Value, String> {
    Ok(context()?.root())
}

/// The model has one Physics object. Its condition entries are keyed by their
/// own IDs alongside the interface settings.
pub(crate) fn physics(component: &Value) -> Option<&Value> {
    component.get("physics").filter(|value| value.is_object())
}

pub(crate) fn physics_conditions(physics: &Value) -> impl Iterator<Item = &Value> {
    physics
        .as_object()
        .into_iter()
        .flat_map(|properties| properties.iter())
        .filter_map(|(key, value)| {
            (value.get("id").and_then(Value::as_str) == Some(key.as_str())
                && value.get("type").and_then(Value::as_str).is_some())
            .then_some(value)
        })
}

pub(crate) fn physics_conditions_mut(physics: &mut Value) -> impl Iterator<Item = &mut Value> {
    physics
        .as_object_mut()
        .into_iter()
        .flat_map(|properties| properties.iter_mut())
        .filter_map(|(key, value)| {
            (value.get("id").and_then(Value::as_str) == Some(key.as_str())
                && value.get("type").and_then(Value::as_str).is_some())
            .then_some(value)
        })
}

pub(crate) fn physics_boundary_conditions(physics: &Value) -> impl Iterator<Item = &Value> {
    physics_conditions(physics).filter(|value| {
        matches!(
            value.get("type").and_then(Value::as_str),
            Some("dirichlet" | "neumann" | "impedance")
        )
    })
}

pub(crate) fn physics_periodic_conditions(physics: &Value) -> impl Iterator<Item = &Value> {
    physics_conditions(physics)
        .filter(|value| value.get("type").and_then(Value::as_str) == Some("periodicCondition"))
}

/// Resolve a registered executable name from the selected model schema.
pub(crate) fn external_program_executable(id: &str) -> Result<&'static str, String> {
    let mut found = None;
    for document in context()?.documents() {
        if let Some(program) = document
            .pointer("/x-fullmeta-runtime/externalPrograms")
            .and_then(|programs| programs.get(id))
        {
            if found.is_some() {
                return Err(format!("Duplicate external program {id:?}."));
            }
            found = program
                .get("executable")
                .and_then(Value::as_str)
                .and_then(|path| path.rsplit('/').next());
        }
    }
    found.ok_or_else(|| format!("The model schema does not register external program {id:?}."))
}

pub(crate) fn external_program_path(id: &str) -> Result<PathBuf, String> {
    let origin = env::var("FULLMETA_SCHEMA_ORIGIN")
        .map(PathBuf::from)
        .unwrap_or(runtime_model_schema_path()?);
    fullmeta_model::runtime_paths::external_program_path(
        context()?.documents(),
        &origin,
        &fullmeta_model::runtime_paths::installation_root()?,
        id,
    )
}

pub(crate) fn select_schema_for_model_file(
    model: &Value,
    file: &std::path::Path,
) -> Result<(), String> {
    let reference = model
        .get("x-fullmeta-schema")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("The model must declare x-fullmeta-schema as a relative schema path.")?;
    if reference.contains(['\\', ':', '?', '#']) || std::path::Path::new(reference).is_absolute() {
        return Err("x-fullmeta-schema must be a relative file path.".into());
    }
    select_model_schema_path(
        file.parent()
            .ok_or("The model file has no directory.")?
            .join(reference),
    )
}

/// Command declarations are root-local data. Read them without loading runtime
/// registrations or validating scenario metadata so help remains available when
/// those independently configured resources are unavailable.
pub(crate) fn command_schema() -> Result<&'static Value, String> {
    static COMMAND_SCHEMA: OnceLock<Result<Value, String>> = OnceLock::new();
    match COMMAND_SCHEMA.get_or_init(|| {
        let path = runtime_model_schema_path()?;
        let bytes = std::fs::read(&path).map_err(|error| {
            format!("Could not read command schema {}: {error}", path.display())
        })?;
        serde_json::from_slice(&bytes).map_err(|error| {
            format!(
                "Could not decode command schema {}: {error}",
                path.display()
            )
        })
    }) {
        Ok(schema) => Ok(schema),
        Err(error) => Err(error.clone()),
    }
}

/// Execute an explicitly selected native-input contract from the supplied
/// application schema. The generic interpreter owns its checking algorithms.
pub(crate) fn validate_declaration(annotation: &str, input: &Value) -> Result<(), String> {
    validate_declaration_in_context(context()?, annotation, input)
}

fn validate_declaration_in_context(
    context: &SchemaContext,
    annotation: &str,
    input: &Value,
) -> Result<(), String> {
    let declaration = context
        .root()
        .get(annotation)
        .ok_or_else(|| format!("The model schema must declare {annotation}."))?;
    let errors = fullmeta_model::rules::validate_declaration(
        input,
        context.root(),
        context.documents(),
        &json!({}),
        declaration,
    )?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

fn validate_metadata_context(context: &SchemaContext) -> Result<(), String> {
    let mut capabilities = json!({"schema":"fem-engine-capabilities/2"});
    for declared in context.document_annotations("x-fem-capabilities") {
        merge_capabilities(&mut capabilities, declared.clone());
    }
    let parts = context
        .schema_annotations("x-fullmeta-role")?
        .into_iter()
        .filter(|declaration| has_role(declaration, "field.plot.parts"))
        .collect::<Vec<_>>();
    validate_declaration_in_context(
        context,
        "x-fullmeta-metadataValidation",
        &json!({
            "documents":context.documents(),
            "fieldPlotPartDeclarations":parts,
            "capabilities":capabilities,
        }),
    )
}

pub(crate) fn runtime_model_schema_path() -> Result<PathBuf, String> {
    if let Some(path) = SELECTED_MODEL_SCHEMA_PATH.get() {
        return Ok(path.clone());
    }
    if let Ok(configured) = env::var("FULLMETA_MODEL_SCHEMA_FILE") {
        let path = PathBuf::from(&configured);
        if !path.is_absolute() {
            return Err(
                "FULLMETA_MODEL_SCHEMA_FILE must be an absolute path to the model's schema.".into(),
            );
        }
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Could not read the selected model schema {}: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "The selected model schema {} must be a regular file.",
                path.display()
            ));
        }
        return Ok(path);
    }
    let executable = env::current_exe()
        .map_err(|error| format!("Could not locate the native executable: {error}"))?;
    if let Some(application_dir) = executable.ancestors().find(|ancestor| {
        ancestor
            .parent()
            .and_then(|parent| parent.file_name())
            .and_then(|name| name.to_str())
            == Some("applications")
    }) {
        if let Some(schema) = schema_in_application_directory(application_dir) {
            return Ok(schema);
        }
    }
    // Cargo may place development binaries outside the application directory.
    let directory = env::current_dir()
        .map_err(|error| format!("Could not locate the native working directory: {error}"))?;
    for root in directory.ancestors() {
        let applications = root.join("applications");
        let Ok(entries) = fs::read_dir(&applications) else {
            continue;
        };
        let mut schemas = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| schema_in_application_directory(&entry.path()))
            .collect::<Vec<_>>();
        schemas.sort();
        schemas.dedup();
        match schemas.as_slice() {
            [schema] => return Ok(schema.clone()),
            [] => continue,
            _ => {
                return Err(format!(
                    "Could not select an application schema near {}: found multiple application schemas. Set FULLMETA_MODEL_SCHEMA_FILE or select a project model first.",
                    applications.display()
                ));
            }
        }
    }
    Err("Could not locate an application schema above the native executable or working directory. Native schema operations require the model schema and its referenced documents.".into())
}

fn schema_in_application_directory(application_dir: &std::path::Path) -> Option<PathBuf> {
    let application_id = application_dir.file_name()?.to_str()?;
    let path = application_dir.join(format!("{application_id}.schema.json"));
    let schema: Value = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
    let declared_id = schema
        .pointer("/properties/application/const")
        .and_then(Value::as_str)?;
    (declared_id == application_id).then_some(path)
}

/// Select the schema declared by a saved model before any schema context is
/// initialized. The path is resolved by the caller relative to that model file;
/// solver binaries may live elsewhere in the runtime installation.
pub(crate) fn select_model_schema_path(path: PathBuf) -> Result<(), String> {
    if !path.is_absolute() {
        return Err(
            "The model's x-fullmeta-schema path must resolve to an absolute file path.".into(),
        );
    }
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        format!(
            "Could not read the model schema {}: {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "The model schema {} must be a regular file.",
            path.display()
        ));
    }
    if let Some(selected) = SELECTED_MODEL_SCHEMA_PATH.get() {
        if selected != &path {
            return Err(
                "A different model schema was already selected for this dispatcher process.".into(),
            );
        }
        return Ok(());
    }
    SELECTED_MODEL_SCHEMA_PATH
        .set(path.clone())
        .map_err(|_| "A different model schema was selected concurrently.".to_string())
}

fn has_role(schema: &Value, role: &str) -> bool {
    match schema.get("x-fullmeta-role") {
        Some(Value::String(value)) => value == role,
        Some(Value::Array(values)) => values.iter().any(|value| value.as_str() == Some(role)),
        _ => false,
    }
}

fn merge_capabilities(target: &mut Value, source: Value) {
    if let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) {
        for (key, value) in source {
            if let Some(existing) = target.get_mut(key) {
                merge_capabilities(existing, value.clone());
            } else {
                target.insert(key.clone(), value.clone());
            }
        }
    } else {
        *target = source;
    }
}

fn metadata_string<'a>(value: &'a Value, key: &str, context: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Could not read {context}.{key} as a string."))
}

fn metadata_strings<'a>(value: Option<&'a Value>, context: &str) -> Result<Vec<&'a str>, String> {
    value
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Could not read {context} as an array."))?
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value
                .as_str()
                .ok_or_else(|| format!("Could not read {context}[{index}] as a string."))
        })
        .collect()
}

/// Read scenario annotations from the registered root-first schema documents.
/// Embedded profile definitions are exposed as in-memory documents by the FEM caller.
pub fn scenario_metadata_for_profile(profile_id: &str) -> Result<&'static Value, String> {
    context()?
        .document_annotations("x-fem-scenario")
        .into_iter()
        .find(|metadata| {
            metadata.pointer("/profile/id").and_then(Value::as_str) == Some(profile_id)
        })
        .ok_or_else(|| format!("Could not read schema metadata for profile {profile_id:?}."))
}

/// Material declarations are application data, read from the selected schema.
pub fn material_properties_for_profile(profile_id: &str) -> Result<&'static [Value], String> {
    let metadata = scenario_metadata_for_profile(profile_id)?;
    let properties = metadata
        .pointer("/materials/properties")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Could not read material properties for profile {profile_id:?}."))?;
    Ok(properties.as_slice())
}

fn model_profile_id(model: &Value) -> Result<String, String> {
    let values = context()?.model_role_values(model, "profile.id")?;
    values
        .into_iter()
        .find_map(|value| value.as_str().map(str::to_string))
        .ok_or_else(|| "Could not read the model profile.id value declared by its schema.".into())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldVariable {
    pub symbol: String,
    pub unit: String,
    pub role: String,
    pub identically_zero: bool,
    pub solver_family: Option<String>,
    pub axis: Option<String>,
}

pub fn field_symbol_for_solver_component<'a>(
    variables: &'a [FieldVariable],
    family: &str,
    axis: Option<&str>,
) -> Option<&'a str> {
    variables
        .iter()
        .find(|variable| {
            variable.solver_family.as_deref() == Some(family) && variable.axis.as_deref() == axis
        })
        .map(|variable| variable.symbol.as_str())
}

/// A model may reserve all formulations, while outputs use its selected one.
/// A single supported formulation is unambiguous even without a selection.
pub fn field_variables_for_profile(
    profile_id: &str,
    formulation: Option<&str>,
    all_formulations: bool,
) -> Result<Vec<FieldVariable>, String> {
    let metadata = scenario_metadata_for_profile(profile_id)?;
    let allowed = metadata_strings(
        metadata.pointer("/profile/formulations"),
        &format!("Scenario {profile_id:?} metadata.profile.formulations"),
    )?;
    let fields = metadata
        .pointer("/fields/formulations")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!("Scenario {profile_id:?} metadata.fields.formulations must be an object.")
        })?;
    let dimension = metadata
        .pointer("/profile/dimension")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            format!("Could not read the schema dimension for profile {profile_id:?}.")
        })?;
    let selected = formulation
        .filter(|selected| allowed.contains(selected))
        .or_else(|| {
            if allowed.len() == 1 {
                Some(allowed[0])
            } else {
                None
            }
        });
    let mut output = Vec::new();
    for formulation in allowed {
        let definition = fields.get(formulation).ok_or_else(|| {
            format!("Could not read the field catalog for {profile_id:?} {formulation:?}.")
        })?;
        for (key, role) in [
            ("fieldVariables", "primary"),
            ("derivedFieldVariables", "derived"),
            ("zeroFieldVariables", "zero"),
        ] {
            let context = format!("Scenario {profile_id:?} formulation {formulation:?} {key}");
            let variables = definition
                .get(key)
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{context} must be an array."))?;
            for (index, variable) in variables.iter().enumerate() {
                let context = format!("{context}[{index}]");
                let symbol = metadata_string(variable, "symbol", &context)?;
                let unit = metadata_string(variable, "unit", &context)?;
                let mut zero_in_dimension = false;
                if let Some(dimensions) = variable.get("zeroInDimensions") {
                    let dimensions = dimensions.as_array().ok_or_else(|| {
                        format!("{context}.zeroInDimensions must be an integer array.")
                    })?;
                    for value in dimensions {
                        let value = value.as_u64().ok_or_else(|| {
                            format!(
                                "Could not read {context}.zeroInDimensions as integer dimensions."
                            )
                        })?;
                        zero_in_dimension |= value == dimension;
                    }
                }
                let mut solver_family = None;
                let mut axis = None;
                if let Some(component) = variable.get("solverComponent") {
                    if component.get("family").is_some() {
                        solver_family = Some(
                            metadata_string(
                                component,
                                "family",
                                &format!("{context}.solverComponent"),
                            )?
                            .to_string(),
                        );
                    }
                    if component.get("axis").is_some() {
                        axis = Some(
                            metadata_string(
                                component,
                                "axis",
                                &format!("{context}.solverComponent"),
                            )?
                            .to_string(),
                        );
                    }
                }
                if all_formulations || selected == Some(formulation) {
                    output.push(FieldVariable {
                        symbol: symbol.to_string(),
                        unit: unit.to_string(),
                        role: if zero_in_dimension {
                            "zero".into()
                        } else {
                            role.into()
                        },
                        identically_zero: zero_in_dimension || role == "zero",
                        solver_family,
                        axis,
                    });
                }
            }
        }
    }
    Ok(output)
}

fn selected_formulation(model: &Value) -> Result<Option<String>, String> {
    for role in [
        "profile.formulation.primary",
        "profile.formulation.fallback",
    ] {
        let values = context()?.model_role_values(model, role)?;
        if let Some(value) = values
            .into_iter()
            .find_map(|value| value.as_str().map(str::to_string))
        {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

pub fn field_variables_for_model(
    model: &Value,
    all_formulations: bool,
) -> Result<Vec<FieldVariable>, String> {
    field_variables_for_profile(
        &model_profile_id(model)?,
        selected_formulation(model)?.as_deref(),
        all_formulations,
    )
}

pub fn field_plot_symbols_for_model(model: &Value) -> Result<Vec<String>, String> {
    Ok(field_variables_for_model(model, false)?
        .into_iter()
        .filter(|field| {
            !field.identically_zero && matches!(field.role.as_str(), "primary" | "derived")
        })
        .map(|field| field.symbol)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

pub fn field_plot_parts() -> Result<Vec<String>, String> {
    let declarations = context()?.schema_annotations("x-fullmeta-role")?;
    let declaration = declarations
        .iter()
        .find(|node| has_role(node, "field.plot.parts"))
        .ok_or("Could not read the field.plot.parts schema declaration.")?;
    Ok(
        metadata_strings(declaration.get("enum"), "Shared field plot parts")?
            .into_iter()
            .map(str::to_string)
            .collect(),
    )
}

pub fn material_symbols_for_model(model: &Value) -> Result<Vec<String>, String> {
    material_properties_for_profile(&model_profile_id(model)?)?
        .iter()
        .map(|property| {
            metadata_string(property, "symbol", "Material schema declaration").map(str::to_string)
        })
        .collect()
}

/// Reserve every declared field formulation so parameter names remain valid
/// when a model changes formulation. Expression built-ins are owned by callers.
pub fn reserved_symbols_for_model(model: &Value) -> Result<Vec<String>, String> {
    let mut symbols = field_variables_for_model(model, true)?
        .into_iter()
        .map(|field| field.symbol)
        .collect::<BTreeSet<_>>();
    symbols.extend(material_symbols_for_model(model)?);
    Ok(symbols.into_iter().collect())
}

pub fn scenario_profile_ids() -> Result<Vec<String>, String> {
    let mut profiles = BTreeSet::new();
    for metadata in context()?.document_annotations("x-fem-scenario") {
        let profile = metadata
            .get("profile")
            .ok_or("Scenario schema x-fem-scenario metadata requires a profile.")?;
        let id = metadata_string(profile, "id", "Schema x-fem-scenario.profile")?;
        profiles.insert(id.to_string());
    }
    Ok(profiles.into_iter().collect())
}

pub fn sweep_result_quantities_for_profile(profile_id: &str) -> Result<Vec<String>, String> {
    let metadata = scenario_metadata_for_profile(profile_id)?;
    let context = format!("Scenario {profile_id:?} metadata.sweepResultQuantities");
    Ok(
        metadata_strings(metadata.get("sweepResultQuantities"), &context)?
            .into_iter()
            .map(str::to_string)
            .collect(),
    )
}

/// Validate installed declarations once at native command boundaries.
pub fn ensure_scenario_metadata() -> Result<(), String> {
    context().map(|_| ())
}

/// Compose the application capability view from the metadata declared by its schemas.
/// The schemas remain the source of truth; this assembled value is only a runtime adapter.
pub fn engine_capabilities() -> Result<Value, String> {
    let mut capabilities = serde_json::json!({ "schema": "fem-engine-capabilities/2" });
    for declared in context()?.document_annotations("x-fem-capabilities") {
        merge_capabilities(&mut capabilities, declared.clone());
    }
    Ok(capabilities)
}

pub fn geometry_feature_dimensions(feature_type: &str) -> Option<Vec<usize>> {
    let context = context().ok()?;
    for key in [
        "x-fullmeta-geometryDimensions",
        "x-fullmeta-geometryDimension",
    ] {
        for declaration in context.schema_annotations(key).ok()? {
            if declaration
                .pointer("/properties/type/const")
                .and_then(Value::as_str)
                != Some(feature_type)
            {
                continue;
            }
            if let Some(dimensions) = declaration
                .get("x-fullmeta-geometryDimensions")
                .and_then(Value::as_array)
            {
                return Some(
                    dimensions
                        .iter()
                        .filter_map(Value::as_u64)
                        .map(|value| value as usize)
                        .collect(),
                );
            }
            if let Some(dimension) = declaration
                .get("x-fullmeta-geometryDimension")
                .and_then(Value::as_u64)
            {
                return Some(vec![dimension as usize]);
            }
        }
    }
    None
}

pub fn integration_kind_for_selection(selection: &Value) -> Option<String> {
    integration_kind_for_selection_in_context(selection, context().ok()?)
}

pub fn integration_kind_for_selection_with_schema(
    selection: &Value,
    schema: &Value,
) -> Option<String> {
    let context = SchemaContext::new(schema.clone(), Vec::new()).ok()?;
    integration_kind_for_selection_in_context(selection, &context)
}

fn integration_kind_for_selection_in_context(
    selection: &Value,
    context: &SchemaContext,
) -> Option<String> {
    context
        .schema_annotations("x-fullmeta-role")
        .ok()?
        .iter()
        .filter(|schema| has_role(schema, "integration.selection"))
        .find_map(|schema| {
            let contracts = schema
                .pointer("/x-fullmeta-modelReference/targetSchema/oneOf")?
                .as_array()?;
            contracts.iter().find_map(|contract| {
                let kind = contract.get("x-fullmeta-role")?.as_str()?;
                fullmeta_model::schema::validate_with_root(selection, contract, context.root())
                    .is_empty()
                    .then(|| kind.to_string())
            })
        })
}

pub(crate) fn validate(value: &Value, schema: &Value) -> Vec<String> {
    fullmeta_model::schema::validate(value, schema)
}

pub(crate) fn validate_with_root(value: &Value, schema: &Value, root: &Value) -> Vec<String> {
    fullmeta_model::schema::validate_with_root(value, schema, root)
}

/// Every child receives the same selected schema and installation context.
pub(crate) fn command(
    executable: impl AsRef<std::ffi::OsStr>,
) -> Result<std::process::Command, String> {
    let mut command = std::process::Command::new(executable);
    command.env("FULLMETA_MODEL_SCHEMA_FILE", runtime_model_schema_path()?);
    command.env(
        "FULLMETA_ROOT",
        fullmeta_model::runtime_paths::installation_root()?,
    );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command)
}
