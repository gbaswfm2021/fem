//! Scenario-neutral prepared vector P1 runner.
//!
//! A model adapter supplies selected mesh entities, sampled material inputs,
//! evaluated boundary points and complete point constraints. Weak forms,
//! matrix coefficients, recovery and output serialization come from the
//! compiled scenario plan. A prepared complex periodic node map is supported;
//! file/model binding and geometric periodic pairing remain external.
use std::collections::HashMap;
use crate::{C, SolverCache};
use crate::model_expression::Scalar;
use crate::scenario_plan::ScenarioPlan;
use crate::vector_p1;
use crate::weak_form::Coefficient;

pub(crate) struct Point<const N: usize> {
    pub(crate) basis: [f64; N],
    pub(crate) weight: f64,
    pub(crate) symbols: HashMap<String, C>,
}
pub(crate) struct Cell<const N: usize> {
    pub(crate) nodes: [usize; N],
    pub(crate) points: Vec<Point<N>>,
    pub(crate) recovery_symbols: HashMap<String, C>,
}
pub(crate) struct OutputMapping {
    pub(crate) vector_components: Vec<String>,
    pub(crate) tensor_matrix: String,
}
pub(crate) struct Result {
    pub(crate) solution: vector_p1::Solution,
    pub(crate) data: crate::result_contract::ResultData,
}

enum Execution {
    Solved(Result),
    Assembled(vector_p1::LinearSystem),
}

/// Bind a material sample with the generic unit/bounds declarations before
/// it is supplied to the numerical runner. Context names (e.g. frequency)
/// are merged by the caller and remain scenario independent.
pub(crate) fn bind_material(plan: &ScenarioPlan, material: &HashMap<String, Scalar>, context: &HashMap<String, Scalar>, path: &str) -> std::result::Result<HashMap<String, C>, String> {
    let binding = plan.material_bindings.as_ref().ok_or("Vector formulation requires materialBindings.")?;
    let mut values: HashMap<String, C> = context.iter().map(|(name, value)| (name.clone(), C { re:value.value, im:0. })).collect();
    values.extend(binding.evaluate(material, context, path)?.into_iter().map(|(name, value)| (name, C { re:value.value, im:0. })));
    Ok(values)
}

/// Evaluate declared tensor/scalar coefficients and integrate the supplied
/// samples through the same sparse vector runner in either dimension.
pub(crate) fn solve<const D: usize, const FC: usize, const N: usize>(
    plan: &ScenarioPlan, nodes: &[[f64; D]], cells: &[Cell<N>],
    boundaries: &[vector_p1::BoundaryPoint<'_, D>], essential: &[vector_p1::EssentialPoint<FC>], periodic: Option<vector_p1::PeriodicMap<'_>>,
    essential_name: &str, mapping: &OutputMapping, mut data: crate::result_contract::ResultData,
    backend: &str, ordering: &str, threads: usize, tolerance: f64, cache: &mut SolverCache,
    loads: &[crate::weak_form::VolumeLoadSamples<'_>],
) -> std::result::Result<Result, String> {
    match execute(plan,nodes,cells,boundaries,essential,periodic,essential_name,mapping,data,
        backend,ordering,threads,tolerance,cache,loads,false)? {
        Execution::Solved(result)=>Ok(result),
        Execution::Assembled(_)=>Err("Prepared vector solve returned an assembly-only result.".into()),
    }
}

/// Assemble sparse master rows using the same declaration and element path as
/// the frequency solve, without entering the linear solver. Propagation-mode
/// adapters use this to construct their sparse operator pencil.
pub(crate) fn assemble<const D: usize, const FC: usize, const N: usize>(
    plan: &ScenarioPlan, nodes: &[[f64; D]], cells: &[Cell<N>],
    boundaries: &[vector_p1::BoundaryPoint<'_, D>], essential: &[vector_p1::EssentialPoint<FC>], periodic: Option<vector_p1::PeriodicMap<'_>>,
    essential_name: &str, mapping: &OutputMapping, data: crate::result_contract::ResultData,
    backend: &str, ordering: &str, threads: usize, tolerance: f64, cache: &mut SolverCache,
    loads: &[crate::weak_form::VolumeLoadSamples<'_>],
) -> std::result::Result<vector_p1::LinearSystem, String> {
    match execute(plan,nodes,cells,boundaries,essential,periodic,essential_name,mapping,data,
        backend,ordering,threads,tolerance,cache,loads,true)? {
        Execution::Assembled(system)=>Ok(system),
        Execution::Solved(_)=>Err("Prepared vector assembly unexpectedly solved the operator.".into()),
    }
}

fn execute<const D: usize, const FC: usize, const N: usize>(
    plan: &ScenarioPlan, nodes: &[[f64; D]], cells: &[Cell<N>],
    boundaries: &[vector_p1::BoundaryPoint<'_, D>], essential: &[vector_p1::EssentialPoint<FC>], periodic: Option<vector_p1::PeriodicMap<'_>>,
    essential_name: &str, mapping: &OutputMapping, mut data: crate::result_contract::ResultData,
    backend: &str, ordering: &str, threads: usize, tolerance: f64, cache: &mut SolverCache,
    loads: &[crate::weak_form::VolumeLoadSamples<'_>], assemble_only: bool,
) -> std::result::Result<Execution, String> {
    if mapping.vector_components.len() != FC || mapping.tensor_matrix.is_empty() {
        return Err("Vector output mapping does not match the field dimension.".into());
    }
    let recovery = plan.tensor_recovery.as_ref().ok_or("Vector formulation requires tensor recovery.")?;
    recovery.require_shape(D, FC)?;
    recovery.require_policy("cellCentroid", "cellAndNodalArithmeticMean")?;
    let essential_plan = plan.essential.get(essential_name).ok_or("Vector formulation lacks the selected essential form.")?;
    // Retain owned matrices while quadrature points borrow them.
    let mut evaluated = Vec::with_capacity(cells.len());
    for cell in cells {
        let mut points = Vec::with_capacity(cell.points.len());
        for point in &cell.points {
            let scalars = plan.volume.evaluate_coefficients(&point.symbols)?;
            let matrices = plan.coefficient_matrices.iter().map(|(name, matrix)| Ok((name.clone(), matrix.evaluate(&scalars)?)))
                .collect::<std::result::Result<HashMap<_,_>,String>>()?;
            points.push((scalars, matrices));
        }
        let coefficients = plan.volume.evaluate_coefficients(&cell.recovery_symbols)?;
        let mut recovery_symbols = cell.recovery_symbols.clone();
        recovery_symbols.extend(coefficients);
        evaluated.push((points, recovery_symbols));
    }
    let mut numerical = Vec::with_capacity(cells.len());
    for (cell, (samples, recovery_symbols)) in cells.iter().zip(&evaluated) {
        let mut quadrature = Vec::with_capacity(samples.len());
        for (point, (scalars, matrices)) in cell.points.iter().zip(samples) {
            let mut coefficients: HashMap<String, Coefficient<'_>> = scalars.iter().map(|(name, &value)| (name.clone(), Coefficient::Scalar(value))).collect();
            for (name, values) in matrices {
                let (rows, columns) = plan.coefficient_matrices[name].shape();
                coefficients.insert(name.clone(), Coefficient::Matrix { rows, columns, values });
            }
            quadrature.push(vector_p1::VolumePoint { basis:point.basis, weight:point.weight, coefficients });
        }
        numerical.push(vector_p1::Cell { nodes:cell.nodes, quadrature, recovery_symbols:recovery_symbols.clone() });
    }
    let problem = vector_p1::Problem { nodes, cells:&numerical, form:&plan.volume, boundaries, essential, essential_plan, recovery, periodic };
    if assemble_only {
        return Ok(Execution::Assembled(problem.assemble_with_volume_load_routes(&mut cache.assembly,loads)?));
    }
    let solution = problem.solve_with_volume_load_routes(backend, ordering, threads, tolerance, cache, loads)?;
    for (axis, name) in mapping.vector_components.iter().enumerate() {
        if name.is_empty() || data.series.contains_key(name) { return Err("Vector output component names must be nonempty and unique.".into()); }
        data.series.insert(name.clone(), (0..nodes.len()).map(|node| C { re:solution.real[FC*node+axis], im:solution.imaginary[FC*node+axis] }).collect());
    }
    let labels = recovery.components();
    let mut sums = vec![vec![C::default(); labels.len()]; nodes.len()];
    let mut counts = vec![0usize; nodes.len()];
    for (cell, values) in cells.iter().zip(&solution.recovered_cells) {
        if values.len() != labels.len() { return Err("Recovered tensor component count is inconsistent.".into()); }
        for &node in &cell.nodes {
            counts[node] += 1;
            for (sum, value) in sums[node].iter_mut().zip(values) { sum.re += value.re; sum.im += value.im; }
        }
    }
    for (axis, name) in labels.iter().enumerate() {
        if data.series.contains_key(name) { return Err("Tensor and vector output names overlap.".into()); }
        data.series.insert(name.clone(), sums.iter().zip(&counts).map(|(values, &count)| if count == 0 { C::default() } else { C { re:values[axis].re / count as f64, im:values[axis].im / count as f64 } }).collect());
    }
    if data.matrices.insert(mapping.tensor_matrix.clone(), solution.recovered_cells.clone()).is_some() { return Err("Duplicate tensor matrix output name.".into()); }
    Ok(Execution::Solved(Result { solution, data }))
}


