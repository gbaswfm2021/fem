//! Generic scalar P1 simplex numerical runner.
//!
//! This module operates on a prepared mesh, form, quadrature samples and
//! boundary/constraint data. It does not select a scenario. Separate generic
//! adapters bind model inputs and publish the declared results.

use std::collections::{BTreeMap, HashMap};

use crate::boundary_form::BoundaryFormPlan;
use crate::essential_form::EssentialFormPlan;
use crate::recovery::RecoveryPlan;
use crate::solver_common::{c_add, solve_linear_system, SolverCache, C};
use crate::weak_form::FormPlan;

pub(crate) struct SimplexVolumePoint<const N: usize> {
    /// Simplex barycentric coordinates, also the scalar P1 basis values.
    pub(crate) basis: [f64; N],
    /// Weight normalized so the physical weight is `measure * weight`.
    pub(crate) weight: f64,
    /// Values for the form's bound symbols, in the declared input order.
    pub(crate) symbols: Vec<C>,
}

pub(crate) struct SimplexCell<const N: usize> {
    pub(crate) nodes: [usize; N],
    pub(crate) quadrature: Vec<SimplexVolumePoint<N>>,
}

/// One already-evaluated boundary quadrature point. The plan determines its
/// generic matrix/load operations and source trace, not the scenario name.
pub(crate) struct SimplexBoundaryPoint<'a, const F: usize, const D: usize> {
    pub(crate) nodes: [usize; F],
    pub(crate) basis: [f64; F],
    pub(crate) normal: [f64; D],
    pub(crate) weight: f64,
    pub(crate) plan: &'a BoundaryFormPlan,
    pub(crate) coefficients: HashMap<String, C>,
    pub(crate) sources: HashMap<String, Vec<C>>,
}

pub(crate) struct SimplexProblem<'a, const N: usize, const D: usize, const F: usize> {
    pub(crate) nodes: &'a [[f64; 3]],
    pub(crate) cells: &'a [SimplexCell<N>],
    /// Connectivity and geometry checks selected by the caller.
    pub(crate) topology: crate::simplex_topology::TopologyPolicy,
    pub(crate) form: &'a FormPlan,
    pub(crate) symbol_names: &'a [&'a str],
    pub(crate) boundaries: &'a [SimplexBoundaryPoint<'a, F, D>],
    /// Physical nodal constraints in input order, including duplicates.
    pub(crate) essential: &'a [(usize, C)],
    /// Required for periodic constraints or repeated prescribed DOFs.
    pub(crate) essential_plan: Option<&'a EssentialFormPlan>,
    pub(crate) periodic: Option<PeriodicMap<'a>>,
    /// Request component-wise mean gauges only for forms with a constant
    /// nullspace; anchored components must remain unaffected.
    pub(crate) mean: Option<MeanPolicy>,
}

pub(crate) type VolumePoint = SimplexVolumePoint<3>;
pub(crate) type Cell = SimplexCell<3>;
pub(crate) type BoundaryPoint<'a> = SimplexBoundaryPoint<'a, 2, 2>;
pub(crate) type Problem<'a> = SimplexProblem<'a, 3, 2, 2>;
pub(crate) type TetraVolumePoint = SimplexVolumePoint<4>;
pub(crate) type TetraCell = SimplexCell<4>;
pub(crate) type TetraBoundaryPoint<'a> = SimplexBoundaryPoint<'a, 3, 3>;
pub(crate) type TetraProblem<'a> = SimplexProblem<'a, 4, 3, 3>;

pub(crate) struct PeriodicMap<'a> {
    pub(crate) representative: &'a [usize],
    pub(crate) weights: &'a [C],
    pub(crate) count: usize,
    /// Maximum distance from unity accepted for a declared zero-phase map.
    pub(crate) zero_phase_tolerance: f64,
    /// Whether essential inputs are physical values or direct representative values.
    pub(crate) essential_mapping: EssentialMapping,
}

#[derive(Clone, Copy)]
pub(crate) enum EssentialMapping {
    WeightedPhysical,
    DirectRepresentative,
}

pub(crate) struct MeanPolicy {
    pub(crate) value: C,
    pub(crate) balance_relative_tolerance: f64,
}

pub(crate) struct SolveOptions<'a> {
    pub(crate) backend: &'a str,
    pub(crate) ordering: &'a str,
    pub(crate) threads: usize,
    pub(crate) residual_tolerance: f64,
}

pub(crate) struct LinearSystem {
    pub(crate) matrix: Vec<BTreeMap<usize, C>>,
    pub(crate) rhs_re: Vec<f64>,
    pub(crate) rhs_im: Vec<f64>,
    pub(crate) field_dofs: usize,
    pub(crate) mean_constraints: usize,
}

pub(crate) struct Solution {
    pub(crate) real: Vec<f64>,
    pub(crate) imaginary: Vec<f64>,
    pub(crate) residual: f64,
    pub(crate) field_dofs: usize,
    pub(crate) mean_constraints: usize,
    pub(crate) auxiliary_real: Vec<f64>,
    pub(crate) auxiliary_imaginary: Vec<f64>,
}

fn root(parent: &mut [usize], node: usize) -> usize {
    let mut value = node;
    while parent[value] != value {
        value = parent[value];
    }
    let mut current = node;
    while parent[current] != current {
        let next = parent[current];
        parent[current] = value;
        current = next;
    }
    value
}

fn join(parent: &mut [usize], a: usize, b: usize) {
    let left = root(parent, a);
    let right = root(parent, b);
    if left != right {
        parent[right] = left;
    }
}

fn geometry<const N: usize, const D: usize>(
    points: [[f64; 3]; N],
) -> Result<(f64, [[f64; D]; N]), String> {
    match (N, D) {
        (3, 2) => {
            let (measure, gradients) =
                crate::mesh2d::triangle_geometry([points[0], points[1], points[2]])?;
            Ok((
                measure,
                std::array::from_fn(|i| std::array::from_fn(|d| gradients[i][d])),
            ))
        }
        (4, 3) => {
            // The caller's declared determinant policy is checked in topology
            // validation. This geometry calculation additionally rejects zero.
            let (measure, gradients) = crate::simplex::tetra_p1(
                [points[0], points[1], points[2], points[3]],
                crate::simplex::DeterminantTolerance::Absolute(0.0),
            )?;
            if measure == 0.0
                || !measure.is_finite()
                || gradients.iter().flatten().any(|v| !v.is_finite())
            {
                return Err(
                    "Tetrahedron measure or basis gradients are non-finite or degenerate.".into(),
                );
            }
            Ok((
                measure,
                std::array::from_fn(|i| std::array::from_fn(|d| gradients[i][d])),
            ))
        }
        _ => Err("Unsupported scalar P1 simplex shape.".into()),
    }
}

impl<const N: usize, const D: usize, const F: usize> SimplexProblem<'_, N, D, F> {
    /// Assemble the supplied generic cell and facet forms directly into
    /// sparse rows, then apply the supplied nodal essential constraints.
    pub(crate) fn assemble(&self) -> Result<LinearSystem, String> {
        self.assemble_with_workspace(&mut crate::assembler::SparseAssemblyWorkspace::default())
    }

    pub(crate) fn assemble_with_workspace(
        &self,
        workspace: &mut crate::assembler::SparseAssemblyWorkspace,
    ) -> Result<LinearSystem, String> {
        self.assemble_with_volume_loads(workspace, None)
    }

    /// Optional per-cell, per-quadrature source samples. The no-source path
    /// avoids preparing or visiting any load data.
    pub(crate) fn assemble_with_volume_loads(
        &self,
        workspace: &mut crate::assembler::SparseAssemblyWorkspace,
        loads: Option<(&crate::weak_form::VolumeLoadPlan, &[Vec<Vec<C>>])>,
    ) -> Result<LinearSystem, String> {
        if let Some((_, samples)) = loads {
            if samples.len() != self.cells.len() || samples.iter().zip(self.cells).any(|(row, cell)| row.len() != cell.quadrature.len()) {
                return Err("Volume load samples must match the cell quadrature.".into());
            }
        }
        if self.nodes.is_empty() || self.cells.is_empty() {
            return Err("A scalar P1 problem requires nodes and simplex cells.".into());
        }
        if self.nodes.iter().flatten().any(|v| !v.is_finite()) {
            return Err("Scalar P1 mesh coordinates must be finite.".into());
        }
        let cell_kind = match (N, D, F) {
            (3, 2, 2) => {
                let connectivity: Vec<[usize; 3]> = self
                    .cells
                    .iter()
                    .map(|cell| [cell.nodes[0], cell.nodes[1], cell.nodes[2]])
                    .collect();
                crate::simplex_topology::triangles(self.nodes, &connectivity, &[], self.topology)?;
                crate::discretization::Cell::Triangle
            }
            (4, 3, 3) => {
                let connectivity: Vec<[usize; 4]> = self
                    .cells
                    .iter()
                    .map(|cell| [cell.nodes[0], cell.nodes[1], cell.nodes[2], cell.nodes[3]])
                    .collect();
                crate::simplex_topology::tetrahedra(self.nodes, &connectivity, &[], self.topology)?;
                crate::discretization::Cell::Tetrahedron
            }
            _ => return Err("Unsupported scalar P1 simplex shape.".into()),
        };
        self.form.require_shape(D, 1)?;
        self.form.require_discretization(
            cell_kind,
            crate::discretization::Family::Lagrange,
            1,
            1,
        )?;
        let identity: Vec<usize> = (0..self.nodes.len()).collect();
        let identity_weights = vec![C { re: 1.0, im: 0.0 }; self.nodes.len()];
        let (representative, weights, field_dofs) = if let Some(map) = &self.periodic {
            if map.representative.len() != self.nodes.len()
                || map.weights.len() != self.nodes.len()
                || map.count == 0
                || !map.zero_phase_tolerance.is_finite()
                || map.zero_phase_tolerance < 0.0
                || map.representative.iter().any(|&index| index >= map.count)
                || map.weights.iter().any(|weight| {
                    !weight.re.is_finite()
                        || !weight.im.is_finite()
                        || (weight.re == 0.0 && weight.im == 0.0)
                })
            {
                return Err("Invalid periodic scalar DOF map.".into());
            }
            let mut seen = vec![false; map.count];
            for &index in map.representative {
                seen[index] = true;
            }
            if seen.contains(&false) {
                return Err("Periodic scalar DOF map has an unused representative.".into());
            }
            (map.representative, map.weights, map.count)
        } else {
            (
                identity.as_slice(),
                identity_weights.as_slice(),
                self.nodes.len(),
            )
        };
        let element_dofs: Vec<Vec<usize>> =
            self.cells.iter().map(|cell| cell.nodes.to_vec()).collect();
        workspace.prepare(self.nodes.len(), &element_dofs)?;
        let mut bound = self.form.bind_coefficients(self.symbol_names)?;
        let mut nodal_area = vec![0.0; self.nodes.len()];
        let mut absolute_loads = vec![0.0; self.nodes.len()];
        let mut system = LinearSystem {
            matrix: Vec::new(),
            rhs_re: vec![0.0; self.nodes.len()],
            rhs_im: vec![0.0; self.nodes.len()],
            field_dofs,
            mean_constraints: 0,
        };
        for (index, cell) in self.cells.iter().enumerate() {
            if cell.nodes.iter().any(|&node| node >= self.nodes.len()) || cell.quadrature.is_empty()
            {
                return Err(format!(
                    "Scalar P1 cell {index} has invalid nodes or no quadrature."
                ));
            }
            let (area, gradient) = geometry::<N, D>(cell.nodes.map(|node| self.nodes[node]))?;
            for &node in &cell.nodes {
                nodal_area[node] += area / N as f64;
            }
            for (point_index, point) in cell.quadrature.iter().enumerate() {
                let coefficients = bound.evaluate(&point.symbols)?;
                crate::assembler::accumulate_scalar_volume(
                    self.form,
                    workspace,
                    &cell.nodes,
                    &point.basis,
                    &gradient,
                    area * point.weight,
                    coefficients,
                )?;
                if let Some((plan, samples)) = loads {
                    let values: Vec<[f64; 1]> = point.basis.iter().map(|v| [*v]).collect();
                    let basis: Vec<_> = (0..N).map(|i| crate::weak_form::BasisSample {
                        value: &values[i], gradient: &gradient[i][..D], curl: &[],
                    }).collect();
                    let mut local = vec![C::default(); N];
                    plan.accumulate(&basis, &samples[index][point_index], area * point.weight, &mut local)?;
                    for (&node, value) in cell.nodes.iter().zip(local) {
                        system.rhs_re[node] += value.re;
                        system.rhs_im[node] += value.im;
                        absolute_loads[node] += value.re.hypot(value.im);
                        if !system.rhs_re[node].is_finite() || !system.rhs_im[node].is_finite() || !absolute_loads[node].is_finite() {
                            return Err(format!("Volume load accumulation is non-finite at node {node}."));
                        }
                    }
                }
            }
        }
        for point in self.boundaries {
            if point.nodes.iter().any(|&node| node >= self.nodes.len()) {
                return Err("Boundary point references a missing node.".into());
            }
            point.plan.require_shape(D, 1)?;
            let values: Vec<[f64; 1]> = point.basis.iter().map(|value| [*value]).collect();
            let basis: Vec<&[f64]> = values.iter().map(|value| value.as_slice()).collect();
            let mut local = vec![vec![C::default(); F]; F];
            let mut rhs = vec![C::default(); F];
            point.plan.accumulate_matrix(
                &basis,
                &point.normal,
                &point.coefficients,
                point.weight,
                &mut local,
            )?;
            point.plan.accumulate_load(
                &basis,
                &point.normal,
                &point.coefficients,
                &point.sources,
                point.weight,
                &mut rhs,
            )?;
            if point.plan.has_matrix_terms() {
                crate::assembler::scatter_local(workspace, &point.nodes, &local)?;
            }
            for (i, node) in point.nodes.iter().enumerate() {
                system.rhs_re[*node] += rhs[i].re;
                system.rhs_im[*node] += rhs[i].im;
                absolute_loads[*node] += rhs[i].re.hypot(rhs[i].im);
                if !system.rhs_re[*node].is_finite()
                    || !system.rhs_im[*node].is_finite()
                    || !absolute_loads[*node].is_finite()
                {
                    return Err(format!(
                        "Boundary load accumulation is non-finite at node {node}."
                    ));
                }
            }
        }
        let physical_rhs = self
            .mean
            .as_ref()
            .map(|_| (system.rhs_re.clone(), system.rhs_im.clone()));
        if self.periodic.is_some() {
            (system.matrix, system.rhs_re, system.rhs_im) = workspace.reduce_weighted(
                &system.rhs_re,
                &system.rhs_im,
                representative,
                weights,
                field_dofs,
            )?;
        } else {
            system.matrix = workspace.rows()?;
        }
        let mut essential = HashMap::new();
        if self.periodic.is_some() && !self.essential.is_empty() && self.essential_plan.is_none() {
            return Err("Periodic essential values need a declared essential form.".into());
        }
        if let Some(plan) = self.essential_plan {
            plan.require_shape(D, 1)?;
        }
        for &(node, value) in self.essential {
            if node >= self.nodes.len() {
                return Err("Essential value references a missing node.".into());
            }
            let constraint_weight = if let Some(map) = &self.periodic {
                match map.essential_mapping {
                    EssentialMapping::WeightedPhysical => weights[node],
                    EssentialMapping::DirectRepresentative => {
                        if (weights[node].re - 1.0).abs() > map.zero_phase_tolerance
                            || weights[node].im.abs() > map.zero_phase_tolerance
                        {
                            return Err(
                                "Direct representative constraints require a zero-phase map."
                                    .into(),
                            );
                        }
                        C { re: 1.0, im: 0.0 }
                    }
                }
            } else {
                C { re: 1.0, im: 0.0 }
            };
            let dof = representative[node];
            if let Some(plan) = self.essential_plan {
                plan.insert_dof(
                    &mut essential,
                    dof,
                    value,
                    constraint_weight,
                    "essential constraint",
                )?;
            } else {
                if !value.re.is_finite() || !value.im.is_finite() {
                    return Err("Essential value must be finite.".into());
                }
                if essential.insert(dof, value).is_some() {
                    return Err("Repeated essential values need a declared essential form.".into());
                }
            }
        }

        let mut means = Vec::<Vec<(usize, f64)>>::new();
        if let Some(policy) = &self.mean {
            let (physical_rhs_re, physical_rhs_im) = physical_rhs
                .as_ref()
                .expect("mean requires physical load snapshot");
            if !policy.value.re.is_finite()
                || !policy.value.im.is_finite()
                || !policy.balance_relative_tolerance.is_finite()
                || policy.balance_relative_tolerance < 0.0
            {
                return Err("Mean constraint value and tolerance must be finite and valid.".into());
            }
            let phase_tolerance = self
                .periodic
                .as_ref()
                .map(|map| map.zero_phase_tolerance)
                .unwrap_or(0.0);
            if weights.iter().any(|weight| {
                (weight.re - 1.0).abs() > phase_tolerance || weight.im.abs() > phase_tolerance
            }) {
                return Err(
                    "A constant mean constraint requires zero-phase periodic weights.".into(),
                );
            }
            if nodal_area
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
            {
                return Err(
                    "Mean constraint requires all mesh nodes to belong to a nondegenerate cell."
                        .into(),
                );
            }
            let mut parent: Vec<usize> = (0..field_dofs).collect();
            for cell in self.cells {
                for &node in &cell.nodes[1..] {
                    join(
                        &mut parent,
                        representative[cell.nodes[0]],
                        representative[node],
                    );
                }
            }
            let mut areas = vec![0.0; field_dofs];
            let mut loads = vec![0.0; field_dofs];
            let mut physical_re = vec![0.0; field_dofs];
            let mut physical_im = vec![0.0; field_dofs];
            for (node, &dof) in representative.iter().enumerate() {
                areas[dof] += nodal_area[node];
                loads[dof] += absolute_loads[node];
                physical_re[dof] += physical_rhs_re[node];
                physical_im[dof] += physical_rhs_im[node];
            }
            let mut components = BTreeMap::<usize, Vec<(usize, f64)>>::new();
            for (dof, &area) in areas.iter().enumerate() {
                let component = root(&mut parent, dof);
                components.entry(component).or_default().push((dof, area));
            }
            let mut anchored = std::collections::HashSet::new();
            for &dof in essential.keys() {
                if dof >= field_dofs {
                    return Err("Essential value references a missing DOF.".into());
                }
                anchored.insert(root(&mut parent, dof));
            }
            for (component, members) in components {
                if anchored.contains(&component) {
                    continue;
                }
                let mut net = C::default();
                let mut total_load = 0.0;
                for &(dof, _) in &members {
                    net.re += physical_re[dof];
                    net.im += physical_im[dof];
                    total_load += loads[dof];
                    // A mean gauge is valid only if the constant vector is a
                    // null mode; e.g. a mass or Robin term may remove it.
                    let mut row_sum = C::default();
                    let mut row_scale = 0.0;
                    for value in system.matrix[dof].values() {
                        row_sum = c_add(row_sum, *value);
                        row_scale += value.re.hypot(value.im);
                    }
                    if row_sum.re.hypot(row_sum.im) > 1e-10 * row_scale {
                        return Err(format!(
                            "Component {component} does not have a constant nullspace for a mean constraint."
                        ));
                    }
                }
                if !net.re.is_finite()
                    || !net.im.is_finite()
                    || !total_load.is_finite()
                    || net.re.hypot(net.im) > policy.balance_relative_tolerance * total_load
                {
                    return Err(format!(
                        "Component {component} has an incompatible net load."
                    ));
                }
                means.push(members);
            }
        }
        crate::scalar2d::apply_complex_dirichlet(
            &mut system.matrix,
            &mut system.rhs_re,
            &mut system.rhs_im,
            &essential,
        )?;
        if let Some(policy) = &self.mean {
            system.mean_constraints = means.len();
            for members in means {
                let total_area: f64 = members.iter().map(|(_, area)| area).sum();
                if !total_area.is_finite() || total_area <= 0.0 {
                    return Err("Invalid mean constraint area.".into());
                }
                let row = system.matrix.len();
                system.matrix.push(BTreeMap::new());
                system.rhs_re.push(policy.value.re);
                system.rhs_im.push(policy.value.im);
                for (dof, area) in members {
                    let weight = C {
                        re: area / total_area,
                        im: 0.0,
                    };
                    system.matrix[dof].insert(row, weight);
                    system.matrix[row].insert(dof, weight);
                }
            }
        }
        if system
            .rhs_re
            .iter()
            .chain(&system.rhs_im)
            .any(|value| !value.is_finite())
            || system
                .matrix
                .iter()
                .flat_map(|row| row.values())
                .any(|value| !value.re.is_finite() || !value.im.is_finite())
        {
            return Err("The assembled scalar system contains non-finite values.".into());
        }
        Ok(system)
    }

    pub(crate) fn solve(
        &self,
        options: &SolveOptions<'_>,
        cache: &mut SolverCache,
    ) -> Result<Solution, String> {
        self.solve_with_volume_loads(options, cache, None)
    }

    pub(crate) fn solve_with_volume_loads(
        &self, options: &SolveOptions<'_>, cache: &mut SolverCache,
        loads: Option<(&crate::weak_form::VolumeLoadPlan, &[Vec<Vec<C>>])>,
    ) -> Result<Solution, String> {
        let system = self.assemble_with_volume_loads(&mut cache.assembly, loads)?;
        let (real, imaginary, residual) = solve_linear_system(
            &system.matrix,
            &system.rhs_re,
            &system.rhs_im,
            options.residual_tolerance,
            options.backend,
            options.ordering,
            options.threads,
            cache,
        )?;
        if real.len() != system.matrix.len() || imaginary.len() != system.matrix.len() {
            return Err("Scalar solution length does not match the assembled system.".into());
        }
        let auxiliary_real = real[system.field_dofs..].to_vec();
        let auxiliary_imaginary = imaginary[system.field_dofs..].to_vec();
        let (real, imaginary) = if let Some(map) = &self.periodic {
            crate::constraints::expand_weighted(
                &real[..system.field_dofs],
                &imaginary[..system.field_dofs],
                map.representative,
                map.weights,
                map.count,
            )?
        } else {
            (
                real[..system.field_dofs].to_vec(),
                imaginary[..system.field_dofs].to_vec(),
            )
        };
        Ok(Solution {
            real,
            imaginary,
            residual,
            field_dofs: system.field_dofs,
            mean_constraints: system.mean_constraints,
            auxiliary_real,
            auxiliary_imaginary,
        })
    }

    /// Recover one complex three-vector per cell from a P1 scalar gradient.
    /// The caller explicitly supplies the material sampling policy and values.
    pub(crate) fn recover_cells(
        &self,
        solution: &Solution,
        recovery: &RecoveryPlan,
        sampling: &str,
        symbols: &[HashMap<String, C>],
    ) -> Result<Vec<[C; 3]>, String> {
        recovery.require_policy(sampling, "cell")?;
        self.recover_raw_cells(solution, recovery, symbols, false)
    }

    /// Evaluate a translation-invariant gradient after subtracting one nodal
    /// reference, reducing cancellation when a field has a large offset.
    pub(crate) fn recover_cells_centered(
        &self,
        solution: &Solution,
        recovery: &RecoveryPlan,
        sampling: &str,
        symbols: &[HashMap<String, C>],
    ) -> Result<Vec<[C; 3]>, String> {
        recovery.require_policy(sampling, "cell")?;
        self.recover_raw_cells(solution, recovery, symbols, true)
    }

    /// Arithmetically average cell contributions at each incident node,
    /// preserving the existing field-output convention.
    pub(crate) fn recover_nodes(
        &self,
        solution: &Solution,
        recovery: &RecoveryPlan,
        sampling: &str,
        symbols: &[HashMap<String, C>],
    ) -> Result<Vec<[C; 3]>, String> {
        recovery.require_policy(sampling, "nodalArithmeticMean")?;
        let cells = self.recover_raw_cells(solution, recovery, symbols, false)?;
        let mut sums = vec![[C::default(); 3]; self.nodes.len()];
        let mut counts = vec![0usize; self.nodes.len()];
        for (cell, value) in self.cells.iter().zip(cells) {
            crate::field_output::accumulate_cell_vector(
                &mut sums,
                &mut counts,
                &cell.nodes,
                value,
            )?;
        }
        crate::field_output::average_nodal_vectors(&mut sums, &counts);
        Ok(sums)
    }

    fn recover_raw_cells(
        &self,
        solution: &Solution,
        recovery: &RecoveryPlan,
        symbols: &[HashMap<String, C>],
        centered: bool,
    ) -> Result<Vec<[C; 3]>, String> {
        if solution.real.len() != self.nodes.len()
            || solution.imaginary.len() != self.nodes.len()
            || symbols.len() != self.cells.len()
        {
            return Err("Recovery data does not match scalar P1 mesh dimensions.".into());
        }
        self.cells
            .iter()
            .zip(symbols)
            .map(|(cell, symbols)| {
                if cell.nodes.iter().any(|&node| node >= self.nodes.len()) {
                    return Err("Recovery cell references a missing node.".into());
                }
                let (_, gradient) = geometry::<N, D>(cell.nodes.map(|node| self.nodes[node]))?;
                let mut field_gradient = [C::default(); 3];
                let reference = if centered {
                    C {
                        re: solution.real[cell.nodes[0]],
                        im: solution.imaginary[cell.nodes[0]],
                    }
                } else {
                    C::default()
                };
                for (i, &node) in cell.nodes.iter().enumerate() {
                    for axis in 0..D {
                        let value = C {
                            re: (solution.real[node] - reference.re) * gradient[i][axis],
                            im: (solution.imaginary[node] - reference.im) * gradient[i][axis],
                        };
                        field_gradient[axis] = c_add(field_gradient[axis], value);
                    }
                }
                recovery.apply(field_gradient, symbols)
            })
            .collect()
    }
}


