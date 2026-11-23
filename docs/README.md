# FEM project guide

This is the public documentation entry for people and agents creating FEM project model JSONs. It is the Help guide for a model interpreted by [fem.schema.json](../fem.schema.json). The public format and guides work independently of the application's builder specification.

The development application declares the registered Physics profiles and
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
These observations were recorded before the scenario-discovery and mesher
reorganisation; they are not acceptance evidence for the current build. Full
editing, execution and numerical acceptance remain open.

## Physics profiles and runtime

The application currently registers these profiles:

| Profile ID | Physics | Formulations | Study type |
| --- | --- | --- | --- |
| `acoustic-2d` | [2D pressure acoustics](../fem-rust/docs/notes_acoustic2d.md) | `pressure` | Harmonic, Eigenfrequency, Propagation Mode, Transient |
| `acoustic-3d` | [3D pressure acoustics](../fem-rust/docs/notes_acoustic3d.md) | `pressure` | Harmonic, Eigenfrequency, Transient |
| `em-2d` | [2D electromagnetics](../fem-rust/docs/notes_em2d.md) | `Ez`, `Hz` | Harmonic, Eigenfrequency, Propagation Mode, Transient |
| `em-3d` | [3D electromagnetics](../fem-rust/docs/notes_em3d.md) | `electric` | Harmonic, Eigenfrequency, Transient |
| `elastic-2d` | [2D elasticity](../fem-rust/docs/notes_elastic2d.md) | `planeStress`, `planeStrain` | Harmonic, Eigenfrequency, Propagation Mode, Transient |
| `elastic-3d` | [3D elasticity](../fem-rust/docs/notes_elastic3d.md) | `displacement` | Harmonic, Eigenfrequency, Transient |
| `thermal-2d` | [2D heat conduction](../fem-rust/docs/notes_thermal2d.md) | `T` | Stationary, Eigenfrequency, Transient |
| `thermal-3d` | [3D heat conduction](../fem-rust/docs/therm3d.md) | `T` | Stationary, Eigenfrequency, Transient |

Propagation Mode is available for Acoustic 2D pressure, both full-polarization
EM2D formulations, and the three-component plane-strain-parent Elastic 2D
formulation. It fixes a real operating frequency and solves the complex
longitudinal wavenumber `kz` directly. The Study supports a complex search shift,
mode count, Krylov dimension, and residual tolerance; result fields are
cross-sectional amplitudes with `exp(-i*kz*z)` dependence. The shared
scenario-neutral implementation and its current element, material, and boundary
limits are documented in the [propagation-mode implementation plan](../fem-rust/docs/implement_2d_propagation_mode_study.md).

Thermal 3D uses first-order tetrahedra, material-wise constant isotropic
conductivity, prescribed temperature or heat flux, optional Domain Sources,
and zero-phase matched periodic conditions for stationary Studies. Spatially
varying conductivity is not supported by the stationary adapter. The
scenario-neutral sparse Transient Study is enabled for all eight scenarios;
its equations and declarations are documented in the
[transient contract](../fem-rust/docs/transient_study_contract.md). The thermal
transient form requires a positive volumetric heat capacity and permits
prescribed material expressions in space and time.

The scenario registry selects `em-3d` as the default for new projects; an
existing model keeps its stored profile ID. A declared option is not by itself
proof of acceptance for every backend or case.

The editor, mesher and dispatcher compose the scenario contracts with
`fem.schema.json` when they load. Geometry checking, meshing and numerical
solvers use the same `fullmeta-fem` executable; Gmsh remains a separate bundled
third-party tool. Reload the editor and restart native processes after changing
the registry or a scenario contract. See the [FEM Rust guide](../fem-rust/README.md)
and [mesher guide](../fem-rust/src/mesh/GUIDE.md) for the native interfaces.

## Editing and validation

Select **Geometry features** to edit the geometry tolerance and its unit. Expand
the section to reach the individual features; right-click the heading to add a
feature.

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

Right-click **PHYSICS** and choose **Add Domain Source** to prescribe a load
within a selected domain. Each source has its own selection and expressions;
multiple sources add where their selections overlap. With no enabled Domain
Source, the domain load is zero. The scenario and formulation determine the
component labels, units, and physical meaning; see the scenario manuals below.
Spatial expressions are sampled at the Study's volume quadrature points.

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
After editing a plot's settings, select **Apply item** to refresh its Plot and
Table using the existing Study results; a new Study run is not needed.
Frequencies are stored in Hz. Display labels and frequency axes choose an
engineering unit by magnitude unless an axis unit was explicitly selected.
Earlier GHz-based result files remain readable.

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

- Scenario manuals: [2D electromagnetics](../fem-rust/docs/notes_em2d.md), [3D electromagnetics](../fem-rust/docs/notes_em3d.md), [2D acoustics](../fem-rust/docs/notes_acoustic2d.md), [3D acoustics](../fem-rust/docs/notes_acoustic3d.md), and [2D thermal conduction](../fem-rust/docs/notes_thermal2d.md). These connect each model's equations, field spaces, material and Physics settings to the implemented solver.
- [FEM Model 2](fullmeta-model-2.md): author and interpret project model JSON.
- [Native mesh artifact](fullmeta-mesh-1.md): interpret the mesh data produced by the native mesher.
- [FEM Rust and scenario interfaces](fem-rust-and-scenario-interfaces.md): understand native commands, scenario contracts and runner integration.
- [Workspace files and layouts](../../../platform/workspace/docs/workspace-files-and-layout.md): open documents, browse project branches, use menus and resize panes.
- [Workspace guide](../../../platform/workspace/docs/README.md): find applications and individual model files.

The application guide must be updated when the application is delivered to describe its actual supported profiles, editing, validation, Study, cancellation, recovery and result workflows. Until then, format availability and the installed schema's capabilities remain distinct.

## Help and editing

Help resolves `docs/README.md` beside the active interpreting schema. A FEM model therefore opens this guide. When editing the FEM schema itself, the active interpreter is Schema Editor, so Help opens the [schema-authoring guide](../../../platform/schema-editor/docs/README.md).

File Open and import dialogs use the checked in-application workspace picker. View offers Stacked and Columns; Stacked starts with equal Graphics and Settings heights, and the dividers remain resizable. Human controls and available WebMCP commands use the same normal application actions and document state.
