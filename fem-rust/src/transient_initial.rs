//! Scenario-neutral initial field expression sampling.
//!
//! Element adapters may interpolate nodal values or project these samples
//! into other declared spaces; this sampler does not choose a discretization.
use crate::boundary_input::{Number, Quantity, Sampling};
use crate::model_expression::{self, Scalar};
use crate::solver_common::C;
use serde_json::Value;
use std::collections::HashMap;

pub(crate) struct InitialField {
    components: Vec<Quantity>,
}

impl InitialField {
    /// Each field's component count and unit are supplied by its scenario
    /// contract. The model supplies the corresponding quantity expressions.
    pub(crate) fn compile(
        values: &Value,
        unit: &str,
        components: usize,
        path: &str,
    ) -> Result<Self, String> {
        let values = values
            .as_array()
            .ok_or_else(|| format!("{path} must be an array of initial field components."))?;
        if components == 0 || values.len() != components {
            return Err(format!("{path} requires {components} components."));
        }
        let components = values
            .iter()
            .enumerate()
            .map(|(i, value)| {
                Quantity::compile(
                    value,
                    unit,
                    Number::Real,
                    Sampling::Point,
                    &format!("{path}[{i}]"),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { components })
    }

    pub(crate) fn sample(
        &self,
        parameters: &HashMap<String, Scalar>,
        point: [f64; 3],
        time: f64,
        path: &str,
    ) -> Result<Vec<C>, String> {
        if !time.is_finite() || point.iter().any(|v| !v.is_finite()) {
            return Err(format!(
                "{path} has a non-finite coordinate or initial time."
            ));
        }
        if parameters.contains_key("t") {
            return Err(format!(
                "{path}: parameter t conflicts with transient time."
            ));
        }
        let mut symbols = parameters.clone();
        symbols.insert(
            "t".into(),
            Scalar {
                value: time,
                dimension: model_expression::unit("s")?.1,
            },
        );
        for (name, value) in ["x", "y", "z"].into_iter().zip(point) {
            symbols.insert(
                name.into(),
                Scalar {
                    value,
                    dimension: model_expression::unit("m")?.1,
                },
            );
        }
        self.components
            .iter()
            .enumerate()
            .map(|(i, value)| value.evaluate(&symbols, &format!("{path}[{i}]")))
            .collect()
    }
}

/// Interpret a model's field-keyed initial values using scenario field names,
/// component counts and physical units. Missing fields are initialized by the
/// element adapter; unknown names and unitless declarations are rejected.
pub(crate) fn compile_fields(
    plan: &crate::transient_plan::TransientPlan,
    values: &Value,
) -> Result<HashMap<String, InitialField>, String> {
    let values = values
        .as_object()
        .ok_or("Initial fields must be an object keyed by declared field name.")?;
    let mut fields = HashMap::with_capacity(values.len());
    for (id, value) in values {
        let definition = plan
            .fields
            .iter()
            .find(|field| field.id == *id)
            .ok_or_else(|| format!("Initial state names undeclared field {id:?}."))?;
        let unit = definition
            .unit
            .as_deref()
            .ok_or_else(|| format!("Initial field {id:?} has no declared physical unit."))?;
        fields.insert(
            id.clone(),
            InitialField::compile(
                value,
                unit,
                definition.components,
                &format!("initial field {id}"),
            )?,
        );
    }
    Ok(fields)
}

/// Validate the optional initial-field shape against the same scenario form
/// consumed by the transient solver, before geometry or mesh preparation.
pub(crate) fn validate_study_contract(
    package: &Value,
    formulation: &str,
    study: &Value,
    parameters: &HashMap<String, Scalar>,
) -> Result<(), String> {
    let Some(initial) = study.get("initialFields") else {
        return Ok(());
    };
    let declaration = package
        .get("transientForms")
        .and_then(|forms| forms.get(formulation))
        .ok_or("Scenario has no transient form for the selected formulation.")?;
    let plan = crate::transient_plan::TransientPlan::from_json(declaration)?;
    compile_fields(&plan, initial)?;
    let mut allowed = std::collections::HashSet::from(["x", "y", "z", "t", "pi", "e"]);
    for name in parameters.keys() { allowed.insert(name); }
    let policy=crate::model_schema::root()?.pointer("/x-fullmeta-expressionRuntime/real/literal")
        .ok_or("The real expression policy is missing.")?;
    let mut dimensions=serde_json::Map::new();
    for (name,value) in parameters { dimensions.insert(name.clone(),serde_json::json!({"value":value.value,"dimension":value.dimension})); }
    let mut runtime=Vec::new();
    for (name,unit) in [("x","m"),("y","m"),("z","m"),("t","s")] {
        dimensions.insert(name.into(),serde_json::json!({"value":1.,"dimension":model_expression::unit(unit)?.1}));
        runtime.push(serde_json::json!({"symbol":name,"requiresRuntimeSample":true}));
    }
    for (field, components) in initial.as_object().unwrap() {
        for (index, component) in components.as_array().unwrap().iter().enumerate() {
            let expression=component.get("expression").and_then(Value::as_str)
                .ok_or("Initial-field expression is missing.")?;
            let (_, symbols, _)=model_expression::inspect(expression)?;
            for symbol in symbols {
                if !allowed.contains(symbol.as_str()) {
                    return Err(format!("initial field {field}[{index}] uses unknown symbol {symbol:?}."));
                }
            }
            let definition=plan.fields.iter().find(|row|row.id==*field).unwrap();
            let report=fullmeta_model::execute_json(serde_json::json!({"operation":"validate_semantic_quantities","input":{
                "kind":"quantity","value":component,"expectedUnit":definition.unit,"path":format!("initial field {field}[{index}]"),
                "symbols":dimensions,"runtimeSymbols":runtime,"evaluationPolicy":policy,"allowedFunctions":policy["allowedFunctions"],
                "dimensionTolerance":policy["dimensionTolerance"]}}))?;
            let errors=report.get("errors").and_then(Value::as_array).ok_or("Initial field validation returned no errors array.")?;
            if !errors.is_empty(){return Err(errors.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n"));}
        }
    }
    Ok(())
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_scenario_validates_initial_fields_from_its_own_form() {
        for text in [include_str!("scenarios/acoustic2d.json"),include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),include_str!("scenarios/em3d.json"),
            include_str!("scenarios/thermal2d.json"),include_str!("scenarios/thermal3d.json")] {
            let package:Value=serde_json::from_str(text).unwrap();
            for (form,declaration) in package["transientForms"].as_object().unwrap() {
                let plan=crate::transient_plan::TransientPlan::from_json(declaration).unwrap();
                assert!(validate_study_contract(&package,form,&json!({}),&HashMap::new()).is_ok());
                for field in &plan.fields {
                    let unit=field.unit.as_deref().unwrap();
                    let components=vec![json!({"expression":"0","unit":unit});field.components];
                    let mut study=json!({"initialFields":{field.id.clone():components}});
                    validate_study_contract(&package,form,&study,&HashMap::new()).unwrap();
                    study["initialFields"][&field.id][0]["expression"]=json!("unknownSymbol");
                    assert!(validate_study_contract(&package,form,&study,&HashMap::new()).unwrap_err().contains("unknownSymbol"));
                    study["initialFields"][&field.id][0]["expression"]=json!("x/L+t/tau");
                    let parameters=HashMap::from([("L".into(),Scalar{value:1.,dimension:model_expression::unit("m").unwrap().1}),
                        ("tau".into(),Scalar{value:1.,dimension:model_expression::unit("s").unwrap().1})]);
                    validate_study_contract(&package,form,&study,&parameters).unwrap();
                    study["initialFields"][&field.id][0]["expression"]=json!("1/(x/L-1)");
                    validate_study_contract(&package,form,&study,&parameters).unwrap();
                    study["initialFields"][&field.id][0]["expression"]=json!("x+t");
                    assert!(validate_study_contract(&package,form,&study,&parameters).is_err());
                    study["initialFields"][&field.id][0]["expression"]=json!("x/L+t/tau");
                    study["initialFields"][&field.id].as_array_mut().unwrap().push(json!({"expression":"0","unit":unit}));
                    assert!(validate_study_contract(&package,form,&study,&parameters).is_err());
                    assert!(validate_study_contract(&package,form,&json!({"initialFields":{"unknown":[{"expression":"0","unit":"1"}]}}),&HashMap::new()).is_err());
                }
            }
        }
    }
}

/// Sample a declared field, supplying the same zero default for element
/// adapters that use projection rather than nodal interpolation.
pub(crate) fn sample_field(
    plan: &crate::transient_plan::TransientPlan,
    fields: &HashMap<String, InitialField>,
    id: &str,
    parameters: &HashMap<String, Scalar>,
    point: [f64; 3],
    start: f64,
) -> Result<Vec<C>, String> {
    let field = plan
        .fields
        .iter()
        .find(|field| field.id == id)
        .ok_or_else(|| format!("Initial state names undeclared field {id:?}."))?;
    match fields.get(id) {
        Some(values) => values.sample(parameters, point, start, &format!("initial field {id}")),
        None => Ok(vec![C::default(); field.components]),
    }
}

/// Interpolate initial expressions at the nodes of fields using a shared
/// nodal P1 space. An omitted field starts at zero. Other element spaces use
/// the same sampler with their appropriate moment or projection operation.
pub(crate) fn nodal_state(
    plan: &crate::transient_plan::TransientPlan,
    layout: &crate::transient_plan::StateLayout,
    fields: &HashMap<String, InitialField>,
    nodes: &[[f64; 3]],
    parameters: &HashMap<String, Scalar>,
    start: f64,
) -> Result<Vec<C>, String> {
    if fields
        .keys()
        .any(|id| !plan.fields.iter().any(|field| &field.id == id))
    {
        return Err("Initial state names an undeclared field.".into());
    }
    let mut state = vec![C::default(); layout.dimension];
    for (index, definition) in plan.fields.iter().enumerate() {
        let count = nodes
            .len()
            .checked_mul(definition.components)
            .ok_or("Initial nodal state size overflow.")?;
        if layout.field_count(index) != Some(count) {
            return Err(format!(
                "Initial nodal layout differs from field {:?}.",
                definition.id
            ));
        }
        let Some(initial) = fields.get(&definition.id) else {
            continue;
        };
        if initial.components.len() != definition.components {
            return Err(format!(
                "Initial field {:?} has the wrong component count.",
                definition.id
            ));
        }
        for (node, &point) in nodes.iter().enumerate() {
            let values = initial.sample(
                parameters,
                point,
                start,
                &format!("initial field {}", definition.id),
            )?;
            let local = (0..definition.components)
                .map(|c| node * definition.components + c)
                .collect::<Vec<_>>();
            for (dof, value) in layout.global_dofs(index, &local)?.into_iter().zip(values) {
                state[dof] = value;
            }
        }
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn initial_fields_evaluate_spatial_expressions_at_start_time() {
        let values = json!([{"expression":"amplitude*x/L*(1+t/tau)","unit":"Pa"}]);
        let field = InitialField::compile(&values, "Pa", 1, "initial").unwrap();
        let symbols = HashMap::from([
            (
                "amplitude".into(),
                Scalar {
                    value: 4.,
                    dimension: model_expression::unit("Pa").unwrap().1,
                },
            ),
            (
                "L".into(),
                Scalar {
                    value: 2.,
                    dimension: model_expression::unit("m").unwrap().1,
                },
            ),
            (
                "tau".into(),
                Scalar {
                    value: 2.,
                    dimension: model_expression::unit("s").unwrap().1,
                },
            ),
        ]);
        assert_eq!(
            field.sample(&symbols, [1., 0., 0.], 2., "initial").unwrap()[0].re,
            4.
        );
        assert!(InitialField::compile(&values, "m", 1, "initial").is_err());
        assert!(InitialField::compile(&values, "Pa", 2, "initial").is_err());
        assert!(
            field
                .sample(&symbols, [f64::NAN, 0., 0.], 0., "initial")
                .is_err()
        );
    }

    #[test]
    fn nodal_state_follows_field_layout_and_defaults_to_zero() {
        let value = json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"p","components":1},{"id":"v","components":2}],
            "terms":[{"testField":"p","trialField":"p","test":"value","trial":"value","coefficient":"a","timeDerivative":"state"},
                {"testField":"v","trialField":"v","test":"value","trial":"value","coefficient":"a","timeDerivative":"state"}]});
        let plan = crate::transient_plan::TransientPlan::from_json(&value).unwrap();
        let layout = crate::transient_plan::StateLayout::new(&plan, &[2, 4]).unwrap();
        let p = InitialField::compile(&json!([{"expression":"x/L","unit":"Pa"}]), "Pa", 1, "p")
            .unwrap();
        let params = HashMap::from([(
            "L".into(),
            Scalar {
                value: 1.,
                dimension: model_expression::unit("m").unwrap().1,
            },
        )]);
        let state = nodal_state(
            &plan,
            &layout,
            &HashMap::from([("p".into(), p)]),
            &[[0., 0., 0.], [2., 0., 0.]],
            &params,
            0.,
        )
        .unwrap();
        assert_eq!(
            state.iter().map(|v| v.re).collect::<Vec<_>>(),
            vec![0., 2., 0., 0., 0., 0.]
        );
    }

    #[test]
    fn model_initial_values_follow_scenario_field_units() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/acoustic2d.json")).unwrap();
        let plan = crate::transient_plan::TransientPlan::from_json(
            &package["transientForms"]["pressure"],
        )
        .unwrap();
        assert!(
            compile_fields(&plan, &json!({"pressure":[{"expression":"1","unit":"Pa"}]})).is_ok()
        );
        assert!(
            compile_fields(&plan, &json!({"pressure":[{"expression":"1","unit":"m"}]})).is_err()
        );
        assert!(
            compile_fields(&plan, &json!({"missing":[{"expression":"1","unit":"Pa"}]})).is_err()
        );
    }
}
