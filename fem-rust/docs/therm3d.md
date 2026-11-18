# 3D thermal conduction: model and implementation

This guide describes the current `thermal-3d` stationary conduction scenario,
its model settings and its linear-tetrahedron solver. See the [FEM project
guide](../../docs/README.md), [Model 2 reference](../../docs/fullmeta-model-2.md),
[scenario contract](../src/scenarios/thermal3d.json), and
[solver implementation](../src/prepared_scalar.rs).

## Physics and fields

Temperature `T` in kelvin is the primary unknown. For isotropic conductivity
`k`, the heat-flux vector `q` and stationary equation are

```math
\mathbf q=-k\nabla T,\qquad \nabla\cdot\mathbf q=Q,
\qquad -\nabla\cdot(k\nabla T)=Q.
```

`Q` is volumetric heat generation in `W/m^3` and is zero when no Domain Source
is enabled. The scenario has no transient heat capacity and does not model
convection, radiation, or thermal impedance boundary conditions. Heat can also
enter or leave through prescribed-temperature or prescribed-flux boundaries.

The scenario exposes `T` as its primary plot field. The solver also recovers a
three-component heat flux for each tetrahedron and writes it to the field file;
the scenario does not declare those components as selectable derived field
symbols.

## Weak form and finite elements

With test function `v`, and outward normal heat flux `q_n = q · n`, the weak
equation is

```math
\int_\Omega k\nabla v\cdot\nabla T\,dV
=\int_\Omega vQ\,dV-\int_{\Gamma_N}v\,q_n\,dS.
```

Prescribed temperatures on the Dirichlet boundary are imposed separately. On
each straight tetrahedron, the solver uses four first-order nodal temperature
values and linear Lagrange shape functions `N_i`:

```math
T_h=\sum_{i=1}^4 T_iN_i,\qquad
K_{ij}^{(e)}=k_e V_e\nabla N_i\cdot\nabla N_j,\qquad
\mathbf q_h=-k_e\sum_{i=1}^4 T_i\nabla N_i.
```

For constant `k` on a tetrahedron, the gradient and recovered heat flux are
constant within that cell. Temperature is continuous at shared nodes; flux
can jump between cells, including across material interfaces. The recovered
cell flux is not an independently solved, globally continuous normal-flux
field.

## Materials and boundaries

Assign each active domain a positive, finite, scalar `thermalConductivity`
(`k`) in `W/(m*K)`. The scenario's default is `1 W/(m*K)`. Each assignment is
constant in space; different domains can use different values. Conductivity,
boundary values, and the optional mean temperature may use expressions of
model parameters. The thermal adapter rejects spatial `x`, `y`, and `z`
expressions for these quantities.

| Condition | Meaning |
| --- | --- |
| Dirichlet | Prescribes one real scalar temperature `T = d` in K. The default is `293.15 K`. |
| Neumann | Prescribes one real scalar **outward normal heat flux** `q · n = q_n` in `W/m^2`. The default is `0 W/m^2`. |

Positive Neumann flux removes heat from the domain; negative flux adds heat.
The solver applies `-q_n A/3` to each node of a triangular boundary face of
area `A`. An unassigned exterior face has zero prescribed normal flux, so it
is insulated. The 3D thermal solver uses the scalar boundary `value`; it does
not implement a three-component `fieldValues` heat-flux condition. It rejects
conflicting prescribed temperatures at shared nodes, repeated or invalid
boundary faces, and multiple conditions targeting the same boundary tag.

Periodic thermal-3D conditions and periodic mesh pairs are unsupported.

### Pure-flux components

If a connected part of the mesh has no prescribed temperature, its integrated
outward heat flux must equal the integrated heat generation. Otherwise the solver rejects the case.
For each such connected component, a volume-weighted mean-temperature
constraint fixes the otherwise arbitrary additive temperature. The Study's
`meanTemperature` supplies the reference; its default is `0 K`. Set a suitable
reference if the absolute temperature is important. The reference fixes the
temperature level; it does not compensate for an unbalanced heat load.

## Mesh, Study and output

1. Create 3D domains, assign their thermal materials, and define exterior
   boundary selections for temperatures or outward heat fluxes.
2. Generate a 3D tetrahedral mesh and select a stationary Study with the `T`
   formulation and first-order elements. Solver backend, ordering, and thread
   settings are resolved through the shared solver capabilities.
3. Run the Study and inspect the temperature result. The scenario does not
   enable parameter sweeps, pointwise variables, or global variables. Its
   contract declares integration operators, but that declaration alone does
   not establish a stationary integration-result workflow.

When field output is enabled, the default `temperature.json` contains nodal
temperature, per-cell heat flux and conductivity, mesh data, solver metadata,
volume, and recovered outward boundary power and area by mesh tag. Recovered
power is calculated from the cell flux on each recorded boundary face; it is
an approximate postprocessed result and need not exactly balance on every
finite mesh. The default `thermal-summary.csv` reports the unknown count,
residual, and minimum and maximum temperature.

## Simple check

For a homogeneous slab `0 <= x <= L` with zero domain source, insulated on its other faces, with
`T(0) = T_left` and `T(L) = T_right`, the exact solution is

```math
T(x)=T_{\rm left}+\frac{T_{\rm right}-T_{\rm left}}{L}x,
\qquad q_x=-k\frac{T_{\rm right}-T_{\rm left}}{L}.
```

The outward normal flux is `-q_x` on the left and `q_x` on the right. Compare
the calculated temperature and flux with these values and check heat balance;
use mesh refinement for more complex geometries or material interfaces.

## Domain Source

The current implementation supports multiple Domain Source entries under PHYSICS, each with an existing domain selection and expressions in spatial coordinates. Overlapping entries add; no entry means zero source. The project model stores selections and expressions, while the scenario contract declares their physical interpretation and weak-form contribution. This implementation is undergoing verification; its availability in a deployed installation depends on compatible FEM and platform runtimes.

For stationary conduction, the source is volumetric heat generation. The pure-Neumann compatibility check includes the integrated source.

For source density Q in W/m^3, use `-div(k grad T) = Q` and add `integral(test * Q) dV`.

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.
