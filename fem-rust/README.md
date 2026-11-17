# FullMeta FEM executables

`fullmeta-fem.exe` is the single FEM command entry point and solver executable. It performs common model preflight, prepares projects and Studies, and dispatches numerical requests through the generated scenario-module registry. Scenario packages are discovered through `src/scenarios/registry.json` and composed with the scenario-neutral base schema; scenario additions do not require profile declarations in `fem.schema.json`. Each scenario keeps its JSON contract and numerical runner together under `src/scenarios/`, while common project and stationary Study orchestration remains in `src/coordinator.rs` and `src/study_prepare.rs`. Geometry and meshing live under `src/mesh/`. The wave solvers support harmonic sweeps and field/global-variable output; thermal solves a stationary case with a temperature field and summary CSV. See the [FEM Rust and scenario interface guide](../docs/fem-rust-and-scenario-interfaces.md), [scenario contributor guide](src/scenarios/README.md) and [mesher guide](src/mesh/GUIDE.md).

FEM uses the public core, model and operation-contracts SDK adapters and their three DLLs. It embeds the application-owned `../native-operations.json`; it does not need platform implementation source or a supplied core Rust library.

Validation has two scopes: **model validation** checks the supplied model's declarations, and **run-time validation** checks generated data and numerical operations when they are used. A successful model validation does not guarantee that meshing or solving will succeed.

## Build FEM on Windows

### Build using the platform DLLs

Keep these paths under the same FullMeta root:

- `applications/fem/`, including FEM source, `Cargo.toml`, `Cargo.lock`, scenario contracts and `native-operations.json`.
- `platform/sdk-rust/core/`, `platform/sdk-rust/model/` and `platform/sdk-rust/operation-contracts/` (the public Rust adapters).
- `platform/core-rust/lib/fullmeta_core.dll`, `platform/model-rust/lib/fullmeta_model.dll` and `platform/operation-contracts-rust/lib/fullmeta_operation_contracts.dll`.

With Node.js 20.11 or later, the Windows x64 GNU Rust toolchain, GCC, and the normal third-party Cargo dependencies available, run from `fem-rust/`:

    node prebuilt-tools/build.mjs --offline --stage

Omit `--offline` if Cargo should download missing third-party packages. Omit `--stage` to leave the executable and DLLs in `target/dll-build/x86_64-pc-windows-gnu/release/`. With `--stage`, they are copied together into `applications/fem/.runtime/x86_64-pc-windows-gnu/release/`. Cargo may generate temporary Rust libraries for the SDKs and third-party crates; no prebuilt platform `.rlib` bundle or procedural-macro DLL is distributed.

FEM uses the three DLLs beside its executable as one set. If only part of that set is present, startup reports an error. If none is present, development builds can locate the libraries in the platform `lib/` directories. Explicit `FULLMETA_CORE_DLL`, `FULLMETA_MODEL_DLL` and `FULLMETA_CONTRACTS_DLL` overrides are intended for testing and can override this selection. Distribute the staged executable with all three matching DLLs when deploying FEM to a platform.

To refresh the DLLs from a full platform source checkout, run `node prebuilt-tools/prepare.mjs --offline` and then rebuild FEM. The platform `target/` directories are rebuildable caches and are not needed by a colleague building FEM.

This FEM-only build does not rebuild shared WASM or other applications and does not refresh the workspace-wide runtime fingerprint.

| Location | Responsibility |
| --- | --- |
| `platform/model-rust/src/schema.rs`, `context.rs` and `validation.rs` | Reusable schema validation, explicit schema contexts, generic role/annotation queries, reference bundling and ID checks. |
| `src/model_schema.rs` | Caller-owned FEM schema discovery and metadata interpretation, using the generic context API. |
| `src/model_validation.rs` | Common FEM model validation: schema-supported settings, references, expressions, units, dependencies and consistent geometry/material/boundary/mesh/Study/result declarations. Configured by deployed schema metadata and compiled into the dispatcher. |
| `src/scenarios/*.rs` | Scenario numerical solvers and run-time validation of prepared requests, generated mesh data, actual material/boundary samples, constraints and solves. |
| `src/mesh/` | Geometry compilation, Gmsh invocation and mesh artifact generation within the FEM executable. |
| `coordinator.rs`, `study_prepare.rs` | Common project lifecycle, mesh preparation and Study-specific orchestration, including stationary Studies. |
| `dispatcher.rs` | Validate the model, check runtime registration and select the executable before preparation or execution. |
| `preflight_client.rs` | Delegate direct scenario validation and raw-model commands to the dispatcher's common preflight; reuse its receipt for an unchanged model. |

The editor buttons use these scopes as follows:

| Action | Flow |
| --- | --- |
| **Validate** | Rust's generic command runner invokes schema-registered `fullmeta-fem --validate -`; native preflight reports structural, schema-declared semantic and scenario-registration diagnostics. |
| **Build mesh** | Common model validation, then the mesh module's geometry construction and mesh-generation checks in the stable `fem_runs/mesh` directory. |
| **Run study** | Common model validation, Study preparation and mesh preparation/reuse, then the selected module's run-time checks and numerical solve in the same executable. |

The editor's **Validate** action runs `fullmeta-fem --validate -` through the Rust server's generic command runner. Browser model WASM remains responsible for immediate local edit checks. Native validation accepts an unsaved model and checks its schema-declared rules, installation scenario registration and sibling model-ID uniqueness. The generic Rust command host passes the opened document path to the process as `FULLMETA_DOCUMENT`; the FEM validator owns the identity rule and sibling-document scan. Native validation does not prepare a solver request, generate a mesh or check solver executable availability. Saved-project validation remains available through the CLI. **Build mesh** does not run scenario solver validation; its geometry and meshing guards report failures encountered while generating the mesh. Scenario run-time validation includes actual periodic correspondence, element material coverage, sampled coefficients and denominators, conflicting numerical constraints, factorization failures, finite solutions and residuals.

`platform/model-rust` remains application-neutral. It validates the supplied JSON/schema and supplies generic navigation and query operations; it does not interpret FEM metadata. The dispatcher and compiled scenario modules share the native model-library dependency. Scenario modules use it for schema and metadata queries even though common whole-model validation runs in the dispatcher.

The Rust Kernel serves `/api/model/study/run` from the model schema's generic `ndjson-stdio` API contract and launches `fullmeta-fem.exe --server`. The schema binds the request's selected Study and model location to the process environment, and declares the completion event's output-file list for Rust to verify. Rust owns HTTP streaming, process tracking and cancellation; the FEM package owns model validation, Study selection, run records, mesh preparation and numerical execution. Its registered `--cancel-run` entry marks an interrupted run record as cancelled. Figure export uses `/api/model/action/saveFigure` with a schema-registered `json-stdio` call to `fullmeta-fem.exe --save-figure`: the browser renders PNG/JPEG, while the FEM executable validates datasets and writes PNG, SVG, or PDF output. Rust owns only route matching, request transport and JSON response framing. The CLI command `fullmeta-fem.exe <project-folder>` reads the saved project and performs the same validation, mesh preparation, solve, and output collection without opening the editor. The editor-only tree and graphics interactions are outside the coordinator. All scenarios use the common Physics schema and its Dirichlet, Neumann, impedance, and periodic-condition definitions; each solver declares which conditions it supports. EM3D accepts selected impedance-boundary expressions without requiring an input/output port pair or fixed measurement planes. Pointwise variable expressions are evaluated at integration points through the shared expression core, so model-defined global expressions can integrate solved fields with operators such as `intop(PW)` and define their own transmission/reflection observables. The `electric` mode solves the E-only curl–curl equation with first- or second-order Nédélec edge elements.

## Common Physics contract

`applications/fem/fem.schema.json#/$defs/shared/physicsInterface` owns the shared Physics object and editor annotations. Scenario schemas specialize its formulation and study requirements without copying its identity, domain-selection, boundary, or feature properties. The common Dirichlet, Neumann, impedance, and periodic actions use the same creation mechanics and schema-driven settings renderer for every scenario.

Each scenario schema declares `x-fem-physics.supportedBoundaryTypes`, `supportedFeatureTypes`, and `boundaryDefaultsByFormulation`. The defaults are keyed by serialized Physics formulation, including `pressure`, `Ez`, `Hz`, `electric`, and `T`; each condition entry supplies the quantities and units needed to create that condition. Scenario schema constraints and common FEM preflight enforce accepted quantities and model-level solver limits. Adding a scenario does not require adding a Physics-family branch to the shared JavaScript boundary creator.

The serialized model paths are retained: Dirichlet, Neumann, and impedance conditions are stored in `physics[].boundaries[]`. Periodic conditions are stored in `physics[].features[]`, using `periodicPairId` to reference a matched mesh pair in `mesh.periodicPairs[]`. A common condition definition does not imply numerical support in every solver. Thermal supports Dirichlet temperature, Neumann outward heat flux (including zero flux for insulation), and zero-phase periodic temperature equality. Its schema and metadata reject impedance and configure common preflight to reject nonzero periodic wave vectors.

## Run

Open a model through `platform/loader.html`, which mounts its selected Model Editor widget, and click **Run study** for a supported Model 2 scenario, or run `applications/fem/.runtime/x86_64-pc-windows-gnu/release/fullmeta-fem.exe <project-folder>` directly from the repository root. The editor passes its current model snapshot to `fullmeta-fem --server`; the CLI reads `projects/<project-folder>/fem_model.json`. The dispatcher validates the model, prepares or reuses its mesh in the project's shared `fem_runs/mesh` directory, selects the scenario module from the registered package, and writes Study outputs under a stable per-Study folder. Rerunning the same Study replaces that Study folder while retaining reusable geometry-hash mesh caches. Run `fullmeta-fem.exe <project-folder> --validate` to perform common preflight on the saved model without launching a solver or running a Study. Run `node scripts/build-runtime.mjs` from the repository root when a full rebuild of the dispatcher, mesher, shared WASM modules and registered application targets is needed. The shared quantity core accepts legacy four-exponent JSON vectors by appending a zero temperature exponent.

The combined parameter sweep limit is 10,000 cases, configured by `x-fem-capabilities.study.maximumSweepPoints` in the model schema. The shared runner reads that declaration for both scalar-wave and EM3D runs; it has no separate 32-case ceiling. Independent parameter ranges multiply to give the number of cases.

The shared runner batches a frequency sweep into one scenario-runner process, so a solver can retain reusable state such as symbolic matrix analysis across cases. It reports case progress independently of physics diagnostics. Its `FULLMETA_PROGRESS ` lines carry JSON with `event` (`case-start` or `case-complete`), zero-based `caseIndex`, `totalCases`, and `frequencyHz`. A completion is emitted only after that case succeeds. The coordinator translates these events into browser progress updates and writes solver stdout to the run log as lines arrive.

## Add a scenario through its JSON contract

Scenario-neutral FEM model structure is composed from the generic base schema
and the registered JSON packages in `fem-rust/src/scenarios/`. Adding a
profile does not require editing `fem.schema.json` or adding a profile branch
to the shared dispatcher, mesher, editor loader, or schema composer. The
one-time runner hook is described in
[`src/scenarios/README.md`](src/scenarios/README.md).

For a new `thermal-3d` profile:

1. Add `src/scenarios/thermal3d.json` and register its ID and
   sibling filename in `src/scenarios/registry.json`. Put its profile dimension,
   physics declarations, primary and derived fields, material coefficients,
   supported Physics flags, study declarations, solver runtime, and result
   capabilities in the package contract.
2. In `schemaDocument.x-fem-scenario`, set `solverModule` to `thermal3d`.
   This is the only routing value the shared dispatcher needs; it does not
   inspect the profile ID or infer physics from the dimension.
3. Add `src/scenarios/thermal3d.rs` with the common module entry point.
   It receives the prepared numerical request and owns the scenario's
   numerical method and checks on generated mesh and solver data. The shared
   stationary Study path owns project preparation, run records, and outputs.
4. Register the solver program in the scenario JSON and point its executable
   to the built `fullmeta-fem` dispatcher. `fem-rust/build.rs` discovers the
   module from the registry, checks that the source exists, and generates the
   module table at build time. The application runtime build already builds
   the dispatcher, so no extra Cargo target, feature, artifact mapping, or
   package script entry is needed.
5. Rebuild the FEM runtime and validate the scenario contract, project
   structure, mesher handoff, solver run, and result mapping.

The editor loader and Rust schema consumers discover the same registered
scenario package data. The mesher reads dimension, mesh, material, field, and
Physics declarations from the composed schema graph. The dispatcher validates
and selects the declared solver module. The module receives a prepared solver
request in a child process; the shared server retains progress-event validation
and the run lifecycle. A scenario using the common `stationaryPlan` or
`frequencyPlan` preparation does not need its own project-level `--server` or
`--prepare-study` handler.

The editor derives Study type from `x-fem-scenario.studyType`, and generic result handling derives
stationary behavior from the saved Study type. Declare extra solver-consumed
Study properties and defaults in `x-fem-scenario.studyFingerprint`; use
`x-fem-scenario.resultDefaults` for a nonstandard frequency CSV column or a
stationary field label. This keeps the shared editor and result resolver from
needing new profile-specific branches.

Built-in and future solvers all use the same registered dispatcher executable.
The module hook keeps physics source scenario-specific while shared runtime
code remains independent of new profile IDs. Any genuinely new common model
or output capability still requires a deliberate shared contract change. The
generic authored `fem.schema.json` remains unchanged for scenario additions.

## Thermal 2D

The thermal profile is available through the shared `fullmeta-fem` executable with triangular meshing, conductivity editing, common Dirichlet/Neumann actions configured for temperature/heat flux, periodic conditions, Stationary Studies, native validation and temperature plotting. The shared coordinator handles project lifecycle and stationary Study preparation; `thermal2d.rs` handles the prepared numerical solve. The dispatcher supports the following entries:

| Command | Behavior |
| --- | --- |
| `fullmeta-fem <project-folder>` | Validate, mesh and solve a saved project. |
| `fullmeta-fem <project-folder> --validate` | Validate the saved model without a Study run. |
| `fullmeta-fem --validate <model.json>` | Validate a model file. |
| `fullmeta-fem --validate -` | Validate stdin and return `fullmeta-model-validation/1`. |
| `fullmeta-fem --server` | Run the submitted model from stdin and emit editor NDJSON progress. |
| `fullmeta-fem --scenario-module thermal2d --solve <request.json>` | Solve prepared thermal mesh/model-map inputs. |

The shared dispatcher completes model preflight before project, preparation, and server operations. Scenario numerical dispatch is an internal child invocation of the same executable.

The full runtime build produces one FEM executable containing the registered scenario modules. To build it from the repository root, retain Cargo's default features:

```powershell
node scripts/build-runtime.mjs
applications/fem/.runtime/x86_64-pc-windows-gnu/release/fullmeta-fem.exe --validate path/to/thermal-model.json
applications/fem/.runtime/x86_64-pc-windows-gnu/release/fullmeta-fem.exe --scenario-module thermal2d --solve path/to/thermal-request.json
```

Cargo's disposable incremental cache is stored in `applications/fem/.build/`.
The runtime build stages the registered executable and required DLLs under `.runtime/`;
delete `.build/` whenever you want to reclaim its space, with the next build
recompiling dependencies.

The registered thermal schema and executable mapping are scenario metadata. The schema supplies thermal model constraints and `x-fem-validation` options to common preflight. The thermal module retains guards on constitutive finiteness, boundary correspondence, mesh-map consistency, connected-component heat compatibility, temperature references and the assembled solve. Adding or updating a runner rebuilds the single dispatcher executable. Browser model WASM continues to perform supplied-schema structural/edit checks.

The kernel solves `-div(k grad(T)) = 0` with linear triangular elements and constant isotropic conductivity on each material selection. Conductivity, prescribed temperature and prescribed outward heat flux accept constant parameter expressions through the shared quantity core, with units compatible with `W/(m*K)`, `K` and `W/m^2`. Spatial material/boundary expressions are explicitly unsupported. Conductivity and temperature signs are unrestricted; invalid divisions, non-finite systems, factorization failures and residual errors still fail the solve. Positive outward flux removes heat; unspecified exterior edges are insulated. A Physics periodic condition references an enabled matched mesh pair and enforces equal temperatures at paired nodes. Its wave vector must be zero; prescribed temperature or flux cannot also be assigned to periodic edges. Each connected component without prescribed temperature uses the stationary Study's optional `meanTemperature` quantity, default `0 K`, as an area-weighted mean constraint. Such a component must have zero net prescribed heat flux within numerical tolerance; the reference does not supply missing heat. Extra nullspaces caused by degenerate constitutive assignments can still cause factorization failure. Shared corners must have consistent prescribed temperatures. The solver reduces periodic degrees of freedom before solving and writes the expanded temperature field at every mesh node. Thermal global/pointwise variables, parameter sweeps, volumetric heat sources and transient studies remain unavailable. Optional schema resource limits use `study.solverResourceLimitsByProfile.<profile>.maximumNodes` and `maximumCells`; absent limits impose no arbitrary mesh-count cap.

`--solve` consumes a prepared model, triangular FullMeta mesh, and dimension-2 `fullmeta-mesh-map/6` sidecar. An example request is:

```json
{
  "schema": "fem-solver-request/1",
  "solver": "thermal-2d",
  "studyId": "study1",
  "fieldFormulation": "T",
  "elementOrder": 1,
  "constitutiveQuadrature": "standard",
  "solverBackend": "rivrs",
  "solverOrdering": "amd",
  "solverThreads": 1,
  "modelFile": "thermal-model.json",
  "meshFile": "thermal.mesh",
  "modelMapFile": "thermal-model-map.json",
  "outputDirectory": "thermal-output"
}
```

Paths are relative to the request file unless absolute; `studyId` must identify a stationary study in the model. The executable writes `temperature.json` under `outputDirectory`, using schema `fullmeta-thermal-field/1`. It contains nodal temperature in kelvin, per-cell heat flux and conductivity, and solver diagnostics. Per-cell flux preserves jumps at material interfaces. Full project runs also record their model snapshot, mesh inputs, progress and summary CSV under `projects/<project>/fem_runs/`; result adapters resolve the stored `temperature.json` for browser plotting.

## Focused checks

The commands below are available for an explicit verification step.

Run `node --test tests/fem-dispatcher.test.js` from the repository root after building the dispatcher to check registration, routing, stdin forwarding, exit statuses, and rejected registrations. The test installs a synthetic scenario into an isolated fixture and checks that the dispatcher's file hash stays unchanged; it does not run a numerical FEM solve.

Run `node --test tests/native-model-validation.test.js` after building the dispatcher to check stdin validation against unsaved copies of six project models, structural and semantic failures, and the JavaScript test adapter's subprocess/protocol handling. These checks leave saved project files unchanged and do not run Studies.

Run `node --test tests/model-wasm-neutrality.test.js` after rebuilding the model WASM to check caller-supplied schemas, external document references, generic edit transactions, and the absence of embedded FEM contract markers.

## Existing wave solver notes

The default backend for the E-only curl–curl system is RIVRS sparse LDLᵀ, enabled by the default Cargo feature. Set `solverBackend` to `"faer"` in a request to select Faer's complex sparse LU. RIVRS uses a 2N real symmetric representation for complex-symmetric matrices and an exact 4N symmetric augmentation when Floquet reduction produces a nonsymmetric operator.

EM3D uses a collection of selected impedance boundaries with real finite nonzero impedance expressions. An incoming boundary supplies a dimensionless complex incident factor and a polarization vector with one unitful quantity per primary-field component; outgoing boundaries omit both. Boundary count, source count, propagation axis and measurement-plane placement are not fixed by the solver. The supplied source expression defines its spatial phase and must agree with any selected Floquet trace relation. Wave schemas enable nonzero real Floquet vectors. Model-defined variables and integration operators determine reported observables; no built-in port-mode normalization is implied. The current numerical implementation remains isotropic and harmonic; radiation/PML boundaries, anisotropic materials and time-dependent PDEs require additional implementations. Geometry compilation and Gmsh meshing are handled by the `src/mesh/` module.

Historical request examples and their reference datasets are archived under `backup/data-archive-2026-10-14/`. Their paths use the former `data/` layout, so they are preserved for reference and are not directly runnable from the current project layout. Use the model editor's **Run study** action for the current workflow.
