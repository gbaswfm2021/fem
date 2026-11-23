//! Scenario-neutral fixed-frequency propagation-mode execution.
//!
//! Each operator sample is routed by the selected formulation's JSON
//! `propagationModes.numericalFamily` declaration. The common adapter samples
//! its declared quadratic sparse operator, linearizes it with a first
//! companion pencil, and returns `kz` itself.
use crate::solver_common::{C,SolverCache};
use crate::transient_execution::MeshData;
use serde_json::{Value,json};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path,PathBuf};

fn request_path(request:&Value,base:&Path,key:&str)->Result<PathBuf,String>{
    let value=request.get(key).and_then(Value::as_str).filter(|value|!value.is_empty()).ok_or_else(||format!("Propagation-mode request {key} is required."))?;
    let path=PathBuf::from(value);Ok(if path.is_absolute(){path}else{base.join(path)})
}

fn load_json(path:&Path,label:&str)->Result<Value,String>{
    serde_json::from_slice(&fs::read(path).map_err(|error|format!("Could not read propagation-mode {label}: {error}"))?)
        .map_err(|error|format!("Invalid propagation-mode {label} JSON: {error}"))
}

fn sparse_scale(nodes:&[[f64;3]])->Result<f64,String>{
    if nodes.len()<3{return Err("Propagation-mode mesh requires at least three nodes.".into());}
    let mut minimum=[f64::INFINITY;2];let mut maximum=[f64::NEG_INFINITY;2];
    for point in nodes{for axis in 0..2{if !point[axis].is_finite(){return Err("Propagation-mode mesh coordinates must be finite.".into());}
        minimum[axis]=minimum[axis].min(point[axis]);maximum[axis]=maximum[axis].max(point[axis]);}}
    let length=(maximum[0]-minimum[0]).hypot(maximum[1]-minimum[1]);
    if !length.is_finite()||length<=0.{return Err("Propagation-mode mesh has no positive transverse length scale.".into());}
    Ok(1./length)
}

fn mesh2d(mesh:&MeshData)->Result<crate::mesh2d::Mesh2D,String>{
    if mesh.dimension!=2{return Err("Propagation-mode studies require a two-dimensional cross-section mesh.".into());}
    let triangles=mesh.cells.iter().map(|cell|Ok((cell.nodes.as_slice().try_into().map_err(|_|"Propagation-mode cell must be a triangle.")?,cell.tag)))
        .collect::<Result<Vec<([usize;3],usize)>,String>>()?;
    let boundary_edges=mesh.facets.iter().map(|(nodes,tag)|Ok((nodes.as_slice().try_into().map_err(|_|"Propagation-mode boundary facet must be an edge.")?,*tag)))
        .collect::<Result<Vec<([usize;2],usize)>,String>>()?;
    Ok(crate::mesh2d::Mesh2D{nodes:mesh.nodes.clone(),triangles,boundary_edges,integration_edges:mesh.integration_edges.clone()})
}

fn field(real:Vec<f64>,imaginary:Vec<f64>,unit:&str)->Value{
    json!({"unit":unit,"real":real,"imag":imaginary})
}

fn cell_field(real:Vec<Vec<f64>>,imaginary:Vec<Vec<f64>>,unit:&str)->Value{
    json!({"unit":unit,"real":real,"imag":imaginary})
}

fn expand_nodal_scalar(model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,values:&[C])->Result<Vec<C>,String>{
    let periodic=crate::scalar2d::periodic_dof_map(model,map,mesh)?;
    if values.len()!=periodic.dof_count{return Err("Propagation-mode scalar eigenvector differs from its reduced nodal space.".into());}
    Ok((0..mesh.nodes.len()).map(|node|crate::c_mul(periodic.node_weights[node],values[periodic.node_to_dof[node]])).collect())
}

fn expand_nodal_vector(model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,values:&[C],components:usize)->Result<Vec<C>,String>{
    let periodic=crate::scalar2d::periodic_dof_map(model,map,mesh)?;
    if values.len()!=periodic.dof_count*components{return Err("Propagation-mode vector eigenvector differs from its reduced nodal space.".into());}
    let mut expanded=Vec::with_capacity(mesh.nodes.len()*components);
    for node in 0..mesh.nodes.len(){for axis in 0..components{
        expanded.push(crate::c_mul(periodic.node_weights[node],values[components*periodic.node_to_dof[node]+axis]));
    }}
    Ok(expanded)
}

#[derive(Default)]
struct ModeFieldValues {
    fields: HashMap<String,Value>,
    cell_fields: HashMap<String,Value>,
}

/// Evaluate the declared scalar density at each cell-local vertex. Keeping a
/// recovered quantity cell-local preserves jumps at material interfaces.
fn scalar_recovery_densities(package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,
    formulation:&str,frequency:f64,parameters:&HashMap<String,crate::model_expression::Scalar>)
    ->Result<Option<Vec<Vec<f64>>>,String>{
    let declaration=crate::propagation_mode_assembly::formulation(package,formulation)?;
    let recoveries=declaration.get("derivedFieldRecoveries").and_then(Value::as_array).cloned().unwrap_or_default();
    let Some(recovery)=recoveries.iter().find(|row|row.get("operator").and_then(Value::as_str)==Some("scalarLongitudinalMomentum")) else {
        return Ok(None);
    };
    let density_symbol=recovery.get("materialDensitySymbol").and_then(Value::as_str)
        .filter(|value|!value.is_empty()).ok_or("Scalar longitudinal-momentum recovery requires materialDensitySymbol.")?;
    let profile=package.get("id").and_then(Value::as_str).ok_or("Propagation-mode scenario package ID is missing.")?;
    let plan=crate::scenario_plan::ScenarioPlan::from_package(package,formulation)?;
    let selection=plan.selections.as_ref().ok_or("Scalar propagation recovery requires a declared selection plan.")?;
    let mesh_tags=HashMap::from([
        ("domainSources".to_string(),mesh.triangles.iter().map(|(_,tag)|*tag).collect()),
        ("materials".to_string(),mesh.triangles.iter().map(|(_,tag)|*tag).collect()),
        ("boundaries".to_string(),mesh.boundary_edges.iter().map(|(_,tag)|*tag).collect()),
    ]);
    let selected=selection.bind(model,map,&mesh_tags)?;
    let rows=selected.get("materials").ok_or("Scalar propagation recovery has no selected materials.")?
        .iter().map(|row|(row.tag,row.row.clone())).collect::<HashMap<_,_>>();
    let materials=crate::material::CompiledMaterialAssignments::compile(profile,&rows)?;
    mesh.triangles.iter().enumerate().map(|(cell_index,(nodes,tag))|nodes.iter().map(|node|{
        let point=mesh.nodes[*node];
        let values=materials.evaluate(*tag,parameters,frequency,point,&format!("propagation mode cell {cell_index} vertex"))?;
        values.get(density_symbol).map(|value|value.value).filter(|value|value.is_finite()&&*value>0.)
            .ok_or_else(||format!("Scalar propagation recovery material {density_symbol:?} must be finite and positive."))
    }).collect()).collect::<Result<Vec<Vec<f64>>,String>>().map(Some)
}

fn declared_scalar_recoveries(package:&Value,formulation:&str,primary:&[C],cell_nodes:&[([usize;3],usize)],
    densities:Option<&[Vec<f64>]>,kz:C,frequency:f64)->Result<HashMap<String,Value>,String>{
    let declaration=crate::propagation_mode_assembly::formulation(package,formulation)?;
    let Some(recoveries)=declaration.get("derivedFieldRecoveries").and_then(Value::as_array) else {return Ok(HashMap::new());};
    let mut output=HashMap::new();
    for recovery in recoveries{
        let operator=recovery.get("operator").and_then(Value::as_str).ok_or("Derived-field recovery operator is missing.")?;
        if operator!="scalarLongitudinalMomentum"{return Err(format!("Unsupported propagation-mode derived-field operator {operator:?}."));}
        let symbol=recovery.get("symbol").and_then(Value::as_str).filter(|value|!value.is_empty()).ok_or("Scalar derived-field recovery symbol is missing.")?;
        let primary_field=recovery.get("primaryField").and_then(Value::as_str).ok_or("Scalar derived-field primaryField is missing.")?;
        if primary_field!="p"{return Err(format!("Scalar longitudinal-momentum recovery expects pressure primary field p, got {primary_field:?}."));}
        let unit=recovery.get("unit").and_then(Value::as_str).ok_or("Scalar derived-field recovery unit is missing.")?;
        if unit!="m/s"{return Err("Scalar longitudinal-momentum recovery output unit must be m/s.".into());}
        let densities=densities.ok_or("Scalar longitudinal-momentum recovery has no evaluated material density.")?;
        if densities.len()!=cell_nodes.len(){return Err("Scalar longitudinal-momentum recovery density topology does not match the mesh.".into());}
        let omega=std::f64::consts::TAU*frequency;
        if !omega.is_finite()||omega<=0.{return Err("Scalar longitudinal-momentum recovery requires a positive angular frequency.".into());}
        let mut real=Vec::with_capacity(cell_nodes.len());let mut imaginary=Vec::with_capacity(cell_nodes.len());
        for (cell_index,((nodes,_),rho)) in cell_nodes.iter().zip(densities).enumerate(){
            if rho.len()!=nodes.len(){return Err(format!("Scalar longitudinal-momentum recovery density row {cell_index} has the wrong width."));}
            let mut real_local=Vec::with_capacity(nodes.len());let mut imaginary_local=Vec::with_capacity(nodes.len());
            for (&node,&density) in nodes.iter().zip(rho){
                let pressure=primary.get(node).ok_or("Scalar longitudinal-momentum recovery references a missing pressure value.")?;
                let value=crate::c_scale(crate::c_mul(kz,*pressure),1./(omega*density));
                if !value.re.is_finite()||!value.im.is_finite(){return Err(format!("Recovered scalar field {symbol:?} is not finite."));}
                real_local.push(value.re);imaginary_local.push(value.im);
            }
            real.push(real_local);imaginary.push(imaginary_local);
        }
        output.insert(symbol.to_owned(),cell_field(real,imaginary,unit));
    }
    Ok(output)
}

/// Reuse the scenario-declared tensor recovery for vector-family propagation
/// modes. The longitudinal derivative is supplied by the selected complex
/// `kz`; material and constitutive expressions remain contract-driven.
fn declared_vector_recovery(package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,formulation:&str,
    frequency:f64,kz:C,parameters:&HashMap<String,crate::model_expression::Scalar>,displacement:&[C])
    ->Result<HashMap<String,Value>,String>{
    let declaration=crate::propagation_mode_assembly::formulation(package,formulation)?;
    let Some(tensor_formulation)=declaration.get("tensorRecoveryFormulation").and_then(Value::as_str) else {return Ok(HashMap::new());};
    let profile=package.get("id").and_then(Value::as_str).ok_or("Propagation-mode scenario package ID is missing.")?;
    let mut plan=crate::scenario_plan::ScenarioPlan::from_package(package,tensor_formulation)?;
    plan.set_longitudinal_wavenumber(kz)?;
    let recovery=plan.tensor_recovery.as_ref().ok_or("Declared propagation tensorRecoveryFormulation has no tensor recovery plan.")?;
    if !recovery.has_longitudinal_operator(){return Err("Propagation tensor recovery must include a longitudinal derivative.".into());}
    if displacement.len()!=mesh.nodes.len()*3{return Err("Propagation tensor recovery requires three displacement samples per mesh node.".into());}
    let selections=plan.selections.as_ref().ok_or("Propagation tensor recovery requires a declared material selection plan.")?;
    let mesh_tags=HashMap::from([
        ("domainSources".to_string(),mesh.triangles.iter().map(|(_,tag)|*tag).collect()),
        ("materials".to_string(),mesh.triangles.iter().map(|(_,tag)|*tag).collect()),
        ("boundaries".to_string(),mesh.boundary_edges.iter().map(|(_,tag)|*tag).collect()),
    ]);
    let selected=selections.bind(model,map,&mesh_tags)?;
    let rows=selected.get("materials").ok_or("Propagation tensor recovery has no selected materials.")?
        .iter().map(|row|(row.tag,row.row.clone())).collect::<HashMap<_,_>>();
    let materials=crate::material::CompiledMaterialAssignments::compile(profile,&rows)?;
    let mut context=parameters.clone();
    context.insert("omega".into(),crate::model_expression::Scalar{value:std::f64::consts::TAU*frequency,dimension:[0.;5]});
    let result_fields=declaration.get("resultFields").and_then(Value::as_array).ok_or("Vector propagation mode must declare resultFields for tensor recovery.")?;
    let mut output=HashMap::new();
    let mut real=vec![Vec::new();recovery.components().len()];
    let mut imaginary=vec![Vec::new();recovery.components().len()];
    for (cell_index,(nodes,tag)) in mesh.triangles.iter().enumerate(){
        let coordinates=nodes.map(|node|mesh.nodes[node]);
        let (_,gradients)=crate::mesh2d::triangle_geometry(coordinates)?;
        let centroid=std::array::from_fn(|axis|nodes.iter().map(|&node|mesh.nodes[node][axis]).sum::<f64>()/3.);
        let material=materials.evaluate(*tag,parameters,frequency,centroid,&format!("propagation tensor recovery cell {cell_index}"))?;
        let bound=crate::prepared_vector::bind_material(&plan,&material,&context,"propagation tensor recovery material")?;
        let symbols=plan.volume.evaluate_coefficients(&bound)?;
        let mut gradient=vec![C::default();9];
        for component in 0..3{
            let mut centroid_value=C::default();
            for local in 0..3{
                let value=displacement[nodes[local]*3+component];
                centroid_value=crate::c_add(centroid_value,crate::c_scale(value,1./3.));
                for direction in 0..2{
                    let index=component*3+direction;
                    gradient[index]=crate::c_add(gradient[index],crate::c_scale(value,gradients[local][direction]));
                }
            }
            gradient[component*3+2]=recovery.longitudinal_derivative(centroid_value);
        }
        let stress=recovery.apply(&gradient,&symbols)?;
        if stress.len()!=recovery.components().len(){return Err("Propagation tensor recovery returned an inconsistent stress vector.".into());}
        for (component,value) in stress.into_iter().enumerate(){
            real[component].push(vec![value.re;3]);
            imaginary[component].push(vec![value.im;3]);
        }
    }
    for (component,symbol) in recovery.components().iter().enumerate(){
        let descriptor=result_fields.iter().find(|field|field.get("symbol").and_then(Value::as_str)==Some(symbol))
            .ok_or_else(||format!("Propagation tensor component {symbol:?} is missing from resultFields."))?;
        let unit=descriptor.get("unit").and_then(Value::as_str).filter(|value|!value.is_empty()).ok_or("Propagation tensor result field unit is missing.")?;
        output.insert(symbol.clone(),cell_field(std::mem::take(&mut real[component]),std::mem::take(&mut imaginary[component]),unit));
    }
    Ok(output)
}

fn hcurl_fields(package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,study_id:&str,
    formulation:&str,frequency:f64,kz:C,parameters:&HashMap<String,crate::model_expression::Scalar>,
    reduced:&[C])->Result<HashMap<String,Value>,String>{
    use crate::propagation_mode_assembly::Family;
    let operator=crate::longitudinal::operator_formulation(package,formulation);
    let source=crate::longitudinal::source_formulation(package,formulation);
    let mut plan=crate::scenario_plan::ScenarioPlan::from_package_with_source_formulation(package,operator,source)?;
    plan.set_longitudinal_wavenumber(kz)?;
    let connectivity=mesh.triangles.iter().map(|(cell,_)|*cell).collect::<Vec<_>>();
    let topology=crate::hcurl2d::DofTopology::triangles(mesh.nodes.len(),&connectivity)?;
    let map_dofs=crate::prepared_hcurl2d_model::periodic_dof_map(model,map,mesh,&topology)?;
    if reduced.len()!=map_dofs.count{return Err("Propagation-mode electromagnetic vector has the wrong reduced size.".into());}
    let mut full=vec![C::default();topology.count];
    for (raw,slot) in full.iter_mut().enumerate(){*slot=crate::c_mul(map_dofs.weights[raw],reduced[map_dofs.representatives[raw]]);}
    let config=package.get("preparedHcurl2d").ok_or("H(curl) propagation contract has no preparedHcurl2d policy.")?;
    let derived=config.get("derivedInputs").map(crate::coefficient_expr::CoefficientExpressions::from_json).transpose()?;
    let mesh_tags=HashMap::from([("domainSources".into(),mesh.triangles.iter().map(|(_,tag)|*tag).collect()),
        ("materials".into(),mesh.triangles.iter().map(|(_,tag)|*tag).collect()),
        ("boundaries".into(),mesh.boundary_edges.iter().map(|(_,tag)|*tag).collect())]);
    let selection=plan.selections.as_ref().ok_or("Propagation-mode H(curl) selection plan is missing.")?;
    let selected=selection.bind(model,map,&mesh_tags)?;
    let rows=selected.get("materials").ok_or("Propagation-mode H(curl) materials are missing.")?.iter().map(|row|(row.tag,row.row.clone())).collect();
    let profile=package.get("id").and_then(Value::as_str).ok_or("Propagation-mode scenario package ID is missing.")?;
    let compiled=crate::material::CompiledMaterialAssignments::compile(profile,&rows)?;
    let constants=compiled.position_independent_values(parameters,frequency)?;
    let mut context=parameters.clone();context.insert("omega".into(),crate::model_expression::Scalar{value:std::f64::consts::TAU*frequency,dimension:[0.;5]});
    let mut electric=vec![[C::default();3];mesh.nodes.len()];let mut node_counts=vec![0usize;mesh.nodes.len()];
    let mut magnetic=vec![[C::default();3];mesh.nodes.len()];let mut magnetic_counts=vec![0usize;mesh.nodes.len()];
    let coordinates=mesh.nodes.iter().map(|point|[point[0],point[1]]).collect::<Vec<_>>();
    for (cell_index,(cell,tag)) in mesh.triangles.iter().enumerate(){
        let dofs=&topology.elements[cell_index];
        let gradients=crate::mesh2d::triangle_geometry(cell.map(|node|mesh.nodes[node]))?.1;
        for (local_node,&node) in cell.iter().enumerate(){
            let barycentric=std::array::from_fn(|axis|if axis==local_node{1.}else{0.});
            let (values,_)=crate::hcurl2d::basis(&coordinates,*cell,barycentric,gradients)?;
            for (basis,dof) in values.iter().zip(dofs){for axis in 0..3{electric[node][axis]=crate::c_add(electric[node][axis],crate::c_mul(full[*dof],C{re:basis[axis],im:0.}));}}
        }
        for &node in cell{node_counts[node]+=1;}
        let barycentric=[1./3.;3];let (values,derivatives)=crate::hcurl2d::basis(&coordinates,*cell,barycentric,gradients)?;
        let mut e=[C::default();3];let mut grad=[[C::default();2];3];
        for ((value,derivative),dof) in values.iter().zip(&derivatives).zip(dofs){for axis in 0..3{e[axis]=crate::c_add(e[axis],crate::c_scale(full[*dof],value[axis]));
            for direction in 0..2{grad[axis][direction]=crate::c_add(grad[axis][direction],crate::c_scale(full[*dof],derivative[axis*2+direction]));}}}
        let curl=[crate::c_add(grad[2][1],crate::c_scale(crate::longitudinal::derivative_value(kz,e[1],true),-1.)),
            crate::c_add(crate::longitudinal::derivative_value(kz,e[0],true),crate::c_scale(grad[2][0],-1.)),
            crate::c_add(grad[1][0],crate::c_scale(grad[0][1],-1.))];
        let centroid=std::array::from_fn(|axis|cell.iter().map(|&node|mesh.nodes[node][axis]).sum::<f64>()/3.);
        let raw=if let Some(value)=constants.get(tag){value.clone()}else{compiled.evaluate(*tag,parameters,frequency,centroid,&format!("mode material cell {cell_index}"))?};
        let material=crate::prepared_hcurl2d_model::material_symbols(&plan,config,derived.as_ref(),&raw,&context)?;
        let h=plan.recovery.as_ref().ok_or("Propagation-mode H(curl) recovery is missing.")?
            .apply_curl_with_domain_sources(curl,&material,&HashMap::new())?;
        for &node in cell{for axis in 0..3{magnetic[node][axis]=crate::c_add(magnetic[node][axis],h[axis]);}magnetic_counts[node]+=1;}
    }
    for node in 0..mesh.nodes.len(){if node_counts[node]>0{for value in &mut electric[node]{*value=crate::c_scale(*value,1./node_counts[node] as f64);}}
        if magnetic_counts[node]>0{for value in &mut magnetic[node]{*value=crate::c_scale(*value,1./magnetic_counts[node] as f64);}}}
    let mut fields=HashMap::new();
    for (axis,name) in ["Ex","Ey","Ez"].into_iter().enumerate(){fields.insert(name.into(),field(electric.iter().map(|row|row[axis].re).collect(),electric.iter().map(|row|row[axis].im).collect(),"V/m"));}
    for (axis,name) in ["Hx","Hy","Hz"].into_iter().enumerate(){fields.insert(name.into(),field(magnetic.iter().map(|row|row[axis].re).collect(),magnetic.iter().map(|row|row[axis].im).collect(),"A/m"));}
    let _=(study_id,Family::Hcurl);Ok(fields)
}

fn fields_for_mode(package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,study_id:&str,formulation:&str,
    frequency:f64,kz:C,parameters:&HashMap<String,crate::model_expression::Scalar>,mode:&crate::eigenfrequency::PencilMode,
    cell_densities:Option<&[Vec<f64>]>)->Result<ModeFieldValues,String>{
    let declaration=crate::propagation_mode_assembly::formulation(package,formulation)?;
    match crate::propagation_mode_assembly::family(package,formulation)?{
        crate::propagation_mode_assembly::Family::Scalar=>{
            let pressure=expand_nodal_scalar(model,map,mesh,&mode.vector)?;
            let fields=HashMap::from([("p".to_string(),field(pressure.iter().map(|v|v.re).collect(),pressure.iter().map(|v|v.im).collect(),"Pa"))]);
            let cell_nodes=mesh.triangles.iter().map(|(nodes,tag)|(*nodes,*tag)).collect::<Vec<_>>();
            let cell_fields=declared_scalar_recoveries(package,formulation,&pressure,&cell_nodes,cell_densities,kz,frequency)?;
            Ok(ModeFieldValues{fields,cell_fields})
        }
        crate::propagation_mode_assembly::Family::Vector=>{
            let components=declaration.get("fieldComponents").and_then(Value::as_u64).unwrap_or(3) as usize;
            if components!=3{return Err("Propagation-mode plane-strain parent requires three displacement components.".into());}
            let displacement=expand_nodal_vector(model,map,mesh,&mode.vector,components)?;
            let fields=["ux","uy","uz"].into_iter().enumerate().map(|(axis,name)|(name.into(),field(
                displacement.iter().skip(axis).step_by(components).map(|value|value.re).collect(),
                displacement.iter().skip(axis).step_by(components).map(|value|value.im).collect(),"m"))).collect();
            let cell_fields=declared_vector_recovery(package,model,map,mesh,formulation,frequency,kz,parameters,&displacement)?;
            Ok(ModeFieldValues{fields,cell_fields})
        }
        crate::propagation_mode_assembly::Family::Hcurl=>Ok(ModeFieldValues{fields:hcurl_fields(package,model,map,mesh,study_id,formulation,frequency,kz,parameters,&mode.vector)?,cell_fields:HashMap::new()}),
    }
}

pub(crate) fn solve_case(package:&Value,model:&Value,map:&Value,mesh:&MeshData,study_id:&str,case_index:usize,case:&Value,
    settings:&Value,field_files:bool)->Result<Vec<Value>,String>{
    let profile=package.get("id").and_then(Value::as_str).ok_or("Propagation-mode scenario package ID is missing.")?;
    if model.pointer("/scenario/profileId").and_then(Value::as_str)!=Some(profile){return Err("Propagation-mode model and scenario package differ.".into());}
    let study=model.pointer("/model/study").and_then(Value::as_array).and_then(|studies|studies.iter().find(|row|row.get("id").and_then(Value::as_str)==Some(study_id)))
        .filter(|row|row.get("type").and_then(Value::as_str)==Some("propagationMode")).ok_or("Selected propagationMode Study is missing or has the wrong type.")?;
    let physics=model.pointer("/model/physics").ok_or("Propagation-mode Physics is missing.")?;
    let formulation=settings.get("fieldFormulation").and_then(Value::as_str).filter(|value|!value.is_empty()).ok_or("Propagation-mode fieldFormulation is required.")?;
    if study.get("physicsId").and_then(Value::as_str)!=physics.get("id").and_then(Value::as_str)||physics.get("formulation").and_then(Value::as_str)!=Some(formulation){
        return Err("Propagation-mode Study, Physics and formulation do not agree.".into());
    }
    let contract=crate::propagation_mode_assembly::formulation(package,formulation)?;
    let operator=contract.get("operatorFormulation").and_then(Value::as_str).unwrap_or(formulation);
    if package.pointer(&format!("/weakForms/{operator}/dimension")).and_then(Value::as_u64)!=Some(2){return Err("Propagation-mode operator must be a declared 2D formulation.".into());}
    let parameters=crate::study_case_parameters::resolve(model,case,"Propagation-mode")?;
    let frequency=case.get("frequencyHz").and_then(Value::as_f64).filter(|value|value.is_finite()&&*value>0.).ok_or("Propagation-mode case frequencyHz must be positive and finite.")?;
    let shift=case.get("shift").ok_or("Propagation-mode case shift is missing.")?;
    let target=C{re:shift.get("realRadPerMetre").and_then(Value::as_f64).filter(|value|value.is_finite()).ok_or("Propagation-mode real shift is invalid.")?,
        im:shift.get("imaginaryRadPerMetre").and_then(Value::as_f64).filter(|value|value.is_finite()).ok_or("Propagation-mode imaginary shift is invalid.")?};
    let mode_count=settings.get("modeCount").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).filter(|value|*value>0).ok_or("Propagation-mode modeCount is invalid.")?;
    let krylov=settings.get("krylovDimension").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).filter(|value|*value>=mode_count).ok_or("Propagation-mode krylovDimension is invalid.")?;
    let tolerance=settings.get("residualTolerance").and_then(Value::as_f64).filter(|value|value.is_finite()&&*value>0.&&*value<1.).ok_or("Propagation-mode residualTolerance is invalid.")?;
    let quadrature=settings.get("constitutiveQuadrature").and_then(Value::as_str).ok_or("Propagation-mode constitutiveQuadrature is missing.")?;
    let backend=settings.get("solverBackend").and_then(Value::as_str).ok_or("Propagation-mode solverBackend is missing.")?;
    let ordering=settings.get("solverOrdering").and_then(Value::as_str).ok_or("Propagation-mode solverOrdering is missing.")?;
    let threads=settings.get("solverThreads").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).filter(|value|*value>0).ok_or("Propagation-mode solverThreads is invalid.")?;
    let order=settings.get("elementOrder").and_then(Value::as_u64).ok_or("Propagation-mode elementOrder is missing.")?;
    let mesh=mesh2d(mesh)?;
    // The geometric inverse length can be far below the longitudinal scale
    // of an optical or acoustic mode. Sampling the quadratic operator only
    // at 1/L then subtracts nearly equal P(kz) matrices to recover P2 and can
    // lose enough precision to reject every physical Ritz pair. The user's
    // search shift is the available scenario-neutral scale for kz itself.
    let scale=sparse_scale(&mesh.nodes)?.max(target.re.hypot(target.im));
    let modal_model=crate::transient_case::eigenfrequency_model(package,formulation,model)?;
    let (p0,p1,p2)=crate::propagation_mode_assembly::assemble_polynomial(package,&modal_model,map,&mesh,study_id,formulation,frequency,
        order,quadrature,backend,ordering,threads,&parameters,scale,&mut SolverCache::default())?;
    let modes=crate::eigenfrequency::solve_quadratic_sparse_polynomial(&p0,&p1,&p2,target,mode_count,krylov,tolerance,
        backend,ordering,threads,&mut SolverCache::default())?;
    let density_recovery=if field_files{scalar_recovery_densities(package,&modal_model,map,&mesh,formulation,frequency,&parameters)?}else{None};
    let mut documents=Vec::with_capacity(modes.len());
    for (mode_index,mode) in modes.iter().enumerate(){
        let recovered=if field_files{fields_for_mode(package,&modal_model,map,&mesh,study_id,formulation,frequency,mode.eigenvalue,&parameters,mode,density_recovery.as_deref())?}
            else{ModeFieldValues::default()};
        documents.push(json!({"schema":"fullmeta-propagation-mode/1","studyType":"propagationMode","studyId":study_id,
            "caseIndex":case_index,"modeIndex":mode_index,"frequencyHz":frequency,
            "longitudinalWavenumber":{"realRadPerMetre":mode.eigenvalue.re,"imaginaryRadPerMetre":mode.eigenvalue.im,"unit":"rad/m"},
            "relativeResidual":mode.relative_residual,"dimension":2,"nodes":mesh.nodes,
            "triangles":mesh.triangles.iter().map(|(nodes,tag)|json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>(),
            "fields":recovered.fields,"cellFields":recovered.cell_fields,
            "resultFields":contract.get("resultFields").cloned().unwrap_or_else(||json!([]))}));
    }
    Ok(documents)
}

pub(crate) fn run_request(path:&Path,profile:&str)->Result<(),String>{
    let request=load_json(path,"request")?;
    if request.get("schema").and_then(Value::as_str)!=Some("fullmeta-propagation-mode-case/1")
        ||request.get("studyType").and_then(Value::as_str)!=Some("propagationMode")
        ||request.get("solver").and_then(Value::as_str)!=Some(profile){return Err("Propagation-mode request schema, Study type or solver identity is invalid.".into());}
    let base=path.parent().unwrap_or(Path::new("."));
    let model=load_json(&request_path(&request,base,"modelFile")?,"model")?;
    crate::preflight_client::validate(&model).map_err(|error|format!("Model validation:\n{error}"))?;
    let map=load_json(&request_path(&request,base,"modelMapFile")?,"mesh map")?;
    let scenario=crate::model_schema::scenario_metadata_for_profile(profile)?;
    let dimension=scenario.pointer("/profile/dimension").and_then(Value::as_u64).ok_or("Propagation-mode scenario dimension is missing.")? as usize;
    let mesh_text=fs::read_to_string(request_path(&request,base,"meshFile")?).map_err(|error|format!("Could not read propagation-mode mesh: {error}"))?;
    let mesh=MeshData::parse(dimension,&mesh_text)?;
    let case_index=request.get("caseIndex").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).ok_or("Propagation-mode caseIndex is invalid.")?;
    let study_id=request.get("studyId").and_then(Value::as_str).filter(|value|!value.is_empty()).ok_or("Propagation-mode studyId is missing.")?;
    let settings=request.get("solverSettings").ok_or("Propagation-mode solverSettings are missing.")?;
    let field_files=request.get("fieldFiles").and_then(Value::as_bool).ok_or("Propagation-mode fieldFiles must be boolean.")?;
    let case=request.get("case").ok_or("Propagation-mode parameter case is missing.")?;
    let documents=solve_case(&crate::scenario_package::load_installed(profile)?,&model,&map,&mesh,study_id,case_index,case,settings,field_files)?;
    let output=request_path(&request,base,"outputDirectory")?;fs::create_dir_all(&output).map_err(|error|format!("Could not create propagation-mode output directory: {error}"))?;
    for (mode_index,document) in documents.iter().enumerate(){
        let name=format!("case-{case_index:06}-mode-{mode_index:06}.json");
        if field_files{let mut file=fs::OpenOptions::new().write(true).create_new(true).open(output.join(&name)).map_err(|error|format!("Could not create propagation-mode artifact: {error}"))?;
            serde_json::to_writer(&mut file,document).map_err(|error|format!("Could not encode propagation-mode artifact: {error}"))?;file.flush().map_err(|error|format!("Could not flush propagation-mode artifact: {error}"))?;}
        println!("FULLMETA_PROPAGATION_MODE {}",json!({"caseIndex":case_index,"modeIndex":mode_index,"name":if field_files{Some(name)}else{None},
            "frequencyHz":document["frequencyHz"],"longitudinalWavenumber":document["longitudinalWavenumber"],"relativeResidual":document["relativeResidual"]}));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transient_execution::MeshData;
    use crate::transient_mesh::NodalCell;

    fn acoustic_case()->(Value,Value,Value,MeshData,Value){
        let package:Value=serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let model=json!({"scenario":{"profileId":"acoustic-2d","dimension":2,"physics":"acoustics"},"model":{
            "parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"guide","type":"propagationMode","physicsId":"wave",
                "useLongitudinalWavenumber":false,
                "longitudinalWavenumber":{"real":quantity("1000","rad/m"),"imaginary":quantity("0","rad/m")}}],
            "materials":[{"id":"fluid","selectionId":"domain","density":quantity("1","kg/m^3"),
                "soundSpeed":quantity("1","m/s"),"lossFactor":"0"}],
            "physics":{"id":"wave","type":"acoustics","formulation":"pressure","frequency":quantity("1","Hz")}
        }});
        let map=json!({"schema":"fullmeta-mesh-map/6","dimension":2,
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],"boundarySelections":[],"domainCellSelections":{}});
        let mesh=MeshData{dimension:2,nodes:vec![[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]],
            cells:vec![NodalCell{nodes:vec![0,1,2],tag:1}],facets:Vec::new(),integration_edges:Vec::new()};
        let settings=json!({"fieldFormulation":"pressure","elementOrder":1,"constitutiveQuadrature":"standard",
            "solverBackend":"faer","solverOrdering":"amd","solverThreads":1,"modeCount":1,"krylovDimension":6,
            "residualTolerance":1e-6});
        (package,model,map,mesh,settings)
    }

    fn square_mesh(divisions:usize)->MeshData{
        let mut nodes=Vec::with_capacity((divisions+1)*(divisions+1));
        for j in 0..=divisions{for i in 0..=divisions{nodes.push([i as f64/divisions as f64,j as f64/divisions as f64,0.]);}}
        let index=|i:usize,j:usize|j*(divisions+1)+i;
        let mut cells=Vec::with_capacity(2*divisions*divisions);
        for j in 0..divisions{for i in 0..divisions{
            let (a,b,c,d)=(index(i,j),index(i+1,j),index(i+1,j+1),index(i,j+1));
            cells.push(NodalCell{nodes:vec![a,b,c],tag:1});cells.push(NodalCell{nodes:vec![a,c,d],tag:1});
        }}
        let mut facets=Vec::with_capacity(4*divisions);
        for i in 0..divisions{facets.push((vec![index(i,0),index(i+1,0)],2));facets.push((vec![index(i,divisions),index(i+1,divisions)],2));}
        for j in 0..divisions{facets.push((vec![index(0,j),index(0,j+1)],2));facets.push((vec![index(divisions,j),index(divisions,j+1)],2));}
        MeshData{dimension:2,nodes,cells,facets,integration_edges:Vec::new()}
    }

    fn cross_section_map()->Value{
        json!({"schema":"fullmeta-mesh-map/6","dimension":2,
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],
            "boundarySelections":[{"selectionId":"outer","physicalTag":2}],"domainCellSelections":{}})
    }

    fn propagation_settings(formulation:&str,krylov:usize)->Value{
        json!({"fieldFormulation":formulation,"elementOrder":1,"constitutiveQuadrature":"standard",
            "solverBackend":"faer","solverOrdering":"amd","solverThreads":1,"modeCount":1,"krylovDimension":krylov,
            "residualTolerance":1e-6})
    }

    #[test]
    fn end_to_end_acoustic_case_emits_recoverable_direct_kz_mode_artifact(){
        let (package,model,map,mesh,settings)=acoustic_case();
        let case=json!({"frequencyHz":1.,"shift":{"realRadPerMetre":6.2,"imaginaryRadPerMetre":0.},
            "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
        let artifacts=solve_case(&package,&model,&map,&mesh,"guide",0,&case,&settings,true).unwrap();
        assert_eq!(artifacts.len(),1);
        let artifact=&artifacts[0];
        assert_eq!(artifact["schema"],"fullmeta-propagation-mode/1");
        assert_eq!(artifact["studyType"],"propagationMode");
        assert_eq!(artifact["longitudinalWavenumber"]["unit"],"rad/m");
        let kz=artifact["longitudinalWavenumber"]["realRadPerMetre"].as_f64().unwrap();
        assert!((kz-std::f64::consts::TAU).abs()<1e-6,"expected guide mode kz=omega/c=2pi, got {kz}");
        assert!(artifact["longitudinalWavenumber"]["imaginaryRadPerMetre"].as_f64().unwrap().abs()<1e-6);
        assert!(artifact["relativeResidual"].as_f64().unwrap()<1e-6);
        assert_eq!(artifact["fields"]["p"]["real"].as_array().unwrap().len(),3);
        let axial=artifact.pointer("/cellFields/vz").expect("contract-declared axial velocity should be recoverable");
        assert_eq!(axial["unit"],"m/s");
        let local_real=axial["real"][0].as_array().unwrap();
        let local_imag=axial["imag"][0].as_array().unwrap();
        let pressure_real=artifact["fields"]["p"]["real"].as_array().unwrap();
        let pressure_imag=artifact["fields"]["p"]["imag"].as_array().unwrap();
        for local in 0..3 {
            assert!((local_real[local].as_f64().unwrap()-pressure_real[local].as_f64().unwrap()).abs()<1e-7);
            assert!((local_imag[local].as_f64().unwrap()-pressure_imag[local].as_f64().unwrap()).abs()<1e-7);
        }
    }

    #[test]
    fn acoustic_rectangular_guide_dispersion_refines_and_reaches_cutoff(){
        let dimensionless_frequency=4.0_f64;
        let expected=(dimensionless_frequency.powi(2)-std::f64::consts::PI.powi(2)).sqrt();
        let mut dispersion_errors=Vec::new();
        for divisions in [4,8]{
            let (package,model,map,_,_)=acoustic_case();
            let case=json!({"frequencyHz":dimensionless_frequency/std::f64::consts::TAU,
                "shift":{"realRadPerMetre":expected,"imaginaryRadPerMetre":0.},
                "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
            let modes=solve_case(&package,&model,&map,&square_mesh(divisions),"guide",0,&case,
                &propagation_settings("pressure",24),false).unwrap();
            assert_eq!(modes.len(),1);
            let mode=&modes[0];
            let real=mode.pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
            let imaginary=mode.pointer("/longitudinalWavenumber/imaginaryRadPerMetre").and_then(Value::as_f64).unwrap();
            assert!(real>0.&&imaginary.abs()<1e-6,"expected a propagating first transverse mode, got kz={real}+i{imaginary}");
            assert!(mode["relativeResidual"].as_f64().unwrap()<1e-6);
            dispersion_errors.push((real-expected).abs());
        }
        assert!(dispersion_errors[1]<dispersion_errors[0],
            "unit-square rigid-guide mode should converge to kz=sqrt(k0^2-pi^2)={expected}: coarse={}, fine={}",
            dispersion_errors[0],dispersion_errors[1]);

        let mut cutoff_magnitudes=Vec::new();
        for divisions in [4,8]{
            let (package,model,map,_,_)=acoustic_case();
            let case=json!({"frequencyHz":std::f64::consts::PI/std::f64::consts::TAU,
                "shift":{"realRadPerMetre":0.,"imaginaryRadPerMetre":0.5},
                "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
            let modes=solve_case(&package,&model,&map,&square_mesh(divisions),"guide",0,&case,
                &propagation_settings("pressure",24),false).unwrap();
            assert_eq!(modes.len(),1);
            let mode=&modes[0];
            let real=mode.pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
            let imaginary=mode.pointer("/longitudinalWavenumber/imaginaryRadPerMetre").and_then(Value::as_f64).unwrap();
            assert!(real.abs()<1e-6&&imaginary>0.,"at the first transverse cutoff, the conforming mode should approach kz=0 through the evanescent branch; got {real}+i{imaginary}");
            assert!(mode["relativeResidual"].as_f64().unwrap()<1e-6);
            cutoff_magnitudes.push(real.hypot(imaginary));
        }
        assert!(cutoff_magnitudes[1]<cutoff_magnitudes[0],
            "the cutoff branch should approach kz=0 under transverse refinement: coarse={}, fine={}",
            cutoff_magnitudes[0],cutoff_magnitudes[1]);

        let dirichlet_expected=(25.0-2.0*std::f64::consts::PI.powi(2)).sqrt();
        let mut dirichlet_errors=Vec::new();
        for divisions in [4,8]{
            let (package,mut model,_,_,_)=acoustic_case();
            model["model"]["physics"]["dirichlet"]=json!({"id":"zero-pressure-wall","type":"dirichlet",
                "selectionId":"outer","value":{"expression":"0","unit":"Pa"}});
            let map=cross_section_map();
            let case=json!({"frequencyHz":5.0/std::f64::consts::TAU,
                "shift":{"realRadPerMetre":dirichlet_expected,"imaginaryRadPerMetre":0.},
                "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
            let modes=solve_case(&package,&model,&map,&square_mesh(divisions),"guide",0,&case,
                &propagation_settings("pressure",24),false).unwrap();
            assert_eq!(modes.len(),1);
            let mode=&modes[0];
            let real=mode.pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
            let imaginary=mode.pointer("/longitudinalWavenumber/imaginaryRadPerMetre").and_then(Value::as_f64).unwrap();
            assert!(real>0.&&imaginary.abs()<1e-6,"homogeneous pressure-Dirichlet guide mode should propagate, got kz={real}+i{imaginary}");
            assert!(mode["relativeResidual"].as_f64().unwrap()<1e-6);
            dirichlet_errors.push((real-dirichlet_expected).abs());
        }
        assert!(dirichlet_errors[1]<dirichlet_errors[0],
            "homogeneous pressure-Dirichlet guide mode should converge to kz=sqrt(k0^2-2*pi^2)={dirichlet_expected}: coarse={}, fine={}",
            dirichlet_errors[0],dirichlet_errors[1]);
    }

    #[test]
    fn acoustic_mode_keeps_impedance_operator_but_drops_incident_load(){
        let (package,base_model,_,_,_)=acoustic_case();
        let map=cross_section_map();
        let mesh=mesh2d(&square_mesh(4)).unwrap();
        let mut impedance_model=base_model.clone();
        impedance_model["model"]["physics"]["matched-wall"]=json!({"id":"matched-wall","type":"impedance",
            "selectionId":"outer","impedance":{"expression":"1","unit":"Pa*s/m"}});
        let mut driven_model=impedance_model.clone();
        driven_model["model"]["physics"]["matched-wall"]["incidentField"]=json!({"expression":"1","unit":"Pa"});
        driven_model["model"]["physics"]["matched-wall"]["polarization"]=json!([{"expression":"1","unit":"1"}]);
        let free=crate::transient_case::eigenfrequency_model(&package,"pressure",&base_model).unwrap();
        let impedance=crate::transient_case::eigenfrequency_model(&package,"pressure",&impedance_model).unwrap();
        let driven=crate::transient_case::eigenfrequency_model(&package,"pressure",&driven_model).unwrap();
        assert_eq!(impedance,driven,"the incident amplitude must be removed from the source-free mode model");
        assert_eq!(driven.pointer("/model/physics/matched-wall/type").and_then(Value::as_str),Some("impedance"),
            "the impedance boundary operator must remain in the modal model");
        assert!(driven.pointer("/model/physics/matched-wall/incidentField").is_none());
        assert!(driven.pointer("/model/physics/matched-wall/polarization").is_none());

        let parameters=HashMap::new();
        let frequency=4.0/std::f64::consts::TAU;
        let scale=sparse_scale(&mesh.nodes).unwrap();
        let free_operator=crate::propagation_mode_assembly::assemble_polynomial(&package,&free,&map,&mesh,"guide","pressure",
            frequency,1,"standard","faer","amd",1,&parameters,scale,&mut SolverCache::default()).unwrap();
        let operator=crate::propagation_mode_assembly::assemble_polynomial(&package,&impedance,&map,&mesh,"guide","pressure",
            frequency,1,"standard","faer","amd",1,&parameters,scale,&mut SolverCache::default()).unwrap();
        let driven_operator=crate::propagation_mode_assembly::assemble_polynomial(&package,&driven,&map,&mesh,"guide","pressure",
            frequency,1,"standard","faer","amd",1,&parameters,scale,&mut SolverCache::default()).unwrap();
        let raw_operator=crate::propagation_mode_assembly::assemble_polynomial(&package,&impedance_model,&map,&mesh,"guide","pressure",
            frequency,1,"standard","faer","amd",1,&parameters,scale,&mut SolverCache::default()).unwrap();
        assert!(crate::eigenfrequency::sparse_matrices_close(&operator.0,&driven_operator.0,1e-12).unwrap());
        let modal_changes= !crate::eigenfrequency::sparse_matrices_close(&operator.0,&free_operator.0,1e-12).unwrap();
        let raw_changes= !crate::eigenfrequency::sparse_matrices_close(&raw_operator.0,&free_operator.0,1e-12).unwrap();
        assert!(modal_changes,"the impedance operator must change the source-free master matrix (raw boundary change: {raw_changes})");
    }

    #[test]
    fn parameter_sweep_case_rebuilds_propagation_operator_from_its_staged_material(){
        let (package,base_model,map,mesh,_)=acoustic_case();
        let sound_speed_dimension=crate::model_expression::unit("m/s").unwrap().1;
        let mut roots=Vec::new();
        for speed in [1.,2.] {
            let mut model=base_model.clone();
            model["model"]["parameters"]=json!([{"id":"speed","symbol":"c0","expression":speed.to_string(),"unit":"m/s"}]);
            model["model"]["materials"][0]["soundSpeed"]=json!({"expression":"c0","unit":"m/s"});
            let case=json!({"frequencyHz":1.,"shift":{"realRadPerMetre":std::f64::consts::TAU/speed*1.001,"imaginaryRadPerMetre":0.},
                "globalParameters":{"c0":speed},"globalParameterDimensions":{"c0":sound_speed_dimension},
                "parameterOverrides":model["model"]["parameters"]});
            let mode=solve_case(&package,&model,&map,&mesh,"guide",roots.len(),&case,
                &propagation_settings("pressure",6),false).unwrap().remove(0);
            roots.push(mode.pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap());
        }
        assert!((roots[0]-std::f64::consts::TAU).abs()<1e-7,"c0=1: kz={}",roots[0]);
        assert!((roots[1]-std::f64::consts::PI).abs()<1e-7,"c0=2: kz={}",roots[1]);
    }

    #[test]
    fn end_to_end_full_polarization_em_case_emits_field_artifact(){
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let frequency=5.*299_792_458.0/std::f64::consts::TAU;
        let model=json!({"scenario":{"profileId":"em-2d","dimension":2,"physics":"electromagnetics"},"model":{
            "parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"guide","type":"propagationMode","physicsId":"wave",
                "useLongitudinalWavenumber":false,
                "longitudinalWavenumber":{"real":quantity("1000","rad/m"),"imaginary":quantity("0","rad/m")}}],
            "materials":[{"id":"vacuum","selectionId":"domain","relativePermittivity":"1","relativePermeability":"1",
                "conductivity":quantity("0","S/m")}],
            "physics":{"id":"wave","type":"electromagnetics","formulation":"Full","frequency":quantity(&frequency.to_string(),"Hz"),
                "pec":{"id":"pec","type":"dirichlet","selectionId":"outer","value":quantity("0","V/m"),
                    "fieldValues":[quantity("0","V/m"),quantity("0","V/m"),quantity("0","V/m")]}}
        }});
        let case=json!({"frequencyHz":frequency,"shift":{"realRadPerMetre":4.1,"imaginaryRadPerMetre":0.},
            "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
        let modes=solve_case(&package,&model,&cross_section_map(),&square_mesh(10),"guide",0,&case,
            &propagation_settings("Full",100),true).unwrap();
        assert_eq!(modes.len(),1);
        let artifact=&modes[0];
        let kz=artifact.pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
        let expected=(25.-std::f64::consts::PI.powi(2)).sqrt();
        assert!((kz-expected).abs()<0.15,"PEC square-guide TE10 mode should approach kz={expected} rad/m, got {kz}");
        let coarse=solve_case(&package,&model,&cross_section_map(),&square_mesh(5),"guide",0,&case,
            &propagation_settings("Full",100),false).unwrap();
        let coarse_kz=coarse[0].pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
        assert!((kz-expected).abs()<(coarse_kz-expected).abs(),
            "isotropic PEC-guide TE10 kz should converge with transverse mesh refinement: coarse={coarse_kz}, fine={kz}, expected={expected}");
        assert!(artifact["relativeResidual"].as_f64().unwrap()<1e-6);
        for field in ["Ex","Ey","Ez","Hx","Hy","Hz"]{
            assert_eq!(artifact["fields"][field]["real"].as_array().unwrap().len(),121,"field {field}");
        }

        for selected in ["Ez","Hz"] {
            let mut scalar_model=model.clone();
            scalar_model["model"]["physics"]["formulation"]=json!(selected);
            let error=solve_case(&package,&scalar_model,&cross_section_map(),&square_mesh(10),"guide",0,&case,
                &propagation_settings(selected,100),false).unwrap_err();
            assert!(error.contains("requires the Full formulation"),"{selected}: {error}");
        }
        let below_cutoff_k0=2.5_f64;
        let expected_decay=(std::f64::consts::PI.powi(2)-below_cutoff_k0.powi(2)).sqrt();
        let mut below_cutoff_model=model.clone();
        below_cutoff_model["model"]["physics"]["frequency"]=quantity(&(below_cutoff_k0/std::f64::consts::TAU).to_string(),"Hz");
        let below_cutoff_case=json!({"frequencyHz":below_cutoff_k0/std::f64::consts::TAU,
            "shift":{"realRadPerMetre":0.,"imaginaryRadPerMetre":expected_decay},
            "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
        let below_cutoff=solve_case(&package,&below_cutoff_model,&cross_section_map(),&square_mesh(10),"guide",0,
            &below_cutoff_case,&propagation_settings("Full",100),false).unwrap();
        assert_eq!(below_cutoff.len(),1);
        let decay=below_cutoff[0].pointer("/longitudinalWavenumber/imaginaryRadPerMetre").and_then(Value::as_f64).unwrap();
        assert!(decay>0.&&(decay-expected_decay).abs()<0.15,
            "below-cutoff TE10 should have the direct positive evanescent kz branch near i*{expected_decay}, got {decay}");
        assert!(below_cutoff[0]["relativeResidual"].as_f64().unwrap()<1e-6);

        let mut lossy_model=model.clone();
        lossy_model["model"]["materials"][0]["conductivity"]=quantity("1e-4","S/m");
        let lossy_case=json!({"frequencyHz":frequency,"shift":{"realRadPerMetre":expected,"imaginaryRadPerMetre":-0.01},
            "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
        let lossy_modes=solve_case(&package,&lossy_model,&cross_section_map(),&square_mesh(10),"guide",0,&lossy_case,
            &propagation_settings("Full",100),true).unwrap();
        assert_eq!(lossy_modes.len(),1);
        let lossy_kz_real=lossy_modes[0].pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
        let lossy_kz_imag=lossy_modes[0].pointer("/longitudinalWavenumber/imaginaryRadPerMetre").and_then(Value::as_f64).unwrap();
        assert!((lossy_kz_real-expected).abs()<0.15,"lossy TE10 mode should retain its propagating real part, got {lossy_kz_real}");
        assert!(lossy_kz_imag < -1e-5,"passive conductivity should retain a signed attenuating imaginary kz, got {lossy_kz_imag}");
        assert!(lossy_modes[0]["relativeResidual"].as_f64().unwrap()<1e-6);
    }

    #[cfg(feature="rivrs-backend")]
    #[test]
    fn em2d_full_propagation_resolves_si_scale_modes_with_rivrs(){
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let c0=299_792_458.0;
        let k0=5.0e6;
        let width=1.0e-6;
        let frequency=k0*c0/std::f64::consts::TAU;
        let model=json!({"scenario":{"profileId":"em-2d","dimension":2,"physics":"electromagnetics"},"model":{
            "parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"guide","type":"propagationMode","physicsId":"wave"}],
            "materials":[{"id":"vacuum","selectionId":"domain","relativePermittivity":"1","relativePermeability":"1",
                "conductivity":quantity("0","S/m")}],
            "physics":{"id":"wave","type":"electromagnetics","formulation":"Full","frequency":quantity(&frequency.to_string(),"Hz"),
                "pec":{"id":"pec","type":"dirichlet","selectionId":"outer","value":quantity("0","V/m"),
                    "fieldValues":[quantity("0","V/m"),quantity("0","V/m"),quantity("0","V/m")]}}
        }});
        let case=json!({"frequencyHz":frequency,"shift":{"realRadPerMetre":3.9e6,"imaginaryRadPerMetre":0.},
            "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
        let mut mesh=square_mesh(8);
        for node in &mut mesh.nodes { node[0]*=width; node[1]*=width; }
        let settings=json!({"fieldFormulation":"Full","elementOrder":1,"constitutiveQuadrature":"standard",
            "solverBackend":"rivrs","solverOrdering":"metis","solverThreads":1,"modeCount":1,"krylovDimension":32,
            "residualTolerance":1e-6});
        let modes=solve_case(&package,&model,&cross_section_map(),&mesh,"guide",0,&case,&settings,false)
            .unwrap_or_else(|error|panic!("SI-scale Full EM2D mode solve failed: {error}"));
        assert_eq!(modes.len(),1);
        let mode=&modes[0];
        let kz=mode.pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
        let kz_imag=mode.pointer("/longitudinalWavenumber/imaginaryRadPerMetre").and_then(Value::as_f64).unwrap();
        let expected=(k0*k0-(std::f64::consts::PI/width).powi(2)).sqrt();
        assert!((kz-expected).abs()<0.03*expected,
            "SI-scale PEC square-guide TE10 mode should approach kz={expected} rad/m, got {kz}");
        assert!(kz_imag.abs()<1e-4*expected,"lossless guide mode should remain real, got Im(kz)={kz_imag}");
        assert!(mode["relativeResidual"].as_f64().unwrap()<1e-6);
    }

    #[test]
    fn end_to_end_elastic_plane_strain_parent_emits_three_component_mode(){
        let package:Value=serde_json::from_str(include_str!("scenarios/elastic2d.json")).unwrap();
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let frequency=0.05;let omega=std::f64::consts::TAU*frequency;
        let expected=omega/(12.0_f64.sqrt());
        let model=json!({"scenario":{"profileId":"elastic-2d","dimension":2,"physics":"elasticity"},"model":{
            "parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"guide","type":"propagationMode","physicsId":"wave"}],
            "materials":[{"id":"solid","selectionId":"domain","density":quantity("1","kg/m^3"),
                "youngModulus":quantity("12","Pa"),"poissonRatio":"0.25","densityLossFactor":"0"}],
            "physics":{"id":"wave","type":"elasticity","formulation":"planeStrain","frequency":quantity("1","Hz")}
        }});
        let case=json!({"frequencyHz":frequency,"shift":{"realRadPerMetre":expected*1.001,"imaginaryRadPerMetre":0.},
            "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
        let mesh=square_mesh(2);let map=cross_section_map();
        let modes=solve_case(&package,&model,&map,&mesh,"guide",0,&case,
            &propagation_settings("planeStrain",30),true).unwrap();
        assert_eq!(modes.len(),1);
        let artifact=&modes[0];
        let kz=artifact.pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
        assert!((kz-expected).abs()<0.15*expected,"long-wave rod extension should approach kz=omega/sqrt(E/rho)={expected}, got {kz}");
        assert!(artifact["relativeResidual"].as_f64().unwrap()<1e-6);
        for field in ["ux","uy","uz"]{assert_eq!(artifact["fields"][field]["real"].as_array().unwrap().len(),9,"field {field}");}
        for field in ["sigma_xx","sigma_yy","sigma_zz","sigma_xy","sigma_yz","sigma_xz"]{
            let recovered=artifact.pointer(&format!("/cellFields/{field}")).unwrap_or_else(||panic!("missing recovered stress {field}"));
            assert_eq!(recovered["unit"],"Pa","stress field {field} unit");
            assert_eq!(recovered["real"].as_array().unwrap().len(),8,"one local stress sample row per triangle for {field}");
            assert_eq!(recovered["real"][0].as_array().unwrap().len(),3,"P1 stress cell samples for {field}");
        }
        assert_eq!(artifact["resultFields"].as_array().unwrap().iter().filter_map(|field|field["symbol"].as_str()).filter(|symbol|symbol.starts_with("sigma_")).count(),6);

        let mu=12.0_f64/(2.0*(1.0+0.25));
        let expected_shear=omega/mu.sqrt();
        let shear_case=json!({"frequencyHz":frequency,"shift":{"realRadPerMetre":expected_shear*1.001,"imaginaryRadPerMetre":0.},
            "parameterOverrides":[],"globalParameters":{},"globalParameterDimensions":{}});
        let shear_modes=solve_case(&package,&model,&map,&square_mesh(2),"guide",0,&shear_case,
            &propagation_settings("planeStrain",30),true).unwrap();
        assert_eq!(shear_modes.len(),1);
        let shear=&shear_modes[0];
        let shear_kz=shear.pointer("/longitudinalWavenumber/realRadPerMetre").and_then(Value::as_f64).unwrap();
        assert!((shear_kz-expected_shear).abs()<0.15*expected_shear,
            "homogeneous isotropic shear branch should approach kz=omega/sqrt(mu)={expected_shear}, got {shear_kz}");
        assert!(shear["relativeResidual"].as_f64().unwrap()<1e-6);
        let axial_norm=shear.pointer("/fields/uz/real").and_then(Value::as_array).unwrap().iter()
            .map(|value|value.as_f64().unwrap().powi(2)).sum::<f64>().sqrt();
        let transverse_norm=["ux","uy"].into_iter().flat_map(|field|
            shear.pointer(&format!("/fields/{field}/real")).and_then(Value::as_array).unwrap().iter())
            .map(|value|value.as_f64().unwrap().powi(2)).sum::<f64>().sqrt();
        assert!(transverse_norm>0.&&axial_norm<1e-5*transverse_norm,
            "the shear eigenmode should be transversely polarized, with ||uz||={axial_norm} and transverse norm {transverse_norm}");
    }
}
