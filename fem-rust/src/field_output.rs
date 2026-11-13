use crate::C;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// A schema-driven mapping from a declared field symbol to either a scalar
/// solver family or one component of a vector solver family. Vector inputs to
/// the projection helpers always use model coordinate order: x, y, z.
#[derive(Clone, Debug)]
pub(crate) struct FieldProjection {
    symbol: String,
    family: String,
    axis: Option<usize>,
}

pub(crate) fn field_projection(
    declarations: &[crate::model_schema::FieldVariable],
) -> Result<Vec<FieldProjection>, String> {
    declarations
        .iter()
        .map(|declaration| {
            let family = declaration.solver_family.clone().ok_or_else(|| {
                format!(
                    "Scenario field {} has no solver component family.",
                    declaration.symbol
                )
            })?;
            let axis = declaration
                .axis
                .as_deref()
                .map(|axis| match axis {
                    "x" => Ok(0),
                    "y" => Ok(1),
                    "z" => Ok(2),
                    other => Err(format!(
                        "Scenario field {} has unsupported model axis {other:?}.",
                        declaration.symbol
                    )),
                })
                .transpose()?;
            Ok(FieldProjection {
                symbol: declaration.symbol.clone(),
                family,
                axis,
            })
        })
        .collect()
}

/// Apply schema field declarations to nodal scalar/vector solver outputs.
pub(crate) fn project_nodal_fields(
    projection: &[FieldProjection],
    scalar_families: &HashMap<&str, &[C]>,
    vector_families: &HashMap<&str, [&[C]; 3]>,
) -> Result<HashMap<String, Vec<C>>, String> {
    let mut output = HashMap::new();
    for field in projection {
        let values = if let Some(axis) = field.axis {
            let components = vector_families.get(field.family.as_str()).ok_or_else(|| {
                format!(
                    "Solver did not provide vector field family {:?} for {}.",
                    field.family, field.symbol
                )
            })?;
            components[axis].to_vec()
        } else {
            scalar_families
                .get(field.family.as_str())
                .copied()
                .ok_or_else(|| {
                    format!(
                        "Solver did not provide scalar field family {:?} for {}.",
                        field.family, field.symbol
                    )
                })?
                .to_vec()
        };
        output.insert(field.symbol.clone(), values);
    }
    Ok(output)
}

/// Apply the same schema field declarations to a sampled solver vector.
pub(crate) fn project_point_fields(
    projection: &[FieldProjection],
    scalar_families: &HashMap<&str, C>,
    vector_families: &HashMap<&str, [C; 3]>,
) -> Result<HashMap<String, C>, String> {
    let mut output = HashMap::new();
    for field in projection {
        let value = if let Some(axis) = field.axis {
            vector_families.get(field.family.as_str()).ok_or_else(|| {
                format!(
                    "Solver did not provide vector field family {:?} for {}.",
                    field.family, field.symbol
                )
            })?[axis]
        } else {
            *scalar_families.get(field.family.as_str()).ok_or_else(|| {
                format!(
                    "Solver did not provide scalar field family {:?} for {}.",
                    field.family, field.symbol
                )
            })?
        };
        output.insert(field.symbol.clone(), value);
    }
    Ok(output)
}

/// Serialize solver-produced values using the field declarations selected by
/// the model's scenario profile. The model schema owns output symbols,
/// units, and whether a field is primary, derived, or identically zero.
pub(crate) fn declared_component_fields(
    model: &Value,
    values: &HashMap<String, Vec<C>>,
) -> Result<Map<String, Value>, String> {
    let declarations = crate::model_schema::field_variables_for_model(model, false)?;
    let dimension = model
        .pointer("/scenario/dimension")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    declared_component_fields_for(&declarations, dimension as usize, values)
}

pub(crate) fn declared_component_fields_for(
    declarations: &[crate::model_schema::FieldVariable],
    dimension: usize,
    values: &HashMap<String, Vec<C>>,
) -> Result<Map<String, Value>, String> {
    if declarations.is_empty() {
        return Err(
            "The scenario profile has no field-variable declarations for the selected formulation."
                .into(),
        );
    }
    let mut output = Map::new();
    for declaration in declarations {
        let field_values = values.get(&declaration.symbol).ok_or_else(|| {
            format!(
                "The solver did not produce declared field {}.",
                declaration.symbol
            )
        })?;
        let source = if declaration.identically_zero {
            format!("identicallyZeroIn{dimension}D")
        } else if declaration.role == "primary" {
            "solved".to_string()
        } else {
            "derived".to_string()
        };
        output.insert(
            declaration.symbol.clone(),
            component_field(field_values, &declaration.unit, &source),
        );
    }
    Ok(output)
}

/// Gradient of a linear complex-valued scalar field on one cell.
pub(crate) fn p1_complex_gradient(
    nodes: &[usize],
    shape_gradients: &[[f64; 3]],
    real: &[f64],
    imaginary: &[f64],
) -> [C; 3] {
    std::array::from_fn(|axis| {
        let mut value = C::default();
        for (&node, gradient) in nodes.iter().zip(shape_gradients) {
            value.re += gradient[axis] * real[node];
            value.im += gradient[axis] * imaginary[node];
        }
        value
    })
}

pub(crate) fn acoustic_particle_velocity(
    pressure_gradient: [C; 3],
    omega: f64,
    density: f64,
) -> [C; 3] {
    let scale = 1.0 / (omega * density);
    std::array::from_fn(|axis| C {
        re: -pressure_gradient[axis].im * scale,
        im: pressure_gradient[axis].re * scale,
    })
}

/// Accumulate a constant cell vector at its incident nodes for nodal recovery.
pub(crate) fn accumulate_cell_vector(
    sums: &mut [[C; 3]],
    counts: &mut [usize],
    nodes: &[usize],
    value: [C; 3],
) -> Result<(), String> {
    for &node in nodes {
        let Some(sum) = sums.get_mut(node) else {
            return Err(format!(
                "Derived field cell references missing node {node}."
            ));
        };
        let Some(count) = counts.get_mut(node) else {
            return Err(format!(
                "Derived field cell references missing node {node}."
            ));
        };
        for axis in 0..3 {
            sum[axis].re += value[axis].re;
            sum[axis].im += value[axis].im;
        }
        *count += 1;
    }
    Ok(())
}

/// Match EM3D's arithmetic mean of incident-cell values at each mesh node.
pub(crate) fn average_nodal_vectors(sums: &mut [[C; 3]], counts: &[usize]) {
    for (node, count) in counts.iter().copied().enumerate() {
        if count == 0 {
            continue;
        }
        for axis in 0..3 {
            sums[node][axis].re /= count as f64;
            sums[node][axis].im /= count as f64;
        }
    }
}

pub(crate) fn component_field(values: &[C], unit: &str, source: &str) -> Value {
    json!({
        "real": values.iter().map(|value| value.re).collect::<Vec<_>>(),
        "imag": values.iter().map(|value| value.im).collect::<Vec<_>>(),
        "unit": unit,
        "source": source
    })
}
