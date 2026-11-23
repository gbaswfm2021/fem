//! Scenario-neutral compilation of declared boundary quantities.
//!
//! A contract selects model paths, units, sampling scope, scalar/vector
//! precedence and names consumed by an essential or boundary form. The caller
//! supplies global parameters and, for point sampling, the physical position.
//! Mesh selection, normals, quadrature and sparse scattering are separate.
use crate::C;
use crate::boundary_form::BoundaryFormPlan;
use crate::model_expression::{self, Scalar};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy)]
pub(crate) enum Sampling {
    Parameters,
    Point,
}
#[derive(Clone, Copy)]
pub(crate) enum Number {
    Real,
    Complex,
}

#[derive(Clone)]
pub(crate) enum Quantity {
    Real {
        compiled: model_expression::CompiledQuantity,
        source: Value,
    },
    Complex(model_expression::CompiledComplexQuantity),
}
impl Quantity {
    pub(crate) fn compile(
        value: &Value,
        unit: &str,
        number: Number,
        sampling: Sampling,
        path: &str,
    ) -> Result<Self, String> {
        let actual = value
            .get("unit")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{path}.unit is required."))?;
        let expected_dimension = model_expression::unit(unit)?.1;
        let actual_dimension = model_expression::unit(actual)?.1;
        if actual_dimension
            .iter()
            .zip(expected_dimension)
            .any(|(a, b)| (a - b).abs() > 1e-10)
        {
            return Err(format!("{path} must use units compatible with {unit}."));
        }
        if matches!(sampling, Sampling::Parameters) {
            let expression = value
                .get("expression")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{path}.expression is required."))?;
            let (_, names, _) = model_expression::inspect(expression)?;
            if names
                .iter()
                .any(|name| matches!(name.as_str(), "x" | "y" | "z"))
            {
                return Err(format!(
                    "{path} cannot use coordinates in parameters scope."
                ));
            }
        }
        match number {
            Number::Real => Ok(Self::Real {
                compiled: model_expression::CompiledQuantity::compile(value, path)?,
                source: value.clone(),
            }),
            Number::Complex => Ok(Self::Complex(
                model_expression::CompiledComplexQuantity::compile(value, path)?,
            )),
        }
    }
    pub(crate) fn evaluate(
        &self,
        symbols: &HashMap<String, Scalar>,
        path: &str,
    ) -> Result<C, String> {
        let value = match self {
            Self::Real { compiled, .. } => C {
                re: compiled.evaluate(symbols, path)?,
                im: 0.,
            },
            Self::Complex(quantity) => {
                let v = quantity.evaluate(symbols, path)?;
                C { re: v.re, im: v.im }
            }
        };
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err(format!("{path} must evaluate to a finite value."));
        }
        Ok(value)
    }
    fn time_derivative(
        &self,
        symbols: &HashMap<String, Scalar>,
        path: &str,
    ) -> Result<Option<C>, String> {
        match self {
            Self::Real { source, .. } => Ok(crate::transient_derivative::evaluate(
                source, symbols, path,
            )?
            .map(|re| C { re, im: 0. })),
            Self::Complex(_) => Ok(None),
        }
    }
}

#[derive(Clone)]
struct NamedQuantity {
    name: String,
    quantity: Option<Quantity>,
    nonzero: bool,
    minimum_real: Option<f64>,
}
#[derive(Clone)]
struct Prescribed {
    scalar: Option<Quantity>,
    vector: Option<Vec<Quantity>>,
    zero_companion: bool,
    zero_always: bool,
    scalar_expansion: usize,
    scalar_component: Option<usize>,
    trace: bool,
    tolerance: f64,
    name: String,
}

/// Compiled once for one model row. `form_key` is determined by the declared
/// precedence and can be used to select a plan from `ScenarioPlan`.
#[derive(Clone)]
pub(crate) struct BoundaryInput {
    sampling: Sampling,
    form_key: Option<String>,
    prescribed: Option<Prescribed>,
    coefficients: Vec<NamedQuantity>,
    sources: Vec<NamedQuantity>,
}

pub(crate) struct BoundaryValues {
    pub(crate) form_key: Option<String>,
    /// Scalar point values are returned separately for essential constraints.
    pub(crate) essential: Option<C>,
    /// Full Cartesian data for point or oriented-moment essential forms.
    pub(crate) essential_vector: Option<Vec<C>>,
    pub(crate) prescribed_scalar: Option<C>,
    pub(crate) coefficient_inputs: HashMap<String, C>,
    pub(crate) coefficients: HashMap<String, C>,
    pub(crate) sources: HashMap<String, Vec<C>>,
}

fn text<'a>(object: &'a Value, key: &str, path: &str) -> Result<&'a str, String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("{path}.{key} must be a nonempty string."))
}
fn number(object: &Value, path: &str) -> Result<Number, String> {
    match text(object, "number", path)? {
        "real" => Ok(Number::Real),
        "complex" => Ok(Number::Complex),
        other => Err(format!("{path}.number {other:?} is unsupported.")),
    }
}
fn find<'a>(row: &'a Value, path: &str) -> Option<&'a Value> {
    if path.starts_with('/') {
        row.pointer(path)
    } else {
        None
    }
}
fn compile_at(
    row: &Value,
    spec: &Value,
    sampling: Sampling,
    path: &str,
    optional: bool,
) -> Result<Option<Quantity>, String> {
    let pointer = text(spec, "path", path)?;
    if !pointer.starts_with('/') {
        return Err(format!("{path}.path must be a JSON pointer."));
    }
    let unit = text(spec, "unit", path)?;
    model_expression::unit(unit)?;
    let numeric = number(spec, path)?;
    let Some(value) = find(row, pointer) else {
        return if optional {
            Ok(None)
        } else {
            Err(format!(
                "{path}: required quantity at {pointer:?} is missing."
            ))
        };
    };
    Ok(Some(Quantity::compile(
        value, unit, numeric, sampling, path,
    )?))
}

/// Compile an incident scalar represented as a dimensionless factor times a
/// selected component of a Cartesian quantity array. This preserves the
/// existing incident-field expression and dimensional semantics.
fn compile_product(
    row: &Value,
    spec: &Value,
    sampling: Sampling,
    path: &str,
    optional: bool,
) -> Result<Option<Quantity>, String> {
    let product = spec
        .get("product")
        .ok_or("Incident product declaration is missing.")?;
    let factor_path = text(product, "factorPath", path)?;
    let components_path = text(product, "componentsPath", path)?;
    if !factor_path.starts_with('/') || !components_path.starts_with('/') {
        return Err(format!("{path}: product paths must be JSON pointers."));
    }
    let count = product
        .get("components")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| (1..=3).contains(v))
        .ok_or_else(|| format!("{path}: product requires 1–3 components."))?;
    let index = product
        .get("component")
        .and_then(Value::as_u64)
        .and_then(|v| usize::try_from(v).ok())
        .filter(|v| *v < count)
        .ok_or_else(|| format!("{path}: invalid product component."))?;
    let unit = text(spec, "unit", path)?;
    model_expression::unit(unit)?;
    let numeric = number(spec, path)?;
    let Some(factor) = find(row, factor_path) else {
        return if optional {
            Ok(None)
        } else {
            Err(format!("{path}: incident factor is missing."))
        };
    };
    let values = find(row, components_path)
        .and_then(Value::as_array)
        .filter(|v| v.len() == count)
        .ok_or_else(|| format!("{path}: incident components have the wrong shape."))?;
    let combined = json!({"incidentField":factor,"polarization":[values[index].clone()]});
    let quantities =
        model_expression::incident_component_quantities(&combined, &[unit.to_owned()], path)?
            .ok_or_else(|| format!("{path}: incident factor is missing."))?;
    Ok(Some(Quantity::compile(
        &quantities[0],
        unit,
        numeric,
        sampling,
        path,
    )?))
}

impl BoundaryInput {
    /// Exact instantaneous prescribed rate where the real expression can be
    /// symbolically differentiated. None denotes unsupported differentiation.
    pub(crate) fn prescribed_time_derivative(
        &self,
        parameters: &HashMap<String, Scalar>,
        point: [f64; 3],
        time: f64,
    ) -> Result<Option<(C, Option<Vec<C>>)>, String> {
        let prescribed = self
            .prescribed
            .as_ref()
            .ok_or("Boundary has no prescribed value.")?;
        if parameters.contains_key("t")
            || !time.is_finite()
            || point.iter().any(|value| !value.is_finite())
        {
            return Err("Boundary derivative has invalid time, point or parameters.".into());
        }
        let mut symbols = parameters.clone();
        symbols.insert(
            "t".into(),
            Scalar {
                value: time,
                dimension: model_expression::unit("s")?.1,
            },
        );
        if matches!(self.sampling, Sampling::Point) {
            for (axis, value) in ["x", "y", "z"].into_iter().zip(point) {
                symbols.insert(
                    axis.into(),
                    Scalar {
                        value,
                        dimension: model_expression::unit("m")?.1,
                    },
                );
            }
        }
        if let Some(vector) = &prescribed.vector {
            let mut result = Vec::with_capacity(vector.len());
            for value in vector {
                let Some(value) = value.time_derivative(&symbols, "initial boundary rate")? else {
                    return Ok(None);
                };
                result.push(value);
            }
            Ok(Some((C::default(), Some(result))))
        } else if let Some(scalar) = &prescribed.scalar {
            Ok(scalar
                .time_derivative(&symbols, "initial boundary rate")?
                .map(|value| (value, None)))
        } else {
            Ok(Some((C::default(), None)))
        }
    }
    /// Evaluate a boundary at a transient stage. The caller remains responsible
    /// for selecting a boundary law with a valid time-domain interpretation.
    pub(crate) fn evaluate_at_time(
        &self,
        parameters: &HashMap<String, Scalar>,
        point: Option<[f64; 3]>,
        time: f64,
        inputs: &HashMap<String, C>,
        form: Option<&BoundaryFormPlan>,
    ) -> Result<BoundaryValues, String> {
        if !time.is_finite() {
            return Err("Boundary time must be finite.".into());
        }
        if parameters.contains_key("t") || inputs.contains_key("t") {
            return Err("Boundary parameter t conflicts with transient time.".into());
        }
        let mut symbols = parameters.clone();
        symbols.insert(
            "t".into(),
            Scalar {
                value: time,
                dimension: model_expression::unit("s")?.1,
            },
        );
        let mut numerical = inputs.clone();
        numerical.insert("t".into(), C { re: time, im: 0. });
        self.evaluate(&symbols, point, &numerical, form)
    }
    /// Contract modes: `point` prescribes a scalar essential value; `trace`
    /// chooses a scalar or Cartesian source; `impedance` has no prescribed
    /// trace but accepts named coefficient inputs and optional sources.
    /// Unsupported modes and incomplete declarations fail at compile time.
    pub(crate) fn compile(contract: &Value, row: &Value) -> Result<Self, String> {
        let mode = text(contract, "mode", "boundaryInput")?;
        if !matches!(
            mode,
            "point" | "moment" | "trace" | "momentOrTrace" | "impedance"
        ) {
            return Err(format!("Unsupported boundary input mode {mode:?}."));
        }
        let sampling = match text(contract, "sampling", "boundaryInput")? {
            "parameters" => Sampling::Parameters,
            "point" => Sampling::Point,
            other => return Err(format!("Unsupported boundary sampling {other:?}.")),
        };
        let mut form_key = None;
        let prescribed = if mode != "impedance" {
            let spec = contract
                .get("prescribed")
                .ok_or("boundaryInput.prescribed is required.")?;
            let scalar_spec = spec
                .get("scalar")
                .ok_or("boundaryInput.prescribed.scalar is required.")?;
            let scalar_optional = spec
                .get("scalarOptionalWhenVector")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let vector_spec = spec.get("vector");
            let vector_path = vector_spec
                .and_then(|vector| vector.get("path"))
                .and_then(Value::as_str);
            let vector_fallback_path = vector_spec
                .and_then(|vector| vector.get("fallbackPath"))
                .and_then(Value::as_str);
            let vector_is_present = vector_path
                .and_then(|path| find(row, path))
                .is_some_and(Value::is_array)
                || vector_fallback_path
                    .and_then(|path| find(row, path))
                    .is_some_and(Value::is_array);
            let scalar_path = scalar_spec.get("path").and_then(Value::as_str);
            let scalar_is_present = scalar_path
                .and_then(|path| find(row, path))
                .is_some_and(|value| !value.is_array());
            let scalar_is_omitted_because_vector_is_supplied =
                scalar_optional && vector_is_present && !scalar_is_present;
            let scalar = if scalar_is_omitted_because_vector_is_supplied {
                None
            } else {
                compile_at(
                    row,
                    scalar_spec,
                    sampling,
                    "boundaryInput.prescribed.scalar",
                    false,
                )?
            };
            let mut vector = match spec.get("vector") {
                None => None,
                Some(vector) => {
                    let pointer = text(vector, "path", "boundaryInput.prescribed.vector")?;
                    if !pointer.starts_with('/') {
                        return Err("Boundary vector path must be a JSON pointer.".into());
                    }
                    let length = vector
                        .get("components")
                        .and_then(Value::as_u64)
                        .and_then(|v| usize::try_from(v).ok())
                        .filter(|v| (2..=3).contains(v))
                        .ok_or("Boundary vector requires 2 or 3 components.")?;
                    let unit = text(vector, "unit", "boundaryInput.prescribed.vector")?;
                    model_expression::unit(unit)?;
                    let number = number(vector, "boundaryInput.prescribed.vector")?;
                    let legacy_counts = vector
                        .get("legacyComponentCounts")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .map(|value| {
                            value
                                .as_u64()
                                .and_then(|value| usize::try_from(value).ok())
                                .filter(|value| *value > 0 && *value < length)
                                .ok_or("Boundary legacy component count is invalid.")
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if legacy_counts.iter().any(|count| {
                        legacy_counts.iter().filter(|other| *other == count).count() > 1
                    }) {
                        return Err("Boundary legacy component counts cannot repeat.".into());
                    }
                    let minimum_legacy = legacy_counts.iter().copied().min().unwrap_or(length);
                    let defaults = vector
                        .get("missingComponentDefaults")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .enumerate()
                        .map(|(index, value)| {
                            Quantity::compile(
                                value,
                                unit,
                                number,
                                sampling,
                                &format!("boundary vector missingComponentDefaults[{index}]"),
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if !legacy_counts.is_empty() && defaults.len() != length - minimum_legacy {
                        return Err("Boundary vector missingComponentDefaults do not match the legacy component shapes.".into());
                    }
                    let alternative = vector.get("fallbackPath").and_then(Value::as_str);
                    if alternative.is_some_and(|p| !p.starts_with('/')) {
                        return Err("Boundary vector fallbackPath must be a JSON pointer.".into());
                    }
                    match find(row, pointer).or_else(|| {
                        alternative
                            .and_then(|p| find(row, p))
                            .filter(|value| value.is_array())
                    }) {
                        None => None,
                        Some(value) => {
                            let values = value
                                .as_array()
                                .filter(|v| v.len() == length || legacy_counts.contains(&v.len()))
                                .ok_or("Boundary vector has the wrong number of components.")?;
                            let mut quantities = values
                                .iter()
                                .enumerate()
                                .map(|(i, value)| {
                                    Quantity::compile(
                                        value,
                                        unit,
                                        number,
                                        sampling,
                                        &format!("boundary vector[{i}]"),
                                    )
                                })
                                .collect::<Result<Vec<_>, _>>()?;
                            if quantities.len() != length {
                                let offset = quantities.len().saturating_sub(minimum_legacy);
                                quantities.extend(defaults.iter().skip(offset).cloned());
                            }
                            Some(quantities)
                        }
                    }
                }
            };
            if scalar.is_none() && vector.is_none() {
                return Err("Boundary input requires a scalar or Cartesian vector.".into());
            }
            let precedence = match spec.get("vectorPrecedence") {
                None => "vector",
                Some(v) => v.as_str().ok_or("vectorPrecedence must be a string.")?,
            };
            match precedence {
                "vector" => {}
                "scalar" => vector = None,
                "reject" if vector.is_some() => {
                    return Err("Boundary contract rejects an accompanying vector.".into());
                }
                "reject" => {}
                other => return Err(format!("Unsupported vector precedence {other:?}.")),
            }
            let companion = match spec.get("companionScalar") {
                None => "ignore",
                Some(v) => v.as_str().ok_or("companionScalar must be a string.")?,
            };
            let zero_companion = match companion {
                "ignore" => false,
                "zero" => true,
                other => return Err(format!("Unsupported companion scalar policy {other:?}.")),
            };
            let zero_always = match spec
                .get("scalarPolicy")
                .and_then(Value::as_str)
                .unwrap_or("any")
            {
                "any" => false,
                "zeroOnly" => true,
                other => return Err(format!("Unsupported boundary scalar policy {other:?}.")),
            };
            let scalar_expansion = spec
                .get("scalarExpansion")
                .and_then(Value::as_u64)
                .unwrap_or(1) as usize;
            if !(1..=3).contains(&scalar_expansion) {
                return Err("Boundary scalar expansion must be 1–3.".into());
            }
            let scalar_component = spec
                .get("scalarComponent")
                .map(|value| {
                    value
                        .as_u64()
                        .and_then(|index| usize::try_from(index).ok())
                        .filter(|index| *index < scalar_expansion)
                        .ok_or("Boundary scalarComponent must index scalarExpansion.")
                })
                .transpose()?;
            let tolerance = spec
                .get("zeroTolerance")
                .map(|v| {
                    v.as_f64()
                        .filter(|v| v.is_finite() && *v >= 0.)
                        .ok_or("zeroTolerance must be nonnegative and finite.")
                })
                .transpose()?
                .unwrap_or(0.);
            let mixed_moment_or_trace = mode == "momentOrTrace";
            let trace = mode == "trace" || (mixed_moment_or_trace && vector.is_none());
            if trace {
                let choice = if vector.is_some() {
                    "vectorForm"
                } else {
                    "scalarForm"
                };
                form_key = Some(text(spec, choice, "boundaryInput.prescribed")?.into());
            }
            let name = if trace {
                text(spec, "source", "boundaryInput.prescribed")?.into()
            } else {
                String::new()
            };
            Some(Prescribed {
                scalar,
                vector,
                zero_companion,
                zero_always,
                scalar_expansion,
                scalar_component,
                trace,
                tolerance,
                name,
            })
        } else {
            form_key = Some(text(contract, "form", "boundaryInput")?.into());
            None
        };
        let compile_named = |key: &str| -> Result<Vec<NamedQuantity>, String> {
            let Some(values) = contract.get(key) else {
                return Ok(Vec::new());
            };
            let values = values
                .as_array()
                .ok_or_else(|| format!("boundaryInput.{key} must be an array."))?;
            let mut names = HashSet::new();
            values
                .iter()
                .enumerate()
                .map(|(i, item)| {
                    let path = format!("boundaryInput.{key}[{i}]");
                    let name = text(item, "name", &path)?.to_string();
                    if !names.insert(name.clone()) {
                        return Err(format!("{path}: duplicate symbol {name:?}."));
                    }
                    let optional = match item.get("optional") {
                        None => false,
                        Some(v) => v
                            .as_bool()
                            .ok_or_else(|| format!("{path}.optional must be boolean."))?,
                    };
                    let nonzero = match item.get("nonzero") {
                        None => false,
                        Some(v) => v
                            .as_bool()
                            .ok_or_else(|| format!("{path}.nonzero must be boolean."))?,
                    };
                    let minimum_real = item
                        .get("minimumReal")
                        .map(|v| {
                            v.as_f64()
                                .filter(|v| v.is_finite())
                                .ok_or_else(|| format!("{path}.minimumReal must be finite."))
                        })
                        .transpose()?;
                    let quantity = if item.get("product").is_some() {
                        if key != "sources" || item.get("path").is_some() {
                            return Err(format!(
                                "{path}: product is supported only for sources without path."
                            ));
                        }
                        compile_product(row, item, sampling, &path, optional)?
                    } else {
                        compile_at(row, item, sampling, &path, optional)?
                    };
                    Ok(NamedQuantity {
                        name,
                        quantity,
                        nonzero,
                        minimum_real,
                    })
                })
                .collect()
        };
        let coefficients = compile_named("coefficients")?;
        let sources = compile_named("sources")?;
        if prescribed
            .as_ref()
            .is_some_and(|p| !p.name.is_empty() && sources.iter().any(|s| s.name == p.name))
        {
            return Err("Boundary prescribed source duplicates a named source.".into());
        }
        Ok(Self {
            sampling,
            form_key,
            prescribed,
            coefficients,
            sources,
        })
    }

    pub(crate) fn form_key(&self) -> Option<&str> {
        self.form_key.as_deref()
    }

    /// `inputs` contains numerical symbols such as frequency; named boundary
    /// coefficients cannot overwrite them. Absent optional sources are zero.
    pub(crate) fn evaluate(
        &self,
        parameters: &HashMap<String, Scalar>,
        point: Option<[f64; 3]>,
        inputs: &HashMap<String, C>,
        form: Option<&BoundaryFormPlan>,
    ) -> Result<BoundaryValues, String> {
        let mut symbols = parameters.clone();
        match self.sampling {
            Sampling::Parameters => {
                for name in ["x", "y", "z"] {
                    symbols.remove(name);
                }
            }
            Sampling::Point => {
                let point = point.ok_or("Boundary point sampling requires a physical position.")?;
                if point.iter().any(|v| !v.is_finite()) {
                    return Err("Boundary sample position must be finite.".into());
                }
                let dimension = model_expression::unit("m")?.1;
                for (i, name) in ["x", "y", "z"].iter().enumerate() {
                    symbols.insert(
                        (*name).into(),
                        Scalar {
                            value: point[i],
                            dimension,
                        },
                    );
                }
            }
        }
        let mut essential = None;
        let mut essential_vector = None;
        let mut prescribed_scalar = None;
        let mut sources = HashMap::new();
        if let Some(prescribed) = &self.prescribed {
            let scalar = prescribed
                .scalar
                .as_ref()
                .map(|v| v.evaluate(&symbols, "boundary scalar"))
                .transpose()?
                .unwrap_or_default();
            prescribed_scalar = Some(scalar);
            if prescribed.zero_always && scalar.re.hypot(scalar.im) > prescribed.tolerance {
                return Err("Boundary scalar must evaluate to zero.".into());
            }
            if let Some(vector) = &prescribed.vector {
                if prescribed.zero_companion && scalar.re.hypot(scalar.im) > prescribed.tolerance {
                    return Err("Boundary companion scalar must evaluate to zero when a vector is supplied.".into());
                }
                let values = vector
                    .iter()
                    .map(|v| v.evaluate(&symbols, "boundary vector component"))
                    .collect::<Result<Vec<_>, _>>()?;
                if prescribed.trace {
                    sources.insert(prescribed.name.clone(), values);
                } else {
                    essential = Some(scalar);
                    essential_vector = Some(values);
                }
            } else if prescribed.trace {
                let mut expanded = vec![C::default(); prescribed.scalar_expansion];
                if let Some(component) = prescribed.scalar_component {
                    expanded[component] = scalar;
                } else {
                    expanded.fill(scalar);
                }
                sources.insert(prescribed.name.clone(), expanded);
            } else {
                essential = Some(scalar);
            }
        }
        for source in &self.sources {
            let value = source
                .quantity
                .as_ref()
                .map(|q| q.evaluate(&symbols, &source.name))
                .transpose()?
                .unwrap_or_default();
            if source.nonzero && value.re == 0.0 && value.im == 0.0 {
                return Err(format!("Boundary input {:?} must be nonzero.", source.name));
            }
            if source
                .minimum_real
                .is_some_and(|minimum| value.re < minimum)
            {
                return Err(format!(
                    "Boundary input {:?} has a real part below its declared minimum.",
                    source.name
                ));
            }
            sources.insert(source.name.clone(), vec![value]);
        }
        let mut coefficient_inputs = inputs.clone();
        for coefficient in &self.coefficients {
            if coefficient_inputs.contains_key(&coefficient.name) {
                return Err(format!(
                    "Boundary coefficient input {:?} duplicates a supplied symbol.",
                    coefficient.name
                ));
            }
            let value = coefficient
                .quantity
                .as_ref()
                .map(|q| q.evaluate(&symbols, &coefficient.name))
                .transpose()?
                .unwrap_or_default();
            if coefficient.nonzero && value.re == 0.0 && value.im == 0.0 {
                return Err(format!(
                    "Boundary input {:?} must be nonzero.",
                    coefficient.name
                ));
            }
            if coefficient
                .minimum_real
                .is_some_and(|minimum| value.re < minimum)
            {
                return Err(format!(
                    "Boundary input {:?} has a real part below its declared minimum.",
                    coefficient.name
                ));
            }
            coefficient_inputs.insert(coefficient.name.clone(), value);
        }
        let coefficients = match (self.form_key.as_ref(), form) {
            (Some(_), Some(form)) => form.evaluate_coefficients(&coefficient_inputs)?,
            (None, None) => HashMap::new(),
            _ => return Err("Boundary input and supplied form do not agree.".into()),
        };
        Ok(BoundaryValues {
            form_key: self.form_key.clone(),
            essential,
            essential_vector,
            prescribed_scalar,
            coefficient_inputs,
            coefficients,
            sources,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(expression: &str, unit: &str) -> Value {
        json!({"expression":expression,"unit":unit})
    }
    fn vector_form() -> BoundaryFormPlan {
        BoundaryFormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,
            "coefficients":{"load":1}, "loadTerms":[{"test":"value","sourceTrace":"normal","source":"flux","coefficient":"load"}]})).unwrap()
    }

    #[test]
    fn declared_three_dimensional_flux_vector_matches_scalar_normal_flux() {
        let package: Value =
            serde_json::from_str(include_str!("scenarios/thermal3d.json")).unwrap();
        let contract = &package["boundaryInputs"]["T"]["neumann"];
        let vector_form =
            BoundaryFormPlan::from_json(&package["boundaryForms"]["T"]["neumannVector"]).unwrap();
        let scalar_form =
            BoundaryFormPlan::from_json(&package["boundaryForms"]["T"]["neumannScalar"]).unwrap();
        let vector = BoundaryInput::compile(
            contract,
            &json!({"value":q("0","W/m^2"),
            "fieldValues":[q("2","W/m^2"),q("3","W/m^2"),q("4","W/m^2")]}),
        )
        .unwrap();
        let v = vector
            .evaluate(&HashMap::new(), None, &HashMap::new(), Some(&vector_form))
            .unwrap();
        for (normal, scalar_value) in [
            ([1., 0., 0.], "2"),
            ([0., -1., 0.], "-3"),
            ([0., 0., 1.], "4"),
            ([0.6, 0.8, 0.], "3.6"),
        ] {
            let scalar =
                BoundaryInput::compile(contract, &json!({"value":q(scalar_value,"W/m^2")}))
                    .unwrap();
            let s = scalar
                .evaluate(&HashMap::new(), None, &HashMap::new(), Some(&scalar_form))
                .unwrap();
            let mut vector_load = vec![C::default(); 3];
            let mut scalar_load = vec![C::default(); 3];
            let basis = [&[1. / 3.][..], &[1. / 3.][..], &[1. / 3.][..]];
            vector_form
                .accumulate_load(
                    &basis,
                    &normal,
                    &v.coefficients,
                    &v.sources,
                    0.5,
                    &mut vector_load,
                )
                .unwrap();
            scalar_form
                .accumulate_load(
                    &basis,
                    &normal,
                    &s.coefficients,
                    &s.sources,
                    0.5,
                    &mut scalar_load,
                )
                .unwrap();
            for (a, b) in vector_load.iter().zip(&scalar_load) {
                assert!((a.re - b.re).abs() < 1e-12);
            }
        }
        let bad = BoundaryInput::compile(
            contract,
            &json!({"value":q("1","W/m^2"),
            "fieldValues":[q("2","W/m^2"),q("3","W/m^2"),q("4","W/m^2")]}),
        )
        .unwrap();
        assert!(
            bad.evaluate(&HashMap::new(), None, &HashMap::new(), Some(&vector_form))
                .is_err()
        );
    }
    fn vector_contract(scope: &str) -> Value {
        json!({"mode":"trace","sampling":scope,
        "prescribed":{"scalar":{"path":"/value","unit":"W/m^2","number":"real"},
        "vector":{"path":"/fieldValues","unit":"W/m^2","number":"real","components":2},
        "scalarForm":"scalarFlux","vectorForm":"vectorFlux","source":"flux","companionScalar":"zero"}})
    }

    #[test]
    fn vector_precedence_and_normal_trace_produce_expected_load() {
        let row = json!({"value":q("0","W/m^2"),"fieldValues":[q("2","W/m^2"),q("3","W/m^2")]});
        let input = BoundaryInput::compile(&vector_contract("parameters"), &row).unwrap();
        assert_eq!(input.form_key(), Some("vectorFlux"));
        let form = vector_form();
        let value = input
            .evaluate(&HashMap::new(), None, &HashMap::new(), Some(&form))
            .unwrap();
        assert_eq!(value.form_key.as_deref(), Some("vectorFlux"));
        let mut load = vec![C::default(); 2];
        form.accumulate_load(
            &[&[0.25], &[0.75]],
            &[1., 0.],
            &value.coefficients,
            &value.sources,
            2.,
            &mut load,
        )
        .unwrap();
        assert_eq!((load[0].re, load[1].re), (1., 3.));
        let mut bad = row.clone();
        bad["value"] = q("1", "W/m^2");
        let bad = BoundaryInput::compile(&vector_contract("parameters"), &bad).unwrap();
        assert!(
            bad.evaluate(&HashMap::new(), None, &HashMap::new(), Some(&form))
                .is_err()
        );
        let scalar = BoundaryInput::compile(
            &vector_contract("parameters"),
            &json!({"value":q("1","W/m^2")}),
        )
        .unwrap();
        assert_eq!(scalar.form_key(), Some("scalarFlux"));
        let mut scalar_first = vector_contract("parameters");
        scalar_first["prescribed"]["vectorPrecedence"] = json!("scalar");
        let scalar = BoundaryInput::compile(&scalar_first, &row).unwrap();
        assert_eq!(scalar.form_key(), Some("scalarFlux"));
        let mut reject = vector_contract("parameters");
        reject["prescribed"]["vectorPrecedence"] = json!("reject");
        assert!(BoundaryInput::compile(&reject, &row).is_err());
        reject["prescribed"]["vectorPrecedence"] = json!("unknown");
        assert!(BoundaryInput::compile(&reject, &row).is_err());
    }

    #[test]
    fn point_scope_evaluates_coordinates_and_parameters_scope_rejects_them() {
        let contract = json!({"mode":"point","sampling":"point","prescribed":{
            "scalar":{"path":"/value","unit":"m","number":"real"}}});
        let row = json!({"value":q("x","m")});
        let input = BoundaryInput::compile(&contract, &row).unwrap();
        assert!(
            input
                .evaluate(&HashMap::new(), None, &HashMap::new(), None)
                .is_err()
        );
        let value = input
            .evaluate(&HashMap::new(), Some([2., 3., 0.]), &HashMap::new(), None)
            .unwrap();
        assert_eq!(value.essential.unwrap().re, 2.);
        let mut global = contract.clone();
        global["sampling"] = json!("parameters");
        assert!(BoundaryInput::compile(&global, &row).is_err());
    }

    #[test]
    fn transient_boundary_samples_dimensioned_time_and_position() {
        let contract = json!({"mode":"point","sampling":"point","prescribed":{
            "scalar":{"path":"/value","unit":"m","number":"real"}}});
        let row = json!({"value":q("x*(1+t/tau)","m")});
        let input = BoundaryInput::compile(&contract, &row).unwrap();
        let params = HashMap::from([(
            "tau".to_string(),
            Scalar {
                value: 2.,
                dimension: model_expression::unit("s").unwrap().1,
            },
        )]);
        let value = input
            .evaluate_at_time(&params, Some([4., 0., 0.]), 3., &HashMap::new(), None)
            .unwrap();
        assert!((value.essential.unwrap().re - 10.).abs() < 1e-12);
        assert!(
            input
                .evaluate_at_time(&params, Some([0.; 3]), f64::NAN, &HashMap::new(), None)
                .is_err()
        );
    }

    #[test]
    fn cartesian_point_and_oriented_moment_inputs_keep_complex_components() {
        for mode in ["point", "moment"] {
            let contract = json!({"mode":mode,"sampling":"point","prescribed":{
                "scalar":{"path":"/value","unit":"m","number":"complex"},
                "vector":{"path":"/fieldValues","fallbackPath":"/value","unit":"m","number":"complex","components":3},
                "scalarOptionalWhenVector":true,"companionScalar":"zero","zeroTolerance":1e-14
            }});
            let row =
                json!({"value":q("1e-15","m"),"fieldValues":[q("1+i","m"),q("2","m"),q("x","m")]});
            let input = BoundaryInput::compile(&contract, &row).unwrap();
            let values = input
                .evaluate(&HashMap::new(), Some([3., 0., 0.]), &HashMap::new(), None)
                .unwrap();
            assert_eq!(values.essential.unwrap().re, 1e-15);
            let vector = values.essential_vector.unwrap();
            assert_eq!((vector[0].re, vector[0].im, vector[2].re), (1., 1., 3.));
            let array_row = json!({"value":[q("1","m"),q("2+i","m"),q("3","m")]});
            let value = BoundaryInput::compile(&contract, &array_row)
                .unwrap()
                .evaluate(&HashMap::new(), Some([0.; 3]), &HashMap::new(), None)
                .unwrap();
            assert_eq!(value.essential.unwrap().re, 0.);
            assert_eq!(value.essential_vector.unwrap()[1].im, 1.);
            let bad =
                json!({"value":q("1e-5","m"),"fieldValues":[q("1","m"),q("2","m"),q("3","m")]});
            assert!(
                BoundaryInput::compile(&contract, &bad)
                    .unwrap()
                    .evaluate(&HashMap::new(), Some([0.; 3]), &HashMap::new(), None)
                    .is_err()
            );
        }
    }

    #[test]
    fn zero_only_scalar_trace_can_expand_to_a_cartesian_source() {
        let contract = json!({"mode":"trace","sampling":"point","prescribed":{
            "scalar":{"path":"/value","unit":"Pa","number":"complex"},
            "vector":{"path":"/fieldValues","unit":"Pa","number":"complex","components":2},
            "scalarForm":"load","vectorForm":"load","source":"traction","scalarPolicy":"zeroOnly",
            "zeroTolerance":1e-14,"scalarExpansion":2}});
        let form = BoundaryFormPlan::from_json(&json!({"dimension":2,"fieldComponents":2,"coefficients":{"load":1},
            "loadTerms":[{"test":"value","sourceTrace":"value","source":"traction","coefficient":"load"}]})).unwrap();
        let row = json!({"value":q("1e-15","Pa")});
        let values = BoundaryInput::compile(&contract, &row)
            .unwrap()
            .evaluate(&HashMap::new(), Some([0.; 3]), &HashMap::new(), Some(&form))
            .unwrap();
        assert_eq!(values.sources["traction"].len(), 2);
        assert_eq!(values.sources["traction"][1].re, 1e-15);
        let bad = json!({"value":q("1","Pa")});
        assert!(
            BoundaryInput::compile(&contract, &bad)
                .unwrap()
                .evaluate(&HashMap::new(), Some([0.; 3]), &HashMap::new(), Some(&form))
                .is_err()
        );
    }

    #[test]
    fn declared_legacy_boundary_vector_gets_its_explicit_zero_component() {
        let contract = json!({"mode":"point","sampling":"point","prescribed":{
            "scalar":{"path":"/value","unit":"m","number":"complex"},
            "vector":{"path":"/fieldValues","unit":"m","number":"complex","components":3,
                "legacyComponentCounts":[2],"missingComponentDefaults":[{"expression":"0","unit":"m"}]}}});
        let row = json!({"value":q("0","m"),"fieldValues":[q("1+i","m"),q("2","m")]});
        let input = BoundaryInput::compile(&contract, &row).unwrap();
        let values = input
            .evaluate(&HashMap::new(), Some([0.; 3]), &HashMap::new(), None)
            .unwrap();
        let vector = values.essential_vector.unwrap();
        assert_eq!(vector.len(), 3);
        assert_eq!((vector[0].re, vector[0].im), (1., 1.));
        assert_eq!(vector[1].re, 2.);
        assert_eq!(vector[2], C::default());
    }

    #[test]
    fn impedance_binds_complex_incident_product_and_coefficient() {
        let contract = json!({"mode":"impedance","sampling":"point","form":"robin",
            "coefficients":[{"name":"Z","path":"/impedance","unit":"Pa*s/m","number":"real"}],
            "sources":[{"name":"incident","unit":"Pa","number":"complex","optional":true,
                "product":{"factorPath":"/incidentField","componentsPath":"/polarization","components":1,"component":0}}]});
        let row = json!({"impedance":q("2","Pa*s/m"), "incidentField":q("2+i","1"), "polarization":[q("3","Pa")]});
        let input = BoundaryInput::compile(&contract, &row).unwrap();
        let form = BoundaryFormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,
            "coefficients":{"robin":{"symbol":"Z"}},"matrixTerms":[{"test":"value","trial":"value","coefficient":"robin"}]})).unwrap();
        let values = input
            .evaluate(
                &HashMap::new(),
                Some([0., 0., 0.]),
                &HashMap::new(),
                Some(&form),
            )
            .unwrap();
        assert_eq!(values.coefficients["robin"].re, 2.);
        assert_eq!(
            (
                values.sources["incident"][0].re,
                values.sources["incident"][0].im
            ),
            (6., 3.)
        );
        let no_incident =
            BoundaryInput::compile(&contract, &json!({"impedance":q("2","Pa*s/m")})).unwrap();
        let values = no_incident
            .evaluate(
                &HashMap::new(),
                Some([0., 0., 0.]),
                &HashMap::new(),
                Some(&form),
            )
            .unwrap();
        assert_eq!(values.sources["incident"][0].re, 0.);
    }

    #[test]
    fn declared_nonzero_coefficient_is_checked_at_each_point() {
        let contract = json!({"mode":"impedance","sampling":"point","form":"robin",
            "coefficients":[{"name":"Z","path":"/impedance","unit":"m","number":"real","nonzero":true}]});
        let row = json!({"impedance":q("x","m")});
        let input = BoundaryInput::compile(&contract, &row).unwrap();
        let form = BoundaryFormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,
            "coefficients":{"robin":{"symbol":"Z"}},"matrixTerms":[{"test":"value","trial":"value","coefficient":"robin"}]})).unwrap();
        assert!(
            input
                .evaluate(
                    &HashMap::new(),
                    Some([0., 0., 0.]),
                    &HashMap::new(),
                    Some(&form)
                )
                .err()
                .unwrap()
                .contains("nonzero")
        );
        assert_eq!(
            input
                .evaluate(
                    &HashMap::new(),
                    Some([2., 0., 0.]),
                    &HashMap::new(),
                    Some(&form)
                )
                .unwrap()
                .coefficients["robin"]
                .re,
            2.
        );
        let mut bad = contract;
        bad["coefficients"][0]["nonzero"] = json!("yes");
        assert!(BoundaryInput::compile(&bad, &row).is_err());
    }

    #[test]
    fn declared_real_lower_bound_accepts_boundary_and_rejects_negative() {
        let contract = json!({"mode":"impedance","sampling":"point","form":"robin",
            "coefficients":[{"name":"Z","path":"/impedance","unit":"Pa*s/m","number":"complex","minimumReal":0}]});
        let form = BoundaryFormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,
            "coefficients":{"robin":{"symbol":"Z"}},"matrixTerms":[{"test":"value","trial":"value","coefficient":"robin"}]})).unwrap();
        for (expression, valid) in [("0+i", true), ("1+i", true), ("-1+i", false)] {
            let row = json!({"impedance":q(expression,"Pa*s/m")});
            let input = BoundaryInput::compile(&contract, &row).unwrap();
            assert_eq!(
                input
                    .evaluate(&HashMap::new(), Some([0.; 3]), &HashMap::new(), Some(&form))
                    .is_ok(),
                valid
            );
        }
        let mut invalid = contract;
        invalid["coefficients"][0]["minimumReal"] = json!("zero");
        assert!(BoundaryInput::compile(&invalid, &json!({"impedance":q("1","Pa*s/m")})).is_err());
    }

    #[test]
    fn rejects_wrong_units_shapes_and_unsupported_capabilities() {
        let mut row = json!({"value":q("0","Pa"),"fieldValues":[q("2","W/m^2"),q("3","W/m^2")]});
        assert!(BoundaryInput::compile(&vector_contract("parameters"), &row).is_err());
        row["value"] = q("0", "W/m^2");
        row["fieldValues"] = json!([q("2", "W/m^2")]);
        assert!(BoundaryInput::compile(&vector_contract("parameters"), &row).is_err());
        let mut contract = vector_contract("parameters");
        contract["mode"] = json!("unknown");
        assert!(BoundaryInput::compile(&contract, &row).is_err());
        let mut contract = vector_contract("parameters");
        contract["sampling"] = json!("quadratureRule");
        assert!(BoundaryInput::compile(&contract, &row).is_err());
    }
}
