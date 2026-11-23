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

enum Execution {
    Solved(prepared_vector::Result),
    Assembled(vector_p1::LinearSystem),
}

pub(crate) fn solve(case: &PreparedCase, package: &Value, model: &Value, map: &Value,
    mesh: Mesh<'_>, parameters: &HashMap<String, Scalar>, cache: &mut SolverCache) -> Result<prepared_vector::Result, String> {
    match execute(case,package,model,map,mesh,parameters,cache,false)? {
        Execution::Solved(result)=>Ok(result),
        Execution::Assembled(_)=>Err("Prepared vector model solve returned an assembly-only result.".into()),
    }
}

pub(crate) fn assemble(case: &PreparedCase, package: &Value, model: &Value, map: &Value,
    mesh: Mesh<'_>, parameters: &HashMap<String, Scalar>, cache: &mut SolverCache) -> Result<vector_p1::LinearSystem, String> {
    match execute(case,package,model,map,mesh,parameters,cache,true)? {
        Execution::Assembled(system)=>Ok(system),
        Execution::Solved(_)=>Err("Prepared vector model assembly unexpectedly solved the operator.".into()),
    }
}

fn execute(case: &PreparedCase, package: &Value, model: &Value, map: &Value,
    mesh: Mesh<'_>, parameters: &HashMap<String, Scalar>, cache: &mut SolverCache, assemble_only:bool) -> Result<Execution, String> {
    let frequency = match case.kind { CaseKind::Frequency { frequency_hz, .. } => frequency_hz, _ => return Err("Prepared vector execution requires a frequency case.".into()) };
    if package.get("id").and_then(Value::as_str) != Some(case.profile.as_str()) { return Err("Prepared vector case and package identities differ.".into()); }
    if case.settings.element_order != 1 { return Err("Prepared vector execution requires first-order elements.".into()); }
    let policy = package.get("preparedVector").ok_or("preparedVector policy is required.")?;
    validate_formulation_match(model,&case.settings.formulation,policy)?;
    let longitudinal_wavenumber = crate::longitudinal::prescribed_wavenumber(
        package, model, &case.study_id, &case.settings.formulation, parameters,
    )?;
    if let CaseKind::Frequency { case: prepared_case, .. } = &case.kind {
        crate::longitudinal::validate_prepared_case_value(prepared_case, longitudinal_wavenumber)?;
    }
    let propagation_operator = model.pointer("/model/study").and_then(Value::as_array)
        .and_then(|studies|studies.iter().find(|study|study.get("id").and_then(Value::as_str)==Some(&case.study_id)))
        .and_then(|study|study.get("type")).and_then(Value::as_str)==Some("propagationMode");
    let operator_formulation = if longitudinal_wavenumber != C::default() || propagation_operator {
        crate::longitudinal::operator_formulation(package, &case.settings.formulation)
    } else {
        case.settings.formulation.as_str()
    };
    let mut plan = ScenarioPlan::from_package(package, operator_formulation)?;
    plan.set_longitudinal_wavenumber(longitudinal_wavenumber)?;
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
            match plan.volume.shape().1 {
                2=>run::<2,2,3,2>(case,package,model,map,parameters,cache,frequency,longitudinal_wavenumber,operator_formulation,&plan,policy,&mesh.nodes,&nodes,&mesh.triangles,&mesh.boundary_edges,&topology,points,facets,assemble_only),
                3=>run::<2,3,3,2>(case,package,model,map,parameters,cache,frequency,longitudinal_wavenumber,operator_formulation,&plan,policy,&mesh.nodes,&nodes,&mesh.triangles,&mesh.boundary_edges,&topology,points,facets,assemble_only),
                _=>Err("Prepared vector field component count must be two or three on a triangle mesh.".into()),
            }
        },
        Mesh::Tetrahedron(mesh) => {
            let connectivity: Vec<_> = mesh.tetrahedra.iter().map(|(nodes,_)| *nodes).collect();
            let topology = simplex_topology::tetrahedra(&mesh.nodes, &connectivity, &[], topology_policy)?;
            let points = crate::quadrature::tetrahedron(&case.settings.quadrature)?.into_iter().map(|q| ([q[0],q[1],q[2],q[3]],q[4])).collect();
            if policy.get("boundaryQuadrature").and_then(Value::as_str) != Some("triangle3") { return Err("Unsupported vector face quadrature.".into()); }
            let facets = vec![([2./3.,1./6.,1./6.],1./3.),([1./6.,2./3.,1./6.],1./3.),([1./6.,1./6.,2./3.],1./3.)];
            if plan.volume.shape().1!=3{return Err("Tetrahedral vector fields require three components.".into());}
            run::<3,3,4,3>(case, package, model, map, parameters, cache, frequency, longitudinal_wavenumber, operator_formulation, &plan, policy, &mesh.nodes, &mesh.nodes, &mesh.tetrahedra, &mesh.boundary_faces, &topology, points, facets,assemble_only)
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

fn run<const D: usize, const FC: usize, const N: usize, const F: usize>(case: &PreparedCase, package: &Value, model: &Value, map: &Value,
    parameters: &HashMap<String, Scalar>, cache: &mut SolverCache, frequency: f64, longitudinal_wavenumber:C, operator_formulation:&str, plan: &ScenarioPlan, policy: &Value,
    physical_nodes: &[[f64;3]], nodes: &[[f64;D]], cells: &[([usize;N],usize)], facets: &[([usize;F],usize)],
    topology: &SimplexTopology<F>, quadrature: Vec<([f64;N],f64)>, facet_quadrature: Vec<([f64;F],f64)>, assemble_only:bool) -> Result<Execution,String> {
    let periodic_policy = policy.get("periodic").and_then(Value::as_str).ok_or("Vector periodic policy is required.")?;
    let physics = model.pointer("/model/physics").ok_or("Model physics is required.")?;
    let periodic_enabled = crate::model_schema::physics_periodic_conditions(physics).any(|value| value.get("enabled").and_then(Value::as_bool) != Some(false));
    if periodic_policy == "reject" && periodic_enabled { return Err("Periodic vector conditions are unsupported by this package.".into()); }
    if !matches!(periodic_policy,"reject"|"matchedNodes") { return Err("Unsupported vector periodic policy.".into()); }
    let selection_plan = plan.selections.as_ref().ok_or("Vector selection plan is required.")?;
    let source_by_cell = selection_plan.uses_cell_map("domainSources", map);
    let selected = selection_plan.bind(model, map, &HashMap::from([
        ("domainSources".into(),cells.iter().map(|(_,tag)| *tag).collect()), ("materials".into(),cells.iter().map(|(_,tag)| *tag).collect()), ("boundaries".into(),facets.iter().map(|(_,tag)| *tag).collect())]))?;
    let source_data = crate::domain_source::prepare_routes(&plan.domain_sources, selected.get("domainSources").map(Vec::as_slice))?;
    let mut source_samples = source_data.iter().map(|_| Vec::with_capacity(cells.len())).collect::<Vec<Vec<Vec<Vec<C>>>>>();
    let material_rows = selected.get("materials").ok_or("Vector materials selection is required.")?.iter().map(|row| (row.tag,row.row.clone())).collect();
    let compiled = crate::material::CompiledMaterialAssignments::compile(&case.profile,&material_rows)?;
    let constant = compiled.position_independent_values(parameters,frequency)?;
    let mut context = parameters.clone(); context.insert("omega".into(), Scalar { value:std::f64::consts::TAU*frequency, dimension:[0.;5] });
    let mut prepared_cells = Vec::new();
    for (index,(cell,tag)) in cells.iter().enumerate() {
        let mut points = Vec::new();
        let mut loads = source_data.iter().map(|_| Vec::with_capacity(quadrature.len())).collect::<Vec<Vec<Vec<C>>>>();
        for (basis,weight) in &quadrature {
            let position = std::array::from_fn(|axis| (0..N).map(|i| basis[i]*physical_nodes[cell[i]][axis]).sum());
            let raw = if let Some(value) = constant.get(tag) { value.clone() } else { compiled.evaluate(*tag,parameters,frequency,position,&format!("cell {index}"))? };
            let symbols = prepared_vector::bind_material(plan,&raw,&context,"material")?;
            for (route_index, (_, load_plan, sources)) in source_data.iter().enumerate() {
                let source_key = if source_by_cell { index } else { *tag };
                loads[route_index].push(crate::domain_source::sample_load(load_plan, sources, source_key, position, parameters, Some(frequency), &symbols)?);
            }
            points.push(Point { basis:*basis, weight:*weight, symbols });
        }
        for (all, loads) in source_samples.iter_mut().zip(loads) { all.push(loads); }
        let centroid = std::array::from_fn(|axis| cell.iter().map(|&node| physical_nodes[node][axis]).sum::<f64>()/N as f64);
        let raw = if let Some(value) = constant.get(tag) { value.clone() } else { compiled.evaluate(*tag,parameters,frequency,centroid,&format!("cell {index} centroid"))? };
        prepared_cells.push(Cell { nodes:*cell, points, recovery_symbols:prepared_vector::bind_material(plan,&raw,&context,"material")? });
    }
    let selected_boundaries = selected.get("boundaries").ok_or("Vector boundaries selection is required.")?;
    let inputs = crate::declared_boundary::compile_rows(package,operator_formulation,selected_boundaries.iter().map(|row| (row.tag,row.row)))?;
    let tags = selected_boundaries.iter().map(|row| row.tag).collect::<Vec<_>>();
    let chosen = facets.iter().copied().filter(|(_,tag)| tags.contains(tag)).collect::<Vec<_>>();
    topology.bind_selected_facets(&chosen,&tags,SelectedFacetPolicy { ownership:SelectedFacetOwnership::ExteriorOnly, duplicates:SelectedFacetDuplicates::KeepAll, require_positive_tag:true })?;
    let mut boundaries = Vec::new(); let mut essential = Vec::new();
    let field_policy=policy.get("formulations").and_then(|value|value.get(operator_formulation)).unwrap_or(policy);
    let essential_name = field_policy.get("essentialForm").and_then(Value::as_str).or_else(||policy.get("essentialForm").and_then(Value::as_str)).ok_or("Vector essential form name is required.")?;
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
                let values: [C;FC] = values.try_into().map_err(|_| "Vector prescribed value has wrong component count.")?;
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
    let mapping = OutputMapping { vector_components:field_policy.get("vectorComponents").and_then(Value::as_array).or_else(||policy.get("vectorComponents").and_then(Value::as_array)).ok_or("Vector output components are required.")?.iter()
        .map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_owned).ok_or("Vector output component must be a name.".to_string())).collect::<Result<_,_>>()?,
        tensor_matrix:field_policy.get("tensorMatrix").and_then(Value::as_str).or_else(||policy.get("tensorMatrix").and_then(Value::as_str)).ok_or("Vector tensor output name is required.")?.to_owned() };
    let maximum_component = field_policy.get("maximumComponent").and_then(Value::as_u64).or_else(||policy.get("maximumComponent").and_then(Value::as_u64)).filter(|&axis| axis < FC as u64).ok_or("Vector maximum component is invalid.")? as usize;
    let material_fields = crate::material::fields_by_cell(model,map,frequency,physical_nodes,cells,[0,1,2],[0.;3])?;
    let count = periodic.as_ref().map_or(nodes.len(),|mapping| mapping.dof_count)*FC;
    let data = crate::result_contract::ResultData { values:HashMap::from([
        ("frequency".into(),json!(frequency)),("formulation".into(),json!(case.settings.formulation)),("nodes".into(),json!(physical_nodes)),
        ("cells".into(),json!(cells.iter().map(|(nodes,tag)| json!({"nodes":nodes.as_slice(),"materialTag":tag})).collect::<Vec<_>>())),
        ("materialFields".into(),Value::Object(material_fields)),("backend".into(),json!(case.settings.backend)),("ordering".into(),json!(case.settings.ordering)),
        ("threads".into(),json!(case.settings.threads)),("unknowns".into(),json!(count)),("cellCount".into(),json!(cells.len())),("residual".into(),json!(0.)),("maximum".into(),json!(0.)),
        ("longitudinalWavenumberReal".into(),json!(longitudinal_wavenumber.re)),
        ("longitudinalWavenumberImaginary".into(),json!(longitudinal_wavenumber.im)),
        ("longitudinalDependence".into(),json!(if longitudinal_wavenumber==C::default(){Value::Null}else{json!({"factor":"exp(-i*kz*z)","unit":"rad/m","operatorFormulation":operator_formulation})})),
    ]),complex:HashMap::new(),series:HashMap::new(),matrices:HashMap::new() };
    let periodic = periodic.as_ref().map(|p| vector_p1::PeriodicMap { representative:&p.node_to_dof, weights:&p.node_weights, count:p.dof_count });
    let load_routes = source_data.iter().zip(&source_samples)
        .map(|((_, load_plan, _), samples)| (&load_plan.form, samples.as_slice()))
        .collect::<Vec<_>>();
    if assemble_only {
        return Ok(Execution::Assembled(prepared_vector::assemble::<D,FC,N>(plan,nodes,&prepared_cells,&boundaries,&essential,periodic,essential_name,&mapping,data,
            &case.settings.backend,&case.settings.ordering,case.settings.threads,1e-7,cache,&load_routes)?));
    }
    let mut result = prepared_vector::solve::<D,FC,N>(plan,nodes,&prepared_cells,&boundaries,&essential,periodic,essential_name,&mapping,data,
        &case.settings.backend,&case.settings.ordering,case.settings.threads,1e-7,cache,&load_routes)?;
    let maximum = result.solution.real.iter().zip(&result.solution.imaginary).skip(maximum_component).step_by(FC)
        .map(|(re,im)| re.hypot(*im)).fold(0.0, f64::max);
    result.data.values.insert("maximum".into(),json!(maximum)); result.data.values.insert("residual".into(),json!(result.solution.residual));
    Ok(Execution::Solved(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepared_case::{Output, SolverSettings};
    use serde_json::json;

    #[test]
    fn finite_kz_plane_strain_axial_plane_wave_solves_and_recovers_three_dimensional_stress() {
        let package: Value = serde_json::from_str(include_str!("scenarios/elastic2d.json")).unwrap();
        let kz = 0.5_f64;
        let density = 1.0_f64;
        let young = 12.0_f64;
        let poisson = 0.25_f64;
        let mu = young / (2.0 * (1.0 + poisson));
        let lambda = young * poisson / ((1.0 + poisson) * (1.0 - 2.0 * poisson));
        let omega = kz * ((lambda + 2.0 * mu) / density).sqrt();
        let frequency = omega / std::f64::consts::TAU;
        let quantity = |expression: &str, unit: &str| json!({"expression":expression,"unit":unit});
        let model = json!({"scenario":{"profileId":"elastic-2d"},"model":{
            "parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],
            "mesh":{"periodicPairIds":[]},
            "study":[{"id":"harmonic","type":"frequencyDomain","physicsId":"wave",
                "longitudinalWavenumber":{"real":quantity("0.5","rad/m"),"imaginary":quantity("0","rad/m")}}],
            "materials":[{"id":"solid","selectionId":"domain","density":quantity("1","kg/m^3"),
                "youngModulus":quantity("12","Pa"),"poissonRatio":"0.25","densityLossFactor":"0"}],
            "physics":{"id":"wave","type":"elasticity","formulation":"planeStrain","dirichlet":{
                "id":"dirichlet","type":"dirichlet","selectionId":"outer","value":quantity("0","m"),
                "fieldValues":[quantity("0","m"),quantity("0","m"),quantity("1","m")]}}
        }});
        let map = json!({"schema":"fullmeta-mesh-map/6","dimension":2,
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],
            "boundarySelections":[{"selectionId":"outer","physicalTag":2}],"domainCellSelections":{}});
        let mesh = crate::mesh2d::Mesh2D {
            nodes:vec![[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]],
            triangles:vec![([0,1,2],1)],
            boundary_edges:vec![([0,1],2),([1,2],2),([2,0],2)],integration_edges:vec![],
        };
        let case = PreparedCase {request:json!({"studyId":"harmonic"}),profile:"elastic-2d".into(),study_id:"harmonic".into(),
            kind:CaseKind::Frequency {frequency_hz:frequency,case:json!({"longitudinalWavenumber":{"real":kz,"imaginary":0.,"unit":"rad/m"}})},
            settings:SolverSettings {formulation:"planeStrain".into(),element_order:1,quadrature:"standard".into(),
                backend:"faer".into(),ordering:"amd".into(),threads:1},output:Output::Frequency {sweep_csv:"result.csv".into()},
            model_path:Default::default(),mesh_path:Default::default(),map_path:Default::default(),output_directory:Default::default()};
        let parameters = crate::model_expression::resolve_parameters(&[],&model).unwrap();
        let mut declared_plan=ScenarioPlan::from_package(&package,"planeStrainLongitudinal").unwrap();
        declared_plan.set_longitudinal_wavenumber(C{re:kz,im:0.}).unwrap();
        assert_eq!(declared_plan.tensor_recovery.as_ref().unwrap().longitudinal_derivative(C{re:1.,im:0.}),C{re:0.,im:-kz});
        let direct=declared_plan.tensor_recovery.as_ref().unwrap().apply(&[C::default();8].into_iter().chain([C{re:0.,im:-kz}]).collect::<Vec<_>>(),&HashMap::from([("lambda".into(),C{re:lambda,im:0.}),("mu".into(),C{re:mu,im:0.})])).unwrap();
        assert!((direct[2].im+kz*(lambda+2.*mu)).abs()<1e-8,"direct recovery = {direct:?}");
        let result = solve(&case,&package,&model,&map,Mesh::Triangle(&mesh),&parameters,&mut crate::SolverCache::default()).unwrap();
        assert!(result.solution.residual < 1e-7);
        assert_eq!(result.solution.real.len(),mesh.nodes.len()*3);
        for node in 0..mesh.nodes.len() {
            assert!(result.solution.real[node*3].abs()<1e-8);
            assert!(result.solution.real[node*3+1].abs()<1e-8);
            assert!((result.solution.real[node*3+2]-1.).abs()<1e-8,"node {node} uz = {}",result.solution.real[node*3+2]);
            assert!(result.solution.imaginary[node*3].abs()<1e-8);
            assert!(result.solution.imaginary[node*3+1].abs()<1e-8);
            assert!(result.solution.imaginary[node*3+2].abs()<1e-8);
        }
        let sigma_zz = result.solution.recovered_cells[0][2];
        assert!(sigma_zz.re.abs()<1e-7);
        assert!((sigma_zz.im + kz*(lambda+2.0*mu)).abs()<1e-7,"sigma_zz = {sigma_zz:?}");
        for component in [0,1] {
            let stress=result.solution.recovered_cells[0][component];
            assert!(stress.re.abs()<1e-7);
            assert!((stress.im+kz*lambda).abs()<1e-7,"normal stress {component} = {stress:?}");
        }
        for component in [3,4,5] {
            assert!(result.solution.recovered_cells[0][component].re.hypot(result.solution.recovered_cells[0][component].im)<1e-7);
        }
        assert_eq!(result.data.values["longitudinalDependence"]["factor"],json!("exp(-i*kz*z)"));
    }
}


