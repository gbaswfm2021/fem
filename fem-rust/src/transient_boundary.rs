//! Scenario-declared, time-sampled transient boundary adapters.
//!
//! This module handles nodal point values and nodal facet forms. Unsupported laws fail
//! explicitly; selecting a harmonic boundary does not imply a transient law.
use crate::boundary_form::BoundaryFormPlan;
use crate::boundary_input::BoundaryInput;
use crate::essential_form::EssentialFormPlan;
use crate::model_bindings::BoundSelection;
use crate::model_expression::Scalar;
use crate::transient_mesh::NodalMesh;
use crate::transient_plan::{StateLayout, TransientPlan};
use crate::transient_step::TransientOperators;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

struct NodalValue {
    field: String,
    rate_field: Option<String>,
    nodes: Vec<usize>,
    input: BoundaryInput,
    form: EssentialFormPlan,
}

struct Facet {
    nodes: Vec<usize>,
    normal: Vec<f64>,
    measure: f64,
}
struct NodalTrace {
    field: usize,
    facets: Vec<Facet>,
    input: BoundaryInput,
    key: String,
    form: BoundaryFormPlan,
}

struct EdgeValue {
    field: String,
    faces: Vec<[usize; 3]>,
    input: BoundaryInput,
    form: EssentialFormPlan,
}
struct EdgeTrace {
    field: usize,
    points: Vec<crate::transient_edge::EdgeFacePoint>,
    input: BoundaryInput,
    form: BoundaryFormPlan,
    source_vectors: HashMap<String, [String; 3]>,
}
pub(crate) struct EdgeBoundaries {
    values: Vec<EdgeValue>,
    traces: Vec<EdgeTrace>,
}
impl EdgeBoundaries {
    pub(crate) fn prepare(
        plan: &TransientPlan,
        laws: &Value,
        essential_forms: &Value,
        boundary_forms: &Value,
        selected: &[BoundSelection<'_>],
        mesh: &crate::transient_edge::EdgeTransientMesh,
        facets: &[([usize; 3], usize)],
    ) -> Result<Self, String> {
        let laws = laws
            .as_object()
            .ok_or("Transient boundary laws must be an object.")?;
        let wanted = selected.iter().map(|row| row.tag).collect::<HashSet<_>>();
        let mut by_tag = HashMap::<usize, Vec<[usize; 3]>>::new();
        let mut seen = HashSet::new();
        for &(mut face, tag) in facets.iter().filter(|(_, tag)| wanted.contains(tag)) {
            face.sort_unstable();
            if !seen.insert(face) {
                return Err("Transient edge boundary face is duplicated.".into());
            }
            mesh.exterior_face_moments(face, |_| Ok([crate::C::default(); 3]))?;
            by_tag.entry(tag).or_default().push(face);
        }
        let mut values = Vec::new();
        let mut traces = Vec::new();
        for row in selected {
            let kind = row
                .row
                .get("type")
                .and_then(Value::as_str)
                .ok_or("Transient boundary type is required.")?;
            let law = laws.get(kind).ok_or_else(|| {
                format!("Transient boundary type {kind:?} has no declared time-domain law.")
            })?;
            let field = law
                .get("field")
                .and_then(Value::as_str)
                .ok_or("Transient boundary field is required.")?;
            let (field_index, state) = plan
                .fields
                .iter()
                .enumerate()
                .find(|(_, value)| value.id == field)
                .ok_or("Transient boundary names an undeclared field.")?;
            let input = law
                .get("input")
                .ok_or("Transient boundary input is required.")?;
            let faces = by_tag.get(&row.tag).cloned().ok_or_else(|| {
                format!(
                    "Transient boundary tag {} has no selected exterior faces.",
                    row.tag
                )
            })?;
            let input_plan = BoundaryInput::compile(input, row.row)?;
            if law.get("kind").and_then(Value::as_str) == Some("edgeTrace") {
                if !matches!(
                    input.get("mode").and_then(Value::as_str),
                    Some("trace" | "impedance")
                ) {
                    return Err("Transient edge trace requires trace or impedance mode.".into());
                }
                let key = input_plan
                    .form_key()
                    .ok_or("Transient edge input must select a form.")?;
                let form = BoundaryFormPlan::from_json(
                    boundary_forms
                        .get(key)
                        .ok_or("Transient edge boundary form is not declared.")?,
                )?;
                form.require_shape(plan.dimension, state.components)?;
                let mut points = Vec::new();
                for &face in &faces {
                    points.extend(mesh.exterior_face_points(face)?);
                }
                let mut source_vectors = HashMap::new();
                if let Some(rows) = law.get("sourceVectors") {
                    for (name, value) in rows
                        .as_object()
                        .ok_or("Transient sourceVectors must be an object.")?
                    {
                        let entries = value
                            .as_array()
                            .filter(|v| v.len() == 3)
                            .ok_or("Transient source vector needs three component names.")?;
                        let names = entries
                            .iter()
                            .map(|v| {
                                v.as_str()
                                    .filter(|s| !s.is_empty())
                                    .map(str::to_owned)
                                    .ok_or(
                                        "Transient source component must be a string.".to_string(),
                                    )
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        source_vectors.insert(
                            name.clone(),
                            names
                                .try_into()
                                .map_err(|_| "Transient source vector needs three names.")?,
                        );
                    }
                }
                traces.push(EdgeTrace {
                    field: field_index,
                    points,
                    input: input_plan,
                    form,
                    source_vectors,
                });
                continue;
            }
            if law.get("kind").and_then(Value::as_str) != Some("edgeMoment") {
                return Err(format!("Unsupported transient edge boundary law {kind:?}."));
            }
            let form_name = law
                .get("essentialForm")
                .and_then(Value::as_str)
                .ok_or("Transient essentialForm is required.")?;
            let form = EssentialFormPlan::from_json(
                essential_forms
                    .get(form_name)
                    .ok_or("Transient essential form is not declared.")?,
            )?;
            form.require_shape(plan.dimension, state.components)?;
            form.require_tangential_moments()?;
            if input.get("mode").and_then(Value::as_str) != Some("moment") {
                return Err("Transient edge input must use moment mode.".into());
            }
            values.push(EdgeValue {
                field: field.into(),
                faces,
                input: input_plan,
                form,
            });
        }
        Ok(Self { values, traces })
    }
    fn prescribed(
        boundary: &EdgeValue,
        mesh: &crate::transient_edge::EdgeTransientMesh,
        parameters: &HashMap<String, Scalar>,
        time: f64,
    ) -> Result<Vec<(usize, crate::C)>, String> {
        let mut result = Vec::new();
        for &face in &boundary.faces {
            result.extend(mesh.exterior_face_moments(face, |point| {
                let values = boundary.input.evaluate_at_time(
                    parameters,
                    Some(point),
                    time,
                    &HashMap::new(),
                    None,
                )?;
                let field = boundary.form.moment_values(
                    values.essential.unwrap_or_default(),
                    values.essential_vector.as_deref(),
                    "transient edge boundary",
                )?;
                field
                    .try_into()
                    .map_err(|_| "Transient edge boundary requires three components.".to_string())
            })?);
        }
        Ok(result)
    }
    pub(crate) fn apply(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        mesh: &crate::transient_edge::EdgeTransientMesh,
        parameters: &HashMap<String, Scalar>,
        time: f64,
        operators: &mut TransientOperators,
    ) -> Result<(), String> {
        for boundary in &self.values {
            let prescribed = Self::prescribed(boundary, mesh, parameters, time)?;
            crate::transient_runner::add_essential_values(
                plan,
                layout,
                &boundary.field,
                &prescribed,
                operators,
            )?;
        }
        for boundary in &self.traces {
            for point in &boundary.points {
                let mut values = boundary.input.evaluate_at_time(
                    parameters,
                    Some(point.point),
                    time,
                    &HashMap::new(),
                    Some(&boundary.form),
                )?;
                for (name, components) in &boundary.source_vectors {
                    if values.sources.contains_key(name) {
                        return Err(format!("Transient boundary source {name:?} is duplicated."));
                    }
                    let mut vector = Vec::new();
                    for component in components {
                        let value = values
                            .sources
                            .remove(component)
                            .ok_or("Transient boundary vector component is missing.")?;
                        if value.len() != 1 {
                            return Err("Transient boundary vector component is not scalar.".into());
                        }
                        vector.push(value[0]);
                    }
                    values.sources.insert(name.clone(), vector);
                }
                let basis = point.basis.iter().map(|v| v.as_slice()).collect::<Vec<_>>();
                plan.assemble_boundary_sample(
                    layout,
                    boundary.field,
                    &point.dofs,
                    &basis,
                    &point.normal,
                    &boundary.form,
                    &values.coefficients,
                    &values.sources,
                    point.weight,
                    None,
                    operators,
                )?;
            }
        }
        Ok(())
    }
    /// Compare the user's continuous initial field and prescribed boundary
    /// trace as oriented moments before the constrained L2 projection.
    pub(crate) fn validate_initial(
        &self,
        plan: &TransientPlan,
        mesh: &crate::transient_edge::EdgeTransientMesh,
        initial: &HashMap<String, crate::transient_initial::InitialField>,
        parameters: &HashMap<String, Scalar>,
        time: f64,
    ) -> Result<(), String> {
        for boundary in &self.values {
            let prescribed = Self::prescribed(boundary, mesh, parameters, time)?;
            let mut initial_moments = Vec::new();
            for &face in &boundary.faces {
                initial_moments.extend(mesh.exterior_face_moments(face, |point| {
                    crate::transient_initial::sample_field(
                        plan,
                        initial,
                        &boundary.field,
                        parameters,
                        point,
                        time,
                    )?
                    .try_into()
                    .map_err(|_| "Initial oriented field requires three components.".to_string())
                })?);
            }
            if prescribed.len() != initial_moments.len() {
                return Err("Initial edge trace shape is inconsistent.".into());
            }
            for ((a, value), (b, initial)) in prescribed.into_iter().zip(initial_moments) {
                if a != b
                    || (value.re - initial.re).hypot(value.im - initial.im)
                        > 1e-9 * (1. + value.re.hypot(value.im).max(initial.re.hypot(initial.im)))
                {
                    return Err(format!(
                        "Initial field conflicts with prescribed oriented boundary moment {a}."
                    ));
                }
            }
        }
        Ok(())
    }
}

struct TriangleEssentialValue {
    field: String,
    edges: Vec<[usize; 2]>,
    input: BoundaryInput,
    form: EssentialFormPlan,
}

/// Essential electric trace data for a 2D vector field in the compatible
/// Whitney-edge plus longitudinal-nodal space. The edge moment constrains the
/// in-plane tangential component; endpoint values constrain the longitudinal
/// component.
pub(crate) struct TriangleBoundaries {
    values: Vec<TriangleEssentialValue>,
}

impl TriangleBoundaries {
    pub(crate) fn prepare(
        plan:&TransientPlan,
        laws:&Value,
        essential_forms:&Value,
        selected:&[BoundSelection<'_>],
        mesh:&crate::transient_triangle::TriangleTransientMesh,
        edges:&[([usize;2],usize)],
    )->Result<Self,String>{
        let laws=laws.as_object().ok_or("Transient boundary laws must be an object.")?;
        let wanted=selected.iter().map(|row|row.tag).collect::<HashSet<_>>();
        let mut by_tag=HashMap::<usize,Vec<[usize;2]>>::new();let mut seen=HashSet::new();
        for &(mut edge,tag) in edges.iter().filter(|(_,tag)|wanted.contains(tag)){
            edge.sort_unstable();
            if !seen.insert(edge){return Err("Transient triangle boundary edge is duplicated.".into());}
            mesh.exterior_edge_moment(edge,|_|Ok([crate::C::default();3]))?;
            by_tag.entry(tag).or_default().push(edge);
        }
        let mut values=Vec::new();
        for row in selected{
            let kind=row.row.get("type").and_then(Value::as_str).ok_or("Transient boundary type is required.")?;
            let law=laws.get(kind).ok_or_else(||format!("Transient boundary type {kind:?} has no declared time-domain law."))?;
            if law.get("kind").and_then(Value::as_str)!=Some("triangleEssential"){
                return Err(format!("Transient triangle boundary law {kind:?} is not supported by the declared essential-trace adapter."));
            }
            let field=law.get("field").and_then(Value::as_str).ok_or("Transient triangle boundary field is required.")?;
            let definition=plan.fields.iter().find(|item|item.id==field).ok_or("Transient triangle boundary names an undeclared field.")?;
            if definition.components!=3{return Err("Transient triangle oriented essential trace requires a three-component field.".into());}
            let input=law.get("input").ok_or("Transient triangle boundary input is required.")?;
            if input.get("mode").and_then(Value::as_str)!=Some("moment"){
                return Err("Transient triangle essential boundary input must use moment mode.".into());
            }
            let name=law.get("essentialForm").and_then(Value::as_str).ok_or("Transient triangle essentialForm is required.")?;
            let form=EssentialFormPlan::from_json(essential_forms.get(name).ok_or("Transient triangle essential form is not declared.")?)?;
            form.require_shape(plan.dimension,definition.components)?;form.require_tangential_moments()?;
            let edges=by_tag.get(&row.tag).cloned().ok_or_else(||format!("Transient boundary tag {} has no selected exterior edges.",row.tag))?;
            values.push(TriangleEssentialValue{field:field.into(),edges,input:BoundaryInput::compile(input,row.row)?,form});
        }
        Ok(Self{values})
    }

    fn values_at(
        boundary:&TriangleEssentialValue,
        mesh:&crate::transient_triangle::TriangleTransientMesh,
        parameters:&HashMap<String,Scalar>,
        time:f64,
    )->Result<Vec<(usize,crate::C)>,String>{
        let mut result=Vec::new();let mut seen_nodes=HashSet::new();
        for &edge in &boundary.edges{
            let (edge_dof,moment)=mesh.exterior_edge_moment(edge,|point|{
                let input=boundary.input.evaluate_at_time(parameters,Some(point),time,&HashMap::new(),None)?;
                let values=boundary.form.moment_values(input.essential.unwrap_or_default(),input.essential_vector.as_deref(),"transient triangle essential trace")?;
                values.try_into().map_err(|_|"Transient triangle essential trace requires three components.".to_string())
            })?;
            result.push((edge_dof,moment));
            for node in edge{
                if !seen_nodes.insert(node){continue;}
                let point=mesh.node(node).ok_or("Transient triangle boundary vertex is out of range.")?;
                let input=boundary.input.evaluate_at_time(parameters,Some(point),time,&HashMap::new(),None)?;
                let values=boundary.form.moment_values(input.essential.unwrap_or_default(),input.essential_vector.as_deref(),"transient triangle essential trace")?;
                let dof=mesh.longitudinal_node_dof(node).ok_or("Transient triangle longitudinal DOF is missing.")?;
                result.push((dof,values[2]));
            }
        }
        Ok(result)
    }

    pub(crate) fn apply(&self,plan:&TransientPlan,layout:&StateLayout,mesh:&crate::transient_triangle::TriangleTransientMesh,
        parameters:&HashMap<String,Scalar>,time:f64,operators:&mut TransientOperators)->Result<(),String>{
        for boundary in &self.values{
            crate::transient_runner::add_essential_values(plan,layout,&boundary.field,
                &Self::values_at(boundary,mesh,parameters,time)?,operators)?;
        }
        Ok(())
    }

    pub(crate) fn validate_initial(&self,plan:&TransientPlan,mesh:&crate::transient_triangle::TriangleTransientMesh,
        initial:&HashMap<String,crate::transient_initial::InitialField>,parameters:&HashMap<String,Scalar>,time:f64)->Result<(),String>{
        for boundary in &self.values{
            let prescribed=Self::values_at(boundary,mesh,parameters,time)?;
            let mut actual=Vec::new();let mut seen_nodes=HashSet::new();
            for &edge in &boundary.edges{
                let (dof,moment)=mesh.exterior_edge_moment(edge,|point|{
                    crate::transient_initial::sample_field(plan,initial,&boundary.field,parameters,point,time)?
                        .try_into().map_err(|_|"Initial triangle vector field requires three components.".to_string())
                })?;
                actual.push((dof,moment));
                for node in edge{
                    if !seen_nodes.insert(node){continue;}
                    let point=mesh.node(node).ok_or("Transient triangle boundary vertex is out of range.")?;
                    let value=crate::transient_initial::sample_field(plan,initial,&boundary.field,parameters,point,time)?;
                    let dof=mesh.longitudinal_node_dof(node).ok_or("Transient triangle longitudinal DOF is missing.")?;
                    actual.push((dof,value[2]));
                }
            }
            if actual.len()!=prescribed.len(){return Err("Initial triangle trace shape is inconsistent.".into());}
            for ((dof,expected),(actual_dof,value)) in prescribed.into_iter().zip(actual){
                let scale=1.+expected.re.hypot(expected.im).max(value.re.hypot(value.im));
                if dof!=actual_dof||(expected.re-value.re).hypot(expected.im-value.im)>1e-9*scale{
                    return Err(format!("Initial field conflicts with prescribed triangle trace DOF {dof}."));
                }
            }
        }
        Ok(())
    }
}

pub(crate) struct NodalBoundaries {
    values: Vec<NodalValue>,
    traces: Vec<NodalTrace>,
}
impl NodalBoundaries {
    /// Compare initial rate fields to exact differentiable prescribed traces.
    /// An unavailable derivative is rejected rather than approximated.
    pub(crate) fn validate_initial_rates(
        &self,plan:&TransientPlan,mesh:&NodalMesh<'_>,initial:&HashMap<String,crate::transient_initial::InitialField>,
        parameters:&HashMap<String,Scalar>,time:f64,
    )->Result<(),String> {
        for boundary in &self.values {
            let Some(target)=boundary.rate_field.as_deref() else {continue;};
            for &node in &boundary.nodes {
                let point=*mesh.nodes.get(node).ok_or("Transient boundary node disappeared.")?;
                let values=crate::transient_initial::sample_field(plan,initial,target,parameters,point,time)?;
                let Some((scalar,vector))=boundary.input.prescribed_time_derivative(parameters,point,time)? else {
                    return Err(format!("Cannot determine the initial prescribed boundary rate at node {node}; use an expression differentiable by the transient runtime at the initial time."));
                };
                let rates=boundary.form.point_values(scalar,vector.as_deref(),"initial boundary rate")?;
                if rates.len()!=values.len(){return Err("Initial boundary rate component count differs from its target field.".into());}
                for (value,rate) in values.iter().zip(rates){
                    let difference=(value.re-rate.re).hypot(value.im-rate.im);
                    let scale=1f64.max(value.re.hypot(value.im)).max(rate.re.hypot(rate.im));
                    if difference>1e-10*scale {
                        return Err(format!("Initial field {target:?} conflicts with the prescribed boundary rate at node {node}."));
                    }
                }
            }
        }
        Ok(())
    }
    /// Bind each selected physical facet to an explicitly declared transient
    /// law and validate that it is a unique exterior facet of the solved mesh.
    pub(crate) fn prepare(
        plan: &TransientPlan,
        laws: &Value,
        essential_forms: &Value,
        boundary_forms: &Value,
        selected: &[BoundSelection<'_>],
        mesh: &NodalMesh<'_>,
        facets: &[(Vec<usize>, usize)],
    ) -> Result<Self, String> {
        let laws = laws
            .as_object()
            .ok_or("Transient boundary laws must be an object.")?;
        if mesh.dimension != plan.dimension || !(2..=3).contains(&mesh.dimension) {
            return Err("Transient boundary and mesh dimensions differ.".into());
        }
        let mut owners = HashMap::<Vec<usize>, Vec<usize>>::new();
        for cell in mesh.cells {
            if cell.nodes.len() != mesh.dimension + 1
                || cell.nodes.iter().any(|&n| n >= mesh.nodes.len())
                || cell.nodes.iter().copied().collect::<HashSet<_>>().len() != cell.nodes.len()
            {
                return Err("Transient boundary mesh has invalid simplex connectivity.".into());
            }
            for excluded in 0..cell.nodes.len() {
                let mut key = cell
                    .nodes
                    .iter()
                    .enumerate()
                    .filter_map(|(i, &n)| (i != excluded).then_some(n))
                    .collect::<Vec<_>>();
                key.sort_unstable();
                owners.entry(key).or_default().push(cell.nodes[excluded]);
            }
        }
        let mut by_tag = HashMap::<usize, Vec<(Vec<usize>, usize)>>::new();
        let wanted = selected.iter().map(|row| row.tag).collect::<HashSet<_>>();
        let mut seen = HashSet::new();
        for (vertices, tag) in facets.iter().filter(|(_, tag)| wanted.contains(tag)) {
            let mut key = vertices.clone();
            key.sort_unstable();
            if key.len() != mesh.dimension
                || key.windows(2).any(|pair| pair[0] == pair[1])
                || owners.get(&key).is_none_or(|v| v.len() != 1)
                || !seen.insert(key.clone())
            {
                return Err(
                    "A selected transient boundary must be a unique exterior facet.".into(),
                );
            }
            let opposite = owners[&key][0];
            by_tag.entry(*tag).or_default().push((key, opposite));
        }
        let mut values = Vec::new();
        let mut traces = Vec::new();
        for row in selected {
            let kind = row
                .row
                .get("type")
                .and_then(Value::as_str)
                .ok_or("Transient boundary type is required.")?;
            let law = laws.get(kind).ok_or_else(|| {
                format!("Transient boundary type {kind:?} has no declared time-domain law.")
            })?;
            let field = law
                .get("field")
                .and_then(Value::as_str)
                .ok_or("Transient boundary field is required.")?;
            let (field_index, state) = plan
                .fields
                .iter()
                .enumerate()
                .find(|(_, f)| f.id == field)
                .ok_or("Transient boundary names an undeclared field.")?;
            let input = law
                .get("input")
                .ok_or("Transient boundary input is required.")?;
            let facet_rows = by_tag.get(&row.tag).ok_or_else(|| {
                format!(
                    "Transient boundary tag {} has no selected exterior facets.",
                    row.tag
                )
            })?;
            let input_plan = BoundaryInput::compile(input, row.row)?;
            if law.get("kind").and_then(Value::as_str) == Some("nodalTrace") {
                if !matches!(
                    input.get("mode").and_then(Value::as_str),
                    Some("trace" | "impedance")
                ) {
                    return Err(
                        "Transient nodal trace input must use trace or impedance mode.".into(),
                    );
                }
                let key = input_plan
                    .form_key()
                    .ok_or("Transient trace input must select a boundary form.")?
                    .to_owned();
                let form = BoundaryFormPlan::from_json(
                    boundary_forms
                        .get(&key)
                        .ok_or("Transient boundary form is not declared.")?,
                )?;
                form.require_shape(plan.dimension, state.components)?;
                let facets = facet_rows
                    .iter()
                    .map(|(vertices, opposite)| facet_geometry(mesh, vertices, *opposite))
                    .collect::<Result<Vec<_>, _>>()?;
                traces.push(NodalTrace {
                    field: field_index,
                    facets,
                    input: input_plan,
                    key,
                    form,
                });
                continue;
            }
            if law.get("kind").and_then(Value::as_str) != Some("nodalValue")
                || input.get("mode").and_then(Value::as_str) != Some("point")
            {
                return Err(format!("Unsupported transient boundary law {kind:?}."));
            }
            let form_name = law
                .get("essentialForm")
                .and_then(Value::as_str)
                .ok_or("Transient essentialForm is required.")?;
            let form = EssentialFormPlan::from_json(
                essential_forms
                    .get(form_name)
                    .ok_or("Transient essential form is not declared.")?,
            )?;
            form.require_shape(plan.dimension, state.components)?;
            let rate_field=law.get("rateField").map(|value| {
                let name=value.as_str().filter(|v|!v.is_empty()).ok_or("Transient rateField must be a nonempty string.")?;
                let other=plan.fields.iter().find(|field|field.id==name).ok_or("Transient rateField is not declared.")?;
                if name==field || other.components!=state.components {return Err("Transient rateField must be a different field with matching components.");}
                Ok(name.to_owned())
            }).transpose()?;
            let mut nodes = facet_rows
                .iter()
                .flat_map(|(vertices, _)| vertices.iter().copied())
                .collect::<Vec<_>>();
            nodes.sort_unstable();
            nodes.dedup();
            values.push(NodalValue {
                field: field.into(),
                rate_field,
                nodes,
                input: input_plan,
                form,
            });
        }
        Ok(Self { values, traces })
    }

    pub(crate) fn apply(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        mesh: &NodalMesh<'_>,
        parameters: &HashMap<String, Scalar>,
        time: f64,
        operators: &mut TransientOperators,
    ) -> Result<(), String> {
        for boundary in &self.values {
            let components = plan
                .fields
                .iter()
                .find(|f| f.id == boundary.field)
                .ok_or("Transient boundary field disappeared.")?
                .components;
            let mut values = Vec::new();
            for &node in &boundary.nodes {
                let point = *mesh
                    .nodes
                    .get(node)
                    .ok_or("Transient boundary node disappeared.")?;
                let input = boundary.input.evaluate_at_time(
                    parameters,
                    Some(point),
                    time,
                    &HashMap::new(),
                    None,
                )?;
                let prescribed = boundary.form.point_values(
                    input
                        .essential
                        .ok_or("Transient boundary returned no essential value.")?,
                    input.essential_vector.as_deref(),
                    "transient boundary point",
                )?;
                for (component, value) in prescribed.into_iter().enumerate() {
                    let dof = node
                        .checked_mul(components)
                        .and_then(|n| n.checked_add(component))
                        .ok_or("Transient boundary degree of freedom overflow.")?;
                    values.push((dof, value));
                }
            }
            crate::transient_runner::add_essential_values(
                plan,
                layout,
                &boundary.field,
                &values,
                operators,
            )?;
            if let Some(target) = &boundary.rate_field {
                let local = values.iter().map(|(dof, _)| *dof).collect::<Vec<_>>();
                crate::transient_runner::add_essential_rates(
                    plan,
                    layout,
                    &boundary.field,
                    target,
                    &local,
                    operators,
                )?;
            }
        }
        for boundary in &self.traces {
            let components = plan.fields[boundary.field].components;
            for facet in &boundary.facets {
                let quadrature: Vec<Vec<f64>> = if plan.dimension == 2 {
                    let a = 0.5 - 0.5 / 3f64.sqrt();
                    vec![vec![a, 1. - a], vec![1. - a, a]]
                } else {
                    vec![
                        vec![2. / 3., 1. / 6., 1. / 6.],
                        vec![1. / 6., 2. / 3., 1. / 6.],
                        vec![1. / 6., 1. / 6., 2. / 3.],
                    ]
                };
                let weight = facet.measure / (quadrature.len() as f64);
                for weights in quadrature {
                    let mut point = [0.; 3];
                    let mut dofs = Vec::new();
                    let mut basis = Vec::<Vec<f64>>::new();
                    for (&node, &value) in facet.nodes.iter().zip(&weights) {
                        for axis in 0..3 {
                            point[axis] += value * mesh.nodes[node][axis];
                        }
                        for component in 0..components {
                            dofs.push(
                                node.checked_mul(components)
                                    .and_then(|n| n.checked_add(component))
                                    .ok_or("Transient boundary DOF overflow.")?,
                            );
                            let mut b = vec![0.; components];
                            b[component] = value;
                            basis.push(b);
                        }
                    }
                    let basis = basis.iter().map(Vec::as_slice).collect::<Vec<_>>();
                    plan.assemble_boundary_at_time(
                        layout,
                        boundary.field,
                        &dofs,
                        &basis,
                        &facet.normal,
                        &boundary.key,
                        &boundary.form,
                        &boundary.input,
                        parameters,
                        &HashMap::new(),
                        crate::transient_plan::SampleLocation {
                            cell: 0,
                            material_tag: 0,
                            point,
                            time,
                        },
                        weight,
                        None,
                        operators,
                    )?;
                }
            }
        }
        Ok(())
    }
}

fn facet_geometry(
    mesh: &NodalMesh<'_>,
    vertices: &[usize],
    opposite: usize,
) -> Result<Facet, String> {
    let p = mesh.nodes[vertices[0]];
    let q = mesh.nodes[vertices[1]];
    let a = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
    let mut normal = if mesh.dimension == 2 {
        vec![a[1], -a[0]]
    } else {
        let r = mesh.nodes[vertices[2]];
        let b = [r[0] - p[0], r[1] - p[1], r[2] - p[2]];
        vec![
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let norm = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= 0. {
        return Err("Transient boundary facet is degenerate.".into());
    }
    for x in &mut normal {
        *x /= norm;
    }
    if normal
        .iter()
        .enumerate()
        .map(|(i, n)| n * (mesh.nodes[opposite][i] - p[i]))
        .sum::<f64>()
        > 0.
    {
        for x in &mut normal {
            *x = -*x;
        }
    }
    Ok(Facet {
        nodes: vertices.to_vec(),
        normal,
        measure: if mesh.dimension == 2 { norm } else { norm / 2. },
    })
}
