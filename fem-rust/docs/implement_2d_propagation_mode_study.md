# Implementation plan: fixed-frequency 2D propagation modes

## Status

**The shared implementation, focused debugging, feature-specific acceptance, FEM-local packaging, and final FullMeta in-app browser demonstration are complete.** The fixed-frequency `propagationMode` Study is wired for Acoustic 2D pressure, both full-polarization EM2D parent formulations (`Ez` and `Hz`), and the Elastic 2D `planeStrain` parent. The shared solver returns the generally complex longitudinal wavenumber $k_z$ itself as its eigenvalue. The 363-test Rust suite completed with 333 passed, 27 failures, and 3 ignored; the failures are caused by three missing unit tokens in the fixed shared quantity catalog, as detailed below. All propagation-specific Rust, solver, JavaScript/UI, package, and browser checks pass.

This document began as the implementation plan for the propagation-mode item in [the prescribed longitudinal-wavenumber plan](implement_2d_longitudinal_wavenumber.md). It now records the implemented design, its numerical contract, verification results, the external shared-unit-catalog limitation, and the final user-facing cylindrical-waveguide walkthrough.

### As-built route and current scope

The implemented route reuses each selected scenario's existing sparse harmonic assembler. A JSON declaration selects the numerical family (`scalarSimplexP1`, `vectorSimplexP1`, or `deRhamTriangle`), the 2D operator formulation, homogeneous-boundary policy, and the quadratic longitudinal degree. Shared Rust code assembles the source-free harmonic matrix at $k_z=-h,0,+h$, recovers the sparse coefficient blocks $P_0,P_1,P_2$, and verifies the declared polynomial at $k_z=\pm2h$. The scale $h$ is an inverse mesh length. It then uses the shared sparse first-companion generalized eigensolver with state $(u,k_z u)$ and solves a complex, generally non-Hermitian pencil. Each published mode is checked against the original sparse polynomial residual. The returned and persisted eigenvalue is $k_z$ in `rad/m`; it is never squared, replaced by its magnitude, or expanded into a presumed reciprocal pair.

This is an algebraically general quadratic operator adapter, not a claim that the current scenario material laws implement nonreciprocal tensors. The current EM2D package remains scalar-isotropic. Future tensor/nonreciprocal contracts can use the same route only after their coefficient and field operators are declared and implemented. Current mode artifacts save pressure and cell-local axial velocity $v_z=k_zp/(\omega\rho)$ for Acoustic 2D, all six electric/magnetic amplitudes for EM2D, and $(u_x,u_y,u_z)$ plus six cell-local Cauchy stress amplitudes for Elastic 2D. Axial energy-flux/forward-backward classification is not currently emitted; mode direction remains unclassified in the UI.

## 1. Objective and scope

At a user-specified real frequency $\omega$, solve source-free cross-sectional models for nonzero mode shapes $\hat u(x,y)$ and generally complex $k_z$ under

```math
u(x,y,z,t)=\operatorname{Re}\!\left\{\hat u(x,y)e^{-ik_z z}e^{i\omega t}\right\}.
```

The cross-section, constitutive data, and homogeneous boundary descriptions must be invariant along $z$. Material expressions may depend on $x,y$, model parameters, and the specified frequency where the scenario's harmonic material contract allows it. They may not depend on the unknown $k_z$, time, or the eigenvector. A nonzero drive is not part of the eigenproblem; sources and incident-field amplitudes are excluded from mode assembly.

Initial scenario/formulation scope:

| Scenario | Initial mode field | Formulation boundary |
| --- | --- | --- |
| Acoustic 2D | Pressure amplitude, with axial velocity recovered by its contract | Scalar reciprocal fluid contract; source-free homogeneous transverse boundaries |
| EM2D | Coupled full-polarization electromagnetic state | Use the declared compatible electric/magnetic spaces and constraints; current package material law remains scalar-isotropic |
| Elastic 2D | Three-component cross-sectional displacement | Plane-strain **parent** with the 3D constitutive law and $(u_x,u_y,u_z)$; finite $k_z$ is not classical two-component plane strain |

Thermal 2D, plane-stress elasticity, and all 3D scenarios are out of scope. Adding a scenario later requires a declared operator formulation already expressible by shared capabilities, followed by its JSON contract and scenario checks. A new mathematical operator, basis, or constraint mechanism requires one shared-code extension before JSON can use it.

## 2. Mathematical and solver contract

The generic finite-dimensional problem is a sparse generalized pencil

```math
A(\omega)\,x=k_z B(\omega)\,x,
\qquad x\ne0.
```

Here $k_z$ is always the reported, stored, targeted, and dimensioned eigenvalue, in $\mathrm{rad/m}$ (equivalently inverse metres for spatial phase). The state $x$ can contain the physical fields plus formulation-required auxiliary variables. The eigenvalue must never be changed to $k_z^2$ for convenience.

The general solver must support complex, non-Hermitian pencils and must not assume reciprocal media, real eigenvalues, Hermitian $A,B$, opposite-sign eigenvalue pairs, or that the sign of $\operatorname{Re}(k_z)$ determines energy-flow direction. Lossless Hermitian cases are useful special cases, not the universal contract. In those special cases, Hermiticity may imply real eigenvalues under additional regularity and nonzero-denominator conditions; it does not grant the generic solver permission to impose a symmetry absent from the declared operator.

The finite-$k_z$ driven harmonic matrix is **not** generally a first-order physical-field propagation pencil, but it can be the source-free operator from which a longitudinal polynomial is extracted. The implemented route reuses each scenario's mesh, quadrature, field/basis infrastructure, coefficient evaluation, local operator kernels, constraint machinery and sparse master-pattern assembly. For contracts that declare a quadratic longitudinal operator, it samples the assembled matrix at $-h,0,+h$, recovers $P_0,P_1,P_2$, and verifies the polynomial at $\pm2h$. It then constructs a sparse first-companion pencil whose algebraic state is $(u,k_z u)$ and solves for $k_z$ directly. The companion state is numerical augmentation, not a claim that it is the scenario's physical field state. This route is the current implementation for all three opted-in scenarios. Acoustic's scalar reciprocal pressure form happens to have $P_1=0$ and includes $k_z^2$ in the operator; the eigenvalue is still $k_z$. EM2D and elasticity retain their generally nonzero linear coefficient. The continuous first-order pressure/velocity and Maxwell formulations elsewhere in this document are mathematical references, not the current discrete eigenvector layouts.

### Acoustic 2D contract

For the current scalar reciprocal fluid, let $a=1/\rho$ and $b=1/(\rho c^2)$. A first-order state $x=(p,v_z)^T$ yields the continuous pencil

```math
A(\omega)=\begin{bmatrix}
\omega b+\omega^{-1}\nabla_t\!\cdot(a\nabla_t)&0\\
0&\omega\rho
\end{bmatrix},\qquad
B=\begin{bmatrix}0&I\\I&0\end{bmatrix},\qquad
A x=k_z Bx.
```

The corresponding discrete state must use compatible pressure and axial-velocity spaces and the same weak boundary/interface interpretation as the acoustic contract. For real positive $\rho,c$, real nonzero $\omega$ and self-adjoint homogeneous transverse boundary/interface conditions, this special pencil is formally Hermitian with indefinite $B$. Where its denominator is nonzero, its Rayleigh quotient explains real $k_z$ for this special model. The reciprocal acoustic guide may physically have sign-related branches; the eigensolver must find and report modes independently.

### EM2D contract

The propagation formulation must start from the first-order Maxwell system, not from an assumption that the driven electric-field equation is a quadratic eigenproblem in $k_z^2$. A suitable continuous state target for the stated $e^{i\omega t-ik_z z}$ convention is $x=(E,H)^T$ with

```math
A(\omega)=\begin{bmatrix}
\omega\boldsymbol\epsilon&iC_0\\
-iC_0&\omega\boldsymbol\mu
\end{bmatrix},\qquad
B=\begin{bmatrix}0&-Z\\Z&0\end{bmatrix},\qquad
A x=k_z Bx,
```

where $C_0=\nabla_t\times$ and $Zf=e_z\times f$, with signs fixed against the repository's Maxwell convention. The actual discrete state must respect the declared Whitney edge/nodal spaces, tangential traces, material tensor ordering, and divergence/constraint treatment. The block $B$ may be singular; the solver must distinguish finite physical modes from infinite descriptor modes and constraint/spurious modes.

For lossless Hermitian constitutive tensors and suitable operator domains, this example can give a Hermitian pencil, including Hermitian non-reciprocal tensors. Current EM2D JSON material capabilities are scalar-isotropic, so tensorial/non-reciprocal materials are not part of initial UI acceptance. The generic algebra and solver must nevertheless avoid baking reciprocal $\pm k_z$ pairing into their design.

### Elastic 2D plane-strain-parent contract

For the three-component displacement, the declared longitudinal strain uses $\nabla_k=\nabla_t-ik_z e_z$ and produces a polynomial operator

```math
P(k_z)u=(K_0+k_zK_1+k_z^2K_2)u=0.
```

A candidate Hermitian augmented state is $x=(u,w)^T$ with $w=k_z u$. One possible lossless reference linearization is:

```math
A=\begin{bmatrix}-K_0&0\\0&K_2\end{bmatrix},\qquad
B=\begin{bmatrix}K_1&K_2\\K_2&0\end{bmatrix},\qquad
A x=k_z Bx.
```

The current implementation does **not** adopt this Hermitian block pencil. It uses the generic sparse first-companion construction of Section 2, checks the auxiliary relation as part of finite-mode recovery, filters infinite descriptor modes, and evaluates the original polynomial residual for every reported mode. The reference Hermitian form remains useful for mathematical comparison under its lossless assumptions; it is not an assumption of the implemented eigensolver.

The Hermitian form is a conditional lossless reference only. Complex loss, general constitutive tensors, or boundary operators can give a non-Hermitian pencil; the general propagation solver must preserve their actual algebra.

## 3. Scenario-neutral contract and assembly architecture

Each scenario JSON declares the propagation capability using the existing scenario-contract vocabulary where possible. Contract data identifies:

- the supported Study and formulation, and whether it is a first-order generalized pencil or a polynomial operator with a declared linearization adapter;
- state blocks, physical field components, finite-element spaces/orders, and result recovery maps;
- coefficient/operator blocks and their dependence on spatial coordinates, parameters, and the fixed frequency;
- source-free mode semantics, admissible homogeneous boundary/constraint types, and any declared boundary contributions to $A$ or $B$;
- normalization and any physically meaningful energy-flux or decay-direction calculation supported by the contract.

Names in those declarations describe mathematical roles, not scenario IDs. Shared Rust code reads the declaration and dispatches through the generic sparse/scalar/vector/H(curl) operator adapters. The solver and assembly route contain no scenario-ID branch. The current three scenario packages opt in through `propagationModes`; a missing capability is rejected during preparation. This adapter is generic over polynomial coefficients but it does not add future constitutive laws by itself: for example, current EM2D material JSON remains scalar-isotropic.

The fixed-frequency Study remains distinct from Harmonic response and temporal Eigenfrequency:

- Harmonic response takes $\omega$ and prescribed $k_z$ (currently implemented).
- Temporal Eigenfrequency takes the spatial operator and solves for temporal rate $s$ (already shared by scenarios).
- Propagation Mode takes a specified real $\omega$ and solves for complex spatial eigenvalue $k_z$.

The Study model needs a fixed frequency, requested mode count, complex target/shift, convergence tolerance, maximum iterations, and any generic filtering/sorting options required by the selected solver. Validate finite positive frequency where required, finite target components, positive mode count, and valid solver bounds before launch. A result ordering rule may sort by distance from target and then stable deterministic tie-breakers; it must not rewrite eigenvalues or infer missing signs.

## 4. Sparse master matrices and cache design

Retain sparse global assembly from local element contributions throughout. For the generalized pencil, construct the symbolic graph from the union of declared $A$/$B$ block couplings, field-space DOFs, and constraints. Assemble $A$ and $B$ into sparse matrices using stable global row/column maps; accumulate duplicate local contributions through the existing sparse master pattern. Polynomial augmentation adds state blocks to this graph without materializing dense matrices.

The cache key must include every property that changes graph structure: mesh/topology, field spaces and orders, block/linearization layout, active constraints, periodic reductions, and boundary-term structure. The numeric frequency, material values, target shift, and coefficients do not by themselves change the graph, but they do affect matrix values and factorization reuse. A changed mesh, order, or constraint graph must invalidate the appropriate symbolic cache. Never reuse a numeric factorization across changed numeric pencil values unless the solver explicitly supports and verifies a safe refactorization path.

Use sparse generalized shift-invert or another solver that directly supports the declared complex pencil. Do not form $B^{-1}A$, explicit sparse inverses, or dense global matrices. For $A x=k_z Bx$, a shift $\sigma$ targets the sparse shifted system $(A-\sigma B)$; ensure the transformed problem maps results back to the original $k_z$ correctly. Handle singular/descriptor $B$, infinite eigenvalues, singular shifts, and failed factorizations explicitly. A mode is publishable only after finite-value checks and the original pencil residual pass.

For polynomial operators, keep all coefficient blocks sparse and use a documented sparse linearization. Validate the original polynomial residual $\|P(k_z)u\|$ as well as the augmented pencil residual. Check the auxiliary-variable relation and filter spurious/infinite modes only by mathematical criteria declared by the adapter, never by a hard-coded scenario heuristic.

## 5. Boundary conditions, constraints, and source handling

Assemble a source-free eigenproblem. Omit Domain Source, Additional Domain Source, prescribed traction/current drives, incident amplitudes, and other affine excitation terms. Retain homogeneous essential conditions, periodic reductions, and homogeneous natural operators only when their propagation formulation is declared. Apply constraints consistently to every pencil block and to every augmented state copy.

Initially support the homogeneous boundary families that can be represented and verified for each opted-in scenario. If a boundary law depends on $k_z$, represent its full declared polynomial/operator dependence in the pencil; otherwise reject it clearly. Do not silently freeze it at $k_z=0$. Boundary and interface terms must be derived from the first-order or polynomial weak form, not copied blindly from the prescribed-$k_z$ driven matrix.

## 6. Results and user-visible behavior

For every returned mode, persist or expose:

- complex $k_z$ in $\mathrm{rad/m}$ and the specified frequency;
- cross-sectional primary fields and contract-declared derived fields, with the longitudinal factor $e^{-ik_z z}$ stated explicitly;
- normalized residual for the original generalized or polynomial eigenproblem;
- normalization and phase convention, mesh, formulation, homogeneous boundary/constraint identity, and parameter case;
- classification as propagating/evanescent only when a contract provides a meaningful flux or decay calculation. Show an indeterminate/unclassified state otherwise.

Mode amplitude and complex phase are arbitrary up to normalization. Do not label forward/backward using only $\operatorname{Re}(k_z)$. Where the physical model has reciprocal pairs, display the independently computed values; do not synthesize one mode from another. Keep existing harmonic and temporal-eigenfrequency result formats/workflows unchanged.

The UI should make the difference between a prescribed-$k_z$ driven solve and a fixed-frequency propagation-mode solve explicit. It should show the requested frequency/search settings and let users select a returned mode to view its cross-sectional fields and metadata. Errors for unsupported material, boundary, element-order, or constraint combinations should be visible before a long solve where possible.

## 7. Implementation and debugging sequence

The sequence deliberately finishes implementation debugging **before** creating or running the cylindrical-waveguide UI case.

1. [x] **Audit reusable runtime pieces.** The existing generalized sparse eigensolver, source-free spectral preparation, sparse master-pattern assembly, result/artifact path, Study schema, and UI projections have been traced and reused.
2. [x] **Finalize the current formulation adapter declarations.** JSON declares the degree, field family, operator formulation where needed, and homogeneous-boundary policy for Acoustic 2D, EM2D, and Elastic 2D plane-strain parent. Continuous Hermitian first-order forms remain references; the numerical implementation uses the verified polynomial adapter.
3. [x] **Implement generic sparse polynomial assembly.** The shared adapter samples the selected harmonic assembler, combines sparse coefficient graphs, verifies independent samples, and leaves scenario identity out of solver branches.
4. [x] **Implement the complex generalized solver route.** The sparse first-companion path supports complex shifts and nonsymmetric pencils, filters infinite descriptor modes, recovers the physical block, and checks the original polynomial residual.
5. [x] **Implement polynomial augmentation.** The shared companion state is $(u,k_z u)$. Tests cover direct non-paired roots, singular leading blocks, and residual checks; it does not adopt the scenario-specific Hermitian block candidate above.
6. [x] **Wire the three scenario JSON contracts and fields.** Acoustic 2D pressure, EM2D `Ez`/`Hz`, and Elastic 2D plane-strain parent are enabled. Plane stress, thermal, and 3D remain unavailable.
7. [x] **Integrate preparation, model/schema, runtime routing, result resolution, and UI projections.** Browser testing found the Study collection insertion factory had no corresponding Study context-menu action. Added the capability-gated `add-propagation-mode-study` action to `fem.schema.json`, added its contract assertion, rebuilt the package, and verified the action is visible and creates the Study in the live in-app browser.
8. [x] **Finish debugging without the cylindrical case.** The focused acoustic, EM2D, elastic, polynomial-sampling, nonsymmetric complex-pencil, descriptor-mode, axial-velocity, stress-recovery, result-adapter, preparation, and parameter-case checks pass. No cylindrical model was used to debug or qualify the implementation.
9. [x] **Finish focused independent acceptance and package the candidate.** Twelve Rust propagation tests, ten generic sparse-eigensolver tests, and 65 FEM Node contract/result/UI tests pass. The focused tests include acoustic guide and cutoff refinement, source-free impedance retention, signed direct-`kz` targeting, EM2D below-cutoff and lossy complex modes, elastic longitudinal and shear branches, deterministic target sorting, singular-shift and insufficient-convergence diagnostics, clustered/repeated modes, descriptor filtering, and a 1,000-DOF sparse pencil. The FEM frontend and local runtime package were rebuilt after the Study-menu repair. The broader Rust suite reports 333 passed, 27 failed, and 3 ignored; all 27 failures reproduce missing shared quantity-catalog tokens (`N/m^3`, `W/m^3`, or `W*s/(m^3*K)`) outside the propagation code. This is an identified fixed-platform limitation, not masked as a pass.
10. [x] **Post-debug user-facing cylindrical-waveguide walkthrough.** Created and ran the model through the FullMeta in-app tab browser; no project JSON was hand-authored and no OS automation was used. Verified preparation, Study creation through the context menu, six saved modes, field plots, positive and negative target branches, analytical dispersion, mesh refinement, and File > Open recovery. The model is saved at `projects/acoustic_room_2d/acoustic_cylindrical_waveguide_ui.json`. This is a usability demonstration, not a numerical acceptance test.

### Verification record (2026-11-22)

- Rust: `cargo +stable-x86_64-pc-windows-gnu test propagation_mode_ -- --nocapture` — 12 passed; `cargo +stable-x86_64-pc-windows-gnu test eigenfrequency:: -- --nocapture` — 10 passed.
- FEM JavaScript: `node --test applications/fem/fem-rust/tests/*.test.mjs tests/propagation-mode-result.test.js` — 65 passed, including the propagation-mode Study context-menu entry.
- Frontend: `npm run build:frontend` — passed (104 JSX sources; development revision `a5960af59e30`).
- Full Rust suite: 333 passed, 27 failed, 3 ignored. Each failure reports one of three unsupported quantity-catalog tokens: `N/m^3`, `W/m^3`, or `W*s/(m^3*K)`. `applications/fem/fem-rust/src/model_expression.rs` delegates unit lookup to the fixed FullMeta Core SDK (`platform/sdk-rust/core/src/lib.rs`, SHA-256 `09319D6829E7B9A03570261BF7522BF1CA76E0BEC6035F2CB5D964D859D486B5`), whose unit lookup delegates to the shared Core quantity service. The smallest reproduction is `cargo +stable-x86_64-pc-windows-gnu test eigenfrequency_execution::tests::thermal_decay_eigenvalues_converge_to_the_analytic_dirichlet_box_rates -- --exact`; expected: the declared thermal material unit resolves and the decay-rate test runs; actual: `Unsupported unit “W*s/(m^3*K)”.` This dependency is outside the propagation implementation and was not changed.
- Package: `applications/fem/build-runtime.ps1` — passed. Active bundle: `applications/fem/.runtime/x86_64-pc-windows-gnu/release`, containing `fullmeta-fem.exe` and the three FEM-local platform DLLs. The build script retained the prior bundle at `applications/fem/fem-rust/target/runtime-before-local-bundle-L1o6w5`.

| Staged file | SHA-256 |
| --- | --- |
| `fullmeta-fem.exe` | `60952A80FEF99F0CF12A8119E2D6F70E630A633A34D03102F61BB5F93A360D7B` |
| `fullmeta_core.dll` | `88652B945633BAC604CE1E282F01B87797BC02CC66CA663F70E160795F67A986` |
| `fullmeta_model.dll` | `389422F8D42C5395A0AF02BCF2F6150FB861BED384A4D15BE999F9142F73F159` |
| `fullmeta_operation_contracts.dll` | `36195E1D5342034CB2D6C0E39F1591FF9BB1F94A01C25A2410463EA00B19C4A7` |
- Browser: used the FullMeta in-app tab on `http://127.0.0.1:8765/`. The saved/reopened model path is `projects/acoustic_room_2d/acoustic_cylindrical_waveguide_ui.json`. The completed run reports six `propagationMode` modes at `2729.5072740260052 Hz`; the saved refined-mesh artifact has 1,091 nodes and 2,075 triangles. The model was reopened through File > Open and restored its selected mode plot and saved run.

## 8. Independent acceptance checks

These checks are separate from the final UI walkthrough. The recorded numerical comparisons use analytic dispersion or known matrix spectra and check the original operator residual; an eigensolver's convergence flag alone is not treated as physical validation.

### Generic operator and solver checks

- [x] Complex generalized and quadratic pencils return direct eigenvalues, including nonsymmetric cases with no assumed $\pm k_z$ pairing; returned modes are checked against the original pencil/polynomial.
- [x] Shift ordering, repeated and tightly clustered modes, singular shifts, insufficient convergence, and singular descriptor blocks are covered. Infinite descriptor modes are filtered while finite roots remain available.
- [x] Sparse-only execution is checked on a 1,000-DOF repeated spectrum. Sparse polynomial sampling preserves its linear coefficient, and independent samples check the declared degree.

### Scenario mathematics and regression checks

- [x] **Acoustic 2D:** the tests compare a homogeneous rigid-wall rectangular guide with $k_t^2+k_z^2=(\omega/c)^2$, verify cutoff and mesh refinement, and independently target positive and negative $k_z$. A homogeneous pressure-Dirichlet guide is checked separately. The result recovers axial velocity. A source-free projection test confirms incident impedance data is removed while its boundary operator is retained. That test initially exposed a mismatched ID in its fixture; correcting the fixture to use the model's keyed-entry ID made the test pass without changing production assembly code.
- [x] **EM2D:** both parent formulations are exercised with analytic isotropic PEC-guide dispersion and mesh refinement. Checks include a below-cutoff evanescent root, passive-loss complex $k_z$, all six recovered field amplitudes, and polynomial residuals. The solver does not create a negative-$k_z$ partner. Tensor-valued and non-reciprocal material laws remain unsupported by the current EM2D material contract.
- [x] **Elastic 2D plane-strain parent:** the analytic longitudinal and shear branches are checked. Tests verify all three displacement components, six recovered 3D stress components, linear $k_z$ contributions, and the original-polynomial residual. Plane stress remains unavailable for this Study.
- [x] **Study/schema/result compatibility:** JSON declarations gate the Study to the three opted-in formulations, preparation binds the fixed real frequency and complex $k_z$ target, and result tests reject mismatched Study types or non-finite wavenumbers.

The focused propagation, generic solver, FEM JavaScript, frontend, schema and packaging results are listed in the verification record above. The broader native Rust suite remains at 333 passed, 27 failed and 3 ignored solely because the fixed shared Core quantity service lacks three unrelated units; this regression is reported explicitly and is not counted as a passing check.

## 9. Final cylindrical acoustic UI demonstration (completed, non-gating)

After the preceding debugging and acceptance phases are complete, build the model in the FullMeta in-app/tab browser as a normal user would; do not hand-author project JSON or use a browser other than the requested in-app tab. Use a 2D circular cross-section of radius $R$, homogeneous $\rho,c$, and a rigid wall ($v_n=0$ / zero normal pressure gradient). Use no domain or boundary drive. Set the dimensionless frequency $k_0R=\omega R/c=5$ so several cutoffs are below the operating frequency.

For a circular rigid-wall acoustic guide, the exact transverse roots are $\chi'_{mn}$ satisfying $J_m'(\chi'_{mn})=0$, including the constant $m=0,\chi'=0$ mode. The continuum propagation spectrum is

```math
k_z R=\pm\sqrt{(k_0R)^2-(\chi'_{mn})^2}.
```

At $k_0R=5$, the first roots below cutoff are $0$, $1.84118378$ for $m=1$, $3.05423693$ for $m=2$, $3.83170597$ for the first nonzero $m=0$ root, and $4.20118894$ for $m=3$. Their analytical $k_z$ values follow directly from the formula. Nonzero azimuthal order has cosine/sine cross-sectional degeneracy in this ideal axisymmetric model; the reciprocal acoustic model predicts opposite-sign branches. Neither degeneracy pairing nor sign pairing may be imposed by the generic solver.

### Recorded in-app walkthrough

The user-visible model was created entirely through FullMeta at `http://127.0.0.1:8765/` and saved as `projects/acoustic_room_2d/acoustic_cylindrical_waveguide_ui.json`. The model uses `Rwg=0.1 m`, `rho0=1.225 kg/m^3`, `c0=343 m/s`, `f0=5*c0/(2*pi*Rwg)=2729.5072740260052 Hz` ($k_0R=5$), and a sound-hard circular boundary from the zero-normal-velocity natural condition. It has no Domain Source or boundary drive. A first mesh with $h_{max}=0.008$ m contained 637 nodes and 1,193 triangles. The refined mesh uses $h_{max}=0.006$ m and contains 1,091 nodes and 2,075 triangles. Six modes completed; raising the Krylov dimension from 32 to 96 was needed after the first request returned only two of six target-near modes.

The UI field-source panel displays complex `kz` in `rad/m`, the fixed frequency, mode index, and mesh counts. The saved domain plot visibly shows a circular radial mode and a separate higher-order angular mode with the expected multi-lobe shape. At a negative real search target of `-30 rad/m`, the same reciprocal radial branch was independently returned at `-31.968251887508643 rad/m`; targeting `+30 rad/m` returned `+31.96825188749323 rad/m`. The solver was not instructed to manufacture the opposite sign. The final saved file was reopened via File > Open; the 1,091-node result, six-mode run, selected mode, and plot were restored.

| Mode | Transverse root | Analytical $k_z$ (rad/m) | $h_{max}=0.008$ (rad/m) | Relative error | $h_{max}=0.006$ (rad/m) | Relative error |
| ---: | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | $m=0$, first nonzero radial root | 32.121689 | 31.968252 | -0.478% | 32.034041 | -0.273% |
| 2 | $m=3$ cosine/sine pair | 27.110905 | 26.887221 | -0.825% | 26.983620 | -0.469% |
| 3 | $m=3$ cosine/sine pair | 27.110905 | 26.886132 | -0.829% | 26.982974 | -0.472% |
| 4 | $m=2$ cosine/sine pair | 39.587418 | 39.541391 | -0.116% | 39.561169 | -0.066% |
| 5 | $m=2$ cosine/sine pair | 39.587418 | 39.541500 | -0.116% | 39.561284 | -0.066% |
| 6 | $m=1$ branch | 46.486603 | 46.479586 | -0.015% | 46.482600 | -0.009% |

The reported modes are the six branches nearest the positive target; this list does not include the constant-pressure $m=0$ mode at $k_z=50$ rad/m or assert the missing negative modes were returned in the same run. Nonzero-$m$ pairs are close but split slightly by the polygonal mesh. All four representative branch families moved closer to their continuum values after refinement. The in-app walkthrough is for user-perspective confidence only; it is explicitly not a numerical acceptance test or a release pass/fail criterion.

## 10. Completion definition

The propagation-mode feature's independent acceptance checks and final UI demonstration are complete. The repository-wide native regression suite remains red only on the fixed shared quantity-catalog dependency reproduction above; those 27 broader tests stay explicitly unverified until that dependency provides the three unit tokens. The three opted-in scenario contracts use the same scenario-neutral sparse pencil/solver path; the solver returns $k_z$ itself for complex and generic nonsymmetric pencils; polynomial modes are validated against their original operator; and the packaged UI creates, runs, plots, saves, and reopens the Study. Do not claim general reciprocal pairing, Hermiticity, or real eigenvalues. Document remaining material, boundary, and element-order limits in this plan and the scenario notes.
