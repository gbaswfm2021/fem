# target application Settings control contract

This contract specifies the controls that appear inside a selected item's Settings. The [tree and Settings inventory](../UI.md#canonical-tree-item-inventory) specifies the heading and owning path. The [shared widget contract](shared-widget-contract.md) specifies widget bindings. Resolve each control from the saved schema for the active profile, dimension, item kind, and stored field presence.

For each applicable case below, open a disposable model written from the specification. Select the owning item, check the ordered visible controls and their accessible names, and record the effective declaration and field path. Check choice availability, editability, and help text separately from the Settings heading. Use Save and reopen to confirm that the exact edited values changed and the other stored values and optional members were preserved.

## Quantity values and display units

Treat every saved `expression` and `unit` as a pair. The displayed value and its visible and accessible unit labels must describe the same physical quantity. If the control displays the saved expression verbatim, label it with its saved unit. If the control uses a different compatible display unit, convert the value from its saved unit before displaying it. Use the declared unit conversion, including any offset. Apply this independently to each vector component, range endpoint, and table entry. A display unit supplied by a parameter or widget config is a target unit for conversion. For a stored expression that the control cannot convert while preserving its meaning, display the expression in its saved unit. An incompatible unit request needs a diagnostic.

A read or an unedited Save/reopen preserves the saved expression and unit. If a control supports editing in a converted unit, verify that the committed pair represents the quantity entered. For example, `0.002 m` shown in a fixed millimetre display is `2 mm`. Saving without edits preserves `0.002 m` in JSON. A sweep start stored as `1 kHz` can be shown as `1 kHz` or, in a control configured to display Hz, as `1000 Hz`. The sweep editor below uses the stored unit for each endpoint.

## Definition tables

| Tree link | Settings heading | Accessible table name | Ordered columns |
| --- | --- | --- | --- |
| Parameters (`n`) | Parameters | Global parameters | Symbol, Expression, Unit, Value, Description, Actions |
| Global variables (`n`) | Global variables | Global variables | Symbol, Expression, Unit, Description, Actions |
| Pointwise variables (`n`) | Pointwise variables | Pointwise variables | Symbol, Expression, Unit, Description, Actions |

Use the collection declaration's exact title for the Settings heading and its `definitionCollections` `tableLabel` for the accessible name. `n` is the number of stored entries. Show each definition on one row in array order with an accessible label that identifies its symbol. Unit is a selector. Show the resolved parameter value read-only in its declared unit. Give unavailable values a diagnostic. Use **Add parameter**, **Add global variable**, or **Add pointwise variable** for the corresponding table and disable a move that has no adjacent row. Test empty and populated eligible tables and check that changing one expression and reopening preserves the other records, IDs, and order.

Check the saved collection declaration titles for `/model/variables` and `/model/pointwiseVariables` separately from the tree labels and `tableLabel`. The Settings heading comes from the collection declaration. For the accessibility check, add two definitions with distinct symbols and the same nonempty description. Their expression, unit, and row actions must remain individually identifiable by symbol. Verify the saved descriptions stay the same. Record a shared-widget blocker if the installed definition table cannot distinguish the rows through its supported interface.

## Geometry quantities

Use the shared Quantity Vector for ordered geometry vectors of length quantities. The component rows appear in model axis order and contain separate expression and unit controls. Declare the following axis labels, including case and suffix. Use the active dimensionality where a field supports both 2D and 3D.

| Field | Ordered axis labels |
| --- | --- |
| `position` on a rectangle or box, `base` on a cylinder | x-position, y-position, and z-position in 3D |
| `size` on a rectangle or box | x-size, y-size, and z-size in 3D |
| `center` on a circle, circle curve, or disk; `start` and `end` on a line segment; `origin` on a plane rectangle | x-coordinate, y-coordinate, and z-coordinate in 3D |
| `uVector` and `vVector` on a plane rectangle | x-component, y-component, z-component |

Use the compatible length units from the effective declaration and label each control with its axis, for example **x-position expression** and **x-position unit**. Keep the box's position mode as its own enum control. Numeric direction or normal vectors keep their declared numeric shape. A child feature opens its own quantities. Check a two-axis quantity vector and a three-axis quantity vector with different expressions and units, edit one component, and verify the exact ordered quantity objects after reopening.

## Selections and addresses

Use the **Feature membership addresses** group with the shared Keyed Choice Matrix for `/model/selections/*/addresses`. Keep the matrix inside the selection's Settings. Put the legend above the rows with the text **Membership codes: 0 = Outside, 1 = Inside, \* = On boundary. Addresses are combined as a union.** Configure `choiceHelp` for the legend, `rowLabelTemplate` as `Address {index}`, `removeLabel` as `Remove address`, and `removeAriaLabelTemplate` as `Remove address {index}`. Each feature has one membership selector in each stored address. Use the option names **Outside**, **Inside**, and **On boundary** where the selected entity permits them. The accessible label is **Address {row}: {label}**. Show **Add address** beneath the addresses and honor the minimum row count on removal. Use the declared empty-choice and missing-reference diagnostics. [Matrix bindings](shared-widget-contract.md#keyed-choice-matrix-for-selections)

On nondomain selections, set `coordinateExpression` to use Value Control with title **Coordinate filter (equals zero)** and `optionalToggleLabel` **Restrict by coordinate**. Leave `showDefault` absent or false so a missing value shows the presence control. Activating it creates an editable expression initialized to `0`. A stored expression opens as its textbox and filters the resolved address union by its zero set. Omit this control for domain selections. Opening and saving a selection with a missing filter preserves that omission. Check a domain selection, a nondomain selection with an omitted filter, and one with an explicit filter. Verify Add and Remove address on distinct rows and the exact sparse membership keys after editing.

## Mesh parent and named children

The **Mesh** parent contains the **Geometry and mesh preview** panel with **Build mesh** and **Delete mesh**, and Settings for the element family, Gmsh options, **Maximum element size**, **Minimum element size**, and **Periodic pair meshing constraints** where enabled by the profile. Gmsh element order is fixed at `1` because the current converter accepts linear triangle and tetrahedron cells; EM3D Study discretization order is a separate setting and supports orders `1` and `2` for the electric formulation. Use Quantity controls for the size bounds and a Reference Selector for the enabled pair ID list. Show each pair's readable name while storing its ID. Put the `meshSizeControls` and `periodicPairs` arrays in the tree and mark them hidden in the parent's generic Settings. Their objects open through their own rows.

| Child Settings heading | Controls owned by that child |
| --- | --- |
| Mesh size control | Stable automatic ID, editable name, **Geometric selection** reference selector, **Target element size** expression and unit. Explain that the global minimum and maximum constrain the target size. |
| Periodic pair | Stable automatic ID, editable name, lower and upper **boundary selection** reference selectors in endpoint order, and Quantity Vector for `translation` with x-component, y-component, z-component where supported by the dimension. Explain translation and the separate Physics condition's role in matching field values. |

Check a Mesh with a local size and pair. Assert that the parent shows the pair-enabling control and the named child objects appear only in the tree. Open the local size and pair, check the selectors and quantity controls, edit a child quantity, and verify the parent fields and the other child stayed unchanged.

## Material controls

In **Material** Settings, show the automatic ID, editable name, and **domain selection** selector before the **Material properties** group. Use the shared Dynamic Property Set widget bound to an application source registered by `createDynamicPropertySets`. The provider resolves the active profile's ordered material descriptors and supplies each control's key, label, field schema, and effective default. Preserve the flat model fields and their representations. Present a default for an omitted optional property and preserve its absence on open and an unedited Save. [Dynamic Property Set widget](../../../../platform/ui/widgets/dynamic-property-set/runtime.js)

| Profile family | Ordered controls under Material properties |
| --- | --- |
| Acoustic | **Density** quantity in `kg/m^3`, **Sound speed** quantity in `m/s`, **Loss factor** dimensionless expression |
| Electromagnetic | **Relative permittivity** dimensionless expression, **Relative permeability** dimensionless expression, **Electrical conductivity** quantity in `S/m` |
| Thermal | **Thermal conductivity** quantity in `W/(m*K)` |

Use a Reference Selector scoped to the root at `['model', 'selections']` with a domain entity filter, stable `id` values, and readable names. Set `uniqueWithinCollection` to `['model', 'materials']`, `uniqueProperty` to `selectionId`, and `exceptSiblingProperty` to `id`. Offer the current target and eligible unused domains. Keep a stored unavailable reference visible with its diagnostic. In a model with three domains and two assignments, open one assignment and check that its own and the unused domain are offered. The other assignment's target and nondomain selections are excluded from eligible choices. Change a property and verify the exact stored expression or quantity and the untouched reference. [Material field shapes](../../docs/fullmeta-model-2.md#materials-and-physics)

## Study and sweep controls

The individual Study Settings put **Run Study** and **Clear Results** together at the top. Use **Solver settings** for the Related Fields group. Render **Field formulation** and **Element order** using the declared choices and the effective Physics link. The group also renders the sibling `solver` fields once, with the declared **Solver options** sibling heading for the linear solver options. Backend-dependent controls refresh when the backend changes. Keep the Study's explicit Physics link visible. Show the stationary Study's **Mean temperature reference** quantity in K. A named Parameter sweep appears as its own child of an eligible harmonic Study. The parent omits a second editor for that child.

In **Parameter sweep** Settings, title the shared Scan Table **Independent parameter ranges**. Show **Parameter** and **Values** columns. Offer **Choose a parameter…** as the empty choice. Label eligible choices using `{description} · {symbol}`, with the symbol available when description is empty. Exclude a symbol already used in another row. Use **Add parameter** and **Remove** and set `parameterRemoveAriaLabelTemplate` to `Remove {parameter} from the Parameter sweep`. Show **Start**, **End**, and **Count** for a linear range and numbered entries for explicit values with the applicable units. Explain the Cartesian product of independent ranges and the requirement that a frequency sweep vary a parameter used in the Physics frequency expression. Check both modes, two parameters, the exact removed row, and the disabled add or remove state at the declared limit. Save and reopen to verify row order, values mode, expressions, and units.

### Sweep unit checks

Use separate linear-range and explicit-values cases. For each, record the visible expression, its adjacent unit, and the textbox's accessible name separately from the saved JSON result. Each accessible name identifies the parameter, the endpoint or value index, and that quantity's own unit.

| Case | Initial display and accessible names | Save and reopen |
| --- | --- | --- |
| Linear range | Use a frequency parameter declared in Hz with Start stored as `1 kHz` and End stored as `3000 Hz`. Expect `1` beside `kHz` and `3000` beside `Hz`, with names ending in `Start in kHz` and `End in Hz`. | An unedited Save/reopen preserves both quantity objects. Edit Start to `1 + 0` and reopen. Only the Start expression changes, its unit stays `kHz`, and the display continues to identify both endpoint units correctly. |
| Explicit values | Use a length parameter declared in mm with entries stored as `20 mm` and `0.025 m`. Each entry shows its own saved expression and unit and has an accessible name identifying its one-based index and unit. | An unedited Save/reopen preserves the values. Edit the second expression to `0.025 + 0` and reopen. Only that expression changes, both units and the entry order persist, and each label still agrees with its displayed expression. |

For a linear range, pair the expression and unit from `values.start` for Start and from `values.end` for End. Use those same resolved units in the accessible names. The parameter's declared unit can initialize an endpoint or supply a fallback when no unit is stored for it. It cannot replace a stored unit when displaying that endpoint's saved expression.

Pass each mode only when its visible and accessible presentation and its persistence checks all pass. For a shared-widget blocker, record the installed widget version, the saved parameter and quantity objects, and the actual and expected visible units and accessible names. A successful JSON round trip or a pass in the other mode does not clear that blocker. [Quantity display rule](#quantity-values-and-display-units)

## Field and sweep recipe controls

Use the **Domain field plot** heading. Give the active recipe one combined **Quantity** selector and one **Data selection** control backed by the selected artifact's checked cases. Apply the declared quantity and part order, missing-sample behavior, and profile frequency display units. A stationary field presents K and omits frequency controls. A 2D result omits 3D cut controls. A 3D field shows a single **Cut-plane positions** group with X, Y, and Z position mode controls in order. Each offers **Mid-plane** and **Explicit value**. The explicit control starts at the resolved midpoint in the profile display unit and edits that axis's stored expression and unit. Changing a cut position preserves the active case and camera. The field panel reports the source Study, run, case or stationary state, mesh, and available solve information. [Field widget contract](shared-widget-contract.md#field-plot-settings) · [Field runtime behavior](../RUNTIME.md#field-result-function)

Use the **Global variable plot** heading for the Study-backed sweep plot. Give its dataset the **Data selection** control in slice mode and **X axis** choice, and give its series the **Global variables to plot** table. Present the plot and table as views of the same filtered rows. Keep the generated mirror of selected columns in `tableColumns` and hide its separate raw editor. Check series selection, order, and exact stored values when the user switches views and reopens. [Sweep plot bindings](shared-widget-contract.md#sweep-plot-settings)
