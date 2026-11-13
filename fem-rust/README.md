# FullMeta FEM executables

`fullmeta-fem.exe` performs native whole-model FEM preflight and dispatches to a scenario executable. Its selected schema comes from the model document or the schema path supplied by the Rust Kernel. The five built-in scenario schemas and their registrations live under `$defs.femProfiles` in the root schema; optional extension scenarios are discovered through its `$ref` graph. Executable paths come from the schema registry using its declared `pathType`; the standard deployment places the runtime executables together. The five solver executables (`acoustic2d.exe`, `acoustic3d.exe`, `em2d.exe`, `em3d.exe`, and `thermal2d.exe`) own their supported Study run and retain numerical and mesh-dependent checks. The wave solvers support harmonic sweeps and field/global-variable output; thermal solves one stationary case with a temperature field and summary CSV. The dispatcher lives in `src/dispatcher.rs`; `src/coordinator.rs` contains shared wave-run orchestration, and `src/thermal2d_project.rs` owns stationary project runs. Each scenario's numerical implementation lives in its corresponding module.

The FEM application package may distribute this `fem-rust` source tree while keeping the stable shared Rust crates out of the installer. The crate uses path dependencies on `platform/core-rust` and `platform/model-rust`; compiling changes therefore requires a compatible FullMeta source checkout with those crates. The bundled executables remain usable without the shared Rust source.

Validation has two scopes: **model validation** checks the supplied model's declarations, and **run-time validation** checks generated data and numerical operations when they are used. A successful model validation does not guarantee that meshing or solving will succeed.

| Location | Responsibility |
| --- | --- |
| `platform/model-rust/src/schema.rs`, `context.rs` and `validation.rs` | Reusable schema validation, explicit schema contexts, generic role/annotation queries, reference bundling and ID checks. |
| `src/model_schema.rs` | Caller-owned FEM schema discovery and metadata interpretation, using the generic context API. |
| `src/model_validation.rs` | Common FEM model validation: schema-supported settings, references, expressions, units, dependencies and consistent geometry/material/boundary/mesh/Study/result declarations. Configured by deployed schema metadata and compiled into the dispatcher. |
| `em2d.rs`, `em3d.rs`, `acoustic2d.rs`, `acoustic3d.rs` | Numerical solvers and run-time validation of prepared requests, generated mesh data, actual material/boundary samples, constraints and solves. |
| `thermal2d.rs` | Stationary conduction kernel, request/material/boundary conversion and mesh/numerical guards. |
| `dispatcher.rs` | Validate the model, check runtime registration and select the executable before preparation or execution. |
| `preflight_client.rs` | Delegate direct scenario validation and raw-model commands to the dispatcher's common preflight; reuse its receipt for an unchanged model. |

The editor buttons use these scopes as follows:

| Action | Flow |
| --- | --- |
| **Validate** | Rust's generic command runner invokes schema-registered `fullmeta-fem --validate -`; native preflight reports structural, schema-declared semantic and scenario-registration diagnostics. |
| **Build mesh** | Common model validation, then the standalone mesher's geometry construction and mesh-generation checks. |
| **Run study** | Common model validation, Study preparation and mesh preparation/reuse, then the scenario executable's run-time checks and numerical solve. |

The editor's **Validate** action runs `fullmeta-fem --validate -` through the Rust server's generic command runner. Browser model WASM remains responsible for immediate local edit checks. Native validation accepts an unsaved model and checks its schema-declared rules, installation scenario registration and sibling model-ID uniqueness. The generic Rust command host passes the opened document path to the process as `FULLMETA_DOCUMENT`; the FEM validator owns the identity rule and sibling-document scan. Native validation does not prepare a solver request, generate a mesh or check solver executable availability. Saved-project validation remains available through the CLI. **Build mesh** does not run scenario solver validation; its geometry and meshing guards report failures encountered while generating the mesh. Scenario run-time validation includes actual periodic correspondence, element material coverage, sampled coefficients and denominators, conflicting numerical constraints, factorization failures, finite solutions and residuals.

`platform/model-rust` remains application-neutral. It validates the supplied JSON/schema and supplies generic navigation and query operations; it does not interpret FEM metadata. Both the dispatcher and scenario executables retain their native model-library dependency. Scenarios use it for schema and metadata queries even though common whole-model validation now runs in the dispatcher.

The Rust Kernel serves `/api/model/study/run` from the model schema's generic `ndjson-stdio` API contract and launches `fullmeta-fem.exe --server`. The schema binds the request's selected Study and model location to the process environment, and declares the completion event's output-file list for Rust to verify. Rust owns HTTP streaming, process tracking and cancellation; the FEM package owns model validation, Study selection, run records, mesh preparation and numerical execution. Its registered `--cancel-run` entry marks an interrupted run record as cancelled. Figure export uses `/api/model/action/saveFigure` with a schema-registered `json-stdio` call to `fullmeta-fem.exe --save-figure`: the browser renders PNG/JPEG, while the FEM executable validates datasets and writes PNG, SVG, or PDF output. Rust owns only route matching, request transport and JSON response framing. The CLI command `fullmeta-fem.exe <project-folder>` reads the saved project and performs the same validation, mesh preparation, solve, and output collection without opening the editor. The editor-only tree and graphics interactions are outside the coordinator. All scenarios use the common Physics schema and its Dirichlet, Neumann, impedance, and periodic-condition definitions; each solver declares which conditions it supports. EM3D accepts selected impedance-boundary expressions without requiring an input/output port pair or fixed measurement planes. Pointwise variable expressions are evaluated at integration points through the shared expression core, so model-defined global expressions can integrate solved fields with operators such as `intop(PW)` and define their own transmission/reflection observables. The `electric` mode solves the E-only curl–curl equation with first- or second-order Nédélec edge elements. An experimental `firstOrderEH` mode solves a coupled first-order E–H block system using first-order Nédélec elements for each field; see [the formulation note](../docs/first-order-eh-formulation.md).

## Common Physics contract

`applications/fem/fem.schema.json#/$defs/shared/physicsInterface` owns the shared Physics object and editor annotations. Scenario schemas specialize its formulation and study requirements without copying its identity, domain-selection, boundary, or feature properties. The common Dirichlet, Neumann, impedance, and periodic actions use the same creation mechanics and schema-driven settings renderer for every scenario.

Each scenario schema declares `x-fem-physics.supportedBoundaryTypes`, `supportedFeatureTypes`, and `boundaryDefaultsByFormulation`. The defaults are keyed by serialized Physics formulation, including `pressure`, `Ez`, `Hz`, `electric`, and `T`; each condition entry supplies the quantities and units needed to create that condition. Scenario schema constraints and common FEM preflight enforce accepted quantities and model-level solver limits. Adding a scenario does not require adding a Physics-family branch to the shared JavaScript boundary creator.

The serialized model paths are retained: Dirichlet, Neumann, and impedance conditions are stored in `physics[].boundaries[]`. Periodic conditions are stored in `physics[].features[]`, using `periodicPairId` to reference a matched mesh pair in `mesh.periodicPairs[]`. A common condition definition does not imply numerical support in every solver. Thermal supports Dirichlet temperature, Neumann outward heat flux (including zero flux for insulation), and zero-phase periodic temperature equality. Its schema and metadata reject impedance and configure common preflight to reject nonzero periodic wave vectors.

## Run

Open a model through `platform/loader.html`, which mounts its selected Model Editor widget, and click **Run study** for a supported Model 2 profile, or run `applications/fem/.runtime/x86_64-pc-windows-gnu/release/fullmeta-fem.exe <project-folder>` directly from the repository root. The editor passes its current model snapshot to `fullmeta-fem --server`; the CLI reads `projects/<project-folder>/fem_model.json`. The dispatcher selects an executable using the model's `scenario.profileId` and the selected Study's backend. The dispatcher validates the model before launch; the selected executable prepares or reuses its mesh, checks its numerical inputs, executes its supported Study type, then writes its run outputs under the project's `fem_runs/` folder. Run `fullmeta-fem.exe <project-folder> --validate` to perform common preflight on the saved model without launching a scenario or running a Study. Run `node scripts/build-runtime.mjs` from the repository root when a full rebuild of the dispatcher, mesh executable, wave executables, shared WASM modules and registered application targets is needed. The shared quantity core accepts legacy four-exponent JSON vectors by appending a zero temperature exponent.

The combined parameter sweep limit is 10,000 cases, configured by `x-fem-capabilities.study.maximumSweepPoints` in the model schema. The shared runner reads that declaration for both scalar-wave and EM3D runs; it has no separate 32-case ceiling. Independent parameter ranges multiply to give the number of cases.

The shared runner reports case progress independently of physics diagnostics. Its `FULLMETA_PROGRESS ` lines carry JSON with `event` (`case-start` or `case-complete`), zero-based `caseIndex`, `totalCases`, and `frequencyHz`. A completion is emitted only after that case succeeds. The coordinator translates these events into browser progress updates and writes solver stdout to the run log as lines arrive.

## Add a scenario executable

A new scenario supplies its schema constraints/metadata and numerical implementation. If its rules fit the deployed common FEM contracts, it can be compiled independently without rebuilding shared WASM, the dispatcher or other scenario executables. New common rule mechanisms still require a shared runtime update. Direct scenario commands delegate whole-model preflight to the deployed dispatcher; schema/metadata queries use the root model schema and its optional extension imports, with no central Rust scenario-validation catalog.

To expose that scenario through the dispatcher and editor, add its schema to the root model schema's imports and register it with the FEM browser package:

1. Create a scenario schema under `applications/fem/extensions/`. Its `x-fem-scenario` owns `profile` (physics, dimension, formulations, mesh families, editor options and UI profile reference), `fields.formulations` (primary, derived and zero field variables), `materials` (name and property declarations), and `sweepResultQuantities`. Common schema structures remain in the main model schema. Use a unique profile ID and keep schema `$ref` values resolvable within the application package.
2. Import the scenario schema through the selected root model schema's `$ref` graph. Both the browser and native callers discover built-in scenarios from `$defs.femProfiles` and optional extensions from those imports.
3. Register the executable once in the root schema's `x-fullmeta-runtime.externalPrograms`, then refer to its ID from the scenario schema's `x-fem-capabilities.study.solverRuntimesByProfile`. For example, a `custom-2d` backend entry can use `{ "backend": "custom", "kind": "executable", "programId": "solverCustom2d", "solveArgument": "--solve" }`; `backend` must match the enclosing map key. Add `sequenceArgument` only if the executable implements normalized request sequences. The backend comes from the selected Study's `solver.backend`, then the profile's `solverSettingsByProfile[profileId].defaults.backend`, then the shared `solverDefaults.backend`.
4. Compose its profile constraints and supported Study types with the shared JSON model contract by adding its `$ref` import to `applications/fem/fem.schema.json`, and supply its UI profile. Reuse the common Physics shape and actions; declare supported conditions and formulation-dependent quantity units/defaults in the scenario schema's `x-fem-physics` metadata. The FEM caller adapter follows extension imports using the generic explicit-path loader; browser schema/capability discovery reads embedded profiles and imported extensions.
5. Register a result adapter in `x-fullmeta-runtime.resources.resultAdapters`, mapping its profile ID to a JavaScript module path from the FullMeta main folder. The `*` entry supplies the harmonic default; thermal uses `applications/fem/js/results/thermal-fields.js`. Adapters own artifact normalization, stored-file selection and result labels.
6. Register the executable and its build arguments in `applications/fem/runtime-build.json`; add an `artifacts` mapping when other runtime code needs to resolve it by role. The shared build script reads that plan through the application catalog. Add its binary name to the package list in `scripts/Build-Windows-FEM-Package.ps1` so the Windows FEM installer includes it. The executable must implement the scenario command interface described below, including delegation to common model preflight and its own numerical request guards.

The dispatcher and native scenario commands use the model's selected root schema and any recursively loaded extension `$ref` documents as the runtime registry. The root schema registers external executable names and permitted entries under `x-fullmeta-runtime.externalPrograms`; profile schemas select those programs by ID. The root also owns `$defs.studyType` and shared `x-fem-capabilities.study` execution rules; embedded and extension profile schemas contribute their `x-fem-scenario` metadata and capability declarations. The dispatcher no longer reads `application.json`; the browser and mesher use the same schema graph. Executable paths use the schema-declared `pathType` convention. Missing or malformed registrations fail before a solver launches.

`model_expression.rs`, `material.rs`, `study_prepare.rs`, `model_validation.rs`, `field_output.rs`, and the acoustic/EM solver output adapters use the FEM caller adapter in `src/model_schema.rs` for field/material declarations. That adapter caches an explicit `model-rust::SchemaContext` with the root schema, embedded profile documents, and optional extension documents, and reports missing or malformed metadata. Model validation passes the bundled schema explicitly; the generic library contains no FEM fallback or installed-document lookup. Role annotations identify profile/formulation values and shared plot/integration declarations. Profile dimensions and supported sweep results also come directly from this reader; callers do not name individual scenario schema files. The standalone mesher reads the same selected schema graph through its own `mesh-rust/src/schema_metadata.rs` and does not depend on the model library.

The scenario command interface accepts `<project> [--validate] [--study <id>] [--csv <filename>]`, `--validate -` with the model JSON on stdin, `--server` with the model JSON on stdin, or `--prepare-study` with a request envelope on stdin. The dispatcher performs validation and shared frequency/parameter preparation according to `study.preparationByProfile` in the schema capabilities; a declared `scenario` preparation mode delegates to its executable. Its working directory is the application checkout root. The selected numerical executable orchestrates meshing, solving, output and checks requiring numerical or mesh data; its exit status is returned by the dispatcher. Direct validation and raw-model entry points call `preflight_client`, which invokes `fullmeta-fem --preflight - --runtime <executable-name>` without redispatching to a scenario. An unchanged model launched by the dispatcher carries a model-hash receipt, avoiding repeated whole-model checks. Keep the dispatcher beside scenario executables for these direct commands. `--server` must emit the existing newline-delimited JSON event protocol for the editor. A solver can continue to support `--solve` and `--sequence` for normalized requests, as the current four executables do.

For `--validate -`, the dispatcher returns one JSON object; a direct scenario command delegates and returns the same report. Its form is `{ "schema": "fullmeta-model-validation/1", "validationKind": "model", "valid": true, "errors": [], "structureErrors": [] }`. `validationKind` is an additive scope marker; older version-1 reports may omit it. Both error arrays contain strings; `errors` contains the full model validation result, and `structureErrors` contains structural schema errors. A valid model exits with status 0; an invalid model returns `valid: false` and exits with status 1. Validation accepts an unsaved model and enforces the selected schema's Study cardinality without preparing a Study or checking the scenario executable's availability. The editor's **Validate** action uses this protocol through the local server and passes the current model without saving it or running a Study. Run-time failures are reported by the mesh or Study operation that encounters them.

`--scenario <profile-id>` can explicitly select or check a profile; the dispatcher consumes this option before launching the child. When the model declares a profile, the explicit value must match it. `fullmeta-fem --capabilities` reports the capabilities composed from the current files on disk.

Each new executable supports its own scenario schemas and study types. Browser WebAssembly receives schema documents from the application adapter for structural validation and model-edit checks; its binary contains no embedded FEM schema or scenario catalog. `applications/fem/schema-documents.js` derives in-memory profile documents from `$defs.femProfiles` and loads optional extension modules for structural validation, capabilities and editor schema resolution. Common preflight obtains profile choices and supported model rules from the selected schema. Scenario implementation modules retain numerical and mesh-dependent checks. A future `thermal-3d` extension supplies its own schema, binary, UI profile and result-adapter mapping; it does not require edits to the shared editor implementation in `platform/ui/widgets/model-editor/`, the generic Rust Kernel route host, or `fullmeta-fem` routing code, or a rebuild of the shared model WASM. Changes to shared runtime contracts or units still require rebuilding the affected shared libraries.

## Thermal 2D

`src/bin/thermal2d.rs` builds a separate executable around `src/thermal2d.rs` and `src/thermal2d_project.rs`. The thermal profile is available in the shared editor with triangular meshing, conductivity editing, common Dirichlet/Neumann actions configured for temperature/heat flux, periodic conditions, Stationary Studies, native validation and temperature plotting. It supports the complete project interface:

| Command | Behavior |
| --- | --- |
| `thermal2d <project> [--study <id>] [--csv <filename>]` | Validate, mesh and solve the saved stationary project. |
| `thermal2d <project> --validate` | Validate the saved model without a Study run. |
| `thermal2d --validate <model.json>` | Validate a model file. |
| `thermal2d --validate -` | Validate stdin and return `fullmeta-model-validation/1`. |
| `thermal2d --prepare-study` | Read the model/Study envelope from stdin and return one stationary preparation plan. |
| `thermal2d --server` | Run the submitted model from stdin and emit editor NDJSON progress. |
| `thermal2d --solve <request.json>` | Solve already prepared mesh/model-map inputs. |

The shared dispatcher completes model preflight before forwarding project, preparation and server commands. Its validation command returns the common report directly; invoking the thermal validation command directly delegates back to that same preflight service.

The full runtime build lists thermal explicitly. To build only this executable from the repository root, retain Cargo's default features:

```powershell
cargo +stable-x86_64-pc-windows-gnu build --release --offline --locked --target x86_64-pc-windows-gnu --target-dir applications/fem/.runtime --manifest-path applications/fem/fem-rust/Cargo.toml --bin thermal2d
applications/fem/.runtime/x86_64-pc-windows-gnu/release/thermal2d.exe --validate path/to/thermal-model.json
applications/fem/.runtime/x86_64-pc-windows-gnu/release/thermal2d.exe --solve path/to/thermal-request.json
```

The registered thermal schema and executable mapping are application metadata. The schema supplies thermal model constraints and `x-fem-validation` options to common preflight. The thermal executable delegates whole-model checks to the dispatcher and retains guards on constitutive finiteness, boundary correspondence, mesh-map consistency, connected-component heat compatibility, temperature references and the assembled solve. Rebuilding only its numerical implementation does not require recompiling the dispatcher or the four wave executables. Browser model WASM continues to perform supplied-schema structural/edit checks.

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

Run `node --test tests/native-model-validation.test.js` after building the dispatcher and all five scenario executables to check stdin validation against unsaved copies of six project models, structural and semantic failures, and the JavaScript test adapter's subprocess/protocol handling. These checks leave saved project files unchanged and do not run Studies.

Run `node --test tests/model-wasm-neutrality.test.js` after rebuilding the model WASM to check caller-supplied schemas, external document references, generic edit transactions, and the absence of embedded FEM contract markers.

## Existing wave solver notes

The default backend for the E-only curl–curl system is RIVRS sparse LDLᵀ, enabled by the default Cargo feature. Set `solverBackend` to `"faer"` in a request to select Faer's complex sparse LU. RIVRS uses a 2N real symmetric representation for complex-symmetric matrices and an exact 4N symmetric augmentation when Floquet reduction produces a nonsymmetric operator. The experimental mixed `firstOrderEH` formulation uses twice the global edge DOFs and selects Faer through its schema settings; its same-space Galerkin discretization has not established general stability or convergence.

EM3D uses a collection of selected impedance boundaries with real finite nonzero impedance expressions. An incoming boundary supplies a dimensionless complex incident factor and a polarization vector with one unitful quantity per primary-field component; outgoing boundaries omit both. Boundary count, source count, propagation axis and measurement-plane placement are not fixed by the solver. The supplied source expression defines its spatial phase and must agree with any selected Floquet trace relation. Wave schemas enable nonzero real Floquet vectors. Model-defined variables and integration operators determine reported observables; no built-in port-mode normalization is implied. The current numerical implementation remains isotropic and harmonic; radiation/PML boundaries, anisotropic materials and time-dependent PDEs require additional implementations. Geometry compilation and Gmsh meshing are handled by the separate root `mesh-rust/` project.

Historical request examples and their reference datasets are archived under `backup/data-archive-2026-10-14/`. Their paths use the former `data/` layout, so they are preserved for reference and are not directly runnable from the current project layout. Use the model editor's **Run study** action for the current workflow.
