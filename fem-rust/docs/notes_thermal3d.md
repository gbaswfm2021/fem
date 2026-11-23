# 3D thermal conduction: equations and contracts

The [thermal-3d scenario](../src/scenarios/thermal3d.json) uses a scalar
temperature field `T` in kelvin on a three-dimensional tetrahedral mesh.
The scenario supports stationary and transient Studies. The transient
Study uses the same temperature, capacity, conductivity and source contract
as the 2D case, integrated over the 3D tetrahedral mesh.

The native scenario-balance test checks backward-Euler thermal energy decay
and insulated heat conservation on a 3D simplex mesh. The separate thermal
flux test checks integrated heat balance with prescribed nonzero flux.
Manufactured spatial refinement checks structured and perturbed-interior
simplex meshes.

When a transient Study is present, the scenario enables its declared global-variable, pointwise-variable, integration-operator and parameter-sweep controls. Requested times can produce a checked global history CSV, with field snapshots optional; see [the shared transient contract](transient_study_contract.md) for output and supported limits.

## Stationary conduction

With scalar conductivity `k` in W/(m·K), volumetric Domain Source `Q` in
W/m³, and heat flux `q` in W/m², the stationary model is

```math
\mathbf q=-k\nabla T,\qquad -\nabla\!\cdot(k\nabla T)=Q.
```

The Domain Source is zero when no source feature is enabled. Multiple source
features add where their selected domains overlap. Domain Source and Additional
Domain Source are distinct Physics feature types with independent domain
selections and expression components. Their component meanings and weak-form
routes come from the scenario JSON. Source expressions use spatial coordinates
and model parameters; transient studies additionally provide time `t` in seconds. For test temperature `s`
and prescribed outward heat flux `q_n`, the volume and Neumann weak form is

```math
\int_\Omega k\nabla s\!\cdot\nabla T\,dV
=\int_\Omega sQ\,dV-\int_{\Gamma_N}s q_n\,dS.
```

The primary field uses linear nodal tetrahedral basis functions. The solver
assembles a sparse system; it also recovers cell heat flux for output. The
plotting field catalog declares `T` as primary and does not declare flux
components as selectable derived field variables. The model supplies
conductivity, domain selections, source expressions and boundary data; see
the scenario JSON for the exact input and validation contracts.

Dirichlet conditions prescribe temperature. Neumann conditions prescribe
either an outward scalar normal flux or a Cartesian flux vector, of which the
outward normal component is used; positive outward flux removes heat. An
unassigned exterior face is insulated unless another applicable constraint
acts there. Enabled matched periodic pairs use zero phase and identify
temperature values. The stationary adapter's material and boundary values
use model-parameter scope; the transient Study's space/time sampling below
does not apply to the stationary Study.

## Transient study

The scenario's `transientForms.T` declares the temperature state and
time-dependent equation. Native coordinator tests exercise nonzero sources,
space/time-dependent capacity and prescribed-temperature boundaries. The live
editor and deployed-host workflows still need visible verification.

```math
C(\mathbf x,t)\frac{\partial T}{\partial t}
-\nabla\!\cdot\!\left(k(\mathbf x,t)\nabla T\right)=Q(\mathbf x,t).
```

Here positive volumetric heat capacity `C` multiplies the temperature rate;
this is not `∂(CT)/∂t`. Its unit is `W*s/(m^3*K)`, equivalent to J/(m³·K),
and the model must supply it without an invented default. Conductivity and
capacity are sampled at physical integration points and time stages in the
solver. Arbitrary frequency dispersion is not a time-domain
constitutive law.

With tetrahedral nodal basis functions `N_i`, the sparse system and its
backward-Euler step are

```math
M_C(t)\dot{\mathbf T}+K_k(t)\mathbf T=\mathbf f_Q(t)+\mathbf f_\Gamma(t),
\qquad (M_C)_{ij}=\int_\Omega C(\mathbf x,t)N_iN_j\,dV,
```

```math
\left[\frac{M_C(t_n)}{\Delta t}+K_k(t_n)\right]\mathbf T_n
=\mathbf f_Q(t_n)+\mathbf f_\Gamma(t_n)
+\frac{M_C(t_n)}{\Delta t}\mathbf T_{n-1}.
```

The boundary declarations route prescribed temperature to the
temperature state and scalar or vector heat flux to the thermal load, with
the outward-flux sign in the weak form above. The internal adapter samples
these real expressions in position and time. Internal tests check selected
boundary values and integrated heat balance on a tetrahedron. Initial data
must satisfy essential constraints. Broader mesh/time convergence and visible
editor/deployed-host compatibility remain to be verified.

The [shared transient contract](transient_study_contract.md) documents the
generic block equations, JSON routing, sparse stepping, results and current
limitations for all eight scenarios. The [application guide](../../docs/README.md)
describes the available user-facing capabilities.


## Additional Domain Source

`additionalDomainSource` is a prescribed additive heat-flux vector **q_s** in the Fourier constitutive relation; it is not volumetric heating. It has three components `(q_sx, q_sy, q_sz)` in W/m². The existing `domainSource` remains volumetric heat generation **Q** in W/m³.

~~~text
q = -k * grad(T) + q_s
C * partial_t(T) + div(q) = Q

Additional weak load: integral(grad(test) dot q_s) dV
~~~

The JSON declares the three source components, units, and gradient-tested load used by stationary and transient scalar assembly. The [thermal 3D contract](../src/scenarios/thermal3d.json) supplies this meaning; the shared feature name does not hard-code thermal behavior into the assembler.

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
