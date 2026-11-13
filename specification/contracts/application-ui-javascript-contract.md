# FEM JavaScript and UI integration

This document specifies the application-owned JavaScript package and `ui/` modules. They open Model 2 documents, draw their models, run their declared operations, and display their results. The designer authors the data schema and shared UI declarations. Application JavaScript implements conversion, orchestration, and rendering inputs. [Model format contract](../../docs/fullmeta-model-2.md)

## Application-owned file layout

New visible application components use the supplied React + JSX workflow in [Frontend provider contract](../../../../platform/docs/frontend-provider-contract.md). Keep `.jsx` source under `applications/{APP_ID}/ui/`, compile to sibling `.js`/source maps, and import the compiled entry from the application package or public widget/provider. The conventional `.js` names below are runtime/adapter entry points; pure scene conversion, selection mapping and data adapters can remain JavaScript. A visual implementation can live in a companion `.jsx` module behind those unchanged entry-point signatures.

Keep application-specific code under `applications/{APP_ID}/`:

```text
applications/{APP_ID}/
├── {SCHEMA_FILE}
├── package.js
├── schema-documents.js
├── js/
│   ├── model-preservation.js
│   ├── model-projection.js
│   ├── operation-actions.js
│   └── result-adapters.js
└── ui/
    ├── geometry-scene.js
    ├── selection-adapter.js
    ├── result-presentation.js
    └── field-settings.js
```

The modules may be combined if the exports and ownership below remain clear. `package.js` is the package entry point and composes both `js/` and `ui/`. `js/` owns model-preserving transformations, schema/model projections, route requests, operation orchestration, and result-data normalization. `ui/` owns graphics conversion, selection editing/presentation, result controls, and visible panels. The schema remains the source of field, tree, action, runtime, and graphics-console declarations. JavaScript must not silently rewrite project documents to fit a more convenient UI shape.

### `js/` responsibilities

| Module | Required function |
| --- | --- |
| `model-preservation.js` | Clone and pass through the opened document while preserving IDs, array order, optional-member absence, unknown values allowed by the source contract, and exact JSON value shapes. Any non-mutating projection must be separate from the serialized model. |
| `model-projection.js` | Export `projectModel(model)` as an isolated read-only projection with ordered records and canonical JSON Pointer paths, per-collection ID indexes, explicit relationship edges, and diagnostics. Recipe datasets retain their owning paths and are not merged into a global ID namespace. Duplicate IDs are not silently selected; missing, ambiguous, or ineligible references remain visible. The projection never inserts effective defaults into the model. |
| `operation-actions.js` | Bind visible actions to schema-declared routes; submit the active model/document context; track progress, cancellation, cleanup scope, and errors. |
| `result-adapters.js` | Validate result response/file identity and normalize field, sweep, and table data for the declared graphics widgets. |

These are functional module boundaries; the implementation may use different filenames if `package.js` exposes the same testable interfaces and keeps the `js/` versus `ui/` ownership explicit.

## Required package exports

### `schema-documents.js`

Export:

- `loadModelSchema(schemaUrl, applicationId)`: load the schema selected by the registered application and its local references, and reject it unless `properties.application.const` equals the registered application ID.
- `activateModelSchema(model, modelLocation)`: resolve the model's relative `x-fullmeta-schema`, check that `model.application` equals the registered application ID, and activate that same schema graph.

Resolve paths from the selected project/schema or shared platform services. No target application runtime path may traverse into another application folder.

### `package.js`

Export `createApplicationPackage(options)`. Return the full package contract accepted by the shared application-package loader, including at least:

| Package member | Required target application behavior |
| --- | --- |
| `id`, `manifest`, `modelSchema`, `schemaDocuments` | Identify the application consistently using its registered ID and expose the selected schema graph. |
| `modelEditorSchema`, `modelEditorUi`, `uiProfileSchema` | Bind typed settings, tree behavior, units, validation, and application-owned panels to the editor. Provide `editorOptionsForModel(model)` from the active profile declaration for shared editor guards, and `treeDescriptorForModel(document, descriptor)` to resolve profile-specific `visibleWhen` conditions while preserving canonical model paths. |
| `modelEditorSchema.resolveModelEditorSchema`, `treeActionPlans.itemTitle` | After the shared schema resolver selects the active object variant, project its declared `x-fullmeta-ui-behavior.payload.presentation.propertyOrder` into the Settings property map. Require distinct declared property names, append unspecified properties in their existing order, and preserve the resolved schema, widget/icon metadata and model values. Resolve type headings from declared `selectionNames` for the selected role and dimension; instance display names remain model data. Viewing either projection must preserve optional-member absence. |
| `modelEditorSchema.modelUnitChoices`, `editorDomain` | Supply the definition tables' ordered units from the Model 2 unit declaration. Provide the host's `resolveModelParameters(parameters)` with the declared real/parameter runtime policy, plus `reservedParameterSymbolsForModel(model)` and `reservedExpressionSymbolsForModel(model)` as Sets for preliminary name checks. Preserve the global-scope exception for a declared `t`; full model validation checks each definition's actual scope. `modelWithResultActionSettings(model,id,type,changes)` resolves one current recipe, preserves its ID/type, returns `{model,action}` from the shared typed edit transaction and validates the candidate without mutating its input. |
| `graphicsWidgets`, `graphicsConsole`, `graphicsLayerHandlers` | Register geometry and generic plot/table widgets and connect declared scenes/layers to normalized data. |
| `createWidgetRenderers(context)` | Retain this HTML-string compatibility map only where the documented legacy hook is required; a raw React element, DOM node or mount handle is not its return value. Live schema-selected field presentation uses the catalog's declared React component/mount providers. Resolve shared Settings widgets by their registered IDs. Register `applicationCsvSourceUnits` for the file source's JSON map control. Commit through the shared Settings draft and validate keys and unit tokens against the schema. |
| `treeActionPlans(context)` | Resolve schema tree and action declarations against the real project paths. Return the menu target, ordered entries, action availability, insertion or execution handlers, and `selectItemOnContextMenu: true` for the shared host. Route Rename through the shared inline editor and return a guarded draft edit for Delete. Pass the hierarchy to the shared Tree and use the shared definition collection Settings table and Keyed Choice Matrix. [Tree context menus](tree-context-menu-contract.md#draft-editing-contract) |
| `mountEditorPanels(context)` | Synchronously mount application-owned controls with `{settingsView, settingsTable}` from the host. Return `undefined` for a legacy mount, a synchronous disposer function, or an object with synchronous `dispose()`. The host disposes it before removing old application panels/replacing the package, and on editor disposal. Use the shared bottom Operation strip for progress and Stop. |
| `actionBindings`, `actions(services, bindings)` | Bind visible target application controls to functions that use the active model/document and schema-declared runtime routes. |
| `quantityVectorComponents({ source, model, component })` | Resolve the `primaryFieldVariables` source from the selected FEM profile and Physics formulation. Return the schema-declared ordered `{symbol, label, unit}` descriptors without changing stored values; return no descriptors for an unknown source or unsupported formulation. The shared Quantity Vector widget commits edits through the host's ordinary field commands. |
| `preserveModelDocument(source)` | Return an independent clone of the JSON-compatible document, preserving exact values, array order, unknown allowed members, and optional-member absence. `prepareModelForEditor` uses this hook; projections remain separate. |
| `resultFields.formatFieldFrequency(frequencyGHz, model, action, declaredUnit, context)` | Format checked result coordinates only. Use the linked Physics frequency expression's exact declared parameter symbol and frequency unit when it is a unique match; otherwise use the profile field display unit, defaulting to Hz except EM3D's GHz. A stationary thermal result has no frequency label. |
| `selectionAdapter` | Expose the application-owned `decodeSelection(selection, geometryFeatures)` and `selectionTargets({ selection, selectedAddress, geometryFeatures, scene })` projections below. Preserve ordered sparse address rows, keep entity kinds distinct, report unresolved IDs, and resolve highlights only through stable scene target IDs. |
| `selectionActions({ loadResultAction })` | Return `{ loadResult: loadResultAction }` because the target application graphics-console scenes declare `onSelect: "loadResult"`. The generic Model Editor provides this host function. Do not return an unrelated key such as `application.result.load`; selecting a result must reach the selected recipe, result adapter, and view state. |
| `createDynamicPropertySets(context)` | Supply data for any declared Dynamic Property Set fields. Selection membership is edited by the shared Keyed Choice Matrix. |
| `prepareModelForEditor(source)` | Clone the opened document and preserve its values and optional-member presence. Use a separate projection for display and operations. |
| `saveDocument({requestJson,folder,file,value,expectedHash,copyName,allowApplicationChange})` | Use the shared save hook. Ordinary Save calls the shared model-save route with the unchanged Model ID, expected content hash, and application-change flag. Save As lists the current folder's model-document metadata through `GET /api/models?folder=...`, allocates and checks a fresh Model ID, and uses `POST /api/program/document/save` with the source path, cloned value, expected hash, and `copyName` so the shared host checks the source and atomically creates the target. Return the content hash and full workspace-relative saved file path. Do not mutate the supplied editor value. [Save contract](../UI.md#save-and-editing-behavior) · [Shared routes](../../../../platform/docs/api-and-executable-registries.md#rust-owned-routes) |
| `resolveResourceUrl(path)` | Resolve application-owned resources within `applications/{APP_ID}/` or use an explicitly shared platform resource. |
| `renderPreview(context)` | Tolerate startup calls with `model: null` and `selectedPath: null`; show/clear the empty view until a model exists. For a loaded model, draw its actual model/result scene from editor state, not a document-name/path placeholder. |
| `resultAdapters.forModel(model)` | Select the application result adapter for the model; the adapter must provide result labels and the result-data behavior needed by the editor. |
| `resultPresentation` | Implement the presentation functions listed below for field results, sweep plots, tables, and export. |
| `renderResultSettings(context)` | Display editable field-result controls such as frequency/cut-plane selection and report the state of result loading. |
| `projectStorage` | Use the schema-declared `fem_model.json`, `fem_runs/`, `fem_results/`, and `projects/` locations. |
| `launcher`, `createModelDocument` | Provide the profile list and initializer required by the target application launcher **Create project** button. This initializer is separate from opening existing project JSON; use the exact profile contract in the [Create project interface specification](create-project-interface-contract.md)). |

The package loader requires the standard package members documented in the [runtime integration contract](runtime-integration-boundary.md). This specification does not authorize changes to the shared loader.

### JSX provider lifecycle and state ownership

Package hooks remain registration interfaces. New visible field widgets use catalog-declared `fullmeta-react-widget/1` components/mount adapters; the host supplies data-only projected views and normal command callbacks. Application-panel and graphics adapters retain their documented signatures. `createWidgetRenderers` is a retained HTML-string compatibility hook, not the target implementation of new field presentation: return escaped strings only where that hook is explicitly consumed, never a React element, DOM node or mount handle. Register a new widget's public `widget.js` and catalog capability before selecting it from declarations; use public widget exports across package boundaries. [Frontend provider contract](../../../../platform/docs/frontend-provider-contract.md) defines the current provider and lifecycle contracts.

Use the single supplied React runtime and `mountReactComponent(container, Component, props, options)` for DOM-container bridges. Retain and dispose its returned `{update,inspect,dispose}` handle when the owning provider is removed; never mount twice in one container. Legacy DOM hosts are compatibility adapters, not the target architecture for new visible components. Props carry projected values, paths, diagnostics and availability; callbacks invoke existing host draft/action commands. Keep schema/model interpretation, validation and service effects outside the component. Store serializable view state separately from React roots, DOM/canvas resources and services. Resolve resources through `resolveResourceUrl` and checked artifact access as already specified.

For application panels, return that mount handle (or a disposer that releases all panel-owned roots, subscriptions and resources) from `mountEditorPanels`. The supported optional return is synchronous; a Promise or an object without `dispose()` is rejected. Cleanup runs once before old `[data-application-panel]` nodes are removed and before replacement mounting. A cleanup failure is reported and prevents replacement mounting; stale panel nodes are still removed. The shared host owns this package-level lifecycle. A renderer that replaces its own field/panel children must also dispose the resources it owns before replacing them.

Frontend build and source maps are required; same-origin revision watching and fixture replay provide the optional development surface exercised by source-change acceptance. [Frontend provider contract](../../../../platform/docs/frontend-provider-contract.md) defines their versioned interfaces, command discovery and reload behavior. A compiled view or structured checkpoint is not a substitute for browser input, layout or rendered graphics evidence.

## Minimum `ui/` module interfaces

The preview host can activate the package layout before loading a document. `renderPreview` must accept `model === null` and `selectedPath === null` without destructuring model fields or reading `.length` from the path. Guard the empty state first; normalize a view-only path to `[]` only when needed. This does not authorize inserting defaults into the saved model. Once a model exists, the real geometry/result and graphics acceptance requirements below apply unchanged.

### `geometry-scene.js`

Export a pure scene-conversion function with the equivalent contract:

```js
buildApplicationScene({ model, selectedPath, selectedValue, mesh, fieldState, visibility, camera })
  -> { dimension: 2 | 3, primitives: Array<ScenePrimitive>, bounds, selectionTargets }
```

The normalized primitives use the shared `sceneViewRenderer` input `{ dimension, primitives: [{ type, points, fill, stroke, opacity, lineWidth, closed }] }`, with drawable primitive types `point`, `polyline`, and `triangle`. Convert each declared geometry kind to finite coordinates. A composed feature keeps its own shape and the stable IDs and graphics targets of its children. [Geometry model contract](../../docs/fullmeta-model-2.md#geometry-features)

The adapter must produce a useful initial fit, preserve view state while visibility changes, and return a visible highlight for the exact selected geometry/selection. Empty geometry, unresolved IDs, unsupported feature types, or non-finite coordinates must produce a visible, path-specific message; the canvas must not quietly fall back to the application name and selected JSON path.

For an open Model 2 project, the editor chrome identifies the simulation workspace and the root node identifies the active application by its display name (`FEM`). The canonical document path remains visible in the status row, so the root node need not repeat the project name or model ID. Do not identify the project as a schema-authoring workspace or show the application ID in place of its display name. The geometry console uses a concise scene title (`Geometry` for the model-geometry scene), a truthful subtitle such as the number/type of drawable objects, and help text that describes controls actually present in the canvas. If the help mentions axis arrows, the canvas displays the labeled orientation axes.

### `selection-adapter.js`

Export functions equivalent to:

```js
decodeSelection(selection, geometryFeatures) -> { dimension, entityIds, addressRows, coordinateFilter }
selectionTargets({ selection, selectedAddress, geometryFeatures, scene }) -> stable scene target IDs
```

`decodeSelection` returns `dimension` as the serialized entity-kind token (`domain`, `boundary`, `edge`, or `point`), ordered `addressRows` shaped as `{addressIndex, membership, entityIds}`, the union of resolved `entityIds`, `coordinateFilter: {expression, explicit}`, and `diagnostics`. An absent coordinate expression projects to the effective expression `0` while `explicit` remains false; projection never inserts it into the model. `selectionTargets` accepts an address index or address membership object for a row-specific highlight and returns only target IDs present in `scene.selectionTargets`.

Rules:

1. `selection.entity` controls the entity dimension. Domain, boundary, edge, and point selections must remain distinct.
2. Each object in `selection.addresses` is one serialized address row. Preserve row count and order. A displayed “add address” action adds exactly one row; it does not duplicate or expand every feature membership.
3. Within an address, preserve every serialized key/value in `membership`. A key omitted from `membership` means Outside. Do not default omitted features to `inside` or `boundary`, add empty membership rows, or display omitted features as selected.
4. The editor exposes the three semantic states Outside, Inside, and Boundary. If the serialized contract uses sparse membership, selecting Outside removes that feature key from the row; it does not invent an unsupported serialized token.
5. Address rows combine using the project selection contract; coordinate-expression filters remain additional filters. The UI must show the same resolved set that will be passed to meshing/solving.
6. Unresolved geometry IDs are reported as errors and must not be replaced with a guessed feature.
7. Malformed membership objects, unsupported membership tokens, and membership values incompatible with the selection entity produce diagnostics and do not contribute highlight targets.

The named selection row opens the shared Keyed Choice Matrix in its Settings and drives the selection highlight. Feed the widget the declared addressable feature records and stable IDs. Use the selected object's ordered addresses and entity type to resolve the graphics target. Each Settings address group shows which feature memberships it represents. [Shared widget contract](shared-widget-contract.md)

### `js/operation-actions.js`

Export an action factory equivalent to `createApplicationActions({ services, modelSchema, modelEditorUi })`. The returned functions must be bound from `package.js` through the platform's `actionBindings` and `actions` interfaces. They use `services.draft.commit()` before an operation, `services.network` for declared routes, and `services.operation` for progress, cancellation, and finish state.

Provide visible, enabled-state-aware actions for:

- Validate current model.
- Build/preview mesh and clear only this project's generated mesh artifacts.
- Run the selected Study, report stage and case progress, Stop its active workers from the Operation strip, and reconcile the saved run state.
- Resolve/open the selected result recipe and clear only this project's generated Study/result artifacts after an explicit UI confirmation.
- Save a plot figure when the result renderer and route support export.

Each bound action must have a visible control supplied either by the schema-declared UI or by the target application package's `mountEditorPanels`/application extension. Use stable control IDs that match the `actionBindings.selector` values. The button state must reflect missing references, validation failures, and active-operation state.

For Study, register the operation and call `services.operation.beginProgress` before preparation starts. Use the same operation ID and kind for updates and Stop. Set `completed` to the number of successfully finished cases, set `total` to the positive known case count, and pass `progress` as `100 * completed / total`. Pass `progress: null` while the total is unknown. Provide the active stage in the strip message and the current frequency or other case coordinates in its detail. Finish the strip with the verified terminal state and keep the run identity for recovery. [Study lifecycle and Stop](../RUNTIME.md#study-execution-function)

Before dispatch, commit drafts, validate, and save the current document when it is dirty. Await the host save promise. After it resolves, verify the same canonical folder and filename remain open and that the document has no new unsaved edits. Only then capture the saved snapshot and send that exact snapshot to the Study route. An edit made while the save promise is pending must reject the dispatch with a recoverable "model changed while saving" message; it must not start progress, create preparation/run output, or launch a worker. Test the save barrier using a deferred host-save promise as well as a successful save.

Every action must send the current target application model snapshot and opened project identity to its schema-declared API. Do not hard-code application-specific paths, infer a program from a filename, call an undeclared route, or delete a workspace-level results directory.

### `js/result-adapters.js`

Export `createApplicationResultAdapters({ resolveArtifact, modelSchema })` and return an object with `forModel(model)`. The selected adapter must provide `pointLabel(state, { formatFrequency })` and the application quantity/scenario metadata needed to decide whether a result recipe is a field result or a tabular/sweep result.

Adapters must validate response/file identity, dimensions, quantity, frequency/parameter point, units, row/sample counts, and finite numeric values. They must distinguish a missing run/artifact from malformed data and report the exact route or artifact path in the error. Resolve artifacts through the shared checked artifact route; never read arbitrary local paths from a model field.

Field normalization must not assume every supported native field file serializes an explicit `dimension` member. If that member is present, validate it against the active project's scenario. If it is absent, infer the dimension only from unambiguous topology in the artifact (for example, non-empty tetrahedra identify 3D data and non-empty triangles identify 2D data), then validate node-coordinate width and cell references. Reject contradictory or ambiguous topology with the artifact path in the diagnostic. Return one normalized field object with an explicit dimension for the renderer. Missing metadata that can be derived safely must not make a valid field artifact unavailable.

### `result-presentation.js`

Export the `resultPresentation` object consumed by the Model Editor. It must implement these host-called operations:

| Method | Required result |
| --- | --- |
| `selectionForAction({ action, model, stored })` | Current field frequency/filter or plot/table choice. |
| `resolveBody({ folder, file, model, action, selection })` | Request body for the schema-declared result-resolution route. |
| `fieldResolution({ action, resolved, missingMessage })` | A checked field-artifact URL or a clear error. |
| `prepareField({ raw, resolved, action, resolvedAction, model, dimension })` | Normalized field state for the generic 2D/3D renderer, including data dimension and quantity metadata. |
| `selectionAfterLoad(...)`, `selectionAfterFilterChange(...)` | Stable user selection after the dataset or filter changes. |
| `prepareDataset(dataset, action, model)` | A normalized plot/table entry with the selected x column, units, rows, and any data error. For a checked external CSV, use the recipe's literal `source.columnUnits` declarations to normalize raw samples before applying display units. |
| `selectEntries(action, entries)` | Entries chosen by dataset/series/filter declarations in the model. |
| `tableColumns(...)`, `tableSectionTitle(...)` | Columns and headings matching the serialized recipe. |
| `renderPlot({ canvas, ready, action, model, parameters, renderer })` | Plot render/legend output for the registered generic 1D widget; no silent series substitution. |
| `cutPlaneChange(...)`, `fieldLoadingMessage(...)`, `fieldPlotSummary(...)` | Field-control update, stage text, and selected-result summary. |
| `sourceSummary(...)`, `freshnessNotice(...)`, `flowSummary(...)`, `missingSummary(...)` | Visible provenance, stale-run, workflow, and missing-output states. |
| `figureSaveInfo(...)`, `figureImageDataUrl(...)`, `figureSaveBody(...)`, `savedFigureLabel(...)` | Safe figure-save request and success/error presentation. |

Do not render an empty plot as a successful result. When there is no completed run, show “no completed run” with the selected Study name; keep the Plot/Table controls usable where the generic result view supports them.

Result resolution uses the canonical model file, selected Study ID, profile, input fingerprint, and checked artifact paths. Declare `resultResolve` as `readOnlyDuringOperation: true` in the schema and keep its handler read-only. Load the newest compatible completion that satisfies the recipe, including an earlier compatible completion while a newer Study remains active. Keep progress separate from the displayed completion. Report a missing compatible result explicitly. If a saved older result is offered, require an explicit choice and preserve its stale label and saved context. Every resolve/load request settles to a ready, missing, or error state. [Result freshness and fallback](../RUNTIME.md#result-freshness-and-fallback) · [Concurrent model and result reads](../RUNTIME.md#concurrent-model-and-result-reads)

For tabular results, use the recipe's serialized `xColumn` as the semantic model quantity. Resolve it directly when the artifact contains that column; otherwise use explicit output-column metadata or a checked mapping from the Study sweep parameter and units. Never rewrite the project JSON's result recipe to match a solver-specific CSV header, and never substitute a different column silently. Validate that the mapped x values are finite, monotonic where the sweep declares an ordered range, and displayed in the recipe's declared unit.

### `field-settings.js`

Export `renderFieldSettings(context)` for `package.renderResultSettings`. Display the stored quantity, frequency/filter, cut planes, data availability, and active load/error state. Commit quantity and cut changes through `updateFieldPlotRecipe` or `updateFieldCutPlanePosition` and redraw locally from the checked artifact on commit. Changing the saved case is a separate operation that resolves its own checked data. A model edit remains unsaved until Save; there is no separate Apply step for an individual quantity or cut edit. Settings must not become a second source of model values.

## Graphics declaration and controls

The schema declares `x-fullmeta-ui-graphicsConsole` and, where needed, `x-fullmeta-ui-graphicsInterfaces`. Use the documented `fullmeta-graphics-console/1` structure and register each declared adapter, widget, handler, and action. Author the rule order, scene inputs, layer roles, focus modes, and presentation in the schema. Check the saved declarations and their resolved inputs using spec-derived cases. [Graphics and results schema acceptance](graphics-and-results-schema-acceptance.md)

Use `structure` as the default scene. Resolve the selected path and data in this order.

| Selected item | Scene and graphics target |
| --- | --- |
| `field` result | `field`. Load the selected recipe's checked field artifact and field controls. |
| `plot.sweep` or `sweep` result | `globalPlot`. Resolve the checked dataset and open the Plot/Table view. |
| Geometry feature, including a group child | `structure`. Highlight the selected feature or group region by its own ID. |
| Periodic condition | `structure`. Resolve `periodicPairId`, then both boundary selections of that pair. Highlight the two targets. |
| Periodic pair | `structure`. Resolve and highlight the lower and upper boundary selections. |
| Named selection | `structure`. Resolve its addresses and coordinate filter and highlight that entity set. |
| An item with `selectionId` in Geometry, Selections, Mesh, Materials, Physics, or Integration operators | `structure`. Resolve and highlight the referenced selection. |
| Other item | `structure` with no item-specific highlight. Keep the selected item's Settings. |

For `structure`, bind the model component, parameter environment, checked preview mesh, and current selected item. For `field`, bind those inputs plus checked field data and the selected recipe. For `globalPlot`, bind selected recipe Settings and resolved table entries to the Plot and Table tabs. Field and tabular scenes both dispatch `loadResult`. A newer selection supersedes any in-flight load from an earlier selection. A late response may populate a cache but must never replace the current scene, Settings, or selection.

The scene rule for a nested geometry child uses that child's stable ID. Each focus mode follows the declared reference steps and supplies exact feature or named selection targets. Filter periodic endpoints to boundary selections. A named selection uses the union of its ordered address rows and its coordinate filter. Draw the resolved domain region, boundary, edge, or point with the accent highlight. A subtractive address and an interface highlight must distinguish the selected part from the surrounding feature. Highlight both ends of a periodic pair. Show a specific no-match message when a selection resolves to no geometry.

Use Geometry, 1D plot, 2D plot, and 3D plot as renderer labels. In layered views, draw a small orientation guide at the lower right with arrowheads, x and y labels in 2D, and x, y, and z labels in 3D. The arrows follow the viewport projection, with corresponding unit vectors and colors `#ff657a`, `#60dca6`, and `#75aeff`. Use a dark backing to keep the guide legible. Give the viewport a dimension-appropriate accessible label and interaction hint.

In Geometry, identify the visible primitive bounds, the checked mesh surface count when drawn, the sampled mesh count when capped, and the applicable highlight or no-match state. Turning Geometry off must not continue to report its hidden primitive outlines as drawn. Label the 2D hint with pan and wheel zoom and the 3D hint with rotation, wheel zoom, and the orientation guide. When showing mesh, explain that its surface triangles can include named boundaries and domain interfaces. A field summary identifies the recipe and the resolved case in its display unit, while a sweep summary identifies the datasets and checked sample counts.

Declare `x-fullmeta-ui-profiles` as a map keyed by every profile ID in the loaded FEM scenario schema documents. Each value contains only a `layout` object with integer `treeWidthPx` and `graphicsHeightPercent` members. The package maps these layout declarations into the shared `uiProfiles[profileId]` contract, filling its other required shared fields from the profile's declared display name and empty defaults. The map must have exactly one entry for each loaded profile, and the layout must satisfy the shared ranges (`treeWidthPx` 180–420; `graphicsHeightPercent` 30–80).

Use these FEM defaults:

```json
{
  "x-fullmeta-ui-profiles": {
    "acoustic-2d": { "layout": { "treeWidthPx": 260, "graphicsHeightPercent": 58 } },
    "acoustic-3d": { "layout": { "treeWidthPx": 260, "graphicsHeightPercent": 60 } },
    "em-2d": { "layout": { "treeWidthPx": 260, "graphicsHeightPercent": 58 } },
    "em-3d": { "layout": { "treeWidthPx": 260, "graphicsHeightPercent": 60 } },
    "thermal-2d": { "layout": { "treeWidthPx": 260, "graphicsHeightPercent": 58 } }
  }
}
```

These defaults apply with no saved splitter preference and are subject to the shared layout's viewport limits. Keep the user's splitter choices across tree and scene changes.

Declare the same stable layer roles and explicit `defaultVisible` values for Geometry, Mesh, and Edges in `structure` and `field`. Initialize one set of checkbox preferences for the open canonical model. Share it across structural rows, field recipes, repaint, and return from Plot/Table. Resolve each scene's data and control availability separately. When a layer lacks usable data, disable its control, give a reason, and omit the layer until eligible data becomes available. A disabled control retains its preference for later use.

| Scene | Initial Geometry preference | Initial Mesh preference | Initial Edges preference | Other initial view |
| --- | --- | --- | --- | --- |
| Model geometry and mesh | On | Off | On | Show the selection highlight when the selected path resolves to geometry. |
| Field result | On | Off | On | Field surface and applicable selection highlight on. |
| Sweep plot/table | Unavailable | Unavailable | Unavailable | Plot tab selected. Show the Plot and Table controls for the result data. |

Plot/Table hides these three controls without changing their preferences. A successful explicit Build Mesh enables Mesh for the newly verified preview. Restoration of an existing matching mesh on initial model open also enables Mesh if the user has left that preference untouched and the initial scene is still active. Declare `previewLayerRole: "mesh"` in the effective `modelEditorUi` and use the shared preview service for these transitions. A delayed load or restore must never replace the active selection, scene, or a preference the user has changed. A fresh open of a different canonical model gets a fresh set of preferences.

| Layer role | Bound data and visible effect |
| --- | --- |
| `geometry` | The model geometry and evaluated parameters. Draw neutral 2D contours and 3D boundary lines. Boolean groups draw the actual composed boundary, including thin features. Internal tessellation lines and consumed child outlines are omitted from the normal view. Selecting a consumed child makes that feature available as the focus highlight. |
| `mesh` | Checked surface mesh for the open document or the checked field artifact's mesh in its saved frame. Draw translucent faces, with material/domain boundaries discernible where tags exist. |
| `edges` | The same checked mesh data. Declare `requiresVisible: ["mesh"]`. Draw triangle wires when the data, Mesh preference, and Edges preference all permit it. With eligible wires and Mesh off, the Edges control is still enabled; changing it stores the preference without drawing wires. Re-enable Mesh to see the stored Edges choice. Geometry owns CAD outlines. |
| `field` | Resolved field surface in the current result's model coordinates. Draw it when ready. This layer has no separate toolbar checkbox. |
| `selection` | The declared focus target from the selected item and model references. Draw the accent highlight above the geometry and data overlays. |

Draw the field first, translucent mesh faces and mesh edges above it, then geometry contours and focus highlights. Keep the color scale and orientation guide above the scene. Use the topology, physical tags, palette, stroke widths, sampling, and camera fit in [Mesh appearance and scene rendering](mesh-appearance.md). The target application selection renderer owns the [CAD face highlight pattern](mesh-appearance.md#cad-face-highlight-pattern). Keep fill tessellation and visible strokes distinct so unrelated partition edges cannot alter the highlight. Use the same fit and camera for geometry, mesh, and field coordinates, based on eligible data bounds independent of layer toggles.

Tree clicks select the exact item, update Settings, and select the matching scene. Clicking a Definition collection opens its collection Settings and the structure scene. Plot and Table tabs only switch the result presentation. A stationary click in the layered canvas retains the current tree selection and Settings. Drag pans in 2D and rotates in 3D, and the wheel zooms. Preserve the camera across tree clicks, structure/field transitions, and return from Plot or Table. Reset view restores the centered fit, zero pan, and initial 3D orientation while preserving selection, checked data, and visibility. [Graphics Console contract](../../../../platform/ui/widgets/graphics-console/README.md) · [Icon inventory](icon-inventory.md)

Show recipe controls only in the selected result's Settings. The bottom Operation strip stays visible across tree and scene changes while a Study is active and displays the terminal result at the end of the operation.

For a 1D result, the graphics console provides Plot and Table views over the same selected dataset rows and series. The first activation opens Plot. Preserve the chosen tab while re-rendering the same active recipe. Table shows the coordinate and value columns from the applied recipe. Report absent 1D zoom and Reset controls as N/A. [Result projection and controls](../RUNTIME.md#results-resolution-and-plots)

## Schema/UI ownership boundary

The schema declares the project format, properties, defaults, units, references, validation constraints, tree, add-item actions, graphics scenes, runtime routes, program registrations, storage, and result-resource declarations. application JavaScript:

- converts exact project geometry/selection/result values into normalized data;
- invokes only schema-declared actions/routes;
- registers/returns the adapters and renderers the package loader expects;
- renders the application-specific field settings and operation panels;
- maps each tree path/ID to the matching graphics and Settings behavior.

Do not add a workaround to shared schema-editor JavaScript/CSS/runtime/widgets or shared editor schemas. If the frozen platform cannot express a required contract, attach the exact model, tree path, action, and reproduction and mark that requirement blocked.

For model-tree edits, route each command through the installed shared host. A host-owned inline command consumes its input through the shared editor. A declared synchronous command returns the changed document and selected path through the existing draft transaction. Check the target identity and live references before deletion. Verify the resulting draft and its persistence through Save. [Project-level command gate](tree-context-menu-contract.md#project-level-command-gate)

## Interface verification

- [ ] The catalog loads `schema-documents.js` and `package.js`; the package loader accepts the package with no console/runtime exceptions.
- [ ] The package's selected schema graph matches the model's `application` and relative `x-fullmeta-schema`.
- [ ] Every non-generic custom widget declared by the schema has a registered renderer.
- [ ] The shared Tree, definition collection Settings tables, and Keyed Choice Matrix consume the declared project paths and render the behaviors in the shared widget contract.
- [ ] Every graphics widget referenced by the console is present in `manifest.graphicsWidgets` and `graphicsWidgets`.
- [ ] Every action binding has a visible target and an action function with the same binding ID.
- [ ] For every command exposed by an FEM-owned widget, perform the corresponding interaction through a human control. It must reach the appropriate host draft, widget reducer, or declared application action with the required availability and revision guards; invalid raw drafts remain repairable, stale or unavailable commands have no effect, and persisted edits survive Save/reopen.
- [ ] `selectionActions({ loadResultAction })` returns the exact `loadResult` key requested by every result scene's `onSelect` value; selecting field, plot, and table recipes reaches the result view without an unresolved-action message.
- [ ] Every scene input/layer has a registered adapter/handler and live model or artifact data.
- [ ] Switching scenes preserves the open model's checkbox preferences, disables unavailable layers with a reason, and keeps the active path and Settings synchronized. Mesh gates wire pixels without changing the Edges checkbox.
- [ ] Opening a spec-derived document and saving it without edits preserves all model values and optional-member presence.
- [ ] Every app-owned runtime path and import resolves within the application or a documented shared platform resource.

## Shared platform references

- [Shared application package loader](../../../../platform/js/application-packages.js)
- [Shared runtime registration schema](../../../../platform/contracts/fullmeta-model-runtime-1.schema.json)
- [Graphics console contract and renderer registry](../../../../platform/ui/widgets/graphics-console/README.md)
- [Shared scene renderer input contract](../../../../platform/ui/widgets/scene-view/runtime.js)
- [target application runtime integration contract](runtime-integration-boundary.md)
- [Model/UI function contract](../UI.md)

