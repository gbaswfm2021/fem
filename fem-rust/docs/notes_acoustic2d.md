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

## 4. Mesh, Study and results

- **Geometry / selections / mesh:** build 2D domains, assign materials to domain selections, and select boundary edges for conditions and integrations. The geometry mesh uses linear triangles. Mesh size controls resolve geometry and wavelength; perform a refinement study rather than treating a fixed elements-per-wavelength count as a guarantee.
- **Study:** frequency-domain, pressure formulation, order 1. A single frequency or declared parameter/frequency sweep produces separate cases. Backends and quadrature are Study solver settings; centroid integration can change accuracy and may underintegrate mass terms. Mesh order and solution order are separate concepts, even though both are 1 here.
- **Definitions and results:** parameters and pointwise/global expressions may use declared scope, including pressure, velocity and material symbols. Boundary/domain/edge integration and point evaluation operate on selected entities. Outputs include complex pressure and recovered velocity, field plots by real/imaginary/magnitude, configured global-variable data and `max_pressure_magnitude_Pa` sweep output. Inspect output selections, units and any averaging before interpreting an interface value. See [Model 2 results and Study](../../docs/fullmeta-model-2.md).

No nonzero acoustic material loss, nonlinear acoustics, or transient solver is claimed here. Use a mesh and frequency convergence check and an analytic plane-wave or cavity benchmark appropriate to the selected boundaries.

For a simple check in a homogeneous lossless fluid, a +x travelling plane wave has `p = P exp(-i omega x/c)` and `vx = p/(rho c)`. Set boundary data consistent with that wave and the outward normals; compare complex pressure and velocity rather than only their magnitudes. The 2D result is a cross-sectional field per unit out-of-plane extent, not a total 3D radiated power.

## Future eigenfrequency study

An eigenfrequency Study is not implemented. For a source-free, lossless cavity with positive, frequency-independent `rho` and `c`, write the pressure equation from section 1 as

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

## Domain Source

The current implementation supports multiple Domain Source entries under PHYSICS, each with an existing domain selection and expressions in spatial coordinates. Overlapping entries add; no entry means zero source. The project model stores selections and expressions, while the scenario contract declares their physical interpretation and weak-form contribution. This implementation is undergoing verification; its availability in a deployed installation depends on compatible FEM and platform runtimes.

For this pressure formulation, Q is a volume injection rate per volume, in Hz, in `i omega p/(rho c^2) + div(v) = Q`. The pressure weak equation receives `+i omega integral(test * Q) dA`, per unit out-of-plane thickness.

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.
