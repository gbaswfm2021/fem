//! Execute one prepared eigenfrequency parameter case through the common
//! transient-form, boundary, sparse-pair and result contracts.
use crate::solver_common::{C,SolverCache};
use crate::transient_execution::MeshData;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path,PathBuf};

fn resolve_request_path(request:&Value,base:&Path,key:&str)->Result<PathBuf,String>{
    let raw=request.get(key).and_then(Value::as_str).filter(|value|!value.is_empty())
        .ok_or_else(||format!("Eigenfrequency request {key} is required."))?;
    let path=PathBuf::from(raw);Ok(if path.is_absolute(){path}else{base.join(path)})
}

pub(crate) fn solve_case(
    package:&Value,model:&Value,map:&Value,mesh:&MeshData,study_id:&str,formulation:&str,order:u8,
    case_index:usize,case:&Value,settings:&Value,field_files:bool,
)->Result<Vec<Value>,String>{
    let profile=package.get("id").and_then(Value::as_str).ok_or("Eigenfrequency package ID is missing.")?;
    if model.pointer("/scenario/profileId").and_then(Value::as_str)!=Some(profile){return Err("Eigenfrequency model and scenario package differ.".into());}
    let study=model.pointer("/model/study").and_then(Value::as_array).and_then(|rows|rows.iter().find(|row|row.get("id").and_then(Value::as_str)==Some(study_id)))
        .filter(|row|row.get("type").and_then(Value::as_str)==Some("eigenfrequency")).ok_or("Selected eigenfrequency Study is missing or has the wrong type.")?;
    let physics=model.pointer("/model/physics").ok_or("Eigenfrequency Physics is required.")?;
    if study.get("physicsId").and_then(Value::as_str)!=physics.get("id").and_then(Value::as_str)||physics.get("formulation").and_then(Value::as_str)!=Some(formulation){
        return Err("Eigenfrequency Study, Physics and formulation do not agree.".into());
    }
    let parameters=crate::study_case_parameters::resolve(model,case,"Eigenfrequency")?;
    let longitudinal_wavenumber=crate::longitudinal::study_wavenumber(
        study,&parameters,&format!("Eigenfrequency Study {study_id}.longitudinalWavenumber"),
    )?;
    crate::longitudinal::validate_prepared_case_value(case,longitudinal_wavenumber)?;
    let shift=case.get("shift").ok_or("Eigenfrequency case temporal shift is missing.")?;
    let target=C{re:shift.get("realPerSecond").and_then(Value::as_f64).filter(|value|value.is_finite()).ok_or("Eigenfrequency real shift must be finite.")?,
        im:shift.get("imaginaryPerSecond").and_then(Value::as_f64).filter(|value|value.is_finite()).ok_or("Eigenfrequency imaginary shift must be finite.")?};
    let setting_text=|key:&str|settings.get(key).and_then(Value::as_str).filter(|value|!value.is_empty()).ok_or_else(||format!("Eigenfrequency solver setting {key} is required."));
    let quadrature=setting_text("constitutiveQuadrature")?;let backend=setting_text("solverBackend")?;let ordering=setting_text("solverOrdering")?;
    let threads=settings.get("solverThreads").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).filter(|value|*value>0).ok_or("Eigenfrequency solverThreads must be positive.")?;
    let tolerance=settings.get("residualTolerance").and_then(Value::as_f64).filter(|value|value.is_finite()&&*value>0.&&*value<1.).ok_or("Eigenfrequency residualTolerance must be between 0 and 1.")?;
    let mode_count=settings.get("modeCount").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).filter(|value|*value>0).ok_or("Eigenfrequency modeCount must be positive.")?;
    let krylov=settings.get("krylovDimension").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).filter(|value|*value>=mode_count).ok_or("Eigenfrequency krylovDimension must be at least modeCount.")?;
    let dimension=package.pointer("/schemaDocument/x-fem-scenario/profile/dimension").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).ok_or("Eigenfrequency profile dimension is missing.")?;
    if mesh.dimension!=dimension{return Err("Eigenfrequency mesh and scenario dimensions differ.".into());}
    if mesh.integration_edges.len()>0 { /* retained for shared output integrations; not part of the modal pencil */ }
    let cell_tags=mesh.cells.iter().map(|cell|cell.tag).collect::<Vec<_>>();
    let facet_tags=mesh.facets.iter().map(|(_,tag)|*tag).collect::<Vec<_>>();
    let tags=HashMap::from([("materials".into(),cell_tags.clone()),("domainSources".into(),cell_tags),("boundaries".into(),facet_tags)]);
    let modal_model=crate::transient_case::eigenfrequency_model(package,formulation,model)?;
    let mut bound=crate::transient_case::BoundTransientCase::prepare_eigenfrequency(package,profile,formulation,&modal_model,map,&tags)?;
    bound.plan.set_longitudinal_wavenumber(longitudinal_wavenumber)?;
    let declarations=crate::model_schema::field_variables_for_profile(profile,Some(formulation),false)?;
    let mut modes=Vec::new();
    let execution=bound.plan.element_space.as_deref()
        .or_else(||package.pointer("/schemaDocument/x-fem-scenario/execution/kind").and_then(Value::as_str))
        .ok_or("Eigenfrequency element-space adapter is missing.")?;
    match execution {
        "scalarSimplexP1"|"vectorSimplexP1"=>{
            if order!=1{return Err("The shared eigenfrequency nodal adapter currently supports element order 1 only.".into());}
            let nodal=crate::transient_mesh::NodalMesh{dimension:mesh.dimension,nodes:&mesh.nodes,cells:&mesh.cells};
            let periodic=crate::transient_periodic::nodal_with_parameters(&bound,&parameters,&modal_model,map,&nodal,&mesh.facets)?;
            let fixed=periodic.as_fixed();
            let (layout,mut operators)=bound.assemble_eigen_nodal(&parameters,&nodal,quadrature)?;
            let boundary=bound.prepare_nodal_boundaries(&nodal,&mesh.facets)?;
            boundary.apply(&bound.plan,&layout,&nodal,&parameters,0.,&mut operators)?;
            let eigenmodes=crate::eigenfrequency::solve_transient_pencil(&operators,(!periodic.is_identity()).then_some(&fixed),target,
                mode_count,krylov,tolerance,backend,ordering,threads,&mut SolverCache::default())?;
            for (mode_index,mode) in eigenmodes.iter().enumerate(){
                if field_files{modes.push(bound.nodal_eigen_snapshot(&parameters,&nodal,&layout,&mode.vector,study_id,case_index,mode_index,
                    mode.eigenvalue,mode.relative_residual,&declarations)?);}else{modes.push(serde_json::json!({"schema":"fullmeta-eigenfrequency-mode-summary/1",
                    "studyId":study_id,"caseIndex":case_index,"modeIndex":mode_index,"temporalEigenvalue":{"realPerSecond":mode.eigenvalue.re,
                    "imaginaryPerSecond":mode.eigenvalue.im},"relativeResidual":mode.relative_residual}));}
            }
        }
        "orientedTetrahedronNedelec"=>{
            if mesh.dimension!=3||!matches!(order,1|2){return Err("Eigenfrequency oriented tetrahedra require dimension 3 and order 1 or 2.".into());}
            let cells=mesh.cells.iter().map(|cell|Ok((cell.nodes.clone().try_into().map_err(|_|"Eigenfrequency edge cell must be tetrahedral.")?,cell.tag)))
                .collect::<Result<Vec<([usize;4],usize)>,String>>()?;
            let facets=mesh.facets.iter().map(|(nodes,tag)|Ok((nodes.clone().try_into().map_err(|_|"Eigenfrequency edge facet must be triangular.")?,*tag)))
                .collect::<Result<Vec<([usize;3],usize)>,String>>()?;
            let edge=crate::transient_edge::EdgeTransientMesh::prepare(&mesh.nodes,&cells,order)?;
            let output_cells=mesh.cells.iter().map(|cell|(cell.nodes.clone(),cell.tag)).collect::<Vec<_>>();
            let periodic=crate::transient_periodic::edge_with_parameters(&bound,&parameters,&modal_model,map,&edge,&mesh.nodes,&output_cells,&mesh.facets)?;
            let fixed=periodic.as_fixed();
            let (layout,mut operators)=bound.assemble_eigen_edge(&parameters,&edge,quadrature)?;
            let boundary=bound.prepare_edge_boundaries(&edge,&facets)?;
            boundary.apply(&bound.plan,&layout,&edge,&parameters,0.,&mut operators)?;
            let eigenmodes=crate::eigenfrequency::solve_transient_pencil(&operators,(!periodic.is_identity()).then_some(&fixed),target,
                mode_count,krylov,tolerance,backend,ordering,threads,&mut SolverCache::default())?;
            for (mode_index,mode) in eigenmodes.iter().enumerate(){
                if field_files{modes.push(bound.edge_eigen_snapshot(&parameters,&edge,&mesh.nodes,&output_cells,&layout,&mode.vector,study_id,case_index,
                    mode_index,mode.eigenvalue,mode.relative_residual,&declarations)?);}else{modes.push(serde_json::json!({"schema":"fullmeta-eigenfrequency-mode-summary/1",
                    "studyId":study_id,"caseIndex":case_index,"modeIndex":mode_index,"temporalEigenvalue":{"realPerSecond":mode.eigenvalue.re,
                    "imaginaryPerSecond":mode.eigenvalue.im},"relativeResidual":mode.relative_residual}));}
            }
        }
        "orientedTriangleNedelecH1DirectSum"=>{
            if mesh.dimension!=2||order!=1{return Err("Eigenfrequency triangle Whitney–H1 direct-sum space requires a 2D mesh and element order 1.".into());}
            let cells=mesh.cells.iter().map(|cell|Ok((cell.nodes.clone().try_into()
                .map_err(|_|"Eigenfrequency triangle cell must have three vertices.")?,cell.tag)))
                .collect::<Result<Vec<([usize;3],usize)>,String>>()?;
            let triangles=crate::transient_triangle::TriangleTransientMesh::prepare(&mesh.nodes,&cells)?;
            let boundary_edges=mesh.facets.iter().map(|(nodes,tag)|Ok((nodes.as_slice().try_into()
                .map_err(|_|"Eigenfrequency triangle boundary facet must have two vertices.")?,*tag)))
                .collect::<Result<Vec<([usize;2],usize)>,String>>()?;
            let boundary=bound.prepare_triangle_boundaries(&triangles,&boundary_edges)?;
            let periodic=crate::transient_periodic::triangle(&bound,&parameters,&modal_model,map,&triangles,&mesh.nodes,&cells,&mesh.facets)?;
            let fixed=periodic.as_fixed();
            let output_cells=mesh.cells.iter().map(|cell|(cell.nodes.clone(),cell.tag)).collect::<Vec<_>>();
            let (layout,mut operators)=bound.assemble_eigen_oriented(&parameters,&triangles,quadrature)?;
            boundary.apply(&bound.plan,&layout,&triangles,&parameters,0.,&mut operators)?;
            let eigenmodes=crate::eigenfrequency::solve_transient_pencil(&operators,(!periodic.is_identity()).then_some(&fixed),target,mode_count,krylov,tolerance,
                backend,ordering,threads,&mut SolverCache::default())?;
            for (mode_index,mode) in eigenmodes.iter().enumerate(){
                if field_files{modes.push(bound.triangle_eigen_snapshot(&parameters,&triangles,&mesh.nodes,&output_cells,&layout,&mode.vector,
                    study_id,case_index,mode_index,mode.eigenvalue,mode.relative_residual,&declarations)?);}else{modes.push(serde_json::json!({
                    "schema":"fullmeta-eigenfrequency-mode-summary/1","studyId":study_id,"caseIndex":case_index,"modeIndex":mode_index,
                    "temporalEigenvalue":{"realPerSecond":mode.eigenvalue.re,"imaginaryPerSecond":mode.eigenvalue.im},"relativeResidual":mode.relative_residual}));}
            }
        }
        other=>return Err(format!("Unsupported eigenfrequency execution capability {other:?}.")),
    }
    if modes.len()!=mode_count{return Err("Eigenfrequency solve did not produce every requested mode.".into());}
    Ok(modes)
}

pub(crate) fn run_request(path:&Path,profile:&str)->Result<(),String>{
    let request:Value=serde_json::from_slice(&std::fs::read(path).map_err(|error|format!("Could not read eigenfrequency request: {error}"))?)
        .map_err(|error|format!("Invalid eigenfrequency request JSON: {error}"))?;
    if request.get("schema").and_then(Value::as_str)!=Some("fullmeta-eigenfrequency-case/1")||request.get("studyType").and_then(Value::as_str)!=Some("eigenfrequency")
        ||request.get("solver").and_then(Value::as_str)!=Some(profile){return Err("Eigenfrequency request schema, Study type or profile is invalid.".into());}
    let base=path.parent().unwrap_or(Path::new("."));
    let read_json=|key:&str|->Result<Value,String>{serde_json::from_slice(&std::fs::read(resolve_request_path(&request,base,key)?).map_err(|error|format!("Could not read eigenfrequency {key}: {error}"))?)
        .map_err(|error|format!("Invalid eigenfrequency {key} JSON: {error}"))};
    let model=read_json("modelFile")?;crate::preflight_client::validate(&model).map_err(|error|format!("Model validation:\n{error}"))?;
    let map=read_json("modelMapFile")?;
    let study_id=request.get("studyId").and_then(Value::as_str).filter(|value|!value.is_empty()).ok_or("Eigenfrequency request studyId is required.")?;
    let study=model.pointer("/model/study").and_then(Value::as_array).and_then(|rows|rows.iter().find(|row|row.get("id").and_then(Value::as_str)==Some(study_id))).ok_or("Eigenfrequency request Study is missing.")?;
    let settings=request.get("solverSettings").ok_or("Eigenfrequency solverSettings are required.")?;
    let formulation=settings.get("fieldFormulation").and_then(Value::as_str).ok_or("Eigenfrequency fieldFormulation is required.")?;
    let order=settings.get("elementOrder").and_then(Value::as_u64).and_then(|value|u8::try_from(value).ok()).ok_or("Eigenfrequency elementOrder is invalid.")?;
    let dimension=package_dimension(profile)?;
    let mesh_text=std::fs::read_to_string(resolve_request_path(&request,base,"meshFile")?).map_err(|error|format!("Could not read eigenfrequency mesh: {error}"))?;
    let mesh=MeshData::parse(dimension,&mesh_text)?;
    let case=request.get("case").ok_or("Eigenfrequency parameter case is required.")?;
    let case_index=request.get("caseIndex").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).ok_or("Eigenfrequency caseIndex is invalid.")?;
    let field_files=request.get("fieldFiles").and_then(Value::as_bool).ok_or("Eigenfrequency fieldFiles policy is required.")?;
    if study.pointer("/output/fieldFiles").and_then(Value::as_bool).is_some_and(|value|value!=field_files){return Err("Eigenfrequency field output policy differs from the model.".into());}
    let output=resolve_request_path(&request,base,"outputDirectory")?;std::fs::create_dir_all(&output).map_err(|error|format!("Could not create eigenfrequency output directory: {error}"))?;
    let package=crate::scenario_package::load_installed(profile)?;
    let modes=solve_case(&package,&model,&map,&mesh,study_id,formulation,order,case_index,case,settings,field_files)?;
    for (mode_index,document) in modes.iter().enumerate(){
        let name=format!("case-{case_index:06}-mode-{mode_index:06}.json");
        if field_files{let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(output.join(&name)).map_err(|error|format!("Could not create eigenfrequency mode artifact: {error}"))?;
            serde_json::to_writer(&mut file,document).map_err(|error|format!("Could not encode eigenfrequency mode: {error}"))?;use std::io::Write;file.flush().map_err(|error|format!("Could not flush eigenfrequency mode: {error}"))?;}
        println!("FULLMETA_EIGENFREQUENCY_MODE {}",serde_json::json!({"caseIndex":case_index,"modeIndex":mode_index,"name":if field_files{Some(name)}else{None},"temporalEigenvalue":document["temporalEigenvalue"],"relativeResidual":document["relativeResidual"]}));
    }
    Ok(())
}

fn package_dimension(profile:&str)->Result<usize,String>{crate::model_schema::scenario_metadata_for_profile(profile)?.pointer("/profile/dimension").and_then(Value::as_u64)
    .and_then(|value|usize::try_from(value).ok()).ok_or("Eigenfrequency profile dimension is invalid.".into())}

#[cfg(test)]
mod tests{
    use super::*;
    use serde_json::json;

    fn structured_modal_mesh(dimension:usize,divisions:usize)->MeshData{
        use crate::transient_mesh::NodalCell;
        let side=divisions+1;
        let node=|i:usize,j:usize,k:usize|(k*side+j)*side+i;
        let mut nodes=Vec::with_capacity(side.pow(dimension as u32));
        for k in 0..if dimension==3{side}else{1}{for j in 0..side{for i in 0..side{
            nodes.push([i as f64/divisions as f64,j as f64/divisions as f64,k as f64/divisions as f64]);
        }}}
        let mut cells=Vec::new();
        if dimension==2{
            for j in 0..divisions{for i in 0..divisions{
                let bl=node(i,j,0);let br=node(i+1,j,0);let tl=node(i,j+1,0);let tr=node(i+1,j+1,0);
                cells.push(NodalCell{nodes:vec![bl,br,tr],tag:7});cells.push(NodalCell{nodes:vec![bl,tr,tl],tag:7});
            }}
        }else{
            let offsets=[
                ([0,0,0],[1,0,0],[1,1,0],[1,1,1]),([0,0,0],[1,0,0],[1,0,1],[1,1,1]),
                ([0,0,0],[0,0,1],[1,0,1],[1,1,1]),([0,0,0],[0,0,1],[0,1,1],[1,1,1]),
                ([0,0,0],[0,1,0],[0,1,1],[1,1,1]),([0,0,0],[0,1,0],[1,1,0],[1,1,1]),
            ];
            for k in 0..divisions{for j in 0..divisions{for i in 0..divisions{for tet in offsets{
                let index=|o:[usize;3]|node(i+o[0],j+o[1],k+o[2]);
                cells.push(NodalCell{nodes:vec![index(tet.0),index(tet.1),index(tet.2),index(tet.3)],tag:7});
            }}}}
        }
        let mut owners=HashMap::<Vec<usize>,(usize,Vec<usize>)>::new();
        for cell in &cells{for excluded in 0..cell.nodes.len(){
            let facet=cell.nodes.iter().enumerate().filter_map(|(i,&value)|(i!=excluded).then_some(value)).collect::<Vec<_>>();
            let mut key=facet.clone();key.sort_unstable();
            owners.entry(key).and_modify(|entry|entry.0+=1).or_insert((1,facet));
        }}
        let facets=owners.into_values().filter_map(|(count,facet)|(count==1).then_some((facet,8))).collect();
        MeshData{dimension,nodes,cells,facets,integration_edges:Vec::new()}
    }

    fn eigen_settings(shift_real:f64,shift_imaginary:f64)->(Value,Value){
        let study=json!({"id":"study","type":"eigenfrequency","physicsId":"physics","eigenfrequency":{
            "modeCount":1,"krylovDimension":64,"residualTolerance":1e-8,
            "shift":{"real":{"expression":shift_real.to_string(),"unit":"Hz"},"imaginary":{"expression":shift_imaginary.to_string(),"unit":"Hz"}}}});
        let settings=json!({"constitutiveQuadrature":"standard","solverBackend":"faer","solverOrdering":"amd",
            "solverThreads":1,"residualTolerance":1e-8,"modeCount":1,"krylovDimension":64});
        (study,settings)
    }

    fn run_thermal_decay(profile:&str,dimension:usize,divisions:usize,exact_rate:f64)->f64{
        let package:Value=serde_json::from_str(match profile{"thermal-2d"=>include_str!("scenarios/thermal2d.json"),"thermal-3d"=>include_str!("scenarios/thermal3d.json"),_=>unreachable!()}).unwrap();
        let mesh=structured_modal_mesh(dimension,divisions);
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":[{"id":"wall","physicalTag":8}],
            "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let material=json!({"id":"material","selectionId":"domain",
            "thermalConductivity":{"expression":"1","unit":"W/(m*K)"},
            "volumetricHeatCapacity":{"expression":"1","unit":"W*s/(m^3*K)"}});
        let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
            "materials":[material],"physics":{"id":"physics","formulation":"T",
                "wall":{"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"0","unit":"K"}}},
            "study":[]}});
        let (study,settings)=eigen_settings(exact_rate,0.);let mut model=model;model["model"]["study"]=json!([study]);
        let case=json!({"globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[],
            "shift":{"realPerSecond":exact_rate,"imaginaryPerSecond":0.}});
        let modes=solve_case(&package,&model,&map,&mesh,"study","T",1,0,&case,&settings,false)
            .unwrap_or_else(|error|panic!("{profile}, mesh={divisions}: {error}"));
        assert!(modes[0]["relativeResidual"].as_f64().unwrap()<1e-8,"{profile}, mesh={divisions}: {}",modes[0]);
        let rate=modes[0]["temporalEigenvalue"]["realPerSecond"].as_f64().unwrap();
        assert!(modes[0]["temporalEigenvalue"]["imaginaryPerSecond"].as_f64().unwrap().abs()<1e-7);
        assert!(rate<0.,"{profile}, mesh={divisions}: expected a decaying mode, got {rate}");
        rate
    }

    fn run_scaled_thermal_decay(conductivity:f64)->f64{
        let package:Value=serde_json::from_str(include_str!("scenarios/thermal2d.json")).unwrap();
        let mesh=structured_modal_mesh(2,2);
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":[{"id":"wall","physicalTag":8}],
            "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let parameter=json!({"id":"conductivity","name":"Conductivity","symbol":"conductivity",
            "expression":conductivity.to_string(),"unit":"W/(m*K)"});
        let material=json!({"id":"material","selectionId":"domain",
            "thermalConductivity":{"expression":"conductivity","unit":"W/(m*K)"},
            "volumetricHeatCapacity":{"expression":"1","unit":"W*s/(m^3*K)"}});
        let target=-20.*conductivity;
        let (study,settings)=eigen_settings(target,0.);
        let model=json!({"scenario":{"profileId":"thermal-2d"},"model":{"parameters":[parameter],"mesh":{"periodicPairIds":[]},
            "materials":[material],"physics":{"id":"physics","formulation":"T",
                "wall":{"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"0","unit":"K"}}},
            "study":[study]}});
        let case=json!({"globalParameters":{"conductivity":conductivity},
            "globalParameterDimensions":{"conductivity":crate::model_expression::unit("W/(m*K)").unwrap().1},
            "parameterOverrides":model["model"]["parameters"],
            "shift":{"realPerSecond":target,"imaginaryPerSecond":0.}});
        let modes=solve_case(&package,&model,&map,&mesh,"study","T",1,0,&case,&settings,false)
            .unwrap_or_else(|error|panic!("thermal conductivity={conductivity}: {error}"));
        assert!(modes[0]["relativeResidual"].as_f64().unwrap()<1e-8,"thermal conductivity={conductivity}: {}",modes[0]);
        let rate=modes[0]["temporalEigenvalue"]["realPerSecond"].as_f64().unwrap();
        assert!(rate<0.,"thermal conductivity={conductivity}: expected a decaying mode, got {rate}");
        rate
    }

    #[test]
    fn parameter_cases_reassemble_and_solve_distinct_modal_operators(){
        let unit_rate=run_scaled_thermal_decay(1.);
        let doubled_rate=run_scaled_thermal_decay(2.);
        assert!((doubled_rate/unit_rate-2.).abs()<1e-7,
            "doubling conductivity should double the decay rate; rates were {unit_rate} and {doubled_rate}");
    }

    fn run_acoustic_resonance(profile:&str,dimension:usize,divisions:usize,exact_rate:f64)->C{
        let package:Value=serde_json::from_str(match profile{"acoustic-2d"=>include_str!("scenarios/acoustic2d.json"),"acoustic-3d"=>include_str!("scenarios/acoustic3d.json"),_=>unreachable!()}).unwrap();
        let mesh=structured_modal_mesh(dimension,divisions);
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],
            "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let material=json!({"id":"material","selectionId":"domain","density":{"expression":"1","unit":"kg/m^3"},
            "soundSpeed":{"expression":"1","unit":"m/s"}});
        let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
            "materials":[material],"physics":{"id":"physics","formulation":"pressure"},"study":[]}});
        let target=C{re:0.,im:exact_rate*1.05};
        let (study,settings)=eigen_settings(target.re,target.im);let mut model=model;model["model"]["study"]=json!([study]);
        let case=json!({"globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[],
            "shift":{"realPerSecond":target.re,"imaginaryPerSecond":target.im}});
        let modes=solve_case(&package,&model,&map,&mesh,"study","pressure",1,0,&case,&settings,false)
            .unwrap_or_else(|error|panic!("{profile}, mesh={divisions}: {error}"));
        assert!(modes[0]["relativeResidual"].as_f64().unwrap()<1e-8,"{profile}, mesh={divisions}: {}",modes[0]);
        let rate=C{re:modes[0]["temporalEigenvalue"]["realPerSecond"].as_f64().unwrap(),
            im:modes[0]["temporalEigenvalue"]["imaginaryPerSecond"].as_f64().unwrap()};
        assert!(rate.im>0.,"{profile}, mesh={divisions}: expected the positive-frequency branch, got {rate:?}");
        assert!(rate.re.abs()<1e-6*exact_rate.abs(),"{profile}, mesh={divisions}: expected a lossless mode, got {rate:?}");
        rate
    }

    fn run_em2d_resonance(formulation:&str,divisions:usize,exact_rate:f64)->C{
        run_em2d_resonance_at_kz(formulation,divisions,exact_rate,0.)
    }

    fn run_em2d_resonance_at_kz(formulation:&str,divisions:usize,exact_rate:f64,kz:f64)->C{
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let mesh=structured_modal_mesh(2,divisions);
        let pec=formulation!="Hz";
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":if pec {json!([{"id":"wall","physicalTag":8}])}else{json!([])},
            "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let eps0=8.8541878128e-12_f64;let mu0=1.25663706212e-6_f64;
        let vacuum_speed=1./(eps0*mu0).sqrt();
        let material=json!({"id":"material","selectionId":"domain","relativePermittivity":vacuum_speed.to_string(),
            "relativePermeability":vacuum_speed.to_string(),"conductivity":{"expression":"0","unit":"S/m"}});
        let mut physics=json!({"id":"physics","formulation":formulation});
        if pec {
            physics["wall"]=if formulation=="Full" {
                json!({"id":"wall","type":"dirichlet","selectionId":"wall","value":quantity("0","V/m"),
                    "fieldValues":[quantity("0","V/m"),quantity("0","V/m"),quantity("0","V/m")]})
            } else {
                json!({"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"0","unit":"V/m"}})
            };
        }
        let model=json!({"scenario":{"profileId":"em-2d"},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
            "materials":[material],"physics":physics,"study":[]}});
        let target=C{re:0.,im:exact_rate*1.05};let (study,settings)=eigen_settings(target.re,target.im);
        let mut study=study;
        if kz!=0. {
            study["longitudinalWavenumber"]=json!({"real":quantity(&kz.to_string(),"rad/m"),"imaginary":quantity("0","rad/m")});
        }
        let mut model=model;model["model"]["study"]=json!([study]);
        let mut case=json!({"globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[],
            "shift":{"realPerSecond":target.re,"imaginaryPerSecond":target.im}});
        if kz!=0. {case["longitudinalWavenumber"]=json!({"real":kz,"imaginary":0.,"unit":"rad/m"});}
        let modes=solve_case(&package,&model,&map,&mesh,"study",formulation,1,0,&case,&settings,false)
            .unwrap_or_else(|error|panic!("em-2d/{formulation}, mesh={divisions}: {error}"));
        assert!(modes[0]["relativeResidual"].as_f64().unwrap()<1e-8,"em-2d/{formulation}, mesh={divisions}: {}",modes[0]);
        let rate=C{re:modes[0]["temporalEigenvalue"]["realPerSecond"].as_f64().unwrap(),
            im:modes[0]["temporalEigenvalue"]["imaginaryPerSecond"].as_f64().unwrap()};
        assert!(rate.im>0.,"em-2d/{formulation}, mesh={divisions}: expected the positive-frequency branch, got {rate:?}");
        assert!(rate.re.abs()<1e-6*exact_rate.abs(),"em-2d/{formulation}, mesh={divisions}: expected a lossless mode, got {rate:?}");
        rate
    }

    fn run_em3d_resonance(divisions:usize,order:u8,exact_rate:f64)->C{
        let package:Value=serde_json::from_str(include_str!("scenarios/em3d.json")).unwrap();
        let mesh=structured_modal_mesh(3,divisions);
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":[{"id":"wall","physicalTag":8}],
            "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let eps0=8.8541878128e-12_f64;let mu0=1.25663706212e-6_f64;
        let vacuum_speed=1./(eps0*mu0).sqrt();
        let material=json!({"id":"material","selectionId":"domain","relativePermittivity":vacuum_speed.to_string(),
            "relativePermeability":vacuum_speed.to_string(),"conductivity":{"expression":"0","unit":"S/m"}});
        let model=json!({"scenario":{"profileId":"em-3d"},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
            "materials":[material],"physics":{"id":"physics","formulation":"electric",
                "wall":{"id":"wall","type":"dirichlet","selectionId":"wall","value":{"expression":"0","unit":"V/m"},
                    "fieldValues":[{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"},{"expression":"0","unit":"V/m"}]}},"study":[]}});
        let target=C{re:0.,im:exact_rate*1.05};let (study,settings)=eigen_settings(target.re,target.im);
        let mut model=model;model["model"]["study"]=json!([study]);
        let case=json!({"globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[],
            "shift":{"realPerSecond":target.re,"imaginaryPerSecond":target.im}});
        let modes=solve_case(&package,&model,&map,&mesh,"study","electric",order,0,&case,&settings,false)
            .unwrap_or_else(|error|panic!("em-3d/electric/order{order}, mesh={divisions}: {error}"));
        assert!(modes[0]["relativeResidual"].as_f64().unwrap()<1e-8,"em-3d/order{order} mesh={divisions}: {}",modes[0]);
        let rate=C{re:modes[0]["temporalEigenvalue"]["realPerSecond"].as_f64().unwrap(),
            im:modes[0]["temporalEigenvalue"]["imaginaryPerSecond"].as_f64().unwrap()};
        assert!(rate.im>0.,"em-3d/order{order} mesh={divisions}: expected the positive-frequency branch, got {rate:?}");
        assert!(rate.re.abs()<1e-6*exact_rate.abs(),"em-3d/order{order} mesh={divisions}: expected a lossless mode, got {rate:?}");
        rate
    }

    fn run_elastic_periodic_longitudinal(profile:&str,formulation:&str,dimension:usize,divisions:usize,exact_rate:f64)->C{
        let package:Value=serde_json::from_str(match profile{"elastic-2d"=>include_str!("scenarios/elastic2d.json"),
            "elastic-3d"=>include_str!("scenarios/elastic3d.json"),_=>unreachable!()}).unwrap();
        let mut mesh=structured_modal_mesh(dimension,divisions);
        for (facet,tag) in &mut mesh.facets{
            *tag=if facet.iter().all(|&node|mesh.nodes[node][0].abs()<1e-12){1}
                else if facet.iter().all(|&node|(mesh.nodes[node][0]-1.).abs()<1e-12){2}else{8};
        }
        let pair=json!({"id":"px","dimension":dimension,"lowerSelectionId":"xLower","upperSelectionId":"xUpper",
            "translation":[{"expression":"1","unit":"m"},{"expression":"0","unit":"m"},{"expression":"0","unit":"m"}]});
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],
            "boundarySelections":[{"id":"xLower","physicalTag":1},{"id":"xUpper","physicalTag":2}],
            "periodicPairs":[pair],"domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let material=json!({"id":"material","selectionId":"domain","density":{"expression":"1","unit":"kg/m^3"},
            "youngModulus":{"expression":"1","unit":"Pa"},"poissonRatio":"0"});
        let target=C{re:0.,im:exact_rate*0.999};let (study,settings)=eigen_settings(target.re,target.im);
        let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":["px"]},
            "materials":[material],"physics":{"id":"physics","formulation":formulation,
                "periodic":{"id":"periodic","type":"periodicCondition","periodicPairId":"px"}},"study":[study]}});
        let case=json!({"globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[],
            "shift":{"realPerSecond":target.re,"imaginaryPerSecond":target.im}});
        let modes=solve_case(&package,&model,&map,&mesh,"study",formulation,1,0,&case,&settings,false)
            .unwrap_or_else(|error|panic!("{profile}/{formulation}, mesh={divisions}: {error}"));
        assert!(modes[0]["relativeResidual"].as_f64().unwrap()<1e-8,"{profile}/{formulation}, mesh={divisions}: {}",modes[0]);
        let rate=C{re:modes[0]["temporalEigenvalue"]["realPerSecond"].as_f64().unwrap(),
            im:modes[0]["temporalEigenvalue"]["imaginaryPerSecond"].as_f64().unwrap()};
        assert!(rate.im>0.,"{profile}/{formulation}, mesh={divisions}: expected the positive-frequency branch, got {rate:?}");
        assert!(rate.re.abs()<1e-6*exact_rate.abs(),"{profile}/{formulation}, mesh={divisions}: expected an undamped mode, got {rate:?}");
        rate
    }

    fn run_constant_zero_mode(profile:&str,dimension:usize)->Value{
        let package:Value=serde_json::from_str(match profile{"acoustic-2d"=>include_str!("scenarios/acoustic2d.json"),
            "acoustic-3d"=>include_str!("scenarios/acoustic3d.json"),"thermal-2d"=>include_str!("scenarios/thermal2d.json"),
            "thermal-3d"=>include_str!("scenarios/thermal3d.json"),_=>unreachable!()}).unwrap();
        let mesh=structured_modal_mesh(dimension,2);
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],
            "domainCellSelections":{"domain":(0..mesh.cells.len()).collect::<Vec<_>>()}});
        let material=if profile.starts_with("acoustic"){
            json!({"id":"material","selectionId":"domain","density":{"expression":"1","unit":"kg/m^3"},
                "soundSpeed":{"expression":"1","unit":"m/s"}})
        }else{
            json!({"id":"material","selectionId":"domain","thermalConductivity":{"expression":"1","unit":"W/(m*K)"},
                "volumetricHeatCapacity":{"expression":"1","unit":"W*s/(m^3*K)"}})
        };
        let form=if profile.starts_with("acoustic"){"pressure"}else{"T"};
        let shift=C{re:-0.03,im:0.02};let (study,settings)=eigen_settings(shift.re,shift.im);
        let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
            "materials":[material],"physics":{"id":"physics","formulation":form},"study":[study]}});
        let case=json!({"globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[],
            "shift":{"realPerSecond":shift.re,"imaginaryPerSecond":shift.im}});
        let modes=solve_case(&package,&model,&map,&mesh,"study",form,1,0,&case,&settings,true)
            .unwrap_or_else(|error|panic!("{profile} constant mode: {error}"));
        assert!(modes[0]["relativeResidual"].as_f64().unwrap()<1e-8,"{profile}: {}",modes[0]);
        let real=modes[0]["temporalEigenvalue"]["realPerSecond"].as_f64().unwrap();
        let imaginary=modes[0]["temporalEigenvalue"]["imaginaryPerSecond"].as_f64().unwrap();
        assert!(real.hypot(imaginary)<1e-7,"{profile}: expected the constant zero mode, got s={real}+i{imaginary}");
        let symbol=if profile.starts_with("acoustic"){"p"}else{"T"};
        let values=modes[0]["fields"][symbol]["real"].as_array().unwrap();
        let imag_values=modes[0]["fields"][symbol]["imag"].as_array().unwrap();
        let mean_real=values.iter().map(Value::as_f64).map(Option::unwrap).sum::<f64>()/values.len() as f64;
        let mean_imag=imag_values.iter().map(Value::as_f64).map(Option::unwrap).sum::<f64>()/imag_values.len() as f64;
        let amplitude=mean_real.hypot(mean_imag).max(f64::MIN_POSITIVE);
        let error=values.iter().zip(imag_values).map(|(real,imag)|{
            let dr=real.as_f64().unwrap()-mean_real;let di=imag.as_f64().unwrap()-mean_imag;dr.hypot(di)
        }).fold(0_f64,f64::max)/amplitude;
        assert!(error<1e-7,"{profile}: expected a spatially constant zero-mode shape, relative variation={error}");
        modes[0].clone()
    }

    #[test]
    fn every_scenario_form_assembles_and_solves_a_small_modal_case(){
        let profiles:[(&str,&str,&[&str],u8);8]=[
            ("acoustic-2d",include_str!("scenarios/acoustic2d.json"),&["pressure"],1),
            ("acoustic-3d",include_str!("scenarios/acoustic3d.json"),&["pressure"],1),
            ("elastic-2d",include_str!("scenarios/elastic2d.json"),&["planeStress","planeStrain"],1),
            ("elastic-3d",include_str!("scenarios/elastic3d.json"),&["displacement"],1),
            ("em-2d",include_str!("scenarios/em2d.json"),&["Ez","Hz"],1),
            ("em-3d",include_str!("scenarios/em3d.json"),&["electric"],2),
            ("thermal-2d",include_str!("scenarios/thermal2d.json"),&["T"],1),
            ("thermal-3d",include_str!("scenarios/thermal3d.json"),&["T"],1),
        ];
        for (profile,text,forms,order) in profiles{
            let package:Value=serde_json::from_str(text).unwrap();
            let dimension=package.pointer("/schemaDocument/x-fem-scenario/profile/dimension").and_then(Value::as_u64).unwrap() as usize;
            let material=match profile{
                "acoustic-2d"|"acoustic-3d"=>json!({"density":{"expression":"1","unit":"kg/m^3"},"soundSpeed":{"expression":"1","unit":"m/s"}}),
                "elastic-2d"|"elastic-3d"=>json!({"density":{"expression":"1","unit":"kg/m^3"},"youngModulus":{"expression":"1","unit":"Pa"},"poissonRatio":"0.25"}),
                "em-2d"|"em-3d"=>json!({"relativePermittivity":"141.9510639","relativePermeability":"1","conductivity":{"expression":"0","unit":"S/m"}}),
                "thermal-2d"|"thermal-3d"=>json!({"thermalConductivity":{"expression":"1","unit":"W/(m*K)"},"volumetricHeatCapacity":{"expression":"1","unit":"W*s/(m^3*K)"}}),
                _=>unreachable!(),
            };
            let mesh_text=if dimension==2{
                "FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nQ,0,0,1,2,7\n"
            }else{
                "FULLMETA_MESH/1\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nT,0,0,1,2,3,7\nFULLMETA_MESH_DATA_END\n"
            };
            let mesh=MeshData::parse(dimension,mesh_text).unwrap();
            let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":7}],"boundarySelections":[],"domainCellSelections":{"domain":[0]}});
            for &form in forms{
                let mut material_row=json!({"id":"material","selectionId":"domain"});
                material_row.as_object_mut().unwrap().extend(material.as_object().unwrap().clone());
                let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"mesh":{"periodicPairIds":[]},
                    "materials":[material_row],
                    "physics":{"id":"physics","formulation":form},
                    "study":[{"id":"study","type":"eigenfrequency","physicsId":"physics","eigenfrequency":{"modeCount":1,"krylovDimension":64,
                        "residualTolerance":1e-7,"shift":{"real":{"expression":"-0.73","unit":"Hz"},"imaginary":{"expression":"0.91","unit":"Hz"}}}}]}});
                let settings=json!({"fieldFormulation":form,"elementOrder":order,"constitutiveQuadrature":"standard","solverBackend":"faer",
                    "solverOrdering":"amd","solverThreads":1,"residualTolerance":1e-7,"modeCount":1,"krylovDimension":64});
                let case=json!({"globalParameters":{},"globalParameterDimensions":{},"parameterOverrides":[],
                    "shift":{"realPerSecond":-0.73,"imaginaryPerSecond":0.91}});
                let modes=solve_case(&package,&model,&map,&mesh,"study",form,order,0,&case,&settings,false)
                    .unwrap_or_else(|error|panic!("{profile}/{form}: {error}"));
                assert_eq!(modes.len(),1,"{profile}/{form}");
                assert!(modes[0]["relativeResidual"].as_f64().unwrap()<1e-7,"{profile}/{form}: {}",modes[0]);
                assert!(modes[0]["temporalEigenvalue"]["realPerSecond"].as_f64().unwrap().is_finite());
            }
        }
    }

    #[test]
    fn thermal_decay_eigenvalues_converge_to_the_analytic_dirichlet_box_rates(){
        for (profile,dimension,refinements,multiplier) in [
            ("thermal-2d",2,vec![2,3,4,6],2.),
            ("thermal-3d",3,vec![2,3,4,6,8],3.),
        ]{
            let exact=-multiplier*std::f64::consts::PI.powi(2);
            let rates=refinements.iter().map(|&divisions|run_thermal_decay(profile,dimension,divisions,exact)).collect::<Vec<_>>();
            let errors=rates.iter().map(|rate|(rate-exact).abs()).collect::<Vec<_>>();
            assert!(errors.windows(2).all(|pair|pair[1]<pair[0]),"{profile} eigenvalue errors should decrease under refinement: {errors:?}; rates={rates:?}");
            assert!(errors.last().unwrap()/exact.abs()<0.08,"{profile} finest eigenvalue error is too large: {errors:?}; rates={rates:?}");
        }
    }

    #[test]
    fn acoustic_resonances_converge_to_the_analytic_rigid_box_rates(){
        for (profile,dimension,refinements) in [
            ("acoustic-2d",2,vec![2,3,4,6,8]),
            ("acoustic-3d",3,vec![2,3,4,6]),
        ]{
            let exact=std::f64::consts::PI;
            let rates=refinements.iter().map(|&divisions|run_acoustic_resonance(profile,dimension,divisions,exact)).collect::<Vec<_>>();
            let errors=rates.iter().map(|rate|(rate.im-exact).abs()).collect::<Vec<_>>();
            assert!(errors.windows(2).all(|pair|pair[1]<pair[0]),"{profile} eigenfrequency errors should decrease under refinement: {errors:?}; rates={rates:?}");
            assert!(errors.last().unwrap()/exact<0.12,"{profile} finest eigenfrequency error is too large: {errors:?}; rates={rates:?}");
        }
    }

    #[test]
    fn em2d_polarization_resonances_converge_to_analytic_box_rates(){
        let pi=std::f64::consts::PI;
        for (formulation,exact) in [("Ez",pi*2f64.sqrt()),("Hz",pi),("Full",pi*2f64.sqrt())]{
            let rates=[4,6,8].into_iter().map(|divisions|run_em2d_resonance(formulation,divisions,exact)).collect::<Vec<_>>();
            let errors=rates.iter().map(|rate|(rate.im-exact).abs()).collect::<Vec<_>>();
            assert!(errors.windows(2).all(|pair|pair[1]<pair[0]),"em-2d/{formulation} eigenfrequency errors should decrease under refinement: {errors:?}; rates={rates:?}");
            assert!(errors.last().unwrap()/exact<0.12,"em-2d/{formulation} finest eigenfrequency error is too large: {errors:?}; rates={rates:?}");
        }
    }

    #[test]
    fn em2d_full_zero_kz_spectrum_contains_both_scalar_polarization_routes(){
        let pi=std::f64::consts::PI;
        for (formulation,exact) in [("Ez",pi*2f64.sqrt()),("Hz",pi)]{
            for divisions in [4,6,8]{
                let scalar=run_em2d_resonance(formulation,divisions,exact);
                let full=run_em2d_resonance("Full",divisions,exact);
                let relative=(scalar.im-full.im).abs()/exact;
                assert!(relative<0.05,
                    "zero-kz Full {formulation} polarization should agree with its scalar route on the same mesh; divisions={divisions}, scalar={scalar:?}, Full={full:?}, relative difference={relative}");
            }
        }
    }

    #[test]
    fn em2d_full_temporal_eigenfrequency_keeps_fixed_finite_kz(){
        let kt=std::f64::consts::PI*2f64.sqrt();
        let kz=2.;
        let expected=(kt*kt+kz*kz).sqrt();
        let rate=run_em2d_resonance_at_kz("Full",8,expected,kz);
        assert!((rate.im-expected).abs()/expected<0.12,
            "EM2D Full temporal rate at fixed kz={kz} should approach sqrt(kt^2+kz^2)={expected}, got {rate:?}");
    }

    #[test]
    fn em3d_edge_modes_converge_to_the_analytic_pec_cavity_rate(){
        let exact=std::f64::consts::PI*2f64.sqrt();
        for order in [1,2]{
            let rates=[2,3,4].into_iter().map(|divisions|run_em3d_resonance(divisions,order,exact)).collect::<Vec<_>>();
            let errors=rates.iter().map(|rate|(rate.im-exact).abs()).collect::<Vec<_>>();
            assert!(errors.last().unwrap()< &errors[0],"em-3d/order{order} edge eigenfrequency error should decrease under refinement: {errors:?}; rates={rates:?}");
            assert!(errors.last().unwrap()/exact<0.2,"em-3d/order{order} finest eigenfrequency error is too large: {errors:?}; rates={rates:?}");
        }
    }

    #[test]
    fn elastic_formulations_converge_to_the_analytic_periodic_longitudinal_rate(){
        // For nu=0, the longitudinal plane wave u_x=exp(i 2*pi*x) has no
        // lateral Poisson coupling, so the free transverse faces and one
        // periodic x-pair give the exact c_p=sqrt(E/rho) reference in both
        // plane reductions and in 3D.
        let exact=2.*std::f64::consts::PI;
        for (profile,dimension,forms) in [
            ("elastic-2d",2,&["planeStress","planeStrain"][..]),
            ("elastic-3d",3,&["displacement"][..]),
        ]{
            for formulation in forms{
                let refinements=if dimension==2{vec![4,6,8]}else{vec![2,4,6]};
                let rates=refinements.into_iter().map(|divisions|
                    run_elastic_periodic_longitudinal(profile,formulation,dimension,divisions,exact)).collect::<Vec<_>>();
                let errors=rates.iter().map(|rate|(rate.im-exact).abs()).collect::<Vec<_>>();
                assert!(errors.last().unwrap()< &errors[0],"{profile}/{formulation} eigenfrequency error should decrease under refinement: {errors:?}; rates={rates:?}");
                assert!(errors.last().unwrap()/exact<0.15,"{profile}/{formulation} finest eigenfrequency error is too large: {errors:?}; rates={rates:?}");
            }
        }
    }

    #[test]
    fn acoustic_and_thermal_constant_zero_modes_survive_result_snapshotting(){
        for (profile,dimension) in [("acoustic-2d",2),("acoustic-3d",3),("thermal-2d",2),("thermal-3d",3)]{
            let mode=run_constant_zero_mode(profile,dimension);
            assert_eq!(mode["schema"],"fullmeta-eigenfrequency-mode/1");
            assert!(mode.get("nodes").is_some());
            assert!(mode.get(if dimension==2{"triangles"}else{"tetrahedra"}).is_some());
        }
    }
}
