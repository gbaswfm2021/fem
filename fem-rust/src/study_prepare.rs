//! Converts a submitted FullMeta model and Study into the solver's run request.
//!
//! The common FEM dispatcher expands frequency cases and prepares solver data
//! using contracts declared by the scenario schemas. Material expressions remain
//! authoritative and are sampled by the solver on its actual mesh. The editor
//! supplies the model snapshot, selected Study, and filesystem paths.
//!
//! Entry precondition: the dispatcher has validated both the model and the
//! preparation envelope against their deployed schema rule programs. This
//! module extracts those validated declarations and performs numerical work;
//! model admissibility and request-format criteria are not repeated here.

use crate::model_expression::{self, Scalar};
use serde_json::{Map, Value, json};
use std::collections::{BTreeSet, HashMap, HashSet};

type Result<T> = std::result::Result<T, String>;

fn field<'a>(value: &'a Value, key: &str, path: &str) -> Result<&'a Value> {
    value
        .get(key)
        .ok_or_else(|| format!("{path}.{key} is required."))
}
fn text<'a>(value: &'a Value, path: &str) -> Result<&'a str> {
    value
        .as_str()
        .ok_or_else(|| format!("{path} must be a string."))
}
fn array<'a>(value: &'a Value, path: &str) -> Result<&'a [Value]> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{path} must be an array."))
}
fn str_field<'a>(value: &'a Value, key: &str, path: &str) -> Result<&'a str> {
    text(field(value, key, path)?, &format!("{path}.{key}"))
}
fn opt_string<'a>(value: &'a Value, key: &str, fallback: &'a str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or(fallback)
}
fn find_id<'a>(items: &'a [Value], id: &str) -> Option<&'a Value> {
    items
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
}

fn values_by_symbol(model: &Value, parameters: &[Value]) -> Result<HashMap<String, Scalar>> {
    model_expression::resolve_parameters(parameters, model)
}
fn quantity(value: &Value, symbols: &HashMap<String, Scalar>, path: &str) -> Result<f64> {
    model_expression::evaluate_quantity_with_symbols(value, symbols, path)
}
fn sweep_values(sweep: &Value, symbols: &HashMap<String, Scalar>, path: &str) -> Result<Vec<f64>> {
    match sweep.get("type").and_then(Value::as_str) {
        Some("linear") => {
            let count = field(sweep, "count", path)?
                .as_u64()
                .and_then(|count| usize::try_from(count).ok())
                .ok_or_else(|| {
                    format!("{path}.count cannot be represented as an allocation size.")
                })?;
            let intervals = count
                .checked_sub(1)
                .filter(|count| *count > 0)
                .ok_or_else(|| {
                    format!("{path} has no interpolation intervals after validation.")
                })?;
            let start = quantity(
                field(sweep, "start", path)?,
                symbols,
                &format!("{path}.start"),
            )?;
            let end = quantity(field(sweep, "end", path)?, symbols, &format!("{path}.end"))?;
            let mut points = Vec::new();
            points.try_reserve_exact(count).map_err(|error| {
                format!("Could not allocate the evaluated sweep points for {path}: {error}")
            })?;
            points.extend(
                (0..count).map(|index| start + (end - start) * index as f64 / intervals as f64),
            );
            Ok(points)
        }
        Some("values") => {
            let items = array(field(sweep, "values", path)?, &format!("{path}.values"))?;
            let mut points = Vec::new();
            points.try_reserve_exact(items.len()).map_err(|error| {
                format!("Could not allocate the evaluated sweep points for {path}: {error}")
            })?;
            for (index, item) in items.iter().enumerate() {
                points.push(quantity(item, symbols, &format!("{path}.values[{index}]"))?);
            }
            Ok(points)
        }
        _ => Err(format!(
            "Cannot extract the validated sweep representation at {path}."
        )),
    }
}

struct PreparedFrequencyCase {
    frequency: f64,
    symbols: HashMap<String, Scalar>,
    parameters: Vec<Value>,
    longitudinal_wavenumber: crate::C,
}

/// Expand Study parameter axes independently of a frequency expression.
fn parameter_cases(study:&Value,model:&Value) -> Result<Vec<(HashMap<String,Scalar>,Vec<Value>)>> {
    let model_root = field(model, "model", "model")?;
    let parameters = array(
        field(model_root, "parameters", "model")?,
        "model.parameters",
    )?;
    let sweep_parameters = match study.get("parameterSweep") {
        Some(sweep) => array(
            field(sweep, "parameters", "Parameter sweep")?,
            "Parameter sweep.parameters",
        )?,
        None => &[],
    };
    let baseline_symbols = values_by_symbol(model, parameters)?;
    let mut axes = Vec::with_capacity(sweep_parameters.len());
    for (index, sweep_parameter) in sweep_parameters.iter().enumerate() {
        let parameter_path = format!("Parameter sweep parameter {}", index + 1);
        let symbol = str_field(sweep_parameter, "parameter", &parameter_path)?;
        let parameter = parameters
            .iter()
            .find(|entry| entry.get("symbol").and_then(Value::as_str) == Some(symbol))
            .ok_or_else(|| format!("Cannot extract validated sweep parameter {symbol}."))?;
        let values = field(sweep_parameter, "values", &parameter_path)?;
        let points = sweep_values(
            values,
            &baseline_symbols,
            &format!("{parameter_path}.values"),
        )?;
        let (unit_scale, _) = model_expression::unit(str_field(
            parameter,
            "unit",
            &format!("parameter {symbol}"),
        )?)?;
        axes.push((symbol.to_string(), points, unit_scale));
    }
    axes.sort_by(|left, right| left.0.cmp(&right.0));
    let total_points = axes
        .iter()
        .try_fold(1usize, |total, (_, values, _)| {
            total.checked_mul(values.len())
        })
        .ok_or("The evaluated sweep cardinality cannot be represented as an allocation size.")?;
    let mut expanded_parameters = Vec::new();
    expanded_parameters
        .try_reserve_exact(total_points)
        .map_err(|error| format!("Could not allocate the evaluated sweep cases: {error}"))?;
    expand_parameter_axes(&axes, 0, parameters.to_vec(), &mut expanded_parameters)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(expanded_parameters.len())
        .map_err(|error| format!("Could not allocate the prepared frequency cases: {error}"))?;
    for case_parameters in &expanded_parameters {
        let resolved = values_by_symbol(model, case_parameters)?;
        output.push((resolved,case_parameters.clone()));
    }
    Ok(output)
}

fn frequency_cases(study:&Value,physics:&Value,model:&Value,profile_id:&str,formulation:&str) -> Result<Vec<PreparedFrequencyCase>> {
    let physics_id = str_field(physics, "id", "physics")?;
    let frequency_expression = field(physics, "frequency", "physics")?;
    let study_id = str_field(study,"id","Study")?;
    let package = crate::scenario_package::load_installed(profile_id)?;
    let cases=parameter_cases(study,model)?;
    let mut output=Vec::with_capacity(cases.len());
    for (index,(resolved,case_parameters)) in cases.into_iter().enumerate() {
        let frequency = quantity(
            frequency_expression,
            &resolved,
            &format!("{physics_id}.frequency[{index}]"),
        )?;
        let longitudinal_wavenumber=crate::longitudinal::prescribed_wavenumber(
            &package,model,study_id,formulation,&resolved,
        ).map_err(|error|format!("{study_id}.longitudinalWavenumber[{index}]: {error}"))?;
        output.push(PreparedFrequencyCase {
            frequency,
            symbols: resolved,
            parameters: case_parameters,
            longitudinal_wavenumber,
        });
    }
    Ok(output)
}

pub(crate) struct PreparedTransientCase {
    pub(crate) grid:crate::transient_request::TimeGrid,
    pub(crate) symbols:HashMap<String,Scalar>,
    pub(crate) parameters:Vec<Value>,
    pub(crate) longitudinal_wavenumber:crate::solver_common::C,
}

/// Time settings are resolved independently for every parameter sweep case.
pub(crate) fn transient_cases(study:&Value,model:&Value)->Result<Vec<PreparedTransientCase>> {
    let grid=field(study,"time","Transient Study")?;
    parameter_cases(study,model)?.into_iter().enumerate().map(|(index,(symbols,parameters))| {
        let grid=crate::transient_request::TimeGrid::from_json(grid,&symbols)
            .map_err(|error|format!("Transient sweep case {}: {error}",index+1))?;
        let longitudinal_wavenumber=crate::longitudinal::study_wavenumber(
            study,&symbols,&format!("Transient Study longitudinalWavenumber[{}]",index+1),
        )?;
        Ok(PreparedTransientCase {grid,symbols,parameters,longitudinal_wavenumber})
    }).collect()
}

/// Enforce a scenario-declared field-layout requirement when a longitudinal
/// wavenumber is explicitly selected, including the meaningful `kz = 0`
/// limit. A nonzero legacy value remains gated even if it predates the toggle.
fn validate_study_wavenumber_formulation(
    package:&Value,
    study_type:&str,
    formulation:&str,
    wavenumber:crate::solver_common::C,
    explicitly_enabled:bool,
)->Result<()> {
    if !explicitly_enabled&&wavenumber==crate::solver_common::C::default(){return Ok(());}
    let studies=package.get("studyLongitudinalWavenumber").and_then(|value|value.get("studies"))
        .ok_or_else(||format!("Scenario does not declare fixed longitudinal-wavenumber support for {study_type} studies."))?;
    let declaration=studies.get(study_type)
        .ok_or_else(||format!("Scenario does not declare fixed longitudinal-wavenumber support for {study_type} studies."))?;
    let formulations=declaration.get("formulations").and_then(Value::as_array)
        .ok_or_else(||format!("Scenario longitudinal-wavenumber contract for {study_type} must declare formulations."))?;
    if formulations.iter().any(|value|value.as_str()==Some(formulation)){return Ok(());}
    let required=declaration.get("requiredFormulation").and_then(Value::as_str)
        .or_else(||formulations.first().and_then(Value::as_str)).unwrap_or("a declared formulation");
    Err(format!("A prescribed longitudinal wavenumber in a {study_type} Study requires the {required} formulation; selected {formulation:?}."))
}

/// Stable, dimensioned per-case data for the generic transient coordinator.
/// Parameters remain separate from time so every case owns its own schedule.
pub(crate) fn serialize_transient_cases(cases:&[PreparedTransientCase])->Vec<Value> {
    cases.iter().map(|case| json!({
        "time":{"startSeconds":case.grid.start,"endSeconds":case.grid.end,"maxStepSeconds":case.grid.max_step,
            "maxSteps":case.grid.max_steps,"outputTimesSeconds":case.grid.output_times},
        "globalParameters":case.symbols.iter().map(|(name,value)|(name.clone(),json!(value.value))).collect::<Map<_,_>>(),
        "globalParameterDimensions":case.symbols.iter().map(|(name,value)|(name.clone(),json!(value.dimension))).collect::<Map<_,_>>(),
        "longitudinalWavenumber":{"real":case.longitudinal_wavenumber.re,"imaginary":case.longitudinal_wavenumber.im,"unit":"rad/m"},
        "parameterOverrides":case.parameters
    })).collect()
}

/// A shared model may contain several Study kinds. Time is meaningful only
/// for transient material evaluation; reject it before a harmonic or
/// stationary solve rather than silently substituting an arbitrary value.
fn reject_time_dependent_materials(model:&Value,profile:&str,study_type:&str)->Result<()> {
    let properties=crate::model_schema::material_properties_for_profile(profile)?;
    let materials=model.pointer("/model/materials").and_then(Value::as_array).ok_or("model.materials must be an array.")?;
    for (index,material) in materials.iter().enumerate() {
        for definition in properties {
            if study_type!="propagationMode"&&definition.get("studyTypes").and_then(Value::as_array).is_some_and(|types|!types.iter().any(|kind|kind.as_str()==Some(study_type))) {continue;}
            let Some(key)=definition.get("key").and_then(Value::as_str) else {continue;};
            let Some(raw)=material.get(key).or_else(||definition.get("default")) else {continue;};
            let expression=if definition.get("representation").and_then(Value::as_str)==Some("quantity") {
                raw.get("expression").and_then(Value::as_str)
            } else {raw.as_str()};
            if let Some(expression)=expression {
                let (_,symbols,_)=model_expression::inspect(expression)?;
                if symbols.iter().any(|name|name=="t") {
                    return Err(format!("Material {} ({key}) uses time t, which requires a transient Study.",
                        material.get("id").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(||index.to_string())));
                }
            }
        }
    }
    Ok(())
}

/// Modal input coefficients use the same fixed-time, frequency-independent
/// constitutive contract as the shared transient element forms. Applied
/// volume sources are omitted from this source-free solve and are not scanned.
fn reject_eigenfrequency_physics_inputs(model:&Value,package:&Value,formulation:&str)->Result<()> {
    let source_free=crate::transient_case::eigenfrequency_model(package,formulation,model)?;
    reject_physics_symbols(&source_free,&["t","freq","frequency","omega","i"],
        "must be time-independent and frequency-independent for an eigenfrequency Study")
}

fn reject_eigenfrequency_output_inputs(model:&Value)->Result<()> {
    for key in ["variables","pointwiseVariables"] {
        for (index,row) in model.pointer(&format!("/model/{key}")).and_then(Value::as_array).into_iter().flatten().enumerate(){
            if let Some(expression)=row.get("expression").and_then(Value::as_str){
                let (_,symbols,_)=model_expression::inspect(expression)?;
                if let Some(name)=symbols.iter().find(|name|matches!(name.as_str(),"t"|"freq"|"frequency"|"omega"|"i")){
                    return Err(format!("model.{key}[{index}].expression uses {name}, which is outside the static eigenfrequency output scope."));
                }
            }
        }
    }
    Ok(())
}

/// A shared model can retain harmonic material expressions. Only selecting a
/// transient Study rejects frequency-domain or complex constitutive input.
fn reject_nontransient_materials(model:&Value,profile:&str,study_type:&str)->Result<()> {
    let properties=crate::model_schema::material_properties_for_profile(profile)?;
    let materials=model.pointer("/model/materials").and_then(Value::as_array).ok_or("model.materials must be an array.")?;
    for (index,material) in materials.iter().enumerate() {
        for definition in properties {
            if definition.get("studyTypes").and_then(Value::as_array).is_some_and(|types|!types.iter().any(|kind|kind.as_str()==Some("transient"))) {continue;}
            let Some(key)=definition.get("key").and_then(Value::as_str) else {continue;};
            let Some(raw)=material.get(key).or_else(||definition.get("default")) else {continue;};
            let expression=if definition.get("representation").and_then(Value::as_str)==Some("quantity") {raw.get("expression").and_then(Value::as_str)}else{raw.as_str()};
            if let Some(expression)=expression {
                let (_,symbols,_)=model_expression::inspect(expression)?;
                if let Some(name)=symbols.iter().find(|name|matches!(name.as_str(),"freq"|"frequency"|"omega"|"i")) {
                    let id=material.get("id").and_then(Value::as_str).map(str::to_owned).unwrap_or_else(||index.to_string());
                    let requirement=if study_type=="eigenfrequency" {
                        "must be independent of harmonic frequency for an eigenfrequency Study"
                    } else {
                        "requires a declared causal time-domain material model for a transient Study"
                    };
                    return Err(format!("Material {id} ({key}) uses {name}, which {requirement}."));
                }
            }
        }
    }
    Ok(())
}

/// Shared model validation permits harmonic inputs in a mixed-study model.
/// The selected transient Study must use real, time-domain Physics inputs.
fn reject_physics_symbols(model:&Value,forbidden:&[&str],description:&str)->Result<()> {
    fn visit(value:&Value,path:&str,forbidden:&[&str],description:&str)->Result<()> {
        match value {
            Value::Object(object)=>{
                if object.get("enabled").and_then(Value::as_bool)==Some(false){return Ok(());}
                if let Some(expression)=object.get("expression").and_then(Value::as_str){
                    let (_,symbols,_)=model_expression::inspect(expression)?;
                    if let Some(name)=symbols.iter().find(|name|forbidden.contains(&name.as_str())) {
                        return Err(format!("{path}.expression uses {name}, which {description}."));
                    }
                }
                for (key,child) in object {
                    if key!="expression" && !(path=="model.physics"&&key=="frequency") {visit(child,&format!("{path}.{key}"),forbidden,description)?;}
                }
            }
            Value::Array(items)=>for (index,item) in items.iter().enumerate(){visit(item,&format!("{path}[{index}]"),forbidden,description)?;},
            _=>{}
        }
        Ok(())
    }
    visit(model.pointer("/model/physics").ok_or("Physics is missing.")?,"model.physics",forbidden,description)
}
fn reject_nontransient_physics_inputs(model:&Value)->Result<()> {
    reject_physics_symbols(model,&["freq","frequency","omega","i"],"is not a real time-domain transient input")
}

fn reject_nontransient_output_inputs(model:&Value)->Result<()> {
    for key in ["variables","pointwiseVariables"] {
        for (index,row) in model.pointer(&format!("/model/{key}")).and_then(Value::as_array).into_iter().flatten().enumerate() {
            if row.get("symbol").and_then(Value::as_str)==Some("t") {
                return Err(format!("model.{key}[{index}] uses reserved transient time symbol t as a definition name."));
            }
            if let Some(expression)=row.get("expression").and_then(Value::as_str) {
                let (_,symbols,_)=model_expression::inspect(expression)?;
                if let Some(name)=symbols.iter().find(|name|matches!(name.as_str(),"freq"|"frequency"|"omega"|"i")) {
                    return Err(format!("model.{key}[{index}].expression uses {name}, which is not a real time-domain transient output."));
                }
            }
        }
    }
    Ok(())
}

/// Compile each enabled scenario-declared boundary before meshing. Constant
/// quantities can be checked for every parameter case; expressions involving
/// space or time remain subject to checks at the actual quadrature samples.
fn validate_transient_boundaries(model:&Value,package:&Value,formulation:&str,symbols:&HashMap<String,Scalar>)->Result<()> {
    fn has_runtime_expression(value:&Value)->Result<bool> {
        match value {
            Value::Object(rows)=>{
                if let Some(expression)=rows.get("expression").and_then(Value::as_str) {
                    let (_,names,_)=model_expression::inspect(expression)?;
                    if names.iter().any(|name|matches!(name.as_str(),"x"|"y"|"z"|"t")){return Ok(true);}
                }
                for child in rows.values(){if has_runtime_expression(child)? {return Ok(true);}}
            },
            Value::Array(rows)=>for child in rows {if has_runtime_expression(child)? {return Ok(true);}},
            _=>{}
        }
        Ok(false)
    }
    let declaration=package.get("transientForms").and_then(|rows|rows.get(formulation)).ok_or("Scenario transient form is missing.")?;
    let laws=declaration.get("boundaryLaws").and_then(Value::as_object).ok_or("Scenario transient boundary laws are missing.")?;
    let physics=model.pointer("/model/physics").ok_or("Physics is missing.")?;
    for row in crate::model_schema::physics_boundary_conditions(physics) {
        if row.get("enabled").and_then(Value::as_bool)==Some(false){continue;}
        let id=row.get("id").and_then(Value::as_str).unwrap_or("boundary");
        let kind=row.get("type").and_then(Value::as_str).ok_or("Boundary type is missing.")?;
        let law=laws.get(kind).ok_or_else(||format!("Boundary {id} ({kind}) has no declared transient law."))?;
        let input=law.get("input").ok_or("Transient boundary input contract is missing.")?;
        let compiled=crate::boundary_input::BoundaryInput::compile(input,row).map_err(|error|format!("Boundary {id}: {error}"))?;
        if input.pointer("/prescribed/scalarPolicy").and_then(Value::as_str)==Some("zeroOnly") {
            let path=input.pointer("/prescribed/scalar/path").and_then(Value::as_str).ok_or("Transient zero-only scalar path is missing.")?;
            if let Some(value)=row.pointer(path).filter(|value|value.is_object()) {
                if !has_runtime_expression(value)? {
                    let value=model_expression::evaluate_quantity(value,symbols,&format!("Boundary {id} scalar"))?;
                    let tolerance=input.pointer("/prescribed/zeroTolerance").and_then(Value::as_f64).unwrap_or(0.);
                    if value.abs()>tolerance{return Err(format!("Boundary {id} scalar input must be zero for the declared transient law; use a supported vector trace or zero."));}
                }
            }
        }
        if !has_runtime_expression(row)? {
            let form=compiled.form_key().map(|key| {
                let forms=declaration.get("boundaryForms").or_else(||package.get("boundaryForms").and_then(|rows|rows.get(formulation)))
                    .ok_or("Transient boundary forms are missing.")?;
                crate::boundary_form::BoundaryFormPlan::from_json(forms.get(key).ok_or("Transient boundary form is missing.")?)
            }).transpose()?;
            compiled.evaluate_at_time(symbols,Some([0.;3]),0.,&HashMap::new(),form.as_ref())
                .map_err(|error|format!("Boundary {id}: {error}"))?;
        }
    }
    Ok(())
}

/// Check constant values restricted by the selected transient form. Spatial
/// or temporal expressions are checked at actual quadrature samples instead.
fn reject_unimplemented_constant_materials(model:&Value,profile:&str,package:&Value,form:&str,symbols:&HashMap<String,Scalar>)->Result<()> {
    let declaration=package.get("transientForms").and_then(|rows|rows.get(form)).ok_or("Scenario transient form is missing.")?;
    let bindings=declaration.get("materialBindings").or_else(||package.get("materialBindings").and_then(|rows|rows.get(form)))
        .and_then(Value::as_array).ok_or("Scenario transient material bindings are missing.")?;
    let properties=crate::model_schema::material_properties_for_profile(profile)?;
    let materials=model.pointer("/model/materials").and_then(Value::as_array).ok_or("model.materials must be an array.")?;
    for material in materials {
        for binding in bindings {
            let Some(implemented)=binding.get("implementedValues").and_then(Value::as_array) else {continue;};
            let source=binding.get("source").and_then(Value::as_str).ok_or("Transient material source is missing.")?;
            let Some(property)=properties.iter().find(|row|row.get("symbol").and_then(Value::as_str)==Some(source)) else {continue;};
            let key=property.get("key").and_then(Value::as_str).ok_or("Material property key is missing.")?;
            let Some(raw)=material.get(key).or_else(||property.get("default")) else {continue;};
            let unit=property.get("unit").and_then(Value::as_str).ok_or("Material property unit is missing.")?;
            let quantity=if property.get("representation").and_then(Value::as_str)==Some("quantity") {raw.clone()}else{
                json!({"expression":raw.as_str().ok_or("Material expression must be a string.")?,"unit":unit})};
            let expression=quantity.get("expression").and_then(Value::as_str).ok_or("Material expression is missing.")?;
            let (_,used,_)=model_expression::inspect(expression)?;
            if used.iter().any(|name|matches!(name.as_str(),"x"|"y"|"z"|"t")){continue;}
            let value=model_expression::evaluate_quantity(&quantity,symbols,&format!("material {key}"))?;
            if !implemented.iter().any(|allowed|allowed.as_f64()==Some(value)) {
                return Err(format!("Material {} ({key}) value {value} is not implemented for a transient Study.",material.get("id").and_then(Value::as_str).unwrap_or("material")));
            }
        }
    }
    Ok(())
}

fn expand_parameter_axes(
    axes: &[(String, Vec<f64>, f64)],
    axis_index: usize,
    parameters: Vec<Value>,
    output: &mut Vec<Vec<Value>>,
) -> Result<()> {
    if axis_index == axes.len() {
        output.push(parameters);
        return Ok(());
    }
    let (symbol, points, unit_scale) = &axes[axis_index];
    for point in points {
        let mut case_parameters = parameters.clone();
        let target = case_parameters
            .iter_mut()
            .find(|entry| entry.get("symbol").and_then(Value::as_str) == Some(symbol.as_str()))
            .ok_or_else(|| {
                format!("Parameter sweep references unknown model parameter {symbol}.")
            })?;
        target["expression"] = json!((point / unit_scale).to_string());
        expand_parameter_axes(axes, axis_index + 1, case_parameters, output)?;
    }
    Ok(())
}

/// Integration operators can be referenced by their stable ID or their
/// user-facing symbol. Expressions normally use the symbol, while prepared
/// solver data uses the stable ID.
fn integration_operator_aliases(operators: &[Value]) -> Result<HashMap<String, String>> {
    let mut aliases = HashMap::<String, String>::new();
    for (index, operator) in operators.iter().enumerate() {
        let path = format!("model.integrationOperators[{index}]");
        let id = str_field(operator, "id", &path)?.to_string();
        let name = opt_string(operator, "name", &id).to_string();
        for alias in [id.as_str(), name.as_str()] {
            aliases.insert(alias.to_string(), id.clone());
        }
    }
    Ok(aliases)
}

fn normalize_integration_operator_calls(node: &mut Value, aliases: &HashMap<String, String>) {
    if node.get("kind").and_then(Value::as_str) == Some("call") {
        if let Some(name) = node.get("name").and_then(Value::as_str).map(str::to_string) {
            if let Some(id) = aliases.get(&name) {
                node["name"] = json!(id);
            }
        }
    }
    match node {
        Value::Array(items) => {
            for item in items {
                normalize_integration_operator_calls(item, aliases);
            }
        }
        Value::Object(object) => {
            for item in object.values_mut() {
                normalize_integration_operator_calls(item, aliases);
            }
        }
        _ => {}
    }
}

fn variables_for_study(
    model: &Value,
    component: &Value,
) -> Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
    let model_root = field(model, "model", "model")?;
    let variable_values = array(field(model_root, "variables", "model")?, "model.variables")?;
    let operators = model
        .pointer("/model/integrationOperators")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let operator_aliases = integration_operator_aliases(&operators)?;
    let pointwise_values = model
        .pointer("/model/pointwiseVariables")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut pointwise_output = Vec::with_capacity(pointwise_values.len());
    for (index, variable) in pointwise_values.iter().enumerate() {
        let path = format!("model.pointwiseVariables[{index}]");
        let symbol = str_field(variable, "symbol", &path)?;
        let expression = str_field(variable, "expression", &path)?;
        let (ast, _, _) = model_expression::inspect(expression)
            .map_err(|error| format!("{path}.expression: {error}"))?;
        pointwise_output.push(json!({"symbol":symbol,"ast":ast}));
    }
    let mut definitions = HashMap::<String, Value>::new();
    let mut variable_order = Vec::<String>::new();
    let mut symbols_by_var = HashMap::<String, BTreeSet<String>>::new();
    let mut calls_by_var = HashMap::<String, BTreeSet<String>>::new();
    for (index, variable) in variable_values.iter().enumerate() {
        let path = format!("model.variables[{index}]");
        let symbol = str_field(variable, "symbol", &path)?.to_string();
        let expression = str_field(variable, "expression", &path)?;
        let (mut ast, symbols, calls) = model_expression::inspect(expression)
            .map_err(|error| format!("{path}.expression: {error}"))?;
        normalize_integration_operator_calls(&mut ast, &operator_aliases);
        let mut definition = variable.clone();
        definition["ast"] = ast;
        definitions.insert(symbol.clone(), definition);
        variable_order.push(symbol.clone());
        symbols_by_var.insert(symbol.clone(), symbols);
        calls_by_var.insert(symbol, calls);
    }
    let mut ordered_symbols = Vec::new();
    let mut complete = HashSet::new();
    fn visit_variable(
        symbol: &str,
        definitions: &HashMap<String, Value>,
        dependencies: &HashMap<String, BTreeSet<String>>,
        complete: &mut HashSet<String>,
        ordered: &mut Vec<String>,
    ) {
        // The schema's expression validator already proved acyclicity. Mark
        // on entry to keep this output-order traversal bounded independently.
        if !complete.insert(symbol.to_string()) {
            return;
        }
        if let Some(references) = dependencies.get(symbol) {
            for dependency in references {
                if definitions.contains_key(dependency) {
                    visit_variable(dependency, definitions, dependencies, complete, ordered);
                }
            }
        }
        ordered.push(symbol.to_string());
    }
    for symbol in &variable_order {
        visit_variable(
            symbol,
            &definitions,
            &symbols_by_var,
            &mut complete,
            &mut ordered_symbols,
        );
    }
    let ordered = ordered_symbols
        .iter()
        .filter_map(|symbol| definitions.get(symbol).cloned())
        .collect::<Vec<_>>();
    let referenced_calls = calls_by_var
        .values()
        .flat_map(|calls| calls.iter())
        .filter_map(|call| operator_aliases.get(call).cloned())
        .collect::<HashSet<_>>();
    let mut integration_output = Vec::new();
    for operator in operators.iter().filter(|operator| {
        operator
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| referenced_calls.contains(id))
    }) {
        let id = str_field(operator, "id", "integrationOperator")?;
        let name = opt_string(operator, "name", id);
        let selection_id = str_field(
            operator,
            "selectionId",
            &format!("integration operator {name}"),
        )?;
        let selection = find_id(
            array(field(component, "selections", "model")?, "model.selections")?,
            selection_id,
        )
        .ok_or_else(|| {
            format!("Integration operator {name} references unknown selection {selection_id}.")
        })?;
        let operator_type = crate::model_schema::integration_kind_for_selection(selection)
            .ok_or_else(|| format!("Integration operator {name} selection {selection_id} has no schema-declared integration kind."))?;
        // The mesh map resolves this selection to tagged mesh entities.
        // Selection expressions are interpreted by the mesher.
        integration_output
            .push(json!({"id":id,"integrationKind":operator_type,"selectionId":selection_id}));
    }
    Ok((ordered, integration_output, pointwise_output))
}

fn periodic_pairs_for_symbols(
    component: &Value,
    physics: &Value,
    symbols: &HashMap<String, Scalar>,
) -> Result<Vec<Value>> {
    let mesh = field(component, "mesh", "model")?;
    let pairs = array(
        field(mesh, "periodicPairs", "model.mesh")?,
        "model.mesh.periodicPairs",
    )?;
    let mut periodic_request = Vec::new();
    for feature in crate::model_schema::physics_periodic_conditions(physics)
        .filter(|feature| feature.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        let pair_id = str_field(feature, "periodicPairId", "periodic condition")?;
        let pair = find_id(pairs, pair_id)
            .ok_or_else(|| format!("Unknown mesh periodic pair {pair_id}."))?;
        let translation = array(
            field(pair, "translation", pair_id)?,
            &format!("{pair_id}.translation"),
        )?;
        let phase = feature
            .get("phaseVector")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_else(|| vec![json!({"expression":"0","unit":"rad/m"}); 3]);
        let translation = translation
            .iter()
            .enumerate()
            .map(|(index, value)| {
                quantity(value, symbols, &format!("{pair_id}.translation[{index}]"))
            })
            .collect::<Result<Vec<_>>>()?;
        let phase = phase
            .iter()
            .enumerate()
            .map(|(index, value)| {
                quantity(value, symbols, &format!("{pair_id}.phaseVector[{index}]"))
            })
            .collect::<Result<Vec<_>>>()?;
        periodic_request.push(json!({
            "id":pair_id,
            "lowerSelectionId":field(pair, "lowerSelectionId", pair_id)?,
            "upperSelectionId":field(pair, "upperSelectionId", pair_id)?,
            "translationM":translation,
            "phaseVector":phase
        }));
    }
    Ok(periodic_request)
}

/// Installed schemas select a common preparation contract or a scenario adapter.
pub(crate) fn preparation_entry_for_study<'a>(capabilities:&'a Value,profile:&str,study_type:Option<&str>)->Option<&'a Value> {
    let typed=study_type.and_then(|kind|capabilities.pointer("/study/preparationByStudyTypeByProfile")
        .and_then(|profiles|profiles.get(profile)).and_then(|entries|entries.get(kind)));
    typed.or_else(||capabilities.pointer("/study/preparationByProfile").and_then(|profiles|profiles.get(profile)))
}

pub(crate) fn preparation_for_study<'a>(capabilities:&'a Value,profile:&str,study_type:Option<&str>)->Option<&'a str> {
    preparation_entry_for_study(capabilities,profile,study_type).and_then(|entry|entry.get("kind")).and_then(Value::as_str)
}

pub(crate) fn selected_study_type<'a>(model:&'a Value,study_id:Option<&str>)->Option<&'a str> {
    study_id.and_then(|id|model.pointer("/model/study").and_then(Value::as_array)
        .and_then(|studies|studies.iter().find(|study|study.get("id").and_then(Value::as_str)==Some(id)))
        .and_then(|study|study.get("type")).and_then(Value::as_str))
}

pub fn uses_common_preparation(model: &Value, capabilities: &Value, study_id:Option<&str>) -> Result<bool> {
    let profile = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .ok_or("The model requires scenario.profileId.")?;
    let study_type=selected_study_type(model,study_id);
    Ok(matches!(
        preparation_for_study(capabilities,profile,study_type),
        Some("frequencyPlan" | "configuredFrequencyRequest" | "stationaryPlan" | "transientPlan" | "eigenfrequencyPlan" | "propagationModePlan")
    ))
}

pub(crate) fn prepare_validated_study(envelope: &Value, capabilities: &Value) -> Result<Value> {
    let model = field(envelope, "model", "prepare request")?;
    let profile_id = model
        .pointer("/scenario/profileId")
        .and_then(Value::as_str)
        .ok_or("The model requires scenario.profileId.")?;
    let study_id = str_field(envelope, "studyId", "prepare request")?;
    let model_root = field(model, "model", "model")?;
    let study = find_id(array(field(model_root, "study", "model")?, "model.study")?,study_id)
        .ok_or_else(|| format!("Unknown study {study_id}."))?;
    let preparation=preparation_for_study(capabilities,profile_id,study.get("type").and_then(Value::as_str))
        .ok_or("The scenario schema must declare a Study preparation contract.")?;
    if !uses_common_preparation(model, capabilities,Some(study_id))? {
        return Err(format!(
            "Preparation contract {preparation:?} is owned by its scenario adapter."
        ));
    }
    let capability_study = field(capabilities, "study", "engineCapabilities")?;
    let settings_catalog = field(
        capability_study,
        "solverSettingsByProfile",
        "engineCapabilities.study",
    )?;
    let profile_settings = settings_catalog.get(profile_id).ok_or_else(|| {
        format!("No solver settings are declared for scenario profile {profile_id:?}.")
    })?;
    let defaults = field(profile_settings, "defaults", "scenario solver settings")?;
    let component = model_root;
    let physics_id = str_field(study, "physicsId", "study")?;
    let physics = crate::model_schema::physics(component)
        .filter(|physics| physics.get("id").and_then(Value::as_str) == Some(physics_id))
        .ok_or_else(|| {
            format!("Study {study_id} references unknown Physics interface {physics_id}.")
        })?;
    let discretization = study.get("discretization").unwrap_or(&Value::Null);
    let solver = study.get("solver").unwrap_or(&Value::Null);
    let physics_formulation = physics.get("formulation").and_then(Value::as_str);
    let default_formulation = if profile_settings
        .get("fieldFormulationsFromPhysics")
        .and_then(Value::as_bool)
        == Some(true)
    {
        physics_formulation
            .ok_or("The linked Physics interface does not declare a field formulation.")?
    } else {
        str_field(
            defaults,
            "fieldFormulation",
            "scenario solver settings.defaults",
        )?
    };
    let formulation = discretization
        .get("fieldFormulation")
        .and_then(Value::as_str)
        .unwrap_or(default_formulation);
    let order_default = field(
        defaults,
        "elementOrder",
        "scenario solver settings.defaults",
    )?
    .as_u64()
    .ok_or("Scenario default elementOrder must be an unsigned integer.")?;
    let order = discretization
        .get("elementOrder")
        .and_then(Value::as_u64)
        .unwrap_or(order_default);
    let solver_settings = Value::Object(crate::solver_options::resolve(
        capability_study,
        profile_settings,
        solver,
        formulation,
        order,
    )?);
    let backend = str_field(&solver_settings, "backend", "resolved solver settings")?;
    let constitutive_quadrature = str_field(
        &solver_settings,
        "constitutiveQuadrature",
        "resolved solver settings",
    )?;
    let ordering = str_field(&solver_settings, "ordering", "resolved solver settings")?;
    let threads = solver_settings
        .get("threads")
        .and_then(Value::as_u64)
        .ok_or("Resolved solver settings omit an unsigned thread count.")?;
    if study.get("type").and_then(Value::as_str)==Some("eigenfrequency") {
        reject_time_dependent_materials(model,profile_id,"transient")?;
        reject_nontransient_materials(model,profile_id,"eigenfrequency")?;
        let package=crate::scenario_package::load_installed(profile_id)?;
        reject_eigenfrequency_physics_inputs(model,&package,formulation)?;
        reject_eigenfrequency_output_inputs(model)?;
    } else if study.get("type").and_then(Value::as_str)==Some("propagationMode") {
        reject_time_dependent_materials(model,profile_id,"propagationMode")?;
    } else if let Some(study_type)=study.get("type").and_then(Value::as_str).filter(|kind|*kind!="transient") {
        reject_time_dependent_materials(model,profile_id,study_type)?;
        reject_physics_symbols(model,&["t"],"requires a transient Study")?;
    }
    if preparation == "transientPlan" {
        if study.get("type").and_then(Value::as_str)!=Some("transient") {
            return Err("A transient preparation contract requires a transient Study.".into());
        }
        reject_nontransient_materials(model,profile_id,"transient")?;
        reject_nontransient_physics_inputs(model)?;
        reject_nontransient_output_inputs(model)?;
        let package=crate::scenario_package::load_installed(profile_id)?;
        let use_longitudinal_wavenumber=crate::longitudinal::use_study_wavenumber(study,"Transient Study")?;
        let cases=transient_cases(study,model)?;
        for (index,case) in cases.iter().enumerate() {
            validate_study_wavenumber_formulation(&package,"transient",formulation,case.longitudinal_wavenumber,use_longitudinal_wavenumber)
                .map_err(|error|format!("Transient case {}: {error}",index+1))?;
            reject_unimplemented_constant_materials(model,profile_id,&package,formulation,&case.symbols)
                .map_err(|error|format!("Transient case {}: {error}",index+1))?;
            crate::transient_initial::validate_study_contract(&package,formulation,study,&case.symbols)
                .map_err(|error|format!("Transient case {} initial fields: {error}",index+1))?;
            validate_transient_boundaries(model,&package,formulation,&case.symbols)
                .map_err(|error|format!("Transient case {}: {error}",index+1))?;
        }
        let residual_tolerance=capability_study.get("transientResidualTolerance").and_then(Value::as_f64)
            .filter(|value|value.is_finite() && *value>0.)
            .ok_or("Study capability must declare a positive transientResidualTolerance.")?;
        let output=study.get("output").unwrap_or(&Value::Null);
        let field_files=output.get("fieldFiles").map(|value|value.as_bool().ok_or("Transient fieldFiles must be boolean.")).transpose()?.unwrap_or(true);
        let csv_name=envelope.get("csvFilename").and_then(Value::as_str).or_else(||output.get("csvFilename").and_then(Value::as_str)).unwrap_or("transient-history.csv");
        if csv_name.len()>103||!csv_name.ends_with(".csv")||csv_name.len()<=4||!csv_name.chars().enumerate().all(|(index,c)|
            if index==0 {c.is_ascii_alphanumeric()}else{c.is_ascii_alphanumeric()||matches!(c,'.'|'_'|'-')}) {
            return Err("The transient Study CSV must be one safe .csv filename.".into());
        }
        return Ok(json!({"schema":"fullmeta-transient-study-plan/1","studyType":"transient","studyId":study_id,"profileId":profile_id,
            "solverSettings":{"fieldFormulation":formulation,"elementOrder":order,"constitutiveQuadrature":constitutive_quadrature,
                "solverBackend":backend,"solverOrdering":ordering,"solverThreads":threads,"residualTolerance":residual_tolerance},
            "fieldFiles":field_files,"csvFilename":csv_name,"cases":serialize_transient_cases(&cases)}));
    }
    if preparation == "eigenfrequencyPlan" {
        if study.get("type").and_then(Value::as_str)!=Some("eigenfrequency") {
            return Err("An eigenfrequency preparation contract requires an eigenfrequency Study.".into());
        }
        let package=crate::scenario_package::load_installed(profile_id)?;
        let modal_model=crate::transient_case::eigenfrequency_model(&package,formulation,model)?;
        let cases=parameter_cases(study,model)?;
        for (index,(symbols,_)) in cases.iter().enumerate(){
            reject_unimplemented_constant_materials(model,profile_id,&package,formulation,symbols)
                .map_err(|error|format!("Eigenfrequency case {}: {error}",index+1))?;
            validate_transient_boundaries(&modal_model,&package,formulation,symbols)
                .map_err(|error|format!("Eigenfrequency case {}: {error}",index+1))?;
        }
        let settings=study.get("eigenfrequency").unwrap_or(&Value::Null);
        let mode_count=settings.get("modeCount").and_then(Value::as_u64).and_then(|v|usize::try_from(v).ok()).filter(|v|*v>0&&*v<=256)
            .ok_or("Eigenfrequency modeCount must be an integer from 1 to 256.")?;
        let krylov_dimension=settings.get("krylovDimension").and_then(Value::as_u64).and_then(|v|usize::try_from(v).ok()).filter(|v|*v>0&&*v<=512)
            .ok_or("Eigenfrequency krylovDimension must be an integer from 1 to 512.")?;
        if krylov_dimension<mode_count{return Err("Eigenfrequency krylovDimension must be at least modeCount.".into());}
        let residual_tolerance=settings.get("residualTolerance").and_then(Value::as_f64).filter(|v|v.is_finite()&&*v>0.&&*v<1.)
            .ok_or("Eigenfrequency residualTolerance must be finite and between 0 and 1.")?;
        let shift=settings.get("shift").ok_or("Eigenfrequency temporal shift is required.")?;
        let shift_real=field(shift,"real","eigenfrequency.shift")?;
        let shift_imag=field(shift,"imaginary","eigenfrequency.shift")?;
        // The bundled legacy core DLL exposes Hz, but not the equivalent
        // spelling 1/s. Both have SI dimension T^-1 and unit scale 1, so the
        // stored values remain the temporal eigenvalue s in inverse seconds.
        let inverse_seconds=model_expression::unit("Hz")?.1;
        let quantity_shift=|value:&Value,path:&str,symbols:&HashMap<String,Scalar>|->Result<f64>{
            let unit=str_field(value,"unit",path)?;
            if model_expression::unit(unit)?.1.iter().zip(inverse_seconds).any(|(a,b)|(a-b).abs()>1e-10){
                return Err(format!("{path} must have inverse-second units for the temporal eigenvalue s."));
            }
            quantity(value,symbols,path)
        };
        let mut prepared_cases=Vec::with_capacity(cases.len());
        for (index,(symbols,parameters)) in cases.into_iter().enumerate(){
            let real=quantity_shift(shift_real,&format!("eigenfrequency.shift.real[{}]",index+1),&symbols)?;
            let imaginary=quantity_shift(shift_imag,&format!("eigenfrequency.shift.imaginary[{}]",index+1),&symbols)?;
            let longitudinal_wavenumber=crate::longitudinal::study_wavenumber(
                study,&symbols,&format!("Eigenfrequency Study longitudinalWavenumber[{}]",index+1),
            )?;
            let use_longitudinal_wavenumber=crate::longitudinal::use_study_wavenumber(study,"Eigenfrequency Study")?;
            validate_study_wavenumber_formulation(&package,"eigenfrequency",formulation,longitudinal_wavenumber,use_longitudinal_wavenumber)
                .map_err(|error|format!("Eigenfrequency case {}: {error}",index+1))?;
            prepared_cases.push(json!({
                "shift":{"realPerSecond":real,"imaginaryPerSecond":imaginary},
                "globalParameters":symbols.iter().map(|(name,value)|(name.clone(),json!(value.value))).collect::<Map<_,_>>(),
                "globalParameterDimensions":symbols.iter().map(|(name,value)|(name.clone(),json!(value.dimension))).collect::<Map<_,_>>(),
                "longitudinalWavenumber":{"real":longitudinal_wavenumber.re,"imaginary":longitudinal_wavenumber.im,"unit":"rad/m"},
                "parameterOverrides":parameters
            }));
        }
        let output=study.get("output").unwrap_or(&Value::Null);
        let field_files=output.get("fieldFiles").and_then(Value::as_bool)
            .or_else(||capability_study.pointer("/outputDefaults/fieldFiles").and_then(Value::as_bool))
            .ok_or("Study output defaults must declare fieldFiles.")?;
        return Ok(json!({"schema":"fullmeta-eigenfrequency-study-plan/1","studyType":"eigenfrequency",
            "studyId":study_id,"profileId":profile_id,
            "solverSettings":{"fieldFormulation":formulation,"elementOrder":order,"constitutiveQuadrature":constitutive_quadrature,
                "solverBackend":backend,"solverOrdering":ordering,"solverThreads":threads,"residualTolerance":residual_tolerance,
                "modeCount":mode_count,"krylovDimension":krylov_dimension},
            "fieldFiles":field_files,"cases":prepared_cases}));
    }
    if preparation == "propagationModePlan" {
        if study.get("type").and_then(Value::as_str)!=Some("propagationMode") {
            return Err("A propagation-mode preparation contract requires a propagationMode Study.".into());
        }
        let package=crate::scenario_package::load_installed(profile_id)?;
        let contract=package.get("propagationModes").and_then(Value::as_object)
            .ok_or("Scenario package does not declare propagationModes.")?;
        let formulation_contract=contract.get("formulations").and_then(|value|value.get(formulation))
            .ok_or_else(||{
                let required=contract.get("requiredFormulation").and_then(Value::as_str);
                required.map(|value|format!("Propagation Mode requires the {value} formulation; selected {formulation:?}."))
                    .unwrap_or_else(||format!("Scenario does not declare propagation modes for formulation {formulation:?}."))
            })?;
        if formulation_contract.get("operatorFormulation").and_then(Value::as_str).is_some_and(|operator|package.pointer(&format!("/weakForms/{operator}/dimension")).and_then(Value::as_u64)!=Some(2)) {
            return Err("Propagation-mode operator formulation must be two-dimensional.".into());
        }
        let mode_settings=study.get("propagationMode").unwrap_or(&Value::Null);
        let mode_count=mode_settings.get("modeCount").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).filter(|value|*value>0&&*value<=256)
            .ok_or("Propagation-mode modeCount must be an integer from 1 to 256.")?;
        let krylov_dimension=mode_settings.get("krylovDimension").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok()).filter(|value|*value>0&&*value<=512)
            .ok_or("Propagation-mode krylovDimension must be an integer from 1 to 512.")?;
        if krylov_dimension<mode_count{return Err("Propagation-mode krylovDimension must be at least modeCount.".into());}
        let residual_tolerance=mode_settings.get("residualTolerance").and_then(Value::as_f64).filter(|value|value.is_finite()&&*value>0.&&*value<1.)
            .ok_or("Propagation-mode residualTolerance must be finite and between 0 and 1.")?;
        let frequency=field(mode_settings,"frequency","propagationMode")?;
        let shift=field(mode_settings,"shift","propagationMode")?;
        let shift_real=field(shift,"real","propagationMode.shift")?;
        let shift_imaginary=field(shift,"imaginary","propagationMode.shift")?;
        let inverse_length=model_expression::unit("rad/m")?.1;
        let wave_number=|value:&Value,path:&str,symbols:&HashMap<String,Scalar>|->Result<f64>{
            let unit=str_field(value,"unit",path)?;
            if model_expression::unit(unit)?.1.iter().zip(inverse_length).any(|(left,right)|(left-right).abs()>1e-10){
                return Err(format!("{path} must have inverse-length units for kz."));
            }
            quantity(value,symbols,path)
        };
        let modal_model=crate::transient_case::eigenfrequency_model(&package,formulation,model)?;
        reject_physics_symbols(&modal_model,&["t"],"requires time-independent propagation-mode boundary and material inputs")?;
        let cases=parameter_cases(study,model)?;
        let mut prepared_cases=Vec::with_capacity(cases.len());
        for (index,(symbols,parameters)) in cases.into_iter().enumerate(){
            let case_frequency=quantity(frequency,&symbols,&format!("propagationMode.frequency[{}]",index+1))?;
            let frequency_unit=str_field(frequency,"unit","propagationMode.frequency")?;
            if model_expression::unit(frequency_unit)?.1.iter().zip(model_expression::unit("Hz")?.1).any(|(left,right)|(left-right).abs()>1e-10){
                return Err("propagationMode.frequency must have frequency units (Hz).".into());
            }
            if !case_frequency.is_finite()||case_frequency<=0.{return Err("Propagation-mode frequency must be positive and finite.".into());}
            let real=wave_number(shift_real,&format!("propagationMode.shift.real[{}]",index+1),&symbols)?;
            let imaginary=wave_number(shift_imaginary,&format!("propagationMode.shift.imaginary[{}]",index+1),&symbols)?;
            validate_transient_boundaries(&modal_model,&package,formulation,&symbols)
                .map_err(|error|format!("Propagation-mode case {}: {error}",index+1))?;
            reject_unimplemented_constant_materials(model,profile_id,&package,formulation,&symbols)
                .map_err(|error|format!("Propagation-mode case {}: {error}",index+1))?;
            prepared_cases.push(json!({"frequencyHz":case_frequency,"shift":{"realRadPerMetre":real,"imaginaryRadPerMetre":imaginary},
                "globalParameters":symbols.iter().map(|(name,value)|(name.clone(),json!(value.value))).collect::<Map<_,_>>(),
                "globalParameterDimensions":symbols.iter().map(|(name,value)|(name.clone(),json!(value.dimension))).collect::<Map<_,_>>(),
                "parameterOverrides":parameters}));
        }
        let field_files=study.get("output").and_then(|output|output.get("fieldFiles")).and_then(Value::as_bool)
            .or_else(||capability_study.pointer("/outputDefaults/fieldFiles").and_then(Value::as_bool)).unwrap_or(true);
        return Ok(json!({"schema":"fullmeta-propagation-mode-study-plan/1","studyType":"propagationMode","studyId":study_id,"profileId":profile_id,
            "solverSettings":{"fieldFormulation":formulation,"elementOrder":order,"constitutiveQuadrature":constitutive_quadrature,
                "solverBackend":backend,"solverOrdering":ordering,"solverThreads":threads,"residualTolerance":residual_tolerance,
                "modeCount":mode_count,"krylovDimension":krylov_dimension},"fieldFiles":field_files,"cases":prepared_cases}));
    }
    if preparation == "stationaryPlan" {
        let scenario = crate::model_schema::scenario_metadata_for_profile(profile_id)?;
        let result_defaults = scenario.get("resultDefaults").unwrap_or(&Value::Null);
        let default_csv = scenario
            .pointer("/studyFingerprint/defaultCsvFilename")
            .and_then(Value::as_str);
        let output = study.get("output").unwrap_or(&Value::Null);
        let csv_name = envelope
            .get("csvFilename")
            .and_then(Value::as_str)
            .or_else(|| output.get("csvFilename").and_then(Value::as_str))
            .or(default_csv)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{study_id}.csv"));
        if csv_name.len() > 103
            || !csv_name.ends_with(".csv")
            || csv_name.len() <= 4
            || !csv_name.chars().enumerate().all(|(index, character)| {
                if index == 0 {
                    character.is_ascii_alphanumeric()
                } else {
                    character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
                }
            })
        {
            return Err("The stationary Study CSV must be one safe .csv filename.".into());
        }
        let field_files = output
            .get("fieldFiles")
            .and_then(Value::as_bool)
            .or_else(|| capability_study.pointer("/outputDefaults/fieldFiles").and_then(Value::as_bool))
            .ok_or("Study output defaults must declare fieldFiles.")?;
        let field_filename = result_defaults
            .get("stationaryFieldFilename")
            .and_then(Value::as_str);
        let field_schema = result_defaults
            .get("stationaryFieldSchema")
            .and_then(Value::as_str);
        if field_files && (field_filename.is_none() || field_schema.is_none()) {
            return Err("A stationary scenario with field output must declare stationaryFieldFilename and stationaryFieldSchema in its result defaults.".into());
        }
        let request = json!({
            "schema":"fem-solver-request/1",
            "solver":profile_id,
            "studyId":study_id,
            "fieldFormulation":formulation,
            "elementOrder":order,
            "constitutiveQuadrature":constitutive_quadrature,
            "solverBackend":backend,
            "solverOrdering":ordering,
            "solverThreads":threads,
            "csvFilename":csv_name,
            "fieldFiles":field_files,
            "fieldFilename":field_filename,
            "modelFile":envelope.get("modelFile").and_then(Value::as_str).unwrap_or("model.json"),
            "meshFile":str_field(envelope, "meshFile", "prepare request")?,
            "modelMapFile":envelope.get("modelMapFile").and_then(Value::as_str).unwrap_or("mesh/model-model-map.json"),
            "outputDirectory":envelope.get("outputDirectory").and_then(Value::as_str).unwrap_or("output")
        });
        return Ok(json!({
            "schema":"fullmeta-study-plan/1",
            "studyId":study_id,
            "profileId":profile_id,
            "studyType":"stationary",
            "totalPoints":1,
            "solverSettings":{
                "fieldFormulation":formulation,
                "elementOrder":order,
                "constitutiveQuadrature":constitutive_quadrature,
                "solverBackend":backend,
                "solverOrdering":ordering,
                "solverThreads":threads
            },
            "request":request,
            "study":study,
            "component":component,
            "csvFilename":csv_name,
            "fieldFiles":field_files,
            "stationaryFieldFilename":field_filename,
            "stationaryFieldSchema":field_schema,
            "stationaryFieldLabel":result_defaults.get("stationaryFieldLabel")
        }));
    }
    let cases = frequency_cases(study, physics, model,profile_id,formulation)?;
    let frequencies_hz = cases.iter().map(|case| case.frequency).collect::<Vec<_>>();
    for (index, frequency) in frequencies_hz.iter().enumerate() {
        crate::model_validation::validate_prepared_frequency(
            *frequency,
            &format!("{study_id}.frequency[{index}]"),
        )?;
    }
    let frequency_cases = cases.iter().map(|case| {
        let global_parameters = case.symbols.iter().map(|(symbol, scalar)| (symbol.clone(), json!(scalar.value))).collect::<Map<_,_>>();
        json!({"frequencyHz":case.frequency,"longitudinalWavenumber":{"real":case.longitudinal_wavenumber.re,"imaginary":case.longitudinal_wavenumber.im,"unit":"rad/m"},"globalParameters":global_parameters,"globalParameterDimensions":case.symbols.iter().map(|(name,value)|(name.clone(),json!(value.dimension))).collect::<Map<_,_>>(),"parameterOverrides":case.parameters})
    }).collect::<Vec<_>>();
    if preparation == "frequencyPlan" {
        let output = study.get("output").unwrap_or(&Value::Null);
        let csv_name = envelope
            .get("csvFilename")
            .and_then(Value::as_str)
            .or_else(|| output.get("csvFilename").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| format!("{study_id}.csv"));
        let field_files = output
            .get("fieldFiles")
            .and_then(Value::as_bool)
            .or_else(|| {
                capability_study
                    .pointer("/outputDefaults/fieldFiles")
                    .and_then(Value::as_bool)
            })
            .ok_or("Study output defaults must declare fieldFiles.")?;
        return Ok(
            json!({"schema":"fullmeta-study-plan/1","studyId":study_id,"profileId":profile_id,
            "solverSettings":{"fieldFormulation":formulation,"elementOrder":order,"constitutiveQuadrature":constitutive_quadrature,
                "solverBackend":backend,"solverOrdering":ordering,"solverThreads":threads},
            "frequenciesHz":frequencies_hz,"frequencyCases":frequency_cases,
            "csvFilename":csv_name,"fieldFiles":field_files}),
        );
    }
    let mesh_file = str_field(envelope, "meshFile", "prepare request")?;
    let parameters = array(
        field(model_root, "parameters", "model")?,
        "model.parameters",
    )?;
    let symbols = values_by_symbol(model, parameters)?;
    let (variables, integration_operators, pointwise_variables) =
        variables_for_study(model, component)?;
    let mut impedance_boundaries = Vec::new();
    let mut prescribed_boundaries = Vec::new();
    for boundary in crate::model_schema::physics_boundary_conditions(physics)
        .filter(|boundary| boundary.get("enabled").and_then(Value::as_bool) != Some(false))
    {
        let id = str_field(boundary, "id", "impedance boundary")?;
        let kind = boundary.get("type").and_then(Value::as_str).unwrap_or("");
        if matches!(kind, "dirichlet" | "neumann") {
            let mut prepared = json!({"id":id,"type":kind,"selectionId":str_field(boundary, "selectionId", id)?,"value":field(boundary, "value", id)?});
            if let Some(vector) = boundary.get("fieldValues") { prepared["fieldValues"] = vector.clone(); }
            prescribed_boundaries.push(prepared);
            continue;
        }
        if kind != "impedance" {
            return Err(format!(
                "Profile {profile_id}, Study {study_id}, preparation: enabled boundary {id} must use impedance."
            ));
        }
        let selection_id = str_field(boundary, "selectionId", id)?;
        let mut prepared =
            json!({"id":id,"selectionId":selection_id,"impedance":field(boundary,"impedance",id)?});
        if let Some(incident) = boundary.get("incidentField") {
            prepared["incidentField"] = incident.clone();
            prepared["polarization"] = boundary
                .get("polarization")
                .cloned()
                .ok_or_else(|| format!("Boundary {id} requires incident polarization."))?;
        }
        impedance_boundaries.push(prepared);
    }
    let periodic_request = periodic_pairs_for_symbols(component, physics, &symbols)?;
    let domain_sources = crate::model_schema::physics_domain_sources(physics)
        .filter(|source| source.get("enabled").and_then(Value::as_bool) != Some(false))
        .cloned().collect::<Vec<_>>();
    let mut assignments = Vec::new();
    for (index, material) in array(field(component, "materials", "model")?, "model.materials")?
        .iter()
        .enumerate()
    {
        assignments.push(json!({"selectionId":field(material,"selectionId","material")?,"materialId":material.get("id"),"physicalTag":index+1,"material":material}));
    }
    let domain_selection_ids = physics
        .get("domainSelectionIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let output = study.get("output").unwrap_or(&Value::Null);
    let csv_name = envelope
        .get("csvFilename")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .or_else(|| {
            output
                .get("csvFilename")
                .and_then(Value::as_str)
                .filter(|name| !name.is_empty())
        })
        .map(str::to_string)
        .unwrap_or_else(|| format!("{study_id}.csv"));
    let debug_csv = format!("{}-debug.csv", &csv_name[..csv_name.len() - 4]);
    let field_files = output
        .get("fieldFiles")
        .and_then(Value::as_bool)
        .or_else(|| {
            capability_study
                .pointer("/outputDefaults/fieldFiles")
                .and_then(Value::as_bool)
        })
        .ok_or("Study output defaults must declare fieldFiles.")?;
    let metadata_id = model
        .pointer("/metadata/id")
        .and_then(Value::as_str)
        .unwrap_or("model");
    let mut request = json!({"schema":"fem-solver-request/1","solver":profile_id,"studyId":study_id,"meshFile":mesh_file,
        "elementOrder":order,"fieldFormulation":formulation,"constitutiveQuadrature":constitutive_quadrature,
        "solverBackend":backend,"solverOrdering":ordering,"solverThreads":threads,"frequencyCases":frequency_cases,
        "outputDirectory":"output","sweepCsv":csv_name,"debugCsv":debug_csv,"fieldFilePrefix":format!("{metadata_id}_{study_id}_field"),
        "modelConfiguration":{"schema":capability_study.get("requestConfigurationSchema"),"scenarioProfileId":profile_id,
            "fieldComponentCoordinateSystem":"model","physicsId":physics_id,"domainSelectionIds":domain_selection_ids,
            "periodicPairIds":periodic_request.iter().map(|entry|entry.get("id")).collect::<Vec<_>>(),"periodicPairs":periodic_request,
            "impedanceBoundaries":impedance_boundaries,"prescribedBoundaries":prescribed_boundaries,"materialAssignments":assignments,"variables":variables,"pointwiseVariables":pointwise_variables,
            "integrationOperators":integration_operators,"fieldFiles":field_files}});
    if !domain_sources.is_empty() { request["modelConfiguration"]["domainSources"] = json!(domain_sources); }
    // Configured requests need a materialized configuration for every sweep
    // case. Keep this in the shared preparer so the coordinator can prepare a
    // whole sweep in one dispatcher call. Only periodic quantities currently
    // depend on resolved sweep parameters; the rest of the configuration is
    // identical across cases and is cloned from the shared base request.
    let case_model_configurations = if preparation == "configuredFrequencyRequest" {
        let base_configuration = request
            .get("modelConfiguration")
            .cloned()
            .ok_or("Configured Study preparation omitted modelConfiguration.")?;
        let mut configurations = Vec::with_capacity(cases.len());
        for prepared_case in &cases {
            let periodic_pairs =
                periodic_pairs_for_symbols(component, physics, &prepared_case.symbols)?;
            let mut configuration = base_configuration.clone();
            configuration["periodicPairs"] = json!(periodic_pairs);
            configurations.push(configuration);
        }
        Some(configurations)
    } else {
        None
    };
    let mut prepared = json!({"request":request,"study":study,"component":component,"assignments":assignments,
        "frequenciesHz":frequencies_hz});
    if let Some(configurations) = case_model_configurations {
        prepared["caseModelConfigurations"] = json!(configurations);
    }
    Ok(prepared)
}

#[cfg(test)]
mod multi_study_preparation_tests {
    #[test]
    fn transient_boundary_preparation_uses_declared_laws_and_per_case_parameters() {
        for text in [include_str!("scenarios/acoustic2d.json"),include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/elastic2d.json"),include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/em2d.json"),include_str!("scenarios/em3d.json"),
            include_str!("scenarios/thermal2d.json"),include_str!("scenarios/thermal3d.json")] {
            let package:serde_json::Value=serde_json::from_str(text).unwrap();let profile=package["id"].as_str().unwrap();
            for form in package["transientForms"].as_object().unwrap().keys(){
                let defaults=&package["schemaDocument"]["x-fem-physics"]["boundaryDefaultsByFormulation"][form];
                for kind in ["dirichlet","neumann"] {
                    // Physics defaults can include inputs for another study
                    // route. Exercise transient validation only for laws the
                    // selected transient formulation actually declares.
                    if !package["transientForms"][form]["boundaryLaws"][kind].is_object() { continue; }
                    let mut row=defaults[kind].clone();row["id"]=serde_json::json!("wall");row["type"]=serde_json::json!(kind);row["selectionId"]=serde_json::json!("wallSelection");
                    let mut model=serde_json::json!({"model":{"physics":{"wall":row}}});
                    super::validate_transient_boundaries(&model,&package,form,&std::collections::HashMap::new())
                        .unwrap_or_else(|error|panic!("{profile}/{form}/{kind}: {error}"));
                    if kind=="neumann" && package["transientForms"][form]["boundaryLaws"][kind]["input"]["prescribed"]["scalarPolicy"]=="zeroOnly" {
                        model["model"]["physics"]["wall"]["value"]["expression"]=serde_json::json!("a");
                        let unit=crate::model_expression::unit("1").unwrap().1;
                        let symbols=std::collections::HashMap::from([("a".into(),crate::model_expression::Scalar{value:1.,dimension:unit})]);
                        let error=super::validate_transient_boundaries(&model,&package,form,&symbols).unwrap_err();
                        assert!(error.contains("wall")&&error.contains("must be zero"),"{profile}/{form}: {error}");
                        model["model"]["physics"]["wall"]["enabled"]=serde_json::json!(false);
                        super::validate_transient_boundaries(&model,&package,form,&symbols).unwrap();
                    }
                }
            }
        }
    }
    #[test]
    fn transient_output_scope_reserves_time_and_rejects_harmonic_symbols() {
        let mut model=serde_json::json!({"model":{"variables":[{"symbol":"clock","expression":"t"}],
            "pointwiseVariables":[{"symbol":"wave","expression":"p*t"}]}});
        super::reject_nontransient_output_inputs(&model).unwrap();
        model["model"]["variables"][0]["expression"]=serde_json::json!("freq/freq");
        assert!(super::reject_nontransient_output_inputs(&model).unwrap_err().contains("time-domain transient output"));
        model["model"]["variables"][0]["expression"]=serde_json::json!("t");
        model["model"]["pointwiseVariables"][0]["symbol"]=serde_json::json!("t");
        assert!(super::reject_nontransient_output_inputs(&model).unwrap_err().contains("reserved transient time"));
    }
    #[test]
    fn transient_selection_checks_enabled_physics_inputs_without_rewriting_model() {
        let mut model=serde_json::json!({"model":{"physics":{"id":"physics","frequency":{"expression":"1","unit":"Hz"},
            "source":{"type":"domainSource","values":[{"expression":"freq/freq+i","unit":"1"}]}}}});
        let before=model.clone();
        let error=super::reject_nontransient_physics_inputs(&model).unwrap_err();
        assert!(error.contains("source")&&error.contains("transient"));assert_eq!(model,before);
        model["model"]["physics"]["source"]["enabled"]=serde_json::json!(false);
        super::reject_nontransient_physics_inputs(&model).unwrap();
        model["model"]["physics"]["source"]["enabled"]=serde_json::json!(true);
        model["model"]["physics"]["source"]["values"][0]["expression"]=serde_json::json!("t/tau+x/L");
        super::reject_nontransient_physics_inputs(&model).unwrap();
        assert!(super::reject_physics_symbols(&model,&["t"],"requires a transient Study").unwrap_err().contains("source"));
    }
    #[test]
    fn transient_selection_rejects_harmonic_material_symbols_without_removing_them() {
        for (profile,key,quantity) in [
            ("acoustic-2d","density",true),("acoustic-3d","density",true),
            ("em-2d","relativePermittivity",false),("em-3d","relativePermittivity",false),
            ("elastic-2d","density",true),("elastic-3d","density",true),
        ] {
            let value=if quantity {serde_json::json!({"expression":"freq/freq","unit":"kg/m^3"})}else{serde_json::json!("freq/freq")};
            let mut model=serde_json::json!({"model":{"materials":[{"id":"material"}]}});
            model["model"]["materials"][0][key]=value.clone();
            let error=super::reject_nontransient_materials(&model,profile,"transient").unwrap_err();
            assert!(error.contains("freq")&&error.contains("causal"),"{profile}: {error}");
            assert_eq!(model["model"]["materials"][0][key],value);
            let mut time_model=model.clone();
            if quantity {time_model["model"]["materials"][0][key]["expression"]=serde_json::json!("1+t/tau");}
            else {time_model["model"]["materials"][0][key]=serde_json::json!("1+t/tau");}
            super::reject_nontransient_materials(&time_model,profile,"transient").unwrap();
        }
    }
    #[test]
    fn transient_selection_rejects_constant_unimplemented_loss_without_changing_model() {
        for (profile,form,text,key) in [
            ("acoustic-2d","pressure",include_str!("scenarios/acoustic2d.json"),"lossFactor"),
            ("elastic-2d","planeStrain",include_str!("scenarios/elastic2d.json"),"densityLossFactor"),
        ] {
            let package:serde_json::Value=serde_json::from_str(text).unwrap();
            let mut model=serde_json::json!({"model":{"materials":[{"id":"material"}]}});
            model["model"]["materials"][0][key]=serde_json::json!("0.1");
            let error=super::reject_unimplemented_constant_materials(&model,profile,&package,form,&std::collections::HashMap::new()).unwrap_err();
            assert!(error.contains(key)&&error.contains("not implemented"),"{error}");
            assert_eq!(model["model"]["materials"][0][key],"0.1");
            model["model"]["materials"][0][key]=serde_json::json!("0");
            super::reject_unimplemented_constant_materials(&model,profile,&package,form,&std::collections::HashMap::new()).unwrap();
        }
    }
    use super::*;
    #[test]
    fn study_type_selects_its_preparation_with_legacy_fallback() {
        let caps=json!({"study":{"preparationByProfile":{"profile":{"kind":"frequencyPlan"}},
            "preparationByStudyTypeByProfile":{"profile":{"transient":{"kind":"transientPlan"}}}}});
        let model=json!({"scenario":{"profileId":"profile"},"model":{"study":[
            {"id":"harmonic","type":"frequencyDomain"},{"id":"time","type":"transient"}]}});
        assert_eq!(preparation_for_study(&caps,"profile",Some("transient")),Some("transientPlan"));
        assert_eq!(preparation_for_study(&caps,"profile",Some("frequencyDomain")),Some("frequencyPlan"));
        assert!(uses_common_preparation(&model,&caps,Some("harmonic")).unwrap());
        assert!(uses_common_preparation(&model,&caps,Some("time")).unwrap());
    }

    #[test]
    fn fixed_wavenumber_formulation_gate_is_contract_driven_and_allows_zero_kz() {
        let package=json!({"studyLongitudinalWavenumber":{"studies":{
            "transient":{"formulations":["Full"],"requiredFormulation":"Full"}}}});
        assert!(validate_study_wavenumber_formulation(&package,"transient","Ez",crate::solver_common::C::default(),false).is_ok());
        assert!(validate_study_wavenumber_formulation(&package,"transient","Full",crate::solver_common::C::default(),true).is_ok());
        assert!(validate_study_wavenumber_formulation(&package,"transient","Full",crate::solver_common::C{re:2.,im:0.},false).is_ok());
        let enabled_zero=validate_study_wavenumber_formulation(&package,"transient","Ez",crate::solver_common::C::default(),true).unwrap_err();
        assert!(enabled_zero.contains("requires the Full formulation"),"{enabled_zero}");
        let error=validate_study_wavenumber_formulation(&package,"transient","Ez",crate::solver_common::C{re:2.,im:0.},false).unwrap_err();
        assert!(error.contains("requires the Full formulation"),"{error}");
        assert!(validate_study_wavenumber_formulation(&json!({}),"transient","Full",crate::solver_common::C::default(),true).is_err());
    }

    #[test]
    fn em2d_transient_and_temporal_eigenfrequency_require_full_when_kz_is_selected() {
        let package:Value=serde_json::from_str(include_str!("scenarios/em2d.json")).unwrap();
        for study_type in ["transient","eigenfrequency"] {
            assert!(validate_study_wavenumber_formulation(&package,study_type,"Ez",crate::solver_common::C::default(),false).is_ok());
            assert!(validate_study_wavenumber_formulation(&package,study_type,"Full",crate::solver_common::C::default(),true).is_ok());
            assert!(validate_study_wavenumber_formulation(&package,study_type,"Full",crate::solver_common::C{re:3.,im:0.},true).is_ok());
            let zero_error=validate_study_wavenumber_formulation(&package,study_type,"Hz",crate::solver_common::C::default(),true).unwrap_err();
            assert!(zero_error.contains("requires the Full formulation"),"{study_type}: {zero_error}");
            let error=validate_study_wavenumber_formulation(&package,study_type,"Hz",crate::solver_common::C{re:3.,im:0.},false).unwrap_err();
            assert!(error.contains("requires the Full formulation"),"{study_type}: {error}");
        }
    }

    #[test]
    fn em2d_harmonic_preparation_gates_the_selected_kz_route_for_scalar_formulations() {
        let quantity=|expression:&str,unit:&str|json!({"expression":expression,"unit":unit});
        let study=json!({"id":"harmonic","type":"frequencyDomain",
            "longitudinalWavenumber":{"real":quantity("2","rad/m"),"imaginary":quantity("0","rad/m")}});
        let physics=json!({"id":"wave","frequency":quantity("1e8","Hz")});
        let model=json!({"scenario":{"profileId":"em-2d"},"model":{"parameters":[],"study":[study.clone()]}});
        for formulation in ["Ez","Hz"] {
            let result=frequency_cases(&study,&physics,&model,"em-2d",formulation);
            let error=match result {Err(error)=>error,Ok(_)=>panic!("{formulation} unexpectedly prepared with nonzero kz")};
            assert!(error.contains("requires the Full formulation"),"{formulation}: {error}");
        }
        assert_eq!(frequency_cases(&study,&physics,&model,"em-2d","Full").unwrap().len(),1);
        let mut zero_study=study;
        zero_study["longitudinalWavenumber"]["real"]=quantity("0","rad/m");
        let mut zero_model=json!({"scenario":{"profileId":"em-2d"},"model":{"parameters":[],"study":[zero_study.clone()]}});
        for formulation in ["Ez","Hz","Full"] {
            assert_eq!(frequency_cases(&zero_study,&physics,&zero_model,"em-2d",formulation).unwrap().len(),1);
        }
        zero_study["useLongitudinalWavenumber"]=json!(true);
        zero_model["model"]["study"][0]["useLongitudinalWavenumber"]=json!(true);
        for formulation in ["Ez","Hz"] {
            let error=match frequency_cases(&zero_study,&physics,&zero_model,"em-2d",formulation) {
                Err(error)=>error,
                Ok(_)=>panic!("{formulation} unexpectedly prepared with explicitly selected kz=0"),
            };
            assert!(error.contains("requires the Full formulation"),"{formulation} at explicitly selected kz=0: {error}");
        }
        assert_eq!(frequency_cases(&zero_study,&physics,&zero_model,"em-2d","Full").unwrap().len(),1);
    }

    #[test]
    fn transient_case_serialization_keeps_time_and_dimensioned_parameters() {
        let case=PreparedTransientCase {grid:crate::transient_request::TimeGrid {start:0.,end:2.,max_step:0.25,max_steps:10,output_times:vec![0.,2.]},
            symbols:HashMap::from([("length".into(),Scalar {value:3.,dimension:[1.,0.,0.,0.,0.]})]),parameters:vec![json!({"symbol":"length"})],
            longitudinal_wavenumber:crate::solver_common::C::default()};
        let result=serialize_transient_cases(&[case]);
        assert_eq!(result[0]["time"]["endSeconds"],2.);
        assert_eq!(result[0]["time"]["outputTimesSeconds"],json!([0.,2.]));
        assert_eq!(result[0]["globalParameters"]["length"],3.);
        assert_eq!(result[0]["globalParameterDimensions"]["length"],json!([1.,0.,0.,0.,0.]));
    }
    #[test]
    fn transient_preparation_emits_a_case_without_harmonic_frequency() {
        let mut model:Value=serde_json::from_str(include_str!("../../examples/acoustic_room_2d/fem_model.json")).unwrap();
        let study=&mut model["model"]["study"][0];
        study["type"]=json!("transient");
        study["time"]=json!({"startTime":{"expression":"0","unit":"s"},"endTime":{"expression":"1","unit":"s"},
            "maxStep":{"expression":"0.25","unit":"s"},"maxSteps":8,"outputTimes":[{"expression":"1","unit":"s"}]});
        let mut capabilities=crate::model_schema::engine_capabilities().unwrap();
        capabilities["study"]["preparationByStudyTypeByProfile"]["acoustic-2d"]=json!({"transient":{"kind":"transientPlan"}});
        let result=prepare_validated_study(&json!({"model":model,"studyId":"study"}),&capabilities).unwrap();
        assert_eq!(result["schema"],"fullmeta-transient-study-plan/1");
        assert_eq!(result["cases"][0]["time"]["maxStepSeconds"],0.25);
        assert!(result.get("frequencyCases").is_none());
        let grid=crate::transient_request::TimeGrid::from_prepared(&result["cases"][0]["time"]).unwrap();
        assert_eq!((grid.start,grid.end,grid.max_step),(0.,1.,0.25));
        let case=crate::transient_request::PreparedTransientCase::from_json(&result["cases"][0]).unwrap();
        assert_eq!(case.grid.end,1.);
        assert_eq!(case.parameters.len(),case.overrides.len());
    }
    #[test]
    fn eigenfrequency_preparation_uses_direct_s_shift_and_omits_excitation_inputs(){
        let mut model:Value=serde_json::from_str(include_str!("../../examples/acoustic_room_2d/fem_model.json")).unwrap();
        let study=&mut model["model"]["study"][0];study["type"]=json!("eigenfrequency");
        study["eigenfrequency"]=json!({"modeCount":2,"krylovDimension":12,"residualTolerance":1e-8,
            "shift":{"real":{"expression":"-1","unit":"Hz"},"imaginary":{"expression":"2*pi","unit":"Hz"}}});
        model["model"]["physics"]["impedance_boundary"]["incidentField"]["expression"]=json!("t+freq");
        model["model"]["physics"]["impedance_boundary"]["polarization"][0]["expression"]=json!("t+freq");
        model["model"]["physics"]["volume_source"]=json!({"id":"volume_source","type":"domainSource","values":[{"expression":"t+freq","unit":"1"}]});
        let mut capabilities=crate::model_schema::engine_capabilities().unwrap();
        capabilities["study"]["preparationByStudyTypeByProfile"]["acoustic-2d"]["eigenfrequency"]=json!({"kind":"eigenfrequencyPlan"});
        let prepared=prepare_validated_study(&json!({"model":model.clone(),"studyId":"study"}),&capabilities).unwrap();
        assert_eq!(prepared["schema"],"fullmeta-eigenfrequency-study-plan/1");
        assert_eq!(prepared["studyType"],"eigenfrequency");
        assert_eq!(prepared["solverSettings"]["modeCount"],2);
        assert_eq!(prepared["cases"][0]["shift"]["realPerSecond"],-1.);
        assert!((prepared["cases"][0]["shift"]["imaginaryPerSecond"].as_f64().unwrap()-2.*std::f64::consts::PI).abs()<1e-12);
        assert!(prepared["cases"][0].get("frequencyHz").is_none());
        for expression in ["1+t","1+freq","1+omega"] {
            let mut varying=model.clone();
            varying["model"]["materials"][0]["density"]["expression"]=json!(expression);
            let error=prepare_validated_study(&json!({"model":varying,"studyId":"study"}),&capabilities).unwrap_err();
            let expected=if expression.contains('t') {"uses time t"} else {"independent of harmonic frequency"};
            assert!(error.contains(expected),"{expression}: {error}");
        }
        let mut varying_boundary=model.clone();
        varying_boundary["model"]["physics"]["impedance_boundary"]["impedance"]["expression"]=json!("Z_air*(1+t)");
        let error=prepare_validated_study(&json!({"model":varying_boundary,"studyId":"study"}),&capabilities).unwrap_err();
        assert!(error.contains("must be time-independent and frequency-independent for an eigenfrequency Study"),"{error}");
    }
    #[test]
    fn parameter_cases_eigenfrequency_preparation_resolves_each_sweep_case(){
        let mut model:Value=serde_json::from_str(include_str!("../../examples/acoustic_room_2d/fem_model.json")).unwrap();
        model["model"]["parameters"].as_array_mut().unwrap().push(json!({
            "id":"modalScale","name":"Modal scale","symbol":"modalScale","expression":"1","unit":"1"
        }));
        let study=&mut model["model"]["study"][0];study["type"]=json!("eigenfrequency");
        study["eigenfrequency"]=json!({"modeCount":1,"krylovDimension":12,"residualTolerance":1e-8,
            "shift":{"real":{"expression":"-1","unit":"Hz"},"imaginary":{"expression":"0","unit":"Hz"}}});
        study["parameterSweep"]=json!({"type":"parameterSweep","parameters":[{"parameter":"modalScale","values":{
            "type":"linear","count":2,"start":{"expression":"1","unit":"1"},"end":{"expression":"2","unit":"1"}}}]});
        let mut capabilities=crate::model_schema::engine_capabilities().unwrap();
        capabilities["study"]["preparationByStudyTypeByProfile"]["acoustic-2d"]["eigenfrequency"]=json!({"kind":"eigenfrequencyPlan"});
        let prepared=prepare_validated_study(&json!({"model":model,"studyId":"study"}),&capabilities).unwrap();
        let cases=prepared["cases"].as_array().unwrap();assert_eq!(cases.len(),2);
        for (index,value) in [1.,2.].into_iter().enumerate(){
            assert_eq!(cases[index]["globalParameters"]["modalScale"],json!(value));
            assert_eq!(cases[index]["parameterOverrides"].as_array().unwrap().last().unwrap()["expression"],json!(value.to_string()));
            assert_eq!(cases[index]["shift"]["realPerSecond"],json!(-1.));
        }
    }
    #[test]
    fn propagation_mode_preparation_declares_fixed_frequency_and_direct_kz_target(){
        let mut model:Value=serde_json::from_str(include_str!("../../examples/acoustic_room_2d/fem_model.json")).unwrap();
        let study=&mut model["model"]["study"][0];study["type"]=json!("propagationMode");
        study["propagationMode"]=json!({"frequency":{"expression":"2","unit":"kHz"},"modeCount":3,
            "krylovDimension":12,"residualTolerance":1e-8,"shift":{
                "real":{"expression":"4","unit":"rad/m"},"imaginary":{"expression":"-0.2","unit":"rad/m"}}});
        let capabilities=crate::model_schema::engine_capabilities().unwrap();
        let prepared=prepare_validated_study(&json!({"model":model,"studyId":"study"}),&capabilities).unwrap();
        assert_eq!(prepared["schema"],"fullmeta-propagation-mode-study-plan/1");
        assert_eq!(prepared["studyType"],"propagationMode");
        assert_eq!(prepared["solverSettings"]["modeCount"],3);
        assert_eq!(prepared["cases"][0]["frequencyHz"],2_000.);
        assert_eq!(prepared["cases"][0]["shift"]["realRadPerMetre"],4.);
        assert_eq!(prepared["cases"][0]["shift"]["imaginaryRadPerMetre"],-0.2);
        let mut invalid=model;
        invalid["model"]["study"][0]["propagationMode"]["shift"]["real"]["unit"]=json!("Hz");
        let error=prepare_validated_study(&json!({"model":invalid,"studyId":"study"}),&capabilities).unwrap_err();
        assert!(error.contains("inverse-length units"),"{error}");
    }
    #[test]
    fn transient_preparation_recalculates_time_grid_for_each_sweep_case() {
        let mut model:Value=serde_json::from_str(include_str!("../../examples/acoustic_room_2d/fem_model.json")).unwrap();
        model["model"]["parameters"].as_array_mut().unwrap().push(json!({"id":"duration","name":"duration","symbol":"duration","expression":"1","unit":"s"}));
        let study=&mut model["model"]["study"][0];
        study["type"]=json!("transient");
        study["time"]=json!({"startTime":{"expression":"0","unit":"s"},"endTime":{"expression":"duration","unit":"s"},
            "maxStep":{"expression":"duration/4","unit":"s"},"maxSteps":8,"outputTimes":[{"expression":"duration","unit":"s"}]});
        study["parameterSweep"]=json!({"type":"parameterSweep","parameters":[{"parameter":"duration","values":{
            "type":"linear","count":2,"start":{"expression":"1","unit":"s"},"end":{"expression":"2","unit":"s"}}}]});
        let mut capabilities=crate::model_schema::engine_capabilities().unwrap();
        capabilities["study"]["preparationByStudyTypeByProfile"]["acoustic-2d"]=json!({"transient":{"kind":"transientPlan"}});
        let result=prepare_validated_study(&json!({"model":model,"studyId":"study"}),&capabilities).unwrap();
        let cases=result["cases"].as_array().unwrap();
        assert_eq!(cases.len(),2);
        for (index,end) in [1.,2.].into_iter().enumerate() {
            let case=crate::transient_request::PreparedTransientCase::from_json(&cases[index]).unwrap();
            assert_eq!((case.grid.end,case.grid.max_step),(end,end/4.));
            assert_eq!(case.parameters["duration"].value,end);
        }
    }
    #[test]
    fn harmonic_preparation_rejects_time_dependent_materials_explicitly() {
        let mut model:Value=serde_json::from_str(include_str!("../../examples/acoustic_room_2d/fem_model.json")).unwrap();
        model["model"]["materials"][0]["density"]["expression"]=json!("rho_air*(1+t/tau)");
        let capabilities=crate::model_schema::engine_capabilities().unwrap();
        let error=prepare_validated_study(&json!({"model":model,"studyId":"study"}),&capabilities).unwrap_err();
        assert!(error.contains("uses time t")&&error.contains("requires a transient Study"));
    }
    #[test]
    fn transient_only_material_property_does_not_block_stationary_preparation() {
        let mut model:Value=serde_json::from_str(include_str!("../../examples/thermal_square_insulated/fem_model.json")).unwrap();
        model["model"]["materials"][0]["volumetricHeatCapacity"]=json!({"expression":"1+t","unit":"W*s/(m^3*K)"});
        assert!(reject_time_dependent_materials(&model,"thermal-2d","stationary").is_ok());
    }
}
