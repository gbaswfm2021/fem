# Transient Study: equations and implementation contracts

## Status and scope

**Enabled in all eight scenario packages.** Each JSON declares its `transientForms`, selection plan, supported Study type and `transientPlan` preparation route. The scenario-neutral sparse assembler and coordinator are connected to the same executable used for other Studies. The release coordinator has passed synthetic runs for all declared formulations, including nonzero Domain Sources, space/time-dependent materials and time-varying prescribed boundaries. Browser-visible editor behavior and the older deployed MSIX host still need live verification; these checks are not inferred from native tests.

Each project supplies geometry, mesh, selections, material expressions, sources, boundary and initial values, Study settings and parameter-sweep values. A scenario package declares field names and units, weak-form terms, constitutive coefficients, source routing and output symbols. Shared code interprets those declarations. An additional scenario can use the common machinery through data only **when** its equations, fields, finite-element spaces, boundaries, materials, recovery and solver needs are already supported by the generic contracts. New mathematical capabilities still require generic code and verification.

The transient fields below are time-domain state variables, not harmonic phasors. All spatial coordinates are in the model frame and all times are in seconds. Material coefficients may be prescribed expressions of parameters, position and time. General nonlinear state-dependent constitutive laws, memory kernels and arbitrary frequency-domain dispersion are not supported.

## Common weak form and sparse matrix equation

Let `u` collect the degrees of freedom of every declared state field, including different field blocks, and let each test and trial operator be specified by the scenario. A spatial term is assembled from the declared coefficient and test/trial operators. The generic solver permits both a **state derivative**, `M(t) du/dt`, and a **conserved-product derivative**, `d[M(t)u]/dt`, in the same system. These are different for time-dependent coefficients. The assembled system is

```math
\sum_{a\in\mathcal S} M_a(t)\,\dot{\mathbf u}(t)
+\sum_{b\in\mathcal P}\frac{d}{dt}\!\left(M_b(t)\mathbf u(t)\right)
+K(t)\mathbf u(t)=\mathbf f(t).
```

Each `M_a`, `M_b` and `K` is assembled and stored sparsely, including rectangular cross-field contributions in the global block layout. Materials and active sources are sampled at physical quadrature points and at the integration stage. Repeated Domain Source features add on overlapping selections. With no Domain Source features, the load contribution is zero and no source expressions are compiled or sampled. The current mesh adapters use nodal P1 simplices, or oriented tetrahedral edge elements of orders 1 and 2; they do not yet support arbitrary mixed element families in one run.

More explicitly, for a declared term `r` with test basis `N_i^a`, trial basis
`N_j^b`, operators `L_r` and `R_r`, and coefficient `c_r(x,t)`, its element
contribution has the schematic form

```math
A_{ij}^{(r,e)}(t)=\int_{\Omega_e}
  \bigl(L_r N_i^a\bigr)^*\,c_r(\mathbf x,t)\,
  \bigl(R_r N_j^b\bigr)\,d\Omega.
```

The actual tensor contractions and component ordering follow the declared
operator and coefficient shapes. Element contributions scatter into the row
block for field `a` and column block for field `b`, including orientation
factors of the discrete basis. The term's derivative tag assigns the resulting
sparse block to `M_a`, `M_b`, or `K` in the equation above. A Domain Source
integral uses its declared test operator, target row block, coefficient and
component values. The star denotes conjugate transpose where applicable; the
supported time-domain inputs are real.

For backward Euler, with `h = t_n - t_(n-1)`, the internally implemented step is

```math
\left[\frac{1}{h}\sum_{a\in\mathcal S}M_a(t_n)
+\frac{1}{h}\sum_{b\in\mathcal P}M_b(t_n)+K(t_n)\right]\mathbf u_n
=\mathbf f(t_n)
+\frac{1}{h}\sum_{a\in\mathcal S}M_a(t_n)\mathbf u_{n-1}
+\frac{1}{h}\sum_{b\in\mathcal P}M_b(t_{n-1})\mathbf u_{n-1}.
```

Numerical values are recomputed at each stage; topology-derived sparse assembly patterns and solver symbolic analysis can be reused when their inputs remain valid. The current solver refactorizes numerically at each step. It clips steps to requested output times and enforces a maximum step count. Backward Euler has numerical damping, particularly relevant for waves; convergence and accuracy are not guaranteed by a successful solve alone.

For a **fixed** weighted identification `u = P û`, each matrix and load are reduced as `Pᴴ M P`, `Pᴴ K P` and `Pᴴ f`. Orientation signs and fixed periodic factors are represented in `P`; a time-dependent map would introduce extra derivative terms and is not implemented by this operation. Prescribed values are applied to the effective sparse system at each step. The periodic adapter evaluates each prepared case's parameters, checks matching exterior facet connectivity and rejects nonzero phase or overlapping ordinary boundary conditions. It lifts the node identification to nodal fields and the oriented moment identification to edge fields. A changed mesh requires a freshly constructed map. Initial data must satisfy identified and essential constraints; the edge-boundary runner checks essential moments independently before constrained projection. The native coordinator exercises this path for advertised periodic scenarios.

For order-two oriented face moments, the current correspondence requires the
matched canonical tangent modes to remain parallel mode by mode. A mapping
that mixes those moments is rejected; a general face-mode transformation is
not yet implemented. The tests cover translated matched surfaces at both
orders, not every possible geometry or coordinate transformation.

## Scenario declarations

### Reading a scenario contract

The following excerpt is taken from the enabled `thermal2d.json` contract. It is
an illustration of the scenario declaration, not a project model:

```json
{
  "fields": [{ "id": "temperature", "components": 1, "unit": "K" }],
  "terms": [
    { "testField": "temperature", "trialField": "temperature",
      "test": "value", "trial": "value", "coefficient": "capacity",
      "timeDerivative": "state" },
    { "testField": "temperature", "trialField": "temperature",
      "test": "gradient", "trial": "gradient", "coefficient": "conductivity",
      "timeDerivative": "none" }
  ],
  "coefficients": { "scalars": {
    "capacity": { "symbol": "Cvol" }, "conductivity": { "symbol": "k" }
  }, "matrices": {} }
}
```

`testField` selects the matrix row block and `trialField` selects the column
block; they need not name the same field. `test` and `trial` select operators
on the corresponding basis functions, and `coefficient` names the sampled
scalar or tensor that contracts those operators. Thus the first term contributes
`integral(N_i Cvol N_j)` to the temperature block multiplying the **state
derivative**; the second contributes `integral(grad(N_i)·k grad(N_j))` to
the spatial matrix. `timeDerivative: "none"` marks a spatial/algebraic term.
The other supported derivative law, `"conservedProduct"`, differentiates the assembled
coefficient times the state and has the distinct backward-Euler history term
shown above. The actual JSON's `dimension`, field component counts and tensor
shapes constrain the permitted operators; malformed combinations are rejected.

`materialProperties` declares the material symbol, unit and any default;
`materialBindings` associates it with a coefficient and supplies bounds.
The project provides the actual material expressions and domain assignments.
`domainSources` declares each source route's feature type, target field,
component count, unit, test operator and sign; each enabled project source
supplies its own expression and selection. `transientSelectionPlan` declares
how materials, sources and boundary features are associated with mesh entities.
A selection declaration alone does not implement the corresponding transient
boundary physics. Finally, `stateOutputs` maps state fields and components to
result symbols. Stress recovery is a separate declared operation, not another
primary state field.

### Additional Domain Source route across scenarios

All eight 2D and 3D scenario packages declare the same `additionalDomainSource`
feature type. Its physical meaning comes from that scenario's JSON route. The
shared assembler reads the selected-domain expressions, samples them at the
declared quadrature points, applies the configured weak-form operator and
coefficient, and adds the load to the sparse right-hand side. An absent source
has no expression-sampling cost; overlapping source selections add.

| Scenario | Additional source meaning | Transient target and weak-form route |
| --- | --- | --- |
| Thermal 2D/3D | Additive heat flux `q_s`, W/m² | Temperature equation, gradient test; `integral(grad(test) dot q_s)` |
| Acoustic 2D/3D | Momentum force density `f`, N/m³ | Velocity equation, value test; harmonic pressure route uses gradient test with `1/rho` |
| Elastic 2D | Eigenstress `S`, four row-major tensor components in Pa | Displacement equation, symmetric-gradient test; the 2D integral is per unit thickness |
| Elastic 3D | Eigenstress `S`, nine row-major tensor components in Pa | Displacement equation, symmetric-gradient test |
| EM2D | Complementary in-plane Maxwell source; Ez uses V/m² components and Hz uses A/m² components | Companion in-plane magnetic or electric equation; harmonic route uses the declared rotated-gradient operation |
| EM3D | Vector source in Faraday's curl equation, V/m² | Magnetic-field equation, value test; harmonic electric-field route uses curl test |

The acoustic source is distinct from scalar volume injection `Q`. In the
first-order acoustic system, `f` enters momentum while `Q` enters continuity.
For elasticity, `S` is subtracted in the constitutive stress relation, so its
weak load pairs with the symmetric displacement gradient. For thermal
conduction, `q_s` is an additive constitutive heat flux. These interpretations
are data in the scenario contracts; no scenario ID is used by the source
assembler.

The transient source collection uses route objects rather than a single source
declaration when a scenario has multiple channels. Each route names its field,
`VolumeLoadPlan` test operator, expression component shape and coefficient.
This lets the same sparse source assembly operation handle scalar, vector,
tensor-gradient and curl loads across 2D and 3D without scenario-specific source
branches.

The equations describe the enabled scenario contracts. The JSON and shared
code define runtime behavior; verification claims below are limited to the
named native tests. When a contract changes, update these equations, units and
availability statements together with the scenario notes and tests.

### Boundary routing

`boundaryLaws` is keyed by the project physics feature type. Each entry
names the target state field, the time-domain input contract and the generic
operation. A `nodalValue` entry also names an existing essential-form
declaration; its value is sampled at each selected node and current physical
time. A `nodalTrace` entry selects an existing boundary form through the input
contract and is integrated over selected exterior facets. Linear edges use a
two-point rule and triangular faces a three-point rule, with the outward normal
computed from the adjacent simplex. The same matrix/load scatter is used for
all declared fields. Conflicting prescribed values and missing, duplicate or
non-exterior selected facets are errors.

The enabled declarations cover prescribed scalar values for acoustic,
thermal and 2D electromagnetic fields, acoustic normal velocity and impedance,
thermal scalar/vector heat flux, and elastic displacement, traction and
impedance. They are real time-domain quantities, sampled in space and
time; thermal outward flux contributes with a minus sign and elastic traction
loads the momentum equation with a plus sign. An unlisted boundary type is
rejected by this adapter rather than interpreted using its harmonic contract.
For elastic prescribed displacement `g`, the scenario's `rateField` routes
the same constrained components to velocity. Backward Euler applies

```math
\mathbf u_n=\mathbf g(t_n),\qquad
\mathbf v_n=\frac{\mathbf g(t_n)-\mathbf g(t_{n-1})}{t_n-t_{n-1}}.
```

This is the integrator-consistent rate, not generally the exact instantaneous
derivative at `t_n`. The source and target field DOFs are both transformed
through any fixed weighted identification. Initial displacement is checked
against the initial boundary value. The staged runner checks initial velocity
against an exact symbolic time derivative for constant traces and supported
smooth arithmetic and functions, without using a finite time step. It also
handles the differentiable branches of `abs`, `min`, `max`, `sign`, `floor`
and `ceil`. A trace at a nondifferentiable point, or whose symbolic derivative
cannot be evaluated at the initial point, is rejected with an initial-rate
diagnostic instead of silently assuming a rate.

The acoustic instantaneous impedance law uses $v_n=(p-2p_{\rm inc})/Z$,
with real nonzero $Z$ in Pa·s/m. It adds $\int q p/Z$ to the pressure block
and $\int 2q p_{\rm inc}/Z$ to its load. The elastic impedance law uses
$\boldsymbol\sigma\mathbf n=-Z\mathbf v$ and adds
$\int Z\mathbf w\cdot\mathbf v$ to the momentum/velocity block. These are
instantaneous local laws; a frequency-dependent or complex harmonic
impedance does not automatically supply a causal time-domain boundary.

The 2D electromagnetic adapter integrates the scalar-field curl equation by
parts. With $\mathbf t=(-n_y,n_x)$, its vector Neumann input prescribes the
complementary tangential field: $H_t=\mathbf t\cdot\mathbf H$ in the `Ez`
formulation and $E_t=\mathbf t\cdot\mathbf E$ in `Hz`. Their boundary loads
are respectively $+\int q H_t$ and $-\int q E_t$. An unassigned natural
boundary gives the corresponding zero tangential field. The legacy scalar
Neumann input has different units and may currently only be zero; a nonzero
value is rejected until its time-domain relation and initial data are
implemented. For a real nonzero instantaneous impedance $Z$, the `Ez` form
adds $\int qE_z/Z$ and has incident load $\int 2qE_{z,\rm inc}/Z$; the `Hz`
form adds $\int qZH_z$ and has incident load $\int 2qZH_{z,\rm inc}$.

The 3D edge adapter prescribes tangential electric-field moments for Dirichlet
boundaries and uses $\mathbf n\times\mathbf H$ as the vector Neumann load in
the electric equation. Its instantaneous impedance law adds the tangential
electric mass term weighted by $1/Z$ and the tangential incident-field load
weighted by $2/Z$. The legacy scalar Neumann input may only be zero until its
time-domain relation is implemented. The edge adapter checks the initial
field's prescribed moments before its constrained projection. These declared
laws route through the native Study coordinator; visible editor and host
behavior remain separate verification items.

### Acoustic 2D and 3D

`acoustic2d.json` and `acoustic3d.json`, formulation `pressure`, scalar pressure `p` in Pa and velocity `v` in m/s with two or three components. Write `b = 1/(ρc²)`, where `ρ` is density and `c` is sound speed. Their declared lossless first-order equations are

```math
\frac{\partial(bp)}{\partial t}+\nabla\!\cdot\mathbf v=Q,
\qquad
\frac{\partial(\rho\mathbf v)}{\partial t}+\nabla p=0.
```

For scalar pressure test `q` and vector velocity test `w`, integrating the
continuity equation by parts yields the weak terms

```math
\int_\Omega q\,\partial_t(bp)\,d\Omega-
\int_\Omega\nabla q\!\cdot\mathbf v\,d\Omega
=\int_\Omega qQ\,d\Omega-\int_{\Gamma_v}q\,g_n\,d\Gamma,
\qquad
\int_\Omega\mathbf w\!\cdot\partial_t(\rho\mathbf v)\,d\Omega+
\int_\Omega\mathbf w\!\cdot\nabla p\,d\Omega=0.
```

Here `g_n` is the prescribed outward normal velocity; an unassigned natural
boundary has zero normal velocity. A vector input is projected onto the
outward normal. On a pressure-Dirichlet boundary the corresponding test trace
vanishes. The shared assembler evaluates this real, position/time-dependent normal
velocity as a boundary load. Expanding the fields in their declared bases,
with `D` assembled from minus the test gradient and `G` from the trial
gradient, gives

```math
\frac{d}{dt}\!\left[
\begin{pmatrix}M_b&0\\0&M_\rho\end{pmatrix}
\begin{pmatrix}\mathbf p\\\mathbf v\end{pmatrix}\right]
+\begin{pmatrix}0&D\\G&0\end{pmatrix}
\begin{pmatrix}\mathbf p\\\mathbf v\end{pmatrix}
=\begin{pmatrix}\mathbf f_Q+\mathbf f_\Gamma\\0\end{pmatrix}.
```

These are prescribed-storage laws: changes in `b` or `ρ` contribute through the product derivatives. `Q` is the scalar pressure-equation Domain Source in Hz (s⁻¹), with coefficient `+1`. It is not a pressure value. The additional acoustic Domain Source is a force density `f` in N/m³ routed to the velocity momentum equation. The 2D contract has two velocity components and the current z-invariant reduction. Nonzero harmonic `lossFactor` needs a separately declared causal time-domain law and is unsupported.

### Elastic 2D and 3D

`elastic2d.json` declares `planeStress` and `planeStrain`; `elastic3d.json` declares `displacement`. State fields are displacement `u` in m and velocity `v` in m/s, each with the declared dimension. With symmetric strain `ε(u) = (∇u + ∇uᵀ)/2`, instantaneous isotropic stress and body force density `f`, the implemented contract is

```math
\dot{\mathbf u}-\mathbf v=0,\qquad
\frac{\partial(\rho\mathbf v)}{\partial t}-\nabla\!\cdot\boldsymbol\sigma=\mathbf f,
\qquad \boldsymbol\sigma=\lambda\operatorname{tr}(\boldsymbol\varepsilon)I+2\mu\boldsymbol\varepsilon.
```

For displacement test `a` and momentum test `w`, the declared volume weak terms are

```math
\int_\Omega\mathbf a\!\cdot(\dot{\mathbf u}-\mathbf v)\,d\Omega=0,
\qquad
\int_\Omega\mathbf w\!\cdot\partial_t(\rho\mathbf v)\,d\Omega+
\int_\Omega\boldsymbol\varepsilon(\mathbf w):\boldsymbol\sigma(\mathbf u)\,d\Omega
=\int_\Omega\mathbf w\!\cdot\mathbf f\,d\Omega+\ell_\Gamma(\mathbf w).
```

Here `ℓ_Γ` denotes the declared transient boundary contributions. Expanding the state fields gives

```math
\begin{pmatrix}M_0&0\\0&0\end{pmatrix}\dot{\mathbf u}_{\!s}
+\frac{d}{dt}\!\left[
\begin{pmatrix}0&0\\0&M_\rho\end{pmatrix}\mathbf u_{\!s}\right]
+\begin{pmatrix}0&-M_0\\K_\sigma&0\end{pmatrix}\mathbf u_{\!s}
=\begin{pmatrix}0\\\mathbf f\end{pmatrix},
\qquad \mathbf u_{\!s}=\begin{pmatrix}\mathbf u\\\mathbf v\end{pmatrix}.
```

For plane strain and 3D, `λ = Eν/[(1+ν)(1−2ν)]`; for plane stress, the in-plane coefficient is `λ = Eν/(1−ν²)`. All use `μ = E/[2(1+ν)]`. The Domain Source is a two- or three-component body force density in N/m³, coefficient `+1` in the momentum equation. The additional source is an eigenstress in Pa, paired with the symmetric displacement gradient. The stress recovery uses the same declared time-stage stiffness and the existing centroid/arithmetic-mean policy. Nonzero harmonic density loss needs an explicit causal transient law.

### Electromagnetic 2D and 3D

`em3d.json`, formulation `electric`, declares electric field `E` in V/m and magnetic field `H` in A/m, each with three components. For scalar instantaneous coefficients, electric Domain Source `J_s` and magnetic Domain Source `M_s`, it declares

```math
\frac{\partial(\epsilon\mathbf E)}{\partial t}+\sigma\mathbf E
-\nabla\!\times\mathbf H=-\mathbf J_s,\qquad
\frac{\partial(\mu\mathbf H)}{\partial t}+\nabla\!\times\mathbf E=-\mathbf M_s.
```

For electric and magnetic tests `F` and `G`, integrating the first curl by
parts gives the 3D weak terms

```math
\int_\Omega\mathbf F\!\cdot\partial_t(\epsilon\mathbf E)\,d\Omega+
\int_\Omega\mathbf F\!\cdot\sigma\mathbf E\,d\Omega-
\int_\Omega(\nabla\!\times\mathbf F)\!\cdot\mathbf H\,d\Omega
=-\int_\Omega\mathbf F\!\cdot\mathbf J_s\,d\Omega+
\int_{\Gamma_H}\mathbf F\!\cdot(\mathbf n\times\mathbf H)\,d\Gamma,
```

```math
\int_\Omega\mathbf G\!\cdot\partial_t(\mu\mathbf H)\,d\Omega+
\int_\Omega\mathbf G\!\cdot(\nabla\!\times\mathbf E)\,d\Omega=
-\int_\Omega\mathbf G\!\cdot\mathbf M_s\,d\Omega.
```

Expanding the fields with the scenario-declared discrete curl blocks `C_E` and `C_H` gives, for the prescribed magnetic trace,

```math
\frac{d}{dt}\!\left[
\begin{pmatrix}M_\epsilon&0\\0&M_\mu\end{pmatrix}
\begin{pmatrix}\mathbf E\\\mathbf H\end{pmatrix}\right]
+\begin{pmatrix}M_\sigma&-C_H\\C_E&0\end{pmatrix}
\begin{pmatrix}\mathbf E\\\mathbf H\end{pmatrix}
=\begin{pmatrix}-\mathbf j_s+\mathbf f_\Gamma\\-\mathbf m_s\end{pmatrix}.
```

The electric Domain Source is a three-component current density in A/m². The Additional Domain Source is a three-component magnetic current density in V/m² and loads the H equation with coefficient `−1`. They are separate additive PHYSICS entries, can use different domain selections, and overlapping entries of the same kind add. With no active entry of either kind its right-hand-side block is zero.

Here `j_s` and `m_s` are the assembled edge-test load vectors for those two expressions; the sparse state layout and matrix blocks are otherwise unchanged.

`em2d.json` has two separate z-invariant polarization formulations. `Ez` uses scalar `E_z` and two-component `H_t`, with scalar electric current density `J_z` in A/m². `Hz` uses scalar `H_z` and two-component `E_t`, with scalar magnetic current density `M_z` in V/m² in the magnetic equation. The additional Domain Source supplies the complementary in-plane source: magnetic in-plane components for `Ez`, electric in-plane components for `Hz`. Both transient source routes use coefficient `−1`. The JSON supplies the signed 2D curl tensors; the scalar-field harmonic equation uses its rotated test gradient, with the tangential boundary terms described above. Coupling the polarizations at nonzero longitudinal wavenumber is a separate planned capability, not part of these formulations.

Explicitly, using `∂z = 0`, the `Ez` volume equations are

```math
\partial_t(\epsilon E_z)+\sigma E_z-\partial_xH_y+\partial_yH_x=-J_z,\qquad
\partial_t(\mu H_x)+\partial_yE_z=0,\qquad
\partial_t(\mu H_y)-\partial_xE_z=0.
```

The `Hz` volume equations are

```math
\partial_t(\mu H_z)+\partial_xE_y-\partial_yE_x=-M_z,\qquad
\partial_t(\epsilon E_x)+\sigma E_x-\partial_yH_z=0,\qquad
\partial_t(\epsilon E_y)+\sigma E_y+\partial_xH_z=0.
```

Their discrete block equations have the same product-derivative structure as
the 3D equation. In field order `(E_z, H_t)` for `Ez` and `(H_z, E_t)` for
`Hz`, respectively, they are

```math
\frac{d}{dt}\!\left[\begin{pmatrix}M_\epsilon&0\\0&M_\mu\end{pmatrix}
\begin{pmatrix}\mathbf E_z\\\mathbf H_t\end{pmatrix}\right]
+\begin{pmatrix}M_\sigma&C_{EH}\\C_{HE}&0\end{pmatrix}
\begin{pmatrix}\mathbf E_z\\\mathbf H_t\end{pmatrix}
=\begin{pmatrix}-\mathbf j_z\\0\end{pmatrix},
```

```math
\frac{d}{dt}\!\left[\begin{pmatrix}M_\mu&0\\0&M_\epsilon\end{pmatrix}
\begin{pmatrix}\mathbf H_z\\\mathbf E_t\end{pmatrix}\right]
+\begin{pmatrix}0&C_{HE}\\C_{EH}&M_\sigma\end{pmatrix}
\begin{pmatrix}\mathbf H_z\\\mathbf E_t\end{pmatrix}
=\begin{pmatrix}-\mathbf m_z\\0\end{pmatrix}.
```

Here each `C` is assembled from the signed differential operator in the
corresponding component equations; the symbol does not assert that the two
cross blocks are identical or adjoints under every discrete boundary contract.

The forms use `ε = ε₀ ε_r`, `μ = μ₀ μ_r`, instantaneous conductivity `σ` and product derivatives for `εE` and `μH`. Frequency-dependent permittivity or permeability cannot be turned into a time-domain expression by inserting a frequency. A causal constitutive model and any needed auxiliary states must first be declared and implemented. General tensor or nonreciprocal materials are not claimed by these scalar-coefficient contracts.

### Thermal 2D and 3D

`thermal2d.json` and `thermal3d.json`, formulation `T`, declare scalar temperature `T` in K, scalar conductivity `k` and positive volumetric heat capacity `C`. The chosen law is

```math
C(\mathbf x,t)\frac{\partial T}{\partial t}
-\nabla\!\cdot\!\left(k(\mathbf x,t)\nabla T\right)=Q.
```

The capacity multiplies the temperature rate: the contract does **not** use `∂(CT)/∂t`.

For a scalar temperature test `q` and outward heat flux `q_n`, the intended weak form is

```math
\int_\Omega q C\,\partial_tT\,d\Omega+
\int_\Omega k\nabla q\!\cdot\nabla T\,d\Omega
=\int_\Omega qQ\,d\Omega-\int_{\Gamma_N}q q_n\,d\Gamma.
```

The boundary term is part of the enabled contract. Expanding the temperature field gives

```math
M_C(t)\dot{\mathbf T}+K_k(t)\mathbf T=\mathbf f_Q(t)+\mathbf f_\Gamma(t),\qquad
(M_C)_{ij}=\int_\Omega C(\mathbf x,t)N_iN_j\,d\Omega.
```

Here `f_Γ` is the assembled boundary load, `−integral(N_i q_n)` for prescribed outward flux. The runner binds the declared nodal boundary adapter, including prescribed temperature and flux; native release coordinator cases cover both thermal dimensions and time-varying prescribed data. Live application and older-host behavior remain unverified. `volumetricHeatCapacity`, symbol `Cvol`, is a required transient material expression with unit `W*s/(m^3*K)` (equivalent to J/(m³·K)); no solver default is supplied. The optional material editor entry starts with an empty expression, requiring an explicit value. Conductivity uses W/(m·K), and the scalar Domain Source is volumetric heat generation in W/m³ with coefficient `+1`. The additional Domain Source is an additive heat-flux vector in W/m² and enters through a gradient test. Conductivity and capacity can depend on position and time and are sampled at quadrature points and time stages. The thermal scenario rules require capacity when a transient Study is present and check its expression, units and applicable bounds. The FEM-local compiler and native semantic rules handle the heat-capacity unit; compatibility with the older host's own unit catalog remains unverified.

## Model, result and validation contracts

- `transientForms` declares state fields, terms, derivative law, scalar/tensor coefficients, Domain Source and state-output mappings. `transientSelectionPlan` describes project selection binding. `x-fem-scenario.transientInitialFieldsByFormulation` is generated from those state declarations for the Study settings projection; `sync-transient-initial-field-metadata.mjs --check` verifies they agree. The projected component counts and units provide zero display defaults without writing omitted initial fields to the model. Thermal `materialProperties` and `materialBindings` add Study-specific heat capacity without making it a stationary requirement. Elastic forms declare tensor stress-recovery operations.
- The time settings contract has dimensioned `startTime`, `endTime`, `maxStep`, `maxSteps` and ordered `outputTimes`. Each parameter-sweep case resolves its own time grid and parameter set. All eight packages advertise transient and route preparation through their declared `transientPlan`. The symbol `t` is reserved for physical time. Explicit `initialFields` use declared field names, component counts and units; omitted fields take the declared zero default. Study preparation checks field names, component counts, units, expression compilation, referenced symbols and inferred dimensions per prepared parameter case against the same declared form and sampling scope used by the solver. Numerical evaluation occurs at actual sampling points and stages.
- Initial nodal data are sampled at nodes; oriented edge data use sparse L² projection. An edge field is not a nodal vector. Its exported nodal visualization is an arithmetic average of incident element traces; cell-centroid values remain separately meaningful.
- `stateOutputs` map solved state components to existing scenario field symbols. Zero fields follow each scenario declaration. Elastic stress recovery uses the declared tensor operation and time-stage moduli. The `fullmeta-transient-field/1` snapshot includes the Study ID, sweep-case index, time-sample index, physical time, mesh topology, declared field values and optional time-sampled cell-local material values. The FEM result normalizer supports this format and evaluates supplied pointwise definitions with `t`; frequency symbols are rejected in transient pointwise expressions. FEM-owned case/time controls and result filtering are implemented; visible interactive behavior remains to be verified.
- The transient mesh reader retains selected integration curves separately from boundary-condition facets in both dimensions. Samplers evaluate nodal fields with entity barycentric weights and oriented fields in their actual basis, using scenario-declared point-output symbol mappings. At each requested snapshot, configured global expressions can use physical time, pointwise fields, selected integration operators and material values sampled at that time and location; the snapshot stores their values as `globalVariables`. Tests cover vector components, both oriented orders, a linear second-order field, time-dependent materials and point probes across the scenario formulations. Elastic stress is evaluated from the owning cell and sampled moduli. The coordinator publishes a history CSV (default `transient-history.csv`, or the Study’s safe custom CSV filename) with case index, sample index, time `t` in seconds and real/imaginary global-variable columns. When `output.fieldFiles` is false it evaluates and records those values without constructing or publishing full field snapshots. The result resolver checks saved case/time coordinates and supports sweep filtering. Elastic and thermal packages declare additive transient editor controls for global and pointwise variables, integration operators and parameter sweeps; these activate only when the package advertises transient and the model contains that Study. Visible editor verification remains outstanding.
- The Study fingerprint includes transient time settings, initial fields and referenced parameters, so changing them invalidates earlier results. The existing run identity remains tied to model and Study IDs. A new mesh or changed DOF layout requires new sparse patterns and constraint maps.
- Preparation and runtime reject invalid time grids, missing required transient material values, nonpositive sampled capacity through declared bounds, inconsistent mapped initial values and incompatible duplicate prescribed values. Model semantic validation checks the selected Study's expression scopes and declared input shapes; sampled-value checks run at the actual point and time.
- The internal `BoundTransientCase` bridge binds the selected formulation,
  material and Domain Source selections, initial expressions and prepared
  per-case parameter/time data to either the nodal or oriented-edge sparse
  runner. Its caller must provide boundary assembly, any fixed constraint map
  and a result observer; the bridge does not infer missing boundary physics.
  All enabled formulations have zero-state and nonzero-source coordinator
  coverage. The bridge is a shared numerical layer; the Study coordinator
  supplies boundary assembly, constraint maps and result observation in
  production execution.
- The same bridge can project an observed nodal or oriented-edge state into
  a time snapshot using `stateOutputs`, declared zero fields and optional
  time-sampled material fields. A `zeroOutputs` entry declares an additional
  zero field for this transient reduction; for acoustic 2D, `vz` is zero under
  the current z-invariant assumption. Elastic tensor output uses its
  `tensorRecovery` and the material coefficients at the snapshot time.
  The coordinator writes snapshots and registers verified artifacts in the
  run record.
- The internal file runner accepts `fullmeta-transient-case/1` with
  `studyType`, `solver`, `studyId`, `caseIndex`, `modelFile`, `meshFile`,
  `modelMapFile`, `outputDirectory`, `solverSettings` and one prepared `case`.
  The case carries SI time values, dimensioned resolved parameters and the
  complete parameter rows. Before solving, the runner checks the model's
  parameter rows and resolved values against the case and recomputes the time
  grid from the selected Study. It writes each requested snapshot with a unique
   case/sample filename using create-new semantics. All eight packages declare
   transient preparation. The coordinator stages case meshes and models,
   launches the runner and registers verified time-indexed artifacts. Release
   executable tests use the enabled scenario declarations. Thermal 2D and 3D
   coordinator runs exercise space/time-varying capacity, sources and
   prescribed temperature.
  Thermal, acoustic, EM and elastic boundary quantity rules recognize real
  space and time expressions when a transient Study is present. An acoustic 2D
  time-dependent pressure boundaries in 2D and 3D, both EM 2D polarizations,
  the EM 3D tangential-field boundary (including second-order elements), and
  elastic 2D and 3D prescribed-displacement cases with matching initial
  velocity have passed synthetic native coordinator tests.
  Shared Domain Source validation also uses a real space-time scope when a
  transient Study is present; mixed harmonic/transient wave models retain
  harmonic source and boundary expression scope. Study preparation checks
  the selected Study's Physics expressions and rejects symbols unsupported
  by that Study. A thermal 2D case with time-varying source and capacity has
  passed native preparation and execution.
  Synthetic mixed-study coordinator cases for all 2D and 3D acoustic,
  electromagnetic and elastic formulations retain a frequency-dependent
  material and source for a successful harmonic solve, then verify that
  selecting the transient Study rejects each with its time-domain diagnostic.
  Replacing those expressions with real time-domain inputs permits the
  transient solve. Mixed stationary/transient cases in both thermal dimensions
  also verify that selecting the stationary Study rejects time-dependent
  conductivity or sources, that stationary inputs still solve, and that
  time-dependent inputs solve when the transient Study is selected.
  A four-case thermal sweep also
  checks material changes, changed geometry, mesh reuse and per-case results.
  The same sweep now passes from the long sandbox path after shortening generated
  geometry-cache directory names; each directory records the full hash to detect
  prefix collisions. This does not establish support for arbitrarily long paths.
   Synthetic zero-state and uniform nonzero Domain Source runs have also passed
   the full native coordinator path for every scenario and enabled formulation,
  including both orders of the 3D oriented element. These simple one-step
  examples do not establish full boundary behavior. A separate one-step
  coordinator probe applies two sources to the same domain and checks their
  sum for every scenario and formulation. Synthetic staged-runner and
  native coordinator tests also verify spatially and temporally varying declared
  material coefficients and nonzero sources across every scenario form, including
  the respective product and state-derivative laws. The same native coordinator
   probe can use the FEM-local release bundle via `TRANSIENT_RELEASE=1` and
   consumes the enabled scenario declarations. A separate two- and
  three-dimensional thermal manufactured case with spatially and temporally
  varying capacity demonstrates first-order temporal convergence to
  $T(1)=\ln(2)/2$. The all-form time-varying material case now repeats each of
  the ten declared forms at 4, 8, 16 and 32 steps. It checks analytic
  conserved-product responses for acoustic and electromagnetic forms and
  first-order temporal convergence for both elastic reductions and both
  thermal dimensions. A separate manufactured heat solution now checks spatial
  convergence for both thermal dimensions using 2, 4 and 8 divisions; the
  nodal RMS error decreases on each refinement. The exact field is a product
  of `X(1-X)` factors, with zero boundary temperature and a matching domain
  source. Equivalent structured spatial studies cover acoustic in 2D and 3D,
  elastic plane stress, plane strain and 3D, and electromagnetic Ez/Hz in 2D
  plus the 3D edge formulation. Acoustic, elastic and EM3D cases show decreasing
  nodal RMS error against analytic fields on 2/4/8-division meshes; the two
  EM2D cases show decreasing nodal differences against a 16-division reference
  at the same time step. Focused tests also verify the backward-Euler energy
  identities for a conservative wave system, an elastic oscillator and Neumann
  heat diffusion, and verify integrated thermal boundary flux in both dimensions.
  `scenario_operators_obey_physical_energy_and_heat_balances` additionally
  assembles every declared scenario form and checks acoustic and electromagnetic
  energy identities, elastic kinetic-plus-strain energy, and thermal energy
  decay with insulated heat conservation. Structured and deterministically
  perturbed-interior simplex refinements now cover every physical family.
  Broader mesh-quality coverage and supported-host compatibility remain
  unverified. A separate two-cell manufactured
  test in both thermal dimensions uses discontinuous, differently
  time-dependent heat capacities and corresponding domain-specific sources;
  it checks the uniform exact temperature at every node across the interface.
  Another two-cell staged-runner test covers every scenario formulation and
  both 3D electromagnetic element orders using discontinuous coefficients
  with a shared time dependence and selected sources that yield a uniform
  solution. Neither test verifies arbitrary interface jumps or mesh convergence.
  A staged-runner periodic test covers every formulation and both 3D
  electromagnetic orders. It compares paired values against a run without
  periodic constraints under an asymmetric source, so the check detects
  failure to apply the zero-phase map. The elastic packages do not currently
  advertise periodic boundary conditions; their test exercises the shared
  internal machinery, not an available elastic model feature. Synthetic
  coordinator runs also cover periodic mesh generation and an asymmetric
  source for 2D/3D thermal, acoustic and electromagnetic scenarios. For the
  3D edge formulation, the coordinator probe checks execution and meshing;
  oriented tangential moments are verified by a separate internal test because
  visualization averages need not match pointwise across periodic faces.
  The native coordinator and bundled executable paths are covered; visible
  interactive verification on the older deployed host remains outstanding.
  The shared transient residual tolerance is declared by
  `study.transientResidualTolerance` in the FEM schema.
- When a model contains a transient Study, the acoustic and electromagnetic
  material quantity rules recognize `t` in addition to their existing runtime
  symbols; the thermal conductivity and elastic material rules recognize
  `x`, `y`, `z` and `t`. Shared model validation retains harmonic frequency
  scope for wave and elastic materials. Selecting a transient Study rejects
  frequency and imaginary symbols and checks constant values against its
  declared implemented-value restrictions; point-dependent bounds and values
  are checked at sampling.
  Models containing only the original Study kind retain their original
  validation scope. Selecting a non-transient Study still rejects a material
  expression that directly uses `t`. This is model-level validation of
  expression scope, with runtime sampling checks for declared time-dependent
  material, source and boundary inputs.
- The FEM-local material compiler checks the heat-capacity unit and its
  sampler checks positivity. Scenario semantic rules check that quantity with
  FEM's native and local WASM runtimes. The older host's own unit catalog does
  not recognize `W*s/(m^3*K)`; the complete deployed-host validation route
  remains unverified.

## Verification status and remaining acceptance work

The package contracts are enabled for all eight scenarios, and native release-coordinator checks cover every declared formulation, sources, space/time-dependent materials, prescribed boundaries, periodic maps and output identity. The interrupted boundary-preparation regression and complete Rust/frontend/package checks are recorded in the implementation status. Cross-form temporal refinement is covered. Structured and deterministically perturbed-interior simplex refinements cover thermal and acoustic in both dimensions, both elastic 2D reductions and 3D, both EM2D polarizations, and EM3D edge fields. The scenario-assembled balance test verifies backward-Euler energy identities for every acoustic, elastic and electromagnetic form, plus thermal energy decay and insulated heat conservation in both dimensions; a separate thermal boundary test checks integrated-flux balance. The transient step observer's cancellation unit test passes, but operation-level cancellation/restart while a solver child is active remains unverified. Broader mesh-quality convergence, plus visible interactive editor and older deployed-host compatibility, also remain unverified. These limits delimit what has and has not been verified; they do not mean transient is disabled in the scenario packages.

### Verification snapshot

- The focused boundary-preparation test `transient_boundary_preparation_uses_declared_laws_and_per_case_parameters` passed. The full native Rust suite passed **304 tests**, with **3 ignored**, when run with `FULLMETA_CORE_DLL`, `FULLMETA_MODEL_DLL` and `FULLMETA_CONTRACTS_DLL` pointing at the matching DLLs in the FEM-local bundle. Running against the older default core DLL instead rejects the transient heat-capacity/source units; use the FEM-local bundle when testing this implementation.
- The release bundle in `target/fem-local-bundle/x86_64-pc-windows-gnu/release/` passed three coordinator probes: all declared formulations; space/time-dependent materials with nonzero sources; and time-varying prescribed boundaries. Each probe completed all **10 formulations**. Separate release coordinator probes passed for EM3D order 2 with a source, space/time-varying materials and a time-varying tangential boundary.
- The `declared_time_varying_materials_drive_all_scenario_forms` native test passes at 4, 8, 16 and 32 steps for every declared form. Thermal and elastic errors decrease at first order against analytic solutions; the selected acoustic and EM conserved-product cases reproduce their analytic final values on each grid.
- The `thermal_manufactured_solution_converges_spatially_in_both_dimensions` native test runs 2D and 3D thermal solutions on both structured and perturbed-interior simplex meshes; nodal RMS error decreases with refinement. `scenario_operators_obey_physical_energy_and_heat_balances` verifies scenario-assembled identities for every declared form, including EM3D Nédélec orders 1 and 2. `thermal_flux_trace_conserves_the_time_integrated_heat` checks thermal boundary-flux balance in both dimensions. The perturbation changes only interior mesh vertices and preserves the boundary and simplex connectivity; broader mesh-quality coverage remains outstanding.
- Spatial refinement also covers acoustic 2D/3D, elastic plane stress/plane strain/3D, and electromagnetic Ez/Hz plus 3D edge fields. Test names are `acoustic_manufactured_solution_converges_spatially_in_both_dimensions`, `elastic_manufactured_solution_converges_spatially_in_all_reductions`, `electromagnetic_manufactured_gradient_converges_spatially_in_3d`, and `electromagnetic_manufactured_solutions_converge_spatially_for_both_2d_forms`. The EM3D perturbed-mesh sequence uses 2/4/6 divisions for runtime; structured cases use 2/4/8. The EM2D comparison uses a 16-division reference at the same time step because its manufactured wave case includes backward-Euler time error; it checks 2/4/8-division meshes against that reference for both polarizations on both mesh sets.
- The scenario-assembled balance test uses JSON-declared operators on perturbed 2D and 3D simplex meshes. It checks acoustic and EM backward-Euler energy decrement, elastic kinetic-plus-strain energy for plane stress, plane strain and 3D, and thermal decay plus insulated heat conservation in both dimensions. EM3D is checked at both supported Nédélec orders.
- `npm run build:frontend` passed for 104 JSX sources. Transient UI projection tests passed 5/5, scenario registry tests 4/4, FEM-local WASM tests 3/3, and both transient metadata synchronization checks passed.
- The full FEM Node test suite passed **57/57**, including transient editor, expression-scope, boundary-vector, source, result and scenario-capability checks.
- The tested executable SHA-256 is `A6FD0930414D86B21686D6EFE8E2205E83C7486E9B91A37EC031588F5D367FB2`. This candidate was built and tested in the FEM-local bundle directory; it was not staged into or installed over the user's Store application.
