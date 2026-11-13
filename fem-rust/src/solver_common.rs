use faer::Col;
#[cfg(feature = "rivrs-backend")]
use faer::Par;
use faer::complex::Complex as FaerComplex;
#[cfg(feature = "rivrs-backend")]
use faer::dyn_stack::{MemBuffer, MemStack};
use faer::prelude::Solve;
use faer::sparse::linalg::solvers::{Lu, SymbolicLu};
use faer::sparse::{SparseColMat, Triplet};
#[cfg(feature = "rivrs-backend")]
use rivrs_sparse::symmetric::{AnalyzeOptions, FactorOptions, OrderingStrategy, SparseLDLT};
use std::collections::BTreeMap;
use std::time::Instant;

pub(crate) const MU0: f64 = 1.256_637_062_12e-6;
pub(crate) const TET_Q: [[f64; 5]; 11] = [
    [0.25, 0.25, 0.25, 0.25, -0.0131555555555556],
    [
        0.785714285714286,
        0.0714285714285714,
        0.0714285714285714,
        0.0714285714285714,
        0.00762222222222222,
    ],
    [
        0.0714285714285714,
        0.785714285714286,
        0.0714285714285714,
        0.0714285714285714,
        0.00762222222222222,
    ],
    [
        0.0714285714285714,
        0.0714285714285714,
        0.785714285714286,
        0.0714285714285714,
        0.00762222222222222,
    ],
    [
        0.0714285714285714,
        0.0714285714285714,
        0.0714285714285714,
        0.785714285714286,
        0.00762222222222222,
    ],
    [
        0.399403576166799,
        0.399403576166799,
        0.100596423833201,
        0.100596423833201,
        0.0248888888888889,
    ],
    [
        0.399403576166799,
        0.100596423833201,
        0.399403576166799,
        0.100596423833201,
        0.0248888888888889,
    ],
    [
        0.399403576166799,
        0.100596423833201,
        0.100596423833201,
        0.399403576166799,
        0.0248888888888889,
    ],
    [
        0.100596423833201,
        0.399403576166799,
        0.399403576166799,
        0.100596423833201,
        0.0248888888888889,
    ],
    [
        0.100596423833201,
        0.399403576166799,
        0.100596423833201,
        0.399403576166799,
        0.0248888888888889,
    ],
    [
        0.100596423833201,
        0.100596423833201,
        0.399403576166799,
        0.399403576166799,
        0.0248888888888889,
    ],
];
pub(crate) const TRI_Q: [[f64; 4]; 7] = [
    [1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0, 0.225],
    [
        0.059715871789770,
        0.470142064105115,
        0.470142064105115,
        0.132394152788506,
    ],
    [
        0.470142064105115,
        0.059715871789770,
        0.470142064105115,
        0.132394152788506,
    ],
    [
        0.470142064105115,
        0.470142064105115,
        0.059715871789770,
        0.132394152788506,
    ],
    [
        0.797426985353087,
        0.101286507323456,
        0.101286507323456,
        0.125939180544827,
    ],
    [
        0.101286507323456,
        0.797426985353087,
        0.101286507323456,
        0.125939180544827,
    ],
    [
        0.101286507323456,
        0.101286507323456,
        0.797426985353087,
        0.125939180544827,
    ],
];

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct C {
    pub(crate) re: f64,
    pub(crate) im: f64,
}
impl C {
    pub(crate) fn div(self, b: C) -> C {
        let d = b.re * b.re + b.im * b.im;
        C {
            re: (self.re * b.re + self.im * b.im) / d,
            im: (self.im * b.re - self.re * b.im) / d,
        }
    }
}
pub(crate) fn c_add(a: C, b: C) -> C {
    C {
        re: a.re + b.re,
        im: a.im + b.im,
    }
}
pub(crate) fn c_mul(a: C, b: C) -> C {
    C {
        re: a.re * b.re - a.im * b.im,
        im: a.re * b.im + a.im * b.re,
    }
}
pub(crate) fn c_scale(a: C, s: f64) -> C {
    C {
        re: a.re * s,
        im: a.im * s,
    }
}
pub(crate) fn c_exp(a: C) -> C {
    let scale = a.re.exp();
    C {
        re: scale * a.im.cos(),
        im: scale * a.im.sin(),
    }
}
pub(crate) fn c_sqrt(a: C) -> C {
    let mag = c_abs(a);
    let re = ((mag + a.re) * 0.5).max(0.0).sqrt();
    let im = ((mag - a.re) * 0.5)
        .max(0.0)
        .sqrt()
        .copysign(if a.im < 0.0 { -1.0 } else { 1.0 });
    C { re, im }
}
pub(crate) fn c_abs(a: C) -> f64 {
    a.re.hypot(a.im)
}
pub(crate) fn c_conj(a: C) -> C {
    C {
        re: a.re,
        im: -a.im,
    }
}

/// With exp(+i omega t), translated traces satisfy u(x + delta) = exp(-i k.delta) u(x).
pub(crate) fn floquet_factor(wave_vector: [f64; 3], translation: [f64; 3]) -> Result<C, String> {
    if wave_vector
        .iter()
        .chain(&translation)
        .any(|value| !value.is_finite())
    {
        return Err("Floquet wave vectors and translations must be finite.".into());
    }
    let angle = wave_vector
        .iter()
        .zip(translation)
        .map(|(wave, delta)| wave * delta)
        .sum::<f64>();
    if !angle.is_finite() {
        return Err("Floquet phase angle is not finite.".into());
    }
    Ok(C {
        re: angle.cos(),
        im: -angle.sin(),
    })
}

/// Weighted equivalence classes: value[node] = weight[node] * value[class].
/// This preserves translations, orientations, and consistency at periodic corners.
pub(crate) struct PhaseUnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
    weight: Vec<C>,
}

impl PhaseUnionFind {
    pub(crate) fn new(count: usize) -> Self {
        Self {
            parent: (0..count).collect(),
            rank: vec![0; count],
            weight: vec![C { re: 1.0, im: 0.0 }; count],
        }
    }

    fn find(&mut self, node: usize) -> (usize, C) {
        let parent = self.parent[node];
        if parent != node {
            let (root, parent_weight) = self.find(parent);
            self.weight[node] = c_mul(self.weight[node], parent_weight);
            self.parent[node] = root;
        }
        (self.parent[node], self.weight[node])
    }

    /// Impose value[upper] = phase * value[lower].
    pub(crate) fn constrain(&mut self, lower: usize, upper: usize, phase: C) -> Result<(), String> {
        if lower >= self.parent.len() || upper >= self.parent.len() {
            return Err(
                "A periodic constraint references an out-of-range degree of freedom.".into(),
            );
        }
        if !phase.re.is_finite() || !phase.im.is_finite() || c_abs(phase) < 1e-150 {
            return Err("A periodic constraint has a non-finite or zero phase factor.".into());
        }
        let (lower_root, lower_weight) = self.find(lower);
        let (upper_root, upper_weight) = self.find(upper);
        let expected = c_mul(phase, lower_weight);
        if lower_root == upper_root {
            let defect = (upper_weight.re - expected.re).hypot(upper_weight.im - expected.im);
            if defect > 1e-10 * c_abs(upper_weight).max(c_abs(expected)).max(1.0) {
                return Err(format!(
                    "Inconsistent Floquet constraints meet at periodic degree of freedom {upper} (phase defect {defect:.3e})."
                ));
            }
            return Ok(());
        }
        if self.rank[lower_root] >= self.rank[upper_root] {
            self.parent[upper_root] = lower_root;
            self.weight[upper_root] = expected.div(upper_weight);
            if self.rank[lower_root] == self.rank[upper_root] {
                self.rank[lower_root] += 1;
            }
        } else {
            self.parent[lower_root] = upper_root;
            self.weight[lower_root] = upper_weight.div(expected);
        }
        Ok(())
    }

    pub(crate) fn mapping(&mut self) -> (Vec<usize>, Vec<C>, usize) {
        let mut roots = BTreeMap::new();
        let mut dofs = Vec::with_capacity(self.parent.len());
        let mut weights = Vec::with_capacity(self.parent.len());
        for node in 0..self.parent.len() {
            let (root, weight) = self.find(node);
            let next = roots.len();
            dofs.push(*roots.entry(root).or_insert(next));
            weights.push(weight);
        }
        (dofs, weights, roots.len())
    }
}

#[cfg(test)]
mod floquet_tests {
    use super::*;

    fn close(first: C, second: C) {
        assert!(
            (first.re - second.re).hypot(first.im - second.im) < 1e-12,
            "{first:?} != {second:?}"
        );
    }

    #[test]
    fn floquet_sign_and_multiaxis_dot_product() {
        close(
            floquet_factor([std::f64::consts::FRAC_PI_2, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap(),
            C { re: 0.0, im: -1.0 },
        );
        close(
            floquet_factor([0.2, 0.3, 0.4], [2.0, -1.0, 0.5]).unwrap(),
            C {
                re: 0.3_f64.cos(),
                im: -0.3_f64.sin(),
            },
        );
        assert!(floquet_factor([f64::NAN, 0.0, 0.0], [1.0, 0.0, 0.0]).is_err());
    }

    #[test]
    fn phase_union_find_preserves_corner_products_and_rejects_conflicts() {
        let px = floquet_factor([0.4, 0.0, 0.0], [1.0, 0.0, 0.0]).unwrap();
        let py = floquet_factor([0.0, -0.7, 0.0], [0.0, 1.0, 0.0]).unwrap();
        let mut classes = PhaseUnionFind::new(5);
        classes.constrain(0, 1, px).unwrap();
        classes.constrain(0, 2, py).unwrap();
        classes.constrain(1, 3, py).unwrap();
        classes.constrain(2, 3, px).unwrap();
        let (dofs, weights, count) = classes.mapping();
        assert_eq!(count, 2);
        assert_eq!(dofs[0], dofs[3]);
        close(weights[3].div(weights[0]), c_mul(px, py));
        close(weights[4], C { re: 1.0, im: 0.0 });
        assert!(classes.constrain(0, 3, C { re: 1.0, im: 0.0 }).is_err());
        assert!(classes.constrain(0, 5, px).is_err());
    }

    #[test]
    fn phase_union_find_keeps_orientation_when_rank_reverses() {
        let mut classes = PhaseUnionFind::new(4);
        let phase = C { re: 0.0, im: -1.0 };
        classes.constrain(1, 2, C { re: 1.0, im: 0.0 }).unwrap();
        classes.constrain(0, 2, phase).unwrap();
        classes.constrain(2, 3, C { re: -1.0, im: 0.0 }).unwrap();
        let (_, weights, count) = classes.mapping();
        assert_eq!(count, 1);
        close(weights[2].div(weights[0]), phase);
        close(weights[3].div(weights[2]), C { re: -1.0, im: 0.0 });
    }
}

fn residual_vector(
    matrix: &[BTreeMap<usize, C>],
    xr: &[f64],
    xi: &[f64],
    br: &[f64],
    bi: &[f64],
) -> (Vec<f64>, Vec<f64>, f64) {
    let mut rr = vec![0.0; xr.len()];
    let mut ri = vec![0.0; xr.len()];
    let mut err = 0.0;
    let mut rhs = 0.0;
    for i in 0..xr.len() {
        let (mut sr, mut si) = (0.0, 0.0);
        for (&j, &a) in &matrix[i] {
            sr += a.re * xr[j] - a.im * xi[j];
            si += a.im * xr[j] + a.re * xi[j]
        }
        rr[i] = br[i] - sr;
        ri[i] = bi[i] - si;
        err += rr[i] * rr[i] + ri[i] * ri[i];
        rhs += br[i] * br[i] + bi[i] * bi[i]
    }
    (rr, ri, err.sqrt() / rhs.sqrt().max(1.0))
}

struct SymbolicCache {
    pattern: Vec<Vec<usize>>,
    symbolic: SymbolicLu<usize>,
}

#[cfg(feature = "rivrs-backend")]
struct RivrsCache {
    pattern: Vec<Vec<usize>>,
    augmented: bool,
    solver: SparseLDLT,
}

#[derive(Default)]
pub(crate) struct SolverCache {
    faer: Option<SymbolicCache>,
    #[cfg(feature = "rivrs-backend")]
    rivrs: Option<RivrsCache>,
}

pub(crate) fn complex_symmetry_defect(matrix: &[BTreeMap<usize, C>]) -> f64 {
    let mut defect: f64 = 0.0;
    for (i, row) in matrix.iter().enumerate() {
        for (&j, &a) in row {
            let b = matrix
                .get(j)
                .and_then(|row| row.get(&i))
                .copied()
                .unwrap_or_default();
            // Compare each mirrored pair at its own scale: a large conducting
            // interior must not hide asymmetric Floquet couplings in an air region.
            let scale = a.re.hypot(a.im).max(b.re.hypot(b.im));
            if scale > 0.0 {
                defect = defect.max((a.re - b.re).hypot(a.im - b.im) / scale);
            }
        }
    }
    defect
}

pub(crate) fn stage_started(stage: &str) {
    use std::io::{self, Write};
    let mut output = io::stdout().lock();
    let event = serde_json::json!({"event":"stage-start", "stage":stage});
    let _ = writeln!(output, "FULLMETA_STAGE {event}").and_then(|_| output.flush());
}

pub(crate) fn solve_linear_system(
    matrix: &[BTreeMap<usize, C>],
    rhs_re: &[f64],
    rhs_im: &[f64],
    tolerance: f64,
    backend: &str,
    ordering: &str,
    threads: usize,
    cache: &mut SolverCache,
) -> Result<(Vec<f64>, Vec<f64>, f64), String> {
    let n = matrix.len();
    let setup_started = Instant::now();
    if n == 0
        || rhs_re.len() != n
        || rhs_im.len() != n
        || rhs_re.iter().chain(rhs_im).any(|value| !value.is_finite())
        || matrix.iter().any(|row| {
            row.iter().any(|(&column, value)| {
                column >= n || !value.re.is_finite() || !value.im.is_finite()
            })
        })
    {
        return Err("The linear system must have finite coefficients and matching nonempty matrix and RHS dimensions.".into());
    }
    stage_started("symbolic-analysis");
    if backend == "rivrs" {
        #[cfg(feature = "rivrs-backend")]
        {
            let relative_asymmetry = complex_symmetry_defect(matrix);
            // Bloch reduction P^H A P generally loses complex transpose symmetry.
            // For that case solve the exact real operator B through [0 B; B^T 0]
            // rather than symmetrizing A or forming normal equations. The solution
            // lies in the second block; the auxiliary first block is zero.
            let augmented = relative_asymmetry > 1e-10;
            let n2 = n.checked_mul(2).ok_or("real-block dimension overflow")?;
            let block_n = if augmented {
                n2.checked_mul(2)
                    .ok_or("augmented real-block dimension overflow")?
            } else {
                n2
            };
            let mut triplets: Vec<Triplet<usize, usize, f64>> = Vec::with_capacity(
                matrix.iter().map(BTreeMap::len).sum::<usize>() * if augmented { 8 } else { 4 },
            );
            for (i, row) in matrix.iter().enumerate() {
                for (&j, &a) in row {
                    if augmented {
                        for (row, column, value) in [
                            (i, j, a.re),
                            (i, n + j, -a.im),
                            (n + i, j, a.im),
                            (n + i, n + j, a.re),
                        ] {
                            triplets.push(Triplet::new(row, n2 + column, value));
                            triplets.push(Triplet::new(n2 + column, row, value));
                        }
                    } else {
                        triplets.push(Triplet::new(i, j, a.re));
                        triplets.push(Triplet::new(i, n + j, -a.im));
                        triplets.push(Triplet::new(n + i, j, -a.im));
                        triplets.push(Triplet::new(n + i, n + j, -a.re));
                    }
                }
            }
            let real_block =
                SparseColMat::<usize, f64>::try_new_from_triplets(block_n, block_n, &triplets)
                    .map_err(|e| format!("rivrs real-block CSC construction failed: {e:?}"))?;
            let pattern: Vec<Vec<usize>> = matrix
                .iter()
                .map(|row| row.keys().copied().collect())
                .collect();
            let analyze_started = Instant::now();
            let had_cached_symbolic = cache.rivrs.is_some();
            let reused = cache.rivrs.as_ref().is_some_and(|existing| {
                existing.pattern == pattern && existing.augmented == augmented
            });
            if !reused {
                let strategy = match ordering {
                    "amd" => OrderingStrategy::Amd,
                    "metis" => OrderingStrategy::Metis,
                    _ => {
                        return Err(format!(
                            "Unknown rivrs solverOrdering '{ordering}'; use 'amd' or 'metis'."
                        ));
                    }
                };
                let solver = SparseLDLT::analyze_with_matrix(
                    &real_block,
                    &AnalyzeOptions { ordering: strategy },
                )
                .map_err(|e| format!("rivrs symbolic analysis failed: {e}"))?;
                cache.rivrs = Some(RivrsCache {
                    pattern,
                    augmented,
                    solver,
                });
            }
            let symbolic_status = if reused {
                "reused"
            } else if had_cached_symbolic {
                "rebuilt after matrix-pattern change"
            } else {
                "built"
            };
            println!(
                "  profile: rivrs real-block CSC build and symbolic analysis {symbolic_status} {:.3}s ({}× dimension; relative complex-symmetry defect {:.2e})",
                analyze_started.elapsed().as_secs_f64(),
                if augmented { 4 } else { 2 },
                relative_asymmetry
            );
            let parallelism = Par::rayon(threads.max(1));
            stage_started("factorization");
            let factor_started = Instant::now();
            let factor_options = FactorOptions {
                par: parallelism,
                ..FactorOptions::default()
            };
            let solver = &mut cache.rivrs.as_mut().expect("rivrs symbolic cache").solver;
            solver
                .refactor(&real_block, &factor_options)
                .map_err(|e| format!("rivrs numeric factorization failed: {e}"))?;
            println!(
                "  profile: rivrs real-symmetric LDLᵀ factorization {:.3}s",
                factor_started.elapsed().as_secs_f64()
            );
            stage_started("solve");
            let solve_started = Instant::now();
            let block_rhs = |real: &[f64], imaginary: &[f64]| {
                Col::<f64>::from_fn(block_n, |i| {
                    if i < n {
                        real[i]
                    } else if i < n2 {
                        if augmented {
                            imaginary[i - n]
                        } else {
                            -imaginary[i - n]
                        }
                    } else {
                        0.0
                    }
                })
            };
            let rhs = block_rhs(rhs_re, rhs_im);
            let req = solver.solve_scratch(1);
            let mut mem = MemBuffer::new(req);
            let solution = solver
                .solve(&rhs, &mut MemStack::new(&mut mem), parallelism)
                .map_err(|e| format!("rivrs solve failed: {e}"))?;
            let solution_offset = if augmented { n2 } else { 0 };
            let mut xr: Vec<f64> = (0..n).map(|i| solution[solution_offset + i]).collect();
            let mut xi: Vec<f64> = (0..n).map(|i| solution[solution_offset + n + i]).collect();
            if xr.iter().chain(&xi).any(|v| !v.is_finite()) {
                return Err("rivrs LDLᵀ returned non-finite values.".into());
            }
            let mut residual = residual_vector(matrix, &xr, &xi, rhs_re, rhs_im).2;
            for _ in 0..3 {
                if residual <= tolerance {
                    break;
                }
                let (rr, ri, _) = residual_vector(matrix, &xr, &xi, rhs_re, rhs_im);
                let correction_rhs = block_rhs(&rr, &ri);
                let req = solver.solve_scratch(1);
                let mut mem = MemBuffer::new(req);
                let correction = solver
                    .solve(&correction_rhs, &mut MemStack::new(&mut mem), parallelism)
                    .map_err(|e| format!("rivrs iterative refinement solve failed: {e}"))?;
                for i in 0..n {
                    xr[i] += correction[solution_offset + i];
                    xi[i] += correction[solution_offset + n + i];
                }
                residual = residual_vector(matrix, &xr, &xi, rhs_re, rhs_im).2;
            }
            println!(
                "  profile: rivrs triangular solve, refinement, and residual {:.3}s",
                solve_started.elapsed().as_secs_f64()
            );
            if !residual.is_finite() || residual > tolerance {
                return Err(format!(
                    "rivrs sparse LDLᵀ residual {:0.3e} exceeds tolerance {:0.3e}.",
                    residual, tolerance
                ));
            }
            return Ok((xr, xi, residual));
        }
        #[cfg(not(feature = "rivrs-backend"))]
        {
            return Err(
                "solverBackend 'rivrs' requires building with --features rivrs-backend.".into(),
            );
        }
    }
    if backend != "faer" {
        return Err(format!(
            "Unknown solverBackend '{backend}'; use 'faer' or 'rivrs'."
        ));
    }
    let triplets: Vec<Triplet<usize, usize, FaerComplex<f64>>> = matrix
        .iter()
        .enumerate()
        .flat_map(|(i, row)| {
            row.iter()
                .map(move |(&j, &v)| Triplet::new(i, j, FaerComplex::new(v.re, v.im)))
        })
        .collect();
    let sparse = SparseColMat::<usize, FaerComplex<f64>>::try_new_from_triplets(n, n, &triplets)
        .map_err(|e| format!("Sparse CSC construction failed: {e:?}"))?;
    let pattern: Vec<Vec<usize>> = matrix
        .iter()
        .map(|row| row.keys().copied().collect())
        .collect();
    let had_cached_symbolic = cache.faer.is_some();
    let reused = cache
        .faer
        .as_ref()
        .is_some_and(|cached| cached.pattern == pattern);
    let symbolic = if reused {
        cache
            .faer
            .as_ref()
            .expect("matched Faer symbolic cache")
            .symbolic
            .clone()
    } else {
        let symbolic = SymbolicLu::try_new(sparse.symbolic())
            .map_err(|e| format!("Sparse LU symbolic analysis failed: {e:?}"))?;
        cache.faer = Some(SymbolicCache {
            pattern,
            symbolic: symbolic.clone(),
        });
        symbolic
    };
    let symbolic_status = if reused {
        "reused"
    } else if had_cached_symbolic {
        "rebuilt after matrix-pattern change"
    } else {
        "built"
    };
    println!(
        "  profile: CSC build and symbolic ordering {symbolic_status} {:.3}s",
        setup_started.elapsed().as_secs_f64()
    );
    stage_started("factorization");
    let factor_started = Instant::now();
    let lu = Lu::try_new_with_symbolic(symbolic, sparse.as_ref())
        .map_err(|e| format!("Sparse LU factorization failed: {e:?}"))?;
    println!(
        "  profile: Faer sparse LU factorization {:.3}s",
        factor_started.elapsed().as_secs_f64()
    );
    stage_started("solve");
    let solve_started = Instant::now();
    let rhs = Col::<FaerComplex<f64>>::from_fn(n, |i| FaerComplex::new(rhs_re[i], rhs_im[i]));
    let solution = lu.solve(&rhs);
    let mut xr: Vec<f64> = (0..n).map(|i| solution[i].re).collect();
    let mut xi: Vec<f64> = (0..n).map(|i| solution[i].im).collect();
    if xr.iter().chain(&xi).any(|v| !v.is_finite()) {
        return Err("Faer sparse LU returned non-finite values.".into());
    }
    let mut residual = residual_vector(matrix, &xr, &xi, rhs_re, rhs_im).2;
    for _ in 0..3 {
        if residual <= tolerance {
            break;
        }
        let (rr, ri, _) = residual_vector(matrix, &xr, &xi, rhs_re, rhs_im);
        let correction_rhs =
            Col::<FaerComplex<f64>>::from_fn(n, |i| FaerComplex::new(rr[i], ri[i]));
        let correction = lu.solve(&correction_rhs);
        for i in 0..n {
            xr[i] += correction[i].re;
            xi[i] += correction[i].im;
        }
        residual = residual_vector(matrix, &xr, &xi, rhs_re, rhs_im).2;
    }
    println!(
        "  profile: Faer triangular solve, refinement, and residual {:.3}s",
        solve_started.elapsed().as_secs_f64()
    );
    if !residual.is_finite() || residual > tolerance {
        return Err(format!(
            "Sparse LU residual {:0.3e} exceeds tolerance {:0.3e}.",
            residual, tolerance
        ));
    }
    Ok((xr, xi, residual))
}

#[cfg(all(test, feature = "rivrs-backend"))]
mod floquet_linear_solver_tests {
    use super::*;

    #[test]
    fn small_floquet_couplings_are_not_hidden_by_a_large_material_coefficient() {
        let matrix = vec![
            BTreeMap::from([(0, C { re: 1e12, im: 0.0 })]),
            BTreeMap::from([(1, C { re: 1.0, im: 0.0 }), (2, C { re: 1.0, im: 0.0 })]),
            BTreeMap::from([(1, C { re: 2.0, im: 0.0 }), (2, C { re: 3.0, im: 0.0 })]),
        ];
        let mut cache = SolverCache::default();
        let (re, im, _) = solve_linear_system(
            &matrix,
            &[1e12, 2.0, 5.0],
            &[0.0; 3],
            1e-10,
            "rivrs",
            "amd",
            1,
            &mut cache,
        )
        .unwrap();
        assert!(cache.rivrs.as_ref().unwrap().augmented);
        for value in re {
            assert!(
                (value - 1.0).abs() < 1e-10,
                "Incorrect small-row solution: {value}"
            );
        }
        for value in im {
            assert!(value.abs() < 1e-10);
        }
    }

    #[test]
    fn nonsymmetric_floquet_operator_solves_without_changing_backend() {
        let nonsymmetric = vec![
            BTreeMap::from([(0, C { re: 3.0, im: 1.0 }), (1, C { re: 2.0, im: 2.0 })]),
            BTreeMap::from([(0, C { re: 4.0, im: -1.0 }), (1, C { re: 5.0, im: -2.0 })]),
        ];
        let symmetric = vec![
            BTreeMap::from([(0, C { re: 3.0, im: 1.0 }), (1, C { re: 2.0, im: 2.0 })]),
            BTreeMap::from([(0, C { re: 2.0, im: 2.0 }), (1, C { re: 5.0, im: -2.0 })]),
        ];
        let expected = [C { re: 1.0, im: 0.3 }, C { re: -0.2, im: 0.7 }];
        let mut cache = SolverCache::default();
        // Reuse the same pattern across symmetry changes, as in a k sweep.
        for (matrix, augmented) in [
            (&nonsymmetric, true),
            (&symmetric, false),
            (&nonsymmetric, true),
        ] {
            let rhs = matrix
                .iter()
                .map(|row| {
                    row.iter().fold(C::default(), |sum, (&column, &value)| {
                        c_add(sum, c_mul(value, expected[column]))
                    })
                })
                .collect::<Vec<_>>();
            let br = rhs.iter().map(|value| value.re).collect::<Vec<_>>();
            let bi = rhs.iter().map(|value| value.im).collect::<Vec<_>>();
            let (re, im, residual) =
                solve_linear_system(matrix, &br, &bi, 1e-10, "rivrs", "amd", 1, &mut cache)
                    .unwrap();
            assert!(residual < 1e-10);
            for i in 0..2 {
                assert!((re[i] - expected[i].re).abs() < 1e-10);
                assert!((im[i] - expected[i].im).abs() < 1e-10);
            }
            assert_eq!(cache.rivrs.as_ref().unwrap().augmented, augmented);
        }
    }
}
