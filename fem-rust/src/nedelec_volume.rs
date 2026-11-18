//! Quadrature and sparse scattering for oriented Nedelec volume forms.
//!
//! The element library supplies physical basis values and curls, already
//! oriented consistently with the global moment degrees of freedom. This
//! adapter neither chooses the PDE coefficients nor applies orientation or
//! periodic phases a second time. Boundary contributions may be added to the
//! same local matrix before scattering.

#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::HashMap;
use crate::assembler::SparseMatrixSink;
use crate::C;
use crate::nedelec::Basis;
use crate::weak_form::{BasisSample, Coefficient, FormPlan};

pub(crate) fn accumulate_quadrature(
    plan: &FormPlan,
    basis: &[Basis],
    coefficients: &HashMap<String, Coefficient<'_>>,
    weight: f64,
    local: &mut [Vec<C>],
) -> Result<(), String> {
    let samples = basis.iter().map(|basis| BasisSample {
        value: &basis.value,
        gradient: &[],
        curl: &basis.curl,
    }).collect::<Vec<_>>();
    plan.accumulate(&samples, coefficients, weight, local)
}

/// Add the whole local matrix, including any facet terms, to sparse rows.
/// Validate indexing before mutating the global matrix.
pub(crate) fn scatter_element<M: SparseMatrixSink + ?Sized>(
    matrix: &mut M,
    dofs: &[usize],
    local: &[Vec<C>],
) -> Result<(), String> {
    let n = dofs.len();
    if local.len() != n || local.iter().any(|row| row.len() != n)
        || dofs.iter().any(|&dof| dof >= matrix.dimension()) {
        return Err("Invalid Nedelec element matrix or global degree of freedom.".into());
    }
    crate::assembler::scatter_local(matrix, dofs, local)
}

pub(crate) fn scatter_element_at<M: SparseMatrixSink + ?Sized>(
    matrix: &mut M,
    element: usize,
    dofs: &[usize],
    local: &[Vec<C>],
) -> Result<(), String> {
    let n = dofs.len();
    if local.len() != n || local.iter().any(|row| row.len() != n)
        || dofs.iter().any(|&dof| dof >= matrix.dimension()) {
        return Err("Invalid Nedelec element matrix or global degree of freedom.".into());
    }
    matrix.scatter_element(element, dofs, local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TET_Q, simplex::{geometry, grad_lambda, points_for}};
    use crate::nedelec::{EDGES, FACES, bases_at, entity_key, quadratic_candidates, second_interpolation};
    use serde_json::json;

    fn local_keys(tet: [usize; 4], order: u8) -> Vec<(Vec<usize>, usize)> {
        let mut keys = Vec::new();
        for [a, b] in EDGES {
            for mode in 0..usize::from(order) {
                keys.push((entity_key(vec![tet[a], tet[b]]), mode));
            }
        }
        if order == 2 {
            for face in FACES {
                for mode in 0..2 {
                    keys.push((entity_key(face.map(|i| tet[i]).to_vec()), mode));
                }
            }
        }
        keys
    }

    #[test]
    fn both_orders_assemble_oriented_complex_forms_and_sparse_rows() {
        let plan = FormPlan::from_json(&json!({"dimension":3,"fieldComponents":3,"terms":[
            {"test":"curl","trial":"curl","coefficient":"a"},
            {"test":"value","trial":"value","coefficient":"b"}
        ]})).unwrap();
        let coefficients = HashMap::from([
            ("a".into(), Coefficient::Scalar(C { re: 2., im: -0.5 })),
            ("b".into(), Coefficient::Scalar(C { re: -3., im: 4. })),
        ]);
        let nodes = [[0.,0.,0.], [2.,0.,0.], [0.,3.,0.], [0.,0.,4.]];
        for order in [1, 2] {
            let reference_keys = local_keys([0,1,2,3], order);
            let n = reference_keys.len();
            let mut reference = None;
            for tet in [[0,1,2,3], [1,0,3,2], [3,1,2,0]] {
                let pts = points_for(tet, &nodes);
                let geo = geometry(pts).unwrap();
                let grad = grad_lambda(geo);
                let candidates = if order == 2 { quadratic_candidates() } else { Vec::new() };
                let inverse = if order == 2 { Some(second_interpolation(geo, tet, &nodes, &candidates).unwrap()) } else { None };
                let mut local = vec![vec![C::default(); n]; n];
                let mut expected = vec![vec![C::default(); n]; n];
                for q in TET_Q {
                    let point = std::array::from_fn(|axis| (0..4).map(|i| pts[i][axis] * q[i]).sum());
                    let basis = bases_at(order, geo, grad, tet, &nodes, inverse.as_ref(), &candidates, point);
                    let weight = geo.det.abs() * q[4];
                    accumulate_quadrature(&plan, &basis, &coefficients, weight, &mut local).unwrap();
                    for i in 0..n {
                        for j in 0..n {
                            let curl: f64 = (0..3).map(|k| basis[i].curl[k] * basis[j].curl[k]).sum();
                            let value: f64 = (0..3).map(|k| basis[i].value[k] * basis[j].value[k]).sum();
                            expected[i][j].re += weight * (2. * curl - 3. * value);
                            expected[i][j].im += weight * (-0.5 * curl + 4. * value);
                        }
                    }
                }
                let keys = local_keys(tet, order);
                let dofs = keys.iter().map(|key| reference_keys.iter().position(|other| other == key).unwrap()).collect::<Vec<_>>();
                let mut sparse = vec![BTreeMap::new(); n];
                scatter_element(&mut sparse, &dofs, &local).unwrap();
                for i in 0..n {
                    for j in 0..n {
                        assert!((local[i][j].re - expected[i][j].re).abs() < 1e-9);
                        assert!((local[i][j].im - expected[i][j].im).abs() < 1e-9);
                    }
                }
                if let Some(ref first) = reference {
                    let first: &Vec<BTreeMap<usize, C>> = first;
                    for i in 0..n { for j in 0..n {
                        assert!((sparse[i][&j].re - first[i][&j].re).abs() < 1e-8, "order {order}, entry {i},{j}");
                        assert!((sparse[i][&j].im - first[i][&j].im).abs() < 1e-8, "order {order}, entry {i},{j}");
                    }}
                } else { reference = Some(sparse); }
            }
        }
    }

    #[test]
    fn invalid_scatter_does_not_modify_sparse_matrix() {
        let mut matrix = vec![BTreeMap::from([(0, C { re: 3., im: 1. })])];
        assert!(scatter_element(&mut matrix, &[0, 1], &vec![vec![C::default(); 2]; 2]).is_err());
        assert_eq!(matrix[0][&0].re, 3.);
        assert_eq!(matrix[0][&0].im, 1.);
        assert!(scatter_element(&mut matrix, &[0], &[vec![C {re:f64::NAN,im:0.}]]).is_err());
        assert_eq!(matrix[0][&0].re, 3.);
    }
}
