# 3D linear elasticity: model and implementation

This note describes the `elastic-3d` displacement scenario and its free-plate example. The initial P1 implementation and example require mesh and frequency convergence checks: a successful Study is not by itself numerical acceptance. The [implementation instruction](implement_elastic3d.md), [Model 2 reference](../../docs/fullmeta-model-2.md), and [solver capability contract](../../specification/contracts/solver-capability-contract.md) give the design and application context.

## 1. Physical model and fields

Consider small displacements and strains in a three-dimensional solid. The primary field is displacement `u = (ux, uy, uz)` in metres. The secondary, derived field is the symmetric Cauchy stress tensor `sigma` in pascals, with six independent components `sigma_xx`, `sigma_yy`, `sigma_zz`, `sigma_xy`, `sigma_yz`, `sigma_xz`. Strain is an intermediate in its definition, not a separately declared field. With body-force density `f` in N/m³,

```math
\boldsymbol\varepsilon(\mathbf u)=\operatorname{sym}(\nabla\mathbf u)
=\tfrac12(\nabla\mathbf u+(\nabla\mathbf u)^T),\qquad
\boldsymbol\sigma=\mathsf C:\boldsymbol\varepsilon(\mathbf u),\qquad
\nabla\cdot\boldsymbol\sigma+\mathbf f=\rho\,\partial_t^2\mathbf u.
```

Here `(div sigma)_i = partial_j sigma_ij`, `rho` is mass density, and `C` is the fourth-order elastic stiffness tensor. The constitutive relation and momentum balance use first-order spatial operators when primary and derived fields are considered together. That field convention does not require stress to be an independent numerical unknown: the proposed displacement method eliminates it in assembly. A mixed stress–displacement method would require its own spaces and stability analysis.

For harmonic amplitudes, the solver uses `exp(+i omega t)`: the real field is `Re(u exp(i omega t))`, `omega = 2 pi frequency`, and

```math
\boldsymbol\sigma=\mathsf C:\operatorname{sym}(\nabla\mathbf u),\qquad
\nabla\cdot\boldsymbol\sigma+\mathbf f=-\rho^*\omega^2\mathbf u.
```

Here `u`, `sigma` and `f` are complex amplitudes. The implemented scenario uses real elastic stiffness and the effective complex density defined below; its example is driven by boundary traction, and Domain Source can prescribe a volume-force density. Lossless resonances may make the harmonic system singular or ill-conditioned. An unrestrained body has six rigid-body modes in 3D, and the static free-body problem has corresponding null modes; positive frequency alone does not exclude other resonances.

## 2. Isotropic material and the Christoffel tensor

The scenario uses isotropic, linear elastic materials. The material inputs are density `rho` (kg/m³), Young's modulus `E` (Pa), and Poisson ratio `nu` (dimensionless). These provide density and the two independent elastic constants. Shear modulus `G`, the Lamé parameters and stiffness are derived:

```math
G=\frac{E}{2(1+\nu)},\qquad
\mu=G,\qquad
\lambda=\frac{E\nu}{(1+\nu)(1-2\nu)},\qquad
C_{ijkl}=\lambda\delta_{ij}\delta_{kl}
+\mu(\delta_{ik}\delta_{jl}+\delta_{il}\delta_{jk}).
```

For a stable ordinary 3D isotropic material, require finite `rho > 0`, `E > 0` and `-1 < nu < 1/2`; these give `G > 0`. Nearly incompressible materials need special numerical care. `G`, `c_L` and `c_T` may be useful derived values, not additional independent inputs in this design. Absorption needs a separate choice of model and parameter.

The stiffness `C` is distinct from the direction-dependent Christoffel (acoustic) tensor. For a unit bulk propagation direction `n`, define

```math
\Gamma_{ik}(\mathbf n)=C_{ijkl}n_jn_l
=\mu\delta_{ik}+(\lambda+\mu)n_i n_k,\qquad
\Gamma(\mathbf n)\mathbf a=\rho c^2\mathbf a.
```

Some references divide `Gamma` by density; the definition here does not. In the lossless isotropic case its longitudinal and two transverse eigenvalues give `c_L = sqrt((lambda + 2 mu)/rho)` and `c_T = sqrt(mu/rho)`. Density is needed for speeds, not to construct the unnormalized `Gamma`. The isotropic inputs cannot determine a general anisotropic stiffness tensor; anisotropy would require additional independent coefficients and an orientation convention. Christoffel bulk speeds are useful checks, but thin-plate guided waves are dispersive and cannot in general be described by those bulk speeds alone. See [wave propagation in anisotropic elastic media](https://sepwww.stanford.edu/data/media/public/docs/sep93/jim1/paper_html/node2.html) and [elastodynamic equations and their simple solutions](https://seis.karlov.mff.cuni.cz/papers.bin/a01vc1.pdf).

The implemented frequency-domain damping model is `rho* = rho (1 - i eta_rho)`, where `eta_rho` is the nonnegative dimensionless `densityLossFactor` material expression (default zero). The negative imaginary density gives a positive imaginary mass contribution to the dynamic stiffness with the stated time convention. This is an illustrative effective mass-proportional loss, not measured aluminium damping or a complete broadband causal time-domain material law. A complex stiffness loss is not implemented.

## 3. Weak form and tetrahedral basis

The element-selection guideline is: choose the primary fields and weak formulation, then select a conforming element family that supports their derivatives and boundary traces. Use the lowest-order member as the minimal starting point, while allowing higher order or another suitable formulation when stability or accuracy requires it. Secondary fields are derived from the primary fields unless the chosen formulation explicitly makes them independent unknowns. This guideline is a design preview, not an automatic proof of numerical stability or accuracy.

For the displacement formulation, the primary field is `u` and the weak form uses its symmetric gradient. Three `H1`-conforming nodal fields give the required continuous displacement trace. Their minimal starting elements on a tetrahedron are three copies of the P1 Lagrange, or lowest-order Whitney 0-form, basis. Stress is derived through the constitutive relation; it is not assigned an independent element family here.

For a test displacement `v` that vanishes on the essential boundary, the proposed harmonic weak equation is

```math
\int_\Omega \boldsymbol\varepsilon(\mathbf v):\mathsf C:
\boldsymbol\varepsilon(\mathbf u)\,dV
-\omega^2\int_\Omega\rho^*\,\mathbf v\cdot\mathbf u\,dV
+i\omega\int_{\Gamma_Z}Z\,\mathbf v\cdot\mathbf u\,dS
=\int_\Omega\mathbf v\cdot\mathbf f\,dV
+\int_{\Gamma_N}\mathbf v\cdot\bar{\mathbf t}\,dS.
```

For complex test functions the displayed test factors are conjugated, as in the usual sesquilinear form. The corresponding system is `(K - omega² M* + i omega B_Z) u = F`. The implementation accepts standard tetrahedral quadrature; centroid-only integration is rejected because it underintegrates the P1 consistent mass matrix.

The standard conforming displacement space is `[H1(Omega)]^3`. On a straight tetrahedron with barycentric coordinates `lambda_a`, the tentative P1 approximation is

```math
\mathbf u_h=\sum_{a=1}^{4}\lambda_a\mathbf u_a,\qquad
\boldsymbol\varepsilon(\mathbf u_h)
=\operatorname{sym}\left(\sum_{a=1}^{4}\mathbf u_a\otimes\nabla\lambda_a\right).
```

The `lambda_a` are Whitney 0-forms, or linear Lagrange functions: there are 12 local complex displacement coefficients. Components are continuous across conforming interfaces; their gradients and derived stress can jump between cells. This vector field uses three nodal scalar bases because the displacement weak form calls for `H1` conformity; a physical vector does not automatically call for an edge basis.

**Primary Whitney representation:** each displacement component is a Whitney 0-form expansion. Componentwise, `d(u_i)` is a 1-form in the compatible Whitney 1-form space; the gradients are used to build the symmetric tensor strain. That observation does not turn the displacement itself into a Whitney 1-form field.

**Derived representation:** stress is computed from the symmetric gradient and the stiffness tensor. For constant material on a straight P1 tetrahedron, its six independent scalar components are cellwise constant (discontinuous P0). Calling these scalar components P0 does not make the physical stress tensor an intrinsic Whitney 3-form or an independently solved Whitney field. This displacement approximation also does not impose an independently `H(div)`-conforming stress space with pointwise continuous normal traction. If stress is averaged to nodes for display, label that recovery and retain or otherwise make clear the underlying cell values: smoothing can hide jumps at material interfaces. The Whitney sequence and material/metric-dependent Hodge maps help organize discretizations, but the field, weak form and trace requirements determine the space choice; see [finite element exterior calculus](https://www-users.cse.umn.edu/~arnold/papers/acta.pdf).

P1 tetrahedra can be too stiff for bending and can lock near incompressibility. A conforming formulation alone does not establish practical accuracy or efficiency. Compare mesh refinement and, if necessary, higher-order or other suitable elements before claiming useful thin-plate performance. A P2 nodal tetrahedron is a possible later choice: it has ten scalar basis functions and 30 local displacement coefficients, and uses a higher-order 0-form space rather than the lowest-order Whitney basis. Solution order and geometry order are separate choices.

## 4. Boundaries and existing contracts

Let `n` be the unit outward normal to the solid and `t = sigma n` its outward-normal traction vector, in Pa (force per unit area). Its three components include both normal and tangential surface forces. The following are the elastic boundary meanings.

| Condition | Equation | Physical meaning |
| --- | --- | --- |
| Dirichlet | `u = u_bar` | Prescribe displacement in metres. `u_bar = 0` fixes all three components on the selected surface. |
| Neumann | `sigma n = t_bar` | Prescribe outward-normal traction in Pa. `t_bar = 0` is a free surface; nonzero traction is a surface load. |
| Impedance | `sigma n + Z v = 0`, `v = i omega u` | Couple traction to boundary velocity using the same scalar specific impedance in each direction. This is a potentially low-reflecting approximation; impedance incident excitation is not implemented. |

### Dirichlet: displacement

The essential condition is `u = u_bar` on the selected face. A zero vector fixes translations at every point of that face and models an ideal clamped solid boundary. A nonzero vector prescribes motion, which may itself drive a harmonic response. The discrete solution imposes the chosen values on the corresponding boundary degrees of freedom; test displacements vanish there. The complete condition constrains all three Cartesian components. Componentwise or normal-only constraints are useful but require a separate explicit contract, including behavior at intersecting boundaries and periodic identifications.

### Neumann: traction and free surfaces

The natural condition is `sigma n = t_bar`. For example, uniform pressure `p > 0` pushing inward on a face has `t_bar = -p n`, while an applied shear traction is tangential to the face. The Neumann contribution to the right-hand side of the weak form is `+ integral(v_test · t_bar) dS`, conjugating the test function for a complex sesquilinear form. The same outward-normal convention must be used regardless of boundary face vertex ordering. In a displacement FEM the natural condition is enforced in the weak sense, not necessarily pointwise by the recovered cell stress on every mesh face.

A **free boundary** is homogeneous Neumann: `sigma n = 0` in all three traction components. It does not mean `u = 0`, and it is mechanically free even when the bulk material has loss. An unassigned exterior face has zero natural traction. Explicit zero Neumann and an unassigned face give the same assembled system. Elastic periodic conditions are not supported by this scenario.

### Impedance: traction coupled to velocity

The implementation uses a scalar specific impedance `Z` times the 3 x 3 identity, rather than an independently specified matrix. Its purpose is to let outgoing waves leave an artificial boundary with as little reflection as this local approximation permits. With the outward-normal traction and harmonic convention used in this note,

```math
\boldsymbol\sigma\mathbf n+Z\mathbf v=\mathbf t_{\rm drive},
\qquad \mathbf v=i\omega\mathbf u.
```

`Z` has units `Pa*s/m`; `Z I` maps the three velocity components to a traction vector and treats normal and tangential velocity equally. The implemented zero-drive boundary accepts finite complex `Z` with `Re(Z) >= 0`, including zero. For positive real `Z`, the traction `-Z v` opposes motion. The harmonic weak-form contribution is `+i omega integral(v_test · Z u) dS` on the left, conjugating the test function for a complex sesquilinear form. No inverse of `Z` is needed for this elastic form.

The boundary source `t_drive` would be an equivalent traction, not automatically the incident displacement. Impedance incident inputs are not implemented and are rejected. Use a Neumann traction for the example's source. A future incident-wave extension must construct its source from compatible incident stress and velocity traces; no universal factor of two is assumed.

For `Z = 0` and `t_drive = 0`, this equation reduces to free traction. The default free boundary needs no impedance entry. Other scenarios retain their own impedance value restrictions.

A passive scalar `Z` is not automatically reflectionless. Normally incident bulk longitudinal and shear waves in an isotropic solid generally have different characteristic impedances `rho c_L` and `rho c_T`; one scalar cannot exactly match both. It also cannot generally match every oblique or dispersive plate mode. Reflection should be quantified for the modes, frequencies and boundary geometry actually used. A normal/tangential matrix or a more general, possibly nonlocal, outgoing operator is a future option rather than part of this initial scalar design. See the [elastic-wave absorbing-boundary discussion](https://specfem.org/komatitsch.free.fr/published_papers/ijnme_99.pdf). Material absorption in the volume and an impedance load on a surface are separate mechanisms.

### Model representation and periodic boundaries

The shared boundary schema has scalar `value` and a scenario-declared flat `fieldValues` array. For elastic Dirichlet, `fieldValues` gives displacement in x/y/z order in `m`; for Neumann it gives traction directly in x/y/z order in `Pa`. The scalar compatibility `value` must be zero. With no array, zero denotes the zero vector. Nonzero scalar values are rejected rather than assigned an arbitrary direction.

Elastic periodic conditions are not implemented or advertised. A future periodic extension would need to define and validate vector mapping, matching faces and phase-weighted constraints under the application's existing wave convention.

## 5. Free-plate example and verification

The [example model](../../examples/elastic_plate_3d/fem_model.json) is a rectangular `200 x 100 x 4 mm` aluminium-like plate with `E = 70 GPa`, `nu = 0.33`, `rho = 2700 kg/m³`, and `densityLossFactor = 0.01`. These are illustrative inputs, not verified material data. A `2500 Pa` harmonic transverse traction is applied over the full side face at `x = 0`; that face has area `0.0004 m²`, giving a `1 N` total applied force. Other exterior faces are free. The Study samples 450–1800 Hz at 15 Hz increments and reports the maximum nodal magnitude of `uz`. Because the full side load is symmetric, some antisymmetric modes are weakly excited or absent.

This is a preliminary, coarse P1 example. An independent [Kirchhoff-Love Rayleigh–Ritz reference script](../../examples/elastic_plate_3d/reference_modes.py) predicts its first excitable thin-plate bending mode at approximately 527 Hz. With a `4 mm` maximum mesh-size setting, a driven response peak appeared near 1095 Hz; a `2.5 mm` setting produced a peak near 1050 Hz on a 50 Hz scan, and a `1.5 mm` setting produced a peak near 640 Hz on a 10 Hz scan. After removing rigid translation and rotation, the coarse peak's top-surface transverse displacement has a 0.99945 complex shape correlation with the first reference bending mode. This supports the mode identification, not the frequency accuracy. The frequencies are not converged and should not be presented as accurate plate eigenfrequencies. The 2D plate reference is itself approximate relative to the 3D solid. Further mesh and sampling studies or a more suitable higher-order element are needed.

Mesh refinement must resolve the thickness, bending deformation and relevant guided wavelengths, not just bulk wavelengths. Vary through-thickness layers and in-plane resolution independently, then refine together; check element quality and convergence of resonance frequencies, complex displacement and appropriately defined stress quantities. A finite traction patch avoids relying on a mesh-dependent point load, though stresses near a sharply bounded load patch can converge slowly.

Before trusting the plate response, verify rigid translation and rotation produce zero strain, an affine displacement field gives the expected constant strain and stress, and a simple traction case has the correct sign and resultant. Compare lossless bulk longitudinal and shear waves with the speeds above, and check harmonic energy balance and positive dissipated power for the selected loss convention. For a selected scalar impedance, measure reflected amplitudes for representative incident longitudinal, shear and plate waves, and check the sign and passivity of its boundary contribution. Compare bending deflections and low-frequency modes with an applicable plate or beam reference, and use mesh convergence or a trusted 3D reference to assess the limits of that comparison. Inspect real and imaginary parts as well as magnitudes, and refine the frequency sampling around resonances.

The scenario provides harmonic solves, not an eigenfrequency Study. Compare peaks and their complex field patterns with a separate free-plate reference, accounting for finite damping and source symmetry. A successful sweep alone does not establish plate bending accuracy.

## Future eigenfrequency study

An eigenfrequency Study is not implemented. With no body force or boundary drive, zero density loss, and frequency-independent elastic properties and density, the momentum equation in section 1 becomes

```math
-\nabla\cdot\boldsymbol\sigma(\mathbf u)=\lambda\rho\mathbf u,
\qquad \boldsymbol\sigma(\mathbf u)=C:\varepsilon(\mathbf u),
\qquad \lambda=\omega^2.
```

The left operator represents elastic stiffness and the right operator multiplies displacement by mass density. With homogeneous fixed boundaries (`u = 0`) and/or free boundaries (`sigma n = 0`), testing by `w` and integrating by parts yields

```math
\int_\Omega\varepsilon(\overline{\mathbf w}):C:\varepsilon(\mathbf u)\,dV
=\lambda\int_\Omega\rho\,\overline{\mathbf w}\cdot\mathbf u\,dV.
```

`w` vanishes on fixed boundaries and the bar denotes complex conjugation. Expanding displacement and test functions in the real vector nodal basis gives

```math
K\mathbf u=\omega^2M\mathbf u,\qquad
K_{ij}=\int_\Omega\varepsilon(\mathbf N_i):C:\varepsilon(\mathbf N_j)\,dV,\qquad
M_{ij}=\int_\Omega\rho\,\mathbf N_i\cdot\mathbf N_j\,dV.
```

Here `N_i` are vector displacement basis functions, `C` is the elastic stiffness tensor, and `M` is the consistent density-weighted mass matrix. Apply homogeneous displacement constraints to both matrices; a free connected body has six rigid-body zero modes. For positive eigenvalues, `f = sqrt(lambda)/(2 pi)`. Loss, impedance or frequency-dependent coefficients require a more general eigenproblem. Mode shapes have arbitrary amplitude and do not by themselves give a driven response.

## Domain Source

The current implementation supports multiple Domain Source entries under PHYSICS, each with an existing domain selection and expressions in spatial coordinates. Overlapping entries add; no entry means zero source. The project model stores selections and expressions, while the scenario contract declares their physical interpretation and weak-form contribution. This implementation is undergoing verification; its availability in a deployed installation depends on compatible FEM and platform runtimes.

The source is a three-component body-force density `f` in N/m^3, integrated over the selected volume. It contributes `integral(test · f) dV` and is distinct from a surface traction.

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.
