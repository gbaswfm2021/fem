# Solver and mesher capability contract

This file specifies the supported target application solver and mesher functions. The profile declarations and deployed native programs must agree about supported inputs, execution routes, and outputs.

## Shared execution lifecycle

The native FEM entry point performs common whole-model preflight and dispatches to one scenario executable. Scenario executables retain numerical and mesh-dependent checks. The runtime sequence is:

1. Receive the selected model/schema context and Study request.
2. Run native model preflight and verify runtime registration.
3. Resolve profile, Study, Physics interface, solver backend, and any prepared sweep cases.
4. Prepare or reuse a verified geometry/mesh for each required component.
5. Convert the request into the scenario executable’s supported numerical inputs.
6. Check actual mesh correspondence, material/boundary coverage, coefficient finiteness, constraints, and solver compatibility.
7. Assemble and solve the requested system.
8. Verify finite output and solver residual criteria.
9. Write run records, logs, field/global output, CSVs, and progress/completion events.
10. Return the concrete output-file list for UI result resolution.

Model validation, geometry/mesh validation, and numerical validation are separate scopes. A successful earlier stage does not imply later stages will pass.

## Preparation and solver dispatch

The Study endpoint starts the registered dispatcher in `--server` mode. Read `scenario.profileId`, the selected Study ID, its Physics reference, and its effective backend from the model snapshot and schema. Resolve preparation through `x-fem-capabilities.study.preparationByProfile` and the numerical program through `x-fem-capabilities.study.solverRuntimesByProfile`. Resolve each program ID against `x-fullmeta-runtime.externalPrograms`. The backend comes from the selected Study, then its profile defaults, then the common solver defaults.

| Profile | Preparation kind and owner | Numerical program ID and Windows executable | Prepared numerical entries |
| --- | --- | --- | --- |
| `acoustic-2d` | `frequencyPlan` in the common preparation handler | `solverAcoustic2d` → `acoustic2d.exe` | `--solve`, `--sequence` |
| `acoustic-3d` | `frequencyPlan` in the common preparation handler | `solverAcoustic3d` → `acoustic3d.exe` | `--solve`, `--sequence` |
| `em-2d` | `frequencyPlan` in the common preparation handler | `solverEm2d` → `em2d.exe` | `--solve`, `--sequence` |
| `em-3d` | `configuredFrequencyRequest` in the common preparation handler | `solverEm3d` → `em3d.exe` | `--solve`, `--sequence` |
| `thermal-2d` | `scenario` through `thermal2d --prepare-study` | `solverThermal2d` → `thermal2d.exe` | `--solve` |

Both `rivrs` and `faer` resolve to the registered profile executable when the build supports the selected backend. A wave preparation produces its normalized plan or request once for the chosen model and Study. Thermal preparation returns a stationary plan from its scenario entry. Preparation receives the same model and Study context throughout. A preparation handler returns a plan or an error and terminates. Keep the preparation path acyclic, including any shared preflight call. Preflight returns validation and registration results. The numerical handler receives the prepared inputs. Reject a registration that resolves back to the launcher as a numerical executable or routes preparation back through the active dispatcher entry.

`frequencyPlan` returns a harmonic Study plan with the profile and Study IDs, effective solver settings, ordered frequencies in Hz, and case parameter values and dimensions. `configuredFrequencyRequest` prepares the EM3D solver request with the checked mesh file, at least one material assignment, and the prepared cases. Its enabled ordinary boundary input is impedance. Under `allAssignedMaterials`, the active domain set matches the material assignment set. Report unsupported enabled boundary declarations during preparation. Thermal preparation uses the stationary Study and returns its temperature, material, and boundary configuration through the scenario entry.

Resolve the handler, executable, entry, profile, and backend before starting that stage. A missing registration, unsupported preparation kind, missing executable, or unsupported backend ends the operation promptly with a terminal error. Identify the profile, Study, failed stage, and the specific registration or executable to correct. Preserve the run diagnostic. Acceptance exercises every profile's actual preparation and solve path, with a separate missing-route failure case. [Runtime registration and storage](runtime-integration-boundary.md)

## Schema metadata consumed by native programs

Use the selected target application schema graph for the dispatcher, scenario programs, and mesh compiler. Embed the built-in profile schemas in `$defs.femProfiles`, keyed by profile ID, and reference their model constraints from the root. Keep imported references resolvable within the application or the documented shared platform interfaces. The declarations below are runtime inputs. Their values must resolve to the exact model fields, profile choices, and executable registrations in the contracts.

| Declaration | Required contents and consumer |
| --- | --- |
| `x-fem-capabilities` | Set `schema` to `fem-engine-capabilities/2` and the documented editor API contract `fullmeta-model-editor-api/18`. Provide expression arities and scopes, geometry kinds, mesh families and dimension eligibility, Physics and periodic policies, integration capabilities, and Study options/defaults. Compose per-profile preparation and runtime maps from the scenario declarations. Each runtime descriptor includes a nonblank `backend` equal to its enclosing backend key and a registered `programId`. |
| Profile `x-fem-scenario` | Supply `profile` identity, name, dimension, family, supported formulations, and editor options. `fields.formulations` maps every supported formulation to a catalog whose `formulation` member equals its map key, with primary `fieldVariables`, `derivedFieldVariables`, and `zeroFieldVariables`. Each field declares symbol and unit and its solver component or zero-in-dimension behavior. `materials` declares the property keys, units, symbols, representation, defaults, and optionality. Declare the supported `sweepResultQuantities`. |
| Profile `x-fem-physics` | Declare boundary and feature types and the formulation-specific `boundaryDefaultsByFormulation`. The editor actions and native validation use the same units and quantities. |
| `x-fem-modelValidation` | Bind `profilePath` to `/scenario/profileId`, `scenarioAnnotation` to `x-fem-scenario`, `dimensionMetadataPath` to `/profile/dimension`, and `validationAnnotation` to `x-fem-validation`. Declare the expression, quantity, geometry, selection, mesh, periodic, Study, and preparation policies consumed by the rules and native guards. |
| `x-fullmeta-rules` and named `x-fullmeta-*Validation` programs | The model program evaluates the declarative model predicates. Named metadata, registration, dispatch, launch, preparation, and sweep programs evaluate their stage envelopes. [Validation functions](../VALIDATION.md) |
| `x-fullmeta-runtime` | Register project storage, programs, entries, routes, result adapters, and file contracts. The dispatcher and Kernel both resolve declared program IDs from this graph. [Runtime integration](runtime-integration-boundary.md) |

Use the profile, expression, material, boundary, and result tables to author these declarations. The normalized wave request uses `fem-solver-request/1` and its configured preparation uses `fem-solver-configuration/6`. Use the copied programs' documented entries for the prepared argument and output shapes. Verify effective metadata and backend settings with the deployed `--capabilities` output and real profile launches. [Model 2 contract](../../docs/fullmeta-model-2.md)

### Built-in capability catalog values

The root `x-fem-capabilities.mesh` catalog declares `familyByDimension` (`"2": "triangle"`, `"3": "tetrahedron"`) and `elementOrdersByProfile`. These orders apply to Study discretization: `[1]` for acoustic 2D, acoustic 3D, and EM 2D; `[1, 2]` for EM 3D; and `[1]` for thermal 2D. Profile formulation and Study constraints still determine which combinations are valid. The current Gmsh converter accepts only linear triangle and tetrahedron mesh elements, so `model.mesh.gmsh.elementOrder` is fixed to `1` for every profile. EM3D second-order electric Study elements run on that supported linear geometry mesh; the Study order is independent of Gmsh's geometric mesh order. A saved Gmsh order of `2` is a model-validation error, before meshing starts.

It also declares `mesh.fieldFormulations`, keyed by formulation token, with an
`elementOrders` array per entry. The required values are `[1]` for `pressure`,
`Ez`, `Hz`, `firstOrderEH` and `T`, and `[1, 2]` for `electric`. The fixed EM3D
solver reads this map before accepting its requested order; profile mesh orders
alone do not supply that interface. These values describe the matrix below and
do not enable a Study/Physics formulation mismatch.

`study.discretizationChoicesByProfile` declares each profile's ordered Study
formulation records, with stored `value` and display `label`, using the matrix
below. Their allowed orders come from `mesh.fieldFormulations`, intersected
with the profile's mesh orders. Settings project these records and the existing solver
resolution catalogs through the shared settings resolver. The EM2D
`fieldFormulationsFromPhysics` flag restricts the offered formulation to the
linked Physics. Displayed defaults do not create optional `discretization` or
`solver` members. These view projections leave the structural schema and
semantic validation guards intact, including the experimental E-H mismatch.

The root `x-fem-capabilities.physics` catalog declares solver-accepted `solverBoundaryTypesByProfile`, available `featureTypesByProfile`, and `periodicPolicyByProfile`. Acoustic 2D, acoustic 3D, and EM 2D accept `dirichlet`, `neumann`, and `impedance` ordinary boundaries. EM 3D preparation accepts `impedance`; other enabled ordinary boundary declarations are rejected during preparation. Thermal 2D accepts `dirichlet` and `neumann`. All five profiles declare `periodicCondition`; wave profiles use the `floquet` phase-vector policy, while thermal uses `zeroPhaseOnly`. Declare `ordinaryPeriodicExclusive: true`: neither endpoint of an enabled periodic Physics condition may also be targeted by an enabled ordinary boundary condition. A mesh pair without an enabled Physics condition does not activate this conflict check.

The root `x-fem-capabilities.integration` declares `boundaryIntegration`, `domainIntegration`, `edgeIntegration`, and `pointEvaluation` with `evaluation: "supported"`. Profile `sweepResultQuantities` lists built-in scalar CSV quantities that can be selected as sweep results: acoustic 2D and 3D declare `max_pressure_magnitude_Pa`; EM 2D declares `max_field_magnitude`. EM 3D and thermal 2D declare empty lists: EM 3D's Study sweep columns are configured global-variable outputs and thermal is stationary. Frequency axes and solver diagnostics are not sweep-result quantities.

For material result choices, each built-in profile declares
`x-fem-scenario.fields.plotSources` containing `material`, and its material property
descriptors declare `plottable: true`. Wave profiles declare `primary`, `derived`,
`pointwise`, and `material` sources; thermal declares `primary` and `material`.
The native resolver and field-result choices use this same source catalog.
A source absent from the catalog or a false/absent property flag excludes
the choice. Eligibility does not assert that a particular artifact contains its
samples; the checked result adapter determines sample availability separately.

## Scenario capability matrix

| Profile | Validated Physics and Study formulations | Mesh and solver orders | Study behavior | Result behavior |
| --- | --- | --- | --- | --- |
| `acoustic-2d` | `pressure` | 2D triangles, order 1 | Harmonic point or parameter sweep | Pressure and velocity field data, global data, and sweep CSV |
| `acoustic-3d` | `pressure` | 3D tetrahedra, order 1 | Harmonic point or parameter sweep | Pressure and velocity field data, global data, and sweep CSV |
| `em-2d` | `Ez` or `Hz`, with the Study following the linked Physics | 2D triangles, order 1 | Harmonic point or parameter sweep | EM field data, global data, and sweep CSV |
| `em-3d` | `electric` | 3D tetrahedra, Nédélec order 1 or 2 | Harmonic point or parameter sweep | EM field data, global data, and sweep CSV |
| `thermal-2d` | `T` | 2D triangles, order 1 | Single stationary case | Nodal temperature, per-cell flux and conductivity, and summary CSV |

The EM3D Study declaration also accepts the experimental `firstOrderEH` token at the structural layer. Its Settings label is **First-order E-H**, with order 1 and the `faer` backend. The built-in Physics formulation is `electric`. FullMeta rejects a Study whose formulation differs from its linked Physics and identifies the formulation field. Keep the structural choice, solver metadata, and semantic guard distinct. [Study serialization and option resolution](../../docs/fullmeta-model-2.md#study-and-sweep)

## Common Physics and material functions

The Physics object identifies the family, formulation, active domains, and frequency for a wave profile. Electromagnetic profiles also declare the phasor convention. Physics owns ordinary boundary conditions and periodic features. Settings fields and units follow the selected profile, formulation, and condition kind.

### Boundary/feature behaviors

| Condition | Stored data | UI behavior | Runtime check |
| --- | --- | --- | --- |
| Dirichlet | `selectionId`, prescribed `value`, identity/name, `type` | Select a boundary and enter a unit-aware prescribed field value. | Check selected boundary coverage, consistency at shared nodes, and compatibility with periodic constraints. |
| Neumann | `selectionId`, prescribed `value`, identity/name, `type` | Select a boundary and enter the outward prescribed flux/load. | Check quantity, sign convention, compatibility and numerical contribution. |
| Impedance | `selectionId`, `impedance`, optional `incidentField`, optional `polarization`, identity/name, `type` | Display source fields only where the profile/condition accepts them. | Check finite, supported impedance and incident-source values; validate actual mesh sampling. |
| Periodic | `periodicPairId`, `phaseVector`, identity/name, `type` | Choose an enabled matched mesh pair; edit only supported phase components. | Verify actual source/destination correspondence and apply the supported trace relation. |

All built-in wave profiles declare Dirichlet, Neumann, impedance, and periodic conditions for editing. EM3D preparation accepts impedance as its enabled ordinary boundary input, and reports other enabled ordinary conditions with an actionable diagnostic. Thermal declares Dirichlet, Neumann, and zero-phase periodic temperature equality. Use scenario validation and preparation to check executable support.

### Material properties

- Acoustic materials supply density and sound speed. Optional loss factor evaluates to zero.
- Electromagnetic materials supply conductivity, relative permeability, and relative permittivity.
- Thermal materials supply isotropic conductivity `k` in `W/(m*K)`.

The UI stores material values and their selection reference. Runtime checks evaluate supported expressions and actual cell coverage. The thermal solver supports constant isotropic conductivity per material selection; spatially varying thermal material/boundary expressions are explicitly unsupported by its copied documentation.

## Geometry and mesh contract

The copied mesh CLI validates and compiles geometry, invokes Gmsh, and exports the mesh. Author every geometry variant in the Model 2 contract. Route meshing through the declared dimension and geometry eligibility rules. Apply the same ownership, address, validation, and build requirements to union and intersection in either supported dimension. The 2D geometry compiler and mesher must preserve standalone top-level `lineSegment` and `circleCurve` features as lower-dimensional curve entities beside domain surfaces. Exercise their validation, compilation, and actual mesh build using a disposable rectangle-plus-line-plus-circle fixture. Verify the curve entities appear in the generated geometry/map, top-dimensional triangle counts and material coverage remain defined by the rectangle domain, and the curves do not create domain-address bits. Reject curves nested under a domain union/intersection with an exact path diagnostic. Report unsupported operations on other declared curves or surfaces at the geometry stage. [Geometry variants](../../docs/fullmeta-model-2.md#geometry-features)

### Mesh settings

Mesh declarations include element family, Gmsh settings, maximum/minimum size, size controls, and periodic pair metadata. Settings use the declared values, units, selection targets, and dimensions. Periodic-condition editing references an existing, enabled mesh pair.

### Runtime output

The mesh path is expected to preserve:

- the generated `.geo` input and geometry/model map;
- the Gmsh `.msh` file and log;
- the FullMeta mesh consumed by the solver;
- domain/boundary/interface tags needed by materials, Physics, mesh controls, and integration;
- the mesh report, run record, and cache/integrity information where the CLI declares them.

The UI reads artifacts through checked workspace paths. It must not treat an old preview as the mesh for a changed geometry unless the runtime verifies its geometry hash and artifact integrity.

## Profile details and explicit limitations

### Acoustic 2D/3D

- Harmonic pressure field, 2D triangle or 3D tetrahedron mesh according to profile.
- Acoustic coefficients come from the assigned density and sound speed. The ordinary boundary conditions are prescribed pressure, outward velocity, and impedance.
- Harmonic sweeps produce per-case field and global-variable outputs according to the Study's output options.
- The UI’s frequency field, boundary choices, mesh family, and result quantity must agree with native capabilities.

### Electromagnetic 2D

- The selected scalar `Ez` or `Hz` determines the primary field, derived quantities, and the units of prescribed and incident values.
- Derive incident sources, material contrasts, and Floquet periodic constraints from the saved model expressions and boundary assignments.
- EM material values include conductivity, relative permeability, and relative permittivity.
- A field plot must use the stored frequency and requested field quantity. A periodic condition references an enabled matched mesh pair.
- Exercise each formulation with a spec-derived single-point Study and check its field components and units.

### Electromagnetic 3D

- The `electric` formulation uses Nédélec edge elements on tetrahedra.
- Support impedance and Floquet periodic features, frequency sweeps, field output, and global expressions based on solved fields and integration operators.
- RIVRS sparse LDLᵀ is the default E-only backend. Faer complex sparse LU is available with its declared ordering and thread limits.
- Floquet periodicity uses the documented translated trace relation `u(r + delta) = exp(-i k·delta) u(r)`, with phase vector in rad/m and translation in metres. Runtime verifies actual mesh correspondence and constraints.
- The solver documentation describes selected impedance boundaries; an incoming boundary includes a dimensionless complex incident factor and polarization vector, and an outgoing boundary omits those source fields. The solver does not infer a plane-wave incidence angle or automatic port normalization.
- Transmission/reflection observables are model-defined global expressions/integration operators; do not present them as automatically normalized ports.

### Thermal 2D

- Solves stationary `-div(k grad(T)) = 0` with linear triangles and constant isotropic conductivity per material selection.
- Conductivity, prescribed temperature, and outward heat flux accept supported constant parameter expressions with units compatible with `W/(m*K)`, `K`, and `W/m^2`.
- Positive outward flux removes heat; unspecified exterior edges are insulated.
- Periodic temperature constraints require zero phase and cannot overlap incompatible prescribed conditions.
- A connected component without prescribed temperature may use the Study’s area-weighted mean-temperature constraint. Its net prescribed heat flux must be compatible within numerical tolerance.
- Shared corner prescribed temperatures must be consistent. Numerical factorization/residual failures are runtime failures even if the model passed declaration validation.
- Thermal global/pointwise variables, parameter sweeps, volumetric heat sources, transient studies, and spatially varying material/boundary expressions are outside the copied thermal solver’s documented scope.
- Output includes nodal temperature, per-cell heat flux/conductivity, solver diagnostics, and a summary CSV.

## Sweep and solver diagnostics

- Independent parameter ranges form a Cartesian product.
- The application contract sets `maximumSweepPoints` to 10,000 for the built-in profiles. The schema, validator, preparation handler, and UI must use this same declared limit; no consumer may enable a larger sweep without a new application capability contract and acceptance case.
- Per-case progress reports case index, total cases, and frequency/parameter value when supplied. Completion is emitted only after the case succeeds.
- A partial sweep is a failure/incomplete result. The UI must not show all cases as completed if one case failed.
- Record solver backend, ordering, thread count, element order, constitutive quadrature, residual, and actual mesh counts whenever the runtime provides them.

## Capability discovery rule

Derive Settings choices from the declared schema and runtime capabilities. Check all effective choices and restrictions with spec-derived examples. For each valid formulation, exercise preparation, numerical execution, and result display. For combinations rejected at validation or preparation, exercise the diagnostic and confirm the worker does not enter the numerical stage.
