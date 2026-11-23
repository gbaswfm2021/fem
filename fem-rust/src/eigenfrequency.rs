//! Scenario-neutral sparse generalized eigenvalue primitives.
//!
//! Scenario-neutral complex generalized-pencil primitives.
//!
//! `solve_generalized_sparse_pencil` solves `A x = lambda B x` directly for
//! finite complex `lambda` using shift-invert Arnoldi. The global operators
//! and shifted solve stay sparse; only the small projected Hessenberg matrix
//! is diagonalized densely. Temporal eigenfrequency is one adapter that maps
//! its declared transient operators to this generic pencil.
use crate::solver_common::{C, SolverCache, c_add, c_conj, c_mul, c_scale, factor_sparse_system};
use crate::transient_step::SparseMatrix;
use faer::Mat;
use faer::complex::Complex as FaerComplex;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(crate) struct PencilMode {
    pub(crate) eigenvalue: C,
    pub(crate) vector: Vec<C>,
    pub(crate) relative_residual: f64,
}

fn finite(value: C) -> bool {
    value.re.is_finite() && value.im.is_finite()
}

fn dot(left: &[C], right: &[C]) -> C {
    left.iter().zip(right).fold(C::default(), |sum, (&a, &b)| {
        c_add(sum, c_mul(c_conj(a), b))
    })
}

fn norm(values: &[C]) -> f64 {
    values.iter().map(|v| v.re * v.re + v.im * v.im).sum::<f64>().sqrt()
}

fn normalize(values: &mut [C]) -> Result<(), String> {
    let magnitude = norm(values);
    if !magnitude.is_finite() || magnitude <= f64::MIN_POSITIVE {
        return Err("Arnoldi encountered a zero or non-finite vector.".into());
    }
    for value in values {
        value.re /= magnitude;
        value.im /= magnitude;
    }
    Ok(())
}

fn deterministic_start_vector(n: usize, seed: u64) -> Vec<C> {
    fn next(state: &mut u64) -> f64 {
        *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = *state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        ((value >> 11) as f64) * (1. / ((1_u64 << 53) as f64)) * 2. - 1.
    }

    let mut state = seed;
    (0..n).map(|_| C { re: next(&mut state), im: next(&mut state) }).collect()
}

fn matvec(matrix: &[BTreeMap<usize, C>], vector: &[C]) -> Vec<C> {
    matrix.iter().map(|row| row.iter().fold(C::default(), |sum, (&j, &a)| {
        c_add(sum, c_mul(a, vector[j]))
    })).collect()
}

fn add_scaled(target: &mut [C], source: &[C], factor: C) {
    for (target, &source) in target.iter_mut().zip(source) {
        *target = c_add(*target, c_mul(factor, source));
    }
}

fn shifted_operator(a: &[BTreeMap<usize, C>], b: &[BTreeMap<usize, C>], shift: C) -> Result<SparseMatrix, String> {
    if a.is_empty() || a.len() != b.len() {
        return Err("Eigenvalue pencil requires matching nonempty sparse operators.".into());
    }
    let n = a.len();
    let mut result = vec![BTreeMap::new(); n];
    for i in 0..n {
        for (&j, &value) in &a[i] {
            result[i].insert(j, value);
        }
        for (&j, &value) in &b[i] {
            let entry = result[i].entry(j).or_default();
            *entry = c_add(*entry, c_scale(c_mul(shift, value), -1.));
        }
        result[i].retain(|_, value| value.re != 0. || value.im != 0.);
    }
    if result.iter().flat_map(|row| row.values()).any(|value| !finite(*value)) {
        return Err("Shifted eigenvalue operator contains non-finite values.".into());
    }
    Ok(result)
}

fn pencil_residual(a: &[BTreeMap<usize, C>], b: &[BTreeMap<usize, C>], value: C, vector: &[C]) -> f64 {
    let av = matvec(a, vector);
    let bv = matvec(b, vector);
    let mut residual = av;
    add_scaled(&mut residual, &bv, c_scale(value, -1.));
    let denominator = (a.iter().map(|row| row.values().map(|v| v.re.hypot(v.im)).sum::<f64>()).sum::<f64>()
        + value.re.hypot(value.im) * b.iter().map(|row| row.values().map(|v| v.re.hypot(v.im)).sum::<f64>()).sum::<f64>()) * norm(vector);
    norm(&residual) / denominator.max(f64::MIN_POSITIVE)
}

/// Build a sparse first-companion pencil for a quadratic matrix polynomial
/// `P0 + lambda P1 + lambda^2 P2`. The returned generalized eigenvalue is
/// `lambda` itself; it is never squared or post-processed as a signed pair.
/// The state is `(u, lambda*u)`, so the second pencil block also supplies a
/// direct recovery check for finite modes when `P2` is singular.
pub(crate) fn linearize_quadratic_polynomial(
    p0: &[BTreeMap<usize, C>],
    p1: &[BTreeMap<usize, C>],
    p2: &[BTreeMap<usize, C>],
) -> Result<(SparseMatrix, SparseMatrix), String> {
    let n = p0.len();
    if n == 0 || p1.len() != n || p2.len() != n || n.checked_mul(2).is_none() {
        return Err("Quadratic pencil blocks must have matching nonempty dimensions.".into());
    }
    for matrix in [p0, p1, p2] {
        if matrix.iter().any(|row| row.iter().any(|(&column, value)| column >= n || !finite(*value))) {
            return Err("Quadratic pencil block contains an invalid index or non-finite value.".into());
        }
    }
    let dimension = 2 * n;
    let mut a = vec![BTreeMap::new(); dimension];
    let mut b = vec![BTreeMap::new(); dimension];
    for row in 0..n {
        a[row].insert(n + row, C { re: 1., im: 0. });
        b[row].insert(row, C { re: 1., im: 0. });
        for (&column, &value) in &p0[row] {
            a[n + row].insert(column, c_scale(value, -1.));
        }
        for (&column, &value) in &p1[row] {
            a[n + row].insert(n + column, c_scale(value, -1.));
        }
        for (&column, &value) in &p2[row] {
            b[n + row].insert(n + column, value);
        }
    }
    Ok((a, b))
}

/// Recover sparse coefficient blocks for an operator whose declared
/// longitudinal dependence is quadratic, from assemblies at `-h`, `0`, and
/// `+h`. The sampling distance `h` is a positive inverse-length scale chosen
/// by the caller from model geometry. This operates on the union of the three
/// sparse graphs and preserves arbitrary complex, nonsymmetric linear terms.
pub(crate) fn quadratic_polynomial_from_samples(
    negative: &[BTreeMap<usize, C>],
    zero: &[BTreeMap<usize, C>],
    positive: &[BTreeMap<usize, C>],
    h: f64,
) -> Result<(SparseMatrix, SparseMatrix, SparseMatrix), String> {
    let n = zero.len();
    if n == 0 || negative.len() != n || positive.len() != n || !h.is_finite() || h <= 0. {
        return Err("Quadratic operator samples require matching nonempty sparse matrices and a positive finite inverse-length scale.".into());
    }
    for matrix in [negative, zero, positive] {
        if matrix.iter().any(|row| row.iter().any(|(&column, value)| column >= n || !finite(*value))) {
            return Err("Quadratic operator sample contains an invalid sparse entry.".into());
        }
    }
    let first_scale = 1. / (2. * h);
    let second_scale = 1. / (2. * h * h);
    if !first_scale.is_finite() || !second_scale.is_finite() {
        return Err("Quadratic operator sampling scale overflows.".into());
    }
    let mut p0 = vec![BTreeMap::new(); n];
    let mut p1 = vec![BTreeMap::new(); n];
    let mut p2 = vec![BTreeMap::new(); n];
    for row in 0..n {
        let mut columns = std::collections::BTreeSet::new();
        columns.extend(negative[row].keys().copied());
        columns.extend(zero[row].keys().copied());
        columns.extend(positive[row].keys().copied());
        for column in columns {
            let minus = negative[row].get(&column).copied().unwrap_or_default();
            let middle = zero[row].get(&column).copied().unwrap_or_default();
            let plus = positive[row].get(&column).copied().unwrap_or_default();
            let linear = c_scale(c_add(plus, c_scale(minus, -1.)), first_scale);
            let quadratic = c_scale(c_add(c_add(plus, minus), c_scale(middle, -2.)), second_scale);
            if middle.re != 0. || middle.im != 0. { p0[row].insert(column, middle); }
            if linear.re != 0. || linear.im != 0. { p1[row].insert(column, linear); }
            if quadratic.re != 0. || quadratic.im != 0. { p2[row].insert(column, quadratic); }
        }
    }
    Ok((p0, p1, p2))
}

/// Evaluate a sparse quadratic polynomial without forming a dense matrix.
pub(crate) fn evaluate_sparse_polynomial(
    p0:&[BTreeMap<usize,C>],p1:&[BTreeMap<usize,C>],p2:&[BTreeMap<usize,C>],value:C,
)->Result<SparseMatrix,String>{
    let n=p0.len();
    if n==0||p1.len()!=n||p2.len()!=n||[p0,p1,p2].into_iter().any(|matrix|matrix.iter().any(|row|row.keys().any(|&column|column>=n))){
        return Err("Sparse polynomial blocks have inconsistent dimensions or indices.".into());
    }
    let value2=c_mul(value,value);let mut result=vec![BTreeMap::new();n];
    for row in 0..n{
        let mut columns=std::collections::BTreeSet::new();columns.extend(p0[row].keys().copied());columns.extend(p1[row].keys().copied());columns.extend(p2[row].keys().copied());
        for column in columns{
            let a=p0[row].get(&column).copied().unwrap_or_default();
            let b=p1[row].get(&column).copied().unwrap_or_default();
            let c=p2[row].get(&column).copied().unwrap_or_default();
            let entry=c_add(c_add(a,c_mul(value,b)),c_mul(value2,c));
            if !finite(entry){return Err("Sparse polynomial evaluation produced a non-finite entry.".into());}
            if entry.re!=0.||entry.im!=0.{result[row].insert(column,entry);}
        }
    }
    Ok(result)
}

/// Relative Frobenius comparison on the union of two sparse graphs.
pub(crate) fn sparse_matrices_close(left:&[BTreeMap<usize,C>],right:&[BTreeMap<usize,C>],tolerance:f64)->Result<bool,String>{
    if left.is_empty()||left.len()!=right.len()||!tolerance.is_finite()||tolerance<0.{return Err("Sparse matrix comparison arguments are invalid.".into());}
    let mut difference=0.;let mut scale=0.;
    for row in 0..left.len(){
        let mut columns=std::collections::BTreeSet::new();columns.extend(left[row].keys().copied());columns.extend(right[row].keys().copied());
        for column in columns{
            if column>=left.len(){return Err("Sparse matrix comparison found an out-of-range column.".into());}
            let a=left[row].get(&column).copied().unwrap_or_default();let b=right[row].get(&column).copied().unwrap_or_default();
            difference+=(a.re-b.re).powi(2)+(a.im-b.im).powi(2);scale+=a.re*a.re+a.im*a.im;
        }
    }
    for row in right{for value in row.values(){scale+=value.re*value.re+value.im*value.im;}}
    Ok(difference.sqrt()<=tolerance*scale.sqrt().max(f64::MIN_POSITIVE))
}

fn polynomial_residual(
    p0: &[BTreeMap<usize, C>], p1: &[BTreeMap<usize, C>], p2: &[BTreeMap<usize, C>],
    value: C, vector: &[C],
) -> f64 {
    let p0u = matvec(p0, vector);
    let p1u = matvec(p1, vector);
    let p2u = matvec(p2, vector);
    let value2 = c_mul(value, value);
    let mut residual = p0u;
    add_scaled(&mut residual, &p1u, value);
    add_scaled(&mut residual, &p2u, value2);
    let matrix_size = |matrix: &[BTreeMap<usize, C>]| matrix.iter().flat_map(|row| row.values())
        .map(|entry| entry.re.hypot(entry.im)).sum::<f64>();
    let magnitude = value.re.hypot(value.im);
    let denominator = (matrix_size(p0) + magnitude * matrix_size(p1) + magnitude * magnitude * matrix_size(p2)) * norm(vector);
    norm(&residual) / denominator.max(f64::MIN_POSITIVE)
}

/// Solve a quadratic sparse operator polynomial while publishing only its
/// physical field block and checking the original polynomial residual.
pub(crate) fn solve_quadratic_sparse_polynomial(
    p0: &[BTreeMap<usize, C>], p1: &[BTreeMap<usize, C>], p2: &[BTreeMap<usize, C>],
    shift: C, requested: usize, krylov_dimension: usize, tolerance: f64,
    backend: &str, ordering: &str, threads: usize, cache: &mut SolverCache,
) -> Result<Vec<PencilMode>, String> {
    // Longitudinal wavenumbers in SI units are commonly millions of rad/m.
    // Linearizing the raw polynomial then places the identity block beside
    // P0 entries of order k0^2 and P2 entries of order one, which badly scales
    // the shifted companion pencil. Solve in a dimensionless variable
    // mu = kz / variable_scale, while preserving kz itself at the API edge.
    let max_entry = |matrix: &[BTreeMap<usize, C>]| matrix.iter().flat_map(|row| row.values())
        .map(|value| value.re.hypot(value.im)).fold(0.0_f64, f64::max);
    let norm0 = max_entry(p0);
    let norm1 = max_entry(p1);
    let norm2 = max_entry(p2);
    // The target is the relevant scale for a local shift-invert solve. A
    // coefficient ratio involving P1 is not a reliable global eigenvalue
    // scale for mixed-field systems: P1 can contain only cross-field
    // derivative couplings, whose largest entries do not characterize the
    // roots of the quadratic pencil. In particular, that ratio can inflate
    // the variable scale by orders of magnitude and move the dimensionless
    // target close to zero. Use the quadratic P0/P2 scale only as a floor
    // when it is available, and as a fallback for a zero shift.
    let target_scale = shift.re.hypot(shift.im);
    let quadratic_scale = if norm0 > 0. && norm2 > 0. {
        (norm0 / norm2).sqrt()
    } else if norm0 > 0. && norm1 > 0. {
        norm0 / norm1
    } else if norm1 > 0. && norm2 > 0. {
        norm1 / norm2
    } else {
        1.0
    };
    let variable_scale = if target_scale > 0. {
        target_scale.max(if quadratic_scale.is_finite() { quadratic_scale } else { 0. })
    } else {
        quadratic_scale.max(1.0)
    };
    let variable_scale_squared = variable_scale * variable_scale;
    if !variable_scale.is_finite() || !variable_scale_squared.is_finite() {
        return Err("Quadratic pencil eigenvalue scale is non-finite.".into());
    }
    let scaled_p1 = p1.iter().map(|row| row.iter().map(|(&column, &value)|
        (column, c_scale(value, variable_scale))).collect::<BTreeMap<_, _>>()).collect::<SparseMatrix>();
    let scaled_p2 = p2.iter().map(|row| row.iter().map(|(&column, &value)|
        (column, c_scale(value, variable_scale_squared))).collect::<BTreeMap<_, _>>()).collect::<SparseMatrix>();
    if scaled_p1.iter().chain(&scaled_p2).any(|row| row.values().any(|value| !finite(*value))) {
        return Err("Quadratic pencil coefficient scaling produced a non-finite value.".into());
    }
    let scaled_shift = c_scale(shift, 1. / variable_scale);
    let (a, b) = linearize_quadratic_polynomial(p0, &scaled_p1, &scaled_p2)?;
    let n = p0.len();
    let augmented = solve_generalized_sparse_pencil(&a, &b, scaled_shift, requested, krylov_dimension, tolerance,
        backend, ordering, threads, cache)?;
    let mut physical = Vec::with_capacity(augmented.len());
    for mut mode in augmented {
        let dimensionless_value = mode.eigenvalue;
        let mut field = mode.vector[..n].to_vec();
        let auxiliary = &mode.vector[n..];
        let relation = auxiliary.iter().zip(&field).map(|(&w, &u)| {
            let difference = c_add(w, c_scale(c_mul(dimensionless_value, u), -1.));
            difference.re * difference.re + difference.im * difference.im
        }).sum::<f64>().sqrt() / norm(auxiliary).max(norm(&field) * mode.eigenvalue.re.hypot(mode.eigenvalue.im)).max(f64::MIN_POSITIVE);
        if !relation.is_finite() || relation > tolerance {
            continue;
        }
        if normalize(&mut field).is_err() { continue; }
        let residual = polynomial_residual(p0, &scaled_p1, &scaled_p2, dimensionless_value, &field);
        if !residual.is_finite() || residual > tolerance { continue; }
        mode.vector = field;
        mode.eigenvalue = c_scale(dimensionless_value, variable_scale);
        mode.relative_residual = residual;
        physical.push(mode);
    }
    if physical.len() < requested {
        return Err(format!("Quadratic pencil yielded {} finite physical modes near the target; {requested} were requested. Increase the Krylov dimension, change the target, or inspect singular/constraint modes.", physical.len()));
    }
    Ok(physical)
}

/// Eliminate homogeneous essential degrees of freedom from both pencil
/// operators, retaining an index map for mode-shape reconstruction.
pub(crate) fn eliminate_homogeneous_essentials(
    a:&[BTreeMap<usize,C>],b:&[BTreeMap<usize,C>],essential:&[(usize,C)],
)->Result<(SparseMatrix,SparseMatrix,Vec<usize>),String>{
    if a.is_empty()||a.len()!=b.len(){return Err("Eigenvalue pencil dimensions do not match.".into());}
    let n=a.len();let mut constrained=vec![false;n];
    for &(index,value) in essential {
        if index>=n||!finite(value){return Err("Eigenfrequency essential constraint is out of range or non-finite.".into());}
        if value.re.hypot(value.im)>1e-12{return Err(format!("Eigenfrequency requires homogeneous essential conditions; DOF {index} is nonzero."));}
        constrained[index]=true;
    }
    let free=(0..n).filter(|&index|!constrained[index]).collect::<Vec<_>>();
    if free.is_empty(){return Err("Homogeneous constraints remove every eigenfrequency degree of freedom.".into());}
    let mut remap=vec![usize::MAX;n];for (new,&old) in free.iter().enumerate(){remap[old]=new;}
    let restrict=|matrix:&[BTreeMap<usize,C>]|->SparseMatrix{
        free.iter().map(|&old_row|matrix[old_row].iter().filter_map(|(&old_column,&value)|
            (remap[old_column]!=usize::MAX).then_some((remap[old_column],value))).collect()).collect()
    };
    Ok((restrict(a),restrict(b),free))
}

/// Assemble and reduce the transient declaration into `(-K, M)`, applying
/// the existing fixed periodic projection and homogeneous essential DOFs.
pub(crate) fn pencil_from_transient_operators(
    operators:&crate::transient_step::TransientOperators,
    mapping:Option<&crate::transient_step::FixedDofMap<'_>>,
)->Result<(SparseMatrix,SparseMatrix,Vec<usize>,Option<(Vec<usize>,Vec<C>,usize)>,usize),String>{
    if operators.load.iter().any(|value|!finite(*value)||value.re.hypot(value.im)>1e-12){
        return Err("Eigenfrequency Studies are source-free; the assembled load is nonzero.".into());
    }
    if !operators.essential_rates.is_empty(){return Err("Eigenfrequency Studies do not support rate-dependent essential constraints.".into());}
    let reduced=if let Some(map)=mapping {
        crate::transient_step::reduce_operators(operators,map.representative,map.weights,map.count)?
    } else {
        crate::transient_step::TransientOperators{derivatives:operators.derivatives.clone(),operator:operators.operator.clone(),load:operators.load.clone(),essential:operators.essential.clone(),essential_rates:Vec::new()}
    };
    let mass=temporal_mass(&reduced)?;
    let mut a=reduced.operator.clone();for row in &mut a {for value in row.values_mut(){*value=c_scale(*value,-1.);}}
    let (a,mass,free)=eliminate_homogeneous_essentials(&a,&mass,&reduced.essential)?;
    let raw_dimension=operators.operator.len();
    let mapping=mapping.map(|map|(map.representative.to_vec(),map.weights.to_vec(),map.count));
    Ok((a,mass,free,mapping,raw_dimension))
}

pub(crate) fn expand_mode(
    reduced:&[C],free:&[usize],raw_dimension:usize,mapping:Option<&(Vec<usize>,Vec<C>,usize)>,
)->Result<Vec<C>,String>{
    let reduced_dimension=mapping.map(|value|value.2).unwrap_or(raw_dimension);
    if free.len()!=reduced.len()||free.iter().any(|&index|index>=reduced_dimension){
        return Err("Eigenfrequency mode cannot be expanded through its constraint map.".into());
    }
    let mut state=vec![C::default();reduced_dimension];
    for (&index,&value) in free.iter().zip(reduced){state[index]=value;}
    if let Some((representative,weights,count))=mapping{
        if *count!=state.len(){return Err("Periodic eigenfrequency map has the wrong reduced dimension.".into());}
        let re=state.iter().map(|v|v.re).collect::<Vec<_>>();let im=state.iter().map(|v|v.im).collect::<Vec<_>>();
        let (re,im)=crate::constraints::expand_weighted(&re,&im,representative,weights,*count)?;
        let result=re.into_iter().zip(im).map(|(re,im)|C{re,im}).collect::<Vec<_>>();
        if result.len()!=raw_dimension{return Err("Expanded eigenfrequency state has the wrong raw dimension.".into());}
        Ok(result)
    }else{Ok(state)}
}

pub(crate) fn solve_transient_pencil(
    operators:&crate::transient_step::TransientOperators,
    mapping:Option<&crate::transient_step::FixedDofMap<'_>>,
    shift:C,requested:usize,krylov_dimension:usize,tolerance:f64,
    backend:&str,ordering:&str,threads:usize,cache:&mut SolverCache,
)->Result<Vec<PencilMode>,String>{
    let (a,b,free,periodic,raw_dimension)=pencil_from_transient_operators(operators,mapping)?;
    let mut modes=solve_generalized_sparse_pencil(&a,&b,shift,requested,krylov_dimension,tolerance,backend,ordering,threads,cache)?;
    for mode in &mut modes {
        mode.vector=expand_mode(&mode.vector,&free,raw_dimension,periodic.as_ref())?;
        if let Some(pivot)=mode.vector.iter().max_by(|left,right|left.re.hypot(left.im).total_cmp(&right.re.hypot(right.im))).copied(){
            let magnitude=pivot.re.hypot(pivot.im);
            if magnitude>f64::MIN_POSITIVE{
                let phase=C{re:pivot.re/magnitude,im:-pivot.im/magnitude};
                for value in &mut mode.vector{*value=c_mul(*value,phase);}
            }
        }
    }
    Ok(modes)
}

/// Solve for finite eigenvalues closest to `shift` in the complex plane.
/// `a` is `-K` and `b` is `M`; neither is assumed Hermitian or nonsingular.
/// Zero Ritz values of the shift-invert operator represent infinite pencil
/// eigenvalues and are discarded, which permits regular descriptor pencils.
pub(crate) fn solve_generalized_sparse_pencil(
    a: &[BTreeMap<usize, C>],
    b: &[BTreeMap<usize, C>],
    shift: C,
    requested: usize,
    krylov_dimension: usize,
    tolerance: f64,
    backend: &str,
    ordering: &str,
    threads: usize,
    cache: &mut SolverCache,
) -> Result<Vec<PencilMode>, String> {
    let n = a.len();
    if n == 0 || n != b.len() || requested == 0 || krylov_dimension == 0
        || !finite(shift) || !tolerance.is_finite() || tolerance <= 0.
        || a.iter().chain(b).any(|row| row.iter().any(|(&j, &v)| j >= n || !finite(v)))
    {
        return Err("Eigenvalue request has invalid dimensions, target, tolerance, or sparse values.".into());
    }
    let shifted = shifted_operator(a, b, shift)?;
    let factor = factor_sparse_system(&shifted, backend, ordering, threads, cache)
        .map_err(|error| format!("Could not factor shifted eigenvalue pencil: {error}"))?;
    let m = krylov_dimension.min(n);
    let mut basis = Vec::<Vec<C>>::with_capacity(m + 1);
    // A single starting vector cannot recover the geometric multiplicity of
    // an exactly repeated eigenvalue: its Krylov sequence contains only one
    // direction in that eigenspace. Seed a block of independent vectors so
    // repeated modes remain available to the projected problem.
    let start_count = requested.min(m);
    for start in 0..start_count {
        let mut accepted = None;
        for attempt in 0..8 {
            let mut candidate = deterministic_start_vector(
                n,
                0x6a09_e667_f3bc_c909_u64
                    .wrapping_add((start as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
                    .wrapping_add(attempt as u64),
            );
            for _ in 0..2 {
                for previous in &basis {
                    let coefficient = dot(previous, &candidate);
                    add_scaled(&mut candidate, previous, c_scale(coefficient, -1.));
                }
            }
            let magnitude = norm(&candidate);
            if magnitude.is_finite() && magnitude > 1e-12 {
                for value in &mut candidate {
                    value.re /= magnitude;
                    value.im /= magnitude;
                }
                accepted = Some(candidate);
                break;
            }
        }
        basis.push(accepted.ok_or("Could not construct independent eigenvalue start vectors.")?);
    }
    let mut hessenberg = vec![vec![C::default(); m]; m + 1];
    let mut actual = 0usize;
    let mut column = 0usize;
    while column < basis.len() && column < m {
        let rhs = matvec(b, &basis[column]);
        let mut next = factor.solve(&rhs, threads)
            .map_err(|error| format!("Shift-invert solve failed at Arnoldi step {}: {error}", column + 1))?;
        // Modified Gram-Schmidt with a second pass is more stable for clustered
        // and non-normal modes than a single classical projection. The initial
        // block is not Hessenberg ordered, so project against every vector
        // already in the basis, including later start vectors.
        for _ in 0..2 {
            for row in 0..basis.len() {
                let coefficient = dot(&basis[row], &next);
                hessenberg[row][column] = c_add(hessenberg[row][column], coefficient);
                add_scaled(&mut next, &basis[row], c_scale(coefficient, -1.));
            }
        }
        let beta = norm(&next);
        actual = column + 1;
        if beta.is_finite() && beta > 1e-13 && basis.len() < m {
            hessenberg[basis.len()][column] = C { re: beta, im: 0. };
            for value in &mut next { value.re /= beta; value.im /= beta; }
            basis.push(next);
        }
        column += 1;
    }
    if actual == 0 {
        return Err("Arnoldi produced an empty Krylov subspace.".into());
    }
    let projected = Mat::<FaerComplex<f64>>::from_fn(actual, actual, |i, j| {
        let value = hessenberg[i][j];
        FaerComplex::new(value.re, value.im)
    });
    let decomposition = projected.eigen().map_err(|error| format!("Projected complex eigenproblem failed: {error:?}"))?;
    let values = decomposition.S();
    let vectors = decomposition.U();
    let mut candidates = Vec::new();
    for column in 0..actual {
        let mu = C { re: values[column].re, im: values[column].im };
        let magnitude = mu.re.hypot(mu.im);
        if !magnitude.is_finite() || magnitude <= 1e-14 { continue; }
        let value = c_add(shift, C { re: mu.re / (magnitude * magnitude), im: -mu.im / (magnitude * magnitude) });
        if !finite(value) { continue; }
        let mut vector = vec![C::default(); n];
        for basis_column in 0..actual {
            let y = vectors[(basis_column, column)];
            add_scaled(&mut vector, &basis[basis_column], C { re: y.re, im: y.im });
        }
        normalize(&mut vector)?;
        let residual = pencil_residual(a, b, value, &vector);
        if residual.is_finite() && residual <= tolerance {
            candidates.push(PencilMode { eigenvalue: value, vector, relative_residual: residual });
        }
    }
    candidates.sort_by(|left, right| {
        let dl = c_add(left.eigenvalue, c_scale(shift, -1.)).re.hypot(c_add(left.eigenvalue, c_scale(shift, -1.)).im);
        let dr = c_add(right.eigenvalue, c_scale(shift, -1.)).re.hypot(c_add(right.eigenvalue, c_scale(shift, -1.)).im);
        dl.total_cmp(&dr).then(left.relative_residual.total_cmp(&right.relative_residual))
    });
    candidates.dedup_by(|left, right| {
        let distance = c_add(left.eigenvalue, c_scale(right.eigenvalue, -1.)).re
            .hypot(c_add(left.eigenvalue, c_scale(right.eigenvalue, -1.)).im);
        let overlap = dot(&left.vector, &right.vector);
        let overlap_magnitude = overlap.re.hypot(overlap.im);
        distance <= 1e-9 * (1. + left.eigenvalue.re.hypot(left.eigenvalue.im))
            && overlap_magnitude >= 1. - 1e-10
    });
    if candidates.len() < requested {
        return Err(format!("Shift-invert Arnoldi converged {} finite modes near the target; {requested} were requested. Increase the Krylov dimension or change the target.", candidates.len()));
    }
    candidates.truncate(requested);
    Ok(candidates)
}

/// Sum the declared derivative blocks to form the temporal mass operator.
pub(crate) fn temporal_mass(operators: &crate::transient_step::TransientOperators) -> Result<SparseMatrix, String> {
    if operators.derivatives.is_empty() {
        return Err("The declared transient form has no time-derivative terms for an eigenfrequency Study.".into());
    }
    let n = operators.operator.len();
    if operators.derivatives.iter().any(|(_, matrix)| matrix.len() != n) {
        return Err("Transient derivative blocks have inconsistent dimensions.".into());
    }
    let mut mass = vec![BTreeMap::new(); n];
    for (_, matrix) in &operators.derivatives {
        for (i, row) in matrix.iter().enumerate() {
            for (&j, &value) in row {
                let entry = mass[i].entry(j).or_default();
                *entry = c_add(*entry, value);
            }
        }
    }
    Ok(mass)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver_common::c_abs;

    fn diagonal(values: &[C]) -> SparseMatrix {
        values.iter().enumerate().map(|(i, &value)| BTreeMap::from([(i, value)])).collect()
    }

    #[test]
    fn sparse_quadratic_sampling_preserves_nonsymmetric_linear_operator() {
        let c = |re, im| C { re, im };
        let p0 = vec![BTreeMap::from([(0, c(2., 0.)), (1, c(0., 1.))]), BTreeMap::from([(0, c(-1., 0.)), (1, c(3., 0.))])];
        let p1 = vec![BTreeMap::from([(0, c(1., 0.)), (1, c(4., -2.))]), BTreeMap::from([(0, c(-3., 1.)), (1, c(2., 0.))])];
        let p2 = vec![BTreeMap::from([(0, c(2., 0.))]), BTreeMap::from([(1, c(0.5, 0.))])];
        let h = 3.25;
        let sample = |k: f64| -> SparseMatrix {
            (0..2).map(|row| {
                let mut result = BTreeMap::new();
                let mut columns = std::collections::BTreeSet::new();
                columns.extend(p0[row].keys().copied());
                columns.extend(p1[row].keys().copied());
                columns.extend(p2[row].keys().copied());
                for column in columns {
                    let value = c_add(c_add(p0[row].get(&column).copied().unwrap_or_default(),
                        c_scale(p1[row].get(&column).copied().unwrap_or_default(), k)),
                        c_scale(p2[row].get(&column).copied().unwrap_or_default(), k * k));
                    result.insert(column, value);
                }
                result
            }).collect()
        };
        let (got0, got1, got2) = quadratic_polynomial_from_samples(&sample(-h), &sample(0.), &sample(h), h).unwrap();
        let equal = |actual: C, expected: C| (actual.re - expected.re).abs().max((actual.im - expected.im).abs()) < 1e-12;
        for row in 0..2 { for column in 0..2 {
            assert!(equal(got0[row].get(&column).copied().unwrap_or_default(), p0[row].get(&column).copied().unwrap_or_default()));
            assert!(equal(got1[row].get(&column).copied().unwrap_or_default(), p1[row].get(&column).copied().unwrap_or_default()));
            assert!(equal(got2[row].get(&column).copied().unwrap_or_default(), p2[row].get(&column).copied().unwrap_or_default()));
        }}
        assert_ne!(got1[0][&1], got1[1][&0]);
    }

    #[test]
    fn quadratic_companion_returns_kz_directly_without_reciprocal_pairing() {
        // P(k) = k^2 + k - 2 has roots 1 and -2, not a +/- pair.
        let p0 = diagonal(&[C { re: -2., im: 0. }]);
        let p1 = diagonal(&[C { re: 1., im: 0. }]);
        let p2 = diagonal(&[C { re: 1., im: 0. }]);
        let mut cache = SolverCache::default();
        let modes = solve_quadratic_sparse_polynomial(&p0, &p1, &p2, C { re: 0.2, im: 0.1 },
            2, 2, 1e-8, "faer", "amd", 1, &mut cache).unwrap();
        assert_eq!(modes.len(), 2);
        assert!((modes[0].eigenvalue.re - 1.).abs() < 1e-8 || (modes[0].eigenvalue.re + 2.).abs() < 1e-8);
        assert!((modes[1].eigenvalue.re - 1.).abs() < 1e-8 || (modes[1].eigenvalue.re + 2.).abs() < 1e-8);
        assert!((modes[0].eigenvalue.re + modes[1].eigenvalue.re + 1.).abs() < 1e-8);
        assert!(modes.iter().all(|mode| mode.vector.len() == 1 && mode.relative_residual < 1e-8));
    }

    #[test]
    fn quadratic_companion_scales_large_physical_wavenumbers() {
        let first = 5.862e6;
        let second = 5.842e6;
        let p0 = diagonal(&[C { re: first * second, im: 0. }]);
        let p1 = diagonal(&[C { re: -(first + second), im: 0. }]);
        let p2 = diagonal(&[C { re: 1., im: 0. }]);
        let mut cache = SolverCache::default();
        let modes = solve_quadratic_sparse_polynomial(&p0, &p1, &p2,
            C { re: 5.863e6, im: 0. }, 1, 4, 1e-10,
            "faer", "amd", 1, &mut cache).unwrap();
        assert_eq!(modes.len(), 1);
        assert!((modes[0].eigenvalue.re - first).abs() < 1e-3);
        assert!(modes[0].eigenvalue.im.abs() < 1e-3, "unexpected imaginary part: {:?}", modes[0].eigenvalue);
        assert!(modes[0].relative_residual < 1e-10);
    }

    #[test]
    fn quadratic_companion_does_not_over_scale_for_weak_linear_coupling() {
        let first = 5.862e6;
        let second = -first + 1.0;
        let p0 = diagonal(&[C { re: first * second, im: 0. }]);
        let p1 = diagonal(&[C { re: -(first + second), im: 0. }]);
        let p2 = diagonal(&[C { re: 1., im: 0. }]);
        let mut cache = SolverCache::default();
        let modes = solve_quadratic_sparse_polynomial(&p0, &p1, &p2,
            C { re: first + 100., im: 0. }, 1, 2, 1e-10,
            "faer", "amd", 1, &mut cache).unwrap();
        assert_eq!(modes.len(), 1);
        assert!((modes[0].eigenvalue.re - first).abs() < 1e-3);
        assert!(modes[0].eigenvalue.im.abs() < 1e-3);
        assert!(modes[0].relative_residual < 1e-10);
    }

    #[test]
    fn complex_nonsymmetric_quadratic_companion_keeps_unpaired_direct_roots() {
        // Two independent quadratic factors have roots {1, 2+i} and
        // {-3, 0.5+0.2i}; the pencil is neither Hermitian nor +/- paired.
        let p0=vec![BTreeMap::from([(0,C{re:2.,im:1.})]),BTreeMap::from([(1,C{re:-1.5,im:-0.6})])];
        let p1=vec![BTreeMap::from([(0,C{re:-3.,im:-1.})]),BTreeMap::from([(1,C{re:2.5,im:-0.2})])];
        let p2=diagonal(&[C{re:1.,im:0.},C{re:1.,im:0.}]);
        let expected=[C{re:1.,im:0.},C{re:2.,im:1.},C{re:-3.,im:0.},C{re:0.5,im:0.2}];
        let mut cache=SolverCache::default();
        let modes=solve_quadratic_sparse_polynomial(&p0,&p1,&p2,C{re:0.7,im:0.3},4,4,1e-8,"faer","amd",1,&mut cache).unwrap();
        assert_eq!(modes.len(),expected.len());
        for mode in &modes{
            let distance=expected.iter().map(|root|c_abs(c_add(mode.eigenvalue,c_scale(*root,-1.))))
                .fold(f64::INFINITY,f64::min);
            assert!(distance<1e-8,"unexpected direct longitudinal eigenvalue {:?}",mode.eigenvalue);
            assert!(mode.relative_residual<1e-8);
        }
        for (index,mode) in modes.iter().enumerate(){for other in modes.iter().skip(index+1){
            assert!(c_abs(c_add(mode.eigenvalue,c_scale(other.eigenvalue,-1.)))>1e-3,
                "the general solver must not synthesize an opposite eigenvalue pair");
        }}
    }

    #[test]
    fn quadratic_companion_filters_infinite_modes_from_singular_leading_block() {
        // The second scalar block is a nonzero constant polynomial. Its
        // companion contributes only infinite descriptor modes.
        let p0 = diagonal(&[C { re: -2., im: 0. }, C { re: -3., im: 0. }]);
        let p1 = diagonal(&[C { re: 1., im: 0. }, C::default()]);
        let p2 = diagonal(&[C { re: 1., im: 0. }, C::default()]);
        let mut cache = SolverCache::default();
        let modes = solve_quadratic_sparse_polynomial(&p0, &p1, &p2, C { re: 0.2, im: 0.1 },
            2, 4, 1e-8, "faer", "amd", 1, &mut cache).unwrap();
        assert_eq!(modes.len(), 2);
        assert!(modes.iter().all(|mode| mode.relative_residual < 1e-8));
        assert!(modes.iter().any(|mode| (mode.eigenvalue.re - 1.).abs() < 1e-8));
        assert!(modes.iter().any(|mode| (mode.eigenvalue.re + 2.).abs() < 1e-8));
    }

    #[test]
    fn nonsymmetric_complex_pencil_returns_each_eigenvalue_without_pairing() {
        // A and B have distinct complex eigenvalues with no +/- pairing.
        let a = vec![
            BTreeMap::from([(0, C { re: 2., im: 1. }), (1, C { re: 1., im: -0.5 })]),
            BTreeMap::from([(1, C { re: -1., im: 2. })]),
        ];
        let b = diagonal(&[C { re: 1., im: 0. }, C { re: 2., im: 0. }]);
        let mut cache = SolverCache::default();
        let modes = solve_generalized_sparse_pencil(
            &a, &b, C { re: 0.4, im: 0.8 }, 2, 2, 1e-8,
            "faer", "amd", 1, &mut cache,
        ).unwrap();
        assert_eq!(modes.len(), 2);
        assert!(modes.iter().all(|mode| mode.relative_residual < 1e-8));
        let expected=[C{re:2.,im:1.},C{re:-0.5,im:1.}];
        for mode in &modes {
            let distance=expected.iter().map(|target|{
                let error=c_add(mode.eigenvalue,c_scale(*target,-1.));
                error.re.hypot(error.im)
            }).fold(f64::INFINITY,f64::min);
            assert!(distance<1e-8,"unexpected generalized eigenvalue {:?}",mode.eigenvalue);
        }
    }

    #[test]
    fn singular_mass_discards_infinite_descriptor_mode() {
        let a = diagonal(&[C { re: 2., im: 0. }, C { re: 1., im: 0. }]);
        let b = diagonal(&[C { re: 1., im: 0. }, C::default()]);
        let mut cache = SolverCache::default();
        let modes = solve_generalized_sparse_pencil(
            &a, &b, C { re: 0., im: 0. }, 1, 2, 1e-8,
            "faer", "amd", 1, &mut cache,
        ).unwrap();
        assert!((modes[0].eigenvalue.re - 2.).abs() < 1e-8);
    }

    #[test]
    fn repeated_and_tightly_clustered_eigenvalues_retain_distinct_modes() {
        let a = diagonal(&[
            C { re: 2., im: 0. }, C { re: 2., im: 0. },
            C { re: 2.0000001, im: 0. }, C { re: 5., im: 0. },
        ]);
        let b = diagonal(&[C { re: 1., im: 0. }; 4]);
        let mut cache = SolverCache::default();
        let modes = solve_generalized_sparse_pencil(
            &a, &b, C { re: 2.1, im: 0. }, 3, 4, 1e-8,
            "faer", "amd", 1, &mut cache,
        ).unwrap();
        assert_eq!(modes.len(), 3);
        assert_eq!(modes.iter().filter(|mode| (mode.eigenvalue.re - 2.).abs() < 1e-8).count(), 2);
        assert_eq!(modes.iter().filter(|mode| (mode.eigenvalue.re - 2.0000001).abs() < 1e-8).count(), 1);
        assert!(modes.iter().all(|mode| mode.eigenvalue.im.abs() < 1e-8));
        assert!(modes.iter().all(|mode| mode.relative_residual < 1e-8));
        let mut orthonormal = Vec::<Vec<C>>::new();
        for mode in &modes {
            let mut residual = mode.vector.clone();
            for previous in &orthonormal {
                let coefficient = dot(previous, &residual);
                add_scaled(&mut residual, previous, c_scale(coefficient, -1.));
            }
            let magnitude = norm(&residual);
            assert!(magnitude > 1e-8, "repeated modes must have independent vectors");
            for value in &mut residual { value.re /= magnitude; value.im /= magnitude; }
            orthonormal.push(residual);
        }
    }

    #[test]
    fn generalized_solver_orders_shifted_modes_deterministically() {
        let a = diagonal(&[
            C { re: 5., im: 0. }, C { re: -1., im: 0. },
            C { re: 3., im: 0. }, C { re: 2., im: 0. },
        ]);
        let b = diagonal(&[C { re: 1., im: 0. }; 4]);
        let shift = C { re: 2.1, im: 0. };
        let mut first_cache = SolverCache::default();
        let first = solve_generalized_sparse_pencil(
            &a, &b, shift, 3, 4, 1e-9, "faer", "amd", 1, &mut first_cache,
        ).unwrap();
        let mut second_cache = SolverCache::default();
        let second = solve_generalized_sparse_pencil(
            &a, &b, shift, 3, 4, 1e-9, "faer", "amd", 1, &mut second_cache,
        ).unwrap();
        assert_eq!(first.len(), 3);
        assert_eq!(second.len(), 3);
        for (left, right) in first.iter().zip(&second) {
            assert!(c_abs(c_add(left.eigenvalue, c_scale(right.eigenvalue, -1.))) < 1e-10);
        }
        let distances: Vec<_> = first.iter().map(|mode| c_abs(c_add(mode.eigenvalue, c_scale(shift, -1.)))).collect();
        assert!(distances.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!((first[0].eigenvalue.re - 2.).abs() < 1e-9);
        assert!((first[1].eigenvalue.re - 3.).abs() < 1e-9);
    }

    #[test]
    fn generalized_solver_reports_insufficient_convergence_and_singular_shift() {
        let a = diagonal(&[
            C { re: 2., im: 0. }, C { re: 4., im: 0. },
            C { re: 8., im: 0. }, C { re: 16., im: 0. },
        ]);
        let b = diagonal(&[C { re: 1., im: 0. }; 4]);
        let mut short_cache = SolverCache::default();
        let short = solve_generalized_sparse_pencil(
            &a, &b, C { re: 3., im: 0. }, 2, 1, 1e-12,
            "faer", "amd", 1, &mut short_cache,
        ).unwrap_err();
        assert!(short.contains("converged"), "unexpected convergence diagnostic: {short}");

        let mut singular_cache = SolverCache::default();
        let singular = solve_generalized_sparse_pencil(
            &a, &b, C { re: 2., im: 0. }, 1, 4, 1e-8,
            "faer", "amd", 1, &mut singular_cache,
        ).unwrap_err();
        assert!(singular.contains("Could not factor shifted eigenvalue pencil"),
            "unexpected singular-shift diagnostic: {singular}");
    }

    #[test]
    fn generalized_solver_handles_thousand_dof_sparse_repeated_spectrum() {
        let n = 1024;
        let a_values = vec![C { re: 2., im: 0. }; n];
        let b_values = vec![C { re: 1., im: 0. }; n];
        let a = diagonal(&a_values);
        let b = diagonal(&b_values);
        let mut cache = SolverCache::default();
        let modes = solve_generalized_sparse_pencil(
            &a, &b, C { re: 2.1, im: 0. }, 4, 8, 1e-9,
            "faer", "amd", 1, &mut cache,
        ).unwrap();
        assert_eq!(modes.len(), 4);
        assert!(modes.iter().all(|mode| (mode.eigenvalue.re - 2.).abs() < 1e-9
            && mode.eigenvalue.im.abs() < 1e-9 && mode.relative_residual < 1e-9));
    }
}
