//! Prepared, scenario-neutral vector P1 numerical runner.
//!
//! This accepts pre-evaluated quadrature data and declared forms. Model input,
//! topology validation, geometric periodic pairing and result presentation
//! still require an adapter before this can replace existing production runners.
use std::collections::{BTreeMap, HashMap};
use crate::{C, c_mul, SolverCache};
use crate::boundary_form::BoundaryFormPlan;
use crate::essential_form::EssentialFormPlan;
use crate::tensor_recovery::TensorRecoveryPlan;
use crate::weak_form::{Coefficient, FormPlan};

pub(crate) struct VolumePoint<'a, const N: usize> {
    pub(crate) basis: [f64; N],
    /// Normalized by physical cell area or volume.
    pub(crate) weight: f64,
    pub(crate) coefficients: HashMap<String, Coefficient<'a>>,
}

pub(crate) struct Cell<'a, const N: usize> {
    pub(crate) nodes: [usize; N],
    pub(crate) quadrature: Vec<VolumePoint<'a, N>>,
    /// Coefficient inputs for recovery at this cell's centroid.
    pub(crate) recovery_symbols: HashMap<String, C>,
}

pub(crate) struct BoundaryPoint<'a, const D: usize> {
    pub(crate) nodes: Vec<usize>,
    pub(crate) basis: Vec<f64>,
    pub(crate) normal: [f64; D],
    /// Physical facet quadrature weight.
    pub(crate) weight: f64,
    pub(crate) plan: &'a BoundaryFormPlan,
    pub(crate) coefficients: HashMap<String, C>,
    pub(crate) sources: HashMap<String, Vec<C>>,
}

pub(crate) struct EssentialPoint<const FC: usize> {
    pub(crate) node: usize,
    pub(crate) values: [C; FC],
}

/// Node identification with one complex phase shared by all Cartesian DOFs.
pub(crate) struct PeriodicMap<'a> {
    pub(crate) representative: &'a [usize],
    pub(crate) weights: &'a [C],
    pub(crate) count: usize,
}

pub(crate) struct Problem<'a, const D: usize, const FC: usize, const N: usize> {
    pub(crate) nodes: &'a [[f64; D]],
    pub(crate) cells: &'a [Cell<'a, N>],
    pub(crate) form: &'a FormPlan,
    pub(crate) boundaries: &'a [BoundaryPoint<'a, D>],
    pub(crate) essential: &'a [EssentialPoint<FC>],
    pub(crate) periodic: Option<PeriodicMap<'a>>,
    pub(crate) essential_plan: &'a EssentialFormPlan,
    pub(crate) recovery: &'a TensorRecoveryPlan,
}

pub(crate) struct LinearSystem {
    pub(crate) matrix: Vec<BTreeMap<usize, C>>,
    pub(crate) rhs_re: Vec<f64>,
    pub(crate) rhs_im: Vec<f64>,
}

pub(crate) struct Solution {
    pub(crate) real: Vec<f64>,
    pub(crate) imaginary: Vec<f64>,
    pub(crate) residual: f64,
    pub(crate) recovered_cells: Vec<Vec<C>>,
}

/// Compute Cartesian simplex shape gradients, keeping the same orientation
/// conventions for 2D and 3D. Node order may have either orientation.
fn geometry<const D: usize, const N: usize>(points: [[f64; D]; N]) -> Result<(f64, [[f64; D]; N]), String> {
    if N != D + 1 || !(D == 2 || D == 3) || points.iter().flatten().any(|x| !x.is_finite()) {
        return Err("Vector P1 requires finite 2D triangles or 3D tetrahedra.".into());
    }
    let embedded = |i: usize| -> [f64; 3] {
        std::array::from_fn(|axis| if axis < D { points[i][axis] } else { 0.0 })
    };
    let mut gradient = [[0.0; D]; N];
    let measure;
    if D == 2 {
        let (area, grad) = crate::mesh2d::triangle_geometry([embedded(0), embedded(1), embedded(2)])?;
        measure = area;
        for i in 0..N { for k in 0..D { gradient[i][k] = grad[i][k]; } }
    } else {
        let (volume, grad) = crate::simplex::tetra_p1(
            [embedded(0), embedded(1), embedded(2), embedded(3)],
            crate::simplex::DeterminantTolerance::Absolute(0.0),
        )?;
        measure = volume;
        for i in 0..N { for k in 0..D { gradient[i][k] = grad[i][k]; } }
    }
    Ok((measure, gradient))
}

impl<const D: usize, const FC: usize, const N: usize> Problem<'_, D, FC, N> {
    pub(crate) fn assemble(&self) -> Result<LinearSystem, String> {
        self.assemble_with_workspace(&mut crate::assembler::SparseAssemblyWorkspace::default())
    }

    pub(crate) fn assemble_with_workspace(
        &self,
        workspace: &mut crate::assembler::SparseAssemblyWorkspace,
    ) -> Result<LinearSystem, String> {
        self.assemble_with_volume_loads(workspace, None)
    }

    pub(crate) fn assemble_with_volume_loads(
        &self,
        workspace: &mut crate::assembler::SparseAssemblyWorkspace,
        loads: Option<(&crate::weak_form::VolumeLoadPlan, &[Vec<Vec<C>>])>,
    ) -> Result<LinearSystem, String> {
        let routes = loads.into_iter().collect::<Vec<_>>();
        self.assemble_with_volume_load_routes(workspace, &routes)
    }

    pub(crate) fn assemble_with_volume_load_routes(
        &self,
        workspace: &mut crate::assembler::SparseAssemblyWorkspace,
        loads: &[crate::weak_form::VolumeLoadSamples<'_>],
    ) -> Result<LinearSystem, String> {
        for (_, samples) in loads {
            if samples.len() != self.cells.len() || samples.iter().zip(self.cells).any(|(row, cell)| row.len() != cell.quadrature.len()) {
                return Err("Volume load samples must match the cell quadrature.".into());
            }
        }
        if self.nodes.is_empty() || self.cells.is_empty() || N != D + 1 || !(D == 2 || D == 3) || !(1..=3).contains(&FC) {
            return Err("Vector P1 problem requires nonempty 2D triangles or 3D tetrahedra.".into());
        }
        self.form.require_shape(D, FC)?;
        let cell_type = if D == 2 { crate::discretization::Cell::Triangle } else { crate::discretization::Cell::Tetrahedron };
        self.form.require_discretization(cell_type, crate::discretization::Family::Lagrange, FC, 1)?;
        self.essential_plan.require_shape(D, FC)?;
        self.recovery.require_shape(D, FC)?;
        let n = self.nodes.len().checked_mul(FC).ok_or("Vector P1 DOF count overflow.")?;
        let element_dofs: Vec<Vec<usize>> = self.cells.iter().map(|cell| {
            cell.nodes.iter().flat_map(|node| (0..FC).map(move |component| FC * node + component)).collect()
        }).collect();
        workspace.prepare(n, &element_dofs)?;
        let mut system = LinearSystem { matrix: Vec::new(), rhs_re: vec![0.; n], rhs_im: vec![0.; n] };
        for (cell_index, cell) in self.cells.iter().enumerate() {
            if cell.nodes.iter().any(|&node| node >= self.nodes.len()) || cell.quadrature.is_empty() {
                return Err("Vector P1 cell references a missing node or lacks quadrature.".into());
            }
            let (measure, gradients) = geometry(cell.nodes.map(|node| self.nodes[node]))?;
            for (point_index, point) in cell.quadrature.iter().enumerate() {
                crate::vector_form::add_declared_p1_quadrature_with_space::<D,FC,N,_>(
                    self.form, workspace, cell.nodes, gradients,
                    point.basis, measure * point.weight, &point.coefficients,
                )?;
                for (plan, samples) in loads {
                    let mut values = vec![vec![0.; FC]; N * FC];
                    let mut derivatives = vec![vec![0.; FC * D]; N * FC];
                    for a in 0..N { for component in 0..FC {
                        values[FC*a + component][component] = point.basis[a];
                        for axis in 0..D { derivatives[FC*a + component][component*D + axis] = gradients[a][axis]; }
                    } }
                    let basis: Vec<_> = (0..N*FC).map(|i| crate::weak_form::BasisSample { value: &values[i], gradient: &derivatives[i], curl: &[] }).collect();
                    let mut local = vec![C::default(); N * FC];
                    plan.accumulate(&basis, &samples[cell_index][point_index], measure * point.weight, &mut local)?;
                    for (i, value) in local.into_iter().enumerate() {
                        let dof = FC * cell.nodes[i / FC] + i % FC;
                        system.rhs_re[dof] += value.re; system.rhs_im[dof] += value.im;
                        if !system.rhs_re[dof].is_finite() || !system.rhs_im[dof].is_finite() { return Err(format!("Volume load accumulation is non-finite at DOF {dof}.")); }
                    }
                }
            }
        }
        for point in self.boundaries {
            if point.nodes.len() != D || point.nodes.iter().any(|&node| node >= self.nodes.len()) {
                return Err("Vector P1 boundary quadrature has invalid nodes or basis values.".into());
            }
            point.plan.require_shape(D, FC)?;
            let facet_nodes = point.nodes.len();
            if point.basis.len()!=facet_nodes {return Err("Vector P1 boundary basis differs from its facet nodes.".into());}
            let mut values = vec![vec![0.; FC]; facet_nodes * FC];
            for a in 0..facet_nodes { for component in 0..FC { values[FC*a+component][component] = point.basis[a]; } }
            let basis: Vec<&[f64]> = values.iter().map(Vec::as_slice).collect();
            let mut local = vec![vec![C::default(); facet_nodes*FC]; facet_nodes*FC];
            let mut load = vec![C::default(); facet_nodes*FC];
            point.plan.accumulate_matrix(&basis, &point.normal, &point.coefficients, point.weight, &mut local)?;
            point.plan.accumulate_load(&basis, &point.normal, &point.coefficients, &point.sources, point.weight, &mut load)?;
            let dofs: Vec<usize> = point.nodes.iter().flat_map(|node| (0..FC).map(move |component| FC*node+component)).collect();
            crate::assembler::scatter_local(workspace, &dofs, &local)?;
            for (&dof, value) in dofs.iter().zip(load) { system.rhs_re[dof] += value.re; system.rhs_im[dof] += value.im; }
        }
        let (representative, weights, reduced_count) = if let Some(periodic) = &self.periodic {
            if periodic.count == 0 || periodic.representative.len() != self.nodes.len() || periodic.weights.len() != self.nodes.len()
                || periodic.representative.iter().any(|&i| i >= periodic.count)
                || periodic.weights.iter().any(|w| !w.re.is_finite() || !w.im.is_finite() || (w.re == 0. && w.im == 0.)) {
                return Err("Invalid periodic vector node mapping.".into());
            }
            let mut seen = vec![false; periodic.count]; for &i in periodic.representative { seen[i] = true; }
            if seen.contains(&false) { return Err("Periodic vector map has an unused representative.".into()); }
            let representative: Vec<_> = periodic.representative.iter().flat_map(|&node| (0..FC).map(move |axis| FC*node+axis)).collect();
            let weights: Vec<_> = periodic.weights.iter().flat_map(|&w| (0..FC).map(move |_| w)).collect();
            let count = periodic.count.checked_mul(FC).ok_or("Periodic vector DOF count overflow.")?;
            (representative, weights, count)
        } else { ((0..n).collect(), vec![C { re:1., im:0. }; n], n) };
        if self.periodic.is_some() {
            (system.matrix, system.rhs_re, system.rhs_im) = workspace.reduce_weighted(
                &system.rhs_re, &system.rhs_im, &representative, &weights, reduced_count)?;
        } else {
            system.matrix = workspace.rows()?;
        }
        let mut prescribed = HashMap::new();
        for point in self.essential {
            if point.node >= self.nodes.len() { return Err("Vector essential point references a missing node.".into()); }
            let values = self.essential_plan.point_values(C::default(), Some(&point.values), "vector essential point")?;
            for (component, &value) in values.iter().enumerate() {
                let raw = FC*point.node+component;
                self.essential_plan.insert_dof(&mut prescribed, representative[raw], value, weights[raw], "vector essential point")?;
            }
        }
        for (row_index, row) in system.matrix.iter_mut().enumerate() {
            if let Some(&value) = prescribed.get(&row_index) {
                row.clear(); row.insert(row_index, C { re: 1., im: 0. });
                system.rhs_re[row_index] = value.re; system.rhs_im[row_index] = value.im;
            } else {
                row.retain(|column, coefficient| {
                    if let Some(&value) = prescribed.get(column) {
                        let correction = c_mul(*coefficient, value);
                        system.rhs_re[row_index] -= correction.re;
                        system.rhs_im[row_index] -= correction.im;
                        false
                    } else { true }
                });
            }
        }
        Ok(system)
    }

    /// Recover each cell's declared tensor components from its constant P1 gradient.
    pub(crate) fn recover(&self, real: &[f64], imaginary: &[f64]) -> Result<Vec<Vec<C>>, String> {
        if real.len() != self.nodes.len()*FC || imaginary.len() != real.len() {
            return Err("Vector P1 solution does not match the mesh.".into());
        }
        self.recovery.require_shape(D, FC)?;
        self.cells.iter().map(|cell| {
            if cell.nodes.iter().any(|&node| node >= self.nodes.len()) { return Err("Vector P1 cell references a missing node.".into()); }
            let (_, gradients) = geometry(cell.nodes.map(|node| self.nodes[node]))?;
            let operator_dimension=self.recovery.operator_dimension();
            let mut gradient = vec![C::default(); FC*operator_dimension];
            for a in 0..N { for component in 0..FC { for axis in 0..D {
                let dof = FC*cell.nodes[a]+component;
                gradient[operator_dimension*component+axis].re += real[dof]*gradients[a][axis];
                gradient[operator_dimension*component+axis].im += imaginary[dof]*gradients[a][axis];
            }}}
            if operator_dimension>D { for component in 0..FC {
                let mut mean=C::default();
                for &node in &cell.nodes {let dof=FC*node+component;mean.re+=real[dof]/N as f64;mean.im+=imaginary[dof]/N as f64;}
                gradient[operator_dimension*component+D]=self.recovery.longitudinal_derivative(mean);
            }}
            self.recovery.apply(&gradient, &cell.recovery_symbols)
        }).collect()
    }

    pub(crate) fn solve(&self, backend: &str, ordering: &str, threads: usize, tolerance: f64, cache: &mut SolverCache) -> Result<Solution, String> {
        self.solve_with_volume_loads(backend, ordering, threads, tolerance, cache, None)
    }

    pub(crate) fn solve_with_volume_loads(&self, backend: &str, ordering: &str, threads: usize, tolerance: f64, cache: &mut SolverCache,
        loads: Option<(&crate::weak_form::VolumeLoadPlan, &[Vec<Vec<C>>])>) -> Result<Solution, String> {
        let routes = loads.into_iter().collect::<Vec<_>>();
        self.solve_with_volume_load_routes(backend, ordering, threads, tolerance, cache, &routes)
    }

    pub(crate) fn solve_with_volume_load_routes(&self, backend: &str, ordering: &str, threads: usize, tolerance: f64, cache: &mut SolverCache,
        loads: &[crate::weak_form::VolumeLoadSamples<'_>]) -> Result<Solution, String> {
        let system = self.assemble_with_volume_load_routes(&mut cache.assembly, loads)?;
        let (real, imaginary, residual) = crate::solver_common::solve_linear_system(
            &system.matrix, &system.rhs_re, &system.rhs_im, tolerance, backend, ordering, threads, cache,
        )?;
        let (real, imaginary) = if let Some(periodic) = &self.periodic {
            let representative: Vec<_> = periodic.representative.iter().flat_map(|&node| (0..FC).map(move |axis| FC*node+axis)).collect();
            let weights: Vec<_> = periodic.weights.iter().flat_map(|&w| (0..FC).map(move |_| w)).collect();
            crate::constraints::expand_weighted(&real, &imaginary, &representative, &weights, periodic.count*FC)?
        } else { (real, imaginary) };
        let recovered_cells = self.recover(&real, &imaginary)?;
        Ok(Solution { real, imaginary, residual, recovered_cells })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn affine<const D: usize>(point: &[f64; D], component: usize) -> C {
        let re = (component + 1) as f64 + 2.0*point[0] - 3.0*point[1];
        C { re, im: -0.25*re }
    }

    fn patch<const D: usize, const N: usize>(nodes: Vec<[f64; D]>, connectivity: Vec<[usize; N]>) {
        let cell_kind = if D == 2 { "triangle" } else { "tetrahedron" };
        let form = FormPlan::from_json(&json!({"dimension":D,"fieldComponents":D,
            "element":{"cell":cell_kind,"family":"lagrange","fieldComponents":D,"orders":[1]},
            "terms":[{"test":"symmetricGradient","trial":"symmetricGradient","coefficient":"stiffness"}]
        })).unwrap();
        let essential_plan = EssentialFormPlan::from_json(&json!({"dimension":D,"fieldComponents":D,
            "trace":"value","functional":"point","legacyInput":"zeroOnly",
            "legacyZeroTolerance":1e-14,"conflictRelativeTolerance":1e-9,"conflictScaleFloor":1e-12
        })).unwrap();
        let mut normal_row = vec![0.; D*D]; normal_row[0] = 1.;
        let mut shear_row = vec![0.; D*D]; shear_row[1] = 1.;
        let recovery = TensorRecoveryPlan::from_json(&json!({"dimension":D,"fieldComponents":D,
            "operator":"symmetricGradient","sampling":"cellCentroid","output":"cell",
            "components":["normal","shear"],"coefficients":{"unit":1},"matrix":[normal_row,shear_row]
        })).unwrap();
        let cells: Vec<_> = connectivity.into_iter().map(|nodes| Cell {
            nodes,
            quadrature: vec![VolumePoint {
                basis: [1. / N as f64; N], weight: 1.,
                coefficients: HashMap::from([("stiffness".into(), Coefficient::Scalar(C { re: 2., im: 0. }))]),
            }], recovery_symbols: HashMap::new(),
        }).collect();
        let essential: Vec<_> = (0..N).map(|node| EssentialPoint::<D> {
            node, values: std::array::from_fn(|component| affine(&nodes[node], component)),
        }).collect();
        let problem = Problem { nodes: &nodes, cells: &cells, form: &form, boundaries: &[], essential: &essential,
            essential_plan: &essential_plan, recovery: &recovery, periodic:None };
        let system = problem.assemble().unwrap();
        assert_eq!(system.matrix.len(), nodes.len()*D);
        let load = crate::weak_form::VolumeLoadPlan::new(D, D, crate::weak_form::Operator::Value).unwrap();
        let samples = cells.iter().map(|_| vec![vec![C { re:1., im:0. }; D]]).collect::<Vec<_>>();
        let driven = problem.assemble_with_volume_loads(&mut crate::assembler::SparseAssemblyWorkspace::default(), Some((&load, &samples))).unwrap();
        for (base, with_load) in system.matrix.iter().zip(&driven.matrix) {
            assert_eq!(base.keys().collect::<Vec<_>>(), with_load.keys().collect::<Vec<_>>());
            for (column, value) in base { assert_eq!((value.re, value.im), (with_load[column].re, with_load[column].im)); }
        }
        assert!(system.rhs_re.iter().zip(&driven.rhs_re).any(|(a,b)| (a-b).abs() > 1e-12));
        // The interior block contains all vector components; rows remain sparse.
        for row in N*D..nodes.len()*D { assert!(system.matrix[row].len() <= D); }
        let solution = problem.solve("faer", "default", 1, 1e-7, &mut SolverCache::default()).unwrap();
        assert!(solution.residual < 1e-7);
        for node in 0..nodes.len() { for component in 0..D {
            let expected = affine(&nodes[node], component);
            assert!((solution.real[node*D+component] - expected.re).abs() < 1e-10);
            assert!((solution.imaginary[node*D+component] - expected.im).abs() < 1e-10);
        }}
        for cell in solution.recovered_cells {
            assert!((cell[0].re - 2.).abs() < 1e-10);
            assert!((cell[0].im + 0.5).abs() < 1e-10);
            assert!((cell[1].re + 0.5).abs() < 1e-10);
            assert!((cell[1].im - 0.125).abs() < 1e-10);
        }
    }

    #[test]
    fn affine_complex_patch_solves_and_recovers_in_2d_and_3d() {
        patch(vec![[0.,0.], [2.,0.], [0.,2.], [0.5,0.5]],
              vec![[0,1,3], [1,2,3], [2,0,3]]);
        patch(vec![[0.,0.,0.], [2.,0.,0.], [0.,2.,0.], [0.,0.,2.], [0.25,0.25,0.25]],
              vec![[0,1,2,4], [0,3,1,4], [1,3,2,4], [2,3,0,4]]);
    }

    #[test]
    fn complex_periodic_weights_apply_to_each_node_major_vector_component() {
        let form = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":2,
            "element":{"cell":"triangle","family":"lagrange","fieldComponents":2,"orders":[1]},
            "terms":[{"test":"value","trial":"value","coefficient":"mass"}]})).unwrap();
        let essential_plan = EssentialFormPlan::from_json(&json!({"dimension":2,"fieldComponents":2,
            "trace":"value","functional":"point","legacyInput":"zeroOnly","legacyZeroTolerance":0,
            "conflictRelativeTolerance":1e-10,"conflictScaleFloor":1})).unwrap();
        let recovery = TensorRecoveryPlan::from_json(&json!({"dimension":2,"fieldComponents":2,
            "operator":"symmetricGradient","sampling":"cellCentroid","output":"cell",
            "components":["a"],"coefficients":{"unit":1},"matrix":[[1,0,0,0]]})).unwrap();
        let nodes = [[0.,0.],[1.,0.],[0.,1.]];
        let cells = [Cell { nodes:[0,1,2], quadrature:vec![VolumePoint { basis:[1./3.;3], weight:1.,
            coefficients:HashMap::from([("mass".into(),Coefficient::Scalar(C { re:1.,im:0. }))]) }], recovery_symbols:HashMap::new() }];
        let prescribed = [EssentialPoint { node:0, values:[C { re:1.,im:2. },C { re:3.,im:-1. }] },
            EssentialPoint { node:1, values:[C { re:-2.,im:1. },C { re:1.,im:3. }] },
            EssentialPoint { node:2, values:[C::default();2] }];
        let representative = [0,0,1]; let weights = [C { re:1.,im:0. },C { re:0.,im:1. },C { re:1.,im:0. }];
        let problem = Problem { nodes:&nodes, cells:&cells, form:&form, boundaries:&[], essential:&prescribed,
            periodic:Some(PeriodicMap { representative:&representative, weights:&weights, count:2 }), essential_plan:&essential_plan, recovery:&recovery };
        let solution = problem.solve("faer","default",1,1e-7,&mut SolverCache::default()).unwrap();
        assert_eq!((solution.real[2],solution.imaginary[2]),(-2.,1.));
        assert_eq!((solution.real[3],solution.imaginary[3]),(1.,3.));
        let mut bad = prescribed; bad[1].values[0].re += 0.5;
        let conflict = Problem { nodes:&nodes,cells:&cells,form:&form,boundaries:&[],essential:&bad,
            periodic:Some(PeriodicMap { representative:&representative, weights:&weights, count:2 }),essential_plan:&essential_plan,recovery:&recovery };
        assert!(conflict.assemble().is_err());
    }
}
