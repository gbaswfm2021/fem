//! Validate and bind dimensioned global parameters from prepared spectral
//! Study cases. Eigenfrequency and longitudinal propagation share this path.
use crate::model_expression::{self,Scalar};
use serde_json::Value;
use std::collections::HashMap;

pub(crate) fn resolve(model:&Value,case:&Value,study_name:&str)->Result<HashMap<String,Scalar>,String>{
    let values=case.get("globalParameters").and_then(Value::as_object)
        .ok_or_else(||format!("{study_name} case globalParameters must be an object."))?;
    let dimensions=case.get("globalParameterDimensions").and_then(Value::as_object)
        .ok_or_else(||format!("{study_name} case globalParameterDimensions must be an object."))?;
    let overrides=case.get("parameterOverrides").and_then(Value::as_array)
        .ok_or_else(||format!("{study_name} case parameterOverrides must be an array."))?;
    if values.len()!=dimensions.len()||values.keys().any(|name|matches!(name.as_str(),"t"|"freq"|"frequency"|"omega")){
        return Err(format!("{study_name} case parameters are inconsistent or use a reserved solver symbol."));
    }
    if model.pointer("/model/parameters").and_then(Value::as_array)!=Some(overrides){
        return Err(format!("{study_name} case parameters differ from the staged model."));
    }
    let resolved=model_expression::resolve_parameters(overrides,model)?;
    if resolved.len()!=values.len(){return Err(format!("{study_name} case parameter values differ from the resolved model."));}
    for (name,expected) in values{
        let expected=expected.as_f64().filter(|value|value.is_finite())
            .ok_or_else(||format!("{study_name} parameter {name:?} must be finite."))?;
        let actual=resolved.get(name).ok_or_else(||format!("{study_name} parameter {name:?} is not defined in the staged model."))?;
        let scale=expected.abs().max(actual.value.abs()).max(1.);
        if (actual.value-expected).abs()>32.*f64::EPSILON*scale{
            return Err(format!("{study_name} parameter {name:?} value differs from its staged model."));
        }
        let axes=dimensions.get(name).and_then(Value::as_array).filter(|axes|axes.len()==5)
            .ok_or_else(||format!("{study_name} parameter {name:?} requires five dimensions."))?;
        for (axis,&actual_axis) in axes.iter().zip(actual.dimension.iter()){
            let expected_axis=axis.as_f64().filter(|value|value.is_finite())
                .ok_or_else(||format!("{study_name} parameter {name:?} dimensions must be finite."))?;
            if (expected_axis-actual_axis).abs()>1e-10{
                return Err(format!("{study_name} parameter {name:?} dimension differs from its staged model."));
            }
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests{
    use super::*;
    use serde_json::json;

    #[test]
    fn shared_spectral_case_binding_checks_staged_values_and_dimensions(){
        let model=json!({"scenario":{"profileId":"acoustic-2d"},"model":{"parameters":[{"id":"length","symbol":"L","expression":"2","unit":"m"}]}});
        let length=model_expression::unit("m").unwrap().1;
        let case=json!({"globalParameters":{"L":2.},"globalParameterDimensions":{"L":length},
            "parameterOverrides":model["model"]["parameters"]});
        assert_eq!(resolve(&model,&case,"Propagation-mode").unwrap()["L"].value,2.);
        let mut changed=case.clone();changed["globalParameters"]["L"]=json!(3.);
        assert!(resolve(&model,&changed,"Propagation-mode").unwrap_err().contains("value differs"));
        let mut wrong_dimension=case.clone();wrong_dimension["globalParameterDimensions"]["L"]=json!([0.,0.,0.,0.,0.]);
        assert!(resolve(&model,&wrong_dimension,"Eigenfrequency").unwrap_err().contains("dimension differs"));
        let mut stale_model=model.clone();stale_model["model"]["parameters"][0]["expression"]=json!("4");
        assert!(resolve(&stale_model,&case,"Propagation-mode").unwrap_err().contains("differ from the staged model"));
    }
}
