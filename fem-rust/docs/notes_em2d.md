# 2D electromagnetics: model and implementation

This manual connects the `em-2d` model tree to its equations, triangle discretization and outputs. It is for users, scientists and developers; a declared option does not by itself establish numerical acceptance. See the [project guide](../../docs/README.md), [Model 2 reference](../../docs/fullmeta-model-2.md), [scenario contract](../src/scenarios/em2d.json), and [solver](../src/prepared_frequency_scalar.rs).

## 1. Maxwell equations and scalar formulations

Fields are independent of z. Real physical fields are `Re(field * exp(+i omega t))`, with `omega = 2 pi frequency` and positive frequency. In source-free isotropic materials, the first-order spatial equations are

```math
\nabla\times\mathbf E=-i\omega\mu\mathbf H,\qquad
\nabla\times\mathbf H=(\sigma+i\omega\epsilon)\mathbf E.
```

Here `epsilon = epsilon0 eps_r`, `mu = mu0 mu_r`, and `sigma` is electrical conductivity. Define the solver's relative complex permittivity `eps_c = eps_r - i sigma/(omega epsilon0)` and `k0 = omega sqrt(mu0 epsilon0)`. `eps_r`, `mu_r` and conductivity are real evaluated model expressions; the imaginary part above is generated from conductivity.

The Physics formulation chooses one out-of-plane primary scalar. The other field is derived using the first-order equations:

| Formulation | Primary | Derived | Declared zero components |
| --- | --- | --- | --- |
| `Ez` | `Ez` (V/m) | `Hx`, `Hy` (A/m) | `Ex`, `Ey`, `Hz` |
| `Hz` | `Hz` (A/m) | `Ex`, `Ey` (V/m) | `Ez`, `Hx`, `Hy` |

Writing `u = Ez` or `Hz`, the assembled scalar equations are

```math
\begin{array}{ll}
E_z:\quad -\nabla\cdot(\mu_r^{-1}\nabla u)-k_0^2\epsilon_c u=0,\\
H_z:\quad -\nabla\cdot(\epsilon_c^{-1}\nabla u)-k_0^2\mu_r u=0.
\end{array}
```

For either scalar `u`, define `R(u) = (partial_y u, -partial_x u, 0)`. Then the recovered in-plane field is `H = i R(Ez)/(omega mu)` for `Ez`, and `E = -i R(Hz)/(omega epsilon0 eps_c)` for `Hz`. Both equations use spatially varying coefficients inside the divergence. A driven boundary or other nonzero boundary value is required to produce a nonzero response.

## 2. Why the primary basis is nodal

After integration by parts, the scalar weak equation contains `grad(test)·a grad(u)` and a mass term, with `a = 1/mu_r` for `Ez` or `1/eps_c` for `Hz`. The scalar primary is therefore approximated in `H1` by linear nodal functions on triangles:

```math
u_h=\sum_{i=1}^{3}u_i\lambda_i.
```

The barycentric `lambda_i` are Whitney 0-forms (P1 Lagrange); each triangle has three complex local nodal coefficients. Although `Ez` and `Hz` are components of 3D vector fields, under the z-invariant scalar reduction they are nodal scalars, not in-plane edge unknowns. The gradient of a Whitney 0-form expansion lies in the triangle's Whitney 1-form edge space. The PDE, chosen variables and weak form together motivate this basis; the PDE does not uniquely force a particular element.

Derived in-plane fields are calculated from cell gradients and centroid material values, then arithmetically averaged over incident triangles at each output node. They are not independent edge unknowns; averaging may smooth discontinuities. Whitney forms, their derivative sequence and material/metric Hodge maps give a useful organizing framework; the Hodge star is not itself the basis. See [finite element exterior calculus](https://www-users.cse.umn.edu/~arnold/papers/acta.pdf).

## 3. Material model

Each active domain needs a material assignment. The profile declares `relativePermittivity` (`eps_r`, default `1`), `relativePermeability` (`mu_r`, default `1`) and `conductivity` (`sigma`, default `0 S/m`). They are scalar and isotropic. Wave material expressions may use permitted parameter, coordinate and frequency scopes; their values are sampled for volume quadrature, while recovery uses centroid material values. The solver checks finite values, and requires nonzero `mu_r` for `Ez` or nonzero `eps_c` for `Hz` because those coefficients are inverted. Conductivity accounts for the represented Ohmic loss; a separate general complex permittivity or anisotropic tensor is not declared by this scenario.

## 4. Physics boundaries

A boundary selection identifies exterior mesh edges. Let `a` denote the equation coefficient in section 2, `n` the outward normal and `u` the chosen primary. Defaults are creation values, not a claim that all boundaries are automatically driven.

| Condition | Solver meaning |
| --- | --- |
| Dirichlet | Set the primary scalar `u = value`: `Ez` in V/m or `Hz` in A/m. Zero and nonzero complex expressions are supported. |
| Neumann, field form | Specify `fieldValues = [Hx, Hy]` in A/m for `Ez`, or `[Ex, Ey]` in V/m for `Hz`. The solver uses only the component tangent to the selected edge. The accompanying legacy scalar `value` must be zero. |
| Neumann, legacy scalar form | When `fieldValues` is absent, `value` is the normalized conormal `n·(a grad u)` on the weak-form right-hand side, in `V/m^2` for `Ez` or `A/m^2` for `Hz`. Zero and nonzero complex values remain supported for existing models. |
| Impedance | Robin condition `n·(a grad u) + i beta u = 2 i beta u_inc`, with `beta = omega mu0/Z` for `Ez` and `beta = omega epsilon0 Z` for `Hz`. `Z` defaults to `376.730313668 ohm` and must be finite and nonzero. With no incident fields, `u_inc = 0`; otherwise the dimensionless complex incident factor times the one-component primary-field polarization defines `u_inc`. |

Impedance is a local boundary relation; the solver does not infer an incidence angle or source phase. Unspecified, nonperiodic exterior edges have homogeneous natural conormal data. A `periodicCondition` activates a mesh pair and imposes `u(r+delta) = exp(-i k·delta)u(r)` with `k` in rad/m and `delta` in metres. An enabled periodic endpoint cannot also have an ordinary boundary condition.

### Exact Dirichlet and Neumann equations

For an exterior edge, let `n = (nx,ny)` be the outward normal and `t = (-ny,nx)` its positively rotated tangent. Let `d` be the prescribed primary scalar and `h` or `e` the prescribed in-plane derived field. With `exp(+i omega t)`, the implemented field-valued conditions and their contributions to the normalized scalar weak form are:

```math
\begin{array}{lll}
E_z:\quad E_z=d, & \mathbf t\cdot\mathbf H=\mathbf t\cdot\mathbf h, &
\mathbf n\cdot(\mu_r^{-1}\nabla E_z)=+i\omega\mu_0\,\mathbf t\cdot\mathbf h,\\
H_z:\quad H_z=d, & \mathbf t\cdot\mathbf E=\mathbf t\cdot\mathbf e, &
\mathbf n\cdot(\epsilon_c^{-1}\nabla H_z)=-i\omega\epsilon_0\,\mathbf t\cdot\mathbf e.
\end{array}
```

The normal component of the supplied vector is ignored. The solver obtains the outward normal from the adjacent triangle, independently of the order in which the mesh lists the edge, and integrates the complex field expressions along that edge. Components use the model x and y axes. Dirichlet values are evaluated at boundary nodes; inconsistent values at shared or periodically identified nodes are rejected. Each field component has the same unit and physical meaning as the corresponding derived field, not the conormal's unit.

Zero Dirichlet gives PEC for `Ez` and PMC for `Hz`. Zero Neumann gives PMC for `Ez` and PEC for `Hz`; this is also the natural condition on unassigned nonperiodic exterior edges. These 2D labels refer to the corresponding z-invariant polarization.

## 5. Model tree, Study and interpretation

- **Geometry, selections, materials, Physics:** the cross-sectional geometry and domain selections define material regions; boundary selections identify edges for Physics and integrations. Physics selects `Ez` or `Hz`, domains, frequency and the fixed phasor convention.
- **Mesh and Study:** the mesh has linear triangles and the frequency-domain Study uses order-1 scalar elements. The Study formulation follows Physics. Frequency and parameter sweeps produce separate cases. `standard` and `centroid` constitutive quadrature sample the scalar coefficients differently; check convergence and possible mass underintegration.
- **Definitions and results:** parameters, pointwise variables, global variables and integration operators use the declared field/material symbols and allowed expression scopes. Results include the solved scalar and recovered components, their real/imaginary/magnitude field views, configured integrations/global data, and the `max_field_magnitude` sweep quantity. Report the polarization, frequency, units and normalization with any result.

Validate a model with an appropriate analytic wave or cavity case and mesh refinement. This is a 2D z-invariant approximation, not a general 3D vector simulation. The solver's allowed conditions and settings are described by the [solver capability contract](../../specification/contracts/solver-capability-contract.md).

For example, in a homogeneous lossless medium a +x plane wave with `Ez = E0 exp(-i k x)` has `Hy = -Ez/eta`, where `k = omega sqrt(mu epsilon)` and `eta = sqrt(mu/epsilon)`. With compatible boundary data, compare both the complex phase and the sign of `Hy` against this relation. A magnitude-only plot cannot detect a reversed phase or field sign.

## Lossless second-order frequency eigenproblem (reference)

This familiar frequency-domain reduction is retained as an analytical reference, not as the implemented Study contract. The universal eigenfrequency Study below uses the first-order transient state system and solves directly for the complex temporal rate `s`. For a source-free cavity with zero conductivity and positive frequency-independent `eps_r` and `mu_r`, the scalar wave equations in section 1 become

```math
-\nabla\cdot(a\nabla u)=\lambda\mu_0\epsilon_0 b\,u,
\qquad \lambda=\omega^2,
```

where `(a,b) = (1/mu_r, eps_r)` for `u = Ez`, and `(a,b) = (1/eps_r, mu_r)` for `u = Hz`. The left operator is the weighted scalar stiffness operator; the right operator multiplies the primary field by `mu0 epsilon0 b`. With homogeneous scalar Dirichlet and/or natural boundaries, testing by `v` and integrating by parts gives

```math
\int_\Omega a\,\nabla\overline v\cdot\nabla u\,dA
=\lambda\int_\Omega\mu_0\epsilon_0 b\,\overline v\,u\,dA.
```

`v` vanishes on Dirichlet boundaries and the bar denotes complex conjugation. Expanding `u` and `v` in the real triangle nodal basis gives

```math
K\mathbf u=\omega^2M\mathbf u,\qquad
K_{ij}=\int_\Omega a\,\nabla N_i\cdot\nabla N_j\,dA,\qquad
M_{ij}=\int_\Omega\mu_0\epsilon_0 b\,N_iN_j\,dA.
```

`M` discretizes the weighted field operator on the right; its weight depends on the chosen polarization. Apply homogeneous essential constraints to both matrices. A nonzero eigenvector is a mode shape, and for a positive eigenvalue `f = sqrt(lambda)/(2 pi)`. Natural-boundary problems can include a zero mode, for which the derived-field formulas dividing by `omega` do not apply. Conductivity, impedance or frequency-dependent coefficients require a more general eigenproblem.

## Prescribed longitudinal-wavenumber harmonic study and fixed-frequency propagation modes

**The prescribed-`kz` full-polarization route is declared only for `Full`.** The scenario contract exposes the Physics choices `Ez`, `Hz`, and `Full`; selecting prescribed `kz` with either scalar choice is rejected during Study preparation, including when the prescribed value is zero. Leaving prescribed `kz` off keeps the legacy zero-`kz` `Ez` or `Hz` reduction. In `Full`, a prescribed value of zero still assembles the complete electric-vector operator: under the current scalar-isotropic reciprocal material contract it separates into the two legacy polarization sectors, so a polarization-pure drive reproduces its corresponding scalar result. A Full solve with drives in both sectors contains both responses. A temporal-eigenfrequency regression compares both Full spectral branches against their scalar routes on matching meshes and passes with less than 5% relative difference. At finite `kz`, the sectors generally couple in a heterogeneous cross-section and must not be treated as separate scalar problems. With `e^{+i omega t}` time dependence and `exp(-i kz z)` longitudinal dependence, `partial_z = -i kz`; `kz` is a complex Harmonic Study quantity in `rad/m`. The cross-section and its scalar-isotropic material data, sources, and boundary drives must be invariant in `z` and share the selected longitudinal factor.

The Full field space uses first-order Whitney/Nedelec `H(curl)` edge unknowns for `(E_x,E_y)` and a compatible nodal `H1` P1 field for `E_z`; the magnetic vector is recovered from the electric unknown in Harmonic studies. The reduced curl operator is assembled with the shared sparse weak-form machinery; the Full route returns all six cross-sectional amplitudes `(E_x,E_y,E_z,H_x,H_y,H_z)` and records `kz` plus `exp(-i*kz*z)` in result metadata. The current material law is scalar and isotropic. Legacy projects that leave prescribed `kz` off and select `Ez` or `Hz` continue using their separate zero-wavenumber formulations.

The EM2D contract retains separate electric-current and equivalent magnetic-current meanings and source routes. Their component order, units, signs, and legacy component maps remain scenario declarations; an absent source contributes zero. Native checks drive a homogeneous vacuum axial plane wave, verify all six field amplitudes and the sparse residual, check the separate electric/magnetic source scales, and confirm that activating either source changes the finite-`kz` field. A divergence-free transverse-wave check confirms that the full-polarization field error decreases when the structured triangular mesh is refined from 8 to 16 divisions per side. A two-material finite-`kz` sparse solve also passes with finite fields and a small residual; its shared interior Whitney edge is verified to use one global trace DOF on both adjacent elements. A separate RIVRS/METIS regression at micrometre geometry and SI-scale wavenumber resolves the analytic PEC square-guide TE10 propagation constant within its stated 3% tolerance, with relative residual below `1e-6`. These checks verify scale handling and interface conformity, not the circular-fiber user demonstration or discontinuous-coefficient mesh convergence.

### Full-polarization option and Study compatibility

**Implementation status:** `Full` is an explicit Physics formulation in `em2d.json`. The shared sparse Harmonic and Propagation Mode routes use its declared operator form; Transient and temporal Eigenfrequency use its declared two-field transient form. These are separate Study-specific unknown layouts: the Harmonic electric-field route solves for the full electric vector and recovers the magnetic vector, while the first-order Transient/Eigenfrequency state contains both electric and magnetic vector fields.

The `Full` transient contract declares two three-component vector fields, `electric` and `magnetic`, and one `elementSpace`, `orientedTriangleNedelecH1DirectSum`. The generic triangle adapter creates a separate DOF block for each field, each using the same in-plane oriented Whitney edge functions plus longitudinal nodal H1 functions. JSON weak-form terms declare the inter-field curl couplings. In space notation, this is a product \(V_h\times V_h\), not one set of DOFs shared by both fields. The current form contract uses one element-space declaration for all fields in that form; per-field, different element spaces are not yet represented.

The common Harmonic Study supports the zero-\(k_z\) scalar routes and the Full electric-vector route at zero or prescribed \(k_z\). The longitudinal-wavenumber setting is projected from the scenario JSON: it is shown only when the selected formulation declares support for that Study type. A nonzero value with a scalar formulation fails preparation with a message naming the required Full formulation. Propagation Mode is a separate Study step: it fixes frequency and searches for complex \(k_z\), and its contract is Full-only. The solver returns all six field components and \(k_z\) itself as the eigenvalue.

For Full Harmonic, Transient, and temporal Eigenfrequency, the Settings projection exposes a prescribed-longitudinal-wavenumber checkbox and, when checked, a real/imaginary \(k_z\) quantity based on the scenario contract. Checking it selects the Full route even when the expression evaluates to zero; turning it off means \(k_z=0\). Existing saved projects with a wavenumber value retain the selected route. Harmonic response accepts complex \(k_z\); Transient and temporal Eigenfrequency initially require a real value. The selected Full field layout remains fixed if a sweep sample evaluates \(k_z\) to zero. For Transient, initial fields, sources, and boundary data must share the same longitudinal factor. For temporal Eigenfrequency, \(k_z\) is fixed and the solver returns temporal rate \(s\).

The legacy `Ez`/`Hz` Transient and temporal Eigenfrequency forms remain available at \(k_z=0\). With Full, both studies accept zero or prescribed finite real \(k_z\) and use the six-component first-order state. The generic finite-\(k_z\) operator substitutions are contract-selected and apply to both field blocks. Native checks include a discrete plane-wave phase and amplitude comparison under time-step refinement, and a selected heterogeneous source with mesh-refinement convergence. The [implementation plan](implement_em2d_full_polarization.md) records the acceptance work and remaining scope. Tensor-valued and nonreciprocal material laws are not part of the current scalar-isotropic EM2D material contract.

After implementation debugging and native acceptance tests, the non-gating user-perspective demonstration was completed by building and running a circular step-index optical fiber through the FullMeta in-app UI. The benchmark used $a=5\,\mu\mathrm m$, $n_\mathrm{core}=1.46$, $n_\mathrm{clad}=1.44$, and $\lambda_0=1.55\,\mu\mathrm m$. The first six-mode search identified HE\(_{11}\) and HE\(_{21}\) orientation pairs plus TM\(_{01}\) and TE\(_{01}\); all six real $k_z$ values agree with exact vector Bessel roots within 0.05%. A second search targeted the guided HE\(_{12}\) root and returned its two orientations at 5,857,554.593 and 5,857,253.969 rad/m, respectively, versus the exact 5,859,669.564 rad/m root (0.0361% and 0.0412% errors). Both searches used the same refined 1,764-triangle mesh. Refinement from 1,002 to 1,764 triangles reduced the maximum identified-family error in the first search from about 0.0383% to 0.0231%. Enlarging the square cladding box half-width from $R=15$ to $20\,\mu\mathrm m$ changed the coarse-mesh roots by at most 10 ppm. The detailed values, field-based mode identification, residuals, and the fact that the exterior was square-truncated are in the [Full-polarization implementation plan](implement_em2d_full_polarization.md#completed-demonstration-record). This demonstration is separate from numerical acceptance.

Visible in-app checks confirmed that all three Physics choices are selectable, Full passes model validation and Propagation Mode preparation, and a true Propagation Mode Study using scalar `Ez` is rejected during preparation with `Propagation Mode requires the Full formulation; selected "Ez"`. Restoring Full makes preparation pass. The fiber modes were inspected in the result viewer, including the four-lobed $|E_z|$ field for an HE\(_{21}\) mode and the higher radial HE\(_{12}\) field pattern.

A subsequent regression check found that switching a Study from Propagation Mode to Transient and back could leave `useLongitudinalWavenumber: false` beside the retained Propagation Mode settings. The shared propagation adapter now sets this flag to true only on its internal cloned model while sampling each trial $k_z$, so stale editor state cannot collapse its polynomial samples to $k_z=0$. The `propagation_sampling_overrides_a_disabled_stale_study_wavenumber` Rust regression and end-to-end acoustic and EM2D propagation tests with the same stale disabled value pass, along with the full Rust suite. The latest staged bundle builds and passes executable smoke checks. Its fresh in-app replay on port 8765 prepared successfully and completed six Full-polarization modes; the visible Mode 1 field and $k_z$ were checked against the exact HE\(_{11}\) reference (0.00761% relative error). The separately constructed non-gating UI demonstration retains the multi-family Bessel, mesh-refinement, and cladding-radius comparisons above. Port 8766 is reachable, but it serves a stale FEM scenario registry with five packages instead of the current eight; it omits the three current 2D/3D elasticity and 3D thermal profiles, and rejects `model.physics.formulation = "Full"` as outside the `Ez`/`Hz` enum. The test therefore does not establish compatibility of the current FEM bundle with the older host.

#### Full-polarization transient and temporal eigenfrequency equations

Let \(\mathbf C_k\mathbf F=(\nabla_t-i k_z\mathbf e_z)\times\mathbf F\). With the declared electric- and magnetic-current convention, the six-field transient equations are

$$
\partial_t(\boldsymbol\epsilon\mathbf E)-\mathbf C_k\mathbf H=-\mathbf J_e,
\qquad
\partial_t(\boldsymbol\mu\mathbf H)+\mathbf C_k\mathbf E=-\mathbf J_m.
$$

Both fields have the same longitudinal factor \(\exp(-i k_z z)\). Initial fields, domain sources, and boundary data must use that factor too. If material coefficients vary in time, the equations retain the product derivatives \(\partial_t(\boldsymbol\epsilon\mathbf E)\) and \(\partial_t(\boldsymbol\mu\mathbf H)\).

For time-independent coefficients, the sparse semidiscrete state is

$$
\mathsf M\dot{\mathbf u}+\mathsf K(k_z)\mathbf u=\mathbf f(t),
\qquad
\mathbf u=(E_x,E_y,E_z,H_x,H_y,H_z)^{\mathsf T}.
$$

With \(\mathbf u(t)=\boldsymbol\phi e^{st}\), temporal Eigenfrequency solves

$$
\bigl(\mathsf K(k_z)+s\mathsf M\bigr)\boldsymbol\phi=0,
\qquad
\boldsymbol\phi\ne0.
$$

Here \(k_z\) is prescribed and fixed; the eigenvalue is temporal rate \(s\). This differs from Propagation Mode, where frequency is prescribed and the eigenvalue is \(k_z\).

### Implemented Full-only propagation-mode backend

The propagation-mode backend is a coupled full-polarization solve, and the scenario contract now declares it for `Full` only. Preparation rejects a scalar `Ez` or `Hz` Physics formulation with a diagnostic that names Full as the required formulation.

The Propagation Mode backend fixes positive real frequency and computes complex longitudinal eigenvalues and coupled mode fields. Harmonic response instead prescribes `kz`, while temporal Eigenfrequency solves for the temporal rate `s`.

The shared propagation adapter samples the current source-free full-polarization sparse harmonic operator at three `kz` values, forms the declared quadratic matrix polynomial $P(k_z)=P_0+k_zP_1+k_z^2P_2$, and independently checks the polynomial against further operator samples. A generic sparse first-companion pencil then solves for `kz` itself using the complex generalized eigensolver. Its finite modes are checked against the original polynomial residual. The linear coefficient is retained without symmetry assumptions. The solver does not assume Hermiticity, reciprocity, real eigenvalues, or `+/-` pairing, and does not infer a mode by changing the sign of another eigenvalue.

The current EM2D material contract is scalar-isotropic. This implementation does not yet declare anisotropic or non-reciprocal material tensors; the generic polynomial and sparse solver path preserves arbitrary complex nonsymmetric coefficients, but that alone does not constitute tensor-material support. The artifact contains all six cross-sectional amplitudes `(Ex,Ey,Ez,Hx,Hy,Hz)`, the selected frequency, complex `kz` in `rad/m`, and the original-operator residual. Source and incident amplitudes are omitted; supported homogeneous boundary constraints and operator contributions are kept. Direction/energy-flux classification is not currently emitted.

Native acceptance uses the `Full` formulation for analytic isotropic PEC-guide dispersion and mesh refinement, including a below-cutoff evanescent root and a passive lossy case with complex `kz`. It also verifies that scalar `Ez`/`Hz` requests are rejected, all six field components are emitted, and the original-operator residual is small. The solver does not synthesize a reciprocal sign partner. The complete Rust suite passes with the matching FEM-local DLLs (382 passed, 0 failed, 3 ignored); all 79 FEM Node tests pass; the staged optimized release build and executable `--help`/`--capabilities` smoke checks pass. In-app UI compatibility checks and the non-gating fiber demonstration were completed on local 8765. A fresh replay on the latest staged bundle prepared and completed six modes for the stale-setting EM2D acceptance project, returning Mode 1 at 5,904,508.6065 rad/m (0.00761% from the exact HE\(_{11}\) root). A separate Full Transient UI smoke check used `material_ui_test.json` with prescribed `kz = 10 rad/m`, passed preparation, and completed one 0–1 s case in 15.65 s; its initial fields were zero, so it verifies the editor-to-runtime path rather than nontrivial transient physics. Port 8766 is reachable but serves a stale five-package FEM registry and rejects `Full` as an EM2D Physics formulation, so compatibility with the current package on that host remains unverified.

The current propagation adapter supports first-order triangular `deRhamTriangle` elements. The request supports mode count, complex inverse-length target/shift, Krylov dimension and residual tolerance; parameter-sweep cases are prepared and bound to their staged model parameters. Focused acceptance covers Full-only formulation routing, isotropic PEC-guide dispersion, mesh refinement, an evanescent root and a lossy complex-`kz` case. The [fixed-frequency propagation-mode implementation plan](implement_2d_propagation_mode_study.md) records the shared numerical path, remaining limits, and completed post-debug cylindrical acoustic UI demonstration.


Let $\nabla_t=(\partial_x,\partial_y,0)$, $\mathbf e_z=(0,0,1)$, and define

```math
\nabla_k=\nabla_t-i k_z\mathbf e_z,\qquad C_k\mathbf E=\nabla_k\times\mathbf E,\qquad \epsilon_c=\epsilon-\frac{i\sigma}{\omega}.
```

For a source-free medium, the full coupled cross-sectional electric-field equation is

```math
C_k(\mu^{-1}C_k\mathbf E)-\omega^2\epsilon_c\mathbf E=0,\qquad \mathbf H=\frac{i}{\omega\mu}C_k\mathbf E.
```

With transverse test amplitude $\mathbf V$, a representative weak volume equation is

```math
\int_\Omega (C_{-k}\overline{\mathbf V})\cdot\mu^{-1}C_k\mathbf E\,dA-\omega^2\int_\Omega\overline{\mathbf V}\cdot\epsilon_c\mathbf E\,dA=\ell(\mathbf V).
```

The test operator uses $-k_z$ as the formal integration-by-parts partner, including when searching for complex $k_z$; it is not a substitution of $\overline{k_z}$. Boundary terms and electromagnetic constraints depend on the chosen formulation. In a future tensor-material extension, $\epsilon_c$ and $\mu$ may be tensors, with $\mu^{-1}$ acting as a tensor on the curl. A nonreciprocal constitutive tensor need not be symmetric or Hermitian; preserve tensor order and use a general non-Hermitian eigensolver where required. Do not assume paired eigenvalues at $+k_z$ and $-k_z$, or construct one direction by negating the other. Compute the modes and their fields separately. The mode study must retain $k_z$ itself as the eigenvalue. For example, a suitable mixed or augmented discretization can target the generalized pencil

```math
A(\omega)\mathbf x=k_z B(\omega)\mathbf x,\qquad \mathbf x\ne0,
```

where the state includes the field components and any auxiliary variables and constraints needed by the formulation. Tensor couplings must remain in the operators; the pencil must not enforce opposite-sign pairs. This is a formulation target: suitable spaces and treatment of singular constraints and spurious modes still require design and validation.

### Implemented finite-$k_z$ wave equation

At a prescribed $k_z$, eliminating $\mathbf H$ from Maxwell's equations gives the full-polarization electric wave equation

```math
(\nabla_t-ik_z\mathbf e_z)\times\left[\boldsymbol\mu^{-1}(\nabla_t-ik_z\mathbf e_z)\times\mathbf E\right]-\omega^2\boldsymbol\epsilon_c\mathbf E=-i\omega\mathbf J_e.
```

For the $e^{i\omega t}$ convention, $\boldsymbol\epsilon_c=\boldsymbol\epsilon-i\boldsymbol\sigma/\omega$; $\mathbf J_e$ is the electric current density with the same longitudinal dependence. The driven wave equation and propagation-mode study are implemented for the current scalar-isotropic material contract; tensor-valued and nonreciprocal material laws remain future work. Although the electric-field operator is quadratic in $k_z$, its polynomial companion solve returns $k_z$ itself as the eigenvalue.

### Continuous first-order Maxwell pencil: a Hermitian special case

The following is a continuous first-order Maxwell reference that explains when a Hermitian pencil can arise. It is **not** the discrete state/pencil currently used by the propagation Study above, which uses the existing full-polarization second-order electric-field operator and its sparse quadratic companion. The special-case derivation does not limit the general sparse solver.

At a fixed real frequency, for source-free lossless media with Hermitian (possibly nonreciprocal) permittivity and permeability tensors, define $C_0=\nabla_t\times$ and $Z\mathbf f=\mathbf e_z\times\mathbf f$. For the convention $e^{i\omega t-ik_z z}$, the first-order Maxwell equations give

```math
\mathbf x=\begin{bmatrix}\mathbf E\\\mathbf H\end{bmatrix},\qquad
A(\omega)\mathbf x=k_z B\mathbf x,
```

with the continuous operator blocks

```math
A(\omega)=\begin{bmatrix}
\omega\boldsymbol\epsilon&iC_0\\
-iC_0&\omega\boldsymbol\mu
\end{bmatrix},\qquad
B=\begin{bmatrix}0&-Z\\Z&0\end{bmatrix}.
```

Under boundary and interface conditions that eliminate the integration-by-parts boundary form and give the appropriate operator domains, $C_0$ is formally self-adjoint and $Z^\dagger=-Z$. Thus $A^\dagger=A$ and $B^\dagger=B$ when $\boldsymbol\epsilon^\dagger=\boldsymbol\epsilon$ and $\boldsymbol\mu^\dagger=\boldsymbol\mu$. Hermitian material tensors need not be symmetric: lossless nonreciprocal media are not excluded, and no $+k_z/-k_z$ pairing is assumed.

For an eigenmode with $\mathbf x^\dagger B\mathbf x\ne0$, both quadratic forms below are real, so the propagation eigenvalue is real:

```math
k_z=\frac{\mathbf x^\dagger A(\omega)\mathbf x}{\mathbf x^\dagger B\mathbf x}\in\mathbb R.
```

Here the dagger denotes the cross-sectional inner-product adjoint. The Hermitian $B$ is indefinite and singular, not a positive-definite mass matrix. Modes with zero $B$ quadratic form are not covered by the quotient argument and may include evanescent modes. Loss, gain, open boundaries or general complex material tensors require a potentially non-Hermitian formulation. These are continuous operators. The current discrete study instead uses the declared scalar-isotropic H(curl) harmonic operator, sparse polynomial extraction, and a general first-companion solve; it does not yet realize this first-order $(\mathbf E,\mathbf H)$ pencil or the tensor material contract.

## Transient study

The [scenario JSON](../src/scenarios/em2d.json) declares transient Studies for z-invariant scalar `Ez` and `Hz` and for Full polarization. The Full form declares separate electric and magnetic vector blocks in one shared oriented triangle Whitney edge-plus-nodal space; the assembler allocates independent DOFs per block and reads their couplings, material terms, sources, and boundary laws from the contract. A prescribed finite real `kz` is applied to the full 3D differential operators on the 2D cross-section. The scalar forms are valid only at `kz = 0`. Prescribed scalar-field values, complementary tangential-field inputs, instantaneous impedance, and initial fields use the declared boundary/input routes. A nonzero legacy scalar Neumann input remains unsupported. Native Full-transient checks cover zero and finite `kz`, complex field evolution, backward-Euler energy balance, discrete plane-wave phase/time-step convergence, and heterogeneous selected-source mesh convergence. A visible editor-to-runtime smoke run also completed with finite `kz`; because its initial fields were zero, it does not replace those nontrivial native physics checks.

The native `scenario_operators_obey_physical_energy_and_heat_balances` test checks the assembled backward-Euler electromagnetic energy identity for `Ez`, `Hz`, and Full. A focused Full test checks its energy balance directly. Spatial refinement checks the scalar polarizations on structured and perturbed-interior simplex meshes; the Full harmonic finite-`kz` route has a separate structured-mesh field-error refinement check.

For scalar instantaneous permittivity $\epsilon$, permeability $\mu$, conductivity $\sigma$, electric current density $\mathbf J_{e,s}$ and equivalent magnetic current density $\mathbf J_{m,s}$, the shared 3D equation is

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

Both constitutive derivatives are product derivatives when coefficients depend on time. The transient solver declares separate z-invariant `Ez` and `Hz` reductions: the first uses an electric current source, while the second uses a magnetic current source in its magnetic equation. The full component equations and the two distinct discrete block matrices are given in the shared reference. Harmonic Studies have a separate prescribed-`kz` full-polarization route, described above; this does not change the transient field state. Initial fields must satisfy applicable constraints. Time-domain inputs are real, not harmonic phasors; dispersive or memory-dependent materials require causal constitutive laws and potentially auxiliary variables.

The [shared transient reference](transient_study_contract.md) documents the component equations, weak forms, units, boundary and result contracts, sparse time step, and current limitations.

## Domain Source and Additional Domain Source

The two PHYSICS features represent distinct Maxwell source terms. Domain Source is electric current density $\mathbf J_e$; Additional Domain Source is equivalent magnetic current density $\mathbf J_m$, often denoted $\mathbf M$ in dual-source Maxwell notation. Here $\mathbf J_m$ is an impressed source, not material magnetization. Their SI units are A/m² and V/m², respectively. Each feature has its own domain selection and expression; multiple entries add, including on overlapping selections, and an absent feature contributes zero. Expressions use physical coordinates and model parameters, with frequency variables in harmonic evaluations and $t$ in seconds in transient evaluations.

For the z-invariant scalar formulations, the scenario contract routes the out-of-plane components as follows:

- **Ez:** Domain Source is $J_{e,z}$ (UI label **J_ez**, A/m²), entering Ampere's equation.
- **Hz:** Domain Source is $J_{m,z}$ (UI label **J_mz**, V/m²), entering Faraday's equation.

The normalized harmonic weak-RHS coefficients remain $-i\omega\mu_0$ for Ez and $-i\omega\epsilon_0$ for Hz. The complementary in-plane Additional Domain Source uses $J_{m,x},J_{m,y}$ (UI labels **J_mx, J_my**, V/m²) for Ez, and $J_{e,x},J_{e,y}$ (UI labels **J_ex, J_ey**, A/m²) for Hz.

~~~text
rotatedGrad(a) = (partial_y(a), -partial_x(a))

Ez: mu * partial_t(H_t) + rotatedGrad(Ez) = -(J_mx, J_my)
Hz: epsilon * partial_t(E_t) - rotatedGrad(Hz) = -(J_ex, J_ey)

Ez harmonic load: -integral(rotatedGrad(test) dot (J_mx, J_my) / muRelative) dA
Hz harmonic load:  integral(rotatedGrad(test) dot (J_ex, J_ey) / epsilonComplex) dA
~~~

The two source types have separate selections and expressions and can be active together. The JSON declares each component label, unit, operator, coefficient and sign. In transient studies, $\mathbf J_m$ loads the in-plane magnetic-field equation for Ez, while $\mathbf J_e$ loads the in-plane electric-field equation for Hz; see the [EM2D contract](../src/scenarios/em2d.json).

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.

## Boundary vector count validation

An enabled Cartesian Neumann vector must have exactly two components for either scalar formulation. The scalar-only representation remains supported. Semantic model validation checks the primary `fieldValues` count from the scenario’s declared boundary input; the solver retains its own check. Enabled vectors with an invalid type or component count receive a semantic validation error. To repair one in the editor, turn off “Specify field components” and turn it back on to recreate the declared defaults; this reset replaces the existing components. Disabled conditions skip these semantic checks.

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

Current fixed-frequency propagation acceptance passes 12 Rust tests, 10 generic sparse-eigensolver tests, and 65 focused FEM Node contract/result/UI tests. The propagation tests cover declared JSON preparation and result resolution, all three enabled formulations, acoustic rigid-wall dispersion/cutoff and the pressure-Dirichlet guide, signed direct-`kz` targeting, impedance-operator retention with incident-load removal, parameter-case reassembly, axial-velocity recovery, EM2D PEC dispersion/refinement for both polarizations, a below-cutoff evanescent root, a passive lossy complex root, and elastic longitudinal/shear branches with stress recovery. The complete native Rust suite reports 382 passed, 0 failed, and 3 ignored with the matching FEM-local runtime DLLs; all 79 FEM Node tests pass. The staged optimized bundle and executable smoke checks also pass. The wider FEM verification history is recorded in the [universal Eigenfrequency implementation plan](implement_universal_eigenfrequency_study.md).
