# target application runtime integration contract

This document specifies the runtime interfaces for a fresh target application package. Resolve application paths inside `applications/{APP_ID}/` or to documented shared platform resources.

## Target application registration and storage

Give the target application folder a root `catalog.json` descriptor:

| Catalog field | Required value or purpose |
| --- | --- |
| `id` | `{APP_ID}` |
| `schema` | `applications/{APP_ID}/{SCHEMA_FILE}` |
| `runtimePackage` | `true` |
| `schemaLoader` | `applications/{APP_ID}/schema-documents.js` |
| `package` | `applications/{APP_ID}/package.js` |

The target application schema must declare `fullmeta-model-runtime/1` with this project layout:

| Storage property | Required value |
| --- | --- |
| `projectRootFolder` | `projects` |
| `modelFile` | `fem_model.json` |
| `runsDirectory` | `fem_runs` |
| `resultsDirectory` | `fem_results` |
| `reservedDirectories` | `["fem_geometry", "fullmeta_geometry"]`. The runs and results directory names are also reserved for generated data. |

The target application includes both in-editor **Create project from this schema** and launcher app-card **Create project** with the runtime installed. Both use the declared profiles and model factory. The application package binds the model UI to native APIs and supplies adapters for graphics and result data. [Launcher project lifecycle](create-project-interface-contract.md)

## Generic platform responsibilities

The shared platform provides these mechanisms when target application declares and calls them through the supported interfaces:

- Discover per-application `catalog.json` files, then load the selected schema graph and registered package modules.
- Validate model JSON and render schema-authored tree, settings, references, annotations, and actions.
- Dispatch registered commands through `POST /api/kernel/command/run`, enforcing executable registrations, allowed entries, workspace path checks, and declared outputs.
- Dispatch schema-registered JSON and NDJSON external-program APIs, stream NDJSON events, track operations, and handle cancellation.
- Create projects through `POST /api/project/create` with handler `kernel.workspace.project.create`.
- Delete a project through `POST /api/project/delete` with handler `kernel.workspace.project.delete`, after the launcher confirms the exact project folder.
- Serve checked workspace artifacts through `GET /api/kernel/artifact`.
- Provide generic graphics, plot, and table widgets when the application package registers widgets and supplies renderable data.
- Perform schema-declared project-scoped storage operations.

The platform does not infer target application semantics from field names or executable names. application-owned behavior must select the right model data, build each native request, map returned artifacts into the UI, and report domain-specific errors.

### Runtime compatibility

Use the shared [frontend provider contract](../../../../platform/docs/frontend-provider-contract.md) and retain `fullmeta-model-editor-api/18` as the expected FEM backend API value. Frontend/provider and native-operation identities are separate from this compatibility value. Verify the actual application `/api/model-editor/info` response and controlled mismatch cases below; a successful frontend build does not establish compatibility. Repair application-owned compatibility declarations, and report an incompatible fixed dependency rather than silently changing the expected contract.

Declare the expected values in `x-fullmeta-runtime.compatibility` and expose the resolved expectations through the package's `runtimeCompatibility()` interface. The shared editor compares them against `GET /api/model-editor/info` for the selected application and schema. Resolve each `capabilityPath` from the installed target application capability declaration. A healthy HTTP response is insufficient when a required value is missing or differs.

| Compatibility key | Capability path or fixed value | Expected value |
| --- | --- | --- |
| `apiContract` | `["editorApiContract"]` | `fullmeta-model-editor-api/18` |
| `capabilitySchema` | `["schema"]` | `fem-engine-capabilities/2` |
| `geometryArtifactVersion` | `["geometry", "artifactVersion"]` | `14` |
| `modelSchema` | `["modelSchema"]` | `fem-model-2` |
| `projectApiContract` | Fixed `value` | `fullmeta-project-api/5` |

When the compatibility request fails or a value differs, show the expected and reported values or the request error through the shared compatibility banner. Disable model writes and runtime actions until the check passes. Verify executable identity and capabilities separately at runtime acceptance.

For the controlled mismatch check, use a disposable model that has valid mesh and Study actions and an isolated browser test transport that intercepts only the target application `GET /api/model-editor/info` request. Keep candidate and shared platform files fixed throughout the check. Reopen the editor for each response so it performs a fresh compatibility check.

1. Capture the matching response. Verify the banner is hidden and that **Save** is available for a valid edit and **Build mesh** and **Run Study** are available when their normal prerequisites are met.
2. Change only `geometryArtifactVersion` to a different integer. Observe the banner identifying the differing expected and reported values. Verify that **Save**, **Build mesh**, and **Run Study** cannot dispatch their requests. Confirm that the disposable file and run storage are unchanged.
3. Restore the baseline response and omit only `projectApiContract`. Verify that the banner identifies the missing value and that the same actions stay disabled.
4. Remove the override and reopen. Verify the matching response and restored action availability. Record the exact response differences, banner text, and observed action states for each case.

Use a browser transport that can isolate the single response, such as a disposable same-origin test proxy that passes through every other request. If the available test surface cannot supply and observe the controlled responses, record that evidence gap. An API comparison by itself checks the response contract only.

## application-owned files and interfaces

The package signatures in this document remain in force. Application-owned JSX components compile to sibling `.js` and `.js.map`, and public catalog/provider entries load the compiled files. Use the one supplied local React runtime and the documented mount/dispose adapters; do not send raw JSX or bare npm imports to the browser. The identified FullMeta Rust host serves the compiled assets at its configured localhost origin. The build watcher publishes only the static frontend revision manifest and starts no competing server, proxy or backend route. Source maps, build-error recovery and same-session reload/fixture replay follow [Frontend provider contract](../../../../platform/docs/frontend-provider-contract.md).

| Target file/module | Required responsibility |
| --- | --- |
| `applications/{APP_ID}/catalog.json` | Register ID, schema, runtime-package flag, schema-loader module, and package module. |
| `applications/{APP_ID}/{SCHEMA_FILE}` | Declare model contract, storage, native program/tool registrations, routes, action bindings, artifact contracts, and result-adapter resources. Author it through Model Editor. |
| `applications/{APP_ID}/schema-documents.js` | Export `loadModelSchema(schemaUrl, applicationId)`; activate the same schema graph when a target application document is opened. |
| `applications/{APP_ID}/package.js` | Export `createApplicationPackage(options)`; provide a complete editor package, launcher profiles, `createModelDocument`, editor preparation, action bindings, resource resolution, and widget renderers. |
| `applications/{APP_ID}/js/…` | Preserve the opened document, project model/schema records for runtime use, build operation requests, manage routes/progress/cancellation, and normalize checked result data. |
| `applications/{APP_ID}/ui/…` | Provide application-owned scene conversion, graphics selection/highlight mapping, result adapters, and any custom UI renderers required by the schema. |
| `applications/{APP_ID}/runtime-build.json` | Declare repeatable native build/deployment inputs if the selected deployment requires them. |

The application package must satisfy the platform's Model Editor package contract, including the manifest, schema graph, editor UI declarations, graphics widget registrations, action bindings, renderer creation, tree action plans, resource resolution, and model preparation functions. Implementations must use the platform documentation linked below for exact signatures.

## Route and program contract

| User operation | Platform route/service | target application schema contract | target application package behavior |
| --- | --- | --- | --- |
| Create project from the installed target application schema | `POST /api/project/create` | `editorApi.projectCreate`, handler `kernel.workspace.project.create`, and registered runtime package | The Schema Editor uses the target application factory for the selected profile and opens the returned document. |
| Launcher app-card Create project | `POST /api/project/create` | `editorApi.projectCreate`, handler `kernel.workspace.project.create`, storage names and root | Provide launcher profiles and synchronous `createModelDocument({ modelId, profileId, folder })`. [Launcher project lifecycle](create-project-interface-contract.md) |
| Launcher app-card Delete project | `POST /api/project/delete` | `editorApi.projectDelete`, handler `kernel.workspace.project.delete`, and `storage.projectRootFolder` | The launcher confirms the exact folder, sends the project ID and registered schema context, then refreshes the project list. No target application-specific delete callback or route is required. |
| Create model in a project | `POST /api/model/create` | `editorApi.modelCreate`, handler `kernel.workspace.model.create`. Request fields `modelId`, `folder`, `file`, and `model`; response fields `folder`, `file`, and `modelId`. | Bind `modelEditorUi.newModel` to the factory and open the returned file. |
| Validate model | `POST /api/kernel/command/run` | Registry ID `fem`, entry `--validate` | Submit the active model/document context and display diagnostics. |
| Check/compile geometry | `POST /api/model/mesh/check` and `/api/model/mesh/compile` | Registry ID `mesh`, native operations `mesh.check` and `mesh.compile` | Submit the descriptor's typed input, bind the selected model and project run directory, and display native diagnostics. Keep the existing `check`/`compile` CLI entries available separately through the generic command service. |
| Build mesh | `POST /api/kernel/command/run` | Registry ID `mesh`, entries `mesh` and `build`, with runtime tool `gmsh` | Sequence the declared mesh workflow, verify outputs, update mesh visibility and run state. |
| Preview an existing mesh | `POST /api/kernel/command/run` | `editorApi.meshPreview`, handler `kernel.command.run`, registry ID `mesh`, entry `preview` | Select a matching mesh artifact and pass checked mesh/map data to the graphics adapter. |
| Run Study | `POST /api/model/study/run` | `editorApi.studyRun`, handler `kernel.program.ndjson`, registry ID `fem`, entry `--server`. Register the preparation and solver mapping for each profile. | Submit the selected Study and model snapshot, associate its durable run context, update the bottom Operation strip, and reconcile the checked terminal state. |
| Stop Study | `POST /api/model/operation/cancel` | Register `--cancel-run` on the same dispatcher. Bind the cancellation `runIdPointer` to the run ID in the start event. Forward the original document/schema and resolved run-storage environment. | Send the active operation ID and kind from the Operation strip. Observe worker-tree exit and the saved terminal run record. |
| Check Study worker and recover its run | `POST /api/model/study/status` | `editorApi.runStatus`, handler `kernel.program.json`, protocol `json-stdio`, registry ID `fem`, entry `--run-status` | Query the checked run and present its status. Request reconciliation only for a confirmed-gone worker whose record remains `running`. |
| Select a result scene | Generic Model Editor calls the package's `selectionActions({ loadResultAction })` function | Graphics scene declares `onSelect: "loadResult"`; package must return the same key `{ loadResult: loadResultAction }` | The host `loadResultAction` loads the selected recipe through the target application result adapter. |
| Read saved run records | `GET /api/model/runs?folder=<model-folder>` | `editorApi.modelRuns`, handler `kernel.workspace.model.runs`. Read the declared run storage. | Reconcile the saved run and identify eligible result artifacts for the open model. |
| Resolve field/global results | `POST /api/model/action/resolve` | `editorApi.resultResolve`, handler `kernel.program.json`, protocol `json-stdio`, registry ID `fem`, entry `--resolve-result`, `readOnlyDuringOperation: true` | Select the newest compatible completed run for the selected result recipe and normalize checked file references for adapters. The operation reads existing run records and artifacts and changes no project or run data. |
| Save a figure | `POST /api/model/action/saveFigure` | `editorApi.saveFigure`, handler `kernel.program.json`, protocol `json-stdio`, registry ID `fem`, entry `--save-figure` | Submit active result/dataset context and show the returned export path. |
| Load output data | `GET /api/kernel/artifact` | Restrict artifacts to declared project/run roots | Resolve the artifact URL and load it through the selected result adapter. |
| Clear Study results | `POST /api/model/runs/clear` | `editorApi.clearRuns`, handler `kernel.program.json`, protocol `json-stdio`, registry ID `fem`, entry `--clear-results` | Confirm the exact document, then refresh run and result views and report checked removal counts. |
| Delete mesh | Shared schema-declared storage action | `kernel.storage.removeMatchingDirectories` with the mesh-only manifest selector | Confirm the project and refresh the mesh view. |

The supplied figure export response names that relative export path `figure`,
alongside `actionRunId`, `state: "completed"` and a positive `bytes` count. Bind
and display those fields directly; a successful write must not be reported as
failed because a different response key was expected.

### Read access during an active document operation

The Kernel's per-document queue serializes mutations and long-running operations. It must also permit these checked read-only requests to complete while the same document has an active Study:

- `GET /api/model` reads the canonical saved model.
- `GET /api/model/runs` reads the document's saved run summaries.
- `GET /api/kernel/artifact` reads one checked workspace artifact.
- A schema-declared JSON program route may opt in with `readOnlyDuringOperation: true` only when its handler is `kernel.program.json` and its external program protocol is `json-stdio`.

An opted-in route must be semantically read-only: it may inspect the canonical model, terminal run records, and checked completed artifacts, but must not edit model JSON, run manifests, run files, project storage, or operation state. The Kernel verifies the schema route, selected application, HTTP method, handler, and protocol before bypassing the document queue. It continues to apply the route's normal document, schema, executable, and artifact path checks. An absent, malformed, mismatched, or unsupported opt-in stays in the normal queue. Save, mesh build, Study, cancellation, Clear Results, Delete mesh, figure export, and every unmarked route remain serialized with the active document operation.

`resultResolve` is the target application read-only program route. It selects only a compatible terminal completion and leaves an active or newer incomplete attempt untouched. It may return an earlier completed result while another Study is preparing or solving. Keep the same canonical document and schema headers so the route remains bound to the open project. The shared checked artifact service then returns the selected completed run's files.

The frozen storage selector matches static top-level manifest values within a project folder. Implement document-scoped Clear Results in the registered target application entry. Keep the normal opened-document context and match the request's canonical `folder` and `file` to that context before deletion. Use the schema-declared run storage and the same project run lock as Study execution. Enumerate and check manifest ownership under the lock. Reject links and escaped paths, and report any partial failure with the affected run IDs and counts. [Clear Results scope](../RUNTIME.md#clear-results-scope)

For `modelRuns`, sort by `/startedAt` descending. Project `runId` from `$directoryName`, `studyId` from `/study/id`, and `studyName` from `/study/name`. Project `artifacts`, `startedAt`, `completedAt`, `state`, and `error` from the corresponding manifest fields. The response contains `folder` and `runs`. Read liveness separately from the manifest projection. For a command-run request, pass the declared `registryId` and `entry`, checked `args`, workspace-relative project `cwd`, model snapshot on `stdin`, and the cancellable `operationId`.

For each external-program API, declare its protocol, registry ID, entry, request fields/environment, response content type, cancellation mapping, and expected output checks. The package builds the request; the Kernel launches only the registered executable and allowed entry.

### Study status and reconciliation

The status request identifies the canonical saved model with `folder` and `file`. When the run is known, send its `runId`, `studyId`, and `operationId`. On reopen, a document-scoped lookup may discover a run, then pin subsequent requests to that identity. Send `reconcile: false` for observation, which changes no manifests or locks. Treat an omitted flag as observation. The status-only request may use the shared `services.network.requestJson` option `documentContext: false` so it can finish during a Study. Keep the application and schema headers. The native entry verifies the canonical document, matching application and schema, schema-declared run storage, and run, Study, and operation identities before returning data or changing a manifest. This option applies to status only.

Return `available: false` when there is no matching run to recover. For a matching run return its identity, `state`, `liveness`, `progress`, `error`, `startedAt`, and `completedAt`. A worker is `active` only when its OS process identity, including its start marker, matches the saved identity. A missing process or a reused PID is `gone`. An inconclusive check is `unknown`. An already-terminal record reports `finished`. Use an explicit `reconcile: true` to persist the first confirmed interruption. Guard the transition against concurrent completion or cancellation and preserve an existing terminal record. [Stop and recovery](../RUNTIME.md#stop-and-recovery)

For **Last finished run**, support a read-only lookup with `folder`, `file`, `studyId`, `terminalOnly: true`, and `reconcile: false`. Filter by canonical document, Study ID, and terminal state before ranking. Return the selected record's saved timestamps and identity through the same checked status response.

Use the registered operation ID as the Kernel correlation key from preparation through finish. Resolve the schema, document, Study, project, and run directory once for that operation. Supply the selected Study ID and the `FULLMETA_RUNS_DIRECTORY` context required by the native dispatcher and reuse that environment for `--cancel-run`. Bind its run ID early enough to stop during preparation. Record the start, progress, completion, and cancellation event fields and their consumers. [Study and Stop behavior](../RUNTIME.md#study-execution-function) · [Profile dispatch contract](solver-capability-contract.md#preparation-and-solver-dispatch)

The Study request body is the active Model 2 JSON snapshot. Send the selected application's schema context and these registered operation bindings.

| Request or event field | Required mapping |
| --- | --- |
| `x-fullmeta-operation-id` | Stable Kernel operation ID. The same ID is sent as `operationId` with the operation `kind` for Stop. |
| `x-fullmeta-document` | Encoded canonical saved model path. Use it for the shared document queue and the checked `FULLMETA_DOCUMENT` context. The dispatcher checks that it matches the folder/file bindings and submitted saved snapshot. |
| `x-fullmeta-study-id` | Selected Study ID. Bind to `FULLMETA_STUDY_ID`. |
| `x-fullmeta-model-folder` and `x-fullmeta-model-file` | Encoded canonical document folder and file. Percent-decode them into `FULLMETA_MODEL_FOLDER` and `FULLMETA_MODEL_FILE`. |
| `storage.runsDirectory` | Supply its value to `FULLMETA_RUNS_DIRECTORY`. The original folder and this storage binding determine the run path for both execution and cancellation. |
| Start event `runId` | Stable run identity used by subsequent events and the saved run record. Set the cancellation `runIdPointer` to `/runId`. |
| Complete event `outputFiles` | Register `completionValidation` for event type `complete`, pointer `/outputFiles`, and minimum size one byte. Check required file coverage and content under the verified run as part of the output contract. |

The event stream uses `application/x-ndjson` and the registered `ndjson-stdio` protocol. Pass the worker's stage and case state to the Operation strip. Keep the checked document, schema, registry ID, executable, working directory, environment, and run ID available to the cancel action and to recovery.

A saved-snapshot mismatch ends the stream with a terminal error before a run is created. Identify the saved document and instruct the user to reopen, review changes, and try again. The target application dispatcher owns this check on the shared queued route.

The wave executables report machine-readable case events on lines prefixed with `FULLMETA_PROGRESS ` followed by JSON. The payload has `event` (`case-start` or `case-complete`), zero-based `caseIndex`, `totalCases`, and `frequencyHz`. The coordinator verifies the index, total, and finite positive frequency and projects the events into `type: "progress"` with `runId`, `totalPoints`, and `completedPoints`. Use the known frequency for the active case until it completes. Stationary thermal uses one case and no frequency. Provide structured stage-start events for the active long stages and keep the same case count on those events. Stage durations come from measured stage timestamps.

If the supplied program reports only case start/completion or prints a stage duration after finishing it, the live-stage requirement is blocked by the fixed native dependency. Record the missing event and its acceptance clause; do not patch or rebuild the fixed native dependency within the application build. Reuse compatible optimized binaries for application ID or path changes.

The package and UI module signatures and required behavior are specified in [application-ui-javascript-contract.md](application-ui-javascript-contract.md). That document is normative for the geometry/selection adapter, operation actions, result adapters/presentation, and field settings. The target application package must supply application-owned UI modules for geometry and result behavior; generic widget registration alone does not satisfy these contracts.

## Executable and tool registrations

Register the dispatcher as `x-fullmeta-runtime.externalPrograms.fem`, the mesher as `x-fullmeta-runtime.externalPrograms.mesh`, and Gmsh as `x-fullmeta-runtime.tools.gmsh`. The copied executables use these program keys in schema lookups. The application ID is `{APP_ID}`. Use `pathType: "relative"` and the values below as `executable` paths in the root schema's registrations. Relative program paths use their declaring schema directory. Relative runtime-tool paths use the root schema directory.

```text
fem executable:
  .runtime/x86_64-pc-windows-gnu/release/fullmeta-fem.exe
  allowed entries: --validate, --server, --prepare-study,
                   --capabilities, --resolve-result, --save-figure, --cancel-run,
                   --run-status, --clear-results

scenario solver executables in the same release directory:
  solverAcoustic2d: acoustic2d.exe
  solverAcoustic3d: acoustic3d.exe
  solverEm2d: em2d.exe
  solverEm3d: em3d.exe
  solverThermal2d: thermal2d.exe

mesh executable:
  .runtime/x86_64-pc-windows-gnu/release/fullmeta-mesh.exe
  allowed entries: check, compile, preview, mesh, build

gmsh tool:
  pathType: relative
  mesh-rust/vendor/gmsh-4.15.2/gmsh-4.15.2-Windows64/gmsh.exe
```

If a different target platform is built, preserve the same target application-local directory boundary and declare the correct platform executable path. Do not use paths that traverse another application folder.

Normal UI Studies run the optimized Cargo release build of the dispatcher and every selected scenario solver. Register the actual deployment paths and the supported entries in the profile dispatch table. Keep dispatcher and solver binaries from the same compatible build beside one another for direct preflight. Verify the resolved path, build mode, and backend features for each profile. If the optimized program is missing, report its required registration or build step as a launch failure. [Profile dispatch contract](solver-capability-contract.md#preparation-and-solver-dispatch)

Provision the required executable artifacts before Study acceptance. Reuse the supplied compatible optimized builds. If an executable is absent but the supplied source is unchanged, build only the missing target for deployment and record its source hash, command, target, features, and output hash. Do not alter Rust source or build features to add behavior; a missing capability is a native-dependency blocker. For the Windows deployment below, use Cargo's locked release profile, target `x86_64-pc-windows-gnu`, and target directory `applications/{APP_ID}/.runtime`. Every build writes the same release filenames into that directory and replaces the prior executable. Keep the default `rivrs-backend` and `parallel` features. Select the binary and its required feature as follows.

| Cargo `--bin` | Additional Cargo feature |
| --- | --- |
| `fullmeta-fem` | Default features |
| `acoustic2d` | `solver-native-acoustic2d` |
| `acoustic3d` | `solver-native-acoustic3d` |
| `em2d` | `solver-native-em2d` |
| `em3d` | `solver-native-em3d` |
| `thermal2d` | Default features |

## Expected output files

The Study completion response and UI must account for the output files present in the selected target application run:

| Operation/profile | Expected files/artifacts |
| --- | --- |
| Geometry and mesh | Hash-named `.geo` and `-model-map.json`; Gmsh `.msh` and log; FullMeta `-mesh.mesh`, `-mesh-report.json`, `-mesh-cache.json`; `run.json`. |
| Acoustic field Study | `run.json`, `request.json`, nonempty `logs/solver.stdout.log` and `logs/solver-stage-timings.csv`, required `logs/solver.stderr.log` under the descriptor-specific rule below, each case's Gmsh mesh log, `pressure_<frequency-to-6-decimals>Hz.json`, and `<studyId>.csv` / `<studyId>-debug.csv` when global Study rows are emitted. |
| EM2D field Study | `run.json`, `request.json`, nonempty `logs/solver.stdout.log` and `logs/solver-stage-timings.csv`, required `logs/solver.stderr.log` under the descriptor-specific rule below, each case's Gmsh mesh log, `emfield_<frequency-to-6-decimals>Hz.json`, and `<studyId>.csv` / `<studyId>-debug.csv` when global Study rows are emitted. |
| EM3D field Study | `run.json`, `request.json`, nonempty `logs/solver.stdout.log` and `logs/solver-stage-timings.csv`, required `logs/solver.stderr.log` under the descriptor-specific rule below, each case's Gmsh mesh log, `<fieldFilePrefix>_<frequency-to-12-decimals>GHz.json`, and `<studyId>.csv` / `<studyId>-debug.csv` when global Study rows are emitted. |
| Thermal Study | `run.json`, `request.json`, nonempty `logs/solver.stdout.log` and `logs/solver-stage-timings.csv`, required `logs/solver.stderr.log` under the descriptor-specific rule below, the Gmsh mesh log, `temperature.json` when field output is enabled, and `thermal-summary.csv`. |
| Figure export | Declared target application project output location and `.png`, `.svg`, or `.pdf` file. |

Result adapters must open actual field data and matching tabular output where the profile emits it. A process exit code alone does not establish that a result is available or displayed plausibly.

Every successful Study's terminal NDJSON `outputFiles` array must list the project-relative paths for `run.json`, `request.json`, every required solver and mesh log, and every result, field and tabular output required by that Study. The Kernel checks each listed path against the registered run directory before accepting success. Each path must be unique, identify an existing regular file inside the same run, and satisfy its content/size requirement.

The selected `fem.studyRun` native descriptor explicitly permits an empty `logs/solver.stderr.log`. That file must still exist and appear in the checked output list. This exception applies only to that declared stderr artifact: required stdout, stage-timing, mesh, field and CSV outputs remain nonempty. Do not generalize the exception to another file or operation. A missing file, forbidden-empty output, duplicate path, cross-run path or unlisted required output fails completion.

## Runtime acceptance

- [ ] Catalog, schema loader, and application package load as one application.
- [ ] The target application schema declares the project-delete route, and a disposable target application project can be deleted through the launcher after an explicit folder confirmation; success refreshes the list and failure retains the project card with the exact error.
- [ ] The schema declares each operation, request, output, and storage contract used by the UI.
- [ ] Every executable and tool resolves within `applications/{APP_ID}/` or an explicitly shared platform resource.
- [ ] Validation, mesh operations, and Study requests use the active editor model and correct project folder.
- [ ] Mesh and Study artifacts and run records are present under the declared project storage locations.
- [ ] Progress, cancellation, completion, output files, and failures appear in the UI.
- [ ] Every supported profile resolves its preparation mode and optimized executable and runs through solve and result display. The missing-route case terminates with a stage-specific diagnostic.
- [ ] Stop in preparation and during solving ends the entire registered worker tree and persists the same terminal run state shown in the Operation strip.
- [ ] While a Study holds the document operation queue, reopen the same saved document, read its run summaries, resolve an earlier compatible completed result, and load that checked artifact. Verify the Study continues, no project/run files change due to those reads, and a read-only result request cannot be used for Save figure, Clear Results, or any write route.
- [ ] Result resolution loads fields and tables into registered generic widgets through target application adapters.
- [ ] Every graphics scene `onSelect` ID resolves to a matching `selectionActions()` entry; in particular `loadResult` is registered under the exact key requested by the schema.
- [ ] Graphics selection/highlighting maps selected target application tree items to the correct scene entities.
- [ ] The implementation uses no shared schema-editor modifications.

## Platform references

- [Shared application catalog loader](../../../../platform/js/application-catalog.js)
- [Shared runtime registration schema](../../../../platform/contracts/fullmeta-model-runtime-1.schema.json)
- [Application package loader](../../../../platform/js/application-packages.js)
- [Launcher](../../../../launcher.js)
- [Rust Kernel route dispatch](../../../../platform/kernel-server-rust/src/main.rs)
- [Generic project package](../../../../platform/ui/widgets/model-editor/schema-project-package.js)
- [target application solver and mesher contract](solver-capability-contract.md)

These references define shared interface constraints. They do not authorize edits to shared platform files.


