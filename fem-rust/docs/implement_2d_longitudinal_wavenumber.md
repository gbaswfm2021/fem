# Implementation plan: generic 2D longitudinal-wavenumber studies

## Status and objective

**Implementation status (2026-11-22): prescribed-`kz` harmonic solving is implemented and built in the FEM-local runtime.** It is enabled by JSON contracts for Acoustic 2D pressure, EM2D full polarization, and Elastic 2D plane strain, and the FEM Study editor projects a complex real/imaginary input only for those declared harmonic formulations. The shared sparse assembler applies the test-side `+i*kz` and trial-side `-i*kz` operators; results record the requested frequency, complex `kz`, and `exp(-i*kz*z)` amplitude convention. Fixed-frequency propagation modes, with `kz` as eigenvalue, remain a separate later feature.

The prescribed-`kz` slice has passed analytic solve and transverse mesh-refinement checks for all three declared scenarios; acoustic and EM source-route checks, result metadata, and plane-stress rejection also have focused coverage. The FEM-local native candidate and browser runtime have been built, the staged native bundle matches its candidate, and the Study-projection test passes. Remaining verification limits are listed below; they do not stop users from running the implemented prescribed-`kz` Study. Continue through implementation, packaging, and checks rather than stopping after an audit, prototype, or numbered step. Report a blocker only when an external dependency or missing user decision prevents a necessary action, and keep progressing on independent work.

The first deliverable is prescribed-longitudinal-wavenumber harmonic solving. Fixed-frequency propagation-mode solving is a later, separate feature and is not required to make this deliverable usable.

## EM2D formulation-selection correction

The finite-\(k_z\) EM2D numerical path is a full-polarization operator, but the current scenario contract exposes it through the legacy \(E_z\) and \(H_z\) parent formulations. That is a formulation-selection defect: it makes a coupled solve appear to be an available finite-\(k_z\) option for either decoupled scalar polarization.

The corrected contract must add an explicit Full polarization Physics formulation. Legacy \(E_z\) and \(H_z\) remain valid for zero-\(k_z\) Harmonic, Transient, and temporal Eigenfrequency Studies. Prescribed-\(k_z\) Harmonic, Transient, and temporal Eigenfrequency studies, plus every fixed-frequency Propagation Mode Study, require Full polarization. Incompatible saved settings must produce a clear validation error; they must not be silently reinterpreted.

The common Harmonic Study remains shared. Its longitudinal-mode control selects the declared zero-\(k_z\) or prescribed-\(k_z\) regime, while the generic assembler consumes the selected formulation's contract. The proposed UI and Study compatibility design, including Full-polarization Transient and temporal Eigenfrequency support, is detailed in [the EM2D full-polarization plan](implement_em2d_full_polarization.md). This follow-up does not claim that the new Physics option or six-field transient form has already been implemented.

## Execution directive: continue until usable

This is an implementation task, not a request to stop after planning or after an intermediate phase. Treat the numbered sequence as an internal checklist. On each continuation, resume at the first incomplete or insufficiently verified item and keep implementing, repairing, building, packaging, and checking until the prescribed-`kz` harmonic Study can be configured and run end to end in FullMeta for every declared scenario/formulation. Do not report completion merely because the generic assembler, an individual scenario, backend tests, or a build is done. Do not ask whether to proceed between checklist items. If one verification path is blocked by an external dependency, record the exact blocker and continue all independent implementation and verification work.

The target is one generic extension to the existing sparse FEM assembler for cross-sectional 2D models whose fields vary along a suppressed spatial direction. The assembler must consume declared operators and scenario contracts; it must not branch on scenario ID, physics label, or scenario filename.

Only these scenario/formulation combinations opt in initially:

| Scenario | Initial finite-`kz` capability | Scope |
| --- | --- | --- |
| Acoustic 2D | Driven harmonic solve at prescribed frequency and `kz` | Scalar pressure equation, with the scenario-declared source and boundary routes. |
| EM2D | Driven harmonic solve at prescribed frequency and `kz` | Fully coupled polarization formulation; retain separate electric- and magnetic-current source meanings. |
| Elastic 2D, plane strain parent | Driven harmonic solve at prescribed frequency and `kz` | Cross-sectional 3D displacement and constitutive response. Plane stress is excluded. |

The first deliverable is the driven solve with prescribed `kz`. A separate fixed-frequency propagation-mode Study, with complex `kz` itself as the eigenvalue, follows only after the driven operators and their zero-`kz` limits are verified.

Thermal 2D, plane-stress elasticity, all 3D scenarios, and new material models are outside the initial scenario contracts. The generic assembler may support their future use without advertising those capabilities now. In particular, thermal does not acquire a harmonic Study as a consequence of this work.

## Physical convention and separation assumptions

Use the application's harmonic time convention and a longitudinal factor

```math
u(x,y,z,t)=\operatorname{Re}\!\left\{\widehat u(x,y)\,e^{-i k_z z}e^{+i\omega t}\right\},
\qquad \partial_z\mapsto -i k_z,
\qquad \omega=2\pi f.
```

`kz` has units `1/m`; it is independent of temporal frequency `f`/`omega`. The 2D mesh and its selections describe the cross-section, and result fields are cross-sectional amplitudes. Reconstructing a 3D field requires the declared longitudinal factor.

The separated model requires geometry, material coefficients, and boundary descriptions to be invariant in `z`. A prescribed source or boundary drive must have the same longitudinal dependence as the solved field. Spatial expressions continue to use the coordinates available on the cross-section. `kz = 0` must recover existing supported results.

Represent prescribed `kz` as a dimensioned complex Study value. Reuse the platform's typed complex-value representation if available; otherwise expose explicit real and imaginary parts, both with inverse-length units. Do not overload `frequency`, store an untyped TeX/string value, or silently discard the imaginary part. The default is zero. The Study value is resolved separately from material frequency expressions. Parameter-case evaluation must be deterministic if `kz` is declared from model parameters.

For a complex `kz`, the reduced operator is generally complex. In analytic propagation pencils, the formal test-side operator is the integration-by-parts partner at `-kz`; it does not replace `kz` with its complex conjugate. Preserve this convention explicitly in the generic form representation.

## Architectural requirements

### Scenario-neutral assembler capability

The generic source-code extension comes first. Extend the typed weak-form/operator vocabulary so a declared spatial derivative can be replaced by a study-supplied symbol such as `D_z = -i*kz`. The compiled form must be able to generate constant, linear, and quadratic powers of `kz`, as well as cross-component blocks. A derivative substitution is a generic operator rule, not an `if scenario == ...` branch.

The implementation may assemble at a prescribed `kz` directly, or lower a declared polynomial operator

```math
P(k_z,\omega)=A_0(\omega)+k_zA_1(\omega)+k_z^2A_2(\omega)+\cdots
```

to the existing sparse master-matrix machinery. It must retain all declared powers and block couplings. The assembler must not reduce every problem to `kz^2`, infer reciprocal symmetry, or choose a formulation from a scenario name.

Reuse existing element, quadrature, coefficient-evaluation, boundary, source, constraint, sparse-pattern, and complex-linear-solve infrastructure. Extend those interfaces only by generic typed operators and declared data. Keep global operators sparse; do not form a dense master matrix or explicitly invert a global mass/operator matrix.

### Sparse structure and reuse

Construct the structural pattern from the mesh, active field spaces, declared operator graph, and constraints. The numeric `kz`, frequency, and material values do not define a new graph. Reuse the sparse master pattern and compatible symbolic data across frequency/`kz` samples and parameter cases while topology, DOFs, formulation, and constraints remain identical. Reassemble numeric values for each sample. A remesh, changed DOF layout, or changed constraint map invalidates the applicable pattern and symbolic data.

At `kz = 0`, zero-valued longitudinal terms may remain structurally present; numeric cancellation must not corrupt cached locations. Tests and diagnostics should expose pattern identity and invalidation reasons using the existing cache conventions.

### Scenario JSON contracts

After the generic source path exists, add versioned declarations only to `acoustic2d.json`, `em2d.json`, and `elastic2d.json`. Each declaration must identify:

- which harmonic formulation opts in and whether finite `kz` is allowed;
- the `kz` value's symbol, unit, complex representation, and expression scope;
- the trial and test longitudinal derivative substitutions;
- fields, components, finite-element spaces, and material coefficients;
- each volume, surface, essential, natural, impedance, and periodic operator that changes under longitudinal reduction;
- Domain Source and Additional Domain Source meanings, component labels/order, units, and weak-form load route;
- recovered/output fields and their units;
- exact zero-`kz` compatibility behavior and unsupported combinations.

JSON describes mathematical operations and quantities. It must not select Rust functions by profile ID or embed scenario-specific callbacks. Build-time validation must reject malformed or incomplete operator declarations before a Study reaches assembly.

## Prescribed-`kz` driven harmonic solve

### Study and input behavior

Add a generic longitudinal-wavenumber input to a Harmonic Study only when the active scenario contract advertises it. The displayed default is `kz = 0` and must not alter existing saved model values unless the user commits an edit. The input is resolved for each frequency and parameter case before coefficient/source evaluation.

Provide a clear validation path for unsupported combinations. In particular, nonzero `kz` is rejected for elastic plane stress. Existing zero-`kz` plane-stress and plane-strain projects retain their current form, vector component counts, outputs, and result reading behavior.

Where the current Study UI cannot represent a typed complex inverse-length quantity from the scenario declaration, add the smallest FEM-owned typed control/projection needed to expose it. Do not encode scenario-specific choices in platform UI code. Keep the harmonic study and result route otherwise unchanged.

### Acoustic 2D contract

The scalar pressure amplitude obeys the reduced pressure equation

```math
-\nabla_t\!\cdot\!\left(\rho^{-1}\nabla_t p\right)
+k_z^2\rho^{-1}p-\omega^2(\rho c^2)^{-1}p=s.
```

The sesquilinear cross-sectional weak form uses the declared test-side partner at `-kz`; existing acoustic boundary and source signs remain controlled by their contract. For the current reciprocal scalar-fluid case, the out-of-plane velocity recovery is

```math
v_z=\frac{k_zp}{\omega\rho}.
```

Retain scalar pressure injection `Q` and distributed force density as distinct source quantities. The finite-`kz` contract must declare the longitudinal source component where required; legacy 2-component source data remain valid at `kz = 0`, with any absent new component handled only by an explicit declared zero default. The harmonic result must include pressure amplitude and the recoverable three-component velocity amplitude.

### EM2D contract

Nonzero `kz` couples the existing `Ez` and `Hz` reductions. It must use a full-polarization cross-sectional Maxwell formulation, not two independently solved scalar polarizations. Start with the current declared scalar isotropic material model while keeping the generic operator and solver able to retain nonsymmetric/non-Hermitian blocks in future contracts.

The field-space design is a phase gate before contract completion. Evaluate a compatible cross-sectional de Rham discretization: in-plane tangential electric components in a 2D `H(curl)` edge space and the longitudinal component in a compatible scalar space, or another explicitly justified stable formulation. Record continuity, interface traces, constraint/gauge treatment, and the supported element orders. Do not promote an element choice solely because it is easy to assemble.

The operator must use the full reduced curl, for example `C_k E = (nabla_t - i*kz*e_z) x E`, with declared constitutive coefficients and boundary terms. Preserve the separate physical meanings of electric current density `J_e` and equivalent magnetic current density `J_m`, their units, component order, and weak-load signs. The finite-`kz` contract must expose all components required by the coupled formulation; old `Ez`/`Hz` component inputs remain valid on the `kz = 0` legacy route.

At `kz = 0` with the current scalar isotropic material contract, the full formulation must separate into the existing `Ez` and `Hz` subproblems and reproduce their results. At nonzero `kz`, return the coupled electric components and declared magnetic recovery components with the existing phasor convention.

### Elastic 2D plane-strain contract

Finite `kz` uses the 3D parent constitutive law on a 2D cross-section. It is not the existing two-component plane-strain reduction with one derivative patched in. Use

```math
\nabla_k=\nabla_t-i k_z\mathbf e_z,
\qquad \varepsilon_k(\mathbf u)=\operatorname{sym}(\nabla_k\mathbf u),
\qquad \boldsymbol\sigma_k=\mathsf C:\varepsilon_k(\mathbf u),
```

with all three displacement components `(u_x,u_y,u_z)`. The harmonic volume weak form is

```math
\int_\Omega\varepsilon_{-k}(\overline{\mathbf v}):\mathsf C:\varepsilon_k(\mathbf u)\,dA
-\omega^2\int_\Omega\rho^*\overline{\mathbf v}\cdot\mathbf u\,dA
=\ell(\mathbf v),
```

where `ell` contains the scenario-declared body-force, traction, eigenstress, and other enabled boundary terms. Add the missing longitudinal input/output components through the contract while preserving existing 2-component values for the zero-`kz` path. Domain Source remains body-force density; Additional Domain Source remains eigenstress, with the full tensor component layout and units declared for the finite-`kz` form.

At `kz = 0`, the in-plane block must reproduce current plane-strain results. The out-of-plane shear block is an additional decoupled polarization; do not hide it or claim the complete three-component solution equals the legacy two-component result when that block is excited. Plane-stress models reject nonzero `kz` clearly.

### Results and sources

Keep the 2D mesh and cross-section sampling interfaces. Harmonic result metadata records the complex `kz`, units, frequency, scenario/formulation declaration, and the fact that field values are cross-sectional amplitudes with `exp(-i*kz*z)` dependence. Do not silently report amplitudes as a complete 3D field. Preserve old result formats when `kz = 0`; add only the metadata/field components required by the new form.

Sources and boundary values must be compatible with the selected longitudinal dependence. Each scenario contract owns the mathematical mapping, units, component order, and sign. Shared assembly consumes these declarations identically. Missing finite-`kz` components are either explicit zero defaults or clear validation errors; they are never guessed from another scenario's vector shape.

## Later feature: fixed-frequency propagation-mode Study

This is a separate Study from both Harmonic response and temporal Eigenfrequency. It is source-free, uses a specified real frequency and homogeneous transverse boundary conditions, and solves for generally complex `kz` and nonzero mode fields.

The dedicated implementation sequence, scenario formulations, sparse-pencil design, acceptance checks, and post-debug cylindrical acoustic UI walkthrough are specified in [the fixed-frequency propagation-mode plan](implement_2d_propagation_mode_study.md). The cylindrical walkthrough is deliberately a non-gating user-perspective demonstration, not an acceptance test.

Build the modal operator from the same declared longitudinal form used by the prescribed-`kz` solve. Where a contract yields a polynomial pencil in `kz`, preserve its full polynomial order and use a generic linearization/augmentation to obtain a sparse generalized problem with `kz` itself as eigenvalue. Do not solve for `kz^2` as the stored or returned eigenvalue. Do not assume `+kz/-kz` pairs, Hermiticity, real eigenvalues, or reciprocal energy flow. A reciprocal scalar acoustic contract may have such pair structure as a physical result; the general solver must not impose it.

Reuse the existing generic sparse eigensolver kernel only after confirming its descriptor, complex, non-Hermitian, singular-constraint, and polynomial-linearization requirements. Preserve sparse master matrices and sparse shift solves. Never build a dense global pencil or explicitly invert a sparse operator. Mode search settings must state count, target/shift, convergence tolerance, iteration limits, filtering, and deterministic reporting order.

The Study result records the complex `kz` eigenvalue in `1/m`, requested frequency, residual, normalization and phase convention, field amplitudes, formulation, boundary/constraint identity, mesh identity, and parameter case. Propagating/evanescent classification is based on physical flux or decay direction where the scenario contract supports that quantity. The sign of `Re(kz)` alone is not used as a universal forward/backward classifier.

## Validation and acceptance plan

### Generic assembler checks

- Substitute `kz = 0` into every declared operator and compare with its current 2D harmonic route.
- Exercise synthetic forms with constant, linear, and quadratic `kz` blocks, complex coefficients, nonsymmetric block couplings, and no scenario identifiers.
- Verify test-side `-kz` conventions and complex analytic dependence independently of `conj(kz)`.
- Confirm duplicate accumulation, boundary terms, loads, constraints, and sparse master-pattern identity across frequency/`kz` values.
- Confirm that changes to mesh, field-space layout, or constraints invalidate the correct pattern, while numeric `kz` changes only update matrix values.
- Include a generic nonreciprocal/nonsymmetric pencil with no opposite-sign pairing to prove the mode solver does not impose reciprocal assumptions. This is an operator test, not a claim that tensorial EM materials are part of the initial package contracts.

### Scenario regression and analytic checks

- **Acoustic 2D:** preserve all current `kz = 0` harmonic outputs. Compare a homogeneous fluid against `k_t^2+k_z^2=\omega^2/c^2`, check `v_z` recovery, source signs, transverse boundaries, and mesh convergence.
- **EM2D:** verify `kz = 0` decomposes to the existing isotropic `Ez` and `Hz` results; check all coupled components at nonzero `kz`, electric/magnetic current source signs, PEC/natural traces, interface continuity, dispersion in a homogeneous medium, spurious-mode behavior, and mesh convergence.
- **Elastic 2D plane strain:** verify the in-plane `kz = 0` block matches current plane strain and test the independent out-of-plane shear mode. At finite `kz`, verify all three displacement components, 3D strain/stress recovery, body force, traction and eigenstress terms, and homogeneous isotropic bulk dispersion. Confirm plane stress rejects nonzero `kz`.
- Sweep both positive and negative real `kz` where reciprocal scenario physics predicts equal spectra; keep this a scenario acceptance property, never a generic solver ordering rule.
- For the propagation-mode Study, check generalized/polynomial eigenpair residuals, cutoff and zero-mode handling, material-loss cases allowed by the contract, mode normalization, selection/reordering stability, and mesh convergence.
- Confirm that `kz = 0` existing saved projects, results, ordinary harmonic workflows, and parameter/frequency sweeps remain readable and unchanged.

Numerical verification must distinguish algebraic residuals from physical accuracy. A successful solve or low residual alone does not establish correct dispersion; use analytic homogeneous cases, boundary checks, and mesh convergence before advertising support.

## Implementation sequence

1. [x] **Audit the current assembler and solver interfaces.** Trace how 2D harmonic weak forms, complex coefficients, source and boundary declarations, sparse pattern construction, constraints, and results enter the shared assembly path.
2. [x] **Specify the generic form vocabulary and unit contract.** Define the longitudinal derivative and test-side convention, declared block layout, field-space declarations, and typed `kz` value; keep the contract data independent of scenario ID branches.
3. [x] **Implement the generic sparse assembly path.** Assemble longitudinal terms and block coupling through the declared weak forms, retaining the zero-`kz` route and sparse master pattern.
4. [x] **Implement the acoustic 2D prescribed-`kz` vertical slice.** Add its contract, input scope, field recovery, active axial-force source check, analytic solution, and transverse mesh-refinement check.
5. [x] **Implement elastic 2D plane-strain finite-`kz`.** Add the three-component cross-sectional form, 3D constitutive/strain recovery, and keep plane stress explicitly unsupported for nonzero `kz`.
6. [x] **Implement the full-polarization EM2D form.** Use the contract-selected Whitney edge × nodal space, retain independent electric and magnetic current routes, and verify the zero-`kz` polarization split.
7. [x] **Integrate Study settings, UI annotations, validation, and results.** Gate the typed real/imaginary `rad/m` fields by scenario contract and harmonic Study, preserve zero defaults, reject unsupported formulations, and publish longitudinal-amplitude metadata.
8. [x] **Verify and package the prescribed-`kz` capability.** Run analytic solves and transverse mesh-refinement checks for all three opt-in scenarios; check acoustic/EM source routing, elastic force/eigenstress/traction responses, the EM2D material-interface trace, result metadata and UI projection; then build/stage the FEM-local native and browser runtimes. The in-app browser editor loaded successfully during the final review; the existing open project is a 3D eigenmode model, so no project was altered for a live 2D prescribed-`kz` Study run.
9. [ ] **Future: implement the propagation-mode Study.** Reuse the declared operators; solve for generally complex `kz` itself, then add mode results and physical mode classification. This is not part of prescribed-`kz` driven solving.
10. [x] **Document final contracts and evidence.** Update the three opt-in scenario notes, recording implemented behavior and the remaining validation limits below.

### Current verification limits

- EM2D finite-`kz` analytic, transverse-refinement, electric/magnetic source-route, and two-material interface checks pass. The interface case verifies a shared global Whitney edge trace DOF and finite solution with small residual; a discontinuous-material mesh-convergence study is not part of this check.
- Elastic finite-`kz` plane-wave, transverse-refinement, body-force, spatially varying eigenstress, and natural-traction response checks pass, each with small residuals.
- The Study-projection test passes for declared acoustic pressure, both EM2D parent formulations, and elastic plane strain, and hides the field for transient, thermal, and plane stress. The active browser tab loaded the model editor successfully. A live UI run of a newly authored prescribed-`kz` 2D Study has not yet been performed; the shell cannot probe the local loopback host in this environment.
- The full native Rust suite finished in 129.98 s with 340 passed, 0 failed, and 3 intentionally ignored. The previously failing shared boundary-input tests now pass: an omitted scalar remains optional when a vector is supplied, while an explicitly supplied scalar is preserved and checked against the contract's zero-companion/zero-only policy. The three ignored cases are two opt-in timing benchmarks and the benign worker used by a controlled lifecycle test.

## Completion criteria

The prescribed-`kz` phase is complete only when all three declared scenario/formulation contracts use the same generic derivative and sparse assembly path, the zero-`kz` regression passes, the analytic and mesh checks pass, unsupported cases give clear errors, and result metadata identifies the longitudinal amplitude convention. The propagation-mode phase is complete only when the sparse solver returns converged complex `kz` values with verified residuals and no reciprocal-pair or `kz^2` assumption in the universal solver.

The status and verification notes above record the implementation and packaging work actually completed; the deferred propagation-mode Study remains unimplemented.
