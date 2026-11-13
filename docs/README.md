# FEM project guide

This is the public documentation entry for people and agents creating FEM project model JSONs. It is the Help guide for a model interpreted by [fem.schema.json](../fem.schema.json). The public format and guides work independently of the application's builder specification.

The development application declares the five supported Physics profiles and
their Model 2 fields, Settings, validation and native registrations. Package
integration is in progress; complete editing, execution and numerical acceptance
remain open. The published format below defines model values independently of
that acceptance status.

The development package is registered in the launcher. The Schema Editor can
create and open a profile starter. Geometry Check and Compile and native Validate
have been exercised on a disposable thermal case. Development cases in all five
profiles now complete a Study and display checked field data; this covers Ez
for EM2D and electric for EM3D, rather than every supported formulation/backend.
The thermal case displays a checked 300 K field. Acoustic sweep cases display
checked global-variable Plot/Table results. The Study strip shows reported
counts, frequencies and stages, and ordinary Stop retains prior successful
results. Scoped cleanup, checked preview restoration, combined
field Quantity, 3D cut controls and saved Study history are bound in the
schema/package; their complete live workflows are still being checked.
Named CAD/entity highlighting has focused projection checks; complete visible
coverage of selection kinds and geometric combinations remains open.
Do not treat this development registration as numerical acceptance.

The current validation declarations compare selection membership addresses with
omitted entries treated as Outside, preserving the stored rows. They also
check definition-symbol uniqueness and material quantity expressions. Constant
acoustic loss factors must evaluate to zero; wave material expressions using
coordinates or sampled frequency are checked in their declared scope and await
runtime samples for numeric evaluation. These checks do not establish complete
FEM or numerical acceptance.

Study Settings derive formulation and element-order choices from the active
profile and linked Physics. Solver options use the schema's backend-dependent
catalog and shared resolver. Viewing defaults preserves absent optional groups;
editing a solver option makes that group's resolved values explicit. Mesh and
Edges controls retain per-document preferences and are disabled, with a reason,
until checked mesh or field data is available.

Reference fields show named choices while storing IDs. Material assignments
offer their current domain and eligible unassigned domains. Periodic conditions
offer the mesh-enabled pairs; enable that list in Mesh Settings. Saved
unavailable targets remain visible for repair. Material Settings show the
profile's Material properties group, including optional display defaults;
opening it leaves omitted properties absent.

**Save As** creates a same-folder copy with a fresh Model ID while retaining
the display name and other data. It checks the folder's existing names and IDs;
ordinary Save and the automatic Save before a Study retain the current identity.
Named array rows expose guarded **Delete item**, and geometry rows also expose
**Duplicate item**. A blocked deletion identifies the reference to repair.
Removing a top-level geometry feature clears its declared membership keys while
retaining the address rows. Rename and Delete change the draft before Save.

Sweep controls declare their Start, End and Count roles explicitly. Switching
mixed-unit explicit values to a range preserves endpoint roles and units through
Apply, Save and reopen. Disabled Physics conditions remain selectable with an
inactive appearance; their normal menu offers Enable item. Both paths have
passing focused tests and live editing checks.

Settings use declared type headings and field order: identity and name precede
references and quantities; solver controls follow backend, ordering, threads
and quadrature. This ordering affects presentation only and preserves stored
values, omitted optional groups and supported extensions.

Definition tables now expose the declared units, and Parameters shows resolved
values using the application's expression policy. Symbol edits retarget declared
expressions and Study-backed references while retaining file-backed literal
headers. Expression bindings distinguish parameter, global and pointwise
scopes. Coordinate filters preserve the contextual x/y/z names, and a rename
that would capture a parameter reference with one of those names is rejected.
A declared global `t` keeps its ordinary declaration binding, including inside
integration arguments.

For a file-backed plot, **CSV source units** edits a JSON map from literal
column headers to source units. The X and series units control display
conversion independently. Invalid map text remains in the draft until repaired.
Optional **Axis bounds** can be enabled and cleared; X bounds start in the
dataset unit. Study Settings also expose optional output configuration, and a
Periodic condition can enable an explicit Floquet phase vector.

For a configured global-variable plot, **Save figure** exports its checked
Plot view to the selected `.svg`, `.png` or `.pdf` filename and reports the
saved path and byte count. Table view uses the same selected samples and units.

The Windows mesher supports membership references to nested child features and
nonlinear coordinate zero expressions. For a nonlinear filter, the expression
describes the whole intended contour and is evaluated at a point on each
address-selected CAD entity. The mesher's focused checks cover adjacent and
overlapping child regions, nested intersections, and curved coordinate
filters. Complete schema and application acceptance remains open.

## Creating a project model JSON

1. Read the authoritative [FEM Model 2 format](fullmeta-model-2.md). It defines fields, types, quantities, profiles, references, defaults, expression scopes and stored meanings.
2. Select the deployed application schema and its application ID. Add the public FullMeta routing envelope described in that format reference, including `x-fullmeta-schema` relative to your JSON file. Deployment names are separate from the Model 2 payload.
3. Place the document under a suitable project folder. Nested branches and multiple application models in one folder are supported; keep full workspace-relative paths when opening a document.
4. Preserve IDs, references, units, optional-member absence and array order. Keep UI annotations, transient drafts and view/operation state out of model values unless the published format declares them.
5. Use the interpreting application's available validation and operations. A structurally valid document does not by itself establish supported numerical behavior, a valid mesh or a successful solve. Follow the reported paths and diagnostics when repairing input.

Agents authoring project JSON should use this public format and the deployed interpreting schema. Private build documents and historical project examples are not required.

## Public references

- [FEM Model 2](fullmeta-model-2.md): author and interpret project model JSON.
- [Native mesh artifact](fullmeta-mesh-1.md): interpret the mesh data produced by the native mesher.
- [First-order E-H formulation](first-order-eh-formulation.md): understand the experimental formulation's mathematical scope and constraints.
- [Workspace files and layouts](../../../docs/workspace-files-and-layout.md): open documents, browse project branches, use menus and resize panes.
- [Workspace guide](../../../docs/README.md): find applications and individual model files.

The application guide must be updated when the application is delivered to describe its actual supported profiles, editing, validation, Study, cancellation, recovery and result workflows. Until then, format availability and the installed schema's capabilities remain distinct.

## Help and editing

Help resolves `docs/README.md` beside the active interpreting schema. A FEM model therefore opens this guide. When editing the FEM schema itself, the active interpreter is Schema Editor, so Help opens the [schema-authoring guide](../../../platform/schema-editor/docs/README.md).

File Open and import dialogs use the checked in-application workspace picker. View offers Stacked and Columns; Stacked starts with equal Graphics and Settings heights, and the dividers remain resizable. Human controls and available WebMCP commands use the same normal application actions and document state.
