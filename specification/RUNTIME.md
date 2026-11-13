# FEM runtime and results

This document specifies the operations that connect the target application Model Editor to the copied native FEM and mesh programs. It describes behavior and evidence; it does not assume that the schema designer itself implements these operations.

For the FEM application package and a route-by-route split between generic platform support and application-owned integration, see the [runtime integration contract](contracts/runtime-integration-boundary.md).

## Selected native operation interfaces

Use the identified `fullmeta-native-operations/1` authority through the [shared native-operation contract](../../../platform/docs/native-operation-contract.md). The application declares each selected operation and route; model JSON remains opaque domain data at this boundary.

| Operation | Native entry and transport | FEM request/result obligations |
| --- | --- | --- |
| `mesh.check`, version 1 | `fullmeta-mesh --operation-check`, JSON stdin/stdout | Supply `{model, componentId?}`. Preserve model/component identity. Check the existing geometry compiler's artifact version 14, geometry hash and generated geometry byte count; this operation writes no artifacts. |
| `mesh.compile`, version 1 | `fullmeta-mesh --operation-compile`, JSON stdin/stdout | Supply `{model, componentId?, outputDirectory?}`. Keep the destination inside the checked working/output boundary. Verify geometry/map paths and names, artifact version 14 and geometry hash. Equivalent repeated compilation reuses checked equivalent files; escaping destinations fail before compilation. |
| `fem.prepareStudy`, version 1 | `fullmeta-fem --prepare-study`, JSON stdin/stdout | Supply the original model and selected `studyId`, with supported optional inputs such as `csvFilename`. Accept the native `fullmeta-study-plan/1` frequency plan or the supported envelope containing a `fem-solver-request/1` request. Preserve a valid plan's existing shape and selected Study semantics. |
| `fem.studyRun`, version 1 | `fullmeta-fem --server`, NDJSON stdin/stdout | Supply the original opaque model. Carry selected Study, canonical document, schema, operation/run identity and storage through the checked execution context. Resolve the profile's actual scenario executable and require checked progress followed by one valid terminal outcome. |

Runtime declarations use compact `operationContract` selection and the normal `nativeOperation` workflow where applicable. Resolve the executable from the application's declared registry and preserve the required legacy API18 routes. Native-operation identity and success in one profile do not replace the complete profile, validation-stage, numerical and route cases in [acceptance](ACCEPTANCE.md).

For `fem.studyRun`, the selected descriptor alone permits an empty `logs/solver.stderr.log`. The file must exist and be included in the checked output list. Every other required output retains its declared nonempty/content checks; no schema or caller may generalize this exception to stdout, timing, mesh, field, CSV or arbitrary logs.

## Operation ownership

| Operation | Browser/UI responsibility | Kernel responsibility | FEM/mesh program responsibility |
| --- | --- | --- | --- |
| Validate | Submit current model snapshot and display diagnostics. | Resolve the schema-registered program and transport input. | Common native preflight, scenario registration checks, semantic diagnostics. |
| Build mesh | Request geometry compile/mesh and display progress and artifact. | Allowlisted command launch, checked run directory and declared output verification. | Geometry validation/compilation, Gmsh invocation, mesh export, mesh report/cache. |
| Restore preview | Ask for a matching mesh-only/completed run. | Launch registered preview entry and serve checked artifact. | Geometry-hash lookup and artifact integrity checks. |
| Delete mesh | Request deletion of only matching mesh-only artifacts for the project. | Schema-selected checked storage operation. | Maintain run records/debug data according to the declared deletion selector. |
| Run Study | Submit current model/Study; render progress, completion/failure, and output references. | Dispatch registered NDJSON API, stream events, handle cancellation, verify declared output files. | Validate, prepare Study, prepare/reuse mesh, dispatch solver, write run artifacts and diagnostics. |
| Resolve results | Request data for the selected result recipe and render response. | Validate route/program and serve checked workspace artifacts. | Select the correct completed run and map result recipe to concrete files/series. |
| Save figure | Render the selected plot and submit export request. | Route the registered JSON request safely. | Validate dataset/output and write the requested figure file. |
| Clear results | Confirm deletion of Study runs for the open model document. | Invoke the registered target application cleanup entry with the opened document context. | Check each manifest's ownership and remove only matching Study runs. |

The active Kernel is application-neutral. A route declaration must use a supported Rust-owned handler/protocol. Do not design around JavaScript route fallback or arbitrary executable paths.

## Validation function

### Input

- Current model snapshot, including unsaved changes where the editor supports it.
- Selected target application schema graph and declared scenario/runtime capabilities.
- Opened document identity supplied by the Kernel.

### Expected behavior

- Browser-side structural/edit feedback remains immediate and generic.
- The **Validate** operation invokes the registered FEM validation entry through the generic command route.
- The result differentiates structural and semantic/model diagnostics and identifies affected paths where provided by the executable.
- The toolbar describes the scope as model declarations, expressions, and references, and explains that mesh and solver checks run with those operations. Show **Checking model declarations, expressions and references…** while running. On clean success, show **Model validation passed** and the resolved values from that snapshot. If the model changes while the response is pending, do not apply old diagnostics or values; show **The model changed while validation was running. Select Validate again.**
- Validation does not claim to have meshed or solved the model.
- A validation pass does not establish executable availability, geometry validity, mesh success, numerical convergence, or result readability.

### Evidence

- A valid spec-derived case reports success in the UI.
- Disposable invalid copies cover at least an unknown reference, invalid profile/formulation pairing, invalid geometry field, unsupported condition, and invalid Study reference.
- Store the exact error text and path. Use separate disposable failing cases.

## Mesh function

The copied `fullmeta-mesh` command interface documents:

| Entry | Input | Output/behavior |
| --- | --- | --- |
| `check` | Model JSON, optional component | Geometry/model checks without building a final mesh. |
| `compile` | Model JSON, optional component/output directory | Gmsh `.geo` and model-map inputs for the selected component. |
| `preview` | Model input on stdin, component and runs directory | Locate/verify a recent matching mesh-only or completed run; does not invoke Gmsh. |
| `mesh` | Compiled `.geo` and Gmsh executable | Invoke Gmsh and export a FullMeta mesh. |
| `build` | Model JSON, optional component/output directory | Combined check/compile/mesh pipeline for CLI use. |

### UI build expectations

1. Run the current model through the registered `mesh` executable and `build`/supported command sequence.
2. Keep each generated run inside the current project’s declared run directory.
3. Display which geometry/component was meshed and the resulting element family/count.
4. Verify `.geo`, model-map, Gmsh input/output, FullMeta mesh, log, and report artifacts exist where promised.
5. Show exact errors for missing geometry, unsupported feature, Gmsh failure, export failure, or output verification failure.
6. A successful explicit **Build mesh** enables Mesh for the open model. Keep the Geometry and Edges checkbox values and the selected item.
7. Deleting a mesh preview must not silently erase Study runs; clearing generated results must name the exact project directory and scope.

### Mesh checks

- Geometry input includes the feature IDs used by selections and materials.
- 2D geometry exports triangle meshes with adjacency-derived edges and tags required by the model map.
- 3D geometry exports tetrahedral meshes and boundary/interface tags required by the model map.
- Union features preserve an ID representing the combined domain while children remain traceable.
- Periodic pairs must match actual mesh entities and supported translations.
- Every mesh artifact has a corresponding run record/report and integrity information if declared by the program contract.

## Study execution function

The FEM executable contract includes saved-project CLI operation, `--validate`, stdin validation, `--server`, `--prepare-study`, result resolution, capability reporting, figure saving, cancellation, and scenario-specific solve entries. The browser Study route is expected to use the registered NDJSON protocol rather than a custom route that bypasses the Kernel.

### Request and lifecycle

1. Commit applicable editor drafts and validate the model. Save valid unsaved changes through the shared document service before starting the Study. Use the saved snapshot for the request and run fingerprint. Stop before dispatch if the draft cannot be applied, validation or saving fails, or the document changes during the save. Resolve the selected Study and its linked Physics again in the saved snapshot. Bind the project folder, document, active schema, operation ID, run ID, and schema-declared run storage to one operation context. Establish the run record and associate the active worker with this context before starting preparation.
2. Resolve the profile, backend, preparation handler, and numerical executable. Validate their registrations before launching numerical work. Report the profile, stage, and missing or unsupported declaration promptly if resolution fails.
3. Prepare the selected Study once through its declared preparation handler. Reuse a verified matching mesh or build the required mesh in the operation's project run storage. Dispatch the prepared work to the registered numerical handler. [Profile dispatch contract](contracts/solver-capability-contract.md#preparation-and-solver-dispatch)
4. Stream stages, case starts, case completions, and the run's terminal event as NDJSON. Identify all events with the same operation/run context. A case counts as completed after it succeeds. Count each case once.
5. Verify the expected outputs for every requested case and check each output path in the registered run directory. The successful terminal event lists `run.json`, `request.json`, every required solver stdout/stderr and stage-timing log, each case's mesh log, and all required field and tabular outputs in `outputFiles`. Paths are project-relative, unique and within the same run. Required outputs are nonempty regular files, except the existing `logs/solver.stderr.log` may be empty only where the selected `fem.studyRun` descriptor explicitly permits it. The exception does not relax other output checks or require fabricated stderr text. Publish success and the checked output list only after the worker finishes and the run record reaches `completed`. Missing, forbidden-empty, duplicate, cross-run or out-of-scope required outputs fail verification.

After a save returns, verify that the editor still has the same document open with no intervening local edits. Submit the exact saved JSON through the canonical document queue. At Study entry, compare the submitted JSON with the checked saved document while the Kernel holds the document queue. If another editor has saved different data in the gap, the target application dispatcher reports a terminal snapshot conflict and asks the user to reopen, review, and run again. Reject before creating the run record, preparing the mesh, or launching a scenario worker. Compare JSON values with object key order ignored and array order preserved. On a match, keep the accepted request snapshot immutable throughout execution.

### Bottom Operation strip

Use the workspace's shared bottom Operation strip for the active Study. Keep it visible when the user changes the selected tree item or graphics scene. The strip shows the active stage, current frequency and other case coordinates when applicable, completed count, elapsed time, and Stop. The selected item's Settings remain available. Reject duplicate Study submissions while the operation is active.

| Event or state | Required display |
| --- | --- |
| Preparation or another stage before the total is known | Indeterminate progress, current stage, elapsed time, and Stop. |
| Total known, including while a case is running | Left-aligned fill of `100 × completed / total` percent and `completed / total` count. For 1 of 21 the fill is about 4.8%. Case start updates the current case and frequency. |
| Assembly, factorization, solve, or output writing for a long case | Keep the stage and current frequency visible and advance elapsed time even when the completed count stays fixed. |
| Every case has completed and outputs are being checked | Show the full case count and the verification stage as active. |
| Verified terminal completion | Show success with the final elapsed time and expose the checked run outputs. |
| Failed, cancelled, or interrupted | Show the terminal state, last stage, completed count, and diagnostic or run log reference. |

Replay the strip with a disposable harmonic case whose Physics frequency is linked to a frequency parameter. Set its linear range to `200 Hz` through `400 Hz` with count `21`. Capture the strip before the total is known, after one verified case, during a long stage with the count held, during output verification, and at verified completion. Compare the progress fill to its track, and record the event and corresponding UI for each state. If a transient state is too short to observe, use a controlled stream of the declared events to hold that state for the presentation check. Also run an actual Study through the registered route and verify its worker and outputs.

### Stop and recovery

1. Stop targets the active Study's registered operation even if another item is selected. Send the same operation ID and kind through the shared cancel route. The route resolves the original project, document, Study, run ID, and run-storage context from that registration.
2. Stop is available throughout preparation, meshing, and solving. Show **Stopping…** while cancellation is pending. Terminate and confirm exit of the entire process tree for that operation, including the dispatcher, preparation worker, mesher and solver children as applicable. Preserve other projects and operations.
3. Use the original run-storage context to persist the terminal run state and cancellation details. A successful Stop yields `cancelled` in the saved record and in the strip after worker exit. Preserve the stage, elapsed time, completed count, logs, and partial files. If completion won the race, show the verified completed state. If cancellation fails while a worker remains active, show the error, restore the live stage, and allow retry.
4. After a stream disconnect or editor reload, reconcile the same run and operation with the saved run record and the checked status route. Restore live progress when the editor has access to the model and the worker is still active. Supply the validated saved start through the shared progress update service. Use an existing terminal record when present. If the worker is confirmed gone and the record remains `running`, persist `interrupted` with its diagnostic. Preserve logs and partial files. Keep unknown liveness unresolved. [Study status and reconciliation](contracts/runtime-integration-boundary.md#study-status-and-reconciliation)
5. Resolve results independently of the latest attempt. A newer running, cancelled, interrupted, or failed attempt leaves an earlier compatible completed run eligible. Keep an already displayed compatible result usable during the Study. The shared Kernel must also allow canonical model reads, run-summary reads, checked artifact reads, and the explicitly read-only `resultResolve` route to complete during that Study. [Result resolver contract](contracts/application-ui-javascript-contract.md#result-presentationjs) · [Read access during an active document operation](contracts/runtime-integration-boundary.md#read-access-during-an-active-document-operation)

### Concurrent model and result reads

During a Study, a user may reopen the same canonical saved model in a second editor and load a compatible result from an earlier completed run. Keep the Study, its operation strip, and the selected document active in the original editor. The shared Kernel bypasses the document queue only for checked read-only routes listed in the [runtime integration contract](contracts/runtime-integration-boundary.md#read-access-during-an-active-document-operation). Keep canonical model identity, schema/application identity, route registration, run ownership, terminal state, freshness, and checked artifact scope on each read.

The target application `resultResolve` program must select a compatible completed run. It must not read a partial output from the active run as a completed result, mutate run state, or delay the Study worker. A newer successful compatible completion takes precedence after it is verified. A newer cancelled, failed, interrupted, or active attempt leaves the earlier compatible completion selectable. Preserve the prior run's files.

Verify the positive path while the Study worker is confirmed active: reopen the saved model, read run summaries, resolve the earlier completion, and load its checked artifact. Record the active Study's operation/run IDs before and after the reads, compare the model and run files' hashes, and confirm the read route does not write. Also verify an unmarked JSON route and an opted-in route with a mismatched application remain rejected or serialized; a read-only opt-in must not authorize Save figure, Clear Results, mesh, Study, or model Save.

### Recovered terminal duration

Show the saved duration in a read-only **Last finished run** panel beneath the selected Study's actions in Settings. Mount it through the package's `mountEditorPanels` and declare its visibility in `modelEditorUi.panels` for `/model/study/*`. Show the latest terminal attempt for that exact document and Study ID, ordered by saved start time with a stable run-ID tie-break. Include its run ID, terminal state, saved start and end, fixed duration, and any diagnostic. Show an empty state when the selected Study has no terminal record. Use the shared Operation strip for live progress and the immediate terminal outcome.

Calculate the saved duration from validated UTC timestamps as `completedAt - startedAt` and keep it fixed across reopens. The shared progress controller preserves finite supplied start and finish timestamps for reconstructed terminal operations; it uses the local clock only when a new operation omits those values. For interruption without a recorded terminal event, persist the first reconciliation time at which the original worker is confirmed gone as `completedAt` and label the duration as time to interruption detection. Report duration unavailable when either timestamp is missing or invalid, or when the end precedes the start. Test delayed reopen and switching between Studies and sibling models with different run histories.

### Controlled recovery checks

Use a disposable, spec-derived document and dedicated run storage. Record the candidate build and the tested run and operation identities.

- **Stop during a live stage.** Run two separate disposable Studies. Stop one after preparation starts and before solving starts. Stop the other after the solver starts. Confirm the stage from the operation events and worker identity before clicking Stop. Capture the original process tree and the same operation and run IDs in the cancel request, final UI, and saved record. Verify process exit, preserved diagnostics, and the prior completed result where one exists. A replayed event stream can check the Stop presentation. The process and persistence checks require an actual run.
- **Browser reconnect.** Capture a live run's document, operation ID, run ID, and last seen progress. Disconnect or reload while that run is active. Reopen the same canonical document during the run, observe the recovered status, and compare it with the status route and saved record for the original run. Resolve and load a prior compatible result while the worker remains active. Use separate attempts for a verified completion and a later failed or cancelled run. Confirm the resolved run ID in each case. [Concurrent model and result reads](#concurrent-model-and-result-reads)
- **Worker identity.** Use a benign live helper process and a disposable running manifest with its PID and a deliberately different valid OS start marker. The read-only lookup must report `gone` and leave the manifest and lock intact. Explicit reconciliation records interruption once and leaves the helper alive. Use a matching worker identity as the active control and an unreadable or missing identity as the unknown control. Keep the OS start marker exact when preparing test data. The test needs no natural PID reuse.
- **Terminal race.** Use a deterministic synchronization barrier in a application-owned test harness around the production terminal-transition logic. Pause reconciliation after observation, commit a verified completion, then release reconciliation. The completed state, timestamp, and output list must survive. Exercise two simultaneous reconciliation attempts on a separate confirmed-gone record. Only one transition and detection timestamp may persist. Verify a completed record through the normal status route as an integration control. Keep test synchronization in the harness.

### Study families exercised

Author small disposable models from the [Model 2 contract](../docs/fullmeta-model-2.md) and [solver capability matrix](contracts/solver-capability-contract.md#scenario-capability-matrix). Use a rectangle for 2D and a box for 3D, an explicit domain selection and material assignment covering the entire domain, and named boundary selections for the drive or prescribed values. Use the declared material defaults and a mesh size that resolves the chosen geometry. For each run, record the geometry sizes, boundary quantities, mesh settings, frequency or stationary inputs, and expected output paths before opening the model.

| Profile | Study witness and result to open |
| --- | --- |
| `acoustic-2d` | Harmonic pressure with a nonzero prescribed pressure. Open the checked planar pressure field. Use a parameter-linked harmonic sweep for the Operation strip and a global variable plot for Plot/Table. |
| `acoustic-3d` | Harmonic pressure with a nonzero prescribed pressure. Open the checked pressure field and inspect the three cut controls. |
| `em-2d` | Separate `Ez` and `Hz` single-point cases with a prescribed nonzero primary field in `V/m` and `A/m`, respectively. Open each checked planar field and verify the primary quantity and unit. |
| `em-3d` | Harmonic electric field with impedance and an incident electric polarization tangent to its driven face. Open the checked electric field and inspect the three cut controls. |
| `thermal-2d` | Stationary conduction with constant conductivity, opposite edges at distinct prescribed temperatures, and the other edges insulated. Open the checked temperature field in K and verify that it spans the prescribed temperatures with no frequency control. |

For each profile, open the canonical disposable model in the registered Model Editor, Validate, build a mesh, clear it, and rebuild it. Verify the generated mesh and preview. Select the Study and click **Run Study**. Record the preparation handler, solver executable and build mode, operation and run IDs, progress, and terminal state. Match the final output list to the files under that run and open the result recipes in the same editor. Verify the displayed run, case, quantity, units, and content. An exit code or a field file alone provides only part of this evidence. [Registered outputs](contracts/runtime-integration-boundary.md#expected-output-files)

The schema must not expose a solver option unsupported by the copied executable. Numeric stability, finite coefficients, residual acceptance, actual boundary correspondence, mesh material coverage, and scenario condition support are runtime checks.

## Storage and cleanup

The app schema must declare the project model file and the project-local run/results folders used by its package. Every run record must identify the model/Study inputs and generated artifacts sufficiently to select results and diagnose failures.

### Clear Results scope

Place **Clear Results** on Study rows only, beside **Run Study** in Settings, including for models with no result recipes. The button clears generated terminal Study runs owned by the open model document across its Studies. The confirmation names the exact saved model file and project folder and describes that scope. Cancel or a change of the open document during confirmation leaves storage untouched.

Under the project run lock, remove only directories in declared run storage with a checked terminal `run.json` whose `kind` is `study` and whose canonical `model.file` identifies the open document. Preserve runs for sibling documents, mesh-only runs, and directories with missing or unverified ownership. Preserve external datasets and exported figures outside the selected run directories. If the project run lock is busy or the open document has a nonterminal attempt, report the blocker and leave the files intact. Reconcile an interrupted attempt before clearing it. **Delete mesh** remains a separate mesh-only storage action.

Record removed run and artifact counts, skipped unverified records, and the exact document. Refresh the run summaries and result views for the open document. Verify that a later run produces a new run ID and checked outputs. [Cleanup route](contracts/runtime-integration-boundary.md#route-and-program-contract)

## Results resolution and plots

### Result freshness and fallback

Scope eligible completed runs to the canonical model file, selected Study ID, and profile. Compare a versioned, canonical projection of the current Study inputs with the saved run's projection. The projection includes profile/formulation, model units, geometry and tolerance, selection memberships and filters, mesh and periodic settings, material assignments, the linked Physics and its effective conditions, and the selected Study's discretization, solver, sweep, and generated-output settings. Include integration operators and global and pointwise definitions that govern generated output. Resolve the transitive parameter dependencies of those inputs, including sweep axes and schema-declared expression fields stored as plain strings, such as `coordinateExpression` and relative material properties.

Exclude display names and descriptions, unreferenced parameters, unrelated Studies, result recipes and their axes/series/filters/cuts/export settings, and view controls such as layer visibility and camera position. A symbol, callable name, ID, or reference used for evaluation, linkage, or generated column identity is an input. A recipe edit can require a different saved artifact even when the Study inputs are unchanged. Re-resolve the recipe in that case.

If a run predates the fingerprint, derive it from the verified saved input snapshot when available. Preserve the recorded artifacts and metadata during comparison. Keep a run unverified when the available data is insufficient to reconstruct its input projection.

Load the newest compatible checked completion with the required artifacts. A newer attempt that is active, failed, cancelled, interrupted, or missing the requested artifact leaves other compatible completions eligible. If there is no current matching artifact, explain which result is missing. An explicit **View saved result** action may offer a checked older completion for the same document, Study, and profile. Mark it **Stale** persistently with its run ID and completion time. Display its saved geometry, mesh, definitions, and sample coordinates together. A run whose freshness cannot be established stays **Unverified** and cannot satisfy the current-result check.

### Field result function

The result resolver identifies a checked run, Study, dataset, and field artifact for the selected recipe. Verify the selected quantity, units, sample coordinates, and artifact identity before rendering. The adapter reports the saved case and dataset, dimension, node and cell counts, formulation, and any available solver, discretization, unknown count, and residual metadata. Show the source Study and run ID, the resolved case or stationary state, and the mesh and solve information in Settings. For a 3D field, also show the resolved cut location for each model axis. In the graphics summary, show the recipe name and the resolved parameter and value in the declared display unit, or Stationary for a stationary case. Use the field settings and graphics bindings in the application UI contract. [Application UI contract](contracts/application-ui-javascript-contract.md#graphics-declaration-and-controls)

Resolve the operating frequency label from the checked result's Study and its linked Physics. When the trimmed Physics frequency expression is exactly a declared parameter symbol with a frequency unit, use that symbol and the parameter's declared unit. Convert the checked case's physical frequency into that unit. A parameter `drive` declared in GHz and a checked frequency of `3000000000 Hz` therefore display as `drive = 3 GHz`. This applies to single points and sweeps, including an artifact that labels the raw coordinate `Frequency` or stores it as `frequency_Hz`. Use the checked case value even when the parameter's stored default differs. For a compound or literal Physics frequency, format the checked frequency in the profile's field display unit, falling back to GHz when the profile supplies none. Preserve other case coordinates in their declared labels and units. Use the same frequency label in the Graphics Console and result Settings. A stationary case has no frequency label.

The Graphics Console header shows the recipe name, the resolved case label, and the shared structural orientation. Keep the case ordinal and mesh/sample counts in the result detail area. Give the field viewport a short hint about the shared structural view, with pan in 2D, rotation in 3D, and wheel zoom. Verify framing at a fixed measured canvas size and device scale. Check the overall layout separately, including changes in the available canvas height when a header or hint wraps.

Normalize field-file topology into the active project's spatial dimension. A native artifact may omit a top-level dimension value; infer 2D or 3D only when its non-empty cell topology and node coordinates identify one dimension unambiguously. The normalized renderer input always carries the dimension. Reject conflicting dimension metadata, invalid connectivity, missing requested quantities, or mismatched sample counts with the artifact path and exact reason.

Use one sample selector backed by the saved dataset's coordinate metadata. The selected tuple must resolve to exactly one saved field case. When the recipe has no stored sample coordinates, evaluate its optional frequency to choose the nearest available frequency in the saved dataset, with equal-distance choices ordered by saved case index. With no configured frequency, use the dataset's resolved default. A sample selection updates the recipe's filters with the stored numeric sample coordinates and preserves the recipe's existing frequency expression and unit. A field with one stationary case shows its stationary state without a frequency selector or a synthetic frequency. [Shared result widgets](contracts/shared-widget-contract.md#field-and-result-widget-bindings)

When the checked field artifact is already loaded, changing to an available quantity or part commits the recipe edit immediately and recalculates the field surface and color scale from that artifact. Preserve the selected case, camera, and layer preferences. The local change needs neither a result request nor **Apply item**. **Save** persists the recipe.

#### Field surfaces and color scale

- Render the entire triangulated field in the model's x-y plane in 2D. Keep any saved cut-plane declarations intact and show the full planar field state in Settings.
- For 3D, map solver coordinates into model coordinates using the artifact's declared coordinate order and origin. Apply the origin in the corresponding solver axis before reordering to model x, y, and z. A field component already expressed on model axes retains its identity during the position transform.
- Draw the intersections with the x, y, and z cut planes of the saved tetrahedral field. An omitted axis uses the field-domain midpoint. Evaluate an explicit coordinate from its preserved length expression and unit. Check the model-coordinate bounds with tolerance `max(1e-10 m, span × 1e-9)` and clamp rounding differences within that tolerance. Identify the axis and range when a coordinate is outside the tolerance.
- Use one **Cut-plane positions** group with X, Y, and Z mode controls in that order. Label each control by axis, for example **X cut-plane position mode**, and offer **Mid-plane** and **Explicit value**. When an axis changes from Mid-plane to Explicit value, seed its coordinate with the resolved midpoint converted to the active profile's geometry display unit. Commit the axis mode and seeded quantity immediately. Subsequent commits to the expression or unit update the recipe and redraw from the loaded field artifact. Keep the camera, selected case, and layer preferences. These local edits require neither **Apply item** nor an artifact refetch. **Save** persists them.
- Linearly interpolate the selected scalar between the intersected tetrahedron vertices. Deduplicate coincident points, skip zero-area intersections, order polygon points around the cut, and render every nondegenerate polygon. A three-point intersection gives one triangle and a four-point intersection gives two. Supply the shared renderer with indexed polygon faces or explicit triangles. An inline four-point face without triangulation is incomplete. Retain separate cell-local values at interfaces where the artifact supplies them.
- Calculate the auto-range from the finite scalar values at rendered triangle corners in the displayed field unit. Count each corner occurrence, including a shared vertex repeated across triangles and interpolated cut corners in 3D. Sort those values in ascending order and take percentile `p` at index `floor(p × (n − 1))`, where `n` is the occurrence count. For a magnitude selection, use zero to the 98.5th percentile. For a real or imaginary selection, use the 2nd and 98th percentiles. The selected part determines the range policy, including when a real-valued derived expression happens to be nonnegative. Fall back to finite extrema if needed. If the resulting endpoints coincide, set the upper endpoint to the lower endpoint plus one in the displayed field unit. A field without finite drawable values is an error.
- Pass the same shared `fieldColor` palette and numeric range to the field renderer and the shared vertical colorbar. Label the bar with the selected quantity and unit and show both endpoints. [Shared field palette](../../../platform/ui/widgets/shared/plots/field-color.js) · [Shared colorbar](../../../platform/ui/widgets/colorbar/widget.js)

Use a disposable 3D case with two tetrahedron vertices on each side of a plane to verify both resulting triangles are rendered. Check a boundary-aligned cut and switch an axis to Explicit to verify its seeded midpoint and unit. Commit an expression and a compatible unit change. Verify that each valid edit redraws without changing the camera or requesting data again. Change the quantity with the artifact loaded and check the same behavior. Check a constant field with coincident range endpoints. In a planar case with a repeated vertex and an extreme outlier, compare the drawn color scale with the expected corner-weighted percentile range and check that the legend uses that same range.

### Global/sweep plot function

Resolve each dataset through its declared Study or checked external file source. Match Study CSV rows to saved case metadata before projecting columns. Resolve the recipe's x-axis and fixed parameter keys using checked column metadata or an explicit saved Study-parameter mapping. Preserve the recipe's keys even when an artifact header differs. Normalize source quantities once and convert to compatible display units for the x-axis, fixed coordinates, and each series. Use a common compatible display unit for values on the same axis. Keep source and display units distinct and report an incompatible unit with its column and dataset.

For external CSV files, use `source.columnUnits` to interpret raw numeric cells before applying display units. Match map keys to the parsed headers exactly and report any unmatched key with its dataset and column. An explicit map entry supplies the source unit even when the header contains a unit-like suffix. Preserve the existing source units of the exact `frequency_Hz` and `frequency_GHz` headers when no map entry is present. For any other unmapped file column, retain the legacy interpretation: use `xUnit` for the selected X column, the declared series unit for a series column, or unit `1` when neither applies. When both legacy declarations apply to the same column, use `xUnit`. Under this fallback, editing the assumed unit reinterprets the original numbers. Apply and reopen must use the same interpretation. Declare a source unit explicitly to convert values independently of the display unit. When only source metadata supplies a unit, display values in that unit.

Resolve each declared series and table column to a finite numeric column. For a Study-backed global symbol, use the exact numeric symbol column when available, then its explicitly associated `_re` column. An explicit `_re` or `_im` selection retains its requested part. Default the visible label to `Re(symbol)` or `Im(symbol)` according to the resolved part and honor explicit custom labels. External file column names are literal. Report unavailable, ambiguous, or nonnumeric data with the dataset and column. Keep each series' axis, use the same color for the same series index across datasets, and use the dataset's solid, dashed, or dotted line style.

On **Apply item** for changed dataset axes, filters, or series, reslice the checked dataset with the new x column and fixed values and rebuild the series from the applied settings. When the dataset source changes, resolve the new checked source before rendering. A change to file column units must reinterpret the original CSV numbers using the new source declaration. Use the same resulting ordered rows for Plot and Table. Both views reflect the applied selection without a new Study. Show the source, selected and total sample counts, varying-axis label and unit, and fixed parameter values with their units. In the Table, put the varying x column first, then the fixed-coordinate columns, then the declared result columns in recipe order. Use the declared series columns when `tableColumns` is omitted. Deduplicate columns by key while preserving first occurrence. A slice with no checked matching rows reports the unresolved selection. Saving and reopening the recipe preserves the resulting values for unchanged checked data.

#### Figure export

Enable **Save figure** only in the Plot view of a supported recipe with a checked output path and a rendered figure. Every declared dataset must have a ready slice with a resolved x column and numeric series. Keep export disabled while results are loading or the active Study is running. For an SVG or PNG output filename, capture the plot canvas as PNG. For PDF, capture JPEG with quality `0.94`. Send the image and canvas dimensions through the declared figure-save route with the canonical document and selected recipe. Report success only after the server verifies and writes the requested project-local output and returns `state: "completed"`, its checked path, and a positive byte count. [Figure-save route](contracts/runtime-integration-boundary.md#route-and-program-contract)

### Display controls

Keep Geometry, Mesh, and Edges checkbox preferences for the open canonical model across transitions between structure, field, and Plot/Table views. Start a fresh model view with Geometry on, Mesh off, and Edges on. Plot/Table hides the layer controls and leaves their values available on return. Repainting, changing result recipes or cases, and resetting the camera retain the checkbox values. The current row and Settings remain active while the user changes the display.

The initial restore of a compatible saved mesh may enable Mesh when the same document and its initial view remain active and the user has left Mesh untouched. A successful explicit **Build mesh** enables Mesh for the open model while preserving Geometry and Edges. Later background loads may install checked data for the matching context while leaving the current view and checkbox preferences in place. Discard responses from a superseded document. A response for an earlier scene or result must not replace the active scene, selection, or result.

Apply the following checks in the structure scene and in every 2D or 3D field view.

- **Geometry** shows or hides its own drawing while leaving the frame, mesh, and field in place. A hidden geometry drawing still has valid coordinates for other layers.
- **Mesh** controls faces from the checked mesh. With Mesh off, omit both mesh faces and edge pixels. Leave the Edges checkbox value unchanged.
- **Edges** controls the wire overlay when Mesh is on. With a checked mesh available and Mesh off, Edges remains enabled and stores the user's choice. Turning Mesh on applies that stored choice.
- A control whose layer data is unavailable is disabled with a clear reason. With no checked mesh, disable Mesh and Edges. Retain their checkbox preferences when availability changes.
- Zoom in and out visibly change framing in opposite directions. **Reset view** restores a useful full-model frame and retains the result, layer choices, and tree selection.

Change the checkbox values from their defaults and enter structure, field, and any applicable Plot/Table view. Check the same values when the controls reappear. Verify Edges on and off with Mesh both on and off, and verify the unavailable state with no checked mesh. Exercise initial restoration before and after a user change or a scene transition. Check that a successful explicit build enables Mesh and leaves the other preferences intact. Restore the test view's starting choices when finished. [Graphics declarations](contracts/application-ui-javascript-contract.md#graphics-declaration-and-controls)

For a 1D plot, exercise Plot/Table toggling and any offered zoom or Reset controls. Mark unavailable 1D controls N/A with a reason.

### Output file acceptance

A result passes only if all are true:

1. The expected file exists under the new run directory.
2. The UI resolver identifies the new run, not a stale earlier run.
3. The adapter loads the file without errors.
4. The displayed result matches the profile, quantity, frequency or range, and geometry expected by the project.
5. Plot controls respond and Reset returns a usable view.

Study exit status alone is insufficient.
