//! Compile the numerical declarations for one selected scenario formulation.
//!
//! This aggregates generic plans without selecting a scenario-specific runner;
//! the shared execution paths bind model data to these plans.
use crate::boundary_form::BoundaryFormPlan;
use crate::coefficient_expr::CoefficientExpressions;
use crate::coefficient_matrix::CoefficientMatrixPlan;
use crate::essential_form::EssentialFormPlan;
use crate::material_bindings::MaterialBindings;
use crate::nullspace::MeanConstraintPlan;
use crate::recovery::RecoveryPlan;
use crate::result_contract::ResultPlan;
use crate::selection_plan::SelectionPlan;
use crate::tensor_recovery::TensorRecoveryPlan;
use crate::weak_form::FormPlan;
use serde_json::Value;
use std::collections::HashMap;

fn formulation_entry<'a>(
    package: &'a Value,
    section: &str,
    formulation: &str,
) -> Result<Option<&'a Value>, String> {
    match package.get(section) {
        None => Ok(None),
        Some(value) => Ok(value
            .as_object()
            .ok_or_else(|| format!("{section} must be an object indexed by formulation."))?
            .get(formulation)),
    }
}

pub(crate) struct ScenarioPlan {
    pub(crate) volume: FormPlan,
    pub(crate) boundaries: HashMap<String, BoundaryFormPlan>,
    pub(crate) essential: HashMap<String, EssentialFormPlan>,
    pub(crate) recovery: Option<RecoveryPlan>,
    pub(crate) result: Option<ResultPlan>,
    pub(crate) tensor_recovery: Option<TensorRecoveryPlan>,
    pub(crate) nullspace: Option<MeanConstraintPlan>,
    pub(crate) material_coefficients: Option<CoefficientExpressions>,
    pub(crate) material_bindings: Option<MaterialBindings>,
    pub(crate) coefficient_matrices: HashMap<String, CoefficientMatrixPlan>,
    pub(crate) selections: Option<SelectionPlan>,
    pub(crate) domain_source: Option<crate::domain_source::DomainSourcePlan>,
}

impl ScenarioPlan {
    pub(crate) fn load(profile: &str, formulation: &str) -> Result<Self, String> {
        let package = crate::scenario_package::load_installed(profile)?;
        Self::from_package(&package, formulation)
            .map_err(|error| format!("Scenario {profile:?} formulation {formulation:?}: {error}"))
    }

    pub(crate) fn from_package(package: &Value, formulation: &str) -> Result<Self, String> {
        let weak = package
            .get("weakForms")
            .and_then(|v| v.get(formulation))
            .ok_or_else(|| format!("Missing weak form for {formulation:?}."))?;
        let volume = FormPlan::from_json(weak)?;
        if !volume.has_discretization() {
            return Err(format!("weakForms.{formulation}.element is required."));
        }
        let (dimension, components) = volume.shape();
        let domain_source = formulation_entry(package, "domainSources", formulation)?
            .map(|value| crate::domain_source::DomainSourcePlan::from_json(value, dimension, components))
            .transpose()?;
        let mut coefficient_matrices = HashMap::new();
        if let Some(matrices) = weak.get("matrixCoefficients") {
            for (name, value) in matrices
                .as_object()
                .ok_or("matrixCoefficients must be an object.")?
            {
                let matrix = CoefficientMatrixPlan::from_json(value).map_err(|error| {
                    format!("weakForms.{formulation}.matrixCoefficients.{name}: {error}")
                })?;
                let (rows, columns) = matrix.shape();
                volume
                    .require_matrix_coefficient_shape(name, rows, columns)
                    .map_err(|error| {
                        format!("weakForms.{formulation}.matrixCoefficients.{name}: {error}")
                    })?;
                for symbol in matrix.symbols() {
                    if !volume.declares_coefficient(symbol) {
                        return Err(format!(
                            "weakForms.{formulation}.matrixCoefficients.{name} references undeclared coefficient {symbol:?}."
                        ));
                    }
                }
                coefficient_matrices.insert(name.clone(), matrix);
            }
        }
        for term in volume.terms() {
            if !volume.declares_coefficient(&term.coefficient)
                && !coefficient_matrices.contains_key(&term.coefficient)
            {
                return Err(format!(
                    "weakForms.{formulation} uses undeclared coefficient {:?}.",
                    term.coefficient
                ));
            }
            if volume.declares_coefficient(&term.coefficient)
                && coefficient_matrices.contains_key(&term.coefficient)
            {
                return Err(format!(
                    "weakForms.{formulation} declares coefficient {:?} as both scalar and matrix.",
                    term.coefficient
                ));
            }
        }
        let boundary_forms = formulation_entry(package, "boundaryForms", formulation)?;
        let mut boundaries = HashMap::new();
        if let Some(value) = boundary_forms {
            for (name, form) in value
                .as_object()
                .ok_or("boundaryForms formulation must be an object.")?
            {
                let compiled = BoundaryFormPlan::from_json(form)
                    .map_err(|error| format!("boundaryForms.{formulation}.{name}: {error}"))?;
                compiled
                    .require_shape(dimension, components)
                    .map_err(|error| format!("boundaryForms.{formulation}.{name}: {error}"))?;
                boundaries.insert(name.clone(), compiled);
            }
        }
        let essential_forms = formulation_entry(package, "essentialForms", formulation)?;
        let mut essential = HashMap::new();
        if let Some(value) = essential_forms {
            for (name, form) in value
                .as_object()
                .ok_or("essentialForms formulation must be an object.")?
            {
                let compiled = EssentialFormPlan::from_json(form)
                    .map_err(|error| format!("essentialForms.{formulation}.{name}: {error}"))?;
                compiled
                    .require_shape(dimension, components)
                    .map_err(|error| format!("essentialForms.{formulation}.{name}: {error}"))?;
                essential.insert(name.clone(), compiled);
            }
        }
        let recovery = formulation_entry(package, "recoveries", formulation)?
            .map(|value| {
                let plan = RecoveryPlan::from_json(value)
                    .map_err(|error| format!("recoveries.{formulation}: {error}"))?;
                plan.require_shape(dimension, components)
                    .map_err(|error| format!("recoveries.{formulation}: {error}"))?;
                Ok::<_, String>(plan)
            })
            .transpose()?;
        let result = formulation_entry(package, "resultPlans", formulation)?
            .map(ResultPlan::from_json)
            .transpose()
            .map_err(|error| format!("resultPlans.{formulation}: {error}"))?;
        let tensor_recovery = formulation_entry(package, "tensorRecoveries", formulation)?
            .map(|value| {
                let plan = TensorRecoveryPlan::from_json(value)
                    .map_err(|error| format!("tensorRecoveries.{formulation}: {error}"))?;
                plan.require_shape(dimension, components)
                    .map_err(|error| format!("tensorRecoveries.{formulation}: {error}"))?;
                Ok::<_, String>(plan)
            })
            .transpose()?;
        let nullspace = formulation_entry(package, "nullspaces", formulation)?
            .map(|value| {
                MeanConstraintPlan::from_json(value)
                    .map_err(|error| format!("nullspaces.{formulation}: {error}"))
            })
            .transpose()?;
        let material_coefficients = package
            .get("materialCoefficients")
            .map(CoefficientExpressions::from_json)
            .transpose()?;
        let material_bindings = formulation_entry(package, "materialBindings", formulation)?
            .map(|value| {
                MaterialBindings::from_json(value)
                    .map_err(|error| format!("materialBindings.{formulation}: {error}"))
            })
            .transpose()?;
        let selections = package.get("selectionPlan")
            .map(SelectionPlan::from_json)
            .transpose()
            .map_err(|error| format!("selectionPlan: {error}"))?;
        Ok(Self {
            volume,
            boundaries,
            essential,
            recovery,
            result,
            tensor_recovery,
            nullspace,
            material_coefficients,
            material_bindings,
            coefficient_matrices,
            selections,
            domain_source,
        })
    }
}


