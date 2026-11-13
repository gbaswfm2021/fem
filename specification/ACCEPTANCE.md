# FEM acceptance

Evaluate the saved schema and the complete registered FEM application using cases written solely from the specification. The builder owns the application schema, package, JavaScript, JSX widgets, actions and adapters. Record the candidate, specification version, actual outcomes and artifacts for every required behavior. [Build sequence](README.md)

## Acceptance scope and failure attribution

Repair defects in application-owned schema, package, JavaScript, JSX, actions, adapters and application tests within the application build. The supplied Schema Editor, Model Editor, Kernel, shared libraries, `fem-rust` and `mesh-rust` are fixed dependencies. Record an exact reproduction when one prevents a required outcome; continue independent work and leave the blocked requirement unverified. A defect in application-owned code is an application failure to fix, rather than an external dependency blocker.

Use these outcomes:

- **PASS:** the saved declaration and required application behavior satisfy the specification on the identified candidate.
- **FAIL-SCHEMA:** an application schema declaration is missing, malformed, inconsistent with the contract, or sends the wrong value, path or request. Repair the schema and rerun the affected cases.
- **FAIL-APPLICATION:** application-owned package, JavaScript, JSX, action or adapter behavior fails. Repair the owning application code and rerun the affected cases.
- **BLOCKED-DEPENDENCY:** the application declaration and request are correct, but a fixed shared component or supplied native program prevents verification. This is not a pass.
- **PENDING:** required evidence has not yet been collected and no dependency failure has been established.
- **N/A:** the behavior does not apply to this profile or case; record the contract clause and reason.

A dependency-blocked replay row remains unverified; it is not silently passed. Record the component/version or executable hash, minimal input, exact steps, expected and observed results, message, and blocked clause. Continue independent checks, then report the external blocker separately so it does not create a schema-fix loop.

For widget boundaries, the FEM acceptance baseline covers FEM-owned source and tests and the supplied shared platform interfaces. Run and record the platform-scoped check and the FEM import/provider checks; repair FEM-owned findings and rerun affected checks. A checkout-wide audit, if run, is separate evidence: retain its actual outcome and do not report a scoped pass as a checkout-wide pass. Classify a dependency finding as a FEM blocker only when it actually prevents a required FEM check, with the normal reproduction and identity evidence. See [widget audit scope](PLATFORM.md#widget-audit-scope). All schema, package, visible UI, native and numerical acceptance obligations remain in force.

## Two-level acceptance

Both levels evaluate FEM: its saved schema, its bindings and the resulting FEM
application behavior on specification-derived FEM inputs. Schema Editor and
Model Editor are the authoring and replay tools. Testing their general-purpose
algorithms, generic initialization using another schema, or unrelated
applications belongs to their own acceptance suites and is not a FEM gate.
Dependency regression tests may support a separately authorized repair; report
them separately from FEM acceptance results. Keep checks that exercise FEM's
own declarations through these shared tools.

General Schema Editor transactions, declaration commands and shared-widget
behavior are covered by the neutral fixtures and two-level checks in the
[Schema Editor acceptance contract](../../../platform/schema-editor/specification/ACCEPTANCE.md).
FEM acceptance still exercises FEM's saved declarations, bindings, package and
application behavior. When a FEM case exposes a shared defect, retain the FEM
integration case and add a minimal neutral regression case to the platform suite.

**Level 1 — Schema acceptance** is Gates 0–2. It establishes that the authored schema meets the written declaration, tree/Settings, validation, action, and supported UI-binding contracts. The builder may iterate on schema-caused failures here.

**Level 2 — Behavior replay** is Gates 3–5. It uses the same candidate and disposable models created from the normative format and application contracts. Replay exercises the registered package and the actual Model Editor, operations, and results, including composite cases. Existing saved project JSONs are neither required nor accepted as replay inputs.

For a schema failure, repair the schema, repeat Level 1 and rerun affected Level 2 cases. For an application-code failure, repair that code and repeat its affected integration and replay cases. Only a correctly formed application request blocked by a fixed shared/native dependency is BLOCKED-DEPENDENCY. Keep the owner, reproduction and unresolved acceptance clause explicit.

## Acceptance replay and freeze

Before the first acceptance check, capture the saved schema and its resolved references, application catalog entry and package, specification and checklist, installed shared host and Kernel, and native executable paths, hashes, and build modes. Use these identities throughout the replay and verify them again before freeze. Give each spec-derived input a case ID, record its canonical path and starting JSON or digest, and record the expected outcome before running it. Use separate writable disposable copies for cases whose edits would change another case's starting state.

### Evidence reuse and targeted reruns

Retain passing evidence and reference it when a requirement has already passed on the same relevant candidate identities and the evidence remains valid. Do not rerun a passed check merely because another acceptance area is still in progress. When an identity changes, invalidate and rerun only checks whose behavior or evidence depends on the changed identity; likewise rerun checks affected by a repair, a changed input or environment, expired or incomplete evidence, or an explicit repeat required by a contract. Keep unaffected passing rows linked to their original evidence and record the identity tuple that makes the evidence reusable. If a changed candidate prevents prior evidence from satisfying the freeze requirement, mark only those rows pending until current evidence is collected. This rule avoids redundant work without carrying a result across a relevant change.

#### Acceptance work queue and progress accounting

Keep a concise row-to-evidence index beside the narrative replay log. For each
checklist row, list its required subchecks, the covering case IDs, the evidence
heading or artifact, and the candidate identities on which that evidence
depends. The narrative log records what happened; this index records which
acceptance obligations that evidence closes. Do not infer a completed
requirement from a related test count or a successful subcase.

Track subchecks as **unverified**, **partial**, **pass**, **blocked**, or
**N/A**. These are progress states for the index; the acceptance outcome for a
compound checklist row is **PASS** only when every required subcheck passes or
has a justified N/A disposition. A blocker leaves the row unpassed. Show full
row passes and partial subchecks separately in the monitor.

Before each replay batch, select the next unverified subchecks that close one
or more rows and group them by profile, document, and UI/runtime session. Reuse
unchanged passing evidence. At the end of the batch, update the row-to-evidence
index, the replay log, and monitor counts together. Do not start unrelated
exploration while the selected batch has unresolved steps; record a specific
blocker and move to the next independent batch only when needed.

### Contract-derived case suite

Build the test documents from the Model 2 contract itself; do not copy or inspect saved project documents to create them. Start with one minimal valid document for every profile. Add focused variants that change one field, discriminator, optional-member state, reference, or collection entry at a time. For each validation rule, keep a valid base, an input that violates only that rule, and a repaired input. For each UI or runtime behavior, exercise the smallest model that reaches it.

Small cases are the unit tests, not the whole suite. Add explicit composite cases for every interaction named by the contracts, including profile × formulation, geometry operation × operand type, selection × condition, material × domain, Study × sweep mode, result recipe × artifact type, and scene × available layer. The function-coverage table must name each required interaction and link it to the case IDs that cover it. If two features can alter the same tree, Settings, validation, graphics, dispatch, or result path, test them together. Use generated combinations only where the contract declares the combination valid; include an invalid combination where the contract specifies a rejection.

The acceptance claim is conformance to the complete written format and behavior contracts, supported by this linked case suite. The suite is project-independent and needs no saved project JSONs. Tiny isolated cases cover individual rules; composite cases cover every specified interaction that can change a shared behavior. Do not claim complete coverage from case counts or schema counters alone.

| Replay group | Required evidence |
| --- | --- |
| Schema and Settings | Open the saved schema and record validation, readiness, branch and rule evidence. Record rendered icons and collection labels in each editor separately, including both geometry group variants. Record definition control names, both sweep modes, and activation and clear of the incident pair. |
| Tree commands | Record PRJ-01, PRJ-02, and PRJ-03 individually. Capture the actual menu or inline editor and compare the saved JSON before the action, before Save, and after reopening when the case changes data. |
| Graphics and results | Record each of GX-01 through GX-22 with its declared binding, observed sequence, and artifacts. Track separate evidence for deterministic checks and live browser outcomes when both are required. |
| Package and Study | Exercise the launcher and registered Schema Editor creation paths, compatibility baseline and negative controls, and the complete mesh/Study/results path in each supported profile. Match the UI selection and operation to the recorded runtime artifacts. |
| Stop and recovery | Record each required live Study and recovery outcome separately, including the two Stop stages, successful and unsuccessful newer attempts, and the recovered duration. Attach the controlled race and worker identity evidence to their corresponding cases. |

Use one row per check and required subcase. Include the spec section, case ID, starting state, action, expected and actual outcome, browser or runtime evidence, and status. Use `passed`, `failed`, `blocked`, `pending`, or `N/A` with a declared applicability reason. A gate is `passed` when every applicable mandatory row has the required evidence on that candidate. A declaration audit, smoke test, or pass on another build can guide the replay. Acceptance requires the observations for the candidate being frozen. If an identity changes during replay, start a record for the new identity and obtain current evidence before marking it passed. Keep implementation failures, missing observations, and platform blockers identifiable in the record.

## Level 1 — Schema acceptance

### Frontend provider and visible replay gate

Apply the shared [frontend provider contract](../../../platform/docs/frontend-provider-contract.md) to the identified FEM candidate. Verify FEM bindings, owned providers, commands and visible outcomes here; shared platform verification does not establish application or numerical acceptance.

- [ ] Record the frozen specification identity, pinned frontend lockfile/configuration, successful manifest revision and debug contract separately from backend/native and validation-language identities.
- [ ] Install/build through the supplied workflow. Every application/shared JSX source compiles to the expected public `.js` and source map. The browser resolves the shared React runtime locally with one instance; source maps identify original JSX.
- [ ] Public package/widget/provider imports load through their documented entries and pass the relevant JS/JSX boundary audit. For each application-owned visual provider, mount/update/dispose its JSX component through the declared package hook; preserve its source/model paths and normal callbacks.
- [ ] Reuse schema-selected shared Tree, Settings, dialog and graphics-control presentation. No handwritten application form duplicates a supported shared widget, and no schema field requires generated application JSX.
- [ ] With the actual application visible in a browser at the identified FullMeta origin, record the session/tab, fixture/live mode and candidate identity. Enable development inspection only when exercising its explicit capabilities. Export/restore a checkpoint with selection/expansion, Settings and JSON drafts, an open authoring dialog, validation state, camera/controls and focus/caret. Snapshots contain JSON data, no functions, DOM, roots or graphics resources.
- [ ] Restore establishes fixture mode before ordinary control handlers run. Unregistered fixture requests are blocked with no live fallback; operation-status restoration does not relaunch a worker, confirmation or file action. Live operations require explicit live services.
- [ ] Dispatch normal semantic commands, inspect their resolved tree/Settings/source paths and diagnostic/action bindings, and capture settled panels. Preview, Apply, Save and Validate remain distinct, with ordinary draft/availability/stale-candidate checks.
- [ ] For application-owned frontend sources, exercise the optional development watcher and selected fixture replay in that same visible tab: change an identified source and observe a new successful revision, full browser reload, checkpoint restoration and automatic selected scenario rerun. Verify compile failure retains last-good assets, no failed/stale replay occurs, and a corrected source recovers. Record that this is build/reload/replay, not React HMR.
- [ ] Check real keyboard/focus/caret, drag/reorder, dialogs, responsive layout and canvas/graphics; retain screenshots/pixels where required. Structured snapshots alone do not pass these checks. Identify separate browser/headless sessions.
- [ ] Preserve all required live validation, native execution, cancellation/recovery and checked artifact evidence from the gates below. Fixture success cannot replace it. Record command time, render/service wait, active navigation/input time, retries and errors separately for any speed claim.

Ordinary human editing requires no watcher. Development replay verifies source-change recovery; the application gate separately exercises its registered providers, paths, actions and live behavior on the same candidate.

### Compact bindings and native operation gate

Follow [Frontend provider contract](../../../platform/docs/frontend-provider-contract.md) and [Native operation contract](../../../platform/docs/native-operation-contract.md). Shared boundary acceptance does not pass this application's profile/route/numerical cases.

- [ ] Record `fullmeta-widget-catalog/1`, `fullmeta-react-widget/1`, `fullmeta-native-operations/1`, each selected operation's version/current generated digest, and retained API18/runtime/validation identities separately. Record the current specification, application and fixed-dependency identities.
- [ ] Each field selects only a widget ID/settings in its existing behavior envelope. The selected catalog owns the explicit public module. Verify compact and explicit presentation declarations resolve to the same callable ABI; missing inferred/additive exports block publication without replacing last-good assets.
- [ ] Author native bindings with the ordinary Runtime API routes operation/version/registry controls, then Preview, Apply, Save and reopen. No-op/targeted edits preserve compact source, unknown supported metadata, explicit legacy facts, model optionality and array order. Unsupported versions and conflicting inferred metadata fail before effects.
- [ ] Bind declared native-operation actions to one API role. Human controls and JSX callbacks use the current document, field handlers, draft/availability/revision guards and action service. Artifact-producing work uses normal Save and blocks failed/conflicting saves or changed models.
- [ ] On spec-derived disposable inputs, prove actual mesh check/compile/repeated compile, preparation and a required Study. Check self-described executable/authority identity, structured JSON Pointer diagnostics, NDJSON terminal/exit semantics and every required artifact. Empty solver stderr is permitted only by its explicit native descriptor exception; required other outputs remain nonempty.
- [ ] Run the native mesher profile-contract check independently for each of the five built-in profiles using a disposable valid model of the corresponding dimension. Verify that each profile's nested scenario constants and formulation declaration agree with its metadata, including a numeric integer dimension; retain a separate failing result for each profile that does not pass. This check alone does not establish successful mesh compilation or numerical acceptance.
- [ ] Verify same-ID completed reuse without another worker or output writes, in-flight duplicate observation, changed model/Study/document/schema/contract conflict, missing artifact/receipt rejection, and failed/cancelled terminal retries. Exercise ordinary Stop and bounded process cleanup with cancellation/completion terminal winner semantics.
- [ ] Keep fixture/live mode explicit. Fixture responses cannot satisfy native/math acceptance; restored operation presentation cannot relaunch work. Complete every existing profile/formulation/geometry/material/result case and API18 route requirement separately from shared platform/native-interface tests.

### Workspace, file-dialog and layout gate

The shared [workspace application contract](../../../platform/docs/workspace-application-contract.md) owns universal Explorer, picker, menu, layout and checkpoint mechanisms. Verify the FEM integration points on this candidate:

- [ ] Open a FEM model from nested project folders through the universal loader, preserving its complete canonical path, interpreting-schema reference and application registration. Distinguish equal basenames and multiple applications' model files in one folder.
- [ ] Exercise FEM Open, New schema/destination selection and applicable imports through ordinary controls and the same semantic commands. Use the in-application picker; preserve invalid raw inputs, modal ownership, cancellation and revision guards. Restore app-owned pending New/Save As/import presentation without reading, accepting, submitting or writing, including a selection from a paged listing. Invalid source/owner/path checkpoints must fail before mutation.
- [ ] Verify the declared initial layout and a session override, Stacked and Columns, the initial 50:50 Stacked split, and pointer/keyboard resizing. Existing FEM controls, drafts, graphics, selection and model data survive each layout change. The canonical path remains on the status row.
- [ ] Preserve the same-folder Save As and expected-content-hash contract, including correct relative interpreting-schema/resource references. Verify File/Edit/View/Help integration and documentation resolution for both a FEM model and the FEM schema being edited. These checks do not replace Model 2, VAL, native or numerical acceptance.

### Gate 0 — Boundaries

- [ ] The implementation uses the written function contracts and permitted inputs only; it does not access prohibited application folders or follow project schema pointers into them.
- [ ] Every acceptance input and expected outcome is derived from the written specification. Test artifacts come from those inputs or from the deterministic samples defined by the spec.
- [ ] The case suite has minimal per-profile seeds, isolated field/rule variants, and linked composite interaction cases. Every contract interaction that can change a shared behavior has at least one case ID.
- [ ] The evaluator records the fixed schema/package version and the optimized executable identities for the acceptance evidence.
- [ ] Application implementation edits are limited to `applications/{APP_ID}/`. The application root `catalog.json` is the descriptor and integration boundary. Separately authorized dependency repairs have their explicit scope, source/executable identity and independent regression evidence recorded before choosing the acceptance candidate.
- [ ] Shared Schema Editor, Model Editor, Kernel and validation dependencies stay fixed at their recorded candidate revisions throughout application acceptance. Preparatory dependency repairs require explicit task authorization; their own tests do not establish FEM acceptance.
- [ ] Browser and runtime checks use newly authored disposable cases and their own run storage. Existing project files and comparisons are outside acceptance.
- [ ] target application runtime resources resolve only inside target application or documented shared platform resources.

### Gate 1 — Contract coverage

- [ ] Every normative function has a coverage row with purpose, exact input, expected outcome/diagnostic, owner, and evidence.
- [ ] The Model 2 contract specifies every supported path and discriminator, including required/optional status, defaults, bounds, units, references, and saved shape. Verify every declared variant with spec-derived checks.
- [ ] Every tree item and field has evidence of widget, conditional display, Settings, action, and round-trip behavior.
- [ ] The UI coverage record lists the ordered controls and relevant renderer outcomes per profile, path, variant, and state. Every required row has both effective-binding and observed evidence. Check field and canvas outcomes separately from tree and heading checks. [Settings and renderer coverage](contracts/function-contract-coverage-method.md#settings-and-renderer-coverage)
- [ ] Each tree item has its backing path, parent, role, title, order, visibility, Settings behavior, and selection/highlight behavior.
- [ ] Every icon role maps to its exact token and SVG in the [icon inventory](contracts/icon-inventory.md). Record the declaration or effective descriptor that supplies the icon and the SVG that actually renders. Check the expanded Schema Editor tree and the expanded Model Editor tree separately. Include the collection headings, Physics and its condition kinds, a nested geometry operand, and an optional Parameter sweep. Verify each collection heading's label separately from its icon, including Geometry features and nested Features. Verify the expected absence of artwork on rows whose role has no icon.
- [ ] Each action has a target, inputs/defaults, owning runtime, expected output, and failure/empty state.
- [ ] Every required application-owned JavaScript and UI module has an interface, input/output contract, and observable acceptance check.
- [ ] Any behavior not supported by the written requirements or allowed documentation is recorded as a specification question or blocker; it is not guessed.

### Gate 2 — Designer-authored schema

- [ ] Authoring starts from a valid blank schema template, supplied as the application starting point or created through the editor's New application workflow.
- [ ] target application root identity, application discriminator, model format, interpreter reference, and project schema-path declaration are correct.
- [ ] Schema document validation passes. Run model-rule predicates only on spec-derived inputs in this gate.
- [ ] Schema readiness reports project creation and project editing as ready; collection actions target the correct arrays and create the declared item template.
- [ ] Every tree branch opens and exposes the expected Settings controls.
- [ ] Widgets, units, references, add actions, defaults, validation rules, graphics scenes/adapters, layer controls, and registered actions are inspected.
- [ ] Exercise all GX checks on the saved, reopened schema with spec-derived inputs. Evidence identifies the declaration, resolved binding, and visible outcome. Mark a mismatch in the authored declaration FAIL-SCHEMA; mark an unavailable shared renderer/host behavior BLOCKED-DEPENDENCY with its exact reproduction. [Graphics and results schema acceptance](contracts/graphics-and-results-schema-acceptance.md)
- [ ] Schema Editor and Model Editor context menu checks pass using spec-derived inputs. Record each menu's declaration, ordered actions, availability and reasons, and resolved target or protected declaration. [Tree context menus](contracts/tree-context-menu-contract.md)
- [ ] Before freeze, exercise PRJ-01 through PRJ-03 in the actual Model Editor in the identified visible browser on saved canonical models written from the spec. Evidence includes inline Rename, Escape cancellation, guarded draft Delete, stale-target handling, and Save/reopen with exact JSON differences. If the declaration is correct but the shared host lacks a capability, mark BLOCKED-DEPENDENCY with a reproduction; do not request a schema-builder fix. Leave an unobserved check PENDING. [Project-level command gate](contracts/tree-context-menu-contract.md#project-level-command-gate)
- [ ] TDUV counters are recorded with displayed definitions, plus branch-level and function-level inspection evidence.
- [ ] The root `x-fullmeta-rules.rules` program is executable and nonempty. Each required predicate has its rule pointer, a passing input, an isolated failing input with its diagnostic path, and a successful repair. Verify the named runtime programs separately, including built-in profile identity and `editorOptions` checks in `x-fullmeta-metadataValidation`.
- [ ] The linear range and explicit values modes each pass the [sweep unit checks](contracts/settings-control-contract.md#sweep-unit-checks). Record visible and accessible units separately from saved data. Both modes need their own observed result.
- [ ] From the saved FEM schema, use **Create project from this schema** to create and open a structurally valid FEM starter for each supported profile through its registered factory. Verify the interpreting FEM schema, declared defaults, optional-member absence, tree/Settings bindings and model validation. Reuse that FEM case's evidence for the corresponding Gate 3 creation check; no unrelated schema-only initializer case is required.
- [ ] Schema is saved through Model Editor and reopens correctly.
- [ ] Shared-editor limitations are recorded as BLOCKED-DEPENDENCY with exact reproductions. Do not hide them with platform patches or ask the schema builder to repair them.

## Application documentation delivery

- [ ] Generate or update `applications/{APP_ID}/docs/README.md` for the implemented application. Describe actual supported profiles, prerequisites, controls, validation/Save operations, native execution and results accurately; label limitations instead of claiming unimplemented target requirements. Verify its relative links.
- [ ] Open an actual valid project model in its registered editor and choose Help > Documentation. Confirm that the interpreting application schema resolves its sibling `docs/README.md` and shows the current application guide. Do not substitute a schema-authoring session for this check; editing the application schema must resolve the separate Schema Editor guide.

## Level 2 — Behavior replay

### Gate 3 — Package and native programs

- [ ] With the FEM runtime installed, Schema Editor **Create project from this schema** creates and opens a structurally valid FEM starter through the registered FEM factory and project-create route for each supported profile.
- [ ] Launcher app-card **Create project** is visible with the required profiles and creates and opens a valid starter for each profile through the same factory and route.
- [ ] Launcher app-card **Delete project** confirms the exact project folder, uses the schema-declared project-delete route, removes only that disposable project's folder, refreshes the project list, and reports failures without hiding the project card.
- [ ] target application has `applications/{APP_ID}/catalog.json` with the correct application ID, schema, loader, and package paths.
- [ ] The package and schema loader activate the target application's schema graph.
- [ ] Expected native executables and tools resolve at the paths declared by target application.
- [ ] Normal UI Studies resolve to the optimized build. Every supported profile has a verified preparation handler and numerical executable with its allowed entries.
- [ ] Program registrations use supported relative/canonical path rules and exact `allowedEntries`.
- [ ] API routes use supported handlers and declared JSON/NDJSON protocols.
- [ ] The editor compatibility check matches every declared version. Replay the matching, changed-value, omitted-value, and restored responses in a disposable browser session against the same candidate under test. Record banner text, control availability, and the absence of requests for disabled actions. [Compatibility check](contracts/runtime-integration-boundary.md#runtime-compatibility)
- [ ] For a successful Study in each supported profile, the terminal NDJSON `outputFiles` lists the same run's `run.json`, `request.json`, nonempty stdout and stage-timing logs plus the required stderr log, every case's Gmsh log, and each required field/CSV output. Verify each listed path is unique, inside the registered run, nonempty and loadable where applicable, except that the selected native descriptor may explicitly permit an empty solver stderr log. That stderr file must still exist; the exception does not permit an empty stdout, timing, field or CSV output. Missing, forbidden-empty, duplicate, cross-run or unlisted-required outputs prevent success. [Completion output contract](contracts/runtime-integration-boundary.md#expected-output-files)
- [ ] The app-owned glue in the JavaScript/UI contract exists and each interface passes its observable check.

### Gate 4 — End-to-end application behavior

Use spec-derived disposable cases for all supported profiles and exercise the declared UI and runtime together. Record the input, expected state, action, and outcome at each step. [Runtime operations](RUNTIME.md)

- [ ] Open and save an independently authored compatible model with target application routing. Deep-compare the document after an unedited reopen, including array order and optional-member presence. Replay Rename and Delete on separate disposable targets through the project-level command gate.
- [ ] Use **Save As** on a spec-derived model. Verify that the copy opens with a fresh valid Model ID, its name and every other value are preserved, and the source bytes remain unchanged. Reopen both same-folder files; both must pass native preflight and run a Study independently. Check source-name and existing-target rejection, allocation-collision retry/failure, and an expected-content-hash conflict with no copy write. Verify ordinary Save and Study automatic Save retain the current Model ID. [Save and editing behavior](UI.md#save-and-editing-behavior)
- [ ] Expand all applicable sections and select each declared item kind. Check the exact hierarchy, labels, Settings path, widgets, and applicable highlight. Exercise the GX interaction sequences, including layer availability, camera controls, and field and Plot/Table transitions. [Graphics and results schema acceptance](contracts/graphics-and-results-schema-acceptance.md)
- [ ] Validate, build and clear a generated mesh, then rebuild it. Check the new artifact and visible mesh. Run the [Study profile replay](RUNTIME.md#study-families-exercised) through the registered UI and record the Operation strip, resolved handler, stage outcomes, and required output files for each profile.
- [ ] Run the standalone lower-dimensional geometry regression with one top-level 2D rectangle, line segment, and circle curve. Validate, compile, and build the mesh. Confirm the `.geo` and geometry map preserve both curve entities, the rectangle remains the only material-covered domain, and curves are not treated as extra surfaces or domain selections. [Geometry feature behavior](../docs/fullmeta-model-2.md#geometry-features)
- [ ] Load the generated field and sweep outputs through the declared UI where applicable. Verify the resolved run, case, units, plot or field contents, controls, and reset behavior. Record any inapplicable view with a reason.
- [ ] Failures identify the stage, exact message, handler or action, and artifact state.

### Gate 5 — Operation states and recovery

- [ ] Exercise the indeterminate strip before the total is known. After the total arrives, verify left-aligned proportional fill and count. Check that 1/21 fills about 4.8% and the stage, frequency, and elapsed time remain visible during a long assembly or factorization.
- [ ] Exercise a missing or unsupported preparation route. Confirm prompt terminal failure with the profile, stage, and repairable registration diagnostic. Check that preparation dispatch stays acyclic and worker count stays bounded.
- [ ] Edit a valid model value without manually saving and start a Study. Check that the saved JSON and the request/run fingerprint describe the same inputs. A failed save or an intervening edit during save prevents dispatch.
- [ ] Hold the host `document.save` promise pending, edit the same document again, and release the save. Verify the save barrier reports that the model changed, does not create a Study snapshot, does not start the Operation strip, and sends no Study, preparation, or worker request. The complementary completed-save case dispatches exactly the saved snapshot.
- [ ] Save snapshot A in a disposable document, then save a different valid snapshot B through a second editor before submitting the Study request carrying A. Use the same registered queued route. Verify the snapshot conflict and absence of a new run, preparation, or worker. Reopen and run B to verify the normal path.
- [ ] In a disposable project with two model files, include completed Study runs for both, a mesh-only run, and a run with unverifiable ownership. Check **Clear Results** on a Study row without a result recipe. Verify that confirmation names the exact model and that Cancel or switching documents leaves all files intact. Confirm cleanup and check that only the open document's terminal Study runs were removed. Repeat with a busy project and verify no removal.
- [ ] Exercise result freshness with a display rename, recipe edit, and view change. Verify reuse of compatible artifacts, with the recipe resolving its new selection. Separately change a solver input and a parameter used indirectly or in a string expression such as a coordinate filter. Verify that the old run no longer resolves as current. If the UI offers it, verify that the explicit stale view uses the saved run's inputs, files, and persistent label. [Result freshness](RUNTIME.md#result-freshness-and-fallback)
- [ ] Stop during preparation and again during solving in disposable runs. Record the operation/run identity, original project storage, worker tree before and after, terminal UI and saved run state, and preserved diagnostic files.
- [ ] During an active Study, reopen the same canonical saved model, read its run summaries, resolve an earlier compatible completion, and load its checked artifact while the worker remains active. Verify those requests do not mutate the model, run files, operation state, or the new Study. A later compatible success becomes the newest selectable result; a cancelled or failed newer attempt leaves the earlier result selectable. [Concurrent reads](RUNTIME.md#concurrent-model-and-result-reads)
- [ ] Disconnect or reload during an active Study and reconcile the same run when the model is available. Verify the original worker identity and active, gone, and unknown liveness. Run the disposable mismatched PID/start-marker test and the deterministic completion/reconciliation race. Confirm status observation changes no manifests or locks and reconciliation preserves the first terminal state. [Controlled recovery checks](RUNTIME.md#controlled-recovery-checks)
- [ ] After a delay, reopen each terminal state and compare its fixed **Last finished run** duration with the saved timestamps. For interruption, check that the detection timestamp is persisted once and the duration is labeled accordingly. Switch between Studies with different run histories and verify that the summary uses the selected Study and document. Verify live elapsed time separately in the Operation strip. [Recovered duration](RUNTIME.md#recovered-terminal-duration)

## Independent browser evidence

The evaluator can operate the browser or direct and witness a live user-operated session using the same spec-derived cases. In a witnessed session the evaluator chooses the checks, sees the actions and their results, verifies the candidate identity, and can request a repeat.

Observe creation and reopen, tree and Settings behavior, layer controls, selection/highlight, graphics, Plot/Table and field display, the Operation strip, Stop and recovery, Clear Results confirmation, and the compatibility banner and disabled controls in a browser. API and native tests establish the corresponding backend checks. A visible check without direct or witnessed evidence stays pending.

Record the frozen candidate and spec versions, executable hashes, operator and evaluator, session time, document path, steps and visible outcomes, and the operation and run IDs where applicable. Tie loaded results to their verified manifests and artifact paths. Retain the supporting screenshots, recording, or logs with the evaluator.

## Final result record

Record the outcome for every required function. Include the exact model and Settings paths, graphics outcomes, rule diagnostics, run and artifact IDs, plot and field results, zoom/Reset checks, and every N/A reason or blocker. Separate implementation defects, spec gaps, and platform blockers. The application build passes when all mandatory spec-derived checks pass on the same candidate.

