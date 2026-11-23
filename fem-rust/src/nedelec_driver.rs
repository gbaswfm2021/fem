//! Prepared scenario-neutral tetrahedral Nedelec numerical driver.
//!
//! Connectivity creates canonical edge/face moment DOFs. Physical boundary
//! quadrature, material coefficients, moment values and periodic maps are
//! supplied by model adapters; no physics scenario or profile is selected here.
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use crate::{C, SolverCache, c_add, c_mul, c_scale};
use crate::nedelec;
#[cfg(test)]
use crate::nedelec::{EDGES, FACES};
use crate::weak_form::{Coefficient, FormPlan};
use crate::boundary_form::BoundaryFormPlan;
use crate::essential_form::EssentialFormPlan;
use crate::recovery::RecoveryPlan;

pub(crate) struct Topology { pub(crate) elements: Vec<Vec<usize>>, pub(crate) count: usize }

pub(crate) fn topology(tets: &[[usize; 4]], node_count: usize, order: u8) -> Result<Topology, String> {
    if !(order == 1 || order == 2) || node_count == 0 || tets.is_empty() {
        return Err("Nedelec topology requires tetrahedra and order 1 or 2.".into());
    }
    let numbering = crate::nedelec_dofs::DofTopology::tetrahedra(node_count,tets,order)
        .map_err(|_| "Nedelec tetrahedron has invalid or repeated nodes.".to_string())?;
    Ok(Topology { elements:numbering.element_dofs, count:numbering.count })
}

pub(crate) struct VolumePoint<'a> {
    pub(crate) point: [f64; 3],
    /// Physical quadrature weight.
    pub(crate) weight: f64,
    /// Shared when a coefficient is constant over several quadrature points.
    pub(crate) coefficients: Rc<HashMap<String, Coefficient<'a>>>,
}
pub(crate) struct BoundaryPoint<'a> {
    pub(crate) element: usize,
    pub(crate) point: [f64; 3],
    pub(crate) normal: [f64; 3],
    pub(crate) weight: f64,
    pub(crate) plan: &'a BoundaryFormPlan,
    pub(crate) coefficients: HashMap<String, C>,
    /// A missing source omits the declared load, while preserving any matrix term.
    pub(crate) sources: Option<HashMap<String, Vec<C>>>,
}
pub(crate) struct PeriodicMap<'a> {
    pub(crate) representative: &'a [usize],
    pub(crate) weights: &'a [C],
    pub(crate) count: usize,
}
pub(crate) struct EssentialMoment { pub(crate) raw_dof: usize, pub(crate) value: C }
pub(crate) struct Problem<'a> {
    pub(crate) nodes: &'a [[f64; 3]],
    pub(crate) tets: &'a [[usize; 4]],
    pub(crate) order: u8,
    pub(crate) topology: &'a Topology,
    /// Optional element basis data already prepared by a model adapter.
    pub(crate) prepared: Option<&'a [ElementBasis]>,
    pub(crate) quadrature: &'a [Vec<VolumePoint<'a>>],
    pub(crate) form: &'a FormPlan,
    pub(crate) boundaries: &'a [BoundaryPoint<'a>],
    pub(crate) periodic: &'a PeriodicMap<'a>,
    pub(crate) essential: &'a [EssentialMoment],
    pub(crate) essential_plan: &'a EssentialFormPlan,
    pub(crate) recovery: &'a RecoveryPlan,
}
pub(crate) struct LinearSystem { pub(crate) matrix: Vec<BTreeMap<usize, C>>, pub(crate) re: Vec<f64>, pub(crate) im: Vec<f64> }
pub(crate) struct Solution { pub(crate) real: Vec<f64>, pub(crate) imaginary: Vec<f64>, pub(crate) residual: f64 }

// Called after Problem::assemble has validated map lengths, representatives,
// finite nonzero weights, and representative coverage. A near-identity phase
// is a genuine complex transformation and must still use weighted reduction.
#[cfg(test)]
fn reduce_periodic_system(matrix: Vec<BTreeMap<usize,C>>, re: Vec<f64>, im: Vec<f64>, map: &PeriodicMap<'_>)
    -> Result<crate::constraints::SparseSystem,String> {
    let identity = map.count == matrix.len()
        && map.representative.iter().enumerate().all(|(i,&dof)|i==dof)
        && map.weights.iter().all(|weight|weight.re==1. && weight.im==0.);
    if identity { Ok((matrix,re,im)) }
    else { crate::constraints::reduce_weighted(&matrix,&re,&im,map.representative,map.weights,map.count) }
}

pub(crate) struct ElementBasis {
    pub(crate) tet: [usize; 4],
    pub(crate) points: [[f64; 3]; 4],
    pub(crate) geometry: crate::simplex::Geo,
    pub(crate) gradients: [[f64; 3]; 4],
    pub(crate) inverse: Option<[[f64; 20]; 20]>,
}

impl Problem<'_> {
    fn prepare_basis(&self, tet: [usize; 4], candidates: &[nedelec::Poly2]) -> Result<ElementBasis, String> {
        let points = crate::simplex::points_for(tet, self.nodes);
        let geometry = crate::simplex::geometry(points)?;
        let gradients = crate::simplex::grad_lambda(geometry);
        let inverse = if self.order == 2 { Some(nedelec::second_interpolation(geometry, tet, self.nodes, &candidates)?) } else { None };
        Ok(ElementBasis { tet, points, geometry, gradients, inverse })
    }

    fn basis(&self, prepared: &ElementBasis, candidates: &[nedelec::Poly2], point: [f64; 3]) -> Result<Vec<nedelec::Basis>, String> {
        if point.iter().any(|x| !x.is_finite()) { return Err("Nedelec sample point must be finite.".into()); }
        Ok(nedelec::bases_at(self.order, prepared.geometry, prepared.gradients, prepared.tet, self.nodes, prepared.inverse.as_ref(), candidates, point))
    }

    /// Prepare geometry and interpolation once per element for reuse by both
    /// assembly and point/field recovery. In particular, the order-two
    /// interpolation inverse must not be rebuilt for every sampled point.
    pub(crate) fn prepare_elements(&self) -> Result<Vec<ElementBasis>, String> {
        if !matches!(self.order, 1 | 2) || self.nodes.iter().flatten().any(|x| !x.is_finite())
            || self.tets.iter().flatten().any(|&node| node >= self.nodes.len()) {
            return Err("Nedelec basis preparation requires a finite mesh and order 1 or 2.".into());
        }
        let candidates = if self.order == 2 { nedelec::quadratic_candidates() } else { Vec::new() };
        self.tets.iter().map(|&tet| self.prepare_basis(tet, &candidates)).collect()
    }

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
        loads: &[(&crate::weak_form::VolumeLoadPlan, &[Vec<Vec<C>>])],
    ) -> Result<LinearSystem, String> {
        for (_, samples) in loads {
            if samples.len() != self.quadrature.len() || samples.iter().zip(self.quadrature).any(|(row, points)| row.len() != points.len()) {
                return Err("Volume load samples must match the cell quadrature.".into());
            }
        }
        let actual = topology(self.tets, self.nodes.len(), self.order)?;
        if self.nodes.iter().flatten().any(|x| !x.is_finite()) || self.topology.count != actual.count
            || self.topology.elements != actual.elements || self.quadrature.len() != self.tets.len() {
            return Err("Nedelec mesh, topology or quadrature is inconsistent.".into());
        }
        self.form.require_shape(3, 3)?;
        self.form.require_discretization(crate::discretization::Cell::Tetrahedron, crate::discretization::Family::Nedelec, 3, self.order as usize)?;
        self.essential_plan.require_shape(3, 3)?;
        self.essential_plan.require_tangential_moments()?;
        self.recovery.require_shape(3, 3)?;
        let periodic = self.periodic;
        if periodic.representative.len() != actual.count || periodic.weights.len() != actual.count || periodic.count == 0
            || periodic.representative.iter().any(|&i| i >= periodic.count)
            || periodic.weights.iter().any(|w| !w.re.is_finite() || !w.im.is_finite() || (w.re == 0. && w.im == 0.)) {
            return Err("Nedelec periodic DOF map is invalid.".into());
        }
        let mut seen = vec![false; periodic.count];
        for &index in periodic.representative { seen[index] = true; }
        if seen.contains(&false) { return Err("Nedelec periodic map has an unused representative.".into()); }
        workspace.prepare(actual.count, &actual.elements)?;
        let mut re = vec![0.; actual.count];
        let mut im = vec![0.; actual.count];
        let candidates = if self.order == 2 { nedelec::quadratic_candidates() } else { Vec::new() };
        let owned_prepared;
        let prepared = if let Some(prepared) = self.prepared {
            if prepared.len() != self.tets.len() || prepared.iter().zip(self.tets).any(|(basis, tet)|
                basis.tet != *tet || basis.points != tet.map(|node| self.nodes[node]) || basis.inverse.is_some() != (self.order == 2)) {
                return Err("Prepared Nedelec basis does not match the mesh or order.".into());
            }
            prepared
        } else {
            owned_prepared = self.tets.iter().map(|&tet| self.prepare_basis(tet, &candidates)).collect::<Result<Vec<_>, _>>()?;
            &owned_prepared
        };
        for (element, points) in self.quadrature.iter().enumerate() {
            if points.is_empty() { return Err("Nedelec cell lacks quadrature.".into()); }
            let dofs = &actual.elements[element];
            let mut local = vec![vec![C::default(); dofs.len()]; dofs.len()];
            for (point_index, point) in points.iter().enumerate() {
                let basis = self.basis(&prepared[element], &candidates, point.point)?;
                crate::nedelec_volume::accumulate_quadrature(self.form, &basis, &point.coefficients, point.weight, &mut local)?;
                for (plan, samples) in loads {
                    let functions: Vec<_> = basis.iter().map(|b| crate::weak_form::BasisSample { value: &b.value, gradient: &[], curl: &b.curl }).collect();
                    let mut load = vec![C::default(); dofs.len()];
                    plan.accumulate(&functions, &samples[element][point_index], point.weight, &mut load)?;
                    for (&dof, value) in dofs.iter().zip(load) {
                        re[dof] += value.re; im[dof] += value.im;
                        if !re[dof].is_finite() || !im[dof].is_finite() { return Err(format!("Volume load accumulation is non-finite at DOF {dof}.")); }
                    }
                }
            }
            crate::nedelec_volume::scatter_element_at(workspace, element, dofs, &local)?;
        }
        for point in self.boundaries {
            point.plan.require_shape(3, 3)?;
            let dofs = actual.elements.get(point.element).ok_or("Nedelec boundary references a missing cell.")?;
            let functions = self.basis(&prepared[point.element], &candidates, point.point)?;
            let basis: Vec<&[f64]> = functions.iter().map(|b| b.value.as_slice()).collect();
            let mut local = vec![vec![C::default(); dofs.len()]; dofs.len()];
            let mut load = vec![C::default(); dofs.len()];
            point.plan.accumulate_matrix(&basis, &point.normal, &point.coefficients, point.weight, &mut local)?;
            if let Some(sources) = &point.sources {
                point.plan.accumulate_load(&basis, &point.normal, &point.coefficients, sources, point.weight, &mut load)?;
            }
            crate::nedelec_volume::scatter_element_at(workspace, point.element, dofs, &local)?;
            for (&dof, value) in dofs.iter().zip(load) { re[dof] += value.re; im[dof] += value.im; }
        }
        let (mut matrix, mut re, mut im) = workspace.reduce_weighted(
            &re,
            &im,
            periodic.representative,
            periodic.weights,
            periodic.count,
        )?;
        let mut prescribed = HashMap::new();
        for moment in self.essential {
            let raw = moment.raw_dof;
            if raw >= actual.count { return Err("Nedelec essential moment references a missing DOF.".into()); }
            self.essential_plan.insert_dof(&mut prescribed, periodic.representative[raw], moment.value, periodic.weights[raw], "oriented moment")?;
        }
        for (i, row) in matrix.iter_mut().enumerate() {
            if let Some(&value) = prescribed.get(&i) {
                row.clear(); row.insert(i, C { re: 1., im: 0. }); re[i] = value.re; im[i] = value.im;
            } else { row.retain(|j, value| {
                if let Some(&fixed) = prescribed.get(j) { let correction = c_mul(*value, fixed); re[i] -= correction.re; im[i] -= correction.im; false } else { true }
            }); }
        }
        Ok(LinearSystem { matrix, re, im })
    }

    pub(crate) fn solve(&self, backend: &str, ordering: &str, threads: usize, tolerance: f64, cache: &mut SolverCache) -> Result<Solution, String> {
        self.solve_with_volume_loads(backend, ordering, threads, tolerance, cache, None)
    }

    pub(crate) fn solve_with_volume_loads(&self, backend: &str, ordering: &str, threads: usize, tolerance: f64, cache: &mut SolverCache,
        loads: Option<(&crate::weak_form::VolumeLoadPlan, &[Vec<Vec<C>>])>) -> Result<Solution, String> {
        let system = self.assemble_with_volume_loads(&mut cache.assembly, loads)?;
        let (real, imaginary, residual) = crate::solver_common::solve_linear_system(&system.matrix, &system.re, &system.im, tolerance, backend, ordering, threads, cache)?;
        let (real, imaginary) = crate::constraints::expand_weighted(&real, &imaginary, self.periodic.representative, self.periodic.weights, self.periodic.count)?;
        Ok(Solution { real, imaginary, residual })
    }

    /// Evaluate the primary field and its curl in the internal mesh frame.
    /// Callers using a different model frame must transform both quantities
    /// before applying a model-frame recovery plan.
    pub(crate) fn sample_raw_at(&self, solution: &Solution, element: usize, point: [f64; 3]) -> Result<([C; 3], [C; 3]), String> {
        if solution.real.len() != self.topology.count || solution.imaginary.len() != self.topology.count {
            return Err("Nedelec solution does not match the topology.".into());
        }
        let dofs = self.topology.elements.get(element).ok_or("Nedelec recovery references a missing cell.")?;
        if self.order != 1 && self.order != 2 { return Err("Nedelec recovery requires element order 1 or 2.".into()); }
        let tet = *self.tets.get(element).ok_or("Nedelec recovery references a missing cell.")?;
        if tet.iter().any(|&node| node >= self.nodes.len()) { return Err("Nedelec recovery references a missing node.".into()); }
        let candidates = if self.order == 2 { nedelec::quadratic_candidates() } else { Vec::new() };
        let owned;
        let prepared = if let Some(prepared) = self.prepared {
            let basis = prepared.get(element).ok_or("Prepared Nedelec basis is missing a cell.")?;
            if basis.tet != tet || basis.points != tet.map(|node| self.nodes[node]) || basis.inverse.is_some() != (self.order == 2) {
                return Err("Prepared Nedelec basis does not match the cell or order.".into());
            }
            basis
        } else {
            owned = self.prepare_basis(tet, &candidates)?;
            &owned
        };
        let basis = self.basis(prepared, &candidates, point)?;
        let mut value = [C::default(); 3];
        let mut curl = [C::default(); 3];
        for (&dof, function) in dofs.iter().zip(basis) { for axis in 0..3 {
            let coefficient = C { re: solution.real[dof], im: solution.imaginary[dof] };
            value[axis] = c_add(value[axis], c_scale(coefficient, function.value[axis]));
            curl[axis] = c_add(curl[axis], c_scale(coefficient, function.curl[axis]));
        }}
        Ok((value, curl))
    }

    pub(crate) fn recover_at(&self, solution: &Solution, element: usize, point: [f64; 3], symbols: &HashMap<String, C>) -> Result<([C; 3], [C; 3]), String> {
        let (value, curl) = self.sample_raw_at(solution, element, point)?;
        Ok((value, self.recovery.apply_curl(curl, symbols)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn exact_identity_keeps_sparse_storage_but_near_identity_reduces() {
        let make_matrix = || vec![BTreeMap::from([(0,C {re:2.,im:1.}),(1,C {re:3.,im:-2.})]),
            BTreeMap::from([(0,C {re:4.,im:0.}),(1,C {re:5.,im:1.})])];
        let representatives=[0,1];
        let unit=[C {re:1.,im:0.};2];
        let matrix=make_matrix(); let allocation=matrix.as_ptr();
        let identity=PeriodicMap {representative:&representatives,weights:&unit,count:2};
        let (unchanged,_,_)=reduce_periodic_system(matrix,vec![1.,2.],vec![0.,1.],&identity).unwrap();
        assert_eq!(unchanged.as_ptr(),allocation);
        let near=[C {re:1.,im:1e-6},C {re:1.,im:0.}];
        let map=PeriodicMap {representative:&representatives,weights:&near,count:2};
        let (actual,re,im)=reduce_periodic_system(make_matrix(),vec![1.,2.],vec![0.,1.],&map).unwrap();
        let (expected,er,ei)=crate::constraints::reduce_weighted(&make_matrix(),&[1.,2.],&[0.,1.],&representatives,&near,2).unwrap();
        assert_eq!(re,er); assert_eq!(im,ei);
        for i in 0..2 {for (j,value) in &expected[i] {
            assert_eq!((actual[i][j].re,actual[i][j].im),(value.re,value.im));
        }}
        assert_ne!(actual[0][&1].im,unchanged[0][&1].im);
    }

    #[test]
    fn topology_wrapper_uses_shared_numbering_and_retains_input_guards() {
        let cells = [[0,1,2,3],[2,1,0,4]];
        for order in [1,2] {
            let shared = crate::nedelec_dofs::DofTopology::tetrahedra(5,&cells,order).unwrap();
            let driver = topology(&cells,5,order).unwrap();
            assert_eq!(driver.count,shared.count);
            assert_eq!(driver.elements,shared.element_dofs);
        }
        assert!(topology(&[],5,1).is_err());
        assert!(topology(&[[0,1,2,3]],0,1).is_err());
        assert!(topology(&[[0,1,2,3]],4,3).is_err());
        assert!(topology(&[[0,1,1,3]],4,1).is_err());
    }

    fn keys(tet: [usize; 4], order: u8) -> Vec<(Vec<usize>, usize)> {
        let mut result = Vec::new();
        for [a,b] in EDGES { for mode in 0..order as usize {
            result.push((nedelec::entity_key(vec![tet[a],tet[b]]),mode));
        }}
        if order == 2 { for face in FACES { for mode in 0..2 {
            result.push((nedelec::entity_key(face.map(|i| tet[i]).to_vec()),mode));
        }}}
        result
    }

    #[test]
    fn both_orders_solve_complex_periodic_oriented_moments_and_recover() {
        let form = FormPlan::from_json(&json!({"dimension":3,"fieldComponents":3,
            "element":{"cell":"tetrahedron","family":"nedelec","fieldComponents":3,"orders":[1,2]},
            "terms":[{"test":"curl","trial":"curl","coefficient":"curl"},
                     {"test":"value","trial":"value","coefficient":"mass"}]})).unwrap();
        let essential_plan = EssentialFormPlan::from_json(&json!({"dimension":3,"fieldComponents":3,
            "trace":"tangential","functional":"orientedMoment","legacyInput":"zeroOnly",
            "legacyZeroTolerance":1e-14,"conflictRelativeTolerance":1e-9,"conflictScaleFloor":1e-12})).unwrap();
        let recovery = RecoveryPlan::from_json(&json!({"operator":"curl","transform":[[1,0,0],[0,1,0],[0,0,1]],
            "factor":1,"sampling":"cellCentroid","output":"cell"})).unwrap();
        let nodes = [[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];
        for order in [1,2] {
            let mut reference = keys([0,1,2,3], order); reference.sort();
            let mut previous: Option<([C;3],[C;3])> = None;
            for tet in [[0,1,2,3],[1,0,3,2]] {
                let tets = [tet];
                let topo = topology(&tets, nodes.len(), order).unwrap();
                let ranks: Vec<_> = keys(tet, order).iter().map(|key| reference.iter().position(|v| v == key).unwrap()).collect();
                let representative: Vec<_> = ranks.iter().map(|&rank| if rank < 2 { 0 } else { rank - 1 }).collect();
                let weights: Vec<_> = ranks.iter().map(|&rank| if rank == 1 { C { re:0., im:1. } } else { C { re:1., im:0. } }).collect();
                let periodic = PeriodicMap { representative: &representative, weights: &weights, count: topo.count - 1 };
                let free = periodic.count - 1;
                let moments: Vec<_> = (0..topo.count).filter(|&raw| representative[raw] != free).map(|raw| {
                    let reduced = representative[raw] as f64;
                    EssentialMoment { raw_dof: raw, value: c_mul(weights[raw], C { re: 0.2 + reduced * 0.1, im: -0.3 }) }
                }).collect();
                let quadrature = [crate::TET_Q.iter().map(|q| VolumePoint {
                    point: std::array::from_fn(|axis| (0..4).map(|i| q[i]*nodes[tet[i]][axis]).sum()),
                    weight: q[4], coefficients: Rc::new(HashMap::from([
                    ("curl".into(), Coefficient::Scalar(C { re:1., im:0. })),
                    ("mass".into(), Coefficient::Scalar(C { re:2., im:0.5 }))])) }).collect()];
                let problem = Problem { nodes: &nodes, tets: &tets, order, topology: &topo, prepared: None, quadrature: &quadrature,
                    form: &form, boundaries: &[], periodic: &periodic, essential: &moments,
                    essential_plan: &essential_plan, recovery: &recovery };
                let solution = problem.solve("faer", "default", 1, 1e-7, &mut SolverCache::default()).unwrap();
                assert!(solution.residual < 1e-7);
                for moment in &moments {
                    assert!((solution.real[moment.raw_dof] - moment.value.re).abs() < 1e-10);
                    assert!((solution.imaginary[moment.raw_dof] - moment.value.im).abs() < 1e-10);
                }
                // Independently contract the physical basis to check the one
                // unconstrained complex unknown after periodic reduction.
                let free_raw = representative.iter().position(|&value| value == free).unwrap();
                let candidates = if order == 2 { nedelec::quadratic_candidates() } else { Vec::new() };
                let prepared = problem.prepare_basis(tet, &candidates).unwrap();
                let source = [C { re:1., im:0.5 }, C { re:-0.4, im:0.2 }, C { re:0.3, im:-0.1 }];
                let load = crate::weak_form::VolumeLoadPlan::new(3, 3, crate::weak_form::Operator::Value).unwrap();
                let samples = vec![vec![source.to_vec(); quadrature[0].len()]];
                let mut workspace = crate::assembler::SparseAssemblyWorkspace::default();
                let unloaded = problem.assemble_with_volume_loads(&mut workspace, None).unwrap();
                let driven = problem.assemble_with_volume_loads(&mut workspace, Some((&load, &samples))).unwrap();
                for (base, with_load) in unloaded.matrix.iter().zip(&driven.matrix) {
                    assert_eq!(base.keys().collect::<Vec<_>>(), with_load.keys().collect::<Vec<_>>());
                    for (column, value) in base { assert_eq!((value.re,value.im), (with_load[column].re,with_load[column].im)); }
                }
                let mut expected_load = C::default();
                for point in &quadrature[0] {
                    let basis = problem.basis(&prepared, &candidates, point.point).unwrap();
                    for axis in 0..3 {
                        expected_load.re += point.weight * basis[free_raw].value[axis] * source[axis].re;
                        expected_load.im += point.weight * basis[free_raw].value[axis] * source[axis].im;
                    }
                }
                assert!((driven.re[free] - unloaded.re[free] - expected_load.re).abs() < 1e-10);
                assert!((driven.im[free] - unloaded.im[free] - expected_load.im).abs() < 1e-10);
                if order == 1 {
                    // On the unit tetrahedron, the integral of the canonical
                    // edge {2,3} is oriented by coordinates from vertex 3
                    // to vertex 2. Its Whitney basis integrates to
                    // (grad(lambda_2)-grad(lambda_3))/24 = [0,1,-1]/24.
                    // This is independent of basis evaluation and applies to
                    // either local tetrahedron orientation.
                    assert_eq!(reference.last().unwrap().0, vec![2,3]);
                    let exact_re = (source[1].re - source[2].re) / 24.;
                    let exact_im = (source[1].im - source[2].im) / 24.;
                    assert!((driven.re[free] - unloaded.re[free] - exact_re).abs() < 1e-10,
                        "tet {tet:?}, raw {free_raw}, actual {}, expected {exact_re}, ranks {ranks:?}", driven.re[free]-unloaded.re[free]);
                    assert!((driven.im[free] - unloaded.im[free] - exact_im).abs() < 1e-10);
                    // For this edge, integral |curl W|^2 = 2/3 and
                    // integral |W|^2 = 1/30. With the declared coefficients,
                    // its free diagonal is 11/15 + i/60. Subtracting the
                    // unloaded solution cancels all prescribed-moment terms.
                    let stiffness_re = 11. / 15.;
                    let stiffness_im = 1. / 60.;
                    let norm = stiffness_re*stiffness_re + stiffness_im*stiffness_im;
                    let expected_re = (exact_re*stiffness_re + exact_im*stiffness_im) / norm;
                    let expected_im = (exact_im*stiffness_re - exact_re*stiffness_im) / norm;
                    let solved = problem.solve_with_volume_loads("faer", "default", 1, 1e-7,
                        &mut SolverCache::default(), Some((&load, &samples))).unwrap();
                    assert!((solved.real[free_raw] - solution.real[free_raw] - expected_re).abs() < 1e-9);
                    assert!((solved.imaginary[free_raw] - solution.imaginary[free_raw] - expected_im).abs() < 1e-9);
                } else {
                    // A quadratic vector field is determined by its four
                    // vertex values and six edge-midpoint values. Integrating
                    // the tetrahedral P2 interpolant exactly gives weights
                    // -V/20 at vertices and V/5 at edge midpoints. This is
                    // independent of the assembly quadrature rule.
                    let mut integral = [0.; 3];
                    for &vertex in &tet {
                        let basis = problem.basis(&prepared, &candidates, nodes[vertex]).unwrap();
                        for axis in 0..3 { integral[axis] -= basis[free_raw].value[axis] / 120.; }
                    }
                    for [a,b] in EDGES {
                        let point = std::array::from_fn(|axis| (nodes[tet[a]][axis] + nodes[tet[b]][axis]) / 2.);
                        let basis = problem.basis(&prepared, &candidates, point).unwrap();
                        for axis in 0..3 { integral[axis] += basis[free_raw].value[axis] / 30.; }
                    }
                    let exact_re: f64 = (0..3).map(|axis| integral[axis]*source[axis].re).sum();
                    let exact_im: f64 = (0..3).map(|axis| integral[axis]*source[axis].im).sum();
                    assert!((driven.re[free] - unloaded.re[free] - exact_re).abs() < 1e-10);
                    assert!((driven.im[free] - unloaded.im[free] - exact_im).abs() < 1e-10);

                    // Multiplying the quadratic basis by physical x gives a
                    // cubic polynomial. Compare the production rule with an
                    // independent degree-three five-point tetrahedron rule.
                    let varying = vec![quadrature[0].iter().map(|point| source.iter()
                        .map(|value| C { re:value.re*point.point[0], im:value.im*point.point[0] })
                        .collect::<Vec<_>>()).collect::<Vec<_>>()];
                    let spatial = problem.assemble_with_volume_loads(&mut workspace, Some((&load, &varying))).unwrap();
                    let mut cubic_rule = vec![([0.25; 4], -2. / 15.)];
                    for vertex in 0..4 {
                        let mut bary = [1. / 6.; 4]; bary[vertex] = 0.5;
                        cubic_rule.push((bary, 3. / 40.));
                    }
                    let mut spatial_exact = C::default();
                    for (bary, weight) in cubic_rule {
                        let point = std::array::from_fn(|axis| (0..4).map(|i| bary[i]*nodes[tet[i]][axis]).sum());
                        let basis = problem.basis(&prepared, &candidates, point).unwrap();
                        for axis in 0..3 {
                            spatial_exact.re += weight * point[0] * basis[free_raw].value[axis] * source[axis].re;
                            spatial_exact.im += weight * point[0] * basis[free_raw].value[axis] * source[axis].im;
                        }
                    }
                    assert!((spatial.re[free] - unloaded.re[free] - spatial_exact.re).abs() < 1e-10);
                    assert!((spatial.im[free] - unloaded.im[free] - spatial_exact.im).abs() < 1e-10);
                }
                let mut diagonal = C::default();
                let mut fixed_load = C::default();
                for point in &quadrature[0] {
                    let basis = problem.basis(&prepared, &candidates, point.point).unwrap();
                    for raw in 0..topo.count {
                        let dot_curl: f64 = (0..3).map(|axis| basis[free_raw].curl[axis]*basis[raw].curl[axis]).sum();
                        let dot_value: f64 = (0..3).map(|axis| basis[free_raw].value[axis]*basis[raw].value[axis]).sum();
                        let coefficient = C { re: point.weight*(dot_curl + 2.*dot_value), im: point.weight*0.5*dot_value };
                        if raw == free_raw { diagonal = c_add(diagonal, coefficient); }
                        else {
                            let fixed = moments.iter().find(|m| m.raw_dof == raw).unwrap().value;
                            fixed_load = c_add(fixed_load, c_mul(coefficient, fixed));
                        }
                    }
                }
                let norm = diagonal.re*diagonal.re + diagonal.im*diagonal.im;
                let expected = C { re: -(fixed_load.re*diagonal.re + fixed_load.im*diagonal.im)/norm,
                    im: -(fixed_load.im*diagonal.re - fixed_load.re*diagonal.im)/norm };
                assert!((solution.real[free_raw] - expected.re).abs() < 1e-9);
                assert!((solution.imaginary[free_raw] - expected.im).abs() < 1e-9);
                let recovered = problem.recover_at(&solution, 0, [0.2,0.2,0.2], &HashMap::new()).unwrap();
                if let Some(first) = previous {
                    for axis in 0..3 {
                        for (a,b) in [(recovered.0[axis],first.0[axis]),(recovered.1[axis],first.1[axis])] {
                            assert!((a.re-b.re).abs() < 1e-9 && (a.im-b.im).abs() < 1e-9);
                        }
                    }
                } else { previous = Some(recovered); }
            }
        }
    }
}
