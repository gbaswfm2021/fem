//! Scenario-neutral in-memory runner for stationary scalar simplex P1 cases.
//!
//! Inputs are already loaded and preflighted. Production dispatch, file output,
//! periodic matching and unsupported element families are deliberately outside
//! this initial runner. All matrix rows remain sparse in `nodal_p1`.
use std::collections::{BTreeMap, HashMap, HashSet};
use serde_json::{Value, json};
use crate::boundary_input::BoundaryInput;
use crate::model_expression::{self, Scalar};
use crate::nodal_p1::{self, SimplexBoundaryPoint, SimplexCell, SimplexVolumePoint};
use crate::prepared_case::{CaseKind, PreparedCase};
use crate::scenario_plan::ScenarioPlan;
use crate::simplex_topology::{self, Facet, SelectedFacetDuplicates, SelectedFacetOwnership, SelectedFacetPolicy, SimplexTopology, TopologyPolicy};
use crate::{C, SolverCache};

pub(crate) enum Mesh<'a> {
    Triangle { nodes: &'a [[f64; 3]], cells: &'a [([usize; 3], usize)], facets: &'a [([usize; 2], usize)] },
    Tetrahedron { nodes: &'a [[f64; 3]], cells: &'a [([usize; 4], usize)], facets: &'a [([usize; 3], usize)] },
}

struct RuntimePolicy {
    topology: TopologyPolicy,
    maximum_out_of_plane: Option<f64>,
    periodic_mode: String,
    phase_mode: String,
    phase_tolerance: f64,
    essential_mapping: nodal_p1::EssentialMapping,
    require_model_map_agreement: bool,
    reject_mesh_pairs: bool,
    result_material_symbol: String,
    all_facet_ownership: SelectedFacetOwnership,
    imaginary_tolerance: f64,
    recovery_imaginary_tolerance: f64,
    recovery_centered: bool,
    validate_auxiliary: bool,
    residual_tolerance: f64,
}

impl RuntimePolicy {
    fn from_package(package: &Value) -> Result<Self, String> {
        let input = package.get("preparedScalar").ok_or("Scenario preparedScalar policy is required.")?;
        let recovery_centered = input.get("recoveryCentered").and_then(Value::as_bool).ok_or("preparedScalar.recoveryCentered must be boolean.")?;
        let validate_auxiliary = input.get("validateAuxiliary").and_then(Value::as_bool).ok_or("preparedScalar.validateAuxiliary must be boolean.")?;
        let residual_tolerance = input.get("residualTolerance").and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 0.).ok_or("preparedScalar.residualTolerance must be positive and finite.")?;
        if input.get("solutionNumber").and_then(Value::as_str) != Some("real") { return Err("This stationary result pipeline currently requires real solutions.".into()); }
        let imaginary_tolerance = input.get("imaginaryTolerance").and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.).ok_or("preparedScalar.imaginaryTolerance must be finite and nonnegative.")?;
        let recovery_imaginary_tolerance = input.get("recoveryImaginaryTolerance").and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.).ok_or("preparedScalar.recoveryImaginaryTolerance must be finite and nonnegative.")?;
        let result_material_symbol = input.get("resultMaterialSymbol").and_then(Value::as_str).filter(|v| !v.is_empty()).ok_or("preparedScalar.resultMaterialSymbol is required.")?.to_owned();
        let all_facet_ownership = match input.get("allFacetOwnership").and_then(Value::as_str) {
            Some("anyExisting") => SelectedFacetOwnership::AnyExisting,
            Some("exteriorOnly") => SelectedFacetOwnership::ExteriorOnly,
            _ => return Err("Unsupported allFacetOwnership policy.".into()),
        };
        if input.get("materialScope").and_then(Value::as_str) != Some("parameters") {
            return Err("This stationary scalar runner currently supports materialScope 'parameters' only.".into());
        }
        let topology = input.get("topology").and_then(Value::as_object).ok_or("preparedScalar.topology is required.")?;
        let flag = |name: &str| topology.get(name).and_then(Value::as_bool).ok_or_else(|| format!("preparedScalar.topology.{name} must be boolean."));
        let tolerance = topology.get("determinantTolerance").ok_or("preparedScalar.topology.determinantTolerance is required.")?;
        let number = |object: &Value, name: &str| object.get(name).and_then(Value::as_f64).filter(|v| v.is_finite() && *v >= 0.).ok_or_else(|| format!("{name} must be finite and nonnegative."));
        let equal = tolerance.get("rejectEqual").and_then(Value::as_bool).ok_or("determinantTolerance.rejectEqual must be boolean.")?;
        let determinant_tolerance = match tolerance.get("kind").and_then(Value::as_str) {
            Some("absolute") => simplex_topology::DeterminantTolerance::Absolute { minimum:number(tolerance,"minimum")?, reject_equal:equal },
            Some("relative") => simplex_topology::DeterminantTolerance::Relative { fraction:number(tolerance,"fraction")?, reject_equal:equal },
            _ => return Err("Unsupported determinant tolerance policy.".into()),
        };
        let maximum_out_of_plane = topology.get("maximumOutOfPlane").map(|v| v.as_f64().filter(|x| x.is_finite() && *x >= 0.).ok_or("maximumOutOfPlane must be finite and nonnegative.")).transpose()?;
        let periodic = input.get("periodic").ok_or("preparedScalar.periodic is required.")?;
        let periodic_mode = periodic.get("mode").and_then(Value::as_str).filter(|v| matches!(*v,"reject" | "matchedNodes")).ok_or("Unsupported periodic mode.")?.to_owned();
        let phase_mode = periodic.get("phase").and_then(Value::as_str).filter(|v| matches!(*v,"zero" | "complex")).ok_or("Unsupported periodic phase policy.")?.to_owned();
        let phase_tolerance = number(periodic,"phaseTolerance")?;
        let essential_mapping = match periodic.get("essentialMapping").and_then(Value::as_str) {
            Some("directRepresentative") => nodal_p1::EssentialMapping::DirectRepresentative,
            Some("weightedPhysical") => nodal_p1::EssentialMapping::WeightedPhysical,
            _ => return Err("Unsupported periodic essential mapping.".into()),
        };
        let require_model_map_agreement = periodic.get("requireModelMapAgreement").and_then(Value::as_bool).ok_or("requireModelMapAgreement must be boolean.")?;
        let reject_mesh_pairs = periodic.get("rejectMeshPairs").and_then(Value::as_bool).ok_or("rejectMeshPairs must be boolean.")?;
        Ok(Self { topology:TopologyPolicy { reject_duplicate_cells:flag("rejectDuplicateCells")?, reject_non_manifold_facets:flag("rejectNonManifoldFacets")?,
            require_all_nodes_used:flag("requireAllNodesUsed")?, require_nonempty_cells:flag("requireNonemptyCells")?, determinant_tolerance:Some(determinant_tolerance) },
            maximum_out_of_plane, periodic_mode, phase_mode, phase_tolerance, essential_mapping, require_model_map_agreement, reject_mesh_pairs,
            result_material_symbol, all_facet_ownership, imaginary_tolerance, recovery_imaginary_tolerance,
            recovery_centered, validate_auxiliary, residual_tolerance })
    }
}

/// Return field values and the shared solver's auxiliary/gauge metadata. The
/// caller owns any field recovery and result publication.
pub(crate) fn solve(case: &PreparedCase, package: &Value, model: &Value, map: &Value,
    mesh: Mesh<'_>, parameters: &HashMap<String, Scalar>, cache: &mut SolverCache) -> Result<nodal_p1::Solution, String> {
    Ok(solve_with_result(case, package, model, map, mesh, parameters, cache)?.solution)
}

pub(crate) struct ScalarResult {
    pub(crate) solution: nodal_p1::Solution,
    pub(crate) data: crate::result_contract::ResultData,
}

pub(crate) fn solve_with_result(case: &PreparedCase, package: &Value, model: &Value, map: &Value,
    mesh: Mesh<'_>, parameters: &HashMap<String, Scalar>, cache: &mut SolverCache) -> Result<ScalarResult, String> {
    if !matches!(case.kind, CaseKind::Stationary) { return Err("This scalar runner requires a stationary prepared case.".into()); }
    if package.get("id").and_then(Value::as_str) != Some(case.profile.as_str()) { return Err("Prepared case and scenario package identities differ.".into()); }
    if case.settings.element_order != 1 { return Err("This scalar runner supports first-order elements only.".into()); }
    let plan = ScenarioPlan::from_package(package, &case.settings.formulation)?;
    let runtime_policy = RuntimePolicy::from_package(package)?;
    if plan.volume.shape().1 != 1 || !plan.coefficient_matrices.is_empty() {
        return Err("This scalar runner requires scalar coefficients and one field component.".into());
    }
    model.pointer("/model/physics").filter(|v| v.is_object()).ok_or("model.physics must be an object.")?;
    crate::solver_request::validate_study_reference(model, &case.request)?;
    match mesh {
        Mesh::Triangle { nodes, cells, facets } => {
            plan.volume.require_discretization(crate::discretization::Cell::Triangle, crate::discretization::Family::Lagrange, 1, 1)?;
            if runtime_policy.maximum_out_of_plane.is_some_and(|max| nodes.iter().any(|node| node[2].abs() > max)) { return Err("Two-dimensional simplex nodes exceed the declared out-of-plane tolerance.".into()); }
            let connectivity = cells.iter().map(|(nodes,_)| *nodes).collect::<Vec<_>>();
            let policy = runtime_policy.topology;
            let topology = simplex_topology::triangles(nodes, &connectivity, &[], policy)?;
            let quadrature = crate::quadrature::triangle(&case.settings.quadrature)?.into_iter().map(|q| ([q[0],q[1],q[2]],q[3])).collect();
            solve_simplex::<3,2,2>(case, package, model, map, parameters, cache, &plan, nodes, cells, facets,
                &topology, &runtime_policy, quadrature, &|facet| simplex_topology::edge_normal(nodes, facet, 0))
        }
        Mesh::Tetrahedron { nodes, cells, facets } => {
            plan.volume.require_discretization(crate::discretization::Cell::Tetrahedron, crate::discretization::Family::Lagrange, 1, 1)?;
            let connectivity = cells.iter().map(|(nodes,_)| *nodes).collect::<Vec<_>>();
            let policy = runtime_policy.topology;
            let topology = simplex_topology::tetrahedra(nodes, &connectivity, &[], policy)?;
            let quadrature = crate::quadrature::tetrahedron(&case.settings.quadrature)?.into_iter().map(|q| ([q[0],q[1],q[2],q[3]],q[4])).collect();
            solve_simplex::<4,3,3>(case, package, model, map, parameters, cache, &plan, nodes, cells, facets,
                &topology, &runtime_policy, quadrature, &|facet| simplex_topology::face_normal(nodes, facet, 0))
        }
    }
}

fn check_stationary_material_scope(package: &Value, materials: &HashMap<usize, Value>) -> Result<(), String> {
    let definitions = package.pointer("/schemaDocument/x-fem-scenario/materials/properties")
        .and_then(Value::as_array).ok_or("Scenario material catalog is required.")?;
    for row in materials.values() {
        for definition in definitions {
            if definition.get("studyTypes").and_then(Value::as_array).is_some_and(|types|!types.iter().any(|kind|kind.as_str()==Some("stationary"))) {continue;}
            let key = definition.get("key").and_then(Value::as_str).ok_or("Material property key is required.")?;
            let Some(raw) = row.get(key).or_else(|| definition.get("default")) else { continue; };
            let expression = if definition.get("representation").and_then(Value::as_str) == Some("quantity") {
                raw.get("expression").and_then(Value::as_str)
            } else { raw.as_str() }.ok_or_else(|| format!("Material property {key:?} requires an expression."))?;
            let (_, names, _) = model_expression::inspect(expression)?;
            if names.iter().any(|name| matches!(name.as_str(), "x" | "y" | "z" | "t" | "freq" | "frequency" | "omega")) {
                return Err(format!("Stationary material {key:?} must use parameter-only expressions."));
            }
        }
    }
    Ok(())
}

fn quantity(value: &Value, unit: &str, parameters: &HashMap<String, Scalar>, label: &str) -> Result<f64, String> {
    let actual = value.get("unit").and_then(Value::as_str).ok_or_else(|| format!("{label} requires a unit."))?;
    if model_expression::unit(actual)?.1 != model_expression::unit(unit)?.1 { return Err(format!("{label} must use units compatible with {unit}.")); }
    model_expression::CompiledQuantity::compile(value,label)?.evaluate(parameters,label)
}

fn validate_real_samples(real: &[f64], imaginary: &[f64], tolerance: f64, label: &str) -> Result<(), String> {
    if real.len() != imaginary.len() || real.iter().zip(imaginary).any(|(&real,&imaginary)| {
        !real.is_finite() || !imaginary.is_finite() || imaginary.abs() > tolerance * real.abs().max(1.)
    }) {
        return Err(format!("{label} values are non-finite or violate their declared real-value tolerance."));
    }
    Ok(())
}

fn prepare_periodic<const F: usize>(policy: &RuntimePolicy, model: &Value, map: &Value,
    nodes: &[[f64; 3]], facets: &[([usize; F], usize)], parameters: &HashMap<String, Scalar>, dimension: usize)
    -> Result<Option<crate::scalar2d::PeriodicDofMap>, String> {
    let physics = model.pointer("/model/physics").ok_or("model.physics is required.")?;
    let active = crate::model_schema::physics_periodic_conditions(physics)
        .filter(|v| v.get("enabled").and_then(Value::as_bool) != Some(false)).collect::<Vec<_>>();
    if policy.reject_mesh_pairs && model.pointer("/model/mesh/periodicPairs").and_then(Value::as_array).is_some_and(|pairs| !pairs.is_empty()) {
        return Err("The declared periodic policy rejects mesh periodic pairs.".into());
    }
    if active.is_empty() { return Ok(None); }
    if policy.periodic_mode == "reject" { return Err("The declared periodic policy rejects active periodic conditions.".into()); }
    let map_pairs = map.get("periodicPairs").and_then(Value::as_array).ok_or("Periodic mesh-map pairs are required.")?;
    let model_pairs = model.pointer("/model/mesh/periodicPairs").and_then(Value::as_array);
    let tolerance = map.get("toleranceM").and_then(Value::as_f64).filter(|v| v.is_finite() && *v > 0.).unwrap_or(1e-8).max(1e-10);
    let mut resolved_model = model.clone();
    resolved_model["model"]["parameters"] = json!([]);
    let mut resolved_map = map.clone();
    let mut active_ids = HashSet::new();
    let mut phase_by_id = HashMap::<String, Vec<Value>>::new();
    for feature in active {
        let feature_id = feature.get("id").and_then(Value::as_str).ok_or("Periodic condition ID is required.")?;
        let pair_id = feature.get("periodicPairId").and_then(Value::as_str).ok_or("Periodic pair ID is required.")?;
        if !active_ids.insert(pair_id.to_owned()) { return Err(format!("Periodic pair {pair_id:?} is referenced more than once.")); }
        let matches = map_pairs.iter().filter(|pair| pair.get("id").and_then(Value::as_str) == Some(pair_id)).collect::<Vec<_>>();
        if matches.len() != 1 { return Err(format!("Periodic pair {pair_id:?} must occur exactly once in the mesh map.")); }
        let mapped = matches[0];
        let translations = mapped.get("translation").and_then(Value::as_array).filter(|v| v.len() == 3).ok_or("Periodic map translation requires three quantities.")?;
        if policy.require_model_map_agreement {
            let pair = model_pairs.and_then(|pairs| pairs.iter().find(|p| p.get("id").and_then(Value::as_str) == Some(pair_id))).ok_or_else(|| format!("Periodic pair {pair_id:?} is missing from the model."))?;
            for side in ["lowerSelectionId","upperSelectionId"] {
                if pair.get(side) != mapped.get(side) { return Err(format!("Periodic pair {pair_id:?} {side} disagrees with the mesh map.")); }
            }
            let model_translation = pair.get("translation").and_then(Value::as_array).filter(|v| v.len()==3).ok_or("Periodic model translation requires three quantities.")?;
            for axis in 0..3 {
                let expected = quantity(&model_translation[axis],"m",parameters,"model translation")?;
                let actual = quantity(&translations[axis],"m",parameters,"mesh-map translation")?;
                if (expected-actual).abs() > tolerance { return Err(format!("Periodic pair {pair_id:?} translation disagrees with the mesh map.")); }
            }
        }
        if let Some(phase) = feature.get("phaseVector") {
            let phase = phase.as_array().filter(|v| v.len()==3).ok_or("Periodic phase requires three quantities.")?;
            let mut resolved = Vec::with_capacity(3);
            for value in phase {
                let value = quantity(value,"rad/m",parameters,"periodic phase")?;
                if policy.phase_mode == "zero" && value.abs() > policy.phase_tolerance { return Err("Declared periodic policy requires zero phase.".into()); }
                resolved.push(json!({"expression":value.to_string(),"unit":"rad/m"}));
            }
            phase_by_id.insert(feature_id.into(), resolved);
        }
    }
    if let Some(physics) = resolved_model.pointer_mut("/model/physics") {
        for feature in crate::model_schema::physics_conditions_mut(physics) {
            let Some(id) = feature.get("id").and_then(Value::as_str) else { continue; };
            if let Some(phase) = phase_by_id.get(id) { feature["phaseVector"] = json!(phase); }
        }
    }
    let resolved_pairs = resolved_map.get_mut("periodicPairs").and_then(Value::as_array_mut).ok_or("Periodic mesh-map pairs are required.")?;
    for pair in resolved_pairs {
        let Some(id) = pair.get("id").and_then(Value::as_str) else { continue; };
        if !active_ids.contains(id) { continue; }
        let values = pair.get_mut("translation").and_then(Value::as_array_mut).filter(|v| v.len()==3).ok_or("Periodic map translation requires three quantities.")?;
        for value in values { let resolved = quantity(value,"m",parameters,"mesh-map translation")?; *value = json!({"expression":resolved.to_string(),"unit":"m"}); }
    }
    let entities = facets.iter().map(|(nodes,tag)| (nodes.to_vec(),*tag)).collect::<Vec<_>>();
    let result = crate::scalar2d::periodic_dof_map_for_entities(&resolved_model, &resolved_map, nodes, &entities, dimension)?;
    if policy.phase_mode == "zero" && result.node_weights.iter().any(|weight| (weight.re-1.).abs() > policy.phase_tolerance || weight.im.abs() > policy.phase_tolerance) {
        return Err("Declared periodic policy requires zero phase weights.".into());
    }
    Ok(Some(result))
}

fn solve_simplex<const N: usize, const D: usize, const F: usize>(case: &PreparedCase, package: &Value,
    model: &Value, map: &Value, parameters: &HashMap<String, Scalar>, cache: &mut SolverCache,
    plan: &ScenarioPlan, nodes: &[[f64; 3]], cells: &[([usize; N], usize)], facets: &[([usize; F], usize)],
    topology: &SimplexTopology<F>, policy: &RuntimePolicy, quadrature: Vec<([f64; N],f64)>,
    normal: &dyn Fn(&Facet<F>) -> Result<([f64; 3],f64), String>) -> Result<ScalarResult, String> {
    if map.get("schema").and_then(Value::as_str) != Some("fullmeta-mesh-map/6")
        || map.get("dimension").and_then(Value::as_u64) != Some(D as u64) {
        return Err("Mesh map schema or dimension does not match the simplex element.".into());
    }
    if facets.iter().any(|(vertices,_)| vertices.iter().any(|&node| node >= nodes.len())) {
        return Err("Selected facet references a missing mesh node.".into());
    }
    let periodic = prepare_periodic(policy, model, map, nodes, facets, parameters, D)?;
    let mesh_tags = HashMap::from([("domainSources".into(), cells.iter().map(|(_,tag)| *tag).collect()), ("materials".into(), cells.iter().map(|(_,tag)| *tag).collect()),
        ("boundaries".into(), facets.iter().map(|(_,tag)| *tag).collect())]);
    let selection_plan = plan.selections.as_ref().ok_or("Scenario selectionPlan is required.")?;
    let source_by_cell = selection_plan.uses_cell_map("domainSources", map);
    let bound = selection_plan.bind(model, map, &mesh_tags)?;
    let source_data = crate::domain_source::prepare_routes(&plan.domain_sources, bound.get("domainSources").map(Vec::as_slice))?;
    let mut source_samples = source_data.iter().map(|_| Vec::with_capacity(cells.len())).collect::<Vec<Vec<Vec<Vec<C>>>>>();
    let materials = bound.get("materials").ok_or("Selection plan must define materials.")?.iter()
        .map(|row| (row.tag, row.row.clone())).collect::<HashMap<_,_>>();
    check_stationary_material_scope(package, &materials)?;
    let compiled = crate::material::CompiledMaterialAssignments::compile_for_study(&case.profile, &materials,"stationary")?;
    let raw_values = compiled.position_independent_values(parameters, 0.)?;
    let bindings = plan.material_bindings.as_ref().ok_or("Scenario materialBindings are required.")?;
    if bindings.uses_position() { return Err("Position-dependent material bindings are not supported for stationary scalar cases.".into()); }
    let mut material_values = HashMap::new();
    for (&tag, values) in &raw_values {
        material_values.insert(tag, bindings.evaluate(values, parameters, &format!("material tag {tag}"))?);
    }
    let first = cells.first().and_then(|(_,tag)| material_values.get(tag)).ok_or("Missing material values for first cell.")?;
    let mut names = first.keys().cloned().collect::<Vec<_>>(); names.sort();
    let name_refs = names.iter().map(String::as_str).collect::<Vec<_>>();
    let mut generic_cells = Vec::with_capacity(cells.len());
    for (cell_index, (cell, tag)) in cells.iter().enumerate() {
        if cell.iter().any(|&node| node >= nodes.len()) { return Err("Domain cell references a missing mesh node.".into()); }
        let values = material_values.get(tag).ok_or_else(|| format!("No material values for cell tag {tag}."))?;
        if values.len() != names.len() || names.iter().any(|name| !values.contains_key(name)) { return Err("Material bindings yield inconsistent symbols across cells.".into()); }
        let symbols = names.iter().map(|name| C { re:values[name].value, im:0. }).collect::<Vec<_>>();
        generic_cells.push(SimplexCell { nodes:*cell, quadrature:quadrature.iter().map(|(basis,weight)| SimplexVolumePoint { basis:*basis, weight:*weight, symbols:symbols.clone() }).collect() });
        for (route_index, (_, load_plan, sources)) in source_data.iter().enumerate() {
            let coefficients = values.iter().map(|(name, value)| (name.clone(), C { re:value.value, im:0. })).collect();
            let mut sampled = Vec::with_capacity(quadrature.len());
            for (basis, _) in &quadrature {
                let point = std::array::from_fn(|axis| (0..N).map(|i| basis[i] * nodes[cell[i]][axis]).sum());
                let source_key = if source_by_cell { cell_index } else { *tag };
                sampled.push(crate::domain_source::sample_load(load_plan, sources, source_key, point, parameters, None, &coefficients)?);
            }
            source_samples[route_index].push(sampled);
        }
    }
    let selected = bound.get("boundaries").ok_or("Selection plan must define boundaries.")?;
    let required_tags = selected.iter().map(|row| row.tag).collect::<Vec<_>>();
    if let Some(periodic) = &periodic {
        let periodic_facets = facets.iter().copied().filter(|(nodes,tag)| {
            let mut edge = [0usize;2];
            if F == 2 { edge.copy_from_slice(&nodes[..2]); edge.sort_unstable(); }
            periodic.boundary_tags.contains(tag) || F == 2 && periodic.boundary_edges.contains(&edge)
        }).collect::<Vec<_>>();
        for (_,tag) in &periodic_facets {
            if required_tags.contains(tag) { return Err(format!("Periodic boundary tag {tag} also carries a prescribed boundary condition.")); }
        }
        topology.bind_selected_facets(&periodic_facets, &[], SelectedFacetPolicy { ownership:SelectedFacetOwnership::ExteriorOnly, duplicates:SelectedFacetDuplicates::Reject, require_positive_tag:false })?;
    }
    // Check all selected records for duplicate geometry and an owner; then
    // separately require exterior ownership for each assigned boundary.
    topology.bind_selected_facets(facets, &required_tags, SelectedFacetPolicy { ownership:policy.all_facet_ownership, duplicates:SelectedFacetDuplicates::Reject, require_positive_tag:false })?;
    let selected_records = facets.iter().copied().filter(|(_,tag)| required_tags.contains(tag)).collect::<Vec<_>>();
    topology.bind_selected_facets(&selected_records, &required_tags, SelectedFacetPolicy { ownership:SelectedFacetOwnership::ExteriorOnly, duplicates:SelectedFacetDuplicates::Reject, require_positive_tag:false })?;
    let contracts = package.get("boundaryInputs").and_then(|v| v.get(&case.settings.formulation)).ok_or("Scenario boundaryInputs are required.")?;
    let mut compiled_boundaries = HashMap::new();
    for row in selected {
        let kind = row.row.get("type").and_then(Value::as_str).ok_or("Boundary type is required.")?;
        let contract = contracts.get(kind).ok_or_else(|| format!("Boundary input type {kind:?} is not declared."))?;
        let mode = contract.get("mode").and_then(Value::as_str).ok_or("Boundary input mode is required.")?;
        if !matches!(mode,"point" | "trace") { return Err(format!("Boundary input mode {mode:?} is not supported by the stationary scalar runner.")); }
        if mode == "trace" && contract.get("sampling").and_then(Value::as_str) != Some("parameters") {
            return Err("Point-sampled boundary traces require a declared quadrature rule and are not yet supported.".into());
        }
        compiled_boundaries.insert(row.tag, (kind.to_string(), BoundaryInput::compile(contract,row.row)?));
    }
    let mut boundaries = Vec::<SimplexBoundaryPoint<F,D>>::new();
    let mut essential = Vec::new();
    let mut essential_plan: Option<&crate::essential_form::EssentialFormPlan> = None;
    for (facet_nodes, tag) in selected_records {
        let (kind, input) = compiled_boundaries.get(&tag).ok_or("Missing compiled boundary.")?;
        let facet = topology.facet(facet_nodes).ok_or("Selected facet disappeared.")?;
        let (unit_normal, measure) = normal(facet)?;
        if let Some(key) = input.form_key() {
            let form = plan.boundaries.get(key).ok_or_else(|| format!("Boundary form {key:?} is not declared."))?;
            let point = std::array::from_fn(|axis| facet_nodes.iter().map(|&node| nodes[node][axis]).sum::<f64>() / F as f64);
            let values = input.evaluate(parameters, Some(point), &HashMap::new(), Some(form))?;
            boundaries.push(SimplexBoundaryPoint { nodes:facet_nodes, basis:[1. / F as f64; F], normal:std::array::from_fn(|axis| unit_normal[axis]), weight:measure, plan:form, coefficients:values.coefficients, sources:values.sources });
        } else {
            let form = plan.essential.get(kind).ok_or_else(|| format!("Essential form {kind:?} is not declared."))?;
            if essential_plan.is_some_and(|old| !std::ptr::eq(old, form)) { return Err("Multiple essential form types are not supported by the scalar runner.".into()); }
            essential_plan = Some(form);
            for node in facet_nodes {
                let value = input.evaluate(parameters, Some(nodes[node]), &HashMap::new(), None)?.essential.ok_or("Essential boundary returned no value.")?;
                let values = form.point_values(value, None, "boundary point")?;
                if values.len() != 1 { return Err("Scalar essential form must return one value.".into()); }
                essential.push((node,values[0]));
            }
        }
    }
    let mean = if let Some(nullspace) = &plan.nullspace {
        let study = model.pointer("/model/study").and_then(Value::as_array).and_then(|studies| studies.iter().find(|s| s.get("id").and_then(Value::as_str) == Some(&case.study_id)))
            .ok_or("Selected study is missing.")?;
        let value = if let Some(quantity) = study.get(&nullspace.reference_property) {
            let actual = quantity.get("unit").and_then(Value::as_str).ok_or("Nullspace reference unit is required.")?;
            if model_expression::unit(actual)?.1 != model_expression::unit(&nullspace.reference_unit)?.1 { return Err("Nullspace reference unit is incompatible.".into()); }
            model_expression::CompiledQuantity::compile(quantity,"study reference")?.evaluate(parameters,"study reference")?
        } else { nullspace.reference_default * model_expression::unit(&nullspace.reference_unit)?.0 };
        Some(nodal_p1::MeanPolicy { value:C { re:value, im:0. }, balance_relative_tolerance:nullspace.balance_relative_tolerance })
    } else { None };
    let mean_value = mean.as_ref().map_or(0., |m| m.value.re);
    let periodic_map = periodic.as_ref().map(|map| nodal_p1::PeriodicMap { representative:&map.node_to_dof,
        weights:&map.node_weights, count:map.dof_count, zero_phase_tolerance:policy.phase_tolerance,
        essential_mapping:policy.essential_mapping });
    let problem = nodal_p1::SimplexProblem { nodes, cells:&generic_cells, topology:policy.topology, form:&plan.volume,
        symbol_names:&name_refs, boundaries:&boundaries, essential:&essential, essential_plan, periodic:periodic_map, mean };
    let options = nodal_p1::SolveOptions { backend:&case.settings.backend, ordering:&case.settings.ordering,
        threads:case.settings.threads, residual_tolerance:policy.residual_tolerance };
    let load_routes = source_data.iter().zip(&source_samples)
        .map(|((_, load_plan, _), samples)| (&load_plan.form, samples.as_slice()))
        .collect::<Vec<_>>();
    let solution = problem.solve_with_volume_load_routes(&options,cache,&load_routes)?;
    validate_real_samples(&solution.real,&solution.imaginary,policy.imaginary_tolerance,"Stationary solution")?;
    if policy.validate_auxiliary { validate_real_samples(&solution.auxiliary_real,&solution.auxiliary_imaginary,policy.imaginary_tolerance,"Stationary auxiliary")?; }
    let recovery = plan.recovery.as_ref().ok_or("Scenario scalar recovery is required.")?;
    let sampling = package.get("recoveries").and_then(|v| v.get(&case.settings.formulation)).and_then(|v| v.get("sampling"))
        .and_then(Value::as_str).ok_or("Scenario recovery sampling is required.")?;
    let mut cell_material = Vec::with_capacity(cells.len());
    let mut recovery_symbols = Vec::with_capacity(cells.len());
    for (_,tag) in cells {
        let values = material_values.get(tag).ok_or("Cell material values disappeared.")?;
        let result_value = values.get(&policy.result_material_symbol).ok_or_else(|| format!("Result material symbol {:?} is missing.",policy.result_material_symbol))?;
        cell_material.push(C { re:result_value.value, im:0. });
        recovery_symbols.push(values.iter().map(|(name,value)| (name.clone(),C { re:value.value, im:0. })).collect());
    }
    let recovered = if policy.recovery_centered {
        problem.recover_cells_centered(&solution, recovery, sampling, &recovery_symbols)?
    } else { problem.recover_cells(&solution, recovery, sampling, &recovery_symbols)? };
    if recovered.iter().flatten().any(|value| !value.re.is_finite() || !value.im.is_finite() || value.im.abs() > policy.recovery_imaginary_tolerance) {
        return Err("Recovered values are non-finite or violate their declared imaginary tolerance.".into());
    }
    let mut volume = 0.;
    for (cell,_) in cells {
        let points = cell.map(|node| nodes[node]);
        volume += match (D,N) {
            (2,3) => crate::mesh2d::triangle_geometry([points[0],points[1],points[2]])?.0,
            (3,4) => crate::simplex::tetra_p1([points[0],points[1],points[2],points[3]],crate::simplex::DeterminantTolerance::Absolute(0.))?.0,
            _ => return Err("Unsupported simplex measure.".into()),
        };
    }
    let mut boundary_power = BTreeMap::<usize,f64>::new();
    let mut boundary_area = BTreeMap::<usize,f64>::new();
    for (facet_nodes,tag) in facets {
        let facet = topology.facet(*facet_nodes).ok_or("Result facet is absent from topology.")?;
        if facet.owners.len() != 1 { continue; }
        let (unit_normal, area) = normal(facet)?;
        let cell = facet.owners[0].cell;
        let flux = recovered.get(cell).ok_or("Result facet has no recovered owner cell.")?;
        let outward = (0..D).map(|axis| flux[axis].re * unit_normal[axis]).sum::<f64>();
        *boundary_power.entry(*tag).or_default() += outward * area;
        *boundary_area.entry(*tag).or_default() += area;
    }
    let minimum = solution.real.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = solution.real.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let values = HashMap::from([
        ("studyId".into(), json!(case.study_id)), ("nodes".into(), json!(nodes)),
        ("cells".into(),json!(cells.iter().map(|(nodes,tag)| json!({"nodes":nodes.as_slice(),"materialTag":tag})).collect::<Vec<_>>())),
        ("case".into(),json!(0)), ("cellCount".into(),json!(cells.len())), ("minimum".into(),json!(minimum)), ("maximum".into(),json!(maximum)),
        ("backend".into(),json!(case.settings.backend)), ("ordering".into(),json!(case.settings.ordering)),
        ("threads".into(),json!(case.settings.threads)), ("quadrature".into(),json!(case.settings.quadrature)),
        ("residual".into(),json!(solution.residual)), ("unknowns".into(),json!(solution.field_dofs)),
        ("meanConstraints".into(),json!(solution.mean_constraints)), ("meanValue".into(),json!(mean_value)),
        ("systemUnknowns".into(),json!(solution.field_dofs + solution.mean_constraints)), ("volume".into(),json!(volume)),
        ("boundaryPower".into(),json!(boundary_power)), ("boundaryArea".into(),json!(boundary_area)),
        ("netBoundaryPower".into(),json!(boundary_power.values().sum::<f64>())),
    ]);
    let primary = solution.real.iter().zip(&solution.imaginary).map(|(&re,&im)| C { re,im }).collect();
    let data = crate::result_contract::ResultData { values, complex:HashMap::new(),
        series:HashMap::from([("primary".into(),primary),("cellMaterial".into(),cell_material)]),
        matrices:HashMap::from([("recovered".into(),recovered.iter().map(|value| value.to_vec()).collect())]) };
    Ok(ScalarResult { solution, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    
    
    
    

    

    

    

    

    

    

    

    

    

    

    

    

    

    

    

    

    #[test]
    fn auxiliary_validation_rejects_nonfinite_and_nonreal_values() {
        assert!(validate_real_samples(&[1.],&[1e-12],1e-10,"auxiliary").is_ok());
        assert!(validate_real_samples(&[f64::INFINITY],&[0.],1e-10,"auxiliary").is_err());
        assert!(validate_real_samples(&[0.],&[1e-3],1e-10,"auxiliary").is_err());
        assert!(validate_real_samples(&[0.],&[],1e-10,"auxiliary").is_err());
    }

    #[test]
    fn tetrahedron_vector_flux_matches_scalar_for_permuted_cells() {
        use crate::prepared_case::{CaseKind, Output, PreparedCase, SolverSettings};
        use std::path::PathBuf;
        let package: Value = serde_json::from_str(include_str!("scenarios/thermal3d.json")).unwrap();
        let case = PreparedCase { request:json!({"studyId":"study"}), profile:"thermal-3d".into(), study_id:"study".into(),
            kind:CaseKind::Stationary, settings:SolverSettings { formulation:"T".into(), element_order:1,
                quadrature:"standard".into(), backend:"faer".into(), ordering:"auto".into(), threads:1 },
            output:Output::Stationary { csv:"out.csv".into(), field_files:false, field_filename:None },
            model_path:PathBuf::new(), mesh_path:PathBuf::new(), map_path:PathBuf::new(), output_directory:PathBuf::new() };
        let mut model = json!({"scenario":{"profileId":"thermal-3d","dimension":3,"physics":"thermal"},"model":{
            "materials":[{"selectionId":"domain","thermalConductivity":{"expression":"1","unit":"W/(m*K)"}}],
            "physics":{"fixed":{"id":"fixed","type":"dirichlet","selectionId":"fixed","value":{"expression":"0","unit":"K"}},
                "load":{"id":"load","type":"neumann","selectionId":"load","value":{"expression":"0","unit":"W/m^2"},
                    "fieldValues":[{"expression":"1","unit":"W/m^2"},{"expression":"0","unit":"W/m^2"},{"expression":"0","unit":"W/m^2"}]}},
            "study":[{"id":"study"}], "mesh":{}}});
        let map = json!({"schema":"fullmeta-mesh-map/6","dimension":3,
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],
            "boundarySelections":[{"selectionId":"fixed","physicalTag":2},{"selectionId":"load","physicalTag":3}]});
        let nodes = [[0.,0.,0.],[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]];
        let facets = [([0,2,3],2),([1,2,3],3)];
        let mut expected: Option<Vec<C>> = None;
        for connectivity in [[0,1,2,3],[1,0,2,3],[3,2,1,0]] {
            let cells = [(connectivity,1)];
            model["model"]["physics"]["load"]["value"]["expression"] = json!("0");
            model["model"]["physics"]["load"]["fieldValues"] = json!([
                {"expression":"1","unit":"W/m^2"},{"expression":"0","unit":"W/m^2"},{"expression":"0","unit":"W/m^2"}]);
            let solve = |model: &Value| solve_with_result(&case,&package,model,&map,
                Mesh::Tetrahedron {nodes:&nodes,cells:&cells,facets:&facets},&HashMap::new(),&mut SolverCache::default()).unwrap();
            let vector = solve(&model).data.series["primary"].clone();
            model["model"]["physics"]["load"].as_object_mut().unwrap().remove("fieldValues");
            model["model"]["physics"]["load"]["value"]["expression"] = json!("0.5773502691896258");
            let scalar = solve(&model).data.series["primary"].clone();
            for (a,b) in vector.iter().zip(&scalar) { assert!((a.re-b.re).abs()<1e-10); }
            if let Some(reference) = &expected {
                for (a,b) in vector.iter().zip(reference) { assert!((a.re-b.re).abs()<1e-10); }
            }
            expected=Some(vector);
        }
    }

    #[test]
    fn matched_surface_periodicity_rejects_phase_and_incompatible_facets() {
        let mut package: Value = serde_json::from_str(include_str!("scenarios/thermal3d.json")).unwrap();
        package["preparedScalar"]["periodic"] = json!({"mode":"matchedNodes","phase":"zero","phaseTolerance":1e-12,
            "essentialMapping":"directRepresentative","requireModelMapAgreement":true,"rejectMeshPairs":false});
        let policy = RuntimePolicy::from_package(&package).unwrap();
        let pair = json!({"id":"p","dimension":3,"lowerSelectionId":"lo","upperSelectionId":"hi",
            "translation":[{"expression":"1","unit":"m"},{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}]});
        let mut model = json!({"scenario":{"profileId":"thermal-3d","dimension":3,"physics":"thermal"},"model":{"parameters":[],"mesh":{"periodicPairIds":["p"],"periodicPairs":[pair.clone()]},
            "physics":{"periodic":{"id":"periodic","type":"periodicCondition","periodicPairId":"p"}}}});
        let map = json!({"periodicPairs":[pair],"boundarySelections":[{"id":"lo","physicalTag":1},{"id":"hi","physicalTag":2}]});
        let nodes = [[0.,0.,0.],[0.,1.,0.],[0.,1.,1.],[0.,0.,1.],
            [1.,0.,0.],[1.,1.,0.],[1.,1.,1.],[1.,0.,1.]];
        let facets = [([0,1,2],1),([0,2,3],1),([4,5,6],2),([4,6,7],2)];
        let result = prepare_periodic(&policy,&model,&map,&nodes,&facets,&HashMap::new(),3).unwrap().unwrap();
        for (lo,hi) in [(0,4),(1,5),(2,6),(3,7)] { assert_eq!(result.node_to_dof[lo],result.node_to_dof[hi]); }
        let second = json!({"id":"q","dimension":3,"lowerSelectionId":"lowY","upperSelectionId":"highY",
            "translation":[{"expression":"0","unit":"m"},{"expression":"1","unit":"m"},{"expression":"0","unit":"m"}]});
        let mut two_model = model.clone();
        two_model["model"]["mesh"]["periodicPairIds"] = json!(["p","q"]);
        two_model["model"]["mesh"]["periodicPairs"].as_array_mut().unwrap().push(second.clone());
        two_model["model"]["physics"]["periodicY"] = json!({"id":"periodicY","type":"periodicCondition","periodicPairId":"q"});
        let mut two_map = map.clone();
        two_map["periodicPairs"].as_array_mut().unwrap().push(second);
        two_map["boundarySelections"].as_array_mut().unwrap().extend([
            json!({"id":"lowY","physicalTag":3}),json!({"id":"highY","physicalTag":4})]);
        let two_facets = [([0,1,2],1),([0,2,3],1),([4,5,6],2),([4,6,7],2),
            ([0,4,7],3),([0,7,3],3),([1,5,6],4),([1,6,2],4)];
        let both = prepare_periodic(&policy,&two_model,&two_map,&nodes,&two_facets,&HashMap::new(),3).unwrap().unwrap();
        assert_eq!(both.dof_count,2);
        for group in [[0,1,4,5],[2,3,6,7]] {
            for node in group { assert_eq!(both.node_to_dof[node],both.node_to_dof[group[0]]); }
        }
        let mismatched = [([0,1,2],1),([0,2,3],1),([4,5,7],2),([5,6,7],2)];
        assert!(prepare_periodic(&policy,&model,&map,&nodes,&mismatched,&HashMap::new(),3).is_err());
        model["model"]["physics"]["periodic"]["phaseVector"] = json!([
            {"expression":"1","unit":"rad/m"},{"expression":"0","unit":"rad/m"},{"expression":"0","unit":"rad/m"}]);
        assert!(prepare_periodic(&policy,&model,&map,&nodes,&facets,&HashMap::new(),3).is_err());
    }

    #[test]
    fn matched_tetrahedral_surfaces_preserve_linear_temperature_solution() {
        use crate::prepared_case::{CaseKind, Output, PreparedCase, SolverSettings};
        use std::path::PathBuf;
        let mut package: Value = serde_json::from_str(include_str!("scenarios/thermal3d.json")).unwrap();
        package["preparedScalar"]["periodic"] = json!({"mode":"matchedNodes","phase":"zero","phaseTolerance":1e-12,
            "essentialMapping":"directRepresentative","requireModelMapAgreement":true,"rejectMeshPairs":false});
        let case = PreparedCase { request:json!({"studyId":"study"}), profile:"thermal-3d".into(), study_id:"study".into(),
            kind:CaseKind::Stationary, settings:SolverSettings {formulation:"T".into(),element_order:1,quadrature:"standard".into(),backend:"faer".into(),ordering:"auto".into(),threads:1},
            output:Output::Stationary {csv:"out.csv".into(),field_files:false,field_filename:None},
            model_path:PathBuf::new(),mesh_path:PathBuf::new(),map_path:PathBuf::new(),output_directory:PathBuf::new() };
        let pair = json!({"id":"p","dimension":3,"lowerSelectionId":"lo","upperSelectionId":"hi",
            "translation":[{"expression":"1","unit":"m"},{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}]});
        let model = json!({"scenario":{"profileId":"thermal-3d","dimension":3,"physics":"thermal"},"model":{
            "parameters":[],"materials":[{"selectionId":"domain","thermalConductivity":{"expression":"1","unit":"W/(m*K)"}}],
            "mesh":{"periodicPairIds":["p"],"periodicPairs":[pair.clone()]},
            "physics":{"periodic":{"id":"periodic","type":"periodicCondition","periodicPairId":"p"},
                "bottom":{"id":"bottom","type":"dirichlet","selectionId":"bottom","value":{"expression":"0","unit":"K"}},
                "top":{"id":"top","type":"dirichlet","selectionId":"top","value":{"expression":"300","unit":"K"}}},
            "study":[{"id":"study"}]}});
        let map = json!({"schema":"fullmeta-mesh-map/6","dimension":3,"periodicPairs":[pair],
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],
            "boundarySelections":[{"id":"lo","physicalTag":1},{"id":"hi","physicalTag":2},
                {"id":"bottom","physicalTag":3},{"id":"top","physicalTag":4}]});
        let nodes = [[0.,0.,0.],[1.,0.,0.],[1.,1.,0.],[0.,1.,0.],
            [0.,0.,0.5],[1.,0.,0.5],[1.,1.,0.5],[0.,1.,0.5],
            [0.,0.,1.],[1.,0.,1.],[1.,1.,1.],[0.,1.,1.]];
        let cells = [([0,1,2,6],1),([0,2,3,6],1),([0,3,7,6],1),([0,7,4,6],1),([0,4,5,6],1),([0,5,1,6],1),
            ([4,5,6,10],1),([4,6,7,10],1),([4,7,11,10],1),([4,11,8,10],1),([4,8,9,10],1),([4,9,5,10],1)];
        let facets = [([0,3,7],1),([0,7,4],1),([4,7,11],1),([4,11,8],1),
            ([1,2,6],2),([1,6,5],2),([5,6,10],2),([5,10,9],2),
            ([0,1,2],3),([0,2,3],3),([8,9,10],4),([8,10,11],4)];
        let solve = |model: &Value| solve_with_result(&case,&package,model,&map,
            Mesh::Tetrahedron {nodes:&nodes,cells:&cells,facets:&facets},&HashMap::new(),&mut SolverCache::default()).unwrap();
        let solution = solve(&model);
        for (value, point) in solution.data.series["primary"].iter().zip(nodes) {
            assert!((value.re - 300.*point[2]).abs()<1e-9);
        }
        let mut driven = model.clone();
        driven["model"]["physics"]["source"] = json!({"id":"source","type":"domainSource","selectionId":"domain",
            "values":[{"expression":"8","unit":"W/m^3"}]});
        let solution = solve(&driven);
        for (value, point) in solution.data.series["primary"].iter().zip(nodes) {
            let z = point[2];
            assert!((value.re - (300.*z + 4.*z*(1.-z))).abs()<1e-9);
        }
        let mut conflicting = model.clone();
        conflicting["model"]["physics"]["extra"] = json!({"id":"extra","type":"dirichlet","selectionId":"lo",
            "value":{"expression":"0","unit":"K"}});
        let error = match solve_with_result(&case,&package,&conflicting,&map,
            Mesh::Tetrahedron {nodes:&nodes,cells:&cells,facets:&facets},&HashMap::new(),&mut SolverCache::default()) {
            Ok(_) => panic!("periodic and prescribed boundary must conflict"), Err(error) => error,
        };
        assert!(error.contains("also carries a prescribed boundary condition"), "{error}");
        let mut neumann = model.clone();
        neumann["model"]["physics"].as_object_mut().unwrap().remove("top");
        neumann["model"]["physics"].as_object_mut().unwrap().remove("bottom");
        neumann["model"]["study"][0]["meanTemperature"] = json!({"expression":"100","unit":"K"});
        let solution = solve(&neumann);
        for value in &solution.data.series["primary"] { assert!((value.re-100.).abs()<1e-8); }
        neumann["model"]["physics"]["source"] = driven["model"]["physics"]["source"].clone();
        assert!(solve_with_result(&case,&package,&neumann,&map,
            Mesh::Tetrahedron {nodes:&nodes,cells:&cells,facets:&facets},&HashMap::new(),&mut SolverCache::default()).is_err());
    }
}
