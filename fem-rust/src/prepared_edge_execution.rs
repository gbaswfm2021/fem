//! Declared, configured oriented-field numerical case preparation.
//!
//! Shared numerical preparation evaluates declared forms, constraints and
//! configured integration; a separate generic publisher writes the results.
use std::collections::HashMap;
use std::time::Instant;
use serde_json::Value;
use crate::{C, SolverCache};
use crate::prepared_configured::ConfiguredCase;
use crate::prepared_edge::PreparedEdgeMesh;
use crate::simplex_topology::{SelectedFacetDuplicates, SelectedFacetOwnership, SelectedFacetPolicy, TopologyPolicy};

pub(crate) struct NumericalResult {
    pub(crate) node_count: usize,
    pub(crate) cell_count: usize,
    pub(crate) residual: f64,
    pub(crate) reduced_dofs: usize,
    pub(crate) data: crate::result_contract::ResultData,
}

fn boolean(value:&Value, name:&str) -> Result<bool,String> {
    value.get(name).and_then(Value::as_bool).ok_or_else(||format!("Edge policy {name} must be boolean."))
}

/// This adapter currently interprets Cartesian component values in model
/// coordinates. The scenario must declare the input location and supported
/// value; other coordinate conventions require an explicit transformation.
pub(crate) fn validate_component_coordinates(configuration:&Value,policy:&Value) -> Result<(),String> {
    let declaration=policy.get("componentCoordinates").and_then(Value::as_object)
        .ok_or("Edge componentCoordinates declaration is required.")?;
    let path=declaration.get("inputPath").and_then(Value::as_str).filter(|path|path.starts_with('/') && path.len()>1)
        .ok_or("Edge component coordinate inputPath must be a nonempty JSON pointer.")?;
    let expected=declaration.get("value").and_then(Value::as_str).filter(|value|*value=="model")
        .ok_or("Only model-coordinate field components are supported by the edge adapter.")?;
    if configuration.pointer(path).and_then(Value::as_str)!=Some(expected) {
        return Err(format!("Configured field components at {path:?} must use the declared {expected:?} coordinate system."));
    }
    Ok(())
}

fn validate_quadrature(rule:&str,policy:&Value) -> Result<(),String> {
    let allowed=policy.get("supportedQuadrature").and_then(Value::as_array).filter(|rows|!rows.is_empty())
        .ok_or("Edge supportedQuadrature must be a nonempty array.")?;
    let mut names=std::collections::HashSet::new();
    for row in allowed {
        let name=row.as_str().filter(|v|!v.is_empty()).ok_or("Edge quadrature names must be nonempty strings.")?;
        if !names.insert(name) {return Err("Edge quadrature names must be unique.".into());}
        crate::quadrature::tetrahedron(name)?;
    }
    if !names.contains(rule) {return Err(format!("Constitutive quadrature {rule:?} is not supported by this edge formulation."));}
    Ok(())
}

fn periodic_maps(mesh:&PreparedEdgeMesh, case:&ConfiguredCase, boundaries:&[crate::prepared_edge::CompiledBoundary],
    policy:&Value, order:u8) -> Result<crate::nedelec_dofs::ConstrainedDofs,String> {
    let tolerance = policy.get("matchingTolerance").and_then(Value::as_f64).filter(|x|x.is_finite() && *x>0.)
        .ok_or("Edge periodic matchingTolerance must be positive and finite.")?;
    let orientation = policy.get("orientationTolerance").and_then(Value::as_f64).filter(|x|x.is_finite() && *x>0.)
        .ok_or("Edge orientation tolerance must be positive and finite.")?;
    let rows = case.configuration.get("periodicPairs").and_then(Value::as_array).ok_or("Configured periodicPairs must be an array.")?;
    let map_rows = case.mesh_map.get("periodicPairs").and_then(Value::as_array).ok_or("Mesh map periodicPairs must be an array.")?;
    let selection_rows = case.mesh_map.get("boundarySelections").and_then(Value::as_array).ok_or("Mesh map boundarySelections must be an array.")?;
    let mut tags = HashMap::new();
    for row in selection_rows {
        let id=row.get("id").and_then(Value::as_str).ok_or("Periodic boundary selection has no ID.")?;
        let tag=row.get("physicalTag").and_then(Value::as_u64).filter(|tag|*tag>0).ok_or("Periodic boundary selection has no positive tag.")? as usize;
        if tags.insert(id,tag).is_some() { return Err(format!("Duplicate boundary selection {id:?}.")); }
    }
    let mut map_pairs = HashMap::new();
    for row in map_rows {
        let id=row.get("id").and_then(Value::as_str).ok_or("Mesh periodic pair has no ID.")?;
        let lower=row.get("lowerSelectionId").and_then(Value::as_str).ok_or("Mesh periodic pair has no lower selection.")?;
        let upper=row.get("upperSelectionId").and_then(Value::as_str).ok_or("Mesh periodic pair has no upper selection.")?;
        if map_pairs.insert(id,(lower,upper)).is_some() { return Err(format!("Duplicate mesh periodic pair {id:?}.")); }
    }
    let mut seen = std::collections::HashSet::new();
    let mut prepared = Vec::new();
    for row in rows {
        let id=row.get("id").and_then(Value::as_str).ok_or("Configured periodic pair has no ID.")?;
        if !seen.insert(id) { return Err(format!("Duplicate configured periodic pair {id:?}.")); }
        let lower=row.get("lowerSelectionId").and_then(Value::as_str).ok_or("Configured periodic pair has no lower selection.")?;
        let upper=row.get("upperSelectionId").and_then(Value::as_str).ok_or("Configured periodic pair has no upper selection.")?;
        if map_pairs.get(id).copied()!=Some((lower,upper)) { return Err(format!("Periodic pair {id:?} does not match the mesh map.")); }
        let lower=*tags.get(lower).ok_or("Periodic lower selection is missing from the mesh map.")?;
        let upper=*tags.get(upper).ok_or("Periodic upper selection is missing from the mesh map.")?;
        if lower==upper || boundaries.iter().any(|b|b.tag==lower || b.tag==upper) {
            return Err(format!("Periodic pair {id:?} overlaps itself or an ordinary boundary."));
        }
        let vector = |name:&str| -> Result<[f64;3],String> {
            let values=row.get(name).and_then(Value::as_array).filter(|v|v.len()==3).ok_or_else(||format!("Periodic {name} needs three components."))?;
            let mut result=[0.;3];
            for axis in 0..3 { result[axis]=values[axis].as_f64().filter(|v|v.is_finite()).ok_or_else(||format!("Periodic {name} must be finite."))?; }
            Ok(result)
        };
        let translation=vector("translationM")?;
        let factor=crate::solver_common::floquet_factor(vector("phaseVector")?,translation)?;
        let mapping=mesh.translated_nodes(lower,upper,translation,tolerance)?;
        prepared.push((lower,upper,mapping,factor));
    }
    let pairs:Vec<_> = prepared.iter().map(|(lower,upper,mapping,factor)|crate::prepared_edge::MatchedSurface {
        lower_tag:*lower,upper_tag:*upper,node_map:mapping,factor:*factor }).collect();
    mesh.periodic(order,&pairs,orientation)
}

fn prepare_mesh(case:&ConfiguredCase,policy:&Value,mesh_text:&str,order:u8) -> Result<PreparedEdgeMesh,String> {
    validate_component_coordinates(&case.configuration,policy)?;
    let topology = policy.get("topology").ok_or("Edge topology policy is required.")?;
    let tolerance = match topology.get("determinantTolerance") {
        Some(Value::Null) => None,
        _ => return Err("Unsupported declared edge determinant tolerance policy.".into()),
    };
    let topology = TopologyPolicy { reject_duplicate_cells:boolean(topology,"rejectDuplicateCells")?,
        reject_non_manifold_facets:boolean(topology,"rejectNonManifoldFacets")?,
        require_all_nodes_used:boolean(topology,"requireAllNodesUsed")?,
        require_nonempty_cells:boolean(topology,"requireNonemptyCells")?, determinant_tolerance:tolerance };
    let facet = policy.get("facetPolicy").ok_or("Edge facet policy is required.")?;
    let ownership = match facet.get("ownership").and_then(Value::as_str) {
        Some("anyExisting") => SelectedFacetOwnership::AnyExisting,
        _ => return Err("Unsupported edge facet ownership policy.".into()),
    };
    let duplicates = match facet.get("duplicates").and_then(Value::as_str) {
        Some("reject") => SelectedFacetDuplicates::Reject,
        _ => return Err("Unsupported edge facet duplicate policy.".into()),
    };
    PreparedEdgeMesh::from_stream(mesh_text,policy.get("coordinateFrame").ok_or("Edge coordinate frame is required.")?,
        order,&case.quadrature,topology,SelectedFacetPolicy {ownership,duplicates,require_positive_tag:boolean(facet,"requirePositiveTag")?},
        boolean(policy,"positiveEdgeTags")?)
}

/// Count both declared edge orders using the same selection and weighted
/// periodic rules as a solve, without assembling or publishing output.
pub(crate) fn count_dofs(case:&ConfiguredCase,package:&Value,mesh_text:&str) -> Result<(usize,usize,usize,usize),String> {
    let policy=package.get("preparedEdge").and_then(|v|v.get(&case.formulation)).ok_or("A preparedEdge formulation policy is required.")?;
    let plan=crate::scenario_plan::ScenarioPlan::from_package(package,&case.formulation)?;
    let mut counts=[0usize;2]; let mut shape=None;
    for order in [1u8,2] {
        let mesh=prepare_mesh(case,policy,mesh_text,order)?;
        let bound=mesh.bind_configured(case,plan.selections.as_ref().ok_or("Edge selection plan is required.")?,
            policy.get("selectionEntities").ok_or("Edge selection entities are required.")?)?;
        let contracts=package.get("boundaryInputs").and_then(|v|v.get(&case.formulation)).ok_or("Edge boundary inputs are required.")?;
        let boundaries=crate::prepared_edge::compile_boundaries(&bound,contracts,
            policy.get("boundaryCollections").ok_or("Edge boundary collection policies are required.")?,&plan)?;
        let mapped=periodic_maps(&mesh,case,&boundaries,policy.get("periodic").ok_or("Edge periodic policy is required.")?,order)?;
        counts[usize::from(order-1)]=mapped.count;
        let current=(mesh.nodes.len(),mesh.cells.len());
        if shape.is_some_and(|old|old!=current) {return Err("Edge mesh changes between element orders.".into());}
        shape=Some(current);
    }
    let (nodes,cells)=shape.ok_or("Edge mesh was not prepared.")?;
    Ok((nodes,cells,counts[0],counts[1]))
}

/// Solve a configured frequency case using only declared numerical policies.
/// The returned nodal fields are display projections; integration must use
/// the edge-basis sampler before the problem and solution go out of scope.
pub(crate) fn solve(case:&ConfiguredCase, package:&Value, mesh_text:&str, cache:&mut SolverCache) -> Result<NumericalResult,String> {
    let policy = package.get("preparedEdge").and_then(|v|v.get(&case.formulation)).ok_or("A preparedEdge formulation policy is required.")?;
    validate_quadrature(&case.quadrature,policy)?;
    let order = u8::try_from(case.element_order).map_err(|_|"Edge element order is out of range.")?;
    let plan = crate::scenario_plan::ScenarioPlan::from_package(package,&case.formulation)?;
    let mesh = prepare_mesh(case,policy,mesh_text,order)?;
    let selections = plan.selections.as_ref().ok_or("Edge selection plan is required.")?;
    let source_by_cell = selections.uses_cell_map("domainSources", &case.mesh_map);
    let bound = mesh.bind_configured(case,selections,policy.get("selectionEntities").ok_or("Edge selection entities are required.")?)?;
    let source_data = crate::domain_source::prepare_routes(&plan.domain_sources, bound.get("domainSources").map(Vec::as_slice))?;
    let material = policy.get("material").ok_or("Edge material policy is required.")?;
    if material.get("sampling").and_then(Value::as_str) != Some("cellCentroid") { return Err("Unsupported edge material sampling policy.".into()); }
    let collection = material.get("collection").and_then(Value::as_str).ok_or("Edge material collection is required.")?;
    let rows = bound.get(collection).ok_or("Declared edge material collection was not bound.")?;
    let assignments = crate::prepared_edge::material_rows(rows,material.get("path").and_then(Value::as_str).ok_or("Edge material path is required.")?,
        boolean(material,"fallbackToRow")?)?;
    let samples = mesh.centroid_materials(&case.profile,&assignments,&case.parameters,case.frequency_hz,&plan)?;
    let quadrature = mesh.volume_points(|element,_,_| Ok(samples[element].coefficients.clone()))?;
    let mut source_samples = Vec::with_capacity(source_data.len());
    for (_, load_plan, sources) in &source_data {
        let mut all = Vec::with_capacity(quadrature.len());
        for (element, points) in quadrature.iter().enumerate() {
            let mut sampled = Vec::with_capacity(points.len());
            for point in points {
                let source_key = if source_by_cell { element } else { mesh.cell_tags[element] };
                let mut value = crate::domain_source::sample_load(load_plan, sources, source_key,
                    mesh.frame.point_to_model(point.point), &case.parameters, Some(case.frequency_hz), &samples[element].symbols)?;
                let model: [C; 3] = value.as_slice().try_into().map_err(|_| "Edge domain source requires three components.")?;
                value.copy_from_slice(&mesh.frame.vector_to_internal(model));
                sampled.push(value);
            }
            all.push(sampled);
        }
        source_samples.push(all);
    }
    let contracts = package.get("boundaryInputs").and_then(|v|v.get(&case.formulation)).ok_or("Edge boundary inputs are required.")?;
    let boundaries = crate::prepared_edge::compile_boundaries(&bound,contracts,policy.get("boundaryCollections").ok_or("Edge boundary collection policies are required.")?,&plan)?;
    let periodic_policy = policy.get("periodic").ok_or("Edge periodic policy is required.")?;
    let mapped = periodic_maps(&mesh,case,&boundaries,periodic_policy,order)?;
    let periodic = crate::nedelec_driver::PeriodicMap { representative:&mapped.representative,weights:&mapped.weights,count:mapped.count };
    let mut moments = Vec::new();
    let mut points = Vec::new();
    let symbols = HashMap::from([("omega".to_string(),C {re:std::f64::consts::TAU*case.frequency_hz,im:0.})]);
    let mut boundary_parameters=case.parameters.clone();
    let frequency_dimension=crate::model_expression::unit("Hz")?.1;
    for (name,value) in [("freq",case.frequency_hz),("frequency",case.frequency_hz),("omega",std::f64::consts::TAU*case.frequency_hz)] {
        boundary_parameters.insert(name.into(),crate::model_expression::Scalar {value,dimension:frequency_dimension});
    }
    let source_vectors = policy.get("sourceVectors").and_then(Value::as_object).ok_or("Edge source-vector mappings are required.")?;
    for boundary in &boundaries {
        if boundary.essential {
            moments.extend(mesh.essential_moments(boundary.tag,order,&boundary.input,&plan.essential[&boundary.form],&boundary_parameters)?);
        } else {
            let mut mapping = HashMap::new();
            if let Some(declaration) = source_vectors.get(&boundary.form) {
                for (name,axes) in declaration.as_object().ok_or("Edge source-vector form must be an object.")? {
                    let axes = axes.as_array().filter(|a|a.len()==3).ok_or("Edge source-vector mapping needs three components.")?;
                    let mut names = [String::new(),String::new(),String::new()];
                    for axis in 0..3 { names[axis]=axes[axis].as_str().ok_or("Edge source-vector component must be a string.")?.to_owned(); }
                    mapping.insert(name.clone(),names);
                }
            }
            points.extend(mesh.boundary_points(boundary.tag,&boundary.input,&plan.boundaries[&boundary.form],&boundary_parameters,&symbols,&mapping)?);
        }
    }
    let essential = plan.essential.values().next().ok_or("An edge essential form is required.")?;
    if plan.essential.len()!=1 { return Err("Multiple edge essential-form policies are not yet supported.".into()); }
    let recovery = plan.recovery.as_ref().ok_or("An edge recovery policy is required.")?;
    let problem = crate::nedelec_driver::Problem { nodes:&mesh.nodes,tets:&mesh.cells,order,topology:&mesh.driver_topology,
        prepared:None,quadrature:&quadrature,form:&plan.volume,boundaries:&points,periodic:&periodic,
        essential:&moments,essential_plan:essential,recovery };
    let tolerance = policy.get("solveTolerance").and_then(Value::as_f64)
        .filter(|value|value.is_finite() && *value > 0.).ok_or("Edge solveTolerance must be positive and finite.")?;
    crate::solver_common::stage_started("assembly");
    let element_prepare_started = Instant::now();
    let prepared = problem.prepare_elements()?;
    println!(
        "  profile: generic element basis preparation {:.3}s",
        element_prepare_started.elapsed().as_secs_f64()
    );
    let problem = crate::nedelec_driver::Problem { prepared:Some(&prepared), ..problem };
    let matrix_assembly_started = Instant::now();
    let volume_loads = source_data.iter().zip(&source_samples)
        .map(|((_, load_plan, _), values)| (&load_plan.form, values.as_slice()))
        .collect::<Vec<_>>();
    let system = problem.assemble_with_volume_load_routes(&mut cache.assembly, &volume_loads)?;
    println!(
        "  profile: generic sparse matrix assembly and constraints {:.3}s",
        matrix_assembly_started.elapsed().as_secs_f64()
    );
    let real_block_unknowns = if case.backend == "rivrs" {
        if crate::solver_common::complex_symmetry_defect(&system.matrix) > 1e-10 { 4 * mapped.count } else { 2 * mapped.count }
    } else { mapped.count };
    let (real,imaginary,residual) = crate::solver_common::solve_linear_system(&system.matrix,&system.re,&system.im,
        tolerance,&case.backend,&case.ordering,case.threads,cache)?;
    let (real,imaginary) = crate::constraints::expand_weighted(&real,&imaginary,&mapped.representative,&mapped.weights,mapped.count)?;
    let solution = crate::nedelec_driver::Solution { real,imaginary,residual };
    let declarations = crate::model_schema::field_variables_for_profile(&case.profile,Some(&case.formulation),false)?;
    let output = crate::prepared_edge_output::EdgeOutput::new(&mesh,&problem,&solution,&samples,
        policy.get("fieldFamilies").ok_or("Edge field families are required.")?,&declarations,
        &source_data,&case.parameters,case.frequency_hz,source_by_cell)?;
    let fields = if case.field_files { output.nodal_fields()? } else { HashMap::new() };
    let configured = crate::configured_integration::ConfiguredIntegration::from_parts(&case.configuration,&case.mesh_map,&mesh,
        &crate::model_schema::engine_capabilities()?,policy.get("integrationSelectionKeys").ok_or("Integration selection ID keys are required.")?)?;
    let evaluator = crate::material::CompiledMaterialAssignments::compile(&case.profile,&assignments)?;
    let material_sampler = |cell:&crate::integration::Simplex, point:[f64;3]| {
        let tag=cell.material_tag.unwrap_or(cell.physical_tag);
        let values=evaluator.evaluate(tag,&case.parameters,case.frequency_hz,point,"integration material")?;
        Ok(values.into_iter().map(|(name,value)|(name,fullmeta_core::pointwise::ComplexValue {re:value.value,im:0.})).collect())
    };
    let mut environment:HashMap<String,fullmeta_core::pointwise::ComplexValue> = case.parameters.iter().map(|(name,value)|
        (name.clone(),fullmeta_core::pointwise::ComplexValue {re:value.value,im:0.})).collect();
    for (name,value) in [("freq",case.frequency_hz),("frequency",case.frequency_hz),("omega",std::f64::consts::TAU*case.frequency_hz)] {
        environment.insert(name.into(),fullmeta_core::pointwise::ComplexValue {re:value,im:0.});
    }
    let global_variables=output.evaluate_variables(&configured.variables,&configured.pointwise,&configured.operators,
        &configured.domains,&configured.boundaries,environment,&material_sampler)?;
    let material_output = policy.get("materialOutput").ok_or("Edge materialOutput declaration is required.")?;
    let material_fields = material_output.get("fields").and_then(Value::as_object).ok_or("Edge materialOutput.fields must be an object.")?;
    let representation = material_output.get("representation").and_then(Value::as_str).ok_or("Edge materialOutput representation is required.")?;
    let mut material_properties = serde_json::Map::new();
    for (index,&tag) in mesh.cell_tags.iter().enumerate() {
        let key=tag.to_string();
        if material_properties.contains_key(&key) { continue; }
        let mut row=serde_json::Map::new();
        for (name,symbol) in material_fields {
            let symbol=symbol.as_str().ok_or("Edge material output symbol must be a string.")?;
            let value=samples[index].symbols.get(symbol).ok_or_else(||format!("Edge material sample lacks output symbol {symbol:?}."))?;
            if !value.re.is_finite() || value.im!=0. { return Err(format!("Edge material output {name:?} must be finite and real.")); }
            row.insert(name.clone(),serde_json::json!(value.re));
        }
        row.insert("sampleElementIndex".into(),serde_json::json!(index));
        row.insert("samplePointM".into(),serde_json::json!(samples[index].model_point));
        row.insert("representation".into(),serde_json::json!(representation));
        material_properties.insert(key,Value::Object(row));
    }
    let material_fields = if case.field_files {
        let material_cells:Vec<_> = mesh.cells.iter().copied().zip(mesh.cell_tags.iter().copied()).collect();
        Some(crate::material::fields_by_assignment_cells(&case.profile,&assignments,&case.parameters,
            case.frequency_hz,&mesh.model_nodes,&material_cells,[0,1,2],[0.;3])?)
    } else {
        // Legacy output construction validated material expressions at each
        // cell vertex even without field files. Retain those errors without
        // allocating the per-cell visualization arrays.
        for (element,cell) in mesh.cells.iter().enumerate() { for &node in cell {
            evaluator.evaluate(mesh.cell_tags[element],&case.parameters,case.frequency_hz,mesh.model_nodes[node],"material field")?;
        }}
        None
    };
    let output_policy=policy.get("output").ok_or("Edge output policy is required.")?;
    let order_text=case.element_order.to_string();
    let discretization=output_policy.get("discretizationByOrder").and_then(|v|v.get(&order_text)).and_then(Value::as_str)
        .ok_or("Edge output has no discretization label for the selected order.")?;
    let ordering_backend=output_policy.get("orderingUnavailableUnlessBackend").and_then(Value::as_str)
        .ok_or("Edge output ordering policy is required.")?;
    let origin=mesh.frame.vector_to_internal(mesh.frame.point_to_model([0.;3]));
    let geometry:[f64;3] = std::array::from_fn(|axis| mesh.nodes.iter().map(|p|p[axis]).fold(f64::NEG_INFINITY,f64::max)
        - mesh.nodes.iter().map(|p|p[axis]).fold(f64::INFINITY,f64::min));
    let complex:HashMap<String,C> = global_variables.iter().map(|(name,value)| {
        let re=value.get("re").and_then(Value::as_f64).ok_or("Global variable has no real value.")?;
        let im=value.get("im").and_then(Value::as_f64).ok_or("Global variable has no imaginary value.")?;
        Ok((name.clone(),C {re,im}))
    }).collect::<Result<_,String>>()?;
    let mut values=HashMap::from([
        ("frequency".into(),serde_json::json!(case.frequency_hz)),("frequencyGHz".into(),serde_json::json!(case.frequency_hz/1e9)),
        ("formulation".into(),serde_json::json!(case.formulation)),("discretization".into(),serde_json::json!(discretization)),
        ("geometry".into(),serde_json::json!(geometry)),("origin".into(),serde_json::json!(origin)),
        ("globalVariables".into(),Value::Object(global_variables)),("backend".into(),serde_json::json!(case.backend)),
        ("ordering".into(),serde_json::json!(if case.backend==ordering_backend {case.ordering.as_str()} else {"n/a"})),
        ("residual".into(),serde_json::json!(solution.residual)),("unknowns".into(),serde_json::json!(mapped.count)),
        ("dofsPerField".into(),serde_json::json!(mapped.count)),("realBlockUnknowns".into(),serde_json::json!(real_block_unknowns)),
        ("iterations".into(),serde_json::json!(mapped.count)),
    ]);
    if let Some(material_fields)=material_fields {
        values.insert("nodes".into(),serde_json::json!(mesh.nodes));
        values.insert("cells".into(),serde_json::json!(mesh.cells));
        values.insert("materialProperties".into(),Value::Object(material_properties));
        values.insert("materialFields".into(),Value::Object(material_fields));
    }
    let data = crate::result_contract::ResultData { values,complex,series:fields,matrices:HashMap::new() };
    // Validate the complete declared document while all numerical inputs are
    // still available, before an execution layer can create output files.
    let result_plan=plan.result.as_ref().ok_or("Edge result plan is required.")?;
    if case.field_files { result_plan.document(&data)?; }
    Ok(NumericalResult { node_count:mesh.nodes.len(),cell_count:mesh.cells.len(),
        residual:solution.residual,reduced_dofs:mapped.count,data })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;


    

    #[test]
    fn component_coordinate_contract_is_required_and_scenario_declared() {
        let policy=json!({"componentCoordinates":{"inputPath":"/frame/components","value":"model"}});
        assert!(validate_component_coordinates(&json!({"frame":{"components":"model"}}),&policy).is_ok());
        for configuration in [json!({}),json!({"frame":{"components":"internal"}}),json!({"frame":{"components":3}})] {
            assert!(validate_component_coordinates(&configuration,&policy).is_err());
        }
        assert!(validate_component_coordinates(&json!({"frame":{"components":"model"}}),&json!({})).is_err());
        let invalid=json!({"componentCoordinates":{"inputPath":"/frame/components","value":"internal"}});
        assert!(validate_component_coordinates(&json!({"frame":{"components":"internal"}}),&invalid).is_err());
    }

    

    

    
}
