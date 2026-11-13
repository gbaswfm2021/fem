# Designer authoring workflow

This workflow turns FEM requirements into declarations through the Schema Editor's supported UI or public semantic authoring commands. The application also implements its own package and code; the declarations, actions and observed results must stay traceable to the specification. General editor transactions and declaration editing are specified by the [Schema Editor behavior contract](../../../../platform/schema-editor/specification/contracts/BEHAVIOR.md); shared-widget behavior by the [shared UI widget contract](../../../../platform/schema-editor/specification/contracts/shared-ui-widgets.md); and neutral fixtures by the [acceptance contract](../../../../platform/schema-editor/specification/ACCEPTANCE.md). This document retains FEM's authoring traceability and application-specific evidence.

## Authoring rules

React + JSX is the application component implementation workflow, not a schema authoring prerequisite. Continue using supported declarations and shared widget selections below. Application-owned visual code, public provider registration, compiled assets and visible state/action debugging follow the [Frontend provider contract](../../../../platform/docs/frontend-provider-contract.md); keep its fixture/live mode and Preview/Apply/Save/Validate transactions explicit. A faster replay or structural snapshot does not replace saved-schema and actual runtime evidence.

1. Keep shared Schema Editor code frozen: do not modify its JavaScript, CSS, runtime, widgets, or shared schemas. Its general behavior and regressions belong to the [Schema Editor specification and acceptance suite](../../../../platform/schema-editor/specification/README.md); FEM-specific declaration and integration findings remain covered here.
2. Use the normative specification, its derived temporary typed worksheets, the supplied native interfaces and linked shared platform contracts. Build disposable examples directly from the Model 2 contract for authoring and acceptance. [Build sequence](../README.md) · [Model 2 contract](../../docs/fullmeta-model-2.md)
3. Complete the [function-contract coverage method](function-contract-coverage-method.md) using normative requirements before authoring each area. If a behavior is unclear, request a specification update rather than infer from an existing implementation.
4. Open the supplied blank target application schema in the Schema Editor or create it through the editor's application/schema workflow. Set its application identity, model format, application discriminator and interpreter path through supported designer controls or the same public semantic commands.
5. For each typed import, inspect rows, use Preview, review generated changes, Apply, Save, reopen, and validate before continuing.
6. Use supported forms, typed tables and public authoring commands for schema transactions. Preserve Preview, Apply, Save and Validate guards; semantic commands must not bypass them or maintain hidden alternate document state. Normal source editing remains available for application-owned JavaScript and JSX. Record a fixed-dependency blocker only when the supported authoring surface cannot express a required declaration.
7. Keep UI annotations on schema declarations; do not serialize them into project model JSON.
8. Use the [icon inventory](icon-inventory.md) as the normative list of target application icon payloads and UI roles. Copy the SVG values exactly into the supported declarations that each tree resolves. Inspect the projected row's source in the Schema Editor and the effective row descriptor in the Model Editor. Expand both trees and check which image each row renders.

## Requirement-to-declaration record

Each designer requirement must have a worksheet or traceability row containing:

| Column | Required content |
| --- | --- |
| Function ID | Stable ID from the functional spec, such as UI-04 or RUN-03. |
| Target | Visible tree path or root runtime-contract path. |
| Machine key | Exact serialized JSON property; never inferred from the title. |
| Type/variant | JSON value type and discriminator token. |
| Constraints | Required/optional status, bounds, enums, defaults, array length, and supported expressions. |
| Presentation | Title, description/help, unit, widget, widget settings, and assigned icon token where specified. |
| Tree/action | Parent, role, visibility, ordering, insertion action, and item template. |
| References | Target path/property, uniqueness, scope, and deletion policy. |
| Runtime consumer | Generic editor, target application module, native program, or shared documented resource. |
| Evidence | Spec-derived example and UI/runtime check proving the declaration has the intended effect. |
| Status | Specified, authored, validated, passed, N/A with reason, or blocked. |

## Suggested authoring batches

| Batch | Designer destination | Contents | Required check before next batch |
| --- | --- | --- | --- |
| A | Schema root | Identity, interpreter, model format, project schema-path field, root required keys | Reopen; document validation passes; application ID is displayed. |
| B | Reusable definitions | Identity envelopes, quantity declarations, common item structures, discriminators, local references | Inspect each definition; reject unsupported JSON Schema keywords. |
| C | Tree and collections | Shared Tree hierarchy, Definitions collection links, geometry, selections, integration operators, mesh, materials, Physics, Study, Results | Expand every node. Verify path, parent, order, visibility, and collection role. Compare the rendered icon to the inventory for each role in each editor. |
| D | Field UI | Shared widget bindings, titles, descriptions, units, defaults, conditional fields, definition tables, Keyed Choice Matrix | Select each spec-derived variant and round-trip through Settings. |
| E | References and actions | Selection references, IDs, uniqueness, add menus, insertion templates, delete/edit policies | Check the installed host's inline Rename and draft action paths. Add one of each supported item and complete the live project command checks with Save and reopen. [Project-level command gate](tree-context-menu-contract.md#project-level-command-gate) |
| F | Validation | Structural constraints, executable root `x-fullmeta-rules.rules`, and named runtime programs | Open and inspect the program after Save. Execute each predicate on spec-derived valid, failing, and repaired examples. |
| G | Runtime contract | Program registrations, path types, API bindings, request/response protocols, output checks, storage selectors | Validate registrations; confirm resolved paths stay inside target application or documented shared resources. |
| H | Graphics/results | Graphics interfaces, field plot and global plot declarations, result adapter mappings | Verify saved rule order, ports, focus targets, and widget/action bindings. Pass the schema checks with spec-derived models and checked output before freeze. [Graphics and results schema acceptance](graphics-and-results-schema-acceptance.md) |

Use **Root contracts → Flat path and value rows → Paste path/value table** for supported root contract fields without a dedicated control. Under `x-fullmeta-ui-graphicsConsole`, author the Edges dependency in both scenes at `scenes/<scene>/layers/<edge-index>/requiresVisible/0` as text `mesh`. Use the actual edge index within each scene, then Load table rows, Preview, Apply, Save, and reopen. Use typed leaf paths for the Physics `objectRoot` and `objectEntries` child under `x-fullmeta-ui-tree`, and for Settings titles and dimension-specific `selectionNames` under `x-fullmeta-ui-modelEditor`. Declare `selectionNames` as an object before adding its dimension keys `2` and `3` so the editor does not infer an array. Verify the saved declarations and their consumers together. [Tree and Settings contracts](../UI.md) · [Graphics gate](graphics-and-results-schema-acceptance.md)

## Function modeling decisions

- **Tree declaration versus schema location:** record the visible tree path separately from the reusable schema definition. A definition does not automatically create a visible tree node.
- **Visible title versus machine key:** a title is presentation; a property key is serialized data. Editing one must not silently change the other.
- **Schema ID versus project item ID:** a reusable item kind is a schema symbol; each created project item has its own stable `id` and editable `name`.
- **Discriminator versus label:** a `type` constant is a machine token; the title is a human label.
- **Schema validation versus runtime validation:** schema rules check JSON shape and declared semantics. Mesh connectivity, actual coefficients, numerical stability, and residuals belong to runtime operations.
- **Action versus implementation:** a schema action describes discoverability, target, and template. A registered runtime implements behavior beyond editor-supported declarative operations.
- **Presentation-only groups:** tree organization that has no corresponding project data path must not create a serialized property.

## Validation and TDUV evidence

After each high-risk batch:

1. Run Schema Editor document and JSON validation.
2. Inspect the affected tree branch and selected-item Settings.
3. Record TDUV using the editor's displayed definitions. T counts tree nodes, D covers field paths and declarations, U covers behavior facts and annotation keys, and V counts top-level entries under `x-fullmeta-rules.rules`.
4. Inspect the authored schema's branch audit and confirm each requirement row has a declaration or an explicitly documented runtime owner.
5. Record function IDs against observed evidence. Totals do not replace inspection of important declarations or behavior.

For the validation batch, a nonempty model rule program and evidence for every executable predicate are both required. Record V separately from the metadata, registration, launch, dispatch, preparation, and sweep programs. For each validation function, record the rule or declaration pointer, stage, input paths, selected branch, observed diagnostic and repair, and the same input's passing outcome after repair. A single rule entry may cover several predicates. Exercise each predicate independently. [Validation functions](../VALIDATION.md)

Build validation probes from the specification. To test a declaration invariant, change one declaration while keeping the model valid. To test a model predicate, change one model value while keeping the declaration fixed. To test conditional behavior, vary the condition and confirm which diagnostics appear. Verify that diagnostics point to the repairable field or registration. Verify the effective evaluator when a policy is passed to runtime. If a worksheet leaves a validation function uncovered, author its executable predicate and recheck it through the designer before closing the batch.

## Stop conditions

Pause the affected case when a declaration, action, saved value or runtime result cannot meet its contract. Classify the cause before choosing a repair: an absent application declaration or runtime owner is an application defect to fix. A correctly formed declaration blocked by an unsupported fixed designer, platform or native interface is a dependency blocker. Preserve the failing case and continue independent work.

A finding records the exact disposable case, steps, expected and observed behavior, diagnostic and owning component. Repair missing or incorrect application declarations and application-owned runtime/UI code, then rerun affected checks. A supported declaration blocked by the fixed designer/platform/native dependency is BLOCKED-DEPENDENCY; do not patch that dependency to hide the failure.
