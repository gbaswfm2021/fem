//! Execute a prepared transient case through the shared scenario contracts.
//! The coordinator supplies staged model/mesh/map data and publishes events.
//! This internal entry point does not itself enable a transient Study.
use crate::solver_common::SolverCache;
use crate::transient_case::{BoundTransientCase, SnapshotIdentity};
use crate::transient_mesh::{NodalCell, NodalMesh};
use crate::transient_request::PreparedTransientCase;
use crate::transient_step::LinearOptions;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub(crate) struct MeshData {
    pub(crate) dimension: usize,
    pub(crate) nodes: Vec<[f64; 3]>,
    pub(crate) cells: Vec<NodalCell>,
    pub(crate) facets: Vec<(Vec<usize>, usize)>,
    /// Selected curve entities used by integration and probe output. They
    /// are not boundary-condition facets and must not enter boundary routing.
    pub(crate) integration_edges: Vec<([usize; 2], usize)>,
}
impl MeshData {
    pub(crate) fn parse(dimension: usize, text: &str) -> Result<Self, String> {
        match dimension {
            2 => {
                crate::prepared_scalar_execution::check_dense_2d_records(text)?;
                let mesh = crate::mesh2d::parse_mesh_v2(text)?;
                Ok(Self {
                    dimension,
                    nodes: mesh.nodes,
                    cells: mesh
                        .triangles
                        .into_iter()
                        .map(|(nodes, tag)| NodalCell {
                            nodes: nodes.to_vec(),
                            tag,
                        })
                        .collect(),
                    facets: mesh
                        .boundary_edges
                        .into_iter()
                        .map(|(nodes, tag)| (nodes.to_vec(), tag))
                        .collect(),
                    integration_edges: mesh.integration_edges,
                })
            }
            3 => {
                let mesh =
                    crate::mesh3d::parse_indexed_v1(text, crate::mesh3d::DuplicatePolicy::Reject)?;
                Ok(Self {
                    dimension,
                    nodes: mesh.nodes,
                    cells: mesh
                        .tetrahedra
                        .into_iter()
                        .map(|(nodes, tag)| NodalCell {
                            nodes: nodes.to_vec(),
                            tag,
                        })
                        .collect(),
                    facets: mesh
                        .boundary_faces
                        .into_iter()
                        .map(|(nodes, tag)| (nodes.to_vec(), tag))
                        .collect(),
                    integration_edges: mesh.edge_selections,
                })
            }
            _ => Err("Transient mesh dimension must be 2 or 3.".into()),
        }
    }
}

/// File contract used by a staged coordinator case. The coordinator owns
/// the working directory and final artifact registration.
pub(crate) fn run_request(path: &Path, profile: &str) -> Result<(), String> {
    let package = crate::scenario_package::load_installed(profile)?;
    run_request_with(path, profile, &package, &crate::preflight_client::validate)
}
fn run_request_with(
    path: &Path,
    profile: &str,
    package: &Value,
    preflight: &dyn Fn(&Value) -> Result<(), String>,
) -> Result<(), String> {
    use std::io::Write;
    let request: Value = serde_json::from_slice(
        &std::fs::read(path).map_err(|e| format!("Could not read transient request: {e}"))?,
    )
    .map_err(|e| format!("Invalid transient request JSON: {e}"))?;
    if request.get("schema").and_then(Value::as_str) != Some("fullmeta-transient-case/1")
        || request.get("solver").and_then(Value::as_str) != Some(profile)
        || request.get("studyType").and_then(Value::as_str) != Some("transient")
    {
        return Err("Transient request schema or solver identity is invalid.".into());
    }
    let resolve = |key: &str| -> Result<PathBuf, String> {
        let value = request
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| format!("Transient request {key} is required."))?;
        let value = PathBuf::from(value);
        Ok(if value.is_absolute() {
            value
        } else {
            path.parent().unwrap_or(Path::new(".")).join(value)
        })
    };
    let model: Value = serde_json::from_slice(
        &std::fs::read(resolve("modelFile")?)
            .map_err(|e| format!("Could not read transient model: {e}"))?,
    )
    .map_err(|e| format!("Invalid transient model JSON: {e}"))?;
    preflight(&model).map_err(|e| format!("Model validation:\n{e}"))?;
    let map: Value = serde_json::from_slice(
        &std::fs::read(resolve("modelMapFile")?)
            .map_err(|e| format!("Could not read transient mesh map: {e}"))?,
    )
    .map_err(|e| format!("Invalid transient mesh map JSON: {e}"))?;
    let study_id = request
        .get("studyId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("Transient request studyId is required.")?;
    let study = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(study_id))
        })
        .ok_or("Transient request Study is missing from its model.")?;
    let case = PreparedTransientCase::from_json(
        request
            .get("case")
            .ok_or("Transient request case is required.")?,
    )?;
    if model.pointer("/model/parameters").and_then(Value::as_array) != Some(&case.overrides) {
        return Err("Transient prepared case parameters do not match the staged model.".into());
    }
    let resolved = crate::model_expression::resolve_parameters(&case.overrides, &model)?;
    if resolved.len() != case.parameters.len()
        || resolved.iter().any(|(name, value)| {
            case.parameters.get(name).is_none_or(|saved| {
                saved.value != value.value || saved.dimension != value.dimension
            })
        })
    {
        return Err(
            "Transient prepared parameter values or dimensions do not match the staged model."
                .into(),
        );
    }
    let expected_wavenumber = crate::longitudinal::study_wavenumber(
        study,
        &resolved,
        &format!("Transient Study {study_id}.longitudinalWavenumber"),
    )?;
    crate::longitudinal::validate_prepared_case_value(
        &serde_json::json!({"longitudinalWavenumber":{
            "real":case.longitudinal_wavenumber.re,
            "imaginary":case.longitudinal_wavenumber.im,
            "unit":"rad/m"
        }}),
        expected_wavenumber,
    )?;
    let expected = crate::transient_request::TimeGrid::from_json(
        study
            .get("time")
            .ok_or("Transient Study time is required.")?,
        &resolved,
    )?;
    if (
        expected.start,
        expected.end,
        expected.max_step,
        expected.max_steps,
    ) != (
        case.grid.start,
        case.grid.end,
        case.grid.max_step,
        case.grid.max_steps,
    ) || expected.output_times != case.grid.output_times
    {
        return Err("Transient prepared time grid does not match the staged Study.".into());
    }
    let settings = request
        .get("solverSettings")
        .ok_or("Transient solverSettings are required.")?;
    let string = |key: &str| {
        settings
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("Transient solver setting {key} is required."))
    };
    let formulation = string("fieldFormulation")?;
    let quadrature = string("constitutiveQuadrature")?;
    let order = settings
        .get("elementOrder")
        .and_then(Value::as_u64)
        .and_then(|v| u8::try_from(v).ok())
        .ok_or("Transient element order is invalid.")?;
    let threads = settings
        .get("solverThreads")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v > 0)
        .ok_or("Transient solver thread count is invalid.")?;
    let tolerance = settings
        .get("residualTolerance")
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && *v > 0.)
        .ok_or("Transient residual tolerance is invalid.")?;
    let options = LinearOptions {
        tolerance,
        backend: string("solverBackend")?,
        ordering: string("solverOrdering")?,
        threads,
    };
    let dimension = package
        .pointer("/schemaDocument/x-fem-scenario/profile/dimension")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("Transient profile dimension is required.")?;
    let text = std::fs::read_to_string(resolve("meshFile")?)
        .map_err(|e| format!("Could not read transient mesh: {e}"))?;
    let mesh = MeshData::parse(dimension, &text)?;
    let case_index = request
        .get("caseIndex")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .ok_or("Transient caseIndex is invalid.")?;
    let output = resolve("outputDirectory")?;
    std::fs::create_dir_all(&output)
        .map_err(|e| format!("Could not create transient output directory: {e}"))?;
    let field_files=study.pointer("/output/fieldFiles").and_then(Value::as_bool).unwrap_or(true);
    if request.get("fieldFiles").is_some_and(|v|v.as_bool()!=Some(field_files)) {
        return Err("Transient output policy differs from the staged Study.".into());
    }
    run_case_with_output(
        package,
        &model,
        &map,
        &mesh,
        study_id,
        formulation,
        order,
        case_index,
        &case,
        quadrature,
        &options,
        &mut SolverCache::default(),
        field_files,
        |time, snapshot| {
            if let Some((index, document)) = snapshot {
                let name = format!("case-{case_index:06}-time-{index:06}.json");
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(output.join(&name))
                    .map_err(|e| format!("Could not create transient sample {name}: {e}"))?;
                serde_json::to_writer(&mut file, &document)
                    .map_err(|e| format!("Could not encode transient sample: {e}"))?;
                file.flush()
                    .map_err(|e| format!("Could not flush transient sample: {e}"))?;
                println!(
                    "FULLMETA_TRANSIENT_SAMPLE {}",
                    serde_json::json!({"caseIndex":case_index,"sampleIndex":index,"timeSeconds":time,"name":name})
                );
            }
            Ok(())
        },
    )
}

/// Execute one case and hand each requested time snapshot to the caller.
/// No output file is created here: publication and cancellation belong to
/// the coordinator and are propagated through `observe`.
pub(crate) fn run_case(
    package: &Value,
    model: &Value,
    map: &Value,
    mesh: &MeshData,
    study_id: &str,
    formulation: &str,
    order: u8,
    case_index: usize,
    prepared: &PreparedTransientCase,
    quadrature: &str,
    options: &LinearOptions<'_>,
    cache: &mut SolverCache,
    observe: impl FnMut(f64, Option<(usize, Value)>) -> Result<(), String>,
) -> Result<(), String> {
    run_case_with_output(package,model,map,mesh,study_id,formulation,order,case_index,prepared,quadrature,options,cache,true,observe)
}

fn run_case_with_output(
    package:&Value,model:&Value,map:&Value,mesh:&MeshData,study_id:&str,formulation:&str,order:u8,case_index:usize,
    prepared:&PreparedTransientCase,quadrature:&str,options:&LinearOptions<'_>,cache:&mut SolverCache,field_files:bool,
    mut observe:impl FnMut(f64,Option<(usize,Value)>)->Result<(),String>,
) -> Result<(), String> {
    let profile = package
        .get("id")
        .and_then(Value::as_str)
        .ok_or("Transient package profile is required.")?;
    let scenario = package
        .pointer("/schemaDocument/x-fem-scenario")
        .ok_or("Transient scenario metadata is required.")?;
    if scenario
        .pointer("/profile/dimension")
        .and_then(Value::as_u64)
        != Some(mesh.dimension as u64)
    {
        return Err("Transient mesh dimension does not match the scenario.".into());
    }
    let study = model
        .pointer("/model/study")
        .and_then(Value::as_array)
        .and_then(|rows| {
            rows.iter()
                .find(|row| row.get("id").and_then(Value::as_str) == Some(study_id))
        })
        .filter(|row| row.get("type").and_then(Value::as_str) == Some("transient"))
        .ok_or("Selected transient Study is missing or has the wrong type.")?;
    let physics = model
        .pointer("/model/physics")
        .ok_or("Transient Physics is required.")?;
    if study.get("physicsId").and_then(Value::as_str) != physics.get("id").and_then(Value::as_str)
        || physics.get("formulation").and_then(Value::as_str) != Some(formulation)
    {
        return Err("Transient Study, Physics and formulation do not agree.".into());
    }
    if prepared.grid.output_times.is_empty() {
        return Err("Transient Study must request at least one output time.".into());
    }
    let blank = serde_json::json!({});
    let initial = study.get("initialFields").unwrap_or(&blank);
    let cell_tags = mesh.cells.iter().map(|cell| cell.tag).collect::<Vec<_>>();
    let facet_tags = mesh.facets.iter().map(|(_, tag)| *tag).collect::<Vec<_>>();
    let tags = HashMap::from([
        ("materials".into(), cell_tags.clone()),
        ("domainSources".into(), cell_tags),
        ("boundaries".into(), facet_tags),
    ]);
    let mut bound =
        BoundTransientCase::prepare(package, profile, formulation, model, map, &tags, initial)?;
    bound.plan.set_longitudinal_wavenumber(prepared.longitudinal_wavenumber)?;
    let integrals=crate::transient_integration::TransientIntegration::prepare(model,map,mesh)?;
    let declarations =
        crate::model_schema::field_variables_for_profile(profile, Some(formulation), false)?;
    let mut sample_index = 0usize;
    let kind = bound.plan.element_space.as_deref().or_else(||{
        scenario.pointer("/execution/kind").and_then(Value::as_str)
    }).ok_or("Transient element-space adapter is required by the form or scenario.")?;
    match kind {
        "scalarSimplexP1" | "vectorSimplexP1" => {
            if order != 1 {
                return Err(
                    "The selected transient nodal adapter supports element order 1 only.".into(),
                );
            }
            let nodal = NodalMesh {
                dimension: mesh.dimension,
                nodes: &mesh.nodes,
                cells: &mesh.cells,
            };
            let periodic = crate::transient_periodic::nodal(
                &bound,
                prepared,
                model,
                map,
                &nodal,
                &mesh.facets,
            )?;
            let fixed = periodic.as_fixed();
            let boundary = bound.prepare_nodal_boundaries(&nodal, &mesh.facets)?;
            boundary.validate_initial_rates(&bound.plan,&nodal,&bound.initial,&prepared.parameters,prepared.grid.start)?;
            bound.run_nodal(
                prepared,
                &nodal,
                quadrature,
                (!periodic.is_identity()).then_some(&fixed),
                options,
                cache,
                |_, plan, layout, time, operators| {
                    boundary.apply(plan, layout, &nodal, &prepared.parameters, time, operators)
                },
                |_, layout, time, state, save| {
                    let output = if save {
                        let index = sample_index;
                        sample_index += 1;
                        let mut snapshot=if field_files {bound.nodal_snapshot(
                                prepared,
                                &nodal,
                                layout,
                                state,
                                SnapshotIdentity {
                                    study_id,
                                    case_index,
                                    sample_index: index,
                                    time,
                                },
                                &declarations,
                                true,
                            )?} else {serde_json::json!({"schema":"fullmeta-transient-history-sample/1","studyId":study_id,"caseIndex":case_index,"sampleIndex":index,"timeSeconds":time})};
                        if let Some(integrals)=&integrals {
                            let field=|cell:&crate::integration::Simplex,weights:&[f64],point:[f64;3]| {
                                let owner=cell.element_index.ok_or("Transient integration cell owner is missing.")?;
                                Ok(bound.nodal_symbols_at(prepared,&nodal,layout,state,owner,&cell.nodes,weights,point,time,&declarations)?
                                    .into_iter().map(|(name,v)|(name,fullmeta_core::pointwise::ComplexValue{re:v.re,im:v.im})).collect())
                            };
                            let material=|cell:&crate::integration::Simplex,point:[f64;3]| {
                                let tag=cell.material_tag.ok_or("Transient integration material is missing.")?;
                                Ok(bound.data.material_symbols_at(&prepared.parameters,tag,point,time)?.into_iter()
                                    .map(|(name,v)|(name,fullmeta_core::pointwise::ComplexValue{re:v.re,im:v.im})).collect())
                            };
                            snapshot["globalVariables"]=Value::Object(integrals.evaluate(mesh,&prepared.parameters,time,&field,&material)?);
                        }
                        Some((index,snapshot))
                    } else {
                        None
                    };
                    observe(time, output)
                },
            )?;
        }
        "orientedTetrahedronNedelec" => {
            if mesh.dimension != 3 || !matches!(order, 1 | 2) {
                return Err(
                    "Transient oriented tetrahedra require dimension 3 and order 1 or 2.".into(),
                );
            }
            let cells = mesh
                .cells
                .iter()
                .map(|cell| {
                    Ok((
                        cell.nodes
                            .clone()
                            .try_into()
                            .map_err(|_| "Transient edge cell must be tetrahedral.")?,
                        cell.tag,
                    ))
                })
                .collect::<Result<Vec<([usize; 4], usize)>, String>>()?;
            let facets = mesh
                .facets
                .iter()
                .map(|(nodes, tag)| {
                    Ok((
                        nodes
                            .clone()
                            .try_into()
                            .map_err(|_| "Transient edge facet must be triangular.")?,
                        *tag,
                    ))
                })
                .collect::<Result<Vec<([usize; 3], usize)>, String>>()?;
            let edge =
                crate::transient_edge::EdgeTransientMesh::prepare(&mesh.nodes, &cells, order)?;
            let output_cells = mesh
                .cells
                .iter()
                .map(|cell| (cell.nodes.clone(), cell.tag))
                .collect::<Vec<_>>();
            let periodic = crate::transient_periodic::edge(
                &bound,
                prepared,
                model,
                map,
                &edge,
                &mesh.nodes,
                &output_cells,
                &mesh.facets,
            )?;
            let fixed = periodic.as_fixed();
            let boundary = bound.prepare_edge_boundaries(&edge, &facets)?;
            bound.run_edge_with_boundaries(
                prepared,
                &edge,
                &boundary,
                quadrature,
                (!periodic.is_identity()).then_some(&fixed),
                options,
                cache,
                |_, layout, time, state, save| {
                    let output = if save {
                        let index = sample_index;
                        sample_index += 1;
                        let mut snapshot=if field_files {bound.edge_snapshot(
                                prepared,
                                &edge,
                                &mesh.nodes,
                                &output_cells,
                                layout,
                                state,
                                SnapshotIdentity {
                                    study_id,
                                    case_index,
                                    sample_index: index,
                                    time,
                                },
                                &declarations,
                                true,
                            )?} else {serde_json::json!({"schema":"fullmeta-transient-history-sample/1","studyId":study_id,"caseIndex":case_index,"sampleIndex":index,"timeSeconds":time})};
                        if let Some(integrals)=&integrals {
                            let field=|cell:&crate::integration::Simplex,_weights:&[f64],point:[f64;3]| {
                                let owner=cell.element_index.ok_or("Transient integration cell owner is missing.")?;
                                Ok(bound.edge_symbols_at(&edge,layout,state,owner,point,&declarations)?.into_iter()
                                    .map(|(name,v)|(name,fullmeta_core::pointwise::ComplexValue{re:v.re,im:v.im})).collect())
                            };
                            let material=|cell:&crate::integration::Simplex,point:[f64;3]| {
                                let tag=cell.material_tag.ok_or("Transient integration material is missing.")?;
                                Ok(bound.data.material_symbols_at(&prepared.parameters,tag,point,time)?.into_iter()
                                    .map(|(name,v)|(name,fullmeta_core::pointwise::ComplexValue{re:v.re,im:v.im})).collect())
                            };
                            snapshot["globalVariables"]=Value::Object(integrals.evaluate(mesh,&prepared.parameters,time,&field,&material)?);
                        }
                        Some((index,snapshot))
                    } else {
                        None
                    };
                    observe(time, output)
                },
            )?;
        }
        "orientedTriangleNedelecH1DirectSum" => {
            if mesh.dimension != 2 || order != 1 {
                return Err("Transient triangle Whitney–H1 direct-sum space requires a 2D mesh and element order 1.".into());
            }
            let cells=mesh.cells.iter().map(|cell|Ok((cell.nodes.clone().try_into()
                .map_err(|_|"Transient triangle cell must have three vertices.")?,cell.tag)))
                .collect::<Result<Vec<([usize;3],usize)>,String>>()?;
            let triangles=crate::transient_triangle::TriangleTransientMesh::prepare(&mesh.nodes,&cells)?;
            let output_cells=mesh.cells.iter().map(|cell|(cell.nodes.clone(),cell.tag)).collect::<Vec<_>>();
            let boundary_edges=mesh.facets.iter().map(|(nodes,tag)|Ok((nodes.as_slice().try_into()
                .map_err(|_|"Transient triangle boundary facet must have two vertices.")?,*tag)))
                .collect::<Result<Vec<([usize;2],usize)>,String>>()?;
            let boundary=bound.prepare_triangle_boundaries(&triangles,&boundary_edges)?;
            let periodic=crate::transient_periodic::triangle(&bound,&prepared.parameters,model,map,&triangles,&mesh.nodes,&cells,&mesh.facets)?;
            let fixed=periodic.as_fixed();
            bound.run_triangle_with_boundaries(prepared,&triangles,&boundary,quadrature,
                (!periodic.is_identity()).then_some(&fixed),options,cache,
                |_,layout,time,state,save|{
                    let output=if save{
                        let index=sample_index;sample_index+=1;
                        let mut snapshot=if field_files{bound.triangle_snapshot(prepared,&triangles,&mesh.nodes,&output_cells,
                            layout,state,SnapshotIdentity{study_id,case_index,sample_index:index,time},&declarations,true)?}
                            else{serde_json::json!({"schema":"fullmeta-transient-history-sample/1","studyId":study_id,
                                "caseIndex":case_index,"sampleIndex":index,"timeSeconds":time})};
                        if let Some(integrals)=&integrals{
                            let field=|cell:&crate::integration::Simplex,_weights:&[f64],point:[f64;3]|{
                                let owner=cell.element_index.ok_or("Transient integration cell owner is missing.")?;
                                Ok(bound.triangle_symbols_at(&triangles,layout,state,owner,point,&declarations)?.into_iter()
                                    .map(|(name,value)|(name,fullmeta_core::pointwise::ComplexValue{re:value.re,im:value.im})).collect())
                            };
                            let material=|cell:&crate::integration::Simplex,point:[f64;3]|{
                                let tag=cell.material_tag.ok_or("Transient integration material is missing.")?;
                                Ok(bound.data.material_symbols_at(&prepared.parameters,tag,point,time)?.into_iter()
                                    .map(|(name,value)|(name,fullmeta_core::pointwise::ComplexValue{re:value.re,im:value.im})).collect())
                            };
                            snapshot["globalVariables"]=Value::Object(integrals.evaluate(mesh,&prepared.parameters,time,&field,&material)?);
                        }
                        Some((index,snapshot))
                    }else{None};
                    observe(time,output)
                })?;
        }
        other => {
            return Err(format!(
                "Unsupported transient execution capability {other:?}."
            ));
        }
    }
    if sample_index != prepared.grid.output_times.len() {
        return Err("Transient solver did not produce every requested sample.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn declared_source_component(declaration: &Value, feature_type: &str, fallback_component: usize) -> usize {
        declaration
            .get("domainSources")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|route| route.get("featureType").and_then(Value::as_str) == Some(feature_type))
            .and_then(|route| route.pointer("/input/sourceComponentIndices/0"))
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(fallback_component)
    }

    #[test]
    fn mesh_retains_selected_integration_curves_separately_from_boundary_facets() {
        let two=MeshData::parse(2,"FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nE,0,0,1,8\nC,0,1,2,9\nQ,0,0,1,2,7\n").unwrap();
        assert_eq!(two.facets,vec![(vec![0,1],8)]);
        assert_eq!(two.integration_edges,vec![([1,2],9)]);
        let three=MeshData::parse(3,"FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nF,0,0,1,2,8\nL,0,1,2,9\nT,0,0,1,2,3,7\nFULLMETA_MESH_DATA_END\n").unwrap();
        assert_eq!(three.facets,vec![(vec![0,1,2],8)]);
        assert_eq!(three.integration_edges,vec![([1,2],9)]);
    }
    #[test]
    fn requested_snapshots_include_time_dependent_domain_and_probe_values() {
        for (profile,dim,package_text,mesh_text,measure) in [
            ("thermal-2d",2,include_str!("scenarios/thermal2d.json"),"FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nQ,0,0,1,2,7\n",0.5),
            ("thermal-3d",3,include_str!("scenarios/thermal3d.json"),"FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nT,0,0,1,2,3,7\nFULLMETA_MESH_DATA_END\n",1./6.),
        ] {
            let package:Value=serde_json::from_str(package_text).unwrap();let mesh=MeshData::parse(dim,mesh_text).unwrap();
            let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[{"id":"m","selectionId":"domain","volumetricHeatCapacity":{"expression":"2","unit":"W*s/(m^3*K)"}}],
                "selections":[{"id":"domain","entity":"domain"},{"id":"point","entity":"point"}],
                "integrationOperators":[{"id":"dom","selectionId":"domain"},{"id":"probe","selectionId":"point"}],
                "variables":[{"symbol":"total","expression":"dom(T)+probe(T)+t"}],"pointwiseVariables":[],
                "physics":{"id":"physics","formulation":"T","source":{"id":"source","type":"domainSource","selectionId":"domain","values":[{"expression":"1","unit":"W/m^3"}]}},
                "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
            let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],
                "pointSelections":[{"id":"point","coordinateM":[0.1,0.1,0.]}],"domainCellSelections":{"domain":[0]}});
            let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":0.5,"maxSteps":2,"outputTimesSeconds":[0.5,1.]},
                "globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[]})).unwrap();
            let options=LinearOptions{tolerance:1e-9,backend:"faer",ordering:"amd",threads:1};let mut count=0;
            run_case(&package,&model,&map,&mesh,"study","T",1,0,&case,"standard",&options,&mut SolverCache::default(),|time,snapshot|{
                if let Some((_,value))=snapshot {let expected=time+time*0.5*(1.+measure);
                    let actual=value["globalVariables"]["total"]["re"].as_f64().unwrap();assert!((actual-expected).abs()<1e-9,"{profile}: {actual} != {expected}");count+=1;}
                Ok(())
            }).unwrap_or_else(|error|panic!("{profile}: {error}"));
            assert_eq!(count,2);
            let mut compact=0;
            run_case_with_output(&package,&model,&map,&mesh,"study","T",1,0,&case,"standard",&options,&mut SolverCache::default(),false,|time,snapshot|{
                if let Some((_,value))=snapshot {
                    assert_eq!(value["schema"],"fullmeta-transient-history-sample/1");
                    assert!(value.get("fields").is_none());assert!(value.get("nodes").is_none());
                    let expected=time+time*0.5*(1.+measure);
                    assert!((value["globalVariables"]["total"]["re"].as_f64().unwrap()-expected).abs()<1e-9);
                    compact+=1;
                } Ok(())
            }).unwrap_or_else(|error|panic!("{profile}: {error}"));
            assert_eq!(compact,2);
        }
    }

    #[test]
    fn discontinuous_time_dependent_materials_and_sources_keep_domain_identity() {
        for (profile, dimension, package_text, mesh_text) in [
            ("thermal-2d", 2, include_str!("scenarios/thermal2d.json"),
             "FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,1,1,0\nQ,0,0,1,2,7\nQ,1,1,3,2,8\n"),
            ("thermal-3d", 3, include_str!("scenarios/thermal3d.json"),
             "FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nV,4,0,0,-1\nT,0,0,1,2,3,7\nT,1,0,2,1,4,8\nFULLMETA_MESH_DATA_END\n"),
        ] {
            let package: Value = serde_json::from_str(package_text).unwrap();
            let mesh = MeshData::parse(dimension, mesh_text).unwrap();
            let model = json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[
                    {"id":"m1","selectionId":"d1","volumetricHeatCapacity":{"expression":"2*(1+t/tau)","unit":"W*s/(m^3*K)"}},
                    {"id":"m2","selectionId":"d2","volumetricHeatCapacity":{"expression":"5*(1+2*t/tau)","unit":"W*s/(m^3*K)"}}],
                "physics":{"id":"physics","formulation":"T",
                    "s1":{"id":"s1","type":"domainSource","selectionId":"d1","values":[{"expression":"2*(1+t/tau)","unit":"W/m^3"}]},
                    "s2":{"id":"s2","type":"domainSource","selectionId":"d2","values":[{"expression":"5*(1+2*t/tau)","unit":"W/m^3"}]}},
                "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
            let map = json!({"physicalGroups":[{"selectionId":"d1","physicalTag":7},{"selectionId":"d2","physicalTag":8}],
                "boundarySelections":[],"domainCellSelections":{"d1":[0],"d2":[1]}});
            let case = PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":0.5,"maxSteps":2,"outputTimesSeconds":[0.5,1.]},
                "globalParameters":{"tau":1.},"globalParameterDimensions":{"tau":crate::model_expression::unit("s").unwrap().1},"parameterOverrides":[]})).unwrap();
            let options = LinearOptions { tolerance:1e-9, backend:"faer", ordering:"amd", threads:1 };
            let mut samples = 0;
            run_case(&package,&model,&map,&mesh,"study","T",1,0,&case,"standard",&options,&mut SolverCache::default(),|time,snapshot| {
                if let Some((_,value)) = snapshot {
                    for value in value["fields"]["T"]["real"].as_array().unwrap() {
                        let actual = value.as_f64().unwrap();
                        assert!((actual-time).abs()<1e-9,"{profile}: {actual} != {time}");
                    }
                    samples += 1;
                }
                Ok(())
            }).unwrap_or_else(|error|panic!("{profile}: {error}"));
            assert_eq!(samples,2);
        }
    }

    #[test]
    fn staged_mesh_and_model_produce_complete_time_snapshots_for_both_dimensions() {
        for (profile, dimension, text, mesh_text) in [
            (
                "thermal-2d",
                2,
                include_str!("scenarios/thermal2d.json"),
                "FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nQ,0,0,1,2,7\n",
            ),
            (
                "thermal-3d",
                3,
                include_str!("scenarios/thermal3d.json"),
                "FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nT,0,0,1,2,3,7\nFULLMETA_MESH_DATA_END\n",
            ),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let mesh = MeshData::parse(dimension, mesh_text).unwrap();
            let model = json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[{"id":"material","selectionId":"domain","volumetricHeatCapacity":{"expression":"2","unit":"W*s/(m^3*K)"}}],
                "physics":{"id":"physics","formulation":"T","source":{"id":"source","type":"domainSource","selectionId":"domain","values":[{"expression":"1","unit":"W/m^3"}]}},
                "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{"domain":[0]}});
            let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":0.5,"maxSteps":2,"outputTimesSeconds":[0.,0.5,1.]},
                "globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[]})).unwrap();
            let options = LinearOptions {
                tolerance: 1e-9,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            let mut output = Vec::new();
            run_case(
                &package,
                &model,
                &map,
                &mesh,
                "study",
                "T",
                1,
                3,
                &case,
                "standard",
                &options,
                &mut SolverCache::default(),
                |time, snapshot| {
                    if let Some((index, value)) = snapshot {
                        assert_eq!(value["caseIndex"], 3);
                        assert_eq!(value["sampleIndex"], index);
                        assert_eq!(value["timeSeconds"], time);
                        output.push(value["fields"]["T"]["real"][0].as_f64().unwrap());
                    }
                    Ok(())
                },
            )
            .unwrap();
            assert_eq!(output.len(), 3);
            for (actual, expected) in output.into_iter().zip([0., 0.25, 0.5]) {
                assert!((actual - expected).abs() < 1e-9, "{profile}");
            }
            assert!(
                run_case(
                    &package,
                    &model,
                    &map,
                    &mesh,
                    "missing",
                    "T",
                    1,
                    0,
                    &case,
                    "standard",
                    &options,
                    &mut SolverCache::default(),
                    |_, _| Ok(())
                )
                .is_err()
            );
        }
    }

    #[test]
    fn staged_runner_applies_time_dependent_boundary_in_both_dimensions() {
        for (profile,dimension,package_text,mesh_text) in [
            ("thermal-2d",2,include_str!("scenarios/thermal2d.json"),"FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nE,0,0,1,8\nQ,0,0,1,2,7\n"),
            ("thermal-3d",3,include_str!("scenarios/thermal3d.json"),"FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nF,0,0,1,2,8\nT,0,0,1,2,3,7\nFULLMETA_MESH_DATA_END\n"),
        ] {
            let package:Value=serde_json::from_str(package_text).unwrap();
            let mesh=MeshData::parse(dimension,mesh_text).unwrap();
            let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[{"id":"material","selectionId":"domain","volumetricHeatCapacity":{"expression":"2","unit":"W*s/(m^3*K)"}}],
                "physics":{"id":"physics","formulation":"T","wall":{"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"t/tau*(1+x/L)","unit":"K"}}},
                "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
            let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
            let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":0.5,"maxSteps":2,"outputTimesSeconds":[0.5,1.]},
                "globalParameters":{"L":1.,"tau":1.},"globalParameterDimensions":{"L":crate::model_expression::unit("m").unwrap().1,"tau":crate::model_expression::unit("s").unwrap().1},"parameterOverrides":[]})).unwrap();
            let options=LinearOptions{tolerance:1e-9,backend:"faer",ordering:"amd",threads:1};
            let mut count=0;
            run_case(&package,&model,&map,&mesh,"study","T",1,0,&case,"standard",&options,&mut SolverCache::default(),|time,snapshot|{
                if let Some((_,value))=snapshot {
                    let temperatures=value["fields"]["T"]["real"].as_array().unwrap();
                    assert!((temperatures[0].as_f64().unwrap()-time).abs()<1e-9);
                    assert!((temperatures[1].as_f64().unwrap()-2.*time).abs()<1e-9);
                    count+=1;
                }
                Ok(())
            }).unwrap_or_else(|error|panic!("{profile}: {error}"));
            assert_eq!(count,2);
        }
    }

    #[test]
    fn initial_rate_conflicts_are_rejected_for_supported_prescribed_traces() {
        let package:Value=serde_json::from_str(include_str!("scenarios/elastic2d.json")).unwrap();
        let mesh=MeshData::parse(2,"FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nE,0,0,1,8\nQ,0,0,1,2,7\n").unwrap();
        let mut model=json!({"scenario":{"profileId":"elastic-2d"},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
            "materials":[{"id":"material","selectionId":"domain"}],"physics":{"id":"physics","formulation":"planeStrain",
                "wall":{"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"0","unit":"m"},
                    "fieldValues":[{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}]}},
            "study":[{"id":"study","type":"transient","physicsId":"physics","initialFields":{"velocity":[{"expression":"1","unit":"m/s"},{"expression":"0","unit":"m/s"}]}}]}});
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[{"id":"wall","physicalTag":8}],"domainCellSelections":{}});
        let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":0.5,"maxSteps":2,"outputTimesSeconds":[1.]},
            "globalParameters":{"tau":1.},"globalParameterDimensions":{"tau":crate::model_expression::unit("s").unwrap().1},"parameterOverrides":[]})).unwrap();
        let options=LinearOptions{tolerance:1e-9,backend:"faer",ordering:"amd",threads:1};
        let run=|model:&Value|run_case(&package,model,&map,&mesh,"study","planeStrain",1,0,&case,"standard",&options,&mut SolverCache::default(),|_,_|Ok(()));
        assert!(run(&model).unwrap_err().contains("boundary rate"));
        model["model"]["physics"]["wall"]["fieldValues"][0]["expression"]=json!("t/tau");
        run(&model).unwrap();
        model["model"]["study"][0]["initialFields"]["velocity"][0]["expression"]=json!("2");
        assert!(run(&model).unwrap_err().contains("boundary rate"));
        model["model"]["physics"]["wall"]["fieldValues"][0]["expression"]=json!("sin(2*t/tau)");
        run(&model).unwrap();
        model["model"]["physics"]["wall"]["fieldValues"][0]["expression"]=json!("abs(t/tau)");
        assert!(run(&model).unwrap_err().contains("Cannot determine the initial prescribed boundary rate"));
    }

    #[test]
    fn thermal_time_varying_capacity_converges_at_backward_euler_order() {
        for (profile,dimension,package_text,mesh_text) in [
            ("thermal-2d",2,include_str!("scenarios/thermal2d.json"),"FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nQ,0,0,1,2,7\n"),
            ("thermal-3d",3,include_str!("scenarios/thermal3d.json"),"FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nT,0,0,1,2,3,7\nFULLMETA_MESH_DATA_END\n"),
        ] {
            let package:Value=serde_json::from_str(package_text).unwrap();
            let mesh=MeshData::parse(dimension,mesh_text).unwrap();
            let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[{"id":"material","selectionId":"domain","volumetricHeatCapacity":{"expression":"2*(1+x/L)*(1+t/tau)","unit":"W*s/(m^3*K)"}}],
                "physics":{"id":"physics","formulation":"T","source":{"id":"source","type":"domainSource","selectionId":"domain","values":[{"expression":"1+x/L","unit":"W/m^3"}]}},
                "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
            let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{"domain":[0]}});
            let options=LinearOptions{tolerance:1e-10,backend:"faer",ordering:"amd",threads:1};
            let mut errors=Vec::new();
            for steps in [4,8,16] {
                let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":1./steps as f64,"maxSteps":steps,"outputTimesSeconds":[1.]},
                    "globalParameters":{"L":1.,"tau":1.},"globalParameterDimensions":{"L":crate::model_expression::unit("m").unwrap().1,"tau":crate::model_expression::unit("s").unwrap().1},"parameterOverrides":[]})).unwrap();
                let mut result=None;
                run_case(&package,&model,&map,&mesh,"study","T",1,0,&case,"standard",&options,&mut SolverCache::default(),|_,snapshot|{
                    if let Some((_,value))=snapshot {result=Some(value["fields"]["T"]["real"][0].as_f64().unwrap());}
                    Ok(())
                }).unwrap_or_else(|error|panic!("{profile}/{steps}: {error}"));
                errors.push((result.unwrap()-0.5*2f64.ln()).abs());
            }
            assert!(errors[0]>errors[1]&&errors[1]>errors[2],"{profile}: {errors:?}");
            for pair in errors.windows(2) {assert!((1.8..2.2).contains(&(pair[0]/pair[1])),"{profile}: {errors:?}");}
        }
    }

    #[test]
    fn declared_time_varying_materials_drive_all_scenario_forms() {
        for text in [include_str!("scenarios/acoustic2d.json"),include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),include_str!("scenarios/em3d.json"),
            include_str!("scenarios/thermal2d.json"),include_str!("scenarios/thermal3d.json")] {
            let package:Value=serde_json::from_str(text).unwrap();
            let profile=package["id"].as_str().unwrap();
            let dim=package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"].as_u64().unwrap() as usize;
            let mesh_text=if dim==2 {"FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nQ,0,0,1,2,7\n"} else {"FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nT,0,0,1,2,3,7\nFULLMETA_MESH_DATA_END\n"};
            let mesh=MeshData::parse(dim,mesh_text).unwrap();
            for (form,declaration) in package["transientForms"].as_object().unwrap() {
                let mut material=json!({"id":"material","selectionId":"domain"});
                if profile.starts_with("acoustic-") || profile.starts_with("elastic-") {
                    material["density"]=json!({"expression":"(1+x/L)*(1+t/tau)","unit":"kg/m^3"});
                }
                if profile.starts_with("acoustic-"){material["soundSpeed"]=json!({"expression":"1","unit":"m/s"});}
                if profile.starts_with("elastic-"){material["youngModulus"]=json!({"expression":"1","unit":"Pa"});}
                if profile.starts_with("em-"){
                    material["relativePermittivity"]=json!("(1+x/L)*(1+t/tau)");
                    material["relativePermeability"]=json!("(1+x/L)*(1+t/tau)");
                }
                if profile.starts_with("thermal-"){material["volumetricHeatCapacity"]=json!({"expression":"2*(1+x/L)*(1+t/tau)","unit":"W*s/(m^3*K)"});}
                let defaults=package["schemaDocument"]["x-fem-physics"]["boundaryDefaultsByFormulation"][form]["domainSource"]["values"].as_array().unwrap();
                let source_component=declared_source_component(declaration,"domainSource",
                    if profile=="em-2d"&&form=="Full" {2}else{0});
                let values=defaults.iter().enumerate().map(|(index,value)|json!({"expression":if index!=source_component {"0"} else if profile.starts_with("acoustic-"){"1/(1+x/L)"} else {"1+x/L"},"unit":value["unit"]})).collect::<Vec<_>>();
                let mut model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},"materials":[material],
                    "physics":{"id":"physics","formulation":form,"source":{"id":"source","type":"domainSource","selectionId":"domain","values":values}},
                    "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
                let mut map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{"domain":[0]}});
                let end=if profile.starts_with("em-"){1e-6}else{1.};
                let options=LinearOptions{tolerance:1e-8,backend:"faer",ordering:"amd",threads:1};
                let (symbol,expected)=if profile.starts_with("acoustic-") {("p",2.*end)}
                    else if profile.starts_with("elastic-") {("ux",0.)}
                    else if profile=="em-2d"&&form=="Hz" {("Hz",-end/(2.*0.00000125663706212))}
                    else if profile.starts_with("em-") {(if dim==2 {"Ez"}else{"Ex"},-end/(2.*8.8541878128e-12))}
                    else {("T",0.)};
                model["model"]["selections"]=json!([{"id":"point","entity":"point"}]);
                model["model"]["integrationOperators"]=json!([{"id":"probe","selectionId":"point"}]);
                model["model"]["variables"]=json!([{"symbol":"history","expression":format!("probe({symbol})+t")}]);
                map["pointSelections"]=json!([{"id":"point","coordinateM":if dim==2 {vec![0.1,0.1,0.]}else{vec![0.1,0.1,0.1]}}]);
                let mut errors=Vec::new();
                for steps in [4,8,16,32] {
                    let dt=end/steps as f64;
                    let expected=if profile.starts_with("elastic-") {
                        (1..=steps).map(|i|dt*(i as f64/steps as f64)/(1.+i as f64/steps as f64)).sum::<f64>()
                    } else if profile.starts_with("thermal-") {
                        (1..=steps).map(|i|dt/(2.*(1.+i as f64/steps as f64))).sum::<f64>()
                    } else {expected};
                    let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":end,"maxStepSeconds":dt,"maxSteps":steps,"outputTimesSeconds":[end]},
                        "globalParameters":{"L":1.,"tau":end},"globalParameterDimensions":{"L":crate::model_expression::unit("m").unwrap().1,"tau":crate::model_expression::unit("s").unwrap().1},"parameterOverrides":[]})).unwrap();
                    let mut output=None;
                    run_case(&package,&model,&map,&mesh,"study",form,1,0,&case,"standard",&options,&mut SolverCache::default(),|_,snapshot|{
                        if let Some((_,value))=snapshot {output=Some(value);} Ok(())
                    }).unwrap_or_else(|error|panic!("{profile}/{form}/{steps}: {error}"));
                    let snapshot=output.unwrap();
                    let history=snapshot["globalVariables"]["history"]["re"].as_f64().unwrap();
                    assert!((history-(expected+end)).abs()<1e-7*(expected+end).abs().max(1e-20),"{profile}/{form}/{steps}: history {history}");
                    let values=snapshot["fields"][symbol]["real"].as_array().unwrap().clone();
                    for value in values {let actual=value.as_f64().unwrap();assert!((actual-expected).abs()<1e-7*expected.abs().max(1e-20),"{profile}/{form}/{steps}/{symbol}: {actual} != {expected}");}
                    let exact=if profile.starts_with("thermal-"){Some(0.5*2f64.ln())}
                        else if profile.starts_with("elastic-"){Some(1.-2f64.ln())}else{None};
                    if let Some(exact)=exact {errors.push((steps,(expected-exact).abs()));}
                }
                for pair in errors.windows(2) {assert!(pair[0].1>pair[1].1,"{profile}/{form}: refinement errors {errors:?}");}
                if errors.len()>2 {for pair in errors[1..].windows(2) {let ratio=pair[0].1/pair[1].1;assert!((1.8..2.2).contains(&ratio),"{profile}/{form}: expected first-order convergence, errors {errors:?}");}}
                assert!(declaration["fields"].is_array());
            }
        }
    }

    #[test]
    fn discontinuous_materials_and_selected_sources_cover_all_forms() {
        for text in [include_str!("scenarios/acoustic2d.json"),include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),include_str!("scenarios/em3d.json"),
            include_str!("scenarios/thermal2d.json"),include_str!("scenarios/thermal3d.json")] {
            let package:Value=serde_json::from_str(text).unwrap();
            let profile=package["id"].as_str().unwrap();
            let dim=package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"].as_u64().unwrap() as usize;
            let mesh_text=if dim==2 {
                "FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,1,1,0\nQ,0,0,1,2,7\nQ,1,1,3,2,8\n"
            } else {
                "FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nV,4,0,0,-1\nT,0,0,1,2,3,7\nT,1,0,2,1,4,8\nFULLMETA_MESH_DATA_END\n"
            };
            let mesh=MeshData::parse(dim,mesh_text).unwrap();
            let map=json!({"physicalGroups":[{"selectionId":"d1","physicalTag":7},{"selectionId":"d2","physicalTag":8}],
                "boundarySelections":[],"domainCellSelections":{"d1":[0],"d2":[1]}});
            for form in package["transientForms"].as_object().unwrap().keys() {
                let declaration=&package["transientForms"][form];
                let defaults=package["schemaDocument"]["x-fem-physics"]["boundaryDefaultsByFormulation"][form]["domainSource"]["values"].as_array().unwrap();
                let source_component=declared_source_component(declaration,"domainSource",
                    if profile=="em-2d"&&form=="Full" {2}else{0});
                let mut materials=Vec::new();
                let mut physics=json!({"id":"physics","formulation":form});
                for (id,scale) in [(1,1.),(2,3.)] {
                    let selection=format!("d{id}");
                    let mut material=json!({"id":format!("m{id}"),"selectionId":selection});
                    let varying=format!("{scale}*(1+t/tau)");
                    if profile.starts_with("acoustic-") || profile.starts_with("elastic-") {
                        material["density"]=json!({"expression":varying,"unit":"kg/m^3"});
                    }
                    if profile.starts_with("acoustic-"){material["soundSpeed"]=json!({"expression":"1","unit":"m/s"});}
                    if profile.starts_with("elastic-"){material["youngModulus"]=json!({"expression":"1","unit":"Pa"});}
                    if profile.starts_with("em-") {material["relativePermittivity"]=json!(varying);material["relativePermeability"]=json!(varying);}
                    if profile.starts_with("thermal-"){material["volumetricHeatCapacity"]=json!({"expression":format!("2*{varying}"),"unit":"W*s/(m^3*K)"});}
                    materials.push(material);
                    let strength=if profile.starts_with("acoustic-"){1./scale}else{scale};
                    let values=defaults.iter().enumerate().map(|(index,value)|json!({"expression":if index==source_component {strength.to_string()}else{"0".into()},"unit":value["unit"]})).collect::<Vec<_>>();
                    physics[format!("source{id}")]=json!({"id":format!("source{id}"),"type":"domainSource","selectionId":selection,"values":values});
                }
                let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},"materials":materials,
                    "physics":physics,"study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
                let end=if profile.starts_with("em-"){1e-6}else{1.};
                let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":end,"maxStepSeconds":end/4.,"maxSteps":4,"outputTimesSeconds":[end]},
                    "globalParameters":{"tau":end},"globalParameterDimensions":{"tau":crate::model_expression::unit("s").unwrap().1},"parameterOverrides":[]})).unwrap();
                let options=LinearOptions{tolerance:1e-8,backend:"faer",ordering:"amd",threads:1};
                let (symbol,expected)=if profile.starts_with("acoustic-") {("p",2.*end)}
                    else if profile.starts_with("elastic-") {("ux",(1..=4).map(|i|0.25*(i as f64/4.)/(1.+i as f64/4.)).sum::<f64>())}
                    else if profile=="em-2d"&&form=="Hz" {("Hz",-end/(2.*0.00000125663706212))}
                    else if profile.starts_with("em-") {(if dim==2 {"Ez"}else{"Ex"},-end/(2.*8.8541878128e-12))}
                    else {("T",(1..=4).map(|i|0.125/(1.+i as f64/4.)).sum::<f64>())};
                let orders=if profile=="em-3d"{vec![1,2]}else{vec![1]};
                for order in orders {
                    let mut output=None;
                    run_case(&package,&model,&map,&mesh,"study",form,order,0,&case,"standard",&options,&mut SolverCache::default(),|_,snapshot|{
                        if let Some((_,value))=snapshot {output=Some(value);} Ok(())
                    }).unwrap_or_else(|error|panic!("{profile}/{form}/{order}: {error}"));
                    for value in output.unwrap()["fields"][symbol]["real"].as_array().unwrap() {
                        let actual=value.as_f64().unwrap();
                        assert!((actual-expected).abs()<1e-7*expected.abs().max(1e-20),"{profile}/{form}/{order}/{symbol}: {actual} != {expected}");
                    }

                }
            }
        }
    }

    #[test]
    fn periodic_constraints_advance_driven_fields_for_all_forms() {
        for text in [include_str!("scenarios/acoustic2d.json"),include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),include_str!("scenarios/em3d.json"),
            include_str!("scenarios/thermal2d.json"),include_str!("scenarios/thermal3d.json")] {
            let package:Value=serde_json::from_str(text).unwrap();
            let profile=package["id"].as_str().unwrap();
            let dim=package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"].as_u64().unwrap() as usize;
            let (mesh_text,translation)=if dim==2 {
                ("FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,1,1,0\nV,3,0,1,0\nE,0,0,3,1\nE,1,1,2,2\nQ,0,0,1,2,7\nQ,1,0,2,3,7\n",["L","0","0"])
            } else {
                ("FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,2\nV,4,1,0,2\nV,5,0,1,2\nV,6,0,0,-1\nV,7,0,0,3\nF,0,0,1,2,1\nF,1,3,4,5,2\nT,0,0,1,2,6,7\nT,1,5,4,3,7,7\nFULLMETA_MESH_DATA_END\n",["0","0","2*L"])
            };
            let mesh=MeshData::parse(dim,mesh_text).unwrap();
            let pair=json!({"id":"pair","dimension":dim,"lowerSelectionId":"lo","upperSelectionId":"hi",
                "translation":translation.map(|expression|json!({"expression":expression,"unit":"m"}))});
            let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
                "boundarySelections":[{"id":"lo","physicalTag":1},{"id":"hi","physicalTag":2}],"periodicPairs":[pair],"domainCellSelections":{"domain":[0,1]}});
            for form in package["transientForms"].as_object().unwrap().keys() {
                let declaration=&package["transientForms"][form];
                let source_component=declared_source_component(declaration,"domainSource",
                    if profile=="em-2d"&&form=="Full" {2}else{0});
                let mut material=json!({"id":"material","selectionId":"domain"});
                if profile.starts_with("acoustic-")||profile.starts_with("elastic-"){material["density"]=json!({"expression":"1","unit":"kg/m^3"});}
                if profile.starts_with("acoustic-"){material["soundSpeed"]=json!({"expression":"1","unit":"m/s"});}
                if profile.starts_with("elastic-"){material["youngModulus"]=json!({"expression":"1","unit":"Pa"});}
                if profile.starts_with("thermal-"){material["volumetricHeatCapacity"]=json!({"expression":"2","unit":"W*s/(m^3*K)"});}
                let defaults=package["schemaDocument"]["x-fem-physics"]["boundaryDefaultsByFormulation"][form]["domainSource"]["values"].as_array().unwrap();
                let values=defaults.iter().enumerate().map(|(i,v)|json!({"expression":if i==source_component {"1"}else{"0"},"unit":v["unit"]})).collect::<Vec<_>>();
                let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":["pair"]},"materials":[material],
                    "physics":{"id":"physics","formulation":form,"periodic":{"id":"periodic","type":"periodicCondition","periodicPairId":"pair"},
                        "source":{"id":"source","type":"domainSource","selectionId":"domain","values":values}},
                    "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
                // Resolve electromagnetic propagation across the unit mesh;
                // a much larger step damps the asymmetric free solution.
                let dt=if profile.starts_with("em-"){1e-12}else{1e-6};
                let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":dt,"maxStepSeconds":dt,"maxSteps":1,"outputTimesSeconds":[dt]},
                    "globalParameters":{"L":1.},"globalParameterDimensions":{"L":crate::model_expression::unit("m").unwrap().1},"parameterOverrides":[]})).unwrap();
                let options=LinearOptions{tolerance:1e-8,backend:"faer",ordering:"amd",threads:1};
                let (symbol,expected)=if profile.starts_with("acoustic-") {("p",dt)}
                    else if profile.starts_with("elastic-") {("ux",dt*dt)}
                    else if profile=="em-2d"&&form=="Hz" {("Hz",-dt/0.00000125663706212)}
                    else if profile.starts_with("em-") {(if dim==2 {"Ez"}else{"Ex"},-dt/8.8541878128e-12)}
                    else {("T",dt/2.)};
                for order in if profile=="em-3d"{vec![1,2]}else{vec![1]} {
                    let mut output=None;
                    run_case(&package,&model,&map,&mesh,"study",form,order,0,&case,"standard",&options,&mut SolverCache::default(),|_,snapshot|{
                        if let Some((_,value))=snapshot {output=Some(value);} Ok(())
                    }).unwrap_or_else(|error|panic!("{profile}/{form}/{order}: {error}"));
                    for value in output.unwrap()["fields"][symbol]["real"].as_array().unwrap() {
                        let actual=value.as_f64().unwrap();
                        assert!((actual-expected).abs()<1e-7*expected.abs().max(1e-20),"{profile}/{form}/{order}/{symbol}: {actual} != {expected}");
                    }
                    // An asymmetric source makes the constraint observable:
                    // the paired values agree only when the periodic map is applied.
                    let mut driven=model.clone();
                    driven["model"]["physics"]["source"]["values"][source_component]["expression"]=json!(if dim==2 {"x/L"}else{"z/L"});
                    let solve=|input:&Value| {
                        let mut result=None;
                        run_case(&package,input,&map,&mesh,"study",form,order,0,&case,"standard",&options,&mut SolverCache::default(),|_,snapshot|{
                            if let Some((_,value))=snapshot {result=Some(value);} Ok(())
                        }).unwrap_or_else(|error|panic!("{profile}/{form}/{order}: {error}"));
                        result.unwrap()["fields"][symbol]["real"].as_array().unwrap().iter().map(|v|v.as_f64().unwrap()).collect::<Vec<_>>()
                    };
                    let constrained=solve(&driven);
                    driven["model"]["mesh"]["periodicPairIds"]=json!([]);
                    driven["model"]["physics"].as_object_mut().unwrap().remove("periodic");
                    let free=solve(&driven);
                    let pairs=if dim==2 {vec![(0,1),(3,2)]}else{vec![(0,3),(1,4),(2,5)]};
                    let scale=free.iter().chain(&constrained).fold(1e-30_f64,|m,x|m.max(x.abs()));
                    for &(lo,hi) in &pairs {
                        assert!((constrained[lo]-constrained[hi]).abs()<1e-6*scale,
                            "{profile}/{form}/{order}: periodic values {} and {} differ",constrained[lo],constrained[hi]);
                    }
                    assert!(pairs.iter().any(|&(lo,hi)|(free[lo]-free[hi]).abs()>1e-5*scale),
                        "{profile}/{form}/{order}: source did not distinguish constrained and unconstrained fields");
                }
            }
        }
    }

    #[test]
    fn file_runner_checks_case_consistency_and_writes_distinct_samples() {
        let name = format!(
            "fullmeta-transient-case-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let dir = std::env::temp_dir().join(name);
        std::fs::create_dir(&dir).unwrap();
        let package: Value =
            serde_json::from_str(include_str!("scenarios/thermal2d.json")).unwrap();
        let parameters = json!([
            {"id":"length","name":"length","symbol":"L","expression":"1","unit":"m"},
            {"id":"timeScale","name":"timeScale","symbol":"tau","expression":"1","unit":"s"}
        ]);
        let mut model = json!({"scenario":{"profileId":"thermal-2d"},"model":{"parameters":parameters,"mesh":{"periodicPairIds":[]},
            "materials":[{"id":"material","selectionId":"domain","volumetricHeatCapacity":{"expression":"2*(1+x/L)*(1+t/tau)","unit":"W*s/(m^3*K)"}}],
            "physics":{"id":"physics","formulation":"T","source":{"id":"source","type":"domainSource","selectionId":"domain","values":[{"expression":"1+x/L","unit":"W/m^3"}]}},
            "study":[{"id":"study","type":"transient","physicsId":"physics","time":{
                "startTime":{"expression":"0","unit":"s"},"endTime":{"expression":"1","unit":"s"},"maxStep":{"expression":"0.5","unit":"s"},"maxSteps":2,
                "outputTimes":[{"expression":"0","unit":"s"},{"expression":"1","unit":"s"}]}}]}});
        let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{"domain":[0]}});
        let mut request = json!({"schema":"fullmeta-transient-case/1","studyType":"transient","solver":"thermal-2d","studyId":"study","caseIndex":2,
            "modelFile":"model.json","meshFile":"mesh.txt","modelMapFile":"map.json","outputDirectory":"output",
            "solverSettings":{"fieldFormulation":"T","constitutiveQuadrature":"standard","elementOrder":1,"solverThreads":1,"solverBackend":"faer","solverOrdering":"amd","residualTolerance":1e-9},
            "case":{"time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":0.5,"maxSteps":2,"outputTimesSeconds":[0.,1.]},
                "globalParameters":{"L":1.,"tau":1.},"globalParameterDimensions":{"L":crate::model_expression::unit("m").unwrap().1,"tau":crate::model_expression::unit("s").unwrap().1},"parameterOverrides":model["model"]["parameters"]}});
        std::fs::write(dir.join("model.json"), model.to_string()).unwrap();
        std::fs::write(dir.join("map.json"), map.to_string()).unwrap();
        std::fs::write(
            dir.join("mesh.txt"),
            "FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nQ,0,0,1,2,7\n",
        )
        .unwrap();
        let request_path = dir.join("request.json");
        std::fs::write(&request_path, request.to_string()).unwrap();
        run_request_with(&request_path, "thermal-2d", &package, &|_| Ok(())).unwrap();
        let first: Value = serde_json::from_slice(
            &std::fs::read(dir.join("output/case-000002-time-000000.json")).unwrap(),
        )
        .unwrap();
        let last: Value = serde_json::from_slice(
            &std::fs::read(dir.join("output/case-000002-time-000001.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(first["timeSeconds"], 0.);
        assert_eq!(last["timeSeconds"], 1.);
        // Backward Euler samples C at t=0.5 and 1: 0.5/3 + 0.5/4 = 7/24.
        // The same spatial factor multiplies C and the source, so every
        // nodal temperature must be identical on this insulated element.
        for value in last["fields"]["T"]["real"].as_array().unwrap() {
            assert!((value.as_f64().unwrap() - 7. / 24.).abs() < 1e-9);
        }
        assert!(run_request_with(&request_path, "thermal-2d", &package, &|_| Ok(())).is_err());
        request["case"]["time"]["endSeconds"] = json!(2.);
        request["case"]["time"]["maxSteps"] = json!(4);
        std::fs::write(&request_path, request.to_string()).unwrap();
        assert!(
            run_request_with(&request_path, "thermal-2d", &package, &|_| Ok(()))
                .err()
                .unwrap()
                .contains("time grid")
        );
        // Exercise the real shared sweep expander and serialized case contract
        // through the file runner with two different time-dependent materials.
        model["model"]["study"][0]["parameterSweep"] = json!({"type":"parameterSweep","parameters":[{"parameter":"tau","values":{
            "type":"linear","count":2,"start":{"expression":"1","unit":"s"},"end":{"expression":"2","unit":"s"}}}]});
        let cases=crate::study_prepare::transient_cases(&model["model"]["study"][0],&model).unwrap();
        let cases=crate::study_prepare::serialize_transient_cases(&cases);
        assert_eq!(cases.len(),2);
        for (index,case) in cases.into_iter().enumerate() {
            let mut staged=model.clone();
            staged["model"]["parameters"]=case["parameterOverrides"].clone();
            staged["model"]["study"][0].as_object_mut().unwrap().remove("parameterSweep");
            std::fs::write(dir.join("model.json"),staged.to_string()).unwrap();
            request["case"]=case;
            request["caseIndex"]=json!(index);
            request["outputDirectory"]=json!(format!("sweep-{index}"));
            std::fs::write(&request_path,request.to_string()).unwrap();
            run_request_with(&request_path,"thermal-2d",&package,&|_| Ok(())).unwrap();
            let output:Value=serde_json::from_slice(&std::fs::read(dir.join(format!("sweep-{index}/case-{index:06}-time-000001.json"))).unwrap()).unwrap();
            let expected=[7./24.,11./30.][index];
            for value in output["fields"]["T"]["real"].as_array().unwrap() {
                assert!((value.as_f64().unwrap()-expected).abs()<1e-9);
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn staged_case_executor_runs_each_declared_formulation_without_scenario_dispatch() {
        for text in [
            include_str!("scenarios/acoustic2d.json"),
            include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),
            include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),
            include_str!("scenarios/em3d.json"),
            include_str!("scenarios/thermal2d.json"),
            include_str!("scenarios/thermal3d.json"),
        ] {
            let package: Value = serde_json::from_str(text).unwrap();
            let profile = package["id"].as_str().unwrap();
            let dim = package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"]
                .as_u64()
                .unwrap() as usize;
            let text = if dim == 2 {
                "FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nQ,0,0,1,2,7\n"
            } else {
                "FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nT,0,0,1,2,3,7\nFULLMETA_MESH_DATA_END\n"
            };
            let mesh = MeshData::parse(dim, text).unwrap();
            let mut material = json!({"id":"material","selectionId":"domain"});
            if profile.starts_with("thermal-") {
                material["volumetricHeatCapacity"] = json!({"expression":"2","unit":"W*s/(m^3*K)"});
            }
            let map = json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{}});
            let case=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":1e-9,"maxStepSeconds":1e-9,"maxSteps":1,"outputTimesSeconds":[1e-9]},
                "globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[]})).unwrap();
            let options = LinearOptions {
                tolerance: 1e-6,
                backend: "faer",
                ordering: "amd",
                threads: 1,
            };
            for form in package["transientForms"].as_object().unwrap().keys() {
                let model = json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},"materials":[material.clone()],
                    "physics":{"id":"physics","formulation":form},"study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
                let orders = if package["schemaDocument"]["x-fem-scenario"]["execution"]["kind"]
                    == "orientedTetrahedronNedelec"
                {
                    vec![1, 2]
                } else {
                    vec![1]
                };
                for order in orders {
                    let mut count = 0;
                    run_case(
                        &package,
                        &model,
                        &map,
                        &mesh,
                        "study",
                        form,
                        order,
                        0,
                        &case,
                        "standard",
                        &options,
                        &mut SolverCache::default(),
                        |_, snapshot| {
                            if let Some((index, value)) = snapshot {
                                assert_eq!(index, 0);
                                assert_eq!(value["schema"], "fullmeta-transient-field/1");
                                count += 1;
                            }
                            Ok(())
                        },
                    )
                    .unwrap_or_else(|error| panic!("{profile}/{form}/{order}: {error}"));
                    assert_eq!(count, 1);
                }
            }
        }
    }

    fn structured_simplex_mesh(dimension: usize, divisions: usize) -> MeshData {
        let side = divisions + 1;
        let node = |i: usize, j: usize, k: usize| (k * side + j) * side + i;
        let mut nodes = Vec::with_capacity(side.pow(dimension as u32));
        for k in 0..if dimension == 3 { side } else { 1 } {
            for j in 0..side {
                for i in 0..side {
                    nodes.push([
                        i as f64 / divisions as f64,
                        j as f64 / divisions as f64,
                        k as f64 / divisions as f64,
                    ]);
                }
            }
        }

        let mut cells = Vec::new();
        if dimension == 2 {
            for j in 0..divisions {
                for i in 0..divisions {
                    let bl = node(i, j, 0);
                    let br = node(i + 1, j, 0);
                    let tl = node(i, j + 1, 0);
                    let tr = node(i + 1, j + 1, 0);
                    cells.push(NodalCell { nodes: vec![bl, br, tr], tag: 7 });
                    cells.push(NodalCell { nodes: vec![bl, tr, tl], tag: 7 });
                }
            }
        } else {
            // Freudenthal subdivision: six tetrahedra per cube, sharing the
            // same body diagonal so neighboring cubes have matching faces.
            let offsets = [
                ([0, 0, 0], [1, 0, 0], [1, 1, 0], [1, 1, 1]),
                ([0, 0, 0], [1, 0, 0], [1, 0, 1], [1, 1, 1]),
                ([0, 0, 0], [0, 0, 1], [1, 0, 1], [1, 1, 1]),
                ([0, 0, 0], [0, 0, 1], [0, 1, 1], [1, 1, 1]),
                ([0, 0, 0], [0, 1, 0], [0, 1, 1], [1, 1, 1]),
                ([0, 0, 0], [0, 1, 0], [1, 1, 0], [1, 1, 1]),
            ];
            for k in 0..divisions {
                for j in 0..divisions {
                    for i in 0..divisions {
                        for tet in offsets {
                            let index = |o: [usize; 3]| node(i + o[0], j + o[1], k + o[2]);
                            cells.push(NodalCell {
                                nodes: vec![index(tet.0), index(tet.1), index(tet.2), index(tet.3)],
                                tag: 7,
                            });
                        }
                    }
                }
            }
        }

        let mut face_counts: HashMap<Vec<usize>, (usize, Vec<usize>)> = HashMap::new();
        for cell in &cells {
            let local_facets: Vec<Vec<usize>> = if dimension == 2 {
                vec![
                    vec![cell.nodes[0], cell.nodes[1]],
                    vec![cell.nodes[1], cell.nodes[2]],
                    vec![cell.nodes[2], cell.nodes[0]],
                ]
            } else {
                vec![
                    vec![cell.nodes[0], cell.nodes[1], cell.nodes[2]],
                    vec![cell.nodes[0], cell.nodes[1], cell.nodes[3]],
                    vec![cell.nodes[0], cell.nodes[2], cell.nodes[3]],
                    vec![cell.nodes[1], cell.nodes[2], cell.nodes[3]],
                ]
            };
            for facet in local_facets {
                let mut key = facet.clone();
                key.sort_unstable();
                face_counts
                    .entry(key)
                    .and_modify(|entry| entry.0 += 1)
                    .or_insert((1, facet));
            }
        }
        let facets = face_counts
            .into_values()
            .filter_map(|(count, facet)| (count == 1).then_some((facet, 8)))
            .collect();
        MeshData { dimension, nodes, cells, facets, integration_edges: Vec::new() }
    }

    fn unstructured_simplex_mesh(dimension: usize, divisions: usize) -> MeshData {
        let mut mesh=structured_simplex_mesh(dimension,divisions);
        let amplitude=0.12/divisions as f64;
        for (index,point) in mesh.nodes.iter_mut().enumerate() {
            if (0..dimension).all(|axis|point[axis]>0.&&point[axis]<1.) {
                for axis in 0..dimension {
                    let phase=(index as f64+1.)*(axis as f64+1.37)*0.731;
                    point[axis]+=amplitude*phase.sin();
                }
            }
        }
        mesh
    }

    fn scenario_energy_material(profile: &str) -> Value {
        let value = |expression: &str, unit: &str| json!({"expression":expression,"unit":unit});
        match profile {
            "acoustic-2d" | "acoustic-3d" => json!({
                "density":value("1","kg/m^3"),
                "soundSpeed":value("1","m/s")
            }),
            "em-2d" | "em-3d" => json!({
                // Balance the vacuum constants to keep the energy norm well conditioned.
                "relativePermittivity":"141.9510639",
                "relativePermeability":"1",
                "conductivity":value("0","S/m")
            }),
            "elastic-2d" | "elastic-3d" => json!({
                "density":value("1","kg/m^3"),
                "youngModulus":value("1","Pa"),
                "poissonRatio":"0.25"
            }),
            "thermal-2d" | "thermal-3d" => json!({
                "thermalConductivity":value("1","W/(m*K)"),
                "volumetricHeatCapacity":value("1","W*s/(m^3*K)")
            }),
            _ => panic!("unknown transient energy profile {profile}"),
        }
    }

    fn assemble_energy_probe(
        bound: &crate::transient_case::BoundTransientCase<'_>,
        mesh: &MeshData,
        edge_mesh: Option<&crate::transient_edge::EdgeTransientMesh>,
        triangle_mesh: Option<&crate::transient_triangle::TriangleTransientMesh>,
        layout: &crate::transient_plan::StateLayout,
        workspace: &mut crate::transient_plan::TransientAssemblyWorkspace,
        parameters: &HashMap<String, crate::model_expression::Scalar>,
        time: f64,
    ) -> Result<crate::transient_step::TransientOperators, String> {
        let sampler = bound.data.sampler(parameters);
        let sources = bound.data.sources();
        let mut source_value = |feature_type: &str, location| sampler.source_for(feature_type, location);
        if let Some(edge_mesh) = edge_mesh {
            if !edge_mesh.matches_geometry(&mesh.nodes, &mesh.cells.iter()
                .map(|cell| (cell.nodes.clone(), cell.tag)).collect::<Vec<_>>()) {
                return Err("Energy probe edge mesh differs from its simplex mesh.".into());
            }
            edge_mesh.assemble(
                &bound.plan,
                layout,
                time,
                "standard",
                workspace,
                |location| sampler.coefficients(location),
                &sources,
                &mut source_value,
            )
        } else if let Some(triangle_mesh) = triangle_mesh {
            triangle_mesh.assemble(
                &bound.plan,
                layout,
                time,
                "standard",
                workspace,
                |location| sampler.coefficients(location),
                &sources,
                &mut source_value,
            )
        } else {
            let nodal = crate::transient_mesh::NodalMesh {
                dimension: mesh.dimension,
                nodes: &mesh.nodes,
                cells: &mesh.cells,
            };
            crate::transient_mesh::assemble_nodal_p1(
                &bound.plan,
                layout,
                &nodal,
                time,
                "standard",
                workspace,
                |location| sampler.coefficients(location),
                &sources,
                &mut source_value,
            )
        }
    }

    fn sparse_quadratic(matrix: &crate::transient_step::SparseMatrix, state: &[crate::solver_common::C]) -> f64 {
        matrix.iter().enumerate().map(|(row, entries)| {
            entries.iter().map(|(&column, value)| state[row].re * value.re * state[column].re).sum::<f64>()
        }).sum()
    }

    fn sparse_bilinear(left: &[crate::solver_common::C], matrix: &crate::transient_step::SparseMatrix,
        right: &[crate::solver_common::C]) -> f64 {
        matrix.iter().enumerate().map(|(row, entries)| {
            entries.iter().map(|(&column, value)| left[row].re * value.re * right[column].re).sum::<f64>()
        }).sum()
    }

    fn add_sparse_matrices(matrices: &[crate::transient_step::SparseMatrix], dimension: usize)
        -> crate::transient_step::SparseMatrix {
        let mut sum = vec![std::collections::BTreeMap::new(); dimension];
        for matrix in matrices {
            for (row, entries) in matrix.iter().enumerate() {
                for (&column, value) in entries {
                    sum[row].entry(column).and_modify(|entry: &mut crate::solver_common::C| entry.re += value.re)
                        .or_insert(*value);
                }
            }
        }
        sum
    }

    fn elastic_stiffness_energy(
        operator: &crate::transient_step::SparseMatrix,
        displacement: &[crate::solver_common::C],
        displacement_offset: usize,
        velocity_offset: usize,
    ) -> f64 {
        let n = displacement.len();
        (0..n).map(|row| {
            operator[velocity_offset + row].iter().filter_map(|(&column, value)| {
                (displacement_offset..displacement_offset + n).contains(&column)
                    .then(|| displacement[row].re * value.re * displacement[column - displacement_offset].re)
            }).sum::<f64>()
        }).sum()
    }

    #[test]
    fn em2d_full_transient_executes_at_zero_and_prescribed_kz() {
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let mesh=structured_simplex_mesh(2,1);
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":[],"domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let dt=1e-10;
        let options=LinearOptions{tolerance:1e-10,backend:"faer",ordering:"amd",threads:1};
        let initial=json!({"electric":[quantity("1","V/m"),quantity("0","V/m"),quantity("0","V/m")],
            "magnetic":[quantity("0","A/m"),quantity("1","A/m"),quantity("0","A/m")]});
        let mut outputs=Vec::new();
        for kz in [0.,2.] {
            let mut study=json!({"id":"study","type":"transient","physicsId":"physics","initialFields":initial,
                "time":{"startSeconds":0.,"endSeconds":dt,"maxStepSeconds":dt,"maxSteps":1,
                    "outputTimesSeconds":[dt]}});
            let mut prepared=json!({"time":{"startSeconds":0.,"endSeconds":dt,"maxStepSeconds":dt,"maxSteps":1,
                    "outputTimesSeconds":[dt]},"globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[]});
            if kz!=0. {
                study["longitudinalWavenumber"]=json!({"real":quantity("2","rad/m"),"imaginary":quantity("0","rad/m")});
                prepared["longitudinalWavenumber"]=json!({"real":2.,"imaginary":0.,"unit":"rad/m"});
            }
            let model=json!({"scenario":{"profileId":"em-2d"},"model":{
                "parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[{"id":"material","selectionId":"domain","relativePermittivity":"1",
                    "relativePermeability":"1","conductivity":quantity("0","S/m")}],
                "physics":{"id":"physics","formulation":"Full"},"study":[study]}});
            let case=PreparedTransientCase::from_json(&prepared).unwrap();
            let mut snapshot=None;
            run_case(&package,&model,&map,&mesh,"study","Full",1,0,&case,"standard",&options,
                &mut SolverCache::default(),|_,value|{snapshot=value.map(|(_,document)|document);Ok(())})
                .unwrap_or_else(|error|panic!("EM2D Full transient kz={kz}: {error}"));
            let snapshot=snapshot.expect("the requested transient sample is emitted");
            for symbol in ["Ex","Ey","Ez","Hx","Hy","Hz"] {
                assert_eq!(snapshot["fields"][symbol]["real"].as_array().unwrap().len(),mesh.nodes.len(),"{symbol}");
            }
            outputs.push(snapshot);
        }
        let finite=&outputs[1];
        let has_finite_kz_coupling=["Ex","Ey","Ez","Hx","Hy","Hz"].into_iter().any(|symbol| {
            finite["fields"][symbol]["imag"].as_array().unwrap().iter()
                .any(|value|value.as_f64().unwrap().abs()>1e-12)
        });
        assert!(has_finite_kz_coupling,"finite kz should produce complex cross-sectional field evolution");
        let zero_norm=outputs[0]["fields"]["Ex"]["real"].as_array().unwrap()[0].as_f64().unwrap();
        let finite_norm=outputs[1]["fields"]["Ex"]["real"].as_array().unwrap()[0].as_f64().unwrap();
        assert!((zero_norm-finite_norm).abs()>1e-8,"finite kz should change the evolved field");
    }

    #[test]
    fn em2d_full_transient_plane_wave_phase_converges_at_finite_kz(){
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let mut mesh=structured_simplex_mesh(2,1);
        for (edge,tag) in &mut mesh.facets{
            let points=edge.iter().map(|&node|mesh.nodes[node]).collect::<Vec<_>>();
            if points.iter().all(|point|point[0].abs()<1e-12){*tag=1;}
            else if points.iter().all(|point|(point[0]-1.).abs()<1e-12){*tag=2;}
            else if points.iter().all(|point|point[1].abs()<1e-12){*tag=3;}
            else if points.iter().all(|point|(point[1]-1.).abs()<1e-12){*tag=4;}
        }
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":[{"id":"left","physicalTag":1},{"id":"right","physicalTag":2},
                {"id":"bottom","physicalTag":3},{"id":"top","physicalTag":4}],
            "periodicPairs":[
                {"id":"px","dimension":2,"lowerSelectionId":"left","upperSelectionId":"right",
                    "translation":[quantity("1","m"),quantity("0","m"),quantity("0","m")]},
                {"id":"py","dimension":2,"lowerSelectionId":"bottom","upperSelectionId":"top",
                    "translation":[quantity("0","m"),quantity("1","m"),quantity("0","m")]}],
            "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let epsilon0=8.8541878128e-12_f64;
        let mu0=1.25663706212e-6_f64;
        let kz=2.0_f64;
        let speed=1.0_f64;
        let epsilon_r=1.0/epsilon0;
        let mu_r=1.0/mu0;
        let period=std::f64::consts::TAU/(speed*kz);
        let options=LinearOptions{tolerance:1e-10,backend:"faer",ordering:"amd",threads:1};
        let mut phase_errors=Vec::new();
        for steps in [20,40,80]{
            let dt=period/steps as f64;
            let study=json!({"id":"study","type":"transient","physicsId":"physics",
                "useLongitudinalWavenumber":true,
                "longitudinalWavenumber":{"real":quantity("2","rad/m"),"imaginary":quantity("0","rad/m")},
                "initialFields":{"electric":[quantity("1","V/m"),quantity("0","V/m"),quantity("0","V/m")],
                    "magnetic":[quantity("0","A/m"),quantity("1","A/m"),quantity("0","A/m")]},
                "time":{"startSeconds":0.,"endSeconds":period,"maxStepSeconds":dt,"maxSteps":steps,"outputTimesSeconds":[period]}});
            // E/H are normalized to equal constitutive coefficients, hence
            // the exact plane-wave impedance is one ohm in this fixture.
            let model=json!({"scenario":{"profileId":"em-2d"},"model":{"parameters":[],"mesh":{
                    "periodicPairIds":["px","py"],"periodicPairs":[
                        {"id":"px","lowerSelectionId":"left","upperSelectionId":"right","translation":[quantity("1","m"),quantity("0","m"),quantity("0","m")]},
                        {"id":"py","lowerSelectionId":"bottom","upperSelectionId":"top","translation":[quantity("0","m"),quantity("1","m"),quantity("0","m")]}]},
                "materials":[{"id":"material","selectionId":"domain","relativePermittivity":epsilon_r.to_string(),
                    "relativePermeability":mu_r.to_string(),"conductivity":quantity("0","S/m")}],
                "physics":{"id":"physics","formulation":"Full",
                    "periodicX":{"id":"periodicX","type":"periodicCondition","periodicPairId":"px"},
                    "periodicY":{"id":"periodicY","type":"periodicCondition","periodicPairId":"py"}},
                "study":[study]}});
            let prepared=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":period,
                    "maxStepSeconds":dt,"maxSteps":steps,"outputTimesSeconds":[period]},
                "globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[],
                "longitudinalWavenumber":{"real":kz,"imaginary":0.,"unit":"rad/m"}})).unwrap();
            let mut snapshot=None;
            run_case(&package,&model,&map,&mesh,"study","Full",1,0,&prepared,"standard",&options,
                &mut SolverCache::default(),|_,value|{snapshot=value.map(|(_,document)|document);Ok(())})
                .unwrap_or_else(|error|panic!("Full finite-kz plane wave with {steps} steps: {error}"));
            let snapshot=snapshot.expect("final plane-wave sample is emitted");
            let real=snapshot["fields"]["Ex"]["real"][0].as_f64().unwrap();
            let imaginary=snapshot["fields"]["Ex"]["imag"][0].as_f64().unwrap();
            let actual_phase=imaginary.atan2(real);
            let expected_phase=steps as f64*(speed*kz*dt).atan()-std::f64::consts::TAU;
            phase_errors.push(actual_phase.abs());
            assert!((actual_phase-expected_phase).abs()<2e-6,
                "backward-Euler phase differs from the exact discrete plane-wave response; steps={steps}, actual={actual_phase}, expected={expected_phase}");
            let expected_amplitude=(1.0+(speed*kz*dt).powi(2)).powf(-(steps as f64)/2.);
            assert!((real.hypot(imaginary)-expected_amplitude).abs()<2e-6,
                "backward-Euler amplitude differs from the expected discrete plane-wave response; steps={steps}, actual={}, expected={expected_amplitude}",real.hypot(imaginary));
            assert!(actual_phase.abs()<0.2,"one-period Full transient plane-wave phase did not return toward its initial phase; steps={steps}, phase={actual_phase}");
        }
        assert!(phase_errors[2]<phase_errors[1]&&phase_errors[1]<phase_errors[0],
            "finite-kz transient phase error should decrease under time-step refinement: {phase_errors:?}");
    }

    #[test]
    fn em2d_full_finite_kz_heterogeneous_transient_converges_under_mesh_refinement(){
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let epsilon0=8.8541878128e-12_f64;
        let mu0=1.25663706212e-6_f64;
        let end_time=0.2;
        let kz=0.7;
        let options=LinearOptions{tolerance:1e-10,backend:"faer",ordering:"amd",threads:1};
        let mut solutions=Vec::new();
        for divisions in [2,4,8]{
            let mut mesh=structured_simplex_mesh(2,divisions);
            let mut left_cells=Vec::new();let mut right_cells=Vec::new();
            for (index,cell) in mesh.cells.iter_mut().enumerate(){
                let center=cell.nodes.iter().map(|&node|mesh.nodes[node][0]).sum::<f64>()/3.;
                if center<0.5{cell.tag=7;left_cells.push(index);}else{cell.tag=8;right_cells.push(index);}
            }
            for (_,tag) in &mut mesh.facets{*tag=9;}
            let model=json!({"scenario":{"profileId":"em-2d"},"model":{
                "parameters":[
                    {"id":"length","name":"Length","symbol":"L","expression":"1","unit":"m"},
                    {"id":"timeScale","name":"Time scale","symbol":"tau","expression":"0.2","unit":"s"}],
                "mesh":{"periodicPairIds":[]},
                "materials":[
                    {"id":"core","selectionId":"left","relativePermittivity":(1.0/epsilon0).to_string(),
                        "relativePermeability":(1.0/mu0).to_string(),"conductivity":quantity("0","S/m")},
                    {"id":"cladding","selectionId":"right","relativePermittivity":(2.0/epsilon0).to_string(),
                        "relativePermeability":(1.0/mu0).to_string(),"conductivity":quantity("0","S/m")}],
                "physics":{"id":"physics","formulation":"Full",
                    "drive":{"id":"drive","type":"domainSource","selectionId":"left",
                        "values":[quantity("0","A/m^2"),quantity("0","A/m^2"),
                            quantity("sin(pi*x/L)*sin(pi*y/L)*t/tau","A/m^2")]},
                    "wall":{"id":"wall","type":"dirichlet","selectionId":"outer","value":quantity("0","V/m"),
                        "fieldValues":[quantity("0","V/m"),quantity("0","V/m"),quantity("0","V/m")]}},
                "study":[{"id":"study","type":"transient","physicsId":"physics","useLongitudinalWavenumber":true,
                    "longitudinalWavenumber":{"real":quantity("0.7","rad/m"),"imaginary":quantity("0","rad/m")},
                    "time":{"startTime":quantity("0","s"),"endTime":quantity("0.2","s"),
                        "maxStep":quantity("0.05","s"),"maxSteps":4,"outputTimes":[quantity("0.2","s")]}}]}});
            let map=json!({"physicalGroups":[{"selectionId":"left","physicalTag":7},{"selectionId":"right","physicalTag":8}],
                "boundarySelections":[{"id":"outer","physicalTag":9}],
                "domainCellSelections":{"left":left_cells,"right":right_cells}});
            let prepared=PreparedTransientCase::from_json(&json!({"time":{"startSeconds":0.,"endSeconds":end_time,
                    "maxStepSeconds":0.05,"maxSteps":4,"outputTimesSeconds":[end_time]},
                "globalParameters":{"L":1.,"tau":end_time},
                "globalParameterDimensions":{"L":[1.,0.,0.,0.,0.],"tau":[0.,0.,1.,0.,0.]},
                "parameterOverrides":[],"longitudinalWavenumber":{"real":kz,"imaginary":0.,"unit":"rad/m"}})).unwrap();
            let mut snapshot=None;
            run_case(&package,&model,&map,&mesh,"study","Full",1,0,&prepared,"standard",&options,
                &mut SolverCache::default(),|_,value|{snapshot=value.map(|(_,document)|document);Ok(())})
                .unwrap_or_else(|error|panic!("heterogeneous EM2D mesh divisions={divisions}: {error}"));
            solutions.push((mesh.nodes,snapshot.expect("heterogeneous transient final sample is emitted")));
        }
        let error_at_shared_nodes=|coarse_nodes:&[[f64;3]],coarse:&Value,fine_nodes:&[[f64;3]],fine:&Value|{
            let mut squared=0.;let mut count=0;
            for (index,point) in coarse_nodes.iter().enumerate(){
                let other=fine_nodes.iter().position(|candidate|candidate==point)
                    .unwrap_or_else(||panic!("refined mesh omitted coarse node {point:?}"));
                for field in ["Ex","Ey","Ez","Hx","Hy","Hz"]{
                    let re0=coarse["fields"][field]["real"][index].as_f64().unwrap();
                    let im0=coarse["fields"][field]["imag"][index].as_f64().unwrap();
                    let re1=fine["fields"][field]["real"][other].as_f64().unwrap();
                    let im1=fine["fields"][field]["imag"][other].as_f64().unwrap();
                    squared+=(re0-re1).powi(2)+(im0-im1).powi(2);count+=1;
                }
            }
            (squared/count as f64).sqrt()
        };
        let coarse_medium=error_at_shared_nodes(&solutions[0].0,&solutions[0].1,&solutions[1].0,&solutions[1].1);
        let medium_fine=error_at_shared_nodes(&solutions[1].0,&solutions[1].1,&solutions[2].0,&solutions[2].1);
        let fine_norm=["Ex","Ey","Ez","Hx","Hy","Hz"].into_iter().flat_map(|field|
            solutions[2].1["fields"][field]["real"].as_array().unwrap().iter().filter_map(Value::as_f64))
            .map(f64::abs).fold(0.,f64::max);
        assert!(coarse_medium.is_finite()&&coarse_medium>0.,
            "heterogeneous transient comparison is degenerate: coarse/medium={coarse_medium}, medium/fine={medium_fine}, fine field max={fine_norm}");
        assert!(medium_fine<coarse_medium,
            "Full finite-kz heterogeneous transient difference should decrease under refinement: coarse/medium={coarse_medium}, medium/fine={medium_fine}");
    }

    #[test]
    fn scenario_operators_obey_physical_energy_and_heat_balances() {
        let profiles: [(&str, &str, &[&str]); 8] = [
            ("acoustic-2d", include_str!("scenarios/acoustic2d.json"), &["pressure"]),
            ("acoustic-3d", include_str!("scenarios/acoustic3d.json"), &["pressure"]),
            ("elastic-2d", include_str!("scenarios/elastic2d.json"), &["planeStress", "planeStrain"]),
            ("elastic-3d", include_str!("scenarios/elastic3d.json"), &["displacement"]),
            ("em-2d", include_str!("scenarios/em2d.json"), &["Ez", "Hz", "Full"]),
            ("em-3d", include_str!("scenarios/em3d.json"), &["electric"]),
            ("thermal-2d", include_str!("scenarios/thermal2d.json"), &["T"]),
            ("thermal-3d", include_str!("scenarios/thermal3d.json"), &["T"]),
        ];
        let options = LinearOptions { tolerance:1e-11, backend:"faer", ordering:"amd", threads:1 };
        for (profile, package_text, forms) in profiles {
            let package:Value=serde_json::from_str(package_text).unwrap();
            let dimension=package["schemaDocument"]["x-fem-scenario"]["profile"]["dimension"].as_u64().unwrap() as usize;
            let mesh=unstructured_simplex_mesh(dimension, if dimension==2 {2} else {2});
            let material=scenario_energy_material(profile);
            let mut material_row=json!({"id":"material","selectionId":"domain"});
            material_row.as_object_mut().unwrap().extend(material.as_object().unwrap().clone());
            let model=json!({"scenario":{"profileId":profile},"model":{
                "parameters":[],"mesh":{"periodicPairIds":[]},"materials":[material_row],
                "physics":{"id":"physics"},
                "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
            let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
                "boundarySelections":[],"domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
            let tags=HashMap::from([
                ("materials".into(),vec![7]),("domainSources".into(),Vec::new()),("boundaries".into(),Vec::new()),
            ]);
            for &form in forms {
                let mut selected_model=model.clone();selected_model["model"]["physics"]["formulation"]=json!(form);
                let bound=crate::transient_case::BoundTransientCase::prepare(&package,profile,form,&selected_model,&map,&tags,&json!({}))
                    .unwrap_or_else(|error|panic!("{profile}/{form}: {error}"));
                let edge_orderings: &[u8]=if profile=="em-3d" { &[1,2] } else { &[0] };
                for &order in edge_orderings {
                    let edge_mesh_storage=if order==0 {None} else {
                        let tetrahedra=mesh.cells.iter().map(|cell| {
                            (cell.nodes.as_slice().try_into().expect("tetrahedral simplex"),cell.tag)
                        }).collect::<Vec<([usize;4],usize)>>();
                        Some(crate::transient_edge::EdgeTransientMesh::prepare(&mesh.nodes,&tetrahedra,order)
                            .unwrap_or_else(|error|panic!("{profile}/{form}/{order}: {error}")))
                    };
                    let edge_mesh=edge_mesh_storage.as_ref();
                    let triangle_mesh_storage=if profile=="em-2d"&&form=="Full" {
                        let cells=mesh.cells.iter().map(|cell|(cell.nodes.as_slice().try_into().expect("triangle simplex"),cell.tag))
                            .collect::<Vec<([usize;3],usize)>>();
                        Some(crate::transient_triangle::TriangleTransientMesh::prepare(&mesh.nodes,&cells)
                            .unwrap_or_else(|error|panic!("{profile}/{form}: {error}")))
                    } else {None};
                    let triangle_mesh=triangle_mesh_storage.as_ref();
                    let counts=bound.plan.fields.iter().map(|field|edge_mesh.map_or_else(
                        ||triangle_mesh.map_or(mesh.nodes.len()*field.components,|triangle|triangle.dof_count()),
                        |edge|edge.dof_count())).collect::<Vec<_>>();
                    let layout=crate::transient_plan::StateLayout::new(&bound.plan,&counts).unwrap();
                    let mut offsets=Vec::with_capacity(counts.len());let mut offset=0;
                    for count in &counts {offsets.push(offset);offset+=count;}
                    let mut initial=vec![crate::solver_common::C::default();offset];
                    for (field_index,field) in bound.plan.fields.iter().enumerate() {
                        let amplitude=if profile.starts_with("em-") {
                            if field.id=="electric"||field.id=="electricZ" {1.} else {1.}
                        } else if field.id=="velocity" {0.1} else {1.};
                        for local in 0..counts[field_index] {
                            let phase=(local as f64+1.)*1.371+(field_index as f64)*0.619;
                            initial[offsets[field_index]+local].re=amplitude*(phase.sin()+0.25*phase.cos());
                        }
                    }
                    let parameters=HashMap::new();
                    let mut workspace=crate::transient_plan::TransientAssemblyWorkspace::default();
                    let assembled=assemble_energy_probe(&bound,&mesh,edge_mesh,triangle_mesh,&layout,&mut workspace,&parameters,0.)
                        .unwrap_or_else(|error|panic!("{profile}/{form}/{order}: {error}"));
                    let grid=crate::transient_request::TimeGrid{start:0.,end:0.4,max_step:0.1,max_steps:4,output_times:vec![0.1,0.2,0.3,0.4]};
                    let mut states=Vec::new();
                    crate::transient_step::integrate_backward_euler(0.,grid.end,grid.max_step,grid.max_steps,&initial,&grid.output_times,
                        &options,&mut SolverCache::default(),|time|assemble_energy_probe(&bound,&mesh,edge_mesh,triangle_mesh,&layout,&mut workspace,&parameters,time),
                        |_,state,_|{states.push(state.to_vec());Ok(())})
                        .unwrap_or_else(|error|panic!("{profile}/{form}/{order}: {error}"));
                    assert_eq!(states.len(),5,"{profile}/{form}/{order}");
                    let label=format!("{profile}/{form}/{order}");
                    match profile {
                        "thermal-2d"|"thermal-3d"=>{
                            let mass=&assembled.derivatives[0].1;let operator=&assembled.operator;
                            let temperature_offset=offsets[0];let n=counts[0];
                            let heat=|state:&[crate::solver_common::C]| {
                                let mut ones=vec![crate::solver_common::C::default();state.len()];
                                for value in &mut ones[temperature_offset..temperature_offset+n] {value.re=1.;}
                                sparse_bilinear(&ones,mass,state)
                            };
                            for pair in states.windows(2) {
                                let previous=&pair[0];let current=&pair[1];
                                let delta=previous.iter().zip(current).map(|(a,b)|crate::solver_common::C{re:b.re-a.re,im:0.}).collect::<Vec<_>>();
                                let e0=0.5*sparse_quadratic(mass,previous);let e1=0.5*sparse_quadratic(mass,current);
                                let decay=0.5*sparse_quadratic(mass,&delta)+0.1*sparse_quadratic(operator,current);
                                let scale=(e0.abs()+e1.abs()+decay.abs()).max(1e-20);
                                assert!(e0>0. && decay>=-2e-8*scale,"{label}: nonphysical thermal energy or dissipation");
                                assert!((e0-e1-decay).abs()<=2e-8*scale,"{label}: thermal backward-Euler energy residual {}, scale {scale}",e0-e1-decay);
                                assert!((heat(previous)-heat(current)).abs()<=2e-9*heat(previous).abs().max(1e-20),"{label}: insulated heat changed");
                            }
                        }
                        "elastic-2d"|"elastic-3d"=>{
                            let displacement_index=bound.plan.fields.iter().position(|field|field.id=="displacement").unwrap();
                            let velocity_index=bound.plan.fields.iter().position(|field|field.id=="velocity").unwrap();
                            let u0=offsets[displacement_index];let v0=offsets[velocity_index];let n=counts[displacement_index];
                            let density_mass=&assembled.derivatives.iter().find(|(law,_)|*law==crate::transient_step::TimeDerivative::ConservedProduct).unwrap().1;
                            let energy=|state:&[crate::solver_common::C]| {
                                let u=&state[u0..u0+n];
                                0.5*(elastic_stiffness_energy(&assembled.operator,u,u0,v0)+sparse_quadratic(density_mass,state))
                            };
                            for pair in states.windows(2) {
                                let previous=&pair[0];let current=&pair[1];
                                let delta=previous.iter().zip(current).map(|(a,b)|crate::solver_common::C{re:b.re-a.re,im:0.}).collect::<Vec<_>>();
                                let du=delta[u0..u0+n].to_vec();
                                let e0=energy(previous);let e1=energy(current);
                                let decay=0.5*(elastic_stiffness_energy(&assembled.operator,&du,u0,v0)+sparse_quadratic(density_mass,&delta));
                                let scale=(e0.abs()+e1.abs()+decay.abs()).max(1e-20);
                                assert!(e0>0. && decay>=-2e-8*scale,"{label}: nonphysical elastic energy or dissipation");
                                assert!((e0-e1-decay).abs()<=2e-8*scale,"{label}: elastic energy residual {}, scale {scale}",e0-e1-decay);
                            }
                        }
                        _=>{
                            let mass=add_sparse_matrices(&assembled.derivatives.iter().map(|(_,matrix)|matrix.clone()).collect::<Vec<_>>(),initial.len());
                            for pair in states.windows(2) {
                                let previous=&pair[0];let current=&pair[1];
                                let delta=previous.iter().zip(current).map(|(a,b)|crate::solver_common::C{re:b.re-a.re,im:0.}).collect::<Vec<_>>();
                                let e0=0.5*sparse_quadratic(&mass,previous);let e1=0.5*sparse_quadratic(&mass,current);
                                let decrement=0.5*sparse_quadratic(&mass,&delta);
                                let scale=(e0.abs()+e1.abs()+decrement.abs()).max(1e-20);
                                assert!(e0>0. && decrement>0.,"{label}: expected nonzero positive wave energy and backward-Euler dissipation");
                                assert!((e0-e1-decrement).abs()<=2e-8*scale,"{label}: wave energy residual {}, scale {scale}",e0-e1-decrement);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn em2d_full_transient_obeys_backward_euler_energy_balance() {
        let profile="em-2d";
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let mesh=unstructured_simplex_mesh(2,2);
        let material=scenario_energy_material(profile);
        let mut material_row=json!({"id":"material","selectionId":"domain"});
        material_row.as_object_mut().unwrap().extend(material.as_object().unwrap().clone());
        let model=json!({"scenario":{"profileId":profile},"model":{
            "parameters":[],"mesh":{"periodicPairIds":[]},"materials":[material_row],
            "physics":{"id":"physics","formulation":"Full"},
            "study":[{"id":"study","type":"transient","physicsId":"physics"}]}});
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":[],"domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let tags=HashMap::from([("materials".into(),vec![7]),("domainSources".into(),Vec::new()),("boundaries".into(),Vec::new())]);
        let bound=crate::transient_case::BoundTransientCase::prepare(&package,profile,"Full",&model,&map,&tags,&json!({})).unwrap();
        let cells=mesh.cells.iter().map(|cell|(cell.nodes.as_slice().try_into().expect("triangle simplex"),cell.tag))
            .collect::<Vec<([usize;3],usize)>>();
        let triangle_mesh=crate::transient_triangle::TriangleTransientMesh::prepare(&mesh.nodes,&cells).unwrap();
        let counts=bound.plan.fields.iter().map(|_|triangle_mesh.dof_count()).collect::<Vec<_>>();
        let layout=crate::transient_plan::StateLayout::new(&bound.plan,&counts).unwrap();
        let mut offsets=Vec::with_capacity(counts.len());let mut offset=0;
        for count in &counts{offsets.push(offset);offset+=count;}
        let mut initial=vec![crate::solver_common::C::default();offset];
        for (field_index,count) in counts.iter().enumerate(){
            for local in 0..*count{
                let phase=(local as f64+1.)*1.371+(field_index as f64)*0.619;
                initial[offsets[field_index]+local].re=phase.sin()+0.25*phase.cos();
            }
        }
        let parameters=HashMap::new();let mut workspace=crate::transient_plan::TransientAssemblyWorkspace::default();
        let assembled=assemble_energy_probe(&bound,&mesh,None,Some(&triangle_mesh),&layout,&mut workspace,&parameters,0.).unwrap();
        let grid=crate::transient_request::TimeGrid{start:0.,end:0.4,max_step:0.1,max_steps:4,output_times:vec![0.1,0.2,0.3,0.4]};
        let options=LinearOptions{tolerance:1e-11,backend:"faer",ordering:"amd",threads:1};
        let mut states=Vec::new();
        crate::transient_step::integrate_backward_euler(0.,grid.end,grid.max_step,grid.max_steps,&initial,&grid.output_times,
            &options,&mut SolverCache::default(),|time|assemble_energy_probe(&bound,&mesh,None,Some(&triangle_mesh),&layout,&mut workspace,&parameters,time),
            |_,state,_|{states.push(state.to_vec());Ok(())}).unwrap();
        let mass=add_sparse_matrices(&assembled.derivatives.iter().map(|(_,matrix)|matrix.clone()).collect::<Vec<_>>(),initial.len());
        for pair in states.windows(2){
            let previous=&pair[0];let current=&pair[1];
            let delta=previous.iter().zip(current).map(|(a,b)|crate::solver_common::C{re:b.re-a.re,im:0.}).collect::<Vec<_>>();
            let e0=0.5*sparse_quadratic(&mass,previous);let e1=0.5*sparse_quadratic(&mass,current);
            let decrement=0.5*sparse_quadratic(&mass,&delta);let scale=(e0.abs()+e1.abs()+decrement.abs()).max(1e-20);
            assert!(e0>0.&&decrement>0.,"Full Maxwell energy and backward-Euler dissipation should be positive");
            assert!((e0-e1-decrement).abs()<=2e-8*scale,"Full Maxwell backward-Euler energy residual {} at scale {scale}",e0-e1-decrement);
        }
    }

    #[test]
    fn thermal_manufactured_solution_converges_spatially_in_both_dimensions() {
        for (profile, dimension, package_text) in [
            ("thermal-2d", 2, include_str!("scenarios/thermal2d.json")),
            ("thermal-3d", 3, include_str!("scenarios/thermal3d.json")),
        ] {
            let package: Value = serde_json::from_str(package_text).unwrap();
            let factors = ["x", "y", "z"]
                .into_iter()
                .take(dimension)
                .map(|axis| format!("({axis}/L)*(1-{axis}/L)"))
                .collect::<Vec<_>>();
            let phi = factors.join("*");
            let laplacian_terms = (0..dimension)
                .map(|axis| {
                    factors
                        .iter()
                        .enumerate()
                        .filter_map(|(factor_axis, factor)| (factor_axis != axis).then_some(factor.clone()))
                        .collect::<Vec<_>>()
                        .join("*")
                })
                .collect::<Vec<_>>()
                .join("+");
            let source = format!(
                "C0*rate*({phi})+k0*t*rate*2/(L^2)*({laplacian_terms})"
            );
            let dimensions = json!({
                "L": crate::model_expression::unit("m").unwrap().1,
                "k0": crate::model_expression::unit("W/(m*K)").unwrap().1,
                "C0": crate::model_expression::unit("W*s/(m^3*K)").unwrap().1,
                "rate": crate::model_expression::unit("K/s").unwrap().1,
            });
            let case = PreparedTransientCase::from_json(&json!({
                "time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":1.,"maxSteps":1,"outputTimesSeconds":[1.]},
                "globalParameters":{"L":1.,"k0":1.,"C0":1.,"rate":1.},
                "globalParameterDimensions":dimensions,
                "parameterOverrides":[]
            })).unwrap();
            let model = json!({"scenario":{"profileId":profile},"model":{
                "parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[{"id":"material","selectionId":"domain",
                    "thermalConductivity":{"expression":"k0","unit":"W/(m*K)"},
                    "volumetricHeatCapacity":{"expression":"C0","unit":"W*s/(m^3*K)"}}],
                "physics":{"id":"physics","formulation":"T",
                    "source":{"id":"source","type":"domainSource","selectionId":"domain",
                        "values":[{"expression":source,"unit":"W/m^3"}]},
                    "wall":{"id":"wall","type":"dirichlet","selectionId":"wall",
                        "value":{"expression":"0","unit":"K"}}},
                "study":[{"id":"study","type":"transient","physicsId":"physics"}]
            }});
            let options = LinearOptions { tolerance: 1e-10, backend: "faer", ordering: "amd", threads: 1 };
            for unstructured in [false,true] {
            let mut errors = Vec::new();
            for divisions in [2, 4, 8] {
                let mesh = if unstructured {unstructured_simplex_mesh(dimension, divisions)} else {structured_simplex_mesh(dimension, divisions)};
                let map = json!({
                    "physicalGroups":[{"selectionId":"domain","physicalTag":7}],
                    "boundarySelections":[{"id":"wall","physicalTag":8}],
                    "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}
                });
                let mut result = None;
                run_case(&package, &model, &map, &mesh, "study", "T", 1, 0, &case,
                    "standard", &options, &mut SolverCache::default(), |_, snapshot| {
                        if let Some((_, value)) = snapshot {
                            result = Some(value["fields"]["T"]["real"].as_array().unwrap()
                                .iter().map(|value| value.as_f64().unwrap()).collect::<Vec<_>>());
                        }
                        Ok(())
                    }).unwrap_or_else(|error| panic!("{profile}/{unstructured}/{divisions}: {error}"));
                let values = result.unwrap();
                let squared_error = mesh.nodes.iter().zip(values).map(|(point, actual)| {
                    let exact = point.iter().take(dimension).fold(1.0, |product, coordinate| {
                        product * coordinate * (1. - coordinate)
                    });
                    (actual - exact).powi(2)
                }).sum::<f64>();
                errors.push((squared_error / mesh.nodes.len() as f64).sqrt());
            }
            assert!(errors[0] > errors[1] && errors[1] > errors[2], "{profile}/{unstructured}: {errors:?}");
            assert!(errors[0] / errors[2] > 2.5, "{profile}/{unstructured}: {errors:?}");
            }
        }
    }

    #[test]
    fn acoustic_manufactured_solution_converges_spatially_in_both_dimensions() {
        for (profile, dimension, package_text) in [
            ("acoustic-2d", 2, include_str!("scenarios/acoustic2d.json")),
            ("acoustic-3d", 3, include_str!("scenarios/acoustic3d.json")),
        ] {
            let package: Value = serde_json::from_str(package_text).unwrap();
            let factors = ["x", "y", "z"]
                .into_iter()
                .take(dimension)
                .map(|axis| format!("cos(pi*{axis}/L)"))
                .collect::<Vec<_>>();
            let phi = factors.join("*");
            let source = format!("pAmp*t*{dimension}*pi^2/(rho0*L^2)*({phi})");
            let dimensions = json!({
                "L": crate::model_expression::unit("m").unwrap().1,
                "pAmp": crate::model_expression::unit("Pa").unwrap().1,
                "rho0": crate::model_expression::unit("kg/m^3").unwrap().1,
            });
            let initial_velocity = (0..dimension)
                .map(|_| json!({"expression":"0","unit":"m/s"}))
                .collect::<Vec<_>>();
            let case = PreparedTransientCase::from_json(&json!({
                "time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":1.,"maxSteps":1,"outputTimesSeconds":[1.]},
                "globalParameters":{"L":1.,"pAmp":1.,"rho0":1.},
                "globalParameterDimensions":dimensions,
                "parameterOverrides":[]
            })).unwrap();
            let model = json!({"scenario":{"profileId":profile},"model":{
                "parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[{"id":"material","selectionId":"domain",
                    "density":{"expression":"rho0","unit":"kg/m^3"},
                    "soundSpeed":{"expression":"1","unit":"m/s"}}],
                "physics":{"id":"physics","formulation":"pressure",
                    "source":{"id":"source","type":"domainSource","selectionId":"domain",
                        "values":[{"expression":source,"unit":"Hz"}]},
                    "wall":{"id":"wall","type":"neumann","selectionId":"wall",
                        "value":{"expression":"0","unit":"m/s"}}},
                "study":[{"id":"study","type":"transient","physicsId":"physics",
                    "initialFields":{"pressure":[{"expression":format!("pAmp*({phi})"),"unit":"Pa"}],
                        "velocity":initial_velocity}}]
            }});
            let options = LinearOptions { tolerance: 1e-10, backend: "faer", ordering: "amd", threads: 1 };
            for unstructured in [false,true] {
            let mut errors = Vec::new();
            for divisions in [2, 4, 8] {
                let mesh = if unstructured {unstructured_simplex_mesh(dimension, divisions)} else {structured_simplex_mesh(dimension, divisions)};
                let map = json!({
                    "physicalGroups":[{"selectionId":"domain","physicalTag":7}],
                    "boundarySelections":[{"id":"wall","physicalTag":8}],
                    "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}
                });
                let mut result = None;
                run_case(&package, &model, &map, &mesh, "study", "pressure", 1, 0, &case,
                    "standard", &options, &mut SolverCache::default(), |_, snapshot| {
                        if let Some((_, value)) = snapshot {
                            result = Some(value["fields"]["p"]["real"].as_array().unwrap()
                                .iter().map(|value| value.as_f64().unwrap()).collect::<Vec<_>>());
                        }
                        Ok(())
                    }).unwrap_or_else(|error| panic!("{profile}/{unstructured}/{divisions}: {error}"));
                let values = result.unwrap();
                let squared_error = mesh.nodes.iter().zip(values).map(|(point, actual)| {
                    let exact = point.iter().take(dimension).fold(1.0, |product, coordinate| {
                        product * (std::f64::consts::PI * coordinate).cos()
                    });
                    (actual - exact).powi(2)
                }).sum::<f64>();
                errors.push((squared_error / mesh.nodes.len() as f64).sqrt());
            }
            assert!(errors[0] > errors[1] && errors[1] > errors[2], "{profile}/{unstructured}: {errors:?}");
            assert!(errors[0] / errors[2] > 2.0, "{profile}/{unstructured}: {errors:?}");
            }
        }
    }

    #[test]
    fn elastic_manufactured_solution_converges_spatially_in_all_reductions() {
        for (profile, dimension, package_text, formulations) in [
            ("elastic-2d", 2, include_str!("scenarios/elastic2d.json"), vec!["planeStress", "planeStrain"]),
            ("elastic-3d", 3, include_str!("scenarios/elastic3d.json"), vec!["displacement"]),
        ] {
            let package: Value = serde_json::from_str(package_text).unwrap();
            let factors = ["x", "y", "z"]
                .into_iter()
                .take(dimension)
                .map(|axis| format!("({axis}/L)*(1-{axis}/L)"))
                .collect::<Vec<_>>();
            let phi = factors.join("*");
            let second_derivative_terms = (0..dimension)
                .map(|axis| factors.iter().enumerate()
                    .filter_map(|(index, factor)| (index != axis).then_some(factor.clone()))
                    .collect::<Vec<_>>().join("*"))
                .collect::<Vec<_>>();
            let longitudinal = std::iter::once(format!("2*({})", second_derivative_terms[0]))
                .chain(second_derivative_terms.iter().skip(1).map(|term| format!("({term})")))
                .collect::<Vec<_>>()
                .join("+");
            let mut sources = vec![format!("t*rate*E0/L^2*({longitudinal})")];
            for axis in 1..dimension {
                let cross_factor = format!("(1-2*{}/L)*(1-2*{}/L)", ["x", "y", "z"][0], ["x", "y", "z"][axis]);
                let remaining = factors.iter().enumerate()
                    .filter_map(|(index, factor)| (index != 0 && index != axis).then_some(factor.clone()))
                    .collect::<Vec<_>>().join("*");
                let remaining_factor = if remaining.is_empty() { String::new() } else { format!("*({remaining})") };
                sources.push(format!("-t*rate*E0/(2*L^2)*{cross_factor}{remaining_factor}"));
            }
            let source_values = sources.iter()
                .map(|source| json!({"expression":source,"unit":"N/m^3"}))
                .collect::<Vec<_>>();
            let initial_velocity = (0..dimension)
                .map(|axis| json!({"expression":if axis == 0 {format!("rate*({phi})")} else {"0".into()},"unit":"m/s"}))
                .collect::<Vec<_>>();
            let zero_displacement = (0..dimension)
                .map(|_| json!({"expression":"0","unit":"m"}))
                .collect::<Vec<_>>();
            let dimensions = json!({
                "L": crate::model_expression::unit("m").unwrap().1,
                "rate": crate::model_expression::unit("m/s").unwrap().1,
                "E0": crate::model_expression::unit("Pa").unwrap().1,
                "rho0": crate::model_expression::unit("kg/m^3").unwrap().1,
            });
            let case = PreparedTransientCase::from_json(&json!({
                "time":{"startSeconds":0.,"endSeconds":1.,"maxStepSeconds":1.,"maxSteps":1,"outputTimesSeconds":[1.]},
                "globalParameters":{"L":1.,"rate":1.,"E0":1.,"rho0":1.},
                "globalParameterDimensions":dimensions,
                "parameterOverrides":[]
            })).unwrap();
            let options = LinearOptions { tolerance: 1e-10, backend: "faer", ordering: "amd", threads: 1 };
            for formulation in formulations {
                let model = json!({"scenario":{"profileId":profile},"model":{
                    "parameters":[],"mesh":{"periodicPairIds":[]},
                    "materials":[{"id":"material","selectionId":"domain",
                        "density":{"expression":"rho0","unit":"kg/m^3"},
                        "youngModulus":{"expression":"E0","unit":"Pa"},
                        "poissonRatio":"0"}],
                    "physics":{"id":"physics","formulation":formulation,
                        "source":{"id":"source","type":"domainSource","selectionId":"domain","values":source_values},
                        "wall":{"id":"wall","type":"dirichlet","selectionId":"wall",
                            "value":{"expression":"0","unit":"m"},"fieldValues":zero_displacement}},
                    "study":[{"id":"study","type":"transient","physicsId":"physics",
                        "initialFields":{"velocity":initial_velocity}}]
                }});
                for unstructured in [false,true] {
                let mut errors = Vec::new();
                for divisions in [2, 4, 8] {
                    let mesh = if unstructured {unstructured_simplex_mesh(dimension, divisions)} else {structured_simplex_mesh(dimension, divisions)};
                    let map = json!({
                        "physicalGroups":[{"selectionId":"domain","physicalTag":7}],
                        "boundarySelections":[{"id":"wall","physicalTag":8}],
                        "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}
                    });
                    let mut result = None;
                    run_case(&package, &model, &map, &mesh, "study", formulation, 1, 0, &case,
                        "standard", &options, &mut SolverCache::default(), |_, snapshot| {
                            if let Some((_, value)) = snapshot {
                                result = Some(value["fields"]["ux"]["real"].as_array().unwrap()
                                    .iter().map(|value| value.as_f64().unwrap()).collect::<Vec<_>>());
                            }
                            Ok(())
                        }).unwrap_or_else(|error| panic!("{profile}/{formulation}/{unstructured}/{divisions}: {error}"));
                    let values = result.unwrap();
                    let squared_error = mesh.nodes.iter().zip(values).map(|(point, actual)| {
                        let exact = point.iter().take(dimension).fold(1.0, |product, coordinate| {
                            product * coordinate * (1. - coordinate)
                        });
                        (actual - exact).powi(2)
                    }).sum::<f64>();
                    errors.push((squared_error / mesh.nodes.len() as f64).sqrt());
                }
                assert!(errors[0] > errors[1] && errors[1] > errors[2], "{profile}/{formulation}/{unstructured}: {errors:?}");
                assert!(errors[0] / errors[2] > 2.0, "{profile}/{formulation}/{unstructured}: {errors:?}");
                }
            }
        }
    }

    #[test]
    fn electromagnetic_manufactured_gradient_converges_spatially_in_3d() {
        let profile = "em-3d";
        let dimension = 3;
        let package: Value = serde_json::from_str(include_str!("scenarios/em3d.json")).unwrap();
        let factors = ["x", "y", "z"]
            .into_iter()
            .map(|axis| format!("({axis}/L)*(1-{axis}/L)"))
            .collect::<Vec<_>>();
        let gradient = (0..dimension)
            .map(|axis| {
                let remaining = factors.iter().enumerate()
                    .filter_map(|(index, factor)| (index != axis).then_some(factor.clone()))
                    .collect::<Vec<_>>().join("*");
                format!("J0*(1-2*{}/L)*({remaining})", ["x", "y", "z"][axis])
            })
            .collect::<Vec<_>>();
        let source_values = gradient.iter()
            .map(|source| json!({"expression":source,"unit":"A/m^2"}))
            .collect::<Vec<_>>();
        let dt = 1e-9;
        // J0 = -epsilon_0 * V0 / (dt * L), so the declared -J
        // right-hand side advances E from zero to grad(V0*phi) in one step.
        let j0 = -8.8541878128e-12 / dt;
        let dimensions = json!({
            "L": crate::model_expression::unit("m").unwrap().1,
            "J0": crate::model_expression::unit("A/m^2").unwrap().1,
        });
        let case = PreparedTransientCase::from_json(&json!({
            "time":{"startSeconds":0.,"endSeconds":dt,"maxStepSeconds":dt,"maxSteps":1,"outputTimesSeconds":[dt]},
            "globalParameters":{"L":1.,"J0":j0},
            "globalParameterDimensions":dimensions,
            "parameterOverrides":[]
        })).unwrap();
        let model = json!({"scenario":{"profileId":profile},"model":{
            "parameters":[],"mesh":{"periodicPairIds":[]},
            "materials":[{"id":"material","selectionId":"domain",
                "relativePermittivity":"1","relativePermeability":"1",
                "conductivity":{"expression":"0","unit":"S/m"}}],
            "physics":{"id":"physics","formulation":"electric",
                "source":{"id":"source","type":"domainSource","selectionId":"domain","values":source_values},
                "wall":{"id":"wall","type":"dirichlet","selectionId":"wall",
                    "value":{"expression":"0","unit":"V/m"},
                    "fieldValues":[{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"}]}},
            "study":[{"id":"study","type":"transient","physicsId":"physics"}]
        }});
        let options = LinearOptions { tolerance: 1e-10, backend: "faer", ordering: "amd", threads: 1 };
        for unstructured in [false,true] {
        let mut errors = Vec::new();
        let refinements=if unstructured {vec![2,4,6]} else {vec![2,4,8]};
        for divisions in refinements {
            let mesh = if unstructured {unstructured_simplex_mesh(dimension, divisions)} else {structured_simplex_mesh(dimension, divisions)};
            let map = json!({
                "physicalGroups":[{"selectionId":"domain","physicalTag":7}],
                "boundarySelections":[{"id":"wall","physicalTag":8}],
                "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}
            });
            let mut result = None;
            run_case(&package, &model, &map, &mesh, "study", "electric", 1, 0, &case,
                "standard", &options, &mut SolverCache::default(), |_, snapshot| {
                    if let Some((_, value)) = snapshot {
                        result = Some((0..3).map(|axis| {
                            value["fields"][["Ex", "Ey", "Ez"][axis]]["real"].as_array().unwrap()
                                .iter().map(|value| value.as_f64().unwrap()).collect::<Vec<_>>()
                        }).collect::<Vec<_>>());
                    }
                    Ok(())
            }).unwrap_or_else(|error| panic!("{profile}/{unstructured}/{divisions}: {error}"));
            let values = result.unwrap();
            let squared_error = mesh.nodes.iter().enumerate().map(|(node, point)| {
                let exact = (0..dimension).map(|axis| {
                    let remaining = point.iter().enumerate()
                        .filter_map(|(index, coordinate)| (index != axis).then_some(coordinate * (1. - coordinate)))
                        .product::<f64>();
                    (1. - 2. * point[axis]) * remaining
                }).collect::<Vec<_>>();
                (0..dimension).map(|axis| (values[axis][node] - exact[axis]).powi(2)).sum::<f64>()
            }).sum::<f64>();
            errors.push((squared_error / mesh.nodes.len() as f64).sqrt());
        }
        assert!(errors[0] > errors[1] && errors[1] > errors[2], "{profile}/{unstructured}: {errors:?}");
        assert!(errors[0] / errors[2] > 1.5, "{profile}/{unstructured}: {errors:?}");
        }
    }

    #[test]
    fn electromagnetic_manufactured_solutions_converge_spatially_for_both_2d_forms() {
        let profile = "em-2d";
        let package: Value = serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let fx = "(x/L)*(1-x/L)";
        let fy = "(y/L)*(1-y/L)";
        let phi = format!("({fx})*({fy})");
        let dt = 1e-9;
        let options = LinearOptions { tolerance: 1e-10, backend: "faer", ordering: "amd", threads: 1 };
        for (formulation, output, source_unit, source, dimensions, parameters) in [
            (
                "Ez", "Ez", "A/m^2",
                format!("J0*({phi})+J2*(t/duration)^2*(({fx})+({fy}))"),
                json!({"L":crate::model_expression::unit("m").unwrap().1,
                    "J0":crate::model_expression::unit("A/m^2").unwrap().1,
                    "J2":crate::model_expression::unit("A/m^2").unwrap().1,
                    "duration":crate::model_expression::unit("s").unwrap().1}),
                json!({"L":1.,"duration":dt,"J0":-8.8541878128e-12/dt,"J2":-dt/1.25663706212e-6}),
            ),
            (
                "Hz", "Hz", "V/m^2",
                format!("M0*({phi})+M2*(t/duration)^2*(({fx})+({fy}))"),
                json!({"L":crate::model_expression::unit("m").unwrap().1,
                    "M0":crate::model_expression::unit("V/m^2").unwrap().1,
                    "M2":crate::model_expression::unit("V/m^2").unwrap().1,
                    "duration":crate::model_expression::unit("s").unwrap().1}),
                json!({"L":1.,"duration":dt,"M0":-1.25663706212e-6/dt,"M2":-dt/8.8541878128e-12}),
            ),
        ] {
            let case = PreparedTransientCase::from_json(&json!({
                "time":{"startSeconds":0.,"endSeconds":dt,"maxStepSeconds":dt,"maxSteps":1,"outputTimesSeconds":[dt]},
                "globalParameters":parameters,
                "globalParameterDimensions":dimensions,
                "parameterOverrides":[]
            })).unwrap();
            let model = json!({"scenario":{"profileId":profile},"model":{
                "parameters":[],"mesh":{"periodicPairIds":[]},
                "materials":[{"id":"material","selectionId":"domain",
                    "relativePermittivity":"1","relativePermeability":"1",
                    "conductivity":{"expression":"0","unit":"S/m"}}],
                "physics":{"id":"physics","formulation":formulation,
                    "source":{"id":"source","type":"domainSource","selectionId":"domain",
                        "values":[{"expression":source,"unit":source_unit}]},
                    "wall":{"id":"wall","type":"dirichlet","selectionId":"wall",
                        "value":{"expression":"0","unit":if formulation == "Ez" {"V/m"} else {"A/m"}}}},
                "study":[{"id":"study","type":"transient","physicsId":"physics"}]
            }});
            for unstructured in [false,true] {
            let mut results = Vec::new();
            for divisions in [2, 4, 8, 16] {
                let mesh = if unstructured {unstructured_simplex_mesh(2, divisions)} else {structured_simplex_mesh(2, divisions)};
                let map = json!({
                    "physicalGroups":[{"selectionId":"domain","physicalTag":7}],
                    "boundarySelections":[{"id":"wall","physicalTag":8}],
                    "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}
                });
                let mut result = None;
                run_case(&package, &model, &map, &mesh, "study", formulation, 1, 0, &case,
                    "standard", &options, &mut SolverCache::default(), |_, snapshot| {
                        if let Some((_, value)) = snapshot {
                            result = Some(value["fields"][output]["real"].as_array().unwrap()
                                .iter().map(|value| value.as_f64().unwrap()).collect::<Vec<_>>());
                        }
                        Ok(())
                    }).unwrap_or_else(|error| panic!("{profile}/{formulation}/{unstructured}/{divisions}: {error}"));
                results.push((divisions, mesh.nodes.clone(), result.unwrap()));
            }
            let (_, reference_nodes, reference) = results.last().unwrap();
            let errors = results[..results.len() - 1].iter().map(|(_, nodes, values)| {
                let squared_error = nodes.iter().zip(values).map(|(point,value)| {
                    let (nearest,_) = reference_nodes.iter().enumerate().map(|(index,candidate)| {
                        let distance=(0..2).map(|axis|(point[axis]-candidate[axis]).powi(2)).sum::<f64>();
                        (index,distance)
                    }).min_by(|left,right|left.1.total_cmp(&right.1)).unwrap();
                    (value-reference[nearest]).powi(2)
                }).sum::<f64>();
                (squared_error / nodes.len() as f64).sqrt()
            }).collect::<Vec<_>>();
            assert!(errors[0] > errors[1] && errors[1] > errors[2], "{formulation}/{unstructured}: {errors:?}");
            assert!(errors[0] / errors[2] > 1.5, "{formulation}/{unstructured}: {errors:?}");
            }
        }
    }
}
