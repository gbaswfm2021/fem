# Level 2 behavior replay protocol

Run this level after Level 1 schema acceptance on the same identified candidate. Replay checks the actual registered application behavior using disposable model documents authored from the frozen [Model format contract](../../docs/fullmeta-model-2.md) and application specifications. It does not use saved project JSONs, copied project data, or project-specific expected outcomes. Such files are not required to be present.

## Construct the replay suite

Use the case IDs from the [acceptance checklist](../ACCEPTANCE.md) and [function-contract coverage method](function-contract-coverage-method.md).

1. Make the smallest valid model for every supported profile directly from the format contract.
2. Make focused variants for each field state, discriminator, optional value, reference state, validation predicate, and action. Use separate documents when a change would contaminate another case.
3. Make composite documents for each specified interaction that can change tree projection, Settings, validation, graphics, operation dispatch, or results. Cover the actual valid combinations and the invalid combinations that the contract requires the application to reject. Link every composite case to the clauses and function rows it covers.
4. Route each disposable document to the candidate using the deployment's application ID and a schema path that resolves from that document's directory. Give each case isolated storage and generated outputs.
5. Record the initial JSON, its digest, the candidate identity, expected outcomes, and applicable clauses before execution. Do not infer expected values from a saved project or its output.

Tiny cases isolate causes; composite cases check that individually correct behaviors remain correct together. The suite is incomplete if a documented interaction has no linked case. Counts, TDUV totals, schema validation, or one successful Study are not substitutes for this coverage map.

## Replay sequence

First complete the [frontend/provider and visible-replay gate](../ACCEPTANCE.md#frontend-provider-and-visible-replay-gate) on this candidate. Follow the delivered [development controller contract](../../../../platform/docs/frontend-provider-contract.md), with the actual application visible at the identified FullMeta localhost origin. Identify the tab/session, frontend revision, schema/package and service mode in the evidence. Supplemental headless or separate-browser runs must be named separately.

Restore the relevant JSON-only document/editor state, including selection, expansion, pending drafts, authoring dialogs, diagnostics, focus and view state. Use explicit fixture services for deterministic authoring replay and normal semantic commands for the same transactions as visible controls. Check resolved source paths, diagnostics and action bindings, then capture settled affected panels. Exercise a successful watched source revision, full reload of the same tab and automatic selected fixture replay; exercise compile-error recovery without replaying failed assets. The shared development workflow is build/reload/replay. It is optional for ordinary editing; exercise it here for source-change recovery. Automatic live scenario replay is blocked.

Fixture replay does not establish live validation-rule execution, native operations or real artifacts. The actual live cases below retain their existing execution/evidence requirements. Keep keyboard/focus, drag/reorder, modal/dialog, layout and canvas/graphics checks in the visible browser. When comparing workflows, record active input/navigation effort, normal-command time, render/service waits, retries and errors separately for equivalent starting states and outcomes.

For every applicable case, exercise the registered application and retain observable evidence:

1. Create or open the disposable model. Save and reopen it; verify model data, IDs, array order, units, and optional-member presence against the case's expected JSON.
2. Expand the declared tree sections and select every item kind present. Check its row, parent and order, Settings path and values, and applicable geometry selection/highlight. Exercise the declared actions and verify their in-memory and saved effects.
3. Exercise Geometry, Mesh, and Edges independently wherever their data is available. Verify scene changes, selected-item persistence, zoom in/out, and Reset view. Record disabled controls and their reasons.
4. Run the declared validation cases and compare the exact rule, path, and diagnostic. For valid operational models, clear only this case's generated mesh, build it, and verify the expected mesh artifact and visible result.
5. For each applicable Study profile, invoke the actual application operation through ordinary controls or the same public semantic command. Record the operation and run IDs, dispatch route, stage progress, terminal state, diagnostics, and required output files. A successful exit alone is not a pass.
6. Open each applicable result type from the generated run. Verify run/case identity, units, plausible displayed data, controls, zoom, and Reset view. Mark a view N/A only when the contract makes it inapplicable to this model, and state the reason.
7. Exercise the required cancellation, recovery, compatibility, freshness, and cleanup cases using separate disposable runs and storage. Preserve earlier completed results when the contract requires it.

Use [Graphics and results schema acceptance](graphics-and-results-schema-acceptance.md), [Runtime operations](../RUNTIME.md), and [Operation states and recovery](../ACCEPTANCE.md#gate-5--operation-states-and-recovery) for the detailed expected sequences and outputs.

## Attribute failures and iterate

Compare the observed failure in this order: authored schema declaration, resolved binding/request, application-owned package or JavaScript/UI behavior, shared editor/platform behavior, then supplied native program behavior.

| Finding | Classification and next action |
|---|---|
| A schema declaration is absent, invalid, inconsistent with the frozen contract, or supplies the wrong model path, value, UI binding, validation rule, action, or route. | **FAIL-SCHEMA.** Correct the schema. Repeat Level 1 and rerun every Level 2 case affected by that declaration. |
| The schema is correct, but application-owned package/JavaScript/UI code fails. | **FAIL-APPLICATION.** Record the owning module, case ID, steps, expected and actual outcomes, exact diagnostic and candidate identity. Repair the application-owned code and rerun affected cases; do not disguise a code defect as a schema edit. |
| The schema and application request are correct, but the shared editor/platform or supplied Rust program prevents the result. | **BLOCKED-DEPENDENCY.** Record component or executable identity, minimal case, exact reproduction, message, and affected clause. Report the blocker; do not patch the fixed dependency under this application build contract. |
| The frozen specification does not decide the expected outcome, or the test cannot be constructed from its declared data. | **SPECIFICATION-GAP.** Stop that case, state the ambiguity, and resolve the normative contract before judging the implementation. |
| The disposable input itself violates its declared starting conditions. | **INVALID-CASE.** Correct the synthetic input from the contract and repeat; do not count this as an implementation pass or failure. |

Do not alter a disposable case to make a failing implementation pass. If a general schema fix affects behavior already replayed, rerun those cases. Keep independent checks moving when one case is blocked, but do not mark a blocked requirement as passed.

## Per-case record

| Field | Record |
|---|---|
| Case ID and covered clauses | |
| Profile, focused/composite role, and intended state | |
| Candidate, schema, package, shared-host, and executable identities | |
| Frontend revision, debug/scenario format, browser session and fixture/live mode | |
| Disposable document path, initial digest, and run-storage path | |
| Expected JSON, tree/Settings, graphics, operation, and result behavior | |
| Actions performed and actual outcomes | |
| Run/operation IDs, output paths, and evidence links | |
| Failure owner and classification, or pass/N/A reason | |
| Schema or application-code change, if any, and required rerun cases | |

Level 2 passes only when every applicable mandatory replay requirement has passing evidence on the same candidate, every required composite interaction is covered, and application-owned schema and code failures are repaired. A fixed-dependency blocker remains unverified: report it and the incomplete acceptance scope rather than marking the application passed.
