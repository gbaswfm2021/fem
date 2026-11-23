# 3D electromagnetics: model and implementation

This manual relates the `em-3d` model tree to the Maxwell equations and tetrahedral solver. It is intended for users, scientists and developers; the development status in the [project guide](../../docs/README.md) still applies. The [Model 2 reference](../../docs/fullmeta-model-2.md), [scenario contract](../src/scenarios/em3d.json) and [generic edge solver](../src/prepared_edge_execution.rs) are the detailed references.

## 1. Physics and fields

With the declared `exp(+i omega t)` phasor convention, positive frequency `f`, `omega = 2 pi f`, and real physical field `Re(E exp(i omega t))`, the source-free material equations are

```math
\nabla\times\mathbf E=-i\omega\mu\mathbf H,\qquad
\nabla\times\mathbf H=(\sigma+i\omega\epsilon)\mathbf E.
```

In the built-in Physics formulation `electric`, `E = (Ex,Ey,Ez)` in V/m is **primary** and `H = (Hx,Hy,Hz)` in A/m is **derived**. Eliminating H gives

```math
\nabla\times(\mu^{-1}\nabla\times\mathbf E)
+(i\omega\sigma-\omega^2\epsilon)\mathbf E=0,
\qquad \mathbf H=\frac{i}{\omega\mu}\nabla\times\mathbf E.
```

Here `epsilon = epsilon0 eps_r`, `mu = mu0 mu_r`; conductivity supplies Ohmic loss. A boundary excitation supplies the incident wave. A domain's free-space wavelength alone is not enough to establish mesh resolution, especially in high-index, conducting or resonant regions.

## 2. From weak form to edge functions

The electric weak form contains `integral mu^-1 curl(test)·curl(E)` and a material-weighted vector mass term. Its natural conforming space is `H(curl)`: tangential field traces are continuous across shared faces, whereas all three Cartesian components need not be continuous. Thus E is represented by Nédélec edge elements, not three independent nodal P1 fields. Other formulations and weak forms can use other spaces; the PDE alone does not uniquely determine the discretization.

On a tetrahedron, with barycentric coordinates `lambda_i`, the lowest-order Whitney 1-form basis for oriented edge `i -> j` is

```math
\mathbf w_{ij}=\lambda_i\nabla\lambda_j-\lambda_j\nabla\lambda_i,
\qquad \mathbf E_h=\sum_{i<j} e_{ij}\mathbf w_{ij},
\qquad e_{ij}=\int_{i}^{j}\mathbf E_h\cdot d\mathbf l.
```

There are six complex local edge coefficients. Global orientation and periodic phase must be applied consistently. The curl lies in the compatible Whitney 2-form face space. The Hodge star is a separate geometry/metric map, with material-weighted inner products producing mass-like matrices; it is not the basis itself. See [finite element exterior calculus](https://www-users.cse.umn.edu/~arnold/papers/acta.pdf).

The electric solver supports Nédélec orders 1 and 2 (Study default 2). Order 2 uses higher-order edge and face moments, not only six edge coefficients. The Gmsh geometry mesh still uses linear tetrahedra: solution order and geometry order are different. The solver evaluates E and its curl from the element expansion, recovers H with the element material, and averages element values at nodes for displayed field components. Node plots therefore are post-processing, not the native edge unknowns; interface discontinuities can be smoothed.

## 3. Materials and sampling

Assign `relativePermittivity` (`eps_r`, default `1`), `relativePermeability` (`mu_r`, default `1`) and `conductivity` (`sigma`, default `0 S/m`) to every active domain. These are scalar isotropic coefficients. The solver evaluates permitted expressions for each frequency and, for spatially varying values, at each tetrahedron centroid; the resulting material is held constant over that tetrahedron's assembly and recovery. A finer mesh may be needed for a spatial gradient even if element quadrature is accurate. Values must be finite and the electric formulation requires nonzero `mu_r`. The profile does not declare general anisotropic tensors or independent complex permittivity.

## 4. Physics boundaries and periodicity

The shared editor offers Dirichlet, Neumann and impedance conditions. For EM3D's `electric` formulation, the primary field is E and the derived field is H. Boundary expressions use model Cartesian axes, the outward normal and the `exp(+i omega t)` convention.

| Physics tree selection | Equation enforced by the current EM3D solver |
| --- | --- |
| Enabled Dirichlet | `E_t = P_t d`, prescribing the primary field's tangential trace. |
| Enabled Neumann | `n × H = n × h`, prescribing the derived magnetic field's tangential trace. |
| No ordinary condition on a nonperiodic exterior face | `n × H = 0`, the homogeneous natural condition from the assembled weak equation. |
| Impedance | `n × H + E_t/Z = 2 E_inc,t/Z`, as described below. |

The zero-value mappings are:

| Boundary | Physics condition | Equation | Current support |
| --- | --- | --- | --- |
| Perfect electric conductor (PEC) | Zero Dirichlet | `n × E = 0` | Constrains tangential edge moments and, at order 2, face moments. |
| Perfect magnetic conductor (PMC) | Zero Neumann | `n × H = 0` | Adds no Neumann load; the same natural condition applies on unassigned, nonperiodic exterior faces. |

Let `d = (dx,dy,dz)` be the prescribed electric field in `V/m`, `h = (hx,hy,hz)` the prescribed magnetic field in `A/m`, and `P_t = I - n n^T` the tangential projector. The equations are

```math
\text{Dirichlet:}\quad \mathbf E_t=P_t\mathbf d,
\qquad
\text{Neumann:}\quad \mathbf n\times\mathbf H=\mathbf n\times\mathbf h,
\qquad \mathbf H=\frac{i}{\omega\mu}\nabla\times\mathbf E.
```

Zero `d` describes PEC and zero `h` describes PMC. Nonzero `h` contributes `+i omega integral(v_t · (n × h))` to the right-hand side of the electric weak equation. Each component may vary with the supported expression symbols; the normal component has no effect after taking the tangential trace.

In the Rust solver request, each entry in `modelConfiguration.prescribedBoundaries` has `id`, `selectionId`, `type` and `value`. `value` may be scalar zero, or an ordered array of three expression-and-unit quantities in x, y, z order. A request may instead retain scalar zero in `value` and supply that array as `fieldValues`. A nonzero scalar has no specified tangential direction and is rejected. Expressions are evaluated at boundary quadrature points, so the discrete condition approximates their tangential trace; the solver checks inconsistent constraints at shared or periodically identified moments.

In the model tree, the shared scalar `value` is retained for compatibility and must be zero. EM3D additionally accepts an optional `fieldValues` containing the three Cartesian quantities; new conditions start with all three components zero. An older scalar-zero condition remains valid without migration. The vector components, not the incident factor and polarization of an impedance boundary, prescribe the Dirichlet or Neumann trace.

Each Dirichlet component must evaluate to electric-field dimension `V/m`. A numerical constant such as `1` uses the component's declared unit. For example, if parameter `s` has unit `V/m^2` and `y` is a coordinate in metres, `s*y` is valid in a `V/m` component, whereas `s*y*y` has dimension `V` and is rejected. Neumann vector components must evaluate to magnetic-field dimension `A/m`. The legacy zero-only scalar Neumann value keeps its existing `V/m^2` unit for compatibility. Coordinate-dependent quantities are checked at the boundary integration points as well as by applicable model validation.

### Dirichlet and Neumann traces

For this electric curl–curl equation, integrating by parts gives the following boundary term on the left-hand side, where `v` is the test function and `n` is the outward normal:

```math
\int_{\partial\Omega}\mathbf v_t\cdot
\left[\mathbf n\times(\mu^{-1}\nabla\times\mathbf E)\right]dS,
\qquad
\mathbf n\times(\mu^{-1}\nabla\times\mathbf E)
=-i\omega\,\mathbf n\times\mathbf H.
```

**Implemented natural condition:** on an unselected, nonperiodic exterior face, the electric solver adds no boundary term. Its weak formulation therefore imposes `n × (mu^-1 curl E) = 0`, equivalently `n × H = 0` under the stated phasor convention. This is a homogeneous natural magnetic condition, often called a perfect magnetic conductor (PMC) condition. It is not an absorbing condition and is not imposed by selecting the Neumann item.

**Neumann input:** the vector `h` represents H itself, not its curl-derived scaled trace. The solver takes `n × h` and multiplies its weak boundary contribution by `+i omega`; this fixes the `A/m` units and the sign for the declared phasor convention.

**Dirichlet input:** for this `H(curl)` electric formulation the essential trace is `E_t = n × (E × n)`; `E_t = 0` is PEC. The solver constrains the appropriate oriented edge and, at order 2, face moments after periodic reduction.

An impedance condition selects an exterior triangular boundary and supplies finite nonzero `Z` in ohms (creation default `376.730313668 ohm`). With outward normal `n` and tangential projection subscript `t`, its implemented convention is

```math
\mathbf n\times\mathbf H+\frac{\mathbf E_t}{Z}
=\frac{2\mathbf E_{\mathrm{inc},t}}{Z}.
```

An omitted incident source means zero incident field. When enabled, a dimensionless complex incident factor and three-component electric polarization form the incident vector; its expression must supply spatial phase. The solver does not infer a propagation angle, port pair or automatic reflection/transmission normalization.

An enabled `periodicCondition` references a matching enabled mesh pair. Translation `delta` is in metres, phase vector `k` in rad/m, and the trace relation is `E(r+delta) = exp(-i k·delta) E(r)`. Edge and higher-order face orientations and phase are handled during identification. Ordinary boundary conditions cannot overlap active periodic endpoints.

## 5. Study, experimental mode and results

- **Model tree:** 3D geometry and domain selections define material regions; boundary and integration selections identify geometric entities. Physics supplies frequency, domains and the `electric` formulation. Wave parameters, pointwise/global variables and integration operators use the declared expression scopes and model-axis field symbols.
- **Study:** frequency-domain point or parameter sweep, electric order 1 or 2; electric assembly requires standard constitutive quadrature. A nonzero boundary excitation is normally needed. Result quantities include complex E and recovered H components, material and user-defined fields, and configured global-variable/integration data. EM3D has no built-in scalar sweep-result quantity; configured global variables provide sweep columns. See [Study and results](../../docs/fullmeta-model-2.md).

Verify against an appropriate analytic wave or cavity case, energy/phase conventions, and mesh/order convergence. Output plots and field samples do not alone establish numerical accuracy.

For example, in a homogeneous lossless medium a +x plane wave with `E = E0 exp(-i k x) z_hat` has `H = -(E0/eta) exp(-i k x) y_hat`, where `k = omega sqrt(mu epsilon)` and `eta = sqrt(mu/epsilon)`. With compatible boundary and periodic data, compare the complex field signs and phase with this reference as the mesh and approximation order are refined.

## Lossless second-order frequency eigenproblem (reference)

This familiar frequency-domain reduction is retained as an analytical reference, not as the implemented Study contract. The universal eigenfrequency Study below uses the first-order transient state system and solves directly for the complex temporal rate `s`. For a source-free cavity with zero conductivity and positive frequency-independent `epsilon` and `mu`, the electric wave equation in section 1 becomes

```math
\nabla\times(\mu^{-1}\nabla\times\mathbf E)
=\lambda\epsilon\mathbf E,\qquad \lambda=\omega^2.
```

The left operator is the magnetic-permeability-weighted curl–curl operator; the right operator multiplies the electric field by permittivity. With homogeneous PEC (`n × E = 0`) and/or homogeneous natural PMC (`n × mu^-1 curl(E) = 0`) boundaries, testing by `v` and integrating by parts gives

```math
\int_\Omega\mu^{-1}(\nabla\times\overline{\mathbf v})\cdot(\nabla\times\mathbf E)\,dV
=\lambda\int_\Omega\epsilon\,\overline{\mathbf v}\cdot\mathbf E\,dV.
```

The tangential trace of `v` vanishes on PEC boundaries, and the bar denotes complex conjugation. Expanding the fields in the real Nédélec basis gives

```math
K\mathbf e=\omega^2M\mathbf e,\qquad
K_{ij}=\int_\Omega\mu^{-1}(\nabla\times\mathbf W_i)\cdot(\nabla\times\mathbf W_j)\,dV,\qquad
M_{ij}=\int_\Omega\epsilon\,\mathbf W_i\cdot\mathbf W_j\,dV.
```

Here `W_i` are the Nédélec basis functions and `M` discretizes the permittivity-weighted field operator. Homogeneous PEC constraints apply to both matrices. A nonzero eigenvector is a mode shape, and for positive eigenvalues `f = sqrt(lambda)/(2 pi)`. The curl–curl operator has a gradient-field nullspace, so a future solver must distinguish zero and spurious modes from physical modes; magnetic-field recovery divides by `omega` and does not apply at zero. Conductivity, impedance or frequency-dependent coefficients require a more general eigenproblem.

## Transient study

The [scenario JSON](../src/scenarios/em3d.json) advertises transient Studies using oriented tetrahedral edge spaces of orders 1 and 2. The shared runner binds field blocks, materials, sources, initial fields and time snapshots; tangential electric moments, magnetic flux and impedance facet terms use the declared laws. Synthetic native coordinator cases exercise nonzero sources, time-dependent material and prescribed tangential fields. Fixed zero-phase periodic routing is supported within its matching-facet constraints. Nonzero legacy scalar Neumann data remains unsupported; live editor and deployed-host workflows still need visible verification.

The native `scenario_operators_obey_physical_energy_and_heat_balances` test checks the assembled Maxwell energy identity for the 3D edge formulation at both supported Nédélec orders. Spatial refinement checks both mesh families; the perturbed-interior sequence uses 2, 4 and 6 divisions for runtime.

The transient result's nodal electric and magnetic vectors are visualization averages of the incident element traces. Periodic constraints apply to the oriented tangential degrees of freedom; these averaged nodal vectors need not match pointwise across a periodic boundary. They should not be used as a direct check of periodic trace continuity.

For scalar instantaneous permittivity $\epsilon$, permeability $\mu$, conductivity $\sigma$, electric current density $\mathbf J_{e,s}$ and equivalent magnetic current density $\mathbf J_{m,s}$, the supported 3D equation is

```math
\partial_t(\epsilon\mathbf E)+\sigma\mathbf E-\nabla\!\times\mathbf H=-\mathbf J_{e,s},
\qquad
\partial_t(\mu\mathbf H)+\nabla\!\times\mathbf E=-\mathbf J_{m,s}.
```

The corresponding sparse block equation is

```math
\frac{d}{dt}\!\left[\begin{pmatrix}M_\epsilon&0\\0&M_\mu\end{pmatrix}
\begin{pmatrix}\mathbf E\\\mathbf H\end{pmatrix}\right]
+\begin{pmatrix}M_\sigma&-C_H\\C_E&0\end{pmatrix}
\begin{pmatrix}\mathbf E\\\mathbf H\end{pmatrix}
=\begin{pmatrix}-\mathbf j_{e,s}\\-\mathbf j_{m,s}\end{pmatrix}.
```

Both constitutive derivatives are product derivatives when coefficients depend on time. The state fields use oriented edge degrees of freedom; nodal output values are visualization averages of element traces, not nodal degrees of freedom. Initial fields must satisfy applicable constraints. Time-domain inputs are real, not harmonic phasors; dispersive or memory-dependent materials require causal constitutive laws and potentially auxiliary variables.

The [shared transient reference](transient_study_contract.md) documents the component equations, weak forms, units, boundary and result contracts, sparse time step, and current limitations.

## Domain Source and Additional Domain Source

These are two distinct PHYSICS feature types. Domain Source is electric current density $\mathbf J_e$ with components (J_ex, J_ey, J_ez) in A/m²; it enters Ampere's equation. Additional Domain Source is equivalent magnetic current density $\mathbf J_m$ with components (J_mx, J_my, J_mz) in V/m²; it enters Faraday's equation. The equivalent magnetic source is often written $\mathbf M$ in dual-source Maxwell notation; here it is an impressed source, not material magnetization. Each feature has its own selected domains and expressions, and both may be present in the same model. Multiple entries of either type add on overlapping domains; absence of one type sets only that source to zero. Expressions use physical coordinates and model parameters; harmonic evaluations provide freq, frequency and omega, while transient evaluations provide $t$ in seconds.

For the electric-field formulation, the harmonic electric-current load is $-i\omega\int\overline{\mathbf F}\cdot\mathbf J_e\,dV$, assembled against the declared edge-element test basis.

With the package convention $\exp(+i\omega t)$, the first-order equations with both source types are

~~~text
epsilon * partial_t(E) + sigma * E - curl(H) = -J_e
mu * partial_t(H) + curl(E) = -J_m
~~~

Eliminating $\mathbf H$ gives the E-only equation

~~~text
curl(mu^-1 * curl(E)) + (-omega^2 * epsilon + i*omega*sigma) * E
    = -i*omega*J_e - curl(mu^-1 * J_m)
~~~

For an electric test function $\mathbf F$, the corresponding volume load is

~~~text
-i*omega * integral(conj(F) dot J_e) dV
- integral(curl(conj(F)) dot (mu^-1 * J_m)) dV
~~~

The magnetic contribution is assembled with the curl of the same Nédélec test basis; no spatial derivative of the user expression is formed. The recovered harmonic field is

~~~text
H = i/(omega*mu) * (curl(E) + J_m)
~~~

Thus result fields and integration expressions include the magnetic-source contribution. In transient studies the second equation is $\partial_t(\mu\mathbf H)+\nabla\times\mathbf E=-\mathbf J_{m,s}$, and $\mathbf J_m$ is routed to the H test equation. An absent Additional Domain Source contributes zero and takes no expression-sampling path.

See the [EM3D JSON contract](../src/scenarios/em3d.json) for component labels, units and source routes, and the [implementation plan](implement_domain_sources.md) for generic assembly details.

## Boundary vector count validation

Enabled Cartesian Dirichlet and Neumann vectors must have exactly three components. Semantic model validation checks `fieldValues` and legacy array values supplied through the fallback `value` path from the scenario’s declared boundary input; scalar values remain supported and the solver retains its own check. Enabled vectors with an invalid type or component count receive a semantic validation error. To repair one in the editor, turn off “Specify field components” and turn it back on to recreate the declared defaults; this reset replaces the existing components. Disabled conditions skip these semantic checks.



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
