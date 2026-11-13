# FullMeta Model 2 — public data format

`fem-model/2` is the public, schema-independent data format for finite-element simulation projects. This file is the authoritative description of its persisted structure and stable field meanings. An application schema implements this format; changing an interpreting schema, renderer, validation implementation or native program does not redefine the stored data.

Preserve property names, JSON types, stable IDs, array order, expressions, unit tokens, and optional-member presence. Application IDs, interpreting-schema paths, UI choices, validation execution stages, solver capabilities and style belong to the consuming application. A default is normative only when marked as an effective format default; a creation default is a suggested initial value, not a requirement on an application's style.

This is a data-format contract, not a particular JSON Schema or an implementation-status report. It is usable independently of the FEM application's interpreting-schema implementation. The format payload and the surrounding application-routing envelope have separate responsibilities as defined below.

## FullMeta project routing envelope

To open a Model 2 payload as a FullMeta project, the surrounding document also declares `application` and `x-fullmeta-schema`. The application ID selects the installed registration; the schema reference selects the interpreting JSON Schema and resolves relative to the project JSON file. They route the document and do not change the meaning of `fem-model/2`.

For the supplied FEM registration, `application: "fem"` and schema `applications/fem/fem.schema.json` identify the application. A document saved as `projects/example/model.json` therefore uses `"x-fullmeta-schema": "../../applications/fem/fem.schema.json"`. Use the correct relative path when placing a project at another folder depth or targeting another compatible installed registration. The application ID and physical schema path are deployment values; they are not part of the fixed format identity.

The canonical opened-file path identifies the document context. `metadata.id` is the model ID and is not a prefix for that file path. Use the installed application guide for its creation/save and artifact-storage actions; the data meanings below remain independent of that implementation.

## Notation and common values

Paths below are JSON Pointers. `*` denotes an array element. A required list describes the keys an object must contain. Other listed members are optional unless a variant requires them. Use the original JSON type at each path. An expression is stored as text even when it evaluates to a constant. A numeric setting such as a case count is stored as a JSON number of the specified kind.

| Value | Serialized contract |
| --- | --- |
| ID | String matching `^[A-Za-z][A-Za-z0-9_]{0,63}$`. References use the stored ID. |
| Name | Nonempty string. Shared material, pair, Physics condition, plot, dataset, and series names use 1 to 120 characters. |
| Symbol | String matching `^[A-Za-z_][A-Za-z0-9_]*$`. Apply the declared scope, uniqueness, and reserved-name checks. |
| Expression | Nonempty string. Shared expression fields have a 512-character maximum. Evaluate in the declared scope and expected physical dimension. |
| Unit | One of `1`, `m`, `mm`, `um`, `nm`, `s`, `ms`, `Hz`, `kHz`, `MHz`, `GHz`, `Pa`, `kPa`, `kg`, `kg/m^3`, `m/s`, `m/s^2`, `Pa*s/m`, `ohm`, `S/m`, `V/m`, `A/m`, `V/m^2`, `A/m^2`, `rad/m`, `K`, `W/(m*K)`, `W/m^2`. Restrict the available options to the field's quantity family. |
| Quantity | Object with required `expression` and `unit`. Store the pair and evaluate it with unit conversion. |
| Quantity vector | Ordered array of quantity objects. Component count and labels come from the specific field. |
| Numeric direction | Ordered array of three numbers. Use for a direction or normal where the feature specifies it. |
| Relative output path | Nonempty forward-slash path using letters, digits, `.`, `_`, `-`, and `/`, with neither an absolute prefix nor a `..` segment. Resolve under the declared project storage boundary. |

## Document and profile

| Root key | Requirement |
| --- | --- |
| `modelFormat` | Required string equal to the format identifier stated above. |
| `metadata` | Required object. `id` and `name` are required. Optional `description` is a string. |
| `scenario` | Required object with `profileId`, `dimension`, and `physics`. Use one matching row of the table below. |
| `model` | Required object containing the collections and settings below. |

The `application` and `x-fullmeta-schema` fields may appear in the surrounding workspace project envelope. They select an application and schema; they are not part of this format's model data and are specified by the consuming application's project lifecycle contract. The `modelFormat`, `metadata`, `scenario`, and `model` members above are the format payload. The root, `metadata`, `scenario`, and `model` have the listed keys.

`/metadata/id` is the stable identity of this model document. Its serialized type and syntax are defined here. Same-folder uniqueness, run ownership, allocation, Save, and Save As behavior are application/runtime checks and are specified by the consuming application.

Use closed objects for the envelope, `metadata`, `scenario`, `model`, `geometry`, definition records, quantities, named selections, mesh settings, Gmsh options, periodic pairs, Physics conditions, Studies, and result recipes and their plot records. The material and Physics interfaces allow the profile-defined property and condition keys described below. The rectangle, circle, box, cylinder, mesh size control, individual address row, sweep object and axis/range records, and cut plane mode records permit additional stored members. Preserve those members and apply the declared behavior to the recognized fields.

| `scenario.profileId` | `scenario.dimension` | `scenario.physics` and `model.physics.type` | Physics formulation | Mesh family | Study type |
| --- | ---: | --- | --- | --- | --- |
| `acoustic-2d` | 2 | `acoustics` | `pressure` | `triangle` | `frequencyDomain` |
| `acoustic-3d` | 3 | `acoustics` | `pressure` | `tetrahedron` | `frequencyDomain` |
| `em-2d` | 2 | `electromagnetics` | `Ez` or `Hz` | `triangle` | `frequencyDomain` |
| `em-3d` | 3 | `electromagnetics` | `electric` | `tetrahedron` | `frequencyDomain` |
| `thermal-2d` | 2 | `thermal` | `T` | `triangle` | `stationary` |

## Model object

All of these keys are required under `/model`. Empty arrays are useful while authoring. A completed model must also satisfy the semantic and Study readiness checks. Thermal requires at least one Stationary Study. Its `variables` and `pointwiseVariables` arrays are empty.

| Key | Serialized value |
| --- | --- |
| `units` | String constant `m`. |
| `parameters`, `variables`, `pointwiseVariables` | Ordered definition arrays. Each record requires `id`, `name`, `symbol`, `expression`, `unit`. Optional `description` is a string. |
| `integrationOperators` | Ordered objects requiring `id`, `name`, and `selectionId`. `selectionId` references the eligible named selection used by the operator. |
| `geometry` | Object requiring `features` and `tolerance`. `features` is an ordered feature array. `tolerance` is a length quantity with creation default `1e-9 m`. |
| `selections` | Ordered array of named address selection objects. |
| `mesh` | Object with the mesh settings below. |
| `materials` | Ordered material assignment array. |
| `physics` | One Physics object with named condition members. |
| `study` | Ordered Study objects of the profile's Study type. |
| `results` | Ordered field, sweep quantity, or sweep plot recipes allowed by the profile. |

## Expression and field symbols

Interpret expressions with the functions, physical dimensions and scopes defined here. A consuming application must implement those meanings. Parameter definitions form a real scalar environment with `pi` and `e`. Evaluate dependency order and report cycles, unknown symbols, invalid units, and unsupported calls at the affected definition. Model quantity evaluation uses the expected unit dimension. Compare quantity dimensions with tolerance `1e-10` and require finite intermediates in runtime validation. The complex quantity scope uses `i` as the imaginary symbol.

Addition and subtraction require compatible dimensions even when an operand is zero. `sign` returns +1 at positive zero and −1 at negative zero when the evaluator preserves its sign. The real evaluation policy must retain these meanings, and runtime validation requires finite intermediates. A consuming application binds its evaluator to these semantics; the schema annotation used for that binding is an application implementation detail.

| Scope | Available values and required behavior |
| --- | --- |
| Parameter and ordinary model quantities | Resolved model parameters and supported built-in constants/functions. Reject a runtime field or coordinate dependency where the field requires a parameter-only value. |
| Coordinate selection filter | `x`, `y`, `z`, `pi`, `e`, and eligible parameter values in both dimensions. In 2D supply `z = 0`. Evaluate with numeric SI coordinates and parameter values, mask the parameter dimension vector with `[0, 0, 0, 0, null]` to retain temperature dimension only, and require a dimensionless result. Retain entities where the expression equals zero. |
| Global variable | Parameters, eligible global definitions, and the sampled `freq` in Hz and `omega = 2*pi*freq` for wave cases. A declared integration operator takes one expression over its eligible selection. Inside that argument, coordinates, field and material symbols, pointwise definitions, and declared time symbol `t` are available. |
| Pointwise variable | Parameters, coordinates in metres, sampled wave frequency symbols, active field/material symbols, and eligible pointwise definitions. Evaluate at the requested field location. |
| Study and sweep values | The declared parameter scope. Sweep endpoints and explicit values are independent of every parameter swept in that Study. |

Use one argument for `abs`, `acos`, `asin`, `atan`, `ceil`, `conj`, `cos`, `cosh`, `exp`, `floor`, `im`, `ln`, `log`, `log10`, `re`, `sign`, `sin`, `sinh`, `sqrt`, `tan`, and `tanh`. `pow` takes two arguments. `min` and `max` take one or more arguments. Each function has the listed arity whether or not a particular model expression calls it. Apply the supported expression scope and real or complex evaluator to each use. Integration operators resolve by their declared ID or name and use the selected entity's domain, boundary, edge, or point evaluation. One operator may use the same ID and name; different operators must not claim the same callable alias in the global expression scope. Reserve `t` for parameter and pointwise names. In global scope a declared global `t` is a normal user definition, and the built-in integration-argument `t` has unit `s`.

The model-value function set includes `re`, `im`, and `conj`, whose calls retain their dimensional interpretation. Complex evaluation supports their complex argument semantics. Report a scope-specific error when the selected real evaluator cannot evaluate a call. Geometry and mesh sizing, sweep coordinates, phase quantities, and declared frequency values use the parameter environment. Wave material and ordinary boundary expressions may also use the coordinates and frequency declared for their runtime scope. An incident impedance expression is complex and dimensionless. Each polarization expression has the corresponding primary-field unit. Check unit compatibility even when an invalid frequency prevents evaluating the incident expression.

| Formulation | Primary field symbols | Derived or zero field symbols |
| --- | --- | --- |
| `pressure` | `p` in `Pa` | `vx`, `vy`, `vz` in `m/s`. `vz` is zero in 2D. |
| `Ez` | `Ez` in `V/m` | Derived `Hx`, `Hy` in `A/m`. Zero `Ex`, `Ey` in `V/m` and `Hz` in `A/m`. |
| `Hz` | `Hz` in `A/m` | Derived `Ex`, `Ey` in `V/m`. Zero `Ez` in `V/m` and `Hx`, `Hy` in `A/m`. |
| `electric` | `Ex`, `Ey`, `Ez` in `V/m` | Derived `Hx`, `Hy`, `Hz` in `A/m`. |
| `T` | `T` in `K` | Temperature is the field quantity. |

Material symbols are `rho` and `c` for acoustic density and sound speed, `lossFactor` for acoustic loss, `eps_r`, `mu_r`, and `sigma` for electromagnetic material properties, and `k` for thermal conductivity. Include a material or user-defined pointwise symbol in result choices when the selected profile and result adapter declare it available for plotting.

## Geometry features

Every feature has `id`, `name`, and a discriminator `type`. Additional fields in the Required column are required for that type. `Q2` and `Q3` mean quantity vectors of exactly two and three components. Length quantities use `m`, `mm`, `um`, or `nm`. Group children are serialized feature objects in `features`, in their original order. Group nodes own their own addressable domain ID.

| `type` | Dimension or entity | Required fields | Optional fields and interpretation |
| --- | --- | --- | --- |
| `rectangle` | 2D domain | `position` Q2, `size` Q2 | Position is the corner. Size components are positive. |
| `circle` | 2D domain | `center` Q2, `radius` length quantity | Radius is positive. |
| `box` | 3D domain | `position` Q3, `size` Q3 | `positionMode` is `corner` or `center`. Omission uses corner semantics. Size components are positive. |
| `cylinder` | 3D domain | `base` Q3, `axis` numeric direction, `radius` and `height` length quantities | The axis defines the finite cylinder direction. Radius and height are positive. |
| `union` | Domain group in the active dimension | `features` array of domain features | Resolve the union of the children. The completed group has at least two children. |
| `intersection` | Domain group in the active dimension | `features` array of domain features | Resolve the common region of the children. The completed group has at least two children. |
| `lineSegment` | Curve | `start` and `end`, each Q2 or Q3 | Use the coordinates in model dimension order. Reject when every component differs by at most `1e-14 m`. |
| `circleCurve` | Curve | `center` Q2 or Q3, `radius` length quantity | Radius must be positive and finite. Optional `normal` is a numeric direction with default `[0, 0, 1]`. |
| `diskSurface` | 3D surface | `center` Q3, `normal` numeric direction, `radius` length quantity | Normal sets the plane orientation. |
| `planeRectangle` | 3D surface | `origin`, `uVector`, `vVector`, all Q3 | The two vectors span the planar rectangle. |

Only the domain feature variants can be group children or keys for domain membership. Apply the declared dimensional and direction checks to every eligible feature. In 2D, top-level `lineSegment` and `circleCurve` records represent standalone curve entities alongside the 2D domains and retain distinct identities in a compiled geometry map. They are not surfaces, materials, or domain-address bits, and they do not change which surface cells receive domain material assignments. A construction curve nested inside a domain union/intersection is invalid under the same-dimension rule. In 3D, retain the corresponding lower-dimensional construction geometry meaning. A representation's stored meaning is separate from whether a particular mesher supports an operation on it.

## Selections and mesh

Each `/model/selections/*` requires `id`, `name`, `entity`, and `addresses`. Allowed entities are `domain`, `boundary`, and `point` in 2D, plus `edge` in 3D. `addresses` is an ordered array of one or more distinct objects. Each has a `membership` object whose keys are addressable geometry IDs and whose values are `outside`, `inside`, or `boundary`. A missing key denotes Outside. Preserve explicit Outside keys on an unchanged round trip. A domain address uses Inside and Outside. Other entities require at least one On boundary membership. A selection is the union of its addresses in the declared topological dimension. Optional `coordinateExpression` is a nonempty expression for nondomain selections, with effective default `0`. Evaluate at the entity coordinates and keep the entities where the expression is zero.

| Mesh key | Requirement |
| --- | --- |
| `elementFamily` | Required `triangle` for 2D and `tetrahedron` for 3D. |
| `minSize`, `maxSize` | Required length quantities. Creation defaults are `0.001 m` and `0.01 m`. Evaluate bounds and apply the global/local size rules. |
| `gmsh` | Required object containing all seven fields in the Gmsh table below. |
| `meshSizeControls` | Optional array. Each object requires `id`, `name`, `selectionId`, `size`. Size is a length quantity and the target is eligible in the active dimension. |
| `periodicPairs` | Required array. Each object requires `id`, `name`, `lowerSelectionId`, `upperSelectionId`, and `translation` Q3. Endpoint IDs identify distinct eligible boundary selections. |
| `periodicPairIds` | Optional nonempty unique array of IDs identifying enabled pair objects. Omit it when there are no enabled pairs. |

| Gmsh field | JSON type and constraint | Creation value |
| --- | --- | --- |
| `algorithm3D` | Integer 1–10 | `1` |
| `elementOrder` | Integer at least 1, subject to mesher capability | `1` |
| `optimize` | Boolean | `true` |
| `optimizeNetgen` | Boolean | `true` |
| `optimizeThreshold` | Number 0–1 inclusive | `0.25` |
| `smoothing` | Integer 0–100 | `10` |
| `threads` | Integer constant 1 | `1` |

## Materials and Physics

Each material requires `id`, `name`, and `selectionId`. `selectionId` refers to a domain selection used by Physics. The material property contract is scoped to the active profile. Use the stated values for creation defaults.

| Profile family | Required properties | Optional properties |
| --- | --- | --- |
| Acoustics | `density` quantity, default `1.225 kg/m^3`. `soundSpeed` quantity, default `343 m/s`. | `lossFactor` dimensionless expression string, effective default `"0"`. Supported loss factor evaluates to zero. |
| Electromagnetics | `relativePermittivity` and `relativePermeability` dimensionless expression strings, default `"1"`. `conductivity` quantity, default `0 S/m`. | None in the built-in profiles. |
| Thermal | `thermalConductivity` quantity with unit `W/(m*K)`, default expression `"1"`; its resolved value must be greater than zero. | None in the built-in profile. |

`/model/physics` requires `id`, `name`, `type`, `formulation`, and `domainSelectionIds` (an ordered ID array). Both wave families require `frequency` as a frequency quantity with creation default `1 Hz`. Electromagnetics requires `phasorConvention` with value `exp(+i omega t)`. Acoustic and thermal models omit `phasorConvention`. Thermal omits `frequency`.

Each additional Physics member is a condition object keyed by its own `id`. It requires `id`, `name`, `type`, and the fields below. Optional `enabled` is a boolean with effective default `true`. Disable a condition by saving `false`. Conditions are named object members, so their stable keys remain separate from the user-editable `name`.

| `type` | Required additional fields | Optional fields and rules |
| --- | --- | --- |
| `dirichlet` | `selectionId`, `value` quantity | Value is a prescribed field in the profile/formulation unit. |
| `neumann` | `selectionId`, `value` quantity | Value is the declared outward flux or load. |
| `impedance` | `selectionId`, `impedance` quantity | Optional `incidentField` and `polarization` occur together. `incidentField` is a dimensionless quantity with default `1`. `polarization` is an ordered vector with one quantity per primary field component. Wave profiles expose this condition. |
| `periodicCondition` | `periodicPairId` | Optional `phaseVector` Q3 in `rad/m`. Apply the declared default and tolerance to each evaluated phase component. Pair ID refers to an enabled mesh pair. |

| Formulation | Dirichlet creation value | Neumann creation value | Impedance creation value | Incident polarization when enabled |
| --- | --- | --- | --- | --- |
| `pressure` | `0 Pa` | `0 m/s` | `411.6 Pa*s/m` | One component `1 Pa` |
| `Ez` | `0 V/m` | `0 V/m^2` | `376.730313668 ohm` | One component `1 V/m` |
| `Hz` | `0 A/m` | `0 A/m^2` | `376.730313668 ohm` | One component `1 A/m` |
| `electric` | `0 V/m` | `0 V/m^2` | `376.730313668 ohm` | `1, 0, 0` in `V/m` |
| `T` | `293.15 K` | `0 W/m^2` | Unavailable | Unavailable |

## Study and sweep

Each Study requires `id`, `name`, `type`, and `physicsId`. The Physics reference is the active interface's `id`. Optional `discretization` requires `elementOrder` (integer in `1`, `2`, `3` at the structural layer) and `fieldFormulation`. Resolve the effective order from the Study, then the profile default. The following formulation, order choices and effective defaults define the profile interpretation.

| Study profile | Structural formulation token and label | Order choices | Effective defaults |
| --- | --- | --- | --- |
| `acoustic-2d`, `acoustic-3d` | `pressure` | `1` | `pressure`, order `1` |
| `em-2d` | `Ez` or `Hz`, restricted in Settings to the linked Physics formulation | `1` | Linked Physics formulation, order `1` |
| `em-3d` | `electric` (**Electric field**) | `1`, `2` | `electric`, order `2` |
| `em-3d` | `firstOrderEH` (**First-order E-H**, experimental Study choice) | `1` | Explicit experimental choice only |
| `thermal-2d` | `T` | `1` | `T`, order `1` |

Use the explicit Study formulation, then the linked Physics formulation, then the profile and shared defaults. Validate that the effective value matches the linked Physics and the order is in the selected formulation's order choices. Built-in EM3D Physics uses `electric`, so a Study with `firstOrderEH` fails the formulation-match rule before dispatch.

Optional `solver` requires `backend` (`rivrs` or `faer`) and `threads` (one of `1`, `2`, `3`, `4`, `6`, `8`, `12`, `16`). Optional `ordering` is `metis`, `amd`, or `default`. Optional `constitutiveQuadrature` is `standard` or `centroid`. The resolved `faer` choices are `default` ordering and one thread. The `rivrs` choices are `metis` or `amd` and the declared thread list. Electric formulation uses `standard` quadrature. The experimental `firstOrderEH` capability restricts the backend to `faer`. Resolve explicit values, profile defaults, order-dependent common defaults, then common defaults. Common defaults are `rivrs`, `metis`, `standard`, and 12 threads at order 1 or 3 at order 2. Optional `output` has `csvFilename` matching `^[A-Za-z0-9][A-Za-z0-9._-]{0,98}\.csv$` and boolean `fieldFiles` with effective default `true`. A stationary Study supports optional `meanTemperature` with unit `K` and effective default `0 K`, and uses first-order temperature discretization.

An optional harmonic Study `parameterSweep` is an object with required `id`, `name`, `type: "parameterSweep"`, and a nonempty `parameters` array. Each row requires `parameter` (a model parameter symbol) and `values` (one of the following objects). A symbol appears at most once. Evaluate values in the parameter's dimension. Combine independent axes as a Cartesian product with a maximum of 10,000 cases.

| `values.type` | Required fields | Semantics |
| --- | --- | --- |
| `linear` | `start` and `end` quantities, `count` integer at least 2 | Ordered samples including both endpoints. Require `end >= start` after unit conversion. |
| `values` | `values` as a nonempty array of quantities | Explicit samples in stored order. |

## Result recipes

Every result requires `id`, `name`, and `type`. A recipe selects and presents data produced by the Study or an eligible result dataset.

| `type` | Required additional fields | Optional fields and rules |
| --- | --- | --- |
| `field` | `quantity` with `type: "fieldComponent"`, `component` symbol, and `part` (`real`, `imaginary`, or `magnitude`) | `frequency` quantity for wave profiles, `filters` as a map from parameter key to numeric sample value, and `cutPlanePositions` as an object with optional `x`, `y`, `z`. A cut is `{ "mode": "midPlane" }` or `{ "mode": "explicitValues", "coordinate": <length quantity> }`. Thermal uses component `T` and omits frequency. |
| `sweep` | `quantity` string matching `^[A-Za-z][A-Za-z0-9_]*$` | Select the reported Study output for this quantity when the profile supplies it. EM3D uses `plot.sweep` for its sweep presentation. Thermal uses `field` results. |
| `plot.sweep` | Nonempty `datasets`, nonempty `series`, `output` | Optional `axes` and `tableColumns`. `tableColumns`, when present, is a nonempty ordered list of unique column strings. `axes` may contain `xMin` and `xMax` quantities plus numeric `yLeftMin`, `yLeftMax`, `yRightMin`, `yRightMax`. |

Each dataset requires `id`, `name`, `source`, `xColumn` (nonempty string), and `style` (`solid`, `dashed`, or `dotted`). Optional `xUnit` is the X display unit, `filters` is a numeric parameter map, and `parameterColumns` is an ordered list of unique nonempty strings. Each series requires `id`, `name`, and `column` (nonempty string). Optional `axis` is `left` or `right`. Optional `unit` is the series display unit. `output` requires a checked relative `directory` and `figureFilename` matching `^[A-Za-z0-9][A-Za-z0-9._-]{0,98}\.(svg|png|pdf)$`.

The Study source is `{ "type": "study", "studyId": <Study ID> }`. The file source is `{ "type": "file", "path": <checked relative path> }` with optional `columnUnits`. Each `columnUnits` key is a nonempty literal CSV header and its value is a supported unit token for the stored numbers in that column. For example, `{ "type": "file", "path": "sweep.csv", "columnUnits": { "frequency_kHz": "kHz" } }` declares kHz samples independently of a dataset's `xUnit: "Hz"`. An omitted map or entry uses the [file source-unit interpretation](#file-source-units). A display unit edit preserves `source.columnUnits` and the literal column keys.

## File source units

For external CSV files, use `source.columnUnits` to interpret raw numeric cells before applying display units. Match map keys to the parsed headers exactly and report any unmatched key with its dataset and column. An explicit map entry supplies the source unit even when the header contains a unit-like suffix. Without a map entry, the exact `frequency_Hz` header has source unit `Hz` and `frequency_GHz` has source unit `GHz`. For any other unmapped file column, retain the legacy interpretation: use `xUnit` for the selected X column, the declared series unit for a series column, or unit `1` when neither applies. When both legacy declarations apply to the same column, use `xUnit`. Under this fallback, editing the assumed unit reinterprets the original numbers. Apply and reopen must use the same interpretation. Declare a source unit explicitly to convert values independently of the display unit. When only source metadata supplies a unit, display values in that unit.

## Periodicity and incoming-wave meaning

Wave periodic data denotes the translated trace relation `u(r + delta) = exp(-i k dot delta) u(r)` for the `exp(+i omega t)` convention. Here `phaseVector` is `k` in rad/m and the mesh pair translation is in metres. This fixes the source/destination and phase-sign meaning independently of a particular interpreting schema.

Complex periodic weights apply consistently to matrix reduction, source reduction, prescribed-value consistency and physical field reconstruction. EM3D interpretation preserves edge and face moment orientations for both element orders.

EM3D impedance data describes a collection of selected impedance boundaries. Each boundary supplies its physical impedance expression; an incoming boundary also supplies a complex incident-field expression and polarization. There is no required input/output port pair, propagation axis, free-space reference impedance or monitor-plane arrangement. The incident expression supplies its own spatial phase, for example `exp(-i*ky*y)`, and must be consistent with the Floquet trace relation. An omitted port or propagation description does not request inference of a plane-wave mode or incidence angle.

Transmission, reflection and other observables are model-defined global expressions and integration operators evaluated from solved fields. Normal-incidence expressions may need changes for oblique waves; they are not an automatically normalized measure of all outgoing diffraction orders. Scalar wave interpretation uses the same Floquet relation with its declared field and boundary quantities.

Wave profiles permit nonzero real Floquet vectors. Stationary thermal conduction retains its zero-phase restriction. A model parameter used in `phaseVector` is evaluated for each prepared sweep case; its saved expression is not replaced by a single precomputed value.

## Format and application validation

The field definitions, constraints, references, units, effective defaults and mathematical meanings above belong to the public format. A consuming application declares the profiles and operations it supports and supplies its own diagnostics and execution stages. An unsupported operation or capability must not silently change the meaning of an otherwise representable stored document. Those diagnostics, execution stages and native capability checks do not redefine the stored values.
