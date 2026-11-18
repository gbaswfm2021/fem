//! Bind selected model data to the generic prepared vector P1 runner.
//! This in-memory adapter does not choose a scenario profile or publish files.
use std::collections::{HashMap, HashSet};
use serde_json::{Value, json};
use crate::{C, SolverCache};
use crate::model_expression::Scalar;
use crate::prepared_case::{CaseKind, PreparedCase};
use crate::prepared_vector::{self, Cell, Point, OutputMapping};
use crate::scenario_plan::ScenarioPlan;
use crate::simplex_topology::{self, SimplexTopology, TopologyPolicy, DeterminantTolerance, SelectedFacetPolicy, SelectedFacetOwnership, SelectedFacetDuplicates};
use crate::vector_p1;

pub(crate) enum Mesh<'a> {
    Triangle(&'a crate::mesh2d::Mesh2D),
    Tetrahedron(&'a crate::mesh3d::IndexedMesh3D),
}

pub(crate) fn solve(case: &PreparedCase, package: &Value, model: &Value, map: &Value,
    mesh: Mesh<'_>, parameters: &HashMap<String, Scalar>, cache: &mut SolverCache) -> Result<prepared_vector::Result, String> {
    let frequency = match case.kind { CaseKind::Frequency { frequency_hz, .. } => frequency_hz, _ => return Err("Prepared vector execution requires a frequency case.".into()) };
    if package.get("id").and_then(Value::as_str) != Some(case.profile.as_str()) { return Err("Prepared vector case and package identities differ.".into()); }
    if case.settings.element_order != 1 { return Err("Prepared vector execution requires first-order elements.".into()); }
    let policy = package.get("preparedVector").ok_or("preparedVector policy is required.")?;
    validate_formulation_match(model,&case.settings.formulation,policy)?;
    let plan = ScenarioPlan::from_package(package, &case.settings.formulation)?;
    let topology = policy.get("topology").ok_or("Vector topology policy is required.")?;
    let flag = |key: &str| topology.get(key).and_then(Value::as_bool).ok_or_else(|| format!("Vector topology {key} must be boolean."));
    let tolerance = topology.get("determinantTolerance").ok_or("Vector determinant tolerance is required.")?;
    let threshold = tolerance.get("value").and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.).ok_or("Vector determinant threshold is invalid.")?;
    let reject_equal = tolerance.get("rejectEqual").and_then(Value::as_bool).ok_or("Vector determinant rejectEqual is required.")?;
    let determinant_tolerance = match tolerance.get("kind").and_then(Value::as_str) {
        Some("absolute") => DeterminantTolerance::Absolute { minimum:threshold, reject_equal },
        Some("relative") => DeterminantTolerance::Relative { fraction:threshold, reject_equal },
        _ => return Err("Unsupported vector determinant tolerance.".into()),
    };
    let topology_policy = TopologyPolicy { reject_duplicate_cells:flag("rejectDuplicateCells")?, reject_non_manifold_facets:flag("rejectNonManifoldFacets")?,
        require_all_nodes_used:flag("requireAllNodesUsed")?, require_nonempty_cells:flag("requireNonemptyCells")?, determinant_tolerance:Some(determinant_tolerance) };
    match mesh {
        Mesh::Triangle(mesh) => {
            let connectivity: Vec<_> = mesh.triangles.iter().map(|(nodes,_)| *nodes).collect();
            let topology = simplex_topology::triangles(&mesh.nodes, &connectivity, &[], topology_policy)?;
            let points = crate::quadrature::triangle(&case.settings.quadrature)?.into_iter().map(|q| ([q[0],q[1],q[2]],q[3])).collect();
            let facets = vec![([0.5-0.5/3.0_f64.sqrt(),0.5+0.5/3.0_f64.sqrt()],0.5),([0.5+0.5/3.0_f64.sqrt(),0.5-0.5/3.0_f64.sqrt()],0.5)];
            if policy.get("boundaryQuadrature").and_then(Value::as_str) != Some("gauss2") { return Err("Unsupported vector edge quadrature.".into()); }
            let nodes = mesh.nodes.iter().map(|p| [p[0],p[1]]).collect::<Vec<_>>();
            run::<2,3,2>(case, package, model, map, parameters, cache, frequency, &plan, policy, &mesh.nodes, &nodes, &mesh.triangles, &mesh.boundary_edges, &topology, points, facets)
        },
        Mesh::Tetrahedron(mesh) => {
            let connectivity: Vec<_> = mesh.tetrahedra.iter().map(|(nodes,_)| *nodes).collect();
            let topology = simplex_topology::tetrahedra(&mesh.nodes, &connectivity, &[], topology_policy)?;
            let points = crate::quadrature::tetrahedron(&case.settings.quadrature)?.into_iter().map(|q| ([q[0],q[1],q[2],q[3]],q[4])).collect();
            if policy.get("boundaryQuadrature").and_then(Value::as_str) != Some("triangle3") { return Err("Unsupported vector face quadrature.".into()); }
            let facets = vec![([2./3.,1./6.,1./6.],1./3.),([1./6.,2./3.,1./6.],1./3.),([1./6.,1./6.,2./3.],1./3.)];
            run::<3,4,3>(case, package, model, map, parameters, cache, frequency, &plan, policy, &mesh.nodes, &mesh.nodes, &mesh.tetrahedra, &mesh.boundary_faces, &topology, points, facets)
        }
    }
}

pub(crate) fn validate_formulation_match(model:&Value,formulation:&str,policy:&Value) -> Result<(),String> {
    let Some(declaration)=policy.get("formulationMatch") else {return Ok(());};
    let path=declaration.get("modelPath").and_then(Value::as_str).filter(|path|path.starts_with('/') && path.len()>1)
        .ok_or("Vector formulationMatch.modelPath must be a JSON pointer.")?;
    if model.pointer(path).and_then(Value::as_str)!=Some(formulation) {
        return Err(format!("Model formulation at {path:?} must match the requested formulation {formulation:?}."));
    }
    Ok(())
}

fn run<const D: usize, const N: usize, const F: usize>(case: &PreparedCase, package: &Value, model: &Value, map: &Value,
    parameters: &HashMap<String, Scalar>, cache: &mut SolverCache, frequency: f64, plan: &ScenarioPlan, policy: &Value,
    physical_nodes: &[[f64;3]], nodes: &[[f64;D]], cells: &[([usize;N],usize)], facets: &[([usize;F],usize)],
    topology: &SimplexTopology<F>, quadrature: Vec<([f64;N],f64)>, facet_quadrature: Vec<([f64;F],f64)>) -> Result<prepared_vector::Result,String> {
    let periodic_policy = policy.get("periodic").and_then(Value::as_str).ok_or("Vector periodic policy is required.")?;
    let physics = model.pointer("/model/physics").ok_or("Model physics is required.")?;
    let periodic_enabled = crate::model_schema::physics_periodic_conditions(physics).any(|value| value.get("enabled").and_then(Value::as_bool) != Some(false));
    if periodic_policy == "reject" && periodic_enabled { return Err("Periodic vector conditions are unsupported by this package.".into()); }
    if !matches!(periodic_policy,"reject"|"matchedNodes") { return Err("Unsupported vector periodic policy.".into()); }
    let selection_plan = plan.selections.as_ref().ok_or("Vector selection plan is required.")?;
    let source_by_cell = selection_plan.uses_cell_map("domainSources", map);
    let selected = selection_plan.bind(model, map, &HashMap::from([
        ("domainSources".into(),cells.iter().map(|(_,tag)| *tag).collect()), ("materials".into(),cells.iter().map(|(_,tag)| *tag).collect()), ("boundaries".into(),facets.iter().map(|(_,tag)| *tag).collect())]))?;
    let source_data = crate::domain_source::prepare(plan.domain_source.as_ref(), selected.get("domainSources").map(Vec::as_slice))?;
    let mut source_samples = source_data.as_ref().map(|_| Vec::with_capacity(cells.len()));
    let material_rows = selected.get("materials").ok_or("Vector materials selection is required.")?.iter().map(|row| (row.tag,row.row.clone())).collect();
    let compiled = crate::material::CompiledMaterialAssignments::compile(&case.profile,&material_rows)?;
    let constant = compiled.position_independent_values(parameters,frequency)?;
    let mut context = parameters.clone(); context.insert("omega".into(), Scalar { value:std::f64::consts::TAU*frequency, dimension:[0.;5] });
    let mut prepared_cells = Vec::new();
    for (index,(cell,tag)) in cells.iter().enumerate() {
        let mut points = Vec::new();
        let mut loads = source_data.as_ref().map(|_| Vec::with_capacity(quadrature.len()));
        for (basis,weight) in &quadrature {
            let position = std::array::from_fn(|axis| (0..N).map(|i| basis[i]*physical_nodes[cell[i]][axis]).sum());
            let raw = if let Some(value) = constant.get(tag) { value.clone() } else { compiled.evaluate(*tag,parameters,frequency,position,&format!("cell {index}"))? };
            let symbols = prepared_vector::bind_material(plan,&raw,&context,"material")?;
            if let (Some((load_plan, sources)), Some(loads)) = (&source_data, &mut loads) {
                let source_key = if source_by_cell { index } else { *tag };
                loads.push(crate::domain_source::sample_load(load_plan, sources, source_key, position, parameters, Some(frequency), &symbols)?);
            }
            points.push(Point { basis:*basis, weight:*weight, symbols });
        }
        if let (Some(all), Some(loads)) = (&mut source_samples, loads) { all.push(loads); }
        let centroid = std::array::from_fn(|axis| cell.iter().map(|&node| physical_nodes[node][axis]).sum::<f64>()/N as f64);
        let raw = if let Some(value) = constant.get(tag) { value.clone() } else { compiled.evaluate(*tag,parameters,frequency,centroid,&format!("cell {index} centroid"))? };
        prepared_cells.push(Cell { nodes:*cell, points, recovery_symbols:prepared_vector::bind_material(plan,&raw,&context,"material")? });
    }
    let selected_boundaries = selected.get("boundaries").ok_or("Vector boundaries selection is required.")?;
    let inputs = crate::declared_boundary::compile_rows(package,&case.settings.formulation,selected_boundaries.iter().map(|row| (row.tag,row.row)))?;
    let tags = selected_boundaries.iter().map(|row| row.tag).collect::<Vec<_>>();
    let chosen = facets.iter().copied().filter(|(_,tag)| tags.contains(tag)).collect::<Vec<_>>();
    topology.bind_selected_facets(&chosen,&tags,SelectedFacetPolicy { ownership:SelectedFacetOwnership::ExteriorOnly, duplicates:SelectedFacetDuplicates::KeepAll, require_positive_tag:true })?;
    let mut boundaries = Vec::new(); let mut essential = Vec::new();
    let essential_name = policy.get("essentialForm").and_then(Value::as_str).ok_or("Vector essential form name is required.")?;
    let essential_plan = plan.essential.get(essential_name).ok_or("Declared vector essential form is missing.")?;
    let zero_types: HashSet<&str> = policy.get("zeroOnlyScalarInputs").and_then(Value::as_array).ok_or("Vector scalar policy is required.")?.iter().map(|v| v.as_str().ok_or("Vector scalar policy name must be a string.")).collect::<Result<_,_>>()?;
    let zero_tolerance = policy.get("zeroTolerance").and_then(Value::as_f64).filter(|v| v.is_finite() && *v>=0.).ok_or("Vector scalar tolerance is invalid.")?;
    for (facet,tag) in chosen {
        let input = &inputs[&tag];
        if input.form_key().is_none() {
            for &node in &facet {
                let symbols = crate::scalar2d::symbols_at_point(parameters,frequency,physical_nodes[node]);
                let evaluated = input.evaluate(&symbols,Some(physical_nodes[node]),&HashMap::new(),None)?;
                let values = essential_plan.point_values(evaluated.essential.ok_or("Missing vector prescribed scalar.")?,evaluated.essential_vector.as_deref(),"selected facet")?;
                let values: [C;D] = values.try_into().map_err(|_| "Vector prescribed value has wrong component count.")?;
                essential.push(vector_p1::EssentialPoint { node, values });
            }
        } else {
            let key = input.form_key().unwrap();
            let form = plan.boundaries.get(key).ok_or("Vector boundary form is missing.")?;
            let adjacent = topology.facet(facet).ok_or("Vector boundary has no owner.")?;
            let (normal,measure) = if D == 2 {
                let edge = simplex_topology::Facet::<2> { vertices:[adjacent.vertices[0],adjacent.vertices[1]], owners:adjacent.owners.clone() };
                simplex_topology::edge_normal(physical_nodes,&edge,0)?
            } else {
                let face = simplex_topology::Facet::<3> { vertices:[adjacent.vertices[0],adjacent.vertices[1],adjacent.vertices[2]], owners:adjacent.owners.clone() };
                simplex_topology::face_normal(physical_nodes,&face,0)?
            };
            let kind = selected_boundaries.iter().find(|row| row.tag==tag).and_then(|row| row.row.get("type")).and_then(Value::as_str).ok_or("Boundary type is required.")?;
            for (shape,weight) in &facet_quadrature {
                let point = std::array::from_fn(|axis| (0..F).map(|i| shape[i]*physical_nodes[facet[i]][axis]).sum());
                let symbols = crate::scalar2d::symbols_at_point(parameters,frequency,point);
                let values = input.evaluate(&symbols,Some(point),&HashMap::from([("omega".into(),C { re:std::f64::consts::TAU*frequency,im:0. })]),Some(form))?;
                if zero_types.contains(kind) && values.prescribed_scalar.is_some_and(|v| v.re.hypot(v.im)>zero_tolerance) { return Err("Boundary scalar violates its declared zero policy.".into()); }
                if let Some(bounds) = policy.get("minimumRealInputs").and_then(Value::as_object) { for (name,bound) in bounds {
                    if let Some(value) = values.coefficient_inputs.get(name) {
                        let minimum = bound.as_f64().filter(|v| v.is_finite()).ok_or("Vector coefficient lower bound must be finite.")?;
                        if value.re < minimum { return Err(format!("Boundary coefficient {name:?} violates its declared minimum.")); }
                    }
                }}
                boundaries.push(vector_p1::BoundaryPoint { nodes:facet.to_vec(), basis:shape.to_vec(), normal:std::array::from_fn(|axis| normal[axis]),
                    weight:measure*weight, plan:form, coefficients:values.coefficients, sources:values.sources });
            }
        }
    }
    let periodic = if periodic_policy == "matchedNodes" {
        let entities = facets.iter().map(|(nodes,tag)| (nodes.to_vec(),*tag)).collect::<Vec<_>>();
        Some(crate::scalar2d::periodic_dof_map_for_entities(model,map,physical_nodes,&entities,D)?)
    } else { None };
    let mapping = OutputMapping { vector_components:policy.get("vectorComponents").and_then(Value::as_array).ok_or("Vector output components are required.")?.iter()
        .map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned).ok_or("Vector output component must be a name.".to_string())).collect::<Result<_,_>>()?,
        tensor_matrix:policy.get("tensorMatrix").and_then(Value::as_str).ok_or("Vector tensor output name is required.")?.to_owned() };
    let maximum_component = policy.get("maximumComponent").and_then(Value::as_u64).filter(|&axis| axis < D as u64).ok_or("Vector maximum component is invalid.")? as usize;
    let material_fields = crate::material::fields_by_cell(model,map,frequency,physical_nodes,cells,[0,1,2],[0.;3])?;
    let count = periodic.as_ref().map_or(nodes.len(),|mapping| mapping.dof_count)*D;
    let data = crate::result_contract::ResultData { values:HashMap::from([
        ("frequency".into(),json!(frequency)),("formulation".into(),json!(case.settings.formulation)),("nodes".into(),json!(physical_nodes)),
        ("cells".into(),json!(cells.iter().map(|(nodes,tag)| json!({"nodes":nodes.as_slice(),"materialTag":tag})).collect::<Vec<_>>())),
        ("materialFields".into(),Value::Object(material_fields)),("backend".into(),json!(case.settings.backend)),("ordering".into(),json!(case.settings.ordering)),
        ("threads".into(),json!(case.settings.threads)),("unknowns".into(),json!(count)),("cellCount".into(),json!(cells.len())),("residual".into(),json!(0.)),("maximum".into(),json!(0.)),
    ]),complex:HashMap::new(),series:HashMap::new(),matrices:HashMap::new() };
    let periodic = periodic.as_ref().map(|p| vector_p1::PeriodicMap { representative:&p.node_to_dof, weights:&p.node_weights, count:p.dof_count });
    let mut result = prepared_vector::solve(plan,nodes,&prepared_cells,&boundaries,&essential,periodic,essential_name,&mapping,data,
        &case.settings.backend,&case.settings.ordering,case.settings.threads,1e-7,cache,
        source_data.as_ref().zip(source_samples.as_ref()).map(|((load_plan, _), samples)| (&load_plan.form, samples.as_slice())))?;
    let maximum = result.solution.real.iter().zip(&result.solution.imaginary).skip(maximum_component).step_by(D)
        .map(|(re,im)| re.hypot(*im)).fold(0.0, f64::max);
    result.data.values.insert("maximum".into(),json!(maximum)); result.data.values.insert("residual".into(),json!(result.solution.residual));
    Ok(result)
}


