//! Scenario-neutral adapter from declared 2D harmonic operators to a sparse
//! longitudinal polynomial. The polynomial eigenvalue is `kz` itself after a
//! sparse companion linearization; no reciprocal pairing or `kz^2` solve is
//! assumed.
use crate::prepared_case::{CaseKind, Output, PreparedCase, SolverSettings};
use crate::solver_common::{C, SolverCache};
use crate::transient_step::SparseMatrix;
use crate::model_expression::Scalar;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy)]
pub(crate) enum Family { Scalar, Vector, Hcurl }

pub(crate) fn family(package:&Value,formulation:&str)->Result<Family,String>{
    let declaration=package.get("propagationModes").and_then(|value|value.get("formulations"))
        .and_then(|value|value.get(formulation)).ok_or_else(||format!("Propagation-mode formulation {formulation:?} is not declared by this scenario."))?;
    match declaration.get("numericalFamily").and_then(Value::as_str){
        Some("scalarSimplexP1")=>Ok(Family::Scalar),
        Some("vectorSimplexP1")=>Ok(Family::Vector),
        Some("deRhamTriangle")=>Ok(Family::Hcurl),
        Some(other)=>Err(format!("Unsupported propagation-mode numerical family {other:?}.")),
        None=>Err("Propagation-mode numericalFamily is required.".into()),
    }
}

pub(crate) fn formulation<'a>(package:&'a Value,formulation:&str)->Result<&'a Value,String>{
    let contract=package.get("propagationModes").ok_or("Scenario package does not declare propagationModes.")?;
    if let Some(declaration)=contract.get("formulations").and_then(|value|value.get(formulation)){
        return Ok(declaration);
    }
    let required=contract.get("requiredFormulation").and_then(Value::as_str);
    Err(required.map(|required|format!(
        "Propagation Mode requires the {required} formulation; selected {formulation:?}."
    )).unwrap_or_else(||format!(
        "Propagation-mode formulation {formulation:?} is not declared by this scenario."
    )))
}

fn model_at_kz(model:&Value,study_id:&str,kz:C)->Result<Value,String>{
    if !kz.re.is_finite()||!kz.im.is_finite(){return Err("Longitudinal operator sample must be finite.".into());}
    let mut result=model.clone();
    let rows=result.pointer_mut("/model/study").and_then(Value::as_array_mut).ok_or("Model studies are missing.")?;
    let study=rows.iter_mut().find(|row|row.get("id").and_then(Value::as_str)==Some(study_id)).ok_or("Propagation-mode Study is missing from the staged model.")?;
    // Propagation Mode samples the harmonic operator at explicit trial kz
    // values. It must override a persisted Study checkbox (which can be false
    // after switching from another Study type in the editor), otherwise the
    // declared operator silently resolves every sample at kz=0.
    study["useLongitudinalWavenumber"]=json!(true);
    study["longitudinalWavenumber"]=json!({
        "real":{"expression":kz.re.to_string(),"unit":"rad/m"},
        "imaginary":{"expression":kz.im.to_string(),"unit":"rad/m"}
    });
    Ok(result)
}

fn prepared_case(study_id:&str,profile:&str,formulation:&str,frequency:f64,order:u64,quadrature:&str,
    backend:&str,ordering:&str,threads:usize,kz:C)->PreparedCase{
    PreparedCase{
        request:json!({"studyId":study_id}),profile:profile.to_owned(),study_id:study_id.to_owned(),
        kind:CaseKind::Frequency{frequency_hz:frequency,case:json!({"longitudinalWavenumber":{"real":kz.re,"imaginary":kz.im,"unit":"rad/m"}})},
        settings:SolverSettings{formulation:formulation.to_owned(),element_order:order,quadrature:quadrature.to_owned(),
            backend:backend.to_owned(),ordering:ordering.to_owned(),threads},
        output:Output::Frequency{sweep_csv:"propagation-mode-assembly.csv".into()},
        model_path:PathBuf::new(),mesh_path:PathBuf::new(),map_path:PathBuf::new(),output_directory:PathBuf::new(),
    }
}

/// Assemble the scenario-declared harmonic operator at one `kz`. The same
/// selected mesh, materials, periodic reduction, essential constraints,
/// boundary matrices, and sparse master assembly are used at every sample.
pub(crate) fn assemble_at(
    package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,study_id:&str,
    formulation:&str,frequency:f64,order:u64,quadrature:&str,backend:&str,ordering:&str,
    threads:usize,parameters:&HashMap<String,Scalar>,kz:C,cache:&mut SolverCache,
)->Result<SparseMatrix,String>{
    if !frequency.is_finite()||frequency<=0.{return Err("Propagation-mode frequency must be positive and finite.".into());}
    if order!=1{return Err("The declared propagation-mode element adapter currently supports first-order elements only.".into());}
    let sample_model=model_at_kz(model,study_id,kz)?;
    let case=prepared_case(study_id,package.get("id").and_then(Value::as_str).ok_or("Scenario package ID is missing.")?,
        formulation,frequency,order,quadrature,backend,ordering,threads,kz);
    let family=family(package,formulation)?;
    let system=match family{
        Family::Scalar=>{
            let periodic=crate::scalar2d::periodic_dof_map(&sample_model,map,mesh)?;
            let assembled=crate::prepared_frequency_scalar::assemble(&case,package,&sample_model,map,
                crate::prepared_frequency_scalar::Mesh::Triangle{nodes:&mesh.nodes,cells:&mesh.triangles,facets:&mesh.boundary_edges},
                parameters,Some(&periodic),cache)?;
            assembled.matrix
        }
        Family::Vector=>crate::prepared_vector_model::assemble(&case,package,&sample_model,map,
            crate::prepared_vector_model::Mesh::Triangle(mesh),parameters,cache)?.matrix,
        Family::Hcurl=>crate::prepared_hcurl2d_model::assemble(&case,package,&sample_model,map,mesh,parameters,cache)?.matrix,
    };
    Ok(system)
}

/// Recover the sparse quadratic coefficient matrices from symmetric samples
/// around zero, then verify the declared degree with two independent samples.
/// The extra checks make an invalid `degree: 2` contract fail before solving.
pub(crate) fn assemble_polynomial(
    package:&Value,model:&Value,map:&Value,mesh:&crate::mesh2d::Mesh2D,study_id:&str,
    formulation:&str,frequency:f64,order:u64,quadrature:&str,backend:&str,ordering:&str,
    threads:usize,parameters:&HashMap<String,Scalar>,sample_scale:f64,cache:&mut SolverCache,
)->Result<(SparseMatrix,SparseMatrix,SparseMatrix),String>{
    let propagation_contract=package.get("propagationModes").ok_or("Scenario propagationModes contract is missing.")?;
    let declaration=propagation_contract.get("formulations")
        .and_then(|value|value.get(formulation)).ok_or("Propagation-mode formulation declaration is missing.")?;
    let degree=declaration.get("longitudinalPolynomialDegree").or_else(||propagation_contract.get("longitudinalPolynomialDegree"));
    if degree.and_then(Value::as_u64)!=Some(2){
        return Err("The sparse polynomial adapter currently requires a declared longitudinalPolynomialDegree of 2.".into());
    }
    let h=sample_scale;
    let sample=|scale:f64,cache:&mut SolverCache|assemble_at(package,model,map,mesh,study_id,formulation,frequency,order,
        quadrature,backend,ordering,threads,parameters,C{re:scale*h,im:0.},cache);
    let negative=sample(-1.,cache)?;
    let zero=sample(0.,cache)?;
    let positive=sample(1.,cache)?;
    let (p0,p1,p2)=crate::eigenfrequency::quadratic_polynomial_from_samples(&negative,&zero,&positive,h)?;
    let tolerance=declaration.get("polynomialVerificationTolerance").or_else(||propagation_contract.get("polynomialVerificationTolerance")).and_then(Value::as_f64)
        .filter(|value|value.is_finite()&&*value>0.).unwrap_or(1e-8);
    for scale in [-2.,2.] {
        let actual=sample(scale,cache)?;
        let expected=crate::eigenfrequency::evaluate_sparse_polynomial(&p0,&p1,&p2,C{re:scale*h,im:0.})?;
        if !crate::eigenfrequency::sparse_matrices_close(&actual,&expected,tolerance)?{
            return Err(format!("The declared quadratic longitudinal operator failed its independent sample check at kz={}.",scale*h));
        }
    }
    Ok((p0,p1,p2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture()->(Value,Value,Value,crate::mesh2d::Mesh2D,HashMap<String,Scalar>){
        let package:Value=serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let q=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let model=json!({"scenario":{"profileId":"acoustic-2d"},"model":{
            "parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],"mesh":{"periodicPairIds":[]},
            "study":[{"id":"guide","type":"propagationMode","physicsId":"wave"}],
            "materials":[{"id":"fluid","selectionId":"domain","density":q("1","kg/m^3"),"soundSpeed":q("1","m/s"),"lossFactor":"0"}],
            "physics":{"id":"wave","type":"acoustics","formulation":"pressure"}
        }});
        let map=json!({"schema":"fullmeta-mesh-map/6","dimension":2,
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],"boundarySelections":[]});
        let mesh=crate::mesh2d::Mesh2D{nodes:vec![[0.,0.,0.],[1.,0.,0.],[0.,1.,0.]],triangles:vec![([0,1,2],1)],
            boundary_edges:Vec::new(),integration_edges:Vec::new()};
        let parameters=HashMap::new();
        (package,model,map,mesh,parameters)
    }

    #[test]
    fn homogeneous_acoustic_cross_section_recovers_both_kz_branches_from_assembled_operator(){
        let (package,model,map,mesh,parameters)=fixture();
        let omega=std::f64::consts::TAU;let expected=omega;
        let (p0,p1,p2)=assemble_polynomial(&package,&model,&map,&mesh,"guide","pressure",1.,1,"standard","faer","amd",1,
            &parameters,1.,&mut SolverCache::default()).unwrap();
        let constant=vec![C{re:1.,im:0.};p0.len()];
        for kz in [-expected,expected]{
            let operator=crate::eigenfrequency::evaluate_sparse_polynomial(&p0,&p1,&p2,C{re:kz,im:0.}).unwrap();
            let residual=operator.iter().map(|row|{
                let value=row.iter().fold(C::default(),|sum,(&column,&entry)|crate::c_add(sum,crate::c_mul(entry,constant[column])));
                value.re*value.re+value.im*value.im
            }).sum::<f64>().sqrt();
            let scale=operator.iter().flat_map(|row|row.values()).map(|value|value.re.hypot(value.im)).sum::<f64>();
            assert!(residual/scale.max(f64::MIN_POSITIVE)<1e-10,"kz={kz}: normalized constant-mode residual {residual}");
        }
        assert!(p1.iter().all(|row|row.is_empty()),"reciprocal scalar acoustics happens to have no linear term; the generic solver must not rely on that special case");
    }

    #[test]
    fn acoustic_eigensolve_reports_kz_itself_at_fixed_frequency(){
        let (package,model,map,mesh,parameters)=fixture();let magnitude=std::f64::consts::TAU;
        let (p0,p1,p2)=assemble_polynomial(&package,&model,&map,&mesh,"guide","pressure",1.,1,"standard","faer","amd",1,
            &parameters,1.,&mut SolverCache::default()).unwrap();
        for sign in [-1.,1.] {
            let target=sign*magnitude;
            let modes=crate::eigenfrequency::solve_quadratic_sparse_polynomial(&p0,&p1,&p2,C{re:target,im:0.},1,6,1e-7,
                "faer","amd",1,&mut SolverCache::default()).unwrap();
            assert_eq!(modes.len(),1);
            assert!((modes[0].eigenvalue.re-target).abs()<1e-7,"expected independently targeted kz={target}, got {:?}",modes[0].eigenvalue);
            assert!(modes[0].eigenvalue.im.abs()<1e-7);
            assert!(modes[0].relative_residual<1e-7);
        }
    }

    #[test]
    fn propagation_sampling_overrides_a_disabled_stale_study_wavenumber(){
        let (package,mut model,map,mesh,parameters)=fixture();
        model["model"]["study"][0]["useLongitudinalWavenumber"]=json!(false);
        model["model"]["study"][0]["longitudinalWavenumber"]=json!({
            "real":{"expression":"1000","unit":"rad/m"},
            "imaginary":{"expression":"0","unit":"rad/m"}
        });

        let (p0,p1,p2)=assemble_polynomial(&package,&model,&map,&mesh,"guide","pressure",1.,1,"standard","faer","amd",1,
            &parameters,1.,&mut SolverCache::default()).unwrap();
        let expected=std::f64::consts::TAU;
        for kz in [-expected,expected] {
            let operator=crate::eigenfrequency::evaluate_sparse_polynomial(&p0,&p1,&p2,C{re:kz,im:0.}).unwrap();
            let constant=vec![C{re:1.,im:0.};p0.len()];
            let residual=operator.iter().map(|row|{
                let value=row.iter().fold(C::default(),|sum,(&column,&entry)|crate::c_add(sum,crate::c_mul(entry,constant[column])));
                value.re*value.re+value.im*value.im
            }).sum::<f64>().sqrt();
            let scale=operator.iter().flat_map(|row|row.values()).map(|value|value.re.hypot(value.im)).sum::<f64>();
            assert!(residual/scale.max(f64::MIN_POSITIVE)<1e-10,"kz={kz}: stale Study setting altered sampled operator");
        }
    }

    fn cross_scenario_fixture(profile:&str,formulation:&str)->(Value,Value,Value,crate::mesh2d::Mesh2D,HashMap<String,Scalar>){
        let package:Value=serde_json::from_str(match profile{
            "em-2d"=>include_str!("scenarios/em2d.json"),
            "elastic-2d"=>include_str!("scenarios/elastic2d.json"),_=>unreachable!()}).unwrap();
        let q=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let material=if profile=="em-2d"{json!({"id":"material","selectionId":"domain","relativePermittivity":"1",
            "relativePermeability":"1","conductivity":q("0","S/m")})}
            else{json!({"id":"material","selectionId":"domain","density":q("1","kg/m^3"),
                "youngModulus":q("12","Pa"),"poissonRatio":"0.25","densityLossFactor":"0"})};
        let model=json!({"scenario":{"profileId":profile},"model":{"parameters":[],"variables":[],"pointwiseVariables":[],"integrations":[],
            "mesh":{"periodicPairIds":[]},"study":[{"id":"guide","type":"propagationMode","physicsId":"wave"}],
            "materials":[material],"physics":{"id":"wave","formulation":formulation}}});
        let map=json!({"schema":"fullmeta-mesh-map/6","dimension":2,
            "physicalGroups":[{"selectionId":"domain","physicalTag":1}],"boundarySelections":[],"domainCellSelections":{}});
        let mesh=crate::mesh2d::Mesh2D{nodes:vec![[0.,0.,0.],[1.,0.,0.],[1.,1.,0.],[0.,1.,0.],[0.5,0.5,0.]],
            triangles:vec![([0,1,4],1),([1,2,4],1),([2,3,4],1),([3,0,4],1)],
            boundary_edges:vec![([0,1],2),([1,2],2),([2,3],2),([3,0],2)],integration_edges:Vec::new()};
        (package,model,map,mesh,HashMap::new())
    }

    #[test]
    fn em2d_full_polarization_uses_the_same_sparse_kz_polynomial_adapter(){
        let (package,model,map,mesh,parameters)=cross_scenario_fixture("em-2d","Full");
        let (p0,p1,p2)=assemble_polynomial(&package,&model,&map,&mesh,"guide","Full",1e8,1,"standard","faer","amd",1,
            &parameters,0.5,&mut SolverCache::default()).unwrap();
        assert_eq!(p0.len(),p1.len());assert_eq!(p0.len(),p2.len());
        assert!(p0.iter().flatten().any(|(&column,value)|column<p0.len()&&value.re.is_finite()));
        assert!(p1.iter().flatten().any(|(&column,value)|column<p1.len()&&(value.re!=0.||value.im!=0.)),
            "full-polarization finite-kz operator should retain its generic linear kz coupling");
    }

    #[test]
    fn propagation_formulation_error_uses_the_contract_required_choice(){
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        for selected in ["Ez","Hz"] {
            let error=formulation(&package,selected).unwrap_err();
            assert!(error.contains("requires the Full formulation"),"{selected}: {error}");
        }
        assert!(formulation(&package,"Full").is_ok());
    }

    #[test]
    fn elastic_plane_strain_parent_keeps_linear_kz_terms_in_the_sparse_operator(){
        let (package,model,map,mesh,parameters)=cross_scenario_fixture("elastic-2d","planeStrain");
        let (p0,p1,p2)=assemble_polynomial(&package,&model,&map,&mesh,"guide","planeStrain",1.,1,"standard","faer","amd",1,
            &parameters,0.5,&mut SolverCache::default()).unwrap();
        assert_eq!(p0.len(),p1.len());assert_eq!(p0.len(),p2.len());
        assert!(p1.iter().flatten().any(|(&column,value)|column<p1.len()&&(value.re!=0.||value.im!=0.)),
            "plane-strain-parent elasticity must preserve its generally nonsymmetric linear kz operator");
    }
}
