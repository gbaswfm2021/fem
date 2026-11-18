//! Geometry and quadrature preparation for oriented tetrahedral fields.
//!
//! This adapter is independent of the PDE, material names and scenario ID.
//! The caller chooses mesh and selection policies and supplies coefficient
//! samples; boundary input, periodic relations and output remain separate.
use std::{collections::HashMap, rc::Rc};
use serde_json::Value;
use crate::{coordinate_frame::CoordinateFrame, mesh3d, nedelec_dofs::DofTopology,
    nedelec_driver::{self, VolumePoint}, simplex_topology::{self, SelectedFacet,
        SelectedFacetPolicy, SimplexTopology, TopologyPolicy}, weak_form::Coefficient};

pub(crate) struct EdgeSelection {
    pub(crate) nodes: [usize; 2],
    pub(crate) tag: usize,
    /// First incident cell in input order, suitable for a one-sided sample.
    pub(crate) owner: usize,
}

/// Cell-centroid samples retained for recovery as well as assembly.
pub(crate) struct CellMaterialSample {
    pub(crate) model_point: [f64; 3],
    pub(crate) symbols: HashMap<String, crate::C>,
    pub(crate) coefficients: Rc<HashMap<String, Coefficient<'static>>>,
}

/// An explicitly selected, already-matched pair of exterior surface tags.
pub(crate) struct MatchedSurface<'a> {
    pub(crate) lower_tag: usize,
    pub(crate) upper_tag: usize,
    pub(crate) node_map: &'a HashMap<usize, usize>,
    pub(crate) factor: crate::C,
}

pub(crate) struct CompiledBoundary {
    pub(crate) tag: usize,
    pub(crate) form: String,
    pub(crate) essential: bool,
    pub(crate) input: crate::boundary_input::BoundaryInput,
}

/// Compile configured rows using collection-to-contract mappings supplied by
/// the scenario. Every row must identify a declared, compatible form.
pub(crate) fn compile_boundaries(bound: &HashMap<String, Vec<crate::model_bindings::BoundSelection<'_>>>,
    contracts: &Value, mapping: &Value, plan: &crate::scenario_plan::ScenarioPlan)
    -> Result<Vec<CompiledBoundary>,String> {
    let mapping = mapping.as_object().ok_or("Boundary collection mappings must be an object.")?;
    let contracts = contracts.as_object().ok_or("Boundary input contracts must be an object.")?;
    let mut output = Vec::new();
    let mut used = std::collections::HashSet::new();
    for (collection, declaration) in mapping {
        let rows = bound.get(collection).ok_or_else(||format!("Boundary collection {collection:?} was not bound."))?;
        if !declaration.is_object() || declaration.get("kind").is_some_and(|v|!v.is_string())
            || declaration.get("kindPath").is_some_and(|v|!v.is_string()) {
            return Err(format!("Boundary collection {collection:?} has an invalid kind declaration."));
        }
        let fixed = declaration.get("kind").and_then(Value::as_str);
        let path = declaration.get("kindPath").and_then(Value::as_str);
        if fixed.is_some() == path.is_some() { return Err(format!("Boundary collection {collection:?} needs exactly one kind or kindPath.")); }
        if path.is_some_and(|p|!p.starts_with('/')) { return Err("Boundary kindPath must be a JSON pointer.".into()); }
        for row in rows {
            if !used.insert(row.tag) { return Err(format!("Boundary tag {} is bound more than once.",row.tag)); }
            let kind = fixed.or_else(||path.and_then(|p|row.row.pointer(p)).and_then(Value::as_str))
                .filter(|s|!s.is_empty()).ok_or_else(||format!("Boundary collection {collection:?} has no declared kind."))?;
            let contract = contracts.get(kind).ok_or_else(||format!("Boundary input kind {kind:?} is not declared."))?;
            let input = crate::boundary_input::BoundaryInput::compile(contract,row.row)?;
            let essential = match contract.get("mode").and_then(Value::as_str) {
                Some("moment") => true,
                Some("trace" | "impedance") => false,
                _ => return Err(format!("Boundary input kind {kind:?} is unsupported for oriented fields.")),
            };
            let form = if essential {
                if !plan.essential.contains_key(kind) { return Err(format!("Essential form {kind:?} is not declared.")); }
                kind
            } else {
                let name = input.form_key().ok_or("Trace boundary has no declared form.")?;
                if !plan.boundaries.contains_key(name) { return Err(format!("Boundary form {name:?} is not declared.")); }
                name
            }.to_owned();
            output.push(CompiledBoundary {tag:row.tag,form,essential,input});
        }
    }
    Ok(output)
}

/// Extract catalog material rows with a declared optional nesting pointer.
pub(crate) fn material_rows(rows: &[crate::model_bindings::BoundSelection<'_>],
    path: &str, fallback_to_row: bool) -> Result<HashMap<usize,Value>,String> {
    if !path.starts_with('/') { return Err("Material input path must be a JSON pointer.".into()); }
    let mut output = HashMap::new();
    for row in rows {
        let value = row.row.pointer(path).or_else(||fallback_to_row.then_some(row.row))
            .filter(|value|value.is_object()).ok_or_else(||format!("Material selection {:?} has no object at {path:?}.",row.selection_id))?;
        if output.insert(row.tag,value.clone()).is_some() { return Err(format!("Material tag {} is duplicated.",row.tag)); }
    }
    Ok(output)
}

pub(crate) struct PreparedEdgeMesh {
    pub(crate) model_nodes: Vec<[f64; 3]>,
    pub(crate) nodes: Vec<[f64; 3]>,
    pub(crate) cells: Vec<[usize; 4]>,
    pub(crate) cell_tags: Vec<usize>,
    pub(crate) frame: CoordinateFrame,
    pub(crate) adjacency: SimplexTopology<3>,
    pub(crate) selected_facets: Vec<SelectedFacet>,
    pub(crate) selected_edges: Vec<EdgeSelection>,
    pub(crate) dofs: DofTopology,
    pub(crate) driver_topology: nedelec_driver::Topology,
    order: u8,
    quadrature: Vec<[f64; 5]>,
    volumes: Vec<f64>,
}

impl PreparedEdgeMesh {
    /// Read an indexed node/cell stream with ordered selected entities. The
    /// frame and selection validation are explicit caller-provided policies.
    pub(crate) fn from_stream(text: &str, frame: &Value, order: u8, quadrature: &str,
        topology_policy: TopologyPolicy, facet_policy: SelectedFacetPolicy,
        positive_edge_tags: bool) -> Result<Self, String> {
        let mesh = mesh3d::parse_indexed_entity_stream(text)?;
        let frame = CoordinateFrame::from_json(frame, &mesh.nodes)?;
        if frame.orientation() < 0. {
            return Err("Orientation-reversing coordinate frames are not yet supported for oriented fields.".into());
        }
        let nodes: Vec<_> = mesh.nodes.iter().copied().map(|point| frame.point_to_internal(point)).collect();
        let (cells, cell_tags): (Vec<_>, Vec<_>) = mesh.tetrahedra.into_iter().unzip();
        let adjacency = simplex_topology::tetrahedra(&nodes, &cells, &[], topology_policy)?;
        let selected_facets = adjacency.bind_selected_facets(&mesh.boundary_faces, &[], facet_policy)?;
        let dofs = DofTopology::tetrahedra(nodes.len(), &cells, order)?;
        if nodes.is_empty() || cells.is_empty() { return Err("Oriented tetrahedral mesh must not be empty.".into()); }
        let driver_topology = nedelec_driver::Topology { elements: dofs.element_dofs.clone(), count: dofs.count };
        let mut edge_owners = HashMap::new();
        for (owner, cell) in cells.iter().enumerate() {
            for local in crate::nedelec::EDGES {
                let mut edge = [cell[local[0]], cell[local[1]]];
                edge.sort_unstable();
                edge_owners.entry(edge).or_insert(owner);
            }
        }
        let mut selected_edges = Vec::with_capacity(mesh.edge_selections.len());
        for (mut edge, tag) in mesh.edge_selections {
            if positive_edge_tags && tag == 0 { return Err("Selected edge requires a positive physical tag.".into()); }
            edge.sort_unstable();
            if edge[0] == edge[1] || dofs.edge(edge).is_none() {
                return Err(format!("Selected edge {edge:?} has no adjacent tetrahedron."));
            }
            let owner = *edge_owners.get(&edge)
                .ok_or_else(|| format!("Selected edge {edge:?} has no adjacent tetrahedron."))?;
            selected_edges.push(EdgeSelection { nodes: edge, tag, owner });
        }
        let volumes = cells.iter().map(|&cell| {
            let geometry = crate::simplex::geometry(cell.map(|node| nodes[node]))?;
            let volume = geometry.det.abs() / 6.;
            if !volume.is_finite() || volume == 0. { return Err("Tetrahedron has an invalid physical volume.".into()); }
            Ok(volume)
        }).collect::<Result<Vec<_>, String>>()?;
        let quadrature = crate::quadrature::tetrahedron(quadrature)?;
        Ok(Self { model_nodes: mesh.nodes, nodes, cells, cell_tags, frame, adjacency,
            selected_facets, selected_edges, dofs, driver_topology, order, quadrature, volumes })
    }

    /// Evaluate coefficients at physical quadrature points. The callback gets
    /// both internal and model coordinates, so dimensioned material formulas
    /// can use the original model frame without any assumed symbol names.
    pub(crate) fn volume_points<'a>(&self, mut sample: impl FnMut(usize, [f64; 3], [f64; 3])
        -> Result<Rc<HashMap<String, Coefficient<'a>>>, String>) -> Result<Vec<Vec<VolumePoint<'a>>>, String> {
        let mut cells = Vec::with_capacity(self.cells.len());
        for (element, cell) in self.cells.iter().enumerate() {
            let mut points = Vec::with_capacity(self.quadrature.len());
            for q in &self.quadrature {
                let point = std::array::from_fn(|axis| (0..4).map(|i| q[i] * self.nodes[cell[i]][axis]).sum());
                let model = self.frame.point_to_model(point);
                let weight = q[4] * self.volumes[element];
                if point.iter().chain(model.iter()).any(|x| !x.is_finite()) || !weight.is_finite() {
                    return Err("Tetrahedron quadrature produced a non-finite point or weight.".into());
                }
                points.push(VolumePoint { point, weight, coefficients: sample(element, point, model)? });
            }
            cells.push(points);
        }
        Ok(cells)
    }

    /// Resolve already-bound material assignments at each cell centroid using
    /// the selected scenario's dimensioned catalog and declared expressions.
    /// This explicit centroid method does not silently assume that every
    /// scenario permits that sampling policy.
    pub(crate) fn centroid_materials(&self, profile: &str, assignments: &HashMap<usize, Value>,
        parameters: &HashMap<String, crate::model_expression::Scalar>, frequency: f64,
        plan: &crate::scenario_plan::ScenarioPlan) -> Result<Vec<CellMaterialSample>, String> {
        if !frequency.is_finite() || frequency <= 0. { return Err("Frequency must be positive and finite.".into()); }
        if !plan.coefficient_matrices.is_empty() { return Err("Matrix-valued edge coefficients are not supported by this adapter.".into()); }
        let bindings = plan.material_bindings.as_ref().ok_or("Material bindings are required for edge preparation.")?;
        let materials = crate::material::CompiledMaterialAssignments::compile(profile, assignments)?;
        let constant = materials.position_independent_values(parameters, frequency)?;
        let mut output = Vec::with_capacity(self.cells.len());
        for (index, cell) in self.cells.iter().enumerate() {
            let point = std::array::from_fn(|axis| cell.iter().map(|&node| self.model_nodes[node][axis] * 0.25).sum());
            let tag = self.cell_tags[index];
            let raw = if let Some(values) = constant.get(&tag) { values.clone() }
                else { materials.evaluate(tag, parameters, frequency, point, &format!("cell {index}"))? };
            let context = crate::scalar2d::symbols_at_point(parameters, frequency, point);
            let bound = bindings.evaluate(&raw, &context, &format!("cell {index} material"))?;
            let mut symbols: HashMap<String, crate::C> = context.iter().map(|(name,value)|
                (name.clone(), crate::C { re:value.value, im:0. })).collect();
            symbols.extend(bound.into_iter().map(|(name,value)| (name,crate::C {re:value.value,im:0.})));
            if let Some(derived) = &plan.material_coefficients {
                for (name,value) in derived.evaluate(&symbols)? {
                    if symbols.insert(name.clone(),value).is_some() { return Err(format!("Derived material symbol {name:?} shadows an existing input.")); }
                }
            }
            let coefficients = plan.volume.evaluate_coefficients(&symbols)?.into_iter()
                .map(|(name,value)| (name,Coefficient::Scalar(value))).collect();
            output.push(CellMaterialSample { model_point:point, symbols, coefficients:Rc::new(coefficients) });
        }
        Ok(output)
    }

    fn exterior_faces(&self, tag: usize) -> Result<Vec<([usize; 3], usize, usize)>, String> {
        let mut output = Vec::new();
        for selected in &self.selected_facets {
            if selected.tag != tag { continue; }
            let facet = &self.adjacency.facets[selected.facet_index];
            if facet.owners.len() != 1 { return Err(format!("Selected tag {tag} includes a non-exterior face.")); }
            output.push((facet.vertices, facet.owners[0].cell, selected.facet_index));
        }
        if output.is_empty() { return Err(format!("Selected tag {tag} has no exterior faces.")); }
        Ok(output)
    }

    /// Match translated exterior vertices in O(nodes + nearby candidates).
    /// Translation is given in model coordinates; both sides must match
    /// bijectively within the declared absolute physical tolerance.
    pub(crate) fn translated_nodes(&self, lower_tag: usize, upper_tag: usize,
        translation: [f64; 3], tolerance: f64) -> Result<HashMap<usize,usize>,String> {
        if lower_tag == upper_tag { return Err("Periodic surface tags must differ.".into()); }
        if !tolerance.is_finite() || tolerance <= 0. || translation.iter().any(|x| !x.is_finite()) {
            return Err("Periodic translation must be finite and matching tolerance positive and finite.".into());
        }
        let lower = self.exterior_faces(lower_tag)?.into_iter().flat_map(|(face,_,_)|face).collect::<std::collections::BTreeSet<_>>();
        let upper = self.exterior_faces(upper_tag)?.into_iter().flat_map(|(face,_,_)|face).collect::<std::collections::BTreeSet<_>>();
        if lower.len() != upper.len() { return Err("Periodic surfaces have different node counts.".into()); }
        let translation = self.frame.vector_to_internal(translation);
        let bin = |point:[f64;3]| -> Result<[i64;3],String> {
            let mut key = [0;3];
            for i in 0..3 {
                let scaled = (point[i]/tolerance).floor();
                if !scaled.is_finite() || scaled < i64::MIN as f64 || scaled >= i64::MAX as f64 {
                    return Err("Periodic coordinates exceed the supported matching range.".into());
                }
                key[i] = scaled as i64;
            }
            Ok(key)
        };
        let mut bins = HashMap::<[i64;3],Vec<usize>>::new();
        for &node in &upper { bins.entry(bin(self.nodes[node])?).or_default().push(node); }
        let mut output = HashMap::new();
        let mut matched = std::collections::HashSet::new();
        for node in lower {
            let target = std::array::from_fn(|axis|self.nodes[node][axis]+translation[axis]);
            let key = bin(target)?;
            let mut candidates = Vec::new();
            for dx in -1i64..=1 { for dy in -1i64..=1 { for dz in -1i64..=1 {
                let Some(x) = key[0].checked_add(dx) else { continue }; 
                let Some(y) = key[1].checked_add(dy) else { continue }; 
                let Some(z) = key[2].checked_add(dz) else { continue };
                if let Some(found) = bins.get(&[x,y,z]) {
                    candidates.extend(found.iter().copied().filter(|&other| {
                        (self.nodes[other][0]-target[0]).hypot(self.nodes[other][1]-target[1])
                            .hypot(self.nodes[other][2]-target[2]) <= tolerance
                    }));
                }
            }}}
            if candidates.len()!=1 { return Err(format!("Periodic node {node} has {} translated matches; expected one.",candidates.len())); }
            let other = candidates[0];
            if !matched.insert(other) { return Err(format!("Periodic upper node {other} has multiple matches.")); }
            output.insert(node,other);
        }
        if matched.len()!=upper.len() { return Err("Periodic matching does not cover every upper node.".into()); }
        Ok(output)
    }

    /// Resolve the configured model's selections through the declared plan.
    /// The caller supplies a JSON mapping from collection names to their mesh
    /// entity universe, avoiding assumptions about scenario collection names.
    pub(crate) fn bind_configured<'a>(&self, case: &'a crate::prepared_configured::ConfiguredCase,
        selections: &crate::selection_plan::SelectionPlan, entity_sources: &Value)
        -> Result<HashMap<String, Vec<crate::model_bindings::BoundSelection<'a>>>,String> {
        let declarations = entity_sources.as_object().ok_or("Selection entity sources must be an object.")?;
        let mut tags = HashMap::new();
        for (name, source) in declarations {
            let tags_for_source = match source.as_str() {
                Some("cells") => self.cell_tags.clone(),
                Some("facets") => self.selected_facets.iter().map(|row|row.tag).collect(),
                Some("exteriorFacets") => self.selected_facets.iter().filter_map(|row| {
                    (self.adjacency.facets[row.facet_index].owners.len()==1).then_some(row.tag)
                }).collect(),
                Some("edges") => self.selected_edges.iter().map(|row|row.tag).collect(),
                _ => return Err(format!("Unsupported mesh entity source for collection {name:?}.")),
            };
            tags.insert(name.clone(),tags_for_source);
        }
        selections.bind(&case.request,&case.mesh_map,&tags)
    }

    /// Convert a validated face correspondence and phase into weighted DOFs.
    /// Matching tolerance and the complex factor remain caller-declared.
    pub(crate) fn periodic(&self, order: u8, pairs: &[MatchedSurface<'_>], tolerance: f64)
        -> Result<crate::nedelec_dofs::ConstrainedDofs, String> {
        if order != self.order { return Err("Periodic moment order does not match the prepared mesh.".into()); }
        let mut relations = Vec::new();
        for pair in pairs {
            if pair.lower_tag == pair.upper_tag { return Err("Periodic surface tags must differ.".into()); }
            let lower = self.exterior_faces(pair.lower_tag)?.into_iter().map(|(face,_,_)| face).collect::<Vec<_>>();
            let upper = self.exterior_faces(pair.upper_tag)?.into_iter().map(|(face,_,_)| face).collect::<Vec<_>>();
            relations.extend(crate::nedelec_periodic::matched_surface_relations(&self.dofs, &self.nodes, order,
                &lower, &upper, pair.node_map, pair.factor, tolerance)?);
        }
        self.dofs.constrain(&relations)
    }

    /// Integrate model-space Cartesian essential values into oriented moments.
    pub(crate) fn essential_moments(&self, tag: usize, order: u8,
        input: &crate::boundary_input::BoundaryInput, plan: &crate::essential_form::EssentialFormPlan,
        parameters: &HashMap<String, crate::model_expression::Scalar>) -> Result<Vec<nedelec_driver::EssentialMoment>, String> {
        if order != self.order { return Err("Essential moment order does not match the prepared mesh.".into()); }
        plan.require_tangential_moments()?;
        let mut moments = Vec::new();
        for (face, element, _) in self.exterior_faces(tag)? {
            let tet = self.cells[element];
            let values = crate::nedelec::face_tangential_moments(tet, face, &self.nodes, order, |point| {
                let values = input.evaluate(parameters, Some(self.frame.point_to_model(point)), &HashMap::new(), None)?;
                let field = plan.moment_values(values.essential.unwrap_or_default(), values.essential_vector.as_deref(), "boundary")?;
                let model: [crate::C;3] = field.try_into().map_err(|_| "Oriented field requires three Cartesian components.")?;
                Ok(self.frame.vector_to_internal(model))
            })?;
            for (local, value) in values {
                let raw_dof = *self.driver_topology.elements[element].get(local).ok_or("Boundary moment references a missing local DOF.")?;
                moments.push(nedelec_driver::EssentialMoment { raw_dof, value });
            }
        }
        Ok(moments)
    }

    /// Prepare a declared trace form. `source_vectors` maps an output source
    /// name to three named scalar components, in model coordinate order;
    /// directly supplied three-component sources need no mapping.
    pub(crate) fn boundary_points<'a>(&self, tag: usize, input: &crate::boundary_input::BoundaryInput,
        form: &'a crate::boundary_form::BoundaryFormPlan,
        parameters: &HashMap<String, crate::model_expression::Scalar>, symbols: &HashMap<String, crate::C>,
        source_vectors: &HashMap<String, [String;3]>) -> Result<Vec<nedelec_driver::BoundaryPoint<'a>>, String> {
        let mut output = Vec::new();
        for (face, element, facet_index) in self.exterior_faces(tag)? {
            let (normal, area) = simplex_topology::face_normal(&self.nodes, &self.adjacency.facets[facet_index], 0)?;
            let vertices = face.map(|node| self.nodes[node]);
            for q in crate::TRI_Q {
                let point = std::array::from_fn(|axis| (0..3).map(|i| q[i]*vertices[i][axis]).sum());
                let mut values = input.evaluate(parameters, Some(self.frame.point_to_model(point)), symbols, Some(form))?;
                for (name, components) in source_vectors {
                    if values.sources.contains_key(name) { return Err(format!("Boundary source {name:?} is declared twice.")); }
                    let mut vector = Vec::with_capacity(3);
                    for component in components {
                        let source = values.sources.remove(component).ok_or_else(|| format!("Boundary source component {component:?} is missing."))?;
                        if source.len() != 1 { return Err(format!("Boundary source component {component:?} must be scalar.")); }
                        vector.push(source[0]);
                    }
                    values.sources.insert(name.clone(),vector);
                }
                for source in values.sources.values_mut() {
                    if source.len() == 3 {
                        let model = [source[0],source[1],source[2]];
                        *source = self.frame.vector_to_internal(model).to_vec();
                    }
                }
                let weight = area*q[3];
                if !weight.is_finite() { return Err("Boundary quadrature weight must be finite.".into()); }
                output.push(nedelec_driver::BoundaryPoint { element,point,normal,weight,plan:form,
                    coefficients:values.coefficients,sources:if values.sources.is_empty(){None}else{Some(values.sources)} });
            }
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::simplex_topology::{SelectedFacetDuplicates, SelectedFacetOwnership};

    fn prepare(text: &str, order: u8, rule: &str) -> Result<PreparedEdgeMesh,String> {
        PreparedEdgeMesh::from_stream(text, &json!({"axisOrder":[1,2,0],"origin":"boundsMinimum"}), order, rule,
            TopologyPolicy { reject_duplicate_cells:true, reject_non_manifold_facets:true, require_all_nodes_used:false,
                require_nonempty_cells:true, determinant_tolerance:None },
            SelectedFacetPolicy { ownership:SelectedFacetOwnership::AnyExisting,
                duplicates:SelectedFacetDuplicates::Reject, require_positive_tag:true }, true)
    }
    const MESH: &str = "V,0,10,20,30\nV,1,12,20,30\nV,2,10,23,30\nV,3,10,20,34\nV,4,10,20,25\nT,0,0,1,2,3,4\nT,1,0,2,1,4,5\nF,x,0,1,2,8\nF,x,0,1,3,9\nL,x,1,0,10";

    #[test]
    fn stream_preserves_internal_and_exterior_facets_edges_and_canonical_dofs() {
        for order in [1,2] {
            let mesh = prepare(MESH,order,"standard").unwrap();
            assert_eq!(mesh.nodes[0],[0.,5.,0.]);
            assert_eq!(mesh.model_nodes[3],[10.,20.,34.]);
            assert_eq!(mesh.cell_tags,vec![4,5]);
            assert_eq!(mesh.adjacency.facets[mesh.selected_facets[0].facet_index].owners.len(),2);
            assert_eq!(mesh.adjacency.facets[mesh.selected_facets[1].facet_index].owners.len(),1);
            assert_eq!((mesh.selected_edges[0].nodes,mesh.selected_edges[0].tag,mesh.selected_edges[0].owner),([0,1],10,0));
            assert_eq!(mesh.driver_topology.elements,mesh.dofs.element_dofs);
            assert_eq!(mesh.driver_topology.count,mesh.dofs.count);
        }
    }

    #[test]
    fn physical_volume_quadrature_preserves_model_coordinates_and_linear_integrals() {
        let mesh = prepare(MESH,1,"standard").unwrap();
        let points = mesh.volume_points(|_,_,_| Ok(Rc::new(HashMap::new()))).unwrap();
        for (element, expected_volume) in [4.,5.].into_iter().enumerate() {
            let weight:f64 = points[element].iter().map(|point| point.weight).sum();
            assert!((weight-expected_volume).abs()<1e-12);
            for axis in 0..3 {
                let integral:f64 = points[element].iter().map(|point| point.weight * mesh.frame.point_to_model(point.point)[axis]).sum();
                let centroid:f64 = mesh.cells[element].iter().map(|&i| mesh.model_nodes[i][axis]).sum::<f64>()/4.;
                assert!((integral-expected_volume*centroid).abs()<1e-10);
            }
        }
    }

    #[test]
    fn rejects_invalid_selected_entities_degenerate_cells_and_unknown_rule() {
        assert!(prepare(&format!("{MESH}\nF,x,2,1,0,11"),1,"standard").is_err());
        assert!(prepare(&format!("{MESH}\nL,x,3,4,10"),1,"standard").is_err());
        assert!(prepare(&MESH.replace("F,x,0,1,2,8","F,x,0,1,2,0"),1,"standard").is_err());
        assert!(prepare(&MESH.replace("V,3,10,20,34","V,3,10,20,30"),1,"standard").is_err());
        assert!(prepare(MESH,3,"standard").is_err());
        assert!(prepare(MESH,1,"unknown").is_err());
        assert!(PreparedEdgeMesh::from_stream(MESH,&json!({"axisOrder":[1,0,2],"origin":"zero"}),1,"standard",
            TopologyPolicy { reject_duplicate_cells:true, reject_non_manifold_facets:true, require_all_nodes_used:false,
                require_nonempty_cells:true, determinant_tolerance:None },
            SelectedFacetPolicy { ownership:SelectedFacetOwnership::AnyExisting,
                duplicates:SelectedFacetDuplicates::Reject, require_positive_tag:true }, true).is_err());
    }

    

    

    

    #[test]
    fn matched_exterior_surfaces_create_weighted_periodic_dofs() {
        let text = "V,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,2\nV,4,1,0,2\nV,5,0,1,2\nV,6,0,0,-1\nV,7,0,0,3\nT,0,0,1,2,6,1\nT,1,5,4,3,7,1\nF,x,0,1,2,4\nF,x,3,4,5,5";
        let map = HashMap::from([(0,3),(1,4),(2,5)]);
        for order in [1,2] {
            let mesh = prepare(text,order,"centroid").unwrap();
            assert_eq!(mesh.translated_nodes(4,5,[0.,0.,2.],1e-9).unwrap(),map);
            assert!(mesh.translated_nodes(4,5,[0.,0.,2.1],1e-9).is_err());
            assert!(mesh.translated_nodes(4,5,[0.,0.,2.],0.).is_err());
            assert!(mesh.periodic(if order==1 {2} else {1},&[],1e-8).is_err());
            let pair = MatchedSurface {lower_tag:4,upper_tag:5,node_map:&map,factor:crate::C {re:0.,im:-1.}};
            let periodic = mesh.periodic(order,&[pair],1e-8).unwrap();
            assert!(periodic.count < mesh.dofs.count);
            let lower = mesh.dofs.edge([0,1]).unwrap()[0];
            let upper = mesh.dofs.edge([3,4]).unwrap()[0];
            assert_eq!(periodic.representative[lower],periodic.representative[upper]);
            let expected = crate::c_mul(crate::C {re:0.,im:-1.},periodic.weights[lower]);
            assert!((periodic.weights[upper].re-expected.re).abs()<1e-12);
            assert!((periodic.weights[upper].im-expected.im).abs()<1e-12);
        }
    }

    
}
