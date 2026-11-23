//! Sparse backward-Euler primitive for declared first-order transient systems.
//!
//! This is not a Study route. The caller must assemble physically correct
//! operators, apply constraints and choose the derivative law explicitly.
use crate::solver_common::{C, SolverCache, c_add, c_mul, c_scale, solve_linear_system};
use std::collections::BTreeMap;

pub(crate) type SparseMatrix = Vec<BTreeMap<usize, C>>;

/// Distinguishes M(t) u'(t) from (M(t) u(t))'. These are different when M varies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimeDerivative {
    State,
    ConservedProduct,
}

/// One independently declared derivative term, permitting multiple terms with
/// different constitutive derivative laws in the same equation row.
pub(crate) struct DerivativeTerm<'a> {
    pub(crate) previous: &'a [BTreeMap<usize, C>],
    pub(crate) current: &'a [BTreeMap<usize, C>],
    pub(crate) derivative: TimeDerivative,
}

pub(crate) fn backward_euler_terms(
    terms: &[DerivativeTerm<'_>],
    operator: &[BTreeMap<usize, C>],
    previous_state: &[C],
    load: &[C],
    dt: f64,
) -> Result<(SparseMatrix, Vec<C>), String> {
    let n = previous_state.len();
    if n == 0 || load.len() != n || operator.len() != n || !dt.is_finite() || dt <= 0.0 {
        return Err(
            "Transient step requires matching nonempty dimensions and a finite positive time step."
                .into(),
        );
    }
    let finite = |z: &C| z.re.is_finite() && z.im.is_finite();
    if previous_state.iter().chain(load).any(|v| !finite(v)) {
        return Err("Transient state and load must be finite.".into());
    }
    let valid_matrix = |m: &[BTreeMap<usize, C>]| {
        m.len() == n
            && m.iter()
                .all(|row| row.iter().all(|(&j, v)| j < n && finite(v)))
    };
    if !valid_matrix(operator)
        || terms
            .iter()
            .any(|term| !valid_matrix(term.previous) || !valid_matrix(term.current))
    {
        return Err("Transient operators must have matching dimensions and finite entries.".into());
    }
    let inv_dt = 1.0 / dt;
    if !inv_dt.is_finite() {
        return Err("Transient time step is too small.".into());
    }
    let mut matrix = operator.to_vec();
    let mut rhs = load.to_vec();
    for term in terms {
        let old = match term.derivative {
            TimeDerivative::State => term.current,
            TimeDerivative::ConservedProduct => term.previous,
        };
        for i in 0..n {
            for (&j, &value) in &term.current[i] {
                let entry = matrix[i].entry(j).or_default();
                *entry = c_add(*entry, c_scale(value, inv_dt));
            }
            for (&j, &value) in &old[i] {
                rhs[i] = c_add(rhs[i], c_scale(c_mul(value, previous_state[j]), inv_dt));
            }
        }
    }
    if rhs.iter().any(|v| !finite(v)) || matrix.iter().any(|row| row.values().any(|v| !finite(v))) {
        return Err("Transient effective system contains a non-finite value.".into());
    }
    Ok((matrix, rhs))
}

pub(crate) struct LinearOptions<'a> {
    pub(crate) tolerance: f64,
    pub(crate) backend: &'a str,
    pub(crate) ordering: &'a str,
    pub(crate) threads: usize,
}

/// Project an assembled initial field by solving its sparse mass system. The
/// caller assembles the projection load in the chosen field space and reduces
/// periodic constraints before calling this function.
pub(crate) fn project_initial_state(
    mass: &[BTreeMap<usize, C>],
    load: &[C],
    essential: &[(usize, C)],
    options: &LinearOptions<'_>,
    cache: &mut SolverCache,
) -> Result<Vec<C>, String> {
    if mass.is_empty() || mass.len() != load.len() {
        return Err("Initial projection matrix and load dimensions do not match.".into());
    }
    let prescribed = essential_map(essential, load.len())?;
    let mut matrix = mass.to_vec();
    let mut re: Vec<_> = load.iter().map(|v| v.re).collect();
    let mut im: Vec<_> = load.iter().map(|v| v.im).collect();
    crate::constraints::apply_essential(&mut matrix, &mut re, &mut im, &prescribed)?;
    let (re, im, _) = solve_linear_system(
        &matrix,
        &re,
        &im,
        options.tolerance,
        options.backend,
        options.ordering,
        options.threads,
        cache,
    )?;
    Ok(re
        .into_iter()
        .zip(im)
        .map(|(re, im)| C { re, im })
        .collect())
}

/// A numerical snapshot supplied by a scenario-neutral operator assembler.
pub(crate) struct TransientOperators {
    /// Stable declared order is required at every time sample.
    pub(crate) derivatives: Vec<(TimeDerivative, SparseMatrix)>,
    pub(crate) operator: SparseMatrix,
    pub(crate) load: Vec<C>,
    /// Prescribed values in the already-reduced global DOF space at this time.
    pub(crate) essential: Vec<(usize, C)>,
    /// At each step target = factor * d(source)/dt using the selected
    /// integrator. The source must itself have a prescribed value.
    pub(crate) essential_rates: Vec<EssentialRate>,
}

#[derive(Clone, Copy)]
pub(crate) struct EssentialRate {
    pub(crate) source: usize,
    pub(crate) target: usize,
    pub(crate) factor: C,
}

/// Apply the same fixed full-state constraint map to every transient block.
/// Essential values are mapped as physical values, with conflicts diagnosed
/// before a step is solved. Time-dependent maps require additional terms and
/// therefore must not be supplied to this fixed-map operation.
pub(crate) fn reduce_operators(
    operators: &TransientOperators,
    representative: &[usize],
    weights: &[C],
    count: usize,
) -> Result<TransientOperators, String> {
    let n = operators.load.len();
    if representative.len() != n
        || weights.len() != n
        || count == 0
        || count > n
        || weights
            .iter()
            .any(|w| !w.re.is_finite() || !w.im.is_finite() || w.re.hypot(w.im) == 0.)
    {
        return Err(
            "Transient constraint map must cover each DOF with a finite nonzero weight.".into(),
        );
    }
    let mut covered = vec![false; count];
    for &index in representative {
        *covered
            .get_mut(index)
            .ok_or("Transient constraint representative is out of range.")? = true;
    }
    if covered.iter().any(|v| !v) {
        return Err("Transient constraint map has an unused representative.".into());
    }
    let re: Vec<_> = operators.load.iter().map(|v| v.re).collect();
    let im: Vec<_> = operators.load.iter().map(|v| v.im).collect();
    let (operator, re, im) = crate::constraints::reduce_weighted(
        &operators.operator,
        &re,
        &im,
        representative,
        weights,
        count,
    )?;
    let mut derivatives = Vec::with_capacity(operators.derivatives.len());
    for (law, matrix) in &operators.derivatives {
        let (matrix, _, _) = crate::constraints::reduce_weighted(
            matrix,
            &vec![0.; n],
            &vec![0.; n],
            representative,
            weights,
            count,
        )?;
        derivatives.push((*law, matrix));
    }
    let mut prescribed = BTreeMap::<usize, C>::new();
    for &(raw, value) in &operators.essential {
        let (&index, weight) = representative
            .get(raw)
            .zip(weights.get(raw))
            .ok_or("Transient prescribed DOF is outside the constraint map.")?;
        let scale = weight.re.hypot(weight.im);
        let real = weight.re / scale;
        let imag = weight.im / scale;
        let mapped = C {
            re: (value.re * real + value.im * imag) / scale,
            im: (value.im * real - value.re * imag) / scale,
        };
        if !mapped.re.is_finite() || !mapped.im.is_finite() {
            return Err(
                "Transient prescribed value is non-finite after constraint mapping.".into(),
            );
        }
        if let Some(existing) = prescribed.insert(index, mapped) {
            let distance = (existing.re - mapped.re).hypot(existing.im - mapped.im);
            if distance
                > 1e-10
                    * (1.
                        + existing
                            .re
                            .hypot(existing.im)
                            .max(mapped.re.hypot(mapped.im)))
            {
                return Err("Transient prescribed values conflict on identified DOFs.".into());
            }
        }
    }
    let mut rates = BTreeMap::<usize, EssentialRate>::new();
    for rate in &operators.essential_rates {
        let (&source, source_weight) = representative
            .get(rate.source)
            .zip(weights.get(rate.source))
            .ok_or("Transient rate source is outside the constraint map.")?;
        let (&target, target_weight) = representative
            .get(rate.target)
            .zip(weights.get(rate.target))
            .ok_or("Transient rate target is outside the constraint map.")?;
        let numerator = c_mul(rate.factor, *source_weight);
        let magnitude = target_weight.re.hypot(target_weight.im);
        let re = target_weight.re / magnitude;
        let im = target_weight.im / magnitude;
        let factor = C {
            re: (numerator.re * re + numerator.im * im) / magnitude,
            im: (numerator.im * re - numerator.re * im) / magnitude,
        };
        if source == target || !factor.re.is_finite() || !factor.im.is_finite() {
            return Err(
                "Transient rate constraint has invalid reduced degrees of freedom or factor."
                    .into(),
            );
        }
        let mapped = EssentialRate {
            source,
            target,
            factor,
        };
        if let Some(previous) = rates.insert(target, mapped) {
            let diff = (previous.factor.re - factor.re).hypot(previous.factor.im - factor.im);
            if previous.source != source || diff > 1e-10 * (1. + factor.re.hypot(factor.im)) {
                return Err(
                    "Transient rate constraints conflict on identified degrees of freedom.".into(),
                );
            }
        }
    }
    Ok(TransientOperators {
        derivatives,
        operator,
        load: re
            .into_iter()
            .zip(im)
            .map(|(re, im)| C { re, im })
            .collect(),
        essential: prescribed.into_iter().collect(),
        essential_rates: rates.into_values().collect(),
    })
}

/// A fixed raw-to-representative map shared by every field in a transient
/// state. The raw arrays contain one entry per global DOF, including mixed
/// fields, and may encode orientation as well as periodic identification.
pub(crate) struct FixedDofMap<'a> {
    pub(crate) representative: &'a [usize],
    pub(crate) weights: &'a [C],
    pub(crate) count: usize,
}

/// Integrate in the reduced space and present physical full-state snapshots
/// to the observer. The assembler supplies a full-state sparse system at
/// every stage; the same fixed map reduces all derivative and spatial blocks.
pub(crate) fn integrate_with_constraints(
    grid: &crate::transient_request::TimeGrid,
    initial: &[C],
    mapping: Option<&FixedDofMap<'_>>,
    options: &LinearOptions<'_>,
    cache: &mut SolverCache,
    mut assemble: impl FnMut(f64) -> Result<TransientOperators, String>,
    mut observe: impl FnMut(f64, &[C], bool) -> Result<(), String>,
) -> Result<Vec<C>, String> {
    let initial_re = initial.iter().map(|v| v.re).collect::<Vec<_>>();
    let initial_im = initial.iter().map(|v| v.im).collect::<Vec<_>>();
    let reduced = if let Some(map) = mapping {
        let (re, im) = crate::constraints::restrict_weighted_consistent(
            &initial_re,
            &initial_im,
            map.representative,
            map.weights,
            map.count,
        )?;
        re.into_iter()
            .zip(im)
            .map(|(re, im)| C { re, im })
            .collect::<Vec<_>>()
    } else {
        initial.to_vec()
    };
    let expand = |state: &[C]| -> Result<Vec<C>, String> {
        if let Some(map) = mapping {
            let re = state.iter().map(|v| v.re).collect::<Vec<_>>();
            let im = state.iter().map(|v| v.im).collect::<Vec<_>>();
            let (re, im) = crate::constraints::expand_weighted(
                &re,
                &im,
                map.representative,
                map.weights,
                map.count,
            )?;
            Ok(re
                .into_iter()
                .zip(im)
                .map(|(re, im)| C { re, im })
                .collect())
        } else {
            Ok(state.to_vec())
        }
    };
    let final_state = integrate_backward_euler(
        grid.start,
        grid.end,
        grid.max_step,
        grid.max_steps,
        &reduced,
        &grid.output_times,
        options,
        cache,
        |time| {
            let operators = assemble(time)?;
            if let Some(map) = mapping {
                reduce_operators(&operators, map.representative, map.weights, map.count)
            } else {
                Ok(operators)
            }
        },
        |time, state, output| {
            if mapping.is_some() {
                observe(time, &expand(state)?, output)
            } else {
                observe(time, state, output)
            }
        },
    )?;
    if mapping.is_some() {
        expand(&final_state)
    } else {
        Ok(final_state)
    }
}

fn essential_map(
    values: &[(usize, C)],
    dimension: usize,
) -> Result<std::collections::HashMap<usize, C>, String> {
    let mut result = std::collections::HashMap::new();
    for &(dof, value) in values {
        if dof >= dimension
            || !value.re.is_finite()
            || !value.im.is_finite()
            || result.insert(dof, value).is_some()
        {
            return Err(
                "Transient essential values must be finite, distinct and within the state.".into(),
            );
        }
    }
    Ok(result)
}

fn rate_map(
    operators: &TransientOperators,
    dimension: usize,
) -> Result<BTreeMap<usize, EssentialRate>, String> {
    let mut rates = BTreeMap::new();
    for rate in &operators.essential_rates {
        if rate.source >= dimension
            || rate.target >= dimension
            || rate.source == rate.target
            || !rate.factor.re.is_finite()
            || !rate.factor.im.is_finite()
            || !operators
                .essential
                .iter()
                .any(|(dof, _)| *dof == rate.source)
            || rates.insert(rate.target, *rate).is_some()
        {
            return Err(
                "Transient rate constraints require distinct valid targets and prescribed sources."
                    .into(),
            );
        }
    }
    Ok(rates)
}

fn step_essentials(
    previous: &TransientOperators,
    current: &TransientOperators,
    dimension: usize,
    dt: f64,
) -> Result<std::collections::HashMap<usize, C>, String> {
    let old = essential_map(&previous.essential, dimension)?;
    let mut result = essential_map(&current.essential, dimension)?;
    let old_rates = rate_map(previous, dimension)?;
    let new_rates = rate_map(current, dimension)?;
    if old_rates.len() != new_rates.len() || !dt.is_finite() || dt <= 0. {
        return Err("Transient rate constraints changed or step size is invalid.".into());
    }
    for (target, rate) in new_rates {
        let previous_rate = old_rates
            .get(&target)
            .ok_or("Transient rate constraints changed between steps.")?;
        if rate.source != previous_rate.source
            || rate.factor.re != previous_rate.factor.re
            || rate.factor.im != previous_rate.factor.im
        {
            return Err("Transient rate constraints changed between steps.".into());
        }
        let prior = old
            .get(&rate.source)
            .ok_or("Previous transient prescribed value is missing.")?;
        let now = result
            .get(&rate.source)
            .ok_or("Current transient prescribed value is missing.")?;
        let value = c_mul(
            rate.factor,
            C {
                re: (now.re - prior.re) / dt,
                im: (now.im - prior.im) / dt,
            },
        );
        if !value.re.is_finite() || !value.im.is_finite() {
            return Err("Transient prescribed rate is non-finite.".into());
        }
        if let Some(existing) = result.get(&target) {
            if (existing.re - value.re).hypot(existing.im - value.im)
                > 1e-10 * (1. + existing.re.hypot(existing.im).max(value.re.hypot(value.im)))
            {
                return Err("Transient prescribed value conflicts with its derived rate.".into());
            }
        } else {
            result.insert(target, value);
        }
    }
    Ok(result)
}

/// Advance a fixed maximum step, shortening the final step to land exactly on
/// the end time and on requested output times, without interpolation. The
/// observer receives every step for progress/cancellation and an output flag.
pub(crate) fn integrate_backward_euler(
    start: f64,
    end: f64,
    max_step: f64,
    max_steps: usize,
    initial: &[C],
    output_times: &[f64],
    options: &LinearOptions<'_>,
    cache: &mut SolverCache,
    mut assemble: impl FnMut(f64) -> Result<TransientOperators, String>,
    mut observe: impl FnMut(f64, &[C], bool) -> Result<(), String>,
) -> Result<Vec<C>, String> {
    if !start.is_finite()
        || !end.is_finite()
        || !max_step.is_finite()
        || end <= start
        || max_step <= 0.0
        || max_steps == 0
        || initial.is_empty()
        || initial
            .iter()
            .any(|v| !v.re.is_finite() || !v.im.is_finite())
    {
        return Err("Transient interval, time step or initial state is invalid.".into());
    }
    if output_times
        .iter()
        .any(|&time| !time.is_finite() || time < start || time > end)
        || output_times.windows(2).any(|pair| pair[0] >= pair[1])
    {
        return Err("Transient output times must be finite, strictly increasing and inside the study interval.".into());
    }
    let mut t = start;
    let mut state = initial.to_vec();
    let mut previous = assemble(t)?;
    rate_map(&previous, state.len())?;
    for (dof, value) in essential_map(&previous.essential, state.len())? {
        let difference = (state[dof].re - value.re).hypot(state[dof].im - value.im);
        let scale = 1.0_f64.max(value.re.hypot(value.im));
        if difference > 1e-10 * scale {
            return Err(format!(
                "Initial transient state conflicts with essential value at DOF {dof}."
            ));
        }
    }
    let mut output_index = 0;
    let mut steps = 0usize;
    let initial_output = output_times.first() == Some(&start);
    observe(t, &state, initial_output)?;
    if initial_output {
        output_index = 1;
    }
    while t < end {
        if steps >= max_steps {
            return Err("Transient study exceeded its declared step limit.".into());
        }
        let mut next = (t + max_step).min(end);
        if let Some(&output_time) = output_times.get(output_index) {
            next = next.min(output_time);
        }
        if !next.is_finite() || next <= t {
            return Err("Transient time step cannot advance time at the current precision.".into());
        }
        let current = assemble(next)?;
        if previous.derivatives.len() != current.derivatives.len()
            || previous
                .derivatives
                .iter()
                .zip(&current.derivatives)
                .any(|(a, b)| a.0 != b.0)
        {
            return Err("Transient derivative declarations changed during the study.".into());
        }
        let terms: Vec<_> = previous
            .derivatives
            .iter()
            .zip(&current.derivatives)
            .map(|(old, new)| DerivativeTerm {
                previous: &old.1,
                current: &new.1,
                derivative: new.0,
            })
            .collect();
        let (mut matrix, rhs) =
            backward_euler_terms(&terms, &current.operator, &state, &current.load, next - t)?;
        let mut re: Vec<_> = rhs.iter().map(|v| v.re).collect();
        let mut im: Vec<_> = rhs.iter().map(|v| v.im).collect();
        let essential = step_essentials(&previous, &current, state.len(), next - t)?;
        crate::constraints::apply_essential(&mut matrix, &mut re, &mut im, &essential)?;
        let (re, im, _) = solve_linear_system(
            &matrix,
            &re,
            &im,
            options.tolerance,
            options.backend,
            options.ordering,
            options.threads,
            cache,
        )?;
        state = re
            .into_iter()
            .zip(im)
            .map(|(re, im)| C { re, im })
            .collect();
        steps += 1;
        t = next;
        let output = output_times.get(output_index) == Some(&t);
        observe(t, &state, output)?;
        if output {
            output_index += 1;
        }
        previous = current;
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prescribed_rates_follow_fixed_constraint_weights_and_detect_conflicts() {
        let matrix = vec![BTreeMap::new(); 4];
        let make = |value: f64| TransientOperators {
            derivatives: vec![],
            operator: matrix.clone(),
            load: vec![z(0.); 4],
            essential: vec![(0, z(value)), (1, z(2. * value))],
            essential_rates: vec![
                EssentialRate {
                    source: 0,
                    target: 2,
                    factor: z(1.),
                },
                EssentialRate {
                    source: 1,
                    target: 3,
                    factor: z(1.),
                },
            ],
        };
        let reps = [0, 0, 1, 1];
        let weights = [z(1.), z(2.), z(1.), z(2.)];
        let old = reduce_operators(&make(0.), &reps, &weights, 2).unwrap();
        let current = reduce_operators(&make(2.), &reps, &weights, 2).unwrap();
        let values = step_essentials(&old, &current, 2, 0.5).unwrap();
        assert!((values[&0].re - 2.).abs() < 1e-12);
        assert!((values[&1].re - 4.).abs() < 1e-12);
        let mut bad = make(2.);
        bad.essential.push((2, z(3.)));
        let reduced = reduce_operators(&bad, &reps, &weights, 2).unwrap();
        assert!(step_essentials(&old, &reduced, 2, 0.5).is_err());
        let incompatible = [z(1.), z(2.), z(1.), z(3.)];
        assert!(reduce_operators(&make(2.), &reps, &incompatible, 2).is_err());
    }
    #[test]
    fn transient_reduction_maps_every_operator_and_checks_prescribed_values() {
        let matrix = vec![
            BTreeMap::from([(0, z(2.)), (1, z(1.))]),
            BTreeMap::from([(0, z(1.)), (1, z(3.))]),
        ];
        let operators = TransientOperators {
            derivatives: vec![(TimeDerivative::ConservedProduct, matrix.clone())],
            operator: matrix,
            load: vec![z(2.), z(3.)],
            essential: vec![(0, z(4.)), (1, z(4.))],
            essential_rates: vec![],
        };
        let reduced = reduce_operators(&operators, &[0, 0], &[z(1.), z(1.)], 1).unwrap();
        assert_eq!(reduced.operator[0][&0].re, 7.);
        assert_eq!(reduced.derivatives[0].1[0][&0].re, 7.);
        assert_eq!(reduced.load[0].re, 5.);
        assert_eq!(reduced.essential[0].1.re, 4.);
        let conflicting = TransientOperators {
            essential: vec![(0, z(4.)), (1, z(5.))],
            ..operators
        };
        assert!(
            reduce_operators(&conflicting, &[0, 0], &[z(1.), z(1.)], 1)
                .err()
                .unwrap()
                .contains("conflict")
        );
    }
    #[test]
    fn transient_fixed_identification_preserves_time_varying_storage() {
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let grid = crate::transient_request::TimeGrid {
            start: 0.,
            end: 1.,
            max_step: 0.25,
            max_steps: 8,
            output_times: vec![1.],
        };
        let mapping = FixedDofMap {
            representative: &[0, 0],
            weights: &[z(1.), z(1.)],
            count: 1,
        };
        let solved = integrate_with_constraints(
            &grid,
            &[z(1.), z(1.)],
            Some(&mapping),
            &options,
            &mut SolverCache::default(),
            |t| {
                let mass = vec![
                    BTreeMap::from([(0, z(1. + t))]),
                    BTreeMap::from([(1, z(1. + t))]),
                ];
                Ok(TransientOperators {
                    derivatives: vec![(TimeDerivative::ConservedProduct, mass)],
                    operator: vec![BTreeMap::new(), BTreeMap::new()],
                    load: vec![z(0.), z(0.)],
                    essential: vec![],
                    essential_rates: vec![],
                })
            },
            |_, state, _| {
                assert_eq!(state.len(), 2);
                assert!((state[0].re - state[1].re).abs() < 1e-12);
                Ok(())
            },
        )
        .unwrap();
        assert!((solved[0].re - 0.5).abs() < 1e-10);
        assert_eq!(solved.len(), 2);
        assert!((solved[0].re - solved[1].re).abs() < 1e-12);
    }
    fn z(x: f64) -> C {
        C { re: x, im: 0.0 }
    }
    fn diag(x: f64) -> SparseMatrix {
        vec![BTreeMap::from([(0, z(x))])]
    }
    #[test]
    fn time_varying_mass_distinguishes_conservation_from_state_derivative() {
        let old = diag(2.0);
        let new = diag(4.0);
        let stiffness = diag(0.0);
        let state = [z(3.0)];
        let load = [z(0.0)];
        for (derivative, expected) in [
            (TimeDerivative::ConservedProduct, 1.5),
            (TimeDerivative::State, 3.0),
        ] {
            let terms = [DerivativeTerm {
                previous: &old,
                current: &new,
                derivative,
            }];
            let (matrix, rhs) =
                backward_euler_terms(&terms, &stiffness, &state, &load, 0.25).unwrap();
            assert!((rhs[0].re / matrix[0][&0].re - expected).abs() < 1e-10);
        }
    }
    #[test]
    fn rejects_bad_step_and_out_of_range_sparse_entries() {
        let mass = diag(1.0);
        let invalid = vec![BTreeMap::from([(1, z(1.0))])];
        let state = [z(1.0)];
        let load = [z(0.0)];
        let terms = [DerivativeTerm {
            previous: &mass,
            current: &mass,
            derivative: TimeDerivative::State,
        }];
        assert!(backward_euler_terms(&terms, &invalid, &state, &load, 1.0).is_err());
        assert!(backward_euler_terms(&terms, &mass, &state, &load, 0.0).is_err());
    }

    #[test]
    fn coupled_state_rows_can_choose_distinct_derivative_conventions() {
        let old_a = vec![BTreeMap::from([(0, z(1.0))]), BTreeMap::new()];
        let new_a = vec![BTreeMap::from([(0, z(2.0))]), BTreeMap::new()];
        let old_b = vec![BTreeMap::new(), BTreeMap::from([(1, z(2.0))])];
        let new_b = vec![BTreeMap::new(), BTreeMap::from([(1, z(4.0))])];
        let coupling = vec![
            BTreeMap::from([(1, z(1.0))]),
            BTreeMap::from([(0, z(-1.0))]),
        ];
        let state = [z(3.0), z(4.0)];
        let load = [z(0.0), z(0.0)];
        let terms = [
            DerivativeTerm {
                previous: &old_a,
                current: &new_a,
                derivative: TimeDerivative::State,
            },
            DerivativeTerm {
                previous: &old_b,
                current: &new_b,
                derivative: TimeDerivative::ConservedProduct,
            },
        ];
        let (matrix, rhs) = backward_euler_terms(&terms, &coupling, &state, &load, 1.0).unwrap();
        assert_eq!(
            (
                matrix[0][&0].re,
                matrix[0][&1].re,
                matrix[1][&0].re,
                matrix[1][&1].re
            ),
            (2.0, 1.0, -1.0, 4.0)
        );
        assert_eq!((rhs[0].re, rhs[1].re), (6.0, 8.0));
    }

    #[test]
    fn multiple_derivative_laws_can_contribute_to_one_equation() {
        let state_old = diag(1.0);
        let state_new = diag(2.0);
        let stored_old = diag(3.0);
        let stored_new = diag(5.0);
        let terms = [
            DerivativeTerm {
                previous: &state_old,
                current: &state_new,
                derivative: TimeDerivative::State,
            },
            DerivativeTerm {
                previous: &stored_old,
                current: &stored_new,
                derivative: TimeDerivative::ConservedProduct,
            },
        ];
        let (matrix, rhs) =
            backward_euler_terms(&terms, &diag(0.0), &[z(4.0)], &[z(0.0)], 1.0).unwrap();
        assert_eq!(matrix[0][&0].re, 7.0);
        assert_eq!(rhs[0].re, 20.0);
    }

    #[test]
    fn integration_samples_time_varying_mass_and_reaches_exact_end() {
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let mut times = Vec::new();
        let result = integrate_backward_euler(
            0.0,
            1.0,
            0.4,
            10,
            &[z(2.0)],
            &[0.0, 0.3, 1.0],
            &options,
            &mut SolverCache::default(),
            |t| {
                Ok(TransientOperators {
                    derivatives: vec![(TimeDerivative::ConservedProduct, diag(1.0 + t))],
                    operator: diag(0.0),
                    load: vec![z(0.0)],
                    essential: vec![],
                    essential_rates: vec![],
                })
            },
            |t, _, output| {
                if output {
                    times.push(t)
                };
                Ok(())
            },
        )
        .unwrap();
        assert!((result[0].re - 1.0).abs() < 1e-10);
        assert_eq!(times, vec![0.0, 0.3, 1.0]);
    }

    #[test]
    fn integration_rejects_unordered_output_times_before_assembly() {
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let mut assembled = false;
        let result = integrate_backward_euler(
            0.0,
            1.0,
            0.2,
            10,
            &[z(1.0)],
            &[0.7, 0.3],
            &options,
            &mut SolverCache::default(),
            |_| {
                assembled = true;
                Err("unexpected assembly".into())
            },
            |_, _, _| Ok(()),
        );
        assert!(result.is_err());
        assert!(!assembled);
    }

    #[test]
    fn integration_allows_cancellation_between_sparse_outputs() {
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let mut observed = Vec::new();
        let result = integrate_backward_euler(
            0.0,
            1.0,
            0.25,
            10,
            &[z(1.0)],
            &[1.0],
            &options,
            &mut SolverCache::default(),
            |_| {
                Ok(TransientOperators {
                    derivatives: vec![(TimeDerivative::State, diag(1.0))],
                    operator: diag(0.0),
                    load: vec![z(0.0)],
                    essential: vec![],
                    essential_rates: vec![],
                })
            },
            |t, _, _| {
                observed.push(t);
                if t >= 0.5 {
                    Err("cancelled".into())
                } else {
                    Ok(())
                }
            },
        );
        assert_eq!(result.unwrap_err(), "cancelled");
        assert_eq!(observed, vec![0.0, 0.25, 0.5]);
    }

    #[test]
    fn sparse_backward_euler_obeys_wave_elastic_and_heat_energy_balances() {
        let options = LinearOptions {
            tolerance: 1e-11,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let identity = vec![
            BTreeMap::from([(0, z(1.0))]),
            BTreeMap::from([(1, z(1.0))]),
        ];
        let run = |operator: SparseMatrix, initial: &[C]| {
            let mut states = Vec::new();
            let solved = integrate_backward_euler(
                0.0,
                1.0,
                0.125,
                8,
                initial,
                &[1.0],
                &options,
                &mut SolverCache::default(),
                |_| {
                    Ok(TransientOperators {
                        derivatives: vec![(TimeDerivative::State, identity.clone())],
                        operator: operator.clone(),
                        load: vec![z(0.0), z(0.0)],
                        essential: vec![],
                        essential_rates: vec![],
                    })
                },
                |_, state, _| {
                    states.push(state.to_vec());
                    Ok(())
                },
            )
            .unwrap();
            assert!(states
                .last()
                .unwrap()
                .iter()
                .zip(&solved)
                .all(|(left, right)| (left.re - right.re).hypot(left.im - right.im) < 1e-12));
            states
        };
        let squared_distance = |left: &[C], right: &[C]| {
            left.iter()
                .zip(right)
                .map(|(a, b)| (a.re - b.re).powi(2) + (a.im - b.im).powi(2))
                .sum::<f64>()
        };

        // A skew operator models a source-free first-order wave system.
        let wave = run(
            vec![
                BTreeMap::from([(1, z(-1.0))]),
                BTreeMap::from([(0, z(1.0))]),
            ],
            &[z(1.0), z(0.0)],
        );
        for pair in wave.windows(2) {
            let previous_energy = 0.5 * squared_distance(&pair[0], &[z(0.0), z(0.0)]);
            let current_energy = 0.5 * squared_distance(&pair[1], &[z(0.0), z(0.0)]);
            let decrement = 0.5 * squared_distance(&pair[1], &pair[0]);
            assert!((previous_energy - current_energy - decrement).abs() < 1e-9);
        }

        // The elastic displacement/velocity block has a weighted mechanical
        // energy with the same backward-Euler dissipation identity.
        let omega_squared = 4.0;
        let elastic = run(
            vec![
                BTreeMap::from([(1, z(-1.0))]),
                BTreeMap::from([(0, z(omega_squared))]),
            ],
            &[z(1.0), z(0.0)],
        );
        let mechanical_energy = |state: &[C]| {
            0.5 * (omega_squared * state[0].re.powi(2) + state[1].re.powi(2))
        };
        for pair in elastic.windows(2) {
            let weighted_increment = omega_squared * (pair[1][0].re - pair[0][0].re).powi(2)
                + (pair[1][1].re - pair[0][1].re).powi(2);
            assert!(
                (mechanical_energy(&pair[0]) - mechanical_energy(&pair[1])
                    - 0.5 * weighted_increment)
                    .abs()
                    < 1e-9
            );
        }

        // A two-cell Neumann diffusion operator preserves total heat while
        // dissipating the L2 energy by its exact discrete balance.
        let heat = run(
            vec![
                BTreeMap::from([(0, z(1.0)), (1, z(-1.0))]),
                BTreeMap::from([(0, z(-1.0)), (1, z(1.0))]),
            ],
            &[z(1.0), z(0.0)],
        );
        let heat_sum = |state: &[C]| state[0].re + state[1].re;
        let heat_energy = |state: &[C]| 0.5 * (state[0].re.powi(2) + state[1].re.powi(2));
        for pair in heat.windows(2) {
            assert!((heat_sum(&pair[0]) - heat_sum(&pair[1])).abs() < 1e-10);
            let difference = pair[1][0].re - pair[1][1].re;
            let diffusive_loss = 0.125 * difference.powi(2);
            let step_loss = 0.5 * squared_distance(&pair[1], &pair[0]);
            assert!(
                (heat_energy(&pair[0]) - heat_energy(&pair[1])
                    - step_loss
                    - diffusive_loss)
                    .abs()
                    < 1e-9
            );
        }
    }

    #[test]
    fn integration_enforces_the_declared_step_limit() {
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let result = integrate_backward_euler(
            0.0,
            1.0,
            0.25,
            2,
            &[z(1.0)],
            &[1.0],
            &options,
            &mut SolverCache::default(),
            |_| {
                Ok(TransientOperators {
                    derivatives: vec![(TimeDerivative::State, diag(1.0))],
                    operator: diag(0.0),
                    load: vec![z(0.0)],
                    essential: vec![],
                    essential_rates: vec![],
                })
            },
            |_, _, _| Ok(()),
        );
        assert!(result.unwrap_err().contains("step limit"));
    }

    #[test]
    fn time_varying_essential_values_update_the_sparse_rhs() {
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let result = integrate_backward_euler(
            0.0,
            1.0,
            1.0,
            10,
            &[z(0.0), z(0.0)],
            &[1.0],
            &options,
            &mut SolverCache::default(),
            |t| {
                Ok(TransientOperators {
                    derivatives: vec![(
                        TimeDerivative::State,
                        vec![BTreeMap::from([(0, z(1.0))]), BTreeMap::from([(1, z(1.0))])],
                    )],
                    operator: vec![BTreeMap::new(), BTreeMap::from([(0, z(-1.0)), (1, z(1.0))])],
                    load: vec![z(0.0), z(0.0)],
                    essential: vec![(0, z(t))],
                    essential_rates: vec![],
                })
            },
            |_, _, _| Ok(()),
        )
        .unwrap();
        assert!((result[0].re - 1.0).abs() < 1e-10);
        assert!((result[1].re - 0.5).abs() < 1e-10);
    }

    #[test]
    fn initial_projection_respects_prescribed_values() {
        let options = LinearOptions {
            tolerance: 1e-10,
            backend: "faer",
            ordering: "amd",
            threads: 1,
        };
        let mass = vec![
            BTreeMap::from([(0, z(2.)), (1, z(1.))]),
            BTreeMap::from([(0, z(1.)), (1, z(2.))]),
        ];
        let state = project_initial_state(
            &mass,
            &[z(0.), z(3.)],
            &[(0, z(1.))],
            &options,
            &mut SolverCache::default(),
        )
        .unwrap();
        assert!((state[0].re - 1.).abs() < 1e-10);
        assert!((state[1].re - 1.).abs() < 1e-10);
    }
}
