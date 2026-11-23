//! Scenario-neutral in-memory transient execution. The application adapter
//! supplies model selections, boundary laws, artifacts and lifecycle events.
use crate::model_expression::Scalar;
use crate::solver_common::{C, SolverCache};
use crate::transient_initial::InitialField;
use crate::transient_mesh::{self, NodalMesh};
use crate::transient_plan::{
    SampleLocation, SampledCoefficient, StateLayout, TransientAssemblyWorkspace, TransientPlan,
    TransientSource,
};

/// Oriented element spaces share the same sparse time integrator, state
/// layout, source routing, and initial projection workflow. An element adapter
/// supplies only its quadrature and basis assembly.
pub(crate) trait OrientedTransientSpace {
    fn dof_count(&self) -> usize;
    fn initial_projection(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        start: f64,
        sample: &mut dyn FnMut(&str, [f64; 3], f64) -> Result<Vec<C>, String>,
    ) -> Result<(crate::transient_step::SparseMatrix, Vec<C>), String>;
    fn assemble(
        &self,
        plan: &TransientPlan,
        layout: &StateLayout,
        time: f64,
        quadrature: &str,
        workspace: &mut TransientAssemblyWorkspace,
        coefficient: &mut dyn FnMut(SampleLocation) -> Result<HashMap<String, SampledCoefficient>, String>,
        sources: &[TransientSource<'_>],
        source_value: &mut dyn FnMut(&str, SampleLocation) -> Result<Vec<C>, String>,
    ) -> Result<TransientOperators, String>;
}

impl OrientedTransientSpace for crate::transient_edge::EdgeTransientMesh {
    fn dof_count(&self) -> usize { self.dof_count() }
    fn initial_projection(&self, plan:&TransientPlan, layout:&StateLayout, start:f64,
        sample:&mut dyn FnMut(&str,[f64;3],f64)->Result<Vec<C>,String>)
        ->Result<(crate::transient_step::SparseMatrix,Vec<C>),String>{
        crate::transient_edge::EdgeTransientMesh::initial_projection(self,plan,layout,start,sample)
    }
    fn assemble(&self,plan:&TransientPlan,layout:&StateLayout,time:f64,quadrature:&str,workspace:&mut TransientAssemblyWorkspace,
        coefficient:&mut dyn FnMut(SampleLocation)->Result<HashMap<String,SampledCoefficient>,String>,sources:&[TransientSource<'_>],
        source_value:&mut dyn FnMut(&str,SampleLocation)->Result<Vec<C>,String>)->Result<TransientOperators,String>{
        crate::transient_edge::EdgeTransientMesh::assemble(self,plan,layout,time,quadrature,workspace,coefficient,sources,source_value)
    }
}

impl OrientedTransientSpace for crate::transient_triangle::TriangleTransientMesh {
    fn dof_count(&self) -> usize { self.dof_count() }
    fn initial_projection(&self, plan:&TransientPlan, layout:&StateLayout, start:f64,
        sample:&mut dyn FnMut(&str,[f64;3],f64)->Result<Vec<C>,String>)
        ->Result<(crate::transient_step::SparseMatrix,Vec<C>),String>{
        crate::transient_triangle::TriangleTransientMesh::initial_projection(self,plan,layout,start,sample)
    }
    fn assemble(&self,plan:&TransientPlan,layout:&StateLayout,time:f64,quadrature:&str,workspace:&mut TransientAssemblyWorkspace,
        coefficient:&mut dyn FnMut(SampleLocation)->Result<HashMap<String,SampledCoefficient>,String>,sources:&[TransientSource<'_>],
        source_value:&mut dyn FnMut(&str,SampleLocation)->Result<Vec<C>,String>)->Result<TransientOperators,String>{
        crate::transient_triangle::TriangleTransientMesh::assemble(self,plan,layout,time,quadrature,workspace,coefficient,sources,source_value)
    }
}
use crate::transient_request::TimeGrid;
use crate::transient_step::{self, FixedDofMap, LinearOptions, TransientOperators};

/// Add physical essential values to a declared field, merging compatible
/// repeats from adjacent boundary entities and rejecting incompatible ones.
/// The common fixed-map reduction handles conflicts across periodic classes.
pub(crate) fn add_essential_values(
    plan: &TransientPlan,
    layout: &StateLayout,
    field_id: &str,
    values: &[(usize, C)],
    operators: &mut TransientOperators,
) -> Result<(), String> {
    let field = plan
        .fields
        .iter()
        .position(|field| field.id == field_id)
        .ok_or_else(|| format!("Essential condition names undeclared field {field_id:?}."))?;
    let dofs = layout.global_dofs(
        field,
        &values.iter().map(|(dof, _)| *dof).collect::<Vec<_>>(),
    )?;
    let mut merged = std::collections::BTreeMap::<usize, C>::new();
    for &(dof, value) in &operators.essential {
        if dof >= layout.dimension
            || !value.re.is_finite()
            || !value.im.is_finite()
            || merged.insert(dof, value).is_some()
        {
            return Err("Existing transient essential values are invalid or duplicated.".into());
        }
    }
    for (dof, (_, value)) in dofs.into_iter().zip(values) {
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err("Transient essential values must be finite.".into());
        }
        if let Some(previous) = merged.get(&dof) {
            let difference = (previous.re - value.re).hypot(previous.im - value.im);
            if difference
                > 1e-10 * (1. + previous.re.hypot(previous.im).max(value.re.hypot(value.im)))
            {
                return Err(format!(
                    "Conflicting essential values at transient DOF {dof}."
                ));
            }
        } else {
            merged.insert(dof, *value);
        }
    }
    operators.essential = merged.into_iter().collect();
    Ok(())
}

/// Declare a discrete time derivative between corresponding field DOFs.
/// Constraint reduction accounts for the weights of both fields.
pub(crate) fn add_essential_rates(
    plan: &TransientPlan,
    layout: &StateLayout,
    source: &str,
    target: &str,
    local: &[usize],
    operators: &mut TransientOperators,
) -> Result<(), String> {
    let index = |id: &str| {
        plan.fields
            .iter()
            .position(|field| field.id == id)
            .ok_or_else(|| format!("Rate constraint names undeclared field {id:?}."))
    };
    let source = layout.global_dofs(index(source)?, local)?;
    let target = layout.global_dofs(index(target)?, local)?;
    let mut rates = operators
        .essential_rates
        .iter()
        .map(|rate| (rate.target, *rate))
        .collect::<std::collections::BTreeMap<_, _>>();
    if rates.len() != operators.essential_rates.len() {
        return Err("Duplicate existing transient rate constraint.".into());
    }
    for (source, target) in source.into_iter().zip(target) {
        let rate = transient_step::EssentialRate {
            source,
            target,
            factor: C { re: 1., im: 0. },
        };
        if let Some(previous) = rates.get(&target) {
            if previous.source != source || previous.factor.re != 1. || previous.factor.im != 0. {
                return Err("Conflicting transient rate constraints.".into());
            }
        } else {
            rates.insert(target, rate);
        }
    }
    operators.essential_rates = rates.into_values().collect();
    Ok(())
}
use std::collections::HashMap;

/// Run a shared P1 nodal-space state. The callbacks express only declared
/// coefficients, sources, boundaries and output; no scenario is selected here.
pub(crate) fn run_nodal(
    plan: &TransientPlan,
    layout: &StateLayout,
    mesh: &NodalMesh<'_>,
    grid: &TimeGrid,
    quadrature: &str,
    parameters: &HashMap<String, Scalar>,
    initial_fields: &HashMap<String, InitialField>,
    mapping: Option<&FixedDofMap<'_>>,
    workspace: &mut TransientAssemblyWorkspace,
    options: &LinearOptions<'_>,
    cache: &mut SolverCache,
    mut coefficient: impl FnMut(SampleLocation) -> Result<HashMap<String, SampledCoefficient>, String>,
    sources: &[TransientSource<'_>],
    mut source_value: impl FnMut(&str, SampleLocation) -> Result<Vec<C>, String>,
    mut boundary: impl FnMut(f64, &mut TransientOperators) -> Result<(), String>,
    observe: impl FnMut(f64, &[C], bool) -> Result<(), String>,
) -> Result<Vec<C>, String> {
    let initial = crate::transient_initial::nodal_state(
        plan,
        layout,
        initial_fields,
        mesh.nodes,
        parameters,
        grid.start,
    )?;
    transient_step::integrate_with_constraints(
        grid,
        &initial,
        mapping,
        options,
        cache,
        |time| {
            let mut operators = transient_mesh::assemble_nodal_p1(
                plan,
                layout,
                mesh,
                time,
                quadrature,
                workspace,
                &mut coefficient,
                sources,
                &mut source_value,
            )?;
            boundary(time, &mut operators)?;
            Ok(operators)
        },
        observe,
    )
}

/// Run fields in a common oriented tetrahedral edge space. Explicit initial
/// expressions are projected using a sparse mass solve; fixed identifications
/// and prescribed initial boundary values are applied to that projection.
pub(crate) fn run_edge(
    plan: &TransientPlan,
    layout: &StateLayout,
    mesh: &crate::transient_edge::EdgeTransientMesh,
    grid: &TimeGrid,
    quadrature: &str,
    parameters: &HashMap<String, Scalar>,
    initial_fields: &HashMap<String, InitialField>,
    mapping: Option<&FixedDofMap<'_>>,
    workspace: &mut TransientAssemblyWorkspace,
    options: &LinearOptions<'_>,
    cache: &mut SolverCache,
    mut coefficient: impl FnMut(SampleLocation) -> Result<HashMap<String, SampledCoefficient>, String>,
    sources: &[TransientSource<'_>],
    mut source_value: impl FnMut(&str, SampleLocation) -> Result<Vec<C>, String>,
    mut boundary: impl FnMut(f64, &mut TransientOperators) -> Result<(), String>,
    observe: impl FnMut(f64, &[C], bool) -> Result<(), String>,
) -> Result<Vec<C>, String> {
    run_oriented(plan,layout,mesh,grid,quadrature,parameters,initial_fields,mapping,workspace,options,cache,
        coefficient,sources,source_value,boundary,observe)
}

/// Integrate any contract-selected oriented element space through the shared
/// sparse transient workflow.
pub(crate) fn run_oriented(
    plan: &TransientPlan,
    layout: &StateLayout,
    mesh: &impl OrientedTransientSpace,
    grid: &TimeGrid,
    quadrature: &str,
    parameters: &HashMap<String, Scalar>,
    initial_fields: &HashMap<String, InitialField>,
    mapping: Option<&FixedDofMap<'_>>,
    workspace: &mut TransientAssemblyWorkspace,
    options: &LinearOptions<'_>,
    cache: &mut SolverCache,
    mut coefficient: impl FnMut(SampleLocation) -> Result<HashMap<String, SampledCoefficient>, String>,
    sources: &[TransientSource<'_>],
    mut source_value: impl FnMut(&str, SampleLocation) -> Result<Vec<C>, String>,
    mut boundary: impl FnMut(f64, &mut TransientOperators) -> Result<(), String>,
    observe: impl FnMut(f64, &[C], bool) -> Result<(), String>,
) -> Result<Vec<C>, String> {
    if initial_fields
        .keys()
        .any(|id| !plan.fields.iter().any(|field| &field.id == id))
    {
        return Err("Initial state names an undeclared field.".into());
    }
    let initial = if initial_fields.is_empty() {
        vec![C::default(); layout.dimension]
    } else {
        let mut sample = |id: &str, point: [f64;3], time: f64| {
                crate::transient_initial::sample_field(
                    plan,
                    initial_fields,
                    id,
                    parameters,
                    point,
                    time,
                )
            };
        let (mass, load) = mesh.initial_projection(plan, layout, grid.start, &mut sample)?;
        let mut at_start = mesh.assemble(
            plan,
            layout,
            grid.start,
            quadrature,
            workspace,
            &mut coefficient,
            sources,
            &mut source_value,
        )?;
        boundary(grid.start, &mut at_start)?;
        let projection = TransientOperators {
            derivatives: vec![],
            operator: mass,
            load,
            essential: at_start.essential,
            essential_rates: vec![],
        };
        let projection = if let Some(map) = mapping {
            transient_step::reduce_operators(
                &projection,
                map.representative,
                map.weights,
                map.count,
            )?
        } else {
            projection
        };
        let reduced = transient_step::project_initial_state(
            &projection.operator,
            &projection.load,
            &projection.essential,
            options,
            cache,
        )?;
        if let Some(map) = mapping {
            let re = reduced.iter().map(|v| v.re).collect::<Vec<_>>();
            let im = reduced.iter().map(|v| v.im).collect::<Vec<_>>();
            let (re, im) = crate::constraints::expand_weighted(
                &re,
                &im,
                map.representative,
                map.weights,
                map.count,
            )?;
            re.into_iter()
                .zip(im)
                .map(|(re, im)| C { re, im })
                .collect()
        } else {
            reduced
        }
    };
    transient_step::integrate_with_constraints(
        grid,
        &initial,
        mapping,
        options,
        cache,
        |time| {
            let mut operators = mesh.assemble(
                plan,
                layout,
                time,
                quadrature,
                workspace,
                &mut coefficient,
                sources,
                &mut source_value,
            )?;
            boundary(time, &mut operators)?;
            Ok(operators)
        },
        observe,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn essential_values_merge_shared_facets_and_reject_conflicts() {
        let plan=TransientPlan::from_json(&json!({"format":"fullmeta-transient-form/1","dimension":2,
            "fields":[{"id":"p","components":1},{"id":"v","components":2}],
            "terms":[{"testField":"p","trialField":"p","test":"value","trial":"value","coefficient":"one","timeDerivative":"state"},
                {"testField":"v","trialField":"v","test":"value","trial":"value","coefficient":"one","timeDerivative":"state"}]})).unwrap();
        let layout = StateLayout::new(&plan, &[2, 4]).unwrap();
        let mut operators = TransientOperators {
            derivatives: vec![],
            operator: vec![Default::default(); 6],
            load: vec![C::default(); 6],
            essential: vec![],
            essential_rates: vec![],
        };
        let one = C { re: 1., im: 0. };
        add_essential_values(&plan, &layout, "v", &[(0, one), (0, one)], &mut operators).unwrap();
        assert_eq!(operators.essential[0].0, 2);
        assert!(
            add_essential_values(
                &plan,
                &layout,
                "v",
                &[(0, C { re: 2., im: 0. })],
                &mut operators
            )
            .is_err()
        );
        assert_eq!(operators.essential[0].1.re, 1.);
    }
}
