use super::CompiledGeometry;
use crate::expression::{Result, Scalar, evaluate_quantity, resolve_parameters};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

type V3 = [f64; 3];

struct Feature {
    id: String,
    name: String,
    bounds: [f64; 6],
    geo_id: String,
    primitive: Value,
    geo: String,
}

struct LowerFeature {
    id: String,
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

fn vector3(value: &Value, symbols: &HashMap<String, Scalar>, path: &str) -> Result<V3> {
    let items = value
        .as_array()
        .ok_or_else(|| format!("{path} must be a three-item array."))?;
    if items.len() != 3 {
        return Err(format!("{path} must be a three-item array."));
    }
    Ok([
        quantity(&items[0], symbols, &format!("{path}[0]"))?,
        quantity(&items[1], symbols, &format!("{path}[1]"))?,
        quantity(&items[2], symbols, &format!("{path}[2]"))?,
    ])
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn normalized(vector: V3, path: &str) -> Result<V3> {
    let norm = dot(vector, vector).sqrt();
    if !norm.is_finite() || norm <= 1e-14 {
        return Err(format!("{path} must be a finite, nonzero vector."));
    }
    Ok(vector.map(|value| value / norm))
}

fn lower_feature(
    value: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
    geo: &mut Vec<String>,
) -> Result<LowerFeature> {
    let id = text(
        value
            .get("id")
            .ok_or_else(|| format!("{path}.id is required."))?,
        &format!("{path}.id"),
    )?
    .to_string();
    let name = text(
        value
            .get("name")
            .ok_or_else(|| format!("{path}.name is required."))?,
        &format!("{path}.name"),
    )?
    .to_string();
    let suffix = id.replace(|ch: char| !ch.is_ascii_alphanumeric(), "_");
    let mut geo_ids = Vec::new();
    match value.get("type").and_then(Value::as_str).unwrap_or("") {
        "lineSegment" => {
            let start = vector3(
                value
                    .get("start")
                    .ok_or_else(|| format!("{path}.start is required."))?,
                symbols,
                &format!("{path}.start"),
            )?;
            let end = vector3(
                value
                    .get("end")
                    .ok_or_else(|| format!("{path}.end is required."))?,
                symbols,
                &format!("{path}.end"),
            )?;
            if (0..3).all(|axis| (start[axis] - end[axis]).abs() <= 1e-14) {
                return Err(format!("{path}.start and .end must differ."));
            }
            let p0 = format!("{suffix}_startPoint");
            let p1 = format!("{suffix}_endPoint");
            let line = format!("{suffix}_curve");
            geo.push(format!(
                "{p0}=newp; Point({p0})={{{},{},{},1.0}};",
                fmt(start[0]),
                fmt(start[1]),
                fmt(start[2])
            ));
            geo.push(format!(
                "{p1}=newp; Point({p1})={{{},{},{},1.0}};",
                fmt(end[0]),
                fmt(end[1]),
                fmt(end[2])
            ));
            geo.push(format!(
                "{line}=newl; Line({line})={{{p0},{p1}}}; // {}",
                comment(&name)
            ));
            geo_ids.push(line);
            Ok(LowerFeature { id })
        }
        "circleCurve" | "diskSurface" => {
            let center = vector3(
                value
                    .get("center")
                    .ok_or_else(|| format!("{path}.center is required."))?,
                symbols,
                &format!("{path}.center"),
            )?;
            let normal = if let Some(normal_values) = value.get("normal").and_then(Value::as_array)
            {
                if normal_values.len() != 3 {
                    return Err(format!("{path}.normal must be a three-number vector."));
                }
                normalized(
                    std::array::from_fn(|index| normal_values[index].as_f64().unwrap_or(f64::NAN)),
                    &format!("{path}.normal"),
                )?
            } else {
                [0.0, 0.0, 1.0]
            };
            let radius = quantity(
                value
                    .get("radius")
                    .ok_or_else(|| format!("{path}.radius is required."))?,
                symbols,
                &format!("{path}.radius"),
            )?;
            if !radius.is_finite() || radius <= 0.0 {
                return Err(format!("{path}.radius must be positive and finite."));
            }
            let helper = if normal[0].abs() < 0.8 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let u = normalized(cross(normal, helper), &format!("{path}.normal"))?;
            let v = cross(normal, u);
            let center_tag = format!("{suffix}_centerPoint");
            geo.push(format!(
                "{center_tag}=newp; Point({center_tag})={{{},{},{},1.0}};",
                fmt(center[0]),
                fmt(center[1]),
                fmt(center[2])
            ));
            let mut points = Vec::new();
            for quarter in 0..4 {
                let angle = quarter as f64 * std::f64::consts::FRAC_PI_2;
                let point = std::array::from_fn::<_, 3, _>(|axis| {
                    center[axis] + radius * (u[axis] * angle.cos() + v[axis] * angle.sin())
                });
                let point_tag = format!("{suffix}_point_{quarter}");
                geo.push(format!(
                    "{point_tag}=newp; Point({point_tag})={{{},{},{},1.0}};",
                    fmt(point[0]),
                    fmt(point[1]),
                    fmt(point[2])
                ));
                points.push(point_tag);
            }
            for quarter in 0..4 {
                let curve = format!("{suffix}_curve_{quarter}");
                geo.push(format!(
                    "{curve}=newl; Circle({curve})={{{},{},{}}};",
                    points[quarter],
                    center_tag,
                    points[(quarter + 1) % 4]
                ));
                geo_ids.push(curve);
            }
            if value.get("type").and_then(Value::as_str) == Some("circleCurve") {
                geo.push(format!("// {}", comment(&name)));
                Ok(LowerFeature { id })
            } else {
                let loop_tag = format!("{suffix}_loop");
                let surface_tag = format!("{suffix}_surface");
                geo.push(format!(
                    "{loop_tag}=newll; Curve Loop({loop_tag})={{{}}};",
                    geo_ids.join(",")
                ));
                geo.push(format!(
                    "{surface_tag}=news; Plane Surface({surface_tag})={{{loop_tag}}}; // {}",
                    comment(&name)
                ));
                geo_ids.clear();
                geo_ids.push(surface_tag);
                Ok(LowerFeature { id })
            }
        }
        "planeRectangle" => {
            let origin = vector3(
                value
                    .get("origin")
                    .ok_or_else(|| format!("{path}.origin is required."))?,
                symbols,
                &format!("{path}.origin"),
            )?;
            let u = vector3(
                value
                    .get("uVector")
                    .ok_or_else(|| format!("{path}.uVector is required."))?,
                symbols,
                &format!("{path}.uVector"),
            )?;
            let v = vector3(
                value
                    .get("vVector")
                    .ok_or_else(|| format!("{path}.vVector is required."))?,
                symbols,
                &format!("{path}.vVector"),
            )?;
            if dot(cross(u, v), cross(u, v)).sqrt() <= 1e-14 {
                return Err(format!(
                    "{path}.uVector and .vVector must span a nonzero area."
                ));
            }
            let points = [
                origin,
                std::array::from_fn(|a| origin[a] + u[a]),
                std::array::from_fn(|a| origin[a] + u[a] + v[a]),
                std::array::from_fn(|a| origin[a] + v[a]),
            ];
            let point_tags = (0..4)
                .map(|index| format!("{suffix}_point_{index}"))
                .collect::<Vec<_>>();
            for (tag, point) in point_tags.iter().zip(points) {
                geo.push(format!(
                    "{tag}=newp; Point({tag})={{{},{},{},1.0}};",
                    fmt(point[0]),
                    fmt(point[1]),
                    fmt(point[2])
                ));
            }
            for index in 0..4 {
                let line = format!("{suffix}_curve_{index}");
                geo.push(format!(
                    "{line}=newl; Line({line})={{{},{}}};",
                    point_tags[index],
                    point_tags[(index + 1) % 4]
                ));
                geo_ids.push(line);
            }
            let loop_tag = format!("{suffix}_loop");
            let surface_tag = format!("{suffix}_surface");
            geo.push(format!(
                "{loop_tag}=newll; Curve Loop({loop_tag})={{{}}};",
                geo_ids.join(",")
            ));
            geo.push(format!(
                "{surface_tag}=news; Plane Surface({surface_tag})={{{loop_tag}}}; // {}",
                comment(&name)
            ));
            geo_ids.clear();
            geo_ids.push(surface_tag);
            Ok(LowerFeature { id })
        }
        other => Err(format!(
            "{path}.type {other:?} is not a supported lower-dimensional 3D primitive."
        )),
    }
}

fn feature(
    feature: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
    geo_id: usize,
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
    // Boolean operations can allocate extra entities before the next primitive.
    // The counter names a variable; Gmsh chooses the actual unused volume tag.
    let geo_id = format!("fullmetaVolume{geo_id}");
    match feature.get("type").and_then(Value::as_str).unwrap_or("") {
        "box" => {
            let position = vector3(
                feature
                    .get("position")
                    .ok_or_else(|| format!("{path}.position is required."))?,
                symbols,
                &format!("{path}.position"),
            )?;
            let size = vector3(
                feature
                    .get("size")
                    .ok_or_else(|| format!("{path}.size is required."))?,
                symbols,
                &format!("{path}.size"),
            )?;
            if size.iter().any(|value| !value.is_finite() || *value <= 0.0) {
                return Err(format!("{path}.size must contain positive finite lengths."));
            }
            let min = if feature.get("positionMode").and_then(Value::as_str) == Some("center") {
                std::array::from_fn(|axis| position[axis] - size[axis] / 2.0)
            } else {
                position
            };
            let bounds = [
                min[0],
                min[1],
                min[2],
                min[0] + size[0],
                min[1] + size[1],
                min[2] + size[2],
            ];
            let geo = format!(
                "{geo_id} = newv; Box({geo_id}) = {{{},{},{},{},{},{}}};",
                fmt(min[0]),
                fmt(min[1]),
                fmt(min[2]),
                fmt(size[0]),
                fmt(size[1]),
                fmt(size[2])
            );
            Ok(Feature {
                id,
                name,
                bounds,
                geo_id,
                primitive: json!({"type":"box","min":min,"size":size}),
                geo,
            })
        }
        "cylinder" => {
            let base = vector3(
                feature
                    .get("base")
                    .ok_or_else(|| format!("{path}.base is required."))?,
                symbols,
                &format!("{path}.base"),
            )?;
            let axis_values = feature
                .get("axis")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("{path}.axis must be a three-number vector."))?;
            if axis_values.len() != 3 {
                return Err(format!("{path}.axis must be a three-number vector."));
            }
            let axis: V3 =
                std::array::from_fn(|index| axis_values[index].as_f64().unwrap_or(f64::NAN));
            let norm = axis.iter().map(|value| value * value).sum::<f64>().sqrt();
            if !norm.is_finite() || norm <= 1e-14 {
                return Err(format!("{path}.axis must be finite and nonzero."));
            }
            let direction = axis.map(|value| value / norm);
            let radius = quantity(
                feature
                    .get("radius")
                    .ok_or_else(|| format!("{path}.radius is required."))?,
                symbols,
                &format!("{path}.radius"),
            )?;
            let height = quantity(
                feature
                    .get("height")
                    .ok_or_else(|| format!("{path}.height is required."))?,
                symbols,
                &format!("{path}.height"),
            )?;
            if !radius.is_finite() || radius <= 0.0 || !height.is_finite() || height <= 0.0 {
                return Err(format!(
                    "{path}.radius and height must be positive and finite."
                ));
            }
            let tip =
                std::array::from_fn::<_, 3, _>(|index| base[index] + direction[index] * height);
            let mut bounds = [0.0; 6];
            for axis in 0..3 {
                let radial = radius * (1.0 - direction[axis] * direction[axis]).max(0.0).sqrt();
                bounds[axis] = base[axis].min(tip[axis]) - radial;
                bounds[axis + 3] = base[axis].max(tip[axis]) + radial;
            }
            let displacement = direction.map(|value| value * height);
            let geo = format!(
                "{geo_id} = newv; Cylinder({geo_id}) = {{{},{},{},{},{},{},{}}};",
                fmt(base[0]),
                fmt(base[1]),
                fmt(base[2]),
                fmt(displacement[0]),
                fmt(displacement[1]),
                fmt(displacement[2]),
                fmt(radius)
            );
            Ok(Feature {
                id,
                name,
                bounds,
                geo_id,
                primitive: json!({"type":"cylinder","base":base,"axis":axis,"height":height,"radius":radius}),
                geo,
            })
        }
        kind => Err(format!(
            "{path}.type {kind:?} is not supported by the 3D geometry compiler."
        )),
    }
}

fn compile_domain_feature(
    value: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
    next_geo_id: &mut usize,
    next_boolean_id: &mut usize,
    geo: &mut Vec<String>,
) -> Result<Feature> {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    if matches!(kind, "union" | "intersection") {
        let id = text(
            value
                .get("id")
                .ok_or_else(|| format!("{path}.id is required."))?,
            &format!("{path}.id"),
        )?
        .to_string();
        let name = text(
            value
                .get("name")
                .ok_or_else(|| format!("{path}.name is required."))?,
            &format!("{path}.name"),
        )?
        .to_string();
        let children = value
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
            // Retain child interfaces in the union so that later membership
            // queries can distinguish the resulting volume partitions.
            geo.push(format!("{result}[] = BooleanFragments{{ Volume{{{object}}}; Delete; }}{{ Volume{{{tools}}}; Delete; }}; // {}",comment(&name)));
        } else {
            let current = format!("{result}Current");
            geo.push(format!("{current}[] = {{{}}};", built[0].geo_id));
            for (index, child) in built.iter().enumerate().skip(1) {
                let step = format!("{result}Step{index}");
                geo.push(format!("{step}[] = {{}};"));
                geo.push(format!("If (#{current}[] > 0)"));
                geo.push(format!("  {step}[] = BooleanIntersection{{ Volume{{{current}[]}}; Delete; }}{{ Volume{{{}}}; Delete; }};",child.geo_id));
                geo.push("Else".to_string());
                geo.push(format!("  Delete{{ Volume{{{}}}; }}", child.geo_id));
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
        let bounds = std::array::from_fn(|axis| {
            if kind == "union" {
                if axis < 3 {
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
            } else if axis < 3 {
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
        });
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
            geo: String::new(),
        });
    }
    let built = feature(value, symbols, path, *next_geo_id)?;
    *next_geo_id += 1;
    geo.push(format!("{} // {}", built.geo, comment(&built.name)));
    Ok(built)
}

fn emit_boolean_membership_map(
    geo: &mut Vec<String>,
    feature_index: usize,
    probe_variable: &str,
    tolerance: f64,
) {
    let membership = format!("domainFeatureMembership{feature_index}");
    let volume_index = format!("{membership}VolumeIndex");
    let target_copy = format!("{membership}TargetCopy");
    let feature_copy = format!("{membership}FeatureCopy");
    let intersections = format!("{membership}Intersections");
    let part_index = format!("{membership}PartIndex");
    let overlap_mass = format!("{membership}OverlapMass");
    let tolerance_volume = tolerance * tolerance * tolerance;
    geo.push(format!("{membership}[] = {{}};"));
    geo.push(format!("For {volume_index} In {{0:#allVolumes[]-1}}"));
    geo.push(format!(
        "  {target_copy}[] = Duplicata{{ Volume{{allVolumes[{volume_index}]}}; }};"
    ));
    geo.push(format!(
        "  {feature_copy}[] = Duplicata{{ Volume{{{probe_variable}[]}}; }};"
    ));
    geo.push(format!("  {intersections}[] = BooleanIntersection{{ Volume{{{target_copy}[]}}; Delete; }}{{ Volume{{{feature_copy}[]}}; Delete; }};"));
    geo.push(format!("  {overlap_mass} = 0;"));
    geo.push(format!("  For {part_index} In {{0:#{intersections}[]-1}}"));
    geo.push(format!(
        "    {overlap_mass} += Mass Volume{{{intersections}[{part_index}]}};"
    ));
    geo.push("  EndFor".to_string());
    geo.push(format!("  If ({overlap_mass} > {})", fmt(tolerance_volume)));
    geo.push(format!("    {membership}[] += {{1}};"));
    geo.push("  Else".to_string());
    geo.push(format!("    {membership}[] += {{0}};"));
    geo.push("  EndIf".to_string());
    geo.push(format!("  Delete{{ Volume{{{intersections}[]}}; }}"));
    geo.push("EndFor".to_string());
    geo.push(format!("Delete{{ Volume{{{probe_variable}[]}}; }}"));
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
    let membership = address_primitives
        .iter()
        .map(|primitive| {
            features
                .iter()
                .enumerate()
                .find(|(_, feature)| primitive["id"].as_str() == Some(feature.id.as_str()))
                .and_then(|(index, feature)| {
                    matches!(
                        feature.primitive.get("type").and_then(Value::as_str),
                        Some("union" | "intersection")
                    )
                    .then(|| format!("domainFeatureMembership{index}"))
                })
        })
        .collect::<Vec<_>>();
    crate::selection_address::emit_domain_address_union_with_membership(
        geo,
        variable,
        3,
        &addresses,
        &primitives,
        "VolumesList",
        tolerance,
        Some(&membership),
    )
}

pub fn compile(model: &Value, component_id: Option<&str>) -> Result<CompiledGeometry> {
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
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err("model.geometry.tolerance must be positive and finite.".into());
    }
    let feature_values = component
        .pointer("/geometry/features")
        .and_then(Value::as_array)
        .ok_or("model.geometry.features must be an array.")?;
    if feature_values.is_empty() {
        return Err("model.geometry.features must contain at least one feature.".into());
    }
    let mut features = Vec::with_capacity(feature_values.len());
    let mut lower_features = Vec::<LowerFeature>::new();
    let mut feature_by_id = HashMap::<String, usize>::new();
    let mut next_geo_id = 1usize;
    let mut next_boolean_id = 1usize;
    let mut geo = vec![
        format!("// Generated from FullMeta model {}.", comment(c_name)),
        "SetFactory(\"OpenCASCADE\");".to_string(),
    ];
    for (index, value) in feature_values.iter().enumerate() {
        let path = format!("model.geometry.features[{index}]");
        if matches!(
            value.get("type").and_then(Value::as_str),
            Some("circleCurve" | "lineSegment" | "planeRectangle" | "diskSurface")
        ) {
            let built = lower_feature(value, &symbols, &path, &mut geo)?;
            if lower_features.iter().any(|feature| feature.id == built.id)
                || feature_by_id.contains_key(&built.id)
            {
                return Err(format!(
                    "Geometry feature id {:?} is used more than once.",
                    built.id
                ));
            }
            lower_features.push(built);
            continue;
        }
        let built = compile_domain_feature(
            value,
            &symbols,
            &path,
            &mut next_geo_id,
            &mut next_boolean_id,
            &mut geo,
        )?;
        if lower_features.iter().any(|feature| feature.id == built.id) {
            return Err(format!(
                "Geometry feature id {:?} is used more than once.",
                built.id
            ));
        }
        feature_by_id.insert(built.id.clone(), features.len());
        features.push(built);
    }
    if features.is_empty() {
        return Err("At least one box or cylinder volume is required for a 3D mesh.".into());
    }
    let boolean_membership_probes = features
        .iter()
        .enumerate()
        .filter_map(|(index, feature)| {
            matches!(
                feature.primitive.get("type").and_then(Value::as_str),
                Some("union" | "intersection")
            )
            .then(|| {
                (
                    index,
                    format!("domainFeatureProbe{index}"),
                    feature.geo_id.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    for (_, probe, feature_geo_id) in &boolean_membership_probes {
        geo.push(format!(
            "{probe}[] = Duplicata{{ Volume{{{feature_geo_id}}}; }};"
        ));
    }
    if features.len() == 1 {
        if !matches!(
            features[0].primitive.get("type").and_then(Value::as_str),
            Some("union" | "intersection")
        ) {
            geo.push("Coherence;".to_string());
        }
        geo.push(format!("allVolumes[] = {{{}}};", features[0].geo_id));
    } else {
        geo.push("allVolumeInputs[] = {};".to_string());
        for feature in &features {
            geo.push(format!("allVolumeInputs[] += {{{}}};", feature.geo_id));
        }
        geo.push(
            "allVolumes[] = BooleanFragments{ Volume{allVolumeInputs[]}; Delete; }{};".to_string(),
        );
    }
    geo.push(format!("tol={};", fmt(tolerance)));
    for (feature_index, probe, _) in &boolean_membership_probes {
        emit_boolean_membership_map(&mut geo, *feature_index, probe, tolerance);
    }
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
    geo.push("claimedVolumes[] = {};".to_string());
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
        let variable = format!("materialVolumes_{}", index + 1);
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
        geo.push(format!("{variable}[] -= {{claimedVolumes[]}};"));
        geo.push(format!("claimedVolumes[] += {{{variable}[]}};"));
        geo.push(format!("If (#{variable}[] > 0)"));
        geo.push(format!(
            "  Physical Volume(\"{}\", {}) = {{{variable}[]}};",
            quote(&name),
            index + 1
        ));
        geo.push("EndIf".to_string());
        physical_groups
            .push(json!({"id":id,"name":name,"physicalTag":index+1,"selectionId":selection_id}));
    }
    let unassigned_tag = materials.len() + 1;
    geo.push("unassignedVolumes[] = {allVolumes[]};".to_string());
    geo.push("unassignedVolumes[] -= {claimedVolumes[]};".to_string());
    geo.push("If (#unassignedVolumes[] > 0)".to_string());
    geo.push(format!("  Physical Volume(\"__FullMeta_unassigned_volume_check__\", {unassigned_tag}) = {{unassignedVolumes[]}};"));
    geo.push("EndIf".to_string());

    let mesh = component.get("mesh").ok_or("model.mesh is required.")?;
    if mesh.get("elementFamily").and_then(Value::as_str) != Some("tetrahedron") {
        return Err("The 3D geometry compiler requires tetrahedron mesh elements.".into());
    }
    let options = crate::mesh_options::parse(mesh, &symbols)?;
    let mesh_controls = crate::mesh_items::mesh_size_controls(mesh, &symbols)?;
    let domain_feature_ids = address_primitives
        .iter()
        .map(|primitive| primitive["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let min = std::array::from_fn::<_, 3, _>(|axis| {
        features
            .iter()
            .map(|item| item.bounds[axis])
            .fold(f64::INFINITY, f64::min)
    });
    let max = std::array::from_fn::<_, 3, _>(|axis| {
        features
            .iter()
            .map(|item| item.bounds[axis + 3])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let extents = std::array::from_fn(|axis| (min[axis], max[axis]));
    let (periodic_boundaries, periodic_pair_records) = crate::mesh_items::compile_periodic_pairs(
        mesh,
        &selections,
        3,
        &domain_feature_ids,
        extents,
        &symbols,
    )?;
    let mut mesh_entity_variables = HashMap::<String, (String, &'static str)>::new();
    let primitives = address_primitives.to_vec();
    let feature_membership = address_primitives
        .iter()
        .map(|primitive| {
            features
                .iter()
                .enumerate()
                .find(|(_, feature)| primitive["id"].as_str() == Some(feature.id.as_str()))
                .and_then(|(index, feature)| {
                    matches!(
                        feature.primitive.get("type").and_then(Value::as_str),
                        Some("union" | "intersection")
                    )
                    .then(|| format!("domainFeatureMembership{index}"))
                })
        })
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
            3,
            &format!("selection {id}"),
        )?;
        crate::selection_address::emit_address_selection_union_with_membership(
            &mut geo,
            &variable,
            3,
            "boundary",
            &addresses,
            &primitives,
            plane_filter,
            tolerance,
            Some(&feature_membership),
        )?;
        if deferred_filter.is_some() {
            geo.push(format!("// FullMeta deferred zero filter {variable}"));
        }
        geo.push(format!(
            "Physical Surface(\"{}\", {tag}) = {{{variable}[]}};",
            quote(&name)
        ));
        let mut row =
            json!({"id":id,"name":name,"addresses":addresses,"physicalTag":tag,"entity":"face"});
        if let Some(filter) = plane_filter {
            row["plane"] = json!({"pointM":filter.point,"normal":filter.normal});
        }
        if let Some(mut filter) = deferred_filter {
            filter["variable"] = json!(variable);
            row["zeroExpressionFilter"] = filter;
        }
        mesh_entity_variables.insert(id.clone(), (variable, "SurfacesList"));
        boundary_selections.push(row);
    }
    let mut edge_selections = Vec::new();
    for (index, selection) in selection_values
        .iter()
        .filter(|selection| selection.get("entity").and_then(Value::as_str) == Some("edge"))
        .enumerate()
    {
        let id = text(
            selection
                .get("id")
                .ok_or("Edge selection id is required.")?,
            "edge selection.id",
        )?
        .to_string();
        let name = text(
            selection
                .get("name")
                .ok_or("Edge selection name is required.")?,
            "edge selection.name",
        )?
        .to_string();
        let addresses = crate::selection_address::selection_addresses(
            selection,
            &domain_feature_ids,
            &format!("selection {id}"),
        )?;
        let variable = format!("edge_{}", index + 1);
        let tag = index + 1;
        let (plane_filter, deferred_filter) = crate::selection_address::selection_filter(
            selection,
            &symbols,
            3,
            &format!("selection {id}"),
        )?;
        crate::selection_address::emit_address_selection_union_with_membership(
            &mut geo,
            &variable,
            3,
            "edge",
            &addresses,
            &primitives,
            plane_filter,
            tolerance,
            Some(&feature_membership),
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
        mesh_entity_variables.insert(id.clone(), (variable, "CurvesList"));
        edge_selections.push(row);
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
            3,
            &format!("selection {id}"),
        )?;
        crate::selection_address::emit_address_selection_union(
            &mut geo,
            &variable,
            3,
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
    for boundary in &periodic_boundaries {
        let (lower_variable, _) = mesh_entity_variables
            .get(&boundary.lower_selection_id)
            .ok_or_else(|| {
                format!(
                    "Periodic pair {} lower boundary was not compiled.",
                    boundary.name
                )
            })?;
        let (upper_variable, _) = mesh_entity_variables
            .get(&boundary.upper_selection_id)
            .ok_or_else(|| {
                format!(
                    "Periodic pair {} upper boundary was not compiled.",
                    boundary.name
                )
            })?;
        crate::mesh_items::emit_periodic(
            &mut geo,
            boundary,
            lower_variable,
            upper_variable,
            tolerance.max(2e-7),
        );
    }
    let mut size_fields = Vec::<(String, f64, &'static str)>::with_capacity(mesh_controls.len());
    for (index, control) in mesh_controls.iter().enumerate() {
        let path = format!("model.mesh.meshSizeControls[{index}]");
        let selection = selections.get(&control.selection_id).ok_or_else(|| {
            format!(
                "{path}.selectionId references missing selection {:?}.",
                control.selection_id
            )
        })?;
        match selection.get("entity").and_then(Value::as_str) {
            Some("domain") => {
                let variable = format!("meshSizeDomain_{}", index + 1);
                emit_domain_selection(
                    &mut geo,
                    &variable,
                    &control.selection_id,
                    &selections,
                    &features,
                    &address_primitives,
                    &feature_by_id,
                    tolerance,
                )?;
                size_fields.push((variable, control.size, "VolumesList"));
            }
            Some("boundary") | Some("edge") => {
                let (variable, entity_list) = mesh_entity_variables.get(&control.selection_id).ok_or_else(|| format!("Mesh-size control {} references a boundary or edge selection that was not compiled.", control.name))?;
                size_fields.push((variable.clone(), control.size, *entity_list));
            }
            Some(entity) => {
                return Err(format!(
                    "{path}.selectionId must reference a domain, boundary, or edge selection; found {entity:?}."
                ));
            }
            None => {
                return Err(format!(
                    "Selection {:?} has no entity type.",
                    control.selection_id
                ));
            }
        }
    }
    crate::mesh_items::emit_background_entity_size_fields(&mut geo, 1, &size_fields);
    crate::mesh_options::emit(&mut geo, &options, !size_fields.is_empty(), 3);
    geo.push("Mesh 3;".to_string());
    let primitive_rows = address_primitives.clone();
    let domain_selections: Vec<&Value> = selection_values
        .iter()
        .filter(|selection| selection.get("entity").and_then(Value::as_str) == Some("domain"))
        .collect();
    let map_data = json!({
        "schema":"fullmeta-mesh-map/6",
        "modelId":model.pointer("/metadata/id").and_then(Value::as_str).unwrap_or("unknown"),
        "componentId":c_id,
        "dimension":3,
        "units":"m",
        "physicalGroups":physical_groups,
        "boundarySelections":boundary_selections,
        "edgeSelections":edge_selections,
        "pointSelections":point_selections,
        "geometryFeatures":features.iter().map(|item| json!({"id":item.id,"name":item.name,"gmshVolume":item.geo_id})).collect::<Vec<_>>(),
        "toleranceM":tolerance,
        "primitives":primitive_rows,
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
