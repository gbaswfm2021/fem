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
}

impl DomainSourcePlan {
    pub(crate) fn from_json(value: &Value, dimension: usize, components: usize) -> Result<Self, String> {
        let form = crate::weak_form::VolumeLoadPlan::from_json(value.get("form").ok_or("Domain source form is required.")?)?;
        form.require_shape(dimension, components)?;
        let input = value.get("input").filter(|v| v.is_object()).ok_or("Domain source input declaration is required.")?.clone();
        let count = input.get("components").and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()).ok_or("Domain source input components must be an integer.")?;
        if count != form.source_components()? { return Err("Domain source components do not match the test operator.".into()); }
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
        Ok(Self { form, input, coefficient })
    }

    pub(crate) fn bind(&self, rows: &[BoundSelection<'_>]) -> Result<DomainSources, String> {
        DomainSources::compile(&self.input, rows)
    }

    pub(crate) fn scale(&self, values: &mut [C], symbols: &HashMap<String, C>) -> Result<(), String> {
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

/// Form coefficients may depend on already evaluated material values in
/// addition to frequency and parameters. These are supplied by the caller.
pub(crate) fn sample_load(plan: &DomainSourcePlan, sources: &DomainSources, tag: usize,
    point: [f64; 3], parameters: &HashMap<String, Scalar>, frequency: Option<f64>,
    coefficient_symbols: &HashMap<String, C>) -> Result<Vec<C>, String> {
    let mut values = sources.sample(tag, point, parameters, frequency)?;
    plan.scale(&mut values, coefficient_symbols)?;
    Ok(values)
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
            .and_then(|n| usize::try_from(n).ok()).filter(|n| (1..=3).contains(n))
            .ok_or("Domain source components must be an integer from 1 to 3.")?;
        let path = contract.get("path").and_then(Value::as_str)
            .filter(|p| p.starts_with('/')).ok_or("Domain source path must be a JSON pointer.")?;
        let unit = contract.get("unit").and_then(Value::as_str)
            .ok_or("Domain source unit is required.")?;
        let number = match contract.get("number").and_then(Value::as_str) {
            Some("real") => Number::Real,
            Some("complex") => Number::Complex,
            _ => return Err("Domain source number must be real or complex.".into()),
        };
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
            if raw.len() != count { return Err(format!("Domain source {label} requires {count} components.")); }
            let components = raw.iter().enumerate().map(|(i, value)| {
                Quantity::compile(value, unit, number, Sampling::Point, &format!("domain source {label}{path}/{i}"))
            }).collect::<Result<Vec<_>, _>>()?;
            let index = sources.len();
            sources.push(Source { label, components });
            compiled.insert(key, index);
            by_tag.entry(tag).or_default().push(index);
        }
        Ok(Self { components: count, sources, by_tag })
    }

    pub(crate) fn is_empty(&self) -> bool { self.by_tag.is_empty() }

    pub(crate) fn sample(&self, tag: usize, point: [f64; 3], parameters: &HashMap<String, Scalar>, frequency: Option<f64>) -> Result<Vec<C>, String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn declared_coefficient_maps_input_to_complex_load() {
        let declaration = json!({"form":{"dimension":3,"fieldComponents":3,"test":"value"},
            "input":{"components":3,"path":"/values","unit":"1","number":"complex"},
            "coefficient":{"complex":[0,{"multiply":[-1,{"symbol":"omega"}]}]}});
        let plan = DomainSourcePlan::from_json(&declaration, 3, 3).unwrap();
        let mut values = [C { re: 2., im: 1. }];
        plan.scale(&mut values, &HashMap::from([("omega".into(), C { re: 3., im: 0. })])).unwrap();
        assert_eq!((values[0].re, values[0].im), (3., -6.));
        assert!(DomainSourcePlan::from_json(&declaration, 2, 2).is_err());
    }

    

    
}
