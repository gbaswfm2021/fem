//! Generic oriented tetrahedral edge-field adapter for transient forms.
//! Geometry and interpolation are prepared once; coefficients are sampled at
//! physical quadrature points and time. No scenario identities occur here.
use crate::nedelec::{self, Poly2};
use crate::nedelec_driver::{self, Topology};
use crate::simplex::{self, Geo};
use crate::solver_common::C;
use crate::transient_plan::{
    SampleLocation, SampledCoefficient, StateLayout, TransientAssemblyWorkspace, TransientPlan,
    TransientSource,
};
use crate::transient_step::TransientOperators;
use crate::weak_form::BasisSample;
use std::collections::HashMap;

struct PreparedElement {
    tet: [usize; 4],
    tag: usize,
    geometry: Geo,
    gradients: [[f64; 3]; 4],
    inverse: Option<[[f64; 20]; 20]>,
}
pub(crate) struct EdgeTransientMesh {
    nodes: Vec<[f64; 3]>,
    order: u8,
    topology: Topology,
    elements: Vec<PreparedElement>,
    candidates: Vec<Poly2>,
}

pub(crate) struct EdgeFacePoint {
    pub(crate) dofs: Vec<usize>,
    pub(crate) basis: Vec<[f64; 3]>,
    pub(crate) point: [f64; 3],
    pub(crate) normal: [f64; 3],
    pub(crate) weight: f64,
}

impl EdgeTransientMesh {
    pub(crate) fn prepare(
        nodes: &[[f64; 3]],
        cells: &[([usize; 4], usize)],
        order: u8,
    ) -> Result<Self, String> {
        if nodes.iter().flatten().any(|x| !x.is_finite()) {
            return Err("Transient edge mesh coordinates must be finite.".into());
        }
        let tets: Vec<_> = cells.iter().map(|cell| cell.0).collect();
        let topology = nedelec_driver::topology(&tets, nodes.len(), order)?;
        let candidates = if order == 2 {
            nedelec::quadratic_candidates()
        } else {
            Vec::new()
        };
        let mut elements = Vec::with_capacity(cells.len());
        for &(tet, tag) in cells {
            let geometry = simplex::geometry(simplex::points_for(tet, nodes))?;
            let gradients = simplex::grad_lambda(geometry);
            let inverse = if order == 2 {
                Some(nedelec::second_interpolation(
                    geometry,
                    tet,
                    nodes,
                    &candidates,
                )?)
            } else {
                None
            };
            elements.push(PreparedElement {
                tet,
                tag,
                geometry,
                gradients,
                inverse,
            });
        }
        Ok(Self {
            nodes: nodes.to_vec(),
            order,
            topology,
            elements,
            candidates,
        })
    }
    pub(crate) fn dof_count(&self) -> usize {
        self.topology.count
    }

    /// Evaluate a state field in its oriented finite-element basis at a point
    /// of the named cell. This preserves the side of an interface and the
    /// higher-order basis; visualization nodal averages cannot do either.
    pub(crate) fn field_at_point(
        &self, plan: &TransientPlan, layout: &StateLayout, state: &[C],
        field: usize, cell: usize, point: [f64; 3],
    ) -> Result<[C; 3], String> {
        let definition=plan.fields.get(field).ok_or("Transient result field index is out of range.")?;
        if definition.components!=3 || layout.field_count(field)!=Some(self.dof_count()) {
            return Err(format!("Oriented result layout differs from field {:?}.",definition.id));
        }
        let element=self.elements.get(cell).ok_or("Transient result cell index is out of range.")?;
        if point.iter().any(|x|!x.is_finite()) {return Err("Transient result point must be finite.".into());}
        for (local,&node) in element.tet.iter().enumerate() {
            let lambda=1.+simplex::dot(element.gradients[local],simplex::sub(point,self.nodes[node]));
            if !(-1e-9..=1.+1e-9).contains(&lambda) {
                return Err("Transient result point is outside its selected cell.".into());
            }
        }
        let coefficients=layout.field_state(field,state)?;
        let basis=nedelec::bases_at(self.order,element.geometry,element.gradients,element.tet,
            &self.nodes,element.inverse.as_ref(),&self.candidates,point);
        let mut result=[C::default();3];
        for (i,&dof) in self.topology.elements[cell].iter().enumerate() {
            let value=coefficients.get(dof).ok_or("Oriented result DOF is out of range.")?;
            if !value.re.is_finite()||!value.im.is_finite(){return Err("Transient result state contains a non-finite coefficient.".into());}
            for axis in 0..3 {result[axis].re+=basis[i].value[axis]*value.re;result[axis].im+=basis[i].value[axis]*value.im;}
        }
        Ok(result)
    }

    /// Integrate an arbitrary Cartesian trace into the canonical oriented
    /// moments of a unique exterior face. Used by both boundary and initial
    /// field checks so their orientation and quadrature are identical.
    pub(crate) fn exterior_face_moments(
        &self,
        mut face: [usize; 3],
        mut sample: impl FnMut([f64; 3]) -> Result<[C; 3], String>,
    ) -> Result<Vec<(usize, C)>, String> {
        face.sort_unstable();
        if face.windows(2).any(|pair| pair[0] == pair[1])
            || face.iter().any(|&node| node >= self.nodes.len())
        {
            return Err("Transient edge boundary face has invalid vertices.".into());
        }
        let owners = self
            .elements
            .iter()
            .enumerate()
            .filter(|(_, element)| face.iter().all(|node| element.tet.contains(node)))
            .collect::<Vec<_>>();
        if owners.len() != 1 {
            return Err("Transient edge boundary face must have one exterior owner.".into());
        }
        let (element_index, element) = owners[0];
        nedelec::face_tangential_moments(element.tet, face, &self.nodes, self.order, |point| {
            sample(point)
        })?
        .into_iter()
        .map(|(local, value)| {
            self.topology.elements[element_index]
                .get(local)
                .copied()
                .map(|dof| (dof, value))
                .ok_or("Transient face moment references a missing degree of freedom.".to_string())
        })
        .collect()
    }

    /// Quadrature and oriented edge bases on a unique exterior face.
    pub(crate) fn exterior_face_points(
        &self,
        mut face: [usize; 3],
    ) -> Result<Vec<EdgeFacePoint>, String> {
        face.sort_unstable();
        if face.windows(2).any(|pair| pair[0] == pair[1])
            || face.iter().any(|&node| node >= self.nodes.len())
        {
            return Err("Transient edge boundary face has invalid vertices.".into());
        }
        let owners = self
            .elements
            .iter()
            .enumerate()
            .filter(|(_, element)| face.iter().all(|node| element.tet.contains(node)))
            .collect::<Vec<_>>();
        if owners.len() != 1 {
            return Err("Transient edge boundary face must have one exterior owner.".into());
        }
        let (index, element) = owners[0];
        let p = face.map(|node| self.nodes[node]);
        let a = crate::simplex::sub(p[1], p[0]);
        let b = crate::simplex::sub(p[2], p[0]);
        let mut normal = crate::simplex::cross(a, b);
        let magnitude = crate::simplex::norm(normal);
        if !magnitude.is_finite() || magnitude <= 0. {
            return Err("Transient edge boundary face is degenerate.".into());
        }
        for v in &mut normal {
            *v /= magnitude;
        }
        let opposite = *element
            .tet
            .iter()
            .find(|node| !face.contains(node))
            .ok_or("Transient face has no opposite vertex.")?;
        if crate::simplex::dot(normal, crate::simplex::sub(self.nodes[opposite], p[0])) > 0. {
            for v in &mut normal {
                *v = -*v;
            }
        }
        let mut result = Vec::new();
        for q in crate::TRI_Q {
            let point = std::array::from_fn(|axis| (0..3).map(|i| q[i] * p[i][axis]).sum());
            let basis = nedelec::bases_at(
                self.order,
                element.geometry,
                element.gradients,
                element.tet,
                &self.nodes,
                element.inverse.as_ref(),
                &self.candidates,
                point,
            )
            .into_iter()
            .map(|v| v.value)
            .collect();
            result.push(EdgeFacePoint {
                dofs: self.topology.elements[index].clone(),
                basis,
                point,
                normal,
                weight: magnitude * 0.5 * q[3],
            });
        }
        Ok(result)
    }

    /// Do not attach an edge state to a different mesh when publishing it.
    pub(crate) fn matches_geometry(
        &self,
        nodes: &[[f64; 3]],
        cells: &[(Vec<usize>, usize)],
    ) -> bool {
        self.nodes == nodes
            && self.elements.len() == cells.len()
            && self
                .elements
                .iter()
                .zip(cells)
                .all(|(element, (vertices, tag))| {
                    element.tet.as_slice() == vertices.as_slice() && element.tag == *tag
                })
    }

    /// Recover a physical vector at each cell centroid from oriented DOFs.
    /// These are cell samples, not node values; scenario result metadata
    /// determines their names and units.
    pub(crate) fn cell_field_values(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        state: &[C],
        field: usize,
    ) -> Result<Vec<[C; 3]>, String> {
        let definition = plan
            .fields
            .get(field)
            .ok_or("Transient result field index is out of range.")?;
        if definition.components != 3 || layout.field_count(field) != Some(self.dof_count()) {
            return Err(format!(
                "Oriented result layout differs from field {:?}.",
                definition.id
            ));
        }
        let mut output = Vec::with_capacity(self.elements.len());
        for (index, element) in self.elements.iter().enumerate() {
            let point = std::array::from_fn(|axis| {
                element
                    .tet
                    .iter()
                    .map(|&node| self.nodes[node][axis])
                    .sum::<f64>()
                    / 4.
            });
            output.push(self.field_at_point(plan,layout,state,field,index,point)?);
        }
        Ok(output)
    }

    /// Visualization-oriented nodal averages of element traces. An edge
    /// field need not be continuous in its normal component, so the values
    /// are averaged only for display; the underlying state remains oriented.
    pub(crate) fn nodal_field_values(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        state: &[C],
        field: usize,
    ) -> Result<Vec<[C; 3]>, String> {
        let definition = plan
            .fields
            .get(field)
            .ok_or("Transient result field index is out of range.")?;
        if definition.components != 3 || layout.field_count(field) != Some(self.dof_count()) {
            return Err(format!(
                "Oriented result layout differs from field {:?}.",
                definition.id
            ));
        }
        let coefficients = layout.field_state(field, state)?;
        let mut sums = vec![[C::default(); 3]; self.nodes.len()];
        let mut counts = vec![0usize; self.nodes.len()];
        for (index, element) in self.elements.iter().enumerate() {
            for &node in &element.tet {
                let basis = nedelec::bases_at(
                    self.order,
                    element.geometry,
                    element.gradients,
                    element.tet,
                    &self.nodes,
                    element.inverse.as_ref(),
                    &self.candidates,
                    self.nodes[node],
                );
                for (i, &dof) in self.topology.elements[index].iter().enumerate() {
                    let value = coefficients
                        .get(dof)
                        .ok_or("Oriented result DOF is out of range.")?;
                    if !value.re.is_finite() || !value.im.is_finite() {
                        return Err(
                            "Transient result state contains a non-finite coefficient.".into()
                        );
                    }
                    for axis in 0..3 {
                        sums[node][axis].re += basis[i].value[axis] * value.re;
                        sums[node][axis].im += basis[i].value[axis] * value.im;
                    }
                }
                counts[node] += 1;
            }
        }
        for (node, &count) in counts.iter().enumerate() {
            if count == 0 {
                return Err(format!(
                    "Transient result node {node} is not used by any element."
                ));
            }
            for axis in 0..3 {
                sums[node][axis].re /= count as f64;
                sums[node][axis].im /= count as f64;
            }
        }
        Ok(sums)
    }

    /// Lift an oriented mesh-DOF map to all declared fields in this adapter.
    /// Face correspondence, periodic factors and orientation are provided by
    /// the shared mesh constraint machinery.
    pub(crate) fn constraint_map(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        representative: &[usize],
        weights: &[C],
        count: usize,
    ) -> Result<(Vec<usize>, Vec<C>, usize), String> {
        if representative.len() != self.dof_count() {
            return Err("Oriented constraint map does not match the mesh.".into());
        }
        let mut blocks = Vec::with_capacity(plan.fields.len());
        for (index, field) in plan.fields.iter().enumerate() {
            if field.components != 3 || layout.field_count(index) != Some(self.dof_count()) {
                return Err(format!(
                    "Constraint map differs from oriented field {:?}.",
                    field.id
                ));
            }
            blocks.push(crate::constraints::DofBlock {
                representative,
                weights,
                count,
            });
        }
        crate::constraints::join_field_maps(&blocks)
    }

    /// Convert already matched zero-phase surface nodes to oriented edge and
    /// face relations, then lift those relations to every declared field.
    pub(crate) fn periodic_constraint_map(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        pairs: &[crate::scalar2d::PeriodicPairNodes],
        facets: &[(Vec<usize>, usize)],
    ) -> Result<(Vec<usize>, Vec<C>, usize), String> {
        let cells = self
            .elements
            .iter()
            .map(|element| element.tet)
            .collect::<Vec<_>>();
        let topology =
            crate::nedelec_dofs::DofTopology::tetrahedra(self.nodes.len(), &cells, self.order)?;
        if topology.count != self.topology.count || topology.element_dofs != self.topology.elements
        {
            return Err(
                "Transient oriented periodic topology differs from the assembly topology.".into(),
            );
        }
        let mut relations = Vec::new();
        for pair in pairs {
            let faces = |tag| {
                facets
                    .iter()
                    .filter(|(_, candidate)| *candidate == tag)
                    .map(|(nodes, _)| {
                        nodes.clone().try_into().map_err(|_| {
                            "Transient periodic surface requires triangular faces.".to_string()
                        })
                    })
                    .collect::<Result<Vec<[usize; 3]>, _>>()
            };
            let lower = faces(pair.lower_tag)?;
            let upper = faces(pair.upper_tag)?;
            for &face in lower.iter().chain(&upper) {
                self.exterior_face_points(face)?;
            }
            relations.extend(crate::nedelec_periodic::matched_surface_relations(
                &topology,
                &self.nodes,
                self.order,
                &lower,
                &upper,
                &pair.node_map,
                C { re: 1., im: 0. },
                1e-8,
            )?);
        }
        let map = topology.constrain(&relations)?;
        self.constraint_map(plan, layout, &map.representative, &map.weights, map.count)
    }

    /// Assemble a sparse L2 projection of spatial initial fields into the
    /// oriented space. Standard tetrahedral quadrature is required to avoid
    /// under-integrating the order-two mass matrix. Constraints are reduced
    /// and applied by the caller before solving the projection system.
    pub(crate) fn initial_projection(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        start: f64,
        mut sample: impl FnMut(&str, [f64; 3], f64) -> Result<Vec<C>, String>,
    ) -> Result<(crate::transient_step::SparseMatrix, Vec<C>), String> {
        if plan.dimension != 3 || !start.is_finite() {
            return Err("Transient initial edge projection needs 3D and finite time.".into());
        }
        for (index, field) in plan.fields.iter().enumerate() {
            if field.components != 3 || layout.field_count(index) != Some(self.topology.count) {
                return Err(format!(
                    "Initial oriented layout differs from field {:?}.",
                    field.id
                ));
            }
        }
        let mut mass = vec![std::collections::BTreeMap::new(); layout.dimension];
        let mut rhs = vec![C::default(); layout.dimension];
        let quadrature = crate::quadrature::tetrahedron("standard")?;
        for (index, element) in self.elements.iter().enumerate() {
            let local = &self.topology.elements[index];
            for q in &quadrature {
                let point = std::array::from_fn(|axis| {
                    (0..4)
                        .map(|i| q[i] * self.nodes[element.tet[i]][axis])
                        .sum()
                });
                let basis = nedelec::bases_at(
                    self.order,
                    element.geometry,
                    element.gradients,
                    element.tet,
                    &self.nodes,
                    element.inverse.as_ref(),
                    &self.candidates,
                    point,
                );
                let weight = element.geometry.det.abs() / 6. * q[4];
                for (field, definition) in plan.fields.iter().enumerate() {
                    let values = sample(&definition.id, point, start)?;
                    if values.len() != 3
                        || values
                            .iter()
                            .any(|v| !v.re.is_finite() || !v.im.is_finite())
                    {
                        return Err(format!(
                            "Initial field {:?} requires three finite components.",
                            definition.id
                        ));
                    }
                    let dofs = layout.global_dofs(field, local)?;
                    for (i, &row) in dofs.iter().enumerate() {
                        for (j, &column) in dofs.iter().enumerate() {
                            let value = weight
                                * (0..3)
                                    .map(|axis| basis[i].value[axis] * basis[j].value[axis])
                                    .sum::<f64>();
                            let entry = mass[row].entry(column).or_insert(C::default());
                            entry.re += value;
                        }
                        for (axis, value) in values.iter().enumerate() {
                            rhs[row].re += weight * basis[i].value[axis] * value.re;
                            rhs[row].im += weight * basis[i].value[axis] * value.im;
                        }
                    }
                }
            }

        }
        Ok((mass, rhs))
    }

    /// Assemble one time sample for fields using this common oriented space.
    /// Loads and boundaries are resolved from generic declared inputs by caller.
    pub(crate) fn assemble(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        time: f64,
        quadrature: &str,
        workspace: &mut TransientAssemblyWorkspace,
        mut coefficient: impl FnMut(
            SampleLocation,
        ) -> Result<HashMap<String, SampledCoefficient>, String>,
        sources: &[TransientSource<'_>],
        mut source_value: impl FnMut(&str, SampleLocation) -> Result<Vec<C>, String>,
    ) -> Result<TransientOperators, String> {
        if plan.dimension != 3 || !time.is_finite() {
            return Err("Transient oriented field needs 3D and finite time.".into());
        }
        for (field, definition) in plan.fields.iter().enumerate() {
            if definition.components != 3 || layout.field_count(field) != Some(self.topology.count)
            {
                return Err(format!(
                    "Transient oriented layout differs from field {:?}.",
                    definition.id
                ));
            }
        }
        let mut elements = Vec::with_capacity(self.elements.len());
        for local in &self.topology.elements {
            let mut all = Vec::new();
            for field in 0..plan.fields.len() {
                all.extend(layout.global_dofs(field, local)?);
            }
            elements.push(all);
        }
        workspace.prepare(plan, layout, &elements)?;
        let mut loads = if sources.is_empty() {
            None
        } else {
            Some(TransientOperators {
                derivatives: vec![],
                operator: vec![],
                load: vec![C::default(); layout.dimension],
                essential: vec![],
                essential_rates: vec![],
            })
        };
        let quadrature = crate::quadrature::tetrahedron(quadrature)?;
        for (index, element) in self.elements.iter().enumerate() {
            let local = &self.topology.elements[index];
            for q in &quadrature {
                let mut point = [0.; 3];
                for i in 0..4 {
                    for axis in 0..3 {
                        point[axis] += q[i] * self.nodes[element.tet[i]][axis];
                    }
                }
                let basis = nedelec::bases_at(
                    self.order,
                    element.geometry,
                    element.gradients,
                    element.tet,
                    &self.nodes,
                    element.inverse.as_ref(),
                    &self.candidates,
                    point,
                );
                let bases: Vec<Vec<_>> = (0..plan.fields.len())
                    .map(|_| {
                        basis
                            .iter()
                            .map(|b| BasisSample {
                                value: &b.value,
                                gradient: &[],
                                curl: &b.curl,
                            })
                            .collect()
                    })
                    .collect();
                let locals: Vec<_> = (0..plan.fields.len()).map(|_| local.to_vec()).collect();
                let weight = element.geometry.det.abs() / 6. * q[4];
                let location = SampleLocation {
                    cell: index,
                    material_tag: element.tag,
                    point,
                    time,
                };
                let coefficients = coefficient(location)?;
                plan.assemble_quadrature(
                    layout,
                    workspace,
                    &bases,
                    &locals,
                    weight,
                    &coefficients,
                )?;
                if let Some(accumulator) = loads.as_mut() {
                    plan.assemble_sources(
                        layout,
                        &bases,
                        &locals,
                        location,
                        weight,
                        sources,
                        &mut source_value,
                        accumulator,
                    )?;
                }
            }
        }
        let mut result = workspace.rows()?;
        if let Some(accumulator) = loads {
            result.load = accumulator.load;
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oriented_constraint_map_preserves_signs_across_mixed_fields() {
        let package: serde_json::Value =
            serde_json::from_str(include_str!("scenarios/em3d.json")).unwrap();
        let plan = TransientPlan::from_json(&package["transientForms"]["electric"]).unwrap();
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let mesh = EdgeTransientMesh::prepare(&nodes, &[([0, 1, 2, 3], 1)], 1).unwrap();
        let layout = StateLayout::new(&plan, &[6, 6]).unwrap();
        let one = C { re: 1., im: 0. };
        let minus = C { re: -1., im: 0. };
        let (map, weights, count) = mesh
            .constraint_map(
                &plan,
                &layout,
                &[0, 0, 1, 2, 3, 4],
                &[one, minus, one, one, one, one],
                5,
            )
            .unwrap();
        assert_eq!(map, vec![0, 0, 1, 2, 3, 4, 5, 5, 6, 7, 8, 9]);
        assert_eq!(weights[1].re, -1.);
        assert_eq!(weights[7].re, -1.);
        assert_eq!(count, 10);
    }
    #[test]
    fn initial_projection_reproduces_constant_oriented_fields_at_both_orders() {
        use crate::transient_step::{LinearOptions, project_initial_state};
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let declaration = serde_json::json!({"format":"fullmeta-transient-form/1","dimension":3,
            "fields":[{"id":"field","components":3,"unit":"V/m"}],"terms":[{"testField":"field","trialField":"field",
                "test":"value","trial":"value","coefficient":"one","timeDerivative":"state"}]});
        let plan = TransientPlan::from_json(&declaration).unwrap();
        let initial=crate::transient_initial::compile_fields(&plan,&serde_json::json!({"field":[
            {"expression":"1","unit":"V/m"},{"expression":"2","unit":"V/m"},{"expression":"3","unit":"V/m"}]})).unwrap();
        for order in [1, 2] {
            let mesh = EdgeTransientMesh::prepare(&nodes, &[([0, 1, 2, 3], 1)], order).unwrap();
            let layout = StateLayout::new(&plan, &[mesh.dof_count()]).unwrap();
            let (mass, rhs) = mesh
                .initial_projection(&plan, &layout, 0., |field, point, time| {
                    crate::transient_initial::sample_field(
                        &plan,
                        &initial,
                        field,
                        &HashMap::new(),
                        point,
                        time,
                    )
                })
                .unwrap();
            let solution = project_initial_state(
                &mass,
                &rhs,
                &[],
                &LinearOptions {
                    tolerance: 1e-10,
                    backend: "faer",
                    ordering: "amd",
                    threads: 1,
                },
                &mut crate::solver_common::SolverCache::default(),
            )
            .unwrap();
            let sampled = mesh
                .cell_field_values(&plan, &layout, &solution, 0)
                .unwrap();
            let nodal = mesh
                .nodal_field_values(&plan, &layout, &solution, 0)
                .unwrap();
            for axis in 0..3 {
                let value = sampled[0][axis].re;
                assert!(
                    (value - (axis + 1) as f64).abs() < 1e-9,
                    "order {order}, axis {axis}: {value}"
                );
                for value in &nodal {
                    assert!((value[axis].re - (axis + 1) as f64).abs() < 1e-9);
                }
            }
            for point in [[0.1,0.2,0.3],[0.7,0.1,0.1],[0.,0.,0.]] {
                let value=mesh.field_at_point(&plan,&layout,&solution,0,0,point).unwrap();
                for axis in 0..3 {assert!((value[axis].re-(axis+1) as f64).abs()<1e-9);}
            }
            assert!(mesh.field_at_point(&plan,&layout,&solution,0,0,[1.,1.,1.]).is_err());
            assert!(mesh.field_at_point(&plan,&layout,&solution,0,1,[0.,0.,0.]).is_err());
            if order==2 {
                let (mass,rhs)=mesh.initial_projection(&plan,&layout,0.,|_,point,_|
                    Ok(point.into_iter().map(|x|C{re:x,im:2.*x}).collect())).unwrap();
                let linear=project_initial_state(&mass,&rhs,&[],&LinearOptions{tolerance:1e-10,backend:"faer",ordering:"amd",threads:1},
                    &mut crate::solver_common::SolverCache::default()).unwrap();
                for point in [[0.1,0.2,0.3],[0.4,0.2,0.1]] {
                    let value=mesh.field_at_point(&plan,&layout,&linear,0,0,point).unwrap();
                    for axis in 0..3 {assert!((value[axis].re-point[axis]).abs()<1e-9);assert!((value[axis].im-2.*point[axis]).abs()<1e-9);}
                }
            }
        }
    }
    use serde_json::json;
    #[test]
    fn both_edge_orders_assemble_declared_sparse_mass() {
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let cells = [([0, 1, 2, 3], 1)];
        let plan=TransientPlan::from_json(&json!({"format":"fullmeta-transient-form/1","dimension":3,
            "fields":[{"id":"field","components":3}],"terms":[{"testField":"field","trialField":"field",
                "test":"value","trial":"value","coefficient":"mass","timeDerivative":"state"}]})).unwrap();
        for order in [1, 2] {
            let mesh = EdgeTransientMesh::prepare(&nodes, &cells, order).unwrap();
            let layout = StateLayout::new(&plan, &[mesh.dof_count()]).unwrap();
            let mut workspace = TransientAssemblyWorkspace::default();
            let result = mesh
                .assemble(
                    &plan,
                    &layout,
                    0.,
                    "standard",
                    &mut workspace,
                    |_| {
                        Ok(HashMap::from([(
                            "mass".into(),
                            SampledCoefficient::Scalar(C { re: 1., im: 0. }),
                        )]))
                    },
                    &[],
                    |_, _| unreachable!(),
                )
                .unwrap();
            assert_eq!(result.derivatives[0].1.len(), mesh.dof_count());
            assert!(
                result.derivatives[0]
                    .1
                    .iter()
                    .enumerate()
                    .all(|(i, row)| row[&i].re > 0.)
            );
            let reference =
                crate::weak_form::FormPlan::from_json(&json!({"dimension":3,"fieldComponents":3,
                "terms":[{"test":"value","trial":"value","coefficient":"mass"}]}))
                .unwrap();
            let coefficients = std::collections::HashMap::from([(
                "mass".to_string(),
                crate::weak_form::Coefficient::Scalar(C { re: 1., im: 0. }),
            )]);
            let element = &mesh.elements[0];
            let mut local = vec![vec![C::default(); mesh.dof_count()]; mesh.dof_count()];
            for q in crate::quadrature::tetrahedron("standard").unwrap() {
                let mut point = [0.; 3];
                for i in 0..4 {
                    for axis in 0..3 {
                        point[axis] += q[i] * nodes[i][axis];
                    }
                }
                let basis = nedelec::bases_at(
                    order,
                    element.geometry,
                    element.gradients,
                    element.tet,
                    &nodes,
                    element.inverse.as_ref(),
                    &mesh.candidates,
                    point,
                );
                crate::nedelec_volume::accumulate_quadrature(
                    &reference,
                    &basis,
                    &coefficients,
                    element.geometry.det.abs() / 6. * q[4],
                    &mut local,
                )
                .unwrap();
            }
            for i in 0..local.len() {
                for j in 0..local.len() {
                    assert!((result.derivatives[0].1[i][&j].re - local[i][j].re).abs() < 1e-10);
                }
            }
        }
    }

    #[test]
    fn enabled_em3d_form_preserves_uniform_conductive_decay() {
        use crate::transient_plan::TransientCoefficients;
        use crate::transient_step::{
            LinearOptions, integrate_backward_euler, project_initial_state,
        };
        use crate::weak_form::VolumeLoadPlan;
        let package: serde_json::Value =
            serde_json::from_str(include_str!("scenarios/em3d.json")).unwrap();
        let declaration = &package["transientForms"]["electric"];
        let plan = TransientPlan::from_json(declaration).unwrap();
        let coefficients =
            TransientCoefficients::from_json(&plan, &declaration["coefficients"]).unwrap();
        let nodes = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let mesh = EdgeTransientMesh::prepare(&nodes, &[([0, 1, 2, 3], 1)], 1).unwrap();
        let count = mesh.dof_count();
        let layout = StateLayout::new(&plan, &[count, count]).unwrap();
        let inputs = HashMap::from([
            (
                "eps_r".into(),
                C {
                    re: 1. / 8.8541878128e-12,
                    im: 0.,
                },
            ),
            (
                "mu_r".into(),
                C {
                    re: 1. / 1.25663706212e-6,
                    im: 0.,
                },
            ),
            ("sigma".into(), C { re: 1., im: 0. }),
        ]);
        let mut workspace = TransientAssemblyWorkspace::default();
        let load_form =
            VolumeLoadPlan::from_json(&json!({"dimension":3,"fieldComponents":3,"test":"value"}))
                .unwrap();
        let sources = [TransientSource {
            id: "projection",
            field: 0,
            form: &load_form,
        }];
        let projection = mesh
            .assemble(
                &plan,
                &layout,
                0.,
                "standard",
                &mut workspace,
                |_| coefficients.evaluate(&inputs),
                &sources,
                |_, _| Ok(vec![C { re: 1., im: 0. }, C::default(), C::default()]),
            )
            .unwrap();
        let mut mass = vec![std::collections::BTreeMap::new(); layout.dimension];
        for (_, matrix) in &projection.derivatives {
            for (i, row) in matrix.iter().enumerate() {
                for (&j, &value) in row {
                    let entry = mass[i].entry(j).or_insert(C::default());
                    *entry = crate::solver_common::c_add(*entry, value);
                }
            }
        }
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let initial = project_initial_state(
            &mass,
            &projection.load,
            &[],
            &options,
            &mut crate::solver_common::SolverCache::default(),
        )
        .unwrap();
        let initial_fields=crate::transient_initial::compile_fields(&plan,&json!({"electric":[
            {"expression":"1","unit":"V/m"},{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"}]})).unwrap();
        let grid = crate::transient_request::TimeGrid {
            start: 0.,
            end: 1.,
            max_step: 0.25,
            max_steps: 10,
            output_times: vec![1.],
        };
        let result_from_runner = crate::transient_runner::run_edge(
            &plan,
            &layout,
            &mesh,
            &grid,
            "standard",
            &HashMap::new(),
            &initial_fields,
            None,
            &mut workspace,
            &options,
            &mut crate::solver_common::SolverCache::default(),
            |_| coefficients.evaluate(&inputs),
            &[],
            |_, _| unreachable!(),
            |_, _| Ok(()),
            |_, _, _| Ok(()),
        )
        .unwrap();
        let scale = 1.0_f64 / 1.25_f64.powi(4);
        for i in 0..count {
            assert!((result_from_runner[i].re - scale * initial[i].re).abs() < 1e-9);
        }
        let driven = crate::transient_runner::run_edge(
            &plan,
            &layout,
            &mesh,
            &grid,
            "standard",
            &HashMap::new(),
            &HashMap::new(),
            None,
            &mut workspace,
            &options,
            &mut crate::solver_common::SolverCache::default(),
            |_| coefficients.evaluate(&inputs),
            &[],
            |_, _| unreachable!(),
            |time, operators| {
                crate::transient_runner::add_essential_values(
                    &plan,
                    &layout,
                    "electric",
                    &[(0, C { re: time, im: 0. })],
                    operators,
                )
            },
            |_, _, _| Ok(()),
        )
        .unwrap();
        assert!((driven[0].re - 1.).abs() < 1e-10);
        let result = integrate_backward_euler(
            0.,
            1.,
            0.25,
            10,
            &initial,
            &[1.],
            &options,
            &mut crate::solver_common::SolverCache::default(),
            |time| {
                mesh.assemble(
                    &plan,
                    &layout,
                    time,
                    "standard",
                    &mut workspace,
                    |_| coefficients.evaluate(&inputs),
                    &[],
                    |_, _| unreachable!(),
                )
            },
            |_, _, _| Ok(()),
        )
        .unwrap();
        for i in 0..count {
            assert!((result[i].re - scale * initial[i].re).abs() < 1e-9);
        }
        for value in &result[count..] {
            assert!(value.re.abs() < 1e-9);
        }
        let modulated = integrate_backward_euler(
            0.,
            1.,
            0.25,
            10,
            &initial,
            &[1.],
            &options,
            &mut crate::solver_common::SolverCache::default(),
            |time| {
                mesh.assemble(
                    &plan,
                    &layout,
                    time,
                    "standard",
                    &mut workspace,
                    |location| {
                        coefficients.evaluate(&HashMap::from([
                            (
                                "eps_r".into(),
                                C {
                                    re: (1. + location.time) / 8.8541878128e-12,
                                    im: 0.,
                                },
                            ),
                            (
                                "mu_r".into(),
                                C {
                                    re: 1. / 1.25663706212e-6,
                                    im: 0.,
                                },
                            ),
                            ("sigma".into(), C::default()),
                        ]))
                    },
                    &[],
                    |_, _| unreachable!(),
                )
            },
            |_, _, _| Ok(()),
        )
        .unwrap();
        for i in 0..count {
            assert!((modulated[i].re - 0.5 * initial[i].re).abs() < 1e-9);
        }
        for value in &modulated[count..] {
            assert!(value.re.abs() < 1e-9);
        }
    }
}
