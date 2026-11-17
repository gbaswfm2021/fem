# 2D electromagnetics: model and implementation

This manual connects the `em-2d` model tree to its equations, triangle discretization and outputs. It is for users, scientists and developers; a declared option does not by itself establish numerical acceptance. See the [project guide](../../docs/README.md), [Model 2 reference](../../docs/fullmeta-model-2.md), [scenario contract](../src/scenarios/em2d.json), and [solver](../src/scenarios/em2d.rs).

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
