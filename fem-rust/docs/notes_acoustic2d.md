# 2D pressure acoustics: model and implementation

This manual describes the current `acoustic-2d` scenario for users, scientists and developers. It connects the model tree to the frequency-domain pressure equation and the linear-triangle solver. It describes implemented behavior, not a guarantee of numerical acceptance. See the [project guide](../../docs/README.md), [Model 2 reference](../../docs/fullmeta-model-2.md), [scenario contract](../src/scenarios/acoustic2d.json), and [solver](../src/prepared_frequency_scalar.rs).

## 1. Physical model and fields

The domain is a cross-section in the model x–y plane; quantities do not vary in z. It models small-amplitude, linear acoustics in a fluid, not a 3D radiation problem with a finite thickness. The real physical field is the real part of the complex amplitude times `exp(+i omega t)`, where `omega = 2 pi frequency` and frequency is positive. The acoustic model does not store a phasor-convention field; this is the solver's convention.

Pressure `p` (Pa) is the **primary** unknown. Particle velocity `v = (vx, vy, vz)` (m/s) is **derived**; `vz = 0` in this 2D model. The first-order equations, with no volume source, and their eliminated pressure equation are

```math
i\omega\rho\,\mathbf v+\nabla p=0,\qquad
i\omega\frac{p}{\rho c^2}+\nabla\cdot\mathbf v=0,
\qquad
-\nabla\cdot(\rho^{-1}\nabla p)-\frac{\omega^2}{\rho c^2}p=0.
```

Thus `v = i grad(p)/(omega rho)`. Across a fluid interface the weak formulation represents continuous pressure and normal flux, subject to discretization. Lossless resonances can make a frequency-domain system singular or ill-conditioned. A nonzero boundary or incident source is needed for a nonzero driven response.

## 2. From weak equation to triangle basis

Integrating the pressure equation by parts produces a volume term `integral rho^-1 grad(q)·grad(p)` and a pressure mass term. This standard pressure formulation uses `p` and test function `q` in `H1`: pressure is continuous at shared nodes, while its gradient can jump between cells. This is a formulation choice: the PDE alone does not uniquely select a Whitney degree, and mixed or discontinuous methods are possible.

On a triangle with barycentric coordinates `lambda_i`,

```math
p_h=\sum_{i=1}^3 p_i\lambda_i,\qquad
\nabla p_h=\sum_{i=1}^3 p_i\nabla\lambda_i.
```

The `lambda_i` are linear Lagrange functions, also Whitney 0-forms: one complex pressure coefficient per node. The gradient is constant inside each straight triangle and belongs to the corresponding Whitney 1-form edge space. Velocity is recovered from that gradient using the material sampled at the cell centroid, then incident-cell values are arithmetically averaged at nodes for field output; it is not an independently solved edge field. This averaging can smooth material-interface jumps.

Whitney forms form compatible nodal, edge, face and, in 3D, volume spaces under exterior differentiation. The Hodge star is a separate metric/material-dependent map; its finite-element inner products give mass-like matrices. It does not alone prescribe the primary unknown. For background see [Arnold, Falk and Winther, finite element exterior calculus](https://www-users.cse.umn.edu/~arnold/papers/acta.pdf).

## 3. Materials and Physics

Assign a material to each active domain selection. Its scenario properties are `density` (`rho`, default `1.225 kg/m^3`), `soundSpeed` (`c`, default `343 m/s`) and optional dimensionless `lossFactor` (default `0`). The solver requires finite nonzero density and sound speed; physical fluid values should be positive. **Nonzero loss factor is not implemented**, so it cannot model material absorption. Values may depend on parameters and supported coordinates/frequency; wave assembly samples them at quadrature points. Consult the [expression scopes](../../docs/fullmeta-model-2.md) for permitted model names.

Physics selects `pressure`, active domains and frequency. A boundary selection targets exterior mesh edges. The unit-aware entries have these meanings, with `n` the outward normal:

| Physics condition | Meaning and default |
| --- | --- |
| Dirichlet | One complex scalar expression gives the prescribed pressure `p = p_bar` in Pa; default `0 Pa` (pressure-release when zero). |
| Neumann, field form | Two complex expressions `fieldValues = [vx, vy]` in m/s specify the derived in-plane particle velocity. Only its outward normal component is imposed; zero normal velocity is a rigid wall. The accompanying scalar `value` must be zero. |
| Neumann, legacy scalar form | When `fieldValues` is absent, scalar `value` directly prescribes outward normal velocity `v·n = v_bar` in m/s, including nonzero values. Default `0 m/s`. |
| Impedance | `Z` in `Pa*s/m`, default `411.6`; without incident source, `p = Z (v·n)`. An optional dimensionless complex incident factor and one pressure polarization form `p_inc`; the implemented driven relation is `v·n = (p - 2 p_inc)/Z`. A finite nonzero impedance is required. |

The impedance is a local boundary model, not an automatically inferred outgoing mode; the source expression must supply its own spatial phase. Unspecified, nonperiodic exterior edges contribute no boundary term, equivalent to zero normal velocity. A `periodicCondition` references an enabled matched mesh pair, with translation `delta` in metres and phase vector `k` in rad/m. It imposes `p(r+delta) = exp(-i k·delta) p(r)`; zero phase means equal pressure. Do not assign an ordinary condition to a periodic endpoint. A mesh pair alone does not activate the Physics condition.

### Exact Dirichlet and Neumann equations

Let `d` be the prescribed scalar pressure, `w = (wx,wy)` the prescribed in-plane velocity in model coordinates, and `n` the outward unit normal of the boundary edge. For the solver's `exp(+i omega t)` convention, the conditions are

```math
\text{Dirichlet: }p=d,\qquad
\text{Neumann: }\mathbf n\cdot\mathbf v=\mathbf n\cdot\mathbf w,\qquad
\mathbf n\cdot(\rho^{-1}\nabla p)=-i\omega\,\mathbf n\cdot\mathbf w.
```

The weak-form Neumann contribution on the right is `-i omega integral q (n·w) ds`, for test function `q`. The solver evaluates velocity expressions at edge quadrature points, projects onto the outward normal and integrates the scalar. Tangential velocity does not affect the condition. The normal is determined from the adjacent triangle, independently of the order of the edge vertices. Pressure is evaluated at boundary nodes, and inconsistent values at shared or periodically identified nodes are rejected.

Both velocity expressions have the derived field's unit `m/s`, not pressure-gradient units. The accompanying legacy scalar `value` must evaluate to zero wherever `fieldValues` is supplied. Without the vector, the scalar directly specifies normal velocity. An unassigned nonperiodic exterior edge has no load and therefore satisfies homogeneous Neumann (`n·v = 0`): a rigid wall. This 2D model has `vz = 0`, so the input has two components.

An enabled vector Neumann condition must contain exactly two components; model validation reports the count before a Study is run. A disabled condition is skipped by the component-count rule. The solver retains its own input check.

## 4. Mesh, Study and results

- **Geometry / selections / mesh:** build 2D domains, assign materials to domain selections, and select boundary edges for conditions and integrations. The geometry mesh uses linear triangles. Mesh size controls resolve geometry and wavelength; perform a refinement study rather than treating a fixed elements-per-wavelength count as a guarantee.
- **Study:** frequency-domain, pressure formulation, order 1. A single frequency or declared parameter/frequency sweep produces separate cases. Backends and quadrature are Study solver settings; centroid integration can change accuracy and may underintegrate mass terms. Mesh order and solution order are separate concepts, even though both are 1 here.
- **Definitions and results:** parameters and pointwise/global expressions may use declared scope, including pressure, velocity and material symbols. Boundary/domain/edge integration and point evaluation operate on selected entities. Outputs include complex pressure and recovered velocity, field plots by real/imaginary/magnitude, configured global-variable data and `max_pressure_magnitude_Pa` sweep output. Inspect output selections, units and any averaging before interpreting an interface value. See [Model 2 results and Study](../../docs/fullmeta-model-2.md).

The scenario supports harmonic and transient Studies. Nonzero acoustic material loss and nonlinear acoustics are not implemented. Use mesh and frequency/time-step convergence checks and an analytic plane-wave or cavity benchmark appropriate to the selected boundaries.

For a simple check in a homogeneous lossless fluid, a +x travelling plane wave has `p = P exp(-i omega x/c)` and `vx = p/(rho c)`. Set boundary data consistent with that wave and the outward normals; compare complex pressure and velocity rather than only their magnitudes. The 2D result is a cross-sectional field per unit out-of-plane extent, not a total 3D radiated power.

## Lossless second-order frequency eigenproblem (reference)

This familiar frequency-domain reduction is retained as an analytical reference, not as the implemented Study contract. The universal eigenfrequency Study below uses the first-order transient state system and solves directly for the complex temporal rate `s`. For a source-free, lossless cavity with positive, frequency-independent `rho` and `c`, write the pressure equation from section 1 as

```math
-\nabla\cdot(\rho^{-1}\nabla p)=\lambda\,(\rho c^2)^{-1}p,
\qquad \lambda=\omega^2.
```

The operator on the left is the pressure stiffness operator; the operator multiplying `lambda` on the right weights pressure by the fluid compressibility `1/(rho c^2)`. With homogeneous pressure-release boundaries (`p = 0`) and/or rigid walls (`n·rho^-1 grad(p) = 0`), multiply by a test pressure `q`, integrate over the cross-section and integrate the left side by parts. The homogeneous boundary term vanishes, giving

```math
\int_\Omega\rho^{-1}\nabla\overline q\cdot\nabla p\,dA
=\lambda\int_\Omega(\rho c^2)^{-1}\overline q\,p\,dA.
```

`q` satisfies the same homogeneous essential boundary conditions as `p`; the bar denotes complex conjugation. Expanding `p` and `q` in the real nodal basis gives the generalized matrix eigenproblem

```math
K\mathbf p=\omega^2M\mathbf p,\qquad
K_{ij}=\int_\Omega\rho^{-1}\nabla N_i\cdot\nabla N_j\,dA,\qquad
M_{ij}=\int_\Omega(\rho c^2)^{-1}N_iN_j\,dA.
```

Here `N_i` are the triangle basis functions, `M` discretizes the compressibility-weighted operator, `p` is a nonzero mode vector and `f = sqrt(lambda)/(2 pi)` for a positive eigenvalue. Homogeneous essential constraints must be applied to both matrices. A fully rigid connected cavity also has a constant-pressure zero mode; velocity recovery divides by `omega` and does not apply to it. Impedance boundaries or frequency-dependent coefficients require a more general eigenproblem.

## Prescribed longitudinal-wavenumber harmonic study and fixed-frequency propagation modes

**Prescribed-`kz` harmonic solving is implemented** for Acoustic 2D pressure, EM2D full polarization, and Elastic2D plane strain. Thermal 2D, 2D plane-stress elasticity, and the other scenarios do not opt in. The cross-section, material coefficients, sources, and boundary data must be invariant in `z` and share the selected longitudinal dependence. The convention is an x–y amplitude times `exp(-i kz z)`, so `partial_z = -i kz`; `kz` is a complex Study quantity in `rad/m`. Its real and imaginary parts may use expressions and model parameters. The default is zero, and the existing zero-`kz` execution route is retained.

The field files and plots are cross-sectional amplitudes. Result metadata records both parts of `kz` and the factor `exp(-i*kz*z)`; reconstruct a 3D field only by applying that factor. The shared weak-form machinery uses the declared test-side `+i*kz` and trial-side `-i*kz` derivatives and sparse assembly. For pressure, the axial velocity is recovered as `v_z = kz*p/(omega*rho)` under the documented reciprocal-fluid convention. Native analytic checks solve a homogeneous axial plane wave, verify pressure and recovered velocity, activate axial force density in N/m³, and preserve a small sparse residual. A separate source-free transverse sine-wave check confirms the finite-`kz` pressure error decreases when the structured triangular mesh is refined from 4 to 8 divisions per side. Nonzero acoustic domain forces follow the contract's reduced pressure-test gradient route; a feature absent from PHYSICS contributes zero.

### Implemented fixed-frequency propagation-mode Study

The `propagationMode` Study is implemented for this pressure formulation. It fixes a positive real frequency and computes a requested set of generally complex transverse modes and longitudinal eigenvalues. This is distinct from Harmonic response, which prescribes $k_z$, and temporal Eigenfrequency, which solves for the temporal rate $s$. Each saved mode also includes the contract-declared axial velocity recovery below. The post-debug circular-waveguide walkthrough was completed through the FullMeta in-app browser; the project, mode values, signed-branch check, and mesh-refinement comparison are recorded in the implementation plan. It is a user-perspective demonstration, not a numerical acceptance test.

The source-free pressure operator at a trial longitudinal wavenumber is assembled using the existing sparse harmonic route. The longitudinal dependence is $e^{-ik_z z}$, and the current scalar-fluid weak operator has the form

```math
P(k_z)\mathbf p=(P_0+k_zP_1+k_z^2P_2)\mathbf p=0.
```

For scalar reciprocal acoustics, the current pressure operator has no linear term ($P_1=0$), so its continuum dispersion contains $k_z^2$. This does **not** make $k_z^2$ the eigensolver's eigenvalue: the shared sparse first-companion method solves for $k_z$ directly, retains any linear coefficient when another declared operator has one, and does not manufacture a mode by negating a result. The assembled polynomial is checked at additional $k_z$ samples, and each returned mode is checked against the original polynomial residual. Each artifact includes nodal pressure and element-local axial velocity

```math
v_z=\frac{k_z}{\omega\rho}p,
```

with density evaluated at the cell-local samples so a material jump is preserved. The result also carries the fixed frequency, complex `kz` in `rad/m`, and residual. Direction/energy-flux classification is not currently included.

Acceptance coverage compares the rigid-wall rectangular-guide dispersion and first cutoff with the analytic values on successively refined meshes. It also checks a homogeneous pressure-Dirichlet guide, independently targeted positive and negative `kz` branches, the recovered axial velocity, and that the source-free projection drops incident impedance data while retaining the impedance operator. The cylindrical-waveguide UI case is a separate user-perspective demonstration, not an acceptance test.

Mode assembly omits affine drives such as Domain Source and incident amplitudes while retaining supported homogeneous constraints and operator terms. The three scenario-neutral solver settings are requested mode count, complex target/shift in inverse-length units, and residual tolerance/Krylov dimension. Mesh, material parameters and parameter-sweep case are taken from the selected prepared case. Only first-order triangular pressure elements and source-free homogeneous boundary operators accepted by the current preparation/assembler path are supported.

### Implemented finite-$k_z$ pressure equation and continuous first-order reference

For the current scalar reciprocal fluid, eliminating velocity at prescribed $k_z$ gives the pressure wave equation

```math
-\nabla_t\!\cdot(\rho^{-1}\nabla_t p)+k_z^2\rho^{-1}p-\omega^2(\rho c^2)^{-1}p=s.
```

The $k_z^2$ term is part of the implemented driven PDE and the pressure polynomial used by the propagation Study. For context, the continuous scalar acoustic equations can also be rearranged into a pressure/axial-velocity first-order pencil. This is a mathematical reference, not the state discretized by the current FEM propagation solver. For real positive, z-invariant $\rho,c$, real nonzero $\omega$, and homogeneous transverse boundary/interface conditions making the scalar operator self-adjoint, put $a=\rho^{-1}$, $b=(\rho c^2)^{-1}$ and eliminate only transverse velocity. The state and continuous pencil are

```math
\mathbf x=\begin{bmatrix}p\\v_z\end{bmatrix},\qquad
A(\omega)=\begin{bmatrix}\omega b+\omega^{-1}\nabla_t\!\cdot(a\nabla_t)&0\\0&\omega\rho\end{bmatrix},\qquad
B=\begin{bmatrix}0&I\\I&0\end{bmatrix},\qquad A\mathbf x=k_z B\mathbf x.
```

Under those assumptions the continuous reference operators are formally Hermitian and $B$ is indefinite. That conditional property belongs to this reciprocal scalar model. The implemented general sparse companion path does not assume Hermiticity or sign pairing; extending the acoustic material contract to more general constitutive laws requires separately declared and verified operators.

## Transient study

The [scenario JSON](../src/scenarios/acoustic2d.json) advertises the supported transient Study and declares the generic pressure/velocity system, units, source and output mapping. The shared sparse runner binds material and Domain Source expressions, initial fields, prescribed pressure, normal-velocity and instantaneous impedance boundaries. Synthetic native coordinator cases exercise nonzero sources, space/time-dependent density and a time-dependent pressure boundary. The live editor and deployed-host workflows still need visible verification. Nonzero harmonic loss requires a separately declared causal time-domain law.

The native `scenario_operators_obey_physical_energy_and_heat_balances` test checks the assembled pressure/velocity operator's backward-Euler acoustic energy identity on a 2D simplex mesh. Spatial refinement checks structured and perturbed-interior simplex meshes.

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

Here $D$ is assembled from the negative test gradient and velocity basis, and $G$ from the velocity test and pressure gradient. The boundary load has the negative sign shown above; an unassigned natural boundary has zero normal velocity. The instantaneous impedance law uses $v_n=(p-2p_{\rm inc})/Z$, with real nonzero $Z$, contributing $p/Z$ to the pressure operator and $2p_{\rm inc}/Z$ to the load. The common backward-Euler implementation and supported scope are described in the [shared transient reference](transient_study_contract.md). This 2D transient formulation uses two velocity components and declares zero out-of-plane velocity; finite longitudinal variation is implemented for harmonic studies only. Time-domain inputs are real, not harmonic phasors.

## Domain Source

The current implementation supports Domain Source and Additional Domain Source as separate PHYSICS feature types. Each entry has its own domain selection and expression components; multiple entries of either type add, including on overlapping selections, while an absent type contributes zero. Expressions are sampled at volume quadrature points using physical coordinates and model parameters. Where harmonic studies are supported, their evaluations additionally provide `freq`, `frequency`, and `omega`; transient studies additionally provide time `t` in seconds. The scenario JSON declares each type’s physical meaning, units, component order and weak-form route. This implementation is undergoing verification; its availability in a deployed installation depends on compatible FEM and platform runtimes.

For this pressure formulation, Q is a volume injection rate per volume, in Hz, in `i omega p/(rho c^2) + div(v) = Q`. The pressure weak equation receives `+i omega integral(test * Q) dA`, per unit out-of-plane thickness.

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.


## Additional Domain Source

`additionalDomainSource` is a distributed force density **f** in the first-order acoustic momentum equation. In 2D it has in-plane components `(f_x, f_y)` in N/m³. It is distinct from the existing scalar pressure-volume injection **Q** in Hz: **f** accelerates the acoustic velocity, while **Q** injects into the pressure/continuity equation. Both use selected domains and add across overlapping source features.

~~~text
rho * partial_t(v) + grad(p) = f
(1 / (rho * c^2)) * partial_t(p) + div(v) = Q

For the exp(+i*omega*t) convention, the force contribution to the
harmonic pressure weak-form right-hand side is
integral(grad(test) dot f / rho) dA.
The separate Q contribution is i*omega * integral(test * Q) dA.
~~~

The 2D integral is per unit out-of-plane thickness. The JSON declares the force components, units, gradient-tested harmonic route, and transient route to the velocity equation. The feature name is shared across scenarios; the [acoustic 2D contract](../src/scenarios/acoustic2d.json) supplies its meaning to the generic assembler.

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

For thermal scenarios, $s$ describes diffusion decay; a mode need not have an oscillatory frequency. The distinct 2D fixed-frequency propagation Study, which solves for $k_z$, is implemented for Acoustic 2D, EM2D, and the plane-strain-parent Elastic 2D contract.

Current fixed-frequency propagation acceptance passes 12 Rust tests, 10 generic sparse-eigensolver tests, and 65 FEM Node contract/result/UI tests. The propagation tests cover declared JSON preparation and result resolution, all three enabled formulations, acoustic rigid-wall dispersion/cutoff and the pressure-Dirichlet guide, signed direct-`kz` targeting, impedance-operator retention with incident-load removal, parameter-case reassembly, and axial-velocity recovery. The full native Rust suite reports 333 passed, 27 failed, and 3 ignored; those 27 failures reproduce omissions of `N/m^3`, `W/m^3`, or `W*s/(m^3*K)` in the fixed shared Core quantity service and are unrelated to propagation-mode assertions. The FEM frontend and FEM-local runtime bundle build successfully. The wider FEM verification history is recorded in the [universal Eigenfrequency implementation plan](implement_universal_eigenfrequency_study.md).
