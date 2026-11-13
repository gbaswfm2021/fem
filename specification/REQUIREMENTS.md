# FEM requirements

## Target and scope

Build a finite-element application that interprets the public [FEM Model 2 format](../docs/fullmeta-model-2.md), lets users author and preserve its supported values, and executes the declared mesh, Study and result workflows through identified native dependencies. The format is fixed independently of this application implementation.

The requirements include the schema, catalog/package integration, application-owned JavaScript and JSX where needed, validation programs, native registrations, result adapters, user guide and application acceptance. The [architecture](ARCHITECTURE.md) assigns ownership. The [platform profile](PLATFORM.md) selects shared FullMeta interfaces.

## Required capabilities

The [function inventory](contracts/function-inventory.md) assigns stable `UI-*` IDs, including the project-lifecycle subcases. Expand every function across its documented variants, states and interactions; the inventory is an index rather than a substitute for the detailed contracts.

| Capability | Governing requirements |
| --- | --- |
| Author, save and reopen the interpreting schema | `UI-01`; [authoring workflow](contracts/designer-authoring.md) |
| Create, open, save and delete routed projects | `UI-02`, `UI-02b`, `UI-02c`; [project lifecycle](contracts/create-project-interface-contract.md) |
| Navigate, edit and manage model items | `UI-03`–`UI-06`; [UI](UI.md), [Settings](contracts/settings-control-contract.md), [widgets](contracts/shared-widget-contract.md), [tree actions](contracts/tree-context-menu-contract.md) |
| Validate declarations, values and stage inputs | `UI-07`; all required [`VAL-*` predicates](VALIDATION.md) and named runtime programs |
| Preview geometry and build/restore/clear mesh | `UI-08`, `UI-09`; [runtime](RUNTIME.md), [mesh appearance](contracts/mesh-appearance.md) |
| Prepare, run, Stop and recover a Study | `UI-10`; [runtime](RUNTIME.md), [native integration](contracts/runtime-integration-boundary.md), [solver capability](contracts/solver-capability-contract.md) |
| Resolve, display and export results | `UI-11`; [runtime and results](RUNTIME.md), [graphics acceptance](contracts/graphics-and-results-schema-acceptance.md) |
| Control graphics and preserve view preferences | `UI-12`; [UI](UI.md), [JavaScript/UI contract](contracts/application-ui-javascript-contract.md), all required `GX-*` checks |
| Scope operations and resolve declared dependencies | `UI-13`, `UI-14`; [native integration](contracts/runtime-integration-boundary.md) |

## Supported numerical profiles

Implement the profile/formulation, preparation, backend, element-order and output matrix in the [solver capability contract](contracts/solver-capability-contract.md).

| Profile | Main formulation | Core behavior |
| --- | --- | --- |
| `acoustic-2d` | Pressure | Harmonic point and parameter-sweep studies on 2D triangular meshes. |
| `acoustic-3d` | Pressure | Harmonic point and parameter-sweep studies on 3D tetrahedral meshes. |
| `em-2d` | `Ez` or `Hz` | Harmonic studies following the linked Physics interface. |
| `em-3d` | Electric field | Harmonic studies with the specified Nédélec orders and backends. |
| `thermal-2d` | Temperature | Stationary thermal solve with the specified field and summary outputs. |

A structurally permitted option is not automatically an implemented numerical capability. Preserve the exact experimental-option rules, formulation matching and rejection behavior specified by the format, validation and solver contracts. An unsupported valid input needs an explicit unsupported-capability result; it must not be silently reinterpreted.

## Invariants

- Preserve the public data meanings, IDs, quantities, references, optional-member absence, array order and supported unknown extension data described in [DATA_MODEL.md](DATA_MODEL.md).
- Keep schema declarations separate from project values and transient view/draft/operation state. Defaults shown for display do not become stored values until the specified edit.
- Human controls and WebMCP invoke the same normal application transactions, including raw draft, availability, revision, Preview/Apply/Save/Validate and source-conflict guards.
- Use the selected schema, model snapshot, document identity and declared runtime context consistently for an operation. Validate required generated artifacts before publishing success.
- Preserve earlier compatible completed results through later failed, cancelled or interrupted runs where the runtime contract requires it.
- Reuse the shared platform interfaces. New FEM widgets and domain orchestration are application-owned; the supplied shared platform and native dependencies retain their identified interfaces.
- Deliver an accurate application guide and verify the shared Help lookup as part of [acceptance](ACCEPTANCE.md).

## Coverage and completion

Retain every required `UI-*`, `VAL-*`, `GX-*`, project, operation and numerical case. Use the [coverage method](contracts/function-contract-coverage-method.md) and [traceability contract](contracts/function-to-schema-and-code-traceability.md) to connect requirements, declarations, code, normal commands and evidence.

Focused cases isolate rules; composite cases cover specified interactions. Application-owned schema and code failures must be repaired. Fixed-dependency blockers remain open with evidence. Full completion follows [ACCEPTANCE.md](ACCEPTANCE.md), not a schema count or shared-platform smoke.
