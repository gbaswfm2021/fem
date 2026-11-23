# 2D linear elasticity: plane strain and plane stress

This note describes the `elastic-2d` scenario, including its plane-stress and plane-strain displacement formulations. See the [implementation instruction](implement_elastic2d.md), [scenario contract](../src/scenarios/elastic2d.json), [solver](../src/prepared_vector_model.rs), and [plane-stress](../../examples/elastic_plane_stress_2d/README.md) and [plane-strain](../../examples/elastic_plane_strain_2d/README.md) examples. A successful Study is not by itself a guarantee of accuracy; check mesh and frequency convergence for the model being studied.

## 1. Physical model and fields

The model domain `Omega` is a region in the `x-y` plane. Consider small displacements and strains in an isotropic linear-elastic solid. Both formulations in this note use the in-plane displacement as their **primary** field:

```math
\mathbf u=(u_x,u_y),\qquad [\mathbf u]=\mathrm m.
```

The in-plane Cauchy stresses `sigma_xx`, `sigma_yy` and `sigma_xy = sigma_yx` are **derived** fields, in pascals. The scenario also exposes `sigma_zz`, generally nonzero for plane strain and zero for plane stress. Strain is an intermediate quantity, not an additional independently solved field; `epsilon_zz` is not currently exposed as a result field.

With indices `i,j` in `{x,y}`, the in-plane strain and momentum balance are

```math
\varepsilon_{ij}(\mathbf u)=\tfrac12(\partial_j u_i+\partial_i u_j),
\qquad
\partial_j\sigma_{ij}+f_i=\rho\,\partial_t^2 u_i.
```

Here `f` is a body-force density in N/m³ and `rho` is the 3D mass density in kg/m³. Repeated in-plane indices are summed. Domain Source supplies the in-plane components of `f` on the selected domains.

### Plane strain and plane stress

The two formulations share the same in-plane primary variables and balance equation. They differ in the out-of-plane assumption used to obtain the in-plane stress–strain law:

| Formulation | Out-of-plane assumption | Typical interpretation | Important derived quantity |
| --- | --- | --- | --- |
| **Plane strain** | `epsilon_zz = epsilon_xz = epsilon_yz = 0` | A cross-section of a sufficiently long, approximately z-invariant body away from its ends, or a suitable out-of-plane constraint. | `sigma_zz` can be nonzero. |
| **Plane stress** | `sigma_zz = sigma_xz = sigma_yz = 0` | A thin body with approximately traction-free broad faces, undergoing predominantly in-plane deformation. | `epsilon_zz` can be nonzero. |

Plane strain is often represented by `u_z = 0` and no z-dependence of the in-plane displacement. Plane stress is a reduced thin-body approximation: do **not** additionally impose `epsilon_zz = 0`. Its nonzero thickness strain represents Poisson contraction or expansion, even though `u_z` is not an independently solved 2D field. Both are idealizations whose validity depends on geometry, loading and the part of the body examined. Plane stress is most appropriate when thickness effects and out-of-plane inertia are sufficiently small for the frequencies of interest. [COMSOL's solid mechanics documentation](https://doc.comsol.com/6.4/doc/com.comsol.help.sme/sme_ug_solid.07.002.html?type=ext) describes these assumptions and also distinguishes generalized plane strain, which this scenario does not support.

The selection resembles the `Ez`/`Hz` choice in EM2D in that it selects a 2D formulation before solving. It is not the same kind of polarization split: `Ez` and `Hz` choose different scalar electromagnetic unknowns, whereas plane strain and plane stress here both solve the vector `(ux, uy)` and apply different mechanical out-of-plane assumptions. They are not generally complementary parts that can be added to reconstruct an arbitrary 3D solution.

Neither formulation solves out-of-plane transverse displacement `u_z` or a plate's out-of-plane bending waves. A thin plate may be suitable for plane stress for **in-plane** response, but reproducing the flexural modes of the [3D free-plate example](../../examples/elastic_plate_3d/README.md) requires a 3D solid, an appropriate plate/shell model, or another formulation with out-of-plane motion. A 2D in-plane model can still represent bending of a beam-like region *within* the `x-y` plane.

### Frequency-domain convention

Use the same convention as Elastic3D: the physical displacement is `Re(u exp(+i omega t))`, with `omega = 2 pi f` and positive frequency. Complex amplitudes then satisfy

```math
\nabla\cdot\boldsymbol\sigma+\mathbf f
=-\omega^2\rho^*\mathbf u,
\qquad
\boldsymbol\sigma=\mathsf C_{2D}:\boldsymbol\varepsilon(\mathbf u).
```

`C_2D` is the selected plane-strain or plane-stress in-plane constitutive law. The first-order spatial relations connect displacement, strain, stress and momentum, but do not require stress to be an independent FEM unknown. Lossless harmonic problems can become singular or ill-conditioned at resonances. A free 2D body has three in-plane rigid motions: two translations and one rotation; these are static zero modes, not evidence that every positive-frequency system is invertible.

## 2. Isotropic material and reduced constitutive laws

As in Elastic3D, specify real density `rho` in kg/m³, Young's modulus `E` in Pa and dimensionless Poisson ratio `nu` as the independent inputs. Derive

```math
\mu=G=\frac{E}{2(1+\nu)},\qquad
\lambda=\frac{E\nu}{(1+\nu)(1-2\nu)}.
```

For a stable ordinary **3D isotropic parent material**, require finite `rho > 0`, `E > 0`, and `-1 < nu < 1/2`. The reduced 2D coefficients are derived from that parent material; do not silently treat them as unrelated 2D material constants. These three inputs do not define a general anisotropic material. Nearly incompressible materials, especially in plane strain, can require a different numerical treatment.

Write the engineering shear strain `gamma_xy = 2 epsilon_xy = partial_y u_x + partial_x u_y`. In each formulation the in-plane law can be written

```math
\boldsymbol\sigma_{\parallel}
=\lambda_{2D}\,\operatorname{tr}(\boldsymbol\varepsilon_{\parallel})I_2
+2\mu\boldsymbol\varepsilon_{\parallel},
\qquad
\boldsymbol\varepsilon_{\parallel}=\operatorname{sym}(\nabla\mathbf u).
```

For **plane strain**, use `lambda_2D = lambda`. The out-of-plane strain is zero, but the out-of-plane stress generally is not:

```math
\sigma_{zz}=\lambda(\varepsilon_{xx}+\varepsilon_{yy}).
```

For **plane stress**, impose `sigma_zz = 0` in the 3D isotropic constitutive law and eliminate the resulting thickness strain:

```math
\varepsilon_{zz}
=-\frac{\lambda}{\lambda+2\mu}(\varepsilon_{xx}+\varepsilon_{yy})
=-\frac{\nu}{1-\nu}(\varepsilon_{xx}+\varepsilon_{yy}),
\qquad
\lambda_{2D}=\lambda_{\rm ps}
=\frac{2\lambda\mu}{\lambda+2\mu}
=\frac{E\nu}{1-\nu^2}.
```

The shear modulus `mu` is unchanged. With the explicit ordering `[epsilon_xx, epsilon_yy, gamma_xy]`, the two laws are

```math
\begin{bmatrix}\sigma_{xx}\\\sigma_{yy}\\\sigma_{xy}\end{bmatrix}
=
\begin{cases}
\dfrac{E}{(1+\nu)(1-2\nu)}
\begin{bmatrix}1-\nu&\nu&0\\\nu&1-\nu&0\\0&0&(1-2\nu)/2\end{bmatrix}
\begin{bmatrix}\varepsilon_{xx}\\\varepsilon_{yy}\\\gamma_{xy}\end{bmatrix},
&\text{plane strain},\\[12pt]
\dfrac{E}{1-\nu^2}
\begin{bmatrix}1&\nu&0\\\nu&1&0\\0&0&(1-\nu)/2\end{bmatrix}
\begin{bmatrix}\varepsilon_{xx}\\\varepsilon_{yy}\\\gamma_{xy}\end{bmatrix},
&\text{plane stress}.
\end{cases}
```

The explicit shear convention matters: using tensor shear `epsilon_xy` in a matrix expecting `gamma_xy` creates a factor-of-two error. The elimination above is suitable for this linear isotropic model. Other material models can need a different treatment. For example, [COMSOL's plane-stress formulation](https://doc.comsol.com/6.3/doc/com.comsol.help.sme/sme_ug_theory.06.019.html) introduces extra out-of-plane strain degrees of freedom; deriving an equivalent reduced isotropic law here does not claim its internal implementation is identical.

### Wave speeds and material loss

For a homogeneous, lossless reduced model, the in-plane directional Christoffel matrix for a unit direction `n` is

```math
\Gamma_{2D}(\mathbf n)=\mu I_2+(\lambda_{2D}+\mu)\mathbf n\otimes\mathbf n,
\quad
c_{L,2D}=\sqrt{(\lambda_{2D}+2\mu)/\rho},\quad
c_T=\sqrt{\mu/\rho}.
```

The longitudinal eigenvector is parallel to `n` and the in-plane shear eigenvector is perpendicular to it. Plane strain gives the usual 3D bulk longitudinal speed; the reduced plane-stress longitudinal speed is `sqrt(E/(rho (1-nu^2)))`. This is the long-wavelength in-plane thin-body approximation, not a general dispersive plate-wave speed. Here `Gamma` is not divided by density.

The implemented optional `densityLossFactor` is a dimensionless, nonnegative density-loss factor `eta_rho` (default zero), with

```math
\rho^*=\rho(1-i\eta_\rho),\qquad \eta_\rho\geq 0.
```

Under `exp(+i omega t)` this gives a **positive** imaginary contribution to `-omega^2 rho*`. Zero loss recovers the lossless operator. This is an illustrative effective mass-proportional harmonic loss model, not a claim about measured damping or a complete broadband causal material law.

## 3. Weak form, thickness and triangle basis

Choose the primary fields and weak formulation first, then choose a conforming element family that supports their derivatives and boundary traces. The **lowest compatible element family** is the minimal starting point, not a guarantee of accuracy or a prohibition on higher order or another stable formulation.

Let `h > 0` be a constant out-of-plane thickness or representative depth in metres. For a complex test displacement `w` vanishing on the Dirichlet part of the boundary, the physical harmonic weak equation is

```math
\begin{aligned}
&h\int_\Omega\boldsymbol\varepsilon(\overline{\mathbf w}):
\mathsf C_{2D}:\boldsymbol\varepsilon(\mathbf u)\,dA
-\omega^2 h\int_\Omega\rho^*\overline{\mathbf w}\cdot\mathbf u\,dA\\
&\quad+i\omega h\int_{\Gamma_Z}Z\,\overline{\mathbf w}\cdot\mathbf u\,ds
=h\int_\Omega\overline{\mathbf w}\cdot\mathbf f\,dA
+h\int_{\Gamma_N}\overline{\mathbf w}\cdot\bar{\mathbf t}\,ds.
\end{aligned}
```

The bar denotes complex conjugation. Domain Source supplies the body-force term. For constant `h`, using physical traction and body-force density throughout lets the common `h` cancel from the displacement equation. The solver works per unit out-of-plane depth; it does not store a physical thickness. A chosen thickness still matters for **total** mass, force, energy and power. Variable thickness is not implemented.

The displacement and test spaces are `[H1(Omega)]^2`. On a straight triangle, the solver uses three barycentric linear Lagrange functions `N_a`:

```math
\mathbf u_h=\sum_{a=1}^{3}N_a\mathbf u_a,\qquad
\boldsymbol\varepsilon(\mathbf u_h)
=\operatorname{sym}\left(\sum_{a=1}^{3}\mathbf u_a\otimes\nabla N_a\right).
```

Each scalar `N_a` is a Whitney 0-form. Thus the minimal element is two copies of the triangle P1 nodal basis, with **six local complex displacement coefficients**. Displacement components are continuous across conforming triangle edges. Their componentwise exterior derivatives lie in the compatible Whitney 1-form space and are combined into symmetric strain; a physical displacement vector does not by itself imply edge-element unknowns.

Stress is derived from strain and the material law, not solved as an independent Whitney field. For constant material on a straight P1 triangle, in-plane strain and stress are cellwise constant and can jump across edges. This P0 characterization does not turn the stress tensor into an intrinsic Whitney 2-form, nor does it ensure an independently `H(div)`-conforming traction. Nodal averaging for visualization can hide interface jumps; a result contract should identify the recovery method and preserve or clearly expose cell values where possible. The general compatibility viewpoint is described in [finite element exterior calculus](https://www-users.cse.umn.edu/~arnold/papers/acta.pdf).

For constant coefficients, the scalar consistent triangle mass entries are `integral(N_a N_b) dA = A(1 + delta_ab)/12`; apply them independently to the two components, with `rho* h`. Centroid-only integration underintegrates this P1 mass matrix. Variable coefficients and boundary expressions need suitable quadrature, or clearly stated restrictions. P1 triangles can be overly stiff in bending and can exhibit volumetric locking, particularly for nearly incompressible plane strain. A quadratic triangle has six scalar nodes and 12 displacement coefficients; it is a possible higher-order choice, with its own implementation and validation. Geometry order and solution order are distinct.

## 4. Boundary conditions

Let `n = (nx, ny)` be the unit outward normal to a boundary edge and let `t = sigma_parallel n` be the in-plane traction on the corresponding vertical side surface. Its components are Cartesian forces per **physical area**, in Pa. With a constant thickness `h`, a boundary line load per length is `q = h t` in N/m. Keep this distinction explicit when entering loads or reporting total force.

| Condition | Equation | Meaning |
| --- | --- | --- |
| Dirichlet | `u = u_bar` | Prescribe two displacement components in metres. Zero fixes both in-plane components on the edge. |
| Neumann | `sigma_parallel n = t_bar` | Prescribe two traction components in Pa. Zero is a free in-plane edge. |
| Impedance | `sigma_parallel n + Z v = 0`, `v = i omega u` | Relate traction to velocity with scalar specific impedance `Z`; an approximate outgoing boundary. |

Dirichlet and Neumann accept two `fieldValues` quantities in x/y order; the required companion scalar `value` must be zero. Impedance is zero-drive only. Componentwise constraints and periodic boundaries are not supported, and unsupported impedance incident inputs are rejected.

### Dirichlet: displacement

`u = u_bar` specifies the in-plane motion along an edge. Prescribing `(0,0)` fixes translations there; a nonzero complex vector can drive a harmonic response. The test displacement vanishes on that edge. A full vector constraint fixes both components; a roller or symmetry condition constraining only the normal component is a different condition and needs an explicit representation. Values meeting at a node must be mutually consistent.

### Neumann and free edges

`sigma_parallel n = t_bar` specifies normal and tangential traction together. Positive inward pressure `p` corresponds to `t_bar = -p n`; a tangential component represents shear traction. The load contribution is on the right of the weak form. Edge vertex ordering must not reverse the physical Cartesian load or outward-normal convention. Natural traction is imposed weakly, so cell-recovered stresses need not satisfy it pointwise on every edge.

A **free edge** is homogeneous Neumann: `t_bar = 0`. In an ordinary displacement weak formulation, an unassigned exterior edge is naturally free when no other boundary term is present. A plane-stress body's broad top and bottom faces are also assumed approximately traction-free; that out-of-plane assumption is distinct from whether an in-plane **edge** is free, loaded or fixed.

### Impedance: outgoing-wave approximation

Following Elastic3D, the 2D condition uses scalar `Z` times the `2 x 2` identity:

```math
\boldsymbol\sigma_{\parallel}\mathbf n+Z\mathbf v=\mathbf t_{\rm drive},
\qquad \mathbf v=i\omega\mathbf u.
```

`Z` maps two velocity components in m/s to two traction components in Pa, so `[Z] = Pa*s/m`. Equivalently, the line-load impedance is `h Z` in `N*s/m^2` for constant thickness. A finite scalar with `Re(Z) >= 0` is passive under the stated sign convention: for real positive `Z`, `-Z v` opposes the boundary velocity. The term on the weak-form left is `+i omega h integral(Z conjugate(w)·u) ds`. No inverse is required, and `Z = 0` with zero drive gives a free edge.

The purpose is to reduce reflections at an artificial boundary, but a scalar `Z I_2` does not generally make the boundary reflectionless. At normal incidence in a homogeneous lossless reduced model, the longitudinal and shear characteristic specific impedances are `Z_L = rho c_L,2D` and `Z_T = rho c_T`. For either uncoupled plane-wave polarization, the displacement-amplitude reflection coefficient is `(Z_mode - Z)/(Z_mode + Z)` with the outward convention above. A scalar matched to one generally cannot match both; oblique waves and guided or bounded modes add further limitations. A full direction-dependent matrix or nonlocal outgoing operator is not implemented.

`t_drive` would be an equivalent traction source, not automatically the incident displacement. Its construction depends on the actual incident stress, velocity and sign convention; do not assume a universal factor of two. The implementation supports zero-drive impedance and rejects incident inputs; use Neumann traction for excitation.

## 5. Example and verification

The [plane-stress](../../examples/elastic_plane_stress_2d/README.md) and [plane-strain](../../examples/elastic_plane_strain_2d/README.md) examples model a free rectangle driven by an in-plane edge traction. Each plots the maximum nodal `|uy|` and provides displacement and stress fields. The reference thickness is used only to interpret total force. Source symmetry and observation choice can suppress modes, so one response curve is not a complete modal inventory.

For the same region and material, a separate plane-strain run can show how the reduced stiffness and predicted in-plane response change. That comparison changes the physical out-of-plane constraint; it is not simply a different polarization of the same 3D plate. Use a separate appropriate analytical or numerical reference, remove the excitation for any free-body eigenmode reference, and distinguish driven response peaks from independently calculated eigenfrequencies. Static free-body calculations require constraints or a nullspace-aware method; harmonic forcing and small loss do not establish static solvability.

Before treating an implementation as verified, check:

- **Constitutive reduction:** affine displacement fields give the expected in-plane stresses, zero `epsilon_zz` and the correct `sigma_zz` for plane strain, and zero `sigma_zz` with the correct `epsilon_zz` for plane stress. Check engineering shear factors and invalid material ranges.
- **Element and mass:** rigid translations and rotation produce zero strain, triangle orientation and degeneracy are handled correctly, and the consistent mass and density-loss signs match the formulas above. Verify the lossless limit and positive dissipated power for positive loss.
- **Boundary behavior:** test complex traction direction, resultant and edge orientation; prescribed displacements and conflicts at shared nodes; and equivalence of explicit zero traction and an unassigned free edge. Check total force against `h integral(t) ds` when a physical thickness is used.
- **Wave and impedance checks:** compare complex phase and amplitude with homogeneous longitudinal and in-plane shear plane waves for both reductions. At a planar boundary, measure reflection for matched and mismatched scalar impedances; do not claim perfect absorption for oblique or mixed waves from a normal-incidence test.
- **Application integration:** verify scenario discovery, formulation selection, material and boundary validation, mesh and Study preparation, solve, and result units and recovery. Test features the scenario contract declares.
- **Numerical accuracy:** refine mesh and frequency sampling, compare observed response peaks and mode shapes with a stated reference, and investigate locking or bending error. Record geometry, thickness convention, constraints, loss and reference assumptions.

Plane strain, plane stress, generalized plane strain, a plate/shell model, and full 3D elasticity answer different physical questions. Formulation selection must follow the geometry and expected motion, while the primary field and weak form determine the compatible element family.

## Lossless second-order frequency eigenproblem (reference)

This familiar frequency-domain reduction is retained as an analytical reference, not as the implemented Study contract. The universal eigenfrequency Study below uses the first-order transient state system and solves directly for the complex temporal rate `s`. With no body force or boundary drive, zero density loss, and frequency-independent elastic properties and density, the in-plane momentum equation in section 1 becomes

```math
-\nabla\cdot\boldsymbol\sigma(\mathbf u)=\lambda\rho\mathbf u,
\qquad \boldsymbol\sigma(\mathbf u)=C_{2D}:\varepsilon(\mathbf u),
\qquad \lambda=\omega^2.
```

The left operator represents elastic stiffness and the right operator multiplies displacement by mass density. Use the selected plane-stress or plane-strain law for `C_2D`. With homogeneous fixed boundaries (`u = 0`) and/or free boundaries (`sigma n = 0`), testing by `w` and integrating by parts yields

```math
\int_\Omega\varepsilon(\overline{\mathbf w}):C_{2D}:\varepsilon(\mathbf u)\,dA
=\lambda\int_\Omega\rho\,\overline{\mathbf w}\cdot\mathbf u\,dA.
```

`w` vanishes on fixed boundaries and the bar denotes complex conjugation. Expanding displacement and test functions in the real vector nodal basis gives

```math
K\mathbf u=\omega^2M\mathbf u,\qquad
K_{ij}=\int_\Omega\varepsilon(\mathbf N_i):C_{2D}:\varepsilon(\mathbf N_j)\,dA,\qquad
M_{ij}=\int_\Omega\rho\,\mathbf N_i\cdot\mathbf N_j\,dA.
```

The vector basis functions `N_i` describe in-plane displacement; `M` is the consistent density-weighted mass matrix. These integrals are per unit thickness. Apply homogeneous displacement constraints to both matrices; a free connected body has three rigid-body zero modes. A nonzero eigenvector is a mode shape, and for positive eigenvalues `f = sqrt(lambda)/(2 pi)`. Loss, impedance or frequency-dependent coefficients require a more general eigenproblem.

## Prescribed longitudinal-wavenumber harmonic study and fixed-frequency propagation modes

**Prescribed-`kz` harmonic solving is implemented for the plane-strain parent formulation.** It uses the 3D isotropic constitutive law on a 2D cross-section, with all three primary components `(u_x,u_y,u_z)`. The `planeStress` contract rejects nonzero `kz`; at `kz = 0`, the established two-component plane-strain route remains in use. Finite-`kz` results are cross-sectional amplitudes, with result metadata recording the complex `kz` in `rad/m` and the factor `exp(-i*kz*z)`. Geometry, materials, body forces and boundary drives must be z-invariant and share that dependence.

The generic sparse weak form uses the formal test-side `+i*kz` and trial-side `-i*kz` substitutions. Domain Source remains three-component body-force density, and Additional Domain Source remains the nine row-major components of prescribed eigenstress; the scenario contract routes both through the three-dimensional reduced operator. The native axial plane-wave check uses `omega^2*rho = kz^2*(lambda+2*mu)`, verifies all three displacement components, residual, longitudinal metadata and recovered 3D stress. A second analytic shear-wave check confirms decreasing finite-`kz` displacement error as a structured triangular mesh is refined from 4 to 8 divisions per side. An additional finite-`kz` response check activates body force, spatially varying eigenstress, and natural traction separately; each gives a small residual and changes the solution from the unloaded baseline. In row-major gradient order `(xx,xy,xz,yx,yy,yz,zx,zy,zz)`, the six-component stress matrix in `elastic2d.json` maps to `(sigma_xx,sigma_yy,sigma_zz,sigma_xy,sigma_yz,sigma_xz)`.

### Implemented fixed-frequency propagation-mode Study

The `propagationMode` Study is implemented for the `planeStrain` parent only. At the selected real frequency it uses the 3D constitutive law over the 2D cross-section and three displacement components `(ux,uy,uz)`; it is not classical two-component plane strain. Plane stress remains unavailable for this Study.

The scenario-neutral adapter samples the source-free longitudinal weak operator from the existing sparse assembler and forms $P(k_z)=P_0+k_zP_1+k_z^2P_2$. In particular, the linear $P_1$ contribution is retained. A shared sparse first-companion generalized pencil returns $k_z$ itself as a generally complex eigenvalue. The code does not assume the Hermitian special case or synthesize reciprocal `+/-` modes. It verifies the polynomial at additional sample points and checks each reported mode against the original polynomial residual. Source/drive loads are omitted, while supported homogeneous constraints and operator terms are retained.

Each current mode artifact stores all three displacement amplitudes and six cell-local Cauchy stress amplitudes recovered with the declared 3D longitudinal constitutive law, along with frequency, complex `kz` in `rad/m`, and residual. Each stress component is emitted in Pa and retains cell-local values at material boundaries. The six stress components are `sigma_xx`, `sigma_yy`, `sigma_zz`, `sigma_xy`, `sigma_yz`, and `sigma_xz`. The isotropic plane-strain parent uses `lambda+2*mu` for all three normal-strain diagonal coefficients in the longitudinal tensor recovery. Direction/energy-flux classification is not currently emitted. The search controls are mode count, a complex inverse-length target/shift, Krylov dimension and residual tolerance. The numerical adapter currently accepts first-order triangular elements and only the declared plane-strain-parent operator. Acceptance compares both longitudinal and shear branches with analytic isotropic bulk dispersion, verifies all three displacement components, six recovered stresses, and the original-polynomial residual. See the [fixed-frequency propagation-mode implementation plan](implement_2d_propagation_mode_study.md) for the shared assembly, verification results, limits, and completed cylindrical acoustic UI demonstration.

The prescribed-`kz` harmonic operator is


For the three-component cross-sectional displacement amplitude, define

```math
\nabla_k=\nabla_t-i k_z\mathbf e_z,\qquad \varepsilon_k(\mathbf u)=\tfrac12\bigl(\nabla_k\mathbf u+(\nabla_k\mathbf u)^T\bigr),\qquad \sigma_k=\mathsf C:\varepsilon_k(\mathbf u).
```

Using the 3D constitutive tensor $\mathsf C$, the harmonic momentum equation is

```math
\nabla_k\!\cdot\sigma_k+\omega^2\rho^*\mathbf u=-\mathbf f.
```

For test amplitude $\mathbf v$, the corresponding weak volume equation, with boundary traction terms included in $\ell$, is

```math
\int_\Omega\varepsilon_{-k}(\overline{\mathbf v}):\mathsf C:\varepsilon_k(\mathbf u)\,dA-\omega^2\int_\Omega\rho^*\overline{\mathbf v}\cdot\mathbf u\,dA=\ell(\mathbf v).
```

The $-k_z$ on the conjugated test amplitude is the formal integration-by-parts partner; it does not conjugate the eigenvalue. The fixed-frequency mode study must retain $k_z$ itself as the eigenvalue. A suitable mixed or augmented discretization can target the generalized pencil

```math
A(\omega)\mathbf x=k_z B(\omega)\mathbf x,\qquad \mathbf x\ne0,
```

where the state contains displacement components and any auxiliary variables and constraints needed by the continuous first-order formulation. This is an alternative mathematical route; the current implementation uses the sparse quadratic polynomial and companion described above. It retains homogeneous constraints and checks each mode against the original polynomial. Boundary terms or material laws outside the current declared contract still require an implementation extension.

### Finite-$k_z$ wave equation and lossless Hermitian reference

The following quadratic and augmented Hermitian forms are continuous/discrete mathematical references for the lossless isotropic model. The current propagation Study uses the generic sparse first-companion linearization of the assembled quadratic polynomial, not this scenario-specific Hermitian block pencil. These Hermitian properties therefore are not assumptions of the shared solver.

Using the 3D constitutive tensor, the prescribed-$k_z$ displacement wave equation is

```math
-\nabla_k\!\cdot\bigl(\mathsf C:\varepsilon_k(\mathbf u)\bigr)-\omega^2\rho\mathbf u=\mathbf f,\qquad
\nabla_k=\nabla_t-ik_z\mathbf e_z.
```

For a source-free, lossless material with real positive density and Hermitian elastic tensor, real nonzero $\omega$, and boundary/interface conditions that remove the weak boundary form, define $E\mathbf u=\operatorname{sym}(\mathbf e_z\otimes\mathbf u)$ and $\varepsilon_0=\operatorname{sym}(\nabla_t\mathbf u)$. The quadratic displacement equation can be written with $k_z$ as its eigenvalue as

```math
(K_0+k_zK_1+k_z^2K_2)\mathbf u=0,
```

where the Hermitian operators are defined by their weak forms:

```math
\begin{aligned}
\langle\mathbf v,K_0\mathbf u\rangle&=\int_\Omega\varepsilon_0(\mathbf v)^*:\mathsf C:\varepsilon_0(\mathbf u)-\omega^2\rho\,\mathbf v^*\!\cdot\mathbf u\,dA,\\
\langle\mathbf v,K_1\mathbf u\rangle&=i\int_\Omega(E\mathbf v)^*:\mathsf C:\varepsilon_0(\mathbf u)-\varepsilon_0(\mathbf v)^*:\mathsf C:E\mathbf u\,dA,\\
\langle\mathbf v,K_2\mathbf u\rangle&=\int_\Omega(E\mathbf v)^*:\mathsf C:E\mathbf u\,dA.
\end{aligned}
```

With the augmented state $\mathbf x=(\mathbf u,\mathbf w)^T$ and $\mathbf w=k_z\mathbf u$, a generalized Hermitian pencil having $k_z$ itself as eigenvalue is

```math
A(\omega)=\begin{bmatrix}-K_0&0\\0&K_2\end{bmatrix},\qquad
B(\omega)=\begin{bmatrix}K_1&K_2\\K_2&0\end{bmatrix},\qquad
A\mathbf x=k_zB\mathbf x.
```

The pencil is Hermitian but $B$ need not be positive definite or invertible. For $\langle\mathbf x,B\mathbf x\rangle\ne0$, the Hermitian quotient makes $k_z$ real; the argument does not cover zero-flux or evanescent modes. No direction pairing is imposed by the implemented general study. This Hermitian block pencil is not the code path. The current sparse companion route verifies its recovered modes against the original polynomial and filters non-finite/infinite descriptor roots; its generic solver does not assume Hermiticity. Loss or frequency-dependent complex properties still require a separate assessment of the material and operator contracts.

## Transient study

The [scenario JSON](../src/scenarios/elastic2d.json) advertises transient Studies for plane stress and plane strain. The shared runner binds displacement and velocity, space/time-dependent materials, body forces, initial fields, prescribed displacement, traction and instantaneous impedance boundaries, then samples declared tensor stress outputs. Synthetic native coordinator cases exercise both reductions, nonzero forces and prescribed displacement with compatible initial velocity. Initial rates are checked for supported differentiable traces; unsupported initial derivatives are rejected. Live editor and deployed-host workflows still need visible verification. Nonzero harmonic loss requires a separately declared causal time-domain law.

The native `scenario_operators_obey_physical_energy_and_heat_balances` test checks kinetic-plus-strain energy balance for the assembled plane-stress and plane-strain forms. Spatial refinement covers both reductions on structured and perturbed-interior simplex meshes.

When this Study is present, the scenario enables its declared global-variable, pointwise-variable, integration-operator and parameter-sweep controls. Requested times can produce a checked global history CSV, with field snapshots optional; see [the shared transient contract](transient_study_contract.md) for output and supported limits.

For displacement $\mathbf u$, velocity $\mathbf v$, density $\rho$, body force density $\mathbf f$, and instantaneous isotropic stress $\boldsymbol\sigma(\mathbf u)$, the declared first-order system is

```math
\dot{\mathbf u}-\mathbf v=0,\qquad
\partial_t(\rho\mathbf v)-\nabla\!\cdot\boldsymbol\sigma(\mathbf u)=\mathbf f.
```

The density derivative is a product derivative when density depends on time. With $M_0$ the unweighted nodal mass, $M_\rho$ the density-weighted mass, and $K_\sigma$ the declared elasticity operator, the sparse block equation is

```math
\begin{pmatrix}M_0&0\\0&0\end{pmatrix}
\begin{pmatrix}\dot{\mathbf u}\\\dot{\mathbf v}\end{pmatrix}
+\frac{d}{dt}\!\left[\begin{pmatrix}0&0\\0&M_\rho\end{pmatrix}
\begin{pmatrix}\mathbf u\\\mathbf v\end{pmatrix}\right]
+\begin{pmatrix}0&-M_0\\K_\sigma&0\end{pmatrix}
\begin{pmatrix}\mathbf u\\\mathbf v\end{pmatrix}
=\begin{pmatrix}0\\\mathbf f+\mathbf f_\Gamma\end{pmatrix}.
```

The Domain Source is body-force density in N/m³ and traction enters the momentum load. Its instantaneous impedance law $\boldsymbol\sigma\mathbf n=-Z\mathbf v$ adds $Z$ to the boundary velocity block; complex or frequency-dependent impedance requires a separate causal time-domain model. A prescribed displacement $\mathbf g(t)$ also constrains velocity in each backward-Euler step to $[\mathbf g(t_n)-\mathbf g(t_{n-1})]/\Delta t$. Initial displacement is checked against the prescribed value, and initial velocity is checked against the continuous boundary rate where the expression is differentiable by the declared evaluator. The two 2D formulations retain their plane-stress or plane-strain constitutive choices. Time-domain inputs are real, not harmonic phasors.

The [shared transient reference](transient_study_contract.md) records the weak forms, coefficient laws, boundary and result contracts, sparse time step, and required verification.

## Domain Source

The current implementation supports Domain Source and Additional Domain Source as separate PHYSICS feature types. Each entry has its own domain selection and expression components; multiple entries of either type add, including on overlapping selections, while an absent type contributes zero. Expressions are sampled at volume quadrature points using physical coordinates and model parameters. Where harmonic studies are supported, their evaluations additionally provide `freq`, `frequency`, and `omega`; transient studies additionally provide time `t` in seconds. The scenario JSON declares each type’s physical meaning, units, component order and weak-form route. This implementation is undergoing verification; its availability in a deployed installation depends on compatible FEM and platform runtimes.

The source is an in-plane body-force density in N/m³. It contributes `integral(test · f) dA` per unit out-of-plane thickness, for either plane stress or plane strain.

For example, model the in-plane response of a rectangular plate with a Domain Source whose selection is the plate domain. Define parameters `F0` in N/m³ and `L` in m, then enter `0` for the x component and `F0*x/L` for the y component. The load grows linearly from zero at `x = 0` to `F0` at `x = L`. The corresponding Physics entry in the project model has this shape:

```json
{
  "id": "plate_drive",
  "name": "Plate drive",
  "type": "domainSource",
  "selectionId": "plate_domain",
  "values": [
    { "expression": "0", "unit": "N/m^3" },
    { "expression": "F0*x/L", "unit": "N/m^3" }
  ]
}
```

Here `plate_domain` is the ID of an existing domain selection, and `F0` and `L` must be defined in PARAMETERS. This is a force per unit volume applied throughout that selection. A traction boundary condition instead applies force per unit area on a selected boundary. The in-plane 2D formulation does not model out-of-plane plate bending; use an appropriate model for that motion.

See the [implementation plan](implement_domain_sources.md) for expression evaluation, generic assembly and verification requirements.

## Boundary vector count validation

Enabled Cartesian displacement and traction vectors must each have exactly two components for either formulation. Semantic model validation checks the primary `fieldValues` count from the scenario’s declared boundary input; the solver retains its own check. Enabled vectors with an invalid type or component count receive a semantic validation error. To repair one in the editor, turn off “Specify field components” and turn it back on to recreate the declared defaults; this reset replaces the existing components. Disabled conditions skip these semantic checks.


## Additional Domain Source

`additionalDomainSource` is a prescribed eigenstress **S** in the elastic constitutive equation, not a body force. Plane-stress and plane-strain formulations each accept the four row-major in-plane components `(S_xx, S_xy, S_yx, S_yy)` in Pa. Because strain is symmetric, only the symmetric part of **S** contributes. The existing `domainSource` remains body-force density in N/m³.

~~~text
epsilon(u) = sym(grad(u))
sigma(u) = C : epsilon(u) - S
partial_t(u) = velocity
rho * partial_t(velocity) - div(sigma(u)) = body_force

Additional weak load: integral(sym(grad(test)) : S) dA
~~~

The 2D integral is per unit out-of-plane thickness. The transient solver evolves displacement and velocity in this first-order state form. In that system the eigenstress is a load on the momentum equation, so the JSON routes its symmetric-gradient load to the velocity test field. The JSON declares the tensor component order, units, and symmetric-gradient load route for both plane formulations; the same physical source is used in harmonic and transient studies. The [elastic 2D contract](../src/scenarios/elastic2d.json) supplies the route without an elasticity-specific assembler branch.

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

Current fixed-frequency propagation acceptance passes 12 Rust tests, 10 generic sparse-eigensolver tests, and 65 FEM Node contract/result/UI tests. The propagation tests cover declared JSON preparation and result resolution, all three enabled formulations, acoustic rigid-wall dispersion/cutoff and the pressure-Dirichlet guide, signed direct-`kz` targeting, impedance-operator retention with incident-load removal, parameter-case reassembly, EM2D PEC dispersion/refinement for both polarizations, and elastic longitudinal/shear branches with three-component displacement, six stress outputs, and residual checks. The full native Rust suite reports 333 passed, 27 failed, and 3 ignored; those 27 failures reproduce omissions of `N/m^3`, `W/m^3`, or `W*s/(m^3*K)` in the fixed shared Core quantity service and are unrelated to propagation-mode assertions. The FEM frontend and FEM-local runtime bundle build successfully. The wider FEM verification history is recorded in the [universal Eigenfrequency implementation plan](implement_universal_eigenfrequency_study.md).
