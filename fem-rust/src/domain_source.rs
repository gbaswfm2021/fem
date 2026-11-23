//! Preparation of declared, additive domain source expressions.
//!
//! Selection binding is performed by the common selection plan. This module
//! compiles each selected feature once, then samples it in physical coordinates.
use std::collections::HashMap;
use serde_json::Value;
use crate::boundary_input::{Number, Quantity, Sampling};
use crate::model_bindings::BoundSelection;
use crate::model_expression::{self, Scalar};
use crate::{C, c_add, c_mul};

/// The scenario owns the physical quantity and its mapping into the weak RHS.
pub(crate) struct DomainSourcePlan {
    pub(crate) form: crate::weak_form::VolumeLoadPlan,
    input: Value,
    coefficient: crate::coefficient_expr::CoefficientExpressions,
    source_component_indices: Option<Vec<usize>>,
}

impl DomainSourcePlan {
    pub(crate) fn from_json(value: &Value, dimension: usize, components: usize) -> Result<Self, String> {
        let form = crate::weak_form::VolumeLoadPlan::from_json(value.get("form").ok_or("Domain source form is required.")?)?;
        form.require_shape(dimension, components)?;
        let input = value.get("input").filter(|v| v.is_object()).ok_or("Domain source input declaration is required.")?.clone();
        let count = input.get("components").and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()).ok_or("Domain source input components must be an integer.")?;
        let source_component_indices=input.get("sourceComponentIndices").map(|value| -> Result<Vec<usize>,String>{
            let indices=value.as_array().filter(|items|!items.is_empty()).ok_or("Domain source sourceComponentIndices must be a nonempty array.")?
                .iter().map(|value|value.as_u64().and_then(|n|usize::try_from(n).ok()).filter(|index|*index<count)
                    .ok_or("Domain source sourceComponentIndices contains an out-of-range index.")).collect::<Result<Vec<_>,_>>()?;
            if indices.len()!=form.source_components()? {return Err("Domain source sourceComponentIndices must match the load test component count.".into());}
            if indices.iter().any(|index|indices.iter().filter(|other|*other==index).count()>1){return Err("Domain source sourceComponentIndices cannot repeat components.".into());}
            Ok(indices)
        }).transpose()?;
        if source_component_indices.is_none()&&count != form.source_components()? { return Err("Domain source components do not match the test operator.".into()); }
        let legacy_counts = input.get("legacyComponentCounts").map(|values| {
            values.as_array().ok_or("Domain source legacyComponentCounts must be an array.")?.iter().map(|value| {
                value.as_u64().and_then(|n|usize::try_from(n).ok()).filter(|n|*n>0&&*n<count)
                    .ok_or("Domain source legacy component counts must be positive and smaller than components.")
            }).collect::<Result<Vec<_>,_>>()
        }).transpose()?.unwrap_or_default();
        validate_legacy_layouts(&input, count)?;
        if legacy_counts.iter().any(|n| legacy_counts.iter().filter(|m| *m==n).count()>1) {
            return Err("Domain source legacyComponentCounts cannot contain duplicates.".into());
        }
        if !legacy_counts.is_empty() {
            let minimum = *legacy_counts.iter().min().unwrap();
            let defaults = input.get("missingComponentDefaults").and_then(Value::as_array)
                .filter(|values| values.len()==count-minimum)
                .ok_or("Domain source missingComponentDefaults must fill every component omitted by its shortest legacy shape.")?;
            let unit=input.get("unit").and_then(Value::as_str).ok_or("Domain source input unit is required.")?;
            for (index,value) in defaults.iter().enumerate() {
                let path=format!("Domain source missingComponentDefaults[{index}]");
                if value.get("unit").and_then(Value::as_str)!=Some(unit)
                    || !value.get("expression").and_then(Value::as_str).is_some_and(|expression|!expression.trim().is_empty()) {
                    return Err(format!("{path} must declare a nonempty expression in the domain-source unit {unit:?}."));
                }
            }
        }
        if !input.get("path").and_then(Value::as_str).is_some_and(|p| p.starts_with('/') && p.len() > 1) {
            return Err("Domain source input path must be a nonempty JSON pointer.".into());
        }
        if !input.get("unit").and_then(Value::as_str).is_some_and(|unit| !unit.is_empty()) {
            return Err("Domain source input unit must be a nonempty string.".into());
        }
        if !matches!(input.get("number").and_then(Value::as_str), Some("real" | "complex")) {
            return Err("Domain source input number must be real or complex.".into());
        }
        let expression = value.get("coefficient").ok_or("Domain source coefficient is required.")?;
        let coefficient = crate::coefficient_expr::CoefficientExpressions::from_json(&serde_json::json!({"load": expression}))?;
        Ok(Self { form, input, coefficient, source_component_indices })
    }

    pub(crate) fn bind(&self, rows: &[BoundSelection<'_>]) -> Result<DomainSources, String> {
        DomainSources::compile(&self.input, rows)
    }

    pub(crate) fn scale(&self, values: &mut [C], symbols: &HashMap<String, C>) -> Result<(), String> {
        let input_count=self.input.get("components").and_then(Value::as_u64).and_then(|count|usize::try_from(count).ok())
            .ok_or("Domain source input components must be an integer.")?;
        if values.len()!=input_count{return Err("Domain source sample does not match its declared input component count.".into());}
        let coefficients = self.coefficient.evaluate(symbols)?;
        let factor = *coefficients.get("load").ok_or("Domain source coefficient did not evaluate.")?;
        let mut result = Vec::with_capacity(values.len());
        for &value in values.iter() {
            let product = c_mul(value, factor);
            if !product.re.is_finite() || !product.im.is_finite() { return Err("Domain source coefficient produced a non-finite load.".into()); }
            result.push(product);
        }
        values.copy_from_slice(&result);
        Ok(())
    }

    fn project(&self,values:Vec<C>)->Result<Vec<C>,String>{
        if let Some(indices)=&self.source_component_indices {
            if values.len()!=self.input.get("components").and_then(Value::as_u64).and_then(|count|usize::try_from(count).ok()).unwrap_or(0){
                return Err("Domain source sample does not match its declared input component count.".into());
            }
            Ok(indices.iter().map(|&index|values[index]).collect())
        }else if values.len()==self.form.source_components()?{Ok(values)}
        else{Err("Domain source sample does not match its weak-load component count.".into())}
    }
}

/// Leave the no-source path empty so no expression compilation or sampling
/// is needed. The selection plan provides all active rows, including overlaps.
pub(crate) fn prepare<'a>(plan: Option<&'a DomainSourcePlan>,
    selections: Option<&[BoundSelection<'_>]>) -> Result<Option<(&'a DomainSourcePlan, DomainSources)>, String> {
    let rows = selections.unwrap_or(&[]);
    if rows.is_empty() { return Ok(None); }
    let plan = plan.ok_or("Active domain sources require a scenario domain source declaration.")?;
    let sources = plan.bind(rows)?;
    Ok(Some((plan, sources)))
}

/// Bind every declared model feature to its JSON-routed weak load. Feature
/// types identify routes; the assembler remains unaware of their physical
/// names and simply accumulates each declared test operator.
pub(crate) fn prepare_routes<'a>(
    plans: &'a HashMap<String, DomainSourcePlan>,
    selections: Option<&[BoundSelection<'_>]>,
) -> Result<Vec<(String, &'a DomainSourcePlan, DomainSources)>, String> {
    let rows = selections.unwrap_or(&[]);
    if rows.is_empty() { return Ok(Vec::new()); }

    let mut active = std::collections::BTreeSet::new();
    for row in rows {
        let kind = row.row.get("type").and_then(Value::as_str)
            .filter(|kind| !kind.is_empty()).unwrap_or("domainSource");
        if !plans.contains_key(kind) {
            return Err(format!("Active domain source type {kind:?} has no scenario load route."));
        }
        active.insert(kind.to_owned());
    }

    active.into_iter().map(|kind| {
        let plan = plans.get(&kind).ok_or_else(|| format!("Domain source route {kind:?} disappeared."))?;
        let matching = rows.iter().filter(|row| row.row.get("type").and_then(Value::as_str).unwrap_or("domainSource") == kind.as_str())
            .map(|row| BoundSelection { tag: row.tag, selection_id: row.selection_id, row: row.row })
            .collect::<Vec<_>>();
        let sources = plan.bind(&matching)?;
        Ok((kind, plan, sources))
    }).collect()
}

/// Form coefficients may depend on already evaluated material values in
/// addition to frequency and parameters. These are supplied by the caller.
pub(crate) fn sample_load(plan: &DomainSourcePlan, sources: &DomainSources, tag: usize,
    point: [f64; 3], parameters: &HashMap<String, Scalar>, frequency: Option<f64>,
    coefficient_symbols: &HashMap<String, C>) -> Result<Vec<C>, String> {
    let mut values = sources.sample(tag, point, parameters, frequency)?;
    plan.scale(&mut values, coefficient_symbols)?;
    plan.project(values)
}

/// Evaluate a declared transient source and its generic weak-load coefficient.
/// The caller supplies coefficient bindings derived for the current time stage.
pub(crate) fn sample_load_at_time(plan: &DomainSourcePlan, sources: &DomainSources, tag: usize,
    point: [f64; 3], parameters: &HashMap<String, Scalar>, time: f64,
    coefficient_symbols: &HashMap<String, C>) -> Result<Vec<C>, String> {
    let mut values=sources.sample_at_time(tag,point,parameters,time)?;
    plan.scale(&mut values,coefficient_symbols)?;
    plan.project(values)
}

struct Source {
    label: String,
    components: Vec<Quantity>,
}

pub(crate) struct DomainSources {
    components: usize,
    sources: Vec<Source>,
    by_tag: HashMap<usize, Vec<usize>>,
}

impl DomainSources {
    /// Contract fields describe project-row location, dimensions and numeric
    /// representation. A repeated tag keeps every feature in input order.
    pub(crate) fn compile(contract: &Value, rows: &[BoundSelection<'_>]) -> Result<Self, String> {
        Self::compile_membership(contract, rows.iter().map(|row| (row.tag, row.row)))
    }

    /// Compile each source once, even when it belongs to many mesh cells.
    /// Cell indices form a separate lookup universe from material tags.
    pub(crate) fn compile_cells(contract: &Value, cells: &[Vec<&Value>]) -> Result<Self, String> {
        Self::compile_membership(contract, cells.iter().enumerate()
            .flat_map(|(cell, rows)| rows.iter().map(move |row| (cell, *row))))
    }

    fn compile_membership<'a>(contract: &Value, rows: impl IntoIterator<Item = (usize, &'a Value)>) -> Result<Self, String> {
        let count = contract.get("components").and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok()).filter(|n| (1..=9).contains(n))
            .ok_or("Domain source components must be an integer from 1 to 9.")?;
        let path = contract.get("path").and_then(Value::as_str)
            .filter(|p| p.starts_with('/')).ok_or("Domain source path must be a JSON pointer.")?;
        let unit = contract.get("unit").and_then(Value::as_str)
            .ok_or("Domain source unit is required.")?;
        let number = match contract.get("number").and_then(Value::as_str) {
            Some("real") => Number::Real,
            Some("complex") => Number::Complex,
            _ => return Err("Domain source number must be real or complex.".into()),
        };
        let layouts = legacy_layouts(contract, count)?;
        let layout_default = if layouts.is_empty() { None } else {
            let default = contract.get("missingComponentDefault").ok_or("Domain source legacy layouts require missingComponentDefault.")?;
            Some(Quantity::compile(default, unit, number, Sampling::Point, "domain source missingComponentDefault")?)
        };
        let legacy_counts=contract.get("legacyComponentCounts").and_then(Value::as_array).into_iter().flatten()
            .map(|value|value.as_u64().and_then(|n|usize::try_from(n).ok()).ok_or("Domain source legacy component count must be an integer."))
            .collect::<Result<Vec<_>,_>>()?;
        if legacy_counts.iter().any(|n|*n==0||*n>=count)||legacy_counts.iter().any(|n|legacy_counts.iter().filter(|m|*m==n).count()>1){
            return Err("Domain source legacy component counts must be unique positive counts smaller than components.".into());
        }
        let minimum_legacy=legacy_counts.iter().copied().min().unwrap_or(count);
        let defaults=contract.get("missingComponentDefaults").and_then(Value::as_array).into_iter().flatten().enumerate()
            .map(|(index,value)|Quantity::compile(value,unit,number,Sampling::Point,&format!("domain source missingComponentDefaults[{index}]")))
            .collect::<Result<Vec<_>,_>>()?;
        if !legacy_counts.is_empty()&&defaults.len()!=count-minimum_legacy{
            return Err("Domain source missingComponentDefaults do not match the legacy component shapes.".into());
        }
        let mut by_tag: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut sources = Vec::new();
        let mut compiled = HashMap::<*const Value, usize>::new();
        for (tag, row) in rows {
            let key = row as *const Value;
            if let Some(&index) = compiled.get(&key) {
                by_tag.entry(tag).or_default().push(index);
                continue;
            }
            let label = row.get("id").and_then(Value::as_str).filter(|s| !s.is_empty())
                .ok_or("Domain source requires an id.")?.to_string();
            let raw = row.pointer(path).and_then(Value::as_array)
                .ok_or_else(|| format!("Domain source {label}: {path} must be an array."))?;
            if let Some(indices) = layouts.get(&raw.len()) {
                let default = layout_default.as_ref().ok_or("Domain source legacy default is missing.")?;
                let mut components = vec![default.clone(); count];
                for (i, value) in raw.iter().enumerate() {
                    let path=format!("domain source {label}{path}/{i}");
                    components[indices[i]] = Quantity::compile(value, unit, number, Sampling::Point, &path)
                        .map_err(|error|format!("{path}: {error}"))?;
                }
                let index = sources.len();
                sources.push(Source { label, components });
                compiled.insert(key, index);
                by_tag.entry(tag).or_default().push(index);
                continue;
            }
            let legacy=raw.len()!=count&&legacy_counts.contains(&raw.len());
            if raw.len() != count && !legacy { return Err(format!("Domain source {label} requires {count} components{}.",
                if legacy_counts.is_empty(){String::new()}else{format!(" or one of {legacy_counts:?}")})); }
            let mut components = raw.iter().enumerate().map(|(i, value)| {
                let path=format!("domain source {label}{path}/{i}");
                Quantity::compile(value, unit, number, Sampling::Point, &path)
                    .map_err(|error|format!("{path}: {error}"))
            }).collect::<Result<Vec<_>, _>>()?;
            if legacy {
                let default_offset=raw.len().saturating_sub(minimum_legacy);
                components.extend(defaults.iter().skip(default_offset).cloned());
            }
            let index = sources.len();
            sources.push(Source { label, components });
            compiled.insert(key, index);
            by_tag.entry(tag).or_default().push(index);
        }
        Ok(Self { components: count, sources, by_tag })
    }

    pub(crate) fn is_empty(&self) -> bool { self.by_tag.is_empty() }

    pub(crate) fn sample(&self, tag: usize, point: [f64; 3], parameters: &HashMap<String, Scalar>, frequency: Option<f64>) -> Result<Vec<C>, String> {
        self.sample_with_context(tag, point, parameters, frequency, None)
    }

    /// Sample a transient source at a physical time in seconds.
    pub(crate) fn sample_at_time(&self, tag: usize, point: [f64; 3], parameters: &HashMap<String, Scalar>, time: f64) -> Result<Vec<C>, String> {
        self.sample_with_context(tag, point, parameters, None, Some(time))
    }

    fn sample_with_context(&self, tag: usize, point: [f64; 3], parameters: &HashMap<String, Scalar>, frequency: Option<f64>, time: Option<f64>) -> Result<Vec<C>, String> {
        let mut result = vec![C::default(); self.components];
        let Some(sources) = self.by_tag.get(&tag) else { return Ok(result); };
        let mut symbols = parameters.clone();
        for (axis, value) in ["x", "y", "z"].into_iter().zip(point) {
            if !value.is_finite() { return Err(format!("Domain source coordinate {axis} is not finite.")); }
            symbols.insert(axis.into(), Scalar { value, dimension: [1., 0., 0., 0., 0.] });
        }
        if let Some(frequency) = frequency {
            if !frequency.is_finite() { return Err("Domain source frequency must be finite.".into()); }
            let dimension = model_expression::unit("Hz")?.1;
            for (name, value) in [("freq", frequency), ("frequency", frequency), ("omega", std::f64::consts::TAU * frequency)] {
                symbols.insert(name.into(), Scalar { value, dimension });
            }
        }
        if let Some(time) = time {
            if !time.is_finite() { return Err("Domain source time must be finite.".into()); }
            if parameters.contains_key("t") { return Err("Parameter symbol t conflicts with transient time.".into()); }
            symbols.insert("t".into(), Scalar { value: time, dimension: model_expression::unit("s")?.1 });
        }
        for &index in sources {
            let source = &self.sources[index];
            for (i, quantity) in source.components.iter().enumerate() {
                let value = quantity.evaluate(&symbols, &format!("domain source {} component {i}", source.label))?;
                result[i] = c_add(result[i], value);
                if !result[i].re.is_finite() || !result[i].im.is_finite() { return Err(format!("Domain source {} sum is non-finite.", source.label)); }
            }
        }
        Ok(result)
    }
}

fn legacy_layouts(contract:&Value,total:usize)->Result<HashMap<usize,Vec<usize>>,String>{
    let Some(entries)=contract.get("legacyComponentLayouts") else{return Ok(HashMap::new());};
    let entries=entries.as_array().ok_or("Domain source legacyComponentLayouts must be an array.")?;
    let mut layouts=HashMap::new();
    for (index,entry) in entries.iter().enumerate(){
        let count=entry.get("components").and_then(Value::as_u64).and_then(|value|usize::try_from(value).ok())
            .filter(|value|*value>0&&*value<total).ok_or_else(||format!("Domain source legacyComponentLayouts[{index}].components is invalid."))?;
        let indices=entry.get("targetIndices").and_then(Value::as_array).filter(|values|values.len()==count)
            .ok_or_else(||format!("Domain source legacyComponentLayouts[{index}].targetIndices must match its component count."))?;
        let indices=indices.iter().map(|value|value.as_u64().and_then(|value|usize::try_from(value).ok())
            .filter(|value|*value<total).ok_or_else(||format!("Domain source legacyComponentLayouts[{index}] has an invalid target index.")))
            .collect::<Result<Vec<_>,_>>()?;
        let mut unique=std::collections::HashSet::new();
        if indices.iter().any(|value|!unique.insert(*value)){return Err(format!("Domain source legacyComponentLayouts[{index}] repeats a target index."));}
        if layouts.insert(count,indices).is_some(){return Err("Domain source legacy component layouts cannot repeat a component count.".into());}
    }
    Ok(layouts)
}

fn validate_legacy_layouts(input:&Value,total:usize)->Result<(),String>{
    let layouts=legacy_layouts(input,total)?;
    if !layouts.is_empty(){
        if !input.get("missingComponentDefault").is_some_and(Value::is_object){return Err("Domain source legacy layouts require missingComponentDefault.".into());}
        let unit=input.get("unit").and_then(Value::as_str).ok_or("Domain source input unit is required.")?;
        let number=if input.get("number").and_then(Value::as_str)==Some("complex"){Number::Complex}else{Number::Real};
        Quantity::compile(&input["missingComponentDefault"],unit,number,Sampling::Point,"Domain source missingComponentDefault")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transient_source_samples_dimensioned_time_and_rejects_shadowing() {
        let row = json!({"id":"source", "values":[{"expression":"t/tau", "unit":"1"}]});
        let sources = DomainSources::compile_cells(&json!({"components":1,"path":"/values","unit":"1","number":"real"}), &[vec![&row]]).unwrap();
        let params = HashMap::from([("tau".to_string(), Scalar { value: 2.0, dimension: model_expression::unit("s").unwrap().1 })]);
        let sampled = sources.sample_at_time(0, [0.0; 3], &params, 3.0).unwrap();
        assert!((sampled[0].re - 1.5).abs() < 1e-12);
        let plan=DomainSourcePlan::from_json(&json!({"form":{"dimension":2,"fieldComponents":1,"test":"value"},
            "input":{"components":1,"path":"/values","unit":"1","number":"real"},"coefficient":2}),2,1).unwrap();
        let load=sample_load_at_time(&plan,&sources,0,[0.;3],&params,3.,&HashMap::new()).unwrap();
        assert!((load[0].re-3.).abs()<1e-12);
        let mut shadowed = params;
        shadowed.insert("t".into(), Scalar { value: 0.0, dimension: model_expression::unit("s").unwrap().1 });
        assert!(sources.sample_at_time(0, [0.0; 3], &shadowed, 1.0).unwrap_err().contains("conflicts"));
    }

    #[test]
    fn cell_membership_compiles_each_source_once_and_adds_overlaps() {
        let a = json!({"id":"a", "values":[{"expression":"2", "unit":"1"}]});
        let b = json!({"id":"b", "values":[{"expression":"3", "unit":"1"}]});
        let cells = vec![vec![&a], vec![&a, &b], vec![&b], vec![]];
        let sources = DomainSources::compile_cells(&json!({"components":1,"path":"/values","unit":"1","number":"real"}), &cells).unwrap();
        assert_eq!(sources.sources.len(), 2);
        let values = (0..4).map(|cell| sources.sample(cell, [0.; 3], &HashMap::new(), None).unwrap()[0].re).collect::<Vec<_>>();
        assert_eq!(values, [2., 5., 3., 0.]);
    }

    #[test]
    fn selected_sources_add_and_unselected_tags_are_zero() {
        let a = json!({"id":"a", "values":[{"expression":"2", "unit":"1"}]});
        let b = json!({"id":"b", "values":[{"expression":"3", "unit":"1"}]});
        let rows = [BoundSelection { tag: 7, selection_id: "domain", row: &a },
            BoundSelection { tag: 7, selection_id: "domain", row: &b }];
        let sources = DomainSources::compile(&json!({"components":1,"path":"/values","unit":"1","number":"real"}), &rows).unwrap();
        assert!(!sources.is_empty());
        let value = sources.sample(7, [0.; 3], &HashMap::new(), None).unwrap();
        assert_eq!((value[0].re, value[0].im), (5., 0.));
        let other = sources.sample(8, [0.; 3], &HashMap::new(), None).unwrap();
        assert_eq!((other[0].re, other[0].im), (0., 0.));
    }

    #[test]
    fn declared_legacy_vector_shape_gets_only_its_explicit_missing_component_default() {
        let route=json!({"form":{"dimension":2,"operatorDimension":3,"fieldComponents":1,"test":"gradient"},
            "input":{"components":3,"legacyComponentCounts":[2],"missingComponentDefaults":[{"expression":"0","unit":"1"}],
                "path":"/values","unit":"1","number":"complex"},"coefficient":1});
        let plan=DomainSourcePlan::from_json(&route,2,1).unwrap();
        let row=json!({"id":"force","values":[{"expression":"2","unit":"1"},{"expression":"3","unit":"1"}]});
        let rows=[BoundSelection {tag:1,selection_id:"domain",row:&row}];
        let sources=plan.bind(&rows).unwrap();
        let values=sources.sample(1,[0.;3],&HashMap::new(),None).unwrap();
        assert_eq!(values,vec![C{re:2.,im:0.},C{re:3.,im:0.},C::default()]);
    }

    #[test]
    fn declared_legacy_tensor_layout_maps_in_plane_eigenstress_into_three_dimensions() {
        let route=json!({"form":{"dimension":2,"operatorDimension":3,"fieldComponents":3,"test":"symmetricGradient"},
            "input":{"components":9,"legacyComponentLayouts":[{"components":4,"targetIndices":[0,1,3,4]}],
                "missingComponentDefault":{"expression":"0","unit":"Pa"},"path":"/values","unit":"Pa","number":"complex"},"coefficient":1});
        let plan=DomainSourcePlan::from_json(&route,2,3).unwrap();
        let row=json!({"id":"eigenstress","values":[{"expression":"11","unit":"Pa"},{"expression":"12","unit":"Pa"},{"expression":"21","unit":"Pa"},{"expression":"22","unit":"Pa"}]});
        let sources=plan.bind(&[BoundSelection {tag:4,selection_id:"domain",row:&row}]).unwrap();
        let values=sources.sample(4,[0.;3],&HashMap::new(),None).unwrap();
        assert_eq!(values[0].re,11.); assert_eq!(values[1].re,12.);
        assert_eq!(values[3].re,21.); assert_eq!(values[4].re,22.);
        assert_eq!(values.iter().enumerate().filter(|(index,_)|![0,1,3,4].contains(index)).map(|(_,value)|value.re).sum::<f64>(),0.);
    }

    #[test]
    fn declared_coefficient_maps_input_to_complex_load() {
        let declaration = json!({"form":{"dimension":3,"fieldComponents":3,"test":"value"},
            "input":{"components":3,"path":"/values","unit":"1","number":"complex"},
            "coefficient":{"complex":[0,{"multiply":[-1,{"symbol":"omega"}]}]}});
        let plan = DomainSourcePlan::from_json(&declaration, 3, 3).unwrap();
        let mut values = [C { re: 2., im: 1. }; 3];
        plan.scale(&mut values, &HashMap::from([("omega".into(), C { re: 3., im: 0. })])).unwrap();
        assert!(values.iter().all(|value| (value.re, value.im) == (3., -6.)));
        assert!(DomainSourcePlan::from_json(&declaration, 2, 2).is_err());
    }

    

    
}
