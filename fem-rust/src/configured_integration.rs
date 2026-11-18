//! Bind prepared configuration expressions and operators to mesh entities.
//! The resulting inputs are independent of field family and can be evaluated
//! by the shared integration engine with a caller-provided field sampler.
use std::collections::{HashMap, HashSet};
use fullmeta_core::expression::AstNode;
use serde_json::Value;
use crate::{integration::{IntegrationOperator,Simplex}, prepared_edge::PreparedEdgeMesh};

pub(crate) struct ConfiguredIntegration {
    pub(crate) variables: Vec<(String,AstNode)>,
    pub(crate) pointwise: Vec<(String,AstNode)>,
    pub(crate) operators: Vec<IntegrationOperator>,
    pub(crate) domains: Vec<Simplex>,
    pub(crate) boundaries: Vec<Simplex>,
}

fn id(value:&Value,key:&str,path:&str) -> Result<String,String> {
    value.get(key).and_then(Value::as_str).filter(|s|!s.is_empty()).map(str::to_owned)
        .ok_or_else(||format!("{path}.{key} must be a nonempty string."))
}
fn symbol(name:&str) -> bool {
    !name.is_empty() && name.chars().enumerate().all(|(i,c)| if i==0 {c=='_'||c.is_ascii_alphabetic()} else {c=='_'||c.is_ascii_alphanumeric()})
}
fn validate_ast(node:&Value, arities:&Value, operators:&HashSet<String>) -> Result<(),String> {
    match node.get("kind").and_then(Value::as_str) {
        Some("number" | "symbol") => Ok(()),
        Some("unary") => validate_ast(node.get("value").ok_or("Unary expression has no value.")?,arities,operators),
        Some("binary") => {
            validate_ast(node.get("left").ok_or("Binary expression has no left operand.")?,arities,operators)?;
            validate_ast(node.get("right").ok_or("Binary expression has no right operand.")?,arities,operators)
        },
        Some("call") => {
            let name=id(node,"name","expression call")?;
            let args=node.get("args").and_then(Value::as_array).ok_or("Expression call args must be an array.")?;
            if operators.contains(&name) {
                if args.len()!=1 { return Err(format!("Integration operator {name:?} requires one argument.")); }
            } else {
                let arity=arities.get(&name).ok_or_else(||format!("Expression function {name:?} is unsupported."))?;
                let minimum=arity.get("minArgs").and_then(Value::as_u64).ok_or("Function minimum arity is invalid.")? as usize;
                let maximum=arity.get("maxArgs").and_then(Value::as_u64).map(|v|v as usize);
                if args.len()<minimum || maximum.is_some_and(|n|args.len()>n) { return Err(format!("Expression function {name:?} has invalid arity.")); }
            }
            for arg in args {validate_ast(arg,arities,operators)?;} Ok(())
        },
        _ => Err("Expression AST contains an unsupported or missing kind.".into()),
    }
}
fn definitions(config:&Value,key:&str,arities:&Value,operators:&HashSet<String>) -> Result<Vec<(String,AstNode)>,String> {
    let rows=config.get(key).and_then(Value::as_array).ok_or_else(||format!("modelConfiguration.{key} must be an array."))?;
    let mut result=Vec::new(); let mut seen=HashSet::new();
    for (index,row) in rows.iter().enumerate() {
        let path=format!("modelConfiguration.{key}[{index}]"); let name=id(row,"symbol",&path)?;
        if !symbol(&name) || !seen.insert(name.clone()) { return Err(format!("{path} has an invalid or duplicate symbol.")); }
        let ast=row.get("ast").ok_or_else(||format!("{path}.ast is required."))?;
        validate_ast(ast,arities,operators).map_err(|e|format!("{path}: {e}"))?;
        let parsed=serde_json::from_value(ast.clone()).map_err(|e|format!("{path}.ast is invalid: {e}"))?;
        result.push((name,parsed));
    }
    Ok(result)
}
fn tags(map:&Value,section:&str,id_key:&str) -> Result<HashMap<String,usize>,String> {
    if map.get(section).is_none() { return Ok(HashMap::new()); }
    crate::model_bindings::selection_tags_with_keys(map,section,false,Some(id_key))
}

impl ConfiguredIntegration {
    pub(crate) fn from_parts(config:&Value,map:&Value,mesh:&PreparedEdgeMesh,capabilities:&Value,id_keys:&Value) -> Result<Self,String> {
        let arities=capabilities.pointer("/expressions/functionArities").and_then(Value::as_object).ok_or("Engine function arities are missing.")?;
        let arities_value=Value::Object(arities.clone());
        if capabilities.pointer("/expressions/integrationOperatorArgumentCount").and_then(Value::as_u64)!=Some(1) {return Err("The integration evaluator requires one operator argument.".into());}
        let key=|section:&str|id_keys.get(section).and_then(Value::as_str).filter(|value|!value.is_empty())
            .ok_or_else(||format!("Integration selection ID key for {section:?} is required."));
        let groups=tags(map,"physicalGroups",key("physicalGroups")?)?;
        let surfaces=tags(map,"boundarySelections",key("boundarySelections")?)?;
        let edges=tags(map,"edgeSelections",key("edgeSelections")?)?;
        let point_key=key("pointSelections")?;
        let mut host=HashMap::new();
        for section in ["boundarySelections","edgeSelections"] {
            for row in map.get(section).and_then(Value::as_array).into_iter().flatten() {
                if row.get("type").and_then(Value::as_str)!=Some("clip") {continue;}
                let name=row.get(key(section)?).and_then(Value::as_str).ok_or("Clipped selection has no ID.")?;
                let host_id=id(row,"hostSelectionId","clipped selection")?;
                if let Some(tag)=groups.get(&host_id) {host.insert(name.to_owned(),*tag);}
            }
        }
        let rows=config.get("integrationOperators").and_then(Value::as_array).ok_or("modelConfiguration.integrationOperators must be an array.")?;
        let mut operators=Vec::new(); let mut ids=HashSet::new();
        for row in rows {
            let name=id(row,"id","integration operator")?; let kind=id(row,"integrationKind","integration operator")?;
            let selection=id(row,"selectionId","integration operator")?;
            if !ids.insert(name.clone()) || arities.contains_key(&name) {return Err(format!("Integration operator {name:?} is duplicate or conflicts with a built-in function."));}
            if capabilities.get("integration").and_then(|v|v.get(&kind)).and_then(|v|v.get("evaluation")).and_then(Value::as_str)!=Some("supported") {return Err(format!("Integration operator kind {kind:?} is unsupported."));}
            let lookup=|map:&HashMap<String,usize>|map.get(&selection).copied().ok_or_else(||format!("Integration selection {selection:?} is absent from the mesh map."));
            let (tag,point_m)=match kind.as_str() {
                "domainIntegration" => (lookup(&groups)?,None), "boundaryIntegration" => (lookup(&surfaces)?,None),
                "edgeIntegration" => (lookup(&edges)?,None),
                "pointEvaluation" => {
                    let row=map.get("pointSelections").and_then(Value::as_array).into_iter().flatten()
                        .find(|r|r.get(point_key).and_then(Value::as_str)==Some(selection.as_str())).ok_or("Point selection is missing from the mesh map.")?;
                    let coords=row.get("coordinateM").and_then(Value::as_array).filter(|a|a.len()==3).ok_or("Point selection requires three coordinates.")?;
                    let mut point=[0.;3]; for i in 0..3 {point[i]=coords[i].as_f64().filter(|v|v.is_finite()).ok_or("Point coordinates must be finite.")?;} (0,Some(point))
                }, _=>return Err(format!("Integration operator kind {kind:?} is unsupported.")),
            };
            let host_material_tag=if matches!(kind.as_str(),"boundaryIntegration"|"edgeIntegration") {host.get(&selection).copied()}else{None};
            operators.push(IntegrationOperator{id:name,kind,physical_tag:tag,host_material_tag,point_m});
        }
        let variables=definitions(config,"variables",&arities_value,&ids)?;
        let pointwise=definitions(config,"pointwiseVariables",&arities_value,&HashSet::new())?;
        // Compile eagerly so unsupported functions and expression dependencies
        // fail during preparation even if no output is eventually requested.
        crate::core_language::Context::compile(&variables,&pointwise,&operators.iter().map(|o|o.id.clone()).collect::<Vec<_>>())?;
        let mut domains=Vec::new(); let mut boundaries=Vec::new();
        for operator in &operators {
            match operator.kind.as_str() {
                "domainIntegration" | "pointEvaluation" => for (index,cell) in mesh.cells.iter().enumerate() {
                    if operator.kind=="domainIntegration" && mesh.cell_tags[index]!=operator.physical_tag {continue;}
                    domains.push(Simplex{nodes:cell.to_vec(),physical_tag:mesh.cell_tags[index],material_tag:Some(mesh.cell_tags[index]),operator_id:Some(operator.id.clone()),element_index:Some(index),adjacent_element_index:None});
                },
                "boundaryIntegration" => for selected in &mesh.selected_facets {
                    if selected.tag!=operator.physical_tag {continue;}
                    let face=&mesh.adjacency.facets[selected.facet_index]; let owner=face.owners[0].cell;
                    boundaries.push(Simplex{nodes:face.vertices.to_vec(),physical_tag:selected.tag,material_tag:Some(mesh.cell_tags[owner]),operator_id:Some(operator.id.clone()),element_index:Some(owner),adjacent_element_index:face.owners.get(1).map(|v|v.cell)});
                },
                "edgeIntegration" => for edge in &mesh.selected_edges {
                    if edge.tag!=operator.physical_tag {continue;}
                    boundaries.push(Simplex{nodes:edge.nodes.to_vec(),physical_tag:edge.tag,material_tag:Some(mesh.cell_tags[edge.owner]),operator_id:Some(operator.id.clone()),element_index:Some(edge.owner),adjacent_element_index:None});
                }, _=>unreachable!(),
            }
        }
        Ok(Self{variables,pointwise,operators,domains,boundaries})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use crate::simplex_topology::{TopologyPolicy,SelectedFacetPolicy,SelectedFacetOwnership,SelectedFacetDuplicates};
    fn mesh() -> PreparedEdgeMesh {
        let text="V,0,0,0,0\nV,1,1,0,0\nV,2,0,1,0\nV,3,0,0,1\nV,4,0,0,-1\nT,0,0,1,2,3,4\nT,1,0,2,1,4,5\nF,x,0,1,2,8\nF,x,0,1,3,9\nL,x,0,1,10";
        PreparedEdgeMesh::from_stream(text,&json!({"axisOrder":[0,1,2],"origin":"zero"}),1,"standard",
            TopologyPolicy{reject_duplicate_cells:true,reject_non_manifold_facets:true,require_all_nodes_used:false,require_nonempty_cells:true,determinant_tolerance:None},
            SelectedFacetPolicy{ownership:SelectedFacetOwnership::AnyExisting,duplicates:SelectedFacetDuplicates::Reject,require_positive_tag:true},true).unwrap()
    }
    fn inputs() -> (Value,Value) {
        let ast=serde_json::to_value(fullmeta_core::expression::parse_expression("vol(1)+face(1)+line(1)+probe(1)").unwrap()).unwrap();
        let config=json!({"variables":[{"symbol":"sum","ast":ast}],"pointwiseVariables":[],"integrationOperators":[
            {"id":"vol","integrationKind":"domainIntegration","selectionId":"domain"},
            {"id":"face","integrationKind":"boundaryIntegration","selectionId":"surface"},
            {"id":"line","integrationKind":"edgeIntegration","selectionId":"curve"},
            {"id":"probe","integrationKind":"pointEvaluation","selectionId":"point"}]});
        let map=json!({"physicalGroups":[{"selectionId":"domain","physicalTag":4}],
            "boundarySelections":[{"id":"surface","physicalTag":8,"type":"clip","hostSelectionId":"domain"}],
            "edgeSelections":[{"id":"curve","physicalTag":10,"type":"clip","hostSelectionId":"domain"}],
            "pointSelections":[{"id":"point","coordinateM":[0,0,0]}]});
        (config,map)
    }
    #[test]
    fn configured_operators_bind_all_entity_kinds_and_host_materials() {
        let (config,map)=inputs(); let mesh=mesh();
        let capabilities=crate::model_schema::engine_capabilities().unwrap();
        let keys=json!({"physicalGroups":"selectionId","boundarySelections":"id","edgeSelections":"id","pointSelections":"id"});
        let input=ConfiguredIntegration::from_parts(&config,&map,&mesh,&capabilities,&keys).unwrap();
        assert_eq!(input.operators.len(),4); assert_eq!(input.variables.len(),1); assert!(input.pointwise.is_empty());
        assert_eq!(input.domains.iter().filter(|v|v.operator_id.as_deref()==Some("vol")).count(),1);
        assert_eq!(input.domains.iter().filter(|v|v.operator_id.as_deref()==Some("probe")).count(),2);
        let face=input.boundaries.iter().find(|v|v.operator_id.as_deref()==Some("face")).unwrap();
        assert_eq!(face.element_index,Some(0)); assert_eq!(face.adjacent_element_index,Some(1));
        assert_eq!(input.operators[1].host_material_tag,Some(4));
        let edge=input.boundaries.iter().find(|v|v.operator_id.as_deref()==Some("line")).unwrap();
        assert_eq!(edge.nodes,vec![0,1]); assert_eq!(edge.element_index,Some(0));
        assert_eq!(input.operators[2].host_material_tag,Some(4));
        let empty_field=|_:&Simplex,_:&[f64],_:[f64;3]|Ok(HashMap::new());
        let empty_material=|_:&Simplex,_:[f64;3]|Ok(HashMap::new());
        let result=crate::integration::evaluate_variables_with_sampler(&input.variables,&input.pointwise,&input.operators,3,
            &mesh.model_nodes,&input.domains,&input.boundaries,HashMap::new(),&empty_field,&empty_material,[0,1,2],[0.;3]).unwrap();
        let expected=1./6.+0.5+1.+1.;
        assert!((result["sum"]["re"].as_f64().unwrap()-expected).abs()<1e-12);
    }
    #[test]
    fn rejects_duplicate_operators_bad_ast_and_invalid_points() {
        let (mut config,map)=inputs(); let mesh=mesh(); let cap=crate::model_schema::engine_capabilities().unwrap();
        config["integrationOperators"][1]["id"]=json!("vol");
        let keys=json!({"physicalGroups":"selectionId","boundarySelections":"id","edgeSelections":"id","pointSelections":"id"});
        assert!(ConfiguredIntegration::from_parts(&config,&map,&mesh,&cap,&keys).is_err());
        let (mut config,mut map)=inputs(); config["variables"][0]["ast"]=json!({"kind":"bogus"});
        assert!(ConfiguredIntegration::from_parts(&config,&map,&mesh,&cap,&keys).is_err());
        let (config,_)=inputs(); map["pointSelections"][0]["coordinateM"]=json!([1,2]);
        assert!(ConfiguredIntegration::from_parts(&config,&map,&mesh,&cap,&keys).is_err());
    }

    #[test]
    fn selection_id_keys_are_declared_even_when_both_fields_exist() {
        let (config,mut map)=inputs(); let mesh=mesh(); let cap=crate::model_schema::engine_capabilities().unwrap();
        map["physicalGroups"][0]["id"]=json!("wrong-domain");
        map["boundarySelections"][0]["selectionId"]=json!("wrong-surface");
        map["edgeSelections"][0]["selectionId"]=json!("wrong-curve");
        map["pointSelections"][0]["selectionId"]=json!("wrong-point");
        let keys=json!({"physicalGroups":"selectionId","boundarySelections":"id","edgeSelections":"id","pointSelections":"id"});
        let parsed=ConfiguredIntegration::from_parts(&config,&map,&mesh,&cap,&keys).unwrap();
        assert_eq!(parsed.operators.iter().map(|o|o.physical_tag).collect::<Vec<_>>(),vec![4,8,10,0]);
        let invalid=json!({"physicalGroups":"id","boundarySelections":"id","edgeSelections":"id","pointSelections":"id"});
        assert!(ConfiguredIntegration::from_parts(&config,&map,&mesh,&cap,&invalid).is_err());
        assert!(ConfiguredIntegration::from_parts(&config,&map,&mesh,&cap,&json!({})).is_err());
    }
}
