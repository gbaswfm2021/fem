# Implementation plan: universal eigenfrequency Study

## Status and objective

This document records the design and implementation state of the universal eigenfrequency Study. The shared FEM route, sparse eigensolver, result handling, schema/UI settings, and JSON declarations are now present for all eight packages: acoustic 2D/3D, electromagnetic 2D/3D, elastic 2D/3D, and thermal 2D/3D. The route consumes each package's declared linear transient weak form and existing scenario-neutral sparse assembly infrastructure; it adds no scenario-specific Rust assembly branch.

**Verification status:** a fresh Windows GNU-target FEM Rust test build passes: 315 passed, 0 failed, 3 ignored (`fem-rust/target/eigenfrequency-current-runtime-20261121-135346.log`). The suite was run with explicit `FULLMETA_CORE_DLL`, `FULLMETA_MODEL_DLL`, and `FULLMETA_CONTRACTS_DLL` overrides pointing to the staged FEM-local bundle; otherwise SDK discovery selects older workspace DLLs first. The FEM-local native bundle was rebuilt from current sources with `build-local-bundle.mjs --offline --stage`; its prior four files are backed up under `fem-rust/target/runtime-before-local-bundle-nEepv5`. The frontend build succeeds. Twenty-nine targeted FEM Node tests pass, including eigenmode result selection, result refresh/table behavior, scenario index generation, and eigenfrequency declaration/gating across all eight packages. Rust tests cover a general complex nonsymmetric pencil, singular descriptor modes, source removal and operator retention across all eight packages, eigenfrequency preparation, and schema/UI declarations. One-simplex assembly-to-modal-solve smoke cases pass for every advertised package/formulation adapter, with generalized residual below `1e-7`. Structured-mesh analytical/refinement checks cover both thermal dimensions, both acoustic dimensions, EM2D, EM3D with edge orders 1 and 2, and elasticity in 2D plane stress/plane strain plus a 3D periodic longitudinal case. Acoustic and thermal constant zero modes and their saved snapshots are also checked. A nontrivial 2D thermal model in the live local FEM editor (31 nodes, 44 triangles) passed preparation and completed six eigenfrequency modes in 15.474 s; its run history records `completed`. Its saved mode-2 field artifact rendered in the Domain field plot with `s = -10.209437821078957 + 3.2e-15i s^-1` and a nonconstant spatial field. Old-Kernel compatibility was also verified using a fresh FEM 2.4.0 payload installed into the isolated Store 0.2.2.0 clone: its old Kernel served the Study UI, preparation completed, the modal solve returned six modes in 15.196 s, and the saved mode-2 field rendered. The clone Kernel SHA-256 is `D25643103C1534432A5B49F6CAE974002DE715063DF35698457394D4E1A293B3`; the FEM payload SHA-256 is `7505813B37B23F20461A54DAA932DAD9682F3A5F999E0B3B4C24EA74614FDB73`. The clone ran temporarily on port 8777 and was stopped; its previous FEM package remains at `.scratch/fem-store-compat-current-fem/fem-app-before-eigenfrequency-20261121`. The actual port-8766 installation was not changed. Live checks for the remaining scenario families remain outstanding. The earlier MSVC-target test attempt was blocked by the missing `link.exe`; using the installed GNU toolchain allowed a fresh test build and run.

The user-approved coefficient assumption for this Study is that all PDE and boundary-operator coefficients are independent of physical time and of the unknown eigenfrequency. Coefficients may still vary in space and by resolved model parameters. A parameter-sweep case therefore gets its own eigenproblem after its parameter values are resolved.

## Mathematical contract

For the source-free linear transient system represented by a scenario package, collect its state fields into `u`. With time-independent coefficients, each declared `state` or `conservedProduct` derivative term contributes to the same derivative operator. After spatial and boundary assembly, write

```math
M\,\dot{u}(t)+K\,u(t)=0.
```

Here `M` is the assembled sparse operator multiplying state derivatives and `K` is the assembled sparse algebraic/spatial operator. `M` may be singular: first-order state systems can include algebraic equations or constraints. Neither `M` nor `K` is presumed Hermitian, positive definite, or invertible.

Use the time-evolution ansatz `u(t) = phi exp(s t)`. The temporal eigenvalue is `s` itself:

```math
(K+sM)\,\phi=0,
\qquad\text{equivalently}\qquad
(-K)\,\phi=sM\,\phi,
\qquad \phi\ne0.
```

This is a sparse generalized descriptor eigenproblem. The solver must find finite eigenvalues `s` without replacing them by `s^2`, `omega^2`, or a scenario-specific reduced equation. With the existing harmonic convention `exp(+i omega t)`, the reported complex frequency is derived as

```math
f_c=-\frac{i s}{2\pi},
\qquad s=i\,2\pi f_c.
```

For an undamped oscillatory mode, `s=+i omega` maps to positive real frequency. A damped mode can have a complex `s` and complex `f_c`. Diffusive thermal modes can have negative real `s` and no oscillatory part; they should be reported as decay modes (with their derived complex frequency shown only as a consistent conversion), not misrepresented as resonant oscillations.

Time- and frequency-independent coefficients make this pencil linear in `s`; they **do not** make it Hermitian or guarantee real eigenvalues. Hermitian `K phi = omega^2 M phi` is only a special reduction for appropriate conservative, lossless second-order physics. The shared route must preserve general complex, nonsymmetric operators and must not assume eigenvalues occur in `+/-` pairs. This is essential for non-reciprocal or otherwise non-symmetric contracts.

## Scope and eligibility

An eigenfrequency case is an unforced, homogeneous-boundary modal problem:

- Linear constitutive and boundary laws only; no unknown-dependent coefficients or nonlinear state laws.
- All coefficient expressions used by the eigenproblem may use space and resolved parameters, but must not use `t`, `freq`, `frequency`, `omega`, or another eigenvalue alias. Reject unsupported symbols during preparation with a path-specific message.
- Domain Source, Additional Domain Source, and other applied loads contribute no right-hand side to a free eigenmode. Their expressions are not compiled or sampled for this Study; they remain available to other Study steps in the same model.
- Essential conditions are homogeneous for an eigenmode. Apply them consistently to both operators, or use a homogeneous nullspace/constraint projection. Reject nonzero prescribed values instead of treating them as forcing.
- Include a natural or impedance boundary operator only if the scenario's time-domain contract expresses it as a linear, time-independent operator in the shared `M`/`K` assembly. Frequency-dependent impedance, memory kernels, and other dispersive boundary laws are outside this first version.
- Apply the same supported fixed periodic identification to both operators. The reduced operators are `Pᴴ M P` and `Pᴴ K P`; a mesh/topology change invalidates that map and its sparse pattern.
- Nonzero forcing, time-varying material data, frequency dispersion, nonlinear materials, and studies whose equations are not represented by a declared transient form are ineligible.

These restrictions define what “time-independent and frequency-independent coefficients” means operationally. They do not add physical loss or damping that is absent from the scenario's transient equations. A time-independent damping term can participate only when it is already declared and assembled as part of that linear transient operator.

## Reuse of current FEM infrastructure

The key architecture boundary is one reusable scenario-neutral operator assembly path shared by the transient and eigenfrequency Studies. It should return the declared derivative operator(s), algebraic operator, load, field layout and constraint metadata for a resolved model state. The transient time integrator consumes it at its required time stages; the eigenfrequency solver consumes the time-invariant operators once per parameter case. The time integrator and eigensolver are separate consumers, but element weak-form assembly, coefficient evaluation, boundary routing, orientation handling, sparse master-pattern construction, and constraint reduction remain common. This means a defect fixed in one of those shared operations benefits both Studies, and a regression in either consumer can be checked against the same operator contract.

1. **Factor operator assembly from time stepping.** Refactor the transient assembly boundary so a caller can request the declared derivative matrices and algebraic operator once at a resolved parameter case, without choosing a time step or assembling an initial condition. The transient runner continues to use its existing time-sampling path. The eigenfrequency runner calls the same scenario-neutral form, coefficient, element, boundary, orientation, and constraint assemblers.
2. **Combine derivative terms.** For time-independent coefficients, sum all `State` and `ConservedProduct` sparse blocks into `M`. Keep the existing declared term ordering and field block layout; do not infer physics from a scenario ID. Assemble the `timeDerivative: none` terms and eligible algebraic boundary terms into `K`.
3. **Preserve sparse structure.** Assemble `M` and `K` in sparse master patterns derived from the same mesh, field spaces, and constraints. Reuse the topology-derived row/column locations for both matrices and for parameter-sweep cases with unchanged topology. Do not form a dense global matrix or explicitly compute `M⁻¹K`.
4. **Reduce homogeneous constraints before solving.** Apply essential DOF elimination or a generic projection to both matrices. Apply periodic identifications using the same weighted map as the supported Study route. Preserve complex entries and conjugate test-side projection.
5. **Introduce an eigenvalue iteration, retaining `rivrs-sparse` for linear solves.** The current code has sparse direct linear solve backends but no shared sparse generalized eigensolver route. `rivrs-sparse` provides a sparse symmetric-indefinite LDLᵀ factorization, symbolic reuse, and repeated linear solves; `solver_common` already adapts complex and nonsymmetric systems through a real-block/augmented formulation. That makes it a candidate shifted-solve kernel for a shift-invert Arnoldi method, subject to numerical and performance verification. It is not itself the generalized eigensolver: a separate iterative layer must implement eigenvalue extraction for complex, nonsymmetric pencils and finite eigenvalues when `M` is singular. Verify convergence behavior and build compatibility, and do not silently route a large sparse problem through dense diagonalization.
6. **Handle descriptor spectra.** Singular `M` can produce infinite eigenvalues associated with algebraic constraints. The algorithm must target finite eigenvalues of a regular pencil, report singular/indeterminate pencils clearly, and avoid classifying constraint modes as physical modes. If the chosen algorithm cannot robustly handle a declared descriptor pencil, add generic algebraic elimination based on declared block structure or reject that form explicitly; never add a scenario-name workaround.
7. **Keep sparse symbolic reuse safe.** Reuse sparsity patterns and compatible symbolic factorization across modes and parameter cases only while mesh, DOF layout, constraints, and pattern identity match. Recompute numerical values for each parameter case. Shifted numerical factors depend on the shift and parameter values and must not be reused as if they were unchanged.

## Scenario contract and study settings

Each of the eight JSON packages should opt into a common `eigenfrequency` Study capability only if its declared transient form satisfies the eligibility rules. The JSON remains responsible for physical field declarations and operators; shared Rust code owns operator assembly, constraints, eigensolver orchestration, residual checks, and result serialization. Any additional scenario-specific metadata must be declarative, validated at build time, and describe supported generic operations rather than select a Rust branch.

The contract should identify, by existing field IDs and term declarations:

- which transient form defines the linear modal operator;
- coefficient and boundary inputs that are allowed in this Study and their evaluation scope;
- state fields and component units to save for each mode;
- any declared derived-field recovery operation that is valid at an eigenvalue;
- explicitly recognized physical nullspaces (for example, rigid-body or constant-potential/pressure modes) where the package already has a generic declaration for them.

Study settings should initially include a requested mode count, a target/shift in complex frequency or a clearly documented temporal-eigenvalue shift, convergence tolerance, iteration limit, and deterministic mode-selection/order policy. The UI and request contract must document the unit conversion and search target. A real target frequency can be converted to `s_target = i 2 pi f_target` under the stated convention. Avoid requiring a Hermitian-only setting such as “smallest positive `omega²`” in the universal API.

## Sparse result contract

Publish one record per converged finite eigenpair. Store at least:

- complex temporal eigenvalue `s` in `1/s` and derived complex frequency `f_c` in Hz;
- normalized eigenpair residual and solver convergence metadata;
- the normalization rule and a deterministic phase/sign convention for display;
- mode state fields in their declared finite-element spaces, preserving edge/face orientation and component ordering;
- Study ID, parameter-sweep case, scenario form ID, mesh/topology fingerprint, and constraint-map identity.

Mode amplitude is arbitrary. Normalize by a declared generic norm when possible, with a robust fallback for indefinite or singular `M`; never claim mass orthogonality unless the operator assumptions establish it. If the result pipeline cannot represent mode-indexed complex snapshots, add a generic FEM result format and resolver/UI support rather than encoding eigenmodes as fake frequency samples or transient times. Derived fields that divide by `s` or `omega` need explicit zero-mode guards.

## Eight-scenario coverage map

| Scenario | State-space source | Modal interpretation and scenario checks |
| --- | --- | --- |
| Acoustic 2D | Declared pressure/velocity transient form | Cross-sectional acoustic resonances/decays; keep the planned fixed-frequency `kz` propagation problem separate because it has `kz`, not temporal `s`, as its eigenvalue. Account for rigid-wall constant-pressure zero modes and guard velocity recovery at zero temporal frequency. |
| Acoustic 3D | Declared pressure/velocity transient form | Cavity acoustic resonances/decays; validate rigid-cavity zero modes, pressure/velocity recovery, and homogeneous boundary treatment. |
| EM2D | Declared electromagnetic transient form and its actual polarization/state layout | Cross-sectional Maxwell modes. Preserve any coupled field blocks represented by the form; do not infer decoupling from current zero-`kz` harmonic examples. Check gradient/null modes and zero-frequency recovery limits. |
| EM3D | Declared electric/magnetic transient form using the supported oriented edge spaces | Maxwell resonances/decays; preserve order-1/order-2 edge orientation and constraints. Identify curl-curl gradient/null modes without discarding physical modes by a scenario-specific frequency cutoff. |
| Elastic 2D | Declared displacement/velocity transient form and selected plane formulation | In-plane elastic modes/decays. Validate the selected plane formulation, per-unit-thickness integration, and free-body rigid modes; do not treat a driven harmonic peak as an eigenpair. |
| Elastic 3D | Declared displacement/velocity transient form | Three-dimensional elastic modes/decays; check six rigid-body modes for an unconstrained connected body and stress recovery only where defined. |
| Thermal 2D | Declared temperature transient form | Diffusive decay spectrum, generally non-oscillatory for passive symmetric conductivity and positive capacity. Use it to validate decay rates, constant-temperature zero modes for pure Neumann boundaries, and the expected 2D per-unit-thickness form. |
| Thermal 3D | Declared temperature transient form | Diffusive decay spectrum; validate 3D decay rates, the pure-Neumann constant mode, and heterogeneous spatial coefficient sampling. |

All eight packages currently declare the shared eigenfrequency route through their transient forms. This map still defines the physical verification target: package opt-in does not replace scenario reference checks. If a transient form is missing a physically required linear term or boundary operation, correct that scenario JSON contract and generic capability; do not compensate in the eigensolver.

## Implementation phases

### Phase 1 — Contract audit and operator extraction (implemented)

- Audit all eight `transientForms`, coefficient bindings, state spaces, boundary laws, periodic constraints, and existing derived-field recovery.
- Mark any form that cannot be represented as a homogeneous, linear, time-invariant pencil; record the exact missing contract rather than assuming an equation.
- Refactor shared assembly to expose `(M, K, layout, constraints, field metadata)` independently from the time integrator. Confirm existing transient results are unaffected by this structural refactor.

### Phase 2 — Study preparation and validation (implemented)

- Add generic study parsing, preparation request, fingerprinting, result paths, and schema/UI metadata for `eigenfrequency`.
- Validate static coefficient scope, linear form eligibility, homogeneous constraints, source-free modal semantics, finite targets, and per-parameter-case operator inputs.
- Add scenario JSON declarations for all eight packages and synchronize each `docs/notes_<scenario>.md` with its actual mathematical interpretation.

### Phase 3 — Sparse generalized eigensolver (implemented; automated scenario validation passes)

- Choose and integrate the complex nonsymmetric generalized sparse eigensolver backend; document why it handles singular `M` and how shifted sparse solves are performed.
- Implement target-based extraction, finite-eigenvalue filtering, mode convergence, normalization, phase fixing, residual calculation, and clear errors for non-convergence or singular pencils.
- Retain sparse master matrix patterns and avoid dense `M⁻¹K` formation.

### Phase 4 — Results and user workflow (implemented; additional live result validation remains)

- Add the mode snapshot format, mode indexing/filtering, complex frequency metadata, field rendering, and CSV/export support as appropriate.
- Ensure run identity is tied to the selected Study and parameter case; changes in mesh, constraints, coefficients, target, or eigensolver settings invalidate stale results.
- Verify multiple Study steps can share a project while each step uses its own time/frequency scope. Existing harmonic and transient source expressions remain scoped to their respective Study types.

### Phase 5 — Verification and rollout (in progress)

- Automated reference/refinement checks now cover the scenario families listed in the verification status above; these include thermal Dirichlet decay rates, rigid acoustic cavities, two EM2D polarizations, order-1/order-2 EM3D PEC modes, and the elastic plane/periodic-longitudinal references. Constant acoustic/thermal zero modes survive result snapshotting.
- The passing suite covers a complex nonsymmetric pencil, singular descriptor-mass filtering, thermal and acoustic null modes, constraints and supported periodic reductions, plus first-order temporal eigenvalues directly. Structured references verify the conservative second-order special cases without imposing `+/-` pairing on the general solver.
- Add explicit regression cases for repeated/tightly clustered modes and multi-case eigenfrequency parameter sweeps; the current modal suite's smoke and analytic cases use single target/case configurations.
- The local FEM-bundled runtime completed a nontrivial thermal 2D modal solve from the live editor (six modes; completed run record), and the saved mode-2 field artifact rendered through a Domain field plot. The same Study UI, preparation route, six-mode solve, and mode-2 rendering also passed on an isolated clone of the Store 0.2.2.0 Kernel after installing the current FEM payload. The actual port-8766 installation was untouched. Live workflows for the other scenario families remain unverified.

## Acceptance criteria

1. One eigenfrequency orchestration and one generic sparse pencil assembly path serve all eight scenarios; no scenario-ID solver switch is added.
2. Transient and eigenfrequency Studies use the same scenario-neutral weak-form, boundary, sparse-pattern, and constraint-assembly implementation. A shared assembler fix is covered by shared operator checks and transient regressions.
3. The solver consumes the declared time-invariant transient operators and solves `(-K)phi = s M phi` for `s` directly, including complex nonsymmetric pencils; it does not require a Hermitian problem or replace `s` by its square.
4. No dense global operator or explicit inverse is formed for production-sized meshes.
5. Homogeneous essential/periodic constraints are applied consistently to both operators. Ineligible nonzero, time-varying, frequency-dependent, nonlinear, or memory-dependent inputs fail preparation with actionable errors.
6. Every returned mode passes the configured generalized residual tolerance and is associated with its mesh, Study, constraints, and parameter case.
7. All eight notes distinguish this generic temporal eigenproblem from any special conservative squared-frequency reduction and identify thermal results as decay modes where applicable.
8. General-pencil, descriptor, preparation and package-contract checks pass. One-simplex assembly-to-modal-solve smoke cases pass for every advertised package/formulation adapter. Analytic/refinement checks cover all eight scenario packages as summarized above, and acoustic/thermal zero-mode snapshots pass. The local FEM-bundled executable and an isolated old-Kernel host have both completed and rendered a thermal 2D modal solve. Rendered live results across the remaining scenario families remain outstanding; the actual deployed application installation was not modified or tested in this turn.

## Known limits and decisions to resolve during implementation

- The generalized eigensolver is the FEM-owned shift-invert Arnoldi layer in `src/eigenfrequency.rs`; sparse shifted linear solves use the existing solver backend. Only the projected Krylov matrix is diagonalized densely. `rivrs-sparse` remains a sparse linear solver backend, not the eigenvalue iteration itself.
- The scenario notes preserve restricted conservative reductions as special-case references and now separately document the universal temporal pencil.
- A frequency-independent coefficient can still be complex or non-Hermitian. Whether each scenario's current transient contract can represent the corresponding time-domain physics must be checked per contract; the eigensolver itself must not impose reciprocity or Hermiticity.
- The meaning of “frequency” for a purely decaying thermal mode differs from an oscillatory resonance. The UI/result contract retains `s` as the primary eigenvalue and presents the decay interpretation clearly.
- Fixed-frequency propagation in 2D with eigenvalue `kz` is a different Study and a different operator pencil. This plan neither implements nor changes that planned propagation capability.
