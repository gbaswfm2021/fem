use crate::expression::{Result, Scalar, evaluate_quantity};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct MeshSizeControl {
    pub id: String,
    pub name: String,
    pub selection_id: String,
    pub size: f64,
}

#[derive(Clone, Debug)]
pub struct PeriodicPair {
    pub id: String,
    pub name: String,
    pub lower_selection_id: String,
    pub upper_selection_id: String,
    pub translation: Value,
}

#[derive(Clone, Debug)]
pub struct PeriodicBoundary {
    pub id: String,
    pub name: String,
    pub dimension: usize,
    pub lower_selection_id: String,
    pub upper_selection_id: String,
    pub translation: [f64; 3],
}

fn required_text(value: &Value, key: &str, path: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| format!("{path}.{key} must be a string."))
}

pub fn mesh_size_controls(
    mesh: &Value,
    symbols: &HashMap<String, Scalar>,
) -> Result<Vec<MeshSizeControl>> {
    mesh.get("meshSizeControls")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(index, control)| {
            let path = format!("model.mesh.meshSizeControls[{index}]");
            let size = evaluate_quantity(
                control
                    .get("size")
                    .ok_or_else(|| format!("{path}.size is required."))?,
                symbols,
                &format!("{path}.size"),
            )?;
            if !size.is_finite() || size <= 0.0 {
                return Err(format!("{path}.size must be positive and finite."));
            }
            Ok(MeshSizeControl {
                id: required_text(control, "id", &path)?,
                name: required_text(control, "name", &path)?,
                selection_id: required_text(control, "selectionId", &path)?,
                size,
            })
        })
        .collect()
}

pub fn emit_entity_size_fields(
    geo: &mut Vec<String>,
    start_field: usize,
    fields: &[(String, f64, &'static str)],
) -> (usize, Vec<usize>) {
    let mut next = start_field;
    let mut ids = Vec::with_capacity(fields.len());
    for (selection_variable, size, entity_list) in fields {
        geo.push(format!("Field[{next}] = Constant; Field[{next}].IncludeBoundary = 1; Field[{next}].VIn = {}; Field[{next}].VOut = 1e200; Field[{next}].{entity_list} = {{{selection_variable}[]}};", fmt(*size)));
        ids.push(next);
        next += 1;
    }
    (next, ids)
}

/// Emit entity-based Gmsh size fields and combine them into one background field.
/// `entity_list` names the Gmsh entity property (`VolumesList`, `SurfacesList`,
/// `CurvesList`, ...), so 2D and 3D mesh controls share the same implementation.
pub fn emit_background_entity_size_fields(
    geo: &mut Vec<String>,
    start_field: usize,
    fields: &[(String, f64, &'static str)],
) -> usize {
    if fields.is_empty() {
        return start_field;
    }
    let (next_field, ids) = emit_entity_size_fields(geo, start_field, fields);
    let background = if ids.len() == 1 {
        ids[0]
    } else {
        geo.push(format!(
            "Field[{next_field}] = Min; Field[{next_field}].FieldsList = {{{}}};",
            ids.iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ));
        next_field
    };
    geo.push(format!("Background Field = {background};"));
    if ids.len() == 1 {
        next_field
    } else {
        next_field + 1
    }
}

fn periodic_pairs(mesh: &Value) -> Result<Vec<PeriodicPair>> {
    let pairs = mesh
        .get("periodicPairs")
        .and_then(Value::as_array)
        .ok_or("model.mesh.periodicPairs must be an array.")?;
    pairs
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            let path = format!("model.mesh.periodicPairs[{index}]");
            Ok(PeriodicPair {
                id: required_text(pair, "id", &path)?,
                name: required_text(pair, "name", &path)?,
                lower_selection_id: required_text(pair, "lowerSelectionId", &path)?,
                upper_selection_id: required_text(pair, "upperSelectionId", &path)?,
                translation: pair
                    .get("translation")
                    .cloned()
                    .ok_or_else(|| format!("{path}.translation is required."))?,
            })
        })
        .collect()
}

fn enabled_pair_ids(mesh: &Value) -> HashSet<String> {
    mesh.get("periodicPairIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

pub fn compile_periodic_pairs(
    mesh: &Value,
    selections: &HashMap<String, &Value>,
    dimension: usize,
    domain_feature_ids: &[String],
    _extents: [(f64, f64); 3],
    symbols: &HashMap<String, Scalar>,
) -> Result<(Vec<PeriodicBoundary>, Vec<Value>)> {
    let selected = enabled_pair_ids(mesh);
    let mut boundaries = Vec::new();
    let mut records = Vec::new();
    for pair in periodic_pairs(mesh)? {
        if !selected.contains(&pair.id) {
            continue;
        }
        let low_id = &pair.lower_selection_id;
        let high_id = &pair.upper_selection_id;
        let low = selections.get(low_id).ok_or_else(|| {
            format!(
                "Periodic pair {} references missing selection {low_id}.",
                pair.name
            )
        })?;
        let high = selections.get(high_id).ok_or_else(|| {
            format!(
                "Periodic pair {} references missing selection {high_id}.",
                pair.name
            )
        })?;
        let low_entity = low.get("entity").and_then(Value::as_str);
        let high_entity = high.get("entity").and_then(Value::as_str);
        if low_entity != Some("boundary") || high_entity != Some("boundary") {
            return Err(format!(
                "Periodic pair {} needs two boundary selections.",
                pair.name
            ));
        }
        let mut low_addresses = crate::selection_address::selection_addresses(
            low,
            domain_feature_ids,
            &format!("selection {low_id}"),
        )?;
        let mut high_addresses = crate::selection_address::selection_addresses(
            high,
            domain_feature_ids,
            &format!("selection {high_id}"),
        )?;
        low_addresses.sort();
        high_addresses.sort();
        if low_addresses != high_addresses {
            return Err(format!(
                "Periodic pair {} selections must have the same membership addresses.",
                pair.name
            ));
        }
        let lower_plane = crate::selection_address::plane_filter(
            low,
            symbols,
            dimension,
            &format!("selection {low_id}"),
        )?
        .ok_or_else(|| {
            format!(
                "Periodic pair {} lower selection needs a plane filter.",
                pair.name
            )
        })?;
        let upper_plane = crate::selection_address::plane_filter(
            high,
            symbols,
            dimension,
            &format!("selection {high_id}"),
        )?
        .ok_or_else(|| {
            format!(
                "Periodic pair {} upper selection needs a plane filter.",
                pair.name
            )
        })?;
        let normal_dot = (0..3)
            .map(|axis| lower_plane.normal[axis] * upper_plane.normal[axis])
            .sum::<f64>()
            .abs();
        if (normal_dot - 1.0).abs() > 1e-8 {
            return Err(format!(
                "Periodic pair {} selections must have parallel plane normals.",
                pair.name
            ));
        }
        let signed_separation = (0..3)
            .map(|axis| {
                (upper_plane.point[axis] - lower_plane.point[axis]) * lower_plane.normal[axis]
            })
            .sum::<f64>();
        let expected = lower_plane
            .normal
            .map(|component| component * signed_separation);
        let translation_values = pair.translation.as_array().ok_or_else(|| {
            format!(
                "Periodic pair {}.translation must contain three quantities.",
                pair.name
            )
        })?;
        if translation_values.len() != 3 {
            return Err(format!(
                "Periodic pair {}.translation must contain three quantities.",
                pair.name
            ));
        }
        let mut translation = [0.0; 3];
        for component in 0..3 {
            translation[component] = evaluate_quantity(
                &translation_values[component],
                symbols,
                &format!("periodic pair {}.translation[{component}]", pair.name),
            )?;
        }
        if dimension == 2 && translation[2].abs() > 1e-10 {
            return Err(format!(
                "Periodic pair {} cannot translate out of the 2D model plane.",
                pair.name
            ));
        }
        let tolerance = 1e-10_f64.max(
            translation
                .iter()
                .map(|value| value.abs())
                .fold(0.0, f64::max)
                * 1e-9,
        );
        if translation
            .iter()
            .zip(expected)
            .any(|(actual, expected)| (actual - expected).abs() > tolerance)
        {
            return Err(format!(
                "Periodic pair {}.translation must map the first selected boundary onto the translated boundary.",
                pair.name
            ));
        }
        if translation.iter().all(|value| value.abs() <= tolerance) {
            return Err(format!(
                "Periodic pair {} must translate between distinct boundary locations.",
                pair.name
            ));
        }
        boundaries.push(PeriodicBoundary {
            id: pair.id.clone(),
            name: pair.name.clone(),
            dimension,
            lower_selection_id: low_id.clone(),
            upper_selection_id: high_id.clone(),
            translation,
        });
        records.push(json!({"id":pair.id,"name":pair.name,"dimension":dimension,"lowerSelectionId":low_id,"upperSelectionId":high_id,"translation":pair.translation}));
    }
    Ok((boundaries, records))
}

fn fmt(value: f64) -> String {
    format!("{value:.15e}")
}
fn comment(value: &str) -> String {
    value
        .replace('\r', " ")
        .replace('\n', " ")
        .replace("*/", "*")
}

pub fn emit_periodic(
    geo: &mut Vec<String>,
    boundary: &PeriodicBoundary,
    lower_variable: &str,
    upper_variable: &str,
    tolerance: f64,
) {
    let delta = boundary
        .translation
        .iter()
        .map(|value| fmt(*value))
        .collect::<Vec<_>>()
        .join(",");
    let tag = comment(&boundary.id);
    let entity = if boundary.dimension == 2 {
        "Curve"
    } else {
        "Surface"
    };
    geo.push(format!("If (#{lower_variable}[] != #{upper_variable}[]) Error(\"Periodic pair {tag} has different numbers of lower and upper entities.\"); EndIf"));
    geo.push(format!(
        "For periodic_master_{tag} In {{0:#{lower_variable}[]-1}}"
    ));
    geo.push(format!("  periodic_master_bb_{tag}() = BoundingBox {entity} {{{lower_variable}(periodic_master_{tag})}};"));
    geo.push(format!(
        "  periodic_match_count_{tag} = 0; periodic_match_tag_{tag} = -1;"
    ));
    geo.push(format!(
        "  For periodic_slave_{tag} In {{0:#{upper_variable}[]-1}}"
    ));
    geo.push(format!("    periodic_slave_bb_{tag}() = BoundingBox {entity} {{{upper_variable}(periodic_slave_{tag})}};"));
    geo.push(format!("    If (Fabs(periodic_slave_bb_{tag}(0)-periodic_master_bb_{tag}(0)-({})) < {} && Fabs(periodic_slave_bb_{tag}(1)-periodic_master_bb_{tag}(1)-({})) < {} && Fabs(periodic_slave_bb_{tag}(2)-periodic_master_bb_{tag}(2)-({})) < {} && Fabs(periodic_slave_bb_{tag}(3)-periodic_master_bb_{tag}(3)-({})) < {} && Fabs(periodic_slave_bb_{tag}(4)-periodic_master_bb_{tag}(4)-({})) < {} && Fabs(periodic_slave_bb_{tag}(5)-periodic_master_bb_{tag}(5)-({})) < {})", fmt(boundary.translation[0]), fmt(tolerance), fmt(boundary.translation[1]), fmt(tolerance), fmt(boundary.translation[2]), fmt(tolerance), fmt(boundary.translation[0]), fmt(tolerance), fmt(boundary.translation[1]), fmt(tolerance), fmt(boundary.translation[2]), fmt(tolerance)));
    geo.push(format!("      periodic_match_count_{tag} += 1; periodic_match_tag_{tag} = {upper_variable}(periodic_slave_{tag});"));
    geo.push("    EndIf".to_string());
    geo.push("  EndFor".to_string());
    geo.push(format!("  If (periodic_match_count_{tag} != 1) Error(\"Periodic pair {tag} does not have a unique translated geometric match.\"); EndIf"));
    geo.push(format!("  Periodic {entity} {{periodic_match_tag_{tag}}} = {{{lower_variable}(periodic_master_{tag})}} Translate {{{delta}}}; // {}", comment(&boundary.name)));
    geo.push("EndFor".to_string());
}
