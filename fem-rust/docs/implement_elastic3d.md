# Elastic3D: implementation instruction

## Status and intent

This document records the implementation instruction. The `elastic-3d` scenario and its free-plate example have since been added; consult the [elastic3d design note](notes_elastic3d.md) and [example README](../../examples/elastic_plate_3d/README.md) for the implemented subset, verification results, and outstanding accuracy limitations. The requirements below are design targets, not a claim that every suggested verification or extension has been completed.

Build a 3D, small-strain, isotropic linear-elastic frequency-domain scenario on tetrahedra. Solve for displacement and derive stress. Provide a thin, free plate driven by a harmonic traction on one side face; sweep frequency and display the response and field patterns. Include a scalar impedance boundary option, although the free-plate example does not use one.

Follow the current user request and applicable repository instructions. Do not treat example values below as user-selected material data or a guaranteed mesh/frequency specification.

## Repository boundaries and files

Before coding, read [FEM application instructions](../../AGENTS.md), [FEM Rust instructions](../AGENTS.md), the [scenario runner interface](../src/scenarios/README.md), the [FEM specification](../../specification/README.md), and the current [Model 2 reference](../../docs/fullmeta-model-2.md). Recheck current contracts and source; they may change while this design is being discussed. Follow the instruction not to inspect or copy saved project JSONs to select implementation details or acceptance expectations; derive disposable cases from the specifications and this instruction.

Expected scenario work:

- Add `src/scenarios/elastic3d.rs`, exporting the documented scenario runner entry point.
- Add `src/scenarios/elastic3d.json`, declaring a versioned `elastic-3d` package and `schemaDocument.x-fem-scenario.solverModule = "elastic3d"`.
- Register the package in `src/scenarios/registry.json`; preserve the other entries and their defaults.
- Add the requested plate example under `applications/fem/examples/` using an appropriate new example directory and the public model format.
- Add focused verification and update documentation to describe only behavior actually implemented and tested.

The Rust and JSON files alone are not sufficient: registry discovery and an example artifact are also needed. Do not add a separate binary, hand-maintained registry module, Cargo target, profile-ID switch in shared Rust, or scenario-specific content in the base `fem.schema.json`. Use existing shared interfaces. If a genuinely shared semantic, storage or output capability is missing, first identify its smallest required change and applicable ownership; do not assume either that the two scenario files suffice or that shared edits are automatically needed. Reconcile applicable public-format/specification documentation with any authorized contract extension and report any fixed-dependency blockers with evidence.

## Physics and field contract

Use `exp(+i omega t)` and positive frequency, with `omega = 2*pi*f`. Complex amplitudes represent the real part of the amplitude times this time factor. The displacement `u = (ux, uy, uz)` in metres is the only primary unknown. Declare symmetric Cauchy stress as the derived field, with six independently accessible components in pascals: `sigma_xx`, `sigma_yy`, `sigma_zz`, `sigma_xy`, `sigma_yz`, and `sigma_xz`. Choose symbols and ordering explicitly in the scenario contract. Do not declare strain as an additional field merely to construct stress.

The intended spatial relations are

```math
\varepsilon(\mathbf u)=\tfrac12(\nabla\mathbf u+\nabla\mathbf u^T),\qquad
\boldsymbol\sigma=\mathsf C:\varepsilon(\mathbf u),\qquad
\nabla\cdot\boldsymbol\sigma+\mathbf f=-\omega^2\rho^*\mathbf u.
```

`f` denotes body-force density. The example uses a surface traction; do not claim a volume-force input unless one is actually declared, prepared and implemented. Stress is derived, not an independently solved mixed field or intrinsically a Whitney 3-form. Do not describe the first-order spatial field relations as requiring an independent stress solve.

For each active material, specify real density `rho` in `kg/m^3`, Young's modulus `E` in `Pa`, and Poisson ratio `nu` as dimensionless. Derive

```math
\mu=\frac{E}{2(1+\nu)},\qquad
\lambda=\frac{E\nu}{(1+\nu)(1-2\nu)},\qquad
\sigma=\lambda\,\operatorname{tr}(\varepsilon)I+2\mu\varepsilon.
```

Require finite `rho > 0`, `E > 0`, and `-1 < nu < 1/2`. Clearly diagnose invalid or missing material assignments and non-finite evaluated coefficients. Do not silently correct inconsistent or invalid input. Isotropic properties do not define a general anisotropic material. Treat nearly incompressible behavior as an accuracy concern, not as an excuse to silently substitute another formulation.

### Absorption

The agreed first example uses a small imaginary contribution to an **effective density**, not the earlier optional stiffness-loss proposal:

```math
\rho^*=\rho(1-i\eta_\rho),\qquad \eta_\rho\ge 0.
```

Here `eta_rho` is a dimensionless density-loss parameter, separate from the real density. Choose and declare an appropriate model property, unit, default and validation; a real loss-factor property from which the solver constructs `rho*` is a candidate that avoids assuming existing material evaluators accept complex density. Confirm the actual shared material-expression contract before choosing the representation. Do not claim the acoustic scenario's existing loss factor implements elastic damping. A zero loss factor produces the lossless model; a positive value gives mass-proportional frequency-domain damping under this time convention. A positive imaginary density would have the wrong passive sign. The damping is an illustrative effective model, not a claim about measured aluminium loss or a complete broadband time-domain material law.

## Weak form and elements

Use the standard displacement Galerkin formulation on straight tetrahedra, with three nodal P1 Lagrange fields: three copies of the lowest-order Whitney 0-form space. Four vertices and three components give 12 local complex displacement coefficients. The conforming displacement space is `[H1]^3`. The symmetric gradient is built componentwise from the displacement gradients; it does not make stress an independently solved Whitney field.

For test displacement `w` vanishing on the essential boundary, the weak form is

```math
\int_\Omega\varepsilon(\overline{\mathbf w}):\mathsf C:\varepsilon(\mathbf u)\,dV
-\omega^2\int_\Omega \rho^*\,\overline{\mathbf w}\cdot\mathbf u\,dV
+i\omega\int_{\Gamma_Z}Z\,\overline{\mathbf w}\cdot\mathbf u\,dS
=\int_{\Gamma_N}\overline{\mathbf w}\cdot\bar{\mathbf t}\,dS
+\int_{\Gamma_Z}\overline{\mathbf w}\cdot\mathbf t_{\rm drive}\,dS.
```

Add the volume-force integral only if volume forcing is implemented. Assemble `K - omega^2 M*` and any declared impedance term with the correct complex signs. Compute tetrahedral volumes and shape gradients robustly; reject degenerate or invalid cells. For constant coefficients, integrate the consistent mass matrix exactly: `integral N_a N_b dV = V*(1 + delta_ab)/20`. Centroid-only integration underintegrates this term. Use correct quadrature for variable coefficients and face loads, or explicitly restrict and validate supported expressions. Do not claim support for an integration option that compromises the intended operator without documenting and testing it.

For a straight P1 tetrahedron with constant material, recover strain and stress as cellwise constants. Identify any averaging to nodes and preserve a way to inspect cell values if the selected result interface supports it. Do not claim that averaged interface stress is pointwise continuous or that this is an `H(div)`-conforming stress solve. Declare only outputs that the actual result path can resolve; verify six components and units end to end. If the existing shared scalar/vector field-output helper cannot represent six stress components or per-cell results, assess whether scenario-provided scalar component families suffice before proposing a shared extension.

## Physics boundaries

Use the outward unit normal `n`; traction is `sigma n` in Pa. Follow the current generic Physics selection and conflict policies, and enforce normal orientation independent of triangle vertex ordering.

1. **Dirichlet:** prescribe the three Cartesian displacement components in metres. Zero fixes the selected face. Define ordering, units, complex-expression support, scalar compatibility value and behavior at shared or periodically identified degrees of freedom. Reject incompatible constraints. Do not claim component-selective constraints unless explicitly supported.
2. **Neumann:** prescribe the three Cartesian traction components in pascals. Add the traction integral with the sign above. Specify whether the stored vector is traction directly or is reconstructed as a stress tensor's normal trace; the proposed example needs a directly prescribed transverse traction. Existing `fieldValues` support is a precedent, not proof of elastic semantics. Confirm the schema, widgets, preparation and runner agree on the chosen layout and scalar compatibility value.
3. **Free:** impose zero natural traction on each unassigned exterior face not subject to an active periodic or other condition. Explicit zero traction and the unassigned condition must agree. Do not apply free-face logic to an active periodic endpoint.
4. **Scalar impedance:** implement the local proposed relation `sigma n + Z*v = t_drive`, with `v = i*omega*u` and scalar `Z` in `Pa*s/m` applied identically to the three components. The zero-drive, passive model should be verified for appropriate values, including positive real `Z`; define and validate allowed complex values and the handling of zero in the scenario contract. Do not change restrictions in other scenarios. The scalar is a potentially low-reflecting approximation, not a universal no-reflection boundary.

The scalar impedance need not be inverted in this formulation. An incident displacement is not automatically `t_drive`: the source requires compatible incident stress and velocity traces. The free-plate example uses Neumann excitation, so do not silently invent an impedance incident-source conversion or a universal factor of two. If impedance incident inputs cannot be implemented and validated under the selected contract, declare the supported zero-drive subset and reject unsupported enabled inputs with a clear diagnostic rather than ignoring them.

If periodic elasticity is implemented, use the existing wave phase convention, verify matching mesh nodes and complex vector constraints, and enforce ordinary/periodic exclusivity. If it is not implemented in this first pass, do not advertise it; reject unsupported enabled periodic conditions clearly. The plate example does not require periodicity.

## Scenario integration and Study behavior

Declare the 3D profile, displacement formulation, primary and derived field catalogs, material property metadata, supported boundary types, Study preparation/runtime, backend and result metadata in the scenario package. Use registered capabilities and standard scenario discovery. Verify actual behavior through editor/schema composition, model validation, preparation, mesh handoff, dispatch, solve and result resolution; a valid JSON package alone is not evidence of an executable scenario.

Use the existing harmonic frequency Study and parameter/frequency sweep mechanisms where their documented contracts support the scenario. Check how the prepared request carries vector conditions, material properties, loss and sample frequencies. Use an actual compatible complex solver backend and declare only tested settings. Do not silently fall back to a different boundary, solver, field formulation or loss model. Expose useful solver diagnostics and real/imaginary/magnitude displacement and stress values where the result contract supports them.

The intended result is a **driven frequency-response spectrum**, with a repeatable observable such as complex `uz` at a specified point or an explicitly defined displacement measure, plus field patterns at selected frequencies. Avoid using a plot of only one point as evidence that all modes are represented. Track field sampling and stress-recovery semantics.

An eigenfrequency Study is not established by the current documented harmonic Study contract. Do not label response peaks as independently computed eigenvalues or claim in-app eigenmode support unless that capability is separately implemented and verified. Compare with an independent analytical, trusted 3D, or separately computed undamped eigenfrequency/mode-shape reference for the same plate and supports. Frequency-response peaks can shift with damping, overlap, or be missing due to source and observation locations.

## Free-plate example

Model a rectangular plate in the `x-y` plane, with thickness along `z`. Use isotropic material and a small positive density-loss factor. Apply a harmonic **Neumann traction in the `z` direction on one narrow side face**, or a selectable off-centre region of that face if supported, and leave all other exterior faces free. This is a surface traction in Pa; for a uniform load report the loaded area and resulting total force. The forcing is separate from material absorption. Do not add an impedance boundary to this free-plate resonance case.

For a concrete starting case only, the previous discussion suggested dimensions `200 x 100 x 4 mm`, `rho = 2700 kg/m^3`, `E = 70 GPa`, and `nu = 0.33`. These are illustrative, not validated or user-selected physical constants. The exact load amplitude, damping factor, side-face location, sweep limits and frequency spacing remain to be chosen and documented. Use a frequency range and sample spacing justified by preliminary modal estimates and refine the sweep near peaks. Consider an off-centre load and multiple observations to expose more modes. Compare with free-plate eigenmodes: remove the applied Neumann load in the reference, rather than fixing its face. A free 3D body has six zero-frequency rigid-body modes; positive-frequency response can still be ill-conditioned near deforming resonances.

Resolve bending and guided wavelengths as well as the plate thickness. Independently vary the number of through-thickness layers and the in-plane resolution, then refine together. Record element quality, mesh size, solve cost and convergence of frequencies and responses. Do not promise a fixed number of layers or elements per wavelength guarantees accuracy. P1 tetrahedra can over-stiffen bending; if convergence is impractically costly or not achieved, report that limitation and assess a higher-order formulation rather than concealing it.

## Verification and completion

Derive meaningful cases from the stated physics and current acceptance contracts. At minimum:

- **Element and material:** check tetrahedron geometry, positive volume, rigid translations and rotations produce zero strain, affine displacement gives the analytical constant strain/stress, mass integration is consistent, and invalid density/modulus/Poisson values fail with useful diagnostics.
- **Boundary behavior:** check traction resultant, Cartesian directions, complex phase and face orientation; zero explicit traction versus unassigned faces; prescribed displacement and incompatible shared-node values; and rejection of unsupported conditions or representations. Check active periodic exclusions if periodicity is implemented.
- **Loss and impedance:** establish the sign of density damping with a finite driven response and a power/dissipation check; verify zero loss recovers the lossless operator. For scalar `Z`, check the assembled sign and passivity, and measure reflection in representative normal-incidence longitudinal and shear cases. Do not describe a passive boundary as perfectly absorbing without evidence.
- **Integration:** verify scenario discovery, schema composition, material and boundary expression evaluation, Study preparation, generated mesh, solver dispatch and solve, result resolution, units, and all declared displacement/stress components. Exercise the example through the normal model and Study path; helper-level tests alone are insufficient.
- **Numerical example:** compare the sweep's peaks and displacement shapes against a clearly identified independent free-plate reference, and record frequency, mesh and sampling convergence. State the reference's assumptions and uncertainty; do not report a peak as an eigenmode solely because it is large.

Run the relevant tests and build checks defined by current FEM instructions, and record what was actually run, failures and unresolved limitations. Update [notes_elastic3d.md](notes_elastic3d.md) and applicable documentation to remove obsolete design alternatives only after the corresponding behavior is implemented and verified. Preserve the boundary between the illustrative frequency sweep and any future eigenfrequency, anisotropic, higher-order, mixed, transient or nonlocal-impedance work.
