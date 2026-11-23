//! Bind a declared 2D finite-longitudinal operator to the shared H(curl) × H1
//! direct-sum discretization. The scenario package selects the form, sources,
//! boundary laws, material bindings, and output fields; this adapter contains
//! no profile or physics-name dispatch.
use std::collections::{HashMap,HashSet};
use serde_json::{Value,json};
use crate::{C,SolverCache};
use crate::model_expression::Scalar;
use crate::prepared_case::{CaseKind,PreparedCase};
use crate::result_contract::ResultData;
use crate::scenario_plan::ScenarioPlan;

fn topology_policy(value:&Value)->Result<crate::simplex_topology::TopologyPolicy,String>{
    use crate::simplex_topology::{DeterminantTolerance,TopologyPolicy};
    let flag=|key:&str|value.get(key).and_then(Value::as_bool).ok_or_else(||format!("H(curl) topology {key} must be boolean."));
    let tol=value.get("determinantTolerance").ok_or("H(curl) determinant tolerance is required.")?;
    let threshold=tol.get("value").and_then(Value::as_f64).filter(|v|v.is_finite()&&*v>=0.).ok_or("H(curl) determinant threshold is invalid.")?;
    let reject_equal=tol.get("rejectEqual").and_then(Value::as_bool).ok_or("H(curl) determinant rejectEqual is required.")?;
    let determinant_tolerance=match tol.get("kind").and_then(Value::as_str){
        Some("absolute")=>DeterminantTolerance::Absolute{minimum:threshold,reject_equal},
        Some("relative")=>DeterminantTolerance::Relative{fraction:threshold,reject_equal},
        _=>return Err("Unsupported H(curl) determinant tolerance.".into()),
    };
    Ok(TopologyPolicy{reject_duplicate_cells:flag("rejectDuplicateCells")?,reject_non_manifold_facets:flag("rejectNonManifoldFacets")?,
        require_all_nodes_used:flag("requireAllNodesUsed")?,require_nonempty_cells:flag("requireNonemptyCells")?,determinant_tolerance:Some(determinant_tolerance)})
}

pub(super) fn periodic_dof_map(model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,
    dof_topology:&crate::hcurl2d::DofTopology)->Result<crate::hcurl2d::DofMap,String>{
    let entities=mesh.boundary_edges.iter().map(|(edge,tag)|(edge.to_vec(),*tag)).collect::<Vec<_>>();
    let nodal=crate::scalar2d::periodic_dof_map_for_entities(model,map,&mesh.nodes,&entities,2)?;
    let mut classes=crate::solver_common::PhaseUnionFind::new(dof_topology.count);
    for pair in &nodal.pairs{
        let mut phase:Option<C>=None;
        for (&lower,&upper) in &pair.node_map{
            let lower_weight=*nodal.node_weights.get(lower).ok_or("Periodic lower node has no phase weight.")?;
            let upper_weight=*nodal.node_weights.get(upper).ok_or("Periodic upper node has no phase weight.")?;
            let relation=upper_weight.div(lower_weight);
            if let Some(previous)=phase{
                if (previous.re-relation.re).hypot(previous.im-relation.im)>1e-9*previous.re.hypot(previous.im).max(relation.re.hypot(relation.im)).max(1.){
                    return Err("Periodic pair has inconsistent phase factors across its nodes.".into());
                }
            }else{phase=Some(relation);}
            classes.constrain(dof_topology.edge_count+lower,dof_topology.edge_count+upper,relation)?;
        }
        let lower_edges=mesh.boundary_edges.iter().filter(|(_,tag)|*tag==pair.lower_tag).map(|(edge,_)|[edge[0].min(edge[1]),edge[0].max(edge[1])]).collect::<Vec<_>>();
        let upper_edges=mesh.boundary_edges.iter().filter(|(_,tag)|*tag==pair.upper_tag).map(|(edge,_)|[edge[0].min(edge[1]),edge[0].max(edge[1])]).collect::<HashSet<_>>();
        let mut matched=HashSet::new();
        for [a,b] in lower_edges{
            let mapped=[*pair.node_map.get(&a).ok_or("Periodic lower edge has an unmatched endpoint.")?,
                *pair.node_map.get(&b).ok_or("Periodic lower edge has an unmatched endpoint.")?];
            let target=[mapped[0].min(mapped[1]),mapped[0].max(mapped[1])];
            if !upper_edges.contains(&target){return Err("Periodic edge traces do not match under the declared node correspondence.".into());}
            if !matched.insert(target){return Err("Periodic correspondence maps more than one lower edge to an upper edge.".into());}
            let low=dof_topology.edge([a,b]).ok_or("Periodic lower edge has no H(curl) DOF.")?;
            let high=dof_topology.edge(mapped).ok_or("Periodic upper edge has no H(curl) DOF.")?;
            let orientation=if mapped[0]<mapped[1]{1.}else{-1.};
            classes.constrain(low,high,crate::c_scale(phase.unwrap_or(C{re:1.,im:0.}),orientation))?;
        }
        if matched.len()!=upper_edges.len(){return Err("Periodic pair does not match every upper edge.".into());}
    }
    let (representatives,weights,count)=classes.mapping();
    let result=crate::hcurl2d::DofMap{representatives,weights,count};
    result.validate(dof_topology.count)?;
    Ok(result)
}

fn gauss5()->[(f64,f64);5]{[
    (0.046910077030668,0.118463442528095),(0.230765344947158,0.239314335249683),
    (0.5,0.284444444444444),(0.769234655052842,0.239314335249683),(0.953089922969332,0.118463442528095)]}

fn add_load_coefficients(plan:&ScenarioPlan,symbols:&HashMap<String,C>)->Result<HashMap<String,crate::hcurl2d::CoefficientSample>,String>{
    let scalar=plan.volume.evaluate_coefficients(symbols)?;
    let mut out=scalar.iter().map(|(name,&value)|(name.clone(),crate::hcurl2d::CoefficientSample::Scalar(value))).collect::<HashMap<_,_>>();
    for (name,matrix) in &plan.coefficient_matrices{
        let value=matrix.evaluate(&scalar)?;
        let (rows,columns)=matrix.shape();
        out.insert(name.clone(),crate::hcurl2d::CoefficientSample::Matrix{rows,columns,values:value});
    }
    Ok(out)
}

pub(super) fn material_symbols(plan:&ScenarioPlan,config:&Value,derived:Option<&crate::coefficient_expr::CoefficientExpressions>,
    material:&HashMap<String,Scalar>,context:&HashMap<String,Scalar>)->Result<HashMap<String,C>,String>{
    let mut symbols=crate::prepared_vector::bind_material(plan,material,context,"material")?;
    let constants=config.get("boundaryConstants").and_then(Value::as_object).ok_or("H(curl) boundaryConstants are required.")?;
    for (name,value) in constants{
        let value=value.as_f64().filter(|v|v.is_finite()).ok_or("H(curl) boundary constants must be finite.")?;
        symbols.insert(name.clone(),C{re:value,im:0.});
    }
    if let Some(derived)=derived{
        let values=derived.evaluate(&symbols)?;symbols.extend(values);
    }
    Ok(symbols)
}

enum Execution {
    Solved(ResultData),
    Assembled(crate::hcurl2d::LinearSystem),
}

pub(crate) fn solve(case:&PreparedCase,package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,
    parameters:&HashMap<String,Scalar>,cache:&mut SolverCache)->Result<ResultData,String>{
    match execute(case,package,model,map,mesh,parameters,cache,false)? {
        Execution::Solved(data)=>Ok(data),
        Execution::Assembled(_)=>Err("Prepared H(curl) solve returned an assembly-only result.".into()),
    }
}

pub(crate) fn assemble(case:&PreparedCase,package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,
    parameters:&HashMap<String,Scalar>,cache:&mut SolverCache)->Result<crate::hcurl2d::LinearSystem,String>{
    match execute(case,package,model,map,mesh,parameters,cache,true)? {
        Execution::Assembled(system)=>Ok(system),
        Execution::Solved(_)=>Err("Prepared H(curl) assembly unexpectedly solved the operator.".into()),
    }
}

fn execute(case:&PreparedCase,package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,
    parameters:&HashMap<String,Scalar>,cache:&mut SolverCache,assemble_only:bool)->Result<Execution,String>{
    let frequency=match case.kind{CaseKind::Frequency{frequency_hz,..}=>frequency_hz,_=>return Err("Prepared H(curl) execution requires a frequency case.".into())};
    if !frequency.is_finite()||frequency<=0. {return Err("Finite-longitudinal EM2D harmonic recovery requires a positive frequency.".into());}
    if case.settings.element_order!=1{return Err("The declared H(curl) direct-sum space currently supports first-order elements only.".into());}
    let kz=crate::longitudinal::prescribed_wavenumber(package,model,&case.study_id,&case.settings.formulation,parameters)?;
    if let CaseKind::Frequency{case:prepared_case,..}=&case.kind{crate::longitudinal::validate_prepared_case_value(prepared_case,kz)?;}
    let operator_formulation=crate::longitudinal::operator_formulation(package,&case.settings.formulation);
    let source_formulation=crate::longitudinal::source_formulation(package,&case.settings.formulation);
    let boundary_formulation=crate::longitudinal::boundary_input_formulation(package,&case.settings.formulation);
    let mut plan=ScenarioPlan::from_package_with_source_formulation(package,operator_formulation,source_formulation)?;
    plan.set_longitudinal_wavenumber(kz)?;
    let config=package.get("preparedHcurl2d").ok_or("preparedHcurl2d configuration is required for the declared numerical family.")?;
    let derived=config.get("derivedInputs").map(crate::coefficient_expr::CoefficientExpressions::from_json).transpose()?;
    if config.get("meshFormat").and_then(Value::as_str)!=Some("fullmeta-v2"){return Err("H(curl) direct-sum requires fullmeta-v2 mesh data.".into());}
    if config.get("boundaryQuadrature").and_then(Value::as_str)!=Some("gauss5"){return Err("Unsupported H(curl) boundary quadrature.".into());}
    let policy=config.get("topology").ok_or("H(curl) topology policy is required.")?;
    let connectivity=mesh.triangles.iter().map(|(cell,_)|*cell).collect::<Vec<_>>();
    let topology=crate::simplex_topology::triangles(&mesh.nodes,&connectivity,&[],topology_policy(policy)?)?;
    let dof_topology=crate::hcurl2d::DofTopology::triangles(mesh.nodes.len(),&connectivity)?;
    let points=crate::quadrature::triangle(&case.settings.quadrature)?.into_iter().map(|q|crate::hcurl2d::Point{barycentric:[q[0],q[1],q[2]],weight:q[3]}).collect::<Vec<_>>();
    let coordinates=mesh.nodes.iter().map(|point|[point[0],point[1]]).collect::<Vec<_>>();
    let selection_plan=plan.selections.as_ref().ok_or("H(curl) selection plan is required.")?;
    let source_by_cell=selection_plan.uses_cell_map("domainSources",map);
    let selected=selection_plan.bind(model,map,&HashMap::from([
        ("domainSources".into(),mesh.triangles.iter().map(|(_,tag)|*tag).collect()),
        ("materials".into(),mesh.triangles.iter().map(|(_,tag)|*tag).collect()),
        ("boundaries".into(),mesh.boundary_edges.iter().map(|(_,tag)|*tag).collect())]))?;
    let source_data=crate::domain_source::prepare_routes(&plan.domain_sources,selected.get("domainSources").map(Vec::as_slice))?;
    let mut route_samples=source_data.iter().map(|_|Vec::<Vec<Vec<C>>>::with_capacity(mesh.triangles.len())).collect::<Vec<_>>();
    let material_rows=selected.get("materials").ok_or("H(curl) material selection is required.")?.iter().map(|row|(row.tag,row.row.clone())).collect();
    let compiled=crate::material::CompiledMaterialAssignments::compile(&case.profile,&material_rows)?;
    let constant=compiled.position_independent_values(parameters,frequency)?;
    let mut context=parameters.clone();context.insert("omega".into(),Scalar{value:std::f64::consts::TAU*frequency,dimension:[0.;5]});
    let mut cells=Vec::with_capacity(mesh.triangles.len());let mut cell_symbols=Vec::with_capacity(mesh.triangles.len());
    for (cell_index,(cell,tag)) in mesh.triangles.iter().enumerate(){
        let mut samples=Vec::with_capacity(points.len());let mut cell_route_samples=source_data.iter().map(|_|Vec::with_capacity(points.len())).collect::<Vec<_>>();
        for point in &points{
            let physical=std::array::from_fn(|axis|(0..3).map(|i|point.barycentric[i]*mesh.nodes[cell[i]][axis]).sum());
            let raw=if let Some(value)=constant.get(tag){value.clone()}else{compiled.evaluate(*tag,parameters,frequency,physical,&format!("H(curl) cell {cell_index}"))?};
            let symbols=material_symbols(&plan,config,derived.as_ref(),&raw,&context)?;
            for (route_index,(_,load_plan,sources)) in source_data.iter().enumerate(){
                let source_key=if source_by_cell{cell_index}else{*tag};
                cell_route_samples[route_index].push(crate::domain_source::sample_load(load_plan,sources,source_key,physical,parameters,Some(frequency),&symbols)?);
            }
            samples.push((*point,add_load_coefficients(&plan,&symbols)?));
        }
        for (all,one_cell) in route_samples.iter_mut().zip(cell_route_samples){all.push(one_cell);}
        let centroid=std::array::from_fn(|axis|cell.iter().map(|&node|mesh.nodes[node][axis]).sum::<f64>()/3.);
        let raw=if let Some(value)=constant.get(tag){value.clone()}else{compiled.evaluate(*tag,parameters,frequency,centroid,&format!("H(curl) cell {cell_index} centroid"))?};
        cell_symbols.push(material_symbols(&plan,config,derived.as_ref(),&raw,&context)?);
        cells.push(crate::hcurl2d::Cell{nodes:*cell,points:samples});
    }
    let selected_boundaries=selected.get("boundaries").ok_or("H(curl) boundary selection is required.")?;
    let inputs=crate::declared_boundary::compile_rows(package,boundary_formulation,selected_boundaries.iter().map(|row|(row.tag,row.row)))?;
    let boundary_tags=selected_boundaries.iter().map(|row|row.tag).collect::<Vec<_>>();
    let chosen=mesh.boundary_edges.iter().copied().filter(|(_,tag)|boundary_tags.contains(tag)).collect::<Vec<_>>();
    topology.bind_selected_facets(&chosen,&boundary_tags,crate::simplex_topology::SelectedFacetPolicy{
        ownership:crate::simplex_topology::SelectedFacetOwnership::ExteriorOnly,
        duplicates:crate::simplex_topology::SelectedFacetDuplicates::KeepAll,require_positive_tag:true})?;
    let dof_map=periodic_dof_map(model,map,mesh,&dof_topology)?;
    let essential_plan_by_kind=|kind:&str|plan.essential.get(kind).ok_or_else(||format!("No essential form is declared for boundary type {kind:?}."));
    let mut essential=HashMap::<usize,C>::new();let mut boundaries=Vec::new();
    let boundary_constants=config.get("boundaryConstants").and_then(Value::as_object).ok_or("H(curl) boundary constants are required.")?;
    let mut boundary_inputs=HashMap::from([("omega".into(),C{re:std::f64::consts::TAU*frequency,im:0.})]);
    for (name,value) in boundary_constants{boundary_inputs.insert(name.clone(),C{re:value.as_f64().ok_or("Boundary constant is invalid.")?,im:0.});}
    for (edge,tag) in chosen{
        let input=inputs.get(&tag).ok_or("Selected H(curl) boundary input was not compiled.")?;
        let facet=topology.facet(edge).ok_or("Selected H(curl) boundary has no topology entry.")?;
        let owner=facet.owners.first().ok_or("Selected H(curl) boundary has no owner.")?;
        let (normal,length)=crate::simplex_topology::edge_normal(&mesh.nodes,facet,0)?;
        let local_cell=mesh.triangles[owner.cell].0;
        let kind=selected_boundaries.iter().find(|row|row.tag==tag).and_then(|row|row.row.get("type")).and_then(Value::as_str).ok_or("H(curl) boundary type is required.")?;
        if input.form_key().is_none(){
            let essential_form=essential_plan_by_kind(kind)?;
            essential_form.require_tangential_moments()?;
            let lo=edge[0].min(edge[1]);let hi=edge[0].max(edge[1]);
            let dx=mesh.nodes[hi][0]-mesh.nodes[lo][0];let dy=mesh.nodes[hi][1]-mesh.nodes[lo][1];
            let mut edge_moment=C::default();let mut z_values=[C::default();2];
            let mut endpoint_values=[false;2];
            for (t,weight) in gauss5(){
                let p=[mesh.nodes[lo][0]*(1.-t)+mesh.nodes[hi][0]*t,mesh.nodes[lo][1]*(1.-t)+mesh.nodes[hi][1]*t,0.];
                let form=None;let values=input.evaluate(parameters,Some(p),&boundary_inputs,form)?;
                let raw=values.essential.ok_or("Essential boundary requires a scalar value.")?;
                let full=essential_form.moment_values(raw,values.essential_vector.as_deref(),"selected exterior edge")?;
                edge_moment=crate::c_add(edge_moment,crate::c_scale(crate::c_add(crate::c_scale(full[0],dx),crate::c_scale(full[1],dy)),weight));
            }
            for (slot,node) in [lo,hi].into_iter().enumerate(){
                let p=mesh.nodes[node];let values=input.evaluate(parameters,Some(p),&boundary_inputs,None)?;
                let raw=values.essential.ok_or("Essential boundary requires a scalar value.")?;
                let full=essential_form.moment_values(raw,values.essential_vector.as_deref(),"selected exterior edge endpoint")?;
                z_values[slot]=full[2];endpoint_values[slot]=true;
            }
            let edge_dof=dof_topology.edge(edge).ok_or("Selected boundary edge has no H(curl) DOF.")?;
            essential_form.insert_dof(&mut essential,dof_map.representatives[edge_dof],edge_moment,dof_map.weights[edge_dof],"selected exterior edge")?;
            for (slot,node) in [lo,hi].into_iter().enumerate(){if endpoint_values[slot]{
                let dof=dof_topology.edge_count+node;
                essential_form.insert_dof(&mut essential,dof_map.representatives[dof],z_values[slot],dof_map.weights[dof],"selected exterior edge endpoint")?;
            }}
        }else{
            let key=input.form_key().ok_or("Boundary form key disappeared.")?;
            let form=plan.boundaries.get(key).ok_or_else(||format!("H(curl) boundary form {key:?} is not declared."))?;
            for (t,weight) in gauss5(){
                let p=[mesh.nodes[edge[0]][0]*(1.-t)+mesh.nodes[edge[1]][0]*t,mesh.nodes[edge[0]][1]*(1.-t)+mesh.nodes[edge[1]][1]*t,0.];
                let input_values=input.evaluate(parameters,Some(p),&boundary_inputs,Some(form))?;
                let barycentric=local_cell.iter().map(|node|if *node==edge[0]{1.-t}else if *node==edge[1]{t}else{0.}).collect::<Vec<_>>();
                let barycentric:[f64;3]=barycentric.try_into().map_err(|_|"H(curl) boundary barycentric shape is invalid.")?;
                boundaries.push(crate::hcurl2d::BoundaryPoint{element:owner.cell,barycentric,normal,weight:length*weight,plan:form,
                    coefficients:input_values.coefficients,sources:input_values.sources});
            }
        }
    }
    let load_routes=source_data.iter().zip(&route_samples).map(|((_,load_plan,_),samples)|(&load_plan.form,samples.as_slice())).collect::<Vec<_>>();
    if assemble_only {
        return Ok(Execution::Assembled(crate::hcurl2d::assemble_system(&coordinates,&cells,&plan.volume,&boundaries,&essential,&load_routes,&dof_map,cache)?));
    }
    let solution=crate::hcurl2d::assemble(&coordinates,&cells,&plan.volume,&boundaries,&essential,&load_routes,&dof_map,
        &case.settings.backend,&case.settings.ordering,case.settings.threads,1e-7,cache)?;
    let mut series=HashMap::<String,Vec<C>>::new();
    for name in ["Ex","Ey","Ez","Hx","Hy","Hz"]{series.insert(name.into(),vec![C::default();mesh.nodes.len()]);}
    let hcurl_dofs=dof_topology;
    let mut counts=vec![0usize;mesh.nodes.len()];
    for (cell_index,cell) in cells.iter().enumerate(){
        let local_dofs=&hcurl_dofs.elements[cell_index];
        let gradients=crate::mesh2d::triangle_geometry(cell.nodes.map(|node|mesh.nodes[node]))?.1;
        for (local_node,&global_node) in cell.nodes.iter().enumerate(){
            let barycentric=std::array::from_fn(|axis|if axis==local_node{1.}else{0.});
            let (values,_)=crate::hcurl2d::basis(&coordinates,cell.nodes,barycentric,gradients)?;
            let mut e=[C::default();3];
            for (basis,dof) in values.iter().zip(local_dofs){
                let coefficient=C{re:solution.real[*dof],im:solution.imaginary[*dof]};
                for component in 0..3{e[component]=crate::c_add(e[component],crate::c_scale(coefficient,basis[component]));}
            }
            for (name,value) in ["Ex","Ey","Ez"].into_iter().zip(e){let target=series.get_mut(name).unwrap();target[global_node]=crate::c_add(target[global_node],value);}
        }
        for node in cell.nodes{counts[node]+=1;}
        let center=[1./3.;3];let (values,derivatives)=crate::hcurl2d::basis(&coordinates,cell.nodes,center,gradients)?;
        let mut e=[C::default();3];let mut gradient=[[C::default();2];3];
        for ((basis,derivative),dof) in values.iter().zip(&derivatives).zip(local_dofs){
            let coefficient=C{re:solution.real[*dof],im:solution.imaginary[*dof]};
            for component in 0..3{e[component]=crate::c_add(e[component],crate::c_scale(coefficient,basis[component]));
                for axis in 0..2{gradient[component][axis]=crate::c_add(gradient[component][axis],crate::c_scale(coefficient,derivative[component*2+axis]));}}
        }
        // Recover the physical curl of the trial field using ∂z = -i kz.
        // The opposite sign belongs to the sesquilinear test operator used in assembly.
        let curl=[crate::c_add(gradient[2][1],crate::c_scale(crate::longitudinal::derivative_value(kz,e[1],true),-1.)),
            crate::c_add(crate::longitudinal::derivative_value(kz,e[0],true),crate::c_scale(gradient[2][0],-1.)),
            crate::c_add(gradient[1][0],crate::c_scale(gradient[0][1],-1.))];
        let mut source_values=HashMap::new();
        let material=&cell_symbols[cell_index];let point=std::array::from_fn(|axis|cell.nodes.iter().map(|&node|mesh.nodes[node][axis]).sum::<f64>()/3.);
        let tag=mesh.triangles[cell_index].1;
        for (kind,_,sources) in &source_data{let source_key=if source_by_cell{cell_index}else{tag};source_values.insert(kind.clone(),sources.sample(source_key,point,parameters,Some(frequency))?);}
        let recovered=plan.recovery.as_ref().ok_or("H(curl) result requires a declared curl recovery.")?
            .apply_curl_with_domain_sources(curl,material,&source_values)?;
        for (name,value) in ["Hx","Hy","Hz"].into_iter().zip(recovered){
            for node in cell.nodes{let target=series.get_mut(name).unwrap();target[node]=crate::c_add(target[node],value);}
        }
    }
    for values in series.values_mut(){for (value,&count) in values.iter_mut().zip(&counts){if count>0{*value=crate::c_scale(*value,1./count as f64);}}}
    let primary_name=if case.settings.formulation=="Hz"{"Hz"}else{"Ez"};
    let primary=series.get(primary_name).cloned().ok_or("Selected primary EM field is missing.")?;
    let maximum=primary.iter().map(|value|value.re.hypot(value.im)).fold(0.,f64::max);
    series.insert("primary".into(),primary);
    let primary_unit=if primary_name=="Hz"{"A/m"}else{"V/m"};
    let cells_json=mesh.triangles.iter().map(|(nodes,tag)|json!({"nodes":nodes,"materialTag":tag})).collect::<Vec<_>>();
    let material_fields=crate::material::fields_by_cell(model,map,frequency,&mesh.nodes,&mesh.triangles,[0,1,2],[0.;3])?;
    let fields=series.iter().filter(|(name,_)|["Ex","Ey","Ez","Hx","Hy","Hz"].contains(&name.as_str())).map(|(name,value)|(name.clone(),value.clone())).collect::<HashMap<_,_>>();
    let domain=mesh.triangles.iter().map(|(nodes,tag)|crate::integration::Simplex{nodes:nodes.to_vec(),physical_tag:*tag,material_tag:Some(*tag),operator_id:None,element_index:None,adjacent_element_index:None}).collect::<Vec<_>>();
    let mut boundary_cells=Vec::new();
    for (edge,tag) in &mesh.boundary_edges{
        let material=mesh.triangles.iter().find(|(cell,_)|edge.iter().all(|node|cell.contains(node))).map(|(_,tag)|*tag).ok_or("H(curl) boundary edge has no adjacent material cell.")?;
        boundary_cells.push(crate::integration::Simplex{nodes:edge.to_vec(),physical_tag:*tag,material_tag:Some(material),operator_id:None,element_index:None,adjacent_element_index:None});
    }
    let component=model.pointer("/model").ok_or("H(curl) model component is missing.")?;
    let global=crate::integration::evaluate_model_variables(model,component,map,2,&mesh.nodes,&domain,&boundary_cells,&fields,frequency)?;
    let complex=global.iter().map(|(name,value)|Ok((name.clone(),C{re:value.get("re").and_then(Value::as_f64).ok_or("Global variable has no real value.")?,im:value.get("im").and_then(Value::as_f64).ok_or("Global variable has no imaginary value.")?}))).collect::<Result<HashMap<_,_>,String>>()?;
    let longitudinal=json!({"factor":"exp(-i*kz*z)","real":kz.re,"imaginary":kz.im,"unit":"rad/m","operatorFormulation":operator_formulation,"fieldValues":"cross-sectional amplitudes"});
    let data=ResultData{values:HashMap::from([
        ("frequency".into(),json!(frequency)),("formulation".into(),json!(case.settings.formulation)),("nodes".into(),json!(mesh.nodes)),("cells".into(),json!(cells_json)),
        ("materialFields".into(),Value::Object(material_fields)),("globalVariables".into(),Value::Object(global)),("backend".into(),json!(case.settings.backend)),
        ("ordering".into(),json!(case.settings.ordering)),("threads".into(),json!(case.settings.threads)),("quadrature".into(),json!(case.settings.quadrature)),
        ("residual".into(),json!(solution.residual)),("unknowns".into(),json!(solution.unknowns)),("maximum".into(),json!(maximum)),
        ("primaryComponent".into(),json!(primary_name)),("primaryUnit".into(),json!(primary_unit)),
        ("longitudinalWavenumberReal".into(),json!(kz.re)),("longitudinalWavenumberImaginary".into(),json!(kz.im)),("longitudinalDependence".into(),longitudinal),
    ]),complex,series,matrices:HashMap::new()};
    // Keep the solver object alive through recovery and variable evaluation;
    // this explicit use also documents that all derived fields came from it.
    Ok(Execution::Solved(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepared_case::{Output,SolverSettings};
    use serde_json::json;

    #[test]
    fn em2d_finite_kz_uniform_plane_wave_runs_contract_to_sparse_result() {
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        let kz=2.0_f64;
        let omega=299_792_458.0*kz;
        let frequency=omega/std::f64::consts::TAU;
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let model=json!({"scenario":{"profileId":"em-2d"},"model":{
            "parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],
            "mesh":{"periodicPairIds":[]},
            "study":[{"id":"harmonic","type":"frequencyDomain","physicsId":"wave",
                "longitudinalWavenumber":{"real":quantity("2","rad/m"),"imaginary":quantity("0","rad/m")}}],
            "materials":[{"id":"vacuum","selectionId":"solid","relativePermittivity":"1",
                "relativePermeability":"1","conductivity":quantity("0","S/m")}],
            "physics":{"id":"wave","formulation":"Full","pec":{
                "id":"pec","type":"dirichlet","selectionId":"outer","value":quantity("0","V/m"),
                "fieldValues":[quantity("1","V/m"),quantity("0","V/m"),quantity("0","V/m")]}}
        }});
        let map=json!({"physicalGroups":[{"selectionId":"solid","physicalTag":1}],
            "boundarySelections":[{"selectionId":"outer","physicalTag":2}],"domainCellSelections":{}});
        let mesh=crate::mesh2d::Mesh2D {
            nodes:vec![[0.,0.,0.],[1.,0.,0.],[1.,1.,0.],[0.,1.,0.],[0.5,0.5,0.]],
            triangles:vec![([0,1,4],1),([1,2,4],1),([2,3,4],1),([3,0,4],1)],
            boundary_edges:vec![([0,1],2),([1,2],2),([2,3],2),([3,0],2)],integration_edges:vec![]};
        let case=PreparedCase {request:json!({}),profile:"em-2d".into(),study_id:"harmonic".into(),
            kind:CaseKind::Frequency{frequency_hz:frequency,case:json!({"longitudinalWavenumber":{"real":kz,"imaginary":0.,"unit":"rad/m"}})},
            settings:SolverSettings{formulation:"Full".into(),element_order:1,quadrature:"standard".into(),
                backend:"faer".into(),ordering:"amd".into(),threads:1},output:Output::Frequency{sweep_csv:"result.csv".into()},
            model_path:Default::default(),mesh_path:Default::default(),map_path:Default::default(),output_directory:Default::default()};
        let parameters=crate::model_expression::resolve_parameters(&[],&model).unwrap();
        let mut cache=SolverCache::default();
        let result=solve(&case,&package,&model,&map,&mesh,&parameters,&mut cache).unwrap();
        assert!(result.values["residual"].as_f64().unwrap()<1e-7);
        for component in ["Ex","Ey","Ez","Hx","Hy","Hz"] {
            assert_eq!(result.series[component].len(),mesh.nodes.len());
        }
        let expected_hy=kz/(omega*1.25663706212e-6);
        for index in 0..mesh.nodes.len() {
            let close=|actual:C,expected:C|assert!((actual.re-expected.re).hypot(actual.im-expected.im)<1e-7*expected.re.abs().max(1.),"node {index}: actual {actual:?}, expected {expected:?}");
            close(result.series["Ex"][index],C{re:1.,im:0.});
            close(result.series["Ey"][index],C::default());
            close(result.series["Ez"][index],C::default());
            close(result.series["Hx"][index],C::default());
            close(result.series["Hy"][index],C{re:expected_hy,im:0.});
            close(result.series["Hz"][index],C::default());
        }
        assert_eq!(result.values["primaryComponent"],json!("Ez"));
        assert_eq!(result.values["primaryUnit"],json!("V/m"));
        assert_eq!(result.values["longitudinalDependence"]["factor"],json!("exp(-i*kz*z)"));

        let zero_case=PreparedCase {kind:CaseKind::Frequency{frequency_hz:frequency,case:json!({"longitudinalWavenumber":{"real":0.,"imaginary":0.,"unit":"rad/m"}})},..case.clone()};
        let mut zero_model=model.clone();
        zero_model["model"]["study"][0]["longitudinalWavenumber"]["real"]=quantity("0","rad/m");
        let zero_result=solve(&zero_case,&package,&zero_model,&map,&mesh,&parameters,&mut SolverCache::default())
            .unwrap_or_else(|error|panic!("Full polarization harmonic solve at kz=0: {error}"));
        assert!(zero_result.values["residual"].as_f64().unwrap()<1e-7);
        assert_eq!(zero_result.values["longitudinalWavenumberReal"],json!(0.));
        assert!(zero_result.series["Ex"].iter().any(|value|value.re.abs()>1e-8||value.im.abs()>1e-8),
            "zero-kz Full solve should retain its nontrivial electric field");

        // The Full contract declares both independent Maxwell current routes.
        let source=crate::longitudinal::source_formulation(&package,"Full");
        let declared=crate::scenario_plan::ScenarioPlan::from_package_with_source_formulation(
            &package,"fullPolarization",source).unwrap();
        let symbols=HashMap::from([("omega".into(),C{re:omega,im:0.}),
            ("muRelative".into(),C{re:1.,im:0.})]);
        let mut electric=vec![C{re:1.,im:0.};3];
        declared.domain_sources["domainSource"].scale(&mut electric,&symbols).unwrap();
        assert!((electric[0].re).abs()<1e-15);
        assert!((electric[0].im+1.25663706212e-6*omega).abs()<1e-12);
        let mut magnetic=vec![C{re:1.,im:0.};3];
        declared.domain_sources["additionalDomainSource"].scale(&mut magnetic,&symbols).unwrap();
        assert_eq!(magnetic[0],C{re:-1.,im:0.});

        let source_map=json!({"physicalGroups":[{"selectionId":"solid","physicalTag":1}],
            "boundarySelections":[{"selectionId":"outer","physicalTag":2}]});
        for (feature,unit) in [("domainSource","A/m^2"),("additionalDomainSource","V/m^2")] {
            let mut sourced=model.clone();
            sourced["model"]["physics"][feature]=json!({"id":feature,"type":feature,
                "selectionId":"solid","values":[quantity("1",unit),quantity("0",unit),quantity("0",unit)]});
            let response=solve(&case,&package,&sourced,&source_map,&mesh,&parameters,
                &mut SolverCache::default()).unwrap();
            assert!(response.values["residual"].as_f64().unwrap()<1e-7);
            let changed=["Ex","Ey","Ez","Hx","Hy","Hz"].iter().any(|field|
                result.series[*field].iter().zip(&response.series[*field]).any(|(old,new)|
                    (old.re-new.re).hypot(old.im-new.im)>1e-7));
            assert!(changed,"active {feature} did not affect the finite-kz field");
        }

        // A coefficient jump across an interior material interface must use
        // the same global Whitney trace DOF on both adjacent elements.
        let mut interface_model=model.clone();
        interface_model["model"]["materials"]=json!([
            {"id":"left","selectionId":"left","relativePermittivity":"1",
                "relativePermeability":"1","conductivity":quantity("0","S/m")},
            {"id":"right","selectionId":"right","relativePermittivity":"4",
                "relativePermeability":"1","conductivity":quantity("0","S/m")}
        ]);
        let interface_map=json!({"physicalGroups":[
                {"selectionId":"left","physicalTag":1},
                {"selectionId":"right","physicalTag":3}],
            "boundarySelections":[{"selectionId":"outer","physicalTag":2}],
            "domainCellSelections":{}});
        let mut interface_mesh=mesh.clone();
        for (_,tag) in interface_mesh.triangles.iter_mut().skip(2){*tag=3;}
        let interface_connectivity=interface_mesh.triangles.iter().map(|(cell,_)|*cell).collect::<Vec<_>>();
        let dofs=crate::hcurl2d::DofTopology::triangles(interface_mesh.nodes.len(),&interface_connectivity).unwrap();
        assert_eq!(dofs.edge_count,8,"the four-triangle fan has eight unique Whitney edge traces");
        assert_eq!(dofs.elements[0][1],dofs.edge([1,4]).unwrap());
        assert_eq!(dofs.elements[1][2],dofs.edge([1,4]).unwrap(),
            "the material-interface edge must be a single shared trace DOF");
        let interface_result=solve(&case,&package,&interface_model,&interface_map,&interface_mesh,
            &parameters,&mut SolverCache::default()).unwrap();
        assert!(interface_result.values["residual"].as_f64().unwrap()<1e-7);
        assert!(interface_result.values["maximum"].as_f64().unwrap()>0.5);
        assert!(interface_result.series.values().flatten().all(|value|value.re.is_finite()&&value.im.is_finite()));
    }
}
