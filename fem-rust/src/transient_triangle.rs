//! Scenario-neutral transient adapter for a 2D triangle direct-sum Whitney
//! space. Each declared vector field receives an independent copy of the
//! in-plane Whitney H(curl) edge functions plus the longitudinal nodal H1
//! functions. Coupling between copies comes only from the declared weak form.

use crate::solver_common::C;
use crate::transient_plan::{
    SampleLocation, SampledCoefficient, StateLayout, TransientAssemblyWorkspace,
    TransientPlan, TransientSource,
};
use crate::transient_step::TransientOperators;
use crate::weak_form::BasisSample;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone)]
struct Element {
    nodes: [usize; 3],
    tag: usize,
    area: f64,
    gradients: [[f64; 2]; 3],
}

/// Geometry and shared edge/node topology for the lowest-order 2D compatible
/// vector space. `StateLayout` makes a separate global block for each field.
pub(crate) struct TriangleTransientMesh {
    nodes: Vec<[f64; 3]>,
    xy: Vec<[f64; 2]>,
    elements: Vec<Element>,
    topology: crate::hcurl2d::DofTopology,
}

impl TriangleTransientMesh {
    pub(crate) fn prepare(
        nodes: &[[f64; 3]],
        cells: &[([usize; 3], usize)],
    ) -> Result<Self, String> {
        if nodes.is_empty() || cells.is_empty() || nodes.iter().flatten().any(|value| !value.is_finite()) {
            return Err("Transient triangle space requires finite, nonempty geometry.".into());
        }
        let xy = nodes.iter().map(|point| [point[0], point[1]]).collect::<Vec<_>>();
        let connectivity = cells.iter().map(|cell| cell.0).collect::<Vec<_>>();
        let topology = crate::hcurl2d::DofTopology::triangles(nodes.len(), &connectivity)?;
        let mut elements = Vec::with_capacity(cells.len());
        for (index, &(vertices, tag)) in cells.iter().enumerate() {
            let points = vertices.map(|node| [nodes[node][0], nodes[node][1], 0.]);
            let (area, gradients) = crate::mesh2d::triangle_geometry(points)?;
            if !area.is_finite() || area <= 0. {
                return Err(format!("Transient triangle {index} has nonpositive area."));
            }
            elements.push(Element { nodes: vertices, tag, area, gradients });
        }
        Ok(Self { nodes: nodes.to_vec(), xy, elements, topology })
    }

    pub(crate) fn dof_count(&self) -> usize { self.topology.count }
    pub(crate) fn node(&self,node:usize)->Option<[f64;3]>{self.nodes.get(node).copied()}

    pub(crate) fn edge_dof(&self, nodes:[usize;2])->Option<usize>{self.topology.edge(nodes)}
    pub(crate) fn longitudinal_node_dof(&self,node:usize)->Option<usize>{
        (node<self.nodes.len()).then_some(self.topology.edge_count+node)
    }

    /// Integrate a Cartesian vector field against the canonical oriented
    /// Whitney edge trace on one unique exterior segment.
    pub(crate) fn exterior_edge_moment(
        &self,mut edge:[usize;2],mut sample:impl FnMut([f64;3])->Result<[C;3],String>,
    )->Result<(usize,C),String>{
        edge.sort_unstable();
        if edge[0]==edge[1]||edge[1]>=self.nodes.len(){return Err("Transient triangle boundary edge has invalid vertices.".into());}
        let owners=self.elements.iter().enumerate().filter(|(_,element)|edge.iter().all(|node|element.nodes.contains(node)))
            .collect::<Vec<_>>();
        if owners.len()!=1{return Err("Transient triangle boundary edge must have exactly one exterior owner.".into());}
        let [a,b]=edge;let delta=[self.nodes[b][0]-self.nodes[a][0],self.nodes[b][1]-self.nodes[a][1]];
        let length=delta[0].hypot(delta[1]);
        if !length.is_finite()||length<=0.{return Err("Transient triangle boundary edge has zero length.".into());}
        let dof=self.topology.edge(edge).ok_or("Transient triangle boundary edge has no Whitney DOF.")?;
        let abscissa=[0.5-0.5/3f64.sqrt(),0.5+0.5/3f64.sqrt()];let mut value=C::default();
        for t in abscissa{
            let point=[self.nodes[a][0]+t*delta[0],self.nodes[a][1]+t*delta[1],self.nodes[a][2]+t*(self.nodes[b][2]-self.nodes[a][2])];
            let field=sample(point)?;
            if field.iter().any(|component|!component.re.is_finite()||!component.im.is_finite()){
                return Err("Transient triangle boundary field must be finite.".into());
            }
            value.re+=0.5*(field[0].re*delta[0]+field[1].re*delta[1]);
            value.im+=0.5*(field[0].im*delta[0]+field[1].im*delta[1]);
        }
        Ok((dof,value))
    }

    pub(crate) fn matches_geometry(&self, nodes: &[[f64; 3]], cells: &[(Vec<usize>, usize)]) -> bool {
        self.nodes == nodes && self.elements.len() == cells.len()
            && self.elements.iter().zip(cells).all(|(element, (vertices, tag))|
                element.nodes.as_slice() == vertices.as_slice() && element.tag == *tag)
    }

    fn basis(&self, element: &Element, barycentric: [f64; 3]) -> Result<(Vec<Vec<f64>>, Vec<Vec<f64>>), String> {
        crate::hcurl2d::basis(&self.xy, element.nodes, barycentric, element.gradients)
    }

    fn barycentric(&self, element: &Element, point: [f64; 3]) -> Result<[f64; 3], String> {
        if point.iter().any(|value| !value.is_finite()) {
            return Err("Transient triangle sample point must be finite.".into());
        }
        let barycentric = std::array::from_fn(|local| {
            1. + element.gradients[local][0] * (point[0] - self.nodes[element.nodes[local]][0])
                + element.gradients[local][1] * (point[1] - self.nodes[element.nodes[local]][1])
        });
        if barycentric.iter().any(|value| !(-1e-9..=1. + 1e-9).contains(value)) {
            return Err("Transient result point is outside its selected triangle.".into());
        }
        Ok(barycentric)
    }

    pub(crate) fn field_at_point(
        &self, plan: &TransientPlan, layout: &StateLayout, state: &[C], field: usize,
        cell: usize, point: [f64; 3],
    ) -> Result<[C; 3], String> {
        let definition = plan.fields.get(field).ok_or("Transient result field index is out of range.")?;
        if definition.components != 3 || layout.field_count(field) != Some(self.dof_count()) {
            return Err(format!("Triangle result layout differs from field {:?}.", definition.id));
        }
        let element = self.elements.get(cell).ok_or("Transient result cell index is out of range.")?;
        let barycentric = self.barycentric(element, point)?;
        let (values, _) = self.basis(element, barycentric)?;
        let coefficients = layout.field_state(field, state)?;
        let mut result = [C::default(); 3];
        for (local, &dof) in self.topology.elements[cell].iter().enumerate() {
            let coefficient = *coefficients.get(dof).ok_or("Transient result DOF is out of range.")?;
            if !coefficient.re.is_finite() || !coefficient.im.is_finite() {
                return Err("Transient result state contains a non-finite coefficient.".into());
            }
            for component in 0..3 {
                result[component].re += values[local][component] * coefficient.re;
                result[component].im += values[local][component] * coefficient.im;
            }
        }
        Ok(result)
    }

    pub(crate) fn cell_field_values(
        &self, plan: &TransientPlan, layout: &StateLayout, state: &[C], field: usize,
    ) -> Result<Vec<[C; 3]>, String> {
        self.elements.iter().enumerate().map(|(cell, element)| {
            let point = std::array::from_fn(|axis| element.nodes.iter().map(|&node| self.nodes[node][axis]).sum::<f64>() / 3.);
            self.field_at_point(plan, layout, state, field, cell, point)
        }).collect()
    }

    /// Visualization values are recovered by averaging element traces at
    /// vertices; the actual state remains in oriented edge/node DOFs.
    pub(crate) fn nodal_field_values(
        &self, plan: &TransientPlan, layout: &StateLayout, state: &[C], field: usize,
    ) -> Result<Vec<[C; 3]>, String> {
        let mut sums = vec![[C::default(); 3]; self.nodes.len()];
        let mut counts = vec![0usize; self.nodes.len()];
        for (cell, element) in self.elements.iter().enumerate() {
            for &node in &element.nodes {
                let value = self.field_at_point(plan, layout, state, field, cell, self.nodes[node])?;
                for axis in 0..3 {
                    sums[node][axis].re += value[axis].re;
                    sums[node][axis].im += value[axis].im;
                }
                counts[node] += 1;
            }
        }
        for (node, &count) in counts.iter().enumerate() {
            if count == 0 { return Err(format!("Transient result node {node} is unused.")); }
            for component in &mut sums[node] {
                component.re /= count as f64;
                component.im /= count as f64;
            }
        }
        Ok(sums)
    }

    pub(crate) fn constraint_map(
        &self, plan: &TransientPlan, layout: &StateLayout, representative: &[usize],
        weights: &[C], count: usize,
    ) -> Result<(Vec<usize>, Vec<C>, usize), String> {
        if representative.len() != self.dof_count() || weights.len() != self.dof_count() {
            return Err("Triangle constraint map does not match the mesh.".into());
        }
        let mut blocks = Vec::with_capacity(plan.fields.len());
        for (field, definition) in plan.fields.iter().enumerate() {
            if definition.components != 3 || layout.field_count(field) != Some(self.dof_count()) {
                return Err(format!("Triangle constraint map differs from field {:?}.", definition.id));
            }
            blocks.push(crate::constraints::DofBlock { representative, weights, count });
        }
        crate::constraints::join_field_maps(&blocks)
    }

    pub(crate) fn initial_projection(
        &self, plan: &TransientPlan, layout: &StateLayout, start: f64,
        mut sample: impl FnMut(&str, [f64; 3], f64) -> Result<Vec<C>, String>,
    ) -> Result<(crate::transient_step::SparseMatrix, Vec<C>), String> {
        if plan.dimension != 2 || plan.operator_dimension != 3 || !start.is_finite() {
            return Err("Transient triangle projection requires a 2D mesh, 3D operator and finite initial time.".into());
        }
        for (field, definition) in plan.fields.iter().enumerate() {
            if definition.components != 3 || layout.field_count(field) != Some(self.dof_count()) {
                return Err(format!("Initial triangle layout differs from field {:?}.", definition.id));
            }
        }
        let mut mass = vec![BTreeMap::new(); layout.dimension];
        let mut rhs = vec![C::default(); layout.dimension];
        for (element_index, element) in self.elements.iter().enumerate() {
            let local = &self.topology.elements[element_index];
            for q in crate::quadrature::triangle("standard")? {
                let point = std::array::from_fn(|axis| (0..3).map(|i| q[i] * self.nodes[element.nodes[i]][axis]).sum());
                let (values, _) = self.basis(element, [q[0], q[1], q[2]])?;
                let weight = element.area * q[3];
                for (field, definition) in plan.fields.iter().enumerate() {
                    let sampled = sample(&definition.id, point, start)?;
                    if sampled.len() != 3 || sampled.iter().any(|value| !value.re.is_finite() || !value.im.is_finite()) {
                        return Err(format!("Initial field {:?} requires three finite components.", definition.id));
                    }
                    let dofs = layout.global_dofs(field, local)?;
                    for (i, &row) in dofs.iter().enumerate() {
                        for (j, &column) in dofs.iter().enumerate() {
                            let value = weight * (0..3).map(|axis| values[i][axis] * values[j][axis]).sum::<f64>();
                            mass[row].entry(column).and_modify(|entry: &mut C| entry.re += value).or_insert(C { re: value, im: 0. });
                        }
                        for (component, value) in sampled.iter().enumerate() {
                            rhs[row].re += weight * values[i][component] * value.re;
                            rhs[row].im += weight * values[i][component] * value.im;
                        }
                    }
                }
            }
        }
        Ok((mass, rhs))
    }

    pub(crate) fn assemble(
        &self, plan: &TransientPlan, layout: &StateLayout, time: f64, quadrature: &str,
        workspace: &mut TransientAssemblyWorkspace,
        mut coefficient: impl FnMut(SampleLocation) -> Result<HashMap<String, SampledCoefficient>, String>,
        sources: &[TransientSource<'_>],
        mut source_value: impl FnMut(&str, SampleLocation) -> Result<Vec<C>, String>,
    ) -> Result<TransientOperators, String> {
        if plan.dimension != 2 || plan.operator_dimension != 3 || !time.is_finite() {
            return Err("Transient triangle adapter requires a 2D mesh with a 3D differential operator.".into());
        }
        for (field, definition) in plan.fields.iter().enumerate() {
            if definition.components != 3 || layout.field_count(field) != Some(self.dof_count()) {
                return Err(format!("Transient triangle layout differs from field {:?}.", definition.id));
            }
        }
        let locals = self.topology.elements.iter().map(|row| row.to_vec()).collect::<Vec<_>>();
        let elements = locals.iter().map(|local| {
            let mut all = Vec::new();
            for field in 0..plan.fields.len() { all.extend(layout.global_dofs(field, local)?); }
            Ok(all)
        }).collect::<Result<Vec<_>, String>>()?;
        workspace.prepare(plan, layout, &elements)?;
        let mut loads = if sources.is_empty() { None } else { Some(TransientOperators {
            derivatives: vec![], operator: vec![], load: vec![C::default(); layout.dimension], essential: vec![], essential_rates: vec![],
        }) };
        let quadrature = crate::quadrature::triangle(quadrature)?;
        for (index, element) in self.elements.iter().enumerate() {
            let local = &locals[index];
            for q in &quadrature {
                let barycentric = [q[0], q[1], q[2]];
                let point = std::array::from_fn(|axis| (0..3).map(|i| barycentric[i] * self.nodes[element.nodes[i]][axis]).sum());
                let (values, gradients) = self.basis(element, barycentric)?;
                let samples = values.iter().zip(&gradients).map(|(value, gradient)| BasisSample { value, gradient, curl: &[] }).collect::<Vec<_>>();
                let bases = (0..plan.fields.len()).map(|_| samples.iter().map(|sample| BasisSample {
                    value: sample.value, gradient: sample.gradient, curl: sample.curl,
                }).collect::<Vec<_>>()).collect::<Vec<_>>();
                let local_fields = (0..plan.fields.len()).map(|_| local.clone()).collect::<Vec<_>>();
                let weight = element.area * q[3];
                let location = SampleLocation { cell: index, material_tag: element.tag, point, time };
                let coefficients = coefficient(location)?;
                plan.assemble_quadrature(layout, workspace, &bases, &local_fields, weight, &coefficients)?;
                if let Some(accumulator) = loads.as_mut() {
                    plan.assemble_sources(layout, &bases, &local_fields, location, weight, sources, &mut source_value, accumulator)?;
                }
            }
        }
        let mut result = workspace.rows()?;
        if let Some(accumulator) = loads { result.load = accumulator.load; }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn two_vector_field_blocks_assemble_coupled_triangle_curl_at_finite_kz() {
        let mut plan = TransientPlan::from_json(&json!({
            "format":"fullmeta-transient-form/1", "dimension":2, "operatorDimension":3,
            "fields":[{"id":"first","components":3},{"id":"second","components":3}],
            "terms":[
                {"testField":"first","trialField":"second","test":"curl","trial":"value","coefficient":"one","timeDerivative":"none"},
                {"testField":"second","trialField":"first","test":"value","trial":"curl","coefficient":"one","timeDerivative":"none"}
            ]
        })).unwrap();
        plan.set_longitudinal_wavenumber(C { re: 2., im: 0. }).unwrap();
        let mesh = TriangleTransientMesh::prepare(
            &[[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]], &[([0,1,2], 7)],
        ).unwrap();
        let layout = StateLayout::new(&plan, &[mesh.dof_count(), mesh.dof_count()]).unwrap();
        let one = SampledCoefficient::Scalar(C { re: 1., im: 0. });
        let operators = mesh.assemble(&plan, &layout, 0., "standard", &mut TransientAssemblyWorkspace::default(),
            |_| Ok(HashMap::from([("one".into(), one.clone())])), &[], |_,_| unreachable!()).unwrap();
        let first_offset = mesh.dof_count();
        let mut cross_entries = 0;
        let mut imaginary_entries = 0;
        for (row, entries) in operators.operator.iter().enumerate() {
            for (&column, value) in entries {
                if (row < first_offset) != (column < first_offset) {
                    cross_entries += 1;
                    if value.im.abs() > 1e-12 { imaginary_entries += 1; }
                }
            }
        }
        assert!(cross_entries > 0, "declared field blocks must couple in sparse assembly");
        assert!(imaginary_entries > 0, "finite longitudinal derivative must enter the complex triangle operator");
    }
}
