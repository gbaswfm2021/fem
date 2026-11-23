use crate::model_expression::{self, Scalar};
use crate::solver_common::{PhaseUnionFind, c_abs, c_mul, floquet_factor};
use crate::{C, c_add};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub(super) fn resolve_from(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

pub(super) fn quantity(
    value: &Value,
    symbols: &HashMap<String, Scalar>,
    path: &str,
) -> Result<f64, String> {
    model_expression::evaluate_quantity_with_symbols(value, symbols, path)
}

pub(super) fn symbols_at_frequency(
    parameters: &HashMap<String, Scalar>,
    frequency_hz: f64,
) -> HashMap<String, Scalar> {
    let mut symbols = parameters.clone();
    let frequency = Scalar {
        value: frequency_hz,
        dimension: [0.0, 0.0, -1.0, 0.0, 0.0],
    };
    symbols.insert("freq".into(), frequency);
    symbols.insert(
        "omega".into(),
        Scalar {
            value: std::f64::consts::TAU * frequency_hz,
            dimension: frequency.dimension,
        },
    );
    symbols
}

pub(super) fn symbols_at_point(
    parameters: &HashMap<String, Scalar>,
    frequency_hz: f64,
    point: [f64; 3],
) -> HashMap<String, Scalar> {
    let mut symbols = symbols_at_frequency(parameters, frequency_hz);
    for (axis, value) in ["x", "y", "z"].into_iter().zip(point) {
        symbols.insert(
            axis.to_string(),
            Scalar {
                value,
                dimension: [1.0, 0.0, 0.0, 0.0, 0.0],
            },
        );
    }
    symbols
}

pub(super) fn selection_tags(
    map: &Value,
    property: &str,
) -> Result<HashMap<String, usize>, String> {
    let rows = map
        .get(property)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("model map {property} must be an array."))?;
    let mut output = HashMap::new();
    for row in rows {
        let id = row
            .get("selectionId")
            .or_else(|| row.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| format!("model map {property} contains an item without an ID."))?;
        let tag = row
            .get("physicalTag")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("model map {property} item {id:?} has no physicalTag."))?
            as usize;
        if tag == 0 || output.insert(id.to_string(), tag).is_some() {
            return Err(format!(
                "model map {property} has zero or duplicate tag {tag}."
            ));
        }
    }
    Ok(output)
}

pub(super) fn add_entry(matrix: &mut [BTreeMap<usize, C>], row: usize, column: usize, value: C) {
    let entry = matrix[row].entry(column).or_default();
    *entry = c_add(*entry, value);
}

#[derive(Clone, Debug)]
pub(super) struct PeriodicDofMap {
    pub(super) node_to_dof: Vec<usize>,
    pub(super) node_weights: Vec<C>,
    pub(super) dof_count: usize,
    pub(super) boundary_tags: HashSet<usize>,
    pub(super) boundary_edges: HashSet<[usize; 2]>,
    pub(super) pairs: Vec<PeriodicPairNodes>,
}

#[derive(Clone, Debug)]
pub(super) struct PeriodicPairNodes {
    pub(super) lower_tag: usize,
    pub(super) upper_tag: usize,
    pub(super) node_map: HashMap<usize, usize>,
}

fn edge_key(mut edge: [usize; 2]) -> [usize; 2] {
    edge.sort_unstable();
    edge
}

fn coordinate_bin(value: f64, tolerance: f64) -> Result<i64, String> {
    let scaled = (value / tolerance).floor();
    if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
        return Err("Periodic node coordinates exceed the supported matching range.".into());
    }
    Ok(scaled as i64)
}

/// Verify that matched nodes also induce a bijection of triangulated facets.
/// Matching node sets alone admits incompatible diagonals on paired surfaces.
fn verify_periodic_faces(
    entities: &[(Vec<usize>, usize)],
    lower: usize,
    upper: usize,
    nodes: &HashMap<usize, usize>,
) -> Result<(), String> {
    let mut expected = HashSet::new();
    let mut actual = HashSet::new();
    for (face, tag) in entities {
        if *tag != lower && *tag != upper {
            continue;
        }
        if face.len() != 3 {
            return Err("A periodic surface must contain triangular faces.".into());
        }
        let mut vertices = if *tag == lower {
            face.iter()
                .map(|node| {
                    nodes
                        .get(node)
                        .copied()
                        .ok_or("Periodic face has an unmatched node.")
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            face.clone()
        };
        vertices.sort_unstable();
        let target = if *tag == lower {
            &mut expected
        } else {
            &mut actual
        };
        if !target.insert(vertices) {
            return Err("Periodic surface contains duplicate faces.".into());
        }
    }
    if expected != actual {
        return Err(
            "Periodic surface triangulations do not match under the node correspondence.".into(),
        );
    }
    Ok(())
}

/// Build scalar trace DOF equivalences for a matched 2D edge or 3D face pair.
/// Gmsh matches the geometric entities, while the exported mesh-map keeps the
/// selected boundary tags and translation; node correspondence is reconstructed
/// from coordinates instead of relying on Gmsh's internal node numbering.
pub(super) fn periodic_dof_map_for_entities(
    model: &Value,
    map: &Value,
    nodes: &[[f64; 3]],
    boundary_entities: &[(Vec<usize>, usize)],
    dimension: usize,
) -> Result<PeriodicDofMap, String> {
    periodic_dof_map_for_entities_with_symbols(
        model,
        map,
        nodes,
        boundary_entities,
        dimension,
        None,
        false,
    )
}

/// An optional resolved case scope lets transient sweeps evaluate each
/// geometry and periodic phase using that case's actual parameters.
pub(super) fn periodic_dof_map_for_entities_with_symbols(
    model: &Value,
    map: &Value,
    nodes: &[[f64; 3]],
    boundary_entities: &[(Vec<usize>, usize)],
    dimension: usize,
    case_symbols: Option<&HashMap<String, crate::model_expression::Scalar>>,
    require_zero_phase: bool,
) -> Result<PeriodicDofMap, String> {
    if !matches!(dimension, 2 | 3) {
        return Err(format!(
            "Scalar periodic mapping does not support dimension {dimension}."
        ));
    }
    let node_count = nodes.len();
    let mut classes = PhaseUnionFind::new(node_count);
    let mut boundary_tags = HashSet::<usize>::new();
    let mut boundary_edges = HashSet::<[usize; 2]>::new();
    let mut pairs = Vec::new();
    if boundary_entities
        .iter()
        .any(|(vertices, _)| vertices.iter().any(|&node| node >= node_count))
    {
        return Err("Periodic boundary references an out-of-range node.".into());
    }
    let component = model
        .pointer("/model")
        .ok_or("The periodic model is missing.")?;
    let mesh_settings = component
        .get("mesh")
        .ok_or("model.mesh is required for periodic solving.")?;
    let enabled = mesh_settings
        .get("periodicPairIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect::<HashSet<_>>();
    let physics = crate::model_schema::physics(component)
        .ok_or("model.physics is required for periodic solving.")?;
    let periodic_features = crate::model_schema::physics_periodic_conditions(physics)
        .filter(|feature| feature.get("enabled").and_then(Value::as_bool) != Some(false))
        .collect::<Vec<_>>();
    if !periodic_features.is_empty() {
        let pair_rows = map
            .get("periodicPairs")
            .and_then(Value::as_array)
            .ok_or("model map periodicPairs must be an array.")?;
        let boundary_rows = map
            .get("boundarySelections")
            .and_then(Value::as_array)
            .ok_or("model map boundarySelections must be an array.")?;
        let mut selection_tags = HashMap::<String, usize>::new();
        for row in boundary_rows {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .ok_or("A mapped boundary selection has no ID.")?;
            let tag = row
                .get("physicalTag")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("Mapped boundary selection {id:?} has no physicalTag."))?
                as usize;
            if tag == 0 || selection_tags.insert(id.to_string(), tag).is_some() {
                return Err(format!(
                    "Mapped boundary selection {id:?} has a zero or duplicate physicalTag."
                ));
            }
        }
        let parameters = model
            .pointer("/model/parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let resolved;
        let symbols = if let Some(symbols) = case_symbols {
            symbols
        } else {
            resolved = model_expression::resolve_parameters(&parameters, model)?;
            &resolved
        };
        let map_tolerance = map
            .get("toleranceM")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(1e-8);
        let tolerance = map_tolerance.max(1e-10);
        let mut handled = HashSet::<String>::new();
        for feature in periodic_features {
            let pair_id = feature
                .get("periodicPairId")
                .and_then(Value::as_str)
                .ok_or("A periodic condition must reference a mesh periodic pair.")?;
            if !enabled.contains(pair_id) {
                return Err(format!(
                    "Periodic condition references pair {pair_id:?}, but that pair is not enabled under MESH."
                ));
            }
            if !handled.insert(pair_id.to_string()) {
                return Err(format!(
                    "Periodic pair ID {pair_id:?} is referenced more than once."
                ));
            }
            let pair = pair_rows
                .iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(pair_id))
                .ok_or_else(|| {
                    format!("Enabled periodic pair {pair_id:?} is missing from the mesh map.")
                })?;
            if pair.get("dimension").and_then(Value::as_u64) != Some(dimension as u64) {
                return Err(format!(
                    "Periodic pair {pair_id:?} is not a {dimension}D mesh-map pair."
                ));
            }
            let phase = feature
                .get("phaseVector")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| vec![json!({"expression":"0","unit":"rad/m"}); 3]);
            if phase.len() != 3 {
                return Err(format!(
                    "Periodic condition for pair {pair_id:?} must provide three Floquet phase components."
                ));
            }
            let mut wave_vector = [0.0; 3];
            for (index, value) in phase.iter().enumerate() {
                wave_vector[index] = quantity(
                    value,
                    &symbols,
                    &format!("periodic condition {pair_id}.phaseVector[{index}]"),
                )?;
            }
            if require_zero_phase && wave_vector.iter().any(|value| *value != 0.) {
                return Err(format!(
                    "Transient periodic pair {pair_id:?} requires a zero phase vector."
                ));
            }
            let lower_id = pair
                .get("lowerSelectionId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Periodic pair {pair_id:?} has no lowerSelectionId."))?;
            let upper_id = pair
                .get("upperSelectionId")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Periodic pair {pair_id:?} has no upperSelectionId."))?;
            let lower_tag = *selection_tags.get(lower_id).ok_or_else(|| format!("Periodic pair {pair_id:?} lower selection {lower_id:?} is missing from the mesh map."))?;
            let upper_tag = *selection_tags.get(upper_id).ok_or_else(|| format!("Periodic pair {pair_id:?} upper selection {upper_id:?} is missing from the mesh map."))?;
            if lower_tag == upper_tag {
                return Err(format!(
                    "Periodic pair {pair_id:?} maps both sides to the same boundary tag."
                ));
            }
            let translation = pair
                .get("translation")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    format!("Periodic pair {pair_id:?} translation must have three quantities.")
                })?;
            if translation.len() != 3 {
                return Err(format!(
                    "Periodic pair {pair_id:?} translation must have three quantities."
                ));
            }
            let delta = translation
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    quantity(
                        value,
                        &symbols,
                        &format!("periodic pair {pair_id}.translation[{index}]"),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let factor = floquet_factor(wave_vector, [delta[0], delta[1], delta[2]])?;
            if delta
                .iter()
                .skip(dimension)
                .any(|value| value.abs() > tolerance)
            {
                return Err(format!(
                    "Periodic pair {pair_id:?} translates outside the {dimension}D model space."
                ));
            }

            let mut lower_nodes = HashSet::<usize>::new();
            let mut upper_nodes = HashSet::<usize>::new();
            for (entity, tag) in boundary_entities {
                if *tag == lower_tag {
                    lower_nodes.extend(entity);
                    if dimension == 2 && entity.len() == 2 {
                        boundary_edges.insert(edge_key([entity[0], entity[1]]));
                    }
                }
                if *tag == upper_tag {
                    upper_nodes.extend(entity);
                    if dimension == 2 && entity.len() == 2 {
                        boundary_edges.insert(edge_key([entity[0], entity[1]]));
                    }
                }
            }
            if lower_nodes.is_empty() || upper_nodes.is_empty() {
                return Err(format!(
                    "Periodic pair {pair_id:?} has no boundary entities on one or both selected sides."
                ));
            }
            if lower_nodes.len() != upper_nodes.len() {
                return Err(format!(
                    "Periodic pair {pair_id:?} has {} lower nodes and {} upper nodes in the mesh; Gmsh periodic matching was not preserved.",
                    lower_nodes.len(),
                    upper_nodes.len()
                ));
            }
            let mut bins = HashMap::<[i64; 3], Vec<usize>>::new();
            for &node in &upper_nodes {
                let point = nodes[node];
                let mut key = [0i64; 3];
                for axis in 0..dimension {
                    key[axis] = coordinate_bin(point[axis], tolerance)?;
                }
                bins.entry(key).or_default().push(node);
            }
            let mut matched_upper = HashSet::<usize>::new();
            let mut matched_nodes = HashMap::new();
            for &lower_node in &lower_nodes {
                let point = nodes[lower_node];
                let target = [
                    point[0] + delta[0],
                    point[1] + delta[1],
                    point[2] + delta[2],
                ];
                let mut bin = [0i64; 3];
                for axis in 0..dimension {
                    bin[axis] = coordinate_bin(target[axis], tolerance)?;
                }
                let mut candidates = Vec::<usize>::new();
                let oz: std::ops::RangeInclusive<i64> = if dimension == 3 { -1..=1 } else { 0..=0 };
                for ox in -1i64..=1 {
                    for oy in -1i64..=1 {
                        for oz in oz.clone() {
                            let key = [
                                bin[0] + ox,
                                bin[1] + oy,
                                if dimension == 3 { bin[2] + oz } else { 0 },
                            ];
                            if let Some(found) = bins.get(&key) {
                                candidates.extend(found.iter().copied().filter(|&upper_node| {
                                    (0..dimension)
                                        .map(|axis| {
                                            (nodes[upper_node][axis] - target[axis]).powi(2)
                                        })
                                        .sum::<f64>()
                                        .sqrt()
                                        <= tolerance
                                }));
                            }
                        }
                    }
                }
                if candidates.len() != 1 {
                    return Err(format!(
                        "Periodic pair {pair_id:?} node {lower_node} has {} upper-side matches within {tolerance:.3e} m; expected exactly one.",
                        candidates.len()
                    ));
                }
                let upper_node = candidates[0];
                if !matched_upper.insert(upper_node) {
                    return Err(format!(
                        "Periodic pair {pair_id:?} maps more than one lower node to upper node {upper_node}."
                    ));
                }
                matched_nodes.insert(lower_node, upper_node);
                classes
                    .constrain(lower_node, upper_node, factor)
                    .map_err(|error| format!("Periodic pair {pair_id:?}: {error}"))?;
            }
            if matched_upper.len() != upper_nodes.len() {
                return Err(format!(
                    "Periodic pair {pair_id:?} does not match every upper-side node."
                ));
            }
            if dimension == 3 {
                verify_periodic_faces(boundary_entities, lower_tag, upper_tag, &matched_nodes)?;
            }
            pairs.push(PeriodicPairNodes {
                lower_tag,
                upper_tag,
                node_map: matched_nodes,
            });
            boundary_tags.insert(lower_tag);
            boundary_tags.insert(upper_tag);
        }
    }
    let (node_to_dof, node_weights, dof_count) = classes.mapping();
    Ok(PeriodicDofMap {
        node_to_dof,
        node_weights,
        dof_count,
        boundary_tags,
        boundary_edges,
        pairs,
    })
}

pub(super) fn periodic_dof_map(
    model: &Value,
    map: &Value,
    mesh: &crate::mesh2d::Mesh2D,
) -> Result<PeriodicDofMap, String> {
    let entities = mesh
        .boundary_edges
        .iter()
        .map(|(edge, tag)| (edge.to_vec(), *tag))
        .collect::<Vec<_>>();
    periodic_dof_map_for_entities(model, map, &mesh.nodes, &entities, 2)
}

pub(super) fn reduce_system(
    matrix: &[BTreeMap<usize, C>],
    rhs_re: &[f64],
    rhs_im: &[f64],
    dofs: &PeriodicDofMap,
) -> Result<(Vec<BTreeMap<usize, C>>, Vec<f64>, Vec<f64>), String> {
    if matrix.len() != dofs.node_to_dof.len()
        || dofs.node_weights.len() != matrix.len()
        || rhs_re.len() != matrix.len()
        || rhs_im.len() != matrix.len()
    {
        return Err("Periodic DOF mapping does not match the assembled 2D system size.".into());
    }
    crate::constraints::reduce_weighted(
        matrix,
        rhs_re,
        rhs_im,
        &dofs.node_to_dof,
        &dofs.node_weights,
        dofs.dof_count,
    )
}

pub(super) fn apply_dirichlet(
    matrix: &mut [BTreeMap<usize, C>],
    rhs_re: &mut [f64],
    rhs_im: &mut [f64],
    constraints: &HashMap<usize, f64>,
) -> Result<(), String> {
    let complex = constraints
        .iter()
        .map(|(&node, &value)| (node, C { re: value, im: 0.0 }))
        .collect();
    apply_complex_dirichlet(matrix, rhs_re, rhs_im, &complex)
}

/// Normalize a prescribed nodal value by its periodic phase before comparing it.
pub(super) fn insert_dirichlet_constraint(
    constraints: &mut HashMap<usize, C>,
    dofs: &PeriodicDofMap,
    node: usize,
    value: C,
) -> Result<(), String> {
    let dof = *dofs
        .node_to_dof
        .get(node)
        .ok_or("Dirichlet constraint references a missing periodic node.")?;
    let weight = *dofs
        .node_weights
        .get(node)
        .ok_or("Dirichlet constraint references a missing periodic phase.")?;
    if !value.re.is_finite()
        || !value.im.is_finite()
        || !weight.re.is_finite()
        || !weight.im.is_finite()
        || c_abs(weight) < 1e-150
    {
        return Err(
            "A scalar Dirichlet constraint has a non-finite value or periodic phase.".into(),
        );
    }
    let prescribed = value.div(weight);
    if let Some(previous) = constraints.get(&dof) {
        let difference = (previous.re - prescribed.re).hypot(previous.im - prescribed.im);
        if difference > 1e-9 * c_abs(*previous).max(c_abs(prescribed)).max(1.0) {
            return Err(format!(
                "Conflicting Dirichlet values meet at periodic mesh degree of freedom {dof} after Floquet phase normalization."
            ));
        }
    } else {
        constraints.insert(dof, prescribed);
    }
    Ok(())
}

pub(super) fn apply_complex_dirichlet(
    matrix: &mut [BTreeMap<usize, C>],
    rhs_re: &mut [f64],
    rhs_im: &mut [f64],
    constraints: &HashMap<usize, C>,
) -> Result<(), String> {
    crate::constraints::apply_essential(matrix, rhs_re, rhs_im, constraints)
}

pub(super) fn expand_solution(values: &[f64], dofs: &PeriodicDofMap) -> Result<Vec<f64>, String> {
    let (real, imaginary) = expand_complex_solution(values, &vec![0.0; values.len()], dofs)?;
    if imaginary.iter().any(|value| value.abs() > 1e-12) {
        return Err("A nonzero Floquet phase requires a complex solution vector.".into());
    }
    Ok(real)
}

pub(super) fn expand_complex_solution(
    real: &[f64],
    imaginary: &[f64],
    dofs: &PeriodicDofMap,
) -> Result<(Vec<f64>, Vec<f64>), String> {
    if real.len() != dofs.dof_count
        || imaginary.len() != dofs.dof_count
        || dofs.node_to_dof.len() != dofs.node_weights.len()
    {
        return Err("Solved vector does not match the periodic DOF count.".into());
    }
    crate::constraints::expand_weighted(
        real,
        imaginary,
        &dofs.node_to_dof,
        &dofs.node_weights,
        dofs.dof_count,
    )
}

#[cfg(test)]
mod floquet_tests {
    use super::*;

    #[test]
    fn periodic_surface_matching_checks_face_connectivity() {
        let map = HashMap::from([(0, 4), (1, 5), (2, 6), (3, 7)]);
        let valid = vec![
            (vec![0, 1, 2], 1),
            (vec![0, 2, 3], 1),
            (vec![6, 5, 4], 2),
            (vec![7, 6, 4], 2),
        ];
        assert!(verify_periodic_faces(&valid, 1, 2, &map).is_ok());
        let different_diagonal = vec![
            (vec![0, 1, 2], 1),
            (vec![0, 2, 3], 1),
            (vec![4, 5, 7], 2),
            (vec![5, 6, 7], 2),
        ];
        assert!(verify_periodic_faces(&different_diagonal, 1, 2, &map).is_err());
        let mut duplicate = valid.clone();
        duplicate.push((vec![4, 5, 6], 2));
        assert!(verify_periodic_faces(&duplicate, 1, 2, &map).is_err());
    }

    fn dofs() -> PeriodicDofMap {
        PeriodicDofMap {
            node_to_dof: vec![0, 0, 1],
            node_weights: vec![
                C { re: 1.0, im: 0.0 },
                C { re: 0.0, im: -1.0 },
                C { re: 1.0, im: 0.0 },
            ],
            dof_count: 2,
            boundary_tags: HashSet::new(),
            boundary_edges: HashSet::new(),
            pairs: Vec::new(),
        }
    }

    fn close(first: C, second: C) {
        assert!(
            (first.re - second.re).hypot(first.im - second.im) < 1e-12,
            "{first:?} != {second:?}"
        );
    }

    #[test]
    fn floquet_reduction_uses_conjugate_test_weights() {
        let mut matrix = vec![BTreeMap::new(); 3];
        for (row, column, value) in [
            (0, 0, 2.0),
            (0, 1, -1.0),
            (1, 0, -1.0),
            (1, 1, 3.0),
            (0, 2, 0.5),
            (2, 0, 0.5),
            (2, 2, 4.0),
        ] {
            add_entry(&mut matrix, row, column, C { re: value, im: 0.0 });
        }
        let (reduced, real, imaginary) =
            reduce_system(&matrix, &[1.0, 3.0, 5.0], &[2.0, 4.0, 6.0], &dofs()).unwrap();
        close(reduced[0][&0], C { re: 5.0, im: 0.0 });
        close(reduced[0][&1], C { re: 0.5, im: 0.0 });
        close(reduced[1][&0], C { re: 0.5, im: 0.0 });
        close(
            C {
                re: real[0],
                im: imaginary[0],
            },
            C { re: -3.0, im: 5.0 },
        );
        close(
            C {
                re: real[1],
                im: imaginary[1],
            },
            C { re: 5.0, im: 6.0 },
        );
    }

    #[test]
    fn floquet_dirichlet_constraints_are_normalized_and_complex() {
        let map = dofs();
        let mut constraints = HashMap::new();
        insert_dirichlet_constraint(&mut constraints, &map, 0, C { re: 2.0, im: 3.0 }).unwrap();
        insert_dirichlet_constraint(&mut constraints, &map, 1, C { re: 3.0, im: -2.0 }).unwrap();
        assert!(
            insert_dirichlet_constraint(&mut constraints, &map, 1, C { re: 2.0, im: 3.0 }).is_err()
        );
        let mut matrix = vec![BTreeMap::new(); 2];
        add_entry(&mut matrix, 0, 0, C { re: 2.0, im: 0.0 });
        add_entry(&mut matrix, 0, 1, C { re: 1.0, im: 0.5 });
        add_entry(&mut matrix, 1, 0, C { re: 1.0, im: 0.5 });
        add_entry(&mut matrix, 1, 1, C { re: 4.0, im: 0.0 });
        let mut real = vec![7.0, 11.0];
        let mut imaginary = vec![8.0, 12.0];
        apply_complex_dirichlet(&mut matrix, &mut real, &mut imaginary, &constraints).unwrap();
        close(
            C {
                re: real[0],
                im: imaginary[0],
            },
            C { re: 2.0, im: 3.0 },
        );
        close(
            C {
                re: real[1],
                im: imaginary[1],
            },
            C { re: 10.5, im: 8.0 },
        );
        assert!(!matrix[1].contains_key(&0));
        close(matrix[0][&0], C { re: 1.0, im: 0.0 });
    }

    #[test]
    fn floquet_expansion_restores_the_physical_phase() {
        let (real, imaginary) = expand_complex_solution(&[2.0, 4.0], &[3.0, 5.0], &dofs()).unwrap();
        close(
            C {
                re: real[0],
                im: imaginary[0],
            },
            C { re: 2.0, im: 3.0 },
        );
        close(
            C {
                re: real[1],
                im: imaginary[1],
            },
            C { re: 3.0, im: -2.0 },
        );
        close(
            C {
                re: real[2],
                im: imaginary[2],
            },
            C { re: 4.0, im: 5.0 },
        );
    }
}
