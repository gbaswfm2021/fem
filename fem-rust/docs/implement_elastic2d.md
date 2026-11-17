# Elastic2D: implementation instruction

## Status and intent

This document records the implementation instruction. The `elastic-2d` scenario and in-plane examples have since been added. Consult the [Elastic2D note](notes_elastic2d.md) and [plane-stress](../../examples/elastic_plane_stress_2d/README.md) and [plane-strain](../../examples/elastic_plane_strain_2d/README.md) example READMEs for implemented behavior and verification limitations. The requirements below are design targets, not a claim that every suggested extension has been implemented. The [Elastic3D implementation instruction](implement_elastic3d.md) and [Elastic3D note](notes_elastic3d.md) describe related 3D work.

Build a 2D, small-strain, isotropic linear-elastic, frequency-domain displacement scenario on triangles. Support **both plane strain and plane stress**, selectable as distinct formulations. In each, solve for in-plane displacement and derive stress. Provide an illustrative free rectangular body driven by an in-plane harmonic edge traction and observed through a frequency sweep. Include a scalar impedance boundary option, although the free-body example does not use it.

Both formulations use `(ux, uy)` and the same displacement element family. They are different physical out-of-plane assumptions and constitutive reductions, not `Ez`/`Hz`-style choices of different scalar primary fields. A plane-stress sheet describes in-plane motion; it does not reproduce the out-of-plane flexural motion of the Elastic3D plate example. Do not add a plate, shell, generalized-plane-strain, anti-plane, or coupled 3D formulation under either name.

## Repository boundaries and files

Before coding, read the [FEM application instructions](../../AGENTS.md), [FEM Rust instructions](../AGENTS.md), [scenario runner interface](../src/scenarios/README.md), [FEM specification](../../specification/README.md), [Model 2 reference](../../docs/fullmeta-model-2.md), and current scenario contracts and relevant source. Recheck current field, formulation, selection, Study, solver and result interfaces; they may have changed. Derive disposable verification cases from the specifications and this instruction. Do not inspect or copy saved project JSONs to select implementation details, expected values or replay cases.

Expected scenario work:

- Add `src/scenarios/elastic2d.rs` with the documented scenario runner entry point.
- Add `src/scenarios/elastic2d.json`, declaring a versioned `elastic-2d` package with `schemaDocument.x-fem-scenario.solverModule = "elastic2d"`, both formulations and their actual capabilities.
- Register the package in `src/scenarios/registry.json`, preserving other scenarios and their defaults.
- Add a runnable in-plane example under `applications/fem/examples/`; provide a documented way to exercise each formulation and compare their responses.
- Add meaningful verification and update the design note and applicable documentation to report only implemented, tested behavior.

The Rust and JSON files alone do not establish an executable scenario. Use scenario discovery and the existing application-owned interfaces. Do not add a separate binary, hand-maintained registry module, Cargo target, profile-ID branch in shared Rust, or scenario-specific data in the base `fem.schema.json`. If an essential shared capability is missing, identify the smallest cross-scenario change and its owner, reconcile the public-format and specification documentation as needed, and report fixed-dependency blockers with evidence. Do not assume that copying Elastic3D or a scalar 2D solver establishes a supported interface or correct physics.

## Formulation and field contract

The domain is the `x-y` plane. Use `exp(+i omega t)` with positive frequency and `omega = 2*pi*f`. The physical field is the real part of a complex amplitude multiplied by this time factor. The primary unknown in **both** formulations is displacement `u = (ux, uy)` in metres. Use the in-plane symmetric strain and momentum balance

```math
\varepsilon_{ij}=\tfrac12(\partial_j u_i+\partial_i u_j),\qquad
\partial_j\sigma_{ij}+f_i=-\omega^2\rho^*u_i,\qquad i,j\in\{x,y\}.
```

`f` is body-force density in N/m³. It is included to state the continuum model; do not advertise a body-force input unless its contract, preparation and implementation actually support it. Use a displacement formulation, not an independently solved stress or mixed formulation.

Expose the derived in-plane Cauchy stress components `sigma_xx`, `sigma_yy`, and `sigma_xy` in Pa, with explicit symbols and ordering. Also make the out-of-plane result semantics explicit: in plane strain, `sigma_zz` is generally nonzero; in plane stress it is zero while `epsilon_zz` is generally nonzero. Prefer making `sigma_zz` available in both cases and, if the result interface supports a clearly named dimensionless derived scalar, `epsilon_zz` as well. Verify every declared field through the actual result path; do not advertise unresolvable fields. Strain used internally to compute stress need not become an independent primary field.

Declare two distinct formulation values, for example `planeStrain` and `planeStress`, in the scenario contract. Select stable identifiers consistent with the current schema and UI rather than silently reusing `displacement` for both. Provide a clear display name and a documented default. Ensure the selected Physics formulation is propagated to the linked Study, prepared request, solver, field recovery and stored result metadata. Reject an unsupported value or a Physics/Study mismatch; do not silently substitute one formulation for the other. Confirm the actual application composition and dispatch interfaces before finalizing identifiers or any necessary contract extension.

## Material and constitutive reduction

For each active material, specify real density `rho` in kg/m³, Young's modulus `E` in Pa, and dimensionless Poisson ratio `nu`. Use the same parent-material meaning and, where compatible, the same property names as Elastic3D. Require finite `rho > 0`, `E > 0`, and `-1 < nu < 1/2`; clearly diagnose invalid, missing, non-finite or unassigned material values. Derive

```math
\mu=\frac{E}{2(1+\nu)},\qquad
\lambda=\frac{E\nu}{(1+\nu)(1-2\nu)}.
```

For `epsilon_parallel = sym(grad u)`, both in-plane laws take the form

```math
\sigma_{\parallel}=\lambda_{2D}\operatorname{tr}(\varepsilon_{\parallel})I_2
+2\mu\varepsilon_{\parallel}.
```

Implement the two reductions as follows:

| Formulation | Constraint | In-plane coefficient | Out-of-plane recovery |
| --- | --- | --- | --- |
| Plane strain | `epsilon_zz = epsilon_xz = epsilon_yz = 0` | `lambda_2D = lambda` | `sigma_zz = lambda*(epsilon_xx + epsilon_yy)` |
| Plane stress | `sigma_zz = sigma_xz = sigma_yz = 0` | `lambda_2D = 2*lambda*mu/(lambda + 2*mu) = E*nu/(1 - nu^2)` | `epsilon_zz = -nu*(epsilon_xx + epsilon_yy)/(1 - nu)` |

Use `mu` unchanged in both. Plane stress is a thin-body reduction, not plane strain with `sigma_zz` merely hidden: its nonzero thickness strain is essential. In an engineering-strain matrix, define `gamma_xy = 2*epsilon_xy` explicitly and ensure the shear stress is `sigma_xy = mu*gamma_xy`. Tensor-shear and engineering-shear conventions must not be mixed. The [design note](notes_elastic2d.md#2-isotropic-material-and-reduced-constitutive-laws) gives the explicit matrices.

These are reductions of a stable 3D isotropic material, not a general anisotropic law. Nearly incompressible plane strain can suffer locking; check accuracy and report it rather than silently replacing the formulation or relaxing material validation.

### Absorption

Follow the Elastic3D convention with an optional real, dimensionless nonnegative density-loss factor `eta_rho`, default zero:

```math
\rho^*=\rho(1-i\eta_\rho),\qquad \eta_\rho\geq 0.
```

Use compatible material-property naming and expression semantics where supported. A zero factor recovers the lossless operator; a positive factor yields a positive imaginary contribution to `-omega^2 rho*` under `exp(+i omega t)`. Validate finite values and the permitted expression scope. This is an illustrative effective mass-proportional harmonic loss, not a measured material property or complete broadband causal model. Do not claim support for complex stiffness or for a loss model from another scenario.

## Weak form, thickness and triangle elements

Use the standard displacement Galerkin formulation in `[H1(Omega)]^2`. The **lowest compatible element family** is the first-order nodal triangle: two copies of the P1 Lagrange, or lowest-order Whitney 0-form, basis. Three vertices and two components give six local complex displacement coefficients. This is the minimal conforming starting point, not a guarantee of accuracy or a ban on a later higher-order or otherwise stable formulation. Gradients build the symmetric strain; stress is derived and is not an independent Whitney 1-form or 2-form field.

For a positive constant out-of-plane thickness or representative depth `h`, and test displacement `w` vanishing on the essential boundary, the physical weak form is

```math
\begin{aligned}
&h\int_\Omega\varepsilon(\overline{\mathbf w}):\mathsf C_{2D}:\varepsilon(\mathbf u)\,dA
-\omega^2h\int_\Omega\rho^*\overline{\mathbf w}\cdot\mathbf u\,dA\\
&\quad+i\omega h\int_{\Gamma_Z} Z\,\overline{\mathbf w}\cdot\mathbf u\,ds
=h\int_{\Gamma_N}\overline{\mathbf w}\cdot\bar{\mathbf t}\,ds
+h\int_{\Gamma_Z}\overline{\mathbf w}\cdot\mathbf t_{\rm drive}\,ds.
\end{aligned}
```

Add `h integral(conjugate(w) dot f) dA` only if body forcing is actually implemented. Assemble `K - omega^2 M* + i omega B_Z` with the correct complex signs and consistent conjugated-test convention.

Decide and declare the thickness convention explicitly. A constant `h` cancels from the displacement equation when physical volume density and surface traction are used consistently, so a first implementation may solve per unit out-of-plane depth. It must not confuse traction in Pa with line load in N/m or report a per-depth result as a total force. If the example specifies a physical thickness for postprocessing, state whether it is stored in the model or used only for interpretation. A variable-thickness law does not generally cancel: support it only with an explicit contract, correct weighted integration and verification. Validate any represented thickness as finite and positive.

Compute triangle geometry and shape gradients robustly and reject degenerate cells. For constant coefficients, the consistent mass is `integral(N_a N_b) dA = A*(1 + delta_ab)/12`, separately for the two displacement components, with the chosen depth factor. Centroid-only integration underintegrates this term. Use suitable volume and edge quadrature for declared material and boundary expression scopes, or restrict and validate unsupported variation explicitly. Test orientation independence and do not advertise underintegrating options as equivalent to standard integration.

For constant material on a straight P1 triangle, strain and stress are cellwise constant. If output is smoothed or averaged to nodes, identify the recovery method and retain or clearly expose raw cell values where the result interface permits. Stress components can jump at material interfaces; neither nodal averaging nor this displacement formulation makes traction pointwise continuous or stress independently `H(div)`-conforming. Confirm how the 2D result path can represent the required multiple scalar components before extending shared output code.

## Physics boundaries

Let `n` be the outward in-plane unit normal to an exterior edge and `t = sigma_parallel n` be physical traction in Pa. Apply the current generic selection and conflict policies. Edge vertex order must not change physical traction or boundary signs.

1. **Dirichlet:** prescribe the two Cartesian components of displacement in metres. Zero fixes both components along the edge; a nonzero complex vector can drive the response. Declare ordering, allowed expressions, units, legacy scalar compatibility and shared-node conflict behavior. Reject incompatible constraints. Do not claim roller, symmetry or individual-component constraints unless their separate semantics are implemented.
2. **Neumann:** prescribe two Cartesian traction components in Pa directly, with the sign in the weak form. Distinguish these from stress tensor components and from a force per unit edge length. Check the scenario layout, widgets, quantity units, preparation and solver together; a `fieldValues` precedent is not proof of correct Elastic2D semantics. Define the scalar compatibility value and reject unsupported nonzero scalar representations.
3. **Free:** an unassigned nonperiodic exterior edge has homogeneous natural traction. Explicit zero Neumann and an unassigned edge must assemble the same system. A free in-plane edge is distinct from the out-of-plane free broad faces assumed by plane stress.
4. **Scalar impedance:** implement `sigma_parallel n + Z*v = t_drive`, with `v = i omega u`, and scalar `Z` in `Pa*s/m` applied identically to both components. Define and validate allowed complex values, passivity and zero; for the passive zero-drive subset require finite `Re(Z) >= 0`. `Z = 0` with zero drive is free traction. The equivalent line-load impedance for thickness `h` is `h*Z`. This local boundary can reduce reflection but is not universally reflectionless.

Do not invert `Z` unnecessarily or silently import another scenario's impedance restrictions. An incident displacement is not automatically `t_drive`: it must be related to compatible incident stress and velocity traces. If an incident-source representation is not implemented and verified, support the zero-drive subset, reject unsupported enabled incident inputs clearly, and use Neumann traction for the example. Do not assume a universal factor of two.

If periodic elasticity is implemented, follow the existing wave phase convention, validate matching nodes and complex vector constraints, and enforce ordinary/periodic exclusivity. Otherwise, do not advertise it and reject unsupported enabled periodic conditions. The requested example does not require periodicity.

## Scenario integration, Studies and results

Declare the 2D profile, both formulations, displacement and derived field catalogs, material metadata, boundary types, Study preparation, backends and result metadata in the scenario package. Confirm formulation-specific editor choices, validation and Study settings work in the composed schema. The choice of plane strain or plane stress must reach the actual constitutive calculation and stress recovery on every solve and sweep case, not only appear in the UI.

Use the existing compatible frequency-domain Study and parameter/frequency sweep mechanisms where their documented contracts support the scenario. Exercise the prepared request, triangle mesh handoff, scenario dispatch, complex linear solve, result writing and result resolution. Declare only backends, element orders, expression scopes, quadrature modes and outputs actually supported and tested. Do not silently fall back to another solver, formulation, boundary, loss model or unsupported feature.

Expose useful solver diagnostics and, where supported, real, imaginary and magnitude displacement and stress components, with units and selected formulation. Provide an explicitly defined sweep observable such as maximum nodal `|uy|` or a specified point displacement, plus field patterns at selected frequencies. Label any maximum according to exactly what is sampled; it is not automatically a complete description of the response.

An eigenfrequency Study is not established merely by a harmonic sweep. Do not label response peaks as independently computed eigenvalues or claim in-app eigenmode support without a separately implemented and verified capability. Damping, load symmetry and observation position can shift, hide or blend peaks.

## In-plane free-body example

Provide a 2D rectangular body in the `x-y` plane with isotropic material and a small positive density-loss factor. Use a harmonic **in-plane Neumann traction** on one edge or a supported finite patch of that edge; leave the remaining in-plane exterior edges free. No impedance boundary is needed for this resonance example. Explain its physical interpretation, coordinate axes, traction direction, loaded edge length, loss factor, frequency range, sampling and observable.

As illustrative starting values only, the Elastic3D discussion used a `200 x 100 mm` in-plane rectangle, thickness `4 mm`, `rho = 2700 kg/m^3`, `E = 70 GPa` and `nu = 0.33`. These are not measured or user-selected material data. For example, a uniform `2500 Pa` in-plane traction on the full `100 mm` edge corresponds to `1 N` resultant at `4 mm` thickness; a per-unit-depth implementation instead reports `250 N/m`. The amplitude and direction are examples, not accuracy guarantees. Document whichever values are actually used and whether thickness is stored or only used to interpret total force.

Start with a plane-stress sheet and provide a reproducible plane-strain comparison with the same geometry, material, in-plane loading and observation. They represent different physical out-of-plane constraints; they are not two independent in-plane polarizations. Use a source and observable that make the selected comparison visible, and explain any symmetry-induced missing modes. A free 2D body has three rigid-body zero modes; handle static nullspaces and harmonic resonances correctly.

Use a separate independent in-plane reference appropriate to each formulation and the same free boundaries, such as an analytical solution where its assumptions truly apply, a trusted 2D calculation, or a separately computed undamped eigenproblem. Remove the applied traction in the free-body eigenmode reference; do not fix the driven edge. Compare mode shapes as well as peak frequencies, state any reference approximation and uncertainty, and refine mesh and frequency sampling near peaks. This example cannot validate or reproduce the out-of-plane bending frequencies from the Elastic3D plate. If P1 triangles are too stiff for the chosen in-plane bending response or convergence is too expensive, report that result and assess an appropriate higher-order alternative.

## Verification and completion

Derive focused cases from the physics and current acceptance contracts. At minimum:

- **Both constitutive laws:** verify affine displacement and analytical stresses, including engineering shear. For plane strain check `epsilon_zz = 0` and nonzero `sigma_zz` when expected; for plane stress check `sigma_zz = 0` and the recovered thickness strain. Exercise values of `nu` that distinguish the reductions. Check invalid density, modulus, Poisson ratio, loss and missing assignments with useful diagnostics.
- **Element and material:** check triangle geometry, reversed orientation and degeneracy; two rigid translations and one rigid rotation; exact constant-coefficient stiffness and consistent mass; and quadrature for the declared expression scopes. Compare the lossless operator and positive-loss operator, and verify a finite driven response and positive dissipated power under the chosen convention.
- **Boundary behavior:** check the complex Cartesian traction, resultant and edge orientation; displacement constraints and incompatible shared-node values; explicit zero traction versus an unassigned free edge; traction versus line-load units and thickness scaling; and rejection of unsupported inputs. Check periodic exclusions if periodicity is implemented.
- **Impedance:** check assembly sign, zero and passive values. Measure reflected amplitudes in representative normal-incidence longitudinal and in-plane shear cases, using the appropriate wave speed for each selected constitutive law. A scalar matched to one mode need not match the other; do not infer perfect absorption for oblique waves or modes from a normal-incidence check.
- **Integration:** verify scenario discovery and schema composition; both formulation selections and mismatch rejection; material and boundary expression evaluation; Study preparation, generated mesh, dispatch and solve; field recovery and result resolution, units and every declared component. Exercise both formulations through the normal Model 2 and Study path; helper-level tests alone are insufficient.
- **Example and convergence:** run the example, compare the driven peaks and complex displacement patterns to a clearly identified independent in-plane reference for the same free geometry and material assumptions, and record mesh and frequency-sampling convergence for both reductions. State any unverified limitations rather than calling a large peak an eigenmode by itself.

Run the relevant current FEM tests and build checks, record what actually ran, failures and unresolved limitations, and update [notes_elastic2d.md](notes_elastic2d.md) only after corresponding behavior is implemented and verified. Preserve the distinction between this illustrative harmonic response and future eigenfrequency, anisotropic, variable-thickness, higher-order, mixed, transient, plate/shell or nonlocal-impedance work.
