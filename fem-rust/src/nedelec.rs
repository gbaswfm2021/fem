//! Oriented first- and second-order Nedelec tetrahedral elements.
//!
//! Coordinates, basis values and moments are independent of any physics scenario.

use crate::{C, TRI_Q, c_add, c_scale};
use crate::simplex::{Geo, V3, add, sub, scale, dot, cross, norm, ref_point, grad_lambda};
use std::collections::HashSet;

pub(crate) const EDGES: [[usize; 2]; 6] = [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]];
pub(crate) const FACES: [[usize; 3]; 4] = [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]];
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

#[derive(Clone, Copy)]
pub(crate) struct Basis {
    pub(crate) value: V3,
    pub(crate) curl: V3,
    a: usize,
    b: usize,
}

/// Local Nedelec moments whose tangential traces belong to a tetrahedral face.
/// The indices follow `find_dofs` and `second_interpolation`: edge moments
/// first, followed by the two moments for each face at order two.
pub(crate) fn face_tangential_moment_indices(
    tet: [usize; 4],
    face: [usize; 3],
    order: u8,
) -> Result<Vec<usize>, String> {
    if order != 1 && order != 2 {
        return Err("Nedelec boundary moments require element order 1 or 2.".into());
    }
    let face_nodes: HashSet<usize> = face.into_iter().collect();
    if face_nodes.len() != 3 || !face_nodes.iter().all(|node| tet.contains(node)) {
        return Err("Boundary face is not a face of its adjacent tetrahedron.".into());
    }
    let modes = usize::from(order);
    let mut result = Vec::with_capacity(if order == 1 { 3 } else { 8 });
    for (edge_index, [a, b]) in EDGES.into_iter().enumerate() {
        if face_nodes.contains(&tet[a]) && face_nodes.contains(&tet[b]) {
            result.extend((0..modes).map(|mode| edge_index * modes + mode));
        }
    }
    if order == 2 {
        let index = FACES
            .iter()
            .position(|local| local.iter().all(|&vertex| face_nodes.contains(&tet[vertex])))
            .ok_or("Boundary face does not match a tetrahedral face.")?;
        result.extend([12 + 2 * index, 13 + 2 * index]);
    }
    Ok(result)
}

/// Interpolate a prescribed Cartesian vector field using the same oriented
/// edge and face functionals as the element basis construction.
pub(crate) fn face_tangential_moments(
    tet: [usize; 4],
    face: [usize; 3],
    nodes: &[V3],
    order: u8,
    mut field: impl FnMut(V3) -> Result<[C; 3], String>,
) -> Result<Vec<(usize, C)>, String> {
    let indices = face_tangential_moment_indices(tet, face, order)?;
    let members: HashSet<usize> = face.into_iter().collect();
    let mut values = Vec::with_capacity(indices.len());
    let edge_q = [
        (0.046910077030668, 0.118463442528095),
        (0.230765344947158, 0.239314335249683),
        (0.5, 0.284444444444444),
        (0.769234655052842, 0.239314335249683),
        (0.953089922969332, 0.118463442528095),
    ];
    for [a, b] in EDGES {
        if !members.contains(&tet[a]) || !members.contains(&tet[b]) {
            continue;
        }
        let canonical = canonical_points(vec![tet[a], tet[b]], nodes);
        let start = nodes[canonical[0]];
        let delta = sub(nodes[canonical[1]], start);
        let length = norm(delta);
        if !length.is_finite() || length <= 0.0 {
            return Err("Prescribed boundary has a degenerate edge.".into());
        }
        let tangent = if order == 1 { delta } else { scale(delta, 1.0 / length) };
        let mut moments = [C::default(); 2];
        for (s, weight) in edge_q {
            let value = field(add(start, scale(delta, s)))?;
            let component = (0..3).fold(C::default(), |sum, axis| c_add(sum, c_scale(value[axis], tangent[axis])));
            moments[0] = c_add(moments[0], c_scale(component, weight));
            if order == 2 {
                moments[1] = c_add(moments[1], c_scale(component, weight * (2.0 * s - 1.0)));
            }
        }
        values.extend(moments.into_iter().take(usize::from(order)));
    }
    if order == 2 {
        let canonical = canonical_points(face.to_vec(), nodes);
        let p = [nodes[canonical[0]], nodes[canonical[1]], nodes[canonical[2]]];
        let d1 = sub(p[1], p[0]);
        let d2 = sub(p[2], p[0]);
        let l1 = norm(d1);
        let l2 = norm(d2);
        if !l1.is_finite() || !l2.is_finite() || l1 <= 0.0 || l2 <= 0.0 {
            return Err("Prescribed boundary has a degenerate face.".into());
        }
        let tangents = [scale(d1, 1.0 / l1), scale(d2, 1.0 / l2)];
        let mut moments = [C::default(); 2];
        for q in TRI_Q {
            let point = add(add(scale(p[0], q[0]), scale(p[1], q[1])), scale(p[2], q[2]));
            let value = field(point)?;
            for (j, tangent) in tangents.iter().enumerate() {
                let component = (0..3).fold(C::default(), |sum, axis| c_add(sum, c_scale(value[axis], tangent[axis])));
                moments[j] = c_add(moments[j], c_scale(component, q[3]));
            }
        }
        values.extend(moments);
    }
    if values.iter().any(|v| !v.re.is_finite() || !v.im.is_finite()) {
        return Err("Prescribed boundary produced a non-finite moment.".into());
    }
    Ok(indices.into_iter().zip(values).collect())
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
pub(crate) type Poly2 = [[f64; 10]; 3];
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
pub(crate) fn quadratic_candidates() -> Vec<Poly2> {
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
pub(crate) fn canonical_points(mut ids: Vec<usize>, nodes: &[V3]) -> Vec<usize> {
    ids.sort_by(|&a, &b| coordinate_cmp(nodes[a], nodes[b]).then_with(|| a.cmp(&b)));
    ids
}
pub(crate) fn entity_key(mut ids: Vec<usize>) -> Vec<usize> {
    ids.sort_unstable();
    ids
}
pub(crate) fn second_interpolation(
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
pub(crate) fn bases_at(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simplex::{geometry, points_for};

    #[test]
    fn first_order_basis_has_unit_canonical_edge_integrals() {
        let nodes = [[0., 0., 0.], [2., 0., 0.], [0., 3., 0.], [0., 0., 4.]];
        for tet in [[0, 1, 2, 3], [1, 0, 3, 2], [3, 1, 2, 0]] {
            let geo = geometry(points_for(tet, &nodes)).unwrap();
            let gradients = grad_lambda(geo);
            for (edge_index, [a, b]) in EDGES.into_iter().enumerate() {
                let endpoints = canonical_points(vec![tet[a], tet[b]], &nodes);
                let tangent = sub(nodes[endpoints[1]], nodes[endpoints[0]]);
                let midpoint = add(nodes[endpoints[0]], scale(tangent, 0.5));
                let basis = bases_at(1, geo, gradients, tet, &nodes, None, &[], midpoint);
                for (i, function) in basis.iter().enumerate() {
                    let integral = dot(function.value, tangent);
                    let expected = if i == edge_index { 1.0 } else { 0.0 };
                    assert!((integral - expected).abs() < 1e-12,
                        "tet {tet:?}, edge {edge_index}, function {i}: {integral}");
                }
            }
        }
    }

    #[test]
    fn face_moment_functionals_are_dual_to_both_orders_of_basis() {
        let nodes = [[0., 0., 0.], [2., 0., 0.], [0., 3., 0.], [0., 0., 4.]];
        for tet in [[0, 1, 2, 3], [1, 0, 3, 2]] {
            let geo = geometry(points_for(tet, &nodes)).unwrap();
            let gradients = grad_lambda(geo);
            for order in [1, 2] {
                let candidates = if order == 2 { quadratic_candidates() } else { Vec::new() };
                let inverse = if order == 2 {
                    Some(second_interpolation(geo, tet, &nodes, &candidates).unwrap())
                } else { None };
                for face in FACES {
                    let face = face.map(|i| tet[i]);
                    let indices = face_tangential_moment_indices(tet, face, order).unwrap();
                    for &dof in &indices {
                        let moments = face_tangential_moments(tet, face, &nodes, order, |point| {
                            let basis = bases_at(order, geo, gradients, tet, &nodes,
                                inverse.as_ref(), &candidates, point);
                            Ok(basis[dof].value.map(|re| C { re, im: 0.0 }))
                        }).unwrap();
                        for (index, value) in moments {
                            let expected = if index == dof { 1.0 } else { 0.0 };
                            assert!((value.re - expected).abs() < 1e-9 && value.im == 0.0,
                                "tet {tet:?}, order {order}, moment {index}, basis {dof}: {}", value.re);
                        }
                    }
                }
            }
        }
    }
}
