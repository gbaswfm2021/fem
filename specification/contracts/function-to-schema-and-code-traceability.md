# Function-to-schema-and-code traceability

Use this traceability record for FEM requirements; the shared application-specification convention defines the reusable document pattern. Describe behavior first; then map that behavior to the schema, FullMeta rules, application JavaScript, generic platform service, native executable, and an observable check. A schema declaration or a passing counter is evidence only for the part it describes.

## One record per function

Every model, UI, runtime, and validation function has one stable requirement ID and a traceability record with these fields:

| Field | Required description |
| --- | --- |
| ID and purpose | Stable ID and the user or data risk the function addresses. |
| Input contract | Exact JSON Pointer or runtime input, JSON type, required/optional status, allowed values, units, and references. |
| Result contract | Exact serialized value or visible state, including whether any source member may be omitted. |
| Schema declaration | Definition and field paths, required/enum/array constraints, references, root contract, and applicable schema annotation paths. |
| UI annotation behavior | The declared tree role, title, visibility, ordering, widget, widget settings, action menu, graphics scene/layer, or result view, plus what the user sees. When an icon is required, record its target application role and token from the [icon inventory](icon-inventory.md). |
| FullMeta validation | Rule stage, purpose, input paths, predicate, diagnostic, and whether the rule stops a later stage. If no rule applies, state why. |
| Runtime owner | Generic platform, target application `js/`, target application `ui/`, supplied solver executable, supplied mesh executable, or an explicitly shared resource. |
| Runtime contract | Route/registered executable entry, request fields, response/event shape, expected files, and error/cancellation behavior. |
| Preservation rule | What must remain byte-for-byte or structurally unchanged when opening, editing, and saving a project. |
| Evidence | Spec-derived valid and invalid examples, exact UI steps, expected output, and the artifact or log proving the result. |
| Status | Specified, schema-authored, app-owned implementation complete, verified, N/A with reason, or blocked with reproduction. |

Do not leave a row with “handled by the UI”, “validates model”, or “behaves as expected”. Name the actual serialized key, owner, effect, and evidence.

## Describe validations as functions

Write a validation in this order:

1. **Purpose:** what incorrect or ambiguous model state it prevents.
2. **Inputs:** exact paths or external registration/context values it reads.
3. **Predicate:** the condition for acceptance, stated independently of a specific implementation language.
4. **Diagnostic:** the message and the path/value needed to fix the problem.
5. **Stage and scope:** document/schema, metadata, launch, registration, preparation, dispatch, model, geometry, mesh, Study, sweep, or result loading.
6. **Evidence** uses a passing spec-derived model and a copy that fails this predicate in isolation. Repair the value and check that it passes again.
7. **Implementation:** the supported JSON Schema constraint or FullMeta validation DSL expression; use application-owned native validation only when the declared boundary says the check requires geometry or numerical facts unavailable to the schema.

### Example: unique selection addresses

| Requirement | Specification |
| --- | --- |
| ID / purpose | `VAL-SEL-ADDR-UNIQUE`; prevent two address rows from selecting the same region after sparse Outside values are interpreted consistently. |
| Inputs | `/model/geometry/features/**/id`, `/model/selections/*/entity`, `/model/selections/*/addresses/*/membership`. |
| Predicate | Membership keys must be known domain feature IDs. For duplicate detection, normalize an omitted feature key to the declared Outside state, preserve the serialized row, and require each canonical address within one selection to be unique. |
| Diagnostic | Identify the selection and both duplicate row indexes; say that the rows select the same address. |
| Stage / owner | Model validation; express the structural/reference predicate in FullMeta rules. Runtime selection decoding must use the same sparse semantics. |
| Evidence | Make a spec-derived domain selection with two distinct addresses. Copy one address and confirm the duplicate rule fails. Set one member to Outside and verify the widget removes that key. Save and reopen to verify the sparse representation. |

This format explains why the validation exists and lets another designer express it in the supported FullMeta language without copying a prior schema or adding an unreviewable JavaScript check.

## Describe UI annotations as observable functions

For every `x-fullmeta-ui*` declaration, record:

- its exact schema JSON Pointer and annotation key;
- the model/tree path it applies to;
- what the user can see or do because of it;
- the component that consumes it (generic editor, package, graphics widget, or result adapter);
- its relevant state, conditions, and failure/empty behavior; and
- a UI acceptance step that proves the declared behavior.

For example, a `collection` tree role must name its array path, child template, insertion target, visibility condition, and selected-item Settings path. A graphics layer must name its renderer input, default visibility, independent control, and how reset/selection state is preserved. An annotation key with no consumer is not an implemented function.

The [icon inventory](icon-inventory.md) defines literal target application icon payloads by UI role. Assign the listed token and preserve its SVG string exactly. Any annotation that changes visibility, editing, selection, validation, actions, graphics, or results is also in scope.

## Data-driven behavior

For application-owned visual behavior, trace the declaration and model/data path through the pure projection, compiled JSX component/public provider, normal host command and persisted outcome. The [Frontend provider contract](../../../../platform/docs/frontend-provider-contract.md) supplies build/source maps, lifecycle and same-visible-browser state/replay interfaces. Record those frontend identities separately from native/API and validation-language versions. Shared-widget rendering remains shared ownership, and fixture replay is not evidence of native execution.

- Core target application code reads item identities, names, dimensions, frequencies, references, and order from the open model. Use the scenario and schema-declared routes to resolve profile capabilities and runtime handlers.
- Derive authoring examples from the Model 2 contract. Keep the final acceptance projects and their data with the evaluator.
- Schema annotations and typed definitions control field presentation and data shape. target application `js/` owns application-level conversion and runtime request/result logic. target application `ui/` owns application-specific visible components and scene adapters. `package.js` composes them.
- Shared platform behavior is consumed through documented interfaces; do not fork or patch shared schema-editor code to make an annotation appear to work.

## Reusable review gate

Before schema authoring, every function in scope must map to at least one traceability row. Before implementation is called complete, every row must map to an authored schema declaration or an owned runtime interface and to evidence. Rows without evidence remain open; rows that cannot be expressed or verified under the frozen editor are marked blocked with the exact reproduction. This table structure is intended to be reusable for the next application by replacing the application-specific model paths, profile declarations, and runtime owners.



## Frontend and native boundary traceability

Apply [Frontend provider contract](../../../../platform/docs/frontend-provider-contract.md) and [Native operation contract](../../../../platform/docs/native-operation-contract.md) to these rows. Model 2 and VAL predicates retain their defined meanings; interface evidence supplements numerical acceptance.

| Boundary | Current identity and declaration | Required evidence |
| --- | --- | --- |
| Field selection | `fullmeta-ui-behavior/1`, widget ID/settings, original data/schema paths | Human and MCP edits share raw/typed drafts; Apply/Save/reopen preserve unknown metadata, absence, values and order. |
| Public presentation | `fullmeta-widget-catalog/1`, explicit module, normalized `fullmeta-react-widget/1` defaults and additive exports | Compact/explicit ABI equivalence, checked pending build/link exports, actual component/mount/update/dispose and source-map browser paths. |
| Operation selection | Existing runtime API route plus `operationContract.id/version`; current `fullmeta-native-operations/1` authority digest resolved once | Normal selector/Preview/Apply/Save/reopen; current native self-description; rejected stale/unsupported/conflicting bindings before effects. |
| Host execution | Declared `nativeOperation` action/API role; normal UI/WebMCP `operation.execute` | Same visible document and source revision, availability/draft/save/busy guards, actual endpoint/native launch and checked result/event identity. |
| Diagnostics and artifacts | Native operation1 DTOs, JSON Pointer transport paths, original domain messages and descriptor-specific output rules | Invalid input, real failure, missing/wrong-run output, terminal/exit/cancel races, stderr exception only where declared. |
| Durable retry | Original operation ID with exact model/Study/document/schema/contract fingerprint | No second worker on completed/failed/cancelled retry; in-flight observation, source conflicts and missing receipt/artifact rejection. |
| Workspace/file presentation | [Workspace application contract](../../../../platform/docs/workspace-application-contract.md): schema-selected workspaceEditor, complete relative paths, shared picker commands, default/session pane layouts | Nested project routing, human/MCP equivalence, no OS picker, zero-effect raw checkpoint restore, paged selection, unchanged same-folder Save As and schema-relative bindings. |
| Existing compatibility | `fullmeta-model-editor-api/18`, `fullmeta-model-runtime/1`, original Model 2 and VAL obligations | The complete application route/profile/formulation/result matrix remains required; shared native-interface tests do not pass it. |

Record the current normative specification, application candidate and identified fixed platform/native dependencies. Repair application-owned declarations and code; only fixed shared/native components remain outside the application builder's repair scope unless the task explicitly includes them.
