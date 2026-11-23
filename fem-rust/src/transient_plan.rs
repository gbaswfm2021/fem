//! Internal scenario-neutral declaration of transient state and block terms.
//! This parser alone does not advertise or enable a transient Study.
use crate::assembler::{SparseAssemblyWorkspace, SparseMatrixSink, scatter_coupling};
use crate::solver_common::C;
use crate::transient_step::{TimeDerivative, TransientOperators};
use crate::weak_form::{BasisSample, Coefficient, CoupledFormPlan, VolumeLoadPlan};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

pub(crate) struct StateField {
    pub(crate) id: String,
    pub(crate) components: usize,
    pub(crate) unit: Option<String>,
}

pub(crate) struct BlockTerm {
    pub(crate) test_field: usize,
    pub(crate) trial_field: usize,
    pub(crate) coefficient: String,
    /// None denotes an algebraic/spatial operator; Some denotes a derivative.
    pub(crate) derivative: Option<TimeDerivative>,
    pub(crate) form: CoupledFormPlan,
}

pub(crate) struct TransientPlan {
    pub(crate) dimension: usize,
    /// Dimension of the physical differential operator. A 2D cross-section
    /// may carry a three-dimensional operator when a prescribed longitudinal
    /// Fourier derivative is declared by the scenario form.
    pub(crate) operator_dimension: usize,
    /// Optional generic finite-element adapter key. Scenarios declare the
    /// space; runtime dispatch uses this capability without scenario IDs.
    pub(crate) element_space: Option<String>,
    pub(crate) fields: Vec<StateField>,
    pub(crate) terms: Vec<BlockTerm>,
}

#[derive(Clone)]
pub(crate) enum SampledCoefficient {
    Scalar(C),
    Tensor {
        rows: usize,
        columns: usize,
        values: Vec<C>,
    },
}

/// Compile scenario-declared scalar expressions and optional local tensors.
/// Inputs are already dimension-checked material/parameter/time symbols.
pub(crate) struct TransientCoefficients {
    scalars: crate::coefficient_expr::CoefficientExpressions,
    matrices: HashMap<String, crate::coefficient_matrix::CoefficientMatrixPlan>,
}
impl TransientCoefficients {
    pub(crate) fn from_json(plan: &TransientPlan, value: &Value) -> Result<Self, String> {
        let scalars = crate::coefficient_expr::CoefficientExpressions::from_json(
            value
                .get("scalars")
                .ok_or("Transient scalar coefficients are required.")?,
        )?;
        let mut matrices = HashMap::new();
        if let Some(entries) = value.get("matrices") {
            let entries = entries
                .as_object()
                .ok_or("Transient coefficient matrices must be an object.")?;
            for (name, value) in entries {
                if name.is_empty() || scalars.declares(name) {
                    return Err(format!(
                        "Transient matrix coefficient {name:?} is empty or conflicts with a scalar."
                    ));
                }
                let matrix = crate::coefficient_matrix::CoefficientMatrixPlan::from_json(value)?;
                if matrix.symbols().any(|symbol| !scalars.declares(symbol)) {
                    return Err(format!(
                        "Transient matrix coefficient {name:?} refers to an undeclared scalar."
                    ));
                }
                matrices.insert(name.clone(), matrix);
            }
        }
        for term in &plan.terms {
            let shape = term.form.coefficient_shape()?;
            if let Some(matrix) = matrices.get(&term.coefficient) {
                if matrix.shape() != shape {
                    return Err(format!(
                        "Transient coefficient {:?} has an incompatible tensor shape.",
                        term.coefficient
                    ));
                }
            } else if !scalars.declares(&term.coefficient) || shape.0 != shape.1 {
                return Err(format!(
                    "Transient coefficient {:?} is missing or needs a tensor.",
                    term.coefficient
                ));
            }
        }
        Ok(Self { scalars, matrices })
    }
    pub(crate) fn evaluate(
        &self,
        inputs: &HashMap<String, C>,
    ) -> Result<HashMap<String, SampledCoefficient>, String> {
        let values = self.scalars.evaluate(inputs)?;
        let mut result: HashMap<_, _> = values
            .iter()
            .map(|(name, value)| (name.clone(), SampledCoefficient::Scalar(*value)))
            .collect();
        for (name, matrix) in &self.matrices {
            let (rows, columns) = matrix.shape();
            result.insert(
                name.clone(),
                SampledCoefficient::Tensor {
                    rows,
                    columns,
                    values: matrix.evaluate(&values)?,
                },
            );
        }
        Ok(result)
    }

    /// Convert already unit-checked material/parameter values into the
    /// numeric coefficient engine used by both element adapters.
    pub(crate) fn evaluate_quantities(
        &self,
        inputs: &HashMap<String, crate::model_expression::Scalar>,
    ) -> Result<HashMap<String, SampledCoefficient>, String> {
        let mut numeric = HashMap::with_capacity(inputs.len());
        for (name, value) in inputs {
            if !value.value.is_finite() {
                return Err(format!(
                    "Transient coefficient input {name:?} is non-finite."
                ));
            }
            numeric.insert(
                name.clone(),
                C {
                    re: value.value,
                    im: 0.,
                },
            );
        }
        self.evaluate(&numeric)
    }
}
impl SampledCoefficient {
    fn as_coefficient(&self) -> Coefficient<'_> {
        match self {
            Self::Scalar(value) => Coefficient::Scalar(*value),
            Self::Tensor {
                rows,
                columns,
                values,
            } => Coefficient::Matrix {
                rows: *rows,
                columns: *columns,
                values,
            },
        }
    }
}

pub(crate) struct TransientSource<'a> {
    pub(crate) id: &'a str,
    pub(crate) field: usize,
    pub(crate) form: &'a VolumeLoadPlan,
}

/// Resolve the JSON-declared target field and source input contract once.
/// The selected model rows are bound separately by the shared selection code.
pub(crate) struct TransientDomainSourcePlan {
    pub(crate) feature_type: String,
    pub(crate) field: usize,
    pub(crate) input: crate::domain_source::DomainSourcePlan,
}

impl TransientDomainSourcePlan {
    pub(crate) fn from_json(plan: &TransientPlan, value: &Value) -> Result<Self, String> {
        let feature_type = value.get("featureType").and_then(Value::as_str)
            .filter(|v| !v.is_empty()).unwrap_or("domainSource").to_owned();
        let id = value
            .get("field")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or("Transient domain source requires a target field.")?;
        let field = plan
            .fields
            .iter()
            .position(|v| v.id == id)
            .ok_or_else(|| format!("Transient domain source field {id:?} is not declared."))?;
        let input = crate::domain_source::DomainSourcePlan::from_json(
            value,
            plan.dimension,
            plan.fields[field].components,
        )?;
        Ok(Self { feature_type, field, input })
    }
    pub(crate) fn source<'a>(&'a self, id: &'a str) -> TransientSource<'a> {
        TransientSource {
            id,
            field: self.field,
            form: &self.input.form,
        }
    }
}

/// Physical sample context is independent of selection-map representation.
#[derive(Clone, Copy)]
pub(crate) struct SampleLocation {
    pub(crate) cell: usize,
    pub(crate) material_tag: usize,
    pub(crate) point: [f64; 3],
    pub(crate) time: f64,
}

/// Independent contiguous global ranges for fields with unequal DOF counts.
pub(crate) struct StateLayout {
    offsets: Vec<usize>,
    counts: Vec<usize>,
    pub(crate) dimension: usize,
}

/// Reusable per-term sparse patterns for a fixed mixed-field mesh topology.
#[derive(Default)]
pub(crate) struct TransientAssemblyWorkspace {
    operator: SparseAssemblyWorkspace,
    derivatives: Vec<(TimeDerivative, SparseAssemblyWorkspace)>,
    dimension: usize,
}

impl TransientAssemblyWorkspace {
    /// `elements` contains the union of all coupled global field DOFs per
    /// element. Changed topology or field layout is detected by `prepare`.
    pub(crate) fn prepare(
        &mut self,
        plan: &TransientPlan,
        layout: &StateLayout,
        elements: &[Vec<usize>],
    ) -> Result<(), String> {
        let laws: Vec<_> = plan
            .terms
            .iter()
            .filter_map(|term| term.derivative)
            .collect();
        if self
            .derivatives
            .iter()
            .map(|item| item.0)
            .collect::<Vec<_>>()
            != laws
        {
            self.derivatives = laws
                .into_iter()
                .map(|law| (law, SparseAssemblyWorkspace::default()))
                .collect();
        }
        self.operator.prepare(layout.dimension, elements)?;
        for (_, workspace) in &mut self.derivatives {
            workspace.prepare(layout.dimension, elements)?;
        }
        self.dimension = layout.dimension;
        Ok(())
    }

    pub(crate) fn assemble_sample(
        &mut self,
        plan: &TransientPlan,
        layout: &StateLayout,
        term_index: usize,
        test_local: &[usize],
        trial_local: &[usize],
        test_basis: &[BasisSample<'_>],
        trial_basis: &[BasisSample<'_>],
        coefficient: &Coefficient<'_>,
        weight: f64,
    ) -> Result<(), String> {
        if self.dimension != layout.dimension {
            return Err("Transient cached layout has not been prepared.".into());
        }
        let term = plan
            .terms
            .get(term_index)
            .ok_or("Transient term index is out of range.")?;
        let test = layout.global_dofs(term.test_field, test_local)?;
        let trial = layout.global_dofs(term.trial_field, trial_local)?;
        let destination = if term.derivative.is_some() {
            let index = plan.terms[..term_index]
                .iter()
                .filter(|item| item.derivative.is_some())
                .count();
            let (law, matrix) = self
                .derivatives
                .get_mut(index)
                .ok_or("Transient cached derivative accumulator is missing.")?;
            if Some(*law) != term.derivative {
                return Err("Transient cached derivative declaration changed.".into());
            }
            matrix
        } else {
            &mut self.operator
        };
        plan.scatter_term(
            term_index,
            &test,
            &trial,
            test_basis,
            trial_basis,
            coefficient,
            weight,
            destination,
        )
    }

    pub(crate) fn rows(&self) -> Result<TransientOperators, String> {
        Ok(TransientOperators {
            derivatives: self
                .derivatives
                .iter()
                .map(|(law, matrix)| Ok((*law, matrix.rows()?)))
                .collect::<Result<Vec<_>, String>>()?,
            operator: self.operator.rows()?,
            load: vec![C::default(); self.dimension],
            essential: vec![],
            essential_rates: vec![],
        })
    }
}

impl StateLayout {
    pub(crate) fn new(plan: &TransientPlan, counts: &[usize]) -> Result<Self, String> {
        if counts.len() != plan.fields.len() || counts.contains(&0) {
            return Err("Each transient field must declare a nonzero DOF count.".into());
        }
        let mut offsets = Vec::with_capacity(counts.len());
        let mut dimension = 0usize;
        for &count in counts {
            offsets.push(dimension);
            dimension = dimension
                .checked_add(count)
                .ok_or("Transient state DOF count overflow.")?;
        }
        Ok(Self {
            offsets,
            counts: counts.to_vec(),
            dimension,
        })
    }

    pub(crate) fn global_dofs(&self, field: usize, local: &[usize]) -> Result<Vec<usize>, String> {
        let &offset = self
            .offsets
            .get(field)
            .ok_or("Transient field index is out of range.")?;
        let count = self.counts[field];
        local
            .iter()
            .map(|&index| {
                if index < count {
                    Ok(offset + index)
                } else {
                    Err("Transient field DOF is out of range.".into())
                }
            })
            .collect()
    }

    pub(crate) fn field_count(&self, field: usize) -> Option<usize> {
        self.counts.get(field).copied()
    }

    /// Borrow one field's coefficients from a full physical-state snapshot.
    /// Result adapters can interpret these according to the declared element
    /// space; coefficients of oriented fields are not nodal values.
    pub(crate) fn field_state<'a>(&self, field: usize, state: &'a [C]) -> Result<&'a [C], String> {
        if state.len() != self.dimension {
            return Err("Transient result state does not match its field layout.".into());
        }
        let start = *self
            .offsets
            .get(field)
            .ok_or("Transient result field index is out of range.")?;
        Ok(&state[start..start + self.counts[field]])
    }
}

impl TransientPlan {
    pub(crate) fn from_json(value: &Value) -> Result<Self, String> {
        if value.get("format").and_then(Value::as_str) != Some("fullmeta-transient-form/1") {
            return Err("Transient form must declare format fullmeta-transient-form/1.".into());
        }
        let dimension = value
            .get("dimension")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| (2..=3).contains(n))
            .ok_or("Transient form dimension must be 2 or 3.")?;
        let operator_dimension = value
            .get("operatorDimension")
            .and_then(Value::as_u64)
            .and_then(|n| usize::try_from(n).ok())
            .unwrap_or(dimension);
        if !(dimension..=3).contains(&operator_dimension) {
            return Err("Transient operatorDimension must be the mesh dimension or 3.".into());
        }
        let element_space = value.get("elementSpace").map(|space| {
            let name=space.as_str().filter(|name|!name.is_empty()&&name.len()<=128)
                .ok_or("Transient elementSpace must be a nonempty string.")?;
            Ok::<_,String>(name.to_owned())
        }).transpose()?;
        let raw_fields = value
            .get("fields")
            .and_then(Value::as_array)
            .filter(|v| !v.is_empty())
            .ok_or("Transient form needs at least one state field.")?;
        let mut fields = Vec::with_capacity(raw_fields.len());
        let mut indices = HashMap::new();
        for (index, row) in raw_fields.iter().enumerate() {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .ok_or_else(|| format!("Transient field {index} requires an id."))?;
            let components = row
                .get("components")
                .and_then(Value::as_u64)
                .and_then(|v| usize::try_from(v).ok())
                .filter(|v| (1..=3).contains(v))
                .ok_or_else(|| format!("Transient field {id:?} needs 1–3 components."))?;
            if indices.insert(id.to_string(), index).is_some() {
                return Err(format!("Duplicate transient field {id:?}."));
            }
            let unit = row
                .get("unit")
                .map(|value| {
                    let unit = value.as_str().filter(|v| !v.is_empty()).ok_or_else(|| {
                        format!("Transient field {id:?} unit must be a nonempty string.")
                    })?;
                    crate::model_expression::unit(unit)?;
                    Ok::<_, String>(unit.to_owned())
                })
                .transpose()?;
            fields.push(StateField {
                id: id.into(),
                components,
                unit,
            });
        }
        let raw_terms = value
            .get("terms")
            .and_then(Value::as_array)
            .filter(|v| !v.is_empty())
            .ok_or("Transient form needs at least one operator term.")?;
        let mut terms = Vec::with_capacity(raw_terms.len());
        let mut touched = HashSet::new();
        for (index, row) in raw_terms.iter().enumerate() {
            let field = |key: &str| -> Result<usize, String> {
                let id = row
                    .get(key)
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("Transient term {index} requires {key}."))?;
                indices.get(id).copied().ok_or_else(|| {
                    format!("Transient term {index} refers to unknown field {id:?}.")
                })
            };
            let test_field = field("testField")?;
            let trial_field = field("trialField")?;
            let coefficient = row
                .get("coefficient")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| format!("Transient term {index} requires a coefficient binding."))?
                .to_string();
            let derivative = match row.get("timeDerivative").and_then(Value::as_str) {
                Some("none") => None,
                Some("state") => Some(TimeDerivative::State),
                Some("conservedProduct") => Some(TimeDerivative::ConservedProduct),
                _ => {
                    return Err(format!(
                        "Transient term {index} has an unsupported time derivative."
                    ));
                }
            };
            let test = row
                .get("test")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Transient term {index} requires test operator."))?;
            let trial = row
                .get("trial")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Transient term {index} requires trial operator."))?;
            let form = CoupledFormPlan::from_json(&json!({"dimension":dimension,"operatorDimension":operator_dimension,
                "testComponents":fields[test_field].components,"trialComponents":fields[trial_field].components,
                "test":test,"trial":trial}))?;
            touched.insert(test_field);
            terms.push(BlockTerm {
                test_field,
                trial_field,
                coefficient,
                derivative,
                form,
            });
        }
        if touched.len() != fields.len() {
            return Err("Every transient state field needs at least one test equation.".into());
        }
        Ok(Self {
            dimension,
            operator_dimension,
            element_space,
            fields,
            terms,
        })
    }

    /// Apply a study's prescribed Fourier wavenumber to every declared block
    /// form. The same coefficient is used consistently by test and trial
    /// longitudinal derivatives; the form engine owns their conjugate signs.
    pub(crate) fn set_longitudinal_wavenumber(
        &mut self,
        value: C,
    ) -> Result<(), String> {
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err("Transient longitudinal wavenumber must be finite.".into());
        }
        if self.operator_dimension == self.dimension && value != C::default() {
            return Err(
                "Transient form does not declare a longitudinal operator dimension.".into(),
            );
        }
        for term in &mut self.terms {
            term.form.set_longitudinal_wavenumber(value)?;
        }
        Ok(())
    }

    /// Add one sampled coupling term to a sparse operator. The caller resolves
    /// its declared coefficient and selects the correct operator accumulator.
    pub(crate) fn scatter_term<M: SparseMatrixSink + ?Sized>(
        &self,
        term_index: usize,
        test_dofs: &[usize],
        trial_dofs: &[usize],
        test_basis: &[BasisSample<'_>],
        trial_basis: &[BasisSample<'_>],
        coefficient: &Coefficient<'_>,
        weight: f64,
        matrix: &mut M,
    ) -> Result<(), String> {
        let term = self
            .terms
            .get(term_index)
            .ok_or("Transient term index is out of range.")?;
        if test_dofs.len() != test_basis.len() || trial_dofs.len() != trial_basis.len() {
            return Err("Transient field DOFs do not match their basis samples.".into());
        }
        let mut local = vec![vec![C::default(); trial_basis.len()]; test_basis.len()];
        term.form
            .accumulate(test_basis, trial_basis, coefficient, weight, &mut local)?;
        scatter_coupling(matrix, test_dofs, trial_dofs, &local)
    }

    /// Create sparse accumulators in stable derivative declaration order.
    pub(crate) fn empty_operators(&self, layout: &StateLayout) -> TransientOperators {
        let empty = || vec![std::collections::BTreeMap::new(); layout.dimension];
        TransientOperators {
            derivatives: self
                .terms
                .iter()
                .filter_map(|term| term.derivative.map(|law| (law, empty())))
                .collect(),
            operator: empty(),
            load: vec![C::default(); layout.dimension],
            essential: vec![],
            essential_rates: vec![],
        }
    }

    /// Assemble one quadrature sample into its declared algebraic or time
    /// derivative block. Coefficients are evaluated externally at this point.
    pub(crate) fn assemble_sample(
        &self,
        layout: &StateLayout,
        term_index: usize,
        test_local: &[usize],
        trial_local: &[usize],
        test_basis: &[BasisSample<'_>],
        trial_basis: &[BasisSample<'_>],
        coefficient: &Coefficient<'_>,
        weight: f64,
        operators: &mut TransientOperators,
    ) -> Result<(), String> {
        let term = self
            .terms
            .get(term_index)
            .ok_or("Transient term index is out of range.")?;
        let test_dofs = layout.global_dofs(term.test_field, test_local)?;
        let trial_dofs = layout.global_dofs(term.trial_field, trial_local)?;
        let matrix = if term.derivative.is_some() {
            let index = self.terms[..term_index]
                .iter()
                .filter(|item| item.derivative.is_some())
                .count();
            let (law, matrix) = operators
                .derivatives
                .get_mut(index)
                .ok_or("Transient derivative accumulator is missing.")?;
            if Some(*law) != term.derivative {
                return Err(
                    "Transient derivative accumulator does not match its declaration.".into(),
                );
            }
            matrix
        } else {
            &mut operators.operator
        };
        if matrix.len() != layout.dimension {
            return Err("Transient accumulator dimension differs from state layout.".into());
        }
        self.scatter_term(
            term_index,
            &test_dofs,
            &trial_dofs,
            test_basis,
            trial_basis,
            coefficient,
            weight,
            matrix,
        )
    }

    /// Common quadrature contraction for any field-space adapters that supply
    /// physical basis samples and local field DOFs.
    pub(crate) fn assemble_quadrature(
        &self,
        layout: &StateLayout,
        workspace: &mut TransientAssemblyWorkspace,
        bases: &[Vec<BasisSample<'_>>],
        locals: &[Vec<usize>],
        weight: f64,
        coefficients: &HashMap<String, SampledCoefficient>,
    ) -> Result<(), String> {
        if bases.len() != self.fields.len() || locals.len() != self.fields.len() {
            return Err("Transient quadrature fields do not match their declaration.".into());
        }
        for (index, term) in self.terms.iter().enumerate() {
            let sampled = coefficients.get(&term.coefficient).ok_or_else(|| {
                format!(
                    "Transient coefficient {:?} was not evaluated.",
                    term.coefficient
                )
            })?;
            workspace.assemble_sample(
                self,
                layout,
                index,
                &locals[term.test_field],
                &locals[term.trial_field],
                &bases[term.test_field],
                &bases[term.trial_field],
                &sampled.as_coefficient(),
                weight,
            )?;
        }
        Ok(())
    }

    pub(crate) fn assemble_sources(
        &self,
        layout: &StateLayout,
        bases: &[Vec<BasisSample<'_>>],
        locals: &[Vec<usize>],
        location: SampleLocation,
        weight: f64,
        sources: &[TransientSource<'_>],
        source_value: &mut impl FnMut(&str, SampleLocation) -> Result<Vec<C>, String>,
        operators: &mut TransientOperators,
    ) -> Result<(), String> {
        for source in sources {
            let basis = bases
                .get(source.field)
                .ok_or("Transient source field is out of range.")?;
            let local = locals
                .get(source.field)
                .ok_or("Transient source field is out of range.")?;
            let value = source_value(source.id, location)?;
            self.assemble_load(
                layout,
                source.field,
                local,
                basis,
                source.form,
                &value,
                weight,
                operators,
            )?;
        }
        Ok(())
    }

    /// Add a pre-evaluated source at one quadrature point to a selected field.
    pub(crate) fn assemble_load(
        &self,
        layout: &StateLayout,
        field: usize,
        local_dofs: &[usize],
        basis: &[BasisSample<'_>],
        form: &VolumeLoadPlan,
        source: &[C],
        weight: f64,
        operators: &mut TransientOperators,
    ) -> Result<(), String> {
        let state = self
            .fields
            .get(field)
            .ok_or("Transient source field is out of range.")?;
        form.require_shape(self.dimension, state.components)?;
        if local_dofs.len() != basis.len() || operators.load.len() != layout.dimension {
            return Err("Transient load shape does not match its state field.".into());
        }
        let dofs = layout.global_dofs(field, local_dofs)?;
        let mut local = vec![C::default(); basis.len()];
        form.accumulate(basis, source, weight, &mut local)?;
        for (dof, value) in dofs.into_iter().zip(local) {
            let sum = crate::solver_common::c_add(operators.load[dof], value);
            if !sum.re.is_finite() || !sum.im.is_finite() {
                return Err("Transient load is non-finite.".into());
            }
            operators.load[dof] = sum;
        }
        Ok(())
    }

    /// Add an already sampled same-field boundary form to a chosen operator
    /// block and RHS. Cross-field boundary forms require a separate capability.
    pub(crate) fn assemble_boundary_sample(
        &self,
        layout: &StateLayout,
        field: usize,
        local_dofs: &[usize],
        basis: &[&[f64]],
        normal: &[f64],
        form: &crate::boundary_form::BoundaryFormPlan,
        coefficients: &HashMap<String, C>,
        sources: &HashMap<String, Vec<C>>,
        weight: f64,
        derivative_target: Option<usize>,
        operators: &mut TransientOperators,
    ) -> Result<(), String> {
        let state = self
            .fields
            .get(field)
            .ok_or("Transient boundary field is out of range.")?;
        form.require_shape(self.dimension, state.components)?;
        if local_dofs.len() != basis.len() || operators.load.len() != layout.dimension {
            return Err("Transient boundary DOFs do not match its field.".into());
        }
        let dofs = layout.global_dofs(field, local_dofs)?;
        let mut local = vec![vec![C::default(); basis.len()]; basis.len()];
        let mut rhs = vec![C::default(); basis.len()];
        form.accumulate_matrix(basis, normal, coefficients, weight, &mut local)?;
        form.accumulate_load(basis, normal, coefficients, sources, weight, &mut rhs)?;
        let matrix = match derivative_target {
            Some(index) => {
                &mut operators
                    .derivatives
                    .get_mut(index)
                    .ok_or("Transient boundary derivative target is out of range.")?
                    .1
            }
            None => &mut operators.operator,
        };
        if matrix.len() != layout.dimension {
            return Err("Transient boundary operator dimension is invalid.".into());
        }
        if form.has_matrix_terms() {
            scatter_coupling(matrix, &dofs, &dofs, &local)?;
        }
        for (dof, value) in dofs.into_iter().zip(rhs) {
            let sum = crate::solver_common::c_add(operators.load[dof], value);
            if !sum.re.is_finite() || !sum.im.is_finite() {
                return Err("Transient boundary load is non-finite.".into());
            }
            operators.load[dof] = sum;
        }
        Ok(())
    }

    /// Sample a declared time-domain boundary input and scatter its form at
    /// this quadrature point. Form selection is checked explicitly so a
    /// boundary's scalar/vector precedence cannot select the wrong law.
    pub(crate) fn assemble_boundary_at_time(
        &self,
        layout: &StateLayout,
        field: usize,
        local_dofs: &[usize],
        basis: &[&[f64]],
        normal: &[f64],
        form_key: &str,
        form: &crate::boundary_form::BoundaryFormPlan,
        input: &crate::boundary_input::BoundaryInput,
        parameters: &HashMap<String, crate::model_expression::Scalar>,
        coefficient_inputs: &HashMap<String, C>,
        location: SampleLocation,
        weight: f64,
        derivative_target: Option<usize>,
        operators: &mut TransientOperators,
    ) -> Result<(), String> {
        if input.form_key() != Some(form_key) {
            return Err("Transient boundary input selected a different declared form.".into());
        }
        let values = input.evaluate_at_time(
            parameters,
            Some(location.point),
            location.time,
            coefficient_inputs,
            Some(form),
        )?;
        self.assemble_boundary_sample(
            layout,
            field,
            local_dofs,
            basis,
            normal,
            form,
            &values.coefficients,
            &values.sources,
            weight,
            derivative_target,
            operators,
        )
    }

    pub(crate) fn assemble_coupled_boundary_sample(
        &self,
        layout: &StateLayout,
        test_field: usize,
        trial_field: usize,
        test_local: &[usize],
        trial_local: &[usize],
        test_basis: &[&[f64]],
        trial_basis: &[&[f64]],
        normal: &[f64],
        form: &crate::boundary_form::CoupledBoundaryForm,
        coefficient: C,
        weight: f64,
        derivative_target: Option<usize>,
        operators: &mut TransientOperators,
    ) -> Result<(), String> {
        let test = self
            .fields
            .get(test_field)
            .ok_or("Transient boundary test field is out of range.")?;
        let trial = self
            .fields
            .get(trial_field)
            .ok_or("Transient boundary trial field is out of range.")?;
        form.require_shape(self.dimension, test.components, trial.components)?;
        if test_local.len() != test_basis.len() || trial_local.len() != trial_basis.len() {
            return Err("Transient coupled boundary DOFs do not match its bases.".into());
        }
        let test_dofs = layout.global_dofs(test_field, test_local)?;
        let trial_dofs = layout.global_dofs(trial_field, trial_local)?;
        let mut local = vec![vec![C::default(); trial_basis.len()]; test_basis.len()];
        form.accumulate(
            test_basis,
            trial_basis,
            normal,
            coefficient,
            weight,
            &mut local,
        )?;
        let matrix = match derivative_target {
            Some(index) => {
                &mut operators
                    .derivatives
                    .get_mut(index)
                    .ok_or("Transient boundary derivative target is out of range.")?
                    .1
            }
            None => &mut operators.operator,
        };
        if matrix.len() != layout.dimension {
            return Err("Transient boundary operator dimension is invalid.".into());
        }
        scatter_coupling(matrix, &test_dofs, &trial_dofs, &local)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_boundary_input_is_sampled_and_scattered_at_stage_time() {
        let declaration = json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"u","components":1}],"terms":[{"testField":"u","trialField":"u",
                "test":"value","trial":"value","coefficient":"one","timeDerivative":"state"}]});
        let plan = TransientPlan::from_json(&declaration).unwrap();
        let layout = StateLayout::new(&plan, &[2]).unwrap();
        let contract = json!({"mode":"trace","sampling":"point","prescribed":{
            "scalar":{"path":"/value","unit":"1","number":"real"},"scalarForm":"fluxForm","source":"flux"}});
        let input = crate::boundary_input::BoundaryInput::compile(
            &contract,
            &json!({"value":{"expression":"t/tau","unit":"1"}}),
        )
        .unwrap();
        let form=crate::boundary_form::BoundaryFormPlan::from_json(&json!({"dimension":2,"fieldComponents":1,
            "loadTerms":[{"test":"value","source":"flux","sourceTrace":"value","coefficient":"one"}],"coefficients":{"one":1}})).unwrap();
        let mut operators = TransientOperators {
            derivatives: vec![],
            operator: vec![Default::default(); 2],
            load: vec![C::default(); 2],
            essential: vec![],
            essential_rates: vec![],
        };
        let parameters = HashMap::from([(
            "tau".into(),
            crate::model_expression::Scalar {
                value: 2.,
                dimension: crate::model_expression::unit("s").unwrap().1,
            },
        )]);
        let basis: [&[f64]; 2] = [&[0.5], &[0.5]];
        let location = SampleLocation {
            cell: 0,
            material_tag: 1,
            point: [0., 0., 0.],
            time: 4.,
        };
        plan.assemble_boundary_at_time(
            &layout,
            0,
            &[0, 1],
            &basis,
            &[1., 0.],
            "fluxForm",
            &form,
            &input,
            &parameters,
            &HashMap::new(),
            location,
            1.,
            None,
            &mut operators,
        )
        .unwrap();
        assert_eq!(operators.load[0].re, 1.);
        assert_eq!(operators.load[1].re, 1.);
        assert!(
            plan.assemble_boundary_at_time(
                &layout,
                0,
                &[0, 1],
                &basis,
                &[1., 0.],
                "wrong",
                &form,
                &input,
                &parameters,
                &HashMap::new(),
                location,
                1.,
                None,
                &mut operators
            )
            .is_err()
        );
    }

    #[test]
    fn all_scenario_forms_compile_their_declared_transient_forms() {
        let files = [
            include_str!("scenarios/acoustic2d.json"),
            include_str!("scenarios/acoustic3d.json"),
            include_str!("scenarios/em2d.json"),
            include_str!("scenarios/em3d.json"),
            include_str!("scenarios/elastic2d.json"),
            include_str!("scenarios/elastic3d.json"),
            include_str!("scenarios/thermal2d.json"),
            include_str!("scenarios/thermal3d.json"),
        ];
        for text in files {
            let package: Value = serde_json::from_str(text).unwrap();
            let forms = package["transientForms"].as_object().unwrap();
            for form in forms.values() {
                assert_eq!(form["status"], "supported");
                let plan = TransientPlan::from_json(form).unwrap();
                TransientCoefficients::from_json(&plan, &form["coefficients"]).unwrap();
                let routes = form.get("domainSources").and_then(Value::as_array)
                    .cloned().unwrap_or_else(|| vec![form["domainSource"].clone()]);
                for route in routes {
                    TransientDomainSourcePlan::from_json(&plan, &route).unwrap();
                    let mut invalid = route;
                    invalid["field"] = json!("missing");
                    assert!(TransientDomainSourcePlan::from_json(&plan, &invalid).is_err());
                }
            }
        }
    }
    #[test]
    fn mixed_declared_terms_resolve_fields_and_reject_bad_references() {
        let mut declaration = json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"p","components":1},{"id":"v","components":2}],
            "terms":[{"testField":"p","trialField":"v","test":"value","trial":"divergence","coefficient":"one","timeDerivative":"none"},
                {"testField":"v","trialField":"v","test":"value","trial":"value","coefficient":"rho","timeDerivative":"conservedProduct"}]});
        let plan = TransientPlan::from_json(&declaration).unwrap();
        assert_eq!(
            (plan.dimension, plan.fields.len(), plan.terms.len()),
            (2, 2, 2)
        );
        assert_eq!(
            (plan.terms[0].test_field, plan.terms[0].trial_field),
            (0, 1)
        );
        let layout = StateLayout::new(&plan, &[2, 3]).unwrap();
        assert_eq!(layout.dimension, 5);
        assert_eq!(layout.global_dofs(1, &[2, 0]).unwrap(), vec![4, 2]);
        let state = vec![
            C { re: 1., im: 0. },
            C { re: 2., im: 0. },
            C { re: 3., im: 0. },
            C { re: 4., im: 0. },
            C { re: 5., im: 0. },
        ];
        assert_eq!(
            layout
                .field_state(1, &state)
                .unwrap()
                .iter()
                .map(|v| v.re)
                .collect::<Vec<_>>(),
            vec![3., 4., 5.]
        );
        assert!(layout.field_state(1, &state[..4]).is_err());
        assert!(layout.global_dofs(1, &[3]).is_err());
        let test = [BasisSample {
            value: &[0.5],
            gradient: &[],
            curl: &[],
        }];
        let trial = [BasisSample {
            value: &[],
            gradient: &[1., 0., 0., 2.],
            curl: &[],
        }];
        let mut matrix = vec![std::collections::BTreeMap::new(); 2];
        plan.scatter_term(
            0,
            &[0],
            &[1],
            &test,
            &trial,
            &Coefficient::Scalar(C { re: 2., im: 0. }),
            1.,
            &mut matrix,
        )
        .unwrap();
        assert_eq!(matrix[0][&1].re, 3.0);
        let mut operators = plan.empty_operators(&layout);
        plan.assemble_sample(
            &layout,
            0,
            &[0],
            &[0],
            &test,
            &trial,
            &Coefficient::Scalar(C { re: 2., im: 0. }),
            1.,
            &mut operators,
        )
        .unwrap();
        assert_eq!(operators.operator[0][&2].re, 3.0);
        assert_eq!(operators.derivatives.len(), 1);
        let mut workspace = TransientAssemblyWorkspace::default();
        workspace
            .prepare(&plan, &layout, &[vec![0, 1, 2, 3, 4]])
            .unwrap();
        workspace
            .assemble_sample(
                &plan,
                &layout,
                0,
                &[0],
                &[0],
                &test,
                &trial,
                &Coefficient::Scalar(C { re: 2., im: 0. }),
                1.,
            )
            .unwrap();
        assert_eq!(workspace.rows().unwrap().operator[0][&2].re, 3.0);
        workspace
            .prepare(&plan, &layout, &[vec![0, 1, 2, 3, 4]])
            .unwrap();
        workspace
            .assemble_sample(
                &plan,
                &layout,
                0,
                &[0],
                &[0],
                &test,
                &trial,
                &Coefficient::Scalar(C { re: 4., im: 0. }),
                1.,
            )
            .unwrap();
        assert_eq!(workspace.rows().unwrap().operator[0][&2].re, 6.0);
        workspace
            .prepare(&plan, &layout, &[vec![0, 1], vec![2, 3, 4]])
            .unwrap();
        assert!(
            workspace
                .assemble_sample(
                    &plan,
                    &layout,
                    0,
                    &[0],
                    &[0],
                    &test,
                    &trial,
                    &Coefficient::Scalar(C { re: 1., im: 0. }),
                    1.
                )
                .is_err()
        );
        let load_form =
            VolumeLoadPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"test":"value"}))
                .unwrap();
        plan.assemble_load(
            &layout,
            0,
            &[0],
            &test,
            &load_form,
            &[C { re: 4., im: 0. }],
            1.,
            &mut operators,
        )
        .unwrap();
        assert_eq!(operators.load[0].re, 2.0);
        let boundary = crate::boundary_form::BoundaryFormPlan::from_json(
            &json!({"dimension":2,"fieldComponents":1,
            "matrixTerms":[{"test":"value","trial":"value","coefficient":"a"}],
            "loadTerms":[{"test":"value","source":"flux","sourceTrace":"value","coefficient":"b"}],
            "coefficients":{"a":2,"b":3}}),
        )
        .unwrap();
        let coeff = boundary.evaluate_coefficients(&HashMap::new()).unwrap();
        let boundary_basis: [&[f64]; 2] = [&[0.5], &[0.5]];
        let source = HashMap::from([("flux".into(), vec![C { re: 4., im: 0. }])]);
        plan.assemble_boundary_sample(
            &layout,
            0,
            &[0, 1],
            &boundary_basis,
            &[1., 0.],
            &boundary,
            &coeff,
            &source,
            1.,
            None,
            &mut operators,
        )
        .unwrap();
        assert_eq!(operators.operator[0][&1].re, 0.5);
        assert_eq!(operators.load[1].re, 6.0);
        let mixed = crate::boundary_form::CoupledBoundaryForm::from_json(&json!({"dimension":2,
            "testComponents":1,"trialComponents":2,"test":"value","trial":"normal"}))
        .unwrap();
        let mixed_test: &[&[f64]] = &[&[0.5]];
        let mixed_trial: &[&[f64]] = &[&[2., 3.]];
        plan.assemble_coupled_boundary_sample(
            &layout,
            0,
            1,
            &[0],
            &[0],
            mixed_test,
            mixed_trial,
            &[1., 0.],
            &mixed,
            C { re: 2., im: 0. },
            1.,
            None,
            &mut operators,
        )
        .unwrap();
        assert_eq!(operators.operator[0][&2].re, 5.0);
        declaration["terms"][0]["trialField"] = json!("unknown");
        assert!(TransientPlan::from_json(&declaration).is_err());
    }

    #[test]
    fn a_two_dimensional_transient_form_can_declare_a_three_dimensional_curl_operator() {
        let declaration = json!({
            "format":"fullmeta-transient-form/1",
            "dimension":2,
            "operatorDimension":3,
            "fields":[{"id":"electric","components":3},{"id":"magnetic","components":3}],
            "terms":[
                {"testField":"electric","trialField":"magnetic","test":"value","trial":"curl","coefficient":"one","timeDerivative":"none"},
                {"testField":"magnetic","trialField":"electric","test":"value","trial":"curl","coefficient":"one","timeDerivative":"none"}
            ]
        });
        let mut plan = TransientPlan::from_json(&declaration).unwrap();
        assert_eq!(plan.operator_dimension, 3);
        plan.set_longitudinal_wavenumber(C { re: 2., im: 0. }).unwrap();

        let test = [BasisSample { value: &[1., 0., 0.], gradient: &[0.; 6], curl: &[] }];
        let trial = [BasisSample {
            value: &[1., 2., 3.],
            gradient: &[0.1, 0.2, 0.3, 0.4, 0.5, 0.6],
            curl: &[],
        }];
        let mut matrix = vec![std::collections::BTreeMap::new(); 1];
        plan.scatter_term(
            0,
            &[0],
            &[0],
            &test,
            &trial,
            &Coefficient::Scalar(C { re: 1., im: 0. }),
            1.,
            &mut matrix,
        )
        .unwrap();
        // curl(H)_x = d_y H_z - d_z H_y, with d_z = -i kz.
        assert!((matrix[0][&0].re - 0.6).abs() < 1e-12);
        assert!((matrix[0][&0].im - 4.).abs() < 1e-12);
    }

    #[test]
    fn declared_sparse_operator_advances_a_time_dependent_conserved_state() {
        use crate::transient_step::{LinearOptions, integrate_backward_euler};
        let declaration = json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"u","components":1}],"terms":[{"testField":"u","trialField":"u",
                "test":"value","trial":"value","coefficient":"capacity","timeDerivative":"conservedProduct"}]});
        let plan = TransientPlan::from_json(&declaration).unwrap();
        let layout = StateLayout::new(&plan, &[1]).unwrap();
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let result = integrate_backward_euler(
            0.,
            1.,
            0.25,
            10,
            &[C { re: 2., im: 0. }],
            &[1.],
            &options,
            &mut crate::solver_common::SolverCache::default(),
            |time| {
                let basis = [BasisSample {
                    value: &[1.],
                    gradient: &[],
                    curl: &[],
                }];
                let mut operators = plan.empty_operators(&layout);
                plan.assemble_sample(
                    &layout,
                    0,
                    &[0],
                    &[0],
                    &basis,
                    &basis,
                    &Coefficient::Scalar(C {
                        re: 1. + time,
                        im: 0.,
                    }),
                    1.,
                    &mut operators,
                )?;
                Ok(operators)
            },
            |_, _, _| Ok(()),
        )
        .unwrap();
        assert!((result[0].re - 1.).abs() < 1e-10);
    }

    #[test]
    fn coefficient_contract_evaluates_inputs_and_local_tensor() {
        let plan=TransientPlan::from_json(&json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"u","components":1}],"terms":[
                {"testField":"u","trialField":"u","test":"gradient","trial":"gradient","coefficient":"K","timeDerivative":"none"},
                {"testField":"u","trialField":"u","test":"value","trial":"value","coefficient":"capacity","timeDerivative":"state"}]})).unwrap();
        let declaration = json!({"scalars":{"capacity":{"add":[1,{"symbol":"scale"}]},"k":{"symbol":"conductivity"}},
            "matrices":{"K":{"values":[["k",0],[0,"k"]]}}});
        let coefficients = TransientCoefficients::from_json(&plan, &declaration).unwrap();
        let inputs = HashMap::from([
            ("scale".into(), C { re: 2., im: 0. }),
            ("conductivity".into(), C { re: 4., im: 0. }),
        ]);
        let values = coefficients.evaluate(&inputs).unwrap();
        let dimensionless = crate::model_expression::unit("1").unwrap().1;
        let quantities = HashMap::from([
            (
                "scale".into(),
                crate::model_expression::Scalar {
                    value: 2.,
                    dimension: dimensionless,
                },
            ),
            (
                "conductivity".into(),
                crate::model_expression::Scalar {
                    value: 4.,
                    dimension: dimensionless,
                },
            ),
        ]);
        assert!(coefficients.evaluate_quantities(&quantities).is_ok());
        match &values["capacity"] {
            SampledCoefficient::Scalar(value) => assert_eq!(value.re, 3.),
            _ => panic!("expected scalar"),
        }
        match &values["K"] {
            SampledCoefficient::Tensor {
                rows,
                columns,
                values,
            } => {
                assert_eq!((*rows, *columns), (2, 2));
                assert_eq!(values[0].re, 4.);
                assert_eq!(values[3].re, 4.);
            }
            _ => panic!("expected tensor"),
        }
    }

    #[test]
    fn triangle_manufactured_spatial_and_time_dependent_capacity() {
        use crate::model_expression::{self, Scalar};
        use crate::transient_step::{LinearOptions, integrate_backward_euler};
        let declaration = json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"u","components":1}],"terms":[{"testField":"u","trialField":"u",
                "test":"value","trial":"value","coefficient":"capacity","timeDerivative":"conservedProduct"}]});
        let plan = TransientPlan::from_json(&declaration).unwrap();
        let layout = StateLayout::new(&plan, &[3]).unwrap();
        let load_form =
            VolumeLoadPlan::from_json(&json!({"dimension":2,"fieldComponents":1,"test":"value"}))
                .unwrap();
        let capacity = model_expression::CompiledQuantity::compile(
            &json!({"expression":"1+x/L+t/tau","unit":"1"}),
            "capacity",
        )
        .unwrap();
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let result = integrate_backward_euler(
            0.,
            1.,
            0.25,
            10,
            &[C { re: 1., im: 0. }; 3],
            &[1.],
            &options,
            &mut crate::solver_common::SolverCache::default(),
            |time| {
                let mut operators = plan.empty_operators(&layout);
                let points = [
                    [2. / 3., 1. / 6., 1. / 6.],
                    [1. / 6., 2. / 3., 1. / 6.],
                    [1. / 6., 1. / 6., 2. / 3.],
                ];
                for bary in points {
                    // Unit right triangle: x is the second barycentric coordinate.
                    let symbols = std::collections::HashMap::from([
                        (
                            "x".into(),
                            Scalar {
                                value: bary[1],
                                dimension: model_expression::unit("m")?.1,
                            },
                        ),
                        (
                            "L".into(),
                            Scalar {
                                value: 1.,
                                dimension: model_expression::unit("m")?.1,
                            },
                        ),
                        (
                            "t".into(),
                            Scalar {
                                value: time,
                                dimension: model_expression::unit("s")?.1,
                            },
                        ),
                        (
                            "tau".into(),
                            Scalar {
                                value: 1.,
                                dimension: model_expression::unit("s")?.1,
                            },
                        ),
                    ]);
                    let c = capacity.evaluate(&symbols, "capacity")?;
                    let basis_storage = crate::weak_form::NodalP1Samples::new(
                        2,
                        1,
                        &bary,
                        &[vec![-1., -1.], vec![1., 0.], vec![0., 1.]],
                    )?;
                    let basis = basis_storage.samples();
                    plan.assemble_sample(
                        &layout,
                        0,
                        &[0, 1, 2],
                        &[0, 1, 2],
                        &basis,
                        &basis,
                        &Coefficient::Scalar(C { re: c, im: 0. }),
                        1. / 6.,
                        &mut operators,
                    )?;
                    plan.assemble_load(
                        &layout,
                        0,
                        &[0, 1, 2],
                        &basis,
                        &load_form,
                        &[C { re: 1., im: 0. }],
                        1. / 6.,
                        &mut operators,
                    )?;
                }
                Ok(operators)
            },
            |_, _, _| Ok(()),
        )
        .unwrap();
        for value in result {
            assert!((value.re - 1.).abs() < 1e-10);
        }
    }
}
