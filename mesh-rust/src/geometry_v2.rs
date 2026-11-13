use crate::expression::{Result, Scalar, evaluate_quantity, resolve_parameters};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[path = "geometry_v2_3d.rs"]
mod three_d;

type V2 = [f64; 2];

pub struct CompiledGeometry {
    pub component_id: String,
    pub geo: String,
    pub map_data: Value,
}

struct Feature {
    id: String,
    name: String,
    bounds: [f64; 4],
    geo_id: String,
    primitive: Value,
}

fn text<'a>(value: &'a Value, path: &str) -> Result<&'a str> {
    value
        .as_str()
        .ok_or_else(|| format!("{path} must be a string."))
}

fn quote(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\r', " ")
        .replace('\n', " ")
}
fn comment(value: &str) -> String {
    value
        .replace('\r', " ")
        .replace('\n', " ")
        .replace("*/", "*")
}
fn fmt(value: f64) -> String {
    format!("{value:.15e}")
}

fn quantity(value: &Value, symbols: &HashMap<String, Scalar>, path: &str) -> Result<f64> {
    evaluate_quantity(value, symbols, path)
}

fn vector2(value: &Value, symbols: &HashMap<String, Scalar>, path: &str) -> Result<V2> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("{path} must be a two-item array."))?;
    if items.len() != 2 {
        return Err(format!("{path} must be a two-item array."));
    }
    Ok([
        quantity(&items[0], symbols, &format!("{path}[0]"))?,
        quantity(&items[1], symbols, &format!("{path}[1]"))?,
    ])
}

fn compile_lower_feature(
    feature: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
    feature_index: usize,
    geo: &mut Vec<String>,
) -> Result<Value> {
    let name = text(
        feature
            .get("name")
            .ok_or_else(|| format!("{path}.name is required."))?,
        &format!("{path}.name"),
    )?;
    let prefix = format!("fullmetaLower{feature_index}");
    let id = text(
        feature
            .get("id")
            .ok_or_else(|| format!("{path}.id is required."))?,
        &format!("{path}.id"),
    )?;
    let kind = feature.get("type").and_then(Value::as_str).unwrap_or("");
    let gmsh_curves = match kind {
        "lineSegment" => {
            let start = vector2(
                feature
                    .get("start")
                    .ok_or_else(|| format!("{path}.start is required."))?,
                symbols,
                &format!("{path}.start"),
            )?;
            let end = vector2(
                feature
                    .get("end")
                    .ok_or_else(|| format!("{path}.end is required."))?,
                symbols,
                &format!("{path}.end"),
            )?;
            if start.iter().zip(end).all(|(a, b)| (a - b).abs() <= 1e-14) {
                return Err(format!("{path}.start and .end must differ."));
            }
            geo.push(format!(
                "{prefix}Start=newp; Point({prefix}Start)={{{},{},0,1.0}};",
                fmt(start[0]),
                fmt(start[1])
            ));
            geo.push(format!(
                "{prefix}End=newp; Point({prefix}End)={{{},{},0,1.0}};",
                fmt(end[0]),
                fmt(end[1])
            ));
            geo.push(format!(
                "{prefix}Curve=newl; Line({prefix}Curve)={{{prefix}Start,{prefix}End}}; // {}",
                comment(name)
            ));
            vec![format!("{prefix}Curve")]
        }
        "circleCurve" => {
            let center = vector2(
                feature
                    .get("center")
                    .ok_or_else(|| format!("{path}.center is required."))?,
                symbols,
                &format!("{path}.center"),
            )?;
            let radius = quantity(
                feature
                    .get("radius")
                    .ok_or_else(|| format!("{path}.radius is required."))?,
                symbols,
                &format!("{path}.radius"),
            )?;
            if !radius.is_finite() || radius <= 0.0 {
                return Err(format!("{path}.radius must be positive and finite."));
            }
            geo.push(format!(
                "{prefix}Center=newp; Point({prefix}Center)={{{},{},0,1.0}};",
                fmt(center[0]),
                fmt(center[1])
            ));
            for quarter in 0..4 {
                let angle = quarter as f64 * std::f64::consts::FRAC_PI_2;
                let point = [
                    center[0] + radius * angle.cos(),
                    center[1] + radius * angle.sin(),
                ];
                geo.push(format!(
                    "{prefix}Point{quarter}=newp; Point({prefix}Point{quarter})={{{},{},0,1.0}};",
                    fmt(point[0]),
                    fmt(point[1])
                ));
            }
            for quarter in 0..4 {
                let next = (quarter + 1) % 4;
                geo.push(format!("{prefix}Curve{quarter}=newl; Circle({prefix}Curve{quarter})={{{prefix}Point{quarter},{prefix}Center,{prefix}Point{next}}}; // {}", comment(name)));
            }
            (0..4)
                .map(|quarter| format!("{prefix}Curve{quarter}"))
                .collect()
        }
        kind => {
            return Err(format!(
                "{path}.type {kind:?} is not a supported lower-dimensional 2D primitive."
            ));
        }
    };
    Ok(json!({
        "id":id,
        "name":name,
        "type":kind,
        "gmshCurves":gmsh_curves
    }))
}

fn feature_bounds(
    feature: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
) -> Result<([f64; 4], String, Value)> {
    match feature.get("type").and_then(Value::as_str).unwrap_or("") {
        "rectangle" => {
            let position = vector2(
                feature
                    .get("position")
                    .ok_or_else(|| format!("{path}.position is required."))?,
                symbols,
                &format!("{path}.position"),
            )?;
            let size = vector2(
                feature
                    .get("size")
                    .ok_or_else(|| format!("{path}.size is required."))?,
                symbols,
                &format!("{path}.size"),
            )?;
            if size.iter().any(|value| *value <= 0.0) {
                return Err(format!("{path}.size must be positive."));
            }
            let bounds = [
                position[0],
                position[1],
                position[0] + size[0],
                position[1] + size[1],
            ];
            let geo = format!(
                "Rectangle(0) = {{{},{},0,{},{}}};",
                fmt(position[0]),
                fmt(position[1]),
                fmt(size[0]),
                fmt(size[1])
            );
            Ok((
                bounds,
                geo,
                json!({"type":"rectangle","min":position,"size":size}),
            ))
        }
        "circle" => {
            let center = vector2(
                feature
                    .get("center")
                    .ok_or_else(|| format!("{path}.center is required."))?,
                symbols,
                &format!("{path}.center"),
            )?;
            let radius = quantity(
                feature
                    .get("radius")
                    .ok_or_else(|| format!("{path}.radius is required."))?,
                symbols,
                &format!("{path}.radius"),
            )?;
            if radius <= 0.0 {
                return Err(format!("{path}.radius must be positive."));
            }
            let bounds = [
                center[0] - radius,
                center[1] - radius,
                center[0] + radius,
                center[1] + radius,
            ];
            let geo = format!(
                "Disk(0) = {{{},{},0,{},{}}};",
                fmt(center[0]),
                fmt(center[1]),
                fmt(radius),
                fmt(radius)
            );
            Ok((
                bounds,
                geo,
                json!({"type":"circle","center":center,"radius":radius}),
            ))
        }
        kind => Err(format!(
            "{path}.type {kind:?} is not supported by the 2D geometry compiler."
        )),
    }
}

fn compile_domain_feature(
    feature: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
    next_geo_id: &mut usize,
    next_boolean_id: &mut usize,
    geo: &mut Vec<String>,
) -> Result<Feature> {
    let id = text(
        feature
            .get("id")
            .ok_or_else(|| format!("{path}.id is required."))?,
        &format!("{path}.id"),
    )?
    .to_string();
    let name = text(
        feature
            .get("name")
            .ok_or_else(|| format!("{path}.name is required."))?,
        &format!("{path}.name"),
    )?
    .to_string();
    let kind = feature.get("type").and_then(Value::as_str).unwrap_or("");
    if matches!(kind, "union" | "intersection") {
        let children = feature
            .get("features")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{path}.features must be an array."))?;
        if children.len() < 2 {
            return Err(format!(
                "{path}.features must contain at least two same-dimension domain features."
            ));
        }
        let mut built = Vec::with_capacity(children.len());
        for (index, child) in children.iter().enumerate() {
            built.push(compile_domain_feature(
                child,
                symbols,
                &format!("{path}.features[{index}]"),
                next_geo_id,
                next_boolean_id,
                geo,
            )?);
        }
        let result = format!("{}{}", kind, *next_boolean_id);
        *next_boolean_id += 1;
        if kind == "union" {
            let object = built[0].geo_id.clone();
            let tools = built
                .iter()
                .skip(1)
                .map(|child| child.geo_id.as_str())
                .collect::<Vec<_>>()
                .join(",");
            // A fragment operation still represents the union of the child
            // domains, while keeping their shared interfaces available for
            // address-based selections and distinct material assignments.
            geo.push(format!("{result}[] = BooleanFragments{{ Surface{{{object}}}; Delete; }}{{ Surface{{{tools}}}; Delete; }}; // {}",comment(&name)));
        } else {
            let current = format!("{result}Current");
            geo.push(format!("{current}[] = {{{}}};", built[0].geo_id));
            for (index, child) in built.iter().enumerate().skip(1) {
                let step = format!("{result}Step{index}");
                geo.push(format!("{step}[] = {{}};"));
                geo.push(format!("If (#{current}[] > 0)"));
                geo.push(format!("  {step}[] = BooleanIntersection{{ Surface{{{current}[]}}; Delete; }}{{ Surface{{{}}}; Delete; }};",child.geo_id));
                geo.push("Else".to_string());
                geo.push(format!("  Delete{{ Surface{{{}}}; }}", child.geo_id));
                geo.push("EndIf".to_string());
                geo.push(format!("{current}[] = {{{step}[]}};"));
            }
            geo.push(format!(
                "{result}[] = {{{current}[]}}; // {}",
                comment(&name)
            ));
            geo.push(format!(
                "If (#{result}[] == 0) Error(\"Intersection {} has an empty result.\"); EndIf",
                quote(&name)
            ));
        }
        let bounds = [0, 1]
            .map(|axis| {
                if kind == "union" {
                    built
                        .iter()
                        .map(|child| child.bounds[axis])
                        .fold(f64::INFINITY, f64::min)
                } else {
                    built
                        .iter()
                        .map(|child| child.bounds[axis])
                        .fold(f64::NEG_INFINITY, f64::max)
                }
            })
            .into_iter()
            .chain([2, 3].map(|axis| {
                if kind == "union" {
                    built
                        .iter()
                        .map(|child| child.bounds[axis])
                        .fold(f64::NEG_INFINITY, f64::max)
                } else {
                    built
                        .iter()
                        .map(|child| child.bounds[axis])
                        .fold(f64::INFINITY, f64::min)
                }
            }))
            .collect::<Vec<_>>()
            .try_into()
            .expect("four bounds");
        let primitives = built
            .iter()
            .map(|child| {
                let mut primitive = child.primitive.clone();
                primitive["id"] = json!(child.id);
                primitive
            })
            .collect::<Vec<_>>();
        return Ok(Feature {
            id,
            name,
            bounds,
            geo_id: format!("{result}[]"),
            primitive: json!({"type":kind,"features":primitives}),
        });
    }
    let (bounds, primitive_geo, primitive) = feature_bounds(feature, symbols, path)?;
    // Earlier boolean results may use tags beyond the primitive counter.
    let geo_id = format!("fullmetaSurface{}", *next_geo_id);
    *next_geo_id += 1;
    let primitive_geo = primitive_geo.replacen("(0)", &format!("({geo_id})"), 1);
    geo.push(format!(
        "{geo_id} = news; {primitive_geo} // {}",
        comment(&name)
    ));
    Ok(Feature {
        id,
        name,
        bounds,
        geo_id,
        primitive,
    })
}

fn emit_domain_selection(
    geo: &mut Vec<String>,
    variable: &str,
    selection_id: &str,
    selections: &HashMap<String, &Value>,
    features: &[Feature],
    address_primitives: &[Value],
    _feature_by_id: &HashMap<String, usize>,
    tolerance: f64,
) -> Result<()> {
    let selection = selections
        .get(selection_id)
        .ok_or_else(|| format!("Selection {selection_id:?} does not exist."))?;
    if selection.get("entity").and_then(Value::as_str) != Some("domain") {
        return Err(format!("Selection {selection_id:?} must select a domain."));
    }
    let feature_ids = address_primitives
        .iter()
        .map(|primitive| primitive["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let addresses = crate::selection_address::selection_addresses(
        selection,
        &feature_ids,
        &format!("selection {selection_id}"),
    )?;
    let primitives = address_primitives.to_vec();
    crate::selection_address::emit_domain_address_union(
        geo,
        variable,
        2,
        &addresses,
        &primitives,
        "SurfacesList",
        tolerance,
    )
}

pub fn compile(model: &Value, component_id: Option<&str>) -> Result<CompiledGeometry> {
    if model.get("modelFormat").and_then(Value::as_str) != Some("fem-model/2") {
        return Err("The dimension-aware geometry compiler requires fem-model/2.".into());
    }
    match model.pointer("/scenario/dimension").and_then(Value::as_u64) {
        Some(3) => return three_d::compile(model, component_id),
        Some(2) => {}
        _ => {
            return Err(
                "The version 2 geometry compiler requires scenario.dimension to be 2 or 3.".into(),
            );
        }
    }
    let parameters = model
        .pointer("/model/parameters")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let symbols = resolve_parameters(parameters, model)?;
    if component_id.is_some_and(|id| id != "model") {
        return Err("The geometry compiler accepts the model root only.".into());
    }
    let component = model.pointer("/model").ok_or("model is required.")?;
    let c_id = "model".to_string();
    let c_name = model
        .pointer("/metadata/name")
        .and_then(Value::as_str)
        .unwrap_or("Model");
    let tolerance = quantity(
        component
            .pointer("/geometry/tolerance")
            .ok_or("model.geometry.tolerance is required.")?,
        &symbols,
        "model.geometry.tolerance",
    )?;
    if tolerance <= 0.0 {
        return Err("model.geometry.tolerance must be positive.".into());
    }
    let feature_values = component
        .pointer("/geometry/features")
        .and_then(Value::as_array)
        .ok_or("model.geometry.features must be an array.")?;
    if feature_values.is_empty() {
        return Err("model.geometry.features must contain at least one feature.".into());
    }
    let mut features = Vec::with_capacity(feature_values.len());
    let mut construction_features = Vec::new();
    let mut feature_by_id = HashMap::<String, usize>::new();
    let mut lower_feature_ids = HashSet::<String>::new();
    let mut next_geo_id = 1usize;
    let mut next_boolean_id = 1usize;
    let mut geo = vec![
        format!("// Generated from FullMeta model {}.", comment(c_name)),
        "SetFactory(\"OpenCASCADE\");".to_string(),
    ];
    for (index, feature) in feature_values.iter().enumerate() {
        let path = format!("model.geometry.features[{index}]");
        if matches!(
            feature.get("type").and_then(Value::as_str),
            Some("lineSegment" | "circleCurve")
        ) {
            let id = text(
                feature
                    .get("id")
                    .ok_or_else(|| format!("{path}.id is required."))?,
                &format!("{path}.id"),
            )?
            .to_string();
            if feature_by_id.contains_key(&id) || !lower_feature_ids.insert(id.clone()) {
                return Err(format!(
                    "Geometry feature id {id:?} is used more than once."
                ));
            }
            construction_features.push(compile_lower_feature(
                feature, &symbols, &path, index, &mut geo,
            )?);
            continue;
        }
        let id = text(
            feature
                .get("id")
                .ok_or_else(|| format!("{path}.id is required."))?,
            &format!("{path}.id"),
        )?
        .to_string();
        if lower_feature_ids.contains(&id) {
            return Err(format!(
                "Geometry feature id {id:?} is used more than once."
            ));
        }
        let built = compile_domain_feature(
            feature,
            &symbols,
            &path,
            &mut next_geo_id,
            &mut next_boolean_id,
            &mut geo,
        )?;
        feature_by_id.insert(id.clone(), features.len());
        features.push(built);
    }
    if features.is_empty() {
        return Err("At least one rectangle or circle area is required for a 2D mesh.".into());
    }
    if features.len() == 1 {
        if !matches!(
            features[0].primitive.get("type").and_then(Value::as_str),
            Some("union" | "intersection")
        ) {
            geo.push("Coherence;".to_string());
        }
        geo.push(format!("allSurfaces[] = {{{}}};", features[0].geo_id));
    } else {
        geo.push("allSurfaceInputs[] = {};".to_string());
        for feature in &features {
            geo.push(format!("allSurfaceInputs[] += {{{}}};", feature.geo_id));
        }
        geo.push(
            "allSurfaces[] = BooleanFragments{ Surface{allSurfaceInputs[]}; Delete; }{};"
                .to_string(),
        );
    }
    geo.push(format!("tol={};", fmt(tolerance)));

    let root_address_primitives = features
        .iter()
        .map(|feature| {
            let mut primitive = feature.primitive.clone();
            primitive["id"] = json!(feature.id);
            primitive
        })
        .collect::<Vec<_>>();
    let nested_primitives =
        crate::selection_address::nested_address_primitives(&root_address_primitives)?;
    let root_ids = features
        .iter()
        .map(|feature| feature.id.as_str())
        .collect::<HashSet<_>>();
    let has_nested_address = component
        .get("selections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|selection| {
            selection
                .get("addresses")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        })
        .filter_map(|row| row.get("membership").and_then(Value::as_object))
        .flat_map(|map| map.keys())
        .any(|id| !root_ids.contains(id.as_str()));
    let address_primitives = if has_nested_address {
        nested_primitives
    } else {
        root_address_primitives
    };
    let selection_values = component
        .get("selections")
        .and_then(Value::as_array)
        .ok_or("model.selections must be an array.")?;
    let selections: HashMap<String, &Value> = selection_values
        .iter()
        .filter_map(|selection| Some((selection.get("id")?.as_str()?.to_string(), selection)))
        .collect();
    let materials = component
        .get("materials")
        .and_then(Value::as_array)
        .ok_or("model.materials must be an array.")?;
    geo.push("claimedSurfaces[] = {};".to_string());
    let mut physical_groups = Vec::with_capacity(materials.len());
    let mut material_selections = HashSet::new();
    for (index, material) in materials.iter().enumerate() {
        let path = format!("model.materials[{index}]");
        let id = text(
            material
                .get("id")
                .ok_or_else(|| format!("{path}.id is required."))?,
            &format!("{path}.id"),
        )?
        .to_string();
        let name = text(
            material
                .get("name")
                .ok_or_else(|| format!("{path}.name is required."))?,
            &format!("{path}.name"),
        )?
        .to_string();
        let selection_id = text(
            material
                .get("selectionId")
                .ok_or_else(|| format!("{path}.selectionId is required."))?,
            &format!("{path}.selectionId"),
        )?
        .to_string();
        if !material_selections.insert(selection_id.clone()) {
            return Err(format!(
                "Multiple materials use domain selection {selection_id:?}."
            ));
        }
        let selection = selections
            .get(&selection_id)
            .ok_or_else(|| format!("{path} references missing selection {selection_id:?}."))?;
        if selection.get("entity").and_then(Value::as_str) != Some("domain") {
            return Err(format!(
                "{path}.selectionId must reference a domain selection."
            ));
        }
        let variable = format!("materialSurfaces_{}", index + 1);
        emit_domain_selection(
            &mut geo,
            &variable,
            &selection_id,
            &selections,
            &features,
            &address_primitives,
            &feature_by_id,
            tolerance,
        )?;
        geo.push(format!("{variable}[] -= {{claimedSurfaces[]}};"));
        geo.push(format!("claimedSurfaces[] += {{{variable}[]}};"));
        geo.push(format!("If (#{variable}[] > 0)"));
        geo.push(format!(
            "  Physical Surface(\"{}\", {}) = {{{variable}[]}};",
            quote(&name),
            index + 1
        ));
        geo.push("EndIf".to_string());
        physical_groups
            .push(json!({"id":id,"name":name,"physicalTag":index + 1,"selectionId":selection_id}));
    }
    let unassigned_tag = materials.len() + 1;
    geo.push("unassignedSurfaces[] = {allSurfaces[]};".to_string());
    geo.push("unassignedSurfaces[] -= {claimedSurfaces[]};".to_string());
    geo.push("If (#unassignedSurfaces[] > 0)".to_string());
    geo.push(format!("  Physical Surface(\"__FullMeta_unassigned_surface_check__\", {unassigned_tag}) = {{unassignedSurfaces[]}};"));
    geo.push("EndIf".to_string());

    let mesh = component.get("mesh").ok_or("model.mesh is required.")?;
    if mesh.get("elementFamily").and_then(Value::as_str) != Some("triangle") {
        return Err("The 2D geometry compiler requires triangle mesh elements.".into());
    }
    let options = crate::mesh_options::parse(mesh, &symbols)?;
    let controls = crate::mesh_items::mesh_size_controls(mesh, &symbols)?;
    let mut mesh_size_fields = Vec::<(String, f64, &'static str)>::new();
    for (index, control) in controls.iter().enumerate() {
        let path = format!("model.mesh.meshSizeControls[{index}]");
        let selection_id = control.selection_id.as_str();
        let selection = selections.get(selection_id).ok_or_else(|| {
            format!("{path}.selectionId references missing selection {selection_id:?}.")
        })?;
        match selection.get("entity").and_then(Value::as_str) {
            Some("domain") => {
                let variable = format!("meshSizeDomain_{}", index + 1);
                emit_domain_selection(
                    &mut geo,
                    &variable,
                    selection_id,
                    &selections,
                    &features,
                    &address_primitives,
                    &feature_by_id,
                    tolerance,
                )?;
                mesh_size_fields.push((variable, control.size, "SurfacesList"));
            }
            Some("boundary") => {
                let boundary_index = selection_values.iter().filter(|entry| entry.get("entity").and_then(Value::as_str) == Some("boundary"))
                    .position(|entry| entry.get("id").and_then(Value::as_str) == Some(selection_id))
                    .ok_or_else(|| format!("{path}.selectionId references a boundary selection that was not compiled."))?;
                mesh_size_fields.push((
                    format!("boundary_{}", boundary_index + 1),
                    control.size,
                    "CurvesList",
                ));
            }
            _ => {
                return Err(format!(
                    "{path}.selectionId must reference an area or boundary-edge selection in a 2D model."
                ));
            }
        }
    }
    let min_x = features
        .iter()
        .map(|feature| feature.bounds[0])
        .fold(f64::INFINITY, f64::min);
    let min_y = features
        .iter()
        .map(|feature| feature.bounds[1])
        .fold(f64::INFINITY, f64::min);
    let max_x = features
        .iter()
        .map(|feature| feature.bounds[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = features
        .iter()
        .map(|feature| feature.bounds[3])
        .fold(f64::NEG_INFINITY, f64::max);
    let primitives = address_primitives.to_vec();
    let domain_feature_ids = address_primitives
        .iter()
        .map(|primitive| primitive["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let mut boundary_selections = Vec::new();
    for (index, selection) in selection_values
        .iter()
        .filter(|selection| selection.get("entity").and_then(Value::as_str) == Some("boundary"))
        .enumerate()
    {
        let id = text(
            selection
                .get("id")
                .ok_or("Boundary selection id is required.")?,
            "boundary selection.id",
        )?
        .to_string();
        let name = text(
            selection
                .get("name")
                .ok_or("Boundary selection name is required.")?,
            "boundary selection.name",
        )?
        .to_string();
        let addresses = crate::selection_address::selection_addresses(
            selection,
            &domain_feature_ids,
            &format!("selection {id}"),
        )?;
        let variable = format!("boundary_{}", index + 1);
        let tag = index + 1;
        let (plane_filter, deferred_filter) = crate::selection_address::selection_filter(
            selection,
            &symbols,
            2,
            &format!("selection {id}"),
        )?;
        crate::selection_address::emit_address_selection_union(
            &mut geo,
            &variable,
            2,
            "boundary",
            &addresses,
            &primitives,
            plane_filter,
            tolerance,
        )?;
        if deferred_filter.is_some() {
            geo.push(format!("// FullMeta deferred zero filter {variable}"));
        }
        geo.push(format!(
            "Physical Curve(\"{}\", {tag}) = {{{variable}[]}};",
            quote(&name)
        ));
        let mut row =
            json!({"id":id,"name":name,"addresses":addresses,"physicalTag":tag,"entity":"edge"});
        if let Some(filter) = plane_filter {
            row["plane"] = json!({"pointM":filter.point,"normal":filter.normal});
        }
        if let Some(mut filter) = deferred_filter {
            filter["variable"] = json!(variable);
            row["zeroExpressionFilter"] = filter;
        }
        boundary_selections.push(row);
    }
    let periodic_extents = [(min_x, max_x), (min_y, max_y), (0.0, 0.0)];
    let (periodic_boundaries, periodic_pair_records) = crate::mesh_items::compile_periodic_pairs(
        mesh,
        &selections,
        2,
        &domain_feature_ids,
        periodic_extents,
        &symbols,
    )?;
    for periodic in &periodic_boundaries {
        let lower_index = selection_values
            .iter()
            .filter(|entry| entry.get("entity").and_then(Value::as_str) == Some("boundary"))
            .position(|entry| {
                entry.get("id").and_then(Value::as_str) == Some(&periodic.lower_selection_id)
            })
            .ok_or_else(|| {
                format!(
                    "Periodic pair {} lower boundary was not compiled.",
                    periodic.name
                )
            })?;
        let upper_index = selection_values
            .iter()
            .filter(|entry| entry.get("entity").and_then(Value::as_str) == Some("boundary"))
            .position(|entry| {
                entry.get("id").and_then(Value::as_str) == Some(&periodic.upper_selection_id)
            })
            .ok_or_else(|| {
                format!(
                    "Periodic pair {} upper boundary was not compiled.",
                    periodic.name
                )
            })?;
        crate::mesh_items::emit_periodic(
            &mut geo,
            periodic,
            &format!("boundary_{}", lower_index + 1),
            &format!("boundary_{}", upper_index + 1),
            tolerance.max(2e-7),
        );
    }
    let mut point_selections = Vec::new();
    for (index, selection) in selection_values
        .iter()
        .filter(|selection| selection.get("entity").and_then(Value::as_str) == Some("point"))
        .enumerate()
    {
        let id = text(
            selection
                .get("id")
                .ok_or("Point selection id is required.")?,
            "point selection.id",
        )?
        .to_string();
        let name = text(
            selection
                .get("name")
                .ok_or("Point selection name is required.")?,
            "point selection.name",
        )?
        .to_string();
        let addresses = crate::selection_address::selection_addresses(
            selection,
            &domain_feature_ids,
            &format!("selection {id}"),
        )?;
        let variable = format!("point_{}", index + 1);
        let tag = index + 1;
        let (plane_filter, deferred_filter) = crate::selection_address::selection_filter(
            selection,
            &symbols,
            2,
            &format!("selection {id}"),
        )?;
        crate::selection_address::emit_address_selection_union(
            &mut geo,
            &variable,
            2,
            "point",
            &addresses,
            &primitives,
            plane_filter,
            tolerance,
        )?;
        if deferred_filter.is_some() {
            geo.push(format!("// FullMeta deferred zero filter {variable}"));
        }
        geo.push(format!(
            "Physical Point(\"{}\", {tag}) = {{{variable}[]}};",
            quote(&name)
        ));
        let mut row = json!({"id":id,"name":name,"addresses":addresses,"physicalTag":tag});
        if let Some(filter) = plane_filter {
            row["plane"] = json!({"pointM":filter.point,"normal":filter.normal});
        }
        if let Some(mut filter) = deferred_filter {
            filter["variable"] = json!(variable);
            row["zeroExpressionFilter"] = filter;
        }
        point_selections.push(row);
    }
    crate::mesh_options::emit(&mut geo, &options, !mesh_size_fields.is_empty(), 2);
    crate::mesh_items::emit_background_entity_size_fields(&mut geo, 1, &mesh_size_fields);
    geo.push("Mesh 2;".to_string());
    let geometry_features = features
        .iter()
        .map(|feature| json!({"id":feature.id,"name":feature.name,"gmshSurface":feature.geo_id}))
        .collect::<Vec<_>>();
    let primitives = address_primitives.clone();
    let domain_feature_ids = address_primitives
        .iter()
        .map(|primitive| primitive["id"].as_str().unwrap())
        .collect::<Vec<_>>();
    let domain_selections = selection_values
        .iter()
        .filter(|selection| selection.get("entity").and_then(Value::as_str) == Some("domain"))
        .cloned()
        .collect::<Vec<_>>();
    let map_data = json!({
        "schema":"fullmeta-mesh-map/6",
        "modelId":model.pointer("/metadata/id").and_then(Value::as_str).unwrap_or("unknown"),
        "componentId":c_id,
        "dimension":2,
        "units":"m",
        "physicalGroups":physical_groups,
        "boundarySelections":boundary_selections,
        "edgeSelections":[],
        "pointSelections":point_selections,
        "geometryFeatures":geometry_features,
        "constructionFeatures":construction_features,
        "toleranceM":tolerance,
        "primitives":primitives,
        "domainFeatureIds":domain_feature_ids,
        "domainSelections":domain_selections,
        "periodicPairs":periodic_pair_records
    });
    Ok(CompiledGeometry {
        component_id: c_id,
        geo: geo.join("\n") + "\n",
        map_data,
    })
}

#[cfg(test)]
mod tests {
    use super::compile;
    use serde_json::json;

    fn model(features: serde_json::Value) -> serde_json::Value {
        json!({
            "modelFormat": "fem-model/2",
            "x-fullmeta-schema": "../fem.schema.json",
            "metadata": {"id": "lower-feature-test", "name": "Lower feature test"},
            "scenario": {"dimension": 2, "profileId": "acoustic-2d"},
            "model": {
                "parameters": [],
                "geometry": {
                    "tolerance": {"expression": "1e-8", "unit": "m"},
                    "features": features
                },
                "selections": [],
                "materials": [],
                "mesh": {
                    "elementFamily": "triangle",
                    "periodicPairs": [],
                    "meshSizeControls": [],
                    "minSize": {"expression": "0.001", "unit": "m"},
                    "maxSize": {"expression": "0.01", "unit": "m"},
                    "gmsh": {
                        "algorithm3D": 1,
                        "elementOrder": 1,
                        "optimize": false,
                        "optimizeNetgen": false,
                        "optimizeThreshold": 0.3,
                        "smoothing": 0,
                        "threads": 1
                    }
                }
            }
        })
    }

    fn select_schema_once() {
        static SELECTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        SELECTED.get_or_init(|| {
            let model_path =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fem_model.json");
            let model = json!({"x-fullmeta-schema":"../fem.schema.json"});
            crate::schema_metadata::select_model_schema(&model, Some(&model_path))
                .expect("the FEM schema fixture resolves from the mesh crate directory");
        });
    }

    #[test]
    fn compiles_2d_construction_curves_without_adding_domain_membership_bits() {
        select_schema_once();
        let model = model(json!([
            {"id":"area","name":"Area","type":"rectangle","position":[{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}],"size":[{"expression":"1","unit":"m"},{"expression":"1","unit":"m"}]},
            {"id":"segment","name":"Segment","type":"lineSegment","start":[{"expression":"0","unit":"m"},{"expression":"0.5","unit":"m"}],"end":[{"expression":"1","unit":"m"},{"expression":"0.5","unit":"m"}]},
            {"id":"arc","name":"Arc","type":"circleCurve","center":[{"expression":"0.5","unit":"m"},{"expression":"0.5","unit":"m"}],"radius":{"expression":"0.1","unit":"m"}}
        ]));

        let compiled = compile(&model, None).expect("schema-supported 2D curves compile");
        assert!(compiled.geo.contains("Line(fullmetaLower1Curve)"));
        assert!(compiled.geo.contains("Circle(fullmetaLower2Curve0)"));
        assert_eq!(compiled.map_data["domainFeatureIds"], json!(["area"]));
        assert_eq!(
            compiled.map_data["geometryFeatures"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            compiled.map_data["constructionFeatures"],
            json!([
                {"id":"segment","name":"Segment","type":"lineSegment","gmshCurves":["fullmetaLower1Curve"]},
                {"id":"arc","name":"Arc","type":"circleCurve","gmshCurves":["fullmetaLower2Curve0","fullmetaLower2Curve1","fullmetaLower2Curve2","fullmetaLower2Curve3"]}
            ])
        );
        assert_eq!(compiled.map_data["domainFeatureIds"], json!(["area"]));
    }

    #[test]
    fn rejects_a_degenerate_2d_line_segment_with_a_path_specific_error() {
        select_schema_once();
        let model = model(json!([
            {"id":"area","name":"Area","type":"rectangle","position":[{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}],"size":[{"expression":"1","unit":"m"},{"expression":"1","unit":"m"}]},
            {"id":"segment","name":"Segment","type":"lineSegment","start":[{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}],"end":[{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}]}
        ]));

        let error = match compile(&model, None) {
            Ok(_) => panic!("degenerate geometry must be rejected"),
            Err(error) => error,
        };
        assert!(error.contains("model.geometry.features[1].start and .end must differ."));
    }
}
