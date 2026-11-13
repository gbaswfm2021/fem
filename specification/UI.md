# FEM user interface

This file specifies Model 2 project editing in target application. Use the project JSON contract for the complete serialized shapes and the shared widget contract for field bindings. The editor accepts every valid document in the declared model surface and preserves its model data after routing to target application. [Model 2 project JSON contract](../docs/fullmeta-model-2.md) · [Shared widget contract](contracts/shared-widget-contract.md) · [JavaScript and UI integration](contracts/application-ui-javascript-contract.md)

## Project document envelope

Model 2 documents use these root members.

| Root key | Function |
| --- | --- |
| `application` | Selects the registered application. |
| `metadata` | Holds model identity and presentation metadata. |
| `model` | Holds geometry, selections, mesh, materials, Physics, Study, Results, and expression definitions. |
| `modelFormat` | Identifies the serialized model contract `fem-model/2`. |
| `scenario` | Selects profile, spatial dimension, and physics family for the current model. |
| `x-fullmeta-schema` | Selects the interpreting schema by a relative path from the project JSON. |

The root schema must require the identity fields that make a project routable and valid. Model-specific UI annotations stay in the schema; they are not copied into the project document.

On opening an existing project, route through its `application` and `x-fullmeta-schema`. Preserve its `modelFormat`, metadata, scenario, model keys, IDs, array order, expressions, values, units, and optional-member presence. The launcher factory creates new documents under the separate starter contract. [Project lifecycle](contracts/create-project-interface-contract.md)

## Tree-level function requirements

The Model Editor renders the project hierarchy with the shared [Tree widget](../../../platform/ui/widgets/tree/widget.js). Author the hierarchy under `x-fullmeta-ui-tree` and use supported field annotations for child collections. target application supplies paths, parentage, labels, order, visibility, and actions as schema data. A presentation group such as Definitions has no serialized property of its own. The shared widget handles disclosure and selection, and the selected path drives Settings and the graphics highlight.

| Branch | Model data/function | Tree requirements |
| --- | --- | --- |
| Definitions | Parameters, global variables, and pointwise variables. | Expandable presentation group with three counted `collectionLink` rows. Each link opens the collection's shared Settings table. |
| Geometry features | Primitives and composed feature groups under `model.geometry.features`. | Collection with per-kind icons/titles and add actions; selecting a feature highlights its geometry. |
| Selections | Domain, boundary, edge, point, or coordinate-filtered selections under `model.selections`. | Each named selection appears once. Selecting its row highlights the resolved entity set and opens its Settings, including its shared Keyed Choice Matrix. |
| Integration operators | Named operators used by model-defined observables. | Selecting an operator shows its referenced selection and operation settings. |
| Mesh | Element family, Gmsh options, size controls, periodic pairs. | Expose mesh actions and settings; generated mesh visibility is controlled separately from geometry. |
| Materials | Domain-bound material assignments under `model.materials`. | Show each material item by name; selecting it exposes the correct profile-specific property fields. |
| Physics | Physics interface, selected domains, frequency/formulation, boundaries and periodic features. | The top-level Physics row selects the interface and discloses its direct condition rows. Each applicable condition highlights its matching geometry region. |
| Study | Harmonic/frequency-domain or stationary Study and optional sweep. | Show **Run Study** and **Clear Results** beside the selected Study's Settings. The bottom Operation strip owns progress and Stop. |
| Results | Field and sweep/global plot declarations. | Show result settings and open the result UI when saved data is available. |

For every visible node, the designer worksheet must specify parent, actual project path, node role, title, ordering, visibility condition, and whether selection drives a graphics highlight. Assign the corresponding icon token and exact SVG payload from the [icon inventory](contracts/icon-inventory.md). All tree sections must be expanded during acceptance inspection.

The project root row uses the nonempty `metadata.name`, with `metadata.id` as its fallback. Give each independently selectable model object exactly one readable tree row and one Settings target. A row with children can disclose them from the same item. Addresses, periodic ID values, and definition records are edited in the Settings tables or controls bound to their owning collection. Tree rows represent the items in the inventory below.

Use the section titles **Definitions**, **Geometry features**, **Selections**, **Integration operators**, **Mesh**, **Materials**, **Physics**, **Study**, and **Results** in that order, subject to profile visibility. Use these readable titles in declarations and disclosure labels such as **Expand Geometry features**. The shared Tree styles section headings in uppercase. Preserve the original capitalization of project and item names in the model and accessible labels, including when an item also serves as a section heading.

All resting, unselected top-level section headings use the same muted text color, including selectable object headings. Hover, focus and selection feedback follow the shared Tree styling; the selected row uses the accent color.

Use those same section titles on the source declarations that the Schema Editor projects. Set the effective declaration title where the projection gives it precedence over the tree label. Under Definitions expose Parameters, Global variables, and Pointwise variables in that order. Under Geometry features order the declared kind labels as in the [geometry add catalog](contracts/tree-context-menu-contract.md#model-editor), with editable operand collections for both Boolean kinds. Inspect the project and schema tree separately; a matching label in one view does not prove the declaration projects correctly in the other.

Start sections collapsed. A disclosure handle expands or collapses its section without selecting the bound object. Click a selectable row label to open its Settings. Expanding Definitions shows `Parameters (n)`, `Global variables (n)`, and `Pointwise variables (n)` for eligible collections. The active profile schema declares the `editorOptions` consumed by the application package and the tree's `visibleWhen` conditions; an option set to `false` hides its entry while preserving its underlying model path. `globalVariables` and `pointwiseVariables` govern their Definition links. `integrationOperators`, `periodicPairs`, and `parameterSweep` govern those entries. Acoustic and electromagnetic profiles enable all five options. Thermal shows Parameters and Integration operators, permits periodic pairs, and hides global variables, pointwise variables, and parameter sweep. A collection link opens the shared Settings table for its actual array and returns Graphics to the structure scene using the open model's retained layer preferences. Each definition is one table row identified by its symbol. [Shared widget contract](contracts/shared-widget-contract.md)

The Geometry features tree follows feature ownership. Each composed feature is the parent of the features listed in its `features` array. Expanding or collapsing the group controls those children. Selecting the parent selects the composed feature. Selecting a child selects that child's own feature data and highlight target.

Settings are scoped to the selected tree item. A composed feature's Settings show its own identity, type, and composition fields. Select a child row to edit that child's fields and highlight its geometry. A named selection's `addresses` appear only as ordered `Address 1`, `Address 2`, and subsequent groups in the Keyed Choice Matrix in its Settings. Bind the single top-level **Physics** row to `/model/physics` as `objectRoot`. This row is both the branch label and the selectable interface. Its disclosure handle reveals one `objectEntries` row for each named condition at depth 3, immediately beneath the interface at depth 2. The interface's stored `name` appears in its Settings fields. Use the condition's stored name for its tree label. Study, mesh, and result children follow the ownership paths in the inventory.

## Canonical tree item inventory

Use this table as the implementation map for tree identity and behavior. `*` denotes the current array item. An eligible empty collection shows its empty state and available add action. Every displayed row has a readable label and one canonical Settings path.

| Tree item / parent | Project JSON path | Label before section styling | Selecting the item shows | Graphics / action behavior |
| --- | --- | --- | --- | --- |
| Project root | `/metadata/name`, fallback `/metadata/id` | Project name | Project overview and identity | No geometry highlight. |
| Definitions group | Presentation group with no serialized path of its own | Definitions | No separate Settings form | Expands the eligible definition collections. |
| Parameters collection link | `/model/parameters` | `Parameters (count)` | Shared Parameters Settings table in array order | Open the default structure scene. |
| Global variables collection link | `/model/variables` | `Global variables (count)` | Shared Global variables Settings table in array order | Open the default structure scene. |
| Pointwise variables collection link | `/model/pointwiseVariables` | `Pointwise variables (count)` | Shared Pointwise variables Settings table in array order | Open the default structure scene. |
| Geometry features section | Presentation group mapped to `/model/geometry/features` | Geometry features | No separate Settings form | Expands the geometry feature rows. |
| Geometry feature row | `/model/geometry/features/*` and nested `features/*` | Feature `name`, then `id` | Selected feature's own fields | Highlights the selected primitive or composed feature. Children are nested beneath their owning feature. |
| Selections section | `/model/selections` | Selections | Section add action and selection rows | Disclosure leaves the current selection and view active. |
| Named selection row | `/model/selections/*` | Selection `name`, then `id` | Entity, coordinate filter, and ordered addresses in the shared Keyed Choice Matrix | Highlights the resolved selected entity set. An omitted membership key displays Outside. |
| Integration operators section | `/model/integrationOperators` | Integration operators | Section add action and operator rows | No geometry highlight by itself. |
| Integration operator row | `/model/integrationOperators/*` | Operator `name`, then `id` | The selected operator and its referenced selection/settings | Highlights the operator's referenced selection where it resolves to geometry. |
| Mesh section | `/model/mesh` | Mesh | Element family, size quantities, Gmsh settings, enabled-pair references, **Build mesh**, and **Delete mesh** | The preview uses a verified mesh for the active model. Scalar options and ID strings stay in Settings. |
| Mesh size control rows | `/model/mesh/meshSizeControls/*` | Control `name`, then `id` | That control's size and selection reference | Highlights its target selection. |
| Periodic pair rows | `/model/mesh/periodicPairs/*` | Pair `name`, then `id` | That pair's source/target selections and translation | Highlights its paired boundary selections. |
| Materials collection | `/model/materials` | Materials | Section add action and material rows | No geometry highlight by itself. |
| Material row | `/model/materials/*` | Material `name`, then `id` | The selected material's profile-appropriate properties and domain reference | Highlights the material's `selectionId` domain. |
| Physics interface | `/model/physics` | Physics | **Physics** heading and the interface identity, formulation, domain selections, and profile-specific settings | Opens the structure scene. Disclose this same row to reach its conditions. |
| Physics condition row | `/model/physics/<condition-key>` | Condition `name`, then stable `id`, then readable condition type | The condition type's Settings heading and only the selected condition's own fields | Highlights its boundary selection or periodic pair. Each condition appears once immediately under Physics. |
| Study collection | `/model/study` | Study | Section add action and Study rows | No geometry highlight by itself. |
| Study row | `/model/study/*` | Study `name`, then `id` | The selected Study's solver, discretization, Physics link, **Run Study**, and **Clear Results** | Run targets this Study. Clear Results targets the terminal Study runs for the open document and is available even when there is no result recipe. Stop targets the active Study from the bottom Operation strip. |
| Parameter sweep | `/model/study/*/parameterSweep` | Sweep `name`, then `id`, under its owning Study | Sweep parameter and values/range | No geometry highlight; run progress identifies the current sweep point. |
| Results collection | `/model/results` | Results | Section add action and result recipe rows | No graphics highlight by itself. |
| Result recipe row | `/model/results/*` | Result `name`, then `id` | The selected recipe and its dataset, series, filters, and table columns as structured Settings | Opens its declared field, plot, or table scene and resolves matching saved data. Child data is not emitted as blank tree rows. |

An absent optional child renders its applicable empty state. Preserve the absence until the user adds the child.

## Settings by branch

### Definitions and expressions

The model provides these collections and unit context.

- `parameters`: named scalar values that expressions can reference.
- `variables`: named global expressions and units.
- `pointwiseVariables`: expressions evaluated at field/integration locations by supporting solvers.
- `units`: project unit declarations or explicit unit context where present.
- `integrationOperators`: named operations over selected geometry used by model-defined observables.

The editor must provide typed name/value/unit fields and preserve expressions as user data. A global variable used in a sweep plot must resolve to a solver-supported observable. Do not infer expression grammar from a display name.

The Settings table identifies definition rows by `symbol`, including when their stored `name` values repeat. Display collection counts from the current serialized arrays. Each table row binds to its own record and preserves its stable `id`, `name`, expression, unit, and optional description through edits and saves.

### Geometry

Bind the geometry collection to `/model/geometry/features` and expose its length tolerance at `/model/geometry/tolerance`. Geometry actions create the declared type and fields for the active dimension.

| Feature token | Main Settings | Required graphics and tree behavior |
| --- | --- | --- |
| `rectangle` | `id`, `name`, `position`, `size`, `type` | Edit 2D position and size quantities; highlight rectangle bounds. |
| `circle` | `center`, `id`, `name`, `radius`, `type` | Edit center and radius; highlight circular boundary. |
| `box` | `id`, `name`, `position`, `positionMode`, `size`, `type` | Edit 3D position/size and the supported position mode; display in 3D. |
| `cylinder` | Base, axis, radius, height | Show its finite oriented bounds and highlight the selected cylinder. |
| `union`, `intersection` | Group identity, type, and child collection | Display the composed domain and place each owned feature below it. Select the group or its child independently. |
| `lineSegment` | Start and end | Draw the segment and highlight its extent. |
| `circleCurve` | Center, radius, and applicable normal | Draw the curve in the active coordinates and show its orientation. |
| `diskSurface` | Center, normal, radius | Draw the oriented disk and highlight it. |
| `planeRectangle` | Origin and two span vectors | Draw the planar rectangle from its vectors and highlight it. |

Support editing and preview for each declared geometry variant. Scope add and mesh actions to the dimension, geometry kind, and documented mesher capabilities. Report any unsupported operation at the affected feature or mesh stage. [Geometry shapes](../docs/fullmeta-model-2.md#geometry-features)

Union and Intersection are peer composition variants in both 2D and 3D. Both have an addressable group ID, ordered child objects, the same nested Tree behavior, and the same minimum of two children for a completed model. Selecting the group highlights the composed region. Selecting a child highlights that feature. Use the same coverage procedure for both variants, including add, child order, selection, geometry validation, preview, and mesh output. The expected geometry is the combined region for Union and the common region for Intersection.

Geometry rows support drag to reorder within their owning array and to move between the top-level array and group child arrays. Even the sole child of a group can move. Use the top and bottom quarters of a target row to insert before or after it. Use the middle half of a Union or Intersection row to append to its child array. Show the corresponding insertion line or group drop outline while the destination is eligible. The geometry row's drag hint names reordering, moving between groups, and dropping onto Union or Intersection.

Expose callable `treeBehavior` hooks from the target application package for the shared Model Editor drag interaction. Schema declarations continue to describe the rows and paths. The hooks supply the geometry move behavior.

| Package hook | Required behavior |
| --- | --- |
| `arrayPathForItem(model, path)` | Return the owning array's path for an eligible array item, including nested features. Return `null` for Study rows and object-member rows. |
| `isSpecialItemPath(path, model)` and `specialItemDragTitle(label, model, path)` | Identify top-level and nested geometry feature rows for the package's drop rules and provide the geometry drag hint. |
| `dropPlan({ model, sourcePath, targetPath, targetNode, clientY })` | Resolve the row's drop zone from its screen bounds. Return `kind: "reorder"` with `dropAfter` for a same-array reorder, or `kind: "move"` with `destinationPath`, insertion `index`, and `nested` for a parent change. Include `dropAfter` for the insertion cue on a cross-parent before/after drop. Return `null` for an ineligible target. |
| `moveItem(model, sourcePath, destinationPath, index)` | Use the shared model edit transaction for the declared move, preserving the whole item. Return the updated `model` and a message identifying the moved feature and its destination. |
| `affectsPreviewForReorder(model, arrayPath)` | Invalidate the preview when reordering geometry features, named selections, mesh size controls, or periodic pairs. |

Reject drops onto the source, into its own descendants, or into a destination that cannot accept the feature type. Preserve the moved feature's ID and fields. A sibling reorder retains the selected item's identity as its index changes. A move to another parent clears any selection path made stale by the move and leaves the feature selectable at its new path. Permit a group under construction to accept its first child as a draft. On a move that changes top-level addressability, ask the user to review selections and validate the new group before saving. Invalidate the old geometry and mesh previews when the move changes the inputs. Discard or apply a pending JSON draft before moving geometry.

### Tree action declarations

Declare each action on the schema type or collection that owns it. Insertion into an open section appends to that collection. An **after** action inserts after the selected sibling in its owning array. A group action targets the group's own child array. Use the type and dimension rules from the JSON contract to decide whether an action is available. Generate a stable unique ID, focus the new item's Settings, and preserve the order of existing items. [Exact context menu order, availability, and checks](contracts/tree-context-menu-contract.md)

| Geometry type | Action ID | Add label | Sibling insertion label |
| --- | --- | --- | --- |
| `rectangle` | `add-rectangle` | Add Rectangle | Add Rectangle after |
| `circle` | `add-circle` | Add Circle | Add Circle after |
| `box` | `add-box` | Add Box | Add Box after |
| `cylinder` | `add-cylinder` | Add Cylinder | Add Cylinder after |
| `union` | `add-union` | Add Union | Add Union after |
| `intersection` | `add-intersection` | Add Intersection | Add Intersection after |
| `lineSegment` | `add-line-segment` | Add line segment | Add line segment after |
| `circleCurve` | `add-circle-curve` | Add circle circumference | Add circle circumference after |
| `diskSurface` | `add-disk-surface` | Add circular surface | Add circular surface after |
| `planeRectangle` | `add-plane-rectangle` | Add planar rectangle | Add planar rectangle after |

On Add, create the fields below in addition to `id`, `name`, and `type`. Save each length as a quantity with the displayed number as an expression string and unit `m`. Direction and normal vectors are JSON numbers. Base the feature name on the action label without **Add**, followed by the next count for that kind, and make it unique among its siblings.

| Feature token and dimension | Stored initial fields |
| --- | --- |
| `rectangle`, 2D | `position` [0, 0], `size` [0.001, 0.001] |
| `circle`, 2D | `center` [0, 0], `radius` 0.0005 |
| `box`, 3D | `position` [0, 0, 0], `positionMode: "corner"`, `size` [0.001, 0.001, 0.001] |
| `cylinder`, 3D | `base` [0, 0, 0], `axis` [0, 0, 1], `radius` 0.0005, `height` 0.001 |
| `union` and `intersection`, 2D or 3D | `features: []` as an editable draft. Add child features to satisfy the completed-model rule. |
| `lineSegment`, 2D | `start` [0, 0], `end` [0.001, 0.001] |
| `lineSegment`, 3D | `start` [0, 0, 0.25], `end` [0.5, 0.5, 0.25] |
| `circleCurve`, 2D | `center` [0.0028, 0.002], `radius` 0.0015, `normal` omitted |
| `circleCurve`, 3D | `center` [0.25, 0.25, 0.25], `normal` [0, 0, 1], `radius` 0.1 |
| `diskSurface`, 3D | `center` [0.25, 0.25, 0.25], `normal` [0, 0, 1], `radius` 0.1 |
| `planeRectangle`, 3D | `origin` [0, 0, 0.25], `uVector` [0.5, 0, 0], `vVector` [0, 0.5, 0] |

| Action target | Action IDs and visible labels | Result |
| --- | --- | --- |
| Selections | `add-domain-address-selection` offers Add area selection in 2D and Add volume selection in 3D. `add-boundary-address-selection` offers Add edge selection in 2D and Add area selection in 3D. `add-edge-address-selection` offers Add edge selection in 3D. `add-point-address-selection` offers Add point selection in both. | Create one named address selection with the correct `entity` token and its ordered membership editor. |
| Integration operators | `add-integration-operator` offers Add integration operator. | Insert an operator with a valid selection reference from the declared choices. |
| Mesh periodic pairs | `add-periodic-pair` offers Add periodic pair. | Insert one pair object with lower/upper selections and a translation. Use the enabled-pair reference control for mesh activation. |
| Physics | `add-dirichlet-boundary`, `add-neumann-boundary`, `add-impedance-boundary`, and `add-periodic-condition` offer Add Dirichlet condition, Add Neumann condition, Add impedance condition, and Add periodic condition. | Store the condition at the key of its generated ID. Use the profile's type choices, reference eligibility, and formulation defaults. |
| Study | `add-harmonic-study` offers Add Harmonic study. `add-stationary-study` offers Add Stationary study. `add-parameter-sweep` offers Add Parameter sweep on an eligible harmonic Study. | Use names Harmonic study, Stationary study, and Parameter sweep respectively, with the matching type and linked Physics or parent Study. |
| Results | `add-global-variable-plot` offers Add Global variable plot. `add-domain-field-plot` offers Add Domain field plot. | Add `plot.sweep` or `field` under `/model/results` as allowed by the profile. Open the selected recipe's Settings and result scene. |

Bind type and collection actions through `x-fullmeta-editorActions` and the Model Editor action catalog paths. Store profile-dependent boundary, Study, and result choices in the schema metadata. [JSON defaults and shapes](../docs/fullmeta-model-2.md)

An Add selection action creates one address with a sparse membership map. If there is an addressable domain feature, set its first ID to `inside` for a domain selection or `boundary` for the other entities. Start with an empty map when there is no addressable feature. Add integration operator uses the first eligible selection and reports the missing prerequisite when there is none. Add mesh size control uses the first eligible unused selection and copies `mesh.maxSize` into its `size` quantity. With no eligible unused selection, report the required selection and keep the model unchanged.

Add Parameter sweep creates one `values` axis with the parameter's current expression and unit as its first sample. Choose the first parameter referenced in the linked Physics frequency expression, then the first parameter with a frequency unit, then the first parameter in model order. When no parameter exists, prompt the user to add one. When the Study already owns a sweep, direct the user to add a row in the existing sweep.

### Selections

Selection Settings show `id`, `name`, `entity`, and `addresses`. Show `coordinateExpression` for eligible nondomain selections. Use the Keyed Choice Matrix for the address array.

Required functions:

- Distinguish domain selections from boundary/edge/point selections by entity type and dimension.
- Preserve each address row and its order. A selected collection entry maps to one serialized row; adding an address adds exactly one row.
- Represent Outside, Inside, and Boundary. An absent membership key means Outside; do not materialize it as Inside or Boundary. Preserve each explicit membership key and value.
- Support coordinate-expression filtering in addition to address rows where the serialized document contains it.
- Interpret address rows using the serialized selection semantics and show the resolved union/filter result. Never add empty or duplicate rows while rendering or saving.
- Resolve referenced geometry IDs and reject missing feature IDs.
- On explicit deletion of an addressable geometry feature, remove its ID keys from all selection address membership maps through the shared key-reference deletion policy. Preserve address rows, their order, and other keys. Report an already-missing saved reference at its field so the user can repair it.
- Show selection feedback in the graphics panel for every selected tree item.
- Keep selection IDs stable when names change.
- Let material, Physics, mesh controls, and integration operators refer to selections through explicit IDs.

### Mesh settings

The `model.mesh` Settings show `elementFamily`, `gmsh`, `maxSize`, `minSize`, and eligible enabled-pair references. Mesh size controls and periodic pair objects have named tree rows. Select those rows to edit their own quantities and references.

The UI must distinguish editable mesh parameters from generated mesh output. Size and translation fields need quantity/unit presentation. Mesh-size controls target explicit selections. Periodic pairs identify matched endpoint entities and translation/phase behavior; only fields supported by the mesher and solver are editable. A generated mesh is shown only when a verified matching project run exists.

On the Mesh row show a **Geometry and mesh preview** panel with **Build mesh** and **Delete mesh**. Explain that Build mesh generates and opens the separate mesh preview. Run Study prepares its own geometry and reuses a compatible mesh when available. Delete mesh removes only mesh-only preview runs in the active project and leaves Study runs available. On build success show the verified mesh and its node and element counts. Keep the selected row and Mesh Settings active. [Mesh operations and cleanup](RUNTIME.md#mesh-function)

### Materials

Material declarations share `id`, `name`, and `selectionId` and use profile-specific properties.

| Profile family | Material Settings fields |
| --- | --- |
| Acoustic | `density`, `soundSpeed`, and optional `lossFactor` |
| Electromagnetic | `conductivity`, `relativePermeability`, `relativePermittivity` |
| Thermal | `thermalConductivity` |

The material `selectionId` targets a domain selection. The editor must expose only profile-appropriate material properties, their units/quantity widgets, and reference choices. Native model validation and solver checks remain responsible for expression validity and runtime numerical constraints.

In the material selection control, exclude domains assigned to other materials and retain the current material's own target. When a new assignment can use an unused eligible domain, use the first such domain in model order.

### Physics interface and conditions

Physics is an object containing an interface identity, type, formulation, domain selection IDs, wave frequency, and electromagnetic phasor convention where applicable. Conditions are named object members whose keys match their IDs. The selected condition Settings use its type.

Use `x-fullmeta-ui-modelEditor.titles` to set the Settings heading **Physics** for the interface role at path depth 2. For `/model/physics/*`, match the condition type at depth 3 and use the following headings. The tree continues to identify each condition by its stored name.

| Condition type | Settings heading |
| --- | --- |
| `dirichlet` | Dirichlet condition |
| `neumann` | Neumann condition |
| `impedance` | Impedance condition |
| `periodicCondition` | Periodic condition |

In the Physics interface Settings, show the read-only **Solver field and material variables** summary using `x-fullmeta-ui-modelEditor.fieldSummary`. Resolve the active profile and formulation and group the declared symbols into Primary field variables, Derived field variables, Zero components in this formulation, and Material variables or Material parameters as applicable. Show each symbol and its unit. Label zero components and dimension-specific zeros, and explain the supported expression and result scopes. Show a missing-definition message when the profile or formulation has no matching catalog.

- `impedance` shows **id (automatic)**, **name**, a **boundary selection** selector, and an **Impedance** expression with a unit selector. Show **Specify an incident field** while the incident field is absent. Activating it adds the **Incident factor** and its polarization vector together. Label the polarization entries with the active formulation's primary field symbols and display their units. An explicit clear removes both optional values together. [Ordinary condition widget bindings](contracts/shared-widget-contract.md#ordinary-condition-settings)
- `neumann` and `dirichlet` show **id (automatic)**, **name**, **boundary selection**, and the corresponding prescribed **value** quantity. [Ordinary condition widget bindings](contracts/shared-widget-contract.md#ordinary-condition-settings)
- `periodicCondition` shows **id (automatic)**, **name**, and **periodic pair** in that order, followed by the **phaseVector** group. Use the saved pair name for the selected option and **Choose periodic pair…** for the empty option. Offer eligible enabled mesh pairs, and report a saved pair that is no longer eligible without silently changing it. Show the phase quantities as **x-component**, **y-component**, and **z-component**, in order. Each row has a separate expression input and a unit selector, even when `rad/m` is the only allowed choice. [Periodic condition widget bindings](contracts/shared-widget-contract.md#periodic-condition-settings)

Conditions must be inserted through visible actions, use stable IDs and names, and reference valid selections/pairs. The UI must hide fields that do not apply to the selected condition and formulation. The copied solver is the authority on numerical support; a common condition’s presence in the UI does not imply every profile can solve it.

Use the condition row's context action **Disable item** or **Enable item** to update its `enabled` field. Treat omission as enabled. Show an `enabled` checkbox before the ID only when the property is stored. Opening Settings leaves an omitted value absent. Keep a disabled condition in its place with a visible disabled state and an available **Enable item** action. Preserve its ID, quantities, and references. **Rename…** updates its `name` without changing its stable ID. Keep condition keys linked to their IDs.

### Studies and result recipes

On an individual Study row place **Run Study** and **Clear Results** together at the top of Settings. Explain that Run Study prepares geometry and mesh automatically and that the Mesh row builds a separate preview. Starting a Study leaves the current row and Settings selected. Users can navigate while it runs. The Study collection and Parameter sweep row have their own Settings and actions. Keep Stop and live progress in the bottom Operation strip. [Study lifecycle and cleanup](RUNTIME.md#study-execution-function)

Study Settings include `id`, `name`, `type`, and the `physicsId` reference. Group `discretization` under **Solver settings** with **Field formulation** and **Element order**. Group `solver` under **Linear solver options** with backend, ordering, threads, and constitutive quadrature. Derive choices and effective defaults from the selected profile, linked Physics, formulation, and backend. Refresh the dependent choices when an input changes. Explain a stored value that is incompatible with those choices. Opening Settings preserves the saved values and any absent optional settings. [Study and sweep contract](../docs/fullmeta-model-2.md#study-and-sweep)

A harmonic Study can disclose a **Parameter sweep** child when one is configured. Its Settings use the shared Scan Table. Explain that independent parameter ranges form a Cartesian product and that a frequency sweep must include a parameter used by the Physics frequency expression. A stationary Study exposes the **Mean temperature reference** quantity in K and hides the Parameter sweep row and Add Parameter sweep action. Explain that the mean temperature reference applies only to components without a prescribed temperature.

Results use the declared recipe type.

- Field recipes use the **Domain field plot** Settings heading. Show the field component and part in the single Quantity control, the applicable frequency case in Data selection, and cut plane controls for a 3D result.
- Sweep quantity recipes select a supported global quantity.
- Sweep plot recipes show datasets, axes, series, output path, and table columns in their structured Settings. Selecting the recipe opens the shared Plot and Table views.

Bind the series `column` and `tableColumns` entries to global variable symbols when a sweep plot has a Study-sourced dataset. Use the shared symbol-reference declaration with exact symbol, `_re`, and `_im` suffix variants. Renaming a global variable updates those references while retaining suffixes and order. A recipe with only file-sourced datasets uses literal CSV columns.

For a file source, expose **CSV source units** under Source in the dataset Settings as a JSON object control. The keys are literal header strings and the values are supported unit tokens. Use `{}` to leave all columns unmapped. Keep this map independent of **X Unit** and each series' **Unit**. An ordinary Save of a recipe with no map leaves it absent. Applying or saving a display unit edit preserves any existing map and column keys.

Study and result references must resolve to the selected Physics interface, parameters, and dataset. Result recipes describe how to select/plot data; they do not create solver output on their own.

## Settings and annotation requirements

The tree label identifies the saved instance; the Settings heading identifies what is being edited. Resolve the heading from the effective model-editor declaration and widget, using the selected path, type, profile, and dimension. Do not title-case the stored name, replace its punctuation, or use its ID as the heading. Keep the name field and path breadcrumb available in Settings.

| Selected path or type | Settings heading |
| --- | --- |
| Model root | Model settings |
| Parameters, global variables, pointwise variables links | Parameters; Global variables; Pointwise variables |
| Named domain selection | Area selection in 2D; Volume selection in 3D |
| Named boundary selection | Edge selection in 2D; Area selection in 3D |
| Named edge or point selection | Edge selection; Point selection |
| Integration operator | Integration operator |
| Mesh and its named children | Mesh; Mesh size control; Periodic pair |
| Material | Material |
| Physics root and condition children | Physics; see the exact condition headings under [Physics interface and conditions](#physics-interface-and-conditions) |
| Harmonic, stationary, and sweep Settings | Harmonic study; Stationary study; Parameter sweep |
| Field and sweep plot recipes | Domain field plot; Global variable plot |

Bind the selection titles in `x-fullmeta-ui-modelEditor.selectionNames` by dimension and entity. Use role/depth overrides in `x-fullmeta-ui-modelEditor.titles` for object Settings where the widget needs an explicit role title: `mesh` at depth 2, `meshSizeControl` at depth 4, `material` at depth 3, and `physics` at depth 2. Set collection links and variants to the corresponding heading through their effective presentation declarations. Check the resulting heading through the shared Settings host for every variant.

For each property the spec must record the readable title, key, help/description, type, default, required status, units, bounds, and widget. Use the supported annotation envelope for field presentation. The editor should show quantities with units, references with choices, boolean options as checkboxes, enums as selectors, numeric values as steppers only when increment semantics are specified, and complex repeated data as tables or structured collections.

The UI must project a serialized two- or three-component vector of quantity objects into labeled axis controls (`x`, `y`, and, for three dimensions, `z`), with each component's expression and unit editable through the appropriate quantity controls. Saving must reconstruct the exact original array/object representation and retain each component's expression and unit pair in its original order. Component labels must convey the model axis; ordinal labels such as `[1]`, `[2]`, `[3]` alone are insufficient. A scalar unit, vector component, or selection row must not be flattened into unrelated text or silently rewritten on open.

After editing any field, confirm the project JSON contains only the model value. UI annotations, tree metadata, widget settings, and schema-authoring metadata must remain in the schema.

## Save and editing behavior

- Edit operations apply to the selected canonical project document and retain its filename.
- Save validates the current model against the selected target application schema and writes only project data.
- Save As creates a new model document in the same folder with a fresh valid `/metadata/id`. Preserve `/metadata/name`, every other metadata member, all model data, references, array order, optional-member presence, and application/schema routing exactly. The source document's bytes and identity remain unchanged. Open the copy after a successful save.
- Allocate a Save As Model ID as `ModelCopy_` followed by the 32 hexadecimal digits of a fresh UUID. Compare it case-insensitively with the source ID and the Model IDs returned by the shared same-folder model-document listing. Retry a conflicting allocation, at most 16 attempts; an unavailable listing or exhausted allocation prevents the copy write. Reject the source filename and existing target filenames case-insensitively. The shared write must atomically refuse an existing target and check the source's expected content hash. A failed check leaves the source, target, and open edits unchanged.
- Ordinary Save retains the current Model ID and filename and keeps optimistic content-hash conflict detection. A Study's automatic Save uses this same ordinary path. Both source and Save As copy must pass native preflight and remain independently usable for Studies while stored together.
- Reopen must restore all values, selected application/schema routing, tree structure, and Settings controls.
- Unsaved UI edits are not evidence of a successful serialization round-trip.

## UI checks for spec-derived cases

1. Create a disposable project from the Model 2 contract for each profile and declared variant, then load it in the target application Model Editor.
2. Expand every branch, including Definitions.
3. Select every tree item, including nested conditions and results.
4. Confirm the settings panel matches the selected item's type and path.
5. For selections and geometry, confirm the intended graphics item is highlighted.
6. From a field or plot scene, select each eligible Definitions link and verify the shared Settings table and the structure scene with the open model's retained layer preferences. Click the Physics label and check the **Physics** heading. Use its disclosure handle and verify that each condition appears once at the next depth. Click a condition and verify its exact path, type heading, and referenced highlight.
7. Reorder geometry siblings, move an eligible feature into each group kind, and drag the sole child of a draft group back out. Check the drop cues, stable IDs, resulting paths, validation, and mesh preview invalidation. Try a drop into a descendant and verify the geometry is unchanged. Verify that Study rows offer selection and their actions without a drag affordance.
8. Select the Mesh row, an individual Study, its sweep if present, and a result recipe. Verify action placement and the profile-dependent Settings choices. Confirm **Clear Results** remains available on an idle, compatible Study with no result recipes.
9. Edit one harmless value in a disposable copy, save, reopen, and confirm it round-trips.
10. Validate and record exact diagnostics for any failure.
11. In a spec-derived Periodic condition with a saved phase vector and omitted `enabled`, verify the ordered Settings rows, exact pair selector, and the three separate phase expression and unit controls. Change the pair and one phase expression, then Save and reopen. Deep-compare the saved model with the input, expecting only the pair reference and selected expression to change. Check that **Disable item** and **Enable item** update the exact optional field and show the corresponding tree and menu state. [Periodic condition widget checks](contracts/shared-widget-contract.md#periodic-condition-settings)
12. Open spec-derived Dirichlet, Neumann, and Impedance conditions in their supported formulations. Verify the literal identity and reference labels and the effective quantity widgets. For Impedance, check the no-edit round trip with no incident payload. Activate **Specify an incident field**, verify the primary field symbol labels and units, edit the **Incident factor** and one polarization entry, then Save and reopen to verify both quantities and their stored shape. [Ordinary condition widget checks](contracts/shared-widget-contract.md#ordinary-condition-settings)



