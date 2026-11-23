# 3D pressure acoustics: model and implementation

This manual describes the current `acoustic-3d` scenario for users, scientists and developers. It connects the model tree to the frequency-domain pressure equation and the linear-tetrahedron solver. It describes implemented behavior, not a guarantee of numerical acceptance. See the [project guide](../../docs/README.md), [Model 2 reference](../../docs/fullmeta-model-2.md), [scenario contract](../src/scenarios/acoustic3d.json), and [solver](../src/prepared_frequency_scalar.rs).

## 1. Physical model and fields

The domain is a three-dimensional fluid region in the model x, y and z coordinates. It models small-amplitude, linear acoustics in a fluid. The real physical field is the real part of the complex amplitude times `exp(+i omega t)`, where `omega = 2 pi frequency` and frequency is positive. The acoustic model does not store a phasor-convention field; this is the solver's convention.

Pressure `p` (Pa) is the **primary** unknown. Particle velocity `v = (vx, vy, vz)` (m/s) is **derived**. The first-order equations, with no volume source, and their eliminated pressure equation are

```math
i\omega\rho\,\mathbf v+\nabla p=0,\qquad
i\omega\frac{p}{\rho c^2}+\nabla\cdot\mathbf v=0,
\qquad
-\nabla\cdot(\rho^{-1}\nabla p)-\frac{\omega^2}{\rho c^2}p=0.
```

Thus `v = i grad(p)/(omega rho)`. Across a fluid interface the weak formulation represents continuous pressure and normal flux, subject to discretization. Lossless resonances can make a frequency-domain system singular or ill-conditioned. A nonzero boundary or incident source is needed for a nonzero driven response.

## 2. From weak equation to tetrahedron basis

Integrating the pressure equation by parts produces a volume term `integral rho^-1 grad(q)·grad(p)` and a pressure mass term. This standard pressure formulation uses `p` and test function `q` in `H1`: pressure is continuous at shared nodes, while its gradient can jump between cells. This is a formulation choice: the PDE alone does not uniquely select a Whitney degree, and mixed or discontinuous methods are possible.

On a tetrahedron with barycentric coordinates `lambda_i`,

```math
p_h=\sum_{i=1}^4 p_i\lambda_i,\qquad
\nabla p_h=\sum_{i=1}^4 p_i\nabla\lambda_i.
```

The `lambda_i` are linear Lagrange functions, also Whitney 0-forms: one complex pressure coefficient per node. The gradient is constant inside each straight tetrahedron and belongs to the corresponding Whitney 1-form edge space. Velocity is recovered from that gradient using the material sampled at the cell centroid, then incident-cell values are arithmetically averaged at nodes for field output; it is not an independently solved edge field. This averaging can smooth material-interface jumps.

Whitney forms form compatible nodal, edge, face and, in 3D, volume spaces under exterior differentiation. The Hodge star is a separate metric/material-dependent map; its finite-element inner products give mass-like matrices. It does not alone prescribe the primary unknown. For background see [Arnold, Falk and Winther, finite element exterior calculus](https://www-users.cse.umn.edu/~arnold/papers/acta.pdf).

## 3. Materials and Physics

Assign a material to each active domain selection. Its scenario properties are `density` (`rho`, default `1.225 kg/m^3`), `soundSpeed` (`c`, default `343 m/s`) and optional dimensionless `lossFactor` (default `0`). The solver requires finite nonzero density and sound speed; physical fluid values should be positive. **Nonzero loss factor is not implemented**, so it cannot model material absorption. Values may depend on parameters and supported coordinates/frequency; wave assembly samples them at quadrature points. Consult the [expression scopes](../../docs/fullmeta-model-2.md) for permitted model names.

Physics selects `pressure`, active domains and frequency. A boundary selection targets exterior triangular mesh faces. The unit-aware entries have these meanings, with `n` the outward normal:

| Physics condition | Meaning and default |
| --- | --- |
| Dirichlet | One complex scalar expression gives the prescribed pressure `p = p_bar` in Pa; default `0 Pa` (pressure-release when zero). |
| Neumann, field form | Three complex expressions `fieldValues = [vx, vy, vz]` in m/s specify the derived particle-velocity vector. Only its outward normal component is imposed; zero normal velocity is a rigid wall. The accompanying scalar `value` must be zero. |
| Neumann, legacy scalar form | When `fieldValues` is absent, the scalar `value` directly prescribes outward normal velocity `v·n = v_bar` in m/s, including nonzero values. Default `0 m/s`. |

An enabled vector Neumann condition must contain exactly three components; model validation reports the count before a Study is run. A disabled condition is skipped by the component-count rule. The solver retains its own input check.
| Impedance | `Z` in `Pa*s/m`, default `411.6`; without incident source, `p = Z (v·n)`. An optional dimensionless complex incident factor and one pressure polarization form `p_inc`; the implemented driven relation is `v·n = (p - 2 p_inc)/Z`. A finite nonzero impedance is required. |

The impedance is a local boundary model, not an automatically inferred outgoing mode; the source expression must supply its own spatial phase. Unspecified, nonperiodic exterior faces contribute no boundary term, equivalent to zero normal velocity. A `periodicCondition` references an enabled matched mesh pair, with translation `delta` in metres and phase vector `k` in rad/m. It imposes `p(r+delta) = exp(-i k·delta) p(r)`; zero phase means equal pressure. Do not assign an ordinary condition to a periodic endpoint. A mesh pair alone does not activate the Physics condition.

### Exact Dirichlet and Neumann equations

Let `d` be the prescribed scalar pressure, `w = (wx,wy,wz)` the prescribed velocity in model coordinates, and `n` the outward unit normal. For the solver's `exp(+i omega t)` convention, the two conditions are

```math
\text{Dirichlet: } p=d,\qquad
\text{Neumann: } \mathbf n\cdot\mathbf v=\mathbf n\cdot\mathbf w,
\qquad
\mathbf n\cdot(\rho^{-1}\nabla p)=-i\omega\,\mathbf n\cdot\mathbf w.
```

The weak-form Neumann contribution on the right is `-i omega integral q (n·w) dS`, for test function `q`. The solver evaluates each velocity expression at face quadrature points, projects it onto the outward normal and integrates that scalar. Tangential velocity components do not affect the condition. The outward normal is determined from the tetrahedron adjacent to the exterior face, independently of the order of its vertices. Pressure expressions are evaluated at boundary nodes, with inconsistent shared or periodically identified nodal values rejected.

The three vector expressions each have the derived field's unit `m/s`; they are not pressure gradients. Their accompanying `value` remains the legacy scalar normal-velocity quantity and must evaluate to zero wherever `fieldValues` is used. Without `fieldValues`, the existing scalar specifies normal velocity directly. An unassigned nonperiodic exterior face has no load and therefore satisfies homogeneous Neumann (`n·v = 0`): a rigid wall, not an absorbing boundary.

## 4. Mesh, Study and results

- **Geometry / selections / mesh:** build 3D domains, assign materials to domain selections, and select boundary faces for conditions and integrations. The geometry mesh uses linear tetrahedra. Mesh size controls resolve geometry and wavelength; perform a refinement study rather than treating a fixed elements-per-wavelength count as a guarantee.
- **Study:** frequency-domain, pressure formulation, order 1. A single frequency or declared parameter/frequency sweep produces separate cases. Backends and quadrature are Study solver settings; centroid integration can change accuracy and may underintegrate mass terms. Mesh order and solution order are separate concepts, even though both are 1 here.
- **Definitions and results:** parameters and pointwise/global expressions may use declared scope, including pressure, velocity and material symbols. Boundary/domain/edge integration and point evaluation operate on selected entities. Outputs include complex pressure and recovered velocity, field plots by real/imaginary/magnitude, configured global-variable data and `max_pressure_magnitude_Pa` sweep output. Inspect output selections, units and any averaging before interpreting an interface value. See [Model 2 results and Study](../../docs/fullmeta-model-2.md).

The scenario supports harmonic and transient Studies. Nonzero acoustic material loss and nonlinear acoustics are not implemented. Use mesh and frequency/time-step convergence checks and an analytic plane-wave or cavity benchmark appropriate to the selected boundaries.

For a simple check in a homogeneous lossless fluid, a +x travelling plane wave has `p = P exp(-i omega x/c)` and `vx = p/(rho c)`. Set boundary data consistent with that wave and the outward normals; compare complex pressure and velocity rather than only their magnitudes. A 3D boundary integral has surface-area measure, unlike the corresponding 2D edge integral.

## Lossless second-order frequency eigenproblem (reference)

This familiar frequency-domain reduction is retained as an analytical reference, not as the implemented Study contract. The universal eigenfrequency Study below uses the first-order transient state system and solves directly for the complex temporal rate `s`. For a source-free, lossless cavity with positive, frequency-independent `rho` and `c`, write the pressure equation from section 1 as

```math
-\nabla\cdot(\rho^{-1}\nabla p)=\lambda\,(\rho c^2)^{-1}p,
\qquad \lambda=\omega^2.
```

The operator on the left is the pressure stiffness operator; the operator multiplying `lambda` on the right weights pressure by the fluid compressibility `1/(rho c^2)`. With homogeneous pressure-release boundaries (`p = 0`) and/or rigid walls (`n·rho^-1 grad(p) = 0`), multiply by a test pressure `q`, integrate over the volume and integrate the left side by parts. The homogeneous boundary term vanishes, giving

```math
\int_\Omega\rho^{-1}\nabla\overline q\cdot\nabla p\,dV
=\lambda\int_\Omega(\rho c^2)^{-1}\overline q\,p\,dV.
```

`q` satisfies the same homogeneous essential boundary conditions as `p`; the bar denotes complex conjugation. Expanding `p` and `q` in the real nodal basis gives the generalized matrix eigenproblem

```math
K\mathbf p=\omega^2M\mathbf p,\qquad
K_{ij}=\int_\Omega\rho^{-1}\nabla N_i\cdot\nabla N_j\,dV,\qquad
M_{ij}=\int_\Omega(\rho c^2)^{-1}N_iN_j\,dV.
```

Here `N_i` are the tetrahedron basis functions, `M` discretizes the compressibility-weighted operator, `p` is a nonzero mode vector and `f = sqrt(lambda)/(2 pi)` for a positive eigenvalue. Homogeneous essential constraints must be applied to both matrices. A fully rigid connected cavity also has a constant-pressure zero mode; velocity recovery divides by `omega` and does not apply to it. Impedance boundaries or frequency-dependent coefficients require a more general eigenproblem.

## Transient study

The [scenario JSON](../src/scenarios/acoustic3d.json) advertises the supported transient Study and declares the generic pressure/velocity system, units, source and output mapping. The shared sparse runner binds material and Domain Source expressions, initial fields, prescribed pressure, normal-velocity and instantaneous impedance boundaries. Synthetic native coordinator cases exercise nonzero sources and space/time-dependent density. The live editor and deployed-host workflows still need visible verification. Nonzero harmonic loss requires a separately declared causal time-domain law.

The native `scenario_operators_obey_physical_energy_and_heat_balances` test checks the assembled pressure/velocity operator's backward-Euler acoustic energy identity on a 3D simplex mesh. Spatial refinement checks structured and perturbed-interior simplex meshes.

With prescribed density $\rho(\mathbf x,t)$, compressibility $b=1/(\rho c^2)$, pressure $p$, velocity $\mathbf v$, and Domain Source $Q$ in Hz, the supported equations are

```math
\partial_t(bp)+\nabla\!\cdot\mathbf v=Q,\qquad
\partial_t(\rho\mathbf v)+\nabla p=0.
```

Both derivatives act on the coefficient and field together. For pressure test $q$, vector test $\mathbf w$, and prescribed outward normal velocity $g_n$, the declared weak equations are

```math
\int_\Omega q\,\partial_t(bp)-\nabla q\!\cdot\mathbf v\,d\Omega
=\int_\Omega qQ\,d\Omega-\int_{\Gamma_v}qg_n\,d\Gamma,
\quad
\int_\Omega\mathbf w\!\cdot[\partial_t(\rho\mathbf v)+\nabla p]\,d\Omega=0.
```

The discrete sparse block system is

```math
\frac{d}{dt}\!\left[\begin{pmatrix}M_b&0\\0&M_\rho\end{pmatrix}
\begin{pmatrix}\mathbf p\\\mathbf v\end{pmatrix}\right]
+\begin{pmatrix}0&D\\G&0\end{pmatrix}\begin{pmatrix}\mathbf p\\\mathbf v\end{pmatrix}
=\begin{pmatrix}\mathbf f_Q+\mathbf f_\Gamma\\0\end{pmatrix}.
```

Here $D$ is assembled from the negative test gradient and velocity basis, and $G$ from the velocity test and pressure gradient. The boundary load has the negative sign shown above; an unassigned natural boundary has zero normal velocity. The instantaneous impedance law uses $v_n=(p-2p_{\rm inc})/Z$, with real nonzero $Z$, contributing $p/Z$ to the pressure operator and $2p_{\rm inc}/Z$ to the load. The common backward-Euler implementation and supported scope are described in the [shared transient reference](transient_study_contract.md). This 3D formulation uses three velocity components. Time-domain inputs are real, not harmonic phasors.

## Domain Source

The current implementation supports Domain Source and Additional Domain Source as separate PHYSICS feature types. Each entry has its own domain selection and expression components; multiple entries of either type add, including on overlapping selections, while an absent type contributes zero. Expressions are sampled at volume quadrature points using physical coordinates and model parameters. Where harmonic studies are supported, their evaluations additionally provide `freq`, `frequency`, and `omega`; transient studies additionally provide time `t` in seconds. The scenario JSON declares each type’s physical meaning, units, component order and weak-form route. This implementation is undergoing verification; its availability in a deployed installation depends on compatible FEM and platform runtimes.

For this pressure formulation, Q is a volume injection rate per volume, in Hz, in `i omega p/(rho c^2) + div(v) = Q`. The pressure weak equation receives `+i omega integral(test * Q) dV`.

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.


## Additional Domain Source

`additionalDomainSource` is a distributed force density **f** in the first-order acoustic momentum equation. In 3D it has components `(f_x, f_y, f_z)` in N/m³. It is distinct from the existing scalar pressure-volume injection **Q** in Hz: **f** accelerates acoustic velocity, while **Q** injects into the pressure/continuity equation. Both use selected domains and add across overlapping source features.

~~~text
rho * partial_t(v) + grad(p) = f
(1 / (rho * c^2)) * partial_t(p) + div(v) = Q

For the exp(+i*omega*t) convention, the force contribution to the
harmonic pressure weak-form right-hand side is
integral(grad(test) dot f / rho) dV.
The separate Q contribution is i*omega * integral(test * Q) dV.
~~~

The JSON declares the force components, units, gradient-tested harmonic route, and transient route to the velocity equation. The feature name is shared across scenarios; the [acoustic 3D contract](../src/scenarios/acoustic3d.json) supplies its meaning to the generic assembler.

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
