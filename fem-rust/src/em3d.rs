use crate::solver_common::{PhaseUnionFind, c_conj, c_mul, floquet_factor};
use crate::{
    C, MU0, SolverCache, TET_Q, TRI_Q, c_add, c_scale, field_output, integration, material,
    model_expression, solve_linear_system, solver_request,
};
use fullmeta_core::quantity::{Dimension, ZERO_DIMENSION, dimension_from_json};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const EDGES: [[usize; 2]; 6] = [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]];
const FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
const N2_MONOMIALS: [[usize; 3]; 10] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
    [2, 0, 0],
    [0, 2, 0],
    [0, 0, 2],
    [1, 1, 0],
    [1, 0, 1],
    [0, 1, 1],
];
type V3 = [f64; 3];
const INTERNAL_COORDINATE_ORDER: [&str; 3] = ["y", "z", "x"];
#[derive(Clone, Copy)]
struct MaterialProperties {
    relative_permittivity: f64,
    relative_permeability: f64,
    conductivity_s_per_m: f64,
}
fn material_for(
    materials: &HashMap<usize, MaterialProperties>,
    element: usize,
) -> Result<MaterialProperties, String> {
    materials
        .get(&element)
        .copied()
        .ok_or_else(|| format!("Tetrahedron {element} has no evaluated material coefficients."))
}
fn evaluate_element_materials(
    mesh: &Mesh,
    config: &SolveConfig,
    frequency_case_index: usize,
    frequency_hz: f64,
    requires_nonzero_mu: bool,
) -> Result<HashMap<usize, MaterialProperties>, String> {
    let parameter_values = config
        .global_parameter_cases
        .get(frequency_case_index)
        .ok_or_else(|| {
            format!(
                "No saved model parameter values for frequency row {}.",
                frequency_case_index + 1
            )
        })?;
    let dimensions = config.global_parameter_dimensions.get(frequency_case_index);
    let parameters = parameter_values
        .iter()
        .map(|(symbol, value)| {
            let dimension = dimensions
                .and_then(|row| row.get(symbol))
                .copied()
                .unwrap_or(ZERO_DIMENSION);
            (
                symbol.clone(),
                model_expression::Scalar {
                    value: *value,
                    dimension,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut evaluators = HashMap::<usize, (String, material::CompiledMaterialEvaluator)>::new();
    for (index, assignment) in config.material_assignments.iter().enumerate() {
        let tag=assignment.get("physicalTag").and_then(Value::as_u64).filter(|tag|*tag>0).ok_or_else(||format!("modelConfiguration.materialAssignments[{index}].physicalTag must be a positive integer."))? as usize;
        let material = assignment.get("material").unwrap_or(assignment);
        let material_id = material
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("assignment")
            .to_string();
        let evaluator = material::CompiledMaterialEvaluator::compile(
            &config.scenario_profile_id,
            material,
            &format!("material {material_id}"),
        )?;
        if evaluators.insert(tag, (material_id, evaluator)).is_some() {
            return Err(format!(
                "Duplicate material assignment for physical tag {tag}."
            ));
        }
    }
    let mut constant_materials = HashMap::<usize, MaterialProperties>::new();
    let mut output = HashMap::with_capacity(mesh.tets.len());
    for (element, tet) in mesh.tets.iter().enumerate() {
        let tag = *mesh
            .entities
            .get(element)
            .ok_or_else(|| format!("Mesh tetrahedron {element} has no physical tag."))?;
        if let Some(&properties) = constant_materials.get(&tag) {
            output.insert(element, properties);
            continue;
        }
        let (material_id,evaluator)=evaluators.get(&tag).ok_or_else(||format!("Mesh tetrahedron {element} uses physical material tag {tag}, which has no material assignment."))?;
        let internal = points_for(*tet, &mesh.nodes);
        let center: V3 = std::array::from_fn(|axis| {
            (internal[0][axis] + internal[1][axis] + internal[2][axis] + internal[3][axis]) * 0.25
        });
        let point = [
            center[2] + mesh.origin[2],
            center[0] + mesh.origin[0],
            center[1] + mesh.origin[1],
        ];
        let path = format!("material {material_id} in tetrahedron {element}");
        let values = evaluator.evaluate(&parameters, frequency_hz, point, &path)?;
        let value = |symbol: &str| {
            values
                .get(symbol)
                .map(|value| value.value)
                .ok_or_else(|| format!("Scenario material catalog omits {symbol:?}."))
        };
        let relative_permittivity = value("eps_r")?;
        let relative_permeability = value("mu_r")?;
        let conductivity_s_per_m = value("sigma")?;
        if !relative_permittivity.is_finite()
            || !relative_permeability.is_finite()
            || !conductivity_s_per_m.is_finite()
            || requires_nonzero_mu && relative_permeability == 0.0
        {
            return Err(format!(
                "{path} must evaluate to finite electromagnetic coefficients at ({}, {}, {}); the electric formulation additionally requires nonzero permeability.",
                point[0], point[1], point[2]
            ));
        }
        let properties = MaterialProperties {
            relative_permittivity,
            relative_permeability,
            conductivity_s_per_m,
        };
        if !evaluator.uses_position() {
            constant_materials.insert(tag, properties);
        }
        output.insert(element, properties);
    }
    Ok(output)
}
const EPS0: f64 = 8.854_187_812_8e-12;

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn norm(a: V3) -> f64 {
    dot(a, a).sqrt()
}

#[derive(Clone)]
struct Mesh {
    nodes: Vec<V3>,
    tets: Vec<[usize; 4]>,
    entities: Vec<usize>,
    geometry: V3,
    origin: V3,
    boundary: Vec<Face>,
    integration_faces: HashMap<String, Vec<Face>>,
    integration_edges: HashMap<String, Vec<Line>>,
    integration_tets: HashMap<String, Vec<usize>>,
}
#[derive(Clone)]
struct Face {
    nodes: [usize; 3],
    element: usize,
    other_element: Option<usize>,
    tag: usize,
}
#[derive(Clone)]
struct Line {
    nodes: [usize; 2],
    element: usize,
    tag: usize,
}
#[derive(Clone)]
struct PeriodicPair {
    id: String,
    lower_tag: usize,
    upper_tag: usize,
    translation: V3,
    phase_vector: V3,
}
#[derive(Clone)]
struct ImpedanceBoundary {
    id: String,
    tag: usize,
    impedance: Value,
    incident: Option<Vec<model_expression::CompiledComplexQuantity>>,
}
#[derive(Clone)]
struct IntegrationOperator {
    id: String,
    kind: String,
    point_m: Option<V3>,
    physical_tag: usize,
    host_material_tag: Option<usize>,
}
#[derive(Clone)]
struct VariableDefinition {
    symbol: String,
    ast: Value,
}
#[derive(Clone)]
struct PointwiseDefinition {
    symbol: String,
    ast: fullmeta_core::expression::AstNode,
}
#[derive(Clone)]
struct SolveConfig {
    periodic_pairs: Vec<PeriodicPair>,
    impedance_boundaries: Vec<ImpedanceBoundary>,
    global_parameter_cases: Vec<HashMap<String, f64>>,
    global_parameter_dimensions: Vec<HashMap<String, Dimension>>,
    material_assignments: Vec<Value>,
    scenario_profile_id: String,
    variables: Vec<VariableDefinition>,
    pointwise_variables: Vec<PointwiseDefinition>,
    integration_operators: Vec<IntegrationOperator>,
    field_files: bool,
    field_variables: Vec<crate::model_schema::FieldVariable>,
    field_dimension: usize,
}
fn boundary_symbols(
    config: &SolveConfig,
    frequency_case_index: usize,
    frequency_hz: f64,
    point_internal: V3,
) -> Result<HashMap<String, model_expression::Scalar>, String> {
    let parameter_values = config
        .global_parameter_cases
        .get(frequency_case_index)
        .ok_or_else(|| {
            format!(
                "No saved model parameter values for boundary evaluation row {}.",
                frequency_case_index + 1
            )
        })?;
    let dimensions = config.global_parameter_dimensions.get(frequency_case_index);
    let mut symbols = parameter_values
        .iter()
        .map(|(symbol, value)| {
            let dimension = dimensions
                .and_then(|values| values.get(symbol))
                .copied()
                .unwrap_or(ZERO_DIMENSION);
            (
                symbol.clone(),
                model_expression::Scalar {
                    value: *value,
                    dimension,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let length_dimension = [1.0, 0.0, 0.0, 0.0, 0.0];
    for (internal_axis, model_axis) in INTERNAL_COORDINATE_ORDER.iter().enumerate() {
        symbols.insert(
            (*model_axis).to_string(),
            model_expression::Scalar {
                value: point_internal[internal_axis],
                dimension: length_dimension,
            },
        );
    }
    let frequency_dimension = model_expression::unit("Hz")?.1;
    symbols.insert(
        "freq".into(),
        model_expression::Scalar {
            value: frequency_hz,
            dimension: frequency_dimension,
        },
    );
    symbols.insert(
        "frequency".into(),
        model_expression::Scalar {
            value: frequency_hz,
            dimension: frequency_dimension,
        },
    );
    symbols.insert(
        "omega".into(),
        model_expression::Scalar {
            value: std::f64::consts::TAU * frequency_hz,
            dimension: frequency_dimension,
        },
    );
    Ok(symbols)
}
#[derive(Clone, Copy)]
struct Geo {
    p0: V3,
    inv: [[f64; 3]; 3],
    j: [[f64; 3]; 3],
    det: f64,
}
#[derive(Clone, Copy)]
struct Basis {
    value: V3,
    curl: V3,
    a: usize,
    b: usize,
}
struct Local {
    geo: Geo,
    grad: [V3; 4],
    dofs: Vec<usize>,
    order: u8,
    inv_m: Option<[[f64; 20]; 20]>,
}
struct SolveOut {
    output: Value,
    residual: f64,
    unknowns: usize,
}
struct CsvFrequencyRow {
    frequency_ghz: f64,
    residual: f64,
    unknowns: usize,
    variable_values: Vec<C>,
}

struct PeriodicDofs {
    elements: Vec<Vec<usize>>,
    reduced: Vec<usize>,
    weights: Vec<C>,
    count: usize,
}

fn geometry(points: [V3; 4]) -> Result<Geo, String> {
    let p0 = points[0];
    let mut j = [[0.0; 3]; 3];
    for r in 0..3 {
        for c in 0..3 {
            j[r][c] = points[c + 1][r] - p0[r];
        }
    }
    let [a, b, c] = j[0];
    let [d, e, f] = j[1];
    let [g, h, i] = j[2];
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    if !det.is_finite() || det == 0.0 {
        return Err("Tetrahedral mesh contains a degenerate or non-finite element.".into());
    }
    let inv = [
        [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
        ],
        [
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
        ],
        [
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ],
    ];
    if inv.iter().flatten().any(|value| !value.is_finite()) {
        return Err("A tetrahedral coordinate transform exceeds the finite numeric range.".into());
    }
    Ok(Geo { p0, inv, j, det })
}
fn ref_point(g: Geo, p: V3) -> V3 {
    let d = sub(p, g.p0);
    [dot(g.inv[0], d), dot(g.inv[1], d), dot(g.inv[2], d)]
}
fn grad_lambda(g: Geo) -> [V3; 4] {
    let s = add(add(g.inv[0], g.inv[1]), g.inv[2]);
    [scale(s, -1.0), g.inv[0], g.inv[1], g.inv[2]]
}
fn coordinate_cmp(a: V3, b: V3) -> std::cmp::Ordering {
    for axis in 0..3 {
        let qa = (a[axis] * 1.0e12).round();
        let qb = (b[axis] * 1.0e12).round();
        let ord = qa.total_cmp(&qb);
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}
fn first_basis(g: Geo, tet: [usize; 4], nodes: &[V3]) -> [Basis; 6] {
    let grad = grad_lambda(g);
    std::array::from_fn(|k| {
        let mut a = tet[EDGES[k][0]];
        let mut b = tet[EDGES[k][1]];
        if coordinate_cmp(nodes[a], nodes[b]).then_with(|| a.cmp(&b)) != std::cmp::Ordering::Less {
            std::mem::swap(&mut a, &mut b)
        }
        let ia = tet.iter().position(|&x| x == a).unwrap();
        let ib = tet.iter().position(|&x| x == b).unwrap();
        Basis {
            value: [0.0; 3],
            curl: scale(cross(grad[ia], grad[ib]), 2.0),
            a: ia,
            b: ib,
        }
    })
}
fn basis_at(g: Geo, grad: [V3; 4], b0: [f64; 4], basis: &[Basis; 6]) -> [Basis; 6] {
    let _ = g;
    std::array::from_fn(|k| {
        let a = basis[k].a;
        let b = basis[k].b;
        Basis {
            value: sub(scale(grad[b], b0[a]), scale(grad[a], b0[b])),
            curl: basis[k].curl,
            a,
            b,
        }
    })
}
type Poly2 = [[f64; 10]; 3];
fn monomial_index(p: [usize; 3]) -> usize {
    N2_MONOMIALS
        .iter()
        .position(|&q| q == p)
        .expect("quadratic monomial")
}
fn poly_term(p: &mut Poly2, component: usize, powers: [usize; 3], value: f64) {
    let i = monomial_index(powers);
    p[component][i] += value;
}
fn poly_shift_component(
    dst: &mut Poly2,
    target: usize,
    src: &Poly2,
    source: usize,
    axis: usize,
    scale: f64,
) {
    for (i, &powers) in N2_MONOMIALS.iter().enumerate() {
        if src[source][i] == 0.0 {
            continue;
        }
        let mut shifted = powers;
        shifted[axis] += 1;
        let j = monomial_index(shifted);
        dst[target][j] += scale * src[source][i];
    }
}
fn quadratic_candidates() -> Vec<Poly2> {
    let mut out = Vec::with_capacity(20);
    for c in 0..3 {
        for powers in [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]] {
            let mut p = [[0.0; 10]; 3];
            poly_term(&mut p, c, powers, 1.0);
            out.push(p);
        }
    }
    let mut add_cross = |q: Poly2| {
        let mut p = [[0.0; 10]; 3];
        // x cross q = [y*qz-z*qy, z*qx-x*qz, x*qy-y*qx]
        poly_shift_component(&mut p, 0, &q, 2, 1, 1.0);
        poly_shift_component(&mut p, 0, &q, 1, 2, -1.0);
        poly_shift_component(&mut p, 1, &q, 0, 2, 1.0);
        poly_shift_component(&mut p, 1, &q, 2, 0, -1.0);
        poly_shift_component(&mut p, 2, &q, 1, 0, 1.0);
        poly_shift_component(&mut p, 2, &q, 0, 1, -1.0);
        out.push(p);
    };
    for component in 0..3 {
        for axis in 0..3 {
            if component == axis {
                continue;
            }
            let mut q = [[0.0; 10]; 3];
            let mut powers = [0; 3];
            powers[axis] = 1;
            poly_term(&mut q, component, powers, 1.0);
            add_cross(q);
        }
    }
    for (a, b) in [(0, 1), (1, 2)] {
        let mut q = [[0.0; 10]; 3];
        let mut pa = [0; 3];
        let mut pb = [0; 3];
        pa[a] = 1;
        pb[b] = 1;
        poly_term(&mut q, a, pa, 1.0);
        poly_term(&mut q, b, pb, -1.0);
        add_cross(q);
    }
    assert_eq!(out.len(), 20);
    out
}
fn poly_at(poly: &Poly2, x: V3) -> (V3, V3) {
    let mut value = [0.0; 3];
    let mut grad = [[0.0; 3]; 3];
    for c in 0..3 {
        for (i, &powers) in N2_MONOMIALS.iter().enumerate() {
            let coef = poly[c][i];
            if coef == 0.0 {
                continue;
            }
            value[c] += coef
                * x[0].powi(powers[0] as i32)
                * x[1].powi(powers[1] as i32)
                * x[2].powi(powers[2] as i32);
            for axis in 0..3 {
                if powers[axis] == 0 {
                    continue;
                }
                let mut d = coef * powers[axis] as f64;
                for k in 0..3 {
                    d *= x[k].powi((powers[k] - usize::from(k == axis)) as i32);
                }
                grad[c][axis] += d;
            }
        }
    }
    (
        value,
        [
            grad[2][1] - grad[1][2],
            grad[0][2] - grad[2][0],
            grad[1][0] - grad[0][1],
        ],
    )
}
fn candidate_at(g: Geo, p: &[Poly2], point: V3) -> Vec<Basis> {
    let x = ref_point(g, point);
    p.iter()
        .map(|poly| {
            let (v, c) = poly_at(poly, x);
            let value = std::array::from_fn(|i| (0..3).map(|j| g.inv[j][i] * v[j]).sum());
            let curl = std::array::from_fn(|i| (0..3).map(|j| g.j[i][j] * c[j] / g.det).sum());
            Basis {
                value,
                curl,
                a: 0,
                b: 0,
            }
        })
        .collect()
}
fn invert20(a: [[f64; 20]; 20]) -> Result<[[f64; 20]; 20], String> {
    let mut m = [[0.0; 40]; 20];
    for i in 0..20 {
        for j in 0..20 {
            m[i][j] = a[i][j];
        }
        m[i][20 + i] = 1.0;
    }
    for col in 0..20 {
        let mut pivot = col;
        for r in col + 1..20 {
            if m[r][col].abs() > m[pivot][col].abs() {
                pivot = r;
            }
        }
        if m[pivot][col].abs() < 1e-12 {
            return Err("Second-order Nedelec interpolation matrix is singular.".into());
        }
        m.swap(col, pivot);
        let d = m[col][col];
        for j in 0..40 {
            m[col][j] /= d;
        }
        for r in 0..20 {
            if r == col {
                continue;
            }
            let q = m[r][col];
            for j in 0..40 {
                m[r][j] -= q * m[col][j];
            }
        }
    }
    Ok(std::array::from_fn(|i| {
        std::array::from_fn(|j| m[i][20 + j])
    }))
}
fn canonical_points(mut ids: Vec<usize>, nodes: &[V3]) -> Vec<usize> {
    ids.sort_by(|&a, &b| coordinate_cmp(nodes[a], nodes[b]).then_with(|| a.cmp(&b)));
    ids
}
fn entity_key(mut ids: Vec<usize>) -> Vec<usize> {
    ids.sort_unstable();
    ids
}
fn second_interpolation(
    g: Geo,
    tet: [usize; 4],
    nodes: &[V3],
    candidates: &[Poly2],
) -> Result<[[f64; 20]; 20], String> {
    let mut a = [[0.0; 20]; 20];
    let mut row = 0;
    for [u, v] in EDGES {
        let ids = canonical_points(vec![tet[u], tet[v]], nodes);
        let p0 = nodes[ids[0]];
        let d = sub(nodes[ids[1]], p0);
        let tangent = scale(d, 1.0 / norm(d));
        for mode in 0..2 {
            for (s, w) in [(0.211324865405187, 0.5), (0.788675134594813, 0.5)] {
                let bas = candidate_at(g, candidates, add(p0, scale(d, s)));
                let moment = if mode == 0 { 1.0 } else { 2.0 * s - 1.0 };
                for c in 0..20 {
                    a[row][c] += w * dot(bas[c].value, tangent) * moment;
                }
            }
            row += 1;
        }
    }
    for face in FACES {
        let ids = canonical_points(face.iter().map(|&i| tet[i]).collect(), nodes);
        let p = [nodes[ids[0]], nodes[ids[1]], nodes[ids[2]]];
        let tangents = [
            scale(sub(p[1], p[0]), 1.0 / norm(sub(p[1], p[0]))),
            scale(sub(p[2], p[0]), 1.0 / norm(sub(p[2], p[0]))),
        ];
        for tangent in tangents {
            for q in TRI_Q {
                let point = add(add(scale(p[0], q[0]), scale(p[1], q[1])), scale(p[2], q[2]));
                let bas = candidate_at(g, candidates, point);
                for c in 0..20 {
                    a[row][c] += q[3] * dot(bas[c].value, tangent);
                }
            }
            row += 1;
        }
    }
    if row != 20 {
        return Err("Second-order Nedelec local moment count was not 20.".into());
    }
    invert20(a)
}
fn second_basis_at(g: Geo, candidates: &[Poly2], inv_m: &[[f64; 20]; 20], point: V3) -> Vec<Basis> {
    let c = candidate_at(g, candidates, point);
    (0..20)
        .map(|d| {
            let mut value = [0.0; 3];
            let mut curl = [0.0; 3];
            for k in 0..20 {
                for axis in 0..3 {
                    value[axis] += c[k].value[axis] * inv_m[k][d];
                    curl[axis] += c[k].curl[axis] * inv_m[k][d];
                }
            }
            Basis {
                value,
                curl,
                a: 0,
                b: 0,
            }
        })
        .collect()
}
fn bases_at(
    order: u8,
    g: Geo,
    grad: [V3; 4],
    tet: [usize; 4],
    nodes: &[V3],
    inv_m: Option<&[[f64; 20]; 20]>,
    candidates: &[Poly2],
    point: V3,
) -> Vec<Basis> {
    if order == 1 {
        let canonical = first_basis(g, tet, nodes);
        let rp = ref_point(g, point);
        let lambda = [1.0 - rp[0] - rp[1] - rp[2], rp[0], rp[1], rp[2]];
        basis_at(g, grad, lambda, &canonical).to_vec()
    } else {
        second_basis_at(
            g,
            candidates,
            inv_m.expect("second-order interpolation"),
            point,
        )
    }
}
fn points_for(tet: [usize; 4], nodes: &[V3]) -> [V3; 4] {
    std::array::from_fn(|i| nodes[tet[i]])
}
fn face_area(p: [V3; 3]) -> f64 {
    norm(cross(sub(p[1], p[0]), sub(p[2], p[0]))) / 2.0
}

fn parse_mesh(text: &str, integration_operators: &[IntegrationOperator]) -> Result<Mesh, String> {
    let mut physical: Vec<Option<V3>> = Vec::new();
    let mut tets: Vec<Option<[usize; 4]>> = Vec::new();
    let mut entities: Vec<Option<usize>> = Vec::new();
    let mut surface_tags = HashMap::<[usize; 3], usize>::new();
    let mut curve_tags = Vec::<([usize; 2], usize)>::new();
    for line in text.lines() {
        let p: Vec<&str> = line.trim().split(',').collect();
        if p.len() < 5 {
            continue;
        }
        if p[0] == "V" {
            let id = p[1].parse::<usize>().map_err(|e| e.to_string())?;
            if physical.len() <= id {
                physical.resize(id + 1, None)
            }
            physical[id] = Some([
                p[2].parse::<f64>().map_err(|e| e.to_string())?,
                p[3].parse::<f64>().map_err(|e| e.to_string())?,
                p[4].parse::<f64>().map_err(|e| e.to_string())?,
            ]);
        } else if p[0] == "F" && p.len() >= 6 {
            let mut ids = [
                p[2].parse::<usize>().map_err(|e| e.to_string())?,
                p[3].parse::<usize>().map_err(|e| e.to_string())?,
                p[4].parse::<usize>().map_err(|e| e.to_string())?,
            ];
            ids.sort_unstable();
            let tag = p[5].parse::<usize>().map_err(|e| e.to_string())?;
            if tag == 0 {
                return Err("Boundary triangle physical tags must be positive.".into());
            }
            if surface_tags.insert(ids, tag).is_some() {
                return Err("FullMeta mesh contains a duplicate tagged boundary triangle.".into());
            }
        } else if p[0] == "L" && p.len() >= 5 {
            let nodes = [
                p[2].parse::<usize>().map_err(|e| e.to_string())?,
                p[3].parse::<usize>().map_err(|e| e.to_string())?,
            ];
            let tag = p[4].parse::<usize>().map_err(|e| e.to_string())?;
            if tag == 0 {
                return Err("Selected geometry edges must have positive physical tags.".into());
            }
            curve_tags.push((nodes, tag));
        } else if p[0] == "T" && p.len() >= 7 {
            let id = p[1].parse::<usize>().map_err(|e| e.to_string())?;
            if tets.len() <= id {
                tets.resize(id + 1, None);
                entities.resize(id + 1, None)
            }
            tets[id] = Some([
                p[2].parse::<usize>().map_err(|e| e.to_string())?,
                p[3].parse::<usize>().map_err(|e| e.to_string())?,
                p[4].parse::<usize>().map_err(|e| e.to_string())?,
                p[5].parse::<usize>().map_err(|e| e.to_string())?,
            ]);
            entities[id] = Some(p[6].parse::<usize>().map_err(|e| e.to_string())?);
        }
    }
    if physical.is_empty()
        || tets.is_empty()
        || physical.iter().any(Option::is_none)
        || tets.iter().any(Option::is_none)
    {
        return Err("FullMeta mesh has incomplete node or tetrahedron rows.".into());
    }
    let physical: Vec<V3> = physical.into_iter().map(Option::unwrap).collect();
    if physical
        .iter()
        .flatten()
        .any(|coordinate| !coordinate.is_finite())
    {
        return Err("Mesh node coordinates must be finite.".into());
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in &physical {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let nodes: Vec<V3> = physical
        .iter()
        .map(|p| [p[1] - lo[1], p[2] - lo[2], p[0] - lo[0]])
        .collect();
    let geometry = [hi[1] - lo[1], hi[2] - lo[2], hi[0] - lo[0]];
    let tets: Vec<[usize; 4]> = tets.into_iter().map(Option::unwrap).collect();
    let entities: Vec<usize> = entities.into_iter().map(Option::unwrap).collect();
    if tets.iter().flatten().any(|node| *node >= nodes.len())
        || surface_tags
            .keys()
            .flatten()
            .any(|node| *node >= nodes.len())
        || curve_tags
            .iter()
            .flat_map(|(edge, _)| edge)
            .any(|node| *node >= nodes.len())
    {
        return Err("The mesh references an out-of-range node.".into());
    }
    let mut faces: BTreeMap<[usize; 3], (usize, usize, Option<usize>)> = BTreeMap::new();
    for (e, tet) in tets.iter().enumerate() {
        for f in FACES {
            let mut ids = [tet[f[0]], tet[f[1]], tet[f[2]]];
            ids.sort();
            let entry = faces.entry(ids).or_insert((e, 0, None));
            entry.1 += 1;
            if entry.1 == 2 {
                entry.2 = Some(e)
            }
            if entry.1 > 2 {
                return Err("The tetrahedral mesh contains a non-manifold face.".into());
            }
        }
    }
    let mut boundary = Vec::new();
    let mut integration_faces: HashMap<String, Vec<Face>> = integration_operators
        .iter()
        .filter(|operator| operator.kind == "boundaryIntegration")
        .map(|operator| (operator.id.clone(), Vec::new()))
        .collect();
    let mut integration_edges: HashMap<String, Vec<Line>> = integration_operators
        .iter()
        .filter(|operator| operator.kind == "edgeIntegration")
        .map(|operator| (operator.id.clone(), Vec::new()))
        .collect();
    let mut integration_tets: HashMap<String, Vec<usize>> = integration_operators
        .iter()
        .filter(|operator| {
            matches!(
                operator.kind.as_str(),
                "domainIntegration" | "pointEvaluation"
            )
        })
        .map(|operator| (operator.id.clone(), Vec::new()))
        .collect();
    for (element, tag) in entities.iter().copied().enumerate() {
        for operator in integration_operators.iter().filter(|operator| {
            (operator.kind == "domainIntegration" && operator.physical_tag == tag)
                || operator.kind == "pointEvaluation"
        }) {
            integration_tets
                .get_mut(&operator.id)
                .unwrap()
                .push(element);
        }
    }
    for (nodes, tag) in curve_tags {
        let mut key = nodes;
        key.sort_unstable();
        let Some(element) = tets
            .iter()
            .position(|tet| key.iter().all(|node| tet.contains(node)))
        else {
            return Err(format!(
                "Selected geometry edge {key:?} has no adjacent tetrahedron."
            ));
        };
        for operator in integration_operators
            .iter()
            .filter(|operator| operator.kind == "edgeIntegration" && operator.physical_tag == tag)
        {
            integration_edges.get_mut(&operator.id).unwrap().push(Line {
                nodes: key,
                element,
                tag,
            });
        }
    }
    for (ids, (element, count, other_element)) in faces {
        let selected_surface_tag = surface_tags.get(&ids).copied().unwrap_or(0);
        let face = Face {
            nodes: ids,
            element,
            other_element,
            tag: 0,
        };
        for operator in integration_operators.iter().filter(|operator| {
            operator.kind == "boundaryIntegration" && operator.physical_tag == selected_surface_tag
        }) {
            integration_faces
                .get_mut(&operator.id)
                .unwrap()
                .push(face.clone())
        }
        if count != 1 {
            continue;
        }
        // Boundary IDs come from the mesh map's physical selections. Do not
        // synthesize legacy IDs from coordinate planes: these IDs are dynamic
        // selection tags, so a guessed 1..6 can silently alias an unrelated
        // selected boundary (notably a periodic pair).
        let tag = surface_tags.get(&ids).copied().unwrap_or(0);
        let face = Face {
            nodes: ids,
            element,
            other_element,
            tag,
        };
        if tag > 0 {
            boundary.push(face)
        }
    }
    Ok(Mesh {
        nodes,
        tets,
        entities,
        geometry,
        origin: [lo[1], lo[2], lo[0]],
        boundary,
        integration_faces,
        integration_edges,
        integration_tets,
    })
}

fn boundary_face_normal(mesh: &Mesh, face: &Face) -> Result<V3, String> {
    let points = face.nodes.map(|node| mesh.nodes[node]);
    let centroid = scale(add(add(points[0], points[1]), points[2]), 1.0 / 3.0);
    let opposite = mesh.tets[face.element]
        .iter()
        .copied()
        .find(|node| !face.nodes.contains(node))
        .ok_or("An impedance-boundary face has no opposite tetrahedron node.")?;
    let mut normal = cross(sub(points[1], points[0]), sub(points[2], points[0]));
    if dot(normal, sub(centroid, mesh.nodes[opposite])) < 0.0 {
        normal = scale(normal, -1.0)
    }
    let magnitude = norm(normal);
    if !magnitude.is_finite() || magnitude == 0.0 {
        return Err("An impedance boundary contains a degenerate triangle.".into());
    }
    Ok(scale(normal, 1.0 / magnitude))
}

fn tangential_product(left: V3, right: V3, normal: V3) -> f64 {
    dot(left, right) - dot(left, normal) * dot(right, normal)
}

fn periodic_coordinate_bin(value: f64, tolerance: f64) -> Result<i64, String> {
    let scaled = (value / tolerance).floor();
    if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled > i64::MAX as f64 {
        return Err("Periodic mesh coordinates exceed the supported matching range.".into());
    }
    Ok(scaled as i64)
}

fn periodic_node_map(
    mesh: &Mesh,
    pair: &PeriodicPair,
    tolerance: f64,
) -> Result<HashMap<usize, usize>, String> {
    let mut lower_nodes = HashSet::<usize>::new();
    let mut upper_nodes = HashSet::<usize>::new();
    for face in &mesh.boundary {
        if face.tag == pair.lower_tag {
            lower_nodes.extend(face.nodes)
        }
        if face.tag == pair.upper_tag {
            upper_nodes.extend(face.nodes)
        }
    }
    if lower_nodes.is_empty() || upper_nodes.is_empty() {
        return Err(format!(
            "Periodic pair {} has no triangles on one or both selected boundaries.",
            pair.id
        ));
    }
    if lower_nodes.len() != upper_nodes.len() {
        return Err(format!(
            "Periodic pair {} has {} lower nodes and {} translated-boundary nodes; Gmsh periodic matching was not preserved.",
            pair.id,
            lower_nodes.len(),
            upper_nodes.len()
        ));
    }
    let mut bins = HashMap::<[i64; 3], Vec<usize>>::new();
    for &node in &upper_nodes {
        let point = mesh.nodes[node];
        let key = [
            periodic_coordinate_bin(point[0], tolerance)?,
            periodic_coordinate_bin(point[1], tolerance)?,
            periodic_coordinate_bin(point[2], tolerance)?,
        ];
        bins.entry(key).or_default().push(node)
    }
    let mut mapping = HashMap::<usize, usize>::new();
    let mut matched_upper = HashSet::<usize>::new();
    for &node in &lower_nodes {
        let target = add(mesh.nodes[node], pair.translation);
        let key = [
            periodic_coordinate_bin(target[0], tolerance)?,
            periodic_coordinate_bin(target[1], tolerance)?,
            periodic_coordinate_bin(target[2], tolerance)?,
        ];
        let mut candidates = Vec::new();
        for dx in -1i64..=1 {
            for dy in -1i64..=1 {
                for dz in -1i64..=1 {
                    if let Some(found) = bins.get(&[key[0] + dx, key[1] + dy, key[2] + dz]) {
                        candidates.extend(
                            found
                                .iter()
                                .copied()
                                .filter(|&other| norm(sub(mesh.nodes[other], target)) <= tolerance),
                        );
                    }
                }
            }
        }
        if candidates.len() != 1 {
            return Err(format!(
                "Periodic pair {} lower node {node} has {} translated-boundary matches within {tolerance:.3e} m; expected one.",
                pair.id,
                candidates.len()
            ));
        }
        let other = candidates[0];
        if !matched_upper.insert(other) {
            return Err(format!(
                "Periodic pair {} maps more than one lower node to upper node {other}.",
                pair.id
            ));
        }
        mapping.insert(node, other);
    }
    if matched_upper.len() != upper_nodes.len() {
        return Err(format!(
            "Periodic pair {} does not match every node on the translated boundary.",
            pair.id
        ));
    }
    Ok(mapping)
}

fn find_dofs(
    mesh: &Mesh,
    order: u8,
    periodic_pairs: &[PeriodicPair],
) -> Result<PeriodicDofs, String> {
    let mut edge_ids: HashMap<Vec<usize>, Vec<usize>> = HashMap::new();
    let mut face_ids: HashMap<Vec<usize>, Vec<usize>> = HashMap::new();
    let mut raw_elements = Vec::with_capacity(mesh.tets.len());
    let mut raw_n = 0;
    let edge_modes = if order == 1 { 1 } else { 2 };
    for tet in &mesh.tets {
        let mut ids = Vec::with_capacity(if order == 1 { 6 } else { 20 });
        for edge in EDGES {
            let key = entity_key(vec![tet[edge[0]], tet[edge[1]]]);
            let entry = edge_ids.entry(key).or_insert_with(|| {
                let x = (raw_n..raw_n + edge_modes).collect();
                raw_n += edge_modes;
                x
            });
            ids.extend(entry.iter().copied());
        }
        if order == 2 {
            for face in FACES {
                let key = entity_key(face.iter().map(|&i| tet[i]).collect());
                let entry = face_ids.entry(key).or_insert_with(|| {
                    let x = vec![raw_n, raw_n + 1];
                    raw_n += 2;
                    x
                });
                ids.extend(entry.iter().copied());
            }
        }
        raw_elements.push(ids)
    }
    let mut constraints = PhaseUnionFind::new(raw_n);
    for pair in periodic_pairs {
        let phase = floquet_factor(pair.phase_vector, pair.translation)?;
        let node_map = periodic_node_map(mesh, pair, 1e-10)?;
        let upper_faces = mesh
            .boundary
            .iter()
            .filter(|face| face.tag == pair.upper_tag)
            .map(|face| {
                let mut key = face.nodes.to_vec();
                key.sort_unstable();
                key
            })
            .collect::<HashSet<_>>();
        let mut matched_faces = HashSet::<Vec<usize>>::new();
        let mut lower_face_count = 0usize;
        for face in mesh
            .boundary
            .iter()
            .filter(|face| face.tag == pair.lower_tag)
        {
            lower_face_count += 1;
            let mut translated = Vec::with_capacity(3);
            for node in face.nodes {
                translated.push(*node_map.get(&node).ok_or_else(|| {
                    format!(
                        "Periodic pair {} has no node map for lower boundary node {node}.",
                        pair.id
                    )
                })?)
            }
            translated.sort_unstable();
            if !upper_faces.contains(&translated) {
                return Err(format!(
                    "Periodic pair {} lower surface triangle does not map to a triangle on its translated boundary.",
                    pair.id
                ));
            }
            matched_faces.insert(translated.clone());
            if order == 2 {
                let lower_key = entity_key(face.nodes.to_vec());
                let lower_ids = face_ids.get(&lower_key).ok_or_else(|| {
                    format!(
                        "Periodic pair {} lower surface triangle is missing its face DOFs.",
                        pair.id
                    )
                })?;
                let upper_ids = face_ids.get(&translated).ok_or_else(|| {
                    format!(
                        "Periodic pair {} translated surface triangle is missing its face DOFs.",
                        pair.id
                    )
                })?;
                let lower_order = canonical_points(lower_key, &mesh.nodes);
                let upper_order = canonical_points(translated.clone(), &mesh.nodes);
                for mode in 0..2 {
                    let lower_tangent = sub(
                        mesh.nodes[lower_order[mode + 1]],
                        mesh.nodes[lower_order[0]],
                    );
                    let upper_tangent = sub(
                        mesh.nodes[upper_order[mode + 1]],
                        mesh.nodes[upper_order[0]],
                    );
                    let orientation = dot(lower_tangent, upper_tangent)
                        / (norm(lower_tangent) * norm(upper_tangent));
                    if (orientation.abs() - 1.0).abs() > 1e-8 {
                        return Err(format!(
                            "Periodic pair {} has inconsistent canonical second-order face-moment directions after translation.",
                            pair.id
                        ));
                    }
                    constraints.constrain(
                        lower_ids[mode],
                        upper_ids[mode],
                        c_scale(phase, orientation.signum()),
                    )?;
                }
            }
            for (a, b) in [
                (face.nodes[0], face.nodes[1]),
                (face.nodes[1], face.nodes[2]),
                (face.nodes[2], face.nodes[0]),
            ] {
                let lower_key = entity_key(vec![a, b]);
                let lower_ids = edge_ids.get(&lower_key).ok_or_else(|| {
                    format!(
                        "Periodic pair {} lower boundary edge is missing its edge DOFs.",
                        pair.id
                    )
                })?;
                let upper_key = entity_key(vec![
                    *node_map.get(&a).ok_or_else(|| {
                        format!("Periodic pair {} has no translated node for {a}.", pair.id)
                    })?,
                    *node_map.get(&b).ok_or_else(|| {
                        format!("Periodic pair {} has no translated node for {b}.", pair.id)
                    })?,
                ]);
                let upper_ids = edge_ids.get(&upper_key).ok_or_else(|| {
                    format!(
                        "Periodic pair {} translated boundary edge is missing its edge DOFs.",
                        pair.id
                    )
                })?;
                let lower_order = canonical_points(lower_key, &mesh.nodes);
                let upper_order = canonical_points(upper_key, &mesh.nodes);
                let lower_tangent = sub(mesh.nodes[lower_order[1]], mesh.nodes[lower_order[0]]);
                let upper_tangent = sub(mesh.nodes[upper_order[1]], mesh.nodes[upper_order[0]]);
                let orientation =
                    dot(lower_tangent, upper_tangent) / (norm(lower_tangent) * norm(upper_tangent));
                if (orientation.abs() - 1.0).abs() > 1e-8 {
                    return Err(format!(
                        "Periodic pair {} has inconsistent edge-moment directions after translation.",
                        pair.id
                    ));
                }
                for mode in 0..edge_modes {
                    let sign = if mode == 0 { orientation.signum() } else { 1.0 };
                    constraints.constrain(
                        lower_ids[mode],
                        upper_ids[mode],
                        c_scale(phase, sign),
                    )?;
                }
            }
        }
        if lower_face_count == 0
            || matched_faces.len() != lower_face_count
            || matched_faces.len() != upper_faces.len()
        {
            return Err(format!(
                "Periodic pair {} does not match every boundary triangle.",
                pair.id
            ));
        }
    }
    let (reduced, weights, count) = constraints.mapping();
    Ok(PeriodicDofs {
        elements: raw_elements,
        reduced,
        weights,
        count,
    })
}

fn reduce_periodic_system(
    matrix: &[BTreeMap<usize, C>],
    rhs_re: &[f64],
    rhs_im: &[f64],
    dofs: &PeriodicDofs,
) -> Result<(Vec<BTreeMap<usize, C>>, Vec<f64>, Vec<f64>), String> {
    let full = dofs.reduced.len();
    if full == 0
        || matrix.len() % full != 0
        || rhs_re.len() != matrix.len()
        || rhs_im.len() != matrix.len()
    {
        return Err("The EM3D periodic mapping does not match the assembled system.".into());
    }
    let fields = matrix.len() / full;
    let mut reduced = vec![BTreeMap::<usize, C>::new(); fields * dofs.count];
    let mut re = vec![0.0; reduced.len()];
    let mut im = vec![0.0; reduced.len()];
    for (old_row, entries) in matrix.iter().enumerate() {
        let row = (old_row / full) * dofs.count + dofs.reduced[old_row % full];
        let row_weight = c_conj(dofs.weights[old_row % full]);
        let value = c_mul(
            row_weight,
            C {
                re: rhs_re[old_row],
                im: rhs_im[old_row],
            },
        );
        re[row] += value.re;
        im[row] += value.im;
        for (&old_column, &value) in entries {
            if old_column >= matrix.len() {
                return Err("An EM3D matrix column is outside its periodic mapping.".into());
            }
            let column = (old_column / full) * dofs.count + dofs.reduced[old_column % full];
            let projected = c_mul(c_mul(row_weight, value), dofs.weights[old_column % full]);
            let entry = reduced[row].entry(column).or_default();
            *entry = c_add(*entry, projected);
        }
    }
    Ok((reduced, re, im))
}

fn expand_periodic_solution(
    real: &[f64],
    imaginary: &[f64],
    dofs: &PeriodicDofs,
) -> Result<(Vec<f64>, Vec<f64>), String> {
    if dofs.count == 0 || real.len() != imaginary.len() || real.len() % dofs.count != 0 {
        return Err("The solved EM3D vector does not match its periodic mapping.".into());
    }
    let mut re = Vec::with_capacity(real.len() / dofs.count * dofs.reduced.len());
    let mut im = Vec::with_capacity(re.capacity());
    for field in 0..real.len() / dofs.count {
        for (full, &reduced) in dofs.reduced.iter().enumerate() {
            let index = field * dofs.count + reduced;
            let value = c_mul(
                dofs.weights[full],
                C {
                    re: real[index],
                    im: imaginary[index],
                },
            );
            re.push(value.re);
            im.push(value.im);
        }
    }
    Ok((re, im))
}

fn run(
    mesh: &Mesh,
    frequency: f64,
    frequency_case_index: usize,
    order: u8,
    formulation: &str,
    centroid_constitutive: bool,
    materials: &HashMap<usize, MaterialProperties>,
    config: &SolveConfig,
    solver_backend: &str,
    solver_ordering: &str,
    solver_threads: usize,
    solver_cache: &mut SolverCache,
) -> Result<SolveOut, String> {
    if order != 1 && order != 2 {
        return Err("elementOrder must be 1 or 2 (first- or second-order Nedelec).".into());
    }
    if formulation != "electric" && formulation != "firstOrderEH" {
        return Err("fieldFormulation must be 'electric' or 'firstOrderEH'.".into());
    }
    if formulation == "firstOrderEH" && order != 1 {
        return Err(
            "The experimental firstOrderEH formulation currently supports elementOrder 1 only."
                .into(),
        );
    }
    let coupled = formulation == "firstOrderEH";
    if coupled && solver_backend != "faer" {
        return Err("fieldFormulation 'firstOrderEH' currently requires solverBackend 'faer' because its block matrix is nonsymmetric.".into());
    }
    if !(frequency.is_finite() && frequency > 0.0) {
        return Err("Frequency must be positive and finite.".into());
    }
    let omega = 2.0 * std::f64::consts::PI * frequency;
    if !omega.is_finite() || omega == 0.0 {
        return Err("Angular frequency must be finite and nonzero.".into());
    }
    crate::solver_common::stage_started("assembly");
    let assembly_started = Instant::now();
    let dof_map = find_dofs(mesh, order, &config.periodic_pairs)?;
    let element_dofs = &dof_map.elements;
    let n = dof_map.reduced.len();
    let reduced_n = dof_map.count;
    let nd = if order == 1 { 6 } else { 20 };
    let candidates = if order == 2 {
        quadratic_candidates()
    } else {
        Vec::new()
    };
    let system_n = if coupled { 2 * reduced_n } else { reduced_n };
    let assembly_n = if coupled { 2 * n } else { n };
    println!(
        "  Nedelec order {order} · formulation {formulation}; unknowns {system_n} ({} per field); {} impedance boundaries",
        reduced_n,
        config.impedance_boundaries.len()
    );
    let mut matrix = vec![BTreeMap::<usize, C>::new(); assembly_n];
    let mut rhs_re = vec![0.0; assembly_n];
    let mut rhs_im = vec![0.0; assembly_n];
    let boundary_by_tag = config
        .impedance_boundaries
        .iter()
        .map(|boundary| (boundary.tag, boundary))
        .collect::<HashMap<_, _>>();
    let mut boundary_by_element: HashMap<usize, Vec<Face>> = HashMap::new();
    for boundary in &config.impedance_boundaries {
        if !mesh.boundary.iter().any(|face| face.tag == boundary.tag) {
            return Err(format!(
                "Impedance boundary {} has no exterior faces in the mesh.",
                boundary.id
            ));
        }
    }
    for face in &mesh.boundary {
        if boundary_by_tag.contains_key(&face.tag) {
            boundary_by_element
                .entry(face.element)
                .or_default()
                .push(face.clone())
        }
    }
    let mut locals = Vec::with_capacity(mesh.tets.len());
    for (e, &tet) in mesh.tets.iter().enumerate() {
        let pts = points_for(tet, &mesh.nodes);
        let geo = geometry(pts)?;
        let grads = grad_lambda(geo);
        let inv_m = if order == 2 {
            Some(second_interpolation(geo, tet, &mesh.nodes, &candidates)?)
        } else {
            None
        };
        let dofs = element_dofs[e].clone();
        locals.push(Local {
            geo,
            grad: grads,
            dofs: dofs.clone(),
            order,
            inv_m,
        });
        let material = material_for(materials, e)?;
        let epsilon_tet = EPS0 * material.relative_permittivity;
        let mu_tet = MU0 * material.relative_permeability;
        let sigma = material.conductivity_s_per_m;
        let mut mass = vec![vec![0.0f64; nd]; nd];
        let mut curl = vec![vec![0.0f64; nd]; nd];
        let mut lr = vec![vec![0.0f64; nd]; nd];
        let mut li = vec![vec![0.0f64; nd]; nd];
        if !coupled
            && (!mu_tet.is_finite()
                || mu_tet == 0.0
                || !(omega * mu_tet).is_finite()
                || omega * mu_tet == 0.0)
        {
            return Err(format!(
                "Tetrahedron {e} has an undefined magnetic-field recovery denominator omega*mu."
            ));
        }
        for q in TET_Q {
            let point = std::array::from_fn(|axis| (0..4).map(|i| pts[i][axis] * q[i]).sum());
            let bas = bases_at(
                order,
                geo,
                grads,
                tet,
                &mesh.nodes,
                inv_m.as_ref(),
                &candidates,
                point,
            );
            let weight = geo.det.abs() * q[4];
            for i in 0..nd {
                for j in 0..nd {
                    let m = weight * dot(bas[i].value, bas[j].value);
                    let c = weight * dot(bas[i].value, bas[j].curl);
                    if !coupled || !centroid_constitutive {
                        mass[i][j] += m;
                    }
                    curl[i][j] += c;
                    if !coupled {
                        lr[i][j] += weight
                            * (dot(bas[i].curl, bas[j].curl) / mu_tet
                                - omega * omega * epsilon_tet * dot(bas[i].value, bas[j].value));
                        li[i][j] += weight * omega * sigma * dot(bas[i].value, bas[j].value)
                    }
                }
            }
        }
        // Experimental one-point quadrature for constitutive mass terms only; curl coupling above retains TET_Q.
        if coupled && centroid_constitutive {
            let point = scale(add(add(pts[0], pts[1]), add(pts[2], pts[3])), 0.25);
            let bas = bases_at(
                order,
                geo,
                grads,
                tet,
                &mesh.nodes,
                inv_m.as_ref(),
                &candidates,
                point,
            );
            let weight = geo.det.abs() / 6.0;
            for i in 0..nd {
                for j in 0..nd {
                    mass[i][j] = weight * dot(bas[i].value, bas[j].value);
                }
            }
        }
        if let Some(faces) = boundary_by_element.get(&e) {
            for face in faces {
                let boundary = boundary_by_tag[&face.tag];
                let fp = [
                    mesh.nodes[face.nodes[0]],
                    mesh.nodes[face.nodes[1]],
                    mesh.nodes[face.nodes[2]],
                ];
                let area = face_area(fp);
                if !area.is_finite() || area <= 0.0 {
                    return Err("An impedance boundary contains a degenerate triangle.".into());
                }
                let normal = boundary_face_normal(mesh, face)?;
                for q in TRI_Q {
                    let point = add(
                        add(scale(fp[0], q[0]), scale(fp[1], q[1])),
                        scale(fp[2], q[2]),
                    );
                    let bas = bases_at(
                        order,
                        geo,
                        grads,
                        tet,
                        &mesh.nodes,
                        inv_m.as_ref(),
                        &candidates,
                        point,
                    );
                    let weight = area * q[3];
                    let symbols = boundary_symbols(
                        config,
                        frequency_case_index,
                        frequency,
                        add(point, mesh.origin),
                    )?;
                    let z = model_expression::evaluate_quantity_with_symbols(
                        &boundary.impedance,
                        &symbols,
                        "impedance boundary",
                    )?;
                    if !z.is_finite() || z == 0.0 {
                        return Err(format!(
                            "Impedance boundary {} must evaluate to a finite nonzero value at every quadrature point.",
                            boundary.id
                        ));
                    }
                    let admittance = omega / z;
                    for i in 0..nd {
                        for j in 0..nd {
                            let tangential = tangential_product(bas[i].value, bas[j].value, normal);
                            if coupled {
                                let row = n + dofs[i];
                                let column = dofs[j];
                                let entry = matrix[row].entry(column).or_default();
                                entry.im += admittance * weight * tangential;
                            } else {
                                li[i][j] += admittance * weight * tangential;
                            }
                        }
                    }
                    if let Some(incident_quantities) = &boundary.incident {
                        let incident = incident_quantities
                            .iter()
                            .enumerate()
                            .map(|(axis, quantity)| {
                                quantity.evaluate(
                                    &symbols,
                                    &format!(
                                        "impedance boundary {} polarization[{axis}]",
                                        boundary.id
                                    ),
                                )
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        let incident_re = [incident[1].re, incident[2].re, incident[0].re];
                        let incident_im = [incident[1].im, incident[2].im, incident[0].im];
                        let source_scale = 2.0 * admittance * weight;
                        for i in 0..nd {
                            let source_re = tangential_product(bas[i].value, incident_re, normal);
                            let source_im = tangential_product(bas[i].value, incident_im, normal);
                            let row = if coupled { n + dofs[i] } else { dofs[i] };
                            rhs_re[row] -= source_scale * source_im;
                            rhs_im[row] += source_scale * source_re;
                        }
                    }
                }
            }
        }
        for i in 0..nd {
            for j in 0..nd {
                let a = dofs[i];
                let b = dofs[j];
                if coupled {
                    let m = mass[i][j];
                    let c = curl[i][j];
                    let old = *matrix[a].get(&(n + b)).unwrap_or(&C::default());
                    matrix[a].insert(
                        n + b,
                        C {
                            re: old.re,
                            im: old.im + omega * mu_tet * m,
                        },
                    );
                    let old = *matrix[n + a].get(&b).unwrap_or(&C::default());
                    matrix[n + a].insert(
                        b,
                        C {
                            re: old.re - omega * omega * epsilon_tet * m,
                            im: old.im + omega * sigma * m,
                        },
                    );
                    let old = *matrix[n + a].get(&(n + b)).unwrap_or(&C::default());
                    matrix[n + a].insert(
                        n + b,
                        C {
                            re: old.re,
                            im: old.im - omega * curl[j][i],
                        },
                    );
                    let old = *matrix[a].get(&b).unwrap_or(&C::default());
                    matrix[a].insert(
                        b,
                        C {
                            re: old.re + c,
                            im: old.im,
                        },
                    );
                } else {
                    let old = *matrix[a].get(&b).unwrap_or(&C::default());
                    matrix[a].insert(
                        b,
                        C {
                            re: old.re + lr[i][j],
                            im: old.im + li[i][j],
                        },
                    );
                }
            }
        }
    }
    let (matrix, rhs_re, rhs_im) = reduce_periodic_system(&matrix, &rhs_re, &rhs_im, &dof_map)?;
    let real_block_unknowns = if solver_backend == "rivrs" {
        if crate::solver_common::complex_symmetry_defect(&matrix) > 1e-10 {
            4 * system_n
        } else {
            2 * system_n
        }
    } else {
        system_n
    };
    println!(
        "  profile: assembly and DOF mapping {:.3}s",
        assembly_started.elapsed().as_secs_f64()
    );
    let (solution_re, solution_im, residual) = solve_linear_system(
        &matrix,
        &rhs_re,
        &rhs_im,
        1e-7,
        solver_backend,
        solver_ordering,
        solver_threads,
        solver_cache,
    )?;
    let (solution_re, solution_im) =
        expand_periodic_solution(&solution_re, &solution_im, &dof_map)?;
    let edge_re = solution_re[..n].to_vec();
    let edge_im = solution_im[..n].to_vec();
    let h_re = if coupled {
        solution_re[n..].to_vec()
    } else {
        Vec::new()
    };
    let h_im = if coupled {
        solution_im[n..].to_vec()
    } else {
        Vec::new()
    };
    let post_started = Instant::now();
    let mut fields: HashMap<&str, (Vec<f64>, Vec<f64>)> = config
        .field_variables
        .iter()
        .map(|field| {
            (
                field.symbol.as_str(),
                (vec![0.0; mesh.nodes.len()], vec![0.0; mesh.nodes.len()]),
            )
        })
        .collect();
    let mut count = vec![0.0; mesh.nodes.len()];
    for (e, &tet) in mesh.tets.iter().enumerate() {
        let loc = &locals[e];
        for &node in &tet {
            let bas = bases_at(
                order,
                loc.geo,
                loc.grad,
                tet,
                &mesh.nodes,
                loc.inv_m.as_ref(),
                &candidates,
                mesh.nodes[node],
            );
            let mut er = [0.0; 3];
            let mut ei = [0.0; 3];
            let mut cr = [0.0; 3];
            let mut ci = [0.0; 3];
            for (i, b) in bas.iter().enumerate() {
                let d = loc.dofs[i];
                for a in 0..3 {
                    er[a] += b.value[a] * edge_re[d];
                    ei[a] += b.value[a] * edge_im[d];
                    cr[a] += b.curl[a] * edge_re[d];
                    ci[a] += b.curl[a] * edge_im[d]
                }
            }
            let mu_tet = MU0 * material_for(materials, e)?.relative_permeability;
            for a in 0..3 {
                let model_axis = internal_axis_to_model(a)?;
                let ek = model_field_component_key(config, "electric", model_axis)?;
                let hk = model_field_component_key(config, "magnetic", model_axis)?;
                let ep = fields
                    .get_mut(ek.as_str())
                    .ok_or_else(|| format!("Scenario field declarations omit {ek}."))?;
                ep.0[node] += er[a];
                ep.1[node] += ei[a];
                let hp = fields
                    .get_mut(hk.as_str())
                    .ok_or_else(|| format!("Scenario field declarations omit {hk}."))?;
                if coupled {
                    for (i, b) in bas.iter().enumerate() {
                        let d = loc.dofs[i];
                        hp.0[node] += b.value[a] * h_re[d];
                        hp.1[node] += b.value[a] * h_im[d];
                    }
                } else {
                    hp.0[node] += -ci[a] / (omega * mu_tet);
                    hp.1[node] += cr[a] / (omega * mu_tet);
                }
            }
            count[node] += 1.0;
        }
    }
    for (re, im) in fields.values_mut() {
        for i in 0..count.len() {
            if count[i] > 0.0 {
                re[i] /= count[i];
                im[i] /= count[i]
            }
        }
    }
    let global_variables = evaluate_global_variables(
        mesh,
        &locals,
        &edge_re,
        &edge_im,
        &h_re,
        &h_im,
        frequency,
        frequency_case_index,
        omega,
        materials,
        &candidates,
        coupled,
        config,
    )?;
    let mut declared_values = HashMap::new();
    for declaration in &config.field_variables {
        let (re, im) = fields.get(declaration.symbol.as_str()).ok_or_else(|| {
            format!(
                "FEM reconstruction omitted declared field {}.",
                declaration.symbol
            )
        })?;
        declared_values.insert(
            declaration.symbol.clone(),
            re.iter()
                .zip(im)
                .map(|(&re, &im)| C { re, im })
                .collect::<Vec<_>>(),
        );
    }
    let fobj = field_output::declared_component_fields_for(
        &config.field_variables,
        config.field_dimension,
        &declared_values,
    )?;
    let mut material_output = serde_json::Map::new();
    for (element, &tag) in mesh.entities.iter().enumerate() {
        let key = tag.to_string();
        if material_output.contains_key(&key) {
            continue;
        }
        let properties = material_for(materials, element)?;
        let points = points_for(mesh.tets[element], &mesh.nodes);
        let center: V3 =
            std::array::from_fn(|axis| points.iter().map(|point| point[axis]).sum::<f64>() / 4.0);
        let point_m = [
            center[2] + mesh.origin[2],
            center[0] + mesh.origin[0],
            center[1] + mesh.origin[1],
        ];
        material_output.insert(key,json!({
            "relativePermittivity":properties.relative_permittivity,"relativePermeability":properties.relative_permeability,"conductivitySPerM":properties.conductivity_s_per_m,
            "sampleElementIndex":element,"samplePointM":point_m,"representation":"First actual tetrahedron centroid sample for this material tag; assembly uses the evaluated coefficients of each tetrahedron."
        }));
        println!(
            "  material tag {tag}: tetrahedron {element} sample at {point_m:?} m · eps_r={:0.8}, mu_r={:0.8}, sigma={:0.8} S/m",
            properties.relative_permittivity,
            properties.relative_permeability,
            properties.conductivity_s_per_m
        );
    }
    let material_rows = config
        .material_assignments
        .iter()
        .filter_map(|assignment| {
            Some((
                assignment.get("physicalTag")?.as_u64()? as usize,
                assignment.get("material")?.clone(),
            ))
        })
        .collect::<HashMap<_, _>>();
    let material_parameters = config
        .global_parameter_cases
        .get(frequency_case_index)
        .into_iter()
        .flat_map(|values| values.iter())
        .map(|(symbol, value)| {
            let dimension = config
                .global_parameter_dimensions
                .get(frequency_case_index)
                .and_then(|row| row.get(symbol))
                .copied()
                .unwrap_or(ZERO_DIMENSION);
            (
                symbol.clone(),
                model_expression::Scalar {
                    value: *value,
                    dimension,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let material_cells = mesh
        .tets
        .iter()
        .copied()
        .zip(mesh.entities.iter().copied())
        .collect::<Vec<_>>();
    let material_fields = crate::material::fields_by_assignment_cells(
        &config.scenario_profile_id,
        &material_rows,
        &material_parameters,
        frequency,
        &mesh.nodes,
        &material_cells,
        [2, 0, 1],
        [mesh.origin[2], mesh.origin[0], mesh.origin[1]],
    )?;
    let order_name = if order == 1 { "First" } else { "Second" };
    let output = json!({
        "frequencyGHz":frequency/1e9,"fieldFormulation":formulation,
        "discretization":format!("{order_name}-order Nedelec edge elements on tetrahedra"),
        "geometry":mesh.geometry,"origin":mesh.origin,"coordinateOrder":INTERNAL_COORDINATE_ORDER,
        "fieldComponentCoordinateSystem":"model","fieldConvention":"exp(+i omega t)",
        "nodes":mesh.nodes,"tetrahedra":mesh.tets,"fields":fobj,
        "materialPropertiesByTag":material_output,"materialFieldsByCell":material_fields,
        "globalVariables":global_variables,
        "globalVariableSemantics":"Configured expressions and integration operators are evaluated as written; no automatic port or scattering extraction is applied.",
        "constitutiveQuadrature":if coupled&&centroid_constitutive{"tetrahedron centroid only for E-H constitutive mass terms; curl coupling remains tetrahedral quadrature"}else{"standard 11-point tetrahedral quadrature"},
        "solver":{"backend":solver_backend,"ordering":if solver_backend=="rivrs"{solver_ordering}else{"n/a"},"residual":residual,"unknowns":system_n,"dofsPerField":reduced_n,"realBlockUnknowns":real_block_unknowns,"iterations":system_n},
        "provenance":"FEM solver, reconstructed mesh"
    });
    println!(
        "  profile: field reconstruction and expressions {:.3}s",
        post_started.elapsed().as_secs_f64()
    );
    Ok(SolveOut {
        output,
        residual,
        unknowns: system_n,
    })
}

struct ExpressionContext<'a> {
    mesh: &'a Mesh,
    locals: &'a [Local],
    electric_re: &'a [f64],
    electric_im: &'a [f64],
    magnetic_re: &'a [f64],
    magnetic_im: &'a [f64],
    omega: f64,
    materials: &'a HashMap<usize, MaterialProperties>,
    candidates: &'a [Poly2],
    coupled: bool,
    config: &'a SolveConfig,
}

fn expression_element_field_values(
    context: &ExpressionContext<'_>,
    element: usize,
    point: V3,
    need_curl: bool,
) -> Result<([C; 3], [C; 3]), String> {
    let loc = context
        .locals
        .get(element)
        .ok_or_else(|| format!("Integration references missing tetrahedron {element}."))?;
    let tet = context.mesh.tets[element];
    let basis = bases_at(
        loc.order,
        loc.geo,
        loc.grad,
        tet,
        &context.mesh.nodes,
        loc.inv_m.as_ref(),
        context.candidates,
        point,
    );
    let (mut e, mut curl, mut h) = ([C::default(); 3], [C::default(); 3], [C::default(); 3]);
    for (i, b) in basis.iter().enumerate() {
        let dof = loc.dofs[i];
        for axis in 0..3 {
            e[axis].re += b.value[axis] * context.electric_re[dof];
            e[axis].im += b.value[axis] * context.electric_im[dof];
            if need_curl {
                curl[axis].re += b.curl[axis] * context.electric_re[dof];
                curl[axis].im += b.curl[axis] * context.electric_im[dof];
            }
            if context.coupled {
                h[axis].re += b.value[axis] * context.magnetic_re[dof];
                h[axis].im += b.value[axis] * context.magnetic_im[dof];
            }
        }
    }
    if !context.coupled {
        let mu = MU0 * material_for(context.materials, element)?.relative_permeability;
        for axis in 0..3 {
            h[axis] = C {
                re: -curl[axis].im / (context.omega * mu),
                im: curl[axis].re / (context.omega * mu),
            };
        }
    }
    let to_model_axes = |internal: [C; 3]| -> Result<[C; 3], String> {
        let mut model = [C::default(); 3];
        for (model_axis, name) in ["x", "y", "z"].iter().enumerate() {
            let internal_axis = INTERNAL_COORDINATE_ORDER
                .iter()
                .position(|candidate| candidate == name)
                .ok_or_else(|| format!("Internal coordinate order has no model axis {name:?}."))?;
            model[model_axis] = internal[internal_axis];
        }
        Ok(model)
    };
    // Keep the rotated basis coordinates private to the FEM assembly. Sampled
    // vectors returned to model expressions always use the public model x,y,z order.
    Ok((to_model_axes(e)?, to_model_axes(h)?))
}
fn validate_integration_operator_id(id: &str, function_arities: &Value) -> Result<(), String> {
    if function_arities.get(id).is_some() {
        return Err(format!(
            "Integration operator ID {id:?} conflicts with a built-in expression function."
        ));
    }
    Ok(())
}

fn validate_expression_ast_capabilities(
    node: &Value,
    function_arities: &Value,
    integration_operator_argument_count: usize,
    integration_operator_ids: &BTreeSet<String>,
) -> Result<(), String> {
    let kind = node
        .get("kind")
        .and_then(Value::as_str)
        .ok_or("Expression AST node has no kind.")?;
    match kind {
        "number" | "symbol" => Ok(()),
        "unary" => validate_expression_ast_capabilities(
            node.get("value").ok_or("Unary expression has no value.")?,
            function_arities,
            integration_operator_argument_count,
            integration_operator_ids,
        ),
        "binary" => {
            validate_expression_ast_capabilities(
                node.get("left")
                    .ok_or("Binary expression has no left operand.")?,
                function_arities,
                integration_operator_argument_count,
                integration_operator_ids,
            )?;
            validate_expression_ast_capabilities(
                node.get("right")
                    .ok_or("Binary expression has no right operand.")?,
                function_arities,
                integration_operator_argument_count,
                integration_operator_ids,
            )
        }
        "call" => {
            let name = node
                .get("name")
                .and_then(Value::as_str)
                .ok_or("Expression call has no name.")?;
            let args = node
                .get("args")
                .and_then(Value::as_array)
                .ok_or("Expression call arguments are invalid.")?;
            if integration_operator_ids.contains(name) {
                if args.len() != integration_operator_argument_count {
                    return Err(format!(
                        "Integration operator {name} expects exactly {integration_operator_argument_count} expression{}.",
                        if integration_operator_argument_count == 1 {
                            ""
                        } else {
                            "s"
                        }
                    ));
                }
            } else {
                let arity=function_arities.get(name).ok_or_else(||format!("Expression function {name:?} is not supported by the engine capability catalog."))?;
                let minimum=arity.get("minArgs").and_then(Value::as_u64).ok_or_else(||format!("Capability catalog has no valid minArgs for expression function {name}."))? as usize;
                let maximum = arity
                    .get("maxArgs")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize);
                if args.len() < minimum || maximum.is_some_and(|value| args.len() > value) {
                    let expected = if maximum == Some(minimum) {
                        format!("exactly {minimum}")
                    } else {
                        format!("at least {minimum}")
                    };
                    let argument_noun = if minimum == 1 {
                        "argument"
                    } else {
                        "arguments"
                    };
                    return Err(format!(
                        "Expression function {name} expects {expected} {argument_noun}; received {}.",
                        args.len()
                    ));
                }
            }
            for argument in args {
                validate_expression_ast_capabilities(
                    argument,
                    function_arities,
                    integration_operator_argument_count,
                    integration_operator_ids,
                )?;
            }
            Ok(())
        }
        _ => Err(format!("Unsupported expression AST node kind {kind:?}.")),
    }
}

fn evaluate_global_variables(
    mesh: &Mesh,
    locals: &[Local],
    electric_re: &[f64],
    electric_im: &[f64],
    magnetic_re: &[f64],
    magnetic_im: &[f64],
    frequency_hz: f64,
    frequency_case_index: usize,
    omega: f64,
    materials: &HashMap<usize, MaterialProperties>,
    candidates: &[Poly2],
    coupled: bool,
    config: &SolveConfig,
) -> Result<serde_json::Map<String, Value>, String> {
    let context = ExpressionContext {
        mesh,
        locals,
        electric_re,
        electric_im,
        magnetic_re,
        magnetic_im,
        omega,
        materials,
        candidates,
        coupled,
        config,
    };
    let case_parameters = config
        .global_parameter_cases
        .get(frequency_case_index)
        .ok_or_else(|| {
            format!(
                "No saved model parameter values for frequency row {}.",
                frequency_case_index + 1
            )
        })?;
    let dimensions = config.global_parameter_dimensions.get(frequency_case_index);
    let material_parameters = case_parameters
        .iter()
        .map(|(symbol, value)| {
            let dimension = dimensions
                .and_then(|values| values.get(symbol))
                .copied()
                .unwrap_or(ZERO_DIMENSION);
            (
                symbol.clone(),
                model_expression::Scalar {
                    value: *value,
                    dimension,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    let mut environment = case_parameters
        .iter()
        .map(|(symbol, value)| {
            (
                symbol.clone(),
                fullmeta_core::pointwise::ComplexValue {
                    re: *value,
                    im: 0.0,
                },
            )
        })
        .collect::<HashMap<_, _>>();
    environment.insert(
        "freq".into(),
        fullmeta_core::pointwise::ComplexValue {
            re: frequency_hz,
            im: 0.0,
        },
    );
    environment.insert(
        "frequency".into(),
        fullmeta_core::pointwise::ComplexValue {
            re: frequency_hz,
            im: 0.0,
        },
    );
    environment.insert(
        "omega".into(),
        fullmeta_core::pointwise::ComplexValue { re: omega, im: 0.0 },
    );

    let operators = config
        .integration_operators
        .iter()
        .map(|operator| crate::integration::IntegrationOperator {
            id: operator.id.clone(),
            kind: operator.kind.clone(),
            physical_tag: operator.physical_tag,
            host_material_tag: operator.host_material_tag,
            point_m: operator.point_m,
        })
        .collect::<Vec<_>>();
    let mut domain_cells = Vec::new();
    let mut boundary_cells = Vec::new();
    for operator in &config.integration_operators {
        match operator.kind.as_str() {
            "domainIntegration" => {
                let elements = mesh.integration_tets.get(&operator.id).ok_or_else(|| {
                    format!(
                        "Domain integration operator {} has no mapped tetrahedra.",
                        operator.id
                    )
                })?;
                for &element in elements {
                    let nodes=mesh.tets.get(element).ok_or_else(||format!("Domain integration operator {} references missing tetrahedron {element}.",operator.id))?;
                    domain_cells.push(crate::integration::Simplex {
                        nodes: nodes.to_vec(),
                        physical_tag: operator.physical_tag,
                        material_tag: Some(mesh.entities[element]),
                        operator_id: Some(operator.id.clone()),
                        element_index: Some(element),
                        adjacent_element_index: None,
                    });
                }
            }
            "boundaryIntegration" => {
                let faces = mesh.integration_faces.get(&operator.id).ok_or_else(|| {
                    format!(
                        "Boundary integration operator {} has no mapped mesh faces.",
                        operator.id
                    )
                })?;
                for face in faces {
                    boundary_cells.push(crate::integration::Simplex {
                        nodes: face.nodes.to_vec(),
                        physical_tag: operator.physical_tag,
                        material_tag: Some(mesh.entities[face.element]),
                        operator_id: Some(operator.id.clone()),
                        element_index: Some(face.element),
                        adjacent_element_index: face.other_element,
                    });
                }
            }
            "edgeIntegration" => {
                let edges = mesh.integration_edges.get(&operator.id).ok_or_else(|| {
                    format!(
                        "Edge integration operator {} has no mapped mesh edges.",
                        operator.id
                    )
                })?;
                for edge in edges {
                    boundary_cells.push(crate::integration::Simplex {
                        nodes: edge.nodes.to_vec(),
                        physical_tag: edge.tag,
                        material_tag: Some(mesh.entities[edge.element]),
                        operator_id: Some(operator.id.clone()),
                        element_index: Some(edge.element),
                        adjacent_element_index: None,
                    });
                }
            }
            "pointEvaluation" => {
                let elements = mesh.integration_tets.get(&operator.id).ok_or_else(|| {
                    format!(
                        "Point evaluation operator {} has no candidate tetrahedra.",
                        operator.id
                    )
                })?;
                for &element in elements {
                    let nodes=mesh.tets.get(element).ok_or_else(||format!("Point evaluation operator {} references missing tetrahedron {element}.",operator.id))?;
                    domain_cells.push(crate::integration::Simplex {
                        nodes: nodes.to_vec(),
                        physical_tag: mesh.entities[element],
                        material_tag: Some(mesh.entities[element]),
                        operator_id: Some(operator.id.clone()),
                        element_index: Some(element),
                        adjacent_element_index: None,
                    });
                }
            }
            other => {
                return Err(format!(
                    "Integration operator {} has unsupported type {other:?}.",
                    operator.id
                ));
            }
        }
    }

    let variables = config
        .variables
        .iter()
        .map(|definition| {
            let ast = serde_json::from_value::<fullmeta_core::expression::AstNode>(
                definition.ast.clone(),
            )
            .map_err(|error| {
                format!(
                    "Global variable {} has invalid parsed expression: {error}.",
                    definition.symbol
                )
            })?;
            Ok((definition.symbol.clone(), ast))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let pointwise = config
        .pointwise_variables
        .iter()
        .map(|definition| (definition.symbol.clone(), definition.ast.clone()))
        .collect::<Vec<_>>();
    let field_projection = field_output::field_projection(&context.config.field_variables)?;
    let field_sampler =
        |cell: &crate::integration::Simplex,
         _barycentric: &[f64],
         point: [f64; 3]|
         -> Result<HashMap<String, fullmeta_core::pointwise::ComplexValue>, String> {
            let element = cell
                .element_index
                .ok_or("EM3D integration cell has no owning tetrahedron.")?;
            let (electric, mut magnetic) =
                expression_element_field_values(&context, element, point, !context.coupled)?;
            if !context.coupled {
                if let Some(other) = cell.adjacent_element_index {
                    let (_, other_magnetic) =
                        expression_element_field_values(&context, other, point, true)?;
                    for axis in 0..3 {
                        magnetic[axis] = c_scale(c_add(magnetic[axis], other_magnetic[axis]), 0.5);
                    }
                }
            }
            let scalar_families = HashMap::<&str, C>::new();
            let vector_families = HashMap::from([("electric", electric), ("magnetic", magnetic)]);
            let values = field_output::project_point_fields(
                &field_projection,
                &scalar_families,
                &vector_families,
            )?;
            Ok(values
                .into_iter()
                .map(|(symbol, value)| {
                    (
                        symbol,
                        fullmeta_core::pointwise::ComplexValue {
                            re: value.re,
                            im: value.im,
                        },
                    )
                })
                .collect())
        };
    let material_sampler =
        |cell: &crate::integration::Simplex,
         point: [f64; 3]|
         -> Result<HashMap<String, fullmeta_core::pointwise::ComplexValue>, String> {
            let tag = cell.material_tag.unwrap_or(cell.physical_tag);
            let assignment = config
            .material_assignments
            .iter()
            .find(|assignment| {
                assignment.get("physicalTag").and_then(Value::as_u64) == Some(tag as u64)
            })
            .ok_or_else(|| {
                format!(
                    "Integration entity uses material tag {tag}, which has no material assignment."
                )
            })?;
            let material = assignment.get("material").unwrap_or(assignment);
            let values = crate::material::values_for_assignment(
                &config.scenario_profile_id,
                material,
                &material_parameters,
                frequency_hz,
                point,
                "integration material",
            )?;
            Ok(values
                .into_iter()
                .map(|(symbol, value)| {
                    (
                        symbol,
                        fullmeta_core::pointwise::ComplexValue {
                            re: value.value,
                            im: 0.0,
                        },
                    )
                })
                .collect())
        };
    let output = crate::integration::evaluate_variables_with_sampler(
        &variables,
        &pointwise,
        &operators,
        3,
        &mesh.nodes,
        &domain_cells,
        &boundary_cells,
        environment,
        &field_sampler,
        &material_sampler,
        [2, 0, 1],
        [mesh.origin[2], mesh.origin[0], mesh.origin[1]],
    )?;
    Ok(output)
}
fn resolve_from(base: &Path, p: &str) -> PathBuf {
    let x = PathBuf::from(p);
    if x.is_absolute() { x } else { base.join(x) }
}
fn request_string(request: &Value, key: &str) -> Result<String, String> {
    request
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("request.{key} must be a non-empty string."))
}
fn request_u64(request: &Value, key: &str) -> Result<u64, String> {
    request
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("request.{key} must be an unsigned integer."))
}
fn mesh_file_path(request: &Value, base: &Path) -> Result<PathBuf, String> {
    let name = request_string(request, "meshFile")?;
    Ok(resolve_from(base, &name))
}

fn model_vector_to_internal(value: &Value, path: &str) -> Result<V3, String> {
    let values = value
        .as_array()
        .ok_or_else(|| format!("{path} must be a three-component vector."))?;
    if values.len() != 3 {
        return Err(format!("{path} must be a three-component vector."));
    }
    let model: [f64; 3] = std::array::from_fn(|i| values[i].as_f64().unwrap_or(f64::NAN));
    if model.iter().any(|v| !v.is_finite()) {
        return Err(format!("{path} must contain only finite numbers."));
    }
    Ok([model[1], model[2], model[0]])
}
fn internal_axis_to_model(axis: usize) -> Result<&'static str, String> {
    INTERNAL_COORDINATE_ORDER
        .get(axis)
        .copied()
        .ok_or_else(|| format!("Unsupported internal coordinate axis {axis}."))
}
fn model_field_component_key(
    config: &SolveConfig,
    family: &str,
    axis: &str,
) -> Result<String, String> {
    crate::model_schema::field_symbol_for_solver_component(
        &config.field_variables,
        family,
        Some(axis),
    )
    .map(str::to_string)
    .ok_or_else(|| {
        format!("The scenario field declarations have no {family} component on the {axis} axis.")
    })
}
fn mesh_map_selection_tag(
    selections: &HashMap<String, usize>,
    selection_id: &str,
    path: &str,
) -> Result<usize, String> {
    selections.get(selection_id).copied().ok_or_else(|| {
        format!("{path} references selection {selection_id:?}, which is missing from the mesh map.")
    })
}
fn supports_em3d_mesh_map(schema: Option<&str>, dimension: Option<u64>) -> bool {
    schema == Some("fullmeta-mesh-map/6") && dimension == Some(3)
}
const SOLVER_CONFIGURATION_SCHEMA: &str = "fem-solver-configuration/6";
fn validate_solver_configuration(
    configuration: &Value,
    capabilities: &Value,
) -> Result<(), String> {
    let schema = configuration
        .get("schema")
        .and_then(Value::as_str)
        .ok_or("modelConfiguration.schema must be a string.")?;
    let current = capabilities
        .pointer("/study/requestConfigurationSchema")
        .and_then(Value::as_str)
        .ok_or("engine capability catalog study.requestConfigurationSchema must be a string.")?;
    if current != SOLVER_CONFIGURATION_SCHEMA {
        return Err(format!(
            "engine capability catalog declares solver request schema {current:?}, but this Rust build implements {SOLVER_CONFIGURATION_SCHEMA:?}."
        ));
    }
    if schema == current {
        Ok(())
    } else {
        Err(format!(
            "modelConfiguration.schema {schema:?} is unsupported; current schema is {current:?}."
        ))
    }
}
fn configured_field_files(configuration: &Value) -> Result<bool, String> {
    configuration
        .get("fieldFiles")
        .and_then(Value::as_bool)
        .ok_or_else(|| "modelConfiguration.fieldFiles must be a boolean.".to_string())
}
fn validate_field_component_coordinate_system(configuration: &Value) -> Result<(), String> {
    match configuration
        .get("fieldComponentCoordinateSystem")
        .and_then(Value::as_str)
    {
        Some("model") => Ok(()),
        None => Err("modelConfiguration.fieldComponentCoordinateSystem must be 'model'.".into()),
        Some(value) => Err(format!(
            "modelConfiguration.fieldComponentCoordinateSystem {value:?} is unsupported; use 'model'."
        )),
    }
}
fn configured_domain_selection_ids(configuration: &Value) -> Result<Vec<String>, String> {
    configuration
        .get("domainSelectionIds")
        .and_then(Value::as_array)
        .ok_or("modelConfiguration.domainSelectionIds must be an array.")?
        .iter()
        .enumerate()
        .map(|(index, item)| {
            item.as_str().map(str::to_owned).ok_or_else(|| {
                format!("modelConfiguration.domainSelectionIds[{index}] must be a string.")
            })
        })
        .collect()
}
fn parse_integration_operators(
    configuration: &Value,
    capabilities: &Value,
    selections: &HashMap<String, usize>,
    domain_groups: &HashMap<String, usize>,
    edge_groups: &HashMap<String, usize>,
    point_selections: &[Value],
    clip_host_material_tags: &HashMap<String, usize>,
) -> Result<Vec<IntegrationOperator>, String> {
    let mut integration_operators = Vec::new();
    let mut seen_operators = HashMap::new();
    let operators = configuration
        .get("integrationOperators")
        .and_then(Value::as_array)
        .ok_or("modelConfiguration.integrationOperators must be an array.")?;
    for (index, operator) in operators.iter().enumerate() {
        let id = operator
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("modelConfiguration.integrationOperators[{index}].id is required.")
            })?
            .to_string();
        let kind = operator
            .get("integrationKind")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!(
                    "modelConfiguration.integrationOperators[{index}].integrationKind is required."
                )
            })?
            .to_string();
        let capability = capabilities
            .get("integration")
            .and_then(|items| items.get(&kind))
            .ok_or_else(|| format!("Integration operator {id} uses unsupported type {kind}."))?;
        if capability.get("evaluation").and_then(Value::as_str) != Some("supported") {
            return Err(format!(
                "Integration operator {id} ({kind}) is not supported by the Rust Study evaluator."
            ));
        }
        let selection_id = operator
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Integration operator {id} has no selectionId."))?
            .to_string();
        let (point_m, physical_tag) = match kind.as_str() {
            "boundaryIntegration" => {
                let tag = mesh_map_selection_tag(
                    selections,
                    &selection_id,
                    &format!("Boundary integration operator {id}"),
                )?;
                (None, tag)
            }
            "domainIntegration" => {
                let tag=*domain_groups.get(&selection_id).ok_or_else(||format!("Domain integration operator {id} references selection {selection_id} missing from mesh-map physical groups."))?;
                (None, tag)
            }
            "edgeIntegration" => {
                let tag=*edge_groups.get(&selection_id).ok_or_else(||format!("Edge integration operator {id} references selection {selection_id} missing from mesh-map edge selections."))?;
                (None, tag)
            }
            "pointEvaluation" => {
                let selection=point_selections.iter().find(|item|item.get("id").and_then(Value::as_str)==Some(selection_id.as_str()))
                    .ok_or_else(||format!("Point evaluation operator {id} references unknown mesh point selection {selection_id}."))?;
                let coordinates=selection.get("coordinateM").and_then(Value::as_array)
                    .ok_or_else(||format!("Point evaluation operator {id} requires {selection_id} to resolve to one mesh node."))?;
                if coordinates.len() != 3 {
                    return Err(format!(
                        "Point evaluation operator {id} needs three coordinateM values in 3D."
                    ));
                }
                let point =
                    std::array::from_fn(|axis| coordinates[axis].as_f64().unwrap_or(f64::NAN));
                if point.iter().any(|coordinate| !coordinate.is_finite()) {
                    return Err(format!(
                        "Point evaluation operator {id} coordinateM must be finite."
                    ));
                }
                (Some(point), 0)
            }
            _ => {
                return Err(format!(
                    "Integration operator type {kind:?} is not supported by the current FEM solver."
                ));
            }
        };
        if seen_operators.insert(id.clone(), ()).is_some() {
            return Err(format!("Duplicate integration operator {id}."));
        }
        let host_material_tag =
            if matches!(kind.as_str(), "boundaryIntegration" | "edgeIntegration") {
                clip_host_material_tags.get(&selection_id).copied()
            } else {
                None
            };
        integration_operators.push(IntegrationOperator {
            id,
            kind,
            point_m,
            physical_tag,
            host_material_tag,
        });
    }
    Ok(integration_operators)
}

fn parse_solve_config(
    request: &Value,
    base: &Path,
    capabilities: &Value,
) -> Result<SolveConfig, String> {
    let integration_operator_argument_count=capabilities.pointer("/expressions/integrationOperatorArgumentCount").and_then(Value::as_u64).filter(|count|*count==1).ok_or("engine capability catalog expressions.integrationOperatorArgumentCount must be exactly one for the current integration evaluator.")? as usize;
    let configuration = request
        .get("modelConfiguration")
        .ok_or("request.modelConfiguration is required.")?;
    let profile_id = configuration
        .get("scenarioProfileId")
        .and_then(Value::as_str)
        .ok_or("modelConfiguration.scenarioProfileId is required.")?;
    let field_formulation = request
        .get("fieldFormulation")
        .and_then(Value::as_str)
        .ok_or("request.fieldFormulation is required.")?;
    let field_variables = crate::model_schema::field_variables_for_profile(
        profile_id,
        Some(field_formulation),
        false,
    )?;
    if field_variables.is_empty() {
        return Err(format!(
            "Scenario profile {profile_id:?} has no field declarations for formulation {field_formulation:?}."
        ));
    }
    let field_dimension = crate::model_schema::scenario_metadata_for_profile(profile_id)?
        .pointer("/profile/dimension")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            format!(
                "Scenario {profile_id:?} metadata.profile.dimension must be a positive integer."
            )
        })? as usize;
    let map_name = request
        .get("meshMap")
        .and_then(Value::as_str)
        .ok_or("request.meshMap must be a string.")?;
    validate_solver_configuration(configuration, capabilities)?;
    let field_files = configured_field_files(configuration)?;
    validate_field_component_coordinate_system(configuration)?;
    let map_path = resolve_from(base, map_name);
    let map: Value = serde_json::from_slice(
        &fs::read(&map_path)
            .map_err(|e| format!("Could not read model mesh map {}: {e}", map_path.display()))?,
    )
    .map_err(|e| format!("Could not parse model mesh map {}: {e}", map_path.display()))?;
    if !supports_em3d_mesh_map(
        map.get("schema").and_then(Value::as_str),
        map.get("dimension").and_then(Value::as_u64),
    ) {
        return Err("meshMap must use fullmeta-mesh-map/6 with dimension 3.".into());
    }
    let mut selections = HashMap::<String, usize>::new();
    for item in map
        .get("boundarySelections")
        .and_then(Value::as_array)
        .ok_or("meshMap.boundarySelections must be an array.")?
    {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or("meshMap boundary selection has no id.")?
            .to_string();
        let tag = item
            .get("physicalTag")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("meshMap boundary selection {id} has no physical tag."))?
            as usize;
        if tag == 0 || selections.insert(id.clone(), tag).is_some() {
            return Err(format!(
                "meshMap boundary selection {id} is invalid or duplicated."
            ));
        }
    }
    let mut domain_groups = HashMap::<String, usize>::new();
    for group in map
        .get("physicalGroups")
        .and_then(Value::as_array)
        .ok_or("meshMap.physicalGroups must be an array.")?
    {
        let selection_id = group
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or("meshMap physical group has no selectionId.")?
            .to_string();
        let tag = group
            .get("physicalTag")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("meshMap physical group for {selection_id} has no physicalTag.")
            })? as usize;
        if tag == 0 || domain_groups.insert(selection_id.clone(), tag).is_some() {
            return Err(format!(
                "meshMap physical group for {selection_id} is invalid or duplicated."
            ));
        }
    }
    let mut edge_groups = HashMap::<String, usize>::new();
    for edge in map
        .get("edgeSelections")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let selection_id = edge
            .get("id")
            .and_then(Value::as_str)
            .ok_or("meshMap edge selection has no id.")?
            .to_string();
        let tag = edge
            .get("physicalTag")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("meshMap edge selection {selection_id} has no physical tag."))?
            as usize;
        if tag == 0 || edge_groups.insert(selection_id.clone(), tag).is_some() {
            return Err(format!(
                "meshMap edge selection {selection_id} is invalid or duplicated."
            ));
        }
    }
    let mut clip_host_material_tags = HashMap::<String, usize>::new();
    for collection in ["boundarySelections", "edgeSelections"] {
        for selection in map
            .get(collection)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            if selection.get("type").and_then(Value::as_str) != Some("clip") {
                continue;
            }
            let selection_id = selection
                .get("id")
                .and_then(Value::as_str)
                .or_else(|| selection.get("selectionId").and_then(Value::as_str))
                .ok_or("meshMap clipped selection has no id.")?;
            let host_id = selection
                .get("hostSelectionId")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    format!("meshMap clipped selection {selection_id} has no hostSelectionId.")
                })?;
            if let Some(tag) = domain_groups.get(host_id) {
                clip_host_material_tags.insert(selection_id.to_string(), *tag);
            }
        }
    }
    let mut mapped_periodic_pairs = HashMap::<String, (String, String)>::new();
    for pair in map
        .get("periodicPairs")
        .and_then(Value::as_array)
        .ok_or("meshMap.periodicPairs must be an array.")?
    {
        let id = pair
            .get("id")
            .and_then(Value::as_str)
            .ok_or("meshMap periodic pair has no id.")?
            .to_string();
        let lower = pair
            .get("lowerSelectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("meshMap periodic pair {id} has no lower selection."))?
            .to_string();
        let upper = pair
            .get("upperSelectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("meshMap periodic pair {id} has no upper selection."))?
            .to_string();
        if mapped_periodic_pairs
            .insert(id.clone(), (lower, upper))
            .is_some()
        {
            return Err(format!("meshMap periodic pair {id} is duplicated."));
        }
    }
    let assignments = configuration
        .get("materialAssignments")
        .and_then(Value::as_array)
        .ok_or("modelConfiguration.materialAssignments must be an array.")?;
    let mut assigned_selections = HashMap::<String, usize>::new();
    for (index, assignment) in assignments.iter().enumerate() {
        let selection_id = assignment
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("modelConfiguration.materialAssignments[{index}].selectionId is required.")
            })?;
        let tag = assignment
            .get("physicalTag")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!("modelConfiguration.materialAssignments[{index}].physicalTag is required.")
            })? as usize;
        if domain_groups.get(selection_id) != Some(&tag)
            || assigned_selections
                .insert(selection_id.to_string(), tag)
                .is_some()
        {
            return Err(format!(
                "Material assignment for {selection_id} does not match one unique mesh-map physical group."
            ));
        }
    }
    if assigned_selections.len() != domain_groups.len() {
        return Err("Material assignments must cover every mesh-map domain physical group.".into());
    }
    let boundary_values = configuration
        .get("impedanceBoundaries")
        .and_then(Value::as_array)
        .ok_or("modelConfiguration.impedanceBoundaries must be an array.")?;
    let mut impedance_boundaries = Vec::with_capacity(boundary_values.len());
    let mut boundary_ids = HashSet::new();
    let mut boundary_tags = HashSet::new();
    for (index, boundary) in boundary_values.iter().enumerate() {
        let path = format!("modelConfiguration.impedanceBoundaries[{index}]");
        let id = boundary
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{path}.id is required."))?
            .to_string();
        let selection_id = boundary
            .get("selectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{path}.selectionId is required."))?;
        let tag = mesh_map_selection_tag(&selections, selection_id, &path)?;
        if !boundary_ids.insert(id.clone()) || !boundary_tags.insert(tag) {
            return Err(format!(
                "Impedance boundary {id} duplicates a boundary ID or mesh selection."
            ));
        }
        let impedance = boundary
            .get("impedance")
            .cloned()
            .ok_or_else(|| format!("{path}.impedance is required."))?;
        let primary_units = field_variables
            .iter()
            .filter(|field| field.role == "primary")
            .map(|field| field.unit.clone())
            .collect::<Vec<_>>();
        if primary_units.len() != 3 {
            return Err(format!(
                "{path} requires three active electric-field components."
            ));
        }
        let incident =
            model_expression::incident_component_quantities(boundary, &primary_units, &path)?
                .map(|components| {
                    components
                        .iter()
                        .enumerate()
                        .map(|(axis, component)| {
                            model_expression::CompiledComplexQuantity::compile(
                                component,
                                &format!("{path}.polarization[{axis}]"),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
        impedance_boundaries.push(ImpedanceBoundary {
            id,
            tag,
            impedance,
            incident,
        });
    }
    let pair_values = configuration
        .get("periodicPairs")
        .and_then(Value::as_array)
        .ok_or("modelConfiguration.periodicPairs must be an array.")?;
    let mut periodic_pairs = Vec::with_capacity(pair_values.len());
    let mut pair_ids = HashSet::new();
    for (index, pair) in pair_values.iter().enumerate() {
        let id = pair
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("modelConfiguration.periodicPairs[{index}].id is required."))?;
        if !pair_ids.insert(id.to_string()) {
            return Err(format!("Periodic pair {id} is duplicated."));
        }
        let mapped_pair = mapped_periodic_pairs
            .get(id)
            .ok_or_else(|| format!("Periodic pair {id} is missing from the mesh map."))?;
        let lower_id = pair
            .get("lowerSelectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Periodic pair {id} has no lower selection id."))?;
        let upper_id = pair
            .get("upperSelectionId")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("Periodic pair {id} has no upper selection id."))?;
        if mapped_pair.0 != lower_id || mapped_pair.1 != upper_id {
            return Err(format!(
                "Periodic pair {id} selection IDs do not match the mesh map."
            ));
        }
        let lower_tag = mesh_map_selection_tag(
            &selections,
            lower_id,
            &format!("periodic pair {id} lower boundary"),
        )?;
        let upper_tag = mesh_map_selection_tag(
            &selections,
            upper_id,
            &format!("periodic pair {id} translated boundary"),
        )?;
        if lower_tag == upper_tag {
            return Err(format!(
                "Periodic pair {id} must reference two distinct boundary selections."
            ));
        }
        if boundary_tags.contains(&lower_tag) || boundary_tags.contains(&upper_tag) {
            return Err(format!(
                "Periodic pair {id} also carries an impedance boundary condition."
            ));
        }
        let translation = model_vector_to_internal(
            pair.get("translationM")
                .ok_or_else(|| format!("Periodic pair {id}.translationM is required."))?,
            &format!("periodic pair {id}.translationM"),
        )?;
        let phase_vector = model_vector_to_internal(
            pair.get("phaseVector")
                .ok_or_else(|| format!("Periodic pair {id}.phaseVector is required."))?,
            &format!("periodic pair {id}.phaseVector"),
        )?;
        periodic_pairs.push(PeriodicPair {
            id: id.to_string(),
            lower_tag,
            upper_tag,
            translation,
            phase_vector,
        });
    }
    let rows = configuration
        .get("globalParametersByFrequency")
        .and_then(Value::as_array)
        .ok_or("modelConfiguration.globalParametersByFrequency must be an array.")?;
    let mut global_parameter_cases = Vec::new();
    for (row_index, row) in rows.iter().enumerate() {
        let values = row.as_object().ok_or_else(|| {
            format!(
                "modelConfiguration.globalParametersByFrequency[{row_index}] must be an object."
            )
        })?;
        let mut parameters = HashMap::new();
        for (symbol, value) in values {
            let number=value.as_f64().ok_or_else(||format!("modelConfiguration.globalParametersByFrequency[{row_index}].{symbol} must be numeric."))?;
            if !number.is_finite() {
                return Err(format!(
                    "modelConfiguration.globalParametersByFrequency[{row_index}].{symbol} must be finite."
                ));
            }
            parameters.insert(symbol.clone(), number);
        }
        global_parameter_cases.push(parameters);
    }
    let dimension_rows = configuration
        .get("globalParameterDimensionsByFrequency")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut global_parameter_dimensions = Vec::with_capacity(global_parameter_cases.len());
    for (row_index, parameters) in global_parameter_cases.iter().enumerate() {
        let row = dimension_rows.get(row_index).and_then(Value::as_object);
        let mut dimensions = HashMap::new();
        for symbol in parameters.keys() {
            let dimension=match row.and_then(|values|values.get(symbol)) {
                Some(value)=>dimension_from_json(value).ok_or_else(||format!("modelConfiguration.globalParameterDimensionsByFrequency[{row_index}].{symbol} must contain four or five finite SI dimension exponents."))?,
                None=>ZERO_DIMENSION,
            };
            dimensions.insert(symbol.clone(), dimension);
        }
        global_parameter_dimensions.push(dimensions);
    }
    let material_assignments = configuration
        .get("materialAssignments")
        .and_then(Value::as_array)
        .cloned()
        .ok_or("modelConfiguration.materialAssignments must be an array.")?;
    let scenario_profile_id = configuration
        .get("scenarioProfileId")
        .and_then(Value::as_str)
        .ok_or("modelConfiguration.scenarioProfileId must be a string.")?
        .to_string();
    let mut variables = Vec::new();
    let definitions = configuration
        .get("variables")
        .and_then(Value::as_array)
        .ok_or("modelConfiguration.variables must be an array.")?;
    for (index, definition) in definitions.iter().enumerate() {
        let symbol = definition
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("modelConfiguration.variables[{index}].symbol is required."))?
            .to_string();
        let ast = definition
            .get("ast")
            .ok_or_else(|| format!("modelConfiguration.variables[{index}].ast is required."))?
            .clone();
        if !symbol.chars().enumerate().all(|(i, ch)| {
            if i == 0 {
                ch == '_' || ch.is_ascii_alphabetic()
            } else {
                ch == '_' || ch.is_ascii_alphanumeric()
            }
        }) {
            return Err(format!("Global variable symbol {symbol:?} is invalid."));
        }
        variables.push(VariableDefinition { symbol, ast })
    }
    let point_selections = map
        .get("pointSelections")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let integration_operators = parse_integration_operators(
        configuration,
        capabilities,
        &selections,
        &domain_groups,
        &edge_groups,
        point_selections,
        &clip_host_material_tags,
    )?;
    let function_arities = capabilities
        .pointer("/expressions/functionArities")
        .ok_or("engine capability catalog expressions.functionArities must be an object.")?;
    let integration_operator_ids = integration_operators
        .iter()
        .map(|operator| operator.id.clone())
        .collect::<BTreeSet<_>>();
    for operator in &integration_operators {
        validate_integration_operator_id(&operator.id, function_arities)?;
    }
    for variable in &variables {
        validate_expression_ast_capabilities(
            &variable.ast,
            function_arities,
            integration_operator_argument_count,
            &integration_operator_ids,
        )
        .map_err(|error| format!("Global variable {}: {error}", variable.symbol))?;
    }
    let pointwise_values = configuration
        .get("pointwiseVariables")
        .and_then(Value::as_array)
        .ok_or("modelConfiguration.pointwiseVariables must be an array.")?;
    let no_integration_operators = BTreeSet::new();
    let mut pointwise_variables = Vec::with_capacity(pointwise_values.len());
    for (index, definition) in pointwise_values.iter().enumerate() {
        let symbol = definition
            .get("symbol")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                format!("modelConfiguration.pointwiseVariables[{index}].symbol is required.")
            })?
            .to_string();
        if !symbol.chars().enumerate().all(|(i, ch)| {
            if i == 0 {
                ch == '_' || ch.is_ascii_alphabetic()
            } else {
                ch == '_' || ch.is_ascii_alphanumeric()
            }
        }) {
            return Err(format!("Pointwise variable symbol {symbol:?} is invalid."));
        }
        let ast_value = definition
            .get("ast")
            .ok_or_else(|| {
                format!("modelConfiguration.pointwiseVariables[{index}].ast is required.")
            })?
            .clone();
        validate_expression_ast_capabilities(
            &ast_value,
            function_arities,
            integration_operator_argument_count,
            &no_integration_operators,
        )
        .map_err(|error| format!("Pointwise variable {symbol}: {error}"))?;
        let ast = serde_json::from_value(ast_value).map_err(|error| {
            format!("Pointwise variable {symbol} has invalid expression AST: {error}")
        })?;
        pointwise_variables.push(PointwiseDefinition { symbol, ast });
    }
    Ok(SolveConfig {
        periodic_pairs,
        impedance_boundaries,
        global_parameter_cases,
        global_parameter_dimensions,
        material_assignments,
        scenario_profile_id,
        variables,
        pointwise_variables,
        integration_operators,
        field_files,
        field_variables,
        field_dimension,
    })
}

pub(crate) fn run_request(req: &Path) -> Result<(), Box<dyn Error>> {
    let mut solver_cache = SolverCache::default();
    run_request_with_cache(req, &mut solver_cache)
}

pub(crate) fn run_request_with_cache(
    req: &Path,
    solver_cache: &mut SolverCache,
) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let parse_started = Instant::now();
    let mut request: Value = serde_json::from_slice(&fs::read(req)?)?;
    let base = req.parent().unwrap_or(Path::new("."));
    solver_request::validate_single_case_request(&request, "em-3d")
        .map_err(|error| format!("Solver request: {error}"))?;
    expand_frequency_case(&mut request).map_err(|error| format!("Solver request: {error}"))?;
    let capabilities = crate::json_schema::engine_capabilities()?;
    if capabilities.get("schema").and_then(Value::as_str) != Some("fem-engine-capabilities/2") {
        return Err("The embedded schema capability view has an unsupported schema.".into());
    }
    let config = parse_solve_config(&request, base, &capabilities)
        .map_err(|e| format!("Solver configuration: {e}"))?;
    let mesh_path = mesh_file_path(&request, base).map_err(|e| format!("Solver request: {e}"))?;
    let mesh = parse_mesh(
        &fs::read_to_string(mesh_path)?,
        &config.integration_operators,
    )?;
    if request.get("countOnly").and_then(Value::as_bool) == Some(true) {
        println!(
            "Mesh: {} nodes, {} tetrahedra",
            mesh.nodes.len(),
            mesh.tets.len()
        );
        for order in [1, 2] {
            let dofs = find_dofs(&mesh, order, &config.periodic_pairs)?;
            println!("Nedelec order {order}: {} periodic DOFs", dofs.count);
        }
        return Ok(());
    }
    let formulation =
        request_string(&request, "fieldFormulation").map_err(|e| format!("Solver request: {e}"))?;
    let formulation_capabilities = capabilities
        .pointer("/mesh/fieldFormulations")
        .and_then(Value::as_object)
        .ok_or("engine capability catalog mesh.fieldFormulations must be an object.")?;
    let allowed_orders=formulation_capabilities.get(&formulation).and_then(|item|item.get("elementOrders")).and_then(Value::as_array).ok_or_else(||format!("request.fieldFormulation {formulation:?} is not supported by the engine capability catalog."))?;
    let order =
        request_u64(&request, "elementOrder").map_err(|e| format!("Solver request: {e}"))?;
    if !allowed_orders
        .iter()
        .any(|allowed| allowed.as_u64() == Some(order))
    {
        return Err(format!(
            "request.elementOrder {order} is not supported for field formulation {formulation}."
        )
        .into());
    }
    let order = order as u8;
    let constitutive_quadrature = request_string(&request, "constitutiveQuadrature")
        .map_err(|e| format!("Solver request: {e}"))?;
    if constitutive_quadrature != "standard" && constitutive_quadrature != "centroid" {
        return Err("request.constitutiveQuadrature must be 'standard' or 'centroid'".into());
    }
    if constitutive_quadrature == "centroid" && formulation != "firstOrderEH" {
        return Err(
            "centroid constitutive quadrature is currently supported only for firstOrderEH".into(),
        );
    }
    let centroid_constitutive = constitutive_quadrature == "centroid";
    let solver_backend =
        request_string(&request, "solverBackend").map_err(|e| format!("Solver request: {e}"))?;
    if solver_backend != "faer" && solver_backend != "rivrs" {
        return Err("request.solverBackend must be 'faer' or 'rivrs'".into());
    }
    let solver_ordering =
        request_string(&request, "solverOrdering").map_err(|e| format!("Solver request: {e}"))?;
    if solver_backend == "rivrs" && solver_ordering != "metis" && solver_ordering != "amd" {
        return Err(
            "request.solverOrdering must be 'metis' or 'amd' for solverBackend rivrs".into(),
        );
    }
    let solver_threads = request_u64(&request, "solverThreads")
        .map_err(|e| format!("Solver request: {e}"))? as usize;
    if solver_threads == 0 {
        return Err("request.solverThreads must be positive".into());
    }
    let frequencies: Vec<f64> = request["frequenciesGHz"]
        .as_array()
        .ok_or("request.frequenciesGHz must be an array")?
        .iter()
        .map(|v| v.as_f64().ok_or("frequency must be numeric"))
        .collect::<Result<_, _>>()?;
    let maximum_sweep_points = capabilities
        .pointer("/study/maximumSweepPoints")
        .and_then(Value::as_u64)
        .ok_or("engine capability catalog study.maximumSweepPoints must be an integer.")?
        as usize;
    if frequencies.is_empty() {
        return Err("Provide at least one frequency.".into());
    }
    if frequencies.len() > maximum_sweep_points {
        return Err(format!("Provide no more than {maximum_sweep_points} frequencies.").into());
    }
    if frequencies
        .iter()
        .any(|frequency| !frequency.is_finite() || *frequency <= 0.0)
    {
        return Err("Frequencies must be positive and finite.".into());
    }
    if config.global_parameter_cases.len() != frequencies.len() {
        return Err("modelConfiguration.globalParametersByFrequency must have one entry for each requested frequency.".into());
    }
    let output_directory =
        request_string(&request, "outputDirectory").map_err(|e| format!("Solver request: {e}"))?;
    let output_dir = resolve_from(base, &output_directory);
    let field_prefix =
        request_string(&request, "fieldFilePrefix").map_err(|e| format!("Solver request: {e}"))?;
    let materials_by_element_frequency = frequencies
        .iter()
        .enumerate()
        .map(|(index, frequency)| {
            evaluate_element_materials(
                &mesh,
                &config,
                index,
                frequency * 1e9,
                formulation == "electric",
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let sweep_csv =
        request_string(&request, "sweepCsv").map_err(|e| format!("Solver request: {e}"))?;
    let debug_csv =
        request_string(&request, "debugCsv").map_err(|e| format!("Solver request: {e}"))?;
    if sweep_csv == debug_csv {
        return Err(
            "request.sweepCsv and request.debugCsv must name different output files.".into(),
        );
    }
    let sweep_path = output_dir.join(sweep_csv);
    let debug_csv_path = output_dir.join(debug_csv);
    let mut csv_rows = Vec::<CsvFrequencyRow>::new();
    fs::create_dir_all(&output_dir)?;
    println!(
        "Rust FEM · order {order} · formulation {formulation} · solver {solver_backend} · ordering {solver_ordering} · {solver_threads} thread(s) · reconstructed mesh {} nodes, {} tetrahedra · {} frequency point(s)",
        mesh.nodes.len(),
        mesh.tets.len(),
        frequencies.len()
    );
    println!(
        "profile: request/mesh parsing and setup {:.3}s",
        parse_started.elapsed().as_secs_f64()
    );
    for (frequency_index, frequency) in frequencies.iter().copied().enumerate() {
        let t0 = Instant::now();
        let materials = &materials_by_element_frequency[frequency_index];
        println!("Solving {frequency:0.3} GHz...");

        let solved = run(
            &mesh,
            frequency * 1e9,
            frequency_index,
            order,
            &formulation,
            centroid_constitutive,
            materials,
            &config,
            &solver_backend,
            &solver_ordering,
            solver_threads,
            solver_cache,
        )
        .map_err(|e| format!("{frequency:0.3} GHz: {e}"))?;
        let variables = solved.output["globalVariables"].as_object();
        let variable_values = config
            .variables
            .iter()
            .map(|definition| {
                let value = variables
                    .and_then(|values| values.get(&definition.symbol))
                    .ok_or_else(|| {
                        format!(
                            "Solver output omitted global variable {}.",
                            definition.symbol
                        )
                    })?;
                Ok(C {
                    re: value["re"].as_f64().ok_or_else(|| {
                        format!(
                            "Solver output has invalid real value for {}.",
                            definition.symbol
                        )
                    })?,
                    im: value["im"].as_f64().ok_or_else(|| {
                        format!(
                            "Solver output has invalid imaginary value for {}.",
                            definition.symbol
                        )
                    })?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        csv_rows.push(CsvFrequencyRow {
            frequency_ghz: frequency,
            residual: solved.residual,
            unknowns: solved.unknowns,
            variable_values,
        });
        crate::solver_common::stage_started("output");
        if config.field_files {
            let field_name = format!("{field_prefix}_{:0.12}GHz.json", frequency);
            fs::write(
                output_dir.join(field_name),
                serde_json::to_vec(&solved.output)?,
            )?;
        }
        println!(
            "  residual={:0.2e} ({:0.1}s)",
            solved.residual,
            t0.elapsed().as_secs_f64()
        );
    }
    let variable_rows = csv_rows
        .iter()
        .map(|row| row.variable_values.clone())
        .collect::<Vec<_>>();
    let variable_is_real = integration::variable_real_flags(&variable_rows);
    let debug_symbols = config
        .variables
        .iter()
        .map(|definition| definition.symbol.clone())
        .collect::<Vec<_>>();
    let debug_indices = (0..config.variables.len()).collect::<Vec<_>>();
    let debug_rows = csv_rows
        .iter()
        .map(|row| {
            debug_indices
                .iter()
                .map(|&index| row.variable_values[index])
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let debug_real_flags = integration::variable_real_flags(&debug_rows);
    let mut csv = String::from("frequency_GHz");
    integration::append_variable_csv_headers(
        &mut csv,
        &config
            .variables
            .iter()
            .map(|definition| definition.symbol.clone())
            .collect::<Vec<_>>(),
        &variable_is_real,
    );
    csv.push('\n');
    for row in &csv_rows {
        csv.push_str(&format!("{:0.8}", row.frequency_ghz));
        integration::append_variable_csv_values(&mut csv, &row.variable_values, &variable_is_real);
        csv.push('\n');
    }
    let mut debug_csv =
        String::from("frequency_GHz,residual,mesh_nodes,tetrahedra,nedelec_dofs,element_order");
    integration::append_variable_csv_headers(&mut debug_csv, &debug_symbols, &debug_real_flags);
    debug_csv.push('\n');
    for (row, debug_values) in csv_rows.into_iter().zip(debug_rows) {
        debug_csv.push_str(&format!(
            "{:0.8},{:0.6e},{},{},{},{}",
            row.frequency_ghz,
            row.residual,
            mesh.nodes.len(),
            mesh.tets.len(),
            row.unknowns,
            order
        ));
        integration::append_variable_csv_values(&mut debug_csv, &debug_values, &debug_real_flags);
        debug_csv.push('\n');
    }
    println!("  Configured global variables were evaluated from their model expressions.");
    println!("  Study CSV columns: frequency plus all configured global variables");
    println!("  Debug CSV includes mesh and solver diagnostics plus configured global variables");
    fs::write(&sweep_path, csv)?;
    fs::write(&debug_csv_path, debug_csv)?;
    println!(
        "Wrote {} and {} in {:0.1}s",
        sweep_path.display(),
        debug_csv_path.display(),
        start.elapsed().as_secs_f64()
    );
    Ok(())
}
fn expand_frequency_case(request: &mut Value) -> Result<(), String> {
    let cases = request
        .get("frequencyCases")
        .and_then(Value::as_array)
        .cloned()
        .ok_or("request.frequencyCases must be an array.")?;
    if cases.is_empty() {
        return Err("request.frequencyCases must contain at least one frequency.".into());
    }
    let mut frequencies_ghz = Vec::with_capacity(cases.len());
    let mut parameter_cases = Vec::with_capacity(cases.len());
    let mut dimension_cases = Vec::with_capacity(cases.len());
    for (case_index, case) in cases.iter().enumerate() {
        let frequency_hz = case
            .get("frequencyHz")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .ok_or_else(|| {
                format!(
                    "request.frequencyCases[{case_index}].frequencyHz must be positive and finite."
                )
            })?;
        frequencies_ghz.push(frequency_hz / 1e9);
        parameter_cases.push(
            case.get("globalParameters")
                .cloned()
                .unwrap_or_else(|| json!({})),
        );
        dimension_cases.push(
            case.get("globalParameterDimensions")
                .cloned()
                .unwrap_or_else(|| json!({})),
        );
    }
    let configuration = request
        .get_mut("modelConfiguration")
        .and_then(Value::as_object_mut)
        .ok_or("request.modelConfiguration must be an object.")?;
    configuration.insert("globalParametersByFrequency".into(), json!(parameter_cases));
    configuration.insert(
        "globalParameterDimensionsByFrequency".into(),
        json!(dimension_cases),
    );
    request["frequenciesGHz"] = json!(frequencies_ghz);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    fn floquet_cube_mesh() -> Mesh {
        floquet_structured_mesh(1, 1)
    }

    fn floquet_structured_mesh(transverse: usize, axial: usize) -> Mesh {
        let index =
            |x: usize, y: usize, z: usize| x + (transverse + 1) * (y + (transverse + 1) * z);
        let mut nodes = Vec::new();
        for z in 0..=axial {
            for y in 0..=transverse {
                for x in 0..=transverse {
                    nodes.push([
                        x as f64 / transverse as f64,
                        y as f64 / transverse as f64,
                        z as f64 / axial as f64,
                    ]);
                }
            }
        }
        let mut tets = Vec::new();
        for z in 0..axial {
            for y in 0..transverse {
                for x in 0..transverse {
                    let vertices: [usize; 8] = std::array::from_fn(|bits| {
                        index(x + (bits & 1), y + ((bits >> 1) & 1), z + ((bits >> 2) & 1))
                    });
                    for local in [
                        [0, 1, 3, 7],
                        [0, 3, 2, 7],
                        [0, 2, 6, 7],
                        [0, 6, 4, 7],
                        [0, 4, 5, 7],
                        [0, 5, 1, 7],
                    ] {
                        tets.push(local.map(|local| vertices[local]));
                    }
                }
            }
        }
        let mut faces = BTreeMap::<Vec<usize>, (usize, usize, Option<usize>)>::new();
        for (element, tet) in tets.iter().enumerate() {
            for face in FACES {
                let key = entity_key(face.iter().map(|&axis| tet[axis]).collect());
                let entry = faces.entry(key).or_insert((element, 0, None));
                entry.1 += 1;
                if entry.1 == 2 {
                    entry.2 = Some(element);
                }
            }
        }
        let mut boundary = Vec::new();
        for (key, (element, count, other_element)) in faces {
            let mut tag = 0;
            if count == 1 {
                for axis in 0..3 {
                    for side in 0..2 {
                        if key.iter().all(|&node| nodes[node][axis] == side as f64) {
                            tag = axis * 2 + side + 1;
                        }
                    }
                }
            }
            let face = Face {
                nodes: [key[0], key[1], key[2]],
                element,
                other_element,
                tag,
            };
            if count == 1 {
                boundary.push(face);
            }
        }
        let entities = vec![1; tets.len()];
        Mesh {
            nodes,
            tets,
            entities,
            geometry: [1.0; 3],
            origin: [0.0; 3],
            boundary,
            integration_faces: HashMap::new(),
            integration_edges: HashMap::new(),
            integration_tets: HashMap::new(),
        }
    }

    fn floquet_pairs(k: V3) -> Vec<PeriodicPair> {
        vec![
            PeriodicPair {
                id: "periodic_model_y".into(),
                lower_tag: 1,
                upper_tag: 2,
                translation: [1.0, 0.0, 0.0],
                phase_vector: k,
            },
            PeriodicPair {
                id: "periodic_model_z".into(),
                lower_tag: 3,
                upper_tag: 4,
                translation: [0.0, 1.0, 0.0],
                phase_vector: k,
            },
        ]
    }

    fn assert_complex_close(actual: C, expected: C, tolerance: f64) {
        assert!(
            (actual.re - expected.re).hypot(actual.im - expected.im) < tolerance,
            "actual=({},{}), expected=({},{})",
            actual.re,
            actual.im,
            expected.re,
            expected.im
        );
    }

    #[test]
    fn floquet_edge_and_second_order_face_traces_reconstruct_complex_phase() {
        let mesh = floquet_cube_mesh();
        let pairs = floquet_pairs([0.43, -0.27, 0.0]);
        for order in [1, 2] {
            let dofs = find_dofs(&mesh, order, &pairs).unwrap();
            assert!(dofs.count < dofs.reduced.len());
            assert!(dofs.weights.iter().any(|weight| weight.im.abs() > 0.01));
            let reduced_re = (0..2 * dofs.count)
                .map(|index| (index as f64 * 0.37 + 0.2).sin())
                .collect::<Vec<_>>();
            let reduced_im = (0..2 * dofs.count)
                .map(|index| (index as f64 * 0.19 - 0.3).cos())
                .collect::<Vec<_>>();
            let (real, imaginary) =
                expand_periodic_solution(&reduced_re, &reduced_im, &dofs).unwrap();
            let candidates = if order == 2 {
                quadratic_candidates()
            } else {
                Vec::new()
            };
            let locals = mesh
                .tets
                .iter()
                .enumerate()
                .map(|(element, &tet)| {
                    let geo = geometry(points_for(tet, &mesh.nodes)).unwrap();
                    Local {
                        geo,
                        grad: grad_lambda(geo),
                        dofs: dofs.elements[element].clone(),
                        order,
                        inv_m: if order == 2 {
                            Some(second_interpolation(geo, tet, &mesh.nodes, &candidates).unwrap())
                        } else {
                            None
                        },
                    }
                })
                .collect::<Vec<_>>();
            let sample = |element: usize, point: V3, field: usize| {
                let local = &locals[element];
                let basis = bases_at(
                    order,
                    local.geo,
                    local.grad,
                    mesh.tets[element],
                    &mesh.nodes,
                    local.inv_m.as_ref(),
                    &candidates,
                    point,
                );
                let mut value = [C::default(); 3];
                for (index, b) in basis.iter().enumerate() {
                    let dof = field * dofs.reduced.len() + local.dofs[index];
                    for axis in 0..3 {
                        value[axis] = c_add(
                            value[axis],
                            c_scale(
                                C {
                                    re: real[dof],
                                    im: imaginary[dof],
                                },
                                b.value[axis],
                            ),
                        );
                    }
                }
                value
            };
            for pair in &pairs {
                let node_map = periodic_node_map(&mesh, pair, 1e-10).unwrap();
                let factor = floquet_factor(pair.phase_vector, pair.translation).unwrap();
                let normal = pair
                    .translation
                    .iter()
                    .position(|component| component.abs() > 0.5)
                    .unwrap();
                for lower in mesh
                    .boundary
                    .iter()
                    .filter(|face| face.tag == pair.lower_tag)
                {
                    let key = entity_key(lower.nodes.iter().map(|node| node_map[node]).collect());
                    let upper = mesh
                        .boundary
                        .iter()
                        .find(|face| {
                            face.tag == pair.upper_tag && entity_key(face.nodes.to_vec()) == key
                        })
                        .unwrap();
                    let point = add(
                        add(
                            scale(mesh.nodes[lower.nodes[0]], 0.2),
                            scale(mesh.nodes[lower.nodes[1]], 0.3),
                        ),
                        scale(mesh.nodes[lower.nodes[2]], 0.5),
                    );
                    for field in 0..2 {
                        let lower_value = sample(lower.element, point, field);
                        let upper_value =
                            sample(upper.element, add(point, pair.translation), field);
                        for axis in 0..3 {
                            if axis != normal {
                                assert_complex_close(
                                    upper_value[axis],
                                    c_mul(factor, lower_value[axis]),
                                    1e-8,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn floquet_reduction_uses_the_conjugate_test_space_and_reconstructs_both_fields() {
        let dofs = PeriodicDofs {
            elements: Vec::new(),
            reduced: vec![0, 1, 0],
            weights: vec![
                C { re: 1.0, im: 0.0 },
                C { re: 1.0, im: 0.0 },
                C { re: 0.0, im: -1.0 },
            ],
            count: 2,
        };
        let mut matrix = vec![BTreeMap::new(); 6];
        for row in 0..6 {
            for column in 0..6 {
                matrix[row].insert(
                    column,
                    C {
                        re: (row + 2 * column + 1) as f64,
                        im: (2 * row + column) as f64 * 0.1,
                    },
                );
            }
        }
        let rhs_re = vec![0.3; 6];
        let rhs_im = vec![-0.2; 6];
        let (reduced, reduced_re, reduced_im) =
            reduce_periodic_system(&matrix, &rhs_re, &rhs_im, &dofs).unwrap();
        let x = vec![0.2, -0.4, 0.7, 0.1];
        let xi = vec![0.3, 0.1, -0.2, 0.6];
        let (full, full_i) = expand_periodic_solution(&x, &xi, &dofs).unwrap();
        let mut original_energy = C::default();
        let mut reduced_energy = C::default();
        let mut original_load = C::default();
        let mut reduced_load = C::default();
        for row in 0..6 {
            let test = c_conj(C {
                re: full[row],
                im: full_i[row],
            });
            original_load = c_add(
                original_load,
                c_mul(
                    test,
                    C {
                        re: rhs_re[row],
                        im: rhs_im[row],
                    },
                ),
            );
            for (&column, &value) in &matrix[row] {
                original_energy = c_add(
                    original_energy,
                    c_mul(
                        c_mul(test, value),
                        C {
                            re: full[column],
                            im: full_i[column],
                        },
                    ),
                );
            }
        }
        for row in 0..4 {
            let test = c_conj(C {
                re: x[row],
                im: xi[row],
            });
            reduced_load = c_add(
                reduced_load,
                c_mul(
                    test,
                    C {
                        re: reduced_re[row],
                        im: reduced_im[row],
                    },
                ),
            );
            for (&column, &value) in &reduced[row] {
                reduced_energy = c_add(
                    reduced_energy,
                    c_mul(
                        c_mul(test, value),
                        C {
                            re: x[column],
                            im: xi[column],
                        },
                    ),
                );
            }
        }
        assert_complex_close(reduced_energy, original_energy, 1e-10);
        assert_complex_close(reduced_load, original_load, 1e-12);
        assert_complex_close(
            C {
                re: full[5],
                im: full_i[5],
            },
            c_mul(
                C { re: 0.0, im: -1.0 },
                C {
                    re: x[2],
                    im: xi[2],
                },
            ),
            1e-12,
        );
    }

    #[test]
    fn rust_integration_operator_parser_uses_declared_boundary_and_domain_capabilities() {
        let capabilities: Value = crate::json_schema::engine_capabilities().unwrap();
        let selections = HashMap::from([("input-plane".to_string(), 7usize)]);
        let domain_groups = HashMap::from([("copper-domains".to_string(), 8usize)]);
        let edge_groups = HashMap::new();
        let configuration = json!({
            "integrationOperators": [{
                "id": "intop_in",
                "integrationKind": "boundaryIntegration",
                "selectionId": "input-plane"
            }]
        });
        let operators = parse_integration_operators(
            &configuration,
            &capabilities,
            &selections,
            &domain_groups,
            &edge_groups,
            &[],
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(operators.len(), 1);
        assert_eq!(operators[0].id, "intop_in");
        assert_eq!(operators[0].physical_tag, 7);

        let domain_configuration = json!({
            "integrationOperators": [{
                "id": "intop_volume",
                "integrationKind": "domainIntegration",
                "selectionId": "copper-domains"
            }]
        });
        let operators = parse_integration_operators(
            &domain_configuration,
            &capabilities,
            &selections,
            &domain_groups,
            &edge_groups,
            &[],
            &HashMap::new(),
        )
        .unwrap();
        assert_eq!(operators.len(), 1);
        assert_eq!(operators[0].kind, "domainIntegration");
        assert_eq!(operators[0].physical_tag, 8);
    }

    #[test]
    fn field_component_labels_follow_global_model_axes() {
        let declarations =
            crate::model_schema::field_variables_for_profile("em-3d", Some("electric"), false)
                .expect("EM3D field declarations must load from the model schema.");
        for (internal_axis, model_axis) in INTERNAL_COORDINATE_ORDER.iter().enumerate() {
            assert_eq!(internal_axis_to_model(internal_axis).unwrap(), *model_axis);
            assert!(
                crate::model_schema::field_symbol_for_solver_component(
                    &declarations,
                    "electric",
                    Some(model_axis)
                )
                .is_some()
            );
            assert!(
                crate::model_schema::field_symbol_for_solver_component(
                    &declarations,
                    "magnetic",
                    Some(model_axis)
                )
                .is_some()
            );
        }
    }

    #[test]
    fn expression_functions_follow_the_shared_catalog_and_cannot_be_shadowed() {
        let capabilities: Value = crate::json_schema::engine_capabilities().unwrap();
        let function_arities = capabilities
            .pointer("/expressions/functionArities")
            .unwrap();
        let no_operators = BTreeSet::new();
        let mut unsupported_integration_arity = capabilities.clone();
        unsupported_integration_arity["expressions"]["integrationOperatorArgumentCount"] = json!(2);
        let unsupported_arity_error =
            match parse_solve_config(&json!({}), Path::new("."), &unsupported_integration_arity) {
                Err(error) => error,
                Ok(_) => panic!("the evaluator must reject an unsupported integration call arity"),
            };
        assert!(unsupported_arity_error.contains("must be exactly one"));
        let unary = json!({"kind":"call","name":"sin","args":[{"kind":"number","value":0.0}]});
        let integration_operator_argument_count = capabilities
            .pointer("/expressions/integrationOperatorArgumentCount")
            .unwrap()
            .as_u64()
            .unwrap() as usize;
        assert!(
            validate_expression_ast_capabilities(
                &unary,
                function_arities,
                integration_operator_argument_count,
                &no_operators
            )
            .is_ok()
        );

        let unsupported =
            json!({"kind":"call","name":"log2","args":[{"kind":"number","value":2.0}]});
        assert!(
            validate_expression_ast_capabilities(
                &unsupported,
                function_arities,
                integration_operator_argument_count,
                &no_operators
            )
            .unwrap_err()
            .contains("not supported by the engine capability catalog")
        );

        let wrong_arity = json!({"kind":"call","name":"re","args":[{"kind":"number","value":1.0},{"kind":"number","value":2.0}]});
        assert!(
            validate_expression_ast_capabilities(
                &wrong_arity,
                function_arities,
                integration_operator_argument_count,
                &no_operators
            )
            .unwrap_err()
            .contains("expects exactly 1 argument")
        );

        let intop = json!({"kind":"call","name":"intop_in","args":[{"kind":"number","value":1.0}]});
        let integration_operators = BTreeSet::from(["intop_in".to_string()]);
        assert!(
            validate_expression_ast_capabilities(
                &intop,
                function_arities,
                integration_operator_argument_count,
                &integration_operators
            )
            .is_ok()
        );
        let wrong_integration_arity = json!({"kind":"call","name":"intop_in","args":[{"kind":"number","value":1.0},{"kind":"number","value":2.0}]});
        assert!(
            validate_expression_ast_capabilities(
                &wrong_integration_arity,
                function_arities,
                integration_operator_argument_count,
                &integration_operators
            )
            .unwrap_err()
            .contains("expects exactly 1 expression")
        );
        assert!(
            validate_integration_operator_id("abs", function_arities)
                .unwrap_err()
                .contains("conflicts with a built-in expression function")
        );
    }

    #[test]
    fn every_catalogued_global_function_is_implemented_by_the_rust_evaluator() {
        let capabilities: Value = crate::json_schema::engine_capabilities().unwrap();
        let function_arities = capabilities
            .pointer("/expressions/functionArities")
            .unwrap();
        let functions = function_arities.as_object().unwrap();

        for (name, arity) in functions {
            let count = arity.get("minArgs").and_then(Value::as_u64).unwrap() as usize;
            let ast = json!({"kind":"call","name":name,"args":(0..count).map(|_| json!({"kind":"number","value":0.5})).collect::<Vec<_>>()});
            let integration_operator_argument_count = capabilities
                .pointer("/expressions/integrationOperatorArgumentCount")
                .and_then(Value::as_u64)
                .unwrap() as usize;
            validate_expression_ast_capabilities(
                &ast,
                function_arities,
                integration_operator_argument_count,
                &BTreeSet::new(),
            )
            .unwrap_or_else(|error| panic!("catalogued function {name} failed preflight: {error}"));
            let args = (0..count)
                .map(|_| fullmeta_core::pointwise::ComplexValue { re: 0.5, im: 0.0 })
                .collect::<Vec<_>>();
            let value = fullmeta_core::pointwise::evaluate_complex_call(name, &args)
                .unwrap_or_else(|error| {
                    panic!("catalogued function {name} is not implemented: {error}")
                });
            assert!(
                value.re.is_finite() && value.im.is_finite(),
                "catalogued function {name} produced a non-finite value"
            );
        }
    }

    #[test]
    fn solver_requires_complete_model_configuration_inputs() {
        let capabilities: Value = crate::json_schema::engine_capabilities().unwrap();
        let empty_request = json!({});
        let error = match parse_solve_config(&empty_request, Path::new("."), &capabilities) {
            Err(error) => error,
            Ok(_) => panic!("a solver request without modelConfiguration must be rejected"),
        };
        assert!(error.contains("request.modelConfiguration is required"));

        let request = json!({"modelConfiguration": {}, "meshMap": "model-map.json"});
        assert!(
            mesh_file_path(&request, Path::new("."))
                .unwrap_err()
                .contains("request.meshFile must be a non-empty string")
        );
        for key in [
            "outputDirectory",
            "sweepCsv",
            "debugCsv",
            "fieldFilePrefix",
            "solverBackend",
        ] {
            assert!(
                request_string(&request, key)
                    .unwrap_err()
                    .contains(&format!("request.{key} must be a non-empty string"))
            );
        }
        for key in [
            "fieldFormulation",
            "constitutiveQuadrature",
            "solverOrdering",
        ] {
            assert!(
                request_string(&request, key)
                    .unwrap_err()
                    .contains(&format!("request.{key} must be a non-empty string"))
            );
        }
        for key in ["elementOrder", "solverThreads"] {
            assert!(
                request_u64(&request, key)
                    .unwrap_err()
                    .contains(&format!("request.{key} must be an unsigned integer"))
            );
        }
    }

    #[test]
    fn solver_accepts_only_the_current_configuration_schema() {
        let capabilities: Value = crate::json_schema::engine_capabilities().unwrap();
        let current = json!({"schema":"fem-solver-configuration/6", "fieldComponentCoordinateSystem":"model", "fieldFiles":false, "domainSelectionIds":["sel_air"]});
        assert!(validate_solver_configuration(&current, &capabilities).is_ok());
        assert!(!configured_field_files(&current).unwrap());
        assert!(validate_field_component_coordinate_system(&current).is_ok());
        assert_eq!(
            configured_domain_selection_ids(&current).unwrap(),
            vec!["sel_air"]
        );
        assert!(
            configured_field_files(&json!({"schema":"fem-solver-configuration/6"}))
                .unwrap_err()
                .contains("fieldFiles must be a boolean")
        );
        assert!(
            validate_field_component_coordinate_system(
                &json!({"schema":"fem-solver-configuration/6"})
            )
            .unwrap_err()
            .contains("must be 'model'")
        );
        assert!(
            configured_domain_selection_ids(&json!({"schema":"fem-solver-configuration/6"}))
                .unwrap_err()
                .contains("must be an array")
        );
        assert!(
            validate_solver_configuration(&json!({"schema":"unsupported-contract"}), &capabilities)
                .unwrap_err()
                .contains("unsupported")
        );
        let mut mismatched_capabilities = capabilities.clone();
        mismatched_capabilities["study"]["requestConfigurationSchema"] = json!("newer-contract");
        assert!(
            validate_solver_configuration(
                &json!({"schema":"newer-contract"}),
                &mismatched_capabilities
            )
            .unwrap_err()
            .contains("this Rust build implements")
        );
    }

    #[test]
    fn em3d_accepts_current_mesh_maps_and_resolves_boundary_tags_from_the_map() {
        assert!(!supports_em3d_mesh_map(
            Some("fullmeta-mesh-map/3"),
            Some(3)
        ));
        assert!(supports_em3d_mesh_map(Some("fullmeta-mesh-map/6"), Some(3)));
        assert!(!supports_em3d_mesh_map(
            Some("fullmeta-mesh-map/6"),
            Some(2)
        ));
        assert!(!supports_em3d_mesh_map(
            Some("fullmeta-mesh-map/5"),
            Some(3)
        ));
        assert!(!supports_em3d_mesh_map(
            Some("fullmeta-mesh-map/4"),
            Some(2)
        ));

        let selections = HashMap::from([
            ("incident_face".to_string(), 3usize),
            ("periodic_y_min".to_string(), 7usize),
        ]);
        assert_eq!(
            mesh_map_selection_tag(&selections, "incident_face", "inputImpedanceBoundary").unwrap(),
            3
        );
        assert_eq!(
            mesh_map_selection_tag(&selections, "periodic_y_min", "periodic pair y lower face")
                .unwrap(),
            7
        );
        assert!(
            mesh_map_selection_tag(&selections, "missing", "inputImpedanceBoundary")
                .unwrap_err()
                .contains("missing from the mesh map")
        );
    }
}
