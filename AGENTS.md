# Working on the FEM application

These instructions cover application-owned work below `applications/fem/`. Begin with [the specification index](specification/README.md), then its [requirements](specification/REQUIREMENTS.md), [architecture](specification/ARCHITECTURE.md), [platform dependency profile](specification/PLATFORM.md) and [acceptance contract](specification/ACCEPTANCE.md). The specification defines the target; the user guide describes implemented behavior. Documentation maintenance does not itself start the paused application build.

## Ownership

- Implement and repair the application's own schema, package, JavaScript, JSX widgets, actions, adapters and application tests when the task includes an application build. Keep these files under the application folder.
- Treat the supplied shared platform, Schema Editor, Model Editor, Kernel, Core/Model WASM, `fem-rust/` and `mesh-rust/` as identified dependencies. Use their documented interfaces. A task that explicitly includes changing a dependency can change that scope; ordinary application work does not implicitly include it.
- For a fixed dependency that prevents a required result, record its version or executable identity, the smallest reproduction, expected and actual outcome, exact diagnostic and affected requirement. Continue independent application work and keep the blocked outcome unverified.
- Distinguish a schema defect, application-code defect, invalid case, specification gap and fixed-dependency blocker. Application-owned JS/JSX failures are repairable application defects, not external blockers to report instead of fixing.

## Implementation conventions

Preserve [the data model](specification/DATA_MODEL.md): field meanings, units, IDs, references, optional-member absence, array order and supported unknown extension data. Keep presentation/draft/operation state out of persisted model values unless the model contract declares it.

Use registered shared widgets and declared UI/validation/runtime bindings. Build new application-owned presentation with the established React/JSX provider interfaces only where shared capabilities do not express the required behavior. Keep pure projections and orchestration in JavaScript. Edit authoritative source files; generated public `.js` entries and source maps come from the frontend build.

Human controls and WebMCP use the same normal application commands and transaction guards. Discover available commands and their argument schemas. Preserve raw invalid drafts, selection and diagnostics, and the distinct Preview, Apply, Save and Validate effects. Source editing and the ordinary JSX build workflow remain available; authoring tools do not require OS input automation or a second hidden application state.

Use the [shared platform contract](../../platform/docs/application-platform-contract.md) and [frontend build guide](../../platform/ui/frontend-development.md) for installed interfaces and tooling. From the FullMeta root, `npm run build:frontend` compiles authoritative JSX and checks its bindings. Run the relevant application and boundary checks named by [ACCEPTANCE.md](specification/ACCEPTANCE.md); a shared frontend build is not FEM or numerical acceptance.

## Verification and documentation

Create disposable model cases from the frozen specification. Do not inspect or copy saved project JSONs to choose implementation details, expected values or replay cases. Cover isolated predicates and every specified interaction with focused and composite inputs. Keep each case's data, generated storage and expected outcomes identifiable.

Repair application-owned failures and rerun affected schema, integration, UI/command, native and numerical cases. Use the same identified candidate for the evidence. Fixture services do not prove live native outputs; structured state does not prove focus, dragging or canvas pixels. Check the actual visible behavior where the acceptance contract requires it.

Maintain `docs/README.md` as the guide to the implemented application, with valid relative links and accurate capabilities. Verify Help from a project model resolves that guide beside `fem.schema.json`. Editing the FEM schema itself must use the guide beside its interpreting Schema Editor schema. Record target changes in the specification rather than hiding requirements in this file or a migration handover.
