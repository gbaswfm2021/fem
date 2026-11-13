# FEM application specification

This is the entry point for the complete target FEM application. It defines what to build and how to establish acceptance. The [application guide](../docs/README.md) describes delivered behavior; [AGENTS.md](../AGENTS.md) supplies working instructions for an agent.

The application consumes the fixed [FEM Model 2 public format](../docs/fullmeta-model-2.md). Its interpreting schema, package, widgets, validation programs and native bindings implement the application behavior specified here. A format definition and an application capability are separate contracts.

## Reading order

| Document | Purpose |
| --- | --- |
| [REQUIREMENTS.md](REQUIREMENTS.md) | Required FEM capabilities, supported profiles, invariants and stable requirement IDs. |
| [PLATFORM.md](PLATFORM.md) | The shared FullMeta interfaces and options selected by this application. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Components, ownership, bindings and application implementation boundaries. |
| [DATA_MODEL.md](DATA_MODEL.md) | Compliance with the public format, supported variants and preservation requirements. |
| [UI.md](UI.md) | Project tree, Settings, graphics and observable editing behavior. |
| [VALIDATION.md](VALIDATION.md) | Required predicates, stages, diagnostics and repair paths. |
| [RUNTIME.md](RUNTIME.md) | Native operations, storage, progress, results, cancellation and recovery. |
| [ACCEPTANCE.md](ACCEPTANCE.md) | Required cases, evidence, failure attribution and completion. |

These core documents and the detailed contracts linked below are normative. Links to shared platform contracts identify dependencies; they do not replace FEM-specific requirements. A requirement is complete only when its applicable cases meet the acceptance contract.

## Detailed FEM contracts

| Concern | Contract |
| --- | --- |
| Function IDs and coverage | [Function inventory](contracts/function-inventory.md), [coverage method](contracts/function-contract-coverage-method.md), [source and behavior traceability](contracts/function-to-schema-and-code-traceability.md) |
| Schema authoring and project lifecycle | [Authoring workflow](contracts/designer-authoring.md), [project creation and deletion](contracts/create-project-interface-contract.md) |
| Settings, widget choices and row actions | [Settings controls](contracts/settings-control-contract.md), [FEM shared-widget bindings](contracts/shared-widget-contract.md), [tree context menus](contracts/tree-context-menu-contract.md) |
| Application package and graphics | [JavaScript/UI interfaces](contracts/application-ui-javascript-contract.md), [graphics and results acceptance](contracts/graphics-and-results-schema-acceptance.md), [mesh appearance](contracts/mesh-appearance.md), [icon inventory](contracts/icon-inventory.md) |
| Native integration and numerical capabilities | [Runtime integration](contracts/runtime-integration-boundary.md), [solver and mesher capabilities](contracts/solver-capability-contract.md) |
| End-to-end behavior | [Behavior replay protocol](contracts/behavior-replay-protocol.md) |

The [extension guide](contracts/extension-guide.md) is a developer reference for later changes to the application. It does not expand the fixed-dependency scope of the initial application build.

## Application identity parameters

Resolve these deployment parameters before implementation and substitute them consistently:

| Token | Meaning |
| --- | --- |
| `{APP_ID}` | Stable application ID in the catalog and routed project documents. |
| `{APP_DISPLAY_NAME}` | User-facing application name. |
| `{SCHEMA_FILE}` | Registered interpreting-schema filename. |
| `{APP_VERSION}` | Version of the application's behavior and package. |

The current application folder is `applications/fem/`; its schema is [fem.schema.json](../fem.schema.json). A display name does not determine a machine ID or resource path. Deployment identity does not redefine the Model 2 payload. A project's `x-fullmeta-schema` resolves from the project's own directory.

## Build and acceptance

1. Read the requirements, platform profile, architecture and data-format compliance contract. Resolve each required function to its detailed contract and owner.
2. Derive focused and composite disposable cases from those contracts before assessing the candidate. Saved project JSONs and historical authoring worksheets are not implementation inputs.
3. Author the schema with supported declarations and normal Schema Editor commands. Keep Preview, Apply, Save and Validate distinct and inspect the saved/reopened interpretation.
4. Implement the application-owned package, JavaScript, JSX widgets, actions and result adapters. Reuse shared widgets and the established platform contracts; integrate the identified solver and mesher through their documented interfaces.
5. Run the schema, provider, command/visible UI, live native and numerical cases in [ACCEPTANCE.md](ACCEPTANCE.md) against the same identified candidate.
6. Repair application-owned schema and code defects, then repeat affected cases. Report only fixed platform/native dependency failures as dependency blockers, with a minimal reproduction; keep blocked requirements unverified and continue independent work.
7. Deliver and check [docs/README.md](../docs/README.md) against the implemented workflows, including interpreter-based Help from a project model.

Keep numerical meanings, optional-member absence, array order, IDs, references, units and supported extension data intact. Do not infer completion from counters, a validator pass, fixture-only replay or one successful Study.

## Maintaining this set

Use the shared [application specification convention](../../../platform/docs/application-specifications.md) for document roles and the [application platform contract](../../../platform/docs/application-platform-contract.md) for common FullMeta behavior. React/JSX, WebMCP and the workspace interfaces are established platform dependencies; their implementation history is not part of the FEM requirements.

When behavior is unspecified, identify the missing path, state or outcome and update the appropriate normative contract. Keep historical migration records and acceptance snapshots outside the live specification. Documentation organization does not start the application build.
