//! Generic weighted degree-of-freedom reduction and expansion.
//!
//! A raw degree of freedom is related to its representative by `u_i =
//! weight_i * u_hat[representative_i]`. This covers identified periodic DOFs
//! with complex phases or orientation factors. Affine offsets and general
//! multipoint constraints require a separate representation.

use crate::solver_common::{C, c_add, c_conj, c_mul};
use std::collections::BTreeMap;

pub(crate) type SparseSystem = (Vec<BTreeMap<usize, C>>, Vec<f64>, Vec<f64>);

/// Apply `A_hat = P^H A P` and `b_hat = P^H b` without materializing `P`.
/// The same map may be reused for consecutive independent fields.
pub(crate) fn reduce_weighted(
    matrix: &[BTreeMap<usize, C>],
    rhs_re: &[f64],
    rhs_im: &[f64],
    representative: &[usize],
    weights: &[C],
    count: usize,
) -> Result<SparseSystem, String> {
    let full = representative.len();
    if full == 0
        || count == 0
        || weights.len() != full
        || matrix.len() % full != 0
        || rhs_re.len() != matrix.len()
        || rhs_im.len() != matrix.len()
        || representative.iter().any(|&index| index >= count)
        || weights
            .iter()
            .any(|v| !v.re.is_finite() || !v.im.is_finite())
    {
        return Err(
            "Weighted degree-of-freedom mapping does not match the assembled system.".into(),
        );
    }
    let fields = matrix.len() / full;
    let mut reduced = vec![BTreeMap::<usize, C>::new(); fields * count];
    let mut re = vec![0.0; reduced.len()];
    let mut im = vec![0.0; reduced.len()];
    for (old_row, entries) in matrix.iter().enumerate() {
        let row = (old_row / full) * count + representative[old_row % full];
        let row_weight = c_conj(weights[old_row % full]);
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
                return Err("Matrix column is outside the weighted DOF mapping.".into());
            }
            let column = (old_column / full) * count + representative[old_column % full];
            let contribution = c_mul(c_mul(row_weight, value), weights[old_column % full]);
            let entry = reduced[row].entry(column).or_default();
            *entry = c_add(*entry, contribution);
        }
    }
    Ok((reduced, re, im))
}

/// Reconstruct raw DOFs for the same independent-field layout.
pub(crate) fn expand_weighted(
    real: &[f64],
    imaginary: &[f64],
    representative: &[usize],
    weights: &[C],
    count: usize,
) -> Result<(Vec<f64>, Vec<f64>), String> {
    if count == 0
        || real.len() != imaginary.len()
        || real.len() % count != 0
        || representative.len() != weights.len()
        || representative.iter().any(|&index| index >= count)
        || weights
            .iter()
            .any(|v| !v.re.is_finite() || !v.im.is_finite())
    {
        return Err("Solved vector does not match the weighted DOF mapping.".into());
    }
    let mut re = Vec::with_capacity(real.len() / count * representative.len());
    let mut im = Vec::with_capacity(re.capacity());
    for field in 0..real.len() / count {
        for (raw, &reduced) in representative.iter().enumerate() {
            let index = field * count + reduced;
            let value = c_mul(
                weights[raw],
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complex_weighted_reduction_uses_conjugate_test_weights() {
        let mut matrix = vec![BTreeMap::new(); 2];
        matrix[0].insert(0, C { re: 2.0, im: 0.0 });
        matrix[0].insert(1, C { re: 1.0, im: 0.0 });
        matrix[1].insert(0, C { re: 1.0, im: 0.0 });
        matrix[1].insert(1, C { re: 3.0, im: 0.0 });
        let mapping = [0, 0];
        let weights = [C { re: 1.0, im: 0.0 }, C { re: 0.0, im: 1.0 }];
        let (reduced, re, im) =
            reduce_weighted(&matrix, &[1., 2.], &[0., 0.], &mapping, &weights, 1).unwrap();
        assert!((reduced[0][&0].re - 5.0).abs() < 1e-12);
        assert!(reduced[0][&0].im.abs() < 1e-12);
        assert_eq!(re, vec![1.]);
        assert_eq!(im, vec![-2.]);
        let (real, imaginary) = expand_weighted(&[3.], &[4.], &mapping, &weights, 1).unwrap();
        assert_eq!(real, vec![3., -4.]);
        assert_eq!(imaginary, vec![4., 3.]);
    }
    #[test]
    fn rejects_invalid_indices_before_reduction() {
        let matrix = vec![BTreeMap::new()];
        assert!(reduce_weighted(&matrix, &[0.], &[0.], &[1], &[C { re: 1., im: 0. }], 1).is_err());
        assert!(expand_weighted(&[0.], &[0.], &[1], &[C { re: 1., im: 0. }], 1).is_err());
    }
}
