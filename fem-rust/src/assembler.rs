//! Generic sparse assembly operations and runtime weak-form loading.
//!
//! Element adapters use the same typed weak-form engine and sparse scatter;
//! scenario packages declare coefficients, boundaries and recovery policies.

use crate::solver_common::{C, c_add, c_conj, c_mul, c_scale};
use crate::weak_form::{BasisSample, Coefficient, FormPlan, Operator};
use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

/// A destination for sparse matrix entries. Generic element kernels can write
/// into the ordinary sparse row maps used by one-off solves or into a cached
/// topology-derived pattern used by repeated solves.
pub(crate) trait SparseMatrixSink {
    fn dimension(&self) -> usize;
    fn add_entry(&mut self, row: usize, column: usize, value: C) -> Result<(), String>;

    /// Scatter one dense element matrix. The cached implementation can use
    /// precomputed row/column slots; ordinary row maps use the default path.
    fn scatter_element(
        &mut self,
        _element: usize,
        dofs: &[usize],
        local: &[Vec<C>],
    ) -> Result<(), String> {
        scatter_local(self, dofs, local)
    }
}

impl SparseMatrixSink for Vec<BTreeMap<usize, C>> {
    fn dimension(&self) -> usize {
        self.len()
    }

    fn add_entry(&mut self, row: usize, column: usize, value: C) -> Result<(), String> {
        let entry = self
            .get_mut(row)
            .ok_or("Sparse assembly row is outside the global matrix.")?
            .entry(column)
            .or_default();
        *entry = c_add(*entry, value);
        if !entry.re.is_finite() || !entry.im.is_finite() {
            return Err("Sparse assembly produced a non-finite matrix coefficient.".into());
        }
        Ok(())
    }
}

impl SparseMatrixSink for [BTreeMap<usize, C>] {
    fn dimension(&self) -> usize {
        self.len()
    }

    fn add_entry(&mut self, row: usize, column: usize, value: C) -> Result<(), String> {
        let entry = self
            .get_mut(row)
            .ok_or("Sparse assembly row is outside the global matrix.")?
            .entry(column)
            .or_default();
        *entry = c_add(*entry, value);
        if !entry.re.is_finite() || !entry.im.is_finite() {
            return Err("Sparse assembly produced a non-finite matrix coefficient.".into());
        }
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct SparseAssemblyWorkspace {
    pattern: Option<SparseAssemblyPattern>,
    values: Vec<C>,
    master_pattern: Option<MasterMatrixPattern>,
    master_values: Vec<C>,
}

struct SparseAssemblyPattern {
    dimension: usize,
    element_dofs: Vec<Vec<usize>>,
    row_offsets: Vec<usize>,
    columns: Vec<usize>,
    element_offsets: Vec<usize>,
    element_slots: Vec<usize>,
}

/// Reusable Pᴴ A P sparsity graph for a raw assembly pattern and weighted
/// representative map. Numeric periodic weights are applied during reduction;
/// the representative map alone determines the graph.
struct MasterMatrixPattern {
    dimension: usize,
    full_dimension: usize,
    fields: usize,
    representative: Vec<usize>,
    row_offsets: Vec<usize>,
    columns: Vec<usize>,
    raw_to_master_slots: Vec<usize>,
}

impl MasterMatrixPattern {
    fn new(
        raw: &SparseAssemblyPattern,
        representative: &[usize],
        count: usize,
    ) -> Result<Self, String> {
        if count == 0
            || representative.is_empty()
            || raw.dimension % representative.len() != 0
            || representative.iter().any(|&index| index >= count)
        {
            return Err("Weighted sparse pattern does not match the raw matrix.".into());
        }
        let full_dimension = representative.len();
        let fields = raw.dimension / full_dimension;
        let dimension = fields
            .checked_mul(count)
            .ok_or("Weighted master matrix dimension overflow.")?;
        let mut row_columns = vec![Vec::<usize>::new(); dimension];
        for (raw_row, columns) in (0..raw.dimension).map(|row| {
            (
                row,
                &raw.columns[raw.row_offsets[row]..raw.row_offsets[row + 1]],
            )
        }) {
            let field = raw_row / full_dimension;
            let raw_component = raw_row % full_dimension;
            let master_row = field * count + representative[raw_component];
            row_columns[master_row].extend(columns.iter().map(|&column| {
                (column / full_dimension) * count + representative[column % full_dimension]
            }));
        }
        for columns in &mut row_columns {
            columns.sort_unstable();
            columns.dedup();
        }
        let mut row_offsets = Vec::with_capacity(dimension + 1);
        row_offsets.push(0usize);
        for columns in &row_columns {
            row_offsets.push(
                row_offsets
                    .last()
                    .copied()
                    .unwrap()
                    .checked_add(columns.len())
                    .ok_or("Weighted sparse pattern size overflow.")?,
            );
        }
        let mut columns = Vec::with_capacity(*row_offsets.last().unwrap());
        for row in &row_columns {
            columns.extend_from_slice(row);
        }
        let mut raw_to_master_slots = Vec::with_capacity(raw.columns.len());
        for raw_row in 0..raw.dimension {
            let field = raw_row / full_dimension;
            let raw_component = raw_row % full_dimension;
            let master_row = field * count + representative[raw_component];
            let master_columns = &row_columns[master_row];
            for &raw_column in &raw.columns[raw.row_offsets[raw_row]..raw.row_offsets[raw_row + 1]] {
                let master_column =
                    (raw_column / full_dimension) * count + representative[raw_column % full_dimension];
                let relative = master_columns
                    .binary_search(&master_column)
                    .map_err(|_| "Weighted sparse pattern lost a master entry.")?;
                raw_to_master_slots.push(row_offsets[master_row] + relative);
            }
        }
        Ok(Self {
            dimension,
            full_dimension,
            fields,
            representative: representative.to_vec(),
            row_offsets,
            columns,
            raw_to_master_slots,
        })
    }

    fn matches(&self, representative: &[usize], count: usize, raw_dimension: usize) -> bool {
        !representative.is_empty()
            && raw_dimension % representative.len() == 0
            && self.full_dimension == representative.len()
            && self.fields == raw_dimension / representative.len()
            && self.fields.checked_mul(count) == Some(self.dimension)
            && self.representative == representative
    }
}

impl SparseAssemblyPattern {
    fn new(dimension: usize, elements: &[Vec<usize>]) -> Result<Self, String> {
        if dimension == 0 || elements.is_empty() {
            return Err("A sparse assembly pattern needs DOFs and elements.".into());
        }
        let mut row_columns = vec![Vec::<usize>::new(); dimension];
        for dofs in elements {
            if dofs.is_empty()
                || dofs.iter().any(|&dof| dof >= dimension)
                || {
                    let mut sorted = dofs.clone();
                    sorted.sort_unstable();
                    sorted.windows(2).any(|pair| pair[0] == pair[1])
                }
            {
                return Err("Sparse assembly element DOFs are invalid or repeated.".into());
            }
            for &row in dofs {
                row_columns[row].extend(dofs.iter().copied());
            }
        }
        for columns in &mut row_columns {
            columns.sort_unstable();
            columns.dedup();
        }
        let mut row_offsets = Vec::with_capacity(dimension + 1);
        row_offsets.push(0usize);
        for columns in &row_columns {
            row_offsets.push(
                row_offsets
                    .last()
                    .copied()
                    .unwrap()
                    .checked_add(columns.len())
                    .ok_or("Sparse assembly pattern size overflow.")?,
            );
        }
        let mut columns = Vec::with_capacity(*row_offsets.last().unwrap());
        for row in &row_columns {
            columns.extend_from_slice(row);
        }
        let mut element_offsets = Vec::with_capacity(elements.len() + 1);
        element_offsets.push(0usize);
        let mut element_slots = Vec::new();
        for dofs in elements {
            let local_entries = dofs
                .len()
                .checked_mul(dofs.len())
                .ok_or("Sparse element matrix size overflow.")?;
            element_slots.reserve(local_entries);
            for &row in dofs {
                let row_start = row_offsets[row];
                for &column in dofs {
                    let slot = row_columns[row]
                        .binary_search(&column)
                        .map_err(|_| "Sparse assembly pattern lost an element entry.")?;
                    element_slots.push(row_start + slot);
                }
            }
            element_offsets.push(element_slots.len());
        }
        Ok(Self {
            dimension,
            element_dofs: elements.to_vec(),
            row_offsets,
            columns,
            element_offsets,
            element_slots,
        })
    }

    fn matches(&self, dimension: usize, elements: &[Vec<usize>]) -> bool {
        self.dimension == dimension && self.element_dofs == elements
    }
}

impl SparseAssemblyWorkspace {
    /// Select or build the fixed raw sparsity graph for a mesh and clear only
    /// its numeric values. Weighted master-matrix slots are cached separately
    /// when `reduce_weighted` receives the representative map.
    pub(crate) fn prepare(
        &mut self,
        dimension: usize,
        elements: &[Vec<usize>],
    ) -> Result<(), String> {
        let started = Instant::now();
        if self
            .pattern
            .as_ref()
            .is_some_and(|pattern| pattern.matches(dimension, elements))
        {
            self.values.fill(C::default());
            println!(
                "  profile: generic sparse assembly pattern reused in {:.3}s ({} matrix slots, {} elements)",
                started.elapsed().as_secs_f64(),
                self.values.len(),
                elements.len()
            );
            return Ok(());
        }
        let pattern = SparseAssemblyPattern::new(dimension, elements)?;
        self.values = vec![C::default(); pattern.columns.len()];
        self.master_pattern = None;
        self.master_values.clear();
        println!(
            "  profile: generic sparse assembly pattern built in {:.3}s ({} matrix slots, {} element slots)",
            started.elapsed().as_secs_f64(),
            pattern.columns.len(),
            pattern.element_slots.len()
        );
        self.pattern = Some(pattern);
        Ok(())
    }

    pub(crate) fn rows(&self) -> Result<Vec<BTreeMap<usize, C>>, String> {
        let pattern = self
            .pattern
            .as_ref()
            .ok_or("Sparse assembly pattern has not been prepared.")?;
        Ok((0..pattern.dimension)
            .map(|row| {
                (pattern.row_offsets[row]..pattern.row_offsets[row + 1])
                    .map(|slot| (pattern.columns[slot], self.values[slot]))
                    .collect()
            })
            .collect())
    }

    /// Reduce the assembled raw matrix and load vector through a weighted
    /// representative map. Repeated systems reuse the master graph and the
    /// raw-entry-to-master-slot map; numeric weights are applied per system.
    pub(crate) fn reduce_weighted(
        &mut self,
        rhs_re: &[f64],
        rhs_im: &[f64],
        representative: &[usize],
        weights: &[C],
        count: usize,
    ) -> Result<(Vec<BTreeMap<usize, C>>, Vec<f64>, Vec<f64>), String> {
        let pattern = self
            .pattern
            .as_ref()
            .ok_or("Sparse assembly pattern has not been prepared.")?;
        if rhs_re.len() != pattern.dimension
            || rhs_im.len() != pattern.dimension
            || representative.is_empty()
            || weights.len() != representative.len()
            || pattern.dimension % representative.len() != 0
            || count == 0
            || representative.iter().any(|&index| index >= count)
            || weights
                .iter()
                .any(|weight| !weight.re.is_finite() || !weight.im.is_finite())
        {
            return Err("Weighted sparse reduction does not match the assembled system.".into());
        }
        let full_dimension = representative.len();
        let fields = pattern.dimension / full_dimension;
        let master_dimension = fields
            .checked_mul(count)
            .ok_or("Weighted master matrix dimension overflow.")?;
        let identity = count == full_dimension
            && representative.iter().enumerate().all(|(index, &master)| index == master)
            && weights.iter().all(|weight| weight.re == 1.0 && weight.im == 0.0);
        if identity {
            return Ok((self.rows()?, rhs_re.to_vec(), rhs_im.to_vec()));
        }
        let pattern_started = Instant::now();
        let built = !self
            .master_pattern
            .as_ref()
            .is_some_and(|master| master.matches(representative, count, pattern.dimension));
        if built {
            let master = MasterMatrixPattern::new(pattern, representative, count)?;
            self.master_values = vec![C::default(); master.columns.len()];
            self.master_pattern = Some(master);
        }
        let master = self
            .master_pattern
            .as_ref()
            .ok_or("Weighted master matrix pattern was not prepared.")?;
        let pattern_elapsed = pattern_started.elapsed().as_secs_f64();
        println!(
            "  profile: generic master matrix pattern {} in {:.3}s ({} matrix slots, {} raw slots)",
            if built { "built" } else { "reused" },
            pattern_elapsed,
            master.columns.len(),
            master.raw_to_master_slots.len()
        );

        let reduction_started = Instant::now();
        self.master_values.fill(C::default());
        let mut reduced_re = vec![0.0; master_dimension];
        let mut reduced_im = vec![0.0; master_dimension];
        for raw_row in 0..pattern.dimension {
            let raw_component = raw_row % full_dimension;
            let master_row = (raw_row / full_dimension) * count + representative[raw_component];
            let row_weight = c_conj(weights[raw_component]);
            let load = c_mul(
                row_weight,
                C { re: rhs_re[raw_row], im: rhs_im[raw_row] },
            );
            reduced_re[master_row] += load.re;
            reduced_im[master_row] += load.im;
            for raw_slot in pattern.row_offsets[raw_row]..pattern.row_offsets[raw_row + 1] {
                let raw_column = pattern.columns[raw_slot];
                let contribution = c_mul(
                    c_mul(row_weight, self.values[raw_slot]),
                    weights[raw_column % full_dimension],
                );
                let master_slot = master.raw_to_master_slots[raw_slot];
                self.master_values[master_slot] =
                    c_add(self.master_values[master_slot], contribution);
            }
        }
        println!(
            "  profile: generic master matrix numeric reduction {:.3}s",
            reduction_started.elapsed().as_secs_f64()
        );

        let materialize_started = Instant::now();
        let rows = (0..master.dimension)
            .map(|row| {
                (master.row_offsets[row]..master.row_offsets[row + 1])
                    .map(|slot| (master.columns[slot], self.master_values[slot]))
                    .collect()
            })
            .collect();
        println!(
            "  profile: generic master matrix row-map materialization {:.3}s",
            materialize_started.elapsed().as_secs_f64()
        );
        Ok((rows, reduced_re, reduced_im))
    }
}

impl SparseMatrixSink for SparseAssemblyWorkspace {
    fn dimension(&self) -> usize {
        self.pattern
            .as_ref()
            .map_or(0, |pattern| pattern.dimension)
    }

    fn add_entry(&mut self, row: usize, column: usize, value: C) -> Result<(), String> {
        let pattern = self
            .pattern
            .as_ref()
            .ok_or("Sparse assembly pattern has not been prepared.")?;
        if row >= pattern.dimension {
            return Err("Sparse assembly row is outside the global matrix.".into());
        }
        let range = pattern.row_offsets[row]..pattern.row_offsets[row + 1];
        let relative = pattern.columns[range.clone()]
            .binary_search(&column)
            .map_err(|_| "Sparse assembly entry is absent from the prepared pattern.")?;
        let slot = range.start + relative;
        let entry = &mut self.values[slot];
        *entry = c_add(*entry, value);
        if !entry.re.is_finite() || !entry.im.is_finite() {
            return Err("Sparse assembly produced a non-finite matrix coefficient.".into());
        }
        Ok(())
    }

    fn scatter_element(
        &mut self,
        element: usize,
        dofs: &[usize],
        local: &[Vec<C>],
    ) -> Result<(), String> {
        let pattern = self
            .pattern
            .as_ref()
            .ok_or("Sparse assembly pattern has not been prepared.")?;
        let expected = pattern
            .element_dofs
            .get(element)
            .ok_or("Sparse assembly element index is outside the prepared pattern.")?;
        if expected != dofs
            || local.len() != dofs.len()
            || local.iter().any(|row| row.len() != dofs.len())
            || local
                .iter()
                .flatten()
                .any(|value| !value.re.is_finite() || !value.im.is_finite())
        {
            return Err("Invalid cached sparse element scatter shape, DOF or coefficient.".into());
        }
        let start = pattern.element_offsets[element];
        let end = pattern.element_offsets[element + 1];
        let slots = &pattern.element_slots[start..end];
        for (slot, value) in slots.iter().copied().zip(local.iter().flatten().copied()) {
            self.values[slot] = c_add(self.values[slot], value);
            if !self.values[slot].re.is_finite() || !self.values[slot].im.is_finite() {
                return Err("Sparse assembly produced a non-finite matrix coefficient.".into());
            }
        }
        Ok(())
    }
}

/// Add a dense local element matrix to sparse global row maps. Validate all
/// dimensions, indexes and values before altering the global matrix.
pub(crate) fn scatter_local<M: SparseMatrixSink + ?Sized>(
    matrix: &mut M,
    dofs: &[usize],
    local: &[Vec<C>],
) -> Result<(), String> {
    if local.len() != dofs.len()
        || local.iter().any(|row| row.len() != dofs.len())
        || dofs.iter().any(|&dof| dof >= matrix.dimension())
        || local
            .iter()
            .flatten()
            .any(|c| !c.re.is_finite() || !c.im.is_finite())
    {
        return Err("Invalid sparse scatter matrix shape, DOF or coefficient.".into());
    }
    for (i, &row) in dofs.iter().enumerate() {
        for (j, &column) in dofs.iter().enumerate() {
            matrix.add_entry(row, column, local[i][j])?;
        }
    }
    Ok(())
}

/// Evaluate a typed weak form for real-valued scalar P1 basis samples at one
/// quadrature point, then scatter into the sparse master matrix. `weight`
/// includes physical measure and quadrature weight. Coefficients are complex.
pub(crate) fn accumulate_scalar_volume<const N: usize, const D: usize, M: SparseMatrixSink + ?Sized>(
    plan: &FormPlan,
    matrix: &mut M,
    dofs: &[usize; N],
    values: &[f64],
    gradients: &[[f64; D]; N],
    weight: f64,
    coefficients: &HashMap<String, C>,
) -> Result<(), String> {
    if values.len() != N {
        return Err("Scalar basis value count does not match the element DOFs.".into());
    }
    plan.require_shape(D, 1)?;
    if !weight.is_finite() || dofs.iter().any(|&dof| dof >= matrix.dimension()) {
        return Err("Invalid scalar quadrature weight or element DOF.".into());
    }

    // Select a specialized mathematical kernel based only on the declared
    // operators. It has the same scalar-coefficient contract as FormPlan;
    // more general mixed/tensor operators use the generic path below.
    let mut gradient_coefficient = C::default();
    let mut value_coefficient = C::default();
    let mut has_gradient = false;
    let mut has_value = false;
    let mut specialized = true;
    for term in plan.terms() {
        let target = match (term.test, term.trial) {
            (Operator::Gradient, Operator::Gradient) => {
                if has_gradient {
                    specialized = false;
                    break;
                }
                has_gradient = true;
                &mut gradient_coefficient
            }
            (Operator::Value, Operator::Value) => {
                if has_value {
                    specialized = false;
                    break;
                }
                has_value = true;
                &mut value_coefficient
            }
            _ => {
                specialized = false;
                break;
            }
        };
        let coefficient = coefficients
            .get(&term.coefficient)
            .ok_or_else(|| format!("Missing weak-form coefficient {:?}.", term.coefficient))?;
        if !coefficient.re.is_finite() || !coefficient.im.is_finite() {
            return Err(format!(
                "Weak-form coefficient {:?} must be finite.",
                term.coefficient
            ));
        }
        *target = c_add(*target, *coefficient);
    }
    if specialized {
        if (has_value && values.iter().any(|value| !value.is_finite()))
            || (has_gradient && gradients.iter().flatten().any(|value| !value.is_finite()))
        {
            return Err("Scalar basis samples must be finite.".into());
        }
        for i in 0..N {
            for j in 0..N {
                let mut value = C::default();
                if has_gradient {
                    let product: f64 = gradients[i]
                        .iter()
                        .zip(gradients[j].iter())
                        .map(|(a, b)| a * b)
                        .sum();
                    value = c_add(value, c_scale(gradient_coefficient, weight * product));
                }
                if has_value {
                    value = c_add(
                        value,
                        c_scale(value_coefficient, weight * values[i] * values[j]),
                    );
                }
                matrix.add_entry(dofs[i], dofs[j], value)?;
            }
        }
        return Ok(());
    }

    let basis: Vec<_> = (0..N)
        .map(|i| BasisSample {
            value: std::slice::from_ref(&values[i]),
            gradient: &gradients[i],
            curl: &[],
        })
        .collect();
    let terms = coefficients
        .iter()
        .map(|(name, value)| (name.clone(), Coefficient::Scalar(*value)))
        .collect();
    let mut local = vec![vec![C::default(); N]; N];
    plan.accumulate(&basis, &terms, weight, &mut local)?;
    scatter_local(matrix, dofs, &local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 1e-12, "{actual} != {expected}");
    }

    #[test]
    fn triangle_stiffness_and_consistent_complex_mass() {
        let plan = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"terms":[
            {"test":"gradient","trial":"gradient","coefficient":"a"},
            {"test":"value","trial":"value","coefficient":"b"}
        ]}))
        .unwrap();
        let gradients = [[-1.0, -1.0], [1.0, 0.0], [0.0, 1.0]];
        let points = [
            [2.0 / 3.0, 1.0 / 6.0, 1.0 / 6.0],
            [1.0 / 6.0, 2.0 / 3.0, 1.0 / 6.0],
            [1.0 / 6.0, 1.0 / 6.0, 2.0 / 3.0],
        ];
        let coefficients = HashMap::from([
            ("a".into(), C { re: 2.0, im: 0.0 }),
            ("b".into(), C { re: 3.0, im: -4.0 }),
        ]);
        let mut matrix = vec![BTreeMap::new(); 3];
        for point in points {
            accumulate_scalar_volume(
                &plan,
                &mut matrix,
                &[0, 1, 2],
                &point,
                &gradients,
                1.0 / 6.0,
                &coefficients,
            )
            .unwrap();
        }
        for i in 0..3 {
            for j in 0..3 {
                let grad_dot =
                    gradients[i][0] * gradients[j][0] + gradients[i][1] * gradients[j][1];
                let mass = if i == j { 1.0 / 12.0 } else { 1.0 / 24.0 };
                close(matrix[i][&j].re, grad_dot + 3.0 * mass);
                close(matrix[i][&j].im, -4.0 * mass);
            }
        }
    }

    #[test]
    fn tetrahedra_scatter_shared_dofs_without_dense_global_storage() {
        let plan = FormPlan::from_json(&json!({"dimension":3,"fieldComponents":1,"terms":[
            {"test":"gradient","trial":"gradient","coefficient":"a"}
        ]}))
        .unwrap();
        let gradients = [
            [-1.0, -1.0, -1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ];
        let coefficients = HashMap::from([("a".into(), C { re: 1.0, im: 0.0 })]);
        let mut matrix = vec![BTreeMap::new(); 6];
        let values = [0.25; 4];
        accumulate_scalar_volume(
            &plan,
            &mut matrix,
            &[0, 1, 2, 3],
            &values,
            &gradients,
            1.0 / 6.0,
            &coefficients,
        )
        .unwrap();
        accumulate_scalar_volume(
            &plan,
            &mut matrix,
            &[0, 1, 2, 4],
            &values,
            &gradients,
            1.0 / 6.0,
            &coefficients,
        )
        .unwrap();
        close(matrix[0][&0].re, 1.0);
        close(matrix[0][&1].re, -1.0 / 3.0);
        assert!(!matrix[3].contains_key(&4));
        assert!(matrix[5].is_empty());
    }

    #[test]
    fn rejects_invalid_scatter_without_mutation() {
        let mut matrix = vec![BTreeMap::new(); 2];
        let local = vec![vec![C { re: 1., im: 0. }; 2]; 2];
        assert!(scatter_local(&mut matrix, &[0, 2], &local).is_err());
        assert!(matrix.iter().all(|row| row.is_empty()));
    }

    #[test]
    fn reused_sparse_workspace_rebuilds_after_connectivity_changes() {
        let first = vec![vec![0, 1], vec![2, 3]];
        let changed = vec![vec![0, 3], vec![1, 2]];
        let local = vec![vec![C { re: 2., im: 1. }, C { re: -1., im: 0.5 }],
                         vec![C { re: -1., im: 0.5 }, C { re: 3., im: -1. }]];
        let representatives = [0, 1, 2, 0];
        let weights = [C { re: 1., im: 0. }, C { re: 1., im: 0. },
                       C { re: 1., im: 0. }, C { re: 0., im: 1. }];
        let rhs_re = [1., 2., 3., 4.];
        let rhs_im = [4., 3., 2., 1.];
        let assemble = |workspace: &mut SparseAssemblyWorkspace, elements: &[Vec<usize>]| {
            workspace.prepare(4, elements).unwrap();
            for (i, dofs) in elements.iter().enumerate() {
                workspace.scatter_element(i, dofs, &local).unwrap();
            }
            workspace.reduce_weighted(&rhs_re, &rhs_im, &representatives, &weights, 3).unwrap()
        };
        let mut reused = SparseAssemblyWorkspace::default();
        let old = assemble(&mut reused, &first);
        let actual = assemble(&mut reused, &changed);
        let expected = assemble(&mut SparseAssemblyWorkspace::default(), &changed);
        assert_ne!(old.0[0].keys().collect::<Vec<_>>(), actual.0[0].keys().collect::<Vec<_>>());
        assert_eq!(actual.1, expected.1);
        assert_eq!(actual.2, expected.2);
        for (row, reference) in actual.0.iter().zip(&expected.0) {
            assert_eq!(row.keys().collect::<Vec<_>>(), reference.keys().collect::<Vec<_>>());
            for (column, value) in row {
                close(value.re, reference[column].re);
                close(value.im, reference[column].im);
            }
        }
    }

    #[test]
    fn json_term_and_expression_change_the_same_sparse_assembly() {
        let source = |scale| {
            json!({"dimension": 2, "fieldComponents": 1,
                "terms": [{"test": "gradient", "trial": "gradient", "coefficient": "a"}],
                "coefficients": {"a": {"multiply": [scale, {"symbol": "k"}]}}
            })
        };
        let gradients = [[-1.0, -1.0], [1.0, 0.0], [0.0, 1.0]];
        let symbols = HashMap::from([("k".into(), C { re: 2.0, im: 0.0 })]);
        let mut matrices = Vec::new();
        for scale in [1.0, 3.0] {
            let plan = FormPlan::from_json(&source(scale)).unwrap();
            let coefficients = plan.evaluate_coefficients(&symbols).unwrap();
            let mut matrix = vec![BTreeMap::new(); 3];
            accumulate_scalar_volume(
                &plan,
                &mut matrix,
                &[0, 1, 2],
                &[1.0 / 3.0; 3],
                &gradients,
                0.5,
                &coefficients,
            )
            .unwrap();
            matrices.push(matrix);
        }
        close(matrices[0][0][&0].re, 2.0);
        close(matrices[1][0][&0].re, 6.0);
        close(matrices[1][0][&1].re, -3.0);
    }

    #[test]
    fn scalar_path_matches_generic_form_with_multiple_complex_terms() {
        let plan = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"terms":[
            {"test":"value","trial":"value","coefficient":"b"},
            {"test":"gradient","trial":"gradient","coefficient":"a"},
            {"test":"gradient","trial":"gradient","coefficient":"c"}
        ]}))
        .unwrap();
        let gradients = [[-1.0, -1.0], [1.0, 0.0], [0.0, 1.0]];
        let values = [0.2, 0.3, 0.5];
        let coefficients = HashMap::from([
            ("a".into(), C { re: 2.0, im: 3.0 }),
            ("b".into(), C { re: -4.0, im: 5.0 }),
            ("c".into(), C { re: 0.5, im: -1.0 }),
        ]);
        let mut sparse = vec![BTreeMap::new(); 3];
        accumulate_scalar_volume(
            &plan,
            &mut sparse,
            &[0, 1, 2],
            &values,
            &gradients,
            0.25,
            &coefficients,
        )
        .unwrap();
        let basis: Vec<_> = (0..3)
            .map(|i| BasisSample {
                value: std::slice::from_ref(&values[i]),
                gradient: &gradients[i],
                curl: &[],
            })
            .collect();
        let generic_coefficients = coefficients
            .iter()
            .map(|(name, value)| (name.clone(), Coefficient::Scalar(*value)))
            .collect();
        let mut generic = vec![vec![C::default(); 3]; 3];
        plan.accumulate(&basis, &generic_coefficients, 0.25, &mut generic)
            .unwrap();
        for i in 0..3 {
            for j in 0..3 {
                close(sparse[i][&j].re, generic[i][j].re);
                close(sparse[i][&j].im, generic[i][j].im);
            }
        }
    }

    #[test]
    fn unsupported_scalar_cross_term_uses_generic_validation_without_mutation() {
        let plan = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"terms":[
            {"test":"value","trial":"gradient","coefficient":"a"}
        ]}))
        .unwrap();
        let mut sparse = vec![BTreeMap::new(); 3];
        let gradients = [[-1.0, -1.0], [1.0, 0.0], [0.0, 1.0]];
        let coefficients = HashMap::from([("a".into(), C { re: 1.0, im: 0.0 })]);
        assert!(
            accumulate_scalar_volume(
                &plan,
                &mut sparse,
                &[0, 1, 2],
                &[0.3, 0.3, 0.4],
                &gradients,
                0.5,
                &coefficients
            )
            .is_err()
        );
        assert!(sparse.iter().all(BTreeMap::is_empty));
    }

    #[test]
    #[ignore = "explicit scalar quadrature microbenchmark; run optimized with --ignored --nocapture"]
    fn scalar_quadrature_microbenchmark() {
        use std::time::Instant;
        let plan = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"terms":[
            {"test":"gradient","trial":"gradient","coefficient":"a"},
            {"test":"value","trial":"value","coefficient":"b"}
        ], "coefficients": {"a":{"divide":[1,{"symbol":"rho"}]},
            "b":{"multiply":[-1,{"symbol":"omega"},{"symbol":"omega"}]}}}))
        .unwrap();
        let gradients = [[-1.0, -1.0], [1.0, 0.0], [0.0, 1.0]];
        let quadrature = [[0.2, 0.3, 0.5], [0.6, 0.2, 0.2], [0.2, 0.6, 0.2]];
        let loops = 20_000;
        let mut direct = vec![BTreeMap::<usize, C>::new(); 3];
        let start = Instant::now();
        for _ in 0..loops {
            for q in quadrature {
                let q = std::hint::black_box(q);
                let gradients = std::hint::black_box(gradients);
                let density = std::hint::black_box(2.0);
                let omega = std::hint::black_box(3.0);
                for i in 0..3 {
                    for j in 0..3 {
                        let dot =
                            gradients[i][0] * gradients[j][0] + gradients[i][1] * gradients[j][1];
                        let value = (dot / density - omega * omega * q[i] * q[j]) / 6.0;
                        direct[i].entry(j).or_default().re += std::hint::black_box(value);
                    }
                }
            }
        }
        let direct_elapsed = start.elapsed();
        let mut precomputed = vec![BTreeMap::<usize, C>::new(); 3];
        let coefficients = HashMap::from([
            ("a".to_string(), C { re: 0.5, im: 0.0 }),
            ("b".to_string(), C { re: -9.0, im: 0.0 }),
        ]);
        let start = Instant::now();
        for _ in 0..loops {
            for q in quadrature {
                let q = std::hint::black_box(q);
                let gradients = std::hint::black_box(gradients);
                accumulate_scalar_volume(
                    &plan, &mut precomputed, &[0, 1, 2], &q, &gradients,
                    1.0 / 6.0, &coefficients,
                ).unwrap();
            }
        }
        let precomputed_elapsed = start.elapsed();
        let symbols = HashMap::from([
            ("rho".to_string(), C { re: 2.0, im: 0.0 }),
            ("omega".to_string(), C { re: 3.0, im: 0.0 }),
        ]);
        let start = Instant::now();
        for _ in 0..loops * quadrature.len() {
            std::hint::black_box(plan.evaluate_coefficients(std::hint::black_box(&symbols)).unwrap());
        }
        let expressions_elapsed = start.elapsed();
        let mut bound = plan.bind_coefficients(&["rho", "omega"]).unwrap();
        let inputs = [C { re: 2.0, im: 0.0 }, C { re: 3.0, im: 0.0 }];
        let start = Instant::now();
        for _ in 0..loops * quadrature.len() {
            std::hint::black_box(bound.evaluate(std::hint::black_box(&inputs)).unwrap());
        }
        let bound_expression_elapsed = start.elapsed();
        let mut bound_matrix = vec![BTreeMap::<usize, C>::new(); 3];
        let start = Instant::now();
        for _ in 0..loops {
            for q in quadrature {
                let q = std::hint::black_box(q);
                let gradients = std::hint::black_box(gradients);
                let coefficients = bound.evaluate(std::hint::black_box(&inputs)).unwrap();
                accumulate_scalar_volume(
                    &plan, &mut bound_matrix, &[0, 1, 2], &q, &gradients,
                    1.0 / 6.0, coefficients,
                ).unwrap();
            }
        }
        let bound_total_elapsed = start.elapsed();
        let mut declared = vec![BTreeMap::<usize, C>::new(); 3];
        let start = Instant::now();
        for _ in 0..loops {
            for q in quadrature {
                let q = std::hint::black_box(q);
                let gradients = std::hint::black_box(gradients);
                let symbols = HashMap::from([
                    ("rho".to_string(), C { re: 2.0, im: 0.0 }),
                    ("omega".to_string(), C { re: 3.0, im: 0.0 }),
                ]);
                let coefficients = plan.evaluate_coefficients(&symbols).unwrap();
                accumulate_scalar_volume(
                    &plan,
                    &mut declared,
                    &[0, 1, 2],
                    &q,
                    &gradients,
                    1.0 / 6.0,
                    &coefficients,
                )
                .unwrap();
            }
        }
        let declared_elapsed = start.elapsed();
        for i in 0..3 {
            for j in 0..3 {
                let expected = direct[i][&j].re;
                assert!((bound_matrix[i][&j].re - expected).abs() < 1e-8 * expected.abs().max(1.0));
                assert!((precomputed[i][&j].re - expected).abs() < 1e-8 * expected.abs().max(1.0));
                assert!((declared[i][&j].re - expected).abs() < 1e-8 * expected.abs().max(1.0));
            }
        }
        eprintln!(
            "scalar volume {loops} cells, {} points: direct={direct_elapsed:?}, precomputed={precomputed_elapsed:?}, expression_only={expressions_elapsed:?}, declared_with_expression={declared_elapsed:?}, bound_expression={bound_expression_elapsed:?}, bound_total={bound_total_elapsed:?}",
            quadrature.len()
        );
    }
}
