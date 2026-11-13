# Tree context menus

Both editors use the shared Tree. Right-click opens the context menu for the clicked row. Opening or dismissing it executes no menu command. A command that edits or inserts a value resolves the menu target before running and verifies the target is still valid. Show a disabled reason when a displayed command is unavailable. Keep labels, order, and availability in the acceptance evidence.

## Schema Editor

General Schema Editor menu behavior, command availability, title/key preservation, protected declarations, and neutral acceptance cases are specified in the [Schema Editor behavior contract](../../../../platform/schema-editor/specification/contracts/BEHAVIOR.md) and [acceptance contract](../../../../platform/schema-editor/specification/ACCEPTANCE.md). FEM uses the shared interpreter's authoring menu; do not replace it with a FEM project action menu.

| FEM schema projection | Required outcome |
| --- | --- |
| Physics interface and condition kinds | The schema projects one Physics interface with Dirichlet, Neumann, Impedance, and Periodic Condition declarations directly beneath it. Give the shared Physics object its condition entry declaration through `additionalProperties` at the object level, with named-condition variants and separate profile/formulation constraints. The interface is an object declaration, with no movable presentation wrapper. Each condition declaration exposes its own Settings. |
| Union and Intersection | Each Boolean kind projects a selectable child Features collection with its own editable source declaration. Bind the `features` field to the shared project-editing Tree widget (`payload.widget.type: "tree"`, `settings.role: "children"`) and keep the operand declaration reachable. |

Validate the FEM schema document against its interpreter, then validate FEM project rules on spec-derived model values. A valid schema document does not by itself prove any FEM application predicate.

## Model Editor

Author `x-fullmeta-ui-tree.actionMenus`, `menuTargets`, `insertions`, and the `x-fullmeta-editorActions` catalogs so the shared Tree resolves the action for the actual row path. Register any application-owned handler under the matching action ID. Return `selectItemOnContextMenu: true` in the target application package's `treeActionPlans(context)` result. On right-click of a selectable item, commit a valid pending Settings draft as on ordinary selection and open the clicked item's Settings and scene before the menu. A collection or definition-link target without an item path does not select a substitute item. Each row has one effective menu. Resolve profile, dimensional, and prerequisite constraints. Omit actions outside the applicable catalog and explain why a displayed action is disabled. A named item's **Rename…** command opens the shared inline editor for `name` and preserves its ID and references.

| Target | Ordered commands |
| --- | --- |
| Definition links | Parameters: Add parameter. Global variables: Add global variable. Pointwise variables: shared collection Add action when applicable; the Settings table offers Add pointwise variable. |
| Geometry features collection | The applicable Geometry add list below. Append to the collection. |
| Primitive feature | Rename…; Duplicate item; applicable Geometry add list with ` after` appended to each label; Delete item. Insert after the selected item in its owning array. |
| Union or Intersection feature | Rename…; Duplicate item; applicable Geometry add list without suffix; Delete item. Insert into this group's own `features` array. |
| Selections collection | 2D: Add area selection; Add edge selection; Add point selection. 3D: Add volume selection; Add area selection; Add edge selection; Add point selection. |
| Named selection, integration operator, mesh size control, or result recipe | Rename…; Duplicate item; Delete item. |
| Integration operators collection | Add integration operator. |
| Mesh root | Add mesh size control; Add periodic pair when enabled by the profile. No root rename, duplicate, or delete. |
| Periodic pair | Rename…; Delete item. |
| Materials collection | Add Material Assignment. |
| Material | Rename…; Duplicate material; Delete item. |
| Physics root | Rename…; Add Dirichlet condition; Add Neumann condition; Add impedance condition; Add periodic condition, in that order. Leave a declared but unsupported condition visible with its reason, such as Add impedance condition for thermal. No root duplicate or delete. |
| Physics condition | Rename…; Disable item if enabled or Enable item if disabled; Duplicate item for ordinary conditions; Delete item. Omit Duplicate item for a periodic condition. |
| Study collection | Add Harmonic study for wave profiles, or Add Stationary study for the stationary profile. |
| Individual Study | Edit Harmonic study settings or Edit Stationary study settings; Rename…; Add Parameter sweep only for an eligible Study without a sweep; Duplicate item; Delete item. Edit opens that Study's Settings. |
| Parameter sweep | Edit Parameter sweep settings; Delete Parameter sweep. Delete clears only the child of the owning Study in the current draft. |
| Results collection | Add Global variable plot when supported; Add Domain field plot. A supported legacy recipe can be opened without an Add command for its legacy type. |

Geometry add list in 2D: **Add Rectangle**, **Add Circle**, **Add Union**, **Add Intersection**, **Add circle circumference**, **Add line segment**. In 3D: **Add Box**, **Add Cylinder**, **Add Union**, **Add Intersection**, **Add circle circumference**, **Add line segment**, **Add circular surface**, **Add planar rectangle**. Preserve this order. Every geometry type declared by the model schema has a supported or explicitly unavailable entry in the catalog. Keep geometry, Physics, Study, and result catalogs complete so a valid command does not become unavailable because of an unclassified type in another branch. [Targets and item templates](../UI.md#tree-action-declarations)

Duplicate writes the selected kind to the correct collection or object map with a new unique ID and name. For a duplicated geometry subtree, allocate fresh IDs for the descendants and update references that are internal to the duplicate. Delete checks the target and live references; any blocked deletion explains which reference must be changed. Enable/Disable updates only the selected condition's `enabled` flag. Menu visibility is not evidence that a profile can prepare or solve an unsupported condition; apply the declared runtime checks separately.

### Draft editing contract

**Rename…** opens a focused text input in the clicked tree row with the current name prefilled. Enter or a valid blur commits the trimmed name to the draft, and Escape cancels. Blank input shows an error and leaves the row value unchanged. A successful rename changes only `name`; preserve the ID, object key, references, and selected Settings path. Expose the `rename` menu entry through the existing shared project rename route. Leave `inlineEditForAction` and `actionAvailability` unclaimed for a valid host-owned rename so the shared host reaches its built-in name editor. The declared inline edit for a schema `title` is a separate operation. Offer Rename only on rows with an editable stored `name`.

**Delete item** and **Delete Parameter sweep** remove the selected entry from the in-memory draft and return to the model overview. Keep the owning collection or object, neighboring entries, and unrelated branches intact. Use the declared synchronous tree action and the shared host's draft replacement and Save path. Return `path: null` with the updated draft to clear the selection and open the model overview. An empty path selects the document root. Recheck the target path and stable ID and the live reference policy when enabling and executing Delete. If a reference blocks removal, disable the command with the referencing item named. The array deletion transaction applies any declared key-reference cleanup. A sweep deletion removes only `parameterSweep`. Project folder deletion is a different operation with its own launcher confirmation contract. [Launcher project deletion](create-project-interface-contract.md)

The shared host owns the model draft, selection, and canonical Save. A context menu action belongs to the open document and the resolved target. Close or invalidate pending interactions when the document or target changes, and verify that they cannot affect another item. Use the installed host action contracts with no browser-native prompts, DOM interception, or direct file writes. Record any missing host capability with the live reproduction. [Package interface](application-ui-javascript-contract.md#packagejs)

## Schema acceptance evidence

1. Save and reopen the schema. Inspect the shared interpreter binding, projected Physics hierarchy, action IDs, menu targets, item templates, declaration order, and the registered handlers. General command availability and editor transaction evidence is recorded under the linked Schema Editor acceptance; do not duplicate it as FEM acceptance.
2. In spec-derived 2D and 3D inputs, right-click every row kind in the table. Record ordered labels, enabled states and reasons, target path, handler, and resulting Settings selection. Check geometry group versus primitive insertion, an enabled versus disabled condition, an ordinary versus periodic condition, and an eligible Study before and after creating a sweep. Include the stationary profile to exercise exclusions and the disabled impedance entry.
3. Execute applicable commands on spec-derived disposable inputs through the [project-level command gate](#project-level-command-gate).

### Project-level command gate

Use the installed schema, registered target application package, and actual Model Editor in the Codex in-app browser. Open saved, canonical disposable project JSON derived from the Model 2 contract. Include 2D wave, 3D wave, and stationary cases; use both an array item and an object-keyed Physics condition. Also include a referenced item and a Study with a Parameter sweep. An evaluator may direct and witness the clicks. A desktop browser run may supplement the evidence, but it does not establish that the in-app workflow works.

| Check | Project action and required proof |
| --- | --- |
| PRJ-01 Rename | From the actual row, open Rename…, inspect the prefilled inline input and keyboard focus, and press Escape. Verify the original row and clean draft. Reopen, reject a whitespace-only name, then commit a padded name containing punctuation and verify the trimmed value. Exercise Enter on an array item and blur on an object-keyed condition. Check the literal label, Settings heading and path, stable ID and references. Verify the saved file is unchanged before Save. Save, reopen, and deep-compare the full document with the baseline plus the one `name` change. |
| PRJ-02 Delete | Click Delete item on a removable entry. Check the model overview, draft removal, unchanged sibling order, and unchanged saved file before Save. Save and reopen. Repeat for an object-keyed condition and for Delete Parameter sweep, which removes only `parameterSweep`. On a referenced item verify the disabled reason and unchanged project. |
| PRJ-03 Guarded commands | Open a context menu in document A, then use File Open to switch to a distinct canonical document B with an item at the same path and with the same ID. Verify that the old interaction is dismissed and that both documents remain clean. In the action-plan check, submit the target captured in A to the plan for B and verify rejection before any mutation. Exercise Add and Duplicate on a primitive, nested group, and named condition. Check owning paths, new IDs, valid references, selection, and the saved/reopened model. Attempt an incompatible insertion and record the disabled reason or rejection and unchanged draft. |

For each check record the browser surface and version, shared host version, schema/package identities, canonical model path, clicked row and stable ID, visible outcome, and saved JSON before and after. Verify that cancellation leaves a clean model clean and that each valid command changes only the intended draft. Record runtime errors with exact text and steps. A required command that is blocked or lacks live Save/reopen evidence keeps schema acceptance open.
