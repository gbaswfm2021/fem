# 2D thermal conduction: model and implementation

This manual connects the `thermal-2d` model tree to its stationary conduction equation and linear-triangle solver. It is intended for users, scientists and developers. See the [project guide](../../docs/README.md), [Model 2 reference](../../docs/fullmeta-model-2.md), [scenario contract](../src/scenarios/thermal2d.json), and [solver](../src/prepared_scalar.rs). The application's numerical acceptance remains open as described in the project guide.

## 1. Physics and fields

This is a **stationary heat-conduction** problem in an x–y cross-section, not a wave or transient heat equation. Temperature `T` in kelvin is the primary field; the physical heat flux `q` in `W/m^2` follows Fourier's law:

```math
\mathbf q=-k\nabla T,\qquad \nabla\cdot\mathbf q=Q,
\qquad -\nabla\cdot(k\nabla T)=Q.
```

Here `k` is scalar thermal conductivity and `Q` is volumetric heat generation in `W/m^3`, zero when no Domain Source is enabled. The first two equations use only first-order spatial derivatives when flux is included; the last equation eliminates flux. This stationary formulation omits heat capacity, convection/radiation boundaries and thermal impedance; the separate transient formulation includes heat capacity.

The scenario declares `T` as a primary field and **no derived field variables in its plotting catalog**. The solver nevertheless calculates and exports `heatFluxByCell` as a derived per-triangle result; do not assume its components are exposed as selectable derived field symbols in the model tree. It also exports conductivity by cell.

## 2. Weak form and triangular fields

With a test function `s`, integration by parts gives

```math
\int_\Omega k\nabla s\cdot\nabla T\,dA
=\int_\Omega sQ\,dA-\int_{\partial\Omega}s\,\overline q_n\,ds,
\qquad \overline q_n=\mathbf q\cdot\mathbf n.
```

Dirichlet boundaries are imposed separately and the integral shown concerns the prescribed-flux part. `H1`-conforming nodal temperature is natural because the weak form uses its gradient. With triangle barycentric coordinates `lambda_i`,

```math
T_h=\sum_{i=1}^{3}T_i\lambda_i,
\qquad \mathbf q_h=-k\sum_{i=1}^{3}T_i\nabla\lambda_i.
```

The `lambda_i` are Whitney 0-forms, or linear Lagrange shape functions; there are three local nodal temperatures. For constant `k` per material and straight triangles, `grad(T_h)` and the recovered flux are constant within each cell. That cellwise polynomial representation does not by itself make the physical flux an intrinsically Whitney 3-form field, nor does the recovery impose a globally `H(div)`-conforming face/edge flux. A mixed flux–temperature formulation would make a different space choice. The PDE and weak formulation, not the Hodge star alone, guide the selection. See [finite element exterior calculus](https://www-users.cse.umn.edu/~arnold/papers/acta.pdf).

## 3. Materials and Physics boundaries

Each active domain has `thermalConductivity` (`k`), default `1 W/(m*K)`. The model contract requires a positive value. The thermal adapter evaluates material and boundary expressions from model parameters; spatially varying thermal material and boundary expressions are not implemented. Use a separate material assignment for each region requiring a different constant conductivity.

| Physics condition | Meaning |
| --- | --- |
| Dirichlet | One real scalar expression sets the temperature `T = value` in K; default `293.15 K`. |
| Neumann, field form | Two real expressions `fieldValues = [qx, qy]` in W/m² describe the in-plane heat-flux vector. The solver imposes its outward normal component. The accompanying scalar `value` must be zero. |
| Neumann, legacy scalar form | Without `fieldValues`, `value` directly prescribes outward normal heat flux `q·n` in W/m², including nonzero values. Default `0 W/m²`. |
| Impedance | **Not supported** for this profile. A convective or radiative law must not be inferred from this missing option. |

An exterior edge without a prescribed ordinary condition contributes no heat flux, equivalent to insulation, unless it participates in another applicable constraint. Conditions target boundary selections; conflicting temperatures at shared or periodically identified nodes are rejected. An enabled `periodicCondition` references an enabled matched mesh pair and requires zero phase: corresponding temperatures are equal. Ordinary conditions may not overlap active periodic endpoints.

### Exact Dirichlet and Neumann equations

Let `d` be the prescribed scalar temperature, `g = (gx,gy)` the prescribed heat-flux vector in model x–y coordinates, and `n` the exterior unit normal. The implemented boundary equations are

```math
\text{Dirichlet: }T=d,\qquad
\text{Neumann: }\mathbf n\cdot\mathbf q=\mathbf n\cdot\mathbf g,
\qquad
\mathbf n\cdot(k\nabla T)=-\mathbf n\cdot\mathbf g.
```

The Neumann contribution to the right-hand side is `-integral s (n·g) ds`, with test function `s`. Positive `n·g` means heat leaves the domain; negative means heat enters. Tangential components have no effect. The solver obtains the outward normal from the adjacent triangle, independently of edge vertex order. The two field expressions use `W/m^2` and, like the existing scalar boundary expression, must be real and depend only on model parameters: spatially varying thermal boundary expressions are not supported by this adapter.

When `fieldValues` is present, scalar `value` must evaluate to zero. Without the vector, the existing scalar prescribes outward normal heat flux directly. A nonperiodic, unassigned exterior edge has zero normal flux and is insulated. A zero Dirichlet temperature means 0 K; it is not the default insulation condition.

If a connected conduction component has no prescribed temperature, its total applied outward flux must equal its integrated heat generation. For each such component the solver uses the Study's area-weighted `meanTemperature` to fix the additive constant, default `0 K`; it rejects incompatible net heat load. A mean reference cannot compensate for an unbalanced physical heat source. Choose a meaningful mean if absolute temperature matters.

## 4. Mesh, Study and results

- **Geometry, selections and mesh:** define 2D domains and material assignments; use boundary selections for temperature or flux. The solver consumes linear triangular geometry and uses order-1 nodal temperature. For a 2D cross-section, integrated heat loads are per unit out-of-plane thickness.
- **Study:** select the stationary `T` formulation. Frequency and frequency sweeps do not apply. A stationary Study can declare `meanTemperature`; backend, ordering and thread choices control the linear solve. The adapter supports constant conductivity and boundary values per assignment.
- **Definitions and results:** parameter expressions can determine values. Thermal does not offer pointwise/global-variable or parameter-sweep scenario options; its scenario metadata declares primary and material plot sources and integration operators. The stationary solver writes nodal temperature, per-cell heat flux and conductivity, plus solver metadata; the summary CSV reports the stationary result. The integration declaration alone does not establish a configured stationary integration-result workflow. Field plots use `T` and material choices as declared. See [Model 2 results](../../docs/fullmeta-model-2.md).

Check a simple slab with prescribed temperatures or balanced inward/outward flux, confirm the flux sign and energy balance, and refine the mesh for nontrivial geometry or interface behavior. Do not interpret a stationary solution as a thermal wave or a transient temperature history.

The field-result JSON additionally declares `areaM2`, `boundaryLengthMByTag`, `boundaryOutwardPowerWPerMByTag` and `netBoundaryOutwardPowerWPerM`. Boundary power is calculated from the recovered cellwise flux and outward boundary normals, and is reported per metre of out-of-plane depth. It is a recovered approximation, so its net value need not exactly equal the integrated Domain Source on every mesh. These declarations require an updated FEM package; existing result files retain their original contents.

For a homogeneous slab `0 <= x <= L` with zero domain source, `T(0) = T_left`, `T(L) = T_right` and insulated top and bottom, the exact solution is `T(x) = T_left + (T_right-T_left)x/L` and `qx = -k(T_right-T_left)/L`. At the left boundary the outward normal is `-x_hat`, so its outward flux is `-qx`; at the right it is `qx`. This provides a direct sign and conservation check.

## Transient study

The [thermal2d scenario](../src/scenarios/thermal2d.json) supports both stationary and transient Studies. The transient form declares temperature as the state and uses volumetric heat capacity `C`, conductivity `k` and Domain Source `Q`. The shared sparse backward-Euler runner has passed native coordinator cases with space/time-dependent materials, sources and a prescribed-temperature boundary, plus a geometry-changing sweep. Live editor and deployed-host workflows still need visible verification.

The native scenario-balance test checks backward-Euler thermal energy decay and insulated heat conservation on a 2D simplex mesh. The separate `thermal_flux_trace_conserves_the_time_integrated_heat` test checks energy balance with a prescribed nonzero heat flux. Manufactured spatial refinement checks structured and perturbed-interior simplex meshes.

When a transient Study is present, the scenario enables its declared global-variable, pointwise-variable, integration-operator and parameter-sweep controls. Requested times can produce a checked global history CSV, with field snapshots optional; see [the shared transient contract](transient_study_contract.md) for output and supported limits.

The chosen time-dependent material law is

```math
C(\mathbf{x},t)\,\frac{\partial T}{\partial t}
-\nabla\!\cdot\left(k(\mathbf{x},t)\nabla T\right)=Q(\mathbf{x},t).
```

Here `C` multiplies the temperature rate; the formulation does not take the time derivative of `C T`. It requires a positive volumetric heat capacity supplied by the model, with no solver default. Its compatible unit is `W*s/(m^3*K)`, equivalent to J/(m³·K); conductivity uses `W/(m*K)` and the source uses `W/m^3`. Spatially and time-dependent expressions are evaluated at the integration point and time stage. Initial temperature and boundary data must be compatible.

The sparse finite-element system is

```math
M_C(t)\,\dot{\mathbf T}(t)+K(t)\,\mathbf T(t)=\mathbf f(t),
\qquad (M_C)_{ij}=\int_\Omega C(\mathbf{x},t)N_iN_j\,dA.
```

For the backward-Euler step from `t_(n-1)` to `t_n`, the solver assembles

```math
M_C(t_n)\frac{\mathbf T_n-\mathbf T_{n-1}}{\Delta t}
+K(t_n)\mathbf T_n=\mathbf f(t_n).
```

The scenario's `boundaryLaws` route prescribed temperature and outward scalar or vector heat flux through the generic boundary adapter. Tests cover time-varying boundary data, heat balance, initial-value consistency, output and native coordinator execution. Broader mesh/time convergence and visible editor/deployed-host compatibility remain to be verified. Frequency-domain dispersion cannot be substituted directly for a causal time-domain material law.

See the [transient equations and contract reference](transient_study_contract.md) for the shared state equations, sparse time-step formulation, units and supported limits.

## Domain Source

The current implementation supports Domain Source and Additional Domain Source as separate PHYSICS feature types. Each entry has its own domain selection and expression components; multiple entries of either type add, including on overlapping selections, while an absent type contributes zero. Expressions are sampled at volume quadrature points using physical coordinates and model parameters. Where harmonic studies are supported, their evaluations additionally provide `freq`, `frequency`, and `omega`; transient studies additionally provide time `t` in seconds. The scenario JSON declares each type’s physical meaning, units, component order and weak-form route. This implementation is undergoing verification; its availability in a deployed installation depends on compatible FEM and platform runtimes.

For this stationary cross-section, the source is volumetric heat generation. The 2D formulation is interpreted per unit thickness, and the pure-Neumann compatibility check includes the integrated source.

For source density Q in W/m^3, use `-div(k grad T) = Q` and add `integral(test * Q) dA` per unit out-of-plane thickness.

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.

## Boundary vector count validation

An enabled Cartesian Neumann heat-flux vector must have exactly two components. The scalar-only normal-flux form remains supported. Semantic model validation checks the primary `fieldValues` count from the scenario’s declared boundary input; the solver retains its own check. Enabled vectors with an invalid type or component count receive a semantic validation error. To repair one in the editor, turn off “Specify field components” and turn it back on to recreate the declared defaults; this reset replaces the existing components. Disabled conditions skip these semantic checks.


## Additional Domain Source

`additionalDomainSource` is a prescribed additive heat-flux vector **q_s** in the Fourier constitutive relation; it is not volumetric heating. In 2D it has in-plane components `(q_sx, q_sy)` in W/m². The existing `domainSource` remains volumetric heat generation **Q** in W/m³.

~~~text
q = -k * grad(T) + q_s
C * partial_t(T) + div(q) = Q

Additional weak load: integral(grad(test) dot q_s) dA
~~~

The 2D integral is per unit out-of-plane thickness. The JSON declares the two source components, units, and gradient-tested load used by stationary and transient scalar assembly. The [thermal 2D contract](../src/scenarios/thermal2d.json) supplies this meaning; the shared feature name does not hard-code thermal behavior into the assembler.

## Universal Eigenfrequency Study

The eigenfrequency Study is implemented in the shared FEM runtime and declared by all eight scenario packages. Each package points to the same generic preparation and execution route; its existing transient form contract supplies the state layout, derivative terms, spatial operators, materials, and boundary laws. The assembler does not branch on scenario ID.

For the selected formulation, stack its declared transient fields into $u$. With coefficients independent of time and the unknown eigenvalue, its source-free homogeneous transient system has the form

```math
M\,\dot u + K u = 0.
```

A modal solution $u(t)=\phi e^{st}$ gives the generalized sparse pencil

```math
(K+sM)\phi=0,\qquad \phi\ne0.
```

The eigenvalue is the complex temporal rate $s$ itself, in inverse seconds. The implementation does not square it, assume a Hermitian pencil, or impose reciprocal $+/-$ mode pairing. Under the application’s $e^{+i\omega t}$ harmonic convention, the derived complex frequency is $f_c=-i s/(2\pi)$. The UI shift is described as $s^{-1}$; its persisted unit token is $\mathrm{Hz}$, which has the same SI dimension and scale in the current legacy host.

The shared sparse solver uses shift-invert Arnoldi and sparse shifted solves. It applies homogeneous essential constraints and declared periodic reductions to both operators, discards infinite descriptor modes, checks the generalized residual, normalizes each mode, and records the parameter-case and mesh identity. Excitation terms are omitted: package-declared volume sources, prescribed natural loads, and incident fields are removed before the modal assembly. Essential constraints and homogeneous boundary operators, such as impedance terms, are retained.

Spatially varying coefficients and parameter sweeps are supported when each case resolves to a linear time-invariant operator. Coefficients or boundary data depending on time, frequency, the eigenvalue, or the unknown state are outside this Study’s contract. The reusable nodal adapter currently supports simplex P1; the oriented tetrahedral edge adapter supports orders 1 and 2. A requested element order outside those adapters is not supported by the modal runner.

For thermal scenarios, $s$ describes diffusion decay; a mode need not have an oscillatory frequency. The planned 2D fixed-frequency propagation Study with eigenvalue $k_z$ remains a distinct problem.

Automated checks cover the generic complex nonsymmetric pencil, descriptor infinite-mode filtering, load removal and boundary-operator retention across all eight packages, eigenfrequency preparation, UI/schema declarations, and one-simplex assembly-to-modal-solve smoke cases for each advertised package/formulation adapter (generalized residual below `1e-7`). The full native Rust suite passes: 315 passed, 0 failed, 3 ignored; the 12 targeted FEM Node tests also pass, including modal result selection and eigenfrequency declaration/gating for all eight packages. The FEM release executable and frontend builds succeed. Structured mesh-reference checks cover rigid-cavity acoustic modes in 2D and 3D, both EM2D polarizations, EM3D PEC modes with edge orders 1 and 2, thermal decay rates in 2D and 3D, and elastic plane-stress/plane-strain plus 3D periodic-longitudinal cases. Acoustic and thermal constant zero modes survive result snapshotting. A nontrivial 2D thermal smoke model in the live local FEM editor (31 nodes, 44 triangles) passed preparation and completed six eigenfrequency modes in 15.474 s, with the run recorded as completed. This confirms the local UI-to-native-solver path for thermal 2D; rendered modal fields and the other scenario UIs remain to be verified. The older host on port 8766 loaded the FEM app, but its Study menu exposes only Add Stationary study, so it cannot create an eigenfrequency step until it receives the updated FEM application schema/UI; runner compatibility there remains unverified. See the [universal implementation plan](implement_universal_eigenfrequency_study.md).
