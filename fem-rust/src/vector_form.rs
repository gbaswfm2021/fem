//! Generic vector-valued P1 volume forms on simplices.
//!
//! A quadrature point contributes `sym(grad(v)):C:sym(grad(u))` and
//! `m v·u`. `C` is a fourth-order tensor in Cartesian coordinates; no Voigt
//! or engineering-shear convention is used here. Geometry, coefficient
//! sampling, quadrature choice, boundary terms and constraints remain the
//! caller's responsibility during migration to the shared form engine.

#[cfg(test)]
use crate::c_scale;
use crate::{C, c_add};
use crate::assembler::SparseMatrixSink;
#[cfg(test)]
use std::collections::BTreeMap;
use std::collections::HashMap;

pub(crate) type Tensor4<const D: usize> = [[[[C; D]; D]; D]; D];

/// Flatten a Cartesian fourth-order tensor in `(ij, kl)` row-major order for
/// the generic weak-form coefficient interface.
pub(crate) fn flatten_tensor<const D: usize>(tensor: &Tensor4<D>) -> Vec<C> {
    let mut result = Vec::with_capacity(D * D * D * D);
    for i in 0..D {
        for j in 0..D {
            for k in 0..D {
                for l in 0..D {
                    result.push(tensor[i][j][k][l]);
                }
            }
        }
    }
    result
}

/// Evaluate the declared weak-form terms for one vector P1 quadrature point
/// and scatter the element matrix into the global sparse row maps. Form
/// validation occurs before scattering, so a failed sample leaves the global
/// matrix unchanged. Node-major ordering matches the existing vector DOFs.
pub(crate) fn add_declared_p1_quadrature<const D: usize, const N: usize, M: SparseMatrixSink + ?Sized>(
    plan: &crate::weak_form::FormPlan,
    matrix: &mut M,
    nodes: [usize; N],
    gradients: [[f64; D]; N],
    values: [f64; N],
    weight: f64,
    coefficients: &HashMap<String, crate::weak_form::Coefficient<'_>>,
) -> Result<(), String> {
    add_declared_p1_quadrature_with_space::<D,D,N,M>(plan,matrix,nodes,gradients,values,weight,coefficients)
}

/// Assemble a Cartesian vector field with `C` components on a mesh embedded
/// in `D` dimensions. Keeping these counts separate supports cross-sectional
/// vector fields with a declared higher-dimensional differential operator.
pub(crate) fn add_declared_p1_quadrature_with_space<const D: usize, const C: usize, const N: usize, M: SparseMatrixSink + ?Sized>(
    plan:&crate::weak_form::FormPlan,
    matrix:&mut M,
    nodes:[usize;N],
    gradients:[[f64;D];N],
    values:[f64;N],
    weight:f64,
    coefficients:&HashMap<String,crate::weak_form::Coefficient<'_>>,
)->Result<(),String>{
    if !(2..=3).contains(&D)||!(1..=3).contains(&C)||N!=D+1 {
        return Err("Vector P1 assembly requires a 2D/3D simplex and one to three field components.".into());
    }
    let n=N*C;
    let basis_values=(0..N).flat_map(|a|(0..C).map(move |component|(a,component)))
        .map(|(a,component)|{let mut value=vec![0.;C];value[component]=values[a];value}).collect::<Vec<_>>();
    let basis_gradients=(0..N).flat_map(|a|(0..C).map(move |component|(a,component)))
        .map(|(a,component)|{let mut gradient=vec![0.;C*D];for axis in 0..D{gradient[component*D+axis]=gradients[a][axis];}gradient}).collect::<Vec<_>>();
    let basis=(0..n).map(|i|crate::weak_form::BasisSample{value:&basis_values[i],gradient:&basis_gradients[i],curl:&[]}).collect::<Vec<_>>();
    let mut local=vec![vec![C::default();n];n];
    plan.accumulate(&basis,coefficients,weight,&mut local)?;
    for a in 0..N { for i in 0..C { for b in 0..N { for j in 0..C {
        matrix.add_entry(C*nodes[a]+i,C*nodes[b]+j,local[C*a+i][C*b+j])?;
    }}}}
    Ok(())
}

/// Integrate a declared vector P1 facet load into sparse-system RHS arrays.
pub(crate) fn add_declared_p1_facet_load<const D: usize, const N: usize>(
    plan: &crate::boundary_form::BoundaryFormPlan,
    nodes: [usize; N],
    shapes: [f64; N],
    normal: [f64; D],
    coefficients: &HashMap<String, C>,
    source_name: &str,
    source: Vec<C>,
    weight: f64,
    rhs_re: &mut [f64],
    rhs_im: &mut [f64],
) -> Result<(), String> {
    let dofs = N * D;
    if rhs_re.len() != rhs_im.len()
        || nodes.iter().any(|&node| {
            node.checked_mul(D)
                .and_then(|start| start.checked_add(D))
                .is_none_or(|end| end > rhs_re.len())
        })
    {
        return Err("Vector boundary load references an invalid global degree of freedom.".into());
    }
    let mut values = [[[0.0; D]; D]; N];
    for a in 0..N {
        for component in 0..D {
            values[a][component][component] = shapes[a];
        }
    }
    let mut basis = Vec::with_capacity(dofs);
    for a in 0..N {
        for component in 0..D {
            basis.push(values[a][component].as_slice());
        }
    }
    let sources = HashMap::from([(source_name.to_string(), source)]);
    let mut local = vec![C::default(); dofs];
    plan.accumulate_load(&basis, &normal, coefficients, &sources, weight, &mut local)?;
    for a in 0..N {
        for component in 0..D {
            let value = local[D * a + component];
            rhs_re[D * nodes[a] + component] += value.re;
            rhs_im[D * nodes[a] + component] += value.im;
        }
    }
    Ok(())
}

/// Integrate a declared vector P1 facet bilinear form into sparse row maps.
pub(crate) fn add_declared_p1_facet_matrix<const D: usize, const N: usize, M: SparseMatrixSink + ?Sized>(
    plan: &crate::boundary_form::BoundaryFormPlan,
    matrix: &mut M,
    nodes: [usize; N],
    shapes: [f64; N],
    normal: [f64; D],
    coefficients: &HashMap<String, C>,
    weight: f64,
) -> Result<(), String> {
    let dofs = N * D;
    if nodes.iter().any(|&node| {
        node.checked_mul(D)
            .and_then(|start| start.checked_add(D))
            .is_none_or(|end| end > matrix.dimension())
    }) {
        return Err(
            "Vector boundary matrix references an invalid global degree of freedom.".into(),
        );
    }
    let mut values = [[[0.0; D]; D]; N];
    for a in 0..N {
        for component in 0..D {
            values[a][component][component] = shapes[a];
        }
    }
    let mut basis = Vec::with_capacity(dofs);
    for a in 0..N {
        for component in 0..D {
            basis.push(values[a][component].as_slice());
        }
    }
    let mut local = vec![vec![C::default(); dofs]; dofs];
    plan.accumulate_matrix(&basis, &normal, coefficients, weight, &mut local)?;
    for a in 0..N {
        for i in 0..D {
            for b in 0..N {
                for j in 0..D {
                    matrix.add_entry(
                        D * nodes[a] + i,
                        D * nodes[b] + j,
                        local[D * a + i][D * b + j],
                    )?;
                }
            }
        }
    }
    Ok(())
}

/// Construct `lambda I⊗I + mu (I_ik I_jl + I_il I_jk)` without assuming a
/// particular physical scenario. The general assembler also accepts any
/// explicitly supplied fourth-order tensor, including complex coefficients.
pub(crate) fn isotropic_tensor<const D: usize>(lambda: C, mu: C) -> Tensor4<D> {
    let mut tensor = [[[[C::default(); D]; D]; D]; D];
    for i in 0..D {
        for j in 0..D {
            for k in 0..D {
                for l in 0..D {
                    let mut value = C::default();
                    if i == j && k == l {
                        value = c_add(value, lambda);
                    }
                    if i == k && j == l {
                        value = c_add(value, mu);
                    }
                    if i == l && j == k {
                        value = c_add(value, mu);
                    }
                    tensor[i][j][k][l] = value;
                }
            }
        }
    }
    tensor
}

/// Accumulate one quadrature point into a sparse row-map matrix. The caller
/// supplies the full geometric integration weight, P1 basis values and physical
/// basis gradients. DOFs are node-major, one Cartesian component per axis.
///
/// Test basis functions are real, so their complex conjugates are unchanged.
/// `mass` is the complete coefficient of `v·u`, including its physical sign
/// and any complex loss term; this routine does not infer a time convention.
#[cfg(test)]
pub(crate) fn add_p1_quadrature<const D: usize, const N: usize>(
    matrix: &mut [BTreeMap<usize, C>],
    nodes: [usize; N],
    gradients: [[f64; D]; N],
    values: [f64; N],
    weight: f64,
    tensor: &Tensor4<D>,
    mass: C,
) {
    // Apply the tensor to each symmetric trial gradient once, then contract
    // against each symmetric test gradient. C may lack either minor symmetry.
    let mut stresses = [[[[C::default(); D]; D]; D]; N];
    for b in 0..N {
        for j in 0..D {
            for k in 0..D {
                for l in 0..D {
                    let mut stress = C::default();
                    for m in 0..D {
                        let coefficient = c_add(tensor[k][l][j][m], tensor[k][l][m][j]);
                        stress = c_add(stress, c_scale(coefficient, 0.5 * gradients[b][m]));
                    }
                    stresses[b][j][k][l] = stress;
                }
            }
        }
    }
    for a in 0..N {
        for b in 0..N {
            for i in 0..D {
                for j in 0..D {
                    let mut stiffness = C::default();
                    for m in 0..D {
                        let stress = c_add(stresses[b][j][i][m], stresses[b][j][m][i]);
                        stiffness = c_add(stiffness, c_scale(stress, 0.5 * gradients[a][m]));
                    }
                    let mut value = c_scale(stiffness, weight);
                    if i == j {
                        value = c_add(value, c_scale(mass, weight * values[a] * values[b]));
                    }
                    let entry = matrix[D * nodes[a] + i]
                        .entry(D * nodes[b] + j)
                        .or_default();
                    *entry = c_add(*entry, value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: C, re: f64, im: f64) {
        assert!((actual.re - re).abs() < 1e-12, "{actual:?} != {re} + {im}i");
        assert!((actual.im - im).abs() < 1e-12, "{actual:?} != {re} + {im}i");
    }

    #[test]
    fn isotropic_form_matches_independent_tensor_formula_in_two_and_three_dimensions() {
        fn check<const D: usize, const N: usize>(grad: [[f64; D]; N], shape: [f64; N]) {
            let lambda = 2.4;
            let mu = 1.3;
            let weight = 0.7;
            let mass = C { re: -2.1, im: 0.3 };
            let tensor = isotropic_tensor::<D>(
                C {
                    re: lambda,
                    im: 0.0,
                },
                C { re: mu, im: 0.0 },
            );
            let mut matrix = vec![BTreeMap::new(); N * D];
            let nodes = std::array::from_fn(|i| i);
            add_p1_quadrature(&mut matrix, nodes, grad, shape, weight, &tensor, mass);
            for a in 0..N {
                for b in 0..N {
                    let dot: f64 = (0..D).map(|d| grad[a][d] * grad[b][d]).sum();
                    for i in 0..D {
                        for j in 0..D {
                            let stiffness = lambda * grad[a][i] * grad[b][j]
                                + mu * (grad[a][j] * grad[b][i] + if i == j { dot } else { 0.0 });
                            let m = if i == j { shape[a] * shape[b] } else { 0.0 };
                            close(
                                matrix[D * a + i][&(D * b + j)],
                                weight * (stiffness + mass.re * m),
                                weight * mass.im * m,
                            );
                        }
                    }
                }
            }
        }
        check([[-1.0, -1.0], [1.0, 0.0], [0.0, 1.0]], [0.2, 0.3, 0.5]);
        check(
            [
                [-1.0, -1.0, -1.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
            ],
            [0.1, 0.2, 0.3, 0.4],
        );
    }

    #[test]
    fn arbitrary_complex_tensor_contracts_both_symmetric_gradients() {
        let mut tensor = [[[[C::default(); 2]; 2]; 2]; 2];
        // Single nonzero C_xyyx: each symmetrization contributes one half.
        tensor[0][1][1][0] = C { re: 4.0, im: -2.0 };
        let mut matrix = vec![BTreeMap::new(); 2];
        add_p1_quadrature(
            &mut matrix,
            [0],
            [[0.0, 1.0]],
            [1.0],
            2.0,
            &tensor,
            C::default(),
        );
        close(matrix[0][&0], 2.0, -1.0);
    }

    #[test]
    fn declared_terms_control_sparse_p1_assembly() {
        use crate::weak_form::{Coefficient, FormPlan};
        use serde_json::json;
        let plan = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":2,"terms":[
            {"test":"symmetricGradient","trial":"symmetricGradient","coefficient":"C"},
            {"test":"value","trial":"value","coefficient":"mass"}
        ]}))
        .unwrap();
        let mass_only = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":2,"terms":[
            {"test":"value","trial":"value","coefficient":"mass"}
        ]}))
        .unwrap();
        let tensor = isotropic_tensor::<2>(C { re: 3.0, im: 0.0 }, C { re: 4.0, im: 0.0 });
        let flat = flatten_tensor(&tensor);
        let mass = C { re: -2.0, im: 0.2 };
        let coefficients = HashMap::from([
            (
                "C".to_owned(),
                Coefficient::Matrix {
                    rows: 4,
                    columns: 4,
                    values: &flat,
                },
            ),
            ("mass".to_owned(), Coefficient::Scalar(mass)),
        ]);
        let nodes = [0, 1, 2];
        let gradients = [[-1.0, -1.0], [1.0, 0.0], [0.0, 1.0]];
        let values = [0.2, 0.3, 0.5];
        let mut actual = vec![BTreeMap::new(); 6];
        add_declared_p1_quadrature(
            &plan,
            &mut actual,
            nodes,
            gradients,
            values,
            0.5,
            &coefficients,
        )
        .unwrap();
        let mut independent = vec![BTreeMap::new(); 6];
        add_p1_quadrature(
            &mut independent,
            nodes,
            gradients,
            values,
            0.5,
            &tensor,
            mass,
        );
        for i in 0..6 {
            for j in 0..6 {
                let reference = independent[i][&j];
                close(actual[i][&j], reference.re, reference.im);
            }
        }
        let mut no_stiffness = vec![BTreeMap::new(); 6];
        add_declared_p1_quadrature(
            &mass_only,
            &mut no_stiffness,
            nodes,
            gradients,
            values,
            0.5,
            &coefficients,
        )
        .unwrap();
        assert!((actual[0][&0].re - no_stiffness[0][&0].re).abs() > 1.0);
        close(no_stiffness[0][&0], -0.04, 0.004);
    }
}
