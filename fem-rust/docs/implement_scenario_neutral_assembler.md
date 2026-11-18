# Scenario-neutral FEM assembler: implementation plan

> The scenario-specific test scripts and legacy numerical oracles mentioned in
> the historical verification record have been retired. Their past results do
> not establish coverage from the remaining current test suite.

## 1. Status and objective

This document records the implementation plan and the current acceptance
status. All eight existing scenario packages now select shared generic
execution; the former scenario Rust implementations and parity oracles have
been retired. The detailed gates below include completed checks and remaining
performance and acceptance work. The target architecture is:

- Eight scenario JSON packages describe the existing physics, formulations,
  discretizations, boundaries, validation, recovery and result contracts.
- One scenario-neutral `src/assembler.rs` and its **generic** supporting modules
  assemble and solve the systems. The eight scenario-specific production `.rs`
  modules and their numerical dispatch paths have been retired.
- Assembly and solution use a sparse master matrix.
- A future scenario expressible in the implemented mathematical vocabulary is
  authored by adding one scenario JSON package, with no edits to `assembler.rs`,
  another Rust source file, or a hand-maintained scenario registry. Normal
  installation/activation and verification are still required.

### Migration progress (18 November 2026)

The packages declare volume, boundary, essential and recovery operations;
generic sparse assembly, expression, constraint, simplex, Nédélec and
tensor-recovery components execute the existing scenarios. Thermal 2D and 3D
use the generic stationary scalar P1 execution capability selected by JSON.
Historical comparisons checked temperatures, fluxes, constraints and residuals
against the prior numerical paths on small cases with Faer and Rivrs. The elastic
2D and 3D paths use shared vector P1 assembly and recovery. Disposable thermal 2D and 3D
solver requests produced field JSON and CSV matching the existing executable
within numerical tolerance. Thermal 2D and 3D Studies also completed in an
isolated installation, including meshing and output verification; their slab
results matched the analytic temperature and flux. Small elastic 2D and 3D
frequency Studies completed in the same installation. The latest completed
candidate also matched baseline field files and sweep CSVs for disposable
acoustic 2D/3D, EM 2D Ez/Hz and EM 3D order 1/2 Studies. The reusable
isolated Study comparison subsequently passed all ten cases,
covering the eight scenarios and those formulation/order variants. It requires
a built candidate and the existing installed baseline. That completed run used
an elastic 3D fixture without field files; an expanded fixture subsequently
produced matching displacement/stress field files and sweep CSV under the
field-scale tolerance documented in the comparator. After the JSON-driven thermal output change the
harness passed all ten cases again. Isolated opt-in copies of acoustic 2D/3D
and EM 2D Ez/Hz also completed two-frequency Studies on the generic harmonic
path and matched the baseline outputs. Direct comparisons with the previous
numerical paths cover complex periodic and impedance conditions and spatially
varying materials. These packages now declare the generic scalar capability
in production; the rebuilt cutover passed the end-to-end checks, including
run-record and debug-stage compatibility. A separate JSON-only stationary scalar
package also completed an actual isolated Study and editor schema composition;
changing its declared diffusion and recovery coefficients produced the expected
different slab solution. This establishes the addition path for that supported
scalar subset; later checks below also cover the vector and oriented-element
families. It is not a claim that arbitrary future PDEs are supported. The latest completed Rust suite
passed 317 tests, with four ignored; these are not a
replacement for the full end-to-end and performance acceptance gates below.
The focused Node schema, discovery, comparator and structural-count checks
also passed. The supported `node prebuilt-tools/build.mjs --offline` command
built the optimized executable and all three required DLLs under `target/`;
the executable passed its startup check without staging.
An isolated opt-in vector run has matched two-frequency elastic 2D plane stress,
plane strain, CSV-only and elastic 3D field outputs against the installed
baseline. Elastic packages now declare the generic vector capability in
production; the rebuilt cutover also passed those four Studies, including
run-record provenance and output names.
The full ten-case isolated Study harness subsequently passed with seven generic
production packages and the remaining EM 3D adapter, including elastic 3D
field files, followed by the JSON-only stationary scenario.
Isolated opt-in EM 3D Studies have now matched the legacy route for both
Nédélec orders, including fields, CSVs, stage structure and run records. An
additional JSON-only oriented-field package completed schema composition and
an actual Study without a Rust scenario file; changing its curl coefficient
changed its computed solution. Further isolated Studies matched Rivrs/metis,
spatially varying material and configured pointwise/global integration. The
production package now declares the generic edge capability. The rebuilt
all-generic candidate has also passed that complete isolated edge harness and
its JSON-only clone; broader acceptance checks remain pending.
Direct-request parity tests also reject unsupported component coordinates,
underintegrated elastic and edge quadrature, and mismatched elastic 2D model
and request formulations. Configured integration uses JSON-declared selection
ID keys, including when a mesh-map row contains multiple possible IDs.
A fresh all-generic run of the full ten-case Study harness passed, followed by
both generic thermal reruns and the JSON-only thermal Study. A separate
JSON-only vector package also passed discovery, editor schema composition and
normal Studies without a Rust scenario file; changing its declared shear
coefficient changed the displacement field. These exercises cover JSON-only
additions in each of the three implemented execution families.
The eight two-frequency Faer Study variants also logged an initial symbolic
analysis followed by reuse for their second case; this verifies cache reuse on
those fixtures, not its performance on larger or changing meshes.
A preliminary optimized 20,000-triangle thermal comparison matched
outputs; three measured process times were 4.10–5.55 seconds for the baseline
and 4.21–4.56 seconds for the candidate, with sampled peak working sets of
about 178–189 MB and 187–203 MB respectively. Other Studies were running at the
same time, so these samples do not establish the final performance gate.
A second three-sample run on the same thermal fixture, after the other Studies
finished, measured 3.63–3.86 seconds for the baseline and 3.76–3.82 seconds for
the candidate. The sampled process peak working sets were about 176–189 MB and
187–201 MB respectively. This measures only this scalar case; vector, edge,
larger meshes and final-build performance still need assessment.
For a 3,200-triangle elastic 2D request after actual model preflight and with
the dispatcher's exact-model receipt, three alternating optimized runs measured
1.70–1.74 seconds for the baseline and 1.75–1.86 seconds for the candidate.
Sampled peak process working sets were about 165 MiB and 191 MiB respectively;
the field and CSV outputs matched. Standalone requests without a receipt also
run generic preflight and therefore have a different startup cost. This is one
vector fixture, not a substitute for the remaining backend, sweep and EM 3D
benchmarks.
With Rivrs/Metis, the same thermal fixture's standalone-request medians were
3.77 seconds baseline and 3.84 seconds candidate; peak process working sets
were approximately 191–193 MiB and 206–209 MiB. For the receipt-backed elastic
fixture, medians were 1.72 and 1.76 seconds, with approximately 165 MiB and
200 MiB peak working sets. Outputs matched in each case.
On a non-periodic 926-tetrahedron oriented-element fixture, Faer median solver
process times for first and second order were 1.27 and 1.54 seconds baseline,
versus 1.22 and 1.68 seconds candidate. Rivrs/Metis medians were 1.23 and
1.55 seconds baseline versus 1.29 and 1.83 seconds candidate. Each result is
three alternating runs including roughly 1.15–1.30 seconds of process startup;
the corresponding candidate median process peaks ranged from 170 to 304 MiB.
The first- and second-order matrices had 1,402 and 6,912 DOFs and 17,628 and
250,392 structural stored entries after constraints. These counts include
stored zeros and are not measured numerical nonzeros. The benchmark records
polled stage intervals, which include intervening work and are not pure kernel
times. These samples were taken on Windows 10.0.26200, an AMD64 family 25 model
68 system reporting 16 logical processors; the candidate is an optimized
snapshot, not a staged runtime. Larger meshes and sweep performance remain to assess.
After the direct-request validation fixes and reuse of prepared element bases,
the same edge benchmark was repeated with the supported optimized build. Faer
median times were 1.25/1.28 seconds baseline/candidate for order 1 and
1.45/1.57 seconds for order 2. Rivrs/Metis medians were 1.23/1.21 and
1.53/1.61 seconds respectively. Candidate median process peaks were about
170 MiB for order 1 and 289–307 MiB for order 2; all field and CSV artifacts
matched. The same small-mesh, startup and stage-timing limitations apply.
Final supported-build scalar/vector runs with three alternating samples per
backend also matched outputs. For thermal 2D (20,000 triangles), Faer medians
were 3.47/3.55 seconds and 180/193 MiB baseline/candidate; Rivrs/Metis medians
were 3.49/3.62 seconds and 193/208 MiB. For elastic 2D (3,200 triangles) with
a validated Study receipt, Faer medians were 1.57/1.68 seconds and 165/191 MiB;
Rivrs/Metis medians were 1.59/1.72 seconds and 165/200 MiB. The fixtures have
70,601 scalar and 45,764 vector pre-constraint structural entries, including
stored zeros, rather than measured numerical nonzeros or factor fill. The
parent-observed stage intervals are recorded in the benchmark artifacts, but
same-named intervals can include different preparation or recovery work and
are not directly comparable as isolated kernel times.

All eight packages now declare generic execution, and the non-test executable's
generated registry contains no scenario Rust modules. The eight former files
and their scenario-specific parity harnesses have now been retired. Historical
post-cutover Study comparisons matched. Broader performance and numerical
acceptance beyond the measured fixtures remain open. The generated
scenario index automates discovery; a compatible package can use an implemented
generic capability after normal index generation and installation. No rebuilt
executable has been staged to the application runtime as part of this migration.

This is not permission to combine eight solvers into one file with profile-ID
branches, to move such branches into generic-looking helper files, or to name
scenario-specific kernel callbacks in JSON. The factorization test is whether
the same declared mathematical operations execute through the same code for
different scenarios. Branches on element type, operator, data type, quadrature,
constraint or declared formulation are expected; branches or kernel selection
on a scenario's name, physics label or file name are not.

The user's backup covers `fem-rust/`. Keep implementation in that directory as
the working target and preserve external model, editor, command, Study and result
interfaces. Staging the resulting executable writes under `applications/fem/.runtime/`
and is a separate activation step. If a necessary change outside `fem-rust/`
is discovered, identify it before changing that scope. In particular, the
existing application specification describes a manually maintained registry and
scenario-specific runners; reconciling that normative description with the
delivered architecture requires an explicit specification update outside this
directory, or an identified documentation/contract follow-up. Do not pretend
that a plan in this file has changed that specification.

### Source of truth and prerequisites

Before implementation read `../AGENTS.md`, `../../AGENTS.md`, the
[scenario runner interface](../src/scenarios/README.md), the
[FEM specification](../../specification/README.md), and the
[FEM Rust interface guide](../../docs/fem-rust-and-scenario-interfaces.md).
Recheck the current JSON contracts, Rust solvers and notes linked below: they
may have changed since this plan was written. Derive disposable tests from the
contracts and analytic problems; do not use saved user projects as fixtures.
Keep the existing scenario notes accurate as work progresses and distinguish
implemented capabilities from ideas in this plan.

## 2. Architectural boundary

The proposed path from physical problem to solve is:

```text
scenario JSON + existing model + mesh + Study settings
   -> validate and resolve the versioned scenario contract
   -> select a declared formulation, coefficients and boundary relations
   -> type-check and lower forms to a generic assembly plan
   -> construct element DOFs, orientations, constraints and sparse pattern
   -> evaluate coefficients and assemble volume and boundary contributions
   -> reduce constraints, solve, expand and recover declared fields
   -> preserve the existing field, sweep and run-result contracts
```

The JSON is authoritative for scenario-specific definitions. Shared code owns
general machinery: tensor operations, real/complex arithmetic, expression
evaluation, quadrature, basis functions and mappings, mesh topology, sparse
storage, general constraints, linear solves and result-writing primitives. A
generic runner handles the existing prepared request kinds and invokes the
assembly plan. Existing coordinator and model interfaces should continue to see
the same profile IDs, commands, options, artifacts, field symbols and units.

The first executable contract is a **typed weak form**. Record first-order PDEs,
primary/derived-field relations and their units too, so a person can trace the
weak form back to physics. The first implementation need not automatically
derive weak forms or eliminate derived variables from arbitrary PDEs. Those
transformations require choices about unknowns, integration by parts, signs,
traces and regularity; a future form compiler may support a defined subset.
Validate any declared weak form against the stated physical relations.

### Generic components

Names and file boundaries may change during implementation; responsibilities
must remain generic and separately testable:

1. **Contract reader and validator:** versioned scenario package and expression
   tree; declarations, units, shape, finite-value and capability diagnostics.
2. **Form compiler/planner:** typed fields, operators, material and boundary
   expressions, domains, quadrature and recovery lowered once to a plan.
3. **Element library:** triangle/tetrahedron geometry; nodal P1 vector/scalar;
   Nédélec tetrahedron orders 1 and 2; orientation and reference mappings.
4. **Topology and constraints:** entity/DOF maps, essential trace moments,
   periodic transformations, affine constraints and component mean constraints.
5. **`assembler.rs`:** volume and boundary integration, sparse pattern and
   accumulation using the compiled plan, with real/complex systems as needed.
6. **Generic solve and output:** invoke existing `solver_common.rs` backends,
   recover fields, run declared integrations and serialize existing artifacts.

Existing `mesh2d.rs`, `scalar2d.rs`, `field_output.rs`, `model_expression.rs`,
`core_language.rs` and `solver_common.rs` contain potentially reusable code.
Extract or generalize by mathematical responsibility; do not simply rename a
scenario-specific routine. Retain existing platform SDK and batched expression
interfaces rather than introducing per-quadrature JSON calls or a new dependency
without justification.

## 3. Mathematical and finite-element contract

Use the current `exp(+i omega t)` convention for harmonic scenarios. Conjugate
the test argument explicitly. For a declared test field `v` and trial field `u`,
the initial linear form language must express

```math
a(v,u)=\sum_t\int_{\Omega_t}
\overline{D_t v}:C_t:E_t u\,d\Omega
+\sum_b\int_{\Gamma_b}\overline{B_b v}:Z_b:T_b u\,d\Gamma,
\qquad a(v,u)=\ell(v).
```

`D` and `E` initially cover identity, gradient, curl and symmetric gradient;
boundary operators include value, normal and tangential trace/projection.
Coefficients can be real or complex scalars and suitably shaped vectors,
matrices or tensors, including frequency/coordinate/parameter dependencies.
Specify contractions, axes, dimensions, units and complex conjugation, not just
an untyped string. Linear terms support cell and boundary loads. Multiple
fields/couplings should be representable in the IR even if the first acceptance
scope is the existing single-primary-field formulations; reject unsupported
forms rather than silently dropping terms.

Whitney forms provide the compatible-space framework: nodal P1 functions are
0-forms and the lowest-order Nédélec basis is a 1-form; higher-order Nédélec
is a compatible extension, not the same lowest-order Whitney basis. Select the
space from the declared unknown and weak formulation, rather than guessing it
from a physical field's name or vector shape. Elastic displacement is a vector
of nodal fields; stress is recovered, not automatically a Whitney volume-form
unknown. A material-weighted inner product is a Hodge-like operation, but
elasticity's symmetric gradient and constitutive tensor also need explicit
generic representation.

For local basis functions `phi_i`, implement the declared term as

```math
A^{(e)}_{ij}=\sum_q w_q |J_e(q)|\,
\overline{D\phi_i(q)}:C(x_q):E\phi_j(q).
```

Account for element geometry and orientation transforms before scattering.
For affine constraint representation `u = P u_hat + u_0`, preserve the
sesquilinear reduction `A_hat = P^H A P` and
`b_hat = P^H (b - A u_0)`; the implementation may fold locally or globally
without explicitly constructing a dense `P`. Check phase/orientation cycles
and conflicting values, and support post-solve expansion. Mean constraints
can augment the system; do not assume all systems are positive definite.

## 4. Proposed versioned JSON contract

Extend each existing `fullmeta-fem-scenario/1` package compatibly or introduce
an explicitly versioned new contract after checking both JS and Rust consumers.
Do not silently reinterpret an existing package or saved Model 2 value. The
following is illustrative pseudocode showing the intended shape, **not yet a
supported schema or executable syntax**:

```json
{
  "id": "thermal-3d",
  "formulationPlan": {
    "field": {"id": "T", "shape": "scalar", "unit": "K",
              "space": "H1", "element": "simplexP1"},
    "definitions": {"q": "-k * grad(T)"},
    "volumeTerms": [
      {"testOperator": "grad", "coefficient": "k", "trialOperator": "grad"}
    ],
    "boundaries": {
      "dirichlet": {"trace": "value(T)", "input": "temperature"},
      "neumann": {"trace": "normal(q)", "input": "outwardHeatFlux",
                  "loadCoefficient": "-1"}
    },
    "recovery": [{"field": "q", "location": "cell"}]
  }
}
```

The actual contract must also contain the existing scenario schema contributions,
model bindings, material quantities and assignments, supported expression scopes,
defaults, quadrature, nullspace policy, output/result configuration and runtime
metadata. Implement a typed expression AST or another inspectable representation
for scalar/vector/tensor operations; the illustrative strings do not assume that
the current platform expression language can already express every operator.
Support complex quantities, tensor indexing/contraction, gradients, curl,
symmetric gradients, normals and tangential projections, rotations, conditional
formulation selection, and declared physical constants. Use a deliberate typed
bridge to the existing FullMeta expression engine for model scalar expressions.
Decide explicitly which layer owns units and evaluation, and test it. No
arbitrary native function pointers, scenario-name code or executable scripts.

Validate package version, unique safe IDs, dimensions, fields, operator/space
compatibility, term shapes and units, coefficient dependencies and cycles,
supported selections, Study type, quadrature and recovery locations before
assembly. Fail with scenario ID and precise JSON path, term and reason for
unknown/unsupported capabilities. Preserve existing model-validation behavior,
including rejected combinations; an engine feature does not automatically
enable it for every scenario. Preserve zero/nonzero and legacy boundary input
rules. Resolve formulation variants from declared model values, not hardcoded
scenario IDs.

### Concrete term catalogue for the current eight scenarios

In the table, each row means an integral over its domain, with the test factor
conjugated. `u` denotes the declared primary field and `v` its test field.
`epsilon(u) = sym(grad(u))`, `rho* = rho(1-i eta_rho)`,
`epsilon_c = epsilon_r - i sigma/(omega epsilon_0)`, and
`k0^2 = omega^2 mu_0 epsilon_0`. These are formulation definitions, not a
claim that all coefficients are currently exposed as user inputs.

| Scenario and formulation | Field / element | Declared volume integrand | Current capability to preserve |
| --- | --- | --- | --- |
| Thermal 2D | scalar `T`; P1 triangle | `k grad(v)·grad(T)` | Stationary, real; parameter-constant `k`; zero-phase periodic; vector or legacy scalar outward flux; mean per unanchored component. |
| Thermal 3D | scalar `T`; P1 tetrahedron | `k grad(v)·grad(T)` | Stationary, real; parameter-constant `k`; scalar outward flux; no periodic; volume mean per unanchored component. |
| Acoustic 2D | scalar `p`; P1 triangle | `rho^-1 grad(v)·grad(p) - omega^2/(rho c^2) v p` | Complex harmonic; quadrature-point materials; normal velocity, impedance and Floquet periodicity. |
| Acoustic 3D | scalar `p`; P1 tetrahedron | Same acoustic integrand | Same trace semantics on faces and 3D vector velocity. |
| EM 2D `Ez` | scalar `Ez`; P1 triangle | `mu_r^-1 grad(v)·grad(Ez) - k0^2 epsilon_c v Ez` | Scalar primary, derived in-plane `H`; impedance and Floquet periodicity. |
| EM 2D `Hz` | scalar `Hz`; P1 triangle | `epsilon_c^-1 grad(v)·grad(Hz) - k0^2 mu_r v Hz` | Scalar primary, derived in-plane `E`; formulation-specific coefficients and signs. |
| EM 3D `electric` | vector `E`; Nédélec tetrahedron orders 1 and 2 | `mu^-1 curl(v)·curl(E) + (i omega sigma - omega^2 epsilon) v·E` | Tangential moments, face orientation at order 2, impedance, periodic phase; centroid material sampling. |
| Elastic 2D | vector `(ux,uy)`; two P1 triangle fields | `epsilon(v):C_2D:epsilon(u) - omega^2 rho* v·u` | Plane stress and plane strain; per-unit depth; zero-drive scalar impedance; no periodic. |
| Elastic 3D | vector `(ux,uy,uz)`; three P1 tetrahedron fields | `epsilon(v):C:epsilon(u) - omega^2 rho* v·u` | Six recovered stresses; zero-drive scalar impedance; no periodic. |

Consult [thermal 2D](notes_thermal2d.md), [thermal 3D](therm3d.md),
[acoustic 2D](notes_acoustic2d.md), [acoustic 3D](notes_acoustic3d.md),
[EM 2D](notes_em2d.md), [EM 3D](notes_em3d.md),
[elastic 2D](notes_elastic2d.md) and [elastic 3D](notes_elastic3d.md)
for the full equations and current model/solver contracts. The table does not
replace a line-by-line audit of current code and scenario JSON at migration time.

### Materials, constraints and recovery

- Thermal conduction uses `q=-k grad(T)` and no current volumetric source or
  transient term. Require positive conductivity and preserve spatial-expression
  restrictions. Check flux balance for each unanchored connected component,
  then impose its area/volume weighted mean, not an arbitrary pin.
- Acoustics recovers `v = i grad(p)/(omega rho)`, samples materials at the
  declared locations and currently rejects nonzero acoustic loss factor. Keep
  physical density/sound-speed validation and supported expression scope.
- EM2D preserves the `Ez`/`Hz` material inverses, rotated derivatives, Ohmic
  loss and units. EM3D uses `H=i curl(E)/(omega mu)` with centroid material
  sampling. Preserve electric orders 1 and 2 and their quadrature restrictions.
- Elasticity derives `mu=E/[2(1+nu)]` and
  `lambda=E nu/[(1+nu)(1-2nu)]`; for plane stress use
  `lambda_2D=2 lambda mu/(lambda+2 mu)`, for plane strain use `lambda`.
  Declare tensor vs engineering shear conventions and recover `sigma_zz`
  correctly. Preserve density-loss sign and material range checks. Do not add
  body-force inputs, incident impedance excitation or periodicity implicitly.
- Preserve per-cell versus nodally averaged recovery, including material
  sampling and component ordering, as well as selected field and sweep names.
  Recovery averaging is not a new independent FE unknown and may smooth jumps.

## 5. Boundary form contract

Boundary inputs describe physical quantities; the JSON must specify the trace,
weak-form sign, frequency factor, unit, expression scope, integration rule,
default and compatibility behavior. The generic machinery must support both
essential trace interpolation/moments and natural/Robin boundary integrals.

| Physics | Essential | Natural and impedance behavior to preserve |
| --- | --- | --- |
| Thermal | `T=d` | Outward `q·n=g` contributes `-integral(v g)`; unassigned is insulated. 2D permits vector or scalar; 3D scalar only. |
| Acoustic | `p=d` | `v_particle·n=g` contributes `-i omega integral(v g)`; unassigned is rigid. Preserve impedance incident source and legacy scalar/vector forms. |
| EM2D | scalar `Ez` or `Hz` | Physical in-plane derived field is projected tangentially, with formulation-specific sign/scaling; preserve legacy conormal input, impedance, and actual PEC/PMC mappings. |
| EM3D | tangential `n×E=n×d` | `n×H=n×h` contributes `+i omega integral(v_t·(n×h))` to the RHS; preserve zero natural PMC and impedance relation. |
| Elastic | full displacement vector | Traction `sigma n=t`; unassigned is free. Scalar `Z` gives `sigma n+i omega Z u=0`, contributing `+i omega integral(Z v·u)` on the left; no incident impedance drive. |

Here boundary `v` in the integrals is the appropriate conjugated test field;
particle velocity is written explicitly to avoid a naming collision. Use
outward normals computed from adjacent cells, handle reversed boundary
orientation, field units and legacy scalar `value` versus `fieldValues` as the
current contracts specify. EM3D must constrain edge and higher-order face
moments rather than Cartesian nodal values. Reject overlaps with periodic
boundaries and conflicting constraints according to each existing contract.
Do not equate local impedance with an exact nonreflecting condition.

## 6. Sparse assembly and execution

1. Build mesh/entity incidence and reference-to-physical mappings; validate
   geometry, tags, selections, topology, element orientation and degeneracy.
2. Enumerate primary-field DOFs from the selected element family and order.
   Generate oriented local-to-global transforms and identify periodic pairs.
3. Derive a conservative graph from declared cell and boundary couplings and
   transformed DOFs, including mean/multiplier constraints. Do not infer the
   graph merely from current coefficient values. Constraints can introduce
   nonlocal connections; build the final graph after accounting for them.
4. Allocate sparse storage (or sparse triplets followed by coalescing) and
   assemble element and boundary matrices and RHS. Avoid any global dense
   `N x N` matrix; dense *local* element matrices are appropriate.
5. Apply/reduce essential, periodic and mean constraints consistently, then
   invoke Faer sparse LU or Rivrs where the existing contract permits. Do not
   presume symmetry/Hermiticity/positive definiteness; test complex phases,
   loss and augmented systems against each backend's actual requirements.
6. Compute residuals in the correct constrained system, expand the solution,
   recover fields and preserve result metadata and run lifecycle.

Precompute basis/derivative tables and sparse insertion maps where profitable.
Reuse topology, sparsity, ordering or symbolic factorization in a sweep only
when their dependencies are unchanged: mesh, element/field layout, active
terms and boundaries, periodic map, constraint topology and backend. Values,
frequency, parameter-dependent coefficients or phase may require numerical
reassembly and factorization. If the graph is conservative over declared
possibilities, document its cache key. Batch expression evaluation outside
inner loops where possible. Compare assembly and total solve time and peak
memory to the existing implementation on representative identical meshes;
generality alone does not establish equal performance.

## 7. Scenario discovery and JSON-only additions

At the outset of this plan, `build.rs` required each registered package's `solverModule` to
have a same-stem `.rs` file and generated module dispatch. Native
`scenario_runtime.rs` and the application-owned `schema-documents.js` both
read `src/scenarios/registry.json`; the latter is outside `fem-rust/`. The
then-current registry also supplied the default and ordering. Simply adding a JSON
file did **not** register it in either consumer. The generated index and generic
execution progress are described in the migration status above.

Planned discovery and acceptance checks (see the migration status above for
completed checks and remaining activation work):

1. Move package identity, deterministic display order and the single default
   declaration into each package; maintain the existing default and ordering.
   Preserve safe IDs and same-stem naming, validate one default, duplicate IDs,
   duplicate stems, symlinks and version errors. Do not infer identity from a
   directory listing alone.
2. Implement a generic deterministic index generator inside `fem-rust/` that
   discovers and validates scenario packages and produces the existing
   `fullmeta-fem-scenario-registry/1` format. The registry is **generated**, not
   manually edited. Make generation atomic and reproducible, and fail clearly
   on invalid or stale input rather than quietly omitting a package. Ensure
   generated artifacts are included in source/distribution packaging.
3. Wire generation/verification into the supported build and installation or
   scenario-activation path before editor schema composition and native process
   startup. Direct Cargo builds, tests and deployment paths must not accidentally
   consume a stale index. Do not rely on a Rust `build.rs` mutating a source
   file as an undocumented side effect; choose an explicit supported generation
   step and verify every actual entry point. Editors still reading a static
   manifest cannot see an added file until that generation and reload occurs.
4. Replace generated per-scenario Rust dispatch with one generic runner. Carry
   the selected validated package ID and its resolved contract into that runner
   rather than selecting a Rust module by profile name. Adapt the current
   `solverModule` contract and coordinator checks compatibly; do not require
   eight aliases that hide eight numerical implementations.
5. Verify that the browser schema loader, native schema composer, mesher,
   coordinator, packaged installation and runner see the **same** package list
   and version. Define process reload/restart and artifact identity behavior.
   If the existing external consumer or deployment path cannot be kept working
   with an automatically generated manifest produced entirely in `fem-rust/`,
   report the precise external integration change rather than claiming the
   JSON-only goal has been achieved.

Future author workflow: author one conforming JSON, run the supported package
activation/build workflow that automatically refreshes the generated index,
reload/restart as required, then validate and test. No hand-edit of registry,
Rust code or shared base schema. This is JSON-only **authorship**, not a promise
of hot loading or of no activation command. Do not promise that arbitrary new
UI controls, model structures, output formats, nonlinear methods, material
models or element families work unless their general capabilities are already
implemented. A new generic capability can require engine and possibly external
contract work; it then becomes available to subsequent JSON-only scenarios.

## 8. Implementation phases and gates

These phases record the migration approach and its remaining acceptance gates;
the production cutover has occurred, and the old scenario runners have been
retired. Freeze the executable/source identity, scenario
contracts, mesh and input when comparing old and new. Test-only comparison
harnesses are permitted during migration, not permanent alternate production
dispatch. Do not change user-facing defaults or numerical settings as a way to
make a comparison pass.

1. **Baseline and inventory.** Capture eight current capability matrices,
   validation cases, preparation paths, sample locations, output schemas,
   numerical tests and build/staging commands. Include legacy inputs, solver
   backends, fields, nullspaces, periodicity and unsupported features. Identify
   intentional differences between notes and code before transcription.
2. **Contract and generic plan.** Specify/version JSON and typed IR. Implement
   diagnostics, expression/units bridge and generic runner without replacing
   old solvers. Prove a JSON change controls a generic term and that invalid
   terms fail at their paths. Establish index generation and activation tests.
3. **Scalar P1 engine.** Implement triangles/tetrahedra, gradient and mass,
   scalar traces, boundary integration, sparse scatter, periodic mapping,
   mean constraints and recovery. Migrate thermal 2D/3D, acoustic 2D/3D and
   EM2D in small substeps. Preserve parameter-constant and quadrature-sampled
   material policies, both EM2D polarizations and all boundary variants.
4. **Vector nodal engine.** Add symmetric gradient, tensor contractions, vector
   constraints, traction, scalar impedance and stress recovery. Migrate both
   elastic scenarios and both 2D reductions. Preserve consistent mass and
   reject unsupported quadrature or unsupported input combinations.
5. **Edge engine.** Generalize tetrahedral Nédélec order 1 and 2, orientation,
   tangential DOFs, face moments and phase constraints. Migrate EM3D with its
   declared sampling, impedance and boundary traces. Exercise both backends,
   orders and periodic orientations.
6. **Cutover.** Remove eight production scenario Rust modules and per-scenario
   module generation only after every gate passes; update the scenario runner
   guide, scenario notes, any in-scope documentation and generation checks.
   Ensure source packaging, fresh build, staging, application composition and
   all eight scenarios use the same generic runner. Resolve the separate
   application specification/interface documentation scope explicitly.
7. **Prove extension.** Add a disposable ninth test scenario using only a new
   JSON file and supported existing operators, with a distinct ID and an
   independently known solution. Through the ordinary activation path, prove
   editor/native discovery, validation, meshing, Study, outputs and both
   applicable backends. Demonstrate no handwritten registry, Rust, UI or base
   schema changes. Remove the disposable fixture from production packages.

## 9. Verification and acceptance

Use independent analytic expectations as well as old/new comparisons. Record
mesh, inputs, tolerances, backend, candidate executable and contract identities.
For direct matrices compare DOF order and orientation or transform into a
common physical basis; compare reduced matrices and RHS with scaled absolute
and relative tolerances. Do not require bit-identical results from different
summation orders. Compare complex fields, residuals and declared outputs, not
only magnitudes. Near resonance, account for conditioning and use separate
well-conditioned cases for tight equivalence tolerances.

**Common numerical tests**

- Reference triangle/tetrahedron geometry and quadrature; nodal partition of
  unity, exact gradients and consistent mass. Show centroid underintegration
  where applicable and reject it where current contracts require.
- Nédélec order 1 edge integrals and curls; order 2 edge/face moments and
  traces; permutation/orientation invariance, periodic phase and corner-cycle
  consistency. Compare actual tangential traces and manufactured solutions.
- Sparse graph/values against a small independently assembled dense reference;
  empty and multiple domains, heterogeneous materials, mixed term coupling,
  constraints that add graph connections and real/complex arithmetic. Verify
  storage scales with mesh connectivity rather than the square of DOFs.
- Dirichlet RHS correction for nonzero data; normal/tangential projection;
  outward normal invariance; boundary expression quadrature; conflicts and
  periodic overlap. Compare explicit zero and unassigned natural boundaries.
- Balanced/unbalanced pure-Neumann connected components and weighted means;
  nonzero Bloch phase, phase cycles and inconsistent identified constraints.
- Validate unit/shape failures, invalid expressions, unsupported operators,
  missing materials, bad selections, nonfinite coefficients, unsupported
  features and rejected quadrature, with actionable paths and diagnostics.

**Scenario checks**

| Scenario | Independent and parity checks |
| --- | --- |
| Thermal 2D and 3D | Linear slab temperatures, heat-flux signs, balanced flux and component mean, material interface; vector/scalar parity in 2D; supported periodic and rejection cases. |
| Acoustic 2D and 3D | Manufactured complex pressure/velocity, pressure and normal velocity traces, scalar/vector parity, impedance source, phase-periodic case and both solver backends. |
| EM2D | Both `Ez` and `Hz`; manufactured fields, tangent/orientation reversal, physical versus legacy conormal input, PEC/PMC, impedance source, phase periodicity and units. |
| EM3D | Manufactured E/H solution, both element orders and backends; nonzero E/H traces, invariance to normal input components, PEC/PMC, impedance, orientation and Floquet face/edge moments. |
| Elastic 2D | Both plane reductions; rigid modes and affine strain/stress including `sigma_zz`, traction, density loss, mass and impedance signs, in-plane example and reference/convergence. |
| Elastic 3D | Rigid and affine fields, six stresses, traction, mass and loss, normal-incidence impedance, thin-plate example, mesh convergence and both backends. |

Compare existing field files, integration and global-variable paths, sweep
summaries, filenames, selection mapping, metadata, errors, lifecycle/cancellation
and result resolution. Run representative end-to-end native Studies from disposable
models through the real preparation and result path, not only direct kernel
tests. Keep the known elastic plate convergence limitation visible. Verify that
unsupported features remain rejected rather than silently gaining behavior.

For performance, benchmark release builds on representative scalar, vector and
EM3D meshes with both applicable solver backends. Report assembly time,
factorization/solve time, peak memory, sparse nonzero count, and sweep reuse;
record baseline and machine. Investigate material regressions with profiles and
choose an agreed threshold before claiming parity; no fixed speedup is assumed.

**Completion requires:** all eight JSON packages execute via the common runner;
no scenario-ID numerical branches or scenario-specific Rust solvers remain in production;
the master matrix is sparse; old supported semantics and outputs pass parity
and independent tests; the JSON-only ninth-scenario test passes through actual
discovery and Study; build and packaging are reproducible; and documentation
and any necessary external contract reconciliation are explicitly resolved.
