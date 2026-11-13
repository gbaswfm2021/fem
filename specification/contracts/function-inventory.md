# target application function inventory

Use these function IDs for traceability. Expand each row into the paths, variants, conditions, outcomes, and evidence in its linked contracts. A function may need several checks. [Build sequence](../README.md) · [Traceability](function-to-schema-and-code-traceability.md)

| ID | Required function | Verification and governing contract |
| --- | --- | --- |
| `UI-01` | Author and reopen the schema | Use the Schema Editor to create, save, reopen, validate, and inspect each declared branch. [Designer authoring](designer-authoring.md) |
| `UI-02` | Create a project from the schema | With target application registered, exercise **Create project from this schema** and verify that the registered factory creates and opens a structurally valid starter. [Launcher and schema project creation](create-project-interface-contract.md) |
| `UI-02b` | Create and delete projects from the launcher | Create and open one starter for each profile. Confirm project deletion and check its scope on a disposable project. [Project lifecycle](create-project-interface-contract.md) |
| `UI-02c` | Open and save a routed Model 2 document | Route a compatible example authored from the specification and check its data and optional-member presence after save and reopen. [Project JSON contract](../../docs/fullmeta-model-2.md) |
| `UI-03` | Navigate project hierarchy | Expand each shared Tree section and select each named item. Check its path, scoped Settings, and graphics behavior. [Model and UI functions](../UI.md) |
| `UI-04` | Edit Settings through shared widgets | Check the bound widget, labels, units, ordering, conditional availability, and persisted edit for each variant. [Shared widget contract](shared-widget-contract.md) |
| `UI-05` | Create and manage model items | Use each declared add, duplicate, rename, move, and delete action in the actual project editor. Check its template, ID, target, selection, dialog and cancellation where applicable, and saved/reopened behavior. [Model and UI functions](../UI.md) · [Project-level command gate](tree-context-menu-contract.md#project-level-command-gate) |
| `UI-06` | Resolve references and address selections | Verify the eligible choices, stored IDs, Keyed Choice Matrix, and resolved entity highlight. [Shared widget contract](shared-widget-contract.md) |
| `UI-07` | Validate model declarations | Check structural bounds and exercise each executable FullMeta rule with passing, failing, and repaired input. [Validation functions](../VALIDATION.md) |
| `UI-08` | Preview geometry | Check each supported feature variant, nested group, fitted scene, and selection highlight. [Graphics and package contract](application-ui-javascript-contract.md) |
| `UI-09` | Build, restore, and clear project mesh | Check compile, Gmsh, verified mesh/report, matching preview and appearance, guarded visibility changes, and scoped cleanup. [Runtime operations](../RUNTIME.md) · [Mesh appearance](mesh-appearance.md) |
| `UI-10` | Prepare, run, Stop, and recover a Study | Exercise each profile's real handler, progress states, cancellation and restart recovery, missing-route failure, and checked completion. [Study lifecycle](../RUNTIME.md#study-execution-function) |
| `UI-11` | Resolve, display, and export results | Open checked generated fields and tabular data, select cases, compare plot/table rows, and verify figure export. [Result lifecycle](../RUNTIME.md#results-resolution-and-plots) |
| `UI-12` | Control the Graphics Console | Check fresh defaults, data availability, preferences shared across views of the open model, and Mesh-gated edge pixels. Verify selection, Settings, zoom, and Reset. [Graphics contract](application-ui-javascript-contract.md#graphics-declaration-and-controls) · [Graphics and results schema acceptance](graphics-and-results-schema-acceptance.md#scene-selection-and-control-checks) |
| `UI-13` | Scope operations to the current project | Check operation and run identity, storage paths, and separate mesh and result cleanup. [Runtime integration](runtime-integration-boundary.md) |
| `UI-14` | Resolve the package and native registrations | Check each app resource and allowlisted entry, the optimized binaries, and the checked output contract. [Runtime integration](runtime-integration-boundary.md) |
