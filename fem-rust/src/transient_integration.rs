//! Prepare selected entities and evaluate configured expressions at one time.
//! Field and material samplers are supplied by the selected generic adapter.
use std::collections::HashMap;
use fullmeta_core::expression::AstNode;
use fullmeta_core::pointwise::ComplexValue;
use serde_json::{Map,Value};
use crate::integration::{IntegrationOperator,Simplex};
use crate::model_expression::Scalar;
use crate::transient_execution::MeshData;

pub(crate) struct TransientIntegration {
    variables:Vec<(String,AstNode)>, pointwise:Vec<(String,AstNode)>,
    operators:Vec<IntegrationOperator>, domains:Vec<Simplex>, boundaries:Vec<Simplex>,
}

impl TransientIntegration {
    pub(crate) fn prepare(model:&Value,map:&Value,mesh:&MeshData) -> Result<Option<Self>,String> {
        let (variables,pointwise)=crate::integration::model_expression_definitions(model)?;
        if variables.is_empty(){return Ok(None);}
        let component=model.pointer("/model").ok_or("Transient model component is missing.")?;
        let operators=crate::integration::parse_operators(model,component,map)?;
        let rows=component.get("integrationOperators").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
        let selections=map.get("domainCellSelections").and_then(Value::as_object);
        let mut domains=Vec::new();let mut boundaries=Vec::new();
        for operator in &operators {
            let selection=rows.iter().find(|row|row.get("id").and_then(Value::as_str)==Some(operator.id.as_str()))
                .and_then(|row|row.get("selectionId")).and_then(Value::as_str).ok_or("Transient integration selection is missing.")?;
            match operator.kind.as_str() {
                "domainIntegration"|"pointEvaluation"=>{
                    let selected=selections.and_then(|s|s.get(selection)).and_then(Value::as_array);
                    for (index,cell) in mesh.cells.iter().enumerate() {
                        let included=operator.kind=="pointEvaluation" || selected.map_or(cell.tag==operator.physical_tag,|ids|
                            ids.iter().any(|id|id.as_u64()==Some(index as u64)));
                        if included {domains.push(Simplex{nodes:cell.nodes.clone(),physical_tag:cell.tag,material_tag:Some(cell.tag),
                            operator_id:Some(operator.id.clone()),element_index:Some(index),adjacent_element_index:None});}
                    }
                    if let Some(ids)=selected {for id in ids {let n=id.as_u64().and_then(|v|usize::try_from(v).ok()).ok_or("Transient integration cell ID is invalid.")?;
                        if n>=mesh.cells.len(){return Err("Transient integration selection references a missing cell.".into());}}}
                }
                "boundaryIntegration"|"edgeIntegration"=>{
                    let mut entities=mesh.facets.iter().map(|(nodes,tag)|(nodes.clone(),*tag)).collect::<Vec<_>>();
                    entities.extend(mesh.integration_edges.iter().map(|(nodes,tag)|(nodes.to_vec(),*tag)));
                    let expected_nodes=if operator.kind=="edgeIntegration"||mesh.dimension==2 {2}else{3};
                    for (nodes,tag) in entities.into_iter().filter(|(nodes,tag)|*tag==operator.physical_tag&&nodes.len()==expected_nodes) {
                        let mut owners=mesh.cells.iter().enumerate().filter(|(_,cell)|nodes.iter().all(|node|cell.nodes.contains(node))).collect::<Vec<_>>();
                        if let Some(host)=operator.host_material_tag {owners.retain(|(_,cell)|cell.tag==host);}
                        let (index,cell)=owners.first().copied().ok_or("Transient integration entity has no compatible adjacent cell.")?;
                        if owners.len()>1&&owners.iter().any(|(_,other)|other.tag!=cell.tag) {
                            return Err("Transient integration entity has ambiguous material sides; select a host domain.".into());
                        }
                        boundaries.push(Simplex{nodes,physical_tag:tag,material_tag:Some(cell.tag),operator_id:Some(operator.id.clone()),
                            element_index:Some(index),adjacent_element_index:owners.get(1).map(|(index,_)|*index)});
                    }
                }
                _=>return Err(format!("Unsupported transient integration kind {:?}.",operator.kind)),
            }
        }
        Ok(Some(Self{variables,pointwise,operators,domains,boundaries}))
    }

    pub(crate) fn evaluate(&self,mesh:&MeshData,parameters:&HashMap<String,Scalar>,time:f64,
        field:&(dyn Fn(&Simplex,&[f64],[f64;3])->Result<HashMap<String,ComplexValue>,String>+'_),
        material:&(dyn Fn(&Simplex,[f64;3])->Result<HashMap<String,ComplexValue>,String>+'_),
    )->Result<Map<String,Value>,String> {
        if !time.is_finite()||parameters.contains_key("t"){return Err("Transient integration requires a finite time and reserves t.".into());}
        let mut environment=parameters.iter().map(|(name,value)|(name.clone(),ComplexValue{re:value.value,im:0.})).collect::<HashMap<_,_>>();
        environment.insert("t".into(),ComplexValue{re:time,im:0.});
        crate::integration::evaluate_variables_with_sampler(&self.variables,&self.pointwise,&self.operators,
            mesh.dimension,&mesh.nodes,&self.domains,&self.boundaries,environment,field,material,[0,1,2],[0.;3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn integrates_time_dependent_fields_and_point_probes_on_declared_domains() {
        let mesh=MeshData::parse(2,"FULLMETA_MESH/2\nV,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nQ,0,0,1,2,7\n").unwrap();
        let model=json!({"model":{"variables":[{"symbol":"total","expression":"dom(P*Cvol)+probe(P)+t"}],
            "pointwiseVariables":[{"symbol":"P","expression":"T+t"}],
            "selections":[{"id":"domain","entity":"domain"},{"id":"point","entity":"point"}],
            "integrationOperators":[{"id":"dom","selectionId":"domain"},{"id":"probe","selectionId":"point"}]}});
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":99}],"boundarySelections":[],
            "pointSelections":[{"id":"point","coordinateM":[0.25,0.25]}],"domainCellSelections":{"domain":[0]}});
        let integration=TransientIntegration::prepare(&model,&map,&mesh).unwrap().unwrap();
        for (time,expected) in [(0.,1.75),(3.,16.75)] {
            let field=|_:&Simplex,_:&[f64],point:[f64;3]|Ok(HashMap::from([("T".into(),ComplexValue{re:point[0]+2.*point[1]+time,im:0.})]));
            let material=|_:&Simplex,_:[f64;3]|Ok(HashMap::from([("Cvol".into(),ComplexValue{re:2.,im:0.})]));
            let result=integration.evaluate(&mesh,&HashMap::new(),time,&field,&material).unwrap();
            assert!((result["total"]["re"].as_f64().unwrap()-expected).abs()<1e-10);
        }
        let mut invalid=map.clone();invalid["domainCellSelections"]["domain"]=json!([4]);
        assert!(TransientIntegration::prepare(&model,&invalid,&mesh).err().unwrap().contains("missing cell"));
    }
}
