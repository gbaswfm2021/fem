//! Typed, scenario-neutral linear and bilinear facet forms.
//!
//! Facet geometry, material/input evaluation, essential constraints and global
//! sparse scatter remain with callers during the scenario migration. The
//! primitive contracts real finite-element basis traces with complex scalar
//! coefficients or complex field sources; it does not select a physics model.

use crate::coefficient_expr::CoefficientExpressions;
use crate::{c_add, c_mul, c_scale, C};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
enum Trace {
    Value,
    Normal,
    Tangential,
    NormalCross,
    Tangent2d,
    ZComponentScalar,
    NormalCrossZScalar,
}

impl Trace {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "value" => Ok(Self::Value),
            "normal" => Ok(Self::Normal),
            "tangential" => Ok(Self::Tangential),
            "normalCross" => Ok(Self::NormalCross),
            "tangent2d" => Ok(Self::Tangent2d),
            "zComponentScalar" => Ok(Self::ZComponentScalar),
            "normalCrossZScalar" => Ok(Self::NormalCrossZScalar),
            _ => Err(format!("Unsupported boundary trace operator {value:?}.")),
        }
    }

    fn project(self, input: &[C], normal: &[f64]) -> Result<Vec<C>, String> {
        let dim = normal.len();
        if input.is_empty() || input.iter().any(|v| !v.re.is_finite() || !v.im.is_finite()) {
            return Err("Boundary trace input must contain finite components.".into());
        }
        if matches!(self, Self::Value) {
            return Ok(input.to_vec());
        }
        if matches!(self, Self::ZComponentScalar | Self::NormalCrossZScalar) {
            if dim != 3 || input.len() != 1 {
                return Err(format!("Boundary {self:?} trace requires one scalar and a three-dimensional operator."));
            }
            return Ok(match self {
                Self::ZComponentScalar => vec![C::default(), C::default(), input[0]],
                Self::NormalCrossZScalar => vec![c_scale(input[0], normal[1]), c_scale(input[0], -normal[0]), C::default()],
                _ => unreachable!(),
            });
        }
        let planar_trace_from_spatial_vector =
            matches!(self, Self::Tangent2d) && dim == 2 && input.len() == 3;
        if input.len() != dim && !planar_trace_from_spatial_vector {
            return Err(format!(
                "Boundary {self:?} trace requires {dim} vector components."
            ));
        }
        let dot = input
            .iter()
            .zip(normal)
            .fold(C::default(), |sum, (v, n)| c_add(sum, c_scale(*v, *n)));
        match self {
            Self::Value => unreachable!(),
            Self::Normal => Ok(vec![dot]),
            Self::Tangential => Ok(input
                .iter()
                .zip(normal)
                .map(|(v, n)| c_add(*v, c_scale(dot, -*n)))
                .collect()),
            Self::NormalCross if dim == 3 => Ok(vec![
                c_add(c_scale(input[2], normal[1]), c_scale(input[1], -normal[2])),
                c_add(c_scale(input[0], normal[2]), c_scale(input[2], -normal[0])),
                c_add(c_scale(input[1], normal[0]), c_scale(input[0], -normal[1])),
            ]),
            Self::Tangent2d if dim == 2 => Ok(vec![c_add(
                c_scale(input[0], -normal[1]),
                c_scale(input[1], normal[0]),
            )]),
            Self::NormalCross => Err("The normal-cross trace requires dimension 3.".into()),
            Self::Tangent2d => Err("The tangent2d trace requires dimension 2.".into()),
            Self::ZComponentScalar | Self::NormalCrossZScalar => unreachable!(),
        }
    }
}

struct MatrixTerm {
    test: Trace,
    trial: Trace,
    coefficient: String,
}
struct LoadTerm {
    test: Trace,
    source_trace: Trace,
    source: String,
    coefficient: String,
}

pub(crate) struct BoundaryFormPlan {
    dimension: usize,
    operator_dimension: usize,
    components: usize,
    matrix_terms: Vec<MatrixTerm>,
    load_terms: Vec<LoadTerm>,
    expressions: CoefficientExpressions,
}

/// One declared boundary coupling between two possibly different fields.
pub(crate) struct CoupledBoundaryForm {
    dimension:usize,
    test_components:usize,
    trial_components:usize,
    test:Trace,
    trial:Trace,
}
impl CoupledBoundaryForm {
    pub(crate) fn from_json(value:&Value)->Result<Self,String> {
        let integer=|key:&str|value.get(key).and_then(Value::as_u64).and_then(|v|usize::try_from(v).ok())
            .ok_or_else(||format!("Coupled boundary {key} must be an integer."));
        let trace=|key:&str|value.get(key).and_then(Value::as_str)
            .ok_or_else(||format!("Coupled boundary {key} must be a trace."))
            .and_then(Trace::parse);
        let plan=Self {dimension:integer("dimension")?,test_components:integer("testComponents")?,
            trial_components:integer("trialComponents")?,test:trace("test")?,trial:trace("trial")?};
        if !(2..=3).contains(&plan.dimension) || !(1..=3).contains(&plan.test_components)
            || !(1..=3).contains(&plan.trial_components) {
            return Err("Coupled boundary dimensions or field components are invalid.".into());
        }
        Ok(plan)
    }
    pub(crate) fn require_shape(&self,dimension:usize,test_components:usize,trial_components:usize)->Result<(),String> {
        if (self.dimension,self.test_components,self.trial_components)!=(dimension,test_components,trial_components) {
            return Err("Coupled boundary shape differs from its fields.".into());
        }
        Ok(())
    }
    pub(crate) fn accumulate(&self,test_basis:&[&[f64]],trial_basis:&[&[f64]],normal:&[f64],
        coefficient:C,weight:f64,local:&mut [Vec<C>])->Result<(),String> {
        if normal.len()!=self.dimension || normal.iter().any(|v|!v.is_finite())
            || (normal.iter().map(|v|v*v).sum::<f64>()-1.).abs()>1e-9
            || !weight.is_finite() || !coefficient.re.is_finite() || !coefficient.im.is_finite()
            || local.len()!=test_basis.len() || local.iter().any(|row|row.len()!=trial_basis.len())
            || test_basis.iter().any(|v|v.len()!=self.test_components || v.iter().any(|x|!x.is_finite()))
            || trial_basis.iter().any(|v|v.len()!=self.trial_components || v.iter().any(|x|!x.is_finite())) {
            return Err("Coupled boundary basis, normal, coefficient or matrix shape is invalid.".into());
        }
        let convert=|v:&&[f64]|v.iter().map(|&re|C {re,im:0.}).collect::<Vec<_>>();
        let left=test_basis.iter().map(|b|self.test.project(&convert(b),normal)).collect::<Result<Vec<_>,_>>()?;
        let right=trial_basis.iter().map(|b|self.trial.project(&convert(b),normal)).collect::<Result<Vec<_>,_>>()?;
        let mut result=local.to_vec();
        for i in 0..left.len() {for j in 0..right.len() {
            result[i][j]=c_add(result[i][j],c_scale(c_mul(coefficient,inner(&left[i],&right[j])?),weight));
            if !result[i][j].re.is_finite() || !result[i][j].im.is_finite() {
                return Err("Coupled boundary produced a non-finite coefficient.".into());
            }
        }}
        for (dst,src) in local.iter_mut().zip(result) {*dst=src;}
        Ok(())
    }
}

impl BoundaryFormPlan {
    pub(crate) fn has_matrix_terms(&self) -> bool {
        !self.matrix_terms.is_empty()
    }

    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let dimension = value
            .get("dimension")
            .and_then(Value::as_u64)
            .ok_or("Boundary form dimension must be an integer.")? as usize;
        let operator_dimension=value.get("operatorDimension").and_then(Value::as_u64)
            .and_then(|value|usize::try_from(value).ok()).unwrap_or(dimension);
        let components = value
            .get("fieldComponents")
            .and_then(Value::as_u64)
            .ok_or("Boundary form fieldComponents must be an integer.")?
            as usize;
        if !(2..=3).contains(&dimension) || !(dimension..=3).contains(&operator_dimension) || components == 0 || components > 3 {
            return Err("Boundary form requires dimension 2 or 3 and 1–3 field components.".into());
        }
        let get = |term: &Value, key: &str| -> Result<String, String> {
            term.get(key)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| format!("Boundary form term {key} must be a nonempty string."))
        };
        let array = |name: &str| -> Result<Vec<Value>, String> {
            match value.get(name) {
                None => Ok(Vec::new()),
                Some(v) => v
                    .as_array()
                    .cloned()
                    .ok_or_else(|| format!("Boundary form {name} must be an array.")),
            }
        };
        let matrix_terms = array("matrixTerms")?
            .iter()
            .map(|term| {
                Ok(MatrixTerm {
                    test: Trace::parse(&get(term, "test")?)?,
                    trial: Trace::parse(&get(term, "trial")?)?,
                    coefficient: get(term, "coefficient")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let load_terms = array("loadTerms")?
            .iter()
            .map(|term| {
                Ok(LoadTerm {
                    test: Trace::parse(&get(term, "test")?)?,
                    source_trace: Trace::parse(&get(term, "sourceTrace")?)?,
                    source: get(term, "source")?,
                    coefficient: get(term, "coefficient")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        if matrix_terms.is_empty() && load_terms.is_empty() {
            return Err("A boundary form must declare a matrix or load term.".into());
        }
        let expressions = CoefficientExpressions::from_json(
            value
                .get("coefficients")
                .ok_or("Boundary form coefficients are required.")?,
        )?;
        Ok(Self {
            dimension,
            operator_dimension,
            components,
            matrix_terms,
            load_terms,
            expressions,
        })
    }

    pub(crate) fn require_shape(&self, dimension: usize, components: usize) -> Result<(), String> {
        if self.dimension != dimension || self.components != components {
            return Err(format!(
                "Boundary form declares dimension {} and {} components; expected {dimension} and {components}.",
                self.dimension, self.components
            ));
        }
        Ok(())
    }

    pub(crate) fn require_operator_dimension(&self,dimension:usize)->Result<(),String>{
        if self.operator_dimension!=dimension&&self.operator_dimension!=self.dimension{return Err(format!("Boundary form declares operator dimension {}; expected {dimension} or its mesh dimension {}.",self.operator_dimension,self.dimension));}
        Ok(())
    }

    pub(crate) fn evaluate_coefficients(
        &self,
        symbols: &HashMap<String, C>,
    ) -> Result<HashMap<String, C>, String> {
        self.expressions.evaluate(symbols)
    }

    fn validate_inputs(
        &self,
        basis: &[&[f64]],
        normal: &[f64],
        weight: f64,
    ) -> Result<Vec<Vec<C>>, String> {
        if normal.len() != self.operator_dimension
            || normal.iter().any(|n| !n.is_finite())
            || (normal.iter().map(|n| n * n).sum::<f64>() - 1.0).abs() > 1e-9
            || !weight.is_finite()
            || basis
                .iter()
                .any(|b| b.len() != self.components || b.iter().any(|v| !v.is_finite()))
        {
            return Err("Invalid boundary basis, unit normal, or quadrature weight.".into());
        }
        Ok(basis
            .iter()
            .map(|b| b.iter().map(|&re| C { re, im: 0.0 }).collect())
            .collect())
    }

    /// Add this point's contribution to a small local dense facet/element matrix.
    pub(crate) fn accumulate_matrix(
        &self,
        basis: &[&[f64]],
        normal: &[f64],
        coefficients: &HashMap<String, C>,
        weight: f64,
        matrix: &mut [Vec<C>],
    ) -> Result<(), String> {
        let basis = self.validate_inputs(basis, normal, weight)?;
        if matrix.len() != basis.len() || matrix.iter().any(|row| row.len() != basis.len()) {
            return Err("Boundary matrix shape does not match its basis.".into());
        }
        for term in &self.matrix_terms {
            let left = basis
                .iter()
                .map(|b| term.test.project(b, normal))
                .collect::<Result<Vec<_>, _>>()?;
            let right = basis
                .iter()
                .map(|b| term.trial.project(b, normal))
                .collect::<Result<Vec<_>, _>>()?;
            let coeff = *coefficients
                .get(&term.coefficient)
                .ok_or_else(|| format!("Missing boundary coefficient {:?}.", term.coefficient))?;
            if !coeff.re.is_finite()
                || !coeff.im.is_finite()
                || left
                    .iter()
                    .any(|v| v.len() != right.first().map_or(v.len(), Vec::len))
            {
                return Err(
                    "Boundary matrix term has an invalid coefficient or incompatible trace shapes."
                        .into(),
                );
            }
            for (i, a) in left.iter().enumerate() {
                for (j, b) in right.iter().enumerate() {
                    let product = inner(a, b)?;
                    matrix[i][j] = c_add(matrix[i][j], c_scale(c_mul(coeff, product), weight));
                    if !matrix[i][j].re.is_finite() || !matrix[i][j].im.is_finite() {
                        return Err("Boundary matrix coefficient is non-finite.".into());
                    }
                }
            }
        }
        Ok(())
    }

    /// Add declared linear terms to a small local load vector.
    pub(crate) fn accumulate_load(
        &self,
        basis: &[&[f64]],
        normal: &[f64],
        coefficients: &HashMap<String, C>,
        sources: &HashMap<String, Vec<C>>,
        weight: f64,
        load: &mut [C],
    ) -> Result<(), String> {
        let basis = self.validate_inputs(basis, normal, weight)?;
        if load.len() != basis.len() {
            return Err("Boundary load shape does not match its basis.".into());
        }
        for term in &self.load_terms {
            let source = sources
                .get(&term.source)
                .ok_or_else(|| format!("Missing boundary source {:?}.", term.source))?;
            let source = term.source_trace.project(source, normal)?;
            let coeff = *coefficients
                .get(&term.coefficient)
                .ok_or_else(|| format!("Missing boundary coefficient {:?}.", term.coefficient))?;
            if !coeff.re.is_finite() || !coeff.im.is_finite() {
                return Err("Boundary load coefficient is non-finite.".into());
            }
            for (i, b) in basis.iter().enumerate() {
                let test = term.test.project(b, normal)?;
                load[i] = c_add(
                    load[i],
                    c_scale(c_mul(coeff, inner(&test, &source)?), weight),
                );
                if !load[i].re.is_finite() || !load[i].im.is_finite() {
                    return Err("Boundary load is non-finite.".into());
                }
            }
        }
        Ok(())
    }
}

fn inner(left: &[C], right: &[C]) -> Result<C, String> {
    if left.len() != right.len() {
        return Err("Boundary test and trial/source traces have incompatible shapes.".into());
    }
    Ok(left.iter().zip(right).fold(C::default(), |sum, (a, b)| {
        c_add(
            sum,
            c_mul(
                C {
                    re: a.re,
                    im: -a.im,
                },
                *b,
            ),
        )
    }))
}

/// Accumulate a scalar nodal P1 facet load after the caller has evaluated the
/// scenario-declared field expressions at this quadrature point. The source
/// can be scalar or vector; its trace is selected by the plan.
pub(crate) fn add_scalar_nodal_load<const N: usize>(
    plan: &BoundaryFormPlan,
    nodes: &[usize; N],
    shapes: &[f64; N],
    normal: &[f64],
    coefficients: &HashMap<String, C>,
    source_name: &str,
    source: Vec<C>,
    weight: f64,
    rhs_re: &mut [f64],
    rhs_im: &mut [f64],
) -> Result<(), String> {
    if rhs_re.len() != rhs_im.len() || nodes.iter().any(|&i| i >= rhs_re.len()) {
        return Err("Boundary load references an invalid global degree of freedom.".into());
    }
    let basis_values: Vec<[f64; 1]> = shapes.iter().map(|&s| [s]).collect();
    let basis: Vec<&[f64]> = basis_values.iter().map(|value| value.as_slice()).collect();
    let sources = HashMap::from([(source_name.to_string(), source)]);
    let mut local = vec![C::default(); N];
    plan.accumulate_load(&basis, normal, coefficients, &sources, weight, &mut local)?;
    for (i, value) in local.iter().enumerate() {
        rhs_re[nodes[i]] += value.re;
        rhs_im[nodes[i]] += value.im;
    }
    Ok(())
}

/// Accumulate a scalar nodal P1 Robin matrix and optional incident source.
pub(crate) fn add_scalar_nodal_robin<const N: usize>(
    plan: &BoundaryFormPlan,
    nodes: &[usize; N],
    shapes: &[f64; N],
    normal: &[f64],
    coefficients: &HashMap<String, C>,
    incident: Option<C>,
    weight: f64,
    matrix: &mut [std::collections::BTreeMap<usize, C>],
    rhs_re: &mut [f64],
    rhs_im: &mut [f64],
) -> Result<(), String> {
    if matrix.len() != rhs_re.len()
        || rhs_re.len() != rhs_im.len()
        || nodes.iter().any(|&i| i >= matrix.len())
    {
        return Err("Boundary Robin term references an invalid global degree of freedom.".into());
    }
    let basis_values: Vec<[f64; 1]> = shapes.iter().map(|&s| [s]).collect();
    let basis: Vec<&[f64]> = basis_values.iter().map(|value| value.as_slice()).collect();
    let mut local_matrix = vec![vec![C::default(); N]; N];
    let mut local_load = vec![C::default(); N];
    plan.accumulate_matrix(&basis, normal, coefficients, weight, &mut local_matrix)?;
    if let Some(value) = incident {
        let sources = HashMap::from([("incident".to_string(), vec![value])]);
        plan.accumulate_load(
            &basis,
            normal,
            coefficients,
            &sources,
            weight,
            &mut local_load,
        )?;
    }
    crate::assembler::scatter_local(matrix, nodes, &local_matrix)?;
    for (i, value) in local_load.iter().enumerate() {
        rhs_re[nodes[i]] += value.re;
        rhs_im[nodes[i]] += value.im;
    }
    Ok(())
}

/// Resolve a boundary form through the same application registry as the volume form.
pub(crate) fn load_plan(
    profile: &str,
    formulation: &str,
    condition: &str,
) -> Result<BoundaryFormPlan, String> {
    let package = crate::scenario_package::load_installed(profile)?;
    let form = package
        .get("boundaryForms")
        .and_then(|v| v.get(formulation))
        .and_then(|v| v.get(condition))
        .ok_or_else(|| {
            format!("Scenario {profile:?} has no {condition:?} boundary form for {formulation:?}.")
        })?;
    BoundaryFormPlan::from_json(form).map_err(|e| {
        format!("Scenario {profile:?} boundary form {formulation:?}/{condition:?}: {e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn close(actual: C, re: f64, im: f64) {
        assert!(
            (actual.re - re).abs() < 1e-12 && (actual.im - im).abs() < 1e-12,
            "{actual:?} != {re}+{im}i"
        );
    }

    #[test]
    fn coupled_boundary_contracts_distinct_scalar_and_vector_traces() {
        let plan=CoupledBoundaryForm::from_json(&json!({"dimension":2,"testComponents":1,
            "trialComponents":2,"test":"value","trial":"normal"})).unwrap();
        let test:&[&[f64]]=&[&[0.5]];
        let trial:&[&[f64]]=&[&[2.,3.],&[1.,-1.]];
        let mut local=vec![vec![C::default();2]];
        plan.accumulate(test,trial,&[1.,0.],C{re:2.,im:0.},1.,&mut local).unwrap();
        close(local[0][0],2.,0.);close(local[0][1],1.,0.);
        assert!(plan.accumulate(test,trial,&[0.,0.],C{re:1.,im:0.},1.,&mut local).is_err());
        close(local[0][0],2.,0.);
    }

    #[test]
    fn tangential_robin_matrix_and_complex_load_come_from_json() {
        let plan = BoundaryFormPlan::from_json(&json!({"dimension":3,"fieldComponents":3,
            "matrixTerms":[{"test":"tangential","trial":"tangential","coefficient":"a"}],
            "loadTerms":[{"test":"tangential","source":"incident","sourceTrace":"tangential","coefficient":"b"}],
            "coefficients":{"a":{"complex":[0,{"divide":[{"symbol":"omega"},{"symbol":"z"}]}]},
              "b":{"complex":[0,{"divide":[{"multiply":[2,{"symbol":"omega"}]},{"symbol":"z"}]}]}}})).unwrap();
        let values = [[1.0, 2.0, 3.0], [-1.0, 1.0, 4.0]];
        let basis: Vec<&[f64]> = values.iter().map(|v| v.as_slice()).collect();
        let symbols = HashMap::from([
            ("omega".into(), C { re: 6., im: 0. }),
            ("z".into(), C { re: 3., im: 0. }),
        ]);
        let coeff = plan.evaluate_coefficients(&symbols).unwrap();
        let source = HashMap::from([(
            "incident".into(),
            vec![
                C { re: 1., im: 2. },
                C { re: 3., im: -1. },
                C { re: 99., im: 88. },
            ],
        )]);
        let mut matrix = vec![vec![C::default(); 2]; 2];
        let mut load = vec![C::default(); 2];
        plan.accumulate_matrix(&basis, &[0., 0., 1.], &coeff, 0.5, &mut matrix)
            .unwrap();
        plan.accumulate_load(&basis, &[0., 0., 1.], &coeff, &source, 0.5, &mut load)
            .unwrap();
        close(matrix[0][0], 0., 5.);
        close(matrix[0][1], 0., 1.);
        close(load[0], 0., 14.);
        close(load[1], 6., 4.);
    }

    #[test]
    fn normal_cross_source_is_oriented_and_ignores_normal_component() {
        let plan = BoundaryFormPlan::from_json(&json!({"dimension":3,"fieldComponents":3,
            "loadTerms":[{"test":"tangential","source":"field","sourceTrace":"normalCross","coefficient":"a"}],
            "coefficients":{"a":{"complex":[0,{"symbol":"omega"}]}}})).unwrap();
        let input = [2., 1., 5.];
        let basis: Vec<&[f64]> = vec![&input];
        let coeff = plan
            .evaluate_coefficients(&HashMap::from([("omega".into(), C { re: 2., im: 0. })]))
            .unwrap();
        let source = HashMap::from([(
            "field".into(),
            vec![
                C { re: 1., im: 0. },
                C { re: 3., im: 0. },
                C { re: 100., im: 0. },
            ],
        )]);
        let mut load = vec![C::default()];
        plan.accumulate_load(&basis, &[0., 0., 1.], &coeff, &source, 0.5, &mut load)
            .unwrap();
        close(load[0], 0., -5.);
        let mut reverse = vec![C::default()];
        plan.accumulate_load(&basis, &[0., 0., -1.], &coeff, &source, 0.5, &mut reverse)
            .unwrap();
        close(reverse[0], 0., 5.);
    }

    #[test]
    fn planar_tangent_trace_accepts_legacy_and_full_spatial_vectors() {
        let normal = [1., 0.];
        let legacy = Trace::Tangent2d
            .project(&[C { re: 2., im: 0. }, C { re: 3., im: 0. }], &normal)
            .unwrap();
        let full = Trace::Tangent2d
            .project(
                &[
                    C { re: 2., im: 0. },
                    C { re: 3., im: 0. },
                    C { re: 100., im: 0. },
                ],
                &normal,
            )
            .unwrap();
        assert_eq!(legacy, vec![C { re: 3., im: 0. }]);
        assert_eq!(full, legacy);
    }

    #[test]
    fn changing_json_coefficient_changes_load_and_invalid_shapes_fail() {
        let source = |scale| {
            json!({"dimension":2,"fieldComponents":1,
            "loadTerms":[{"test":"value","source":"flux","sourceTrace":"normal","coefficient":"a"}],
            "coefficients":{"a":scale}})
        };
        let basis_data = [0.5];
        let basis: Vec<&[f64]> = vec![&basis_data];
        let fields = HashMap::from([(
            "flux".into(),
            vec![C { re: 2., im: 3. }, C { re: 4., im: -1. }],
        )]);
        let mut results = Vec::new();
        for scale in [-1., 2.] {
            let plan = BoundaryFormPlan::from_json(&source(scale)).unwrap();
            let coeff = plan.evaluate_coefficients(&HashMap::new()).unwrap();
            let mut load = vec![C::default()];
            plan.accumulate_load(&basis, &[1., 0.], &coeff, &fields, 2., &mut load)
                .unwrap();
            results.push(load[0]);
            assert!(plan
                .accumulate_load(&basis, &[2., 0.], &coeff, &fields, 2., &mut load)
                .is_err());
            let mut negative = vec![C::default()];
            plan.accumulate_load(&basis, &[1., 0.], &coeff, &fields, -2., &mut negative)
                .unwrap();
            close(negative[0], -load[0].re, -load[0].im);
        }
        close(results[0], -2., -3.);
        close(results[1], 4., 6.);
    }

    

    

    
}
