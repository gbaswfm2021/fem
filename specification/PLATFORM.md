# FEM platform dependency profile

FEM uses the established [FullMeta application platform](../../../platform/docs/application-platform-contract.md). Shared platform rules are authoritative in their own references; this file records FEM's selected interfaces and application obligations.

## Selected interfaces

| Boundary | FEM selection and application obligation |
| --- | --- |
| Document and interpreter | Project Model 2 JSON declares the application schema through relative `x-fullmeta-schema`; the application schema declares the [Schema Editor interpreter](../../../platform/schema-editor/schema-editor.schema.json). Every owned standalone schema declares its interpreter. |
| UI and widgets | `fullmeta-ui-behavior/1`, `fullmeta-widget-catalog/1` and `fullmeta-react-widget/1`; use the [shared frontend provider contract](../../../platform/docs/frontend-provider-contract.md). |
| Application components | New FEM widget sources live in `applications/{APP_ID}/ui/widgets/<widget>/widget.jsx`; compiled `widget.js` and source maps are the registered public entries. Pure data and orchestration code belongs in application `js/`. |
| Editing and AI access | The shared host state/command and WebMCP interfaces operate the same canonical document and guarded application actions. Discover available tools/commands; do not rely on a fixed tool count or React's private rendering tree. |
| Validation | JSON Schema plus `fullmeta-declarative-rules/1` and the supported native AST. Implement all predicates and stage programs in [VALIDATION.md](VALIDATION.md); preserve authored obligations and derived-rule consistency. |
| Runtime declarations | `fullmeta-model-runtime/1` through the [runtime declaration schema](../../../platform/contracts/fullmeta-model-runtime-1.schema.json). Keep program IDs, routes, storage and artifacts explicit and confined. |
| Native operations | `fullmeta-native-operations/1`; FEM selects the operations below from the [native authority](../../../platform/contracts/native-operations.json) using the [shared native contract](../../../platform/docs/native-operation-contract.md). |
| Backend compatibility | Retain the application's expected `fullmeta-model-editor-api/18` and required route/capability behavior in [runtime integration](contracts/runtime-integration-boundary.md). Shared native identity and frontend build identity do not replace this check. |
| Workspace presentation | Use the [shared workspace contract](../../../platform/docs/workspace-application-contract.md). FEM's initial layout is `stacked`, with Graphics and Settings at 50:50; both supported layouts and draggable/keyboard-resizable dividers remain available. |
| Schema metadata | The FEM schema declares `author: "Jensen Li, Yuee Li"`. The shared schema interpreter presents that metadata through ordinary Settings. |
| User guide | `applications/{APP_ID}/docs/README.md` beside the application schema. A project model's Help opens this guide; editing the FEM schema opens the guide beside its Schema Editor interpreter. |

For the layout selection, use the supported application UI annotation `x-fullmeta-ui-modelEditor.layout.defaultLayout: "stacked"`. Layout, divider sizes, selection, camera and pending drafts remain presentation state rather than Model 2 values.

## Native selection

| Operation | Version | Application integration |
| --- | --- | --- |
| `mesh.check` | 1 | Check an opaque original model through the declared mesher registration, retaining component/model identity and geometry diagnostics. |
| `mesh.compile` | 1 | Compile the checked geometry/map artifacts in the declared working/output boundary, including equivalent retries. |
| `fem.prepareStudy` | 1 | Prepare the selected Study with the supported plan or solver-request result for its profile. |
| `fem.studyRun` | 1 | Run the selected Study through the checked NDJSON route, terminal states and artifact contract. |

Record each selected operation's generated digest, executable identity and actual route binding in candidate evidence. The [FEM runtime](RUNTIME.md) and [runtime integration contract](contracts/runtime-integration-boundary.md) retain original model payloads, native entries, preparation alternatives, artifact requirements, retries, cancellation and full profile behavior.

For `fem.studyRun`, the descriptor permits an empty `logs/solver.stderr.log` only. That file must still exist. Other required outputs retain their nonempty/content checks; this exception does not permit missing stderr or empty stdout, timing, mesh, field or CSV outputs.

## Dependency and application evidence

Use the supplied shared editor/platform and identified `fem-rust/` and `mesh-rust/` dependencies. The application builder owns its schema, package, JavaScript/JSX, actions, adapters and tests. Repair those files when they cause a failure; record fixed-dependency blockers without weakening a requirement.

Record the shared interfaces and candidate-specific frontend/schema/package/native identities in application acceptance. Build and watch commands are maintained in the [frontend development guide](../../../platform/ui/frontend-development.md); the [debugging guide](../../../platform/ui/editor-debugging.md) covers optional source reload and replay. Shared platform acceptance is evidence about those mechanisms, while [FEM acceptance](ACCEPTANCE.md) must exercise FEM's own providers, routes, profiles, live results and numerical outcomes.

### Widget audit scope

The FEM acceptance baseline covers FEM-owned source and tests and the supplied shared platform interfaces. Run `node scripts/check-widget-boundaries.mjs --platform`, and audit FEM's own public widget imports and registered providers. Fix every FEM-owned finding, including imports that bypass a widget's public entry. Record the candidate and each check's actual scope and result.

A checkout-wide audit, when run, is separate evidence and must retain its actual result. A platform-only pass or an absence of FEM findings does not make a failing checkout-wide audit pass. Classify a fixed-dependency finding as a FEM blocker only when it actually prevents a required FEM outcome, with the normal minimal reproduction and identity evidence. This audit distinction does not change the schema authoring, behavior or acceptance requirements.
