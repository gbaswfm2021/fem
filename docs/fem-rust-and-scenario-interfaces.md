# FEM Rust and scenario interfaces

This guide describes the current boundary between the FullMeta editor, the
`fem-rust` executable and its scenario-specific numerical runners. It is an
interface guide for contributors and integrators. The [runtime specification](../specification/RUNTIME.md)
and [runtime integration contract](../specification/contracts/runtime-integration-boundary.md)
define the required behavior and acceptance evidence.

## Components and ownership

```text
FEM editor and schema
    |  registered commands and native operations
    v
FullMeta Kernel  -- starts and monitors -->  fullmeta-fem
                                               | common preflight, Study preparation,
                                               | run records and progress
                                               +--> mesh module --> Gmsh / Gmsh SDK
                                               |
                                               +--> scenario runner in a child
                                                    fullmeta-fem process
```

The application has one staged FEM executable on the current Windows target:
`applications/fem/.runtime/x86_64-pc-windows-gnu/release/fullmeta-fem.exe`.
The dispatcher, mesher, shared orchestration and registered scenario runners
are compiled into it. Gmsh remains a separate third-party executable and SDK
under `fem-rust/vendor/`. The schema registers `fem` and `mesh` program IDs,
and scenario contracts register solver program IDs; these FEM and solver
registrations resolve to the same executable. Their allowed entries still
constrain which commands the Kernel may invoke.

The editor owns model editing and presentation. The Kernel owns generic route
transport, process tracking, cancellation and checks of declared output paths.
FEM owns model preflight, scenario selection, Study preparation, meshing,
numerical execution, run records and result resolution. A successful model
validation does not establish that a mesh or solve will succeed.

## Editor and native command interface

The application schema declares the routes, program registrations, entry
arguments, storage and expected artifacts. The registered operation contracts
provide the structured forms below. Integrators should use the registered
entry and transport rather than guessing a command from a profile ID.

| Purpose | Entry | Input and output |
| --- | --- | --- |
| Inspect native operations | `--describe-operations` | Writes the FEM and mesh operation descriptions as JSON. `--describe-operations mesh` selects the mesh description. |
| Validate a model | `--validate -` | Reads the submitted model on standard input and returns model-validation diagnostics; it does not mesh or solve. |
| Check geometry | `--operation-check` | `mesh.check`: JSON request containing `model` and optional `componentId`; JSON result or structured diagnostics. |
| Compile geometry | `--operation-compile` | `mesh.compile`: JSON request containing `model`, optional `componentId` and optional `outputDirectory`; JSON result with geometry/map information or diagnostics. |
| Prepare a Study | `--prepare-study` | `fem.prepareStudy`: JSON request containing the model and selected `studyId`; returns a Study plan or supported prepared-request envelope. |
| Run a Study | `--server` | `fem.studyRun`: model on standard input; newline-delimited JSON events on standard output. The Kernel supplies the selected document, Study, operation and storage context. |
| Manage a run | `--run-status`, `--cancel-run` | Registered run-status and cancellation entries use the operation context and the respective request/response contracts. |
| Read or remove results | `--resolve-result`, `--clear-results` | Registered result-resolution and cleanup entries use their declared requests and checked document/run context. |
| Export a figure | `--save-figure` | Registered JSON request for the selected checked plot and output file. |

The executable also exposes `check`, `compile`, `preview`, `mesh`, `build`
and `--mesh-only` for geometry and meshing workflows. The
[mesher guide](../fem-rust/src/mesh/GUIDE.md) describes those entries and the
mesh artifacts. Refer to the [runtime contract](../specification/contracts/runtime-integration-boundary.md)
for routes, required context, checked outputs and exact operation obligations.

A Study's terminal success is not just a process exit code: its declared
output files must be present and checked. The FEM dispatcher validates worker
events and handles the terminal outcome; the Kernel streams events and checks
declared outputs. Consult the runtime contract for progress, cancellation,
recovery, result freshness and failure behavior.

## Scenario package interface

The registry and scenario packages are in
[`fem-rust/src/scenarios/`](../fem-rust/src/scenarios/). The registry is a
`fullmeta-fem-scenario-registry/1` JSON document with a `packages` array. For
example, the existing thermal entry is:

```json
{ "id": "thermal-2d", "file": "thermal2d.json", "default": false }
```

The model stores the profile ID, `thermal-2d`. The registry points to
`thermal2d.json`, which declares `schemaDocument.x-fem-scenario.solverModule`
as `thermal2d`; its runner is `thermal2d.rs`. Remove hyphens from the profile
ID to form the JSON and Rust filename stem. The build script checks the
registered contract identity, module name, matching filenames and source
existence. It generates the dispatcher module table at build time.

Each `fullmeta-fem-scenario/1` JSON contract identifies its `id` and `version`
and supplies `schemaDocument`, `capabilities` and `schemaContributions`.
Together these describe the profile, fields and formulations, material
properties, supported Physics conditions, mesh and Study capabilities,
starter defaults, result metadata, validation/UI contributions and runtime
program registration. See the five existing contracts and the
[runtime specification](../specification/RUNTIME.md#runtime-scenario-discovery)
for the declared package shape. A declaration does not implement a numerical
method or prove that the scenario has passed acceptance.

`schema-documents.js` loads the registry and composes the contracts with the
base schema for the editor. Native schema consumers use
`fem-rust/src/scenario_runtime.rs` to compose the registered contracts for the
dispatcher and mesher. They consume the same source files but compose them
independently. A shared content-addressed effective-schema artifact, persisted
contract hashes and byte-identical cross-language composition are not yet
established. An invalid registry entry or package currently fails loading for
that consumer.

## Scenario runner interface

Each registered Rust runner implements this crate-level entry point:

```rust
pub(crate) fn run(arguments: &[String]) -> Result<i32, String>
```

The common dispatcher resolves the selected profile's declared module and
launches the same executable in private `--scenario-module <module>` mode.
The generated registry selects the compiled module, whose `run` function
receives the remaining arguments and inherits the child process's environment
and standard streams. This is an internal Rust interface, not a stable binary
plugin ABI; a new or changed runner must be compiled into the executable.

The common layer handles the project-level entry, model preflight, selected
Study, mesh preparation/reuse, run lifecycle and output collection. The runner
owns the scenario's equations, discretization and checks on prepared numerical
input, actual mesh data, coefficients, constraints, solution and residuals.
Existing runners use `scenario_solver_cli` for prepared request handling:

- `--solve <request.json>` validates and solves one prepared request.
- `--sequence <manifest.json>` processes a list of frequency requests and can
  reuse solver state across cases; it emits `FULLMETA_PROGRESS ` case events.
- Other supported arguments can be delegated to the shared operation handler.

The solver request uses the `fem-solver-request/1` schema declared in the
effective FEM schema. Common validation checks that `request.solver` matches
the runner; frequency requests handled by this helper contain one positive,
finite frequency case. Scenario-specific input requirements and supported
outputs remain the runner and contract's responsibility. See
[`scenario_solver_cli.rs`](../fem-rust/src/scenario_solver_cli.rs),
[`solver_request.rs`](../fem-rust/src/solver_request.rs) and the
[solver capability contract](../specification/contracts/solver-capability-contract.md)
for details.

## Changing or adding a scenario

1. Add or edit the contract and matching Rust runner in `src/scenarios/` and
   register the ID and JSON filename in `registry.json`.
2. Declare a safe matching `solverModule`, supported capabilities, preparation
   and result behavior, and an allowed solver program entry in the contract.
3. Rebuild and stage `fullmeta-fem` using the [FEM-only build guide](../fem-rust/README.md#build-fem-on-windows).
   Reload the editor and restart native processes when registry or contract
   data changes. A runner or build-time module-table change requires a rebuild.
4. Verify contract loading, model validation, mesher handoff, the actual solve,
   results and relevant rejected inputs for the identified candidate. A new
   shared model semantic, element family or output format may also require a
   deliberate common-contract and implementation change.

The [scenario contributor guide](../fem-rust/src/scenarios/README.md) describes
the source layout and contribution steps. The current five registered profiles
are listed in the [project guide](README.md#physics-profiles-and-runtime).
