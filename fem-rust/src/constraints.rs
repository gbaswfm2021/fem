//! Generic weighted degree-of-freedom reduction and expansion.
//!
//! A raw degree of freedom is related to its representative by `u_i =
//! weight_i * u_hat[representative_i]`. This covers identified periodic DOFs
//! with complex phases or orientation factors. Affine offsets and general
//! multipoint constraints require a separate representation.

use crate::solver_common::{C, c_add, c_conj, c_mul};
use std::collections::{BTreeMap, HashMap};

pub(crate) type SparseSystem = (Vec<BTreeMap<usize, C>>, Vec<f64>, Vec<f64>);

pub(crate) struct DofBlock<'a> {
    pub(crate) representative: &'a [usize],
    pub(crate) weights: &'a [C],
    pub(crate) count: usize,
}

/// Join independent field maps in layout order, retaining each field's own
/// DOF count, orientations and representatives.
pub(crate) fn join_field_maps(
    blocks: &[DofBlock<'_>],
) -> Result<(Vec<usize>, Vec<C>, usize), String> {
    if blocks.is_empty() {
        return Err("At least one field constraint map is required.".into());
    }
    let mut representatives = Vec::new();
    let mut weights = Vec::new();
    let mut offset = 0usize;
    for block in blocks {
        if block.count == 0
            || block.representative.len() != block.weights.len()
            || block.representative.is_empty()
            || block.representative.iter().any(|&v| v >= block.count)
            || block
                .weights
                .iter()
                .any(|w| !w.re.is_finite() || !w.im.is_finite() || w.re.hypot(w.im) == 0.)
        {
            return Err("A field constraint map is invalid.".into());
        }
        let next = offset
            .checked_add(block.count)
            .ok_or("Combined constraint map size overflow.")?;
        let mut used = vec![false; block.count];
        for &index in block.representative {
            used[index] = true;
            representatives.push(offset + index);
        }
        if used.iter().any(|v| !*v) {
            return Err("A field constraint map has an unused representative.".into());
        }
        weights.extend_from_slice(block.weights);
        offset = next;
    }
    Ok((representatives, weights, offset))
}

/// Lift a node map to a node-major interleaved vector field without imposing
/// any assumptions about the scenario or the number of vector components.
pub(crate) fn interleaved_field_map(
    block: &DofBlock<'_>,
    components: usize,
) -> Result<(Vec<usize>, Vec<C>, usize), String> {
    if components == 0 {
        return Err("A field must have at least one component.".into());
    }
    let count = block
        .count
        .checked_mul(components)
        .ok_or("Vector constraint map size overflow.")?;
    if block.representative.len() != block.weights.len()
        || block.representative.is_empty()
        || block.count == 0
        || block.representative.iter().any(|&v| v >= block.count)
    {
        return Err("Node constraint map is invalid.".into());
    }
    let mut representative = Vec::new();
    let mut weights = Vec::new();
    for (&node, &weight) in block.representative.iter().zip(block.weights) {
        for component in 0..components {
            representative.push(node * components + component);
            weights.push(weight);
        }
    }
    // Reuse the common validation, including nonzero weights and coverage.
    join_field_maps(&[DofBlock {
        representative: &representative,
        weights: &weights,
        count,
    }])
}

/// Eliminate prescribed values from any square sparse system in place.
pub(crate) fn apply_essential(
    matrix: &mut [BTreeMap<usize, C>],
    rhs_re: &mut [f64],
    rhs_im: &mut [f64],
    constraints: &HashMap<usize, C>,
) -> Result<(), String> {
    if matrix.len() != rhs_re.len() || matrix.len() != rhs_im.len() {
        return Err("Dirichlet constraints do not match the system size.".into());
    }
    if constraints
        .iter()
        .any(|(&dof, value)| dof >= matrix.len() || !value.re.is_finite() || !value.im.is_finite())
    {
        return Err("A Dirichlet constraint has an invalid degree of freedom or value.".into());
    }
    for (&dof, &value) in constraints {
        for row in 0..matrix.len() {
            if row == dof {
                continue;
            }
            if let Some(coefficient) = matrix[row].remove(&dof) {
                let product = c_mul(coefficient, value);
                rhs_re[row] -= product.re;
                rhs_im[row] -= product.im;
            }
        }
        matrix[dof].clear();
        matrix[dof].insert(dof, C { re: 1.0, im: 0.0 });
        rhs_re[dof] = value.re;
        rhs_im[dof] = value.im;
    }
    Ok(())
}

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

/// Restrict a physical state only if every identified DOF already agrees
/// with its representative. This is suitable for prescribed initial data;
/// silently averaging conflicting periodic values would change the model.
pub(crate) fn restrict_weighted_consistent(
    real: &[f64],
    imaginary: &[f64],
    representative: &[usize],
    weights: &[C],
    count: usize,
) -> Result<(Vec<f64>, Vec<f64>), String> {
    if count == 0
        || real.len() != imaginary.len()
        || real.len() != representative.len()
        || real.len() != weights.len()
        || representative.iter().any(|&v| v >= count)
    {
        return Err("Initial state does not match the weighted DOF mapping.".into());
    }
    let mut values = vec![None::<C>; count];
    for raw in 0..real.len() {
        let weight = weights[raw];
        let magnitude = weight.re.hypot(weight.im);
        if !magnitude.is_finite()
            || magnitude == 0.
            || !real[raw].is_finite()
            || !imaginary[raw].is_finite()
        {
            return Err(
                "Initial state and mapping weights must be finite with nonzero weights.".into(),
            );
        }
        let wr = weight.re / magnitude;
        let wi = weight.im / magnitude;
        let value = C {
            re: (real[raw] * wr + imaginary[raw] * wi) / magnitude,
            im: (imaginary[raw] * wr - real[raw] * wi) / magnitude,
        };
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err("Restricted initial state is non-finite.".into());
        }
        let slot = &mut values[representative[raw]];
        if let Some(previous) = slot {
            if (value.re - previous.re).hypot(value.im - previous.im)
                > 1e-10 * (1. + value.re.hypot(value.im).max(previous.re.hypot(previous.im)))
            {
                return Err("Initial state conflicts on identified degrees of freedom.".into());
            }
        } else {
            *slot = Some(value);
        }
    }
    let values = values
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or("Initial constraint map has an unused representative.")?;
    Ok((
        values.iter().map(|v| v.re).collect(),
        values.iter().map(|v| v.im).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixed_field_maps_preserve_interleaved_components_and_block_offsets() {
        let unit = C { re: 1., im: 0. };
        let node_rep = [0, 1, 0];
        let node_weights = [unit; 3];
        let node = DofBlock {
            representative: &node_rep,
            weights: &node_weights,
            count: 2,
        };
        let (vector_rep, vector_weights, vector_count) = interleaved_field_map(&node, 2).unwrap();
        assert_eq!(vector_rep, vec![0, 1, 2, 3, 0, 1]);
        let vector = DofBlock {
            representative: &vector_rep,
            weights: &vector_weights,
            count: vector_count,
        };
        let (all, _, count) = join_field_maps(&[node, vector]).unwrap();
        assert_eq!(all, vec![0, 1, 0, 2, 3, 4, 5, 2, 3]);
        assert_eq!(count, 6);
    }
    #[test]
    fn initial_state_restriction_preserves_weighted_values_and_rejects_conflicts() {
        let weights = [C { re: 1., im: 0. }, C { re: 0., im: 1. }];
        let (re, im) =
            restrict_weighted_consistent(&[2., -3.], &[3., 2.], &[0, 0], &weights, 1).unwrap();
        assert_eq!(re, vec![2.]);
        assert_eq!(im, vec![3.]);
        let (full_re, full_im) = expand_weighted(&re, &im, &[0, 0], &weights, 1).unwrap();
        assert_eq!(full_re, vec![2., -3.]);
        assert_eq!(full_im, vec![3., 2.]);
        assert!(restrict_weighted_consistent(&[2., -2.], &[3., 2.], &[0, 0], &weights, 1).is_err());
    }
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

    #[test]
    fn full_dof_mapping_reduces_mixed_fields_with_unequal_sizes() {
        let real = |re| C { re, im: 0.0 };
        let matrix = vec![
            BTreeMap::from([(0, real(2.)), (2, real(1.))]),
            BTreeMap::from([(1, real(3.)), (2, real(2.))]),
            BTreeMap::from([(0, real(1.)), (1, real(2.)), (2, real(4.))]),
        ];
        let mapping = [0, 0, 1];
        let weights = [real(1.), real(1.), real(1.)];
        let (reduced, rhs, _) =
            reduce_weighted(&matrix, &[1., 2., 4.], &[0.; 3], &mapping, &weights, 2).unwrap();
        assert_eq!(
            (
                reduced[0][&0].re,
                reduced[0][&1].re,
                reduced[1][&0].re,
                reduced[1][&1].re
            ),
            (5., 3., 3., 4.)
        );
        assert_eq!(rhs, vec![3., 4.]);
    }
}
