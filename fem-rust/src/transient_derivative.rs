//! Exact symbolic time derivatives for smooth real boundary expressions.
//! Unsupported or nonsmooth functions return None; they are never checked
//! using a finite time step. The dimensioned scale symbol represents one
//! second, so the derivative remains in the original quantity's units.
use crate::model_expression::{self, Scalar};
use serde_json::{Value,json};
use std::collections::HashMap;

const SCALE:&str="__fullmeta_time_derivative_scale";
fn original(node:&Value)->Result<String,String>{
    match node.get("kind").and_then(Value::as_str){
        Some("number")=>Ok(node["value"].to_string()),
        Some("symbol")=>node["name"].as_str().map(str::to_owned).ok_or("Invalid symbol node.".into()),
        Some("unary")=>Ok(format!("({}{})",node["op"].as_str().ok_or("Invalid unary operator.")?,original(&node["value"])?)),
        Some("binary")=>Ok(format!("({}{}{})",original(&node["left"])?,node["op"].as_str().ok_or("Invalid binary operator.")?,original(&node["right"])?)),
        Some("call")=>Ok(format!("{}({})",node["name"].as_str().ok_or("Invalid function name.")?,
            node["args"].as_array().ok_or("Invalid function arguments.")?.iter().map(original).collect::<Result<Vec<_>,_>>()?.join(","))),
        _=>Err("Invalid expression tree.".into()),
    }
}
// Ok(None) is identically zero; Err(()) means a derivative is unavailable.
fn derivative(node:&Value,symbols:&HashMap<String,Scalar>)->Result<Option<String>,()> {
    fn depends_on_time(node:&Value)->bool {
        match node.get("kind").and_then(Value::as_str) {
            Some("symbol")=>node["name"].as_str()==Some("t"),
            Some("unary")=>depends_on_time(&node["value"]),
            Some("binary")=>depends_on_time(&node["left"])||depends_on_time(&node["right"]),
            Some("call")=>node["args"].as_array().is_some_and(|args|args.iter().any(depends_on_time)),
            _=>false,
        }
    }
    if !depends_on_time(node){return Ok(None);}
    match node.get("kind").and_then(Value::as_str) {
        Some("number")=>Ok(None),
        Some("symbol")=>Ok((node["name"].as_str()==Some("t")).then(||SCALE.to_string())),
        Some("unary")=>{let d=derivative(&node["value"],symbols)?;let op=node["op"].as_str().ok_or(())?;
            match op {"+"=>Ok(d),"-"=>Ok(d.map(|v|format!("(-{v})"))),_=>Err(())}},
        Some("binary")=>{
            let a=&node["left"];let b=&node["right"];let da=derivative(a,symbols)?;let db=derivative(b,symbols)?;
            if da.is_none()&&db.is_none(){return Ok(None);}
            let av=original(a).map_err(|_|())?;let bv=original(b).map_err(|_|())?;
            match (node["op"].as_str().ok_or(())?,da,db) {
                ("+",Some(x),Some(y))=>Ok(Some(format!("({x}+{y})"))),
                ("+",x,None)|( "-",x,None)=>Ok(x),
                ("+",None,y)=>Ok(y),("-",None,Some(y))=>Ok(Some(format!("(-{y})"))),
                ("-",Some(x),Some(y))=>Ok(Some(format!("({x}-{y})"))),
                ("*",Some(x),Some(y))=>Ok(Some(format!("(({x})*({bv})+({av})*({y}))"))),
                ("*",Some(x),None)=>Ok(Some(format!("(({x})*({bv}))"))),
                ("*",None,Some(y))=>Ok(Some(format!("(({av})*({y}))"))),
                ("/",Some(x),Some(y))=>Ok(Some(format!("((({x})*({bv})-({av})*({y}))/(({bv})^2))"))),
                ("/",Some(x),None)=>Ok(Some(format!("(({x})/({bv}))"))),
                ("/",None,Some(y))=>Ok(Some(format!("(-({av})*({y})/(({bv})^2))"))),
                ("^",Some(x),None)=>{
                    if b.get("kind").and_then(Value::as_str)==Some("number") {
                        if b.get("value").and_then(Value::as_f64)==Some(0.) {return Ok(None);}
                        if b.get("value").and_then(Value::as_f64)==Some(1.) {return Ok(Some(x));}
                    }
                    Ok(Some(format!("(({bv})*(({av})^(({bv})-1))*({x}))")))
                },
                ("^",None,Some(y))=>Ok(Some(format!("((({av})^({bv}))*ln({av})*({y}))"))),
                ("^",Some(x),Some(y))=>Ok(Some(format!("((({av})^({bv}))*((({y})*ln({av}))+(({bv})*({x})/({av}))))"))),
                _=>Err(()),
            }
        },
        Some("call")=>{
            let args=node["args"].as_array().ok_or(())?;
            if node["name"].as_str()==Some("pow")&&args.len()==2 {
                return derivative(&json!({"kind":"binary","op":"^","left":args[0],"right":args[1]}),symbols);
            }
            let function=node["name"].as_str().ok_or(())?;
            if matches!(function,"min"|"max")&&!args.is_empty() {
                let mut selected=0;let mut best=None;let mut tied=false;
                for (index,arg) in args.iter().enumerate(){
                    let value=model_expression::evaluate_expression(&original(arg).map_err(|_|())?,symbols,"boundary derivative").map_err(|_|())?.value;
                    match best {None=>{best=Some(value);selected=index;},Some(current) if value==current=>tied=true,
                        Some(current) if if function=="min" {value<current}else{value>current}=>{best=Some(value);selected=index;tied=false;},_=>{}}
                }
                if tied{return Err(());}
                return derivative(&args[selected],symbols);
            }
            if args.len()!=1{return Err(());}
            let Some(d)=derivative(&args[0],symbols)? else{return Ok(None);};
            let x=original(&args[0]).map_err(|_|())?;
            if matches!(function,"abs"|"sign"|"floor"|"ceil") {
                let current=model_expression::evaluate_expression(&x,symbols,"boundary derivative").map_err(|_|())?.value;
                return match function {
                    "abs" if current!=0.=>Ok(Some(format!("({}*({d}))",if current>0. {"1"}else{"-1"}))),
                    "sign" if current!=0.=>Ok(None),
                    "floor"|"ceil" if current.fract()!=0.=>Ok(None),
                    _=>Err(()),
                };
            }
            let factor=match function {
                "sin"=>format!("cos({x})"),"cos"=>format!("(-sin({x}))"),
                "tan"=>format!("(1/(cos({x})^2))"),"exp"=>format!("exp({x})"),
                "ln"|"log"=>format!("(1/({x}))"),"log10"=>format!("(1/(({x})*ln(10)))"),
                "sqrt"=>format!("(1/(2*sqrt({x})))"),"sinh"=>format!("cosh({x})"),
                "cosh"=>format!("sinh({x})"),"tanh"=>format!("(1/(cosh({x})^2))"),
                "asin"=>format!("(1/sqrt(1-({x})^2))"),"acos"=>format!("(-1/sqrt(1-({x})^2))"),
                "atan"=>format!("(1/(1+({x})^2))"),_=>return Err(()),
            };
            Ok(Some(format!("(({factor})*({d}))")))
        },
        _=>Err(()),
    }
}
pub(crate) fn evaluate(quantity:&Value,symbols:&HashMap<String,Scalar>,path:&str)->Result<Option<f64>,String>{
    let source=quantity.get("expression").and_then(Value::as_str).ok_or("Boundary expression is missing.")?;
    let (tree,names,_)=model_expression::inspect(source)?;
    if !names.contains("t"){return Ok(Some(0.));}
    let expression=match derivative(&tree,symbols){Ok(Some(value))=>value,Ok(None)=>return Ok(Some(0.)),Err(())=>return Ok(None)};
    if symbols.contains_key(SCALE){return Err("Boundary derivative scale conflicts with a parameter.".into());}
    let mut environment=symbols.clone();environment.insert(SCALE.into(),Scalar{value:1.,dimension:model_expression::unit("s")?.1});
    let derived=json!({"expression":expression,"unit":quantity.get("unit").ok_or("Boundary unit is missing.")?});
    // A symbolic derivative may have a removable singularity (for example
    // t^n at t=0 when n is a zero-valued parameter). Do not invent a rate or
    // reject a valid trace merely because this representation is singular.
    Ok(model_expression::evaluate_quantity(&derived,&environment,path).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn differentiates_smooth_dimensioned_expressions_without_sampling_a_step(){
        let symbols=HashMap::from([("t".into(),Scalar{value:0.,dimension:model_expression::unit("s").unwrap().1}),
            ("tau".into(),Scalar{value:2.,dimension:model_expression::unit("s").unwrap().1})]);
        for (expression,expected) in [("t/tau",0.5),("sin(t/tau)",0.5),("(t/tau)^2",0.),("(t/tau)^1",0.5),("(t/tau)^0",0.),("exp(t/tau)",0.5),("2^(t/tau)",2f64.ln()/2.),
            ("abs(1+t/tau)",0.5),("min(t/tau,1,2)",0.5),("max(t/tau,1)",0.),("floor(t/tau+0.25)",0.)] {
            let result=evaluate(&json!({"expression":expression,"unit":"m"}),&symbols,"rate").unwrap().unwrap();
            assert!((result-expected).abs()<1e-12,"{expression}: {result}");
        }
        assert_eq!(evaluate(&json!({"expression":"floor(t/tau)","unit":"m"}),&symbols,"rate").unwrap(),None);
        assert_eq!(evaluate(&json!({"expression":"abs(t/tau)","unit":"m"}),&symbols,"rate").unwrap(),None);
    }
}
