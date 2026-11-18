# 2D thermal conduction: model and implementation

This manual connects the `thermal-2d` model tree to its stationary conduction equation and linear-triangle solver. It is intended for users, scientists and developers. See the [project guide](../../docs/README.md), [Model 2 reference](../../docs/fullmeta-model-2.md), [scenario contract](../src/scenarios/thermal2d.json), and [solver](../src/prepared_scalar.rs). The application's numerical acceptance remains open as described in the project guide.

## 1. Physics and fields

This is a **stationary heat-conduction** problem in an x–y cross-section, not a wave or transient heat equation. Temperature `T` in kelvin is the primary field; the physical heat flux `q` in `W/m^2` follows Fourier's law:

```math
\mathbf q=-k\nabla T,\qquad \nabla\cdot\mathbf q=Q,
\qquad -\nabla\cdot(k\nabla T)=Q.
```

Here `k` is scalar thermal conductivity and `Q` is volumetric heat generation in `W/m^3`, zero when no Domain Source is enabled. The first two equations use only first-order spatial derivatives when flux is included; the last equation eliminates flux. Transient heat capacity, convection/radiation boundaries and thermal impedance are not modeled.

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

For a homogeneous slab `0 <= x <= L` with zero domain source, `T(0) = T_left`, `T(L) = T_right` and insulated top and bottom, the exact solution is `T(x) = T_left + (T_right-T_left)x/L` and `qx = -k(T_right-T_left)/L`. At the left boundary the outward normal is `-x_hat`, so its outward flux is `-qx`; at the right it is `qx`. This provides a direct sign and conservation check.

## Domain Source

The current implementation supports multiple Domain Source entries under PHYSICS, each with an existing domain selection and expressions in spatial coordinates. Overlapping entries add; no entry means zero source. The project model stores selections and expressions, while the scenario contract declares their physical interpretation and weak-form contribution. This implementation is undergoing verification; its availability in a deployed installation depends on compatible FEM and platform runtimes.

For this stationary cross-section, the source is volumetric heat generation. The 2D formulation is interpreted per unit thickness, and the pure-Neumann compatibility check includes the integrated source.

For source density Q in W/m^3, use `-div(k grad T) = Q` and add `integral(test * Q) dA` per unit out-of-plane thickness.

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.
