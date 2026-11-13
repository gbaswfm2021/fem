# FEM application architecture

This document defines the application layer. It specifies a finite-element application that consumes the public data contract in [Model format contract](../docs/fullmeta-model-2.md). It does not redefine the meaning of serialized Model 2 fields. The application ID, display name, schema filename, and release version are deployment parameters from the [specification README](README.md#application-identity-parameters).

## Application identity and compatibility

Before authoring, record the selected `{APP_ID}`, `{APP_DISPLAY_NAME}`, `{SCHEMA_FILE}`, and `{APP_VERSION}` in the implementation plan and substitute them consistently. The catalog ID is a stable machine identifier; display labels are user-facing strings. Do not derive one from the other.

The application schema declares which standard profiles and Model 2 variants it supports. A supported profile must use the profile identifier and serialized semantics defined by the model contract. An unsupported but structurally valid profile must produce an explicit unsupported-profile state; the application must not silently reinterpret it or rewrite the model.

Treat an application release as a versioned behavior contract. A project’s relative schema reference identifies the schema intended to interpret its data. Do not overwrite a released schema path with incompatible behavior. Keep a compatible schema/package route available for existing projects, or provide an explicit, reviewed migration that preserves the original data and records the new application/schema target. Never rely on an unversioned “latest schema” path to preserve historical behavior.

## Application-owned implementation surface

The supplied frontend dependency and supported build/provider/debug workflow are defined by [Shared frontend provider contract](../../../platform/docs/frontend-provider-contract.md). Implement new application-owned visual components as React + JSX under `ui/`, compile them to compatible public `.js` entries, and reuse schema-selected shared widgets. Pure data projection, scene conversion and runtime logic may remain ordinary JavaScript. This requirement changes the application implementation workflow, not the persisted Model 2 contract or the schema author's declaration language.

Implement only the files required by the catalog, schema declarations, and these contracts. The conventional application package includes:

| File or location | Responsibility |
|---|---|
| `applications/{APP_ID}/{SCHEMA_FILE}` | Schema-authored tree, Settings, validation rules, widgets, actions, graphics scenes, result recipes, runtime registrations, and storage declarations. |
| `applications/{APP_ID}/schema-documents.js` | Load and activate the selected schema graph using the documented schema-loader interface. |
| `applications/{APP_ID}/package.js` | Export the runtime package, launcher profiles, model factory, bindings, adapters, and application UI hooks required by the schema. |
| `applications/{APP_ID}/js/` | Model-preserving projections, actions, route requests, operation orchestration, result normalization, and application-specific control logic. |
| `applications/{APP_ID}/ui/` | Graphics-scene conversion, selection/highlight mapping, custom Settings, and result-specific panels. |
| `applications/{APP_ID}/docs/README.md` | User guide for the implemented application and actual supported workflows, discovered beside its interpreting schema by Help. Update it as part of application delivery and verify it from a real project model. |
| `applications/{APP_ID}/fem-rust/` | Supplied fixed solver dependency. Integrate through documented entry points without changing native behavior or source. Provision missing build artifacts from unchanged source only through the documented toolchain; record their identities. |
| `applications/{APP_ID}/mesh-rust/` | Supplied fixed mesher dependency. Integrate through documented entry points without changing native behavior or source. Provision missing build artifacts from unchanged source only through the documented toolchain; record their identities. |

Keep implementation ownership explicit. Generic tree, widgets, Settings drafts, document storage, graphics controls, network queue, checked artifact access, and the Operation strip are shared platform responsibilities. The application supplies domain data and bindings plus the application-owned package, adapters, renderers, and runtime registrations required to use them. A required capability missing from the documented shared interfaces or supplied native programs is a blocker with a reproducible case, not permission for an undocumented patch.

## Application behavior contract set

The application specification is the complete implementation contract; every listed document is normative. Follow these ownership boundaries:

| Application behavior | Contract |
|---|---|
| Schema project creation, launcher creation/deletion, catalog, loader, and package | [Create project interface](contracts/create-project-interface-contract.md) |
| Designer workflow and schema evidence | [Designer authoring](contracts/designer-authoring.md) |
| Validation rules and their scope labels | [Validation function inventory](VALIDATION.md) |
| Tree hierarchy, row actions, Settings, editing, and save behavior | [Model and UI functions](UI.md) · [Settings controls](contracts/settings-control-contract.md) · [Tree context menus](contracts/tree-context-menu-contract.md) |
| Shared widget bindings and data projection | [Shared widget contract](contracts/shared-widget-contract.md) |
| Application JavaScript and UI module signatures | [Application UI and JavaScript contract](contracts/application-ui-javascript-contract.md) |
| Frontend installation/build/source maps, shared React runtime, lifecycle and visible replay | [Shared frontend provider contract](../../../platform/docs/frontend-provider-contract.md) |
| Icons, graphics scenes, layer controls, selection, mesh appearance, camera, and field/result rendering | [Icon inventory](contracts/icon-inventory.md) · [Graphics acceptance](contracts/graphics-and-results-schema-acceptance.md) · [Mesh appearance](contracts/mesh-appearance.md) |
| External programs, profiles, storage, dispatch, progress, Stop, recovery, and outputs | [Runtime integration](contracts/runtime-integration-boundary.md) · [Runtime operations](RUNTIME.md) · [Solver capability](contracts/solver-capability-contract.md) |
| Function-to-owner mapping and complete coverage method | [Function inventory](contracts/function-inventory.md) · [Traceability](contracts/function-to-schema-and-code-traceability.md) · [Coverage method](contracts/function-contract-coverage-method.md) |

All UI behavior must be specified as visible results and model effects, not only as a widget choice or JavaScript interface. For each control state, define its value, label, enabled state and reason, input/output paths, selection effects, save behavior, error state, and relevant accessibility name. For each tree item, define its parent, order, readable label, Settings path, scene, highlight, and actions. For each runtime action, define its route, snapshot, storage context, lifecycle events, outputs, failure behavior, and recovery behavior.

## Validation ownership

All executable validation predicates live in the application layer, including checks that are reusable across unrelated applications. The Model 2 contract specifies the data the predicate reads and the stable meaning of that data. The application specification specifies whether a predicate is required, when it runs, its repairable path, diagnostic, and whether processing stops or continues.

The 40 `VAL-*` entries in [Validation function inventory](VALIDATION.md) are application-layer predicates. Their labels distinguish:

- **Shared baseline:** reusable identity, profile-registration, expression-engine, or artifact-integrity checks that an application may adopt without adding them to the data-format contract.
- **Finite-element policy:** geometry, selection, Physics, material, mesh, Study, and result constraints for this application.
- **Mixed:** a stable Model 2 reference or unit semantic plus an application capability, profile rule, or runtime limit. Keep the data meaning in the Model 2 contract and the executable acceptance decision here.

Every rule must have an isolated passing input, a failing input, an exact diagnostic, and a repaired input. Structural JSON constraints, root FullMeta predicates, named registration/dispatch programs, native preflights, and runtime execution are separate evidence classes; do not combine their counters.

## Implementation and acceptance order

1. Resolve the application identity parameters and verify the supplied shared-platform and native-program interfaces, including the shared [frontend contract](../../../platform/docs/frontend-provider-contract.md), pinned tool installation, compiled assets and source maps.
2. Author the full schema from the Model 2 contract and application behavior contracts using the Schema Editor. Save, reopen, validate, inspect TDUV counters and branch behavior, and resolve every declaration path.
3. Build the package and its `js/` and `ui/` interfaces from their named contracts. New visible application components use the supported React + JSX workflow and compiled public provider adapters. Confirm the catalog loads the selected package and schema without application-ID assumptions, with correct mount/update/disposal and shared-widget reuse.
4. Integrate the supplied mesh and solver programs through the documented registrations. Verify their executable paths, allowed entries, input/output contracts, optimized build selection where available, and failure messages without changing those programs.
5. Complete Level 1 schema acceptance, the frontend/provider and visible-replay gate, then Level 2 behavior replay using disposable models authored from the frozen contracts. Use explicit fixture/live services and the same visible localhost editor for normal-command checkpoints and watched reload/selected fixture replay. Preserve real input/layout/graphics and native execution checks. The builder must not receive or inspect saved project JSONs during implementation or debugging.
6. Repair application-owned schema, package, JavaScript/JSX, action and adapter defects, then rerun the affected schema and behavior cases. Reserve dependency blockers for identified fixed shared/native components; record their exact reproduction and keep the affected outcome unverified.

If a required behavior is unspecified, record the exact affected model path, UI action or route, expected result, observed result, and owner. Resolve the normative requirement before treating implementation judgment as acceptance.
