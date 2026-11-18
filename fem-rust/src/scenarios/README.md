# FEM scenario packages

Each scenario has a JSON package in this directory. Packages contribute the
editor schema, model and Study metadata, and numerical declarations used by
the FEM runtime. Remove hyphens from a profile ID to form its package filename:
for example, `thermal-2d` is stored in `thermal2d.json`.

The generic execution capability `scalarSimplexP1` supports the stationary
and harmonic scalar paths described below. The `thermal-2d`, `thermal-3d`,
`acoustic-2d`, `acoustic-3d`, and `em-2d` packages use it in production.
The `elastic-2d` and `elastic-3d` packages declare `vectorSimplexP1` for
harmonic vector execution. The `em-3d` package declares
`orientedTetrahedronNedelec` for configured-frequency oriented edge-element
execution. The former Rust scenario implementations have been retired; the
production dispatcher selects generic execution from each package declaration.

## Generic scalar execution

A compatible package declares
`schemaDocument.x-fem-scenario.execution.kind: "scalarSimplexP1"` instead of
`solverModule`. It supplies the relevant formulation and element, weak and
boundary forms, essential constraints, material symbol bindings, selection
plan, recovery, result plan, and the appropriate `preparedScalar` or
`preparedFrequencyScalar` input, mesh and validation policies. Stationary
problems also declare nullspace treatment. These declarations are interpreted by the
shared solver; numerical coefficients and sources do not require a Rust
scenario switch.

The currently exercised production vocabulary covers real, parameter-only
stationary and complex harmonic scalar P1 triangles and tetrahedra, declared scalar or Cartesian
boundary traces, sparse global assembly, and declared recovery and result
serialization. Periodic node matching and phase policy are declared in JSON:
the 2D thermal package permits zero-phase matching, while the 3D thermal
package rejects periodic pairs; harmonic packages may declare complex phases.
Unsupported declarations fail explicitly. A
new JSON-only scenario can use the existing generic route when its requested
mathematics, input formats, boundary operations, constraints, and outputs are
all expressible by the supported declarations. A new numerical operator or
other unsupported capability still requires a shared implementation and
tests before a package can use it.

The optional `execution.recordModuleLabel` is a historical run-record
compatibility label. Its exact run-record placement follows the declared
Study lifecycle; `solver.execution` identifies the generic execution. The label
is never used to select or load a Rust module. New JSON-only packages need no
historical label.

## Generic harmonic vector execution

`vectorSimplexP1` uses the same declared frequency Study preparation and sparse
execution lifecycle for P1 triangle or tetrahedron vector fields. A package
also declares `preparedVector` policies, constitutive coefficient matrices,
vector weak and boundary forms, component constraints, tensor recovery and
result projections. The elastic packages exercise this path; support for other
formulations is limited to the mathematical operations already implemented.

## Generic oriented edge execution

`orientedTetrahedronNedelec` interprets configured-frequency declarations
for first- and second-order tetrahedral Nédélec elements, including oriented
edge and face moments, complex periodic weights, declared boundary and volume
forms, recovery and result projections. The global matrix remains sparse.
New packages can use this route when their required mathematics and request
semantics are expressible by the implemented declarations. Coordinate frames
must preserve orientation. Second-order periodic face mappings must preserve
the declared moment directions without mixing the two face modes; unsupported
transformations are rejected.

## JSON authoring reference

Start with the closest existing package and retain the declarations required by
its execution path. The tables below are a guide to the implemented contract;
the linked packages show complete examples, including schema and output data.

| Capability and request | Starting package | Numerical and policy declarations |
| --- | --- | --- |
| Stationary `scalarSimplexP1` / `stationaryPlan` | [thermal2d.json](thermal2d.json), [thermal3d.json](thermal3d.json) | `preparedScalar`, `weakForms`, `boundaryForms`, `essentialForms`, `recoveries`, `materialBindings`, `selectionPlan`, `boundaryInputs`, `nullspaces`, `resultPlans`. Declare the nullspace/mean policy needed by an unanchored problem. |
| Frequency `scalarSimplexP1` / `frequencyPlan` | [acoustic2d.json](acoustic2d.json), [acoustic3d.json](acoustic3d.json), [em2d.json](em2d.json) | `preparedFrequencyScalar` keyed by formulation, plus `weakForms`, `boundaryForms`, `essentialForms`, `recoveries`, `materialBindings`, `selectionPlan`, `boundaryInputs`, and `resultPlans`. |
| Frequency `vectorSimplexP1` / `frequencyPlan` | [elastic2d.json](elastic2d.json), [elastic3d.json](elastic3d.json) | `preparedVector`, `weakForms` including `matrixCoefficients`, `boundaryForms`, `essentialForms`, `tensorRecoveries`, `materialBindings`, `selectionPlan`, `boundaryInputs`, and `resultPlans`. |
| Configured frequency `orientedTetrahedronNedelec` / `configuredFrequencyRequest` | [em3d.json](em3d.json) | `preparedEdge` keyed by formulation, `weakForms`, `boundaryForms`, `essentialForms`, `recoveries`, `materialBindings`, `materialCoefficients`, `selectionPlan`, `boundaryInputs`, and `resultPlans`. |

Every package also needs its `contractVersion`, unique `id`, `version`,
discovery metadata as appropriate, and a `schemaDocument` defining its
profile, model and Study contracts. Set
`schemaDocument.x-fem-scenario.execution.kind` to the chosen capability and
declare the matching Study preparation, solver program entries, material and
field catalogs, result defaults, and Study fingerprint. Existing packages also
show their `capabilities` and `schemaContributions`. Do not set
`solverModule` together with `execution`; a new package does not need a Rust
scenario file or a compatibility label.

Within `weakForms`, specify each element's cell, family, field components and
supported orders, and express the test/trial operators and coefficients using
the implemented vocabulary. Bind model material symbols and units through
`materialBindings`; define boundary input shapes, units, forms and essential
constraints with their corresponding declarations. `selectionPlan` maps model
selections to mesh tags and declares coverage and duplicate policies. Recovery
and `resultPlans` specify the fields and outputs. Coefficients, validation and
output mappings must agree with the selected formulation and schema.

Do not omit the execution-policy safeguards when adapting a template:

- For scalar cases, inspect the `preparedScalar` topology, determinant and
  periodic phase/mapping policies, real and recovery tolerances, material
  sampling, residual check and auxiliary-value validation. For harmonic scalar
  cases, check `preparedFrequencyScalar` mesh and topology, `volumeInputs`,
  `nonzeroInputs`, `boundaryQuadrature` and `fieldFamilies`.
- For vector cases, configure `preparedVector.supportedQuadrature`,
  `formulationMatch`, topology, `boundaryQuadrature`, periodic policy, and
  boundary zero/minimum rules and output component mappings. The elastic
  templates currently declare `globalVariables: "ignore"`; do not infer
  general global-variable support from their output plans.
- For oriented edge cases, configure `preparedEdge.supportedQuadrature`,
  `componentCoordinates`, `coordinateFrame`, topology and facet policies,
  positive edge tags, material sampling, selection and integration mappings,
  boundary collections and source vectors, periodic tolerances, solve
  tolerance, field families and output/debug plans. The engine checks the
  declared coordinate convention and rejects orientation-reversing frames and
  unsupported periodic moment transformations.

These safeguards describe the intended model, not switches to relax merely to
make a test pass. Verify supported quadrature and formulation choices, declared
finite and nonzero input checks, boundary compatibility, selection coverage,
periodic consistency and result units for the new package. Unsupported
mathematics or request semantics require shared engine work before they can be
authored solely in JSON.

## Discovery and build

`node prebuilt-tools/scenario-index.mjs` generates `registry.json` from the
package files. An optional top-level `discovery` object supplies a unique
non-negative integer `order` and a boolean `default`; packages without an
order sort after ordered packages by ID. Exactly one package must be the
default. Generate the index after adding or editing packages and reload the
editor or restart native processes. `prebuilt-tools/build.mjs` runs the
generator automatically; `--check` checks the index without changing it.
Direct Cargo builds reject a stale index and print the regeneration command.

The common editor-facing server, preflight, meshing, run records and Study
lifecycle are shared by the generic execution paths.
Stationary Studies use `stationaryPlan`; frequency-domain preparation supports
the declared `frequencyPlan` and `configuredFrequencyRequest` contracts.

Scenario result and Study metadata belong in the package contract. Declare
solver-consumed Study fields and defaults through
`x-fem-scenario.studyFingerprint`, and use `x-fem-scenario.resultDefaults`
for result filenames, schemas and labels. A `frequencyPlan` package must also
declare a safe `resultDefaults.frequencyFieldPrefix`.

## Adding a scenario

1. Add a package named from the profile ID and declare its discovery, model,
   Study, runtime, numerical and result metadata.
2. For an existing supported generic capability, declare its `execution.kind`
   and the complete corresponding numerical contracts. No scenario `.rs`
   file is required. If the required mathematical operations are not yet
   supported, extend and test the shared generic capability before adding the
   new package.
3. Point the scenario's solver program ID to the existing FEM executable and
   declare its supported entries. Generate the index, then rebuild or restart
   the relevant runtime and reload the editor.
4. Verify schema composition and validation, meshing, numerical results,
   declared files, run metadata and failure cases using independent reference
   cases for the mathematical capabilities the package selects.

For the editor-to-native and dispatcher-to-runner boundaries, see the
[FEM Rust and scenario interface guide](../../../docs/fem-rust-and-scenario-interfaces.md).
