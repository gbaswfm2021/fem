# FullMeta PML implementation handover

**Status:** design handover for discussion and later implementation. This is a
new numerical feature, not a claim that FullMeta currently has PML support.
No PML implementation or physical validation is present in this project yet.

**Target:** one shared, application-neutral PML capability that can be enabled
by scenario contracts and configured by a model, without adding
shape-specific solver code or a new Rust branch for each scenario. The shared
geometry and transform recipe must be mathematically defined before coding;
the acceptance gates below are deliberate because the geometry-to-Jacobian
recipe is new.

## The outer boundary still needs a condition

A PML is a transformed **domain layer**, not a boundary condition that makes
the finite computational domain mathematically infinite. It has three
distinct parts:

1. The physical region, ending at the inner PML interface.
2. The PML volume, where the coefficients/operators are pulled back by a
   complex coordinate map.
3. The outer truncation boundary, where the finite computational problem must
   still be terminated.

At the inner interface, do not add an artificial reflecting boundary
condition. If the physical and PML elements share the interface DOFs and the
weak form is assembled consistently, field continuity and the corresponding
flux continuity follow from the global variational problem. The material and
background operator should continue smoothly into the PML. A discontinuity in
the physical medium there is not something coordinate stretching can repair.

At each **artificial terminal cap** of the active PML directions, the
candidate default should be the formulation's **homogeneous natural
condition**, with no added boundary load or essential constraint. For a
scalar divergence-form problem with background tensor A, one common pullback
gives A_PML = det(J) J^-1 A J^-T, so the natural cap condition is

    n dot (A_PML grad(u)) = 0,

not necessarily the untransformed derivative du/dn = 0. This expression is
illustrative of that scalar weak form, not a universal PML flux formula. For
an electric-field H(curl) curl-curl form, the natural boundary term is a
formulation-specific magnetic-flux trace (in untransformed notation,
n cross (mu^-1 curl(E)) = 0); it is not zero tangential E. The scenario
contract must identify what “natural homogeneous” means for the declared
weak form. Omitting a boundary term is acceptable only when the assembler
actually gives that natural condition and no boundary load or other
condition is attached to the cap.

Homogeneous Dirichlet/essential termination remains a valid alternative where
the scenario declares the primary-field trace. Examples include u = 0 for a
scalar field and zero tangential electric field, n x E = 0 (PEC), for an
electric-field H(curl) formulation. Do not implement “set all DOFs to zero”
as a universal rule: edge, vector, reduced, and longitudinal formulations do
not have the same boundary trace.

This zero condition is **not** applied to every boundary of the enlarged
simulation domain. Boundaries transverse to the active stretch directions
retain their existing physical/model conditions. In particular, for an
x-only PML around a rectangular periodic cell, the left/right generated
terminal caps receive the declared natural homogeneous termination by
default, while the top/bottom periodic pair continues across the physical
region and the two x-directed
PML stripes. The generated mesh and periodic mapping must therefore extend
that pair through the PML; periodic constraints must not be replaced by zero
conditions just because those faces lie on the outer envelope of the enlarged
mesh. In 3D, apply the same rule to periodic side faces and their cap-edge
intersections.

This termination is an approximation at finite PML thickness. A homogeneous
natural or essential condition can reflect at the cap; the PML attenuates the
outgoing field before that reflection returns to the physical region, so the
reflected contribution should decrease as layer/profile/mesh are refined.
Neither condition is exactly nonreflecting at finite thickness. A
physics-specific impedance/Robin or exact DtN termination can be added later,
but should not be silently combined with PML or assumed equivalent. Natural
Neumann is a practical default because a weak form may already impose its
homogeneous conormal condition when no cap load/constraint is added, but that
convenience is not evidence that its reflection error is small.

The model contract and UI must therefore keep the **terminal-cap policy** as
an explicit PML setting or a declared scenario default, while retaining
independent conditions on lateral/transverse boundaries. For the first
implementation, prefer the declared natural homogeneous cap policy where
supported; expose it in the effective model/diagnostics even if it is realized
by adding no boundary term or constraint. Do not apply incident sources on
the PML terminal caps by default. COMSOL 6.2 likewise treats PML as a domain
feature; its physics interface and model boundary conditions remain relevant.
See the [COMSOL 6.2 PML research note](../../../../others/comsol/scenarios/pml-comsol-62.md) and its
linked implementation, known-issues, RF, and acoustics sources.

## Design objective and non-goals

### Desired user and scenario split

- The **model** identifies the open physical boundary/boundaries, the PML
  layer thickness, selected stretch directions or a boundary-normal mode,
  profile parameters, wavelength/frequency source, and outer termination.
- The **scenario JSON** declares whether its equation family, study, FE family,
  and boundary trace support each PML mode. It supplies coefficient symbols,
  operator pullback, and any formulation-specific outer trace mapping.
- Shared Rust infrastructure owns geometric layer generation/tagging,
  stretch evaluation, local Jacobian construction, transformed quadrature, and
  the generic boundary-condition application protocol.
- A scenario that does not declare a compatible PML capability is rejected
  with a precise message. It must not appear to work merely because an
  unrecognized JSON extension was accepted.

No persistent scenario should say “rectangle”, “circle”, “sphere”, or
“cylinder”. Geometry is inferred from mesh/model geometry and selected
boundary entities. Global orthogonal directions and their factors are valid
generic inputs; shape labels are not. The user should continue to see and
author the physical region of interest. The generated PML shell is a
computational extension with a preview/diagnostic representation, not a
replacement for the authored geometry.

The user previously expressed a preference for FullMeta's own convention.
The PML control may be exposed with Physics/open-boundary settings and let the
user select the physical boundary where the layer begins. This is a FullMeta
UI choice, not an attempt to reproduce COMSOL's tree organization.

### Important scope limit

“Generic PML” means one shared coordinate-map/geometry service and reusable
assembly machinery. It does **not** mean one scalar multiplier that can be
applied to every matrix or one unchanged equation for every study. Each
weak-form operator family needs a justified pullback. Time-domain PML often
needs auxiliary states or convolution-equivalent equations; it is not the
frequency-domain complex stretch evaluated with a real frequency variable.
The initial implementation must declare and enforce a finite supported set.

## Mathematical contract to implement

Use separate names for the real computational geometry and the complex
virtual coordinate map. Let x denote a real point in the generated physical
mesh, and let

    Phi(x) = x_tilde(x),       J(x) = D Phi / D x.

The physical integration measure is the real mesh measure. The transformed
operator is evaluated using the complex J and the correct determinant/inverse
factors. Do not move mesh nodes to complex coordinates and do not confuse the
real outer mesh boundary with x_tilde.

For a scalar divergence-form operator, if the virtual-coordinate PDE has
coefficient tensor A, one common pulled-back weak-form pattern is

    volume coefficient: det(J)
    gradient tensor:    det(J) J^-1 A J^-T.

The exact transpose/conjugation convention depends on the chosen weak form and
time-harmonic sign convention. This formula is a derivation target, not a
universal shortcut for every operator. Add independent derivations for
Maxwell curl/curl and de Rham fields, longitudinally reduced operators,
elastic strain/stress, and any coupled/mixed form before declaring them
supported. The physical test/trial pairing must preserve the project's
existing sesquilinear conventions.

At each element quadrature point, the geometry service must return a typed
sample containing at least:

- real position and PML-region identity;
- complex map Phi and full complex J;
- inverse J and determinant (with conditioning/singularity diagnostics);
- active stretch directions/profile values;
- a stable indication of which geometric stratum/chart supplied the sample.

The matrix J is generally not diagonal in the global x/y/z frame. For a
rotated rectangle it is expressed in the rotated frame and transformed back
to the global basis. In curved geometry, tangent/radial factors arise from
the derivative of the map; do not set them to unity by assumption.

## Unified geometry-to-map recipe and acceptance gates

The handover's concrete mathematical recipe is in
[pml-geometry-map.md](pml-geometry-map.md). It is part of this implementation
handover, not optional background. It defines how selected boundary patches
and real thicknesses generate real layer charts/outer boundaries, then how
the same charts generate the local complex map and Jacobian. The single
assembler-level rule is

    x = X_e(xi),  xi_tilde = G_e(xi),
    Phi(x) = X_e(xi_tilde),
    J(x) = D X_e(xi_tilde) D G_e(xi) [D X_e(xi)]^-1.

The layer builder, rather than the stretch factor, sets real thickness. For a
2D polygon, the linked note gives the explicit construction from ordered edge
vertices, outward normals, per-edge active thicknesses, adjacent shifted-line
intersections, and product charts at shared vertices. It also gives the
dimension-independent boundary-chart recipe, corner/edge charts, profile
integral, exact radial circle/sphere Jacobian, and rectangle/cuboid
reductions. The assembler receives a valid real element chart; it does not
branch on shape names.

This is a concrete proposed recipe with explicit validity conditions, not a
claim that a generic geometry layer already exists. Its chart builder must be
implemented in the geometry/mesher path and must reject nonunique, singular,
overlapping, or self-intersecting layer charts. The chart and complex map must
agree across shared strata; the inner interface must satisfy Phi(x)=x and
J=I. For a raw polygon approximating a smooth circle/sphere, convergence is a
refinement property; the implementation must not infer smooth curvature from
vertices alone.

### Mandatory geometric recoveries

The proposed recipe must pass these cases from the same core rule, with no
shape-name branch:

1. **Axis-aligned rectangle, x-only:** the generated domain gains left and
   right layers only; top/bottom do not gain layers; no corner PML regions
   appear; in each x layer J reduces to the conventional constant
   x-direction stretch for a constant profile.
2. **Axis-aligned rectangle, x and y:** four side regions and the four
   intersections emerge automatically; in each corner the local J equals
   the composition of the x and y stretches (diagonal in the box frame).
3. **Rigidly rotated/translated rectangle:** same result in the rectangle's
   frame and the tensor rotates/translates correctly in global coordinates.
4. **Regular N-gon sampled from a circle, isotropic radial data:** outer
   vertices and the local map converge under increasing N to the circular
   radial result. A literal polygon and a polygonal approximation of a circle
   are geometrically indistinguishable from vertices alone; a convergent
   polygonal approximation test is therefore the honest claim, not exact
   recognition of a circle.
5. **Shifted circle / shifted regular-N approximation:** translation of all
   geometry and its center must translate the output without changing J.
6. **3D analogues:** cuboid under one/two/three active orthogonal directions,
   rotated/translated cuboid, and increasing-face-count sphere approximation.
7. **x-only PML with transverse periodicity:** only the two x terminal caps
   receive the zero trace; the top/bottom periodic pair remains paired through
   the full PML stripes, including cap intersections, with its phase/Floquet
   relation preserved.

The box cases constrain local product charts and active directions; the
circle/sphere cases constrain curvature and the refined-boundary limit. Use
the linked chart/J equations to derive each case rather than hand-assigning
stripes or corner tensors. Do not declare the geometry layer complete until
the same chart construction passes both groups. If a chart fails a listed
recovery, revise the shared construction and document the exact limitation;
do not add a scenario-specific shape branch.

The geometry layer must also reject or diagnose self-intersections, zero or
negative thickness, inverted cells, non-overlapping layer meshes, singular J,
and map folds (for example, a nonpositive real geometric orientation where
that criterion applies). Thickness may be locally reduced only under an
explicit, reported policy; silently clipping the generated layer is not
acceptable.

## PML profile and parameter semantics

Keep the user-level contract independent of COMSOL control names. The initial
profile vocabulary may include:

- **Polynomial:** normalized layer coordinate xi in [0,1], order/profile
  controls, and complex stretch rate derived from the selected frequency or
  wavelength. The profile must satisfy the chosen matching condition at the
  inner edge.
- **Rational:** only enable after its poles, endpoint behavior, units, and
  solver implications are defined for FullMeta's sign convention.
- **User-defined:** separate real and imaginary profile expressions with
  declared argument units/ranges and validation for finite values and an
  identity/matched inner endpoint.

COMSOL 6.2 documentation offers polynomial, rational, and user-defined
stretching, but its PMLfactor/PMLgamma are COMSOL-specific profile controls;
they should not be copied blindly as universal FullMeta semantics. Their
meaning is summarized in the [COMSOL 6.2 note](../../../../others/comsol/scenarios/pml-comsol-62.md).
FullMeta should expose physically named inputs such as layer thickness,
profile order/curvature, and typical wavelength/frequency source, then derive
the actual map consistently. Define whether thickness is geometric shell
thickness or effective complex thickness; keep those concepts separate.

The wavelength must be supplied from a scenario-supported material/physics
quantity or explicitly by the user, and must correspond to the longest
relevant propagating wavelength expected in the layer. Validate positivity,
units, frequency dependence, and the unknown-frequency case. Eigenfrequency
and propagation-mode studies require an explicit approximation/iteration
policy when the eigenvalue itself sets wavelength; do not let a complex
eigenfrequency or unknown frequency enter profile expressions implicitly.

## Study and physics compatibility

Declare support per scenario contract and per study type. At minimum classify:

| Study/problem | Required treatment before declaring support |
| --- | --- |
| Harmonic frequency-domain, prescribed frequency | Complex coordinate stretch, appropriate operator pullback, explicit outer termination, frequency/wavelength validation. Candidate first scope. |
| Harmonic with finite kz / 2D cross-section | Embed transverse J in the full operator frame and transform all reduced terms consistently, including z derivatives and coupling terms. A 2D mesh does not mean a 2D physical operator. |
| Propagation mode, beta/kz eigenvalue | Verify that transformed operator remains a valid polynomial/eigenvalue problem when J is independent of kz; reject profile definitions that depend implicitly on unknown kz unless a supported nonlinear EVP is implemented. |
| Eigenfrequency | Unknown frequency makes wavelength/profile eigenvalue-dependent. Either use a documented fixed reference wavelength with accuracy limits, implement a nonlinear EVP, or declare unsupported. Check and report PML-localized/spurious modes. |
| Transient | Requires a real-time PML realization, commonly auxiliary variables/convolution/state equations, and a matching outer boundary condition. Do not reuse the harmonic complex matrix unchanged. |
| Stationary/static | A wave-absorbing PML usually has no purpose; reject unless the scenario defines a distinct mathematically justified complex-coordinate use. |

The first implementation should be narrow enough to validate, likely a
prescribed-frequency harmonic scalar wave form plus one separately derived
EM formulation. It may not claim support for all EM formulations simply from
one successful scalar test. The user's goal of scenario JSON-only additions
after the shared infrastructure is installed is met only for scenarios that
fit the already implemented contract and operator families.

## Mesh, region identity, and boundary handling

- The generated shell must be represented in the real mesh and tagged as a
  PML domain/region. The assembler cannot create a PML if the mesh does not
  contain those elements or their identity.
- Preserve the authored physical geometry and selection IDs. Generated outer
  geometry, region tags, and derived boundary selections need stable,
  traceable IDs and must not collide with user-authored entities.
- The physical/PML interface should share nodes/DOFs and match the continued
  exterior material/operator. It should not receive a user-selected
  impedance condition by default.
- Generate a suitable layer mesh: elements resolve the profile and wave;
  curved maps need adequate geometry order. Avoid sliver/inverted cells and
  inspect effective quality under the transformed metric, not only ordinary
  Euclidean mesh quality.
- Apply the selected outer termination only on generated terminal caps normal
  to active stretch directions. The boundary adapter must know the primary
  field space and its correct trace. Preserve other physical boundary
  conditions, sources, ports, and periodic constraints on lateral/transverse
  boundaries.
- For unidirectional PML, a periodic pair in an unstretched direction must be
  propagated across the full computational cell, including the PML stripes;
  its geometric correspondence, phase/Floquet factor, and DOF constraints
  must remain valid after shell generation. Cap constraints at intersections
  with periodic faces must be checked for compatibility.
- Periodic, port, symmetry, and PML selections may conflict, especially when
  periodicity is imposed along a stretched direction or overlaps a terminal
  cap. Validate topology and selection overlap; reject unsupported
  combinations rather than silently changing precedence.

## FullMeta source integration points inspected

These are current architecture anchors, not a claim that each file must be
edited:

- Scenario declarations are registered JSON packages under
  [src/scenarios](../src/scenarios/)
  and loaded through [scenario_package.rs](../src/scenario_package.rs).
  The contributor rule says ordinary scenario additions belong in JSON and
  must not add scenario-specific Rust solver branches; shared runtime changes
  are appropriate only for shared behavior.
- Scenario study/feature capabilities are composed by
  [scenario_runtime.rs](../src/scenario_runtime.rs).
  A new PML contract needs validation and capability composition, not merely
  arbitrary JSON fields.
- The shared typed form engine is
  [weak_form.rs](../src/weak_form.rs), with
  sparse element/scatter infrastructure in
  [assembler.rs](../src/assembler.rs). The form
  engine samples Value, Gradient, RotatedGradient, SymmetricGradient, Curl,
  and Divergence; the PML transform must enter where basis/operator samples
  and coefficients are combined, before local matrices are scattered.
- Current base mesh readers include
  [mesh2d.rs](../src/mesh2d.rs) and
  [mesh3d.rs](../src/mesh3d.rs). They expose
  nodes, cells, and tagged boundary edges/faces. The PML work must trace how
  upstream geometry/mesher selections become cell tags and how generated
  layer geometry is persisted; assembler-only edits cannot invent those
  regions.
- The 2D propagation-mode adapter is
  [propagation_mode_assembly.rs](../src/propagation_mode_assembly.rs).
  Eigenfrequency and transient paths have separate operator preparation and
  execution code; they need explicit PML adapters rather than an assumption
  that the harmonic assembly change reaches every study.
- Acoustic and EM JSON contracts already declare weak forms, study types,
  boundary capabilities, and propagation-mode formulations. Use those as
  patterns, but do not infer PML support from a formulation name.

Before implementation, the receiving session should read
[applications/fem/AGENTS.md](../../AGENTS.md),
[fem-rust/AGENTS.md](../AGENTS.md), and the FEM [specification index](../../specification/README.md),
[requirements](../../specification/REQUIREMENTS.md),
[architecture](../../specification/ARCHITECTURE.md),
[platform profile](../../specification/PLATFORM.md), and
[acceptance contract](../../specification/ACCEPTANCE.md). This handover is not authority to bypass those
specifications or the current in-progress assembler task.

## Suggested contract shape (illustrative, not final schema)

Use a versioned shared capability with explicit operator and study support.
For example, a scenario declaration could express fields equivalent to:

~~~json
{
  "pml": {
    "contract": "fullmeta-pml/1",
    "studies": ["frequencyDomain"],
    "dimensions": [2],
    "geometryInputs": ["selectedPhysicalBoundary", "orthogonalDirections", "thicknesses"],
    "profiles": ["polynomial"],
    "operatorPullbacks": ["declared scalar divergence form"],
    "terminalCapTerminations": ["naturalZeroConormal", "homogeneousPrimaryTrace"],
    "defaultTerminalCapTermination": "naturalZeroConormal"
  }
}
~~~

The actual location and field names must fit the current schema/package rules.
The per-model PML instance then refers to the capability and supplies stable
boundary selections plus profile values. A scenario must declare the actual
formulation pullback/trace adapter; a generic string such as “waves” is not
enough. Extend the shared contract once, then scenario JSON may opt in without
new solver code when it matches implemented mathematical capabilities.

## Validation plan and release gates

### Gate A: geometry and map (no PDE solve)

Implement a standalone deterministic geometry/map evaluator. Verify outer
vertices, Jacobian, inverse, determinant, interface matching, active-set
transitions, translation/rotation covariance, thickness limits, and
non-folding. Test the six geometric recoveries above. Compare analytic J
against finite differences of Phi at random interior points and points near
each stratum; treat nondifferentiable strata explicitly rather than masking
them with tolerances.

### Gate B: transformed operator identities

For each supported operator, verify pullback against an independently
transformed manufactured solution or a direct coordinate-change reference.
Check units, complex signs, determinant, inverse, test/trial convention,
longitudinal terms, source/load transforms, and natural boundary terms. Add
small matrix-level tests before end-to-end solves.

### Gate C: physical PML acceptance

Create solved open-domain benchmark cases with known reflection metric. Report
reflection versus PML thickness/profile and mesh refinement, and show the
physical-region target observable converges. Compare natural homogeneous and
essential homogeneous cap termination on at least one benchmark; their
physical-region results should converge to the same limit as the PML is
refined. Include oblique incidence and a corner/curved case. A field plot that
merely appears to decay is insufficient. Do not state “perfectly matched” or
“validated” until this evidence exists.

For each case, record the mesh, profile, wavelength source, outer termination,
solver/residual, reflection metric, and refinement sequence. Also test that a
wrong scenario/study/outer boundary selection is rejected before assembly.

## Known caveats and open questions

- The single shape-neutral polygon/surface recipe and its J are the central
  design risk. The rectangle, rotated rectangle, circle-limit, and 3D
  counterparts are acceptance constraints, not already-proven results.
- Vertices alone cannot tell whether the input describes a literal polygon or
  samples of a smooth curve. Curved-limit convergence needs an explicit
  geometry interpretation or sufficiently rich boundary representation.
- The outer termination is approximate and formulation-specific. The natural
  conormal operator and any essential trace DOFs must be derived for each
  supported family; a no-op in the boundary adapter is not sufficient unless
  it is verified to produce the declared natural condition.
- Complex determinant branches, orientation, and weak-form conjugation need
  one documented sign convention and independent tests.
- H(curl), finite-kz, nonlinear/material dispersion, eigenfrequency, and
  transient implementations are not implied by scalar harmonic support.
- COMSOL 6.2 offers useful comparisons for profiles, wavelengths, structured
  PML meshing, and failure symptoms, but does not validate FullMeta's proposed
  generic geometry construction. See the research note for its API/runtime
  discrepancy and evidence limits.

## Research and exact evidence scope

The relevant local COMSOL study note is
[pml-comsol-62.md](../../../../others/comsol/scenarios/pml-comsol-62.md). It records installed **COMSOL
Multiphysics 6.2.0.290 (Build 290)** documentation/runtime results and links
the source evidence, including the EM API probe source/stdout/log, the
`r0`/`raxis` property-scope probe, and read-only saved-state inspection. The
COMSOL 6.2 documentation describes the PML as a domain coordinate-stretching
feature, documents Cartesian/cylindrical/spherical and user-defined settings,
profiles, wavelength selection, meshing, and known issues. Probe models were
fresh/in-memory and did not establish solved physical accuracy. No COMSOL 6.4
material should be used to settle the 6.2 behavior.

The starting FullMeta sources named for this handover are
[EM physics equations](../../../../others/comsol/scenarios/em-physics-equations.md),
[EM feature audit](../../../../others/comsol/scenarios/em-feature-audit.md),
[PML API probe status](../../../../others/comsol/evidence/em-pml-api-probe-status.md), and
[EMPmlApiProbe.java](../../../../others/comsol/evidence/EMPmlApiProbe.java), with the corresponding
stdout/log linked from the probe record. The architecture pointers above were
read from the current FullMeta checkout; no user `.mph` files were edited or
overwritten for this handover.

## Handover summary

Implement PML as a versioned shared geometry-map and operator-pullback
capability, with scenario JSON opting in only where the solver has an
implemented adapter. The generated real shell, complex coordinate map, local
J, physics-specific weak-form transform, and outer termination are distinct
parts of the contract. First prove one construction rule against the
rectangular directional cases and circular-limit cases; then implement the
geometry/mesh integration and a deliberately narrow harmonic study; then earn
additional scenario/study support with transformed-operator and solved
reflection/refinement evidence.
