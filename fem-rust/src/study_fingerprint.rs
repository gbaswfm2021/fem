//! Versioned projection of the model data consumed by a Study.

use crate::mesh::sha256;
#[path = "solver_options.rs"]
mod solver_options;

use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};

pub(super) fn snapshot_digest(snapshot: &Value) -> Option<String> {
    serde_json::to_vec(snapshot)
        .ok()
        .map(|bytes| sha256::hex_digest(&bytes))
}

pub(super) fn fingerprint(model: &Value, study_id: &str) -> Option<Value> {
    let capabilities = crate::model_schema::engine_capabilities().ok()?;
    let profile = model.pointer("/scenario/profileId")?.as_str()?;
    let materials = crate::model_schema::material_properties_for_profile(profile).ok()?;
    let projection = inputs(model, study_id, capabilities.get("study")?, materials)?;
    Some(json!({"version":1,"sha256":snapshot_digest(&projection)?}))
}

fn inputs(
    model: &Value,
    study_id: &str,
    capabilities: &Value,
    material_properties: &[Value],
) -> Option<Value> {
    let component = model.get("model")?.as_object()?;
    let scenario = model.get("scenario")?;
    let profile = scenario.get("profileId")?.as_str()?;
    let study = component
        .get("study")?
        .as_array()?
        .iter()
        .find(|entry| entry["id"] == study_id)?;
    let physics = physics_inputs(component.get("physics")?, study.get("physicsId")?.as_str()?)?;
    let study = study_inputs(study, &physics, profile, capabilities)?;
    let geometry = component.get("geometry")?;
    let features = geometry
        .get("features")?
        .as_array()?
        .iter()
        .map(feature_inputs)
        .collect::<Option<Vec<_>>>()?;
    let selections = component
        .get("selections")?
        .as_array()?
        .iter()
        .map(selection_inputs)
        .collect::<Option<Vec<_>>>()?;
    let mesh = mesh_inputs(component.get("mesh")?)?;
    let mut materials = component.get("materials")?.as_array()?.clone();
    let mut expressions = BTreeSet::from(["expression", "coordinateExpression"]);
    for property in material_properties {
        if property.get("representation").and_then(Value::as_str) != Some("expression") {
            continue;
        }
        let key = property.get("key")?.as_str()?;
        expressions.insert(key);
        if let Some(default) = property.get("default") {
            for material in &mut materials {
                material
                    .as_object_mut()?
                    .entry(key)
                    .or_insert_with(|| default.clone());
            }
        }
    }
    let mut projection = json!({
        "scenario": fields(scenario, &["dimension", "physics", "profileId"])?,
        "units": component.get("units")?,
        "geometry": {"tolerance": geometry.get("tolerance")?, "features": features},
        "selections": selections,
        "mesh": mesh,
        "materials": materials.iter().map(without_labels).collect::<Option<Vec<_>>>()?,
        "physics": physics,
        "study": study,
        "variables": definitions(component.get("variables")?)?,
        "pointwiseVariables": definitions(component.get("pointwiseVariables")?)?,
    });
    let mut symbols = BTreeSet::new();
    let mut calls = BTreeSet::new();
    collect_references(&projection, &expressions, &mut symbols, &mut calls)?;
    let mut operators = Vec::new();
    for operator in component
        .get("integrationOperators")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let mut projected = without_labels(operator)?;
        if let Some(name) = operator
            .get("name")
            .and_then(Value::as_str)
            .filter(|name| calls.contains(*name))
        {
            projected["callableName"] = json!(name);
        }
        operators.push(projected);
    }
    projection["integrationOperators"] = json!(operators);
    for axis in projection
        .pointer("/study/parameterSweep/parameters")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        symbols.insert(axis.get("parameter")?.as_str()?.to_string());
    }
    let parameters = component.get("parameters")?.as_array()?;
    projection["parameters"] = json!(relevant_parameters(parameters, symbols)?);
    Some(projection)
}

fn fields(value: &Value, keys: &[&str]) -> Option<Value> {
    let object = value.as_object()?;
    Some(Value::Object(
        keys.iter()
            .filter_map(|key| Some(((*key).to_string(), object.get(*key)?.clone())))
            .collect(),
    ))
}

fn without_labels(value: &Value) -> Option<Value> {
    Some(Value::Object(
        value
            .as_object()?
            .iter()
            .filter(|(key, _)| !matches!(key.as_str(), "name" | "description"))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    ))
}

fn definitions(value: &Value) -> Option<Vec<Value>> {
    value.as_array()?.iter().map(without_labels).collect()
}

fn feature_inputs(feature: &Value) -> Option<Value> {
    let geometry = match feature.get("type")?.as_str()? {
        "rectangle" => &["position", "size"][..],
        "box" => &["position", "size", "positionMode"],
        "circle" => &["center", "radius"],
        "cylinder" => &["base", "axis", "radius", "height"],
        "union" | "intersection" => &[],
        "lineSegment" => &["start", "end"],
        "circleCurve" => &["center", "radius", "normal"],
        "diskSurface" => &["center", "normal", "radius"],
        "planeRectangle" => &["origin", "uVector", "vVector"],
        _ => return None,
    };
    let keys = ["id", "type"]
        .into_iter()
        .chain(geometry.iter().copied())
        .collect::<Vec<_>>();
    let mut result = fields(feature, &keys)?;
    match feature["type"].as_str()? {
        "box" if result.get("positionMode").is_none() => result["positionMode"] = json!("corner"),
        "circleCurve" if result.get("normal").is_none() => result["normal"] = json!([0, 0, 1]),
        "union" | "intersection" => {
            let children = feature
                .get("features")?
                .as_array()?
                .iter()
                .map(feature_inputs)
                .collect::<Option<Vec<_>>>()?;
            result["features"] = json!(children);
        }
        _ => {}
    }
    Some(result)
}

fn selection_inputs(selection: &Value) -> Option<Value> {
    let mut result = fields(selection, &["id", "entity", "coordinateExpression"])?;
    let addresses = selection
        .get("addresses")?
        .as_array()?
        .iter()
        .map(|address| {
            let membership = address
                .get("membership")?
                .as_object()?
                .iter()
                .filter(|(_, membership)| *membership != "outside")
                .map(|(id, membership)| (id.clone(), membership.clone()))
                .collect::<Map<_, _>>();
            Some(json!({"membership":membership}))
        })
        .collect::<Option<Vec<_>>>()?;
    result["addresses"] = json!(addresses);
    if result["entity"] != "domain" && result.get("coordinateExpression").is_none() {
        result["coordinateExpression"] = json!("0");
    }
    Some(result)
}

fn mesh_inputs(mesh: &Value) -> Option<Value> {
    let mut result = fields(mesh, &["elementFamily", "minSize", "maxSize", "gmsh"])?;
    let enabled = mesh
        .get("periodicPairIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<HashSet<_>>();
    let pairs = mesh
        .get("periodicPairs")?
        .as_array()?
        .iter()
        .filter(|pair| {
            pair.get("id")
                .and_then(Value::as_str)
                .is_some_and(|id| enabled.contains(id))
        })
        .map(without_labels)
        .collect::<Option<Vec<_>>>()?;
    let controls = mesh
        .get("meshSizeControls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| fields(value, &["id", "selectionId", "size"]))
        .collect::<Option<Vec<_>>>()?;
    result["periodicPairs"] = json!(pairs);
    result["meshSizeControls"] = json!(controls);
    Some(result)
}

fn physics_inputs(physics: &Value, physics_id: &str) -> Option<Value> {
    let interface = match physics {
        Value::Object(_) => physics,
        Value::Array(entries) => entries.iter().find(|entry| entry["id"] == physics_id)?,
        _ => return None,
    };
    if interface["id"] != physics_id {
        return None;
    }
    let mut result = fields(
        interface,
        &[
            "id",
            "type",
            "formulation",
            "domainSelectionIds",
            "frequency",
            "phasorConvention",
        ],
    )?;
    let mut add_condition = |condition: &Value| -> Option<()> {
        if condition.get("enabled").and_then(Value::as_bool) == Some(false) {
            return Some(());
        }
        let id = condition.get("id")?.as_str()?;
        let mut projected = without_labels(condition)?;
        projected.as_object_mut()?.remove("enabled");
        if projected["type"] == "periodicCondition" && projected.get("phaseVector").is_none() {
            projected["phaseVector"] = json!(vec![json!({"expression":"0","unit":"rad/m"}); 3]);
        }
        if result
            .as_object_mut()?
            .insert(id.to_string(), projected)
            .is_some()
        {
            return None;
        }
        Some(())
    };
    if physics.is_array() {
        for key in ["boundaries", "features"] {
            for condition in interface
                .get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                add_condition(condition)?;
            }
        }
    } else {
        for condition in crate::model_schema::physics_conditions(interface) {
            add_condition(condition)?;
        }
    }
    Some(result)
}

fn study_inputs(
    study: &Value,
    physics: &Value,
    profile: &str,
    capabilities: &Value,
) -> Option<Value> {
    let settings = capabilities.get("solverSettingsByProfile")?.get(profile)?;
    let defaults = settings.get("defaults")?;
    let order = study
        .pointer("/discretization/elementOrder")
        .or_else(|| defaults.get("elementOrder"))?
        .as_u64()?;
    let formulation = study
        .pointer("/discretization/fieldFormulation")
        .or_else(|| physics.get("formulation"))
        .or_else(|| defaults.get("fieldFormulation"))?
        .as_str()?;
    let solver = solver_options::resolve(
        capabilities,
        settings,
        study.get("solver").unwrap_or(&Value::Null),
        formulation,
        order,
    )
    .ok()?;
    let id = study.get("id")?.as_str()?;
    let scenario = crate::model_schema::scenario_metadata_for_profile(profile).ok()?;
    let fingerprint_declaration = scenario.get("studyFingerprint");
    let csv_default = fingerprint_declaration
        .and_then(|declaration| declaration.get("defaultCsvFilename"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("{id}.csv"));
    let csv = study
        .pointer("/output/csvFilename")
        .and_then(Value::as_str)
        .unwrap_or(&csv_default);
    let field_files = study
        .pointer("/output/fieldFiles")
        .and_then(Value::as_bool)
        .or_else(|| {
            capabilities
                .pointer("/outputDefaults/fieldFiles")
                .and_then(Value::as_bool)
        })?;
    let mut result = fields(study, &["id", "type", "physicsId"])?;
    result["discretization"] = json!({"elementOrder":order,"fieldFormulation":formulation});
    result["solver"] = Value::Object(solver);
    result["output"] = json!({"csvFilename":csv,"fieldFiles":field_files});
    if let Some(properties) = fingerprint_declaration
        .and_then(|declaration| declaration.get("studyProperties"))
        .and_then(Value::as_object)
    {
        for (key, declaration) in properties {
            if let Some(value) = study.get(key).or_else(|| declaration.get("default")) {
                result[key] = value.clone();
            }
        }
    }
    if let Some(sweep) = study.get("parameterSweep").or_else(|| {
        study
            .get("steps")?
            .as_array()?
            .iter()
            .find(|step| step["type"] == "parameterSweep")
    }) {
        let mut result_sweep = fields(sweep, &["id", "type"])?;
        let legacy;
        let parameters = if let Some(parameters) =
            sweep.get("parameters").or_else(|| sweep.get("loops"))
        {
            parameters.as_array()?
        } else {
            legacy =
                vec![json!({"parameter":sweep.get("parameter")?,"values":sweep.get("values")?})];
            &legacy
        };
        let axes = parameters
            .iter()
            .map(|axis| {
                let values = fields(
                    axis.get("values")?,
                    &["type", "start", "end", "count", "values"],
                )?;
                Some(json!({"parameter":axis.get("parameter")?, "values":values}))
            })
            .collect::<Option<Vec<_>>>()?;
        result_sweep["parameters"] = json!(axes);
        result["parameterSweep"] = result_sweep;
    }
    Some(result)
}

fn collect_references(
    value: &Value,
    expressions: &BTreeSet<&str>,
    symbols: &mut BTreeSet<String>,
    calls: &mut BTreeSet<String>,
) -> Option<()> {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_references(value, expressions, symbols, calls)?;
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                // A keyed membership map contains IDs, which can look like expression field names.
                if key == "membership" {
                    continue;
                }
                if let Some(expression) = value
                    .as_str()
                    .filter(|_| expressions.contains(key.as_str()))
                {
                    let (_, references, functions) =
                        crate::model_expression::inspect(expression).ok()?;
                    symbols.extend(references);
                    calls.extend(functions);
                } else {
                    collect_references(value, expressions, symbols, calls)?;
                }
            }
        }
        _ => {}
    }
    Some(())
}

fn relevant_parameters(parameters: &[Value], symbols: BTreeSet<String>) -> Option<Vec<Value>> {
    let definitions = parameters
        .iter()
        .map(|row| Some((row.get("symbol")?.as_str()?, row)))
        .collect::<Option<HashMap<_, _>>>()?;
    let mut pending = symbols.into_iter().collect::<Vec<_>>();
    let mut referenced = BTreeSet::new();
    while let Some(symbol) = pending.pop() {
        let Some(parameter) = definitions.get(symbol.as_str()) else {
            continue;
        };
        if !referenced.insert(symbol) {
            continue;
        }
        let (_, dependencies, _) =
            crate::model_expression::inspect(parameter.get("expression")?.as_str()?).ok()?;
        pending.extend(dependencies);
    }
    referenced
        .iter()
        .map(|symbol| without_labels(definitions[symbol.as_str()]))
        .collect()
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    pub(crate) fn specimen() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/study-model.json")).unwrap()
    }

    #[test]
    fn study_fingerprint_ignores_presentation_unreferenced_parameters_and_unrelated_studies() {
        let original = specimen();
        let expected =
            fingerprint(&original, "study").expect("valid spec-derived input projection");
        let mut edited = original;
        edited["metadata"]["name"] = json!("Renamed model");
        edited["model"]["geometry"]["features"][0]["name"] = json!("Renamed box");
        edited["model"]["physics"]["boundary"]["name"] = json!("Renamed condition");
        edited["model"]["parameters"][5]["expression"] = json!("999");
        edited["model"]["study"][1]["output"] = json!({"fieldFiles":false});
        edited["model"]["results"][0]["name"] = json!("Renamed result");
        assert_eq!(fingerprint(&edited, "study"), Some(expected));
    }

    #[test]
    fn study_fingerprint_resolves_transitive_plain_string_dependencies_and_callable_names() {
        let original = specimen();
        let expected =
            fingerprint(&original, "study").expect("valid spec-derived input projection");
        for index in [1, 3] {
            let mut edited = original.clone();
            edited["model"]["parameters"][index]["expression"] = json!("5");
            assert_ne!(fingerprint(&edited, "study"), Some(expected.clone()));
        }
        let mut edited = original;
        edited["model"]["integrationOperators"][0]["name"] = json!("changedOperator");
        edited["model"]["variables"][0]["expression"] = json!("changedOperator(x)");
        assert_ne!(fingerprint(&edited, "study"), Some(expected));
    }

    #[test]
    fn study_fingerprint_uses_effective_defaults_and_preserves_solver_inputs() {
        let original = specimen();
        let expected =
            fingerprint(&original, "study").expect("valid spec-derived input projection");
        let mut edited = original;
        edited["model"]["geometry"]["features"][0]["positionMode"] = json!("corner");
        edited["model"]["selections"][2]["coordinateExpression"] = json!("0");
        edited["model"]["physics"]["boundary"]["enabled"] = json!(true);
        edited["model"]["mesh"]["meshSizeControls"] = json!([]);
        let formulation = edited["model"]["physics"]["formulation"].clone();
        edited["model"]["study"][0]["discretization"] =
            json!({"fieldFormulation":formulation,"elementOrder":2});
        edited["model"]["study"][0]["solver"] = json!({"backend":"rivrs","ordering":"metis","threads":3,"constitutiveQuadrature":"standard"});
        edited["model"]["study"][0]["output"] =
            json!({"fieldFiles":true,"csvFilename":"study.csv"});
        assert_eq!(fingerprint(&edited, "study"), Some(expected.clone()));
        edited["model"]["study"][0]["output"]["fieldFiles"] = json!(false);
        assert_ne!(fingerprint(&edited, "study"), Some(expected));
    }
}
