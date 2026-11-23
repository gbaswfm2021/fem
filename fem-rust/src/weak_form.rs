//! Scenario-neutral, typed bilinear forms evaluated on element basis samples.
//!
//! Callers supply geometry, quadrature and coefficient values; the forms
//! themselves are compiled from scenario declarations.

use crate::{C, c_add, c_mul, c_scale};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operator {
    Value,
    Gradient,
    RotatedGradient,
    SymmetricGradient,
    Curl,
    Divergence,
}

impl Operator {
    fn parse(name: &str) -> Result<Self, String> {
        match name {
            "value" => Ok(Self::Value),
            "gradient" => Ok(Self::Gradient),
            "rotatedGradient" => Ok(Self::RotatedGradient),
            "symmetricGradient" => Ok(Self::SymmetricGradient),
            "curl" => Ok(Self::Curl),
            "divergence" => Ok(Self::Divergence),
            _ => Err(format!("Unsupported weak-form operator {name:?}.")),
        }
    }

    fn component_count(self, dimension: usize, field_components: usize) -> Result<usize, String> {
        match self {
            Self::Value => Ok(field_components),
            Self::Gradient => Ok(field_components * dimension),
            Self::RotatedGradient if dimension == 2 && field_components == 1 => Ok(2),
            Self::RotatedGradient => Err("Rotated gradient requires a scalar field in two dimensions.".into()),
            Self::SymmetricGradient if field_components == dimension => Ok(dimension * dimension),
            Self::Curl if dimension == 3 && field_components == 3 => Ok(3),
            Self::Divergence if field_components == dimension => Ok(1),
            Self::Divergence => Err("Divergence requires a vector field matching the spatial dimension.".into()),
            Self::SymmetricGradient => Err(
                "Symmetric gradient requires a vector field matching the spatial dimension.".into(),
            ),
            Self::Curl => {
                Err("Curl currently requires a three-component field in three dimensions.".into())
            }
        }
    }

    fn sample(
        self,
        basis: &BasisSample<'_>,
        mesh_dimension: usize,
        operator_dimension: usize,
        field_components: usize,
        longitudinal_wavenumber: C,
        side: FormSide,
    ) -> Result<OperatorSample, String> {
        let expected = self.component_count(operator_dimension, field_components)?;
        let mut values = [C::default(); 9];
        match self {
            Self::Value => {
                if basis.value.len() != expected {
                    return Err(format!("Basis sample for {self:?} must have {expected} finite components."));
                }
                for (dst, &src) in values[..expected].iter_mut().zip(basis.value) { dst.re = src; }
            }
            Self::Gradient => {
                let gradient = gradient_sample(basis, mesh_dimension, operator_dimension,
                    field_components, longitudinal_wavenumber, side)?;
                values[..expected].copy_from_slice(&gradient);
            }
            Self::Curl => {
                if operator_dimension != 3 || field_components != 3 {
                    return Err("Curl requires a three-component field and a three-dimensional operator.".into());
                }
                if longitudinal_wavenumber == C::default() && basis.curl.len() == 3 {
                    for (dst, &src) in values[..3].iter_mut().zip(basis.curl) { dst.re = src; }
                } else {
                    let gradient = gradient_sample(basis, mesh_dimension, operator_dimension,
                        field_components, longitudinal_wavenumber, side)?;
                    values[0] = c_add(gradient[7], c_scale(gradient[5], -1.));
                    values[1] = c_add(gradient[2], c_scale(gradient[6], -1.));
                    values[2] = c_add(gradient[3], c_scale(gradient[1], -1.));
                }
            }
            Self::RotatedGradient => {
                if operator_dimension != 2 || mesh_dimension != 2 || field_components != 1 || basis.gradient.len() != 2 {
                    return Err("Basis gradient has an invalid shape for rotated gradient.".into());
                }
                values[0].re = basis.gradient[1];
                values[1].re = -basis.gradient[0];
            }
            Self::SymmetricGradient => {
                let gradient = gradient_sample(basis, mesh_dimension, operator_dimension,
                    field_components, longitudinal_wavenumber, side)?;
                if field_components != operator_dimension {
                    return Err("Symmetric gradient requires a vector field matching the operator dimension.".into());
                }
                for index in 0..expected {
                    let i = index / operator_dimension;
                    let j = index % operator_dimension;
                    values[index] = c_scale(c_add(gradient[i * operator_dimension + j], gradient[j * operator_dimension + i]), 0.5);
                }
            }
            Self::Divergence => {
                let gradient = gradient_sample(basis, mesh_dimension, operator_dimension,
                    field_components, longitudinal_wavenumber, side)?;
                if field_components != operator_dimension {
                    return Err("Divergence requires a vector field matching the operator dimension.".into());
                }
                for i in 0..operator_dimension { values[0] = c_add(values[0], gradient[i * operator_dimension + i]); }
            }
        }
        if values[..expected].iter().any(|x| !x.re.is_finite() || !x.im.is_finite()) {
            return Err(format!("Basis sample for {self:?} must have {expected} finite components."));
        }
        let mut active = [0; 9];
        let mut count = 0;
        for (index, value) in values[..expected].iter().enumerate() {
            if value.re != 0.0 || value.im != 0.0 { active[count] = index; count += 1; }
        }
        Ok(OperatorSample { values, active, count })
    }
}

fn gradient_sample(
    basis: &BasisSample<'_>, mesh_dimension: usize, operator_dimension: usize,
    field_components: usize, kz: C, side: FormSide,
) -> Result<Vec<C>, String> {
    let has_longitudinal_derivative = operator_dimension == 3
        && mesh_dimension == 2
        && kz != C::default();
    if (has_longitudinal_derivative && basis.value.len() != field_components)
        || basis.gradient.len() != field_components * mesh_dimension
        || !matches!((mesh_dimension, operator_dimension), (2, 2) | (2, 3) | (3, 3))
        || !kz.re.is_finite() || !kz.im.is_finite()
    { return Err("Basis sample has an invalid shape for the declared operator dimension.".into()); }
    let mut result = vec![C::default(); field_components * operator_dimension];
    for component in 0..field_components {
        for axis in 0..mesh_dimension {
            result[component * operator_dimension + axis].re = basis.gradient[component * mesh_dimension + axis];
        }
        if has_longitudinal_derivative {
            let factor = match side {
                FormSide::Test => C { re: -kz.im, im: kz.re },
                FormSide::Trial => C { re: kz.im, im: -kz.re },
            };
            result[component * operator_dimension + 2] = c_scale(factor, basis.value[component]);
        }
    }
    Ok(result)
}

/// Operators in the current 2D/3D spaces have at most nine components. Keep
/// them inline and retain only nonzero indices for sparse tensor contraction;
/// this avoids a heap allocation for each sampled basis/operator pair.
struct OperatorSample {
    values: [C; 9],
    active: [usize; 9],
    count: usize,
}

#[derive(Clone, Copy)]
enum FormSide { Test, Trial }

/// A basis function sampled in physical coordinates. Gradients use row-major
/// component-by-coordinate order. Basis functions are real; complex field
/// coefficients and constraint phases are handled by assembly/constraint maps.
pub(crate) struct BasisSample<'a> {
    pub(crate) value: &'a [f64],
    pub(crate) gradient: &'a [f64],
    pub(crate) curl: &'a [f64],
}

struct OwnedBasisSample { value:Vec<f64>, gradient:Vec<f64>, curl:Vec<f64> }

/// Physical P1 nodal basis samples for scalar or Cartesian vector fields.
/// Samples are ordered by node, then component, matching vector P1 DOFs.
pub(crate) struct NodalP1Samples { samples:Vec<OwnedBasisSample> }

impl NodalP1Samples {
    pub(crate) fn new(dimension:usize,components:usize,barycentric:&[f64],gradients:&[Vec<f64>]) -> Result<Self,String> {
        if !(2..=3).contains(&dimension) || !(1..=3).contains(&components)
            || barycentric.len()!=dimension+1 || gradients.len()!=barycentric.len()
            || gradients.iter().any(|row|row.len()!=dimension)
            || barycentric.iter().chain(gradients.iter().flatten()).any(|v|!v.is_finite()) {
            return Err("Nodal P1 samples have invalid dimension, shape or values.".into());
        }
        let mut samples=Vec::with_capacity(barycentric.len()*components);
        for (node,&basis) in barycentric.iter().enumerate() {
            for component in 0..components {
                let mut value=vec![0.;components]; value[component]=basis;
                let mut gradient=vec![0.;components*dimension];
                for axis in 0..dimension {gradient[component*dimension+axis]=gradients[node][axis];}
                let curl=if dimension==3 && components==3 {
                    let g=&gradients[node];
                    match component {0=>vec![0.,g[2],-g[1]],1=>vec![-g[2],0.,g[0]],_=>vec![g[1],-g[0],0.]}
                } else {Vec::new()};
                samples.push(OwnedBasisSample {value,gradient,curl});
            }
        }
        Ok(Self {samples})
    }

    pub(crate) fn samples(&self) -> Vec<BasisSample<'_>> {
        self.samples.iter().map(|s|BasisSample {value:&s.value,gradient:&s.gradient,curl:&s.curl}).collect()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Term {
    pub(crate) test: Operator,
    pub(crate) trial: Operator,
    pub(crate) coefficient: String,
}

/// A declared linear volume form. Source values are sampled by the model
/// adapter at the same physical quadrature point as the test basis.
pub(crate) struct VolumeLoadPlan {
    dimension: usize,
    operator_dimension: usize,
    field_components: usize,
    test: Operator,
    longitudinal_wavenumber: C,
}

/// One independently routed, already sampled volume load for sparse element assembly.
pub(crate) type VolumeLoadSamples<'a> = (&'a VolumeLoadPlan, &'a [Vec<Vec<C>>]);

impl VolumeLoadPlan {
    pub(crate) fn source_components(&self) -> Result<usize, String> {
        self.test.component_count(self.operator_dimension, self.field_components)
    }
    pub(crate) fn has_longitudinal_operator(&self) -> bool { self.operator_dimension > self.dimension }
    pub(crate) fn require_shape(&self, dimension: usize, field_components: usize) -> Result<(), String> {
        if (self.dimension, self.field_components) != (dimension, field_components) {
            return Err("Volume load form shape does not match the primary field.".into());
        }
        Ok(())
    }
    /// Compile the shape and test operator from a scenario declaration.
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let dimension = value.get("dimension").and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()).ok_or("Volume load dimension must be an integer.")?;
        let operator_dimension = value.get("operatorDimension").and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()).unwrap_or(dimension);
        let components = value.get("fieldComponents").and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()).ok_or("Volume load fieldComponents must be an integer.")?;
        let test = value.get("test").and_then(Value::as_str).ok_or("Volume load test must be a string.")?;
        Self::new_with_operator_dimension(dimension, operator_dimension, components, Operator::parse(test)?)
    }

    pub(crate) fn new(dimension: usize, field_components: usize, test: Operator) -> Result<Self, String> {
        Self::new_with_operator_dimension(dimension, dimension, field_components, test)
    }

    pub(crate) fn new_with_operator_dimension(dimension: usize, operator_dimension: usize,
        field_components: usize, test: Operator) -> Result<Self, String> {
        if !(2..=3).contains(&dimension) || !(dimension..=3).contains(&operator_dimension)
            || !(1..=3).contains(&field_components) {
            return Err("Invalid volume load form shape.".into());
        }
        test.component_count(operator_dimension, field_components)?;
        Ok(Self { dimension, operator_dimension, field_components, test,
            longitudinal_wavenumber:C::default() })
    }

    pub(crate) fn set_longitudinal_wavenumber(&mut self, value:C)->Result<(),String>{
        if !value.re.is_finite()||!value.im.is_finite() {return Err("Longitudinal wavenumber must be finite.".into());}
        if self.operator_dimension==self.dimension && (value.re!=0.||value.im!=0.) {
            return Err("This volume-load form does not declare a longitudinal operator dimension.".into());
        }
        self.longitudinal_wavenumber=value;Ok(())
    }

    /// Add the complex source contracted with each real test basis sample.
    /// Validate all inputs and the resulting values before changing the load.
    pub(crate) fn accumulate(&self, basis: &[BasisSample<'_>], source: &[C], weight: f64, local: &mut [C]) -> Result<(), String> {
        let count = self.test.component_count(self.operator_dimension, self.field_components)?;
        if !weight.is_finite() || local.len() != basis.len() || source.len() != count
            || source.iter().any(|x| !x.re.is_finite() || !x.im.is_finite()) {
            return Err("Invalid volume load sample, source shape or quadrature weight.".into());
        }
        let mut result = Vec::with_capacity(basis.len());
        for (sample, previous) in basis.iter().zip(local.iter()) {
            let test = self.test.sample(sample, self.dimension, self.operator_dimension,
                self.field_components, self.longitudinal_wavenumber, FormSide::Test)?;
            let mut value = *previous;
            for i in 0..count {
                value = c_add(value, c_scale(c_mul(source[i], test.values[i]), weight));
            }
            if !value.re.is_finite() || !value.im.is_finite() {
                return Err("Volume load accumulation is non-finite.".into());
            }
            result.push(value);
        }
        local.copy_from_slice(&result);
        Ok(())
    }
}

/// General coefficient map between the test and trial operator components.
/// A scalar denotes the scalar times identity and therefore requires equal
/// component counts. A matrix is stored in row-major test-by-trial order.
pub(crate) enum Coefficient<'a> {
    Scalar(C),
    Matrix {
        rows: usize,
        columns: usize,
        values: &'a [C],
    },
}

pub(crate) struct FormPlan {
    dimension: usize,
    operator_dimension: usize,
    field_components: usize,
    longitudinal_wavenumber: C,
    terms: Vec<Term>,
    expressions: Option<crate::coefficient_expr::CoefficientExpressions>,
    discretization: Option<crate::discretization::Discretization>,
}

/// A single bilinear coupling between independently declared field spaces.
/// The two bases can have different component and local DOF counts.
pub(crate) struct CoupledFormPlan {
    dimension: usize,
    operator_dimension: usize,
    test_components: usize,
    trial_components: usize,
    longitudinal_wavenumber: C,
    test: Operator,
    trial: Operator,
}

impl CoupledFormPlan {
    pub(crate) fn coefficient_shape(&self)->Result<(usize,usize),String> {
        Ok((self.test.component_count(self.operator_dimension,self.test_components)?,
            self.trial.component_count(self.operator_dimension,self.trial_components)?))
    }
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let integer = |key: &str| value.get(key).and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok()).ok_or_else(|| format!("Coupled form {key} must be an integer."));
        let operator = |key: &str| value.get(key).and_then(Value::as_str)
            .ok_or_else(|| format!("Coupled form {key} must be an operator string."))
            .and_then(Operator::parse);
        let dimension=integer("dimension")?;
        let operator_dimension=value.get("operatorDimension").and_then(Value::as_u64)
            .and_then(|v|usize::try_from(v).ok()).unwrap_or(dimension);
        let plan=Self { dimension,operator_dimension,test_components:integer("testComponents")?,
            trial_components:integer("trialComponents")?,longitudinal_wavenumber:C::default(),
            test:operator("test")?,trial:operator("trial")? };
        if !(2..=3).contains(&plan.dimension) || !(plan.dimension..=3).contains(&plan.operator_dimension)
            || !(1..=3).contains(&plan.test_components)
            || !(1..=3).contains(&plan.trial_components) {
            return Err("Coupled form has invalid dimension or field component count.".into());
        }
        plan.test.component_count(plan.operator_dimension,plan.test_components)?;
        plan.trial.component_count(plan.operator_dimension,plan.trial_components)?;
        Ok(plan)
    }

    pub(crate) fn set_longitudinal_wavenumber(&mut self,value:C)->Result<(),String>{
        if !value.re.is_finite()||!value.im.is_finite(){return Err("Longitudinal wavenumber must be finite.".into());}
        if self.operator_dimension==self.dimension&&(value.re!=0.||value.im!=0.){
            return Err("This coupled form does not declare a longitudinal operator dimension.".into());
        }
        self.longitudinal_wavenumber=value;Ok(())
    }

    pub(crate) fn accumulate(&self, test_basis: &[BasisSample<'_>], trial_basis: &[BasisSample<'_>],
        coefficient: &Coefficient<'_>, weight:f64, local:&mut [Vec<C>]) -> Result<(),String> {
        if !weight.is_finite() || local.len()!=test_basis.len()
            || local.iter().any(|row| row.len()!=trial_basis.len()) {
            return Err("Coupled form has invalid quadrature weight or local matrix shape.".into());
        }
        let rows=self.test.component_count(self.operator_dimension,self.test_components)?;
        let columns=self.trial.component_count(self.operator_dimension,self.trial_components)?;
        match coefficient {
            Coefficient::Scalar(c) if rows==columns && c.re.is_finite() && c.im.is_finite()=>{},
            Coefficient::Matrix {rows:r,columns:c,values} if *r==rows && *c==columns
                && values.len()==rows*columns && values.iter().all(|v| v.re.is_finite() && v.im.is_finite())=>{},
            _=>return Err("Coupled coefficient has incompatible shape or non-finite value.".into()),
        }
        let left=test_basis.iter().map(|b| self.test.sample(b,self.dimension,self.operator_dimension,
            self.test_components,self.longitudinal_wavenumber,FormSide::Test)).collect::<Result<Vec<_>,_>>()?;
        let right=trial_basis.iter().map(|b| self.trial.sample(b,self.dimension,self.operator_dimension,
            self.trial_components,self.longitudinal_wavenumber,FormSide::Trial)).collect::<Result<Vec<_>,_>>()?;
        let mut result=local.to_vec();
        for i in 0..left.len() { for j in 0..right.len() {
            let mut value=C::default();
            match coefficient {
                Coefficient::Scalar(c)=>for &r in &left[i].active[..left[i].count] {
                    value=c_add(value,c_scale(c_mul(*c,c_mul(left[i].values[r],right[j].values[r])),1.));
                },
                Coefficient::Matrix {columns,values,..}=>for &r in &left[i].active[..left[i].count] {
                    for &c in &right[j].active[..right[j].count] {
                        value=c_add(value,c_mul(values[r * *columns+c],c_mul(left[i].values[r],right[j].values[c])));
                    }
                },
            }
            result[i][j]=c_add(result[i][j],c_scale(value,weight));
            if !result[i][j].re.is_finite() || !result[i][j].im.is_finite() {
                return Err("Coupled form produced a non-finite matrix entry.".into());
            }
        }}
        for (dst,src) in local.iter_mut().zip(result) { *dst=src; }
        Ok(())
    }
}

impl FormPlan {
    pub(crate) fn declares_coefficient(&self, name: &str) -> bool {
        self.expressions
            .as_ref()
            .is_some_and(|expressions| expressions.declares(name))
    }

    pub(crate) fn require_matrix_coefficient_shape(
        &self,
        name: &str,
        rows: usize,
        columns: usize,
    ) -> Result<(), String> {
        let mut used = false;
        for term in self.terms.iter().filter(|term| term.coefficient == name) {
            used = true;
            let expected_rows = term
                .test
                .component_count(self.operator_dimension, self.field_components)?;
            let expected_columns = term
                .trial
                .component_count(self.operator_dimension, self.field_components)?;
            if (rows, columns) != (expected_rows, expected_columns) {
                return Err(format!(
                    "Coefficient {name:?} must be a {expected_rows}-by-{expected_columns} matrix."
                ));
            }
        }
        if !used {
            return Err(format!(
                "Matrix coefficient {name:?} is not used by a weak-form term."
            ));
        }
        Ok(())
    }
    pub(crate) fn shape(&self) -> (usize, usize) {
        (self.dimension, self.field_components)
    }

    pub(crate) fn operator_dimension(&self) -> usize { self.operator_dimension }
    pub(crate) fn has_longitudinal_operator(&self) -> bool { self.operator_dimension > self.dimension }
    pub(crate) fn has_active_longitudinal_derivative(&self) -> bool { self.longitudinal_wavenumber != C::default() }

    pub(crate) fn set_longitudinal_wavenumber(&mut self,value:C)->Result<(),String>{
        if !value.re.is_finite()||!value.im.is_finite(){return Err("Longitudinal wavenumber must be finite.".into());}
        if self.operator_dimension==self.dimension&&(value.re!=0.||value.im!=0.){
            return Err("This weak form does not declare a longitudinal operator dimension.".into());
        }
        self.longitudinal_wavenumber=value;Ok(())
    }

    pub(crate) fn has_discretization(&self) -> bool {
        self.discretization.is_some()
    }

    pub(crate) fn terms(&self) -> &[Term] {
        &self.terms
    }

    pub(crate) fn new(
        dimension: usize,
        field_components: usize,
        terms: Vec<Term>,
    ) -> Result<Self, String> {
        Self::new_with_operator_dimension(dimension,dimension,field_components,terms)
    }

    pub(crate) fn new_with_operator_dimension(
        dimension: usize,
        operator_dimension: usize,
        field_components: usize,
        terms: Vec<Term>,
    ) -> Result<Self, String> {
        if !(2..=3).contains(&dimension)
            || !(dimension..=3).contains(&operator_dimension)
            || field_components == 0
            || field_components > 3
            || terms.is_empty()
        {
            return Err("A weak form requires dimension 2 or 3, 1–3 field components and at least one term.".into());
        }
        for term in &terms {
            term.test.component_count(operator_dimension, field_components)?;
            term.trial.component_count(operator_dimension, field_components)?;
            if term.coefficient.is_empty() {
                return Err("A weak-form coefficient name cannot be empty.".into());
            }
        }
        Ok(Self {
            dimension,
            operator_dimension,
            field_components,
            longitudinal_wavenumber:C::default(),
            terms,
            expressions: None,
            discretization: None,
        })
    }

    pub(crate) fn require_shape(
        &self,
        dimension: usize,
        field_components: usize,
    ) -> Result<(), String> {
        if self.dimension != dimension || self.field_components != field_components {
            return Err(format!(
                "Weak form declares dimension {} and {} field components; expected {dimension} and {field_components}.",
                self.dimension, self.field_components
            ));
        }
        Ok(())
    }

    /// Parse the operator portion of a JSON form. Field spaces, units, material
    /// expression compilation and complete scenario validation remain future work.
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        let dimension = value
            .get("dimension")
            .and_then(Value::as_u64)
            .ok_or("Weak form dimension must be an integer.")? as usize;
        let components = value
            .get("fieldComponents")
            .and_then(Value::as_u64)
            .ok_or("Weak form fieldComponents must be an integer.")?
            as usize;
        let operator_dimension=value.get("operatorDimension").and_then(Value::as_u64)
            .and_then(|v|usize::try_from(v).ok()).unwrap_or(dimension);
        let raw_terms = value
            .get("terms")
            .and_then(Value::as_array)
            .ok_or("Weak form terms must be an array.")?;
        let terms = raw_terms
            .iter()
            .enumerate()
            .map(|(index, term)| {
                let get = |name: &str| {
                    term.get(name)
                        .and_then(Value::as_str)
                        .ok_or_else(|| format!("Weak form terms[{index}].{name} must be a string."))
                };
                Ok(Term {
                    test: Operator::parse(get("test")?)?,
                    trial: Operator::parse(get("trial")?)?,
                    coefficient: get("coefficient")?.to_string(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut plan = Self::new_with_operator_dimension(dimension, operator_dimension, components, terms)?;
        if let Some(expressions) = value.get("coefficients") {
            plan.expressions = Some(crate::coefficient_expr::CoefficientExpressions::from_json(
                expressions,
            )?);
        }
        if let Some(element) = value.get("element") {
            let discretization = crate::discretization::Discretization::from_json(element)?;
            if discretization.dimension() != dimension || discretization.components() != components
            {
                return Err(
                    "Weak form and element declare incompatible dimension or field components."
                        .into(),
                );
            }
            plan.discretization = Some(discretization);
        }
        Ok(plan)
    }

    pub(crate) fn require_discretization(
        &self,
        cell: crate::discretization::Cell,
        family: crate::discretization::Family,
        components: usize,
        order: usize,
    ) -> Result<(), String> {
        self.discretization
            .as_ref()
            .ok_or("Weak form does not declare its finite element.")?
            .require(cell, family, components, order)
    }

    pub(crate) fn evaluate_coefficients(
        &self,
        symbols: &HashMap<String, C>,
    ) -> Result<HashMap<String, C>, String> {
        self.expressions
            .as_ref()
            .ok_or("This weak form has no declared coefficient expressions.")?
            .evaluate(symbols)
    }

    pub(crate) fn bind_coefficients<'a>(
        &'a self,
        input_names: &[&str],
    ) -> Result<crate::coefficient_expr::BoundCoefficients<'a>, String> {
        self.expressions
            .as_ref()
            .ok_or("This weak form has no declared coefficient expressions.")?
            .bind(input_names)
    }

    /// Add a quadrature sample to a local dense matrix. The global master
    /// matrix remains sparse; local dense element matrices are bounded by the
    /// number of element degrees of freedom.
    pub(crate) fn accumulate(
        &self,
        basis: &[BasisSample<'_>],
        coefficients: &HashMap<String, Coefficient<'_>>,
        weight: f64,
        local: &mut [Vec<C>],
    ) -> Result<(), String> {
        if !weight.is_finite()
            || local.len() != basis.len()
            || local.iter().any(|row| row.len() != basis.len())
        {
            return Err("Invalid weak-form quadrature weight or local matrix shape.".into());
        }
        // Validate the entire sample and all coefficients before modifying the matrix.
        let mut prepared = Vec::with_capacity(self.terms.len());
        for term in &self.terms {
            let left = basis
                .iter()
                .map(|b| term.test.sample(b,self.dimension,self.operator_dimension,self.field_components,
                    self.longitudinal_wavenumber,FormSide::Test))
                .collect::<Result<Vec<_>, _>>()?;
            let right = if term.test == term.trial && self.longitudinal_wavenumber==C::default() {
                None
            } else {
                Some(
                    basis
                        .iter()
                        .map(|b| term.trial.sample(b,self.dimension,self.operator_dimension,self.field_components,
                            self.longitudinal_wavenumber,FormSide::Trial))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            };
            let rows = term
                .test
                .component_count(self.operator_dimension, self.field_components)?;
            let columns = term
                .trial
                .component_count(self.operator_dimension, self.field_components)?;
            let coefficient = coefficients
                .get(&term.coefficient)
                .ok_or_else(|| format!("Missing weak-form coefficient {:?}.", term.coefficient))?;
            match coefficient {
                Coefficient::Scalar(c)
                    if rows == columns && c.re.is_finite() && c.im.is_finite() => {}
                Coefficient::Matrix {
                    rows: r,
                    columns: c,
                    values,
                } if *r == rows
                    && *c == columns
                    && values.len() == rows * columns
                    && values.iter().all(|x| x.re.is_finite() && x.im.is_finite()) => {}
                _ => {
                    return Err(format!(
                        "Weak-form coefficient {:?} has an incompatible shape or non-finite value.",
                        term.coefficient
                    ));
                }
            }
            prepared.push((left, right, coefficient));
        }
        for (left, right, coefficient) in prepared {
            let right = right.as_ref().unwrap_or(&left);
            for i in 0..basis.len() {
                for j in 0..basis.len() {
                    let mut result = C::default();
                    match coefficient {
                        Coefficient::Scalar(value) => {
                            let dot = left[i].active[..left[i].count]
                                .iter()
                                .map(|&r| c_mul(left[i].values[r],right[j].values[r]))
                                .fold(C::default(),c_add);
                            result = c_mul(*value, dot);
                        }
                        Coefficient::Matrix {
                            columns, values, ..
                        } => {
                            for &r in &left[i].active[..left[i].count] {
                                for &c in &right[j].active[..right[j].count] {
                                    let product = c_mul(left[i].values[r],right[j].values[c]);
                                    result = c_add(result,c_mul(values[r * *columns + c],product));
                                }
                            }
                        }
                    }
                    local[i][j] = c_add(local[i][j], c_scale(result, weight));
                    if !local[i][j].re.is_finite() || !local[i][j].im.is_finite() {
                        return Err(
                            "Weak-form integration produced a non-finite matrix entry.".into()
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// Resolve a declared form through the same selected application and registry
/// used by the schema composer. The scenario runner's fixed profile ID is
/// transitional; the future general runner must receive the selected ID.
pub(crate) fn load_plan(profile_id: &str, formulation: &str) -> Result<FormPlan, String> {
    let package = crate::scenario_package::load_installed(profile_id)?;
    form_from_package(&package, profile_id, formulation)
}

fn form_from_package(
    package: &Value,
    profile_id: &str,
    formulation: &str,
) -> Result<FormPlan, String> {
    let form = package
        .get("weakForms")
        .and_then(|v| v.get(formulation))
        .ok_or_else(|| {
            format!("Scenario {profile_id:?} has no weak form for formulation {formulation:?}.")
        })?;
    FormPlan::from_json(form)
        .map_err(|e| format!("Scenario {profile_id:?} weakForms.{formulation}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn load_plan_from_directory(
        root: &std::path::Path,
        profile_id: &str,
        formulation: &str,
    ) -> Result<FormPlan, String> {
        let package = crate::scenario_package::load_from_directory(root, profile_id)?;
        form_from_package(&package, profile_id, formulation)
    }

    fn close(actual: C, re: f64, im: f64) {
        assert!(
            (actual.re - re).abs() < 1e-12 && (actual.im - im).abs() < 1e-12,
            "{actual:?} != {re}+{im}i"
        );
    }

    #[test]
    fn divergence_contracts_the_vector_gradient() {
        let plan=FormPlan::from_json(&json!({"dimension":2,"fieldComponents":2,"terms":[
            {"test":"divergence","trial":"divergence","coefficient":"a"}]})).unwrap();
        let basis=[BasisSample {value:&[],gradient:&[2.,3.,4.,5.],curl:&[]}];
        let mut local=vec![vec![C::default()]];
        let coefficients=HashMap::from([("a".to_string(),Coefficient::Scalar(C{re:2.,im:0.}))]);
        plan.accumulate(&basis,&coefficients,0.5,&mut local).unwrap();
        close(local[0][0],49.,0.);
        assert!(FormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"terms":[
            {"test":"divergence","trial":"value","coefficient":"a"}]})).is_err());
    }

    #[test]
    fn reduced_gradient_uses_distinct_analytic_test_and_trial_derivatives() {
        let mut plan=FormPlan::from_json(&json!({"dimension":2,"operatorDimension":3,
            "fieldComponents":1,"terms":[{"test":"gradient","trial":"gradient","coefficient":"a"}]})).unwrap();
        let basis=[
            BasisSample {value:&[0.6],gradient:&[1.,2.],curl:&[]},
            BasisSample {value:&[0.4],gradient:&[-2.,1.],curl:&[]},
        ];
        let coefficients=HashMap::from([("a".to_string(),Coefficient::Scalar(C{re:1.,im:0.}))]);
        plan.set_longitudinal_wavenumber(C{re:2.,im:3.}).unwrap();
        let mut finite_k=vec![vec![C::default();2];2];
        plan.accumulate(&basis,&coefficients,1.,&mut finite_k).unwrap();
        // The transverse gradients are orthogonal for this pair, leaving
        // (i*kz*0.6)*(-i*kz*0.4) = 0.24*kz^2, with no conjugation of kz.
        close(finite_k[0][1],-1.2,2.88);

        plan.set_longitudinal_wavenumber(C::default()).unwrap();
        let mut zero_k=vec![vec![C::default();2];2];
        plan.accumulate(&basis,&coefficients,1.,&mut zero_k).unwrap();
        let planar=FormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,
            "terms":[{"test":"gradient","trial":"gradient","coefficient":"a"}]})).unwrap();
        let mut legacy=vec![vec![C::default();2];2];
        planar.accumulate(&basis,&coefficients,1.,&mut legacy).unwrap();
        assert_eq!(zero_k,legacy);
    }

    #[test]
    fn coupled_form_integrates_distinct_scalar_and_vector_spaces() {
        let plan=CoupledFormPlan::from_json(&json!({"dimension":2,"testComponents":1,
            "trialComponents":2,"test":"value","trial":"divergence"})).unwrap();
        let test=[BasisSample {value:&[0.5],gradient:&[],curl:&[]}];
        let trial=[BasisSample {value:&[],gradient:&[1.,0.,0.,2.],curl:&[]},
            BasisSample {value:&[],gradient:&[2.,0.,0.,2.],curl:&[]}];
        let mut local=vec![vec![C::default();2]];
        plan.accumulate(&test,&trial,&Coefficient::Scalar(C{re:2.,im:0.}),1.,&mut local).unwrap();
        close(local[0][0],3.,0.);close(local[0][1],4.,0.);
        assert!(plan.accumulate(&test,&trial,&Coefficient::Scalar(C{re:f64::NAN,im:0.}),1.,&mut local).is_err());
        close(local[0][0],3.,0.);
    }

    #[test]
    fn nodal_p1_samples_follow_component_order_and_curl_orientation() {
        let storage=NodalP1Samples::new(3,3,&[0.25;4],&[
            vec![-1.,-1.,-1.],vec![1.,0.,0.],vec![0.,1.,0.],vec![0.,0.,1.]]).unwrap();
        let samples=storage.samples();
        assert_eq!(samples.len(),12);
        assert_eq!(samples[3].value,&[0.25,0.,0.]);
        assert_eq!(samples[4].gradient,&[0.,0.,0.,1.,0.,0.,0.,0.,0.]);
        assert_eq!(samples[4].curl,&[0.,0.,1.]);
        assert!(NodalP1Samples::new(2,1,&[0.5,0.5],&[vec![0.,0.],vec![0.,0.]]).is_err());
    }

    #[test]
    fn volume_load_contracts_vector_basis_and_preserves_load_on_error() {
        let plan = VolumeLoadPlan::from_json(&json!({"dimension":2,"fieldComponents":2,"test":"value"})).unwrap();
        let basis = [
            BasisSample { value: &[0.25, 0.], gradient: &[], curl: &[] },
            BasisSample { value: &[0., 0.25], gradient: &[], curl: &[] },
        ];
        let mut local = [C::default(); 2];
        plan.accumulate(&basis, &[C { re: 4., im: 2. }, C { re: -2., im: 6. }], 2., &mut local).unwrap();
        close(local[0], 2., 1.);
        close(local[1], -1., 3.);
        assert!(plan.accumulate(&basis, &[C { re: f64::NAN, im: 0. }; 2], 1., &mut local).is_err());
        close(local[0], 2., 1.);
        close(local[1], -1., 3.);
        assert!(VolumeLoadPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"test":"curl"})).is_err());
    }

    #[test]
    fn json_form_assembles_complex_curl_and_mass() {
        let plan = FormPlan::from_json(&json!({"dimension":3,"fieldComponents":3,"terms":[
            {"test":"curl","trial":"curl","coefficient":"a"},
            {"test":"value","trial":"value","coefficient":"b"}
        ]}))
        .unwrap();
        let samples = [
            BasisSample {
                value: &[1., 0., 0.],
                gradient: &[],
                curl: &[0., 0., 2.],
            },
            BasisSample {
                value: &[0., 1., 0.],
                gradient: &[],
                curl: &[0., 0., -1.],
            },
        ];
        let coefficients = HashMap::from([
            ("a".into(), Coefficient::Scalar(C { re: 2., im: 0. })),
            ("b".into(), Coefficient::Scalar(C { re: -3., im: 4. })),
        ]);
        let mut local = vec![vec![C::default(); 2]; 2];
        plan.accumulate(&samples, &coefficients, 0.5, &mut local)
            .unwrap();
        close(local[0][0], 2.5, 2.);
        close(local[0][1], -2., 0.);
        close(local[1][1], -0.5, 2.);
    }

    #[test]
    fn tensor_coefficients_and_symmetric_gradient_are_contracted() {
        let plan = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":2,"terms":[
            {"test":"symmetricGradient","trial":"symmetricGradient","coefficient":"C"}
        ]}))
        .unwrap();
        let sample = BasisSample {
            value: &[],
            gradient: &[0., 2., 0., 0.],
            curl: &[],
        };
        let matrix = [C { re: 2., im: 0. }; 16];
        let coefficients = HashMap::from([(
            "C".into(),
            Coefficient::Matrix {
                rows: 4,
                columns: 4,
                values: &matrix,
            },
        )]);
        let mut local = vec![vec![C::default(); 1]; 1];
        plan.accumulate(&[sample], &coefficients, 1., &mut local)
            .unwrap();
        close(local[0][0], 8., 0.);
    }

    #[test]
    fn malformed_form_or_coefficient_fails_before_mutation() {
        assert!(FormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"terms":[{"test":"curl","trial":"curl","coefficient":"x"}]})).is_err());
        let plan = FormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"terms":[{"test":"value","trial":"gradient","coefficient":"x"}]})).unwrap();
        let sample = BasisSample {
            value: &[1.],
            gradient: &[1., 2.],
            curl: &[],
        };
        let coefficients = HashMap::from([("x".into(), Coefficient::Scalar(C { re: 1., im: 0. }))]);
        let mut local = vec![vec![C { re: 7., im: 0. }; 1]; 1];
        assert!(
            plan.accumulate(&[sample], &coefficients, 1., &mut local)
                .is_err()
        );
        close(local[0][0], 7., 0.);
    }

    #[test]
    fn rejects_element_declaration_with_wrong_field_shape() {
        let mismatched = json!({"dimension":2,"fieldComponents":1,
            "element":{"cell":"tetrahedron","family":"lagrange","fieldComponents":1,"orders":[1]},
            "terms":[{"test":"gradient","trial":"gradient","coefficient":"a"}]});
        assert!(FormPlan::from_json(&mismatched).is_err());
    }

    #[test]
    fn loads_and_checks_the_registered_live_json_form() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("fem-weak-form-{}-{unique}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let registry = json!({"format":"fullmeta-fem-scenario-registry/1", "packages":[{"id":"example-2d", "file":"example2d.json"}]});
        fs::write(
            root.join("registry.json"),
            serde_json::to_vec(&registry).unwrap(),
        )
        .unwrap();
        let package = json!({"contractVersion":"fullmeta-fem-scenario/1", "id":"example-2d", "version":"1.0.0",
            "weakForms":{"steady":{"dimension":2,"fieldComponents":1,"terms":[
                {"test":"gradient","trial":"gradient","coefficient":"k"}
            ], "coefficients":{"k":{"symbol":"coefficient"}}}}});
        let path = root.join("example2d.json");
        fs::write(&path, serde_json::to_vec(&package).unwrap()).unwrap();
        let plan = load_plan_from_directory(&root, "example-2d", "steady").unwrap();
        plan.require_shape(2, 1).unwrap();
        let symbols = HashMap::from([("coefficient".to_string(), C { re: 4.0, im: 0.0 })]);
        close(plan.evaluate_coefficients(&symbols).unwrap()["k"], 4.0, 0.0);
        assert!(load_plan_from_directory(&root, "example-2d", "missing").is_err());
        assert!(load_plan_from_directory(&root, "../example", "steady").is_err());
        let mut wrong = package;
        wrong["id"] = json!("other-2d");
        fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
        assert!(load_plan_from_directory(&root, "example-2d", "steady").is_err());
        fs::remove_file(path).unwrap();
        fs::remove_file(root.join("registry.json")).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
